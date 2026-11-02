use crate::localization::{self, UiText};
use crate::win32::message_loop;
use crate::win32::taskbar_embed;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFY_ICON_DATA_FLAGS, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, HMENU, IDI_APPLICATION,
    LoadIconW, MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CLOSE, WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP,
};
use windows::core::{HSTRING, w};

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
        let ok = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
        anyhow::ensure!(ok, "Shell_NotifyIconW(NIM_ADD) failed");
    }
    Ok(())
}

pub fn shutdown(message_hwnd: HWND) {
    unsafe {
        let nid = build_nid(message_hwnd, NOTIFY_ICON_DATA_FLAGS(0));
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub fn handle_message(
    message_hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
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
                    let _ = PostMessageW(
                        message_hwnd,
                        taskbar_embed::WM_APP_TOGGLE_AUDIO,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            } else if taskbar_embed::is_tray_only(message_hwnd) {
                unsafe {
                    let _ = PostMessageW(
                        message_hwnd,
                        taskbar_embed::WM_APP_RETRY_ATTACH,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
            Some(LRESULT(0))
        }
        _ => Some(LRESULT(0)),
    }
}

fn show_context_menu(message_hwnd: HWND) {
    unsafe {
        let paused = taskbar_embed::is_paused(message_hwnd);
        let menu = match ContextMenu::new(
            paused,
            taskbar_embed::is_tray_only(message_hwnd),
            localization::text(),
        ) {
            Ok(menu) => menu,
            Err(error) => {
                tracing::warn!(%error, "tray menu creation failed");
                return;
            }
        };
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(message_hwnd);
        let cmd = TrackPopupMenu(
            menu.0,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            message_hwnd,
            None,
        );
        // Complete the foreground-menu cycle so the next click can dismiss it.
        let _ = PostMessageW(message_hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        match cmd.0 as u32 {
            IDM_TOGGLE_AUDIO => {
                let _ = PostMessageW(
                    message_hwnd,
                    taskbar_embed::WM_APP_TOGGLE_AUDIO,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            IDM_EXIT => {
                let _ = PostMessageW(message_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            IDM_RETRY_ATTACH => {
                let _ = PostMessageW(
                    message_hwnd,
                    taskbar_embed::WM_APP_RETRY_ATTACH,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            IDM_RESTART => {
                let _ = PostMessageW(
                    message_hwnd,
                    message_loop::WM_APP_RESTART,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            _ => {}
        }
    }
}

struct ContextMenu(HMENU);

impl ContextMenu {
    fn new(paused: bool, tray_only: bool, text: &UiText) -> windows::core::Result<Self> {
        let menu = Self(unsafe { CreatePopupMenu()? });
        menu.append(
            IDM_TOGGLE_AUDIO,
            if paused {
                &text.show_visualizer
            } else {
                &text.hide_visualizer
            },
        )?;
        if !paused && tray_only {
            menu.append(IDM_RETRY_ATTACH, &text.retry_taskbar_attachment)?;
        }
        menu.append(IDM_RESTART, &text.restart)?;
        menu.append(IDM_EXIT, &text.exit)?;
        Ok(menu)
    }

    fn append(&self, id: u32, label: &str) -> windows::core::Result<()> {
        // Custom labels are literal text, not Win32 mnemonic definitions.
        let label = HSTRING::from(label.replace('&', "&&"));
        unsafe { AppendMenuW(self.0, MF_STRING, id as usize, &label) }
    }
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        let _ = unsafe { DestroyMenu(self.0) };
    }
}

unsafe fn load_icon() -> HICON {
    let instance = unsafe { GetModuleHandleW(None) }.ok();
    if let Some(instance) = instance
        && let Ok(icon) = unsafe { LoadIconW(instance, w!("MAINICON")) }
        && !icon.is_invalid()
    {
        return icon;
    }
    unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default()
}

unsafe fn build_nid(message_hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: message_hwnd,
        uID: TRAY_ICON_ID,
        uFlags: flags,
        ..Default::default()
    }
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    nid.szTip.fill(0);
    for (i, ch) in tip
        .encode_utf16()
        .take(nid.szTip.len().saturating_sub(1))
        .enumerate()
    {
        nid.szTip[i] = ch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuItemID, GetMenuStringW, MF_BYPOSITION,
    };

    #[test]
    fn native_menu_uses_custom_text_and_preserves_state_and_command_ids() {
        for text in [
            UiText::default(),
            UiText {
                show_visualizer: "\u{663e}\u{793a}".into(),
                hide_visualizer: "Masquer".into(),
                retry_taskbar_attachment: "Réessayer".into(),
                restart: "Reiniciar".into(),
                exit: "Save & Exit".into(),
            },
        ] {
            for (paused, tray_only) in [(false, false), (true, true), (false, true), (true, false)]
            {
                let menu = ContextMenu::new(paused, tray_only, &text).unwrap();
                let mut expected = vec![(
                    IDM_TOGGLE_AUDIO,
                    if paused {
                        &text.show_visualizer
                    } else {
                        &text.hide_visualizer
                    },
                )];
                if !paused && tray_only {
                    expected.push((IDM_RETRY_ATTACH, &text.retry_taskbar_attachment));
                }
                expected.extend([(IDM_RESTART, &text.restart), (IDM_EXIT, &text.exit)]);
                assert_eq!(unsafe { GetMenuItemCount(menu.0) } as usize, expected.len());
                for (position, (id, label)) in expected.into_iter().enumerate() {
                    assert_eq!(unsafe { GetMenuItemID(menu.0, position as i32) }, id);
                    let mut buffer = [0u16; 128];
                    let len = unsafe {
                        GetMenuStringW(menu.0, position as u32, Some(&mut buffer), MF_BYPOSITION)
                    };
                    assert_eq!(
                        String::from_utf16(&buffer[..len as usize]).unwrap(),
                        label.replace('&', "&&")
                    );
                }
            }
        }
    }
}
