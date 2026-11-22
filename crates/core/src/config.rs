use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Nothing is drawn: the app just runs in the background (the floating lights can still be on).
    Off,
    #[default]
    Snow,
    Leaves,
    Blossom,
    Rain,
    Flowers,
    /// Shooting stars that burst into blue sparks where they hit something.
    Comets,
    /// Legacy: the old stand-alone lights effect. Lights are a switch now; this reads as Off with lights on.
    Lights,
    /// Hearts falling like stars, bursting into pink sparks.
    Hearts,
}

impl Effect {
    pub const ALL: [Effect; 8] = [
        Effect::Off,
        Effect::Snow,
        Effect::Leaves,
        Effect::Blossom,
        Effect::Rain,
        Effect::Flowers,
        Effect::Comets,
        Effect::Hearts,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Effect::Off => "off",
            Effect::Snow => "snow",
            Effect::Leaves => "leaves",
            Effect::Blossom => "blossom",
            Effect::Rain => "rain",
            Effect::Flowers => "flowers",
            Effect::Comets => "comets",
            Effect::Lights => "lights",
            Effect::Hearts => "hearts",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Effect::Off => "Off",
            Effect::Snow => "Snow",
            Effect::Leaves => "Autumn Leaves",
            Effect::Blossom => "Cherry Blossom",
            Effect::Rain => "Rain",
            Effect::Flowers => "Flowers",
            Effect::Comets => "Shooting stars",
            Effect::Lights => "Warm lights",
            Effect::Hearts => "Falling hearts",
        }
    }
    pub fn parse(s: &str) -> Option<Effect> {
        let s = s.to_ascii_lowercase();
        if s == "lights" {
            return Some(Effect::Lights);
        }
        Effect::ALL.into_iter().find(|e| e.name() == s)
    }
    /// Whether what falls piles up on window tops.
    pub fn accumulates(self) -> bool {
        matches!(self, Effect::Snow | Effect::Leaves | Effect::Blossom)
    }
}

/// What the flowers are made of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    #[default]
    Natural,
    Lightning,
    Fire,
    Ice,
    Matter,
    Void,
}

impl Element {
    pub const ALL: [Element; 6] = [
        Element::Natural,
        Element::Lightning,
        Element::Fire,
        Element::Ice,
        Element::Matter,
        Element::Void,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Element::Natural => "natural",
            Element::Lightning => "lightning",
            Element::Fire => "fire",
            Element::Ice => "ice",
            Element::Matter => "matter",
            Element::Void => "void",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Element::Natural => "Natural",
            Element::Lightning => "Lightning",
            Element::Fire => "Fire",
            Element::Ice => "Ice",
            Element::Matter => "Matter",
            Element::Void => "Void",
        }
    }
    pub fn parse(s: &str) -> Option<Element> {
        Element::ALL
            .into_iter()
            .find(|e| e.name() == s.to_ascii_lowercase())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renderer {
    /// Soft anti-aliased shapes at native resolution.
    #[default]
    Smooth,
    /// Drawn on a coarse grid with a limited palette and scaled up with hard edges.
    Pixel,
}

impl Renderer {
    pub const ALL: [Renderer; 2] = [Renderer::Smooth, Renderer::Pixel];
    pub fn name(self) -> &'static str {
        match self {
            Renderer::Smooth => "smooth",
            Renderer::Pixel => "pixel",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Renderer::Smooth => "Smooth",
            Renderer::Pixel => "Pixel art",
        }
    }
    pub fn parse(s: &str) -> Option<Renderer> {
        Renderer::ALL
            .into_iter()
            .find(|r| r.name() == s.to_ascii_lowercase())
    }
}

/// How bars and panels (Waybar, Quickshell...) hold things.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarMode {
    /// Only the painted parts: framed modules, pills, a solid strip. Transparent gaps stay bare.
    #[default]
    Visible,
    /// The whole rectangle of the bar.
    Whole,
}

