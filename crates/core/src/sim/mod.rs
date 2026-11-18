//! The world of one monitor: falling things, piles on surfaces, and flowers growing from them.

mod flowers;
mod pile;

use crate::config::Effect;
use crate::draw::Batch;
use crate::geom::Rect;
use crate::palette::{self, Rgb};
use crate::rng::Rng;
use pile::Pile;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfKind {
    Ground,
    Window,
    Layer,
}

/// Something that can hold snow or grow flowers on its top edge. Coordinates are monitor-local logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Surf {
    pub id: u64,
    /// `rect.y` is the top edge things rest on.
    pub rect: Rect,
    pub z: i32,
    pub kind: SurfKind,
    /// How far in from each end the top edge starts (rounded corners).
    pub inset: f32,
    /// False while the surface exists but isn't on screen (another workspace): it keeps what lies on it.
    pub shown: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub w: f32,
    pub h: f32,
    pub surfs: Vec<Surf>,
    pub cursor: Option<[f32; 2]>,
}

#[derive(Clone, Debug)]
pub struct Params {
    pub effect: Effect,
    pub intensity: f32,
    pub melt: bool,
    pub interact: bool,
    pub wind: f32,
    pub ground: bool,
    pub windows: bool,
    pub layers: bool,
    /// Pixel renderer: fewer, chunkier things.
    pub pixel: bool,
    /// Logical pixels per art pixel in the pixel renderer; 0 when smooth.
    pub pixel_size: f32,
    /// Colour of the flowers' glow and sparks.
    pub glow: Rgb,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Flake,
    Streak,
    Splash,
}

struct P {
    kind: Kind,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    phase: f32,
    spin: f32,
    col: u8,
    /// Thrown/dropped things fall under gravity instead of drifting.
    heavy: bool,
    life: f32,
    /// How much pile it carries when it lands; zero means the usual amount for its size.
    mass: f32,
}

/// A pile that is coming apart: carried along by a moving window, or left behind by a closed one.
struct Puff {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
}

struct Shed {
    pile: Pile,
    follow: Option<u64>,
    /// Fraction of the pile that lets go each second.
    rate: f32,
    acc: f32,
}

pub struct World {
    parts: Vec<P>,
    piles: HashMap<u64, Pile>,
    sheds: Vec<Shed>,
    /// Blue puffs of what lost the ground under it.
    puffs: Vec<Puff>,
    flowers: HashMap<u64, flowers::Bed>,
    rng: Rng,
    time: f32,
    spawn_acc: f32,
    scene: Scene,
    last_cursor: Option<[f32; 2]>,
    cursor_speed: f32,
    effect: Effect,
}

const MAX_PARTS: usize = 6000;

impl World {
    pub fn new(seed: u64) -> World {
        World {
            parts: Vec::new(),
            piles: HashMap::new(),
            sheds: Vec::new(),
            puffs: Vec::new(),
            flowers: HashMap::new(),
            rng: Rng::new(seed),
            time: 0.0,
            spawn_acc: 0.0,
            scene: Scene::default(),
            last_cursor: None,
            cursor_speed: 0.0,
            effect: Effect::Snow,
        }
    }

    pub fn particle_count(&self) -> usize {
        self.parts.len()
    }
    /// Everything that lies: the sum of all pile heights.
    pub fn pile_mass(&self) -> f32 {
        self.piles.values().map(|p| p.total()).sum::<f32>() + self.sheds.iter().map(|s| s.pile.total()).sum::<f32>()
    }
    pub fn pile_height(&self, id: u64, x: f32) -> f32 {
        self.piles.get(&id).map_or(0.0, |p| p.height_at(x))
    }

    /// Wipe everything that has settled; it blows away as flakes.
    pub fn clear(&mut self) {
        let ids: Vec<u64> = self.piles.keys().copied().collect();
        for id in ids {
            if let Some(pile) = self.piles.remove(&id) {
                self.spill(&pile);
            }
        }
        for sh in std::mem::take(&mut self.sheds) {
            self.spill(&sh.pile);
        }
        self.flowers.clear();
    }

