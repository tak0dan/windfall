//! A bushy lawn along the top edge of a surface: thick grass everywhere and flowers of mixed size and shape
//! standing in it.
//!
//! Every flower and every blade checks, on its own, whether there is still painted ground under it. If not it
//! waits a moment (a bar that merely flickered keeps its lawn) and then goes, one after another, starting
//! with the one furthest from the ground that stays. Flowers puff away; grass just goes.
//!
//! On a window the whole top edge is covered. In a rounded corner, with the curve running from 0% (where it
//! meets the side) to 100% (where it meets the flat top): flowers grow from 75% to 100%, grass from 30% to
//! 100%, rooted on the curve itself.

use super::{Surf, SurfKind};
use crate::draw::Batch;
use crate::palette::{self, Rgb};
use crate::rng::Rng;

/// How long ground may be missing before what stands on it goes.
const HOLD: f32 = 1.2;
/// How fast the wave of disappearance travels toward the ground that stays, in pixels per second.
const WAVE: f32 = 420.0;
/// Where along a corner's curve things may grow, as a share of the curve (0 = side, 1 = flat top).
const FLOWERS_FROM: f32 = 0.75;
const GRASS_FROM: f32 = 0.30;
const BUCKET: f32 = 4.0;
const BLADES_PER_BUCKET: usize = 8;

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
    /// When it goes, if its ground is gone.
    doom: Option<f32>,
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
    doom: Option<f32>,
}

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
}

pub struct Bed {
    left: f32,
    right: f32,
    radius: f32,
    pub base: f32,
    /// Where flowers / grass have ground under them.
    f_range: (f32, f32),
    g_range: (f32, f32),
    flowers: Vec<Flower>,
    grass: Vec<Blade>,
    sparks: Vec<Spark>,
    spawn_acc: f32,
    burst_acc: f32,
    fill_at: f32,
    age: f32,
    cursor: Option<[f32; 2]>,
    /// The surface is gone: everything on it leaves quickly.
    orphan: bool,
    intensity: f32,
}

/// Where along a surface's top things can stand: (flowers, grass) ranges, and the corner radius.
fn ranges(s: &Surf) -> ((f32, f32), (f32, f32), f32) {
    let (l, r) = (s.rect.x, s.rect.right());
    if s.kind == SurfKind::Window && s.inset > 0.5 {
        let rad = s.inset;
        // A point u (0..1) across the corner sits at curve share t = acos(1-u)/(pi/2).
        let u_of = |t: f32| 1.0 - (t * std::f32::consts::FRAC_PI_2).cos();
        ((l + rad * u_of(FLOWERS_FROM), r - rad * u_of(FLOWERS_FROM)), (l + rad * u_of(GRASS_FROM), r - rad * u_of(GRASS_FROM)), rad)
    } else {
        ((l, r), (l, r), 0.0)
    }
}

impl Bed {
    pub fn new(s: &Surf, rng: &mut Rng) -> Bed {
        let (f_range, g_range, radius) = ranges(s);
        let mut b = Bed {
            left: s.rect.x,
            right: s.rect.right(),
            radius,
            base: s.rect.y,
            f_range,
            g_range,
            flowers: Vec::new(),
            grass: Vec::new(),
            sparks: Vec::new(),
            spawn_acc: 0.0,
            burst_acc: 0.0,
            fill_at: 0.0,
            age: 0.0,
            cursor: None,
            orphan: false,
            intensity: 0.5,
        };
        b.fill_grass(rng);
        b
    }

    /// How far below the flat top the ground is at `x` (the corner's curve).
    fn drop(&self, x: f32) -> f32 {
        let r = self.radius;
        if r <= 0.0 {
            return 0.0;
        }
        let u = if x < self.left + r {
            (x - self.left) / r
        } else if x > self.right - r {
            (self.right - x) / r
        } else {
            return 0.0;
        };
        let k = 1.0 - u.clamp(0.0, 1.0);
        r * (1.0 - (1.0 - k * k).max(0.0).sqrt())
    }

    /// How many flowers the ground can hold.
    fn cap(&self) -> usize {
        ((self.f_range.1 - self.f_range.0).max(0.0) * (0.12 + 0.36 * self.intensity)) as usize
    }

