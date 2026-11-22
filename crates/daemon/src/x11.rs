//! X11 desktop backend. It uses one transparent, input-empty GTK surface per monitor and the shared simulation.
//! Window and bar geometry is compositor-specific; the portable X11 path provides screen-edge effects and
//! suppresses them above fullscreen applications when the EWMH state is available.

use crate::raster::Raster;
use gtk::prelude::*;
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windfall_core::config::{Config, Patch, Renderer};
use windfall_core::draw::Batch;
use windfall_core::geom::Rect;
use windfall_core::ipc::{self, Request, Response, State};
use windfall_core::sim::{Params, Scene, Surf, SurfKind, World};

struct MonitorView {
    name: String,
    window: gtk::Window,
    area: gtk::DrawingArea,
    image: Rc<RefCell<gtk::cairo::ImageSurface>>,
    world: World,
    size: (i32, i32),
}

struct UiRequest {
    request: Request,
    reply: mpsc::Sender<Response>,
}

struct AppState {
    config: Config,
    monitors: Vec<MonitorView>,
    requests: mpsc::Receiver<UiRequest>,
    batch: Batch,
    last: Instant,
    last_fullscreen_check: Instant,
    fullscreen: bool,
}

fn gui_command() -> Command {
    let beside = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("windfall-gui"))
        .filter(|p| p.exists());
    Command::new(beside.unwrap_or_else(|| "windfall-gui".into()))
}

fn serve(tx: mpsc::Sender<UiRequest>) -> std::io::Result<()> {
    let path = ipc::socket_path();
    if path.exists() {
        if ipc::connect().is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)?;
    std::thread::Builder::new()
        .name("windfall-ipc".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        return;
                    }
                    let request = match serde_json::from_str::<Request>(&line) {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = writeln!(
                                stream,
                                "{}",
                                serde_json::to_string(&Response::Error {
                                    message: format!("bad request: {e}")
                                })
                                .unwrap_or_default()
                            );
                            return;
                        }
                    };
                    let (reply, wait) = mpsc::channel();
                    if tx.send(UiRequest { request, reply }).is_err() {
                        return;
                    }
                    if let Ok(response) = wait.recv_timeout(Duration::from_secs(2)) {
                        let _ = writeln!(
                            stream,
                            "{}",
                            serde_json::to_string(&response).unwrap_or_default()
                        );
                    }
                });
            }
        })?;
    Ok(())
}

fn active_fullscreen() -> bool {
    let Ok(root) = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&root.stdout);
    let Some(id) = text
        .split("# ")
        .nth(1)
        .map(str::trim)
        .filter(|s| s.starts_with("0x"))
    else {
        return false;
    };
    let Ok(state) = Command::new("xprop")
        .args(["-id", id, "_NET_WM_STATE"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&state.stdout).contains("_NET_WM_STATE_FULLSCREEN")
}

fn params(c: &Config) -> Params {
    let colors = windfall_core::config::colors_of(c);
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
        pixel_size: if c.renderer == Renderer::Pixel {
            c.pixel_size as f32
        } else {
            0.0
        },
        glow: windfall_core::config::glow_for(c),
        element: c.element,
        lights: c.lights,
        lights_color: colors.0,
        hearts: colors.1,
        comet: colors.2,
        flower_tint: colors.3,
    }
}

fn state(s: &AppState) -> State {
    State {
        config: s.config.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        monitors: s.monitors.iter().map(|m| m.name.clone()).collect(),
    }
}

fn respond(s: &AppState, tx: mpsc::Sender<Response>) {
    let _ = tx.send(Response::Ok { state: state(s) });
}