    /// Takes new surface geometry: piles follow their surface, wait while it is off screen, and come apart
    /// gradually when it moves or goes away.
    pub fn set_scene(&mut self, scene: Scene) {
        let mut carried: Vec<u64> = Vec::new();
        let mut vanished: Vec<u64> = Vec::new();
        let mut cut: Vec<Pile> = Vec::new();
        for (id, pile) in self.piles.iter_mut() {
            match scene.surfs.iter().find(|s| s.id == *id) {
                None => vanished.push(*id),
                Some(s) if !s.shown => pile.dormant = true,
                Some(s) if s.kind == SurfKind::Layer => {
                    // A bar stays put. Its painted parts come and go, and what is left without ground falls.
                    pile.dormant = false;
                    if (s.rect.y - pile.base).abs() > 4.0 {
                        vanished.push(*id);
                    } else {
                        cut.extend(pile.fit(s.rect.x + s.inset, s.rect.right() - s.inset));
                        pile.base = s.rect.y;
                    }
                }
                Some(s) => {
                    let moved = (s.rect.x + s.inset - pile.x0).abs() > 4.0 || (s.rect.y - pile.base).abs() > 4.0;
                    let width = (s.rect.w - 2.0 * s.inset).max(0.0);
                    pile.dormant = false;
                    if moved {
                        carried.push(*id);
                    } else if (width - pile.width()).abs() > pile::CELL {
                        pile.resize(s.rect.x + s.inset, width, s.rect.y);
                    } else {
                        pile.base = s.rect.y;
                    }
                }
            }
        }
        for pile in cut {
            self.sheds.push(Shed { pile, follow: None, rate: 2.2, acc: 0.0 });
        }
        for id in carried {
            if let Some(pile) = self.piles.remove(&id) {
                self.sheds.push(Shed { pile, follow: Some(id), rate: 0.7, acc: 0.0 });
            }
        }
        for id in vanished {
            if let Some(pile) = self.piles.remove(&id) {
                self.sheds.push(Shed { pile, follow: None, rate: 1.8, acc: 0.0 });
            }
        }
        // What a moving window carries rides along with it until it has all slid off.
        for sh in &mut self.sheds {
            if let Some(id) = sh.follow {
                match scene.surfs.iter().find(|s| s.id == id && s.shown) {
                    Some(s) => sh.pile.carry(s.rect.x + s.inset, (s.rect.w - 2.0 * s.inset).max(0.0), s.rect.y),
                    None => sh.follow = None,
                }
            }
        }
        // Flowers whose ground vanished (a bar that shrank or went away) puff into blue sparks.
        let mut lost: Vec<[f32; 2]> = Vec::new();
        let rng = &mut self.rng;
        self.flowers.retain(|id, bed| match scene.surfs.iter().find(|s| s.id == *id) {
            None => {
                lost.extend(bed.heads());
                false
            }
            Some(s) => {
                lost.extend(bed.follow(s, rng));
                true
            }
        });
        for h in lost.into_iter().take(400) {
            for _ in 0..5 {
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let v = self.rng.range(10.0, 46.0);
                self.puffs.push(Puff { x: h[0], y: h[1], vx: a.cos() * v, vy: a.sin() * v - 16.0, life: self.rng.range(0.4, 0.85) });
            }
        }
        self.scene = scene;
    }

    /// Turns a pile into falling chunks at once.
    fn spill(&mut self, pile: &Pile) {
        let cells = pile.h.len();
        let stride = (cells / 160).max(1);
        let mut i = 0;
        while i < cells {
            let h = pile.h[i];
            if h > 0.6 && self.parts.len() < MAX_PARTS {
                let n = ((h / 3.0).ceil() as usize).min(5);
                for _ in 0..n {
                    let x = pile.x0 + (i as f32 + self.rng.f()) * pile::CELL;
                    let y = pile.base - h * self.rng.f();
                    let mut c = self.chunk(x, y, pile.c[i]);
                    c.mass = h / n as f32;
                    self.parts.push(c);
                }
            }
            i += stride;
        }
    }

