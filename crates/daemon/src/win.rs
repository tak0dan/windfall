//! The Windows backend: one click-through layered window per monitor that is drawn with tiny-skia, a tray icon,
//! the windows and the taskbar read through the Win32 API, and a local control channel (JSON lines and a small
//! web controller on 127.0.0.1).

use std::cell::RefCell;
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::raster::Raster;
use windfall_core::config::{BarMode, Config, Effect, Element, Patch, Renderer};
use windfall_core::draw::Batch;
use windfall_core::geom::Rect;
use windfall_core::ipc::{Request, Response, State};
use windfall_core::sim::{Params, Scene, Surf, SurfKind, World};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_WAKE: u32 = WM_APP + 1;
const WM_TRAY: u32 = WM_APP + 2;
const TIMER_FRAME: usize = 1;
const HTML: &str = include_str!("web.html");

static MSG_HWND: AtomicIsize = AtomicIsize::new(0);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn log(line: &str) {
    if let Some(dir) = windfall_core::ipc::port_path().parent() {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("windfalld.log"))
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

enum Msg {
    Ipc {
        req: Request,
        reply: mpsc::Sender<Response>,
    },
}

struct Overlay {
    hwnd: HWND,
    name: String,
    /// Physical position and size.
    rect: [i32; 4],
    scale: f32,
    /// What the simulation sees (logical pixels).
    lw: f32,
    lh: f32,
    mem: HDC,
    dib: HBITMAP,
    bits: *mut u8,
    small: Vec<u8>,
    world: World,
    shown: bool,
}

struct MonInfo {
    name: String,
    rect: [i32; 4],
    work: [i32; 4],
    scale: f32,
}

struct App {
    config: Config,
    overlays: Vec<Overlay>,
    scenes: Vec<(Scene, bool)>,
    rx: mpsc::Receiver<Msg>,
    port: u16,
    batch: Batch,
    last: Instant,
    scene_at: Instant,
    logged: Instant,
    hwnd: HWND,
    icon: HICON,
    quit: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

// ------------------------------------------------------------------------------------------------ entry

pub fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let only_controller = std::env::args().any(|a| a == "--controller");
        let mutex = CreateMutexW(null(), 0, wide("Local\\WindfallDaemon").as_ptr());
        let _ = mutex;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Already running: just show its controller if asked.
            if only_controller {
                open_controller();
            }
            return;
        }
        log("windfalld starting");
        let (tx, rx) = mpsc::channel::<Msg>();
        let port = match start_server(tx) {
            Ok(p) => p,
            Err(e) => {
                log(&format!("no control port: {e}"));
                return;
            }
        };
        let hinst = GetModuleHandleW(null());
        let class = wide("WindfallMessages");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(msg_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Windfall").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            hinst,
            null(),
        );
        if hwnd.is_null() {
            log("no message window");
            return;
        }
        MSG_HWND.store(hwnd as isize, Ordering::SeqCst);
        let config = Config::load();
        let (g, size) = windfall_core::icon::glyph(
            config.effect,
            config.renderer == Renderer::Pixel,
            config.enabled,
        );
        let icon = make_icon(&g, size);
        let mut app = App {
            config,
            overlays: Vec::new(),
            scenes: Vec::new(),
            rx,
            port,
            batch: Batch::default(),
            last: Instant::now(),
            scene_at: Instant::now() - Duration::from_secs(1),
            logged: Instant::now(),
            hwnd,
            icon,
            quit: false,
        };
        register_overlay_class(hinst);
        create_overlays(&mut app);
        tray_add(&app);
        APP.with(|a| *a.borrow_mut() = Some(app));
        SetTimer(hwnd, TIMER_FRAME, 16, None);
        if only_controller {
            open_controller();
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().take() {
                let _ = app.config.save();
                tray_remove(&app);
            }
        });
        let _ = std::fs::remove_file(windfall_core::ipc::port_path());
    }
}

