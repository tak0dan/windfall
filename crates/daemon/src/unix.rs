use crate::{capture, gpu, hypr, msg, server, tray};

use calloop::timer::{TimeoutAction, Timer};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use msg::Msg;
use windfall_core::config::{BarMode, Config, Renderer};
use windfall_core::geom::Rect;
use windfall_core::shape;
use windfall_core::sim::{Surf, SurfKind};
use windfall_core::draw::Batch;
use windfall_core::ipc::{Request, Response, State};
use windfall_core::sim::{Params, Scene, World};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_output, wl_region, wl_registry, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_egl::WlEglSurface;
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, wp_fractional_scale_v1::{self, WpFractionalScaleV1}};
use wayland_protocols::wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

struct Overlay {
    surface: wl_surface::WlSurface,
    layer: ZwlrLayerSurfaceV1,
    viewport: Option<WpViewport>,
    frac: Option<WpFractionalScaleV1>,
    egl_win: Option<WlEglSurface>,
    target: Option<gpu::Target>,
    /// Logical size from the compositor; zero until the first configure.
    size: (u32, u32),
    frac_scale: Option<f32>,
}

struct Mon {
    gname: u32,
    output: wl_output::WlOutput,
    name: Option<String>,
    int_scale: i32,
    done: bool,
    overlay: Option<Overlay>,
    world: World,
}

struct Calibration {
    key: Vec<(u64, [i32; 4])>,
    at: Instant,
    busy: bool,
}

struct App {
    qh: QueueHandle<App>,
    conn: Connection,
    compositor: wl_compositor::WlCompositor,
    layer_shell: ZwlrLayerShellV1,
    viewporter: Option<WpViewporter>,
    frac_mgr: Option<WpFractionalScaleManagerV1>,
    gpu: Option<gpu::Gpu>,
    mons: Vec<Mon>,
    config: Config,
    scenes: HashMap<String, (Scene, bool)>,
    /// What each bar layer turned out to look like; `None` means it could not be examined.
    masks: HashMap<u64, Option<Vec<shape::Segment>>>,
    /// The ids given to a layer's painted parts last time, so a part that merely resized keeps its identity.
    seg_ids: HashMap<u64, Vec<(u64, f32, f32)>>,
    next_seg: u64,
    calibration: HashMap<String, Calibration>,
    /// Bar rectangles per monitor that are kept bare while they are being photographed.
    bare: HashMap<String, Vec<[i32; 4]>>,
    send: calloop::channel::Sender<Msg>,
    batch: Batch,
    last_tick: Instant,
    save_at: Option<Instant>,
    subscribers: Vec<UnixStream>,
    tray: Option<ksni::blocking::Handle<tray::Tray>>,
    awake: Arc<AtomicBool>,
    quit: bool,
    debug: bool,
    debug_at: Instant,
    cal_at: Instant,
}

impl App {
    fn params(&self) -> Params {
        let c = &self.config;
        Params {
            effect: c.effect,
            intensity: c.intensity,
            melt: c.melt,
            interact: c.interact,
            wind: c.wind,
            ground: c.ground,
            windows: c.windows,
            layers: c.layers,
            pixel: c.renderer == Renderer::Pixel,
            pixel_size: if c.renderer == Renderer::Pixel { c.pixel_size as f32 } else { 0.0 },
            glow: windfall_core::config::glow_for(c),
            element: c.element,
            lights: c.lights,
        }
    }

    fn state(&self) -> State {
        State { config: self.config.clone(), version: env!("CARGO_PKG_VERSION").into(), monitors: self.mons.iter().filter_map(|m| m.name.clone()).collect() }
    }

    fn broadcast(&mut self) {
        let mut line = serde_json::to_string(&Response::Ok { state: self.state() }).unwrap_or_default();
        line.push('\n');
        self.subscribers.retain_mut(|s| s.write_all(line.as_bytes()).is_ok());
        if let Some(t) = &self.tray {
            let cfg = self.config.clone();
            t.update(move |tr| tr.config = cfg);
        }
    }

