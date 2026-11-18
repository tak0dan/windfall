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
                self.draw_litter(b, i, x, h0, h1, colors);
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

    /// Leaves and petals lie in a heap: bands of mixed colour, so no column reads as a vertical stripe.
    fn draw_litter(&self, b: &mut Batch, i: usize, x: f32, h0: f32, h1: f32, colors: &[[f32; 3]]) {
        const BAND: f32 = 3.5;
        let n = colors.len();
        let hash = |i: usize, j: usize| ((i.wrapping_mul(73_856_093) ^ j.wrapping_mul(19_349_663)).wrapping_mul(2_654_435_761) >> 9) as usize;
        let shade = |j: usize, depth: f32| {
            let k = hash(i, j);
            let c = colors[(self.c[i] as usize + k % 3) % n];
            let f = (1.0 - 0.34 * depth) * (0.9 + 0.1 * ((k >> 4) % 3) as f32);
            [c[0] * f, c[1] * f, c[2] * f, 1.0]
        };
        let top_h = h0.min(h1);
        let full = (top_h / BAND).floor() as usize;
        let total = h0.max(h1).max(1.0);
        for k in 0..full {
            let (yb, yt) = (self.base + 1.0 - k as f32 * BAND, self.base - (k + 1) as f32 * BAND);
            let col = shade(k, (k as f32 * BAND) / total);
            b.quad4([[x, yt], [x + CELL, yt], [x, yb], [x + CELL, yb]], [col; 4]);
        }
        let yb = self.base + 1.0 - full as f32 * BAND;
        let col = shade(full, (full as f32 * BAND) / total);
        b.quad4([[x, self.base - h0], [x + CELL, self.base - h1], [x, yb], [x + CELL, yb]], [col; 4]);
    }
}
