//! The StatusNotifier icon: left click opens the controller, middle click toggles, the menu has the rest.

use crate::msg::Msg;
use calloop::channel::Sender;
use ksni::menu::*;
use windfall_core::config::{Config, Effect, Element, Patch, Renderer};

pub struct Tray {
    pub config: Config,
    pub tx: Sender<Msg>,
}

impl Tray {
    fn patch(p: Patch) -> Box<dyn Fn(&mut Tray) + Send> {
        Box::new(move |t: &mut Tray| {
            let _ = t.tx.send(Msg::Patch(p.clone()));
        })
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "windfall".into()
    }
    fn title(&self) -> String {
        "Windfall".into()
    }
    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }
    fn icon_name(&self) -> String {
        String::new()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let (data, size) = glyph(self.config.effect, self.config.renderer == Renderer::Pixel, self.config.enabled);
        vec![ksni::Icon { width: size, height: size, data }]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Windfall".into(),
            description: format!("{}, {}{}", self.config.effect.title(), self.config.renderer.title(), if self.config.enabled { "" } else { " (off)" }),
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Msg::OpenGui);
    }
    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Msg::Toggle);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let c = &self.config;
        let level = |label: &str, v: f32| {
            let on = (c.intensity - v).abs() < 0.13;
            CheckmarkItem { label: label.into(), checked: on, activate: Tray::patch(Patch { intensity: Some(v), ..Default::default() }), ..Default::default() }.into()
        };
        vec![
            StandardItem { label: "Open controller".into(), activate: Box::new(|t: &mut Tray| { let _ = t.tx.send(Msg::OpenGui); }), ..Default::default() }.into(),
            MenuItem::Separator,
            CheckmarkItem { label: "Display".into(), checked: c.enabled, activate: Box::new(|t: &mut Tray| { let _ = t.tx.send(Msg::Toggle); }), ..Default::default() }.into(),
            RadioGroup {
                selected: Effect::ALL.iter().position(|e| *e == c.effect).unwrap_or(0),
                select: Box::new(|t: &mut Tray, i| { let _ = t.tx.send(Msg::Patch(Patch { effect: Some(Effect::ALL[i]), ..Default::default() })); }),
                options: Effect::ALL.iter().map(|e| RadioItem { label: e.title().into(), ..Default::default() }).collect(),
            }
            .into(),
            SubMenu {
                label: "Flower element".into(),
                submenu: vec![RadioGroup {
                    selected: Element::ALL.iter().position(|e| *e == c.element).unwrap_or(0),
                    select: Box::new(|t: &mut Tray, i| { let _ = t.tx.send(Msg::Patch(Patch { element: Some(Element::ALL[i]), effect: Some(Effect::Flowers), ..Default::default() })); }),
                    options: Element::ALL.iter().map(|e| RadioItem { label: e.title().into(), ..Default::default() }).collect(),
                }
                .into()],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            RadioGroup {
                selected: Renderer::ALL.iter().position(|r| *r == c.renderer).unwrap_or(0),
                select: Box::new(|t: &mut Tray, i| { let _ = t.tx.send(Msg::Patch(Patch { renderer: Some(Renderer::ALL[i]), ..Default::default() })); }),
                options: Renderer::ALL.iter().map(|r| RadioItem { label: r.title().into(), ..Default::default() }).collect(),
            }
            .into(),
            SubMenu { label: "Intensity".into(), submenu: vec![level("Light", 0.2), level("Steady", 0.5), level("Heavy", 0.85)], ..Default::default() }.into(),
            MenuItem::Separator,
            StandardItem { label: "Clear".into(), activate: Box::new(|t: &mut Tray| { let _ = t.tx.send(Msg::Clear); }), ..Default::default() }.into(),
            StandardItem { label: "Quit".into(), icon_name: "application-exit".into(), activate: Box::new(|t: &mut Tray| { let _ = t.tx.send(Msg::Quit); }), ..Default::default() }.into(),
        ]
    }
}

