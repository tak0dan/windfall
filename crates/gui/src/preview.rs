//! A live miniature of the running effect, drawn with the very same simulation the daemon uses.
//! The pixel renderer is reproduced faithfully: a coarse image, no anti-aliasing, limited palette, scaled up.

use gtk::cairo::{self, Antialias, Context, Filter, Format, ImageSurface, Matrix};
use gtk::prelude::*;
use windfall_core::config::{Config, Effect, Renderer};
use windfall_core::draw::{Batch, FLOATS_PER_VERTEX};
use windfall_core::geom::Rect;
use windfall_core::palette::theme;
use windfall_core::sim::{Params, Scene, Surf, SurfKind, World};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Preview {
    pub area: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

struct State {
    config: Config,
    world: World,
    batch: Batch,
    cursor: Option<[f32; 2]>,
    last: Option<i64>,
    size: (f32, f32),
}

impl Preview {
    pub fn new(config: Config) -> Preview {
        let area = gtk::DrawingArea::builder().content_height(210).hexpand(true).build();
        area.add_css_class("preview");
        let state = Rc::new(RefCell::new(State { config, world: World::new(42), batch: Batch::default(), cursor: None, last: None, size: (0.0, 0.0) }));
        {
            let st = state.clone();
            area.set_draw_func(move |_, cr, w, h| draw(&mut st.borrow_mut(), cr, w as f32, h as f32));
        }
        {
            let st = state.clone();
            area.add_tick_callback(move |area, clock| {
                let now = clock.frame_time();
                let mut s = st.borrow_mut();
                let pixel = s.config.renderer == Renderer::Pixel;
                let min_gap = if pixel { 33_000 } else { 16_000 };
                if s.last.map_or(true, |l| now - l >= min_gap) {
                    let dt = s.last.map_or(0.016, |l| (now - l) as f32 / 1e6);
                    s.last = Some(now);
                    let (w, h) = s.size;
                    if w > 0.0 {
                        let scene = scene(w, h, s.cursor);
                        s.world.set_scene(scene);
                        let p = params(&s.config);
                        s.world.step(dt, &p);
                    }
                    drop(s);
                    area.queue_draw();
                }
                gtk::glib::ControlFlow::Continue
            });
        }
        let motion = gtk::EventControllerMotion::new();
        {
            let st = state.clone();
            motion.connect_motion(move |_, x, y| st.borrow_mut().cursor = Some([x as f32, y as f32]));
            let st = state.clone();
            motion.connect_leave(move |_| st.borrow_mut().cursor = None);
        }
        area.add_controller(motion);
        Preview { area, state }
    }

    pub fn set_config(&self, c: &Config) {
        self.state.borrow_mut().config = c.clone();
    }
}

fn params(c: &Config) -> Params {
    Params {
        effect: c.effect,
        // The preview is small, so it shows a bit more than the screen would.
        intensity: (c.intensity * 1.2).max(0.45),
        melt: true,
        interact: true,
        wind: c.wind,
        ground: true,
        windows: true,
        layers: true,
        pixel: c.renderer == Renderer::Pixel,
        pixel_size: if c.renderer == Renderer::Pixel { c.pixel_size as f32 } else { 0.0 },
        glow: windfall_core::config::glow_for(c),
        element: c.element,
        lights: c.lights,
    }
}

fn scene(w: f32, h: f32, cursor: Option<[f32; 2]>) -> Scene {
    let ground = h - 12.0;
    let win = Rect::new(w * 0.14, h * 0.42, w * 0.46, h);
    Scene {
        w: w * 1.0,
        h,
        surfs: vec![
            Surf { id: 1, rect: Rect::new(0.0, ground, w, 400.0), z: -1, kind: SurfKind::Ground, inset: 0.0, group: 0, shown: true },
            Surf { id: 2, rect: win, z: 0, kind: SurfKind::Window, inset: 6.0, group: 0, shown: true },
        ],
        cursor,
    }
}

fn draw(s: &mut State, cr: &Context, w: f32, h: f32) {
    s.size = (w, h);
    let t = theme(s.config.effect);
    // Backdrop: a sky strip and a mock window the effect settles on.
    cr.set_source_rgb(t.bg[0] as f64, t.bg[1] as f64, t.bg[2] as f64);
    let _ = cr.paint();
    let win = Rect::new(w * 0.14, h * 0.42, w * 0.46, h);
    cr.set_source_rgba(t.surface[0] as f64, t.surface[1] as f64, t.surface[2] as f64, 1.0);
    cr.rectangle(win.x as f64, win.y as f64, win.w as f64, (h - win.y) as f64);
    let _ = cr.fill();
    cr.set_source_rgba(t.border[0] as f64, t.border[1] as f64, t.border[2] as f64, 1.0);
    cr.set_line_width(1.0);
    cr.rectangle(win.x as f64 + 0.5, win.y as f64 + 0.5, win.w as f64, (h - win.y) as f64);
    let _ = cr.stroke();

    s.batch.clear();
    let p = params(&s.config);
    s.world.draw(&mut s.batch, &p);

    let opacity = s.config.opacity as f64;
    if s.config.renderer == Renderer::Pixel {
        let n = s.config.pixel_size.max(2) as f64;
        let (pw, ph) = ((w as f64 / n).ceil() as i32, (h as f64 / n).ceil() as i32);
        if let Ok(mut img) = ImageSurface::create(Format::ARgb32, pw, ph) {
            if let Ok(ic) = Context::new(&img) {
                ic.set_antialias(Antialias::None);
                ic.scale(1.0 / n, 1.0 / n);
                paint_batch(&ic, &s.batch.data);
            }
            img.flush();
            quantize(&mut img, s.config.pixel_levels);
            cr.save().ok();
            cr.scale(n, n);
            let pattern = cairo::SurfacePattern::create(&img);
            pattern.set_filter(Filter::Nearest);
            let _ = cr.set_source(&pattern);
            let _ = cr.paint_with_alpha(opacity);
            cr.restore().ok();
        }
    } else {
        cr.push_group();
        paint_batch(cr, &s.batch.data);
        let _ = cr.pop_group_to_source();
        let _ = cr.paint_with_alpha(opacity);
    }
}

/// Posterize to `levels` steps per channel and make alpha binary, like the daemon's pixel shader does.
fn quantize(img: &mut ImageSurface, levels: u32) {
    let l = (levels.max(2) - 1) as f32;
    let stride = img.stride() as usize;
    let (w, h) = (img.width() as usize, img.height() as usize);
    if let Ok(mut data) = img.data() {
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * 4;
                let a = data[i + 3] as f32 / 255.0;
                if a < 0.4 {
                    data[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                for k in 0..3 {
                    let c = (data[i + k] as f32 / 255.0 / a).clamp(0.0, 1.0);
                    data[i + k] = (((c * l).round() / l) * 255.0) as u8;
                }
                data[i + 3] = 255;
            }
        }
    }
    img.mark_dirty();
}

/// Replays the batch of triangles with cairo. Disc/ellipse quads (recognised by their uv corners) are drawn
/// as real ellipses, everything else as flat triangles.
fn paint_batch(cr: &Context, data: &[f32]) {
    const V: usize = FLOATS_PER_VERTEX;
    let mut i = 0;
    let n = data.len() / V;
    let vert = |k: usize| &data[k * V..k * V + V];
    let col = |v: &[f32]| cr.set_source_rgba(v[4] as f64, v[5] as f64, v[6] as f64, v[7] as f64);
    while i + 3 <= n {
        let a = vert(i);
        // An ellipse is two triangles whose uvs are the unit square corners.
        if i + 6 <= n && (a[2] != 0.0 || a[3] != 0.0) {
            let (v0, v1, v2) = (vert(i), vert(i + 1), vert(i + 2));
            let ex = ((v1[0] - v0[0]) / 2.0, (v1[1] - v0[1]) / 2.0);
            let ey = ((v2[0] - v0[0]) / 2.0, (v2[1] - v0[1]) / 2.0);
            let (cx, cy) = (v0[0] + ex.0 + ey.0, v0[1] + ex.1 + ey.1);
            col(v0);
            cr.save().ok();
            cr.transform(Matrix::new(ex.0 as f64, ex.1 as f64, ey.0 as f64, ey.1 as f64, cx as f64, cy as f64));
            cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
            cr.restore().ok();
            let _ = cr.fill();
            i += 6;
            continue;
        }
        let (b, c) = (vert(i + 1), vert(i + 2));
        // Average the three colours so shaded pile cells keep their gradient look.
        let avg = |k: usize| ((a[k] + b[k] + c[k]) / 3.0) as f64;
        cr.set_source_rgba(avg(4), avg(5), avg(6), avg(7));
        cr.move_to(a[0] as f64, a[1] as f64);
        cr.line_to(b[0] as f64, b[1] as f64);
        cr.line_to(c[0] as f64, c[1] as f64);
        cr.close_path();
        let _ = cr.fill();
        i += 3;
    }
}

#[allow(dead_code)]
fn _unused(_: Effect) {}

/// Renders the preview offscreen after `secs` of simulated time and saves it as a PNG (used for screenshots
/// and checks; needs no display).
pub fn render_png(config: &Config, secs: f32, w: i32, h: i32, path: &str, cursor: Option<[f32; 2]>) -> Result<(), String> {
    let mut st = State { config: config.clone(), world: World::new(7), batch: Batch::default(), cursor: None, last: None, size: (w as f32, h as f32) };
    let p = params(&st.config);
    let dt = if config.renderer == Renderer::Pixel { 1.0 / 30.0 } else { 1.0 / 60.0 };
    let steps = (secs / dt) as usize;
    for i in 0..steps {
        // The pointer stays over the lawn for the last two seconds, if there is one.
        let c = if (steps - i) as f32 * dt <= 2.0 { cursor } else { None };
        st.world.set_scene(scene(w as f32, h as f32, c));
        st.world.step(dt, &p);
    }
    let surface = ImageSurface::create(Format::ARgb32, w, h).map_err(|e| e.to_string())?;
    {
        let cr = Context::new(&surface).map_err(|e| e.to_string())?;
        draw(&mut st, &cr, w as f32, h as f32);
    }
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    surface.write_to_png(&mut f).map_err(|e| e.to_string())
}
