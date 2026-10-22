use crate::config::UiConfig;
use crate::config::user::get_user_config;
use crate::util::dpi::DpiScope;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, SetWindowPos,
    HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutResult {
    Invalid,
    Unchanged,
    Updated,
}

pub fn calculate_layout(shell: HWND) -> anyhow::Result<(i32, i32, i32, i32)> {
    let _dpi_scope = DpiScope::for_window(shell)?;
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
            _ => (client_w - panel_width) / 2,
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
    let _dpi_scope = DpiScope::for_window(shell)?;
    let layout = calculate_layout(shell)?;
    if layout == (0, 0, 0, 0) {
        return Ok(LayoutResult::Invalid);
    }
    if last_layout == Some(layout) {
        // DPI changes or Explorer may move an existing child after we cached its
        // layout. Verify its real parent-relative rectangle before skipping.
        let mut rect = RECT::default();
        unsafe { GetWindowRect(panel_hwnd, &mut rect)?; }
        let mut corners = [POINT { x: rect.left, y: rect.top }, POINT { x: rect.right, y: rect.bottom }];
        unsafe { MapWindowPoints(HWND::default(), shell, &mut corners); }
        let actual = (corners[0].x, corners[0].y,
            corners[1].x - corners[0].x, corners[1].y - corners[0].y);
        if actual == layout { return Ok(LayoutResult::Unchanged); }
    }
    unsafe {
        SetWindowPos(
            panel_hwnd,
            HWND_TOP,
            layout.0,
            layout.1,
            layout.2,
            layout.3,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(LayoutResult::Updated)
}