unsafe extern "system" fn msg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wp == TIMER_FRAME => {
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    tick(app);
                    if app.quit {
                        PostQuitMessage(0);
                    }
                }
            });
            0
        }
        WM_WAKE => {
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    drain(app);
                    if app.quit {
                        PostQuitMessage(0);
                    }
                }
            });
            0
        }
        WM_TRAY => {
            let event = (lp as u32) & 0xFFFF;
            if event == WM_LBUTTONUP {
                open_controller();
            } else if event == WM_RBUTTONUP {
                tray_menu(hwnd);
            }
            0
        }
        WM_DISPLAYCHANGE => {
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    destroy_overlays(app);
                    create_overlays(app);
                }
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ------------------------------------------------------------------------------------------------ overlays

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return HTTRANSPARENT as LRESULT;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn register_overlay_class(hinst: HINSTANCE) {
    let class = wide("WindfallOverlay");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(overlay_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassW(&wc);
}

unsafe extern "system" fn monitor_cb(hmon: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let list = &mut *(lp as *mut Vec<MonInfo>);
    let mut mi: MONITORINFOEXW = std::mem::zeroed();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(hmon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO) == 0 {
        return 1;
    }
    let (mut dx, mut dy) = (96u32, 96u32);
    GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
    let len = mi
        .szDevice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(mi.szDevice.len());
    let r = mi.monitorInfo.rcMonitor;
    let w = mi.monitorInfo.rcWork;
    list.push(MonInfo {
        name: String::from_utf16_lossy(&mi.szDevice[..len]),
        rect: [r.left, r.top, r.right, r.bottom],
        work: [w.left, w.top, w.right, w.bottom],
        scale: (dx.max(96) as f32) / 96.0,
    });
    1
}

unsafe fn monitors() -> Vec<MonInfo> {
    let mut v: Vec<MonInfo> = Vec::new();
    EnumDisplayMonitors(
        null_mut(),
        null(),
        Some(monitor_cb),
        &mut v as *mut Vec<MonInfo> as LPARAM,
    );
    v
}

unsafe fn create_overlays(app: &mut App) {
    let hinst = GetModuleHandleW(null());
    let class = wide("WindfallOverlay");
    for m in monitors() {
        let (x, y, w, h) = (
            m.rect[0],
            m.rect[1],
            m.rect[2] - m.rect[0],
            m.rect[3] - m.rect[1],
        );
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("Windfall overlay").as_ptr(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        if hwnd.is_null() {
            log("could not create an overlay");
            continue;
        }
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut c_void = null_mut();
        let dib = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        SelectObject(mem, dib);
        ReleaseDC(null_mut(), screen);
        let seed = (hwnd as usize as u64).wrapping_mul(7919) ^ 0x5eed;
        app.overlays.push(Overlay {
            hwnd,
            name: m.name.clone(),
            rect: [x, y, w, h],
            scale: m.scale,
            lw: w as f32 / m.scale,
            lh: h as f32 / m.scale,
            mem,
            dib,
            bits: bits as *mut u8,
            small: Vec::new(),
            world: World::new(seed),
            shown: false,
        });
    }
    log(&format!("{} overlay(s)", app.overlays.len()));
}

unsafe fn destroy_overlays(app: &mut App) {
    for o in app.overlays.drain(..) {
        DestroyWindow(o.hwnd);
        DeleteDC(o.mem);
        DeleteObject(o.dib);
    }
}

// ------------------------------------------------------------------------------------------------ scenes

struct Win {
    hwnd: HWND,
    r: [i32; 4],
    zoomed: bool,
}

const SKIP_CLASSES: [&str; 12] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "Button",
    "tooltips_class32",
    "#32768",
    "SysShadow",
];

unsafe fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = GetClassNameW(hwnd, buf.as_mut_ptr(), 128);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp as *mut Vec<Win>);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return 1;
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    // Tool windows, no-activate windows and click-through ones (overlays, tooltips, the shell) don't count.
    if ex & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT) != 0 {
        return 1;
    }
    let mut cloaked: u32 = 0;
    DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED as u32,
        &mut cloaked as *mut u32 as *mut c_void,
        4,
    );
    if cloaked != 0 || GetWindowTextLengthW(hwnd) == 0 {
        return 1;
    }
    if SKIP_CLASSES.contains(&class_of(hwnd).as_str()) {
        return 1;
    }
    let mut r: RECT = std::mem::zeroed();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS as u32,
        &mut r as *mut RECT as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    ) != 0
    {
        GetWindowRect(hwnd, &mut r);
    }
    v.push(Win {
        hwnd,
        r: [r.left, r.top, r.right, r.bottom],
        zoomed: IsZoomed(hwnd) != 0,
    });
    1
}

