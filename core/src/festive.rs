//! The winter decorations: a string of lights along the top of each window, snowmen the user rolls and stacks out of
//! the snow on the ground, and frost creeping in from the edges of the screen that the pointer wipes clear.
//! The Mac app has its own copy of all of it in Swift; the pixel functions here are what its shaders compute.

use crate::geometry::V2;

// MARK: Garland

/// A string of lights hung along the top of the screen, from posts at both ends and between, its bulbs swinging when
/// the pointer brushes them.
#[derive(Clone, Debug, PartialEq)]
pub struct GarlandSprite {
    /// Where the string hangs from: the left end, at the top of the screen (or under the menu bar).
    pub anchor: V2,
    pub width: f32,
    pub seed: f32,
    /// How far each bulb has swung from straight down, in radians, positive towards the right.
    pub angles: Vec<f32>,
    /// How far the wire is pulled down at each bulb, in points.
    pub lifts: Vec<f32>,
}

/// How far below the top a garland reaches, in points.
pub const GARLAND_DEPTH: f32 = 62.0;
/// How far above it the quad it is drawn in starts.
pub const GARLAND_ABOVE: f32 = 4.0;
/// How far from the wire the middle of a bulb hangs.
pub const BULB_DROP: f32 = 11.5;

const GARLAND_COLORS: [[f32; 3]; 5] = [[1.0, 0.16, 0.12], [1.0, 0.72, 0.16], [0.22, 0.92, 0.34], [0.26, 0.56, 1.0], [1.0, 0.32, 0.82]];
/// The widest a bulb may swing.
const SWING_LIMIT: f32 = 1.25;
const WIRE_RISE: f32 = 1.0;

/// How a garland of a width is divided into swags between posts, and the swags into bulbs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GarlandLayout {
    pub spans: f32,
    pub span: f32,
    pub gap: f32,
    pub total: usize,
}

pub fn garland_layout(width: f32) -> GarlandLayout {
    let spans = (width / 150.0).round().max(1.0);
    let span = width.max(40.0) / spans;
    let bulbs = (span / 21.0).round().max(1.0);
    GarlandLayout { spans, span, gap: span / bulbs, total: (spans * bulbs) as usize }
}

/// How far the wire is pulled down at x: the lifts of the two bulbs either side of it.
fn lift_at(x: f32, layout: &GarlandLayout, lifts: &[f32]) -> f32 {
    if lifts.is_empty() {
        return 0.0;
    }
    let u = x / layout.gap - 0.5;
    let j = u.floor();
    let f = u - j;
    let at = |k: f32| if k < 0.0 { 0.0 } else { lifts.get(k as usize).copied().unwrap_or(0.0) };
    at(j) * (1.0 - f) + at(j + 1.0) * f
}

/// The wire's depth below the top of the screen at x, and its slope there. The lift deepens the swag, so the wire
/// stays pinned at the posts.
fn wire_at(x: f32, layout: &GarlandLayout, lifts: &[f32]) -> (f32, f32) {
    let k = (x / layout.span).floor().clamp(0.0, layout.spans - 1.0);
    let t = ((x - k * layout.span) / layout.span).clamp(0.0, 1.0);
    let sag = layout.span * 0.12 + 3.0 + lift_at(x, layout, lifts);
    let y = WIRE_RISE + sag * 4.0 * t * (1.0 - t);
    let slope = sag * 4.0 * (1.0 - 2.0 * t) / layout.span;
    (y, slope)
}

fn fx_hash(x: f32, y: f32) -> f32 {
    let v = (x * 127.1 + y * 311.7).sin() * 43758.5453;
    v - v.floor()
}

fn fx_noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let a = fx_hash(ix, iy);
    let b = fx_hash(ix + 1.0, iy);
    let c = fx_hash(ix, iy + 1.0);
    let d = fx_hash(ix + 1.0, iy + 1.0);
    let (wx, wy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = a + (b - a) * wx;
    let bottom = c + (d - c) * wx;
    top + (bottom - top) * wy
}

fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let keep = 1.0 - top[3];
    [top[0] + under[0] * keep, top[1] + under[1] * keep, top[2] + under[2] * keep, top[3] + under[3] * keep]
}

fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// One pixel of a garland, premultiplied: `x` along the string from its left end, `y` down from the top it hangs from.
pub fn garland_pixel(x: f32, y: f32, width: f32, seed: f32, time: f32, angles: &[f32], lifts: &[f32]) -> [f32; 4] {
    let layout = garland_layout(width);
    if x < -4.0 || x > width + 4.0 {
        return [0.0; 4];
    }
    let (wire, slope) = wire_at(x, &layout, lifts);
    let away = (y - wire) / (1.0 + slope * slope).sqrt();
    let wire_a = saturate(1.15 - away.abs()) * 0.92;

    let first = (x / layout.gap).floor();
    let mut glow = [0.0f32; 3];
    let mut cords = [0.0f32; 4];
    let mut sockets = [0.0f32; 4];
    let mut lamps = [0.0f32; 4];
    for dj in -1..=1 {
        let j = first + dj as f32;
        if j < 0.0 || j > layout.total as f32 - 1.0 {
            continue;
        }
        let bx = (j + 0.5) * layout.gap;
        let (wy, _) = wire_at(bx, &layout, lifts);
        let theta = angles.get(j as usize).copied().unwrap_or(0.0);
        let (sn, cs) = theta.sin_cos();
        let (dx, dy) = (x - bx, y - wy);
        // The bulb's own axes: down along its cord, and across it.
        let (lx, ly) = (dx * cs - dy * sn, dx * sn + dy * cs);
        let tint = GARLAND_COLORS[((fx_hash(j, seed * 91.0) * 5.0) as usize).min(4)];
        let phase = fx_hash(j + 11.0, seed * 7.0) * 6.2832;
        let level = 0.5 + 0.5 * (time * 2.3 + phase + j * 0.7).sin();
        let level = 0.3 + 0.7 * level * level;
        let (gx, gy) = (x - (bx + BULB_DROP * sn), y - (wy + BULB_DROP * cs));
        let halo = (-(gx * gx + gy * gy) / 130.0).exp() * 0.34 * level;
        for c in 0..3 {
            glow[c] += tint[c] * halo;
        }
        let cord = saturate(0.95 - lx.abs() * 1.6) * saturate(ly + 0.5) * saturate(BULB_DROP - 4.6 - ly);
        cords = over([0.05 * cord, 0.13 * cord, 0.09 * cord, cord * 0.9], cords);
        let (sx, sy) = (lx.abs() - 1.25, (ly - (BULB_DROP - 3.8)).abs() - 1.6);
        let socket = saturate(0.5 - sx.max(sy));
        sockets = over([0.10 * socket, 0.13 * socket, 0.11 * socket, socket], sockets);
        let (qx, qy) = (lx / 3.0, (ly - BULB_DROP) / 4.4);
        let rr = (qx * qx + qy * qy).sqrt();
        let body = saturate((1.0 - rr) * 3.2);
        let hot = 0.55 * level * saturate(1.0 - rr * 1.4);
        let (hx, hy) = (qx + 0.38, qy + 0.4);
        let spec = 0.5 * saturate(1.0 - (hx * hx + hy * hy).sqrt() * 4.0);
        let mut lit = [0.0f32; 3];
        for c in 0..3 {
            lit[c] = (tint[c] * (0.45 + 0.75 * level) + hot + spec).min(1.0) * body;
        }
        lamps = over([lit[0], lit[1], lit[2], body], lamps);
    }
    let mut color = [glow[0], glow[1], glow[2], 0.0];
    color = over([0.07 * wire_a, 0.17 * wire_a, 0.12 * wire_a, wire_a], color);
    color = over(cords, color);
    color = over(sockets, color);
    over(lamps, color)
}

