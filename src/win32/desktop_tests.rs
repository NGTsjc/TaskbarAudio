use super::*;
use crate::util::dpi::DpiScope;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClientRect, GetWindowRect, IsWindowVisible, PM_REMOVE,
    PeekMessageW, SendMessageW, WM_LBUTTONDBLCLK,
};

fn pump_until(mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    loop {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if ready() {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "desktop transition timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn visible_panel(shell: HWND) -> Option<HWND> {
    let panel = unsafe { FindWindowExW(shell, None, w!("TaskbarAudioPanel"), None) }.ok()?;
    unsafe { IsWindowVisible(panel) }.as_bool().then_some(panel)
}

#[test]
#[ignore = "requires interactive Windows Explorer; briefly shows a test panel and tray icon"]
fn desktop_hotkey_tray_resume_and_restart() {
    let _runtime_lock = crate::win32::RUNTIME_TEST_LOCK.lock().unwrap();
    let shell =
        unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.expect("interactive taskbar required");
    // Keep this isolated from an already running copy of the application.
    assert!(
        unsafe { FindWindowW(w!("TaskbarAudioHiddenWindow"), None) }.is_err(),
        "exit the running visualizer before this desktop test"
    );
    crate::config::user::init_test_config();
    let loop_ = MessageLoop::new().unwrap();
    let hwnd = loop_.hwnd();
    hotkey::register(hwnd, Some("Ctrl+Alt+Shift+F24"));
    // An identical registration under a different ID must fail; a failure of our
    // initial registration would make the following WM_HOTKEY test meaningless.
    assert!(
        unsafe {
            RegisterHotKey(
                hwnd,
                99,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                0x87,
            )
        }
        .is_err()
    );
    for cycle in 0..3 {
        pump_until(|| visible_panel(shell).is_some());
        let panel = visible_panel(shell).unwrap();
        let _dpi_scope = DpiScope::for_window(shell).unwrap();
        let mut shell_rect = RECT::default();
        let mut panel_rect = RECT::default();
        unsafe {
            GetClientRect(shell, &mut shell_rect).unwrap();
            GetWindowRect(panel, &mut panel_rect).unwrap();
        }
        let mut corners = [
            POINT {
                x: panel_rect.left,
                y: panel_rect.top,
            },
            POINT {
                x: panel_rect.right,
                y: panel_rect.bottom,
            },
        ];
        unsafe {
            MapWindowPoints(None, shell, &mut corners);
        }
        assert_eq!(corners[0].y, 0);
        assert_eq!(corners[1].y, shell_rect.bottom);
        assert_eq!(
            corners[1].x - corners[0].x,
            UiConfig::PANEL_WIDTH_PX.min(shell_rect.right)
        );
        let panel = if cycle == 1 {
            // Simulate the panel being lost during a shell restart without
            // restarting Explorer or disturbing the user's other windows.
            unsafe {
                DestroyWindow(panel).unwrap();
            }
            pump_until(|| visible_panel(shell).is_some());
            visible_panel(shell).unwrap()
        } else {
            panel
        };
        unsafe {
            PostMessageW(
                hwnd,
                WM_HOTKEY,
                WPARAM(hotkey::TOGGLE_HOTKEY_ID as usize),
                LPARAM(0),
            )
            .unwrap();
        }
        pump_until(|| taskbar_embed::is_paused(hwnd));
        assert!(!unsafe { IsWindow(panel) }.as_bool());
        assert!(crate::audio::global_stream().is_none());
        unsafe {
            SendMessageW(
                hwnd,
                taskbar_embed::WM_APP_RETRY_ATTACH,
                WPARAM(0),
                LPARAM(0),
            );
        }
        assert!(taskbar_embed::is_paused(hwnd));
        if cycle != 2 {
            // Real tray callback path: double-click restores a paused controller.
            tray::handle_message(
                hwnd,
                WM_APP + 2,
                WPARAM(1),
                LPARAM(WM_LBUTTONDBLCLK as isize),
            );
            pump_until(|| !taskbar_embed::is_paused(hwnd));
        }
    }
    // Exit/restart remains available while audio is paused.
    unsafe {
        PostMessageW(hwnd, WM_APP_RESTART, WPARAM(0), LPARAM(0)).unwrap();
    }
    loop_.run().unwrap();
    assert!(take_restart_requested());
    assert!(crate::audio::global_stream().is_none());
    // Window destruction unregisters the hotkey; it must be available again.
    unsafe {
        RegisterHotKey(
            None,
            99,
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            0x87,
        )
        .unwrap();
        UnregisterHotKey(None, 99).unwrap();
    }
}