    fn chunk(&mut self, x: f32, y: f32, col: u8) -> P {
        P {
            kind: Kind::Flake,
            x,
            y,
            vx: self.rng.range(-40.0, 40.0),
            vy: self.rng.range(-10.0, 30.0),
            size: self.rng.range(1.6, 3.2),
            phase: self.rng.f() * 6.28,
            spin: self.rng.range(-3.0, 3.0),
            col,
            heavy: true,
            life: 0.0,
            mass: 0.0,
        }
    }

    /// Lets a share of every coming-apart pile go this frame.
    fn shed_step(&mut self, dt: f32) {
        let mut sheds = std::mem::take(&mut self.sheds);
        for sh in &mut sheds {
            let cells = sh.pile.h.len();
            sh.acc += cells as f32 * sh.rate * dt;
            while sh.acc >= 1.0 {
                sh.acc -= 1.0;
                let i = self.rng.below(cells);
                let h = sh.pile.h[i];
                if h > 0.2 && self.parts.len() < MAX_PARTS {
                    let n = ((h / 2.5).ceil() as usize).min(4);
                    for _ in 0..n {
                        let x = sh.pile.x0 + (i as f32 + self.rng.f()) * pile::CELL;
                        let y = sh.pile.base - h * self.rng.f();
                        let mut c = self.chunk(x, y, sh.pile.c[i]);
                        c.mass = h / n as f32;
                        self.parts.push(c);
                    }
                    sh.pile.h[i] = 0.0;
                }
            }
            // Slump what is left so the edges of the gap look natural.
            sh.pile.relax(dt);
        }
        sheds.retain(|sh| sh.pile.total() > 0.3);
        self.sheds = sheds;
    }

    pub fn step(&mut self, dt: f32, p: &Params) {
        let dt = dt.clamp(0.0, 0.05);
        self.time += dt;
        if p.effect != self.effect {
            self.effect = p.effect;
            // A different kind of weather: what lies is swept away.
            self.clear();
        }
        self.track_cursor(dt);
        self.spawn(dt, p);
        self.move_parts(dt, p);
        self.piles_step(dt, p);
        self.shed_step(dt);
        for pf in &mut self.puffs {
            pf.life -= dt;
            pf.vy += 60.0 * dt;
            pf.vx *= 1.0 - 2.0 * dt;
            pf.x += pf.vx * dt;
            pf.y += pf.vy * dt;
        }
        self.puffs.retain(|pf| pf.life > 0.0);
        if p.effect == Effect::Flowers {
            self.flowers_step(dt, p);
        } else if !self.flowers.is_empty() {
            self.flowers.clear();
        }
    }

    fn track_cursor(&mut self, dt: f32) {
        self.cursor_speed = match (self.scene.cursor, self.last_cursor) {
            (Some(a), Some(b)) if dt > 0.0 => {
                let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
                let inst = (dx * dx + dy * dy).sqrt() / dt;
                self.cursor_speed * 0.6 + inst * 0.4
            }
            _ => 0.0,
        };
        self.last_cursor = self.scene.cursor;
    }

