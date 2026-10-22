use crate::audio::{self, SpectrumAnalyzer, SpectrumConfig};
use crate::config::{SpectrumDefaults, UiConfig};
use crate::config::user::get_user_config;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
    D2D1_GRADIENT_STOP, D2D_POINT_2F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1GradientStopCollection, ID2D1HwndRenderTarget,
    ID2D1LinearGradientBrush, ID2D1RenderTarget, ID2D1SolidColorBrush,
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_IMMEDIATELY,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::Interface;

pub struct PanelRenderer {
    factory: ID2D1Factory,
    hwnd_target: Option<ID2D1HwndRenderTarget>,
    target: Option<ID2D1RenderTarget>,
    bar_brush: Option<ID2D1LinearGradientBrush>,
    peak_brush: Option<ID2D1SolidColorBrush>,
    grad_stops: Option<ID2D1GradientStopCollection>,
    analyzer: SpectrumAnalyzer,
    bars: Vec<f32>,
    display_bars: Vec<f32>,
    peak_bars: Vec<f32>,
    last_energy: f32,
    device_recreate_count: u64,
    anim_start: std::time::Instant,
    last_anim_tick: std::time::Instant,
}

impl PanelRenderer {
    pub fn new(hwnd: HWND) -> anyhow::Result<Self> {
        let factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let analyzer = SpectrumAnalyzer::new(SpectrumConfig::default())?;
        let now = std::time::Instant::now();
        let mut this = Self {
            factory,
            hwnd_target: None,
            target: None,
            bar_brush: None,
            peak_brush: None,
            grad_stops: None,
            analyzer,
            bars: vec![0.0; SpectrumConfig::default().bars],
            display_bars: vec![0.0; SpectrumConfig::default().bars],
            peak_bars: vec![0.0; SpectrumConfig::default().bars],
            last_energy: 0.0,
            device_recreate_count: 0,
            anim_start: now,
            last_anim_tick: now,
        };
        this.ensure_device_resources(hwnd)?;
        Ok(this)
    }

    pub fn on_dpi_changed(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        // All layout constants and client sizes are pixels. Keep one drawing
        // unit equal to one pixel, including after hide/resume at 150% DPI.
        if let Some(rt) = &self.target {
            unsafe { rt.SetDpi(96.0, 96.0) };
        }
        self.on_size(hwnd)
    }

    pub fn on_size(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        let size = client_size_u(hwnd)?;
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if let Some(rt) = &self.hwnd_target
            && let Err(error) = unsafe { rt.Resize(&size) } {
            self.drop_device_resources();
            return Err(error.into());
        }
        Ok(())
    }

    pub fn render(&mut self, hwnd: HWND) -> anyhow::Result<bool> {
        self.ensure_device_resources(hwnd)?;
        self.poll_spectrum();
        
        let Some(rt) = self.target.clone() else { return Ok(false) };
        
        if self.last_energy < 1e-5 {
            self.display_bars.fill(0.0);
            self.peak_bars.fill(0.0);
            self.last_anim_tick = std::time::Instant::now();
            return self.clear_canvas(rt);
        }
        
        self.step_animation();
        let client_size = client_size_u(hwnd)?;
        unsafe {
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            rt.Clear(Some(&color_rgba(1.0, 0.0, 1.0, 1.0)));
            let display_mode = get_user_config().display_mode.as_str();
            match display_mode {
                "line" => self.draw_line(&rt, client_size),
                _ => self.draw_bars(&rt, client_size),
            }
            match rt.EndDraw(None, None) {
                Ok(_) => Ok(true),
                Err(err) => {
                    if err.code().0 == 0x8899000C_u32 as i32 {
                        self.drop_device_resources();
                        Ok(false)
                    } else {
                        Err(err.into())
                    }
                }
            }
        }
    }
    
    fn clear_canvas(&mut self, rt: ID2D1RenderTarget) -> anyhow::Result<bool> {
        unsafe {
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            rt.Clear(Some(&color_rgba(1.0, 0.0, 1.0, 1.0)));
            match rt.EndDraw(None, None) {
                Ok(_) => Ok(true),
                Err(err) => {
                    if err.code().0 == 0x8899000C_u32 as i32 {
                        self.drop_device_resources();
                        Ok(false)
                    } else {
                        Err(err.into())
                    }
                }
            }
        }
    }

    pub fn energy(&self) -> f32 {
        self.last_energy
    }

    pub fn device_recreate_count(&self) -> u64 {
        self.device_recreate_count
    }

    fn poll_spectrum(&mut self) {
        let Some(stream) = audio::global_stream() else { return };
        if let Some(frame) = stream.pop_latest_spectrum(&mut self.analyzer) {
            if frame.bars.len() == self.bars.len() {
                self.bars.copy_from_slice(&frame.bars);
            } else {
                self.bars = frame.bars;
            }
        } else {
            for v in &mut self.bars {
                *v = (*v * SpectrumDefaults::SILENT_DECAY).clamp(0.0, 1.0);
            }
        }
        self.last_energy = self.bars.iter().copied().fold(0.0f32, f32::max);
    }