    /// The surface moved or changed size.
    ///
    /// A window's lawn is carried and stretched with it: the corners stay attached to their corners and the
    /// middle is stretched (or squeezed) between them, then the gaps are filled with grown flowers on the
    /// spot, so no bare stretch is ever seen. A bar keeps its lawn where it is, and what is left without
    /// ground is dealt with by `step`.
    pub fn update(&mut self, s: &Surf, rng: &mut Rng) {
        let (f_range, g_range, radius) = ranges(s);
        let carried = s.kind != SurfKind::Layer;
        if carried {
            self.remap(s.rect.x, s.rect.right());
        }
        self.left = s.rect.x;
        self.right = s.rect.right();
        self.base = s.rect.y;
        self.radius = radius;
        self.f_range = f_range;
        self.g_range = g_range;
        if carried {
            self.thin(rng);
            self.fill_grass(rng);
            self.fill_flowers_now(rng);
        } else {
            self.fill_grass(rng);
        }
    }

    /// Moves everything to where it belongs on the new extent, corners anchored.
    fn remap(&mut self, new_left: f32, new_right: f32) {
        let (l0, r0, l1, r1) = (self.left, self.right, new_left, new_right);
        if (l0 - l1).abs() < 1e-3 && (r0 - r1).abs() < 1e-3 {
            return;
        }
        let c0 = self.radius.max(6.0).min((r0 - l0) / 3.0);
        let c1 = c0.min((r1 - l1) / 3.0);
        let map = |x: f32| -> f32 {
            if x <= l0 + c0 {
                l1 + (x - l0).min(c1)
            } else if x >= r0 - c0 {
                r1 - (r0 - x).min(c1)
            } else {
                let t = (x - (l0 + c0)) / ((r0 - c0) - (l0 + c0)).max(1e-3);
                (l1 + c1) + t * ((r1 - c1) - (l1 + c1))
            }
        };
        for f in &mut self.flowers {
            f.x = map(f.x);
        }
        for g in &mut self.grass {
            g.x = map(g.x);
        }
    }

    /// A squeezed lawn is crowded: drop the surplus quietly (flowers) and the excess blades.
    fn thin(&mut self, rng: &mut Rng) {
        let cap = self.cap() + self.cap() / 10;
        while self.flowers.len() > cap {
            let i = rng.below(self.flowers.len());
            self.flowers.swap_remove(i);
        }
        let (lo, hi) = self.g_range;
        let n = ((hi - lo) / BUCKET).ceil().max(1.0) as usize;
        let mut counts = vec![0usize; n];
        self.grass.retain(|g| {
            let b = (((g.x - lo) / BUCKET).floor().max(0.0) as usize).min(n - 1);
            counts[b] += 1;
            counts[b] <= BLADES_PER_BUCKET + 4
        });
    }

    /// Fills every gap at once with flowers that are already full grown.
    fn fill_flowers_now(&mut self, rng: &mut Rng) {
        let cap = self.cap();
        let have = self.flowers.iter().filter(|f| f.doom.is_none()).count();
        if self.f_range.1 - self.f_range.0 < 1.0 {
            return;
        }
        for _ in have..cap {
            let x = self.pick_spot(rng);
            let mut f = new_flower(x, rng, None);
            f.h = f.hmax;
            self.flowers.push(f);
        }
    }

    /// The surface is gone: what stood on it leaves, right to left.
    pub fn abandon(&mut self) {
        self.orphan = true;
        self.f_range = (self.left, self.left);
        self.g_range = (self.left, self.left);
    }

    pub fn is_empty(&self) -> bool {
        self.flowers.is_empty() && self.grass.is_empty() && self.sparks.is_empty()
    }

    /// Tops up the grass wherever it is thin, four pixels at a time: bushy everywhere, and with taller tufts
    /// at the ends of the ground, where it spills over the edge of the flowers.
    fn fill_grass(&mut self, rng: &mut Rng) {
        let (lo, hi) = self.g_range;
        if hi - lo < 1.0 {
            return;
        }
        let n = ((hi - lo) / BUCKET).ceil() as usize;
        let mut counts = vec![0usize; n];
        for g in self.grass.iter().filter(|g| g.doom.is_none()) {
            let b = ((g.x - lo) / BUCKET).floor();
            if b >= 0.0 && (b as usize) < n {
                counts[b as usize] += 1;
            }
        }
        // The flowers' own range is narrower than the grass's at a corner; at both ends there is a tuft.
        let (flo, fhi) = self.f_range;
        for (b, &have) in counts.iter().enumerate() {
            for _ in have..BLADES_PER_BUCKET {
                let x = lo + (b as f32 + rng.f()) * BUCKET;
                let end = (x - flo).abs().min((x - fhi).abs()).min((x - lo).abs()).min((x - hi).abs());
                let tuft = if (x < flo || x > fhi) || end < 14.0 { 1.0 - (end / 14.0).min(1.0) * 0.5 } else { 0.0 };
                let tall = rng.f() < 0.18;
                let h = if tall { rng.range(10.0, 17.0) } else { rng.range(5.0, 11.0) };
                let outward = if x < (lo + hi) * 0.5 { -1.0 } else { 1.0 };
                self.grass.push(Blade {
                    hidden: false,
                    x,
                    h: h * (1.0 + 0.9 * tuft),
                    tint: rng.f(),
                    lean: rng.range(-2.2, 2.2) + outward * tuft * 2.0,
                    width: rng.range(1.2, 2.2),
                    layer: (rng.f() < 0.55) as u8,
                    doom: None,
                });
            }
        }
    }

