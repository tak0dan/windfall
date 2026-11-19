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
        let (data, size) = windfall_core::icon::glyph(self.config.effect, self.config.renderer == Renderer::Pixel, self.config.enabled);
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

