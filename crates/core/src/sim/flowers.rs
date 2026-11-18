//! A dense lawn of tiny flowers along the top edge of a surface. They sway when the cursor brushes them and
//! stretch up while it stays; once it leaves they settle back to their own height.

use super::Surf;
use crate::draw::Batch;
use crate::palette::{self, Rgb};
use crate::rng::Rng;

pub struct Flower {
    x: f32,
    /// Height reached by growing, before any stretching.
    h: f32,
    hmax: f32,
    rate: f32,
    /// Extra height while the cursor plays with it.
    boost: f32,
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
    span: f32,
    flowers: Vec<Flower>,
    spawn_acc: f32,
}

impl Bed {
    pub fn new(s: &Surf) -> Bed {
        Bed { x0: s.rect.x + s.inset, base: s.rect.y, span: (s.rect.w - 2.0 * s.inset).max(0.0), flowers: Vec::new(), spawn_acc: 0.0 }
    }

    /// The surface moved or resized: the lawn goes with it, losing what no longer fits.
    pub fn follow(&mut self, s: &Surf) {
        let (lo, hi) = (s.rect.x + s.inset, s.rect.right() - s.inset);
        let dx = lo - self.x0;
        if dx != 0.0 {
            for f in &mut self.flowers {
                f.x += dx;
            }
        }
        self.x0 = lo;
        self.span = (hi - lo).max(0.0);
        self.base = s.rect.y;
        self.flowers.retain(|f| f.x >= lo && f.x <= hi);
    }

    pub fn step(&mut self, dt: f32, intensity: f32, inset: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) {
        let _ = inset;
        let width = self.span;
        let cap = (width * (0.08 + 0.34 * intensity)) as usize;
        // The lawn fills in over roughly twenty seconds.
        self.spawn_acc += dt * cap as f32 / 20.0;
        while self.spawn_acc >= 1.0 {
            self.spawn_acc -= 1.0;
            if self.flowers.len() < cap {
                let x = self.x0 + rng.f() * width;
                if !self.flowers.iter().any(|f| (f.x - x).abs() < 1.8) {
                    self.flowers.push(new_flower(x, rng));
                }
            }
        }
        for f in &mut self.flowers {
            f.hidden = hidden_at(f.x);
            if intensity < 0.01 {
                f.h -= 6.0 * dt;
                continue;
            }
            if f.h < f.hmax {
                f.h = (f.h + f.rate * dt * (0.35 + 0.65 * (1.0 - f.h / f.hmax))).min(f.hmax);
            }
            let total = f.h + f.boost;
            let mut touched = false;
            let mut push = 0.0;
            if let Some(c) = cursor {
                let head_y = self.base - total;
                let dx = c[0] - f.x;
                if dx.abs() < 24.0 && c[1] > head_y - 28.0 && c[1] < self.base + 10.0 {
                    touched = true;
                    push = (-dx * 0.25).clamp(-5.0, 5.0);
                }
            }
            if touched {
                f.excite = (f.excite + dt * 5.0).min(1.0);
                // Reaches up to about twice its height while it is being played with.
                f.boost = (f.boost + 9.0 * dt).min(f.hmax * 1.1);
            } else {
                f.excite = (f.excite - dt * 2.0).max(0.0);
                f.boost = (f.boost - 7.0 * dt).max(0.0);
            }
            let target = push * f.excite * (total / 8.0).min(1.5);
            f.bend += (target - f.bend) * (1.0 - (-dt * 12.0).exp());
        }
        self.flowers.retain(|f| f.h > 0.3);
    }

    pub fn draw(&self, b: &mut Batch, pixel_size: f32) {
        for f in &self.flowers {
            if !f.hidden {
                f.draw(b, self.base, pixel_size);
            }
        }
    }
}

