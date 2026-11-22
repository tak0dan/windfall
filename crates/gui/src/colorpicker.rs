//! A colour picker in the manner of painting apps: a square to pick saturation and brightness, a hue slider under
//! it, a hex field and a swatch. Everything stays in sync, and a change is reported as `#rrggbb`.

use gtk::cairo;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windfall_core::palette::{hex, hsv, parse_hex};

struct State {
    /// Hue, saturation, value, each 0..1.
    hsv: Cell<[f32; 3]>,
    area: gtk::DrawingArea,
    hue: gtk::Scale,
    entry: gtk::Entry,
    swatch: gtk::DrawingArea,
    quiet: Cell<bool>,
    on_change: RefCell<Option<Box<dyn Fn(String)>>>,
}

#[derive(Clone)]
pub struct Picker {
    /// The square, the hue slider and the hex field.
    pub root: gtk::Box,
    /// A small swatch of the current colour, for the row's header.
    pub swatch: gtk::DrawingArea,
    st: Rc<State>,
}

fn to_hsv(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

impl State {
    fn rgb(&self) -> [f32; 3] {
        let [h, s, v] = self.hsv.get();
        hsv(h, s, v)
    }
}

impl Picker {
    pub fn new() -> Picker {
        let area = gtk::DrawingArea::builder()
            .content_width(260)
            .content_height(150)
            .hexpand(true)
            .build();
        area.add_css_class("sv-square");
        let hue = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 360.0, 1.0);
        hue.set_draw_value(false);
        hue.add_css_class("hue");
        let entry = gtk::Entry::builder().max_length(7).width_chars(8).build();
        entry.set_valign(gtk::Align::Center);
        let swatch = gtk::DrawingArea::builder()
            .content_width(34)
            .content_height(22)
            .valign(gtk::Align::Center)
            .build();
        let st = Rc::new(State {
            hsv: Cell::new([0.0, 0.6, 1.0]),
            area: area.clone(),
            hue: hue.clone(),
            entry: entry.clone(),
            swatch: swatch.clone(),
            quiet: Cell::new(false),
            on_change: RefCell::new(None),
        });

        // The square: white to the right fades in saturation, black toward the bottom darkens.
        {
            let st = st.clone();
            area.set_draw_func(move |_, cr, w, h| {
                let [hu, s, v] = st.hsv.get();
                let pure = hsv(hu, 1.0, 1.0);
                let (w, h) = (w as f64, h as f64);
                cr.set_source_rgb(pure[0] as f64, pure[1] as f64, pure[2] as f64);
                let _ = cr.paint();
                let g = cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
                g.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 1.0);
                g.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
                let _ = cr.set_source(&g);
                let _ = cr.paint();
                let g = cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
                g.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
                g.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 1.0);
                let _ = cr.set_source(&g);
                let _ = cr.paint();
                // The marker.
                let (mx, my) = (s as f64 * w, (1.0 - v as f64) * h);
                cr.arc(mx, my, 7.0, 0.0, std::f64::consts::TAU);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.set_line_width(3.0);
                let _ = cr.stroke_preserve();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(1.5);
                let _ = cr.stroke();
            });
        }
        {
            let st = st.clone();
            swatch.set_draw_func(move |_, cr, w, h| {
                let c = st.rgb();
                cr.set_source_rgb(c[0] as f64, c[1] as f64, c[2] as f64);
                cr.rectangle(0.0, 0.0, w as f64, h as f64);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.45);
                cr.set_line_width(1.5);
                let _ = cr.stroke();
            });
        }
        let picker_root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        picker_root.set_margin_top(10);
        picker_root.set_margin_bottom(10);
        picker_root.set_margin_start(12);
        picker_root.set_margin_end(12);
        picker_root.append(&area);
        picker_root.append(&hue);
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::new(Some("Hex"));
        label.add_css_class("dim-label");
        line.append(&label);
        line.append(&entry);
        picker_root.append(&line);
        let p = Picker {
            root: picker_root,
            swatch,
            st,
        };

        // Pick on the square: press and drag.
        let drag = gtk::GestureDrag::new();
        let start = Rc::new(Cell::new((0.0f64, 0.0f64)));
        {
            let (p, start) = (p.clone(), start.clone());
            drag.connect_drag_begin(move |_, x, y| {
                start.set((x, y));
                p.pick(x, y);
            });
        }
        {
            let (p, start) = (p.clone(), start.clone());
            drag.connect_drag_update(move |_, dx, dy| {
                let (x, y) = start.get();
                p.pick(x + dx, y + dy);
            });
        }
        area.add_controller(drag);
        {
            let p2 = p.clone();
            hue.connect_value_changed(move |s| {
                if p2.st.quiet.get() {
                    return;
                }
                let mut c = p2.st.hsv.get();
                c[0] = (s.value() / 360.0) as f32;
                p2.st.hsv.set(c);
                p2.changed();
            });
        }
        {
            let p2 = p.clone();
            entry.connect_changed(move |e| {
                if p2.st.quiet.get() {
                    return;
                }
                if let Some(rgb) = parse_hex(&e.text()) {
                    // Keep the hue when the colour is grey, so the slider doesn't jump.
                    let mut c = to_hsv(rgb);
                    let old = p2.st.hsv.get();
                    if c[1] < 0.01 || c[2] < 0.01 {
                        c[0] = old[0];
                    }
                    p2.st.hsv.set(c);
                    p2.sync(false);
                    p2.report();
                }
            });
        }
        p
    }

    fn pick(&self, x: f64, y: f64) {
        let (w, h) = (
            self.st.area.width().max(1) as f64,
            self.st.area.height().max(1) as f64,
        );
        let mut c = self.st.hsv.get();
        c[1] = (x / w).clamp(0.0, 1.0) as f32;
        c[2] = (1.0 - y / h).clamp(0.0, 1.0) as f32;
        self.st.hsv.set(c);
        self.changed();
    }

    /// The colour changed because of the user: refresh the other parts and report it.
    fn changed(&self) {
        self.sync(true);
        self.report();
    }

    /// Refreshes the square, slider, hex field and swatches (without reporting anything).
    fn sync(&self, update_entry: bool) {
        let prev = self.st.quiet.replace(true);
        let c = self.st.hsv.get();
        self.st.hue.set_value(c[0] as f64 * 360.0);
        if update_entry {
            self.st.entry.set_text(&hex(self.st.rgb()));
        }
        self.st.area.queue_draw();
        self.st.swatch.queue_draw();
        self.st.quiet.set(prev);
    }

    fn report(&self) {
        if self.st.quiet.get() {
            return;
        }
        if let Some(f) = self.st.on_change.borrow().as_ref() {
            f(hex(self.st.rgb()));
        }
    }

    /// Shows a colour without reporting a change.
    pub fn set_hex(&self, value: &str) {
        let Some(rgb) = parse_hex(value) else { return };
        let old = self.st.hsv.get();
        let mut c = to_hsv(rgb);
        if c[1] < 0.01 || c[2] < 0.01 {
            c[0] = old[0];
        }
        if hex(hsv(c[0], c[1], c[2])) == hex(self.st.rgb())
            && self.st.entry.text().eq_ignore_ascii_case(value)
        {
            return;
        }
        self.st.hsv.set(c);
        self.sync(true);
    }

    pub fn connect_changed(&self, f: impl Fn(String) + 'static) {
        *self.st.on_change.borrow_mut() = Some(Box::new(f));
    }
}

#[allow(dead_code)]
fn _unused(_: &glib::MainContext) {}
