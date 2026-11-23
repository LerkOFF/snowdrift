//! The winter decorations as pictures made on the processor: the frost kept as a picture that is only touched up
//! where it changed, and a layer with the frost, the garlands and the snowmen composed, which the Windows version
//! hands to the graphics card in the pieces that changed.

use crate::festive::{BALL_REACH, BallSprite, GARLAND_ABOVE, GARLAND_DEPTH, GarlandSprite, ball_pixel, frost_pixel, garland_pixel};
use crate::simulation::SnowSimulation;

/// A rectangle of pixels: left, top, right, bottom, the last two not included.
pub type Pixels = [usize; 4];

fn union(a: Option<Pixels>, b: Pixels) -> Pixels {
    match a {
        Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
        None => b,
    }
}

fn meets(a: Pixels, b: Pixels) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// Premultiplied source-over into BGRA bytes.
#[inline]
fn put(pixels: &mut [u8], at: usize, c: [f32; 4]) {
    if c[3] <= 0.0 {
        return;
    }
    let keep = 1.0 - c[3].min(1.0);
    let p = &mut pixels[at..at + 4];
    p[0] = (c[2].min(1.0) * 255.0 + p[0] as f32 * keep + 0.5).min(255.0) as u8;
    p[1] = (c[1].min(1.0) * 255.0 + p[1] as f32 * keep + 0.5).min(255.0) as u8;
    p[2] = (c[0].min(1.0) * 255.0 + p[2] as f32 * keep + 0.5).min(255.0) as u8;
    p[3] = (c[3].min(1.0) * 255.0 + p[3] as f32 * keep + 0.5).min(255.0) as u8;
}

/// The frost as a picture of the display's pixels (BGRA, premultiplied), redone only where it changed.
pub struct FrostLayer {
    pub width: usize,
    pub height: usize,
    /// Pixels to a point of the simulation.
    pub scale: f32,
    pub pixels: Vec<u8>,
    painted: bool,
}

impl FrostLayer {
    pub fn new(width: usize, height: usize, scale: f32) -> FrostLayer {
        FrostLayer { width, height, scale, pixels: vec![0; width * height * 4], painted: false }
    }

    /// Brings the picture up to date; says which pixels changed, if any.
    pub fn update(&mut self, simulation: &mut SnowSimulation) -> Option<Pixels> {
        let frost = &mut simulation.frost;
        if !frost.active() || frost.progress <= 0.0 {
            // Wiped glass and a still-growing frost need their changes taken either way.
            let _ = frost.take_dirty();
            if self.painted {
                self.pixels.fill(0);
                self.painted = false;
                return Some([0, 0, self.width, self.height]);
            }
            return None;
        }
        let rect = match frost.take_dirty() {
            None if self.painted => return None,
            None | Some(None) => [0, 0, self.width, self.height],
            Some(Some([l, t, r, b])) => {
                let s = self.scale;
                [((l as f32 * s).floor() as usize).min(self.width), ((t as f32 * s).floor() as usize).min(self.height), ((r as f32 * s).ceil() as usize).min(self.width), ((b as f32 * s).ceil() as usize).min(self.height)]
            }
        };
        self.painted = true;
        for py in rect[1]..rect[3] {
            let y = ((py as f32 / self.scale) as usize).min(frost.height - 1);
            for px in rect[0]..rect[2] {
                let x = ((px as f32 / self.scale) as usize).min(frost.width - 1);
                let c = frost_pixel(frost, x, y, 0.0);
                let at = (py * self.width + px) * 4;
                self.pixels[at] = (c[2].min(1.0) * 255.0 + 0.5) as u8;
                self.pixels[at + 1] = (c[1].min(1.0) * 255.0 + 0.5) as u8;
                self.pixels[at + 2] = (c[0].min(1.0) * 255.0 + 0.5) as u8;
                self.pixels[at + 3] = (c[3].min(1.0) * 255.0 + 0.5) as u8;
            }
        }
        Some(rect)
    }
}

/// All the decorations in one picture of the display's pixels (BGRA, premultiplied): snowmen and garlands, with the
/// frost over them.
pub struct DecorLayer {
    pub width: usize,
    pub height: usize,
    pub scale: f32,
    pub pixels: Vec<u8>,
    frost: FrostLayer,
    /// Where the garlands and snowmen were drawn the last time.
    drawn: Vec<Pixels>,
    fresh: bool,
}

impl DecorLayer {
    pub fn new(width: usize, height: usize, scale: f32) -> DecorLayer {
        DecorLayer { width, height, scale, pixels: vec![0; width * height * 4], frost: FrostLayer::new(width, height, scale), drawn: Vec::new(), fresh: true }
    }

    fn clip(&self, left: f32, top: f32, right: f32, bottom: f32) -> Option<Pixels> {
        let r = [left.floor().max(0.0) as usize, top.floor().max(0.0) as usize, (right.ceil().max(0.0) as usize).min(self.width), (bottom.ceil().max(0.0) as usize).min(self.height)];
        (r[0] < r[2] && r[1] < r[3]).then_some(r)
    }

