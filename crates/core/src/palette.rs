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
pub const COMETS: &[Rgb] = &[[0.85, 0.93, 1.0], [0.6, 0.8, 1.0], [1.0, 1.0, 1.0]];
pub const LIGHTS: &[Rgb] = &[[1.0, 0.84, 0.38], [1.0, 0.72, 0.28], [1.0, 0.92, 0.6], [1.0, 0.78, 0.45]];
pub const HEARTS: &[Rgb] = &[[1.0, 0.32, 0.5], [1.0, 0.5, 0.68], [0.9, 0.2, 0.4], [1.0, 0.7, 0.8]];
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
        Comets => Theme { bg: c(0x0a1024), surface: c(0x131b38), fg: c(0xe6eeff), muted: c(0x8a9bc8), accent: c(0x7fb0ff), accent_fg: c(0x061232), border: c(0x26335e) },
        Lights => Theme { bg: c(0x1d1408), surface: c(0x2d2010), fg: c(0xfff1d0), muted: c(0xc9a66a), accent: c(0xffc94a), accent_fg: c(0x2b1d00), border: c(0x54401e) },
        Hearts => Theme { bg: c(0x240c16), surface: c(0x36131f), fg: c(0xffe6ee), muted: c(0xd09aac), accent: c(0xff5f86), accent_fg: c(0x3a0816), border: c(0x6b2638) },
        Flowers => Theme { bg: c(0x102016), surface: c(0x1a3322), fg: c(0xeafbe8), muted: c(0x8fbf98), accent: c(0xffd23f), accent_fg: c(0x2b2200), border: c(0x2f5a3a) },
    }
}

/// The accent the controller takes on while the flowers are made of something other than nature.
pub fn element_accent(e: crate::config::Element) -> Option<Rgb> {
    use crate::config::Element::*;
    let c = |h: u32| [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0];
    match e {
        Natural => None,
        Lightning => Some(c(0xc08bff)),
        Fire => Some(c(0xff7a2a)),
        Ice => Some(c(0x7fdcff)),
        Matter => Some(c(0xd9a441)),
        Void => Some(c(0x6f5de0)),
    }
}