/// Builds each monitor's scene from the windows (top to bottom), the taskbar's top edge and the cursor.
unsafe fn read_scenes() -> Vec<(String, Scene, bool)> {
    let mons = monitors();
    let mut wins: Vec<Win> = Vec::new();
    EnumWindows(Some(enum_cb), &mut wins as *mut Vec<Win> as LPARAM);
    let mut cur = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cur);
    let fg = GetForegroundWindow();
    let mut fg_rect: Option<[i32; 4]> = None;
    if !fg.is_null() && !SKIP_CLASSES.contains(&class_of(fg).as_str()) {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(fg, &mut r) != 0 {
            fg_rect = Some([r.left, r.top, r.right, r.bottom]);
        }
    }
    let mut out = Vec::new();
    for m in &mons {
        let (mx, my) = (m.rect[0], m.rect[1]);
        let (lw, lh) = (
            (m.rect[2] - mx) as f32 / m.scale,
            (m.rect[3] - my) as f32 / m.scale,
        );
        let mut surfs = Vec::new();
        // The taskbar (or the bottom of the work area) is the ground.
        let ground = (m.work[3] - my) as f32 / m.scale;
        surfs.push(Surf {
            id: 1,
            rect: Rect::new(0.0, ground, lw, 4000.0),
            z: -1,
            kind: SurfKind::Ground,
            inset: 0.0,
            group: 0,
            shown: true,
        });
        for (i, w) in wins.iter().enumerate() {
            let (cx, cy) = ((w.r[0] + w.r[2]) / 2, (w.r[1] + w.r[3]) / 2);
            if cx < m.rect[0] || cx >= m.rect[2] || cy < m.rect[1] || cy >= m.rect[3] {
                continue;
            }
            let (x, y) = (
                (w.r[0] - mx) as f32 / m.scale,
                (w.r[1] - my) as f32 / m.scale,
            );
            let (ww, wh) = (
                (w.r[2] - w.r[0]) as f32 / m.scale,
                (w.r[3] - w.r[1]) as f32 / m.scale,
            );
            if y < 20.0 || ww < 48.0 || wh < 24.0 {
                continue;
            }
            surfs.push(Surf {
                id: ((w.hwnd as usize as u64).wrapping_mul(0x9E37_79B9) << 1) | 2,
                rect: Rect::new(x, y - 1.0, ww, wh),
                z: 500 - (i as i32).min(480),
                kind: SurfKind::Window,
                inset: if w.zoomed {
                    0.0
                } else {
                    (8.0f32).min(ww / 3.0)
                },
                group: 0,
                shown: true,
            });
        }
        let cursor =
            (cur.x >= m.rect[0] && cur.x < m.rect[2] && cur.y >= m.rect[1] && cur.y < m.rect[3])
                .then(|| [(cur.x - mx) as f32 / m.scale, (cur.y - my) as f32 / m.scale]);
        let fullscreen = fg_rect.is_some_and(|r| {
            r[0] <= m.rect[0] && r[1] <= m.rect[1] && r[2] >= m.rect[2] && r[3] >= m.rect[3]
        });
        out.push((
            m.name.clone(),
            Scene {
                w: lw,
                h: lh,
                surfs,
                cursor,
            },
            fullscreen,
        ));
    }
    out
}

// ------------------------------------------------------------------------------------------------ frames

fn params(c: &Config) -> Params {
    Params {
        effect: c.effect,
        intensity: c.intensity,
        melt: c.melt,
        interact: c.interact,
        wind: c.wind,
        ground: c.ground,
        windows: c.windows,
        layers: false,
        pixel: c.renderer == Renderer::Pixel,
        pixel_size: if c.renderer == Renderer::Pixel {
            c.pixel_size as f32
        } else {
            0.0
        },
        glow: windfall_core::config::glow_for(c),
        element: c.element,
        lights: c.lights,
        lights_color: windfall_core::config::colors_of(c).0,
        hearts: windfall_core::config::colors_of(c).1,
        comet: windfall_core::config::colors_of(c).2,
        flower_tint: windfall_core::config::colors_of(c).3,
    }
}

