//! The snowplow's body: a small layered window that rides along the bottom of the display with the blade of the
//! simulated plow. The snow it scrapes belongs to the simulation; this only draws the machine, with Direct2D.

use std::time::Instant;

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_QUADRATIC_BEZIER_SEGMENT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use snowdrift_core::simulation::PlowState;

use snowdrift::card;

const CLASS: PCWSTR = w!("SnowdriftPlow");
/// The window in DIPs, and where the blade is inside it.
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 150.0;
const ORIGIN_X: f32 = 210.0;
/// How big the machine is drawn, as on the Mac.
const SIZE: f32 = 1.25;

pub struct PlowWindow {
    window: HWND,
    d2d: ID2D1Factory,
    imaging: IWICImagingFactory,
    shown: bool,
    started: Instant,
}

impl PlowWindow {
    pub fn new(shown_in_screenshots: bool) -> Result<PlowWindow> {
        card::register(CLASS, Some(procedure))?;
        unsafe {
            let window = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS,
                w!("Snowdrift plow"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )?;
            crate::screen::set_capture(window, shown_in_screenshots);
            Ok(PlowWindow {
                window,
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                shown: false,
                started: Instant::now(),
            })
        }
    }

    /// Follows the plow along the bottom of the work area: `bottom` is its y in physical pixels, `left` its x, and
    /// `pixels_per_dip` how many pixels the simulation's points are along the display.
    pub fn update(&mut self, plow: Option<PlowState>, left: f32, bottom: f32, pixels_per_dip: f32, scale: f32) {
        let Some(plow) = plow else {
            self.hide();
            return;
        };
        let size = ((WIDTH * scale).ceil() as i32, (HEIGHT * scale).ceil() as i32);
        if let Ok(pixels) = self.paint(&plow, size, scale) {
            let x = left + plow.tip * pixels_per_dip - ORIGIN_X * scale;
            let position = POINT { x: x.round() as i32, y: (bottom - size.1 as f32).round() as i32 };
            card::present(self.window, size, &pixels, Some(position));
            if !self.shown {
                unsafe {
                    let _ = ShowWindow(self.window, SW_SHOWNOACTIVATE);
                }
                self.shown = true;
            }
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.window, SW_HIDE);
            }
            self.shown = false;
        }
    }

    pub fn show_in_screenshots(&self, shown: bool) {
        crate::screen::set_capture(self.window, shown);
    }

    fn paint(&self, plow: &PlowState, size: (i32, i32), scale: f32) -> Result<Vec<u8>> {
        unsafe {
            let bitmap = self.imaging.CreateBitmap(size.0 as u32, size.1 as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties)?;
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let result = self.draw(&target, plow);
            target.EndDraw(None, None)?;
            result?;
            drop(target);
            let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (size.0 * 4) as u32, &mut pixels)?;
            Ok(pixels)
        }
    }

    /// Ground at the bottom of the window; y up from there, in the units the Mac drawing used.
    fn draw(&self, target: &ID2D1RenderTarget, plow: &PlowState) -> Result<()> {
        let t = self.started.elapsed().as_secs_f32();
        let up = |x: f32, y: f32| Vector2 { X: x, Y: y };
        let paint = Paint { target };
        unsafe {
            // The heap of snow in front of the blade, in plain window coordinates with y up.
            let flip_up = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: -1.0, M31: 0.0, M32: HEIGHT };
            target.SetTransform(&flip_up);
            let mound_h = 12.0 + 58.0 * (1.0 - (-plow.pushed / 15000.0).exp());
            let mound_w = 50.0 + mound_h * 0.8;
            if !plow.dumped && plow.pushed > 0.0 {
                let f = plow.dir;
                let end_x = ORIGIN_X + f * mound_w;
                let end_h = mound_h.min(24.0);
                let geometry = self.d2d.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.BeginFigure(up(ORIGIN_X, 0.0), D2D1_FIGURE_BEGIN_FILLED);
                sink.AddLine(up(ORIGIN_X, mound_h));
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: up(ORIGIN_X + f * mound_w * 0.35, mound_h * 1.02),
                    point2: up(ORIGIN_X + f * mound_w * 0.7, mound_h * 0.5),
                    point3: up(end_x, end_h),
                });
                sink.AddLine(up(end_x, 0.0));
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                target.FillGeometry(&geometry, &paint.brush(1.0, 1.0, 1.0, 0.97)?, None);
            }

            // The machine, facing the way the plow goes.
            let machine = Matrix3x2 { M11: SIZE * plow.dir, M12: 0.0, M21: 0.0, M22: -SIZE, M31: ORIGIN_X, M32: HEIGHT };
            target.SetTransform(&machine);
            self.machine(&paint, plow, t)?;
            target.SetTransform(&Matrix3x2::identity());
        }
        Ok(())
    }

    fn machine(&self, p: &Paint, plow: &PlowState, t: f32) -> Result<()> {
        let tau = std::f32::consts::TAU;
        // The shadow under the machine.
        p.oval(-136.0, -3.0, 142.0, 9.0, [0.0, 0.0, 0.0, 0.28])?;
        // Smoke from the exhaust, drifting back.
        for i in 0..5 {
            let ph = (t * 0.7 + i as f32 / 5.0).fract();
            let r = 2.5 + ph * 7.0;
            p.circle(-70.0 - ph * 26.0, 82.0 + ph * 22.0, r, [0.82, 0.82, 0.85, 0.38 * (1.0 - ph)])?;
        }
        // Frame and rear axle.
        p.round(-132.0, 12.0, 118.0, 14.0, 3.0, [0.16, 0.16, 0.18, 1.0])?;
        p.round(-132.0, 22.0, 118.0, 3.0, 1.5, [0.3, 0.3, 0.33, 1.0])?;
        // The gritter's hopper.
        p.polygon(&[(-130.0, 26.0), (-131.0, 56.0), (-68.0, 50.0), (-68.0, 26.0)], [0.92, 0.48, 0.08, 1.0])?;
        p.rect(-129.0, 34.0, 61.0, 4.0, [0.74, 0.34, 0.05, 1.0])?;
        p.rect(-131.0, 53.0, 63.0, 3.0, [1.0, 0.7, 0.3, 1.0])?;
        for i in 0..6 {
            let light = i % 2 == 0;
            p.rect(-131.0, 27.0 + i as f32 * 4.6, 9.0, 4.6, if light { [0.98, 0.85, 0.1, 1.0] } else { [0.1, 0.1, 0.1, 1.0] })?;
        }
        p.round(-128.0, 54.0, 58.0, 5.0, 2.5, [1.0, 1.0, 1.0, 0.97])?;
        // The cab.
        p.round(-66.0, 26.0, 42.0, 46.0, 7.0, [0.96, 0.52, 0.1, 1.0])?;
        p.round(-70.0, 67.0, 50.0, 7.0, 3.0, [0.8, 0.38, 0.06, 1.0])?;
        p.round(-68.0, 72.0, 46.0, 5.0, 2.5, [1.0, 1.0, 1.0, 0.97])?;
        p.round(-61.0, 38.0, 32.0, 27.0, 5.0, [0.13, 0.15, 0.18, 1.0])?;
        p.round(-59.0, 40.0, 28.0, 23.0, 4.0, [0.55, 0.78, 0.93, 1.0])?;
        p.polygon(&[(-57.0, 41.0), (-49.0, 41.0), (-41.0, 62.0), (-49.0, 62.0)], [1.0, 1.0, 1.0, 0.33])?;
        p.line((-36.0, 41.0), (-46.0, 52.0), 1.4, [0.1, 0.1, 0.12, 1.0])?;
        p.round(-39.0, 32.0, 10.0, 3.0, 1.5, [0.2, 0.2, 0.22, 1.0])?;
        // Hood and grille.
        p.round(-26.0, 26.0, 26.0, 23.0, 5.0, [0.98, 0.58, 0.14, 1.0])?;
        p.rect(-24.0, 36.0, 22.0, 2.0, [0.8, 0.38, 0.06, 1.0])?;
        p.round(-5.0, 28.0, 6.0, 18.0, 2.0, [0.12, 0.12, 0.14, 1.0])?;
        for i in 0..3 {
            p.rect(-4.0, 31.0 + i as f32 * 5.0, 4.0, 1.5, [0.5, 0.5, 0.55, 1.0])?;
        }
        // Headlight and its glow.
        for (r, a) in [(28.0, 0.10), (20.0, 0.14), (12.0, 0.2)] {
            p.circle(14.0, 39.0, r, [1.0, 0.95, 0.6, a])?;
        }
        p.circle(0.0, 41.0, 3.0, [1.0, 0.96, 0.7, 1.0])?;
        p.round(-8.0, 21.0, 14.0, 6.0, 2.0, [0.16, 0.16, 0.18, 1.0])?;
        // The hydraulic ram and the arm of the blade.
        p.line((-22.0, 22.0), (2.0, 20.0), 4.0, [0.2, 0.2, 0.22, 1.0])?;
        p.line((-14.0, 28.0), (5.0, 38.0), 2.0, [0.72, 0.72, 0.76, 1.0])?;
        // The blade: a curved yellow plate with a black rubber edge, bolts and ribs.
        unsafe {
            let plate = self.d2d.CreatePathGeometry()?;
            let sink = plate.Open()?;
            sink.BeginFigure(Vector2 { X: -4.0, Y: 9.0 }, D2D1_FIGURE_BEGIN_FILLED);
            for (control, to) in [((3.0, 5.0), (8.0, 6.0)), ((16.0, 32.0), (12.0, 58.0)), ((2.0, 63.0), (-8.0, 62.0))] {
                sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: Vector2 { X: control.0, Y: control.1 },
                    point2: Vector2 { X: to.0, Y: to.1 },
                });
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            p.target.FillGeometry(&plate, &p.brush(0.96, 0.78, 0.1, 1.0)?, None);
        }
        p.polygon(&[(-3.0, 16.0), (0.0, 16.0), (4.0, 56.0), (0.0, 58.0)], [1.0, 0.92, 0.5, 0.75])?;
        for y in [22.0, 36.0, 50.0] {
            p.rect(-2.0, y, 13.0, 2.0, [0.78, 0.58, 0.05, 1.0])?;
        }
        p.round(-5.0, 3.0, 14.0, 5.0, 1.5, [0.1, 0.1, 0.11, 1.0])?;
        for y in [18.0, 33.0, 48.0] {
            p.circle(5.3, y + 1.3, 1.3, [0.42, 0.42, 0.46, 1.0])?;
        }
        // Snow thrown off the blade.
        if plow.pushed > 0.0 {
            for i in 0..7 {
                let ph = (t * 1.7 + i as f32 / 7.0).fract();
                let r = 1.0 + 2.4 * (1.0 - ph);
                p.circle(10.0 + ph * 24.0, 20.0 + (i as f32 * 2.1).sin() * 10.0 + ph * 38.0, r, [1.0, 1.0, 1.0, 0.85 * (1.0 - ph)])?;
            }
        }
        // The wheels turn as the machine goes.
        let spin = -(plow.tip * plow.dir) / 14.0;
        for wx in [-44.0_f32, -104.0] {
            p.circle(wx, 14.0, 14.0, [0.08, 0.08, 0.09, 1.0])?;
            for k in 0..10 {
                let a = spin + k as f32 * tau / 20.0;
                let (s, c) = a.sin_cos();
                p.line((wx + c * 11.2, 14.0 + s * 11.2), (wx + c * 14.4, 14.0 + s * 14.4), 3.2, [0.2, 0.2, 0.22, 1.0])?;
            }
            p.circle(wx, 14.0, 7.0, [0.62, 0.63, 0.67, 1.0])?;
            p.circle(wx, 14.0, 2.5, [0.4, 0.4, 0.44, 1.0])?;
            for k in 0..5 {
                let a = spin + k as f32 * tau / 5.0;
                p.circle(wx + a.cos() * 4.6, 14.0 + a.sin() * 4.6, 1.0, [0.32, 0.32, 0.36, 1.0])?;
            }
            // The fender arch.
            unsafe {
                let arch = self.d2d.CreatePathGeometry()?;
                let sink = arch.Open()?;
                let point = |a: f32| Vector2 { X: wx + 17.0 * a.cos(), Y: 14.0 + 17.0 * a.sin() };
                sink.BeginFigure(point(0.08 * std::f32::consts::PI), windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_HOLLOW);
                for step in 1..=12 {
                    sink.AddLine(point((0.08 + 0.84 * step as f32 / 12.0) * std::f32::consts::PI));
                }
                sink.EndFigure(windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                p.target.DrawGeometry(&arch, &p.brush(0.8, 0.38, 0.06, 1.0)?, 3.5, None);
            }
        }
        // Exhaust pipe and the flasher.
        p.round(-72.0, 50.0, 5.0, 34.0, 2.0, [0.25, 0.25, 0.28, 1.0])?;
        p.round(-73.0, 82.0, 7.0, 4.0, 1.5, [0.12, 0.12, 0.14, 1.0])?;
        let on = (t * 4.0) as i32 % 2 == 0;
        p.round(-52.0, 74.0, 12.0, 3.0, 1.0, [0.2, 0.2, 0.22, 1.0])?;
        if on {
            for (r, a) in [(22.0, 0.12), (15.0, 0.18), (9.0, 0.26)] {
                p.circle(-46.0, 82.0, r, [1.0, 0.55, 0.1, a])?;
            }
        }
        p.circle(-46.0, 81.0, 5.0, [1.0, if on { 0.55 } else { 0.28 }, 0.1, if on { 1.0 } else { 0.6 }])?;
        Ok(())
    }
}

