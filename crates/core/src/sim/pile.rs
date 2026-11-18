use super::flake_colors;
use crate::config::Effect;
use crate::draw::Batch;

pub const CELL: f32 = 3.0;
/// Steepest allowed step between neighbouring cells (a natural angle of repose).
const SLOPE: f32 = 2.2;

pub struct Pile {
    pub x0: f32,
    pub base: f32,
    pub h: Vec<f32>,
    pub c: Vec<u8>,
    age: f32,
    /// Its surface is off screen; the pile waits for it.
    pub dormant: bool,
}

impl Pile {
    pub fn new(x0: f32, width: f32, base: f32) -> Pile {
        let n = ((width / CELL).ceil() as usize).max(1);
        Pile { x0, base, h: vec![0.0; n], c: vec![0; n], age: 0.0, dormant: false }
    }
    pub fn width(&self) -> f32 {
        self.h.len() as f32 * CELL
    }
    pub fn fresh(&self) -> bool {
        self.age < 1.0
    }
    pub fn total(&self) -> f32 {
        self.h.iter().sum()
    }
    pub fn resize(&mut self, x0: f32, width: f32, base: f32) {
        let n = ((width / CELL).ceil() as usize).max(1);
        let mut h = vec![0.0; n];
        let mut c = vec![0; n];
        for i in 0..n {
            let src = (x0 + i as f32 * CELL - self.x0) / CELL;
            if src >= 0.0 && (src as usize) < self.h.len() {
                h[i] = self.h[src as usize];
                c[i] = self.c[src as usize];
            }
        }
        self.x0 = x0;
        self.base = base;
        self.h = h;
        self.c = c;
    }
    /// Keeps the pile where it is but makes it cover exactly `lo..hi` (a bar whose painted part grew or
    /// shrank). What no longer has ground under it comes back as separate pieces, still at its old place.
    pub fn fit(&mut self, lo: f32, hi: f32) -> Vec<Pile> {
        let n = self.h.len() as i64;
        let ia = ((lo - self.x0) / CELL - 0.5).ceil() as i64;
        let ib = (((hi - self.x0) / CELL - 0.5).floor() as i64 + 1).max(ia + 1);
        let mut cut = Vec::new();
        let piece = |from: i64, to: i64, me: &Pile| -> Option<Pile> {
            let (from, to) = (from.clamp(0, n), to.clamp(0, n));
            if to <= from {
                return None;
            }
            let (f, t) = (from as usize, to as usize);
            let mut p = Pile::new(me.x0 + from as f32 * CELL, (t - f) as f32 * CELL, me.base);
            p.h.copy_from_slice(&me.h[f..t]);
            p.c.copy_from_slice(&me.c[f..t]);
            if p.total() > 0.3 { Some(p) } else { None }
        };
        cut.extend(piece(0, ia, self));
        cut.extend(piece(ib, n, self));
        let mut h = Vec::new();
        let mut c = Vec::new();
        for k in ia..ib {
            if k >= 0 && k < n {
                h.push(self.h[k as usize]);
                c.push(self.c[k as usize]);
            } else {
                h.push(0.0);
                c.push(0);
            }
        }
        self.x0 += ia as f32 * CELL;
        self.h = h;
        self.c = c;
        cut
    }