fn tick(app: &mut App) {
    drain(app);
    let now = Instant::now();
    let dt = now.duration_since(app.last).as_secs_f32();
    app.last = now;
    if now.duration_since(app.scene_at) >= Duration::from_millis(33) && app.config.drawing() {
        app.scene_at = now;
        app.scenes = unsafe { read_scenes() }
            .into_iter()
            .map(|(_, s, f)| (s, f))
            .collect::<Vec<_>>();
    }
    if now.duration_since(app.logged) >= Duration::from_secs(5) {
        app.logged = now;
        for (i, (sc, fs)) in app.scenes.iter().enumerate() {
            let ground = sc
                .surfs
                .iter()
                .find(|s| s.kind == SurfKind::Ground)
                .map(|s| s.rect.y);
            log(&format!("scene {i}: {}x{} surfaces={} ground={ground:?} fullscreen={fs} shown={:?} parts={}", sc.w, sc.h, sc.surfs.len(), app.overlays.get(i).map(|o| o.shown), app.overlays.get(i).map_or(0, |o| o.world.particle_count())));
        }
    }
    let p = params(&app.config);
    let pixel = app.config.renderer == Renderer::Pixel;
    let interval = if pixel { 33 } else { 16 };
    unsafe {
        SetTimer(app.hwnd, TIMER_FRAME, interval, None);
    }
    for (i, ov) in app.overlays.iter_mut().enumerate() {
        let want = app.config.drawing()
            && app
                .scenes
                .get(i)
                .is_some_and(|(_, fs)| app.config.over_fullscreen || !fs);
        if !want {
            if ov.shown {
                unsafe { ShowWindow(ov.hwnd, SW_HIDE) };
                ov.shown = false;
            }
            continue;
        }
        if let Some((scene, _)) = app.scenes.get(i) {
            if app.scene_at == now {
                ov.world.set_scene(scene.clone());
            }
        }
        ov.world.step(dt, &p);
        app.batch.clear();
        ov.world.draw(&mut app.batch, &p);
        present(ov, &app.batch.data, &app.config);
        if !ov.shown {
            unsafe { ShowWindow(ov.hwnd, SW_SHOWNOACTIVATE) };
            ov.shown = true;
        }
    }
}

