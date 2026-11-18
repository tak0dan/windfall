//! A bushy lawn along the top edge of a surface: thick grass everywhere and flowers of mixed size and shape
//! standing in it. Flowers sway when the cursor brushes them, stretch up while it stays and settle back
//! afterwards; the plants being played with glow. Now and then a flower bursts into sparks and a different
//! one grows in its place.

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
    hue: f32,
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
    width: f32,
    /// 0 = back row (darker), 1 = front row.
    layer: u8,
}

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
}

pub struct Bed {
    pub x0: f32,
    pub base: f32,
    span: f32,
    flowers: Vec<Flower>,
    grass: Vec<Blade>,
    sparks: Vec<Spark>,
    spawn_acc: f32,
    burst_acc: f32,
    age: f32,
    cursor: Option<[f32; 2]>,
    glow: f32,
    glow_at: f32,
}

impl Bed {
    pub fn new(s: &Surf, rng: &mut Rng) -> Bed {
        let mut b = Bed {
            x0: s.rect.x + s.inset,
            base: s.rect.y,
            span: (s.rect.w - 2.0 * s.inset).max(0.0),
            flowers: Vec::new(),
            grass: Vec::new(),
            sparks: Vec::new(),
            spawn_acc: 0.0,
            burst_acc: 0.0,
            age: 0.0,
            cursor: None,
            glow: 0.0,
            glow_at: 0.0,
        };
        b.sow(rng);
        b
    }

    /// Bushy grass: several blades per pixel, in two rows, tall and short, leaning every which way.
    fn sow(&mut self, rng: &mut Rng) {
        let want = (self.span / 0.5) as usize;
        while self.grass.len() < want {
            let layer = (rng.f() < 0.55) as u8;
            let tall = rng.f() < 0.18;
            self.grass.push(Blade {
                hidden: false,
                x: self.x0 + rng.f() * self.span,
                h: if tall { rng.range(10.0, 17.0) } else { rng.range(5.0, 11.0) },
                tint: rng.f(),
                lean: rng.range(-2.2, 2.2),
                width: rng.range(1.2, 2.2),
                layer,
            });
        }
        self.grass.truncate(want);
    }

    /// Where every flower's head is right now.
    pub fn heads(&self) -> Vec<[f32; 2]> {
        self.flowers.iter().filter(|f| !f.hidden && f.h > 3.0).map(|f| [f.x + f.bend, self.base - f.h - f.boost]).collect()
    }

    /// The surface moved or resized. A window carries its lawn along; a bar stays where it is, so what no
    /// longer has painted ground under it is lost, and the heads of those flowers are returned.
    pub fn follow(&mut self, s: &Surf, rng: &mut Rng) -> Vec<[f32; 2]> {
        let (lo, hi) = (s.rect.x + s.inset, s.rect.right() - s.inset);
        let dx = lo - self.x0;
        if dx != 0.0 && s.kind != super::SurfKind::Layer {
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
        let mut lost = Vec::new();
        let base = self.base;
        self.flowers.retain(|f| {
            let keep = f.x >= lo && f.x <= hi;
            if !keep && !f.hidden && f.h > 3.0 {
                lost.push([f.x + f.bend, base - f.h - f.boost]);
            }
            keep
        });
        self.grass.retain(|g| g.x >= lo && g.x <= hi);
        self.sow(rng);
        lost
    }

    pub fn step(&mut self, dt: f32, intensity: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) {
        self.age += dt;
        self.cursor = cursor;
        let width = self.span;
        // Grass is the lawn; flowers are scattered through it.
        let cap = (width * (0.12 + 0.36 * intensity)) as usize;
        // It fills in over a few seconds.
        self.spawn_acc += dt * cap as f32 / 7.0;
        while self.spawn_acc >= 1.0 {
            self.spawn_acc -= 1.0;
            if self.flowers.len() < cap {
                let x = self.x0 + rng.f() * width;
                self.flowers.push(new_flower(x, rng, None));
            }
        }
        for g in &mut self.grass {
            g.hidden = hidden_at(g.x);
        }
        let mut touched_any = false;
        let mut glow_x = 0.0;
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
                glow_x = f.x;
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
            self.glow_at = if self.glow < 0.1 { glow_x } else { self.glow_at + (c[0] - self.glow_at) * (1.0 - (-dt * 10.0).exp()) };
        } else {
            self.glow = (self.glow - dt * 3.0).max(0.0);
        }
        self.bursts(dt, rng);
        for sp in &mut self.sparks {
            sp.life -= dt;
            sp.vy += 70.0 * dt;
            sp.vx *= 1.0 - 2.0 * dt;
            sp.x += sp.vx * dt;
            sp.y += sp.vy * dt;
        }
        self.sparks.retain(|sp| sp.life > 0.0);
    }

