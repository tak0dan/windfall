//! A small software rasterizer for the batches of triangles and ellipses the simulation produces. It draws tiny
//! shapes much faster than a general path renderer and writes straight into a BGRA, premultiplied-alpha buffer
//! (what Windows layered windows want).
#![allow(dead_code)]

use windfall_core::draw::FLOATS_PER_VERTEX;

pub struct Raster<'a> {
    pub buf: &'a mut [u8],
    pub w: i32,
    pub h: i32,
    /// Anti-aliasing (2x2 samples for triangles, soft edges for ellipses); off for pixel art.
    pub aa: bool,
    /// Multiplies every shape's alpha.
    pub opacity: f32,
}

const SS: [(f32, f32); 4] = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)];

impl Raster<'_> {
    #[inline]
    fn blend(&mut self, x: i32, y: i32, col: [f32; 4], cov: f32) {
        let a = col[3] * cov * self.opacity;
        if a <= 0.0 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let d = &mut self.buf[i..i + 4];
        let ia = 1.0 - a;
        d[0] = (col[2] * a * 255.0 + d[0] as f32 * ia) as u8;
        d[1] = (col[1] * a * 255.0 + d[1] as f32 * ia) as u8;
        d[2] = (col[0] * a * 255.0 + d[2] as f32 * ia) as u8;
        d[3] = (a * 255.0 + d[3] as f32 * ia) as u8;
    }

    fn bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<(i32, i32, i32, i32)> {
        let (minx, maxx) = (
            (x0.floor() as i32).max(0),
            (x1.ceil() as i32).min(self.w - 1),
        );
        let (miny, maxy) = (
            (y0.floor() as i32).max(0),
            (y1.ceil() as i32).min(self.h - 1),
        );
        (minx <= maxx && miny <= maxy).then_some((minx, miny, maxx, maxy))
    }

    pub fn triangle(&mut self, p: [[f32; 2]; 3], col: [f32; 4]) {
        let (p0, p1, p2) = (p[0], p[1], p[2]);
        let area = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
        if area.abs() < 1e-6 {
            return;
        }
        let sg = area.signum();
        let Some((minx, miny, maxx, maxy)) = self.bounds(
            p0[0].min(p1[0]).min(p2[0]),
            p0[1].min(p1[1]).min(p2[1]),
            p0[0].max(p1[0]).max(p2[0]),
            p0[1].max(p1[1]).max(p2[1]),
        ) else {
            return;
        };
        let inside = |qx: f32, qy: f32| {
            let e0 = ((p1[0] - p0[0]) * (qy - p0[1]) - (p1[1] - p0[1]) * (qx - p0[0])) * sg;
            let e1 = ((p2[0] - p1[0]) * (qy - p1[1]) - (p2[1] - p1[1]) * (qx - p1[0])) * sg;
            let e2 = ((p0[0] - p2[0]) * (qy - p2[1]) - (p0[1] - p2[1]) * (qx - p2[0])) * sg;
            e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0
        };
        for y in miny..=maxy {
            for x in minx..=maxx {
                let cov = if self.aa {
                    SS.iter()
                        .filter(|(sx, sy)| inside(x as f32 + sx, y as f32 + sy))
                        .count() as f32
                        / 4.0
                } else if inside(x as f32 + 0.5, y as f32 + 0.5) {
                    1.0
                } else {
                    0.0
                };
                if cov > 0.0 {
                    self.blend(x, y, col, cov);
                }
            }
        }
    }

    /// An ellipse given by its centre and two half-axis vectors.
    pub fn ellipse(&mut self, c: [f32; 2], ex: [f32; 2], ey: [f32; 2], col: [f32; 4]) {
        let det = ex[0] * ey[1] - ey[0] * ex[1];
        if det.abs() < 1e-6 {
            return;
        }
        let (rx, ry) = (ex[0].abs() + ey[0].abs(), ex[1].abs() + ey[1].abs());
        let Some((minx, miny, maxx, maxy)) =
            self.bounds(c[0] - rx, c[1] - ry, c[0] + rx, c[1] + ry)
        else {
            return;
        };
        let reff = det.abs().sqrt();
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (dx, dy) = (x as f32 + 0.5 - c[0], y as f32 + 0.5 - c[1]);
                let u = (ey[1] * dx - ey[0] * dy) / det;
                let v = (-ex[1] * dx + ex[0] * dy) / det;
                let d = (u * u + v * v).sqrt();
                let cov = if self.aa {
                    ((1.0 - d) * reff + 0.5).clamp(0.0, 1.0)
                } else if d <= 1.0 {
                    1.0
                } else {
                    0.0
                };
                if cov > 0.0 {
                    self.blend(x, y, col, cov);
                }
            }
        }
    }

    /// Replays a batch: ellipse quads (recognised by their uv corners) are drawn as ellipses, everything else as
    /// flat triangles. `sx`/`sy` take simulation (logical) coordinates to this buffer's pixels.
    pub fn batch(&mut self, data: &[f32], sx: f32, sy: f32) {
        const V: usize = FLOATS_PER_VERTEX;
        let n = data.len() / V;
        let vert = |k: usize| &data[k * V..k * V + V];
        let mut i = 0;
        while i + 3 <= n {
            let a = vert(i);
            if i + 6 <= n && (a[2] != 0.0 || a[3] != 0.0) {
                let (v0, v1, v2) = (vert(i), vert(i + 1), vert(i + 2));
                let ex = [(v1[0] - v0[0]) / 2.0 * sx, (v1[1] - v0[1]) / 2.0 * sy];
                let ey = [(v2[0] - v0[0]) / 2.0 * sx, (v2[1] - v0[1]) / 2.0 * sy];
                let c = [v0[0] * sx + ex[0] + ey[0], v0[1] * sy + ex[1] + ey[1]];
                self.ellipse(c, ex, ey, [v0[4], v0[5], v0[6], v0[7].clamp(0.0, 1.0)]);
                i += 6;
                continue;
            }
            let (b, c) = (vert(i + 1), vert(i + 2));
            let avg = |k: usize| ((a[k] + b[k] + c[k]) / 3.0).clamp(0.0, 1.0);
            self.triangle(
                [
                    [a[0] * sx, a[1] * sy],
                    [b[0] * sx, b[1] * sy],
                    [c[0] * sx, c[1] * sy],
                ],
                [avg(4), avg(5), avg(6), avg(7)],
            );
            i += 3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(buf: &[u8], w: i32, x: i32, y: i32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn a_triangle_fills_its_inside_only() {
        let mut buf = vec![0u8; 20 * 20 * 4];
        let mut r = Raster {
            buf: &mut buf,
            w: 20,
            h: 20,
            aa: false,
            opacity: 1.0,
        };
        r.triangle([[2.0, 2.0], [18.0, 2.0], [2.0, 18.0]], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(px(&buf, 20, 4, 4), [0, 0, 255, 255]); // BGRA: red
        assert_eq!(px(&buf, 20, 17, 17), [0, 0, 0, 0]);
    }

    #[test]
    fn an_ellipse_is_round_and_soft_at_the_edge() {
        let mut buf = vec![0u8; 40 * 40 * 4];
        let mut r = Raster {
            buf: &mut buf,
            w: 40,
            h: 40,
            aa: true,
            opacity: 1.0,
        };
        r.ellipse([20.0, 20.0], [10.3, 0.0], [0.0, 10.3], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(px(&buf, 40, 20, 20)[3], 255);
        assert_eq!(px(&buf, 40, 2, 2)[3], 0);
        let edge = px(&buf, 40, 30, 20)[3];
        assert!(edge > 0 && edge < 255, "edge alpha {edge}");
    }

    #[test]
    fn opacity_scales_alpha_and_premultiplies() {
        let mut buf = vec![0u8; 4 * 4 * 4];
        let mut r = Raster {
            buf: &mut buf,
            w: 4,
            h: 4,
            aa: false,
            opacity: 0.5,
        };
        r.triangle([[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]], [1.0, 1.0, 1.0, 1.0]);
        let p = px(&buf, 4, 0, 0);
        assert!((p[3] as i32 - 127).abs() <= 1 && (p[0] as i32 - 127).abs() <= 1);
    }
}