    /// Moves the pile along with its window, keeping every cell where it sits on the window.
    pub fn carry(&mut self, x0: f32, width: f32, base: f32) {
        let n = ((width / CELL).ceil() as usize).max(1);
        self.h.resize(n, 0.0);
        self.c.resize(n, 0);
        self.x0 = x0;
        self.base = base;
    }
    fn idx(&self, x: f32) -> usize {
        (((x - self.x0) / CELL).floor().max(0.0) as usize).min(self.h.len() - 1)
    }
    pub fn height_at(&self, x: f32) -> f32 {
        // Linear between cell centres, so falling things land on the drawn slope.
        let f = (x - self.x0) / CELL - 0.5;
        let i = f.floor();
        let t = f - i;
        let get = |k: i32| if k < 0 || k as usize >= self.h.len() { 0.0 } else { self.h[k as usize] };
        get(i as i32) * (1.0 - t) + get(i as i32 + 1) * t
    }
    pub fn add(&mut self, x: f32, amount: f32, col: u8, cap: f32) {
        let i = self.idx(x);
        let n = self.h.len();
        self.h[i] = (self.h[i] + amount * 0.6).min(cap);
        self.c[i] = col;
        if i > 0 {
            self.h[i - 1] = (self.h[i - 1] + amount * 0.2).min(cap);
        }
        if i + 1 < n {
            self.h[i + 1] = (self.h[i + 1] + amount * 0.2).min(cap);
        }
    }
    /// Lets steep slopes slump, with edges tapering off.
    pub fn relax(&mut self, dt: f32) {
        self.age += dt;
        let n = self.h.len();
        for _ in 0..2 {
            for i in 0..n.saturating_sub(1) {
                let d = self.h[i] - self.h[i + 1];
                if d.abs() > SLOPE {
                    let m = (d.abs() - SLOPE) * 0.5;
                    let m = m.min(if d > 0.0 { self.h[i] } else { self.h[i + 1] });
                    let s = d.signum();
                    self.h[i] -= s * m;
                    self.h[i + 1] += s * m;
                    if s > 0.0 {
                        self.c[i + 1] = self.c[i];
                    } else {
                        self.c[i] = self.c[i + 1];
                    }
                }
            }
        }
        // The ends are not allowed to stand tall: the edge of a window tapers off.
        if n > 1 {
            self.h[0] = self.h[0].min(self.h[1] * 0.7);
            self.h[n - 1] = self.h[n - 1].min(self.h[n - 2] * 0.7);
        }
    }
    pub fn melt(&mut self, rate: f32) {
        for h in &mut self.h {
            *h = (*h - (0.02 + *h * rate) * 0.5 * rate.min(1.0) * 20.0).max(0.0);
        }
    }
    /// The cursor ploughs through: removes height near it and reports what to throw.
    pub fn disturb(&mut self, c: [f32; 2], r: f32, amount: f32) -> Vec<(f32, f32, u8, f32, f32)> {
        let mut out = Vec::new();
        let n = self.h.len();
        let lo = ((c[0] - r - self.x0) / CELL).floor().max(0.0) as usize;
        let hi = (((c[0] + r - self.x0) / CELL).ceil().max(0.0) as usize).min(n);
        for i in lo..hi {
            let x = self.x0 + (i as f32 + 0.5) * CELL;
            let top = self.base - self.h[i];
            if self.h[i] < 0.4 || c[1] < top - r || c[1] > self.base + r * 0.5 {
                continue;
            }
            let dx = x - c[0];
            let fall = 1.0 - (dx.abs() / r).min(1.0);
            let take = (amount * fall).min(self.h[i]);
            if take > 0.05 {
                self.h[i] -= take;
                if out.len() < 60 && (take > 0.3 || i % 2 == 0) {
                    out.push((x, top, self.c[i], if dx >= 0.0 { 1.0 } else { -1.0 }, take));
                }
            }
        }
        out
    }

    pub fn draw(&self, b: &mut Batch, effect: Effect, _time: f32, pixel: bool) {
        let colors = flake_colors(effect);
        let n = self.h.len();
        for i in 0..n {
            let (h0, h1) = (self.h[i], if i + 1 < n { self.h[i + 1] } else { 0.0 });
            let h0 = if i == 0 { h0 } else { (h0 + self.h[i - 1]) * 0.5 };
            let h1 = if i + 1 < n { (h1 + self.h[i]) * 0.5 } else { 0.0 };
            if h0 < 0.15 && h1 < 0.15 {
                continue;
            }
            let x = self.x0 + i as f32 * CELL;
            let mut col = colors[self.c[i] as usize % colors.len()];
            if effect == Effect::Snow {
                // Snow lies in one cool white; slight shade by depth gives it body.
                col = [0.96, 0.98, 1.0];
            } else if (i * 7919) % 5 == 0 {
                col = [col[0] * 0.88, col[1] * 0.88, col[2] * 0.88];
            }
            let top = [col[0], col[1], col[2], 1.0];
            let deep = if effect == Effect::Snow { [0.80, 0.88, 0.97, 1.0] } else { [col[0] * 0.7, col[1] * 0.7, col[2] * 0.7, 1.0] };
            if effect != Effect::Snow {
                self.draw_litter(b, effect, i, x, h0, h1, colors);
                continue;
            }
            let (y0, y1) = (self.base - h0, self.base - h1);
            // Slightly below the surface edge so no seam shows between pile and the thing it sits on.
            let by = self.base + 1.0;
            if pixel {
                b.quad4([[x, y0], [x + CELL, y1], [x, by], [x + CELL, by]], [top, top, top, top]);
            } else {
                b.quad4([[x, y0], [x + CELL, y1], [x, by], [x + CELL, by]], [top, top, deep, deep]);
            }
        }
    }