pub fn main() {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .unwrap_or_else(|_| "unknown desktop".into());
    eprintln!("windfall: detected {desktop} / X11 session");
    if let Err(e) = Command::new("xprop").arg("-root").output() {
        eprintln!("windfall: xprop is unavailable; fullscreen detection will be disabled ({e})");
    }
    let (tx, rx) = mpsc::channel();
    if let Err(e) = serve(tx) {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            let _ = gui_command().stdin(std::process::Stdio::null()).spawn();
            return;
        }
        eprintln!("windfall: control socket: {e}");
        std::process::exit(1);
    }

    let app = gtk::Application::new(Some("io.github.tak0dan.Windfall"), Default::default());
    let rx = Rc::new(RefCell::new(Some(rx)));
    app.connect_activate(move |app| {
        let Some(display) = gtk::gdk::Display::default() else {
            eprintln!("windfall: GTK could not find the X11 display");
            app.quit();
            return;
        };
        let list = display.monitors();
        let mut monitors = Vec::new();
        for i in 0..list.n_items() {
            let Some(monitor) = list
                .item(i)
                .and_then(|o| o.downcast::<gtk::gdk::Monitor>().ok())
            else {
                continue;
            };
            let geometry = monitor.geometry();
            let (w, h) = (geometry.width().max(1), geometry.height().max(1));
            let name = monitor
                .connector()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Monitor-{i}"));
            let image = match gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h) {
                Ok(surface) => Rc::new(RefCell::new(surface)),
                Err(e) => {
                    eprintln!("windfall: cannot allocate X11 overlay for {name}: {e}");
                    continue;
                }
            };
            let area = gtk::DrawingArea::new();
            area.set_content_width(w);
            area.set_content_height(h);
            area.set_focusable(false);
            let draw_image = image.clone();
            area.set_draw_func(move |_, cr, _, _| {
                let _ = cr.set_operator(gtk::cairo::Operator::Source);
                let _ = cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.set_operator(gtk::cairo::Operator::Over);
                let _ = cr.set_source_surface(&*draw_image.borrow(), 0.0, 0.0);
                let _ = cr.paint();
            });
            let window = gtk::Window::new();
            window.set_application(Some(app));
            window.set_title(Some("Windfall overlay"));
            window.set_decorated(false);
            window.set_focusable(false);
            window.set_default_size(w, h);
            window.set_child(Some(&area));
            let css = gtk::CssProvider::new();
            css.load_from_string("window, drawingarea { background-color: transparent; }");
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            window.connect_realize(|window| {
                if let Some(surface) = window.surface() {
                    surface.set_input_region(&gtk::cairo::Region::create());
                }
            });
            window.fullscreen_on_monitor(&monitor);
            window.present();
            let ground = Surf {
                id: 1,
                rect: Rect::new(0.0, h as f32, w as f32, 4000.0),
                z: -1,
                kind: SurfKind::Ground,
                inset: 0.0,
                group: 0,
                shown: true,
            };
            let mut world = World::new(7919 * (i as u64 + 1));
            world.set_scene(Scene {
                w: w as f32,
                h: h as f32,
                surfs: vec![ground],
                cursor: None,
            });
            monitors.push(MonitorView {
                name,
                window,
                area,
                image,
                world,
                size: (w, h),
            });
        }
        if monitors.is_empty() {
            eprintln!("windfall: no X11 monitors were found");
            app.quit();
            return;
        }
        let Some(requests) = rx.borrow_mut().take() else {
            return;
        };
        let state = Rc::new(RefCell::new(AppState {
            config: Config::load(),
            monitors,
            requests,
            batch: Batch::default(),
            last: Instant::now(),
            last_fullscreen_check: Instant::now(),
            fullscreen: false,
        }));
        let state_tick = state.clone();
        let app_quit = app.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(33), move || {
            let mut s = state_tick.borrow_mut();
            while let Ok(msg) = s.requests.try_recv() {
                match msg.request {
                    Request::Get | Request::Subscribe => respond(&s, msg.reply),
                    Request::Set { patch } => {
                        s.config.apply(&patch);
                        s.config.save().ok();
                        respond(&s, msg.reply);
                    }
                    Request::Toggle => {
                        let enabled = !s.config.enabled;
                        s.config.apply(&Patch {
                            enabled: Some(enabled),
                            ..Default::default()
                        });
                        s.config.save().ok();
                        respond(&s, msg.reply);
                    }
                    Request::Clear => {
                        for m in &mut s.monitors {
                            m.world.clear();
                        }
                        respond(&s, msg.reply);
                    }
                    Request::Quit => {
                        s.config.save().ok();
                        respond(&s, msg.reply);
                        app_quit.quit();
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
            let now = Instant::now();
            if now.duration_since(s.last_fullscreen_check) >= Duration::from_millis(500) {
                s.fullscreen = active_fullscreen();
                s.last_fullscreen_check = now;
                for m in &s.monitors {
                    m.window
                        .set_visible(!s.fullscreen || s.config.over_fullscreen);
                }
            }
            let dt = now.duration_since(s.last).as_secs_f32().min(0.1);
            s.last = now;
            let p = params(&s.config);
            let drawing = s.config.drawing();
            let aa = s.config.renderer == Renderer::Smooth;
            let opacity = s.config.opacity;
            let mut batch = std::mem::take(&mut s.batch);
            for m in &mut s.monitors {
                if drawing {
                    m.world.step(dt, &p);
                    batch.clear();
                    m.world.draw(&mut batch, &p);
                    let (w, h) = m.size;
                    let mut surface = m.image.borrow_mut();
                    {
                        if let Ok(mut bytes) = surface.data() {
                            bytes.fill(0);
                            Raster {
                                buf: &mut bytes,
                                w,
                                h,
                                aa,
                                opacity,
                            }
                            .batch(&batch.data, 1.0, 1.0);
                        }
                    }
                    surface.mark_dirty();
                } else {
                    let mut surface = m.image.borrow_mut();
                    {
                        if let Ok(mut bytes) = surface.data() {
                            bytes.fill(0);
                        }
                    }
                    surface.mark_dirty();
                }
                m.area.queue_draw();
            }
            s.batch = batch;
            gtk::glib::ControlFlow::Continue
        });
    });
    let args: Vec<_> = std::env::args().collect();
    let _ = app.run_with_args(&args);
    let _ = std::fs::remove_file(ipc::socket_path());
}
