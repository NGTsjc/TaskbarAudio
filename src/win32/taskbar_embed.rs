use crate::audio;
use crate::config::UiConfig;
use crate::util::win32::get_window_rect;
use crate::win32::panel_render::PanelRenderer;
use crate::win32::taskbar_layout::{self, LayoutResult};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, HWND, LPARAM, LRESULT, RECT, SetLastError, WIN32_ERROR, WPARAM, COLORREF,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromWindow, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, GetClientRect, GetWindowLongPtrW,
    GetParent, IsWindowVisible, KillTimer, PostMessageW, RegisterClassW,
    SetLayeredWindowAttributes, SetParent, SetTimer, SendMessageW, SetWindowLongPtrW,
    ShowWindow,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_EXSTYLE, GWL_STYLE, HMENU, HTTRANSPARENT,
    MA_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
    TIMERPROC, LWA_COLORKEY, WM_APP, WM_CREATE, WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_NCHITTEST, WM_NCDESTROY, WM_PAINT, WM_SIZE, WM_TIMER, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP,
};

const RETRY_TIMER_ID: usize = 1;
const RENDER_TIMER_ID: usize = 2;
const WATCHDOG_TIMER_ID: usize = 3;
pub const WM_APP_RETRY_ATTACH: u32 = WM_APP + 1;

pub fn init(message_hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        if GetWindowLongPtrW(message_hwnd, GWLP_USERDATA) != 0 {
            return Ok(());
        }
        let controller = Box::new(TaskbarEmbedController::new()?);
        let controller = Box::into_raw(controller);
        SetWindowLongPtrW(message_hwnd, GWLP_USERDATA, controller as isize);
    }
    unsafe {
        SetTimer(message_hwnd, WATCHDOG_TIMER_ID, UiConfig::WATCHDOG_INTERVAL_MS, None as TIMERPROC);
    }
    let attached = attempt_attach(message_hwnd);
    unsafe {
        if attached {
            let _ = KillTimer(message_hwnd, RETRY_TIMER_ID);
        } else {
            SetTimer(
                message_hwnd,
                RETRY_TIMER_ID,
                UiConfig::RETRY_ATTACH_INTERVAL_MS,
                None as TIMERPROC,
            );
        }
    }
    Ok(())
}

pub fn handle_message(message_hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_TIMER => {
            if wparam.0 == RETRY_TIMER_ID {
                let attached = attempt_attach(message_hwnd);
                unsafe {
                    if attached {
                        let _ = KillTimer(message_hwnd, RETRY_TIMER_ID);
                    }
                }
                return Some(LRESULT(0));
            }
            if wparam.0 == WATCHDOG_TIMER_ID {
                if let Some(controller) = unsafe { get_controller_mut(message_hwnd) } {
                    controller.watchdog_tick(message_hwnd);
                }
                return Some(LRESULT(0));
            }
            None
        }
        WM_APP_RETRY_ATTACH => {
            let attached = attempt_attach(message_hwnd);
            unsafe {
                if attached {
                    let _ = KillTimer(message_hwnd, RETRY_TIMER_ID);
                } else {
                    SetTimer(
                        message_hwnd,
                        RETRY_TIMER_ID,
                        UiConfig::RETRY_ATTACH_INTERVAL_MS,
                        None as TIMERPROC,
                    );
                }
            }
            Some(LRESULT(0))
        }
        _ => None,
    }
}

pub fn is_tray_only(message_hwnd: HWND) -> bool {
    unsafe { get_controller_mut(message_hwnd) }
        .map(|c| c.is_tray_only())
        .unwrap_or(true)
}

pub fn shutdown(message_hwnd: HWND) {
    unsafe {
        let ptr = GetWindowLongPtrW(message_hwnd, GWLP_USERDATA) as *mut TaskbarEmbedController;
        if !ptr.is_null() {
            SetWindowLongPtrW(message_hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
        }
        let _ = KillTimer(message_hwnd, RETRY_TIMER_ID);
        let _ = KillTimer(message_hwnd, WATCHDOG_TIMER_ID);
    }
}

#[allow(dead_code)]
pub fn request_retry_attach(message_hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(message_hwnd, WM_APP_RETRY_ATTACH, WPARAM(0), LPARAM(0));
    }
}

fn attempt_attach(message_hwnd: HWND) -> bool {
    let controller = unsafe { get_controller_mut(message_hwnd) };
    let Some(controller) = controller else { return false };
    match controller.try_attach() {
        Ok(true) => {
            tracing::info!("taskbar embed attached");
            true
        }
        Ok(false) => {
            tracing::warn!("taskbar embed unavailable, fallback to tray-only; will retry");
            controller.set_tray_only();
            false
        }
        Err(err) => {
            tracing::warn!(error = %err, "taskbar embed failed, fallback to tray-only; will retry");
            controller.set_tray_only();
            false
        }
    }
}