    fn spawn(&mut self, dt: f32, p: &Params) {
        let per_sec = match p.effect {
            Effect::Snow => 70.0,
            Effect::Leaves => 9.0,
            Effect::Blossom => 22.0,
            Effect::Rain => 420.0,
            Effect::Flowers => 0.0,
        } * p.intensity
            * (self.scene.w / 1920.0).max(0.3)
            * if p.pixel { 0.7 } else { 1.0 };
        self.spawn_acc += per_sec * dt;
        let w = self.scene.w;
        while self.spawn_acc >= 1.0 && self.parts.len() < MAX_PARTS {
            self.spawn_acc -= 1.0;
            let margin = 120.0 * p.wind.abs();
            let x = self.rng.range(-margin - 20.0, w + margin + 20.0) - if p.wind > 0.0 { margin } else { 0.0 };
            let colors = flake_colors(p.effect).len();
            let col = self.rng.below(colors) as u8;
            let part = match p.effect {
                Effect::Snow => {
                    let size = self.rng.range(1.2, 3.4);
                    P { kind: Kind::Flake, x, y: -8.0, vx: 0.0, vy: 22.0 + size * 14.0, size, phase: self.rng.f() * 6.28, spin: 0.0, col, heavy: false, life: 0.0 , mass: 0.0 }
                }
                Effect::Leaves => {
                    let size = self.rng.range(4.0, 7.0);
                    P { kind: Kind::Flake, x, y: -12.0, vx: 0.0, vy: self.rng.range(35.0, 60.0), size, phase: self.rng.f() * 6.28, spin: self.rng.range(-2.5, 2.5), col, heavy: false, life: 0.0 , mass: 0.0 }
                }
                Effect::Blossom => {
                    let size = self.rng.range(2.5, 4.5);
                    P { kind: Kind::Flake, x, y: -10.0, vx: 0.0, vy: self.rng.range(28.0, 50.0), size, phase: self.rng.f() * 6.28, spin: self.rng.range(-3.0, 3.0), col, heavy: false, life: 0.0 , mass: 0.0 }
                }
                _ => P { kind: Kind::Streak, x, y: -20.0, vx: 0.0, vy: self.rng.range(620.0, 900.0), size: self.rng.range(7.0, 14.0), phase: 0.0, spin: 0.0, col, heavy: false, life: 0.0 , mass: 0.0 },
            };
            self.parts.push(part);
        }
    }