/// How the bulbs of a garland swing and the wire bobs: set going by the pointer brushing them, and settling again.
#[derive(Clone, Debug, Default)]
pub struct Garland {
    pub angles: Vec<f32>,
    speeds: Vec<f32>,
    pub lifts: Vec<f32>,
    lift_speeds: Vec<f32>,
    width: f32,
    previous: Option<V2>,
    velocity: V2,
    /// When anything last moved: the scene is drawn at full rate meanwhile.
    pub moved_at: f64,
}

/// How near the pointer must come to a bulb to take hold of it.
const GARLAND_REACH: f32 = 20.0;

impl Garland {
    pub fn resize(&mut self, width: f32) {
        let total = garland_layout(width).total;
        self.width = width;
        if self.angles.len() != total {
            self.angles = vec![0.0; total];
            self.speeds = vec![0.0; total];
            self.lifts = vec![0.0; total];
            self.lift_speeds = vec![0.0; total];
        }
    }

    pub fn sprite(&self, top: f32) -> GarlandSprite {
        GarlandSprite { anchor: V2::new(0.0, top), width: self.width, seed: 0.37, angles: self.angles.clone(), lifts: self.lifts.clone() }
    }

    /// Advances the swinging, with the pointer brushing the bulbs (`pointer` is None while it is out of reach), the
    /// string hanging from `top` points above the bottom of the screen. Returns whether anything moves.
    pub fn step(&mut self, dt: f32, pointer: Option<V2>, top: f32, clock: f64) -> bool {
        let layout = garland_layout(self.width);
        if self.angles.len() != layout.total {
            self.resize(self.width);
        }
        match pointer {
            Some(p) => {
                if let Some(previous) = self.previous
                    && dt > 0.0
                {
                    let raw = (p - previous) / dt.max(1.0 / 240.0);
                    self.velocity += (raw - self.velocity) * (dt * 20.0).min(1.0);
                }
                self.previous = Some(p);
            }
            None => {
                self.previous = None;
                self.velocity = V2::ZERO;
            }
        }
        let n = self.angles.len();
        if n == 0 {
            return false;
        }
        let near = pointer.is_some_and(|p| p.y > top - (GARLAND_DEPTH + GARLAND_REACH) && p.y < top + GARLAND_REACH);
        let steps = (dt / 0.008).ceil().clamp(1.0, 8.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let (angles, lifts) = (self.angles.clone(), self.lifts.clone());
            for j in 0..n {
                let (left, right) = (if j > 0 { angles[j - 1] } else { 0.0 }, if j + 1 < n { angles[j + 1] } else { 0.0 });
                let (lift_left, lift_right) = (if j > 0 { lifts[j - 1] } else { 0.0 }, if j + 1 < n { lifts[j + 1] } else { 0.0 });
                let theta = angles[j];
                let mut accel = -47.0 * theta.sin() - 1.1 * self.speeds[j] + 14.0 * (left + right - 2.0 * theta);
                let mut lift_accel = -130.0 * lifts[j] - 6.0 * self.lift_speeds[j] + 70.0 * (lift_left + lift_right - 2.0 * lifts[j]);
                if near && let Some(p) = pointer {
                    let bx = (j as f32 + 0.5) * layout.gap;
                    let (wy, _) = wire_at(bx, &layout, &lifts);
                    let (sn, cs) = theta.sin_cos();
                    let pivot = V2::new(bx, top - wy);
                    let lamp = V2::new(bx + BULB_DROP * sn, top - wy - BULB_DROP * cs);
                    let grip = 1.0 - ((lamp - p).length() / GARLAND_REACH).min(1.0);
                    if grip > 0.0 {
                        // The pointer drags the bulb along: the speed it moves across the bulb's arc.
                        let across = (self.velocity.x * cs + self.velocity.y * sn) / BULB_DROP;
                        accel += (across.clamp(-16.0, 16.0) - self.speeds[j]) * grip * 24.0;
                    }
                    let pull = 1.0 - ((pivot - p).length() / GARLAND_REACH).min(1.0);
                    if pull > 0.0 {
                        lift_accel += ((-self.velocity.y * 0.25).clamp(-140.0, 140.0) - self.lift_speeds[j]) * pull * 24.0;
                    }
                }
                self.speeds[j] = (self.speeds[j] + accel * h).clamp(-18.0, 18.0);
                self.lift_speeds[j] += lift_accel * h;
            }
            for j in 0..n {
                self.angles[j] += self.speeds[j] * h;
                if self.angles[j].abs() > SWING_LIMIT {
                    self.angles[j] = self.angles[j].clamp(-SWING_LIMIT, SWING_LIMIT);
                    self.speeds[j] *= -0.3;
                }
                self.lifts[j] += self.lift_speeds[j] * h;
                if self.lifts[j].abs() > 8.0 {
                    self.lifts[j] = self.lifts[j].clamp(-8.0, 8.0);
                    self.lift_speeds[j] = 0.0;
                }
            }
        }
        let active = (0..n).any(|j| self.angles[j].abs() > 0.002 || self.speeds[j].abs() > 0.04 || self.lifts[j].abs() > 0.02 || self.lift_speeds[j].abs() > 0.2);
        if !active {
            for values in [&mut self.angles, &mut self.speeds, &mut self.lifts, &mut self.lift_speeds] {
                values.iter_mut().for_each(|v| *v = 0.0);
            }
        }
        if active || near {
            self.moved_at = clock;
        }
        active
    }
}

// MARK: Snowmen

/// One ball of snow as the renderer draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallSprite {
    pub center: V2,
    pub radius: f32,
    pub seed: f32,
    pub roll: f32,
    /// 0 a plain ball, 1 the body of a snowman (buttons and arms), 2 its head (eyes, nose, mouth and scarf).
    pub role: f32,
    /// 1 just as the snowman was finished, easing to 0: the face pops in.
    pub pop: f32,
}

/// How far a ball's quad reaches from its centre, in radii: past the ball for a snowman's arms and nose.
pub const BALL_REACH: f32 = 2.4;

