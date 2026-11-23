//! Draws one display's simulation into a canvas: the drifts, the snowplow and everything in the air.

use snowdrift_core::decor::FrostLayer;
use snowdrift_core::festive::{BALL_REACH, GARLAND_ABOVE, GARLAND_DEPTH, ball_pixel, garland_pixel};
use snowdrift_core::simulation::{PlowState, Sprite, SnowSimulation};

use crate::canvas::{Canvas, hash, lerp, smooth};

/// How big the machine is drawn, as on the other systems.
const PLOW_SIZE: f32 = 1.25;

pub struct Frame {
    sprites: Vec<Sprite>,
    heights: Vec<f32>,
    /// The frost as a picture, touched up where it changed.
    frost: Option<FrostLayer>,
}

impl Frame {
    pub fn new() -> Frame {
        Frame { sprites: Vec::new(), heights: Vec::new(), frost: None }
    }

    /// `scale` is physical pixels to a point of the simulation; `time` is seconds since the start, for the little
    /// animations (glints, smoke, the flasher).
    pub fn draw(&mut self, canvas: &mut Canvas, simulation: &mut SnowSimulation, scale: f32, time: f32) {
        canvas.clear();
        self.sprites.clear();
        self.sprites.resize(simulation.sprite_count() + 64, Sprite::default());
        let count = simulation.write_sprites(&mut self.sprites);
        self.heights.clear();
        self.heights.resize(simulation.column_count() + 16, 0.0);
        let piles = simulation.write_layers(&mut self.heights);
        let height = canvas.height as f32;

        draw_balls(canvas, simulation, scale, height);
        for pile in &piles {
            let heights = &self.heights[pile.offset..pile.offset + 4 * pile.count];
            draw_pile(canvas, heights, pile.count, pile.origin.x, pile.origin.y, pile.top, pile.bottom, pile.occluders, scale, time);
        }
        if let Some(plow) = simulation.plow() {
            draw_plow(canvas, &plow, scale, time);
        }
        draw_garlands(canvas, simulation, scale, height, time);
        for sprite in &self.sprites[..count] {
            draw_sprite(canvas, sprite, scale, height);
        }
        let layer = self.frost.get_or_insert_with(|| FrostLayer::new(canvas.width, canvas.height, scale));
        if layer.width != canvas.width || layer.height != canvas.height || layer.scale != scale {
            *layer = FrostLayer::new(canvas.width, canvas.height, scale);
        }
        layer.update(simulation);
        canvas.over_layer(&layer.pixels);
    }
}

/// The string of lights along the top of the screen. `height` is the canvas height in pixels: the simulation's y
/// runs up from the bottom of the display.
fn draw_garlands(canvas: &mut Canvas, simulation: &SnowSimulation, scale: f32, height: f32, time: f32) {
    for garland in simulation.garlands() {
        let x0 = ((garland.anchor.x - 4.0) * scale).floor().max(0.0) as i32;
        let x1 = (((garland.anchor.x + garland.width + 4.0) * scale).ceil() as i32).min(canvas.width as i32);
        let top = height - garland.anchor.y * scale;
        let y0 = (top - GARLAND_ABOVE * scale).floor().max(0.0) as i32;
        let y1 = ((top + GARLAND_DEPTH * scale).ceil() as i32).min(canvas.height as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                // In points from the left end and the top the string hangs from.
                let (lx, ly) = ((px as f32 + 0.5) / scale - garland.anchor.x, (py as f32 + 0.5 - top) / scale);
                canvas.over(px, py, garland_pixel(lx, ly, garland.width, garland.seed, time, &garland.angles, &garland.lifts));
            }
        }
    }
}