/// Draws the batch into the overlay's bitmap and hands it to the window.
fn present(ov: &mut Overlay, batch: &[f32], c: &Config) {
    let (w, h) = (ov.rect[2] as usize, ov.rect[3] as usize);
    unsafe {
        let dst = std::slice::from_raw_parts_mut(ov.bits, w * h * 4);
        if c.renderer == Renderer::Pixel {
            // Art pixels: draw small without anti-aliasing, then blow it up with hard edges and fewer colours.
            let n = ((c.pixel_size as f32) * ov.scale).round().max(1.0) as usize;
            let (sw, sh) = (w.div_ceil(n), h.div_ceil(n));
            ov.small.clear();
            ov.small.resize(sw * sh * 4, 0);
            let k = 1.0 / c.pixel_size as f32;
            Raster {
                buf: &mut ov.small,
                w: sw as i32,
                h: sh as i32,
                aa: false,
                opacity: 1.0,
            }
            .batch(batch, k, k);
            let levels = (c.pixel_levels.max(2) - 1) as f32;
            for y in 0..h {
                for x in 0..w {
                    let si = ((y / n) * sw + (x / n)) * 4;
                    let o = (y * w + x) * 4;
                    let a = ov.small[si + 3];
                    if a < 20 {
                        dst[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                        continue;
                    }
                    // A few steps of transparency, so soft rims stay see-through.
                    let af = a as f32 / 255.0;
                    let aq = ((af * 4.0 + 0.5).floor() / 4.0).max(0.25) * c.opacity;
                    let q = |v: u8| {
                        ((((v as f32 / 255.0) / af).clamp(0.0, 1.0) * levels).round() / levels
                            * aq
                            * 255.0) as u8
                    };
                    let al = (aq * 255.0) as u8;
                    dst[o] = q(ov.small[si]);
                    dst[o + 1] = q(ov.small[si + 1]);
                    dst[o + 2] = q(ov.small[si + 2]);
                    dst[o + 3] = al;
                }
            }
        } else {
            dst.fill(0);
            Raster {
                buf: dst,
                w: w as i32,
                h: h as i32,
                aa: true,
                opacity: c.opacity,
            }
            .batch(batch, ov.scale, ov.scale);
        }
        let size = SIZE {
            cx: w as i32,
            cy: h as i32,
        };
        let src = POINT { x: 0, y: 0 };
        let pos = POINT {
            x: ov.rect[0],
            y: ov.rect[1],
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(
            ov.hwnd,
            null_mut(),
            &pos,
            &size,
            ov.mem,
            &src,
            0,
            &blend,
            ULW_ALPHA,
        );
    }
}

// ------------------------------------------------------------------------------------------------ control

fn state(app: &App) -> State {
    State {
        config: app.config.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        monitors: app.overlays.iter().map(|o| o.name.clone()).collect(),
    }
}

fn drain(app: &mut App) {
    while let Ok(m) = app.rx.try_recv() {
        match m {
            Msg::Ipc { req, reply } => {
                match req {
                    Request::Get | Request::Subscribe => {}
                    Request::Set { patch } => apply(app, &patch),
                    Request::Toggle => {
                        let on = !app.config.enabled;
                        apply(
                            app,
                            &Patch {
                                enabled: Some(on),
                                ..Default::default()
                            },
                        );
                    }
                    Request::Clear => app.overlays.iter_mut().for_each(|o| o.world.clear()),
                    Request::Quit => app.quit = true,
                }
                let _ = reply.send(Response::Ok { state: state(app) });
            }
        }
    }
}

fn apply(app: &mut App, patch: &Patch) {
    if app.config.apply(patch) {
        let _ = app.config.save();
        let (g, size) = windfall_core::icon::glyph(
            app.config.effect,
            app.config.renderer == Renderer::Pixel,
            app.config.enabled,
        );
        unsafe {
            let old = app.icon;
            app.icon = make_icon(&g, size);
            tray_update(app);
            DestroyIcon(old);
        }
    }
}

fn start_server(tx: mpsc::Sender<Msg>) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let path = windfall_core::ipc::port_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, port.to_string())?;
    std::thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || serve(stream, tx, port));
            }
        })?;
    Ok(port)
}

fn ask(tx: &mpsc::Sender<Msg>, req: Request) -> Response {
    let (rtx, rrx) = mpsc::channel();
    if tx.send(Msg::Ipc { req, reply: rtx }).is_err() {
        return Response::Error {
            message: "daemon gone".into(),
        };
    }
    let hwnd = MSG_HWND.load(Ordering::SeqCst);
    unsafe {
        PostMessageW(hwnd as HWND, WM_WAKE, 0, 0);
    }
    rrx.recv_timeout(Duration::from_secs(2))
        .unwrap_or(Response::Error {
            message: "daemon busy".into(),
        })
}

fn serve(stream: TcpStream, tx: mpsc::Sender<Msg>, port: u16) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let mut r = BufReader::new(stream);
    let mut first = String::new();
    if r.read_line(&mut first).is_err() {
        return;
    }
    if first.starts_with("GET ") || first.starts_with("POST ") {
        http(first, r, &mut out, &tx, port);
    } else {
        let reply = match serde_json::from_str::<Request>(&first) {
            Ok(req) => ask(&tx, req),
            Err(e) => Response::Error {
                message: format!("bad request: {e}"),
            },
        };
        let mut text = serde_json::to_string(&reply).unwrap_or_default();
        text.push('\n');
        let _ = out.write_all(text.as_bytes());
    }
}

