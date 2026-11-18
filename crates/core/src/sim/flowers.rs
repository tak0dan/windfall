//! A bushy lawn along the top edge of a surface: thick grass everywhere and flowers of mixed size and shape
//! standing in it.
//!
//! A lawn is tied to its surface: a window, or one painted part of a bar. When the surface moves or changes
//! size the lawn goes with it, corners anchored and the middle stretched or squeezed (flowers that get
//! squeezed together simply pile up on one another), with any gaps filled on the spot, so nothing vanishes
//! and regrows. Only when the surface is really gone do its flowers leave: in a wave that runs from the far
//! end toward the other, each puffing away; the grass just goes.
//!
//! On a window the whole top edge is covered. In a rounded corner, with the curve running from 0% (where it
//! meets the side) to 100% (where it meets the flat top): flowers grow from 75% to 100%, grass from 30% to
//! 100%, rooted on the curve itself.

use super::{Surf, SurfKind};
use crate::draw::Batch;
use crate::palette::{self, Rgb};
use crate::rng::Rng;

/// How far past the edge of the ground something may stand before it counts as having lost it.
const TOLERANCE: f32 = 3.0;
/// How fast the wave of disappearance travels toward the ground that stays, in pixels per second.
const WAVE: f32 = 360.0;
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
    /// Where its stem starts: the top of the ground it stands on.
    root: f32,
    /// Its ground is gone and the wave is on its way.
    lost: bool,
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
    root: f32,
    lost: bool,
}

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
}

/// One stretch of ground: a window's top edge, or one painted part of a bar.
#[derive(Clone)]
struct Span {
    left: f32,
    right: f32,
    radius: f32,
    base: f32,
    /// Where flowers / grass may stand.
    f: (f32, f32),
    g: (f32, f32),
}

impl Span {
    fn of(s: &Surf) -> Span {
        let (l, r) = (s.rect.x, s.rect.right());
        if s.kind == SurfKind::Window && s.inset > 0.5 {
            let rad = s.inset;
            // A point u (0..1) across the corner sits at curve share t = acos(1-u)/(pi/2).
            let u_of = |t: f32| 1.0 - (t * std::f32::consts::FRAC_PI_2).cos();
            let (uf, ug) = (u_of(FLOWERS_FROM), u_of(GRASS_FROM));
            Span { left: l, right: r, radius: rad, base: s.rect.y, f: (l + rad * uf, r - rad * uf), g: (l + rad * ug, r - rad * ug) }
        } else {
            Span { left: l, right: r, radius: 0.0, base: s.rect.y, f: (l, r), g: (l, r) }
        }
    }

    /// How far below the flat top the ground is at `x` (a rounded corner's curve).
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
}

pub struct Bed {
    kind: SurfKind,
    spans: Vec<Span>,
    /// Where an orphaned lawn's goodbye wave ends.
    edge: f32,
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
    f_wave: Option<f32>,
    g_wave: Option<f32>,
}

impl Bed {
    pub fn new(s: &Surf, rng: &mut Rng) -> Bed {
        Bed::empty(s.kind, vec![Span::of(s)], rng)
    }

    fn empty(kind: SurfKind, spans: Vec<Span>, rng: &mut Rng) -> Bed {
        let edge = spans.iter().map(|s| s.left).fold(f32::MAX, f32::min);
        let mut b = Bed {
            kind,
            spans,
            edge,
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
            f_wave: None,
            g_wave: None,
        };
        b.fill_grass(rng);
        b
    }

