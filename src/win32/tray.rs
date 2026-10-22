use crate::win32::taskbar_embed;
use crate::win32::message_loop;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HICON, IDI_APPLICATION, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
};
use windows::core::w;

const TRAY_ICON_ID: u32 = 1;
const WM_APP_TRAY: u32 = WM_APP + 2;
const IDM_EXIT: u32 = 1001;
const IDM_RETRY_ATTACH: u32 = 1002;
const IDM_RESTART: u32 = 1003;
const IDM_TOGGLE_AUDIO: u32 = 1004;

pub fn init(message_hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        let mut nid = build_nid(message_hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP);
        nid.uCallbackMessage = WM_APP_TRAY;
        nid.hIcon = load_icon();
        set_tip(&mut nid, "TaskbarAudio");
        let ok = Shell_NotifyIconW(NIM_ADD, &mut nid).as_bool();
        anyhow::ensure!(ok, "Shell_NotifyIconW(NIM_ADD) failed");
    }
    Ok(())
}

pub fn shutdown(message_hwnd: HWND) {
    unsafe {
        let mut nid = build_nid(message_hwnd, NOTIFY_ICON_DATA_FLAGS(0));
        let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}

pub fn handle_message(message_hwnd: HWND, msg: u32, _wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg != WM_APP_TRAY {
        return None;
    }
    let event = lparam.0 as u32;
    match event {
        WM_RBUTTONUP => {
            show_context_menu(message_hwnd);
            Some(LRESULT(0))
        }
        WM_LBUTTONDBLCLK => {
            if taskbar_embed::is_paused(message_hwnd) {
                unsafe {
                    let _ = PostMessageW(message_hwnd, taskbar_embed::WM_APP_TOGGLE_AUDIO, WPARAM(0), LPARAM(0));
                }
            } else if taskbar_embed::is_tray_only(message_hwnd) {
                unsafe {
                    let _ = PostMessageW(message_hwnd, taskbar_embed::WM_APP_RETRY_ATTACH, WPARAM(0), LPARAM(0));
                }
            }
            Some(LRESULT(0))
        }
        _ => Some(LRESULT(0)),
    }
}

fn show_context_menu(message_hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let paused = taskbar_embed::is_paused(message_hwnd);
        let label = if paused { w!("显示音频") } else { w!("隐藏音频") };
        let _ = AppendMenuW(menu, MF_STRING, IDM_TOGGLE_AUDIO as usize, label);
        if !paused && taskbar_embed::is_tray_only(message_hwnd) {
            let _ = AppendMenuW(menu, MF_STRING, IDM_RETRY_ATTACH as usize, w!("重试附加"));
        }
        let _ = AppendMenuW(menu, MF_STRING, IDM_RESTART as usize, w!("重启"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("退出"));
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(message_hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            message_hwnd,
            None,
        );
        match cmd.0 as u32 {
            IDM_TOGGLE_AUDIO => {
                let _ = PostMessageW(message_hwnd, taskbar_embed::WM_APP_TOGGLE_AUDIO, WPARAM(0), LPARAM(0));
            }
            IDM_EXIT => {
                let _ = PostMessageW(message_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            IDM_RETRY_ATTACH => {
                let _ = PostMessageW(message_hwnd, taskbar_embed::WM_APP_RETRY_ATTACH, WPARAM(0), LPARAM(0));
            }
            IDM_RESTART => {
                let _ = PostMessageW(message_hwnd, message_loop::WM_APP_RESTART, WPARAM(0), LPARAM(0));
            }
            _ => {}
        }
        let _ = DestroyMenu(menu);
    }
}

unsafe fn load_icon() -> HICON {
    let instance = unsafe { GetModuleHandleW(None) }.ok();
    if let Some(instance) = instance {
        if let Ok(icon) = unsafe { LoadIconW(instance, w!("MAINICON")) } {
            if !icon.is_invalid() {
                return icon;
            }
        }
    }
    unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default()
}

unsafe fn build_nid(message_hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = message_hwnd;
    nid.uID = TRAY_ICON_ID;
    nid.uFlags = flags;
    nid
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    nid.szTip.fill(0);
    for (i, ch) in tip.encode_utf16().take(nid.szTip.len().saturating_sub(1)).enumerate() {
        nid.szTip[i] = ch;
    }
}