/// The balls of snow and what the snowmen wear.
fn draw_balls(canvas: &mut Canvas, simulation: &SnowSimulation, scale: f32, height: f32) {
    for ball in simulation.snowmen.sprites(simulation.clock()) {
        let reach = ball.radius * BALL_REACH;
        let (cx, cy) = (ball.center.x * scale, height - ball.center.y * scale);
        let x0 = (cx - reach * scale).floor().max(0.0) as i32;
        let x1 = ((cx + reach * scale).ceil() as i32).min(canvas.width as i32);
        let y0 = (cy - reach * scale).floor().max(0.0) as i32;
        let y1 = ((cy + reach * scale).ceil() as i32).min(canvas.height as i32);
        let pixel = 1.0 / (ball.radius * scale).max(1.0);
        for py in y0..y1 {
            for px in x0..x1 {
                let lx = (px as f32 + 0.5 - cx) / (ball.radius * scale);
                let ly = -(py as f32 + 0.5 - cy) / (ball.radius * scale);
                canvas.over(px, py, ball_pixel(lx, ly, pixel, &ball, hash(px as f32, py as f32)));
            }
        }
    }
}


fn sample(values: &[f32], x: f32) -> f32 {
    let count = values.len();
    let f = x / 2.0 - 0.5;
    let base = f.floor();
    let i = (base.max(0.0) as usize).min(count - 1);
    let j = (i + 1).min(count - 1);
    lerp(values[i], values[j], (f - base).clamp(0.0, 1.0))
}

fn leaf_color(seed: f32, petals: f32) -> [f32; 3] {
    if petals > 0.5 {
        let t = (seed * 31.7).fract();
        return [0.98 - 0.05 * t, 0.78 + 0.14 * t, 0.84 + 0.1 * t];
    }
    const PALETTE: [[f32; 3]; 6] = [
        [0.85, 0.36, 0.1],
        [0.78, 0.2, 0.12],
        [0.93, 0.66, 0.15],
        [0.62, 0.38, 0.14],
        [0.88, 0.5, 0.12],
        [0.7, 0.55, 0.15],
    ];
    let pick = ((seed * 7.31).fract() * PALETTE.len() as f32) as usize % PALETTE.len();
    let shade = 0.88 + 0.2 * (seed * 17.3).fract();
    let c = PALETTE[pick];
    [(c[0] * shade).min(1.0), (c[1] * shade).min(1.0), (c[2] * shade).min(1.0)]
}

