use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    #[default]
    Snow,
    Leaves,
    Blossom,
    Rain,
    Flowers,
}

impl Effect {
    pub const ALL: [Effect; 5] = [Effect::Snow, Effect::Leaves, Effect::Blossom, Effect::Rain, Effect::Flowers];
    pub fn name(self) -> &'static str {
        match self {
            Effect::Snow => "snow",
            Effect::Leaves => "leaves",
            Effect::Blossom => "blossom",
            Effect::Rain => "rain",
            Effect::Flowers => "flowers",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Effect::Snow => "Snow",
            Effect::Leaves => "Autumn Leaves",
            Effect::Blossom => "Cherry Blossom",
            Effect::Rain => "Rain",
            Effect::Flowers => "Flowers",
        }
    }
    pub fn parse(s: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|e| e.name() == s.to_ascii_lowercase())
    }
    /// Whether what falls piles up on window tops.
    pub fn accumulates(self) -> bool {
        matches!(self, Effect::Snow | Effect::Leaves | Effect::Blossom)
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
        Renderer::ALL.into_iter().find(|r| r.name() == s.to_ascii_lowercase())
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
        BarMode::ALL.into_iter().find(|m| m.name() == s.to_ascii_lowercase())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub effect: Effect,
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
        if crate::palette::parse_hex(&self.glow_color).is_none() {
            self.glow_color = "#ffd84d".into();
        }
    }

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
            let old = path.parent().and_then(|d| d.parent()).map(|d| d.join("snowhush").join("config.toml"));
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
        set!(enabled, effect, renderer, intensity, opacity, pixel_size, pixel_levels, melt, ground, windows, layers, bar_mode, over_fullscreen, interact, wind);
        self.clamp();
        if let Some(g) = &p.glow_color {
            if crate::palette::parse_hex(g).is_some() {
                self.glow_color = g.to_ascii_lowercase();
            }
        }
        *self != before
    }
}