impl BarMode {
    pub const ALL: [BarMode; 2] = [BarMode::Visible, BarMode::Whole];
    pub fn name(self) -> &'static str {
        match self {
            BarMode::Visible => "visible",
            BarMode::Whole => "whole",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            BarMode::Visible => "Painted parts only",
            BarMode::Whole => "Whole bar",
        }
    }
    pub fn parse(s: &str) -> Option<BarMode> {
        BarMode::ALL
            .into_iter()
            .find(|m| m.name() == s.to_ascii_lowercase())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub effect: Effect,
    /// What the flowers are made of (only for the flowers effect).
    pub element: Element,
    pub renderer: Renderer,
    /// How much falls / grows, 0..=1.
    pub intensity: f32,
    /// Overall opacity, 0.1..=1.
    pub opacity: f32,
    /// Size of one "pixel" in the pixel renderer, 1..=10 screen pixels.
    pub pixel_size: u32,
    /// Colour steps per channel in the pixel renderer, 2..=16.
    pub pixel_levels: u32,
    /// Piles slowly thaw/wither.
    pub melt: bool,
    /// Collect on the bottom of the screen.
    pub ground: bool,
    /// Collect on window tops.
    pub windows: bool,
    /// Collect on bars and panels (Waybar, Quickshell...).
    pub layers: bool,
    /// Which part of a bar collects things.
    pub bar_mode: BarMode,
    /// Soft floating lights over any effect (or over Off), in that effect's colours.
    pub lights: bool,
    /// Colour of the lights when the effect is Off (`#rrggbb`).
    pub lights_color: String,
    /// Colour of the hearts.
    pub hearts_color: String,
    /// Colour of the shooting stars' tails and their sparks.
    pub comet_color: String,
    /// The flowers all take one chosen colour (their shapes stay random).
    pub flower_single: bool,
    /// That colour.
    pub flower_color: String,
    /// Keep drawing above fullscreen windows.
    pub over_fullscreen: bool,
    /// React to the cursor.
    pub interact: bool,
    /// Sideways drift, -1..=1.
    pub wind: f32,
    /// Colour of the flowers' glow and of the sparks when one gives way to a new one (`#rrggbb`).
    pub glow_color: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            effect: Effect::Snow,
            element: Element::Natural,
            renderer: Renderer::Smooth,
            intensity: 0.5,
            opacity: 0.9,
            pixel_size: 2,
            pixel_levels: 8,
            melt: true,
            ground: true,
            windows: true,
            layers: true,
            bar_mode: BarMode::Visible,
            lights: false,
            lights_color: "#ffd84d".into(),
            hearts_color: "#ff7399".into(),
            comet_color: "#a6d1ff".into(),
            flower_single: false,
            flower_color: "#ff7bb0".into(),
            over_fullscreen: false,
            interact: true,
            wind: 0.0,
            glow_color: "#ffd84d".into(),
        }
    }
}

impl Config {
    pub fn clamp(&mut self) {
        self.intensity = self.intensity.clamp(0.0, 1.0);
        self.opacity = self.opacity.clamp(0.1, 1.0);
        self.pixel_size = self.pixel_size.clamp(1, 10);
        self.pixel_levels = self.pixel_levels.clamp(2, 16);
        self.wind = self.wind.clamp(-1.0, 1.0);
        if self.effect == Effect::Lights {
            self.effect = Effect::Off;
            self.lights = true;
        }
        let d = Config::default();
        for (field, default) in [
            (&mut self.lights_color, d.lights_color),
            (&mut self.hearts_color, d.hearts_color),
            (&mut self.comet_color, d.comet_color),
            (&mut self.flower_color, d.flower_color),
        ] {
            if crate::palette::parse_hex(field).is_none() {
                *field = default;
            }
        }
        if crate::palette::parse_hex(&self.glow_color).is_none() {
            self.glow_color = "#ffd84d".into();
        }
    }

