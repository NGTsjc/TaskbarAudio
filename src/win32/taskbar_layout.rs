use crate::config::UiConfig;
use crate::config::user::get_user_config;
use crate::util::win32::get_window_rect;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetSystemMetrics, SetWindowPos,
    SM_CXSCREEN, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutResult {
    Invalid,
    Unchanged,
    Updated,
}

pub fn calculate_layout(shell: HWND) -> anyhow::Result<(i32, i32, i32, i32)> {
    unsafe {
        let mut rc = windows::Win32::Foundation::RECT::default();
        GetClientRect(shell, &mut rc)?;
        let client_w = rc.right - rc.left;
        let client_h = rc.bottom - rc.top;
        if client_w <= 0 || client_h <= 0 {
            return Ok((0, 0, 0, 0));
        }
        let panel_width = UiConfig::PANEL_WIDTH_PX.min(client_w).max(1);
        let user_config = get_user_config();
        let position = user_config.position.as_str();
        let offset_x = user_config.offset_x;
        let base_x = match position {
            "left" => 0,
            "right" => (client_w - panel_width).max(0),
            _ => {
                let shell_rect = get_window_rect(shell)?;
                let shell_w = shell_rect.right - shell_rect.left;
                let inset_x = (shell_w - client_w) / 2;
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let target_center_screen = screen_w / 2;
                let target_left_screen = (target_center_screen - (panel_width / 2)).max(0);
                (target_left_screen - shell_rect.left - inset_x).clamp(0, (client_w - panel_width).max(0))
            }
        };
        let x = (base_x + offset_x).clamp(0, (client_w - panel_width).max(0));
        Ok((x, 0, panel_width, client_h))
    }
}

pub fn update_layout(
    shell: HWND,
    panel_hwnd: HWND,
    last_layout: Option<(i32, i32, i32, i32)>,
) -> anyhow::Result<LayoutResult> {
    let layout = calculate_layout(shell)?;
    if layout == (0, 0, 0, 0) {
        return Ok(LayoutResult::Invalid);
    }
    if last_layout == Some(layout) {
        return Ok(LayoutResult::Unchanged);
    }
    unsafe {
        let _ = SetWindowPos(
            panel_hwnd,
            HWND_TOP,
            layout.0,
            layout.1,
            layout.2,
            layout.3,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(LayoutResult::Updated)
}