    /// Once in a while a grown flower gives way: a tiny burst of sparks, then a different one starts growing.
    fn bursts(&mut self, dt: f32, rng: &mut Rng) {
        self.burst_acc += dt * 0.7 * (self.span / 1000.0);
        if self.burst_acc < 1.0 {
            return;
        }
        self.burst_acc -= 1.0;
        if self.flowers.is_empty() {
            return;
        }
        let i = rng.below(self.flowers.len());
        let f = &self.flowers[i];
        if f.hidden || f.excite > 0.05 || f.h < f.hmax * 0.85 {
            return;
        }
        let (x, y) = (f.x + f.bend, self.base - f.h - f.boost);
        for _ in 0..rng.below(5) + 9 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let v = rng.range(14.0, 52.0);
            self.sparks.push(Spark { x, y, vx: a.cos() * v, vy: a.sin() * v - 20.0, life: rng.range(0.45, 0.9) });
        }
        let old = (f.kind, f.hue);
        let at = f.x;
        self.flowers[i] = new_flower(at, rng, Some(old));
    }

    pub fn draw(&self, b: &mut Batch, pixel_size: f32, glow: Rgb) {
        let grow = (self.age / 4.0).min(1.0);
        let px = pixel_size > 0.0;
        for layer in 0..2u8 {
            for g in self.grass.iter().filter(|g| g.layer == layer && !g.hidden) {
                let sway = match self.cursor {
                    Some(c) if (g.x - c[0]).abs() < 30.0 && c[1] > self.base - 40.0 && c[1] < self.base + 12.0 => -(g.x - c[0]) / 30.0 * 3.5,
                    _ => 0.0,
                };
                let h = g.h * (0.25 + 0.75 * grow);
                let w = if px { pixel_size.max(g.width) } else { g.width };
                let shade = if layer == 0 { 0.62 } else { 1.0 };
                let col = palette::mix([0.12, 0.42, 0.16], [0.40, 0.76, 0.26], g.tint);
                b.tri([g.x - w * 0.5, self.base + 1.0], [g.x + w * 0.5, self.base + 1.0], [g.x + g.lean + sway, self.base - h], [col[0] * shade, col[1] * shade, col[2] * shade, 1.0]);
            }
        }
        if self.glow > 0.02 {
            self.draw_envelope(b, glow);
        }
        for f in &self.flowers {
            if !f.hidden {
                f.draw(b, self.base, pixel_size, glow);
            }
        }
        for sp in &self.sparks {
            let a = (sp.life * 2.5).clamp(0.0, 1.0);
            b.disc(sp.x, sp.y, if px { pixel_size * 0.6 } else { 0.9 }, [glow[0], glow[1], glow[2], a]);
        }
    }

    /// A faint parabolic dome of light standing on the lawn where it is being played with: narrow at the
    /// top, widest at the ground (stepped when pixel art).
    fn draw_envelope(&self, b: &mut Batch, glow: Rgb) {
        const SLICES: usize = 14;
        let (height, half) = (46.0f32, 40.0f32);
        let step = height / SLICES as f32;
        for k in 0..SLICES {
            let t = (k as f32 + 0.5) / SLICES as f32; // 0 at the ground, 1 at the apex
            let hw = half * (1.0 - t).sqrt();
            let y = self.base - (k as f32 + 0.5) * step;
            let a = self.glow * 0.07 * (1.15 - t);
            b.rect(self.glow_at, y, hw, step * 0.5 + 0.3, 0.0, [glow[0], glow[1], glow[2], a]);
        }
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    d.min(1.0 - d)
}

/// `replacing`: the kind and hue of the flower that just burst, which the new one must differ from.
fn new_flower(x: f32, rng: &mut Rng, replacing: Option<(u8, f32)>) -> Flower {
    let mut hue = rng.f();
    let mut kind = rng.below(5) as u8;
    if let Some((k, h)) = replacing {
        while hue_distance(hue, h) < 0.18 {
            hue = rng.f();
        }
        while kind == k {
            kind = rng.below(5) as u8;
        }
    }
    let col = palette::hsv(hue, rng.range(0.45, 0.95), rng.range(0.9, 1.0));
    // Each one is between 60% and 100% of full size.
    let scale = rng.range(0.6, 1.0);
    Flower {
        x,
        h: 0.5,
        hmax: rng.range(9.0, 24.0) * scale,
        rate: rng.range(3.5, 8.0),
        boost: 0.0,
        kind,
        hue,
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
    fn draw(&self, b: &mut Batch, base: f32, pixel_size: f32, glow: Rgb) {
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
        // The glow comes from the plant itself: a halo along the stem and around the head.
        if self.excite > 0.03 {
            let gcol = [glow[0], glow[1], glow[2], 0.3 * self.excite];
            let mut prev = at(0.0);
            for i in 1..=segs {
                let p = at(i as f32 / segs as f32);
                b.line(prev, p, width + 2.6, gcol);
                prev = p;
            }
            b.disc(head[0], head[1], self.size * 2.3 + 1.5, [glow[0], glow[1], glow[2], 0.22 * self.excite]);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_differs_in_shape_and_colour() {
        let mut rng = Rng::new(9);
        for _ in 0..500 {
            let old = (rng.below(5) as u8, rng.f());
            let f = new_flower(10.0, &mut rng, Some(old));
            assert_ne!(f.kind, old.0);
            assert!(hue_distance(f.hue, old.1) >= 0.18);
        }
    }
}