    #[cfg(windows)]
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("windfall").join("config.toml")
    }

    #[cfg(not(windows))]
    pub fn path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("windfall").join("config.toml")
    }

    pub fn load() -> Config {
        // Settings from the working-title days live in ~/.config/snowhush; take them over once.
        let path = Config::path();
        if !path.exists() {
            let old = path
                .parent()
                .and_then(|d| d.parent())
                .map(|d| d.join("snowhush").join("config.toml"));
            if let Some(old) = old.filter(|o| o.exists()) {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::copy(old, &path);
            }
        }
        let mut c: Config = std::fs::read_to_string(Config::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        c.clamp();
        c
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Config::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

/// A partial update; only the present fields change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Patch {
    pub enabled: Option<bool>,
    pub effect: Option<Effect>,
    pub element: Option<Element>,
    pub renderer: Option<Renderer>,
    pub intensity: Option<f32>,
    pub opacity: Option<f32>,
    pub pixel_size: Option<u32>,
    pub pixel_levels: Option<u32>,
    pub melt: Option<bool>,
    pub ground: Option<bool>,
    pub windows: Option<bool>,
    pub layers: Option<bool>,
    pub bar_mode: Option<BarMode>,
    pub lights: Option<bool>,
    pub lights_color: Option<String>,
    pub hearts_color: Option<String>,
    pub comet_color: Option<String>,
    pub flower_single: Option<bool>,
    pub flower_color: Option<String>,
    pub over_fullscreen: Option<bool>,
    pub interact: Option<bool>,
    pub wind: Option<f32>,
    pub glow_color: Option<String>,
}

impl Config {
    /// Applies the patch; returns whether anything changed.
    pub fn apply(&mut self, p: &Patch) -> bool {
        let before = self.clone();
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = p.$f { self.$f = v; } )* };
        }
        set!(
            enabled,
            effect,
            element,
            renderer,
            intensity,
            opacity,
            pixel_size,
            pixel_levels,
            melt,
            ground,
            windows,
            layers,
            bar_mode,
            lights,
            flower_single,
            over_fullscreen,
            interact,
            wind
        );
        self.clamp();
        for (new, field) in [
            (&p.lights_color, &mut self.lights_color),
            (&p.hearts_color, &mut self.hearts_color),
            (&p.comet_color, &mut self.comet_color),
            (&p.flower_color, &mut self.flower_color),
        ] {
            if let Some(c) = new {
                if crate::palette::parse_hex(c).is_some() {
                    *field = c.to_ascii_lowercase();
                }
            }
        }
        if self.effect == Effect::Lights {
            // The old stand-alone lights effect: now Off with the lights switched on.
            self.effect = Effect::Off;
            self.lights = true;
        }
        if let Some(g) = &p.glow_color {
            if crate::palette::parse_hex(g).is_some() {
                self.glow_color = g.to_ascii_lowercase();
            }
        }
        *self != before
    }
}

/// The colour the flowers glow in: the one picked, unless it is still the default and the flowers are made of
/// an element, which then brings its own.
pub fn glow_for(c: &Config) -> crate::palette::Rgb {
    let picked = crate::palette::parse_hex(&c.glow_color).unwrap_or([1.0, 0.85, 0.3]);
    if c.glow_color
        .eq_ignore_ascii_case(&Config::default().glow_color)
    {
        if let Some(e) = crate::palette::element_accent(c.element) {
            return e;
        }
    }
    picked
}

impl Config {
    /// Is anything being drawn at all? Off with no lights means the app idles without an overlay.
    pub fn drawing(&self) -> bool {
        self.enabled && (self.effect != Effect::Off || self.lights)
    }
}

/// The colours the simulation needs, resolved from the settings: (lights, hearts, comets, single flower colour).
pub fn colors_of(
    c: &Config,
) -> (
    crate::palette::Rgb,
    crate::palette::Rgb,
    crate::palette::Rgb,
    Option<crate::palette::Rgb>,
) {
    let rgb = |s: &str, d: &str| {
        crate::palette::parse_hex(s)
            .or_else(|| crate::palette::parse_hex(d))
            .unwrap_or([1.0, 1.0, 1.0])
    };
    let d = Config::default();
    (
        rgb(&c.lights_color, &d.lights_color),
        rgb(&c.hearts_color, &d.hearts_color),
        rgb(&c.comet_color, &d.comet_color),
        c.flower_single
            .then(|| rgb(&c.flower_color, &d.flower_color)),
    )
}
