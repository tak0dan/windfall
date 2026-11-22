//! A flat list of coloured triangles. The simulation writes into a `Batch`, the daemon uploads it.

pub const FLOATS_PER_VERTEX: usize = 8; // x y u v r g b a

#[derive(Default)]
pub struct Batch {
    pub data: Vec<f32>,
}

impl Batch {
    pub fn clear(&mut self) {
        self.data.clear();
    }
    pub fn vertex_count(&self) -> usize {
        self.data.len() / FLOATS_PER_VERTEX
    }
    #[inline]
    fn v(&mut self, x: f32, y: f32, u: f32, v: f32, c: [f32; 4]) {
        self.data
            .extend_from_slice(&[x, y, u, v, c[0], c[1], c[2], c[3]]);
    }

    pub fn tri(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], col: [f32; 4]) {
        self.v(a[0], a[1], 0.0, 0.0, col);
        self.v(b[0], b[1], 0.0, 0.0, col);
        self.v(c[0], c[1], 0.0, 0.0, col);
    }

    /// Quad with per-corner solid colour: a b are the top corners, c d the bottom ones (a-b-d-c winding).
    pub fn quad4(&mut self, p: [[f32; 2]; 4], col: [[f32; 4]; 4]) {
        for &i in &[0usize, 1, 2, 1, 3, 2] {
            self.v(p[i][0], p[i][1], 0.0, 0.0, col[i]);
        }
    }

    /// A disc (soft edge in smooth mode, hard in pixel mode).
    pub fn disc(&mut self, x: f32, y: f32, r: f32, col: [f32; 4]) {
        self.ellipse(x, y, r, r, 0.0, col);
    }

    pub fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32, rot: f32, col: [f32; 4]) {
        let (s, c) = rot.sin_cos();
        let corner = |ux: f32, uy: f32| {
            let px = ux * rx;
            let py = uy * ry;
            (x + px * c - py * s, y + px * s + py * c, ux, uy)
        };
        let q = [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(-1.0, 1.0),
            corner(1.0, 1.0),
        ];
        for &i in &[0usize, 1, 2, 1, 3, 2] {
            self.v(q[i].0, q[i].1, q[i].2, q[i].3, col);
        }
    }

    /// A solid, optionally rotated rectangle (a leaf, a petal, a snow crystal chunk).
    pub fn rect(&mut self, x: f32, y: f32, hw: f32, hh: f32, rot: f32, col: [f32; 4]) {
        let (s, c) = rot.sin_cos();
        let p = |ux: f32, uy: f32| [x + ux * hw * c - uy * hh * s, y + ux * hw * s + uy * hh * c];
        let (a, b, d, e) = (p(-1.0, -1.0), p(1.0, -1.0), p(-1.0, 1.0), p(1.0, 1.0));
        self.tri(a, b, d, col);
        self.tri(b, e, d, col);
    }

    /// A line with width; round enough at small sizes.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, col: [f32; 4]) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-4);
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        let p = [
            [a[0] + nx, a[1] + ny],
            [a[0] - nx, a[1] - ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
        ];
        self.tri(p[0], p[1], p[2], col);
        self.tri(p[1], p[3], p[2], col);
    }

    /// A heart sprite: two round lobes and a point, turned by `rot`, with a little shine.
    pub fn heart(&mut self, x: f32, y: f32, s: f32, rot: f32, col: [f32; 4]) {
        let (sn, cs) = rot.sin_cos();
        let at = |dx: f32, dy: f32| [x + dx * cs - dy * sn, y + dx * sn + dy * cs];
        for sx in [-0.5f32, 0.5] {
            let c = at(sx * s, -0.25 * s);
            self.disc(c[0], c[1], 0.56 * s, col);
        }
        self.tri(
            at(-1.04 * s, -0.08 * s),
            at(1.04 * s, -0.08 * s),
            at(0.0, 1.12 * s),
            col,
        );
        let h = at(-0.55 * s, -0.4 * s);
        self.disc(h[0], h[1], 0.16 * s, [1.0, 1.0, 1.0, col[3] * 0.7]);
    }
}