    /// Chooses where a new flower goes: the emptiest of a few random spots, so the lawn fills evenly.
    fn pick_spot(&self, rng: &mut Rng) -> f32 {
        let (lo, hi) = self.f_range;
        let mut best = (lo + rng.f() * (hi - lo), -1.0f32);
        for _ in 0..6 {
            let x = lo + rng.f() * (hi - lo);
            let gap = self.flowers.iter().filter(|f| f.doom.is_none()).map(|f| (f.x - x).abs()).fold(f32::MAX, f32::min);
            if gap > best.1 {
                best = (x, gap);
            }
        }
        best.0
    }

    /// Advances the lawn. Returns the heads of flowers that just puffed away.
    pub fn step(&mut self, dt: f32, intensity: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) -> Vec<[f32; 2]> {
        self.age += dt;
        self.cursor = cursor;
        self.intensity = intensity;
        let mut puffs = Vec::new();
        let (hold, wave) = if self.orphan { (0.1, WAVE * 1.2) } else { (HOLD, WAVE) };

        // Is there still ground under each one? Find out, and schedule the ones that lost it, farthest first.
        let dist = |x: f32, (lo, hi): (f32, f32)| if x < lo { lo - x } else if x > hi { x - hi } else { 0.0 };
        let fr = self.f_range;
        let maxd = self.flowers.iter().map(|f| dist(f.x, fr)).fold(0.0f32, f32::max);
        for f in &mut self.flowers {
            let d = dist(f.x, fr);
            match (d > 0.0, f.doom) {
                (true, None) => f.doom = Some(self.age + hold + (maxd - d) / wave),
                (false, Some(_)) => f.doom = None,
                _ => {}
            }
        }
        let gr = self.g_range;
        let maxg = self.grass.iter().map(|g| dist(g.x, gr)).fold(0.0f32, f32::max);
        for g in &mut self.grass {
            let d = dist(g.x, gr);
            match (d > 0.0, g.doom) {
                (true, None) => g.doom = Some(self.age + hold + (maxg - d) / wave),
                (false, Some(_)) => g.doom = None,
                _ => {}
            }
        }
        let now = self.age;
        let base = self.base;
        let drops: Vec<f32> = self.flowers.iter().map(|f| self.drop(f.x)).collect();
        let mut keep = drops.iter();
        self.flowers.retain(|f| {
            let off = *keep.next().unwrap();
            if f.doom.is_some_and(|t| now >= t) {
                if !f.hidden && f.h > 2.0 {
                    puffs.push([f.x + f.bend, base + off - f.h - f.boost]);
                }
                false
            } else {
                true
            }
        });
        self.grass.retain(|g| !g.doom.is_some_and(|t| now >= t));

        if !self.orphan {
            // Fill in over a few seconds; grass is topped up where it has gone thin.
            let width = (fr.1 - fr.0).max(0.0);
            let cap = self.cap();
            self.spawn_acc += dt * cap as f32 / 7.0;
            while self.spawn_acc >= 1.0 {
                self.spawn_acc -= 1.0;
                if self.flowers.iter().filter(|f| f.doom.is_none()).count() < cap && width > 1.0 {
                    let x = self.pick_spot(rng);
                    self.flowers.push(new_flower(x, rng, None));
                }
            }
            if self.age - self.fill_at > 1.5 {
                self.fill_at = self.age;
                self.fill_grass(rng);
            }
        }
        for g in &mut self.grass {
            g.hidden = hidden_at(g.x);
        }
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
        if !self.orphan {
            self.bursts(dt, rng);
        }
        for sp in &mut self.sparks {
            sp.life -= dt;
            sp.vy += 70.0 * dt;
            sp.vx *= 1.0 - 2.0 * dt;
            sp.x += sp.vx * dt;
            sp.y += sp.vy * dt;
        }
        self.sparks.retain(|sp| sp.life > 0.0);
        puffs
    }

