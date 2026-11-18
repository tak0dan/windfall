pub type Rgb = [f32; 3];

pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn hex(c: Rgb) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// `#rrggbb` (or `rrggbb`) to 0..1 channels.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0])
}

pub const SNOW: &[Rgb] = &[[1.0, 1.0, 1.0], [0.92, 0.96, 1.0], [0.85, 0.92, 1.0]];
pub const LEAVES: &[Rgb] = &[[0.80, 0.33, 0.08], [0.92, 0.62, 0.10], [0.66, 0.18, 0.10], [0.55, 0.42, 0.08], [0.85, 0.45, 0.12]];
pub const BLOSSOM: &[Rgb] = &[[1.0, 0.72, 0.80], [0.98, 0.58, 0.70], [1.0, 0.85, 0.90], [0.93, 0.50, 0.66]];
pub const RAIN: &[Rgb] = &[[0.62, 0.75, 0.92], [0.72, 0.82, 0.95]];
pub const STEM: Rgb = [0.25, 0.62, 0.30];

/// The GUI's look, derived from the running effect.
#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Rgb,
    pub surface: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub accent_fg: Rgb,
    pub border: Rgb,
}

pub fn theme(effect: crate::config::Effect) -> Theme {
    use crate::config::Effect::*;
    let c = |h: u32| [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0];
    match effect {
        Snow => Theme { bg: c(0x0f1b2b), surface: c(0x172a40), fg: c(0xe8f3ff), muted: c(0x8fa9c4), accent: c(0x8fd3ff), accent_fg: c(0x08243a), border: c(0x2a4666) },
        Leaves => Theme { bg: c(0x24160d), surface: c(0x352012), fg: c(0xfbe9d0), muted: c(0xc09a76), accent: c(0xf0a030), accent_fg: c(0x2a1500), border: c(0x5a3a22) },
        Blossom => Theme { bg: c(0x2a1420), surface: c(0x3d1f2e), fg: c(0xffe8f0), muted: c(0xcf9bb0), accent: c(0xff8fb1), accent_fg: c(0x3a0a1f), border: c(0x68304a) },
        Rain => Theme { bg: c(0x10161e), surface: c(0x1a232e), fg: c(0xdbe7f3), muted: c(0x7f93a8), accent: c(0x6fa8e8), accent_fg: c(0x06182c), border: c(0x2c3a4a) },
        Flowers => Theme { bg: c(0x102016), surface: c(0x1a3322), fg: c(0xeafbe8), muted: c(0x8fbf98), accent: c(0xffd23f), accent_fg: c(0x2b2200), border: c(0x2f5a3a) },
    }
}