fn segment(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (pax, pay) = (px - a.0, py - a.1);
    let (bax, bay) = (b.0 - a.0, b.1 - a.1);
    let along = saturate((pax * bax + pay * bay) / (bax * bax + bay * bay));
    ((pax - bax * along).hypot(pay - bay * along), along)
}

/// One pixel of a ball and what a snowman wears, premultiplied; `lx`, `ly` in radii from its centre with y up, `pixel`
/// one pixel in radii.
pub fn ball_pixel(lx: f32, ly: f32, pixel: f32, ball: &BallSprite, hash: f32) -> [f32; 4] {
    let d = (lx * lx + ly * ly).sqrt();
    let mut result = [0.0f32; 4];
    let edge = saturate((1.0 - d) / pixel + 0.5);
    if edge > 0.0 {
        let z = (1.0 - d * d).max(0.0).sqrt();
        let len = (lx * lx + ly * ly + (z + 0.0001) * (z + 0.0001)).sqrt();
        let n = [lx / len, ly / len, (z + 0.0001) / len];
        let l = [-0.5f32, 0.65, 0.6];
        let ll = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        let toward = n[0] * l[0] / ll + n[1] * l[1] / ll + n[2] * l[2] / ll;
        let mut diffuse = saturate(toward);
        let (cr, sr) = (ball.roll.cos(), ball.roll.sin());
        let (gx, gy) = (lx * cr - ly * sr, lx * sr + ly * cr);
        let lump = fx_noise(gx * (4.0 + ball.radius * 0.06) + ball.seed * 40.0, gy * (4.0 + ball.radius * 0.06) + ball.seed * 40.0) * 0.6
            + fx_noise(gx * 13.0 + ball.seed * 9.0, gy * 13.0 + ball.seed * 9.0) * 0.4;
        diffuse = saturate(diffuse + (lump - 0.5) * 0.24);
        let t = smoothstep(0.05, 0.85, diffuse);
        let shade = [0.70f32, 0.77, 0.88];
        let lit = [0.945f32, 0.955, 0.975];
        let gloss = saturate(toward).powi(6) * 0.35;
        let bounce = (1.0 - z).powi(3) * 0.18 * saturate(0.4 - n[1]);
        let target = [0.985f32, 0.99, 1.0];
        let cool = [0.78f32, 0.86, 0.98];
        let mut color = [0.0f32; 3];
        for c in 0..3 {
            let base = shade[c] + (lit[c] - shade[c]) * t;
            let base = base + (target[c] - base) * gloss;
            color[c] = (base + (cool[c] - base) * bounce + (hash - 0.5) * 0.02).clamp(0.0, 1.0);
        }
        result = [color[0] * edge, color[1] * edge, color[2] * edge, edge];
    }
    let part = |under: [f32; 4], color: [f32; 3], cover: f32| over([color[0] * cover, color[1] * cover, color[2] * cover, cover], under);
    let s = 1.0 + 0.5 * ball.pop;
    let coal = [0.06f32, 0.06, 0.08];
    let twig = [0.36f32, 0.22, 0.12];
    if ball.role > 1.5 {
        let band = 0.10 * (1.0 - (lx * lx).min(1.0));
        let scarf = saturate((ly + 0.80 + band) / pixel + 0.5) * saturate((-0.56 - band - ly) / pixel + 0.5) * edge;
        let stripe = if (lx + 1.0) * 5.5 - ((lx + 1.0) * 5.5).floor() >= 0.5 { 1.0 } else { 0.0 };
        let wool = |stripe: f32| [0.82 + (0.96 - 0.82) * stripe, 0.10 + (0.95 - 0.10) * stripe, 0.14 + (0.96 - 0.14) * stripe];
        result = part(result, wool(stripe), scarf);
        let (tx, ty) = ((lx - 0.37).abs() - 0.10, (ly + 0.83).abs() - 0.22);
        let tail_stripe = if ly * 5.5 - (ly * 5.5).floor() >= 0.5 { 1.0 } else { 0.0 };
        result = part(result, wool(tail_stripe), saturate(0.5 - tx.max(ty) / pixel));
        for e in 0..2 {
            let cx = if e == 0 { -0.30 } else { 0.30 };
            let r = (lx - cx).hypot(ly - 0.22);
            result = part(result, coal, saturate((0.085 * s - r) / pixel + 0.5));
        }
        for m in 0..5 {
            let dm = m as f32 - 2.0;
            let r = (lx - dm * 0.15).hypot(ly - (-0.34 + 0.05 * dm * dm * 0.55));
            result = part(result, coal, saturate((0.042 * s - r) / pixel + 0.5));
        }
        let (dn, along) = segment(lx, ly, (0.04, 0.0), (0.78 * s, -0.10));
        let carrot = saturate(((0.10 * (1.0 - along)) - dn) / pixel + 0.5);
        let shading = 0.82 + 0.3 * saturate((ly - (-0.10 * along)) / 0.1 + 0.5);
        result = part(result, [0.97 * shading, 0.55 * shading, 0.12 * shading], carrot);
    } else if ball.role > 0.5 {
        for k in 0..3 {
            let r = lx.hypot(ly - (0.45 - 0.40 * k as f32));
            result = part(result, coal, saturate((0.085 * s - r) / pixel + 0.5));
        }
        let mut cover = 0.0f32;
        for (a, b, w) in [
            ((-0.82, 0.12), (-2.05, 0.78), 0.05),
            ((-1.5, 0.50), (-1.62, 0.98), 0.04),
            ((0.82, 0.10), (2.0, 0.62), 0.05),
            ((1.55, 0.38), (1.78, 0.90), 0.04),
        ] {
            let (dist, _) = segment(lx, ly, a, b);
            cover = cover.max(saturate((w - dist) / pixel + 0.5));
        }
        result = part(result, twig, cover);
    }
    result
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the snowmen need of the snow under them, handed over by the simulation.
pub struct Probe {
    /// Where a ball rests at this x.
    pub surface: f32,
    /// How deep the snow is there.
    pub depth: f32,
    /// How far under the surface the pointer still counts as on the snow.
    pub below: f32,
}

pub trait Terrain {
    fn width(&self) -> f32;
    fn probe(&self, x: f32) -> Probe;
    /// Takes `share` of the snow from the columns within `half` of x, and returns the area taken.
    fn take(&mut self, x: f32, half: f32, share: f32) -> f32;
    /// Throws an area of snow into the air at a point.
    fn burst(&mut self, area: f32, at: V2, velocity: V2);
}

#[derive(Clone, Debug)]
pub struct Ball {
    pub id: u32,
    pub pos: V2,
    pub vel: V2,
    pub mass: f32,
    /// The ball it rests on, once stacked.
    pub support: Option<u32>,
    pub seed: f32,
    pub roll: f32,
    pub settled: f32,
    pub airborne: bool,
    /// Burst or off the screen: taken out of the list once the step is over.
    pub gone: bool,
    /// When it got its face, if it has one.
    pub adorned: Option<f64>,
    /// Let go of at a throw's speed: it bursts where it lands or hits the edge of the screen.
    pub thrown: bool,
    /// How long it has been in the air since it was let go.
    pub flight: f32,
}

impl Ball {
    pub fn radius(&self) -> f32 {
        radius_of(self.mass)
    }
}

pub const DENSITY: f32 = 1.25;
pub const SMALLEST: f32 = 6.0;
pub const LARGEST: f32 = 54.0;
const GRAVITY: f32 = 2400.0;
/// A ball thrown or dropped harder than this bursts.
const BREAKS: f32 = 1250.0;
/// The pointer's speed, in points a second, from which letting go of a ball throws it.
const THROW_SPEED: f32 = 520.0;
/// A thrown ball that lands sooner than this after it was let go (a push along the ground) just rolls on.
const THROWN_FLIGHT: f32 = 0.12;

pub fn radius_of(mass: f32) -> f32 {
    (mass.max(0.0) / DENSITY).sqrt()
}

pub fn mass_of(radius: f32) -> f32 {
    radius * radius * DENSITY
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
    None,
    Ball(u32),
    Snow,
}

/// Balls of snow rolled up out of the snow on the ground, carried about and stacked into snowmen.
pub struct Snowmen {
    pub enabled: bool,
    pub balls: Vec<Ball>,
    /// The pointer is over something to take hold of: the overlay catches its clicks instead of letting them through.
    pub wants_pointer: bool,
    /// The pointer is holding a ball or rolling one: the snow it passes over is for the ball, not to be swept.
    pub holding: bool,
    next_id: u32,
    held: Option<u32>,
    grab: V2,
    was_down: bool,
    previous: Option<V2>,
    velocity: V2,
    hover: Target,
    /// When a ball last moved: the scene is drawn at full rate meanwhile.
    pub moved_at: f64,
    seed: u32,
}

impl Default for Snowmen {
    fn default() -> Snowmen {
        Snowmen {
            enabled: false,
            balls: Vec::new(),
            wants_pointer: false,
            holding: false,
            next_id: 1,
            held: None,
            grab: V2::ZERO,
            was_down: false,
            previous: None,
            velocity: V2::ZERO,
            hover: Target::None,
            moved_at: -10.0,
            seed: 12345,
        }
    }
}

impl Snowmen {
    pub fn set_enabled(&mut self, on: bool) {
        if self.enabled == on {
            return;
        }
        self.enabled = on;
        if !on {
            self.clear();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.balls.is_empty()
    }

    pub fn clear(&mut self) {
        self.balls.clear();
        self.held = None;
        self.hover = Target::None;
        self.wants_pointer = false;
        self.holding = false;
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Bursts every ball.
    pub fn smash_all(&mut self, terrain: &mut dyn Terrain) {
        for ball in &self.balls {
            terrain.burst(ball.mass * 0.9, ball.pos, V2::new(0.0, 120.0));
        }
        self.clear();
    }

    /// Bursts the balls the blade of the snow plough passes over between two x positions.
    pub fn smash(&mut self, from: f32, to: f32, terrain: &mut dyn Terrain) {
        let (low, high) = (from.min(to), from.max(to));
        let mut gone: Vec<u32> = self
            .balls
            .iter()
            .filter(|b| b.pos.x + b.radius() >= low && b.pos.x - b.radius() <= high && Some(b.id) != self.held)
            .map(|b| b.id)
            .collect();
        if gone.is_empty() {
            return;
        }
        loop {
            let more: Vec<u32> = self.balls.iter().filter(|b| !gone.contains(&b.id) && b.support.is_some_and(|s| gone.contains(&s))).map(|b| b.id).collect();
            if more.is_empty() {
                break;
            }
            gone.extend(more);
        }
        for ball in &self.balls {
            if gone.contains(&ball.id) {
                let side = ((ball.seed * 127.1).sin() * 43758.5453).fract() * 2.0 - 1.0;
                terrain.burst(ball.mass * 0.9, ball.pos, V2::new(side * 160.0, 150.0));
            }
        }
        self.balls.retain(|b| !gone.contains(&b.id));
    }

    /// Advances the balls, and the user's hold on them. `pointer` is None while the pointer is out of reach.
    pub fn update(&mut self, dt: f32, pointer: Option<V2>, down: bool, terrain: &mut dyn Terrain, clock: f64) -> bool {
        if !self.enabled && self.balls.is_empty() {
            self.wants_pointer = false;
            self.holding = false;
            return false;
        }
        let mut changed = false;
        match pointer {
            Some(p) => {
                if let Some(previous) = self.previous
                    && dt > 0.0
                {
                    let raw = (p - previous) / dt.max(1.0 / 240.0);
                    self.velocity += (raw - self.velocity) * (dt * 14.0).min(1.0);
                }
                self.previous = Some(p);
            }
            None => {
                self.previous = None;
                self.velocity = V2::ZERO;
            }
        }

        let pressed = down && !self.was_down;
        self.was_down = down;
        if self.held.is_none() {
            self.hover = match pointer {
                Some(p) if self.enabled => self.target(p, terrain),
                _ => Target::None,
            };
            if pressed && let Some(p) = pointer {
                match self.hover {
                    Target::Ball(id) => self.pick_up(id, p),
                    Target::Snow => self.start_roll(p),
                    Target::None => {}
                }
            }
        } else if !down || pointer.is_none() {
            self.drop_held();
        }
        self.wants_pointer = self.held.is_some() || self.hover != Target::None;
        self.holding = self.held.is_some();

        if let (Some(id), Some(p)) = (self.held, pointer)
            && let Some(i) = self.balls.iter().position(|b| b.id == id)
        {
            self.move_held(i, p + self.grab, dt, terrain);
            changed = true;
        }
        for i in 0..self.balls.len() {
            if Some(self.balls[i].id) != self.held && self.advance(i, dt, terrain) {
                changed = true;
            }
        }
        if self.balls.iter().any(|b| b.gone) {
            let lost: Vec<u32> = self.balls.iter().filter(|b| b.gone).map(|b| b.id).collect();
            self.balls.retain(|b| !b.gone);
            for ball in &mut self.balls {
                if ball.support.is_some_and(|s| lost.contains(&s)) {
                    ball.support = None;
                    ball.airborne = true;
                }
            }
            changed = true;
        }
        self.adorn(clock);
        if changed || self.held.is_some() {
            self.moved_at = clock;
        }
        changed || self.held.is_some()
    }

    fn target(&self, point: V2, terrain: &dyn Terrain) -> Target {
        // The topmost ball first; one with another on top of it is out of reach.
        let mut order: Vec<&Ball> = self.balls.iter().collect();
        order.sort_by(|a, b| b.pos.y.total_cmp(&a.pos.y));
        for ball in order {
            if (point - ball.pos).length() <= ball.radius() + 3.0 {
                return if self.balls.iter().any(|o| o.support == Some(ball.id)) { Target::None } else { Target::Ball(ball.id) };
            }
        }
        let probe = terrain.probe(point.x);
        if probe.depth >= 1.5 && point.y >= probe.surface - probe.below && point.y <= probe.surface + 12.0 {
            return Target::Snow;
        }
        Target::None
    }

    fn pick_up(&mut self, id: u32, point: V2) {
        let Some(i) = self.balls.iter().position(|b| b.id == id) else { return };
        self.held = Some(id);
        self.grab = self.balls[i].pos - point;
        let ball = &mut self.balls[i];
        ball.support = None;
        ball.airborne = false;
        ball.vel = V2::ZERO;
        ball.adorned = None;
        ball.settled = 0.0;
        ball.thrown = false;
        ball.flight = 0.0;
    }

    /// Starts a ball in the snow at the pointer: dragged along the ground it picks up the snow it rolls over.
    fn start_roll(&mut self, point: V2) {
        let seed = 0.01 + self.noise() * 0.99;
        let ball = Ball {
            id: self.next_id,
            pos: point,
            vel: V2::ZERO,
            mass: mass_of(SMALLEST),
            support: None,
            seed,
            roll: 0.0,
            settled: 0.0,
            airborne: false,
            gone: false,
            adorned: None,
            thrown: false,
            flight: 0.0,
        };
        self.next_id += 1;
        self.held = Some(ball.id);
        self.grab = V2::ZERO;
        self.balls.push(ball);
        // Several balls in one place would be a heap: the count is kept to what fits a screen.
        if self.balls.len() > 14 {
            self.balls.remove(0);
        }
    }

    fn drop_held(&mut self) {
        let Some(id) = self.held.take() else { return };
        let Some(i) = self.balls.iter().position(|b| b.id == id) else { return };
        let mut v = self.velocity;
        let speed = v.length();
        if speed > 2800.0 {
            v *= 2800.0 / speed;
        }
        let ball = &mut self.balls[i];
        // Let go of at speed it is a throw: it flies on and bursts at the end of it.
        ball.thrown = speed > THROW_SPEED;
        ball.flight = 0.0;
        ball.vel = if speed > 220.0 { v * 0.9 } else { v * 0.2 };
        ball.airborne = true;
        ball.settled = 0.0;
    }

    fn move_held(&mut self, i: usize, target: V2, dt: f32, terrain: &mut dyn Terrain) {
        let mut ball = self.balls[i].clone();
        let r = ball.radius();
        let from = ball.pos.x;
        let ease = (dt * 30.0).min(1.0);
        ball.pos.x += (target.x - ball.pos.x) * ease;
        ball.pos.x = ball.pos.x.clamp(0.0, terrain.width());
        let floor = self.rest(ball.pos.x, r, ball.pos.y, ball.id, terrain).0;
        ball.pos.y = floor.max(ball.pos.y + (target.y - ball.pos.y) * ease);
        let travel = (ball.pos.x - from).abs();
        if travel > 0.001 {
            ball.roll += (ball.pos.x - from) / r.max(1.0);
            // On the snow and rolling: the snow it passes over wraps around it.
            if ball.pos.y - floor < 2.0 && r < LARGEST && terrain.probe(ball.pos.x).depth > 0.3 {
                let share = (travel / (r * 1.1).max(1.0)).min(1.0) * 0.9;
                let taken = terrain.take(ball.pos.x, r * 0.55, share);
                // Snow packs as it is rolled up: the ball gains less than the snow it took up.
                ball.mass = (ball.mass + taken * 0.5).min(mass_of(LARGEST));
            }
        }
        self.balls[i] = ball;
    }

    /// The balls resting on a ball, and those on them.
    fn above(&self, id: u32) -> Vec<u32> {
        let mut found: Vec<u32> = Vec::new();
        loop {
            let more: Vec<u32> = self
                .balls
                .iter()
                .filter(|b| !found.contains(&b.id) && b.support.is_some_and(|s| s == id || found.contains(&s)))
                .map(|b| b.id)
                .collect();
            if more.is_empty() {
                return found;
            }
            found.extend(more);
        }
    }

    /// Where a ball of radius r at x comes to rest: on the snow or on the ball under it, and which ball that is. Only
    /// the balls lower than it hold it up (a held one rides over any), never those resting on it.
    fn rest(&self, x: f32, r: f32, y: f32, id: u32, terrain: &dyn Terrain) -> (f32, Option<usize>) {
        let mut y0 = terrain.probe(x).surface + r * 0.94;
        let mut on = None;
        let held = Some(id) == self.held;
        let over = self.above(id);
        for (k, other) in self.balls.iter().enumerate() {
            if other.id == id || Some(other.id) == self.held || over.contains(&other.id) {
                continue;
            }
            if !held && !(other.pos.y < y || (other.pos.y == y && other.id < id)) {
                continue;
            }
            let dx = x - other.pos.x;
            let reach = r + other.radius();
            if dx.abs() >= reach {
                continue;
            }
            let top = other.pos.y + (reach * reach - dx * dx).sqrt() * 0.985;
            if top > y0 {
                y0 = top;
                on = Some(k);
            }
        }
        (y0, on)
    }

    fn advance(&mut self, i: usize, dt: f32, terrain: &mut dyn Terrain) -> bool {
        let mut ball = self.balls[i].clone();
        let r = ball.radius();
        let before = ball.pos;
        let (_, spot) = self.rest(ball.pos.x, r, ball.pos.y, ball.id, terrain);
        if ball.airborne {
            ball.vel.y -= GRAVITY * dt;
            ball.pos += ball.vel * dt;
            ball.flight += dt;
            if ball.thrown && ball.flight > THROWN_FLIGHT && (ball.pos.x < r || ball.pos.x > terrain.width() - r) {
                // It hits the side of the screen and bursts there, the snow thrown back into the room.
                let wall = if ball.pos.x < r { 0.0 } else { terrain.width() };
                let back = if ball.pos.x < r { 1.0 } else { -1.0 };
                terrain.burst(ball.mass * 0.9, V2::new(wall + back * r * 0.5, ball.pos.y.max(4.0)), V2::new(back * ball.vel.x.abs() * 0.12, ball.vel.y * 0.2));
                self.balls[i].gone = true;
                return true;
            }
            if ball.pos.x < -r || ball.pos.x > terrain.width() + r || ball.pos.y < -200.0 {
                terrain.burst(ball.mass * 0.9, V2::new(ball.pos.x.clamp(0.0, terrain.width()), 4.0), V2::ZERO);
                self.balls[i].gone = true;
                return true;
            }
            let (floor, _) = self.rest(ball.pos.x, r, ball.pos.y, ball.id, terrain);
            if ball.pos.y <= floor && ball.vel.y <= 0.0 {
                let impact = -ball.vel.y;
                if ball.thrown && ball.flight > THROWN_FLIGHT {
                    terrain.burst(ball.mass * 0.9, V2::new(ball.pos.x, floor), V2::new(ball.vel.x * 0.35, impact * 0.15 + 60.0));
                    self.balls[i].gone = true;
                    return true;
                }
                ball.thrown = false;
                if impact > BREAKS {
                    terrain.burst(ball.mass * 0.9, V2::new(ball.pos.x, floor), V2::new(ball.vel.x * 0.3, impact * 0.12));
                    self.balls[i].gone = true;
                    return true;
                }
                ball.pos.y = floor;
                ball.vel.y = 0.0;
                ball.airborne = false;
            }
            self.balls[i] = ball;
            return true;
        }

        // Resting: a ball on top of another stays if it is centred and not the bigger one, else rolls off.
        if let Some(k) = spot {
            let (under_id, under_x, under_r) = (self.balls[k].id, self.balls[k].pos.x, self.balls[k].radius());
            let dx = ball.pos.x - under_x;
            if dx.abs() <= under_r * 0.5 && r <= under_r * 1.1 {
                ball.support = Some(under_id);
                ball.pos.x += (under_x - ball.pos.x) * (dt * 9.0).min(1.0);
                ball.vel.x = 0.0;
            } else {
                ball.support = None;
                let side = if dx.abs() < 0.5 {
                    if ball.seed > 0.5 { 1.0 } else { -1.0 }
                } else if dx > 0.0 {
                    1.0
                } else {
                    -1.0
                };
                ball.vel.x += side * 900.0 * dt;
                ball.vel.x *= (-dt * 0.8).exp();
                ball.pos.x += ball.vel.x * dt;
                ball.roll += ball.vel.x * dt / r.max(1.0);
            }
        } else {
            ball.support = None;
            ball.vel.x *= (-dt * 6.0).exp();
            if ball.vel.x.abs() < 2.0 {
                ball.vel.x = 0.0;
            }
            ball.pos.x += ball.vel.x * dt;
            ball.roll += ball.vel.x * dt / r.max(1.0);
        }
        // The snow it rests on may have been scraped from under it, or a ball beneath it taken.
        let (floor, _) = self.rest(ball.pos.x, r, ball.pos.y, ball.id, terrain);
        if ball.pos.y > floor + 1.5 {
            ball.airborne = true;
            ball.support = None;
        } else {
            ball.pos.y = floor;
        }
        let moved = (ball.pos - before).length() > 0.12;
        ball.settled = if moved || ball.airborne { 0.0 } else { ball.settled + dt };
        let active = moved || ball.airborne;
        self.balls[i] = ball;
        active
    }

    /// The balls stacked on top of one another, bottom to top, as indices.
    fn stacks(&self) -> Vec<Vec<usize>> {
        let mut result = Vec::new();
        for (i, ball) in self.balls.iter().enumerate() {
            if ball.support.is_some() || ball.airborne || Some(ball.id) == self.held {
                continue;
            }
            let mut chain = vec![i];
            while chain.len() < 8 {
                let top = self.balls[*chain.last().unwrap()].id;
                match self.balls.iter().position(|b| b.support == Some(top) && !b.airborne) {
                    Some(n) => chain.push(n),
                    None => break,
                }
            }
            result.push(chain);
        }
        result
    }

    /// Three balls stacked make a snowman: it gets a face once it has stood still a moment.
    fn adorn(&mut self, clock: f64) {
        let mut finished: Vec<u32> = Vec::new();
        for chain in self.stacks() {
            if chain.len() >= 3 && chain.iter().all(|&i| self.balls[i].settled > 0.35) {
                finished.extend(chain.iter().map(|&i| self.balls[i].id));
            }
        }
        for ball in &mut self.balls {
            if finished.contains(&ball.id) {
                if ball.adorned.is_none() {
                    ball.adorned = Some(clock);
                }
            } else {
                ball.adorned = None;
            }
        }
    }

    /// What to draw, lowest first.
    pub fn sprites(&self, clock: f64) -> Vec<BallSprite> {
        let mut roles: Vec<(u32, f32)> = Vec::new();
        for chain in self.stacks() {
            if chain.len() >= 3 && chain.iter().all(|&i| self.balls[i].adorned.is_some()) {
                for (k, &i) in chain.iter().enumerate() {
                    let role = if k == chain.len() - 1 {
                        2.0
                    } else if k == 0 {
                        0.0
                    } else {
                        1.0
                    };
                    roles.push((self.balls[i].id, role));
                }
            }
        }
        let mut balls: Vec<&Ball> = self.balls.iter().collect();
        balls.sort_by(|a, b| a.pos.y.total_cmp(&b.pos.y));
        balls
            .into_iter()
            .map(|ball| {
                let age = ball.adorned.map_or(0.0, |at| (clock - at) as f32);
                BallSprite {
                    center: ball.pos,
                    radius: ball.radius(),
                    seed: ball.seed,
                    roll: ball.roll,
                    role: roles.iter().find(|r| r.0 == ball.id).map_or(0.0, |r| r.1),
                    pop: if ball.adorned.is_some() { (-age * 4.5).exp() } else { 0.0 },
                }
            })
            .collect()
    }
}

// MARK: Frost

/// Frost growing in from the corners and edges of the screen, drawn as feathery ferns whose growth order is stored in
/// the pattern, and wiped away wherever the pointer passes, to freeze over again slowly.
pub struct Frost {
    pub width: usize,
    pub height: usize,
    /// Two bytes a point, y running down: how much frost is there, and when in its growth it reaches that point.
    pub pattern: Vec<u8>,
    pub cell: usize,
    pub wipe_width: usize,
    pub wipe_height: usize,
    /// How far each cell of the glass has been wiped clear, 0 to 255, rows from the top.
    pub wipe: Vec<u8>,
    /// How far the frost has grown, 0 to 1.
    pub progress: f32,
    pub enabled: bool,
    /// Seconds from clear glass to the screen's edges frozen over.
    pub duration: f32,
    /// Seconds for the wiped glass to freeze over again.
    pub refreeze: f32,
    shown: f32,
    regrow_at: f64,
    wiped: usize,
    pointer: Option<V2>,
    /// The cells changed since the renderer last looked, left, top, right, bottom, inclusive.
    dirty: Option<[usize; 4]>,
    /// Everything changed: the growth moved on.
    grown: bool,
}

pub const FROST_REACH: f32 = 26.0;

impl Default for Frost {
    fn default() -> Frost {
        let duration = std::env::var("SNOWDRIFT_FROST_SECONDS").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 1.0).unwrap_or(210.0);
        Frost {
            width: 0,
            height: 0,
            pattern: Vec::new(),
            cell: 4,
            wipe_width: 0,
            wipe_height: 0,
            wipe: Vec::new(),
            progress: 0.0,
            enabled: false,
            duration,
            refreeze: 50.0,
            shown: 0.0,
            regrow_at: 0.0,
            wiped: 0,
            pointer: None,
            dirty: None,
            grown: true,
        }
    }
}

impl Frost {
    pub fn set_enabled(&mut self, on: bool) {
        if self.enabled == on {
            return;
        }
        self.enabled = on;
        if on {
            self.ensure_pattern();
        } else {
            self.progress = 0.0;
            self.shown = 0.0;
            self.wiped = 0;
            self.wipe.iter_mut().for_each(|v| *v = 0);
        }
        self.grown = true;
    }

    /// Frost on the glass, or growing: the screen is drawn.
    pub fn active(&self) -> bool {
        self.enabled && !self.pattern.is_empty()
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = ((width.round() as usize).max(1), (height.round() as usize).max(1));
        if w == self.width && h == self.height {
            return;
        }
        self.width = w;
        self.height = h;
        self.wipe_width = w.div_ceil(self.cell);
        self.wipe_height = h.div_ceil(self.cell);
        self.wipe = vec![0; self.wipe_width * self.wipe_height];
        self.wiped = 0;
        self.pattern.clear();
        self.grown = true;
        if self.enabled {
            self.ensure_pattern();
        }
    }

    pub fn ensure_pattern(&mut self) {
        if self.pattern.is_empty() && self.width > 0 && self.height > 0 {
            self.pattern = make_pattern(self.width, self.height, 0x5EED);
            self.grown = true;
        }
    }

    /// What changed since the last call: `Some(None)` everything, `Some(Some(rect))` the wiped cells in the rectangle
    /// (in points, left, top, right, bottom), `None` nothing.
    pub fn take_dirty(&mut self) -> Option<Option<[usize; 4]>> {
        if self.grown {
            self.grown = false;
            self.dirty = None;
            return Some(None);
        }
        self.dirty.take().map(|[l, t, r, b]| Some([l * self.cell, t * self.cell, ((r + 1) * self.cell).min(self.width), ((b + 1) * self.cell).min(self.height)]))
    }

    /// Grows the frost, wipes it where the pointer is and lets the wiped glass freeze over; returns whether what is
    /// drawn changed.
    pub fn step(&mut self, dt: f32, pointer: Option<V2>, clock: f64) -> bool {
        if !self.enabled || self.pattern.is_empty() {
            return false;
        }
        let mut changed = false;
        if self.progress < 1.0 {
            self.progress = (self.progress + dt / self.duration).min(1.0);
            if self.progress - self.shown >= 0.0015 || (self.progress >= 1.0 && self.shown < 1.0) {
                self.shown = self.progress;
                self.grown = true;
                changed = true;
            }
        }
        match pointer {
            Some(p) => {
                self.wipe_along(self.pointer.unwrap_or(p), p);
                self.pointer = Some(p);
            }
            None => self.pointer = None,
        }
        if self.wiped > 0 && clock >= self.regrow_at {
            self.regrow_at = clock + 0.2;
            let step = ((255.0 * 0.2 / self.refreeze).ceil() as i32).clamp(1, 255) as u8;
            let mut left = 0;
            let width = self.wipe_width;
            for (i, v) in self.wipe.iter_mut().enumerate() {
                if *v > 0 {
                    *v = v.saturating_sub(step);
                    if *v > 0 {
                        left += 1;
                    }
                    let (x, y) = (i % width, i / width);
                    mark(&mut self.dirty, x, y);
                }
            }
            self.wiped = left;
        }
        changed || self.dirty.is_some() || self.grown
    }

    fn wipe_along(&mut self, a: V2, b: V2) {
        let distance = (b - a).length();
        let steps = ((distance / (FROST_REACH * 0.4)) as usize).max(1);
        for k in 1..=steps {
            let p = a + (b - a) * (k as f32 / steps as f32);
            self.stamp(p);
        }
    }

    /// The pointer at a point in overlay coordinates, whose y runs up from the bottom of the screen.
    fn stamp(&mut self, p: V2) {
        let cell = self.cell as f32;
        let (cx, cy) = (p.x / cell, (self.height as f32 - p.y) / cell);
        let radius = FROST_REACH / cell;
        let x0 = ((cx - radius).floor().max(0.0)) as usize;
        let y0 = ((cy - radius).floor().max(0.0)) as usize;
        let x1 = (((cx + radius).ceil()).max(0.0) as usize).min(self.wipe_width.saturating_sub(1));
        let y1 = (((cy + radius).ceil()).max(0.0) as usize).min(self.wipe_height.saturating_sub(1));
        if x0 > x1 || y0 > y1 {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy)) / radius;
                if d >= 1.0 {
                    continue;
                }
                let t = ((1.0 - d) / 0.45).clamp(0.0, 1.0);
                let amount = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
                let i = y * self.wipe_width + x;
                if amount > self.wipe[i] {
                    if self.wipe[i] == 0 {
                        self.wiped += 1;
                    }
                    self.wipe[i] = amount;
                    mark(&mut self.dirty, x, y);
                }
            }
        }
    }
}