/// One drift, a pixel column at a time: `heights` holds the surface, the bed, and how much is leaves and petals, each
/// `count` long, in points up from `origin`.
#[allow(clippy::too_many_arguments)]
fn draw_pile(canvas: &mut Canvas, heights: &[f32], count: usize, origin_x: f32, origin_y: f32, top: f32, bottom: f32, occluders: &[[f32; 4]], scale: f32, time: f32) {
    let surface = &heights[..count];
    let beds = &heights[count..2 * count];
    let leaves = &heights[2 * count..3 * count];
    let petals = &heights[3 * count..4 * count];
    let height = canvas.height as f32;
    let width_points = count as f32 * 2.0;
    let x_start = (origin_x * scale).floor().max(0.0) as i32;
    let x_end = (((origin_x + width_points) * scale).ceil() as i32).min(canvas.width as i32);
    let top_row = (height - (origin_y + top + 1.0) * scale).floor().max(0.0) as i32;
    let bottom_row = ((height - (origin_y + bottom - 1.0) * scale).ceil() as i32).min(canvas.height as i32);
    if top_row >= bottom_row {
        return;
    }
    let shadow = [0.72, 0.78, 0.88];
    let face = [0.925, 0.94, 0.965];
    for px in x_start..x_end {
        let world_x = (px as f32 + 0.5) / scale;
        let x = world_x - origin_x;
        if x < 0.0 || x >= width_points {
            continue;
        }
        let level0 = sample(surface, x);
        let bed = sample(beds, x);
        let leafy = sample(leaves, x);
        let petal = sample(petals, x);
        if level0 - bed < 0.02 {
            continue;
        }
        let mut level = level0;
        if leafy > 0.0 {
            level -= 2.5 * leafy;
            level += (hash((x * 0.4).floor(), 3.1) - 0.5) * 2.4 * ((level - bed) / 4.0).clamp(0.0, 1.0) * leafy;
        }
        let slope = (sample(surface, x + 2.0) - sample(surface, x - 2.0)) * 0.25;
        let norm = (slope * slope + 1.0).sqrt();
        let lit = ((-slope) * -0.5 + 1.0) / (norm * 1.118);
        let row_from = ((height - (origin_y + level + 1.0) * scale).floor() as i32).max(top_row);
        let row_to = (((height - (origin_y + bed - 1.0) * scale).ceil()) as i32).min(bottom_row);
        for py in row_from..row_to {
            let local_y = (height - (py as f32 + 0.5)) / scale - origin_y;
            let world_y = local_y + origin_y;
            if occluders.iter().any(|o| world_x >= o[0] && world_x < o[2] && world_y >= o[1] && world_y < o[3]) {
                continue;
            }
            let depth = level - local_y;
            let coverage = (depth * scale + 0.5).clamp(0.0, 1.0) * ((local_y - bed) * scale + 0.5).clamp(0.0, 1.0) * ((level - bed) / 0.75).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let mut color = [0.0f32; 3];
            if leafy < 1.0 {
                let shade = ((1.0 - (-depth / 30.0).exp()) * 0.32 + (1.0 - lit) * 0.6 * (-depth / 5.0).exp()).clamp(0.0, 1.0);
                let bright = (-depth * 0.9).exp() * 0.6;
                let noise = (hash(px as f32, py as f32) - 0.5) * 0.02;
                for k in 0..3 {
                    let c = lerp(face[k], shadow[k], shade);
                    color[k] = lerp(c, [0.975, 0.982, 0.995][k], bright) + noise;
                }
                let seed = hash((px as f32 * 0.5).floor() + 31.7, (py as f32 * 0.5).floor() + 31.7);
                if seed > 0.992 {
                    let twinkle = (time * (0.6 + seed * 1.8) + seed * 90.0).sin().clamp(0.0, 1.0).powf(28.0);
                    let glint = twinkle * (-depth / 7.0).exp();
                    for c in color.iter_mut() {
                        *c = lerp(*c, 1.0, glint);
                    }
                }
            }
            if leafy > 0.0 {
                let cell = hash(((x - 0.0) * 0.5).floor(), (local_y * 0.5).floor() + 7.0);
                let patch: f32 = if cell < leafy { 1.0 } else { 0.0 };
                let leaf_share = patch.max(smooth(0.55, 0.95, leafy));
                let seed = hash(((x) * 0.5).floor() + 5.0, (local_y * 0.5).floor());
                let mut leaf = leaf_color(seed, if petal / leafy.max(0.001) > 0.5 { 1.0 } else { 0.0 });
                let deep = (1.0 - (-depth / 18.0).exp()) * 0.4;
                for c in leaf.iter_mut() {
                    *c *= (1.0 - deep) * (0.84 + 0.2 * lit);
                }
                for k in 0..3 {
                    color[k] = lerp(color[k], leaf[k], leaf_share);
                }
            }
            canvas.blend(px, py, [color[0].clamp(0.0, 1.0), color[1].clamp(0.0, 1.0), color[2].clamp(0.0, 1.0)], coverage);
        }
    }
}