    /// Once in a while a grown flower gives way: a tiny burst of sparks, then a different one starts growing.
    fn bursts(&mut self, dt: f32, rng: &mut Rng) {
        self.burst_acc += dt * 0.7 * ((self.right - self.left) / 1000.0);
        if self.burst_acc < 1.0 {
            return;
        }
        self.burst_acc -= 1.0;
        if self.flowers.is_empty() {
            return;
        }
        let i = rng.below(self.flowers.len());
        let f = &self.flowers[i];
        if f.hidden || f.doom.is_some() || f.excite > 0.05 || f.h < f.hmax * 0.85 {
            return;
        }
        let (x, y) = (f.x + f.bend, self.base + self.drop(f.x) - f.h - f.boost);
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
                let root = self.base + self.drop(g.x);
                let h = g.h * (0.25 + 0.75 * grow);
                let w = if px { pixel_size.max(g.width) } else { g.width };
                let shade = if layer == 0 { 0.62 } else { 1.0 };
                let col = palette::mix([0.12, 0.42, 0.16], [0.40, 0.76, 0.26], g.tint);
                b.tri([g.x - w * 0.5, root + 1.0], [g.x + w * 0.5, root + 1.0], [g.x + g.lean + sway, root - h], [col[0] * shade, col[1] * shade, col[2] * shade, 1.0]);
            }
        }
        for f in &self.flowers {
            if !f.hidden {
                f.draw(b, self.base + self.drop(f.x), pixel_size, glow);
            }
        }
        for sp in &self.sparks {
            let a = (sp.life * 2.5).clamp(0.0, 1.0);
            b.disc(sp.x, sp.y, if px { pixel_size * 0.6 } else { 0.9 }, [glow[0], glow[1], glow[2], a]);
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
        doom: None,
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
        // Light given off by the plant being played with: a parabola of light standing on it (widest at the
        // ground, narrowing to the head), a halo along the stem and one around the head.
        if self.excite > 0.03 {
            const SLICES: usize = 6;
            let half = self.size * 2.4 + 3.0;
            let top = h + self.size * 1.8;
            let step = top / SLICES as f32;
            for k in 0..SLICES {
                let t = (k as f32 + 0.5) / SLICES as f32;
                let hw = half * (1.0 - t).sqrt();
                let y = base - (k as f32 + 0.5) * step;
                b.rect(self.x + self.bend * t, y, hw, step * 0.5 + 0.3, 0.0, [glow[0], glow[1], glow[2], 0.075 * self.excite * (1.2 - t)]);
            }
            let gcol = [glow[0], glow[1], glow[2], 0.28 * self.excite];
            let mut prev = at(0.0);
            for i in 1..=segs {
                let p = at(i as f32 / segs as f32);
                b.line(prev, p, width + 2.4, gcol);
                prev = p;
            }
            b.disc(head[0], head[1], self.size * 2.2 + 1.5, [glow[0], glow[1], glow[2], 0.2 * self.excite]);
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
    use crate::geom::Rect;

    fn surf(x: f32, w: f32, kind: SurfKind, inset: f32) -> Surf {
        Surf { id: 1, rect: Rect::new(x, 100.0, w, 50.0), z: 0, kind, inset, shown: true }
    }

    fn grown(s: &Surf) -> (Bed, Rng) {
        let mut rng = Rng::new(3);
        let mut bed = Bed::new(s, &mut rng);
        for _ in 0..(20 * 60) {
            bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false);
        }
        (bed, rng)
    }

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

    #[test]
    fn window_top_is_covered_to_the_corners_with_grass_going_further() {
        let s = surf(100.0, 400.0, SurfKind::Window, 10.0);
        let (bed, _) = grown(&s);
        let min_f = bed.flowers.iter().map(|f| f.x).fold(f32::MAX, f32::min);
        let max_f = bed.flowers.iter().map(|f| f.x).fold(f32::MIN, f32::max);
        // Flowers reach about 75% of the way round the corner: 6.2px in on a 10px radius.
        assert!(min_f >= 100.0 + 6.1 && min_f < 112.0, "{min_f}");
        assert!(max_f <= 500.0 - 6.1 && max_f > 488.0, "{max_f}");
        let min_g = bed.grass.iter().map(|g| g.x).fold(f32::MAX, f32::min);
        assert!(min_g < min_f && min_g >= 100.0 + 1.0, "grass {min_g} flowers {min_f}");
    }

    #[test]
    fn a_two_pixel_change_costs_nothing() {
        let s = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        let before = bed.flowers.len();
        // The bar's right end moves in by two pixels and, a second later, back out.
        let narrower = surf(100.0, 398.0, SurfKind::Layer, 2.0);
        bed.update(&narrower, &mut rng);
        let mut lost = 0;
        for _ in 0..30 {
            lost += bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false).len();
        }
        bed.update(&s, &mut rng);
        for _ in 0..(3 * 60) {
            lost += bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false).len();
        }
        assert_eq!(lost, 0, "a brief flicker must not cost any flower");
        assert!(bed.flowers.len() >= before - 1);
    }

    #[test]
    fn lost_ground_empties_from_the_far_end_in_order_after_a_delay() {
        let s = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        // The right half of the bar stops being painted.
        let half = surf(100.0, 200.0, SurfKind::Layer, 2.0);
        bed.update(&half, &mut rng);
        let mut order = Vec::new();
        let mut first_at = None;
        for i in 0..(4 * 60) {
            for p in bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false) {
                first_at.get_or_insert(i);
                order.push(p[0]);
            }
        }
        assert!(first_at.unwrap() as f32 / 60.0 >= HOLD - 0.05, "went too early: {first_at:?}");
        assert!(order.len() > 20);
        // Mostly decreasing x: the furthest goes first, walking toward the ground that stays.
        let inversions = order.windows(2).filter(|w| w[1] > w[0] + 10.0).count();
        assert!(inversions * 8 < order.len(), "not an orderly wave: {inversions}/{}", order.len());
        assert!(bed.flowers.iter().all(|f| f.x <= 300.5));
    }

    #[test]
    fn stretching_a_window_keeps_corners_and_fills_gaps_at_once() {
        let s = surf(100.0, 300.0, SurfKind::Window, 10.0);
        let (mut bed, mut rng) = grown(&s);
        let near_right = |b: &Bed, right: f32| b.flowers.iter().map(|f| right - f.x).fold(f32::MAX, f32::min);
        let (nl, nr) = (bed.flowers.iter().map(|f| f.x - 100.0).fold(f32::MAX, f32::min), near_right(&bed, 400.0));
        // Pull the right edge out by 500px: no time passes, yet the new stretch must already be planted.
        let wide = surf(100.0, 800.0, SurfKind::Window, 10.0);
        bed.update(&wide, &mut rng);
        assert!((bed.flowers.iter().map(|f| f.x - 100.0).fold(f32::MAX, f32::min) - nl).abs() < 0.01, "left corner moved");
        assert!((near_right(&bed, 900.0) - nr).abs() < 0.01, "right corner did not follow its edge");
        // Evenly covered straight away: every 100px window of the new width holds a decent share.
        let per = |lo: f32, hi: f32| bed.flowers.iter().filter(|f| f.x >= lo && f.x < hi).count();
        let counts: Vec<usize> = (0..8).map(|k| per(100.0 + k as f32 * 100.0, 200.0 + k as f32 * 100.0)).collect();
        let min = *counts.iter().min().unwrap();
        let max = *counts.iter().max().unwrap();
        assert!(min * 2 >= max, "gaps right after the stretch: {counts:?}");
        // And shrinking back keeps it tidy too.
        bed.update(&s, &mut rng);
        assert!(bed.flowers.iter().all(|f| f.x >= 100.0 && f.x <= 400.0));
        assert!(bed.flowers.len() <= bed.cap() + bed.cap() / 10 + 1);
    }

    #[test]
    fn new_ground_gets_a_lawn_too() {
        let s = surf(100.0, 200.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        let wider = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        bed.update(&wider, &mut rng);
        for _ in 0..(15 * 60) {
            bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false);
        }
        let in_new = bed.flowers.iter().filter(|f| f.x > 300.0).count();
        let in_old = bed.flowers.iter().filter(|f| f.x <= 300.0).count();
        assert!(in_new * 10 > in_old * 7, "the new part is bare: {in_new} vs {in_old}");
        let grass_new = bed.grass.iter().filter(|g| g.x > 300.0).count();
        assert!(grass_new > 300, "{grass_new}");
    }
}
