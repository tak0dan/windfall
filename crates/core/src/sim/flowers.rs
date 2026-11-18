//! Flowers that sprout along the top edge of a surface, sway when the cursor brushes them, and keep growing
//! taller for as long as it stays.

use super::Surf;
use crate::draw::Batch;
use crate::palette::{self, Rgb};
use crate::rng::Rng;

pub struct Flower {
    x: f32,
    h: f32,
    /// Height it reaches by itself.
    hmax: f32,
    /// Where bloom starts, fixed at birth.
    h0: f32,
    rate: f32,
    kind: u8,
    col: Rgb,
    center: Rgb,
    stem: Rgb,
    petals: u8,
    size: f32,
    phase: f32,
    bend: f32,
    excite: f32,
    leaf_side: f32,
    hidden: bool,
}

pub struct Bed {
    pub x0: f32,
    pub base: f32,
    width: f32,
    flowers: Vec<Flower>,
    spawn_acc: f32,
}

impl Bed {
    pub fn new(s: &Surf) -> Bed {
        Bed { x0: s.rect.x, base: s.rect.y, width: s.rect.w, flowers: Vec::new(), spawn_acc: 0.0 }
    }

    pub fn follow(&mut self, s: &Surf) {
        self.x0 = s.rect.x;
        self.base = s.rect.y;
        self.width = s.rect.w;
        let (lo, hi) = (s.rect.x + s.inset, s.rect.right() - s.inset);
        self.flowers.retain(|f| f.x >= lo && f.x <= hi);
    }

    pub fn step(&mut self, dt: f32, time: f32, intensity: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) {
        let inset = 6.0;
        let usable = (self.width - 2.0 * inset).max(0.0);
        let cap = (usable * (0.012 + 0.02 * intensity)) as usize;
        self.spawn_acc += dt * (0.4 + 1.6 * intensity) * (usable / 500.0).max(0.2);
        if self.spawn_acc >= 1.0 {
            self.spawn_acc = 0.0;
            if self.flowers.len() < cap {
                let x = self.x0 + inset + rng.f() * usable;
                if !self.flowers.iter().any(|f| (f.x - x).abs() < 9.0) {
                    self.flowers.push(new_flower(x, rng));
                }
            }
        }
        for f in &mut self.flowers {
            f.hidden = hidden_at(f.x);
            if intensity < 0.01 {
                f.h -= 10.0 * dt;
                continue;
            }
            // Slow ease up to its own height.
            if f.h < f.hmax {
                f.h = (f.h + f.rate * dt * (0.4 + 0.6 * (1.0 - f.h / f.hmax))).min(f.hmax);
            }
            let mut touched = false;
            let mut push = 0.0;
            if let Some(c) = cursor {
                let head_y = self.base - f.h;
                let dx = c[0] - f.x;
                if dx.abs() < 46.0 && c[1] > head_y - 46.0 && c[1] < self.base + 14.0 {
                    touched = true;
                    push = (-dx * 0.35).clamp(-14.0, 14.0);
                }
            }
            if touched {
                f.excite = (f.excite + dt * 4.0).min(1.0);
                // Reaches up while it is being played with, to at most twice its height.
                let limit = f.hmax * 2.0;
                if f.h < limit {
                    f.h += 18.0 * dt;
                    if f.h > f.hmax {
                        f.hmax = f.h;
                    }
                }
            } else {
                f.excite = (f.excite - dt * 1.4).max(0.0);
            }
            let idle = (time * 1.1 + f.phase).sin() * 1.6 * (f.h / f.hmax.max(1.0));
            let wave = (time * 7.0 + f.phase).sin() * 6.0 * f.excite;
            let target = idle + wave + push * f.excite;
            f.bend += (target - f.bend) * (1.0 - (-dt * 10.0).exp());
        }
        self.flowers.retain(|f| f.h > 0.5);
    }

    pub fn draw(&self, b: &mut Batch, _time: f32, pixel: bool) {
        for f in &self.flowers {
            if f.hidden {
                continue;
            }
            f.draw(b, self.base, pixel);
        }
    }
}

