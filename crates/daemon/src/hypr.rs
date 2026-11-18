//! Hyprland: where windows, bars and the pointer are, read from its IPC sockets.
//! A poller thread turns that into per-monitor `Scene`s and sends them to the main loop.

use serde::Deserialize;
use snowhush_core::geom::Rect;
use snowhush_core::sim::{Scene, Surf, SurfKind};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn socket(name: &str) -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    Some(PathBuf::from(runtime).join("hypr").join(sig).join(name))
}

pub fn available() -> bool {
    socket(".socket.sock").is_some_and(|p| p.exists())
}

fn query(cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(socket(".socket.sock")?).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn json<T: for<'de> Deserialize<'de>>(cmd: &str) -> Option<T> {
    serde_json::from_str(&query(cmd)?).ok()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WsRef {
    id: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Monitor {
    id: i64,
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    transform: i64,
    reserved: [f64; 4],
    #[serde(rename = "activeWorkspace")]
    active: WsRef,
    #[serde(rename = "specialWorkspace")]
    special: WsRef,
}

impl Monitor {
    fn logical(&self) -> (f32, f32) {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        let (w, h) = if self.transform % 2 == 1 { (self.height, self.width) } else { (self.width, self.height) };
        ((w / s) as f32, (h / s) as f32)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Client {
    address: String,
    at: [f64; 2],
    size: [f64; 2],
    workspace: WsRef,
    monitor: i64,
    mapped: bool,
    hidden: bool,
    floating: bool,
    fullscreen: i64,
    #[serde(rename = "focusHistoryID")]
    focus: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LayerInfo {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    namespace: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LayerMonitor {
    levels: std::collections::HashMap<String, Vec<LayerInfo>>,
}

#[derive(Deserialize)]
struct IntOption {
    int: Option<i64>,
}

fn hash(s: &str, salt: u64) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    salt.hash(&mut h);
    h.finish() | 1
}

pub struct MonScene {
    pub name: String,
    pub scene: Scene,
    /// A fullscreen window covers this monitor.
    pub fullscreen: bool,
}

pub struct Snapshot {
    pub monitors: Vec<MonScene>,
}

fn ignored_layer(ns: &str) -> bool {
    let ns = ns.to_ascii_lowercase();
    ["snowhush", "wallpaper", "awww", "swww", "hyprpaper", "swaybg", "wpaperd", "mpvpaper", "selection", "hyprpicker", "wlogout", "lockscreen", "hyprlock", "rofi", "wofi", "fuzzel", "notification", "swaync", "mako", "dunst"]
        .iter()
        .any(|bad| ns.contains(bad))
}

struct Style {
    rounding: f32,
    border: f32,
}

fn style() -> Style {
    let get = |name: &str, default: f32| json::<IntOption>(&format!("j/getoption {name}")).and_then(|o| o.int).map_or(default, |v| v as f32);
    Style { rounding: get("decoration:rounding", 0.0), border: get("general:border_size", 1.0) }
}

#[derive(Default)]
struct Cache {
    tick: u64,
    monitors: Vec<Monitor>,
    clients: Vec<Client>,
    layers: std::collections::HashMap<String, LayerMonitor>,
}

fn snapshot(style_cache: &Style, cache: &mut Cache) -> Option<(Snapshot, Option<[f32; 2]>)> {
    // The pointer is read every tick; the window list a few times a second less.
    if cache.tick % 3 == 0 || cache.monitors.is_empty() {
        cache.monitors = json("j/monitors")?;
        cache.clients = json("j/clients").unwrap_or_default();
    }
    if cache.tick % 20 == 0 {
        cache.layers = json("j/layers").unwrap_or_default();
    }
    cache.tick += 1;
    let (monitors, clients, layers) = (&cache.monitors, &cache.clients, &cache.layers);
    let cursor: Option<[f64; 2]> = json::<std::collections::HashMap<String, f64>>("j/cursorpos").map(|m| [m.get("x").copied().unwrap_or(0.0), m.get("y").copied().unwrap_or(0.0)]);

    let mut out = Vec::new();
    let mut global_cursor = None;
    for m in monitors {
        let (w, h) = m.logical();
        let mut surfs = Vec::new();
        let bottom = m.reserved[3] as f32;
        surfs.push(Surf { id: 1, rect: Rect::new(0.0, h - bottom, w, 4000.0), z: -1, kind: SurfKind::Ground, inset: 0.0 });
        let mut fullscreen = false;
        for c in clients {
            if !c.mapped || c.hidden || c.monitor != m.id {
                continue;
            }
            let on_screen = c.workspace.id == m.active.id || (m.special.id != 0 && c.workspace.id == m.special.id);
            if !on_screen {
                continue;
            }
            if c.fullscreen > 0 {
                fullscreen = true;
                continue;
            }
            let (x, y) = (c.at[0] as f32 - m.x as f32, c.at[1] as f32 - m.y as f32);
            if y < 20.0 {
                continue;
            }
            // The window's border sits outside its geometry; snow rests on the border.
            let z = if c.floating { 500 - c.focus.min(400) as i32 } else { 0 };
            surfs.push(Surf {
                id: hash(&c.address, 7),
                rect: Rect::new(x, y - style_cache.border, c.size[0] as f32, c.size[1] as f32),
                z,
                kind: SurfKind::Window,
                inset: style_cache.rounding.min(c.size[0] as f32 / 3.0),
            });
        }
        if let Some(lm) = layers.get(&m.name) {
            for (level, list) in &lm.levels {
                if !matches!(level.as_str(), "1" | "2" | "3") {
                    continue;
                }
                for l in list {
                    if ignored_layer(&l.namespace) {
                        continue;
                    }
                    let (x, y) = (l.x as f32 - m.x as f32, l.y as f32 - m.y as f32);
                    let area = (l.w * l.h) as f32;
                    // Whole-screen layers and bars hugging the top edge can't hold anything.
                    if area > 0.45 * w * h || y < 20.0 || l.w < 24.0 {
                        continue;
                    }
                    // A bar on the screen's bottom edge is already the ground.
                    if (y - (h - bottom)).abs() < 3.0 && l.w as f32 > w * 0.5 {
                        continue;
                    }
                    surfs.push(Surf { id: hash(&format!("{}{}{}{}", l.namespace, l.x, l.y, l.w), 11), rect: Rect::new(x, y, l.w as f32, l.h as f32), z: 1000, kind: SurfKind::Layer, inset: 4.0 });
                }
            }
        }
        let mut local = None;
        if let Some(c) = cursor {
            let (cx, cy) = (c[0] as f32 - m.x as f32, c[1] as f32 - m.y as f32);
            if cx >= 0.0 && cy >= 0.0 && cx < w && cy < h {
                local = Some([cx, cy]);
                global_cursor = local;
            }
        }
        out.push(MonScene { name: m.name.clone(), scene: Scene { w, h, surfs, cursor: local }, fullscreen });
    }
    Some((Snapshot { monitors: out }, global_cursor))
}

/// Polls Hyprland on a thread and sends a snapshot whenever it changed.
pub fn spawn(mut send: impl FnMut(Snapshot) -> bool + Send + 'static, awake: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    std::thread::Builder::new()
        .name("hyprland".into())
        .spawn(move || {
            let mut style = style();
            let mut cache = Cache::default();
            let mut last_style = Instant::now();
            let mut last: Option<Vec<(String, Vec<Surf>, Option<[f32; 2]>, bool)>> = None;
            loop {
                if !awake.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(300));
                    continue;
                }
                if last_style.elapsed() > Duration::from_secs(10) {
                    style = self::style();
                    last_style = Instant::now();
                }
                if let Some((snap, _)) = snapshot(&style, &mut cache) {
                    let key: Vec<_> = snap.monitors.iter().map(|m| (m.name.clone(), m.scene.surfs.clone(), m.scene.cursor, m.fullscreen)).collect();
                    if last.as_ref() != Some(&key) {
                        last = Some(key);
                        if !send(snap) {
                            return;
                        }
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(500));
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        })
        .expect("spawn hyprland thread");
}