    fn move_parts(&mut self, dt: f32, p: &Params) {
        let t = self.time;
        let h = self.scene.h;
        let w = self.scene.w;
        let cursor = if p.interact { self.scene.cursor } else { None };
        let accumulate = p.effect.accumulates();
        let mut parts = std::mem::take(&mut self.parts);
        let mut splashes: Vec<P> = Vec::new();
        parts.retain_mut(|q| {
            if q.kind == Kind::Splash {
                q.life -= dt;
                q.vy += 700.0 * dt;
                q.x += q.vx * dt;
                q.y += q.vy * dt;
                return q.life > 0.0;
            }
            let oy = q.y;
            if q.heavy {
                q.vy = (q.vy + 500.0 * dt).min(260.0);
                q.vx *= 1.0 - 1.5 * dt;
                q.x += q.vx * dt;
                q.y += q.vy * dt;
            } else if q.kind == Kind::Streak {
                q.x += (p.wind * 140.0) * dt;
                q.y += q.vy * dt;
            } else {
                let sway = match p.effect {
                    Effect::Leaves => 46.0,
                    Effect::Blossom => 30.0,
                    _ => 14.0,
                };
                let freq = match p.effect {
                    Effect::Leaves => 1.4,
                    Effect::Blossom => 1.8,
                    _ => 1.1,
                };
                q.x += (p.wind * 90.0 + (t * freq + q.phase).sin() * sway) * dt;
                q.y += q.vy * dt;
                q.phase += q.spin * dt * 0.3;
            }
            // The cursor shoves what falls near it.
            if let Some(c) = cursor {
                let (dx, dy) = (q.x - c[0], q.y - c[1]);
                let d2 = dx * dx + dy * dy;
                if d2 < 70.0 * 70.0 && q.kind != Kind::Streak {
                    let d = d2.sqrt().max(1.0);
                    let push = (1.0 - d / 70.0) * 420.0 * dt;
                    q.x += dx / d * push;
                    q.y += dy / d * push * 0.6;
                }
            }
            if q.x < -200.0 || q.x > w + 200.0 || q.y > h + 40.0 {
                return false;
            }
            // Landing: the highest resting place that was crossed this step.
            let mut best: Option<(u64, f32)> = None;
            for s in &self.scene.surfs {
                let usable = match s.kind {
                    SurfKind::Ground => p.ground,
                    SurfKind::Window => p.windows,
                    SurfKind::Layer => p.layers,
                };
                if !usable || !s.shown || s.rect.w - 2.0 * s.inset <= 4.0 {
                    continue;
                }
                if q.x < s.rect.x + s.inset || q.x >= s.rect.right() - s.inset {
                    continue;
                }
                let lift = if accumulate { self.piles.get(&s.id).map_or(0.0, |pl| pl.height_at(q.x)) } else { 0.0 };
                let top = s.rect.y - lift;
                if oy <= top && q.y >= top && best.map_or(true, |(_, bt)| top < bt) {
                    // Hidden under a window in front? Then nothing rests here.
                    let hidden = s.kind != SurfKind::Ground
                        && self.scene.surfs.iter().any(|o| o.id != s.id && o.z > s.z && o.rect.contains(q.x, s.rect.y + 2.0));
                    if !hidden {
                        best = Some((s.id, top));
                    }
                }
            }
            if let Some((id, top)) = best {
                if accumulate {
                    let col = q.col;
                    let amt = if q.mass > 0.0 { q.mass.min(4.0) } else { (q.size * q.size).sqrt() * if p.effect == Effect::Snow { 0.64 } else { 1.0 } };
                    if let Some(s) = self.scene.surfs.iter().find(|s| s.id == id) {
                        let (x0, wd, base) = (s.rect.x + s.inset, (s.rect.w - 2.0 * s.inset).max(0.0), s.rect.y);
                        let cap = if s.kind == SurfKind::Ground { 46.0 } else { 20.0 };
                        let pile = self.piles.entry(id).or_insert_with(|| Pile::new(x0, wd, base));
                        pile.add(q.x, amt, col, cap);
                    }
                    return false;
                } else if q.kind == Kind::Streak {
                    // Rain splashes where it lands.
                    for _ in 0..2 {
                        if splashes.len() < 200 {
                            splashes.push(P {
                                kind: Kind::Splash,
                                x: q.x,
                                y: top,
                                vx: (self.rng.f() - 0.5) * 120.0,
                                vy: -self.rng.range(80.0, 190.0),
                                size: 1.4,
                                phase: 0.0,
                                spin: 0.0,
                                col: q.col,
                                heavy: true,
                                life: self.rng.range(0.15, 0.32),
                                mass: 0.0,
                            });
                        }
                    }
                    return false;
                }
            }
            true
        });
        parts.extend(splashes);
        self.parts = parts;
    }

    fn piles_step(&mut self, dt: f32, p: &Params) {
        let speed = self.cursor_speed;
        let cursor = if p.interact && speed > 90.0 { self.scene.cursor } else { None };
        let melt = p.melt;
        let mut spawned: Vec<P> = Vec::new();
        for pile in self.piles.values_mut().filter(|p| !p.dormant) {
            pile.relax(dt);
            if melt {
                // Thaws slowly: about an hour for a deep drift.
                pile.melt(dt * 0.012);
            }
            if let Some(c) = cursor {
                let thrown = pile.disturb(c, 38.0, (speed * dt * 0.02).min(3.0));
                for (x, y, col, away, take) in thrown {
                    if spawned.len() < 120 && self.parts.len() + spawned.len() < MAX_PARTS {
                        spawned.push(P {
                            kind: Kind::Flake,
                            x,
                            y,
                            vx: away * self.rng.range(40.0, 160.0) + self.rng.range(-30.0, 30.0),
                            vy: -self.rng.range(40.0, 170.0),
                            size: self.rng.range(1.3, 2.8),
                            phase: self.rng.f() * 6.28,
                            spin: self.rng.range(-3.0, 3.0),
                            col,
                            heavy: true,
                            life: 0.0,
                            // What the cursor ploughed up lands somewhere else; nothing is added.
                            mass: take.max(0.05),
                        });
                    }
                }
            }
        }
        self.parts.extend(spawned);
        self.piles.retain(|_, pl| pl.dormant || pl.total() > 0.05 || pl.fresh());
    }