unsafe fn get_controller_mut(message_hwnd: HWND) -> Option<&'static mut TaskbarEmbedController> {
    let ptr = unsafe { GetWindowLongPtrW(message_hwnd, GWLP_USERDATA) as *mut TaskbarEmbedController };
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    }
}

struct TaskbarEmbedController {
    panel_hwnd: HWND,
    tray_only: bool,
    last_layout: Option<(i32, i32, i32, i32)>,
    watchdog_recover_count: u64,
    layout_update_count: u64,
    pending_show: bool,
    pending_since: Instant,
    stable_client_ticks: u32,
    last_shell_client: Option<(i32, i32)>,
}

struct PanelWindowState {
    renderer: PanelRenderer,
    stats: PanelRuntimeStats,
}

struct PanelRuntimeStats {
    last_log: Instant,
    frames: u32,
    render_time_accum: Duration,
    timer_interval_ms: u32,
    last_active: Instant,
}

impl PanelRuntimeStats {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            last_log: now,
            frames: 0,
            render_time_accum: Duration::from_millis(0),
            timer_interval_ms: UiConfig::RENDER_ACTIVE_INTERVAL_MS,
            last_active: now,
        }
    }
}

impl TaskbarEmbedController {
    fn new() -> anyhow::Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name = w!("TaskbarAudioPanel");
            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(panel_wnd_proc),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let ex_style: WINDOW_EX_STYLE =
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
            let style: WINDOW_STYLE = WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN;
            let panel_hwnd = CreateWindowExW(
                ex_style,
                class_name,
                w!("TaskbarAudio"),
                style,
                0,
                0,
                240,
                40,
                None,
                HMENU::default(),
                instance,
                None,
            )?;
            anyhow::ensure!(panel_hwnd.0 != std::ptr::null_mut(), "CreateWindowExW(panel) failed");
            let _ = SetLayeredWindowAttributes(
                panel_hwnd,
                COLORREF(UiConfig::COLORKEY_FUCHSIA_RGB),
                0,
                LWA_COLORKEY,
            );
            let _ = ShowWindow(panel_hwnd, SW_HIDE);
            Ok(Self {
                panel_hwnd,
                tray_only: true,
                last_layout: None,
                watchdog_recover_count: 0,
                layout_update_count: 0,
                pending_show: false,
                pending_since: Instant::now(),
                stable_client_ticks: 0,
                last_shell_client: None,
            })
        }
    }

    fn try_attach(&mut self) -> anyhow::Result<bool> {
        unsafe {
            if std::env::var_os("TASKBARAUDIO_DISABLE_EMBED").is_some() {
                return Ok(false);
            }
            let shell = match FindWindowW(w!("Shell_TrayWnd"), None) {
                Ok(h) => h,
                Err(_) => return Ok(false),
            };
            if shell.0 == std::ptr::null_mut() {
                return Ok(false);
            }
            if !is_bottom_taskbar(shell)? {
                return Ok(false);
            }
            let shell_rect = get_window_rect(shell)?;
            let taskbar_height = shell_rect.bottom - shell_rect.top;
            if taskbar_height <= 0 {
                return Ok(false);
            }
            self.attach_to_shell(shell)?;
            self.tray_only = false;
            self.pending_show = true;
            self.pending_since = Instant::now();
            self.stable_client_ticks = 0;
            self.last_shell_client = None;
            Ok(true)
        }
    }

    fn watchdog_tick(&mut self, message_hwnd: HWND) {
        if self.tray_only {
            return;
        }
        let Ok(shell) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else { return };
        if shell.0 == std::ptr::null_mut() {
            return;
        }
        let Ok(true) = is_bottom_taskbar(shell) else { return };
        unsafe {
            let parent = GetParent(self.panel_hwnd).unwrap_or(HWND(std::ptr::null_mut()));
            if parent.0 != shell.0 {
                match self.try_attach() {
                    Ok(true) => self.watchdog_recover_count = self.watchdog_recover_count.saturating_add(1),
                    _ => {
                        self.set_tray_only();
                        SetTimer(
                            message_hwnd,
                            RETRY_TIMER_ID,
                            UiConfig::RETRY_ATTACH_INTERVAL_MS,
                            None as TIMERPROC,
                        );
                        return;
                    }
                }
            }
        }
        let mut client = RECT::default();
        let _ = unsafe { GetClientRect(shell, &mut client) };
        let client_w = client.right - client.left;
        let client_h = client.bottom - client.top;

        let layout_result = taskbar_layout::update_layout(shell, self.panel_hwnd, self.last_layout).unwrap_or(LayoutResult::Invalid);
        if layout_result == LayoutResult::Updated {
            self.last_layout = Some(taskbar_layout::calculate_layout(shell).unwrap_or((0, 0, 0, 0)));
            self.layout_update_count = self.layout_update_count.saturating_add(1);
        }
        unsafe {
            if layout_result == LayoutResult::Invalid {
                self.pending_show = true;
                self.pending_since = Instant::now();
                self.stable_client_ticks = 0;
                self.last_shell_client = None;
                if IsWindowVisible(self.panel_hwnd).as_bool() {
                    let _ = ShowWindow(self.panel_hwnd, SW_HIDE);
                }
                return;
            }

            if client_w > 0 && client_h > 0 {
                if self.last_shell_client == Some((client_w, client_h)) {
                    self.stable_client_ticks = self.stable_client_ticks.saturating_add(1);
                } else {
                    self.stable_client_ticks = 0;
                    self.last_shell_client = Some((client_w, client_h));
                }
            }

            if self.pending_show {
                let should_show = self.stable_client_ticks >= 2
                    || self.pending_since.elapsed() >= Duration::from_millis(500);
                if should_show {
                    let _ = ShowWindow(self.panel_hwnd, SW_SHOWNOACTIVATE);
                    self.pending_show = false;
                    self.watchdog_recover_count = self.watchdog_recover_count.saturating_add(1);
                }
                return;
            }

            if !IsWindowVisible(self.panel_hwnd).as_bool() {
                let _ = ShowWindow(self.panel_hwnd, SW_SHOWNOACTIVATE);
                self.watchdog_recover_count = self.watchdog_recover_count.saturating_add(1);
            }
        }
    }

    fn attach_to_shell(&mut self, shell: HWND) -> anyhow::Result<()> {
        unsafe {
            SetLastError(WIN32_ERROR(0));
            let _prev = SetParent(self.panel_hwnd, shell);
            let err = GetLastError();
            anyhow::ensure!(err.0 == 0, "SetParent failed: {}", err.0);
            let old_style = GetWindowLongPtrW(self.panel_hwnd, GWL_STYLE) as u32;
            let mut new_style = WINDOW_STYLE(old_style);
            new_style &= !WS_POPUP;
            new_style |= WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN;
            SetWindowLongPtrW(self.panel_hwnd, GWL_STYLE, new_style.0 as isize);
            let required_ex: WINDOW_EX_STYLE =
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
            let old_ex = GetWindowLongPtrW(self.panel_hwnd, GWL_EXSTYLE) as u32;
            let mut new_ex = WINDOW_EX_STYLE(old_ex);
            new_ex |= required_ex;
            SetWindowLongPtrW(self.panel_hwnd, GWL_EXSTYLE, new_ex.0 as isize);
            verify_panel_non_interactive(self.panel_hwnd);
            Ok(())
        }
    }

    fn set_tray_only(&mut self) {
        unsafe {
            let _ = ShowWindow(self.panel_hwnd, SW_HIDE);
        }
        self.tray_only = true;
        self.last_layout = None;
        self.pending_show = false;
        self.stable_client_ticks = 0;
        self.last_shell_client = None;
    }

    fn is_tray_only(&self) -> bool {
        self.tray_only
    }
}