/// A small drawn glyph of the current effect, as ARGB32. The pixel renderer gets a chunky 16x16 one.
fn glyph(effect: Effect, pixel: bool, enabled: bool) -> (Vec<u8>, i32) {
    let n: i32 = if pixel { 16 } else { 32 };
    let out: i32 = 32;
    let mut data = vec![0u8; (out * out * 4) as usize];
    let (r, g, b): (f32, f32, f32) = match effect {
        Effect::Snow => (0.82, 0.93, 1.0),
        Effect::Leaves => (0.95, 0.55, 0.12),
        Effect::Blossom => (1.0, 0.55, 0.72),
        Effect::Rain => (0.45, 0.68, 0.98),
        Effect::Flowers => (1.0, 0.82, 0.25),
        Effect::Comets => (0.62, 0.82, 1.0),
        Effect::Lights => (1.0, 0.85, 0.4),
        Effect::Hearts => (1.0, 0.4, 0.6),
    };
    let alpha_scale = if enabled { 1.0 } else { 0.4 };
    let ns = n as f32;
    for py in 0..n {
        for px in 0..n {
            let (x, y) = ((px as f32 + 0.5) / ns * 2.0 - 1.0, (py as f32 + 0.5) / ns * 2.0 - 1.0);
            let d = (x * x + y * y).sqrt();
            let ang = y.atan2(x);
            let cov: f32 = match effect {
                Effect::Snow => {
                    let arm = (0..3).map(|k| { let a = k as f32 * std::f32::consts::PI / 3.0; ((x * a.sin() - y * a.cos()).abs() < 0.11 && d < 0.88) as i32 as f32 }).fold(0.0, f32::max);
                    arm.max((d < 0.2) as i32 as f32)
                }
                Effect::Leaves => ((x.abs() * 1.1 + y.abs() * 0.7 < 0.8 && (x + y * 0.6).abs() < 0.5) as i32 as f32).max(((y - x).abs() < 0.07 && x > 0.0 && x < 0.95 && y > 0.0) as i32 as f32 * 0.0),
                Effect::Blossom => {
                    let petal = (ang * 5.0).cos().abs();
                    ((d < 0.28 + 0.6 * petal * petal) && d > 0.12) as i32 as f32
                }
                Effect::Rain => {
                    let drop = |cx: f32, cy: f32| { let (dx, dy) = (x - cx, y - cy); ((dx * 0.9 + dy * 0.35).abs() < 0.13 && dy.abs() < 0.32) as i32 as f32 };
                    drop(-0.45, -0.4).max(drop(0.1, -0.1)).max(drop(0.55, 0.3)).max(drop(-0.3, 0.45))
                }
                Effect::Comets => {
                    let tail = ((x - y).abs() < 0.13 && x > -0.85 && x < 0.45) as i32 as f32;
                    tail.max(((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt().lt(&0.28) as i32 as f32)
                }
                Effect::Lights => (d < 0.72) as i32 as f32,
                Effect::Hearts => {
                    let (hx, hy) = (x * 1.25, -y * 1.25 - 0.15);
                    ((hx * hx + hy * hy - 1.0).powi(3) - hx * hx * hy.powi(3) <= 0.0) as i32 as f32
                }
                Effect::Flowers => {
                    let stem = (x.abs() < 0.07 && y > 0.0 && y < 0.95) as i32 as f32;
                    let head = ((x * x + (y + 0.35).powi(2)).sqrt() < 0.42) as i32 as f32;
                    stem.max(head)
                }
            };
            if cov <= 0.0 {
                continue;
            }
            let green = effect == Effect::Flowers && y > 0.05;
            let (cr, cg, cb) = if green { (0.3, 0.75, 0.35) } else { (r, g, b) };
            let reps = out / n;
            for oy in 0..reps {
                for ox in 0..reps {
                    let i = (((py * reps + oy) * out + px * reps + ox) * 4) as usize;
                    data[i] = (255.0 * alpha_scale) as u8;
                    data[i + 1] = (cr * 255.0) as u8;
                    data[i + 2] = (cg * 255.0) as u8;
                    data[i + 3] = (cb * 255.0) as u8;
                }
            }
        }
    }
    (data, out)
}