fn draw_sprite(canvas: &mut Canvas, sprite: &Sprite, scale: f32, height: f32) {
    if sprite.ghost[0] > 0.0 {
        return;
    }
    let (cx, cy) = (sprite.position.x * scale, height - sprite.position.y * scale);
    if cx < -60.0 || cy < -60.0 || cx > canvas.width as f32 + 60.0 || cy > height + 60.0 {
        return;
    }
    if sprite.leaf[0] != 0.0 {
        draw_leaf(canvas, sprite, cx, cy, scale);
        return;
    }
    if sprite.rain[0] > 0.0 {
        let kind = sprite.rain[0];
        if (kind - 1.0).abs() < 0.5 {
            // A raindrop's streak, or a droplet it throws up.
            let (vx, vy) = (sprite.velocity.x * 0.016, sprite.velocity.y * 0.016);
            let length = (vx * vx + vy * vy).sqrt().min(26.0);
            let n = (length * scale / 2.0).max(1.0);
            let (ux, uy) = if length > 0.01 { (vx / length, vy / length) } else { (0.0, 0.0) };
            let tail = (cx - ux * length * scale, cy + uy * length * scale);
            canvas.streak(tail, (cx, cy), (sprite.size * scale * 0.45).max(0.7), 0.6, [0.8, 0.88, 1.0], (sprite.alpha * 0.55 * n.min(1.0)).min(1.0));
        } else if kind > 3.5 && kind < 4.5 {
            canvas.dot(cx, cy, (sprite.size * scale * 0.5).max(0.7), 0.5, [0.85, 0.92, 1.0], [0.6, 0.72, 0.9], sprite.alpha * 0.6);
        }
        return;
    }
    let radius = (sprite.size * scale * 0.5).max(0.8);
    let (vx, vy) = (sprite.velocity.x * 0.018, sprite.velocity.y * 0.018);
    let travel = (vx * vx + vy * vy).sqrt();
    let smear = travel.min(sprite.size * if sprite.lump > 0.0 { 1.2 } else { 2.2 });
    let mut tint_core = [0.965, 0.975, 0.995];
    let mut tint_edge = [0.62, 0.67, 0.77];
    if sprite.tint_amount > 0.0 {
        for k in 0..3 {
            tint_core[k] = lerp(tint_core[k], sprite.tint[k], sprite.tint_amount);
            tint_edge[k] = lerp(tint_edge[k], sprite.tint[k], sprite.tint_amount);
        }
    }
    let alpha = sprite.alpha * (sprite.size * scale / 2.0).min(1.0);
    let reach = if sprite.lump > 0.0 { 1.12 } else { 1.0 };
    if smear > 0.25 {
        let (ux, uy) = (vx / travel, vy / travel);
        let tail = (cx - ux * smear * scale, cy + uy * smear * scale);
        canvas.streak(tail, (cx, cy), radius * reach, sprite.soft, tint_core, alpha);
    } else {
        canvas.dot(cx, cy, radius * reach, sprite.soft, tint_core, tint_edge, alpha);
    }
}

fn draw_leaf(canvas: &mut Canvas, sprite: &Sprite, cx: f32, cy: f32, scale: f32) {
    let seed = sprite.leaf[0].abs();
    let petal = sprite.petal > 0.5;
    let angle = sprite.leaf[1];
    let face = sprite.leaf[2].clamp(-1.0, 1.0);
    let length = sprite.size * scale * if petal { 0.55 } else { 0.9 };
    let width = length * (0.3 + 0.35 * face.abs().max(0.25));
    let color = leaf_color(seed, if petal { 1.0 } else { 0.0 });
    let back = if face < 0.0 { 0.82 } else { 1.0 };
    let color = [color[0] * back, color[1] * back, color[2] * back];
    let (s, c) = angle.sin_cos();
    let mut outline = Vec::with_capacity(16);
    for k in 0..16 {
        let t = k as f32 / 16.0 * std::f32::consts::TAU;
        // A leaf: pointed at both ends, fullest a little before the middle.
        let along = t.cos() * length * 0.5;
        let across = t.sin() * width * 0.5 * (1.0 - 0.35 * t.cos());
        let (x, y) = (along * c - across * s, along * s + across * c);
        outline.push((cx + x, cy - y));
    }
    canvas.polygon(&outline, color, sprite.alpha.min(1.0));
    if !petal {
        let (ex, ey) = (length * 0.5 * c, length * 0.5 * s);
        canvas.polygon(
            &[(cx - ex, cy + ey), (cx + ex, cy - ey), (cx + ex + 0.5, cy - ey + 0.5), (cx - ex + 0.5, cy + ey + 0.5)],
            [color[0] * 0.7, color[1] * 0.7, color[2] * 0.7],
            0.5 * sprite.alpha.min(1.0),
        );
    }
}

// The snowplow