fn http(
    first: String,
    mut r: BufReader<TcpStream>,
    out: &mut TcpStream,
    tx: &mpsc::Sender<Msg>,
    port: u16,
) {
    let mut parts = first.split_whitespace();
    let (method, path) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or("/").to_string(),
    );
    let (mut host, mut len, mut custom) = (String::new(), 0usize, false);
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        let l = line.to_ascii_lowercase();
        if let Some(v) = l.strip_prefix("host:") {
            host = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        } else if l.starts_with("x-windfall:") {
            custom = true;
        }
    }
    let respond = |out: &mut TcpStream, status: &str, ctype: &str, body: &[u8]| {
        let head = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
        let _ = out.write_all(head.as_bytes());
        let _ = out.write_all(body);
    };
    // Only the local page may talk to us: the right Host (against DNS rebinding) and, for changes, a header
    // that other websites cannot send.
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        return respond(out, "403 Forbidden", "text/plain", b"forbidden");
    }
    let mut body = vec![0u8; len.min(1 << 16)];
    if len > 0 && r.read_exact(&mut body).is_err() {
        return;
    }
    let json = |resp: Response| serde_json::to_vec(&resp).unwrap_or_default();
    match (method, path.as_str()) {
        ("GET", "/") => respond(out, "200 OK", "text/html; charset=utf-8", HTML.as_bytes()),
        ("GET", "/api/state") => respond(
            out,
            "200 OK",
            "application/json",
            &json(ask(tx, Request::Get)),
        ),
        ("POST", "/api/set") if custom => match serde_json::from_slice::<Patch>(&body) {
            Ok(patch) => respond(
                out,
                "200 OK",
                "application/json",
                &json(ask(tx, Request::Set { patch })),
            ),
            Err(_) => respond(out, "400 Bad Request", "text/plain", b"bad patch"),
        },
        ("POST", "/api/clear") if custom => respond(
            out,
            "200 OK",
            "application/json",
            &json(ask(tx, Request::Clear)),
        ),
        ("POST", _) => respond(out, "403 Forbidden", "text/plain", b"forbidden"),
        _ => respond(out, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Opens the controller page in an app-style Edge window, or in the default browser.
fn open_controller() {
    let Ok(port) = windfall_core::ipc::port() else {
        return;
    };
    let url = format!("http://127.0.0.1:{port}/");
    unsafe {
        let r = ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide("msedge.exe").as_ptr(),
            wide(&format!("--app={url} --window-size=520,940")).as_ptr(),
            null(),
            SW_SHOWNORMAL,
        );
        if (r as isize) <= 32 {
            ShellExecuteW(
                null_mut(),
                wide("open").as_ptr(),
                wide(&url).as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

// ------------------------------------------------------------------------------------------------ tray

unsafe fn make_icon(argb: &[u8], size: i32) -> HICON {
    // The glyph is A,R,G,B per pixel; a colour bitmap wants B,G,R,A.
    let mut bgra = Vec::with_capacity(argb.len());
    for p in argb.chunks(4) {
        bgra.extend_from_slice(&[p[3], p[2], p[1], p[0]]);
    }
    let color = CreateBitmap(size, size, 1, 32, bgra.as_ptr() as *const c_void);
    let mask = CreateBitmap(size, size, 1, 1, null());
    let ii = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&ii);
    DeleteObject(color);
    DeleteObject(mask);
    icon
}

fn nid(app: &App) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = app.hwnd;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = app.icon;
    let tip: Vec<u16> = format!(
        "Windfall - {}{}",
        app.config.effect.title(),
        if app.config.enabled { "" } else { " (off)" }
    )
    .encode_utf16()
    .take(120)
    .collect();
    n.szTip[..tip.len()].copy_from_slice(&tip);
    n
}

unsafe fn tray_add(app: &App) {
    Shell_NotifyIconW(NIM_ADD, &nid(app));
}

unsafe fn tray_update(app: &App) {
    Shell_NotifyIconW(NIM_MODIFY, &nid(app));
}

unsafe fn tray_remove(app: &App) {
    Shell_NotifyIconW(NIM_DELETE, &nid(app));
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

unsafe fn autostart_on() -> bool {
    RegGetValueW(
        HKEY_CURRENT_USER,
        wide(RUN_KEY).as_ptr(),
        wide("Windfall").as_ptr(),
        RRF_RT_REG_SZ,
        null_mut(),
        null_mut(),
        null_mut(),
    ) == 0
}

unsafe fn set_autostart(on: bool) {
    if on {
        if let Ok(exe) = std::env::current_exe() {
            let value = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                wide("Windfall").as_ptr(),
                REG_SZ,
                value.as_ptr() as *const c_void,
                (value.len() * 2) as u32,
            );
        }
    } else {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide("Windfall").as_ptr(),
        );
    }
}

unsafe fn tray_menu(hwnd: HWND) {
    let snapshot = APP.with(|a| a.borrow().as_ref().map(|app| app.config.clone()));
    let Some(c) = snapshot else { return };
    let menu = CreatePopupMenu();
    let add = |m: HMENU, id: usize, label: &str, checked: bool| {
        AppendMenuW(
            m,
            MF_STRING | if checked { MF_CHECKED } else { 0 },
            id,
            wide(label).as_ptr(),
        );
    };
    add(menu, 51, "Open controller", false);
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    add(menu, 1, "Display", c.enabled);
    let effects = CreatePopupMenu();
    for (i, e) in Effect::ALL.iter().enumerate() {
        add(effects, 10 + i, e.title(), *e == c.effect);
    }
    AppendMenuW(menu, MF_POPUP, effects as usize, wide("Effect").as_ptr());
    let elements = CreatePopupMenu();
    for (i, e) in Element::ALL.iter().enumerate() {
        add(elements, 20 + i, e.title(), *e == c.element);
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        elements as usize,
        wide("Flower element").as_ptr(),
    );
    let renderers = CreatePopupMenu();
    for (i, r) in Renderer::ALL.iter().enumerate() {
        add(renderers, 30 + i, r.title(), *r == c.renderer);
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        renderers as usize,
        wide("Renderer").as_ptr(),
    );
    let intensity = CreatePopupMenu();
    for (i, (l, v)) in [("Light", 0.2f32), ("Steady", 0.5), ("Heavy", 0.85)]
        .iter()
        .enumerate()
    {
        add(intensity, 40 + i, l, (c.intensity - v).abs() < 0.13);
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        intensity as usize,
        wide("Intensity").as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    add(menu, 53, "Floating lights", c.lights);
    add(menu, 50, "Clear", false);
    add(menu, 52, "Start with Windows", autostart_on());
    add(menu, 99, "Quit", false);
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if cmd == 0 {
        return;
    }
    let cmd = cmd as usize;
    let patch = |p: Patch| p;
    APP.with(|a| {
        let mut b = a.borrow_mut();
        let Some(app) = b.as_mut() else { return };
        match cmd {
            1 => {
                let on = !app.config.enabled;
                apply(
                    app,
                    &patch(Patch {
                        enabled: Some(on),
                        ..Default::default()
                    }),
                );
            }
            10..=17 => apply(
                app,
                &Patch {
                    effect: Some(Effect::ALL[cmd - 10]),
                    ..Default::default()
                },
            ),
            20..=25 => apply(
                app,
                &Patch {
                    element: Some(Element::ALL[cmd - 20]),
                    effect: Some(Effect::Flowers),
                    ..Default::default()
                },
            ),
            30..=31 => apply(
                app,
                &Patch {
                    renderer: Some(Renderer::ALL[cmd - 30]),
                    ..Default::default()
                },
            ),
            40..=42 => apply(
                app,
                &Patch {
                    intensity: Some([0.2, 0.5, 0.85][cmd - 40]),
                    ..Default::default()
                },
            ),
            50 => app.overlays.iter_mut().for_each(|o| o.world.clear()),
            53 => {
                let on = !app.config.lights;
                apply(
                    app,
                    &Patch {
                        lights: Some(on),
                        ..Default::default()
                    },
                );
            }
            51 => open_controller(),
            52 => set_autostart(!autostart_on()),
            99 => app.quit = true,
            _ => {}
        }
        if app.quit {
            PostQuitMessage(0);
        }
    });
    let _ = BarMode::Visible;
}