    fn garland_area(&self, garland: &GarlandSprite) -> Option<Pixels> {
        let (s, h) = (self.scale, self.height as f32);
        let top = h - garland.anchor.y * s;
        self.clip((garland.anchor.x - 4.0) * s, top - GARLAND_ABOVE * s, (garland.anchor.x + garland.width + 4.0) * s, top + GARLAND_DEPTH * s)
    }

    fn ball_area(&self, ball: &BallSprite) -> Option<Pixels> {
        let (s, h) = (self.scale, self.height as f32);
        let (cx, cy) = (ball.center.x * s, h - ball.center.y * s);
        let reach = ball.radius * BALL_REACH * s;
        self.clip(cx - reach, cy - reach, cx + reach, cy + reach)
    }

    fn draw_garland(&mut self, garland: &GarlandSprite, area: Pixels, time: f32) {
        let Some(own) = self.garland_area(garland) else { return };
        let (s, h) = (self.scale, self.height as f32);
        let top = h - garland.anchor.y * s;
        let (x0, y0, x1, y1) = (own[0].max(area[0]), own[1].max(area[1]), own[2].min(area[2]), own[3].min(area[3]));
        for py in y0..y1 {
            for px in x0..x1 {
                let sx = (px as f32 + 0.5) / s;
                let c = garland_pixel(sx - garland.anchor.x, (py as f32 + 0.5 - top) / s, garland.width, garland.seed, time, &garland.angles, &garland.lifts);
                put(&mut self.pixels, (py * self.width + px) * 4, c);
            }
        }
    }

    fn draw_ball(&mut self, ball: &BallSprite, area: Pixels) {
        let Some(own) = self.ball_area(ball) else { return };
        let (s, h) = (self.scale, self.height as f32);
        let (cx, cy) = (ball.center.x * s, h - ball.center.y * s);
        let radius = (ball.radius * s).max(1.0);
        let pixel = 1.0 / radius;
        let (x0, y0, x1, y1) = (own[0].max(area[0]), own[1].max(area[1]), own[2].min(area[2]), own[3].min(area[3]));
        for py in y0..y1 {
            for px in x0..x1 {
                let (lx, ly) = ((px as f32 + 0.5 - cx) / radius, -(py as f32 + 0.5 - cy) / radius);
                let noise = ((px as f32 * 12.9898 + py as f32 * 78.233).sin() * 43758.5453).fract().abs();
                put(&mut self.pixels, (py * self.width + px) * 4, ball_pixel(lx, ly, pixel, ball, noise));
            }
        }
    }

    /// Redoes one rectangle: cleared, the snowmen and garlands in it, the frost over them.
    fn refresh(&mut self, area: Pixels, garlands: &[GarlandSprite], balls: &[BallSprite], time: f32) {
        for py in area[1]..area[3] {
            let row = (py * self.width + area[0]) * 4;
            self.pixels[row..row + (area[2] - area[0]) * 4].fill(0);
        }
        for ball in balls {
            self.draw_ball(ball, area);
        }
        for garland in garlands {
            self.draw_garland(garland, area, time);
        }
        for py in area[1]..area[3] {
            for px in area[0]..area[2] {
                let at = (py * self.width + px) * 4;
                let f = &self.frost.pixels[at..at + 4];
                if f[3] == 0 {
                    continue;
                }
                let c = [f[2] as f32 / 255.0, f[1] as f32 / 255.0, f[0] as f32 / 255.0, f[3] as f32 / 255.0];
                put(&mut self.pixels, at, c);
            }
        }
    }

    /// Brings the picture up to date with the simulation; says which rectangles of pixels changed.
    pub fn update(&mut self, simulation: &mut SnowSimulation, time: f32) -> Vec<Pixels> {
        let frost_changed = self.frost.update(simulation);
        let garlands = simulation.garlands();
        let balls = simulation.snowmen.sprites(simulation.clock());
        let mut now: Vec<Pixels> = garlands.iter().filter_map(|g| self.garland_area(g)).collect();
        now.extend(balls.iter().filter_map(|b| self.ball_area(b)));

        if self.fresh {
            self.fresh = false;
            let all = [0, 0, self.width, self.height];
            self.refresh(all, &garlands, &balls, time);
            self.drawn = now;
            return vec![all];
        }
        let mut redo: Vec<Pixels> = std::mem::take(&mut self.drawn);
        redo.extend(now.iter().copied());
        if let Some(rect) = frost_changed {
            redo.push(rect);
        }
        // Rectangles that overlap are done as one.
        let mut merged: Vec<Pixels> = Vec::new();
        for rect in redo {
            let mut joined = rect;
            loop {
                let Some(i) = merged.iter().position(|m| meets(*m, joined)) else { break };
                joined = union(Some(joined), merged.swap_remove(i));
            }
            merged.push(joined);
        }
        for &rect in &merged {
            self.refresh(rect, &garlands, &balls, time);
        }
        self.drawn = now;
        merged
    }

    /// A rectangle of the picture's rows, for uploading.
    pub fn rows(&self, area: Pixels) -> Vec<u8> {
        let mut bytes = Vec::with_capacity((area[2] - area[0]) * (area[3] - area[1]) * 4);
        for py in area[1]..area[3] {
            let row = (py * self.width + area[0]) * 4;
            bytes.extend_from_slice(&self.pixels[row..row + (area[2] - area[0]) * 4]);
        }
        bytes
    }
}