    /// Leaves and petals lie in a heap. The body is a smooth two-dimensional mottling (colour drifts in both
    /// directions, so nothing reads as a vertical stripe) with individual leaves lying on and in it.
    fn draw_litter(&self, b: &mut Batch, effect: Effect, i: usize, x: f32, h0: f32, h1: f32, colors: &[[f32; 3]]) {
        const BAND: f32 = 4.0;
        let n = colors.len();
        let total = h0.max(h1).max(1.0);
        // Colour at a point: value noise picks along the palette; deeper is darker.
        let tint = |px: f32, py: f32| -> [f32; 4] {
            let v = (0.62 * vnoise(px / 7.0, py / 6.0) + 0.38 * vnoise(px / 3.1 + 17.0, py / 2.7 + 5.0)).clamp(0.0, 0.999);
            let t = v * (n - 1) as f32;
            let (a, f) = (t.floor() as usize, t.fract());
            let c = palette_mix(colors[a], colors[(a + 1).min(n - 1)], f);
            let depth = ((self.base - py) / total).clamp(0.0, 1.0);
            let k = 1.0 - 0.38 * depth;
            [c[0] * k, c[1] * k, c[2] * k, 1.0]
        };
        let full = ((h0.min(h1)) / BAND).floor() as usize;
        for k in 0..full {
            let (yb, yt) = (self.base + 1.0 - k as f32 * BAND, self.base - (k + 1) as f32 * BAND);
            b.quad4([[x, yt], [x + CELL, yt], [x, yb], [x + CELL, yb]], [tint(x, yt), tint(x + CELL, yt), tint(x, yb), tint(x + CELL, yb)]);
        }
        let yb = self.base + 1.0 - full as f32 * BAND;
        let (yl, yr) = (self.base - h0, self.base - h1);
        b.quad4([[x, yl], [x + CELL, yr], [x, yb], [x + CELL, yb]], [tint(x, yl), tint(x + CELL, yr), tint(x, yb), tint(x + CELL, yb)]);
        // Single leaves or petals lying on and in the heap, placed by hash so they stay put as it grows.
        let (rx, ry) = if effect == Effect::Leaves { (3.4, 1.5) } else { (2.3, 1.2) };
        let hash = |a: usize, c: usize| -> f32 { ((a.wrapping_mul(73_856_093) ^ c.wrapping_mul(19_349_663)).wrapping_mul(2_654_435_761) >> 8 & 0xffff) as f32 / 65535.0 };
        for k in 0..=full {
            if hash(i, k * 7 + 1) > 0.55 {
                continue;
            }
            let cx = x + hash(i, k * 7 + 2) * CELL * 2.0;
            let cy = self.base - (k as f32 + hash(i, k * 7 + 3)) * BAND;
            let surf = self.base - (h0 + (h1 - h0) * ((cx - x) / CELL).clamp(0.0, 1.0));
            if cy < surf - 1.0 {
                continue;
            }
            let rot = hash(i, k * 7 + 4) * std::f32::consts::PI;
            let mut c = tint(cx, cy);
            let lift = 0.82 + 0.4 * hash(i, k * 7 + 5);
            c = [(c[0] * lift).min(1.0), (c[1] * lift).min(1.0), (c[2] * lift).min(1.0), 1.0];
            b.ellipse(cx, cy, rx * (0.7 + 0.5 * hash(i, k * 7 + 6)), ry, rot, c);
        }
    }
}

fn palette_mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Smooth value noise in 0..1.
fn vnoise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (xf, yf) = (x - xi, y - yi);
    let h = |a: i32, c: i32| -> f32 {
        let n = (a.wrapping_mul(374_761_393) ^ c.wrapping_mul(668_265_263)).wrapping_mul(1_274_126_177);
        ((n >> 8) & 0xffff) as f32 / 65535.0
    };
    let (xi, yi) = (xi as i32, yi as i32);
    let (u, v) = (xf * xf * (3.0 - 2.0 * xf), yf * yf * (3.0 - 2.0 * yf));
    let top = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * u;
    let bot = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * u;
    top + (bot - top) * v
}
