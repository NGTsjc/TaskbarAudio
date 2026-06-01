use std::sync::OnceLock;
use crate::config::UiConfig;
use crate::win32::{taskbar_embed, tray};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT,
    CTRL_SHUTDOWN_EVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HMENU, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WNDCLASSW,
};
use windows::core::w;

static EXIT_HWND_ADDR: OnceLock<isize> = OnceLock::new();

pub struct MessageLoop {
    hwnd: HWND,
}

impl MessageLoop {
    pub fn new() -> anyhow::Result<Self> {
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
                HWND_MESSAGE,
                HMENU::default(),
                instance,
                None,
            )?;
            anyhow::ensure!(hwnd.0 != std::ptr::null_mut(), "CreateWindowExW failed");
            let _ = EXIT_HWND_ADDR.set(hwnd.0 as isize);
            setup_console_ctrl_handler()?;
            taskbar_embed::init(hwnd)?;
            tray::init(hwnd)?;
            Ok(Self { hwnd })
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
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}

impl Drop for MessageLoop {
    fn drop(&mut self) {
        if self.hwnd.0 != std::ptr::null_mut() {
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(result) = tray::handle_message(hwnd, msg, wparam, lparam) {
        return result;
    }
    if let Some(result) = taskbar_embed::handle_message(hwnd, msg, wparam, lparam) {
        return result;
    }
    match msg {
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
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
            if let Some(hwnd_addr) = EXIT_HWND_ADDR.get() {
                unsafe {
                    let _ = PostMessageW(HWND(*hwnd_addr as _), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else {
                unsafe { PostQuitMessage(0) };
            }
            BOOL(1)
        }
        CTRL_CLOSE_EVENT => {
            if UiConfig::EXIT_ON_CONSOLE_CLOSE {
                if let Some(hwnd_addr) = EXIT_HWND_ADDR.get() {
                    unsafe {
                        let _ = PostMessageW(HWND(*hwnd_addr as _), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                } else {
                    unsafe { PostQuitMessage(0) };
                }
            }
            BOOL(1)
        }
        _ => BOOL(0),
    }
}