impl Drop for PlowWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}

struct Paint<'a> {
    target: &'a ID2D1RenderTarget,
}

impl Paint<'_> {
    fn brush(&self, r: f32, g: f32, b: f32, a: f32) -> Result<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush> {
        unsafe { self.target.CreateSolidColorBrush(&D2D1_COLOR_F { r, g, b, a }, None) }
    }

    fn solid(&self, c: [f32; 4]) -> Result<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush> {
        self.brush(c[0], c[1], c[2], c[3])
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) -> Result<()> {
        unsafe { self.target.FillRectangle(&D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h }, &self.solid(c)?) };
        Ok(())
    }

    fn round(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: [f32; 4]) -> Result<()> {
        let rect = D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h };
        unsafe { self.target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: radius, radiusY: radius }, &self.solid(c)?) };
        Ok(())
    }

    fn oval(&self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) -> Result<()> {
        let ellipse = D2D1_ELLIPSE { point: Vector2 { X: x + w / 2.0, Y: y + h / 2.0 }, radiusX: w / 2.0, radiusY: h / 2.0 };
        unsafe { self.target.FillEllipse(&ellipse, &self.solid(c)?) };
        Ok(())
    }

    fn circle(&self, x: f32, y: f32, r: f32, c: [f32; 4]) -> Result<()> {
        let ellipse = D2D1_ELLIPSE { point: Vector2 { X: x, Y: y }, radiusX: r, radiusY: r };
        unsafe { self.target.FillEllipse(&ellipse, &self.solid(c)?) };
        Ok(())
    }

    fn line(&self, from: (f32, f32), to: (f32, f32), width: f32, c: [f32; 4]) -> Result<()> {
        unsafe { self.target.DrawLine(Vector2 { X: from.0, Y: from.1 }, Vector2 { X: to.0, Y: to.1 }, &self.solid(c)?, width, None) };
        Ok(())
    }

    fn polygon(&self, points: &[(f32, f32)], c: [f32; 4]) -> Result<()> {
        unsafe {
            let factory = self.target.GetFactory()?;
            let geometry = factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            sink.BeginFigure(Vector2 { X: points[0].0, Y: points[0].1 }, D2D1_FIGURE_BEGIN_FILLED);
            for point in &points[1..] {
                sink.AddLine(Vector2 { X: point.0, Y: point.1 });
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.target.FillGeometry(&geometry, &self.solid(c)?, None);
        }
        Ok(())
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: windows::Win32::Foundation::WPARAM, lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