    /// Creates or destroys overlays to match what is wanted right now.
    fn reconcile(&mut self) {
        self.awake.store(self.config.enabled, Ordering::Relaxed);
        for i in 0..self.mons.len() {
            let want = self.config.enabled
                && self.mons[i].done
                && self.mons[i].name.as_ref().and_then(|n| self.scenes.get(n)).is_some_and(|(_, fs)| self.config.over_fullscreen || !fs);
            let has = self.mons[i].overlay.is_some();
            if want && !has {
                self.create_overlay(i);
            } else if !want && has {
                self.destroy_overlay(i);
            }
        }
    }

    fn create_overlay(&mut self, i: usize) {
        let gname = self.mons[i].gname;
        let surface = self.compositor.create_surface(&self.qh, ());
        let layer = self.layer_shell.get_layer_surface(&surface, Some(&self.mons[i].output), Layer::Overlay, "windfall".into(), &self.qh, gname);
        layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        // An empty input region: clicks and the pointer go straight through to what is underneath.
        let region = self.compositor.create_region(&self.qh, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, &self.qh, ()));
        let frac = self.frac_mgr.as_ref().map(|m| m.get_fractional_scale(&surface, &self.qh, gname));
        surface.commit();
        if let Some(n) = self.mons[i].name.clone() {
            self.push_scene(&n);
        }
        self.mons[i].overlay = Some(Overlay { surface, layer, viewport, frac, egl_win: None, target: None, size: (0, 0), frac_scale: None });
    }

    fn destroy_overlay(&mut self, i: usize) {
        let Some(mut ov) = self.mons[i].overlay.take() else { return };
        if let (Some(t), Some(g)) = (ov.target.take(), &self.gpu) {
            g.destroy(t);
        }
        ov.egl_win = None;
        if let Some(f) = ov.frac.take() {
            f.destroy();
        }
        if let Some(v) = ov.viewport.take() {
            v.destroy();
        }
        ov.layer.destroy();
        ov.surface.destroy();
    }

    /// The buffer follows the layer surface's size; (re)creates the EGL window when it changes.
    fn configure(&mut self, gname: u32, w: u32, h: u32) {
        let Some(i) = self.mons.iter().position(|m| m.gname == gname) else { return };
        let int_scale = self.mons[i].int_scale.max(1);
        let Some(ov) = self.mons[i].overlay.as_mut() else { return };
        ov.size = (w, h);
        let scale = ov.frac_scale.unwrap_or(int_scale as f32);
        let (bw, bh) = (((w as f32 * scale).round() as i32).max(1), ((h as f32 * scale).round() as i32).max(1));
        if let Some(v) = &ov.viewport {
            v.set_destination(w as i32, h as i32);
        } else {
            ov.surface.set_buffer_scale(int_scale);
        }
        match &ov.egl_win {
            Some(win) => win.resize(bw, bh, 0, 0),
            None => {
                ov.egl_win = Some(WlEglSurface::new(ov.surface.id(), bw, bh).expect("wl_egl_window"));
                if self.gpu.is_none() {
                    match gpu::Gpu::new(self.conn.backend().display_ptr() as *mut _) {
                        Ok(g) => {
                            eprintln!("windfall: GPU {}", g.renderer);
                            self.gpu = Some(g);
                        }
                        Err(e) => {
                            eprintln!("windfall: no GPU: {e}");
                            return;
                        }
                    }
                }
                let g = self.gpu.as_ref().unwrap();
                match g.create_surface(ov.egl_win.as_ref().unwrap().ptr() as *mut _) {
                    Ok(t) => ov.target = Some(t),
                    Err(e) => eprintln!("windfall: {e}"),
                }
            }
        }
    }

    fn interval(&self) -> Duration {
        if self.mons.iter().all(|m| m.overlay.is_none()) {
            return Duration::from_millis(250);
        }
        match self.config.renderer {
            Renderer::Smooth => Duration::from_micros(16_667),
            Renderer::Pixel => Duration::from_millis(33),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        if let Some(t) = self.save_at {
            if now >= t {
                self.save_at = None;
                let _ = self.config.save();
            }
        }
        // Bars change on their own (modules come, go and resize), so look at them again every so often.
        if now.duration_since(self.cal_at) >= Duration::from_millis(1000) {
            self.cal_at = now;
            let names: Vec<String> = self.scenes.keys().cloned().collect();
            for n in &names {
                self.calibrate(n);
            }
        }
        let params = self.params();
        let Some(g) = &self.gpu else { return };
        let debug_now = self.debug && now.duration_since(self.debug_at).as_secs() >= 2;
        for m in &mut self.mons {
            let Some(ov) = m.overlay.as_mut() else { continue };
            let Some(target) = ov.target.as_mut() else { continue };
            if ov.size.0 == 0 {
                continue;
            }
            m.world.step(dt, &params);
            self.batch.clear();
            m.world.draw(&mut self.batch, &params);
            let scale = ov.frac_scale.unwrap_or(m.int_scale.max(1) as f32);
            let buf = (((ov.size.0 as f32 * scale).round() as i32).max(1), ((ov.size.1 as f32 * scale).round() as i32).max(1));
            let opts = gpu::Opts {
                buf,
                logical: (ov.size.0 as f32, ov.size.1 as f32),
                scale,
                opacity: self.config.opacity,
                pixel: (self.config.renderer == Renderer::Pixel).then_some((self.config.pixel_size, self.config.pixel_levels)),
                clear: m.name.as_ref().and_then(|n| self.bare.get(n)).map(|v| v.iter().map(|r| [(r[0] as f32 * scale) as i32, (r[1] as f32 * scale) as i32, (r[2] as f32 * scale).ceil() as i32, (r[3] as f32 * scale).ceil() as i32]).collect()).unwrap_or_default(),
            };
            if self.debug && now.duration_since(self.debug_at).as_secs() >= 2 {
                eprintln!("windfall: {} parts={} verts={} mass={:.0} buf={:?}", m.name.as_deref().unwrap_or("?"), m.world.particle_count(), self.batch.vertex_count(), m.world.pile_mass(), buf);
            }
            g.draw(target, &self.batch.data, &opts);
        }
        if debug_now {
            self.debug_at = now;
        }
    }

    /// The scene for the simulation: bars replaced by the parts of them that are painted.
    fn push_scene(&mut self, name: &str) {
        let Some((raw, _)) = self.scenes.get(name) else { return };
        let mut scene = raw.clone();
        if self.config.bar_mode == BarMode::Visible {
            let mut surfs: Vec<Surf> = Vec::new();
            for s in raw.surfs.iter() {
                if s.kind != SurfKind::Layer {
                    surfs.push(s.clone());
                    continue;
                }
                match self.masks.get(&s.id) {
                    // Not examined yet: keep it bare rather than flash snow over the gaps.
                    None => {}
                    // Could not be examined: the whole rectangle is the best guess.
                    Some(None) => surfs.push(s.clone()),
                    Some(Some(segs)) => {
                        // Match each painted part to one seen before (by overlap) so that a part which only
                        // grew or shrank a little stays the same part, with its lawn and its snow.
                        let prev = self.seg_ids.get(&s.id).cloned().unwrap_or_default();
                        let mut used = vec![false; prev.len()];
                        let mut now: Vec<(u64, f32, f32)> = Vec::new();
                        for g in segs.iter() {
                            let (a0, a1) = (s.rect.x + g.x0, s.rect.x + g.x1);
                            let mut best: Option<(usize, f32)> = None;
                            for (k, (_, p0, p1)) in prev.iter().enumerate() {
                                let ov = (a1.min(*p1) - a0.max(*p0)).max(0.0);
                                if !used[k] && ov >= 0.4 * (a1 - a0).min(p1 - p0) && best.map_or(true, |(_, b)| ov > b) {
                                    best = Some((k, ov));
                                }
                            }
                            let id = match best {
                                Some((k, _)) => {
                                    used[k] = true;
                                    prev[k].0
                                }
                                None => {
                                    self.next_seg += 1;
                                    (s.id ^ self.next_seg.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1
                                }
                            };
                            now.push((id, a0, a1));
                            surfs.push(Surf {
                                id,
                                rect: Rect::new(a0, s.rect.y + g.top, a1 - a0, (s.rect.h - g.top).max(1.0)),
                                z: s.z,
                                kind: SurfKind::Layer,
                                inset: 2.0,
                                group: s.id, shown: true,
                            });
                        }
                        self.seg_ids.insert(s.id, now);
                    }
                }
            }
            scene.surfs = surfs;
        }
        if let Some(m) = self.mons.iter_mut().find(|m| m.name.as_deref() == Some(name)) {
            m.world.set_scene(scene);
        }
    }

    /// Looks at the bars of a monitor when they are new or have not been looked at for a while.
    fn calibrate(&mut self, name: &str) {
        if !self.config.enabled || !self.config.layers || self.config.bar_mode != BarMode::Visible {
            return;
        }
        let Some((raw, _)) = self.scenes.get(name) else { return };
        let layers: Vec<(u64, [i32; 4])> = raw.surfs.iter().filter(|s| s.kind == SurfKind::Layer).map(|s| (s.id, [s.rect.x as i32, s.rect.y as i32, s.rect.w as i32, s.rect.h as i32])).collect();
        if layers.is_empty() {
            return;
        }
        let cal = self.calibration.get(name);
        if cal.is_some_and(|c| c.busy || (c.key == layers && c.at.elapsed() < Duration::from_millis(1200))) {
            return;
        }
        self.calibration.insert(name.to_string(), Calibration { key: layers.clone(), at: Instant::now(), busy: true });
        let (tx, monitor) = (self.send.clone(), name.to_string());
        std::thread::spawn(move || {
            // Keep our own drawing off the bars for a moment so only the bars themselves are photographed.
            let bands: Vec<[i32; 4]> = layers.iter().map(|(_, r)| *r).collect();
            let _ = tx.send(Msg::Hide { monitor: monitor.clone(), bands });
            std::thread::sleep(Duration::from_millis(80));
            for (id, [x, y, w, h]) in &layers {
                let rows = (*h).min(40);
                let segs = capture::grab(&monitor, (*x, *y, *w, rows)).and_then(|img| {
                    let strip = shape::Strip { data: &img.data, w: img.w, h: img.h, stride: img.stride };
                    Some(shape::segments(&strip))
                });
                let _ = tx.send(Msg::Mask { id: *id, segs });
            }
            let _ = tx.send(Msg::Calibrated { monitor, key: layers });
        });
    }

    fn apply(&mut self, patch: &windfall_core::config::Patch) {
        if self.config.apply(patch) {
            self.save_at = Some(Instant::now() + Duration::from_millis(400));
            let names: Vec<String> = self.scenes.keys().cloned().collect();
            for n in &names {
                self.push_scene(n);
                self.calibrate(n);
            }
            self.reconcile();
            self.broadcast();
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::TrayReady(handle) => {
                // Show the current settings in the menu straight away.
                let cfg = self.config.clone();
                handle.update(move |tr| tr.config = cfg);
                self.tray = Some(handle);
            }
            Msg::Snapshot(snap) => {
                let mut names = Vec::new();
                for ms in snap.monitors {
                    names.push(ms.name.clone());
                    self.scenes.insert(ms.name, (ms.scene, ms.fullscreen));
                }
                for n in &names {
                    self.push_scene(n);
                    self.calibrate(n);
                }
                self.reconcile();
            }
            Msg::Hide { monitor, bands } => {
                if bands.is_empty() {
                    self.bare.remove(&monitor);
                } else {
                    self.bare.insert(monitor, bands);
                }
            }
            Msg::Mask { id, segs } => {
                if self.debug {
                    eprintln!("windfall: layer {id:x} -> {:?}", segs.as_ref().map(|v| v.iter().map(|g| (g.x0 as i32, g.x1 as i32, g.top as i32)).collect::<Vec<_>>()));
                }
                self.masks.insert(id, segs);
                let names: Vec<String> = self.scenes.keys().cloned().collect();
                for n in names {
                    self.push_scene(&n);
                }
            }
            Msg::Calibrated { monitor, key } => {
                self.bare.remove(&monitor);
                self.calibration.insert(monitor, Calibration { key, at: Instant::now(), busy: false });
            }
            Msg::Patch(p) => self.apply(&p),
            Msg::Toggle => {
                let on = !self.config.enabled;
                self.apply(&windfall_core::config::Patch { enabled: Some(on), ..Default::default() });
            }
            Msg::Clear => self.mons.iter_mut().for_each(|m| m.world.clear()),
            Msg::OpenGui => {
                let _ = std::process::Command::new("windfall-gui").stdin(std::process::Stdio::null()).spawn();
            }
            Msg::Quit => self.quit = true,
            Msg::Ipc { req, reply, subscriber } => {
                match req {
                    Request::Get => {}
                    Request::Set { patch } => self.apply(&patch),
                    Request::Toggle => self.handle(Msg::Toggle),
                    Request::Clear => self.handle(Msg::Clear),
                    Request::Quit => self.quit = true,
                    Request::Subscribe => {
                        if let Some(mut s) = subscriber {
                            let mut line = serde_json::to_string(&Response::Ok { state: self.state() }).unwrap_or_default();
                            line.push('\n');
                            // The reply below is the first line; later ones come from broadcast().
                            let _ = line;
                            let _ = s.flush();
                            self.subscribers.push(s);
                        }
                    }
                }
                let _ = reply.send(Response::Ok { state: self.state() });
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(app: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &GlobalListContents, _: &Connection, qh: &QueueHandle<Self>) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == "wl_output" => {
                let output = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                app.mons.push(Mon { gname: name, output, name: None, int_scale: 1, done: false, overlay: None, world: World::new(name as u64 * 7919 + 13) });
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(i) = app.mons.iter().position(|m| m.gname == name) {
                    app.destroy_overlay(i);
                    app.mons.remove(i);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for App {
    fn event(app: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, gname: &u32, _: &Connection, _: &QueueHandle<Self>) {
        let Some(m) = app.mons.iter_mut().find(|m| m.gname == *gname) else { return };
        match event {
            wl_output::Event::Name { name } => m.name = Some(name),
            wl_output::Event::Scale { factor } => m.int_scale = factor,
            wl_output::Event::Done => {
                m.done = true;
                app.reconcile();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, u32> for App {
    fn event(app: &mut Self, layer: &ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, gname: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                layer.ack_configure(serial);
                app.configure(*gname, width, height);
                app.tick_one(*gname);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                if let Some(i) = app.mons.iter().position(|m| m.gname == *gname) {
                    app.destroy_overlay(i);
                }
            }
            _ => {}
        }
    }
}

impl App {
    /// Draw right after a configure so the first buffer is attached promptly.
    fn tick_one(&mut self, _gname: u32) {
        self.last_tick = Instant::now();
        self.tick();
    }
}

impl Dispatch<WpFractionalScaleV1, u32> for App {
    fn event(app: &mut Self, _: &WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, gname: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            if let Some(ov) = app.mons.iter_mut().find(|m| m.gname == *gname).and_then(|m| m.overlay.as_mut()) {
                ov.frac_scale = Some(scale as f32 / 120.0);
                let (w, h) = ov.size;
                if w > 0 {
                    app.configure(*gname, w, h);
                }
            }
        }
    }
}

delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_region::WlRegion);
delegate_noop!(App: ignore ZwlrLayerShellV1);
delegate_noop!(App: ignore WpViewporter);
delegate_noop!(App: ignore WpViewport);
delegate_noop!(App: ignore WpFractionalScaleManagerV1);

pub fn main() {
    if !hypr::available() {
        eprintln!("windfall: Hyprland's IPC socket was not found; run this inside a Hyprland session.");
        std::process::exit(1);
    }
    let conn = Connection::connect_to_env().unwrap_or_else(|e| {
        eprintln!("windfall: cannot connect to Wayland: {e}");
        std::process::exit(1);
    });
    let (globals, queue) = registry_queue_init::<App>(&conn).expect("registry");
    let qh = queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 4..=6, ()).expect("wl_compositor");
    let layer_shell = globals.bind::<ZwlrLayerShellV1, _, _>(&qh, 1..=4, ()).unwrap_or_else(|_| {
        eprintln!("windfall: the compositor has no wlr-layer-shell");
        std::process::exit(1);
    });
    let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();
    let frac_mgr = viewporter.as_ref().and_then(|_| globals.bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ()).ok());

    let mut event_loop: EventLoop<App> = EventLoop::try_new().expect("event loop");
    let handle = event_loop.handle();
    let (tx, rx) = calloop::channel::channel::<Msg>();
    if let Err(e) = server::start(tx.clone()) {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            // Already running. Started by hand (or from the app menu) it just opens the controller.
            let args: Vec<String> = std::env::args().collect();
            let as_app = args.first().and_then(|a| std::path::Path::new(a).file_name()).is_some_and(|n| n == "windfall") || args.iter().any(|a| a == "--controller");
            if as_app {
                let _ = std::process::Command::new("windfall-gui").stdin(std::process::Stdio::null()).spawn();
            } else {
                eprintln!("Windfall is already running.");
            }
            return;
        }
        eprintln!("windfall: control socket: {e}");
        std::process::exit(1);
    }
    let awake = Arc::new(AtomicBool::new(true));
    let hypr_tx = tx.clone();
    hypr::spawn(move |s| hypr_tx.send(Msg::Snapshot(s)).is_ok(), awake.clone());

    let config = Config::load();
    // The tray icon. At login this daemon often starts before the bar (the StatusNotifier watcher) exists, so it
    // must not give up: the service waits for the watcher to appear and registers then, and if even starting
    // the service fails (no session bus yet, say) it is tried again every few seconds.
    {
        let (tx, config) = (tx.clone(), config.clone());
        std::thread::Builder::new()
            .name("tray".into())
            .spawn(move || {
                use ksni::blocking::TrayMethods;
                let mut attempt = 0u32;
                loop {
                    match (tray::Tray { config: config.clone(), tx: tx.clone() }).assume_sni_available(true).spawn() {
                        Ok(handle) => {
                            eprintln!("windfall: tray ready");
                            let _ = tx.send(Msg::TrayReady(handle));
                            return;
                        }
                        Err(e) => {
                            attempt += 1;
                            if attempt == 1 || attempt % 20 == 0 {
                                eprintln!("windfall: no tray yet ({e}); trying again");
                            }
                            std::thread::sleep(Duration::from_secs(3));
                        }
                    }
                }
            })
            .expect("spawn tray thread");
    }
    let tray = None;

    let mut app = App {
        qh: qh.clone(),
        conn: conn.clone(),
        compositor,
        layer_shell,
        viewporter,
        frac_mgr,
        gpu: None,
        mons: Vec::new(),
        config,
        scenes: HashMap::new(),
        masks: HashMap::new(),
        seg_ids: HashMap::new(),
        next_seg: 1,
        calibration: HashMap::new(),
        bare: HashMap::new(),
        send: tx.clone(),
        batch: Batch::default(),
        last_tick: Instant::now(),
        save_at: None,
        subscribers: Vec::new(),
        tray,
        awake,
        quit: false,
        debug: std::env::var_os("WINDFALL_DEBUG").is_some(),
        debug_at: Instant::now(),
        cal_at: Instant::now(),
    };

    // Outputs that already exist; later ones arrive as registry events.
    globals.contents().with_list(|list| {
        for g in list.iter().filter(|g| g.interface == "wl_output") {
            let output = globals.registry().bind::<wl_output::WlOutput, _, _>(g.name, g.version.min(4), &qh, g.name);
            app.mons.push(Mon { gname: g.name, output, name: None, int_scale: 1, done: false, overlay: None, world: World::new(g.name as u64 * 7919 + 13) });
        }
    });
    handle.insert_source(rx, |ev, _, app| {
        if let calloop::channel::Event::Msg(m) = ev {
            app.handle(m);
        }
    }).expect("channel source");
    handle.insert_source(Timer::from_duration(Duration::from_millis(50)), |_, _, app| {
        app.tick();
        TimeoutAction::ToDuration(app.interval())
    }).expect("timer");
    let signals = calloop::signals::Signals::new(&[calloop::signals::Signal::SIGINT, calloop::signals::Signal::SIGTERM]).expect("signals");
    handle.insert_source(signals, |_, _, app| app.quit = true).expect("signals source");
    WaylandSource::new(conn, queue).insert(handle).expect("wayland source");

    let stop = event_loop.get_signal();
    let _ = event_loop.run(None, &mut app, |app| {
        if app.quit {
            stop.stop();
        }
    });
    for i in 0..app.mons.len() {
        app.destroy_overlay(i);
    }
    let _ = app.config.save();
    let _ = app.conn.flush();
    let _ = std::fs::remove_file(windfall_core::ipc::socket_path());
}