struct Pen<'a> {
    canvas: &'a mut Canvas,
    /// Where the blade is, in pixels, and the ground line.
    x: f32,
    ground: f32,
    dir: f32,
    k: f32,
}

type Color = [f32; 4];

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + self.dir * self.k * x, self.ground - self.k * y)
    }

    fn poly(&mut self, points: &[(f32, f32)], c: Color) {
        let mapped: Vec<(f32, f32)> = points.iter().map(|p| self.at(p.0, p.1)).collect();
        self.canvas.polygon(&mapped, [c[0], c[1], c[2]], c[3]);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], c);
    }

    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let r = r.min(w / 2.0).min(h / 2.0);
        let mut points = Vec::new();
        for (cx, cy, from) in [(x + w - r, y + r, -90.0f32), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0), (x + r, y + r, 180.0)] {
            for k in 0..=5 {
                let a = (from + k as f32 * 18.0).to_radians();
                points.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        }
        self.poly(&points, c);
    }

    fn oval(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let points: Vec<(f32, f32)> = (0..32)
            .map(|k| {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                (x + w / 2.0 + w / 2.0 * a.cos(), y + h / 2.0 + h / 2.0 * a.sin())
            })
            .collect();
        self.poly(&points, c);
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, c: Color) {
        self.oval(x - r, y - r, 2.0 * r, 2.0 * r, c);
    }

    fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, c: Color) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt().max(0.001);
        let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
        self.poly(&[(from.0 + nx, from.1 + ny), (to.0 + nx, to.1 + ny), (to.0 - nx, to.1 - ny), (from.0 - nx, from.1 - ny)], c);
    }

    /// A closed shape of quadratic curves from `start`, each `(control, end)`.
    fn curves(&mut self, start: (f32, f32), curves: &[((f32, f32), (f32, f32))], c: Color) {
        let mut points = vec![start];
        let mut from = start;
        for &(control, to) in curves {
            for k in 1..=10 {
                let t = k as f32 / 10.0;
                let u = 1.0 - t;
                points.push((u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0, u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1));
            }
            from = to;
        }
        self.poly(&points, c);
    }
}

fn draw_plow(canvas: &mut Canvas, plow: &PlowState, scale: f32, t: f32) {
    let ground = canvas.height as f32;
    let k = scale;
    let (x, dir) = (plow.tip * scale, plow.dir);
    // The heap in front of the blade.
    let mound_h = 12.0 + 58.0 * (1.0 - (-plow.pushed / 15000.0).exp());
    let mound_w = 50.0 + mound_h * 0.8;
    if !plow.dumped && plow.pushed > 0.0 {
        let end_h = mound_h.min(24.0);
        let mut points = vec![(x, ground), (x, ground - mound_h * k)];
        let p0 = (x, ground - mound_h * k);
        let (c1, c2, p3) = (
            (x + dir * mound_w * 0.35 * k, ground - mound_h * 1.02 * k),
            (x + dir * mound_w * 0.7 * k, ground - mound_h * 0.5 * k),
            (x + dir * mound_w * k, ground - end_h * k),
        );
        for s in 1..=16 {
            let u = s as f32 / 16.0;
            let v = 1.0 - u;
            points.push((
                v * v * v * p0.0 + 3.0 * v * v * u * c1.0 + 3.0 * v * u * u * c2.0 + u * u * u * p3.0,
                v * v * v * p0.1 + 3.0 * v * v * u * c1.1 + 3.0 * v * u * u * c2.1 + u * u * u * p3.1,
            ));
        }
        points.push((p3.0, ground));
        canvas.polygon(&points, [1.0, 1.0, 1.0], 0.97);
    }
    let mut p = Pen { canvas, x, ground, dir, k: k * PLOW_SIZE };
    machine(&mut p, plow, t);
}