    fn flowers_step(&mut self, dt: f32, p: &Params) {
        let cursor = if p.interact { self.scene.cursor } else { None };
        for s in &self.scene.surfs {
            let usable = match s.kind {
                SurfKind::Ground => p.ground,
                SurfKind::Window => p.windows,
                SurfKind::Layer => p.layers,
            };
            if !usable || !s.shown || s.rect.w - 2.0 * s.inset <= 6.0 {
                continue;
            }
            let hidden_at = |x: f32| s.kind != SurfKind::Ground && self.scene.surfs.iter().any(|o| o.id != s.id && o.z > s.z && o.rect.contains(x, s.rect.y + 2.0));
            let bed = self.flowers.entry(s.id).or_insert_with(|| flowers::Bed::new(s, &mut self.rng));
            bed.step(dt, p.intensity, cursor, &mut self.rng, &hidden_at);
        }
    }

    pub fn draw(&self, b: &mut Batch, p: &Params) {
        // Flowers and piles first, falling things over them.
        for pile in self.piles.values().filter(|pl| !pl.dormant) {
            pile.draw(b, p.effect, self.time, p.pixel);
        }
        for sh in &self.sheds {
            sh.pile.draw(b, p.effect, self.time, p.pixel);
        }
        for (id, bed) in &self.flowers {
            if self.scene.surfs.iter().any(|s| s.id == *id && s.shown) {
                bed.draw(b, p.pixel_size, p.glow);
            }
        }
        for pf in &self.puffs {
            b.disc(pf.x, pf.y, if p.pixel_size > 0.0 { p.pixel_size * 0.6 } else { 1.0 }, [0.35, 0.62, 1.0, (pf.life * 2.5).clamp(0.0, 1.0)]);
        }
        let colors = flake_colors(p.effect);
        for q in &self.parts {
            let c = colors[(q.col as usize) % colors.len()];
            match q.kind {
                Kind::Splash => {
                    let a = (q.life * 5.0).clamp(0.0, 1.0) * 0.8;
                    b.disc(q.x, q.y, q.size, [c[0], c[1], c[2], a]);
                }
                Kind::Streak => {
                    let len = q.size * if p.pixel { 1.0 } else { 1.0 };
                    b.line([q.x, q.y - len], [q.x + p.wind * 140.0 * len / q.vy, q.y], if p.pixel { 1.5 } else { 1.1 }, [c[0], c[1], c[2], 0.75]);
                }
                Kind::Flake => match p.effect {
                    Effect::Snow => b.disc(q.x, q.y, q.size, [c[0], c[1], c[2], 0.95]),
                    Effect::Leaves => b.rect(q.x, q.y, q.size, q.size * 0.55, q.phase * 2.0 + self.time * q.spin * 0.5, [c[0], c[1], c[2], 1.0]),
                    _ => b.ellipse(q.x, q.y, q.size, q.size * 0.6, q.phase * 2.0 + self.time * q.spin * 0.5, [c[0], c[1], c[2], 0.95]),
                },
            }
        }
    }
}

pub fn flake_colors(e: Effect) -> &'static [Rgb] {
    match e {
        Effect::Snow => palette::SNOW,
        Effect::Leaves => palette::LEAVES,
        Effect::Blossom => palette::BLOSSOM,
        Effect::Rain => palette::RAIN,
        Effect::Flowers => palette::SNOW,
    }
}
