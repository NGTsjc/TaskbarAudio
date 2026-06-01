use crate::audio::{self, SpectrumAnalyzer, SpectrumConfig};
use crate::config::{SpectrumDefaults, UiConfig};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
    D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget, ID2D1SolidColorBrush,
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_IMMEDIATELY,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::Interface;

pub struct PanelRenderer {
    factory: ID2D1Factory,
    hwnd_target: Option<ID2D1HwndRenderTarget>,
    target: Option<ID2D1RenderTarget>,
    bar_brush: Option<ID2D1SolidColorBrush>,
    analyzer: SpectrumAnalyzer,
    bars: Vec<f32>,
    dpi: f32,
    last_energy: f32,
    device_recreate_count: u64,
}

impl PanelRenderer {
    pub fn new(hwnd: HWND) -> anyhow::Result<Self> {
        let factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let analyzer = SpectrumAnalyzer::new(SpectrumConfig::default())?;
        let dpi = window_dpi(hwnd);
        let mut this = Self {
            factory,
            hwnd_target: None,
            target: None,
            bar_brush: None,
            analyzer,
            bars: vec![0.0; SpectrumConfig::default().bars],
            dpi,
            last_energy: 0.0,
            device_recreate_count: 0,
        };
        this.ensure_device_resources(hwnd)?;
        Ok(this)
    }

    pub fn on_dpi_changed(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        self.dpi = window_dpi(hwnd);
        if let Some(rt) = &self.target {
            unsafe { rt.SetDpi(self.dpi, self.dpi) };
        }
        Ok(())
    }

    pub fn on_size(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        let size = client_size_u(hwnd)?;
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if let Some(rt) = &self.hwnd_target {
            unsafe { rt.Resize(&size)? };
        }
        Ok(())
    }

    pub fn render(&mut self, hwnd: HWND) -> anyhow::Result<()> {
        self.ensure_device_resources(hwnd)?;
        self.poll_spectrum();
        let Some(rt) = &self.target else { return Ok(()) };
        let client_size = client_size_u(hwnd)?;
        unsafe {
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            rt.Clear(Some(&color_rgba(1.0, 0.0, 1.0, 1.0)));
            self.draw_bars(rt, client_size);
            match rt.EndDraw(None, None) {
                Ok(_) => Ok(()),
                Err(err) => {
                    if err.code().0 == 0x8899000C_u32 as i32 {
                        self.drop_device_resources();
                        Ok(())
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
            dpiX: self.dpi,
            dpiY: self.dpi,
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
        self.recreate_brushes()?;
        Ok(())
    }

    fn drop_device_resources(&mut self) {
        self.bar_brush = None;
        self.target = None;
        self.hwnd_target = None;
        self.device_recreate_count = self.device_recreate_count.saturating_add(1);
    }

    fn recreate_brushes(&mut self) -> anyhow::Result<()> {
        let Some(rt) = &self.target else { return Ok(()) };
        let bar = unsafe {
            rt.CreateSolidColorBrush(
                &color_rgba(UiConfig::BAR_COLOR_R, UiConfig::BAR_COLOR_G, UiConfig::BAR_COLOR_B, 1.0),
                None,
            )?
        };
        self.bar_brush = Some(bar);
        Ok(())
    }

    fn draw_bars(&self, rt: &ID2D1RenderTarget, client_size: D2D_SIZE_U) {
        let Some(bar_brush) = &self.bar_brush else { return };
        let bars = &self.bars;
        if bars.is_empty() {
            return;
        }
        let size_w = client_size.width as f32;
        let size_h = client_size.height as f32;
        let left = UiConfig::CONTENT_PAD_X_PX;
        let right = (size_w - UiConfig::CONTENT_PAD_X_PX).max(left);
        let top = UiConfig::CONTENT_PAD_TOP_PX;
        let bottom = (size_h - UiConfig::CONTENT_PAD_BOTTOM_PX).max(top) + UiConfig::CONTENT_BOTTOM_FUDGE_PX;
        let inner_w = (right - left).max(1.0);
        let inner_h = (bottom - top).max(1.0);
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
                bar_brush.SetColor(&color_rgba(
                    UiConfig::BAR_COLOR_R * glow,
                    UiConfig::BAR_COLOR_G * glow,
                    UiConfig::BAR_COLOR_B * glow,
                    1.0,
                ));
                rt.FillRectangle(&rect, bar_brush);
            }
            bar_brush.SetColor(&color_rgba(UiConfig::BAR_COLOR_R, UiConfig::BAR_COLOR_G, UiConfig::BAR_COLOR_B, 1.0));
        }
    }
}

fn window_dpi(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 96.0 } else { dpi as f32 }
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