fn machine(p: &mut Pen, plow: &PlowState, t: f32) {
    let tau = std::f32::consts::TAU;
    p.oval(-136.0, -3.0, 142.0, 9.0, [0.0, 0.0, 0.0, 0.28]);
    for i in 0..5 {
        let ph = (t * 0.7 + i as f32 / 5.0).fract();
        p.circle(-70.0 - ph * 26.0, 82.0 + ph * 22.0, 2.5 + ph * 7.0, [0.82, 0.82, 0.85, 0.38 * (1.0 - ph)]);
    }
    p.round(-132.0, 12.0, 118.0, 14.0, 3.0, [0.16, 0.16, 0.18, 1.0]);
    p.round(-132.0, 22.0, 118.0, 3.0, 1.5, [0.3, 0.3, 0.33, 1.0]);
    p.poly(&[(-130.0, 26.0), (-131.0, 56.0), (-68.0, 50.0), (-68.0, 26.0)], [0.92, 0.48, 0.08, 1.0]);
    p.rect(-129.0, 34.0, 61.0, 4.0, [0.74, 0.34, 0.05, 1.0]);
    p.rect(-131.0, 53.0, 63.0, 3.0, [1.0, 0.7, 0.3, 1.0]);
    for i in 0..6 {
        let light = i % 2 == 0;
        p.rect(-131.0, 27.0 + i as f32 * 4.6, 9.0, 4.6, if light { [0.98, 0.85, 0.1, 1.0] } else { [0.1, 0.1, 0.1, 1.0] });
    }
    p.round(-128.0, 54.0, 58.0, 5.0, 2.5, [1.0, 1.0, 1.0, 0.97]);
    p.round(-66.0, 26.0, 42.0, 46.0, 7.0, [0.96, 0.52, 0.1, 1.0]);
    p.round(-70.0, 67.0, 50.0, 7.0, 3.0, [0.8, 0.38, 0.06, 1.0]);
    p.round(-68.0, 72.0, 46.0, 5.0, 2.5, [1.0, 1.0, 1.0, 0.97]);
    p.round(-61.0, 38.0, 32.0, 27.0, 5.0, [0.13, 0.15, 0.18, 1.0]);
    p.round(-59.0, 40.0, 28.0, 23.0, 4.0, [0.55, 0.78, 0.93, 1.0]);
    p.poly(&[(-57.0, 41.0), (-49.0, 41.0), (-41.0, 62.0), (-49.0, 62.0)], [1.0, 1.0, 1.0, 0.33]);
    let (a, b) = (p.at(-36.0, 41.0), p.at(-46.0, 52.0));
    p.canvas.polygon(&thick(a, b, 1.4 * p.k), [0.1, 0.1, 0.12], 1.0);
    p.round(-39.0, 32.0, 10.0, 3.0, 1.5, [0.2, 0.2, 0.22, 1.0]);
    p.round(-26.0, 26.0, 26.0, 23.0, 5.0, [0.98, 0.58, 0.14, 1.0]);
    p.rect(-24.0, 36.0, 22.0, 2.0, [0.8, 0.38, 0.06, 1.0]);
    p.round(-5.0, 28.0, 6.0, 18.0, 2.0, [0.12, 0.12, 0.14, 1.0]);
    for i in 0..3 {
        p.rect(-4.0, 31.0 + i as f32 * 5.0, 4.0, 1.5, [0.5, 0.5, 0.55, 1.0]);
    }
    for (r, a) in [(28.0, 0.10), (20.0, 0.14), (12.0, 0.2)] {
        p.circle(14.0, 39.0, r, [1.0, 0.95, 0.6, a]);
    }
    p.circle(0.0, 41.0, 3.0, [1.0, 0.96, 0.7, 1.0]);
    p.round(-8.0, 21.0, 14.0, 6.0, 2.0, [0.16, 0.16, 0.18, 1.0]);
    p.line((-22.0, 22.0), (2.0, 20.0), 4.0, [0.2, 0.2, 0.22, 1.0]);
    p.line((-14.0, 28.0), (5.0, 38.0), 2.0, [0.72, 0.72, 0.76, 1.0]);
    p.curves((-4.0, 9.0), &[((3.0, 5.0), (8.0, 6.0)), ((16.0, 32.0), (12.0, 58.0)), ((2.0, 63.0), (-8.0, 62.0))], [0.96, 0.78, 0.1, 1.0]);
    p.poly(&[(-3.0, 16.0), (0.0, 16.0), (4.0, 56.0), (0.0, 58.0)], [1.0, 0.92, 0.5, 0.75]);
    for y in [22.0, 36.0, 50.0] {
        p.rect(-2.0, y, 13.0, 2.0, [0.78, 0.58, 0.05, 1.0]);
    }
    p.round(-5.0, 3.0, 14.0, 5.0, 1.5, [0.1, 0.1, 0.11, 1.0]);
    for y in [18.0, 33.0, 48.0] {
        p.circle(5.3, y + 1.3, 1.3, [0.42, 0.42, 0.46, 1.0]);
    }
    if plow.pushed > 0.0 {
        for i in 0..7 {
            let ph = (t * 1.7 + i as f32 / 7.0).fract();
            p.circle(10.0 + ph * 24.0, 20.0 + (i as f32 * 2.1).sin() * 10.0 + ph * 38.0, 1.0 + 2.4 * (1.0 - ph), [1.0, 1.0, 1.0, 0.85 * (1.0 - ph)]);
        }
    }
    let spin = -(plow.tip * plow.dir) / 14.0;
    for wx in [-44.0f32, -104.0] {
        p.circle(wx, 14.0, 14.0, [0.08, 0.08, 0.09, 1.0]);
        for n in 0..10 {
            let a = spin + n as f32 * tau / 20.0;
            let (s, c) = a.sin_cos();
            p.line((wx + c * 11.2, 14.0 + s * 11.2), (wx + c * 14.4, 14.0 + s * 14.4), 3.2, [0.2, 0.2, 0.22, 1.0]);
        }
        p.circle(wx, 14.0, 7.0, [0.62, 0.63, 0.67, 1.0]);
        p.circle(wx, 14.0, 2.5, [0.4, 0.4, 0.44, 1.0]);
        for n in 0..5 {
            let a = spin + n as f32 * tau / 5.0;
            p.circle(wx + a.cos() * 4.6, 14.0 + a.sin() * 4.6, 1.0, [0.32, 0.32, 0.36, 1.0]);
        }
        // The fender arch, a thick arc over the wheel.
        let mut previous = None;
        for n in 0..=12 {
            let a = (0.08 + 0.84 * n as f32 / 12.0) * std::f32::consts::PI;
            let point = (wx + 17.0 * a.cos(), 14.0 + 17.0 * a.sin());
            if let Some(from) = previous {
                p.line(from, point, 3.5, [0.8, 0.38, 0.06, 1.0]);
            }
            previous = Some(point);
        }
    }
    p.round(-72.0, 50.0, 5.0, 34.0, 2.0, [0.25, 0.25, 0.28, 1.0]);
    p.round(-73.0, 82.0, 7.0, 4.0, 1.5, [0.12, 0.12, 0.14, 1.0]);
    let on = (t * 4.0) as i32 % 2 == 0;
    p.round(-52.0, 74.0, 12.0, 3.0, 1.0, [0.2, 0.2, 0.22, 1.0]);
    if on {
        for (r, a) in [(22.0, 0.12), (15.0, 0.18), (9.0, 0.26)] {
            p.circle(-46.0, 82.0, r, [1.0, 0.55, 0.1, a]);
        }
    }
    p.circle(-46.0, 81.0, 5.0, [1.0, if on { 0.55 } else { 0.28 }, 0.1, if on { 1.0 } else { 0.6 }]);
}

/// A thick line between two points already in pixels, as a quad.
fn thick(from: (f32, f32), to: (f32, f32), width: f32) -> Vec<(f32, f32)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt().max(0.001);
    let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
    vec![(from.0 + nx, from.1 + ny), (to.0 + nx, to.1 + ny), (to.0 - nx, to.1 - ny), (from.0 - nx, from.1 - ny)]
}