impl Drop for TaskbarEmbedController {
    fn drop(&mut self) {
        unsafe {
            if self.panel_hwnd.0 != std::ptr::null_mut() {
                let _ = DestroyWindow(self.panel_hwnd);
            }
        }
    }
}

fn is_bottom_taskbar(shell: HWND) -> anyhow::Result<bool> {
    unsafe {
        let shell_rect = get_window_rect(shell)?;
        let monitor = MonitorFromWindow(shell, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return Err(windows::core::Error::from_win32().into());
        }
        let bottom_delta = (mi.rcMonitor.bottom - shell_rect.bottom).abs();
        let is_bottom = bottom_delta <= 2 && shell_rect.top >= mi.rcMonitor.bottom - 200;
        Ok(is_bottom)
    }
}

extern "system" fn panel_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_CREATE => {
            if let Ok(renderer) = PanelRenderer::new(hwnd) {
                let state = Box::new(PanelWindowState {
                    renderer,
                    stats: PanelRuntimeStats::new(),
                });
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
                    SetTimer(
                        hwnd,
                        RENDER_TIMER_ID,
                        UiConfig::RENDER_ACTIVE_INTERVAL_MS,
                        None as TIMERPROC,
                    );
                }
            } else {
                tracing::warn!("panel renderer init failed; panel will be blank");
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == RENDER_TIMER_ID {
                let now = Instant::now();
                if let Some(state) = unsafe { get_panel_state_mut(hwnd) } {
                    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
                    let desired = if !visible {
                        UiConfig::RENDER_HIDDEN_INTERVAL_MS
                    } else {
                        let energy = state.renderer.energy();
                        if energy >= UiConfig::RENDER_ENERGY_ACTIVE_THRESHOLD {
                            state.stats.last_active = now;
                        }
                        let idle_for = now.duration_since(state.stats.last_active);
                        if idle_for >= Duration::from_millis(UiConfig::RENDER_DEEP_IDLE_AFTER_MS) {
                            UiConfig::RENDER_DEEP_IDLE_INTERVAL_MS
                        } else if idle_for >= Duration::from_millis(UiConfig::RENDER_IDLE_AFTER_MS) {
                            UiConfig::RENDER_IDLE_INTERVAL_MS
                        } else {
                            UiConfig::RENDER_ACTIVE_INTERVAL_MS
                        }
                    };
                    if visible {
                        unsafe {
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    if desired != state.stats.timer_interval_ms {
                        unsafe {
                            SetTimer(hwnd, RENDER_TIMER_ID, desired, None as TIMERPROC);
                        }
                        state.stats.timer_interval_ms = desired;
                    }
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SIZE => {
            if let Some(state) = unsafe { get_panel_state_mut(hwnd) } {
                if let Err(err) = state.renderer.on_size(hwnd) {
                    tracing::debug!(error = %err, "panel resize failed");
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(state) = unsafe { get_panel_state_mut(hwnd) } {
                if let Err(err) = state.renderer.on_dpi_changed(hwnd) {
                    tracing::debug!(error = %err, "panel dpi update failed");
                }
                if let Err(err) = state.renderer.on_size(hwnd) {
                    tracing::debug!(error = %err, "panel resize after dpi failed");
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            if let Some(state) = get_panel_state_mut(hwnd) {
                let start = Instant::now();
                if let Err(err) = state.renderer.render(hwnd) {
                    tracing::debug!(error = %err, "panel render failed");
                } else {
                    state.stats.frames = state.stats.frames.saturating_add(1);
                    state.stats.render_time_accum += start.elapsed();
                }
                if state.stats.last_log.elapsed() >= Duration::from_secs(10) {
                    let frames = state.stats.frames.max(1);
                    let elapsed_s = state.stats.last_log.elapsed().as_secs_f64().max(0.001);
                    let fps = frames as f64 / elapsed_s;
                    let avg_render_ms =
                        (state.stats.render_time_accum.as_secs_f64() * 1000.0) / frames as f64;
                    let queue_len = audio::global_stream()
                        .map(|s| s.queue().len())
                        .unwrap_or(0);
                    tracing::info!(
                        fps = fps,
                        avg_render_ms = avg_render_ms,
                        timer_ms = state.stats.timer_interval_ms,
                        energy = state.renderer.energy(),
                        audio_queue_len = queue_len,
                        device_recreate = state.renderer.device_recreate_count(),
                        "runtime stats"
                    );
                    state.stats.frames = 0;
                    state.stats.render_time_accum = Duration::from_millis(0);
                    state.stats.last_log = Instant::now();
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        },
        WM_NCDESTROY => unsafe {
            let _ = KillTimer(hwnd, RENDER_TIMER_ID);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PanelWindowState;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn get_panel_state_mut(hwnd: HWND) -> Option<&'static mut PanelWindowState> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PanelWindowState };
    if ptr.is_null() { None } else { Some(unsafe { &mut *ptr }) }
}

fn verify_panel_non_interactive(hwnd: HWND) {
    unsafe {
        let rect = match get_window_rect(hwnd) {
            Ok(r) => r,
            Err(_) => return,
        };
        let x = rect.left + ((rect.right - rect.left) / 2);
        let y = rect.top + ((rect.bottom - rect.top) / 2);
        let lparam = LPARAM((((y as u32) & 0xFFFF) << 16 | ((x as u32) & 0xFFFF)) as isize);
        let ht = SendMessageW(hwnd, WM_NCHITTEST, WPARAM(0), lparam).0 as u32;
        let ma = SendMessageW(hwnd, WM_MOUSEACTIVATE, WPARAM(0), LPARAM(0)).0 as u32;
        if ht != HTTRANSPARENT as u32 || ma != MA_NOACTIVATE as u32 {
            tracing::warn!(ht, ma, "panel non-interactive verification failed");
        }
    }
}
