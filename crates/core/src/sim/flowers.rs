//! A thick lawn along the top edge of a surface: grass fills every gap and tiny flowers of mixed sizes sit
//! shoulder to shoulder in it. They sway when the cursor brushes them, stretch up while it stays and settle
//! back afterwards; a small yellow glow marks where they are being played with.

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

struct Blade {
    hidden: bool,
    x: f32,
    h: f32,
    tint: f32,
    lean: f32,
}

pub struct Bed {
    pub x0: f32,
    pub base: f32,
    span: f32,
    flowers: Vec<Flower>,
    grass: Vec<Blade>,
    spawn_acc: f32,
    age: f32,
    cursor: Option<[f32; 2]>,
    glow: f32,
    glow_at: [f32; 2],
}

impl Bed {
    pub fn new(s: &Surf, rng: &mut Rng) -> Bed {
        let mut b = Bed { x0: s.rect.x + s.inset, base: s.rect.y, span: (s.rect.w - 2.0 * s.inset).max(0.0), flowers: Vec::new(), grass: Vec::new(), spawn_acc: 0.0, age: 0.0, cursor: None, glow: 0.0, glow_at: [0.0; 2] };
        b.sow(rng);
        b
    }

    /// Grass: a blade about every pixel and a quarter.
    fn sow(&mut self, rng: &mut Rng) {
        let want = (self.span / 1.25) as usize;
        while self.grass.len() < want {
            self.grass.push(Blade { hidden: false, x: self.x0 + rng.f() * self.span, h: rng.range(4.0, 11.0), tint: rng.f(), lean: rng.range(-1.0, 1.0) });
        }
        self.grass.truncate(want);
    }

    /// The surface moved or resized: the lawn goes with it, losing what no longer fits.
    pub fn follow(&mut self, s: &Surf, rng: &mut Rng) {
        let (lo, hi) = (s.rect.x + s.inset, s.rect.right() - s.inset);
        let dx = lo - self.x0;
        if dx != 0.0 {
            for f in &mut self.flowers {
                f.x += dx;
            }
            for g in &mut self.grass {
                g.x += dx;
            }
        }
        self.x0 = lo;
        self.span = (hi - lo).max(0.0);
        self.base = s.rect.y;
        self.flowers.retain(|f| f.x >= lo && f.x <= hi);
        self.grass.retain(|g| g.x >= lo && g.x <= hi);
        self.sow(rng);
    }

    pub fn step(&mut self, dt: f32, intensity: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) {
        self.age += dt;
        self.cursor = cursor;
        let width = self.span;
        // Full lawn: heads overlap, grass shows only underneath.
        let cap = (width * (0.2 + 0.5 * intensity)) as usize;
        // It fills in over roughly twenty seconds.
        self.spawn_acc += dt * cap as f32 / 20.0;
        while self.spawn_acc >= 1.0 {
            self.spawn_acc -= 1.0;
            if self.flowers.len() < cap {
                let x = self.x0 + rng.f() * width;
                self.flowers.push(new_flower(x, rng));
            }
        }
        for g in &mut self.grass {
            g.hidden = hidden_at(g.x);
        }
        let mut touched_any = false;
        for f in &mut self.flowers {
            f.hidden = hidden_at(f.x);
            if intensity < 0.01 {
                f.h -= 8.0 * dt;
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
                if dx.abs() < 34.0 && c[1] > head_y - 40.0 && c[1] < self.base + 12.0 {
                    touched = true;
                    push = (-dx * 0.3).clamp(-8.0, 8.0);
                }
            }
            if touched {
                touched_any = true;
                f.excite = (f.excite + dt * 5.0).min(1.0);
                // Reaches up to about twice its height while it is being played with.
                f.boost = (f.boost + 18.0 * dt).min(f.hmax * 1.1);
            } else {
                f.excite = (f.excite - dt * 2.0).max(0.0);
                f.boost = (f.boost - 14.0 * dt).max(0.0);
            }
            let target = push * f.excite * (total / 14.0).min(1.5);
            f.bend += (target - f.bend) * (1.0 - (-dt * 12.0).exp());
        }
        self.flowers.retain(|f| f.h > 0.3);
        if let (true, Some(c)) = (touched_any, cursor) {
            self.glow = (self.glow + dt * 6.0).min(1.0);
            self.glow_at = [c[0], c[1].clamp(self.base - 44.0, self.base - 2.0)];
        } else {
            self.glow = (self.glow - dt * 3.0).max(0.0);
        }
    }

    pub fn draw(&self, b: &mut Batch, pixel_size: f32) {
        let grow = (self.age / 10.0).min(1.0);
        let gw = if pixel_size > 0.0 { pixel_size * 0.9 } else { 1.5 };
        for g in self.grass.iter().filter(|g| !g.hidden) {
            let sway = match self.cursor {
                Some(c) if (g.x - c[0]).abs() < 30.0 && c[1] > self.base - 40.0 && c[1] < self.base + 12.0 => -(g.x - c[0]) / 30.0 * 3.5,
                _ => 0.0,
            };
            let h = g.h * (0.3 + 0.7 * grow);
            let col = palette::mix([0.18, 0.5, 0.2], [0.42, 0.74, 0.28], g.tint);
            b.tri([g.x - gw * 0.5, self.base + 1.0], [g.x + gw * 0.5, self.base + 1.0], [g.x + g.lean * 1.2 + sway, self.base - h], [col[0], col[1], col[2], 1.0]);
        }
        for f in &self.flowers {
            if !f.hidden {
                f.draw(b, self.base, pixel_size);
            }
        }
        if self.glow > 0.02 {
            // A warm yellow halo in rings: soft when smooth, stepped when pixel art.
            let [gx, gy] = self.glow_at;
            for (k, r) in [38.0f32, 30.0, 23.0, 16.0, 10.0].iter().enumerate() {
                b.disc(gx, gy, *r, [1.0, 0.9, 0.3, self.glow * (0.05 + 0.012 * k as f32)]);
            }
        }
    }
}

fn new_flower(x: f32, rng: &mut Rng) -> Flower {
    let hue = rng.f();
    let col = palette::hsv(hue, rng.range(0.45, 0.95), rng.range(0.9, 1.0));
    // Each one is between 60% and 100% of full size.
    let scale = rng.range(0.6, 1.0);
    Flower {
        x,
        h: 0.5,
        hmax: rng.range(9.0, 24.0) * scale,
        rate: rng.range(1.6, 4.4),
        boost: 0.0,
        kind: rng.below(5) as u8,
        col,
        center: if rng.chance(0.7) { [1.0, 0.85, 0.25] } else { palette::mix(col, [1.0, 1.0, 1.0], 0.6) },
        stem: palette::mix(palette::STEM, [0.55, 0.85, 0.35], rng.f() * 0.7),
        petals: 5 + rng.below(3) as u8,
        size: rng.range(2.0, 3.8) * scale,
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
        // One art pixel wide in the pixel renderer, a fine line otherwise.
        let width = if pixel_size > 0.0 { pixel_size } else { 1.3 };
        let segs = if h < 10.0 { 2 } else { 4 };
        let mut prev = at(0.0);
        for i in 1..=segs {
            let p = at(i as f32 / segs as f32);
            b.line(prev, p, width, sc);
            prev = p;
        }
        if h > 10.0 {
            let l = at(0.45);
            let s = 1.6 + h * 0.07;
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