fn mark(dirty: &mut Option<[usize; 4]>, x: usize, y: usize) {
    match dirty {
        Some(d) => {
            d[0] = d[0].min(x);
            d[1] = d[1].min(y);
            d[2] = d[2].max(x);
            d[3] = d[3].max(y);
        }
        None => *dirty = Some([x, y, x, y]),
    }
}

struct Source(u64);

impl Source {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next()
    }
}

struct Canvas {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
    /// Path length that reads as the end of the growth.
    reach: f32,
}

impl Canvas {
    /// A soft dot of the given radius; where dots overlap the stronger one stays, with its own arrival time.
    fn dot(&mut self, x: f32, y: f32, radius: f32, arrival: f32) {
        let r = radius + 1.0;
        let x0 = ((x - r).floor().max(0.0)) as usize;
        let y0 = ((y - r).floor().max(0.0)) as usize;
        let x1 = (((x + r).ceil()).max(0.0) as usize).min(self.width - 1);
        let y1 = (((y + r).ceil()).max(0.0) as usize).min(self.height - 1);
        if x0 > x1 || y0 > y1 {
            return;
        }
        let time = ((arrival / self.reach).clamp(0.0, 1.0) * 255.0) as u8;
        for py in y0..=y1 {
            for px in x0..=x1 {
                let d = ((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt();
                let cover = (radius + 0.5 - d).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let i = (py * self.width + px) * 2;
                let value = (cover * 255.0) as u8;
                if value > self.bytes[i] {
                    self.bytes[i] = value;
                    self.bytes[i + 1] = time;
                } else if value > 0 && time < self.bytes[i + 1] && self.bytes[i] > 40 {
                    self.bytes[i + 1] = time;
                }
            }
        }
    }
}

/// Draws one frond growing from a point: a curving stem with feathers on both sides that shorten toward the tip.
#[allow(clippy::too_many_arguments)]
fn frond(canvas: &mut Canvas, random: &mut Source, origin: V2, start: f32, length: f32, curl: f32, thickness: f32, start_path: f32, depth: u32) {
    let step = 1.1;
    let steps = ((length / step) as usize).max(1);
    let mut position = origin;
    let mut angle = start;
    let spacing = if depth == 2 { 6 } else { 5 };
    let mut path = start_path;
    for s in 0..steps {
        let along = s as f32 / steps as f32;
        angle += curl / steps as f32;
        position += V2::new(angle.cos(), angle.sin()) * step;
        path += step;
        let radius = (thickness * (1.0 - 0.55 * along)).max(0.45);
        canvas.dot(position.x, position.y, radius, path);
        if depth >= 2 || s % spacing != 2 || along <= 0.03 || along >= 0.97 {
            continue;
        }
        for side in [-1.0f32, 1.0] {
            let feather = length * if depth == 0 { 0.3 } else { 0.4 } * (1.0 - along).powf(0.75) * random.range(0.8, 1.15);
            if feather <= 5.0 {
                continue;
            }
            let spread = 1.18 - 0.5 * along;
            let curl = -side * random.range(0.05, 0.45);
            frond(canvas, random, position, angle + side * spread, feather, curl, thickness * 0.55, path, depth + 1);
        }
    }
}

/// The ferns of frost: stems with feathers on both sides, themselves feathered, drawn from the corners and the middles
/// of the edges toward the middle of the screen. `width` by `height` points, y running down as in a picture.
pub fn make_pattern(width: usize, height: usize, seed: u64) -> Vec<u8> {
    let mut random = Source(seed);
    let (w, h) = (width as f32, height as f32);
    let big = w.min(h);
    let reach = big * 1.15;
    let mut canvas = Canvas { width, height, bytes: vec![0; width * height * 2], reach };

    let corners = [
        (V2::new(-2.0, -2.0), std::f32::consts::FRAC_PI_4),
        (V2::new(w + 2.0, -2.0), std::f32::consts::PI * 3.0 / 4.0),
        (V2::new(-2.0, h + 2.0), -std::f32::consts::FRAC_PI_4),
        (V2::new(w + 2.0, h + 2.0), -std::f32::consts::PI * 3.0 / 4.0),
    ];
    for (origin, direction) in corners {
        for offset in [-0.62f32, -0.2, 0.2, 0.62] {
            let angle = direction + offset + random.range(-0.08, 0.08);
            let length = big * random.range(0.5, 0.78);
            let curl = random.range(-0.5, 0.5);
            let thickness = random.range(1.5, 2.0);
            let start = random.range(0.0, reach * 0.12);
            frond(&mut canvas, &mut random, origin, angle, length, curl, thickness, start, 0);
        }
    }
    let edges = [
        (V2::new(w * random.range(0.35, 0.65), -2.0), std::f32::consts::FRAC_PI_2),
        (V2::new(w * random.range(0.35, 0.65), h + 2.0), -std::f32::consts::FRAC_PI_2),
        (V2::new(-2.0, h * random.range(0.35, 0.65)), 0.0),
        (V2::new(w + 2.0, h * random.range(0.35, 0.65)), std::f32::consts::PI),
    ];
    for (origin, direction) in edges {
        for offset in [-0.55f32, 0.0, 0.55] {
            let angle = direction + offset + random.range(-0.1, 0.1);
            let length = big * random.range(0.3, 0.5);
            let curl = random.range(-0.4, 0.4);
            let thickness = random.range(1.2, 1.7);
            let start = random.range(0.0, reach * 0.18);
            frond(&mut canvas, &mut random, origin, angle, length, curl, thickness, start, 0);
        }
    }
    for _ in 0..10 {
        let side = (random.range(0.0, 4.0) as usize) % 4;
        let t = random.range(0.08, 0.92);
        let (origin, direction) = match side {
            0 => (V2::new(w * t, -2.0), std::f32::consts::FRAC_PI_2),
            1 => (V2::new(w * t, h + 2.0), -std::f32::consts::FRAC_PI_2),
            2 => (V2::new(-2.0, h * t), 0.0),
            _ => (V2::new(w + 2.0, h * t), std::f32::consts::PI),
        };
        let angle = direction + random.range(-0.7, 0.7);
        let length = big * random.range(0.12, 0.26);
        let curl = random.range(-0.5, 0.5);
        let thickness = random.range(1.0, 1.4);
        let start = random.range(reach * 0.05, reach * 0.3);
        frond(&mut canvas, &mut random, origin, angle, length, curl, thickness, start, 0);
    }
    canvas.bytes
}

/// One pixel of the frost, premultiplied, for the point at `x`, `y` (down from the top) in a screen `width` by `height`.
pub fn frost_pixel(frost: &Frost, x: usize, y: usize, time: f32) -> [f32; 4] {
    let i = (y * frost.width + x) * 2;
    let (cover, arrival) = (frost.pattern[i] as f32 / 255.0, frost.pattern[i + 1] as f32 / 255.0);
    let wipe = frost.wipe[(y / frost.cell).min(frost.wipe_height - 1) * frost.wipe_width + (x / frost.cell).min(frost.wipe_width - 1)] as f32 / 255.0;
    let progress = frost.progress;
    let grown = saturate((progress * 1.3 - arrival) / 0.1);
    let (u, v) = (x as f32 / frost.width as f32, y as f32 / frost.height as f32);
    let edge_distance = u.min(1.0 - u).min(v.min(1.0 - v));
    let (px, py) = (x as f32, y as f32);
    let mist = 0.55 * fx_noise(px / 90.0 + 3.0, py / 90.0 + 3.0) + 0.45 * fx_noise(px / 28.0 + 11.0, py / 28.0 + 11.0);
    let haze = saturate(progress * 1.1) * (0.05 + 0.2 * smoothstep(0.5, 0.0, edge_distance)) * (0.4 + 0.9 * mist);
    let body = cover * grown;
    let mut alpha = 1.0 - (1.0 - body * 0.86) * (1.0 - haze);
    let mut color = [0.80 + 0.20 * saturate(body * 1.3), 0.90 + 0.10 * saturate(body * 1.3), 0.99 + 0.01 * saturate(body * 1.3)];
    let seed = fx_hash((px * 0.5).floor() + 17.3, (py * 0.5).floor() + 17.3);
    if seed > 0.985 && body > 0.3 {
        let glint = (time * (0.8 + seed * 1.5) + seed * 80.0).sin().clamp(0.0, 1.0).powi(18);
        for c in &mut color {
            *c += (1.0 - *c) * glint;
        }
    }
    let rim = 4.0 * wipe * (1.0 - wipe) * saturate(progress * 2.0);
    alpha = (alpha * (1.0 - wipe)).max(rim * 0.22);
    [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
}