    /// Where `x` stands relative to the ground: how far outside every stretch of it (0 when on it, or within
    /// a few pixels of it), and the top of the nearest stretch at that point.
    fn locate(&self, x: f32, flowers: bool) -> (f32, Option<f32>) {
        let mut best: Option<(f32, f32)> = None;
        for s in &self.spans {
            let (lo, hi) = if flowers { s.f } else { s.g };
            let d = if x < lo {
                lo - x
            } else if x > hi {
                x - hi
            } else {
                0.0
            };
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, s.base + s.drop(x.clamp(s.left, s.right))));
            }
        }
        match best {
            Some((d, root)) if d <= TOLERANCE => (0.0, Some(root)),
            Some((d, _)) => (d, None),
            // No ground at all (the surface is gone): the wave runs from the right end to the left.
            None => ((x - self.edge).max(TOLERANCE + 0.1), None),
        }
    }

    /// How many flowers the ground can hold.
    fn cap(&self) -> usize {
        let width: f32 = self.spans.iter().map(|s| (s.f.1 - s.f.0).max(0.0)).sum();
        (width * (0.12 + 0.36 * self.intensity)) as usize
    }

    /// The surface moved or changed size: the lawn goes with it. The ends stay attached to their ends and
    /// the middle is stretched (or squeezed, flowers piling up on one another) between them; then any gap is
    /// filled with grown flowers on the spot, so no bare stretch is ever seen.
    pub fn update(&mut self, s: &Surf, rng: &mut Rng) {
        let old = self.spans.first().cloned();
        self.kind = s.kind;
        self.spans = vec![Span::of(s)];
        self.edge = s.rect.x;
        if let Some(o) = old {
            self.remap((o.left, o.right, o.radius), (s.rect.x, s.rect.right()));
        }
        self.thin();
        self.fill_grass(rng);
        self.fill_flowers_now(rng);
    }

    /// Moves everything to where it belongs on the new extent, corners anchored.
    fn remap(&mut self, old: (f32, f32, f32), new: (f32, f32)) {
        let ((l0, r0, radius), (l1, r1)) = (old, new);
        if (l0 - l1).abs() < 1e-3 && (r0 - r1).abs() < 1e-3 {
            return;
        }
        let c0 = radius.max(6.0).min((r0 - l0) / 3.0);
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

    /// A squeezed lawn is crowded: keep the blade count bounded.
    fn thin(&mut self) {
        // Squeezed flowers simply stand on one another; only the grass is kept to a sensible count.
        for sp in self.spans.clone() {
            let (lo, hi) = sp.g;
            let n = ((hi - lo) / BUCKET).ceil().max(1.0) as usize;
            let mut counts = vec![0usize; n];
            self.grass.retain(|g| {
                if g.x < lo || g.x > hi {
                    return true;
                }
                let b = (((g.x - lo) / BUCKET).floor().max(0.0) as usize).min(n - 1);
                counts[b] += 1;
                counts[b] <= BLADES_PER_BUCKET + 4
            });
        }
    }

    /// Fills every gap at once with flowers that are already full grown.
    fn fill_flowers_now(&mut self, rng: &mut Rng) {
        let cap = self.cap();
        let have = self.flowers.iter().filter(|f| !f.lost).count();
        for _ in have..cap {
            let Some((x, root)) = self.pick_spot(rng) else { return };
            let mut f = new_flower(x, root, rng, None);
            f.h = f.hmax;
            self.flowers.push(f);
        }
    }

    /// A whole window is gone (not just a part of a bar): its lawn is gone with it, at once. Returns the heads
    /// of a sample of the flowers, for a few quiet puffs.
    pub fn vanish(&mut self) -> Vec<[f32; 2]> {
        let heads = self.flowers.iter().enumerate().filter(|(i, f)| i % 4 == 0 && !f.hidden && f.h > 2.0).map(|(_, f)| [f.x + f.bend, f.root - f.h - f.boost]).collect();
        self.flowers.clear();
        self.grass.clear();
        self.sparks.clear();
        self.orphan = true;
        self.spans.clear();
        heads
    }

    /// Is this the lawn of a window (or the screen's edge) rather than a part of a bar?
    pub fn is_bar_part(&self) -> bool {
        self.kind == SurfKind::Layer
    }

    /// A part of a bar is gone: what stood on it leaves, right to left.
    pub fn abandon(&mut self) {
        if let Some(l) = self.spans.iter().map(|s| s.left).reduce(f32::min) {
            self.edge = l;
        }
        self.orphan = true;
        self.spans.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.flowers.is_empty() && self.grass.is_empty() && self.sparks.is_empty()
    }

    /// Tops up the grass wherever it is thin, four pixels at a time: bushy everywhere, and with taller tufts
    /// at the ends of the ground, where it spills over the edge of the flowers.
    fn fill_grass(&mut self, rng: &mut Rng) {
        for sp in self.spans.clone() {
            let (lo, hi) = sp.g;
            if hi - lo < 1.0 {
                continue;
            }
            let n = ((hi - lo) / BUCKET).ceil() as usize;
            let mut counts = vec![0usize; n];
            for g in self.grass.iter().filter(|g| !g.lost && g.x >= lo && g.x <= hi) {
                let b = (((g.x - lo) / BUCKET).floor().max(0.0) as usize).min(n - 1);
                counts[b] += 1;
            }
            let (flo, fhi) = sp.f;
            for (b, &have) in counts.iter().enumerate() {
                for _ in have..BLADES_PER_BUCKET {
                    let x = lo + (b as f32 + rng.f()) * BUCKET;
                    let end = (x - flo).abs().min((x - fhi).abs()).min((x - lo).abs()).min((x - hi).abs());
                    let tuft = if x < flo || x > fhi || end < 14.0 { 1.0 - (end / 14.0).min(1.0) * 0.5 } else { 0.0 };
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
                        root: sp.base + sp.drop(x),
                        lost: false,
                    });
                }
            }
        }
    }

    /// Chooses where a new flower goes: the emptiest of a few random spots, so the lawn fills evenly.
    /// Returns the spot and the top of the ground there.
    fn pick_spot(&self, rng: &mut Rng) -> Option<(f32, f32)> {
        let total: f32 = self.spans.iter().map(|s| (s.f.1 - s.f.0).max(0.0)).sum();
        if total < 1.0 {
            return None;
        }
        let mut best: Option<(f32, f32, f32)> = None;
        for _ in 0..6 {
            let mut r = rng.f() * total;
            let mut chosen = &self.spans[0];
            for s in &self.spans {
                let w = (s.f.1 - s.f.0).max(0.0);
                if r <= w {
                    chosen = s;
                    break;
                }
                r -= w;
            }
            let x = chosen.f.0 + rng.f() * (chosen.f.1 - chosen.f.0);
            let gap = self.flowers.iter().filter(|f| !f.lost).map(|f| (f.x - x).abs()).fold(f32::MAX, f32::min);
            if best.map_or(true, |(_, _, g)| gap > g) {
                best = Some((x, chosen.base + chosen.drop(x), gap));
            }
        }
        best.map(|(x, y, _)| (x, y))
    }

    /// Advances the lawn. Returns the heads of flowers that just puffed away.
    pub fn step(&mut self, dt: f32, intensity: f32, cursor: Option<[f32; 2]>, rng: &mut Rng, hidden_at: &dyn Fn(f32) -> bool) -> Vec<[f32; 2]> {
        self.age += dt;
        self.cursor = cursor;
        self.intensity = intensity;
        let mut puffs = Vec::new();
        let speed = if self.orphan { WAVE * 1.3 } else { WAVE };

        // Two things run together. Every flower keeps looking for ground under itself; and those that have
        // none are taken by a wave that sweeps from the furthest one toward the ground that stays. A flower
        // that finds ground again is simply no longer in the wave.
        let dists: Vec<(f32, Option<f32>)> = self.flowers.iter().map(|f| self.locate(f.x, true)).collect();
        let maxd = dists.iter().filter(|(d, _)| *d > 0.0).map(|(d, _)| *d).fold(0.0f32, f32::max);
        let front = match self.f_wave {
            _ if maxd <= 0.0 => None,
            Some(w) => Some((w.max(maxd) - speed * dt).max(0.0)),
            None => Some((maxd - speed * dt).max(0.0)),
        };
        self.f_wave = front;
        let mut i = 0;
        let mut dead = Vec::new();
        self.flowers.retain_mut(|f| {
            let (d, root) = dists[i];
            i += 1;
            match root {
                Some(r) => {
                    f.root = r;
                    f.lost = false;
                    true
                }
                None => {
                    f.lost = true;
                    if front.is_some_and(|w| d >= w) {
                        if !f.hidden && f.h > 2.0 {
                            dead.push([f.x + f.bend, f.root - f.h - f.boost]);
                        }
                        false
                    } else {
                        true
                    }
                }
            }
        });
        puffs.extend(dead);

        // The same for the grass, only faster and without a fuss.
        let gd: Vec<(f32, Option<f32>)> = self.grass.iter().map(|g| self.locate(g.x, false)).collect();
        let gmax = gd.iter().filter(|(d, _)| *d > 0.0).map(|(d, _)| *d).fold(0.0f32, f32::max);
        let gfront = match self.g_wave {
            _ if gmax <= 0.0 => None,
            Some(w) => Some((w.max(gmax) - speed * 2.5 * dt).max(0.0)),
            None => Some((gmax - speed * 2.5 * dt).max(0.0)),
        };
        self.g_wave = gfront;
        let mut k = 0;
        self.grass.retain_mut(|g| {
            let (d, root) = gd[k];
            k += 1;
            match root {
                Some(r) => {
                    g.root = r;
                    g.lost = false;
                    true
                }
                None => {
                    g.lost = true;
                    !gfront.is_some_and(|w| d >= w)
                }
            }
        });

        if !self.orphan {
            // Gaps are planted anew: steadily, and faster the bigger the gap.
            let cap = self.cap();
            let have = self.flowers.iter().filter(|f| !f.lost).count();
            let deficit = cap.saturating_sub(have) as f32;
            self.spawn_acc += dt * (cap as f32 / 7.0).max(deficit * 1.2);
            while self.spawn_acc >= 1.0 {
                self.spawn_acc -= 1.0;
                if self.flowers.iter().filter(|f| !f.lost).count() < cap {
                    if let Some((x, root)) = self.pick_spot(rng) {
                        self.flowers.push(new_flower(x, root, rng, None));
                    }
                }
            }
            if self.age - self.fill_at > 0.5 {
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
                let head_y = f.root - total;
                let dx = c[0] - f.x;
                if dx.abs() < 34.0 && c[1] > head_y - 40.0 && c[1] < f.root + 12.0 {
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
        let width: f32 = self.spans.iter().map(|s| s.right - s.left).sum();
        self.burst_acc += dt * 0.7 * (width / 1000.0);
        if self.burst_acc < 1.0 {
            return;
        }
        self.burst_acc -= 1.0;
        if self.flowers.is_empty() {
            return;
        }
        let i = rng.below(self.flowers.len());
        let f = &self.flowers[i];
        if f.hidden || f.lost || f.excite > 0.05 || f.h < f.hmax * 0.85 {
            return;
        }
        let (x, y) = (f.x + f.bend, f.root - f.h - f.boost);
        for _ in 0..rng.below(5) + 9 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let v = rng.range(14.0, 52.0);
            self.sparks.push(Spark { x, y, vx: a.cos() * v, vy: a.sin() * v - 20.0, life: rng.range(0.45, 0.9) });
        }
        let old = (f.kind, f.hue);
        let (at, root) = (f.x, f.root);
        self.flowers[i] = new_flower(at, root, rng, Some(old));
    }

    pub fn draw(&self, b: &mut Batch, pixel_size: f32, glow: Rgb) {
        let grow = (self.age / 4.0).min(1.0);
        let px = pixel_size > 0.0;
        for layer in 0..2u8 {
            for g in self.grass.iter().filter(|g| g.layer == layer && !g.hidden) {
                let sway = match self.cursor {
                    Some(c) if (g.x - c[0]).abs() < 30.0 && c[1] > g.root - 40.0 && c[1] < g.root + 12.0 => -(g.x - c[0]) / 30.0 * 3.5,
                    _ => 0.0,
                };
                let h = g.h * (0.25 + 0.75 * grow);
                let w = if px { pixel_size.max(g.width) } else { g.width };
                let shade = if layer == 0 { 0.62 } else { 1.0 };
                let col = palette::mix([0.12, 0.42, 0.16], [0.40, 0.76, 0.26], g.tint);
                b.tri([g.x - w * 0.5, g.root + 1.0], [g.x + w * 0.5, g.root + 1.0], [g.x + g.lean + sway, g.root - h], [col[0] * shade, col[1] * shade, col[2] * shade, 1.0]);
            }
        }
        for f in &self.flowers {
            if !f.hidden {
                f.draw(b, f.root, pixel_size, glow);
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
fn new_flower(x: f32, root: f32, rng: &mut Rng, replacing: Option<(u8, f32)>) -> Flower {
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
        root,
        lost: false,
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
        Surf { id: 1, rect: Rect::new(x, 100.0, w, 50.0), z: 0, kind, inset, group: 0, shown: true }
    }

    fn run(bed: &mut Bed, rng: &mut Rng, secs: f32) -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        for _ in 0..(secs * 60.0) as usize {
            out.extend(bed.step(1.0 / 60.0, 0.6, None, rng, &|_| false));
        }
        out
    }

    fn grown(s: &Surf) -> (Bed, Rng) {
        let mut rng = Rng::new(3);
        let mut bed = Bed::new(s, &mut rng);
        run(&mut bed, &mut rng, 20.0);
        (bed, rng)
    }

    #[test]
    fn replacement_differs_in_shape_and_colour() {
        let mut rng = Rng::new(9);
        for _ in 0..500 {
            let old = (rng.below(5) as u8, rng.f());
            let f = new_flower(10.0, 100.0, &mut rng, Some(old));
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
    fn a_small_change_costs_nothing() {
        let s = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        let before = bed.flowers.len();
        bed.update(&surf(100.0, 398.0, SurfKind::Layer, 2.0), &mut rng);
        let mut lost = run(&mut bed, &mut rng, 0.5).len();
        bed.update(&s, &mut rng);
        lost += run(&mut bed, &mut rng, 3.0).len();
        assert_eq!(lost, 0);
        assert!(bed.flowers.len() >= before);
    }

    #[test]
    fn a_shrinking_module_squeezes_its_flowers_together_and_loses_none() {
        let s = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        let before = bed.flowers.len();
        // Down to a quarter of the width: every flower stays, now standing closer.
        bed.update(&surf(100.0, 100.0, SurfKind::Layer, 2.0), &mut rng);
        let lost = run(&mut bed, &mut rng, 3.0).len();
        assert_eq!(lost, 0, "nothing may puff");
        assert!(bed.flowers.len() >= before, "{} of {before}", bed.flowers.len());
        assert!(bed.flowers.iter().all(|f| f.x >= 100.0 && f.x <= 200.0));
        // And it can grow back out again, spreading them and filling any gap straight away.
        bed.update(&s, &mut rng);
        let counts: Vec<usize> = (0..4).map(|k| bed.flowers.iter().filter(|f| f.x >= 100.0 + k as f32 * 100.0 && f.x < 200.0 + k as f32 * 100.0).count()).collect();
        let (min, max) = (*counts.iter().min().unwrap(), *counts.iter().max().unwrap());
        assert!(min * 2 >= max, "gaps after growing back: {counts:?}");
    }

    #[test]
    fn a_module_that_is_gone_empties_from_the_far_end_in_order_starting_at_once() {
        let s = surf(100.0, 400.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        let n = bed.flowers.len();
        bed.abandon();
        let mut order = Vec::new();
        let mut first_at = None;
        for i in 0..(4 * 60) {
            for p in bed.step(1.0 / 60.0, 0.6, None, &mut rng, &|_| false) {
                first_at.get_or_insert(i);
                order.push(p[0]);
            }
        }
        assert!(first_at.unwrap() < 12, "it should start straight away: {first_at:?}");
        assert!(order.len() * 10 > n * 9, "{} of {n}", order.len());
        // The far (right) end goes first, then it works its way left.
        let inversions = order.windows(2).filter(|w| w[1] > w[0] + 10.0).count();
        assert!(inversions * 8 < order.len(), "not an orderly wave: {inversions}/{}", order.len());
        assert!(bed.flowers.is_empty());
    }

    #[test]
    fn stretching_a_window_keeps_corners_and_fills_gaps_at_once() {
        let s = surf(100.0, 300.0, SurfKind::Window, 10.0);
        let (mut bed, mut rng) = grown(&s);
        let near_right = |b: &Bed, right: f32| b.flowers.iter().map(|f| right - f.x).fold(f32::MAX, f32::min);
        let (nl, nr) = (bed.flowers.iter().map(|f| f.x - 100.0).fold(f32::MAX, f32::min), near_right(&bed, 400.0));
        let wide = surf(100.0, 800.0, SurfKind::Window, 10.0);
        bed.update(&wide, &mut rng);
        assert!((bed.flowers.iter().map(|f| f.x - 100.0).fold(f32::MAX, f32::min) - nl).abs() < 0.01, "left corner moved");
        assert!((near_right(&bed, 900.0) - nr).abs() < 0.01, "right corner did not follow its edge");
        let per = |lo: f32, hi: f32| bed.flowers.iter().filter(|f| f.x >= lo && f.x < hi).count();
        let counts: Vec<usize> = (0..8).map(|k| per(100.0 + k as f32 * 100.0, 200.0 + k as f32 * 100.0)).collect();
        let min = *counts.iter().min().unwrap();
        let max = *counts.iter().max().unwrap();
        assert!(min * 2 >= max, "gaps right after the stretch: {counts:?}");
        bed.update(&s, &mut rng);
        assert!(bed.flowers.iter().all(|f| f.x >= 100.0 && f.x <= 400.0));
    }

    #[test]
    fn new_ground_gets_a_lawn_too() {
        let s = surf(100.0, 200.0, SurfKind::Layer, 2.0);
        let (mut bed, mut rng) = grown(&s);
        bed.update(&surf(100.0, 400.0, SurfKind::Layer, 2.0), &mut rng);
        run(&mut bed, &mut rng, 15.0);
        let in_new = bed.flowers.iter().filter(|f| f.x > 300.0).count();
        let in_old = bed.flowers.iter().filter(|f| f.x <= 300.0).count();
        assert!(in_new * 10 > in_old * 7, "the new part is bare: {in_new} vs {in_old}");
        let grass_new = bed.grass.iter().filter(|g| g.x > 300.0).count();
        assert!(grass_new > 300, "{grass_new}");
    }
}
