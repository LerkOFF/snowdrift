//! A software canvas: premultiplied BGRA pixels, which is what an X server wants for a 32-bit window with alpha.
//! Everything is anti-aliased and drawn source-over.

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Canvas {
        Canvas { width, height, pixels: vec![0; width * height * 4] }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    /// Source-over of `rgb` (straight, 0 to 1) at coverage `a`.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, rgb: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 || a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        let i = (y as usize * self.width + x as usize) * 4;
        let keep = 1.0 - a;
        let px = &mut self.pixels[i..i + 4];
        px[0] = (rgb[2] * 255.0 * a + px[0] as f32 * keep + 0.5) as u8;
        px[1] = (rgb[1] * 255.0 * a + px[1] as f32 * keep + 0.5) as u8;
        px[2] = (rgb[0] * 255.0 * a + px[2] as f32 * keep + 0.5) as u8;
        px[3] = (a * 255.0 + px[3] as f32 * keep + 0.5) as u8;
    }

    /// Source-over of a premultiplied colour (red, green, blue, alpha, each 0 to 1).
    #[inline]
    pub fn over(&mut self, x: i32, y: i32, c: [f32; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 || c[3] <= 0.0 {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
        let keep = 1.0 - c[3].min(1.0);
        let px = &mut self.pixels[i..i + 4];
        px[0] = (c[2].min(1.0) * 255.0 + px[0] as f32 * keep + 0.5).min(255.0) as u8;
        px[1] = (c[1].min(1.0) * 255.0 + px[1] as f32 * keep + 0.5).min(255.0) as u8;
        px[2] = (c[0].min(1.0) * 255.0 + px[2] as f32 * keep + 0.5).min(255.0) as u8;
        px[3] = (c[3].min(1.0) * 255.0 + px[3] as f32 * keep + 0.5).min(255.0) as u8;
    }

    /// Source-over of a whole picture of the same size (premultiplied BGRA) that has mostly nothing in it.
    pub fn over_layer(&mut self, layer: &[u8]) {
        for (dst, src) in self.pixels.chunks_exact_mut(4).zip(layer.chunks_exact(4)) {
            if src[3] == 0 {
                continue;
            }
            let keep = 255 - src[3] as u32;
            for k in 0..4 {
                dst[k] = (src[k] as u32 + (dst[k] as u32 * keep + 127) / 255).min(255) as u8;
            }
        }
    }

    /// A soft round dot: full in the middle, fading to nothing at `radius`; `soft` of one fades from the centre.
    pub fn dot(&mut self, cx: f32, cy: f32, radius: f32, soft: f32, rgb_core: [f32; 3], rgb_edge: [f32; 3], alpha: f32) {
        let reach = radius + 1.0;
        let (x0, x1) = ((cx - reach).floor() as i32, (cx + reach).ceil() as i32);
        let (y0, y1) = ((cy - reach).floor() as i32, (cy + reach).ceil() as i32);
        let inner = 0.6 * (1.0 - soft);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let r = (dx * dx + dy * dy).sqrt() / radius.max(0.001);
                if r >= 1.0 + 0.5 / radius.max(0.5) {
                    continue;
                }
                let a = (1.0 - smooth(inner, 1.0, r)) * alpha;
                if a < 0.004 {
                    continue;
                }
                let k = (smooth(0.35, 1.0, r) * 0.65).min(1.0);
                let rgb = [lerp(rgb_core[0], rgb_edge[0], k), lerp(rgb_core[1], rgb_edge[1], k), lerp(rgb_core[2], rgb_edge[2], k)];
                self.blend(x, y, rgb, a);
            }
        }
    }

    /// A short smear from `from` to `to`, brighter at `to`: a fast flake seen through a camera's exposure.
    pub fn streak(&mut self, from: (f32, f32), to: (f32, f32), radius: f32, soft: f32, rgb: [f32; 3], alpha: f32) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        let steps = (length / (radius * 0.6).max(0.5)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let head = 0.15 + 0.85 * t.powf(1.5);
            self.dot(from.0 + dx * t, from.1 + dy * t, radius, soft, rgb, [rgb[0] * 0.65, rgb[1] * 0.7, rgb[2] * 0.85], alpha * head / (1.0 + steps as f32 * 0.35));
        }
    }

    /// Fills a polygon (any winding, even-odd) with coverage anti-aliasing, four sub-scanlines a pixel.
    pub fn polygon(&mut self, points: &[(f32, f32)], rgb: [f32; 3], alpha: f32) {
        if points.len() < 3 || alpha <= 0.0 {
            return;
        }
        let (mut min_y, mut max_y, mut min_x, mut max_x) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in points {
            min_y = min_y.min(p.1);
            max_y = max_y.max(p.1);
            min_x = min_x.min(p.0);
            max_x = max_x.max(p.0);
        }
        let y0 = min_y.floor().max(0.0) as i32;
        let y1 = (max_y.ceil() as i32).min(self.height as i32 - 1);
        let x_lo = min_x.floor().max(0.0) as i32;
        let x_hi = (max_x.ceil() as i32).min(self.width as i32 - 1);
        if y0 > y1 || x_lo > x_hi {
            return;
        }
        const SUB: usize = 4;
        let span = (x_hi - x_lo + 1) as usize;
        let mut cover = vec![0.0f32; span];
        let mut crossings: Vec<f32> = Vec::new();
        for y in y0..=y1 {
            cover.iter_mut().for_each(|c| *c = 0.0);
            for s in 0..SUB {
                let sy = y as f32 + (s as f32 + 0.5) / SUB as f32;
                crossings.clear();
                for i in 0..points.len() {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                        crossings.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                    }
                }
                crossings.sort_by(|a, b| a.total_cmp(b));
                for pair in crossings.chunks_exact(2) {
                    let (l, r) = (pair[0].max(x_lo as f32), pair[1].min((x_hi + 1) as f32));
                    if r <= l {
                        continue;
                    }
                    let (il, ir) = (l.floor() as i32, (r.ceil() as i32 - 1).min(x_hi));
                    for x in il..=ir {
                        let left = (x as f32).max(l);
                        let right = ((x + 1) as f32).min(r);
                        cover[(x - x_lo) as usize] += (right - left).max(0.0) / SUB as f32;
                    }
                }
            }
            for (k, c) in cover.iter().enumerate() {
                if *c > 0.002 {
                    self.blend(x_lo + k as i32, y, rgb, c * alpha);
                }
            }
        }
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A cheap hash of a point, between 0 and 1.
pub fn hash(x: f32, y: f32) -> f32 {
    let mut p = ((x * 123.34).fract(), (y * 456.21).fract());
    let d = p.0 * (p.0 + 45.32) + p.1 * (p.1 + 45.32);
    p.0 += d;
    p.1 += d;
    (p.0 * p.1).fract().abs()
}