fn new_flower(x: f32, rng: &mut Rng) -> Flower {
    let hue = rng.f();
    let col = palette::hsv(hue, rng.range(0.45, 0.95), rng.range(0.9, 1.0));
    Flower {
        x,
        h: 0.5,
        hmax: rng.range(4.5, 12.0),
        rate: rng.range(0.8, 2.2),
        boost: 0.0,
        kind: rng.below(5) as u8,
        col,
        center: if rng.chance(0.7) { [1.0, 0.85, 0.25] } else { palette::mix(col, [1.0, 1.0, 1.0], 0.6) },
        stem: palette::mix(palette::STEM, [0.55, 0.85, 0.35], rng.f() * 0.7),
        petals: 5 + rng.below(3) as u8,
        size: rng.range(1.0, 1.9),
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
    fn draw(&self, b: &mut Batch, base: f32, pixel_size: f32) {
        let h = self.h + self.boost;
        let head = [self.x + self.bend, base - h];
        let ctrl = [self.x + self.bend * 0.1, base - h * 0.6];
        let at = |t: f32| {
            let u = 1.0 - t;
            [u * u * self.x + 2.0 * u * t * ctrl[0] + t * t * head[0], u * u * base + 2.0 * u * t * ctrl[1] + t * t * head[1]]
        };
        let sc = [self.stem[0], self.stem[1], self.stem[2], 1.0];
        // One art pixel wide in the pixel renderer, a fine hairline otherwise.
        let width = if pixel_size > 0.0 { pixel_size } else { 0.8 };
        let segs = if h < 6.0 { 2 } else { 4 };
        let mut prev = at(0.0);
        for i in 1..=segs {
            let p = at(i as f32 / segs as f32);
            b.line(prev, p, width, sc);
            prev = p;
        }
        if h > 6.0 {
            let l = at(0.45);
            let s = 1.0 + h * 0.07;
            b.ellipse(l[0] + self.leaf_side * s * 0.8, l[1] - s * 0.2, s * 0.9, s * 0.4, -self.leaf_side * 0.6, sc);
        }
        let bloom = smooth(0.3, 0.85, self.h / self.hmax.max(1.0));
        let s = self.size * (0.45 + 0.55 * bloom) * (1.0 + 0.35 * (self.boost / self.hmax.max(1.0)));
        if bloom < 0.1 {
            b.disc(head[0], head[1], s * 0.6, [0.5, 0.82, 0.38, 1.0]);
            return;
        }
        let c = [self.col[0], self.col[1], self.col[2], 1.0];
        let cc = [self.center[0], self.center[1], self.center[2], 1.0];
        let [hx, hy] = head;
        match self.kind {
            0 => {
                for k in 0..self.petals {
                    let a = k as f32 / self.petals as f32 * std::f32::consts::TAU + self.phase;
                    b.ellipse(hx + a.cos() * s * 0.8, hy + a.sin() * s * 0.8, s * 0.6, s * 0.32, a, c);
                }
                b.disc(hx, hy, s * 0.45, cc);
            }
            1 => {
                b.ellipse(hx - s * 0.4, hy, s * 0.45, s * 0.8, -0.3, c);
                b.ellipse(hx + s * 0.4, hy, s * 0.45, s * 0.8, 0.3, c);
                b.ellipse(hx, hy - s * 0.15, s * 0.5, s * 0.9, 0.0, [self.col[0] * 0.9, self.col[1] * 0.9, self.col[2] * 0.9, 1.0]);
            }
            2 => {
                b.tri([hx - s * 0.7, hy - s * 0.1], [hx + s * 0.7, hy - s * 0.1], [hx, hy + s * 0.9], c);
                b.disc(hx, hy - s * 0.1, s * 0.4, c);
            }
            3 => {
                for k in 0..5u8 {
                    let a = k as f32 / 5.0 * std::f32::consts::TAU + self.phase;
                    b.disc(hx + a.cos() * s * 0.6, hy + a.sin() * s * 0.6, s * 0.48, c);
                }
                b.disc(hx, hy, s * 0.5, cc);
            }
            _ => {
                for k in 0..5u8 {
                    let a = k as f32 / 5.0 * std::f32::consts::TAU + self.phase;
                    let (l, r) = (a - 0.4, a + 0.4);
                    b.tri([hx + l.cos() * s * 0.45, hy + l.sin() * s * 0.45], [hx + r.cos() * s * 0.45, hy + r.sin() * s * 0.45], [hx + a.cos() * s * 1.25, hy + a.sin() * s * 1.25], c);
                }
                b.disc(hx, hy, s * 0.38, cc);
            }
        }
    }
}