    fn ensure_device_resources(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        if self.target.is_some() {
            return Ok(());
        }
        let size = client_size_u(hwnd)?;
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: size,
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };
        let hwnd_rt = unsafe { self.factory.CreateHwndRenderTarget(&rt_props, &hwnd_props)? };
        let rt: ID2D1RenderTarget = hwnd_rt.cast()?;
        self.hwnd_target = Some(hwnd_rt);
        self.target = Some(rt);
        if let Err(error) = self.recreate_brushes() {
            // Retry the whole resource set on the next frame after partial failure.
            self.drop_device_resources();
            return Err(error);
        }
        Ok(())
    }

    fn drop_device_resources(&mut self) {
        self.bar_brush = None;
        self.peak_brush = None;
        self.grad_stops = None;
        self.target = None;
        self.hwnd_target = None;
        self.device_recreate_count = self.device_recreate_count.saturating_add(1);
    }

    fn recreate_brushes(&mut self) -> anyhow::Result<()> {
        let Some(rt) = &self.target else { return Ok(()) };
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: color_rgba(
                    UiConfig::GRADIENT_BOTTOM_R,
                    UiConfig::GRADIENT_BOTTOM_G,
                    UiConfig::GRADIENT_BOTTOM_B,
                    1.0,
                ),
            },
            D2D1_GRADIENT_STOP {
                position: 0.6,
                color: color_rgba(
                    UiConfig::BAR_COLOR_R,
                    UiConfig::BAR_COLOR_G,
                    UiConfig::BAR_COLOR_B,
                    1.0,
                ),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: color_rgba(UiConfig::GRADIENT_TOP_R, UiConfig::GRADIENT_TOP_G, UiConfig::GRADIENT_TOP_B, 1.0),
            },
        ];
        let stops = unsafe {
            rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?
        };
        self.grad_stops = Some(stops.clone());
        let linear_props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F { x: 0.0, y: 0.0 },
            endPoint: D2D_POINT_2F { x: 1.0, y: 1.0 },
        };
        let bar = unsafe { rt.CreateLinearGradientBrush(&linear_props, None, &stops)? };
        self.bar_brush = Some(bar);
        let peak = unsafe {
            rt.CreateSolidColorBrush(
                &color_rgba(UiConfig::PEAK_COLOR_R, UiConfig::PEAK_COLOR_G, UiConfig::PEAK_COLOR_B, 1.0),
                None,
            )?
        };
        self.peak_brush = Some(peak);
        Ok(())
    }

    fn step_animation(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_anim_tick).as_secs_f32().clamp(0.0, 0.25);
        self.last_anim_tick = now;
        if self.display_bars.len() != self.bars.len() {
            self.display_bars.resize(self.bars.len(), 0.0);
            self.peak_bars.resize(self.bars.len(), 0.0);
        }
        let attack_tau = (SpectrumDefaults::VIS_ATTACK_MS / 1000.0).max(1e-3);
        let release_tau = (SpectrumDefaults::VIS_RELEASE_MS / 1000.0).max(1e-3);
        let a_up = 1.0 - (-dt / attack_tau).exp();
        let a_dn = 1.0 - (-dt / release_tau).exp();
        for i in 0..self.bars.len() {
            let target = self.bars[i].clamp(0.0, 1.0);
            let cur = self.display_bars[i];
            let next = if target > cur {
                cur + (target - cur) * a_up
            } else {
                cur + (target - cur) * a_dn
            };
            self.display_bars[i] = next;
            let peak = self.peak_bars[i];
            let peak_next = if next > peak {
                next
            } else {
                (peak - SpectrumDefaults::PEAK_FALL_PER_SEC * dt).max(next)
            };
            self.peak_bars[i] = peak_next;
        }
    }

    fn draw_bars(&mut self, rt: &ID2D1RenderTarget, client_size: D2D_SIZE_U) {
        let Some(bar_brush) = &self.bar_brush else { return };
        let Some(peak_brush) = &self.peak_brush else { return };
        let bars = &self.display_bars;
        if bars.is_empty() {
            return;
        }
        let size_w = client_size.width as f32;
        let size_h = client_size.height as f32;
        let left = UiConfig::CONTENT_PAD_X_PX;
        let right = (size_w - UiConfig::CONTENT_PAD_X_PX).max(left);
        let top = UiConfig::CONTENT_PAD_TOP_PX;
        let bottom = (size_h - UiConfig::CONTENT_PAD_BOTTOM_PX).max(top);
        let inner_h = (bottom - top).max(1.0);
        let t = self.anim_start.elapsed().as_secs_f32();
        let shimmer = (t * UiConfig::GRADIENT_SHIMMER_SPEED) % 1.0;
        let shift = (shimmer * size_w * 2.0) - size_w;
        unsafe {
            bar_brush.SetStartPoint(D2D_POINT_2F { x: left + shift, y: bottom });
            bar_brush.SetEndPoint(D2D_POINT_2F { x: right + shift, y: top });
        }
        let n = bars.len();
        if n == 0 {
            return;
        }
        let gap = UiConfig::BAR_GAP_PX;
        let left_i = left.floor() as i32;
        let right_i = right.floor() as i32;
        let gap_i = gap.round() as i32;
        let n_i = n as i32;
        let inner_w_i = (right_i - left_i).max(1);
        let total_gap_i = gap_i.saturating_mul((n_i - 1).max(0));
        let available_i = (inner_w_i - total_gap_i).max(n_i);
        let bar_w_i = (available_i / n_i).max(1);
        let remainder_total = (available_i - bar_w_i * n_i).max(0);
        unsafe {
            for (i, v) in bars.iter().copied().enumerate() {
                let i_i = i as i32;
                let extra_before = i_i.min(remainder_total);
                let extra = if i_i < remainder_total { 1 } else { 0 };
                let x0_i = left_i + i_i * (bar_w_i + gap_i) + extra_before;
                let x1_i = (x0_i + bar_w_i + extra).min(right_i);
                let x0 = x0_i as f32;
                let x1 = x1_i as f32;
                let v = (v * SpectrumDefaults::BAR_GAIN)
                    .clamp(0.0, 1.0)
                    .powf(SpectrumDefaults::BAR_POW);
                let h = (v * inner_h).max(UiConfig::MIN_BAR_HEIGHT_PX);
                let y0 = (bottom - h).max(top);
                let rect = D2D_RECT_F { left: x0, top: y0, right: x1, bottom };
                let glow = (SpectrumDefaults::COLOR_GLOW_BASE + v * SpectrumDefaults::COLOR_GLOW_RANGE)
                    .clamp(0.0, 1.0);
                let dim = 1.0 + (UiConfig::GRADIENT_SHIMMER_STRENGTH * (t * 3.5 + i as f32 * 0.22).sin());
                let opacity = (glow * dim).clamp(0.0, 1.0);
                bar_brush.SetOpacity(opacity);
                rt.FillRectangle(&rect, bar_brush);

                let peak_v = self.peak_bars[i].clamp(0.0, 1.0);
                let peak_mapped = (peak_v * SpectrumDefaults::BAR_GAIN)
                    .clamp(0.0, 1.0)
                    .powf(SpectrumDefaults::BAR_POW);
                let peak_y = (bottom - peak_mapped * inner_h).max(top);
                let peak_top = (peak_y - UiConfig::PEAK_HEIGHT_PX).max(top);
                let peak_bottom = (peak_y + UiConfig::PEAK_GAP_PX).min(bottom);
                let peak_rect = D2D_RECT_F { left: x0, top: peak_top, right: x1, bottom: peak_bottom };
                rt.FillRectangle(&peak_rect, peak_brush);
            }
            bar_brush.SetOpacity(1.0);
        }
    }

    fn draw_line(&mut self, rt: &ID2D1RenderTarget, client_size: D2D_SIZE_U) {
        let Some(bar_brush) = &self.bar_brush else { return };
        let bars = &self.display_bars;
        if bars.is_empty() {
            return;
        }
        let size_w = client_size.width as f32;
        let size_h = client_size.height as f32;
        let left = UiConfig::CONTENT_PAD_X_PX;
        let right = (size_w - UiConfig::CONTENT_PAD_X_PX).max(left);
        let top = UiConfig::CONTENT_PAD_TOP_PX;
        let bottom = (size_h - UiConfig::CONTENT_PAD_BOTTOM_PX).max(top);
        let inner_h = (bottom - top).max(1.0);
        let inner_w = (right - left).max(1.0);
        let t = self.anim_start.elapsed().as_secs_f32();
        let shimmer = (t * UiConfig::GRADIENT_SHIMMER_SPEED) % 1.0;
        let shift = (shimmer * size_w * 2.0) - size_w;
        unsafe {
            bar_brush.SetStartPoint(D2D_POINT_2F { x: left + shift, y: bottom });
            bar_brush.SetEndPoint(D2D_POINT_2F { x: right + shift, y: top });
        }
        let n = bars.len();
        if n < 2 {
            return;
        }
        let step_x = inner_w / (n - 1) as f32;
        let mut points = Vec::with_capacity(n);
        for (i, v) in bars.iter().copied().enumerate() {
            let v = (v * SpectrumDefaults::BAR_GAIN)
                .clamp(0.0, 1.0)
                .powf(SpectrumDefaults::BAR_POW);
            let h = (v * inner_h).max(UiConfig::MIN_BAR_HEIGHT_PX);
            let x = left + i as f32 * step_x;
            let y = bottom - h;
            points.push(D2D_POINT_2F { x, y });
        }
        unsafe {
            bar_brush.SetOpacity(1.0);
            for i in 0..points.len() - 1 {
                rt.DrawLine(points[i], points[i + 1], bar_brush, 2.0, None);
            }
        }
    }
}

fn client_size_u(hwnd: HWND) -> anyhow::Result<D2D_SIZE_U> {
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        let w = (rc.right - rc.left).max(0) as u32;
        let h = (rc.bottom - rc.top).max(0) as u32;
        Ok(D2D_SIZE_U { width: w, height: h })
    }
}

fn color_rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

#[cfg(test)]
#[path = "panel_render_tests.rs"]
mod tests;
