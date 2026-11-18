//! Finds the painted parts of a bar from a picture of it.
//!
//! Shells draw their bars in very different ways: one solid strip, a transparent strip with framed modules,
//! floating pills. A layer surface only reports its rectangle, so we look at the pixels: a module shows up as
//! a long, level edge near the top of the strip, while a transparent gap shows the wallpaper and has none.

/// One painted stretch, relative to the layer surface's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub x0: f32,
    pub x1: f32,
    /// Distance from the top of the layer to the top of the painted part.
    pub top: f32,
}

/// A strip of pixels, rows top to bottom, as bytes B G R A (Wayland's little-endian ARGB8888/XRGB8888).
pub struct Strip<'a> {
    pub data: &'a [u8],
    pub w: usize,
    pub h: usize,
    pub stride: usize,
}

impl Strip<'_> {
    fn px(&self, x: usize, y: usize) -> [i32; 3] {
        let i = y * self.stride + x * 4;
        [self.data[i] as i32, self.data[i + 1] as i32, self.data[i + 2] as i32]
    }
}

fn diff(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs()).max((a[2] - b[2]).abs())
}

const EDGE: i32 = 26;
const MIN_RUN: usize = 14;

pub fn segments(s: &Strip) -> Vec<Segment> {
    if s.w < MIN_RUN || s.h < 4 {
        return Vec::new();
    }
    // For each column, where does the first strong horizontal edge sit?
    let depth = s.h.min(24);
    // Edges everywhere means a busy picture, not a bar: nothing reliable to grow on.
    let hits = (0..s.w).map(|x| (2..depth).filter(|&r| diff(s.px(x, r), s.px(x, r - 2)) >= EDGE).count()).sum::<usize>();
    if hits * 10 > s.w * (depth - 2) * 3 {
        return Vec::new();
    }
    let top_of: Vec<Option<usize>> = (0..s.w)
        .map(|x| (2..depth).find(|&r| diff(s.px(x, r), s.px(x, r - 2)) >= EDGE).map(|r| r - 1))
        .collect();
    // Join neighbouring columns whose edges sit at about the same height. A module's top edge is level and
    // nearly unbroken; the tops of letters are not, so text on a solid bar does not count.
    let mut out = Vec::new();
    let mut x = 0;
    while x < s.w {
        let Some(t0) = top_of[x] else {
            x += 1;
            continue;
        };
        let start = x;
        let mut tops = vec![t0];
        let mut last = x;
        let mut cx = x + 1;
        while cx < s.w && cx - last <= 2 {
            if let Some(t) = top_of[cx] {
                if (t as i32 - *tops.last().unwrap() as i32).abs() <= 2 {
                    tops.push(t);
                    last = cx;
                }
            }
            cx += 1;
        }
        let end = last + 1;
        let len = end - start;
        if len >= MIN_RUN && tops.len() * 10 >= len * 8 {
            tops.sort_unstable();
            let top = tops[tops.len() / 2];
            // A module has sides: wallpaper that merely has a level edge somewhere does not.
            if top <= 10 && closed_ends(s, start, end, top) {
                // Rounded corners: keep a little in from both ends.
                let trim = 3.0f32.min(len as f32 / 4.0);
                out.push(Segment { x0: start as f32 + trim, x1: end as f32 - trim, top: top as f32 });
            }
        }
        x = end;
    }
    if !out.is_empty() {
        return out;
    }
    // No modules: either one solid strip (flat along its top rows), which is painted end to end, or bare
    // wallpaper with nothing on it.
    let rows = 3.min(s.h);
    let flat = (0..rows).all(|r| {
        let base = s.px(s.w / 2, r);
        (0..s.w).filter(|&x| diff(s.px(x, r), base) > 14).count() * 20 < s.w
    });
    if flat {
        vec![Segment { x0: 0.0, x1: s.w as f32, top: 0.0 }]
    } else {
        Vec::new()
    }
}

/// Is there a vertical edge at both ends of the run, just below its top? (Ends at the strip's own edge count.)
fn closed_ends(s: &Strip, start: usize, end: usize, top: usize) -> bool {
    let side = |inside: usize, outside: Option<usize>| -> bool {
        let Some(out) = outside else { return true };
        let rows = (top + 2)..(top + 9).min(s.h);
        let total = rows.len();
        let hits = rows.filter(|&r| diff(s.px(inside, r), s.px(out, r)) >= EDGE).count();
        total == 0 || hits * 2 >= total.min(4)
    };
    let left_out = (start >= 2).then(|| start - 2);
    let right_out = (end + 2 <= s.w).then(|| end + 1).filter(|&x| x < s.w);
    side((start + 1).min(s.w - 1), left_out) && side(end.saturating_sub(2), right_out)
}
