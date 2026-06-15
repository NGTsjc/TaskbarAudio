use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, GetClassNameW, IsWindow};

pub fn get_window_rect(hwnd: HWND) -> anyhow::Result<RECT> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect)?;
        Ok(rect)
    }
}

pub fn is_valid_shell_tray_wnd(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return false;
    }
    let mut class_name = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut class_name) };
    if len <= 0 {
        return false;
    }
    let name = String::from_utf16_lossy(&class_name[..len as usize]);
    name == "Shell_TrayWnd"
}