fn new_flower(x: f32, rng: &mut Rng) -> Flower {
    let hmax = rng.range(16.0, 46.0);
    let hue = rng.f();
    let col = palette::hsv(hue, rng.range(0.5, 0.95), rng.range(0.88, 1.0));
    Flower {
        x,
        h: 1.0,
        hmax,
        h0: hmax,
        rate: rng.range(5.0, 11.0),
        kind: rng.below(5) as u8,
        col,
        center: if rng.chance(0.7) { [1.0, 0.82, 0.2] } else { palette::mix(col, [1.0, 1.0, 1.0], 0.6) },
        stem: palette::mix(palette::STEM, [0.5, 0.8, 0.3], rng.f() * 0.6),
        petals: 5 + rng.below(4) as u8,
        size: rng.range(3.4, 6.6),
        phase: rng.f() * 6.28,
        bend: 0.0,
        excite: 0.0,
        leaf_side: if rng.chance(0.5) { 1.0 } else { -1.0 },
        hidden: false,
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Flower {
    fn draw(&self, b: &mut Batch, base: f32, pixel: bool) {
        let head = [self.x + self.bend, base - self.h];
        let ctrl = [self.x + self.bend * 0.15, base - self.h * 0.55];
        let at = |t: f32| {
            let u = 1.0 - t;
            [u * u * self.x + 2.0 * u * t * ctrl[0] + t * t * head[0], u * u * base + 2.0 * u * t * ctrl[1] + t * t * head[1]]
        };
        let sc = [self.stem[0], self.stem[1], self.stem[2], 1.0];
        let width = if pixel { 1.8 } else { 1.0 + self.size * 0.14 };
        let segs = 6;
        let mut prev = at(0.0);
        for i in 1..=segs {
            let p = at(i as f32 / segs as f32);
            b.line(prev, p, width, sc);
            prev = p;
        }
        if self.h > 9.0 {
            let l = at(0.42);
            let s = (self.h / self.hmax.max(1.0)).min(1.0) * self.size;
            b.ellipse(l[0] + self.leaf_side * s * 0.7, l[1] - s * 0.2, s * 0.85, s * 0.34, -self.leaf_side * 0.6, sc);
        }
        let bloom = smooth(0.32, 0.9, self.h / self.h0.max(1.0));
        if bloom < 0.12 {
            b.disc(head[0], head[1], 1.4 + self.size * 0.12, [0.45, 0.78, 0.35, 1.0]);
            return;
        }
        let s = self.size * (0.35 + 0.65 * bloom);
        let c = [self.col[0], self.col[1], self.col[2], 1.0];
        let cc = [self.center[0], self.center[1], self.center[2], 1.0];
        let [hx, hy] = head;
        match self.kind {
            0 => {
                for k in 0..self.petals {
                    let a = k as f32 / self.petals as f32 * std::f32::consts::TAU + self.phase;
                    b.ellipse(hx + a.cos() * s * 0.85, hy + a.sin() * s * 0.85, s * 0.62, s * 0.3, a, c);
                }
                b.disc(hx, hy, s * 0.42, cc);
            }
            1 => {
                b.ellipse(hx - s * 0.45, hy, s * 0.5, s * 0.85, -0.3, c);
                b.ellipse(hx + s * 0.45, hy, s * 0.5, s * 0.85, 0.3, c);
                b.ellipse(hx, hy - s * 0.15, s * 0.55, s * 0.95, 0.0, [self.col[0] * 0.9, self.col[1] * 0.9, self.col[2] * 0.9, 1.0]);
            }
            2 => {
                b.disc(hx, hy - s * 0.1, s * 0.45, c);
                b.tri([hx - s * 0.8, hy - s * 0.05], [hx + s * 0.8, hy - s * 0.05], [hx, hy + s * 1.0], c);
                b.disc(hx, hy + s * 0.95, s * 0.2, cc);
            }
            3 => {
                for k in 0..6u8 {
                    let a = k as f32 / 6.0 * std::f32::consts::TAU + self.phase;
                    b.disc(hx + a.cos() * s * 0.7, hy + a.sin() * s * 0.7, s * 0.5, c);
                }
                b.disc(hx, hy, s * 0.55, cc);
            }
            _ => {
                for k in 0..5u8 {
                    let a = k as f32 / 5.0 * std::f32::consts::TAU + self.phase;
                    let (l, r) = (a - 0.38, a + 0.38);
                    b.tri([hx + l.cos() * s * 0.5, hy + l.sin() * s * 0.5], [hx + r.cos() * s * 0.5, hy + r.sin() * s * 0.5], [hx + a.cos() * s * 1.35, hy + a.sin() * s * 1.35], c);
                }
                b.disc(hx, hy, s * 0.4, cc);
            }
        }
    }
}
