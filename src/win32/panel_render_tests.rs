use super::*;
use crate::util::dpi::DpiScope;
use windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
};
use windows::core::w;

struct Surface {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

#[test]
fn resume_keeps_pixel_bounds_and_thicker_bars_at_every_dpi() {
    let _dpi_scope = DpiScope::per_monitor().unwrap();
    let (width, height) = (UiConfig::PANEL_WIDTH_PX, 72_i32);
    let mut surface = Surface {
        hwnd: unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                width,
                height,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap(),
        dc: unsafe { CreateCompatibleDC(None) },
        bitmap: HBITMAP::default(),
        previous: HGDIOBJ::default(),
    };
    assert!(!surface.dc.is_invalid());
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    surface.bitmap =
        unsafe { CreateDIBSection(surface.dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.unwrap();
    surface.previous = unsafe { SelectObject(surface.dc, surface.bitmap) };
    let mut renderer = PanelRenderer::new(surface.hwnd).unwrap();
    let target = unsafe {
        renderer
            .factory
            .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            })
    }
    .unwrap();
    unsafe {
        target.BindDC(
            surface.dc,
            &RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
        )
    }
    .unwrap();
    let rt: ID2D1RenderTarget = target.cast().unwrap();
    renderer.hwnd_target = None;
    renderer.target = Some(rt.clone());
    renderer.recreate_brushes().unwrap();
    let mut expected_bounds = [None, None];
    for inherited_dpi in [96.0, 144.0, 192.0, 144.0, 96.0] {
        // Simulate render-target DPI inherited when a panel is recreated.
        unsafe {
            rt.SetDpi(inherited_dpi, inherited_dpi);
        }
        renderer.on_dpi_changed(surface.hwnd).unwrap();
        let canvas = unsafe { rt.GetSize() };
        assert_eq!((canvas.width, canvas.height), (width as f32, height as f32));
        for (mode, line) in [false, true].into_iter().enumerate() {
            let energy = 0.0048923218;
            renderer.display_bars.fill(energy);
            renderer.peak_bars.fill(energy);
            unsafe {
                rt.BeginDraw();
                rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
                rt.Clear(Some(&color_rgba(1.0, 0.0, 1.0, 1.0)));
            }
            let size = D2D_SIZE_U {
                width: width as u32,
                height: height as u32,
            };
            if line {
                renderer.draw_line(&rt, size);
            } else {
                renderer.draw_bars(&rt, size);
            }
            unsafe { rt.EndDraw(None, None) }.unwrap();
            unsafe {
                let _ = GdiFlush();
            }
            let pixels = unsafe {
                std::slice::from_raw_parts(bits as *const u32, (width * height) as usize)
            };
            let is_visible = |pixel: &u32| *pixel & 0x00ff_ffff != 0x00ff_00ff;
            let points: Vec<_> = pixels
                .iter()
                .enumerate()
                .filter(|(_, p)| is_visible(p))
                .map(|(i, _)| (i as i32 % width, i as i32 / width))
                .collect();
            assert!(
                points.len() > 100,
                "spectrum clipped at inherited DPI {inherited_dpi}"
            );
            let bounds = (
                points.iter().map(|p| p.0).min().unwrap(),
                points.iter().map(|p| p.1).min().unwrap(),
                points.iter().map(|p| p.0).max().unwrap(),
                points.iter().map(|p| p.1).max().unwrap(),
            );
            assert!(bounds.3 < height);
            if !line {
                assert_eq!(bounds.3, height - 1);
            }
            if let Some(expected) = expected_bounds[mode] {
                assert_eq!(bounds, expected);
            } else {
                expected_bounds[mode] = Some(bounds);
            }
            if !line {
                let row = &pixels[((height - 2) * width) as usize..((height - 1) * width) as usize];
                let mut runs = Vec::new();
                let mut run = 0;
                for pixel in row {
                    if is_visible(pixel) {
                        run += 1;
                    } else if run > 0 {
                        runs.push(run);
                        run = 0;
                    }
                }
                if run > 0 {
                    runs.push(run);
                }
                assert_eq!(runs.len(), SpectrumDefaults::BARS);
                assert!(
                    runs.iter().all(|width| *width == 5),
                    "bars too narrow: {runs:?}"
                );
            }
        }
    }
}
