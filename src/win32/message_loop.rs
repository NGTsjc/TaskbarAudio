use crate::config::UiConfig;
use crate::win32::{hotkey, taskbar_embed, tray};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    SetConsoleCtrlHandler,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, HMENU, IsWindow, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_HOTKEY, WNDCLASSW,
};
use windows::core::w;

static EXIT_HWND_ADDR: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

pub const WM_APP_RESTART: u32 = WM_APP + 10;

pub struct MessageLoop {
    hwnd: HWND,
}

impl MessageLoop {
    pub fn new() -> anyhow::Result<Self> {
        RESTART_REQUESTED.store(false, Ordering::Relaxed);
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
            let class_name = w!("TaskbarAudioHiddenWindow");
            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("TaskbarAudio"),
                Default::default(),
                0,
                0,
                0,
                0,
                None,
                HMENU::default(),
                instance,
                None,
            )?;
            anyhow::ensure!(!hwnd.0.is_null(), "CreateWindowExW failed");
            EXIT_HWND_ADDR.store(hwnd.0 as isize, Ordering::Relaxed);
            let loop_ = Self { hwnd };
            setup_console_ctrl_handler()?;
            TASKBAR_CREATED.get_or_init(|| RegisterWindowMessageW(w!("TaskbarCreated")));
            tray::init(hwnd)?;
            taskbar_embed::init(hwnd)?;
            hotkey::register(
                hwnd,
                crate::config::user::get_user_config()
                    .toggle_hotkey
                    .as_deref(),
            );
            Ok(loop_)
        }
    }

    #[allow(dead_code)]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    #[allow(dead_code)]
    pub fn request_exit(&self) {
        unsafe {
            let _ = PostMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    pub fn run(self) -> anyhow::Result<()> {
        unsafe {
            let mut msg = MSG::default();
            loop {
                match GetMessageW(&mut msg, None, 0, 0).0 {
                    -1 => return Err(windows::core::Error::from_win32().into()),
                    0 => break,
                    _ => {}
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}

pub fn take_restart_requested() -> bool {
    RESTART_REQUESTED.swap(false, Ordering::Relaxed)
}

impl Drop for MessageLoop {
    fn drop(&mut self) {
        EXIT_HWND_ADDR.store(0, Ordering::Relaxed);
        let _ = unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), false) };
        if unsafe { IsWindow(self.hwnd) }.as_bool() {
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if TASKBAR_CREATED
        .get()
        .is_some_and(|id| *id != 0 && msg == *id)
    {
        if let Err(error) = tray::init(hwnd) {
            tracing::warn!(%error, "restore tray icon failed");
        }
        return LRESULT(0);
    }
    if let Some(result) = tray::handle_message(hwnd, msg, wparam, lparam) {
        return result;
    }
    if let Some(result) = taskbar_embed::handle_message(hwnd, msg, wparam, lparam) {
        return result;
    }
    match msg {
        WM_HOTKEY if wparam.0 == hotkey::TOGGLE_HOTKEY_ID as usize => {
            unsafe {
                let _ = PostMessageW(
                    hwnd,
                    taskbar_embed::WM_APP_TOGGLE_AUDIO,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            LRESULT(0)
        }
        WM_APP_RESTART => {
            RESTART_REQUESTED.store(true, Ordering::Relaxed);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            hotkey::unregister(hwnd);
            tray::shutdown(hwnd);
            taskbar_embed::shutdown(hwnd);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn setup_console_ctrl_handler() -> anyhow::Result<()> {
    unsafe {
        SetConsoleCtrlHandler(Some(console_ctrl_handler), true)?;
    }
    Ok(())
}

extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            let hwnd_addr = EXIT_HWND_ADDR.load(Ordering::Relaxed);
            if hwnd_addr != 0 {
                unsafe {
                    let _ = PostMessageW(HWND(hwnd_addr as _), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            BOOL(1)
        }
        CTRL_CLOSE_EVENT => {
            if UiConfig::EXIT_ON_CONSOLE_CLOSE {
                let hwnd_addr = EXIT_HWND_ADDR.load(Ordering::Relaxed);
                if hwnd_addr != 0 {
                    unsafe {
                        let _ = PostMessageW(HWND(hwnd_addr as _), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
            BOOL(1)
        }
        _ => BOOL(0),
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod desktop_tests;
