mod preview;
mod theme;

use adw::prelude::*;
use gtk::glib;
use libadwaita as adw;
use windfall_core::config::{BarMode, Config, Effect, Element, Patch, Renderer};
use windfall_core::ipc::{self, Request, Response};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader};
use std::rc::Rc;
use std::sync::mpsc;

const APP_ID: &str = "io.github.tak0dan.Windfall";

fn main() -> glib::ExitCode {
    // `windfall-gui --render <out.png> <effect> [element] [smooth|pixel] [seconds]`: a picture of the preview.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--render") {
        let mut c = Config::default();
        c.effect = args.get(3).and_then(|s| Effect::parse(s)).unwrap_or(Effect::Snow);
        c.element = args.get(4).and_then(|s| Element::parse(s)).unwrap_or(Element::Natural);
        c.renderer = args.get(5).and_then(|s| Renderer::parse(s)).unwrap_or(Renderer::Smooth);
        c.intensity = 0.8;
        let secs = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(25.0);
        let out = args.get(2).cloned().unwrap_or_else(|| "preview.png".into());
        return match preview::render_png(&c, secs, 640, 260, &out) {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("windfall-gui: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build);
    app.run()
}

/// Makes sure the daemon is there, starting it if it is not.
fn ensure_daemon() -> bool {
    if ipc::connect().is_ok() {
        return true;
    }
    let _ = std::process::Command::new("windfalld").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if ipc::connect().is_ok() {
            return true;
        }
    }
    false
}

struct Ui {
    updating: Cell<bool>,
    config: RefCell<Config>,
    send: mpsc::Sender<Request>,
    provider: gtk::CssProvider,
    preview: preview::Preview,
    effect_buttons: Vec<(Effect, gtk::ToggleButton)>,
    renderer_buttons: Vec<(Renderer, gtk::ToggleButton)>,
    enabled: adw::SwitchRow,
    intensity: gtk::Scale,
    opacity: gtk::Scale,
    wind: gtk::Scale,
    pixel_size: gtk::Scale,
    pixel_levels: gtk::Scale,
    pixel_group: adw::PreferencesGroup,
    melt: adw::SwitchRow,
    ground: adw::SwitchRow,
    windows: adw::SwitchRow,
    layers: adw::SwitchRow,
    bar_mode: adw::ComboRow,
    glow_row: adw::ActionRow,
    element_row: adw::ComboRow,
    glow_button: gtk::ColorDialogButton,
    interact: adw::SwitchRow,
    over_fs: adw::SwitchRow,
    banner: adw::Banner,
}

impl Ui {
    fn patch(&self, p: Patch) {
        if self.updating.get() {
            return;
        }
        let _ = self.send.send(Request::Set { patch: p });
    }

    /// Reflects the daemon's settings in every widget without echoing them back.
    fn show(&self, c: &Config) {
        self.updating.set(true);
        let old = self.config.replace(c.clone());
        for (e, b) in &self.effect_buttons {
            b.set_active(*e == c.effect);
        }
        for (r, b) in &self.renderer_buttons {
            b.set_active(*r == c.renderer);
        }
        self.enabled.set_active(c.enabled);
        self.intensity.set_value((c.intensity * 100.0) as f64);
        self.opacity.set_value((c.opacity * 100.0) as f64);
        self.wind.set_value((c.wind * 100.0) as f64);
        self.pixel_size.set_value(c.pixel_size as f64);
        self.pixel_levels.set_value(c.pixel_levels as f64);
        self.pixel_group.set_visible(c.renderer == Renderer::Pixel);
        self.melt.set_active(c.melt);
        self.ground.set_active(c.ground);
        self.windows.set_active(c.windows);
        self.layers.set_active(c.layers);
        self.bar_mode.set_selected(BarMode::ALL.iter().position(|m| *m == c.bar_mode).unwrap_or(0) as u32);
        self.bar_mode.set_sensitive(c.layers);
        self.glow_row.set_visible(c.effect == Effect::Flowers);
        self.element_row.set_visible(c.effect == Effect::Flowers);
        self.element_row.set_selected(Element::ALL.iter().position(|e| *e == c.element).unwrap_or(0) as u32);
        if let Some(rgb) = windfall_core::palette::parse_hex(&c.glow_color) {
            self.glow_button.set_rgba(&gtk::gdk::RGBA::new(rgb[0], rgb[1], rgb[2], 1.0));
        }
        self.interact.set_active(c.interact);
        self.over_fs.set_active(c.over_fullscreen);
        self.preview.set_config(c);
        if old.effect != c.effect || old.renderer != c.renderer || old.element != c.element || self.provider.to_str().is_empty() {
            self.provider.load_from_string(&theme::css(c.effect, c.renderer, c.element));
        }
        self.updating.set(false);
    }
}

fn slider(min: f64, max: f64, step: f64) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    s.set_width_request(190);
    s.set_draw_value(true);
    s.set_value_pos(gtk::PositionType::Right);
    s.set_valign(gtk::Align::Center);
    s
}

fn scale_row(title: &str, scale: &gtk::Scale) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(scale);
    row
}

fn switch_row(title: &str, subtitle: &str) -> adw::SwitchRow {
    adw::SwitchRow::builder().title(title).subtitle(subtitle).build()
}

fn build(app: &adw::Application) {
    if let Some(w) = app.active_window() {
        w.present();
        return;
    }
    // Always dark: the effect palettes are dark themes of their own.
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    let provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
    }

    let (send, recv) = mpsc::channel::<Request>();
    let config = Config::load();

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar.add_top_bar(&header);
    let banner = adw::Banner::new("Windfall isn't running");
    banner.set_button_label(Some("Start"));
    toolbar.add_top_bar(&banner);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.set_margin_top(10);
    page.set_margin_bottom(18);
    page.set_margin_start(16);
    page.set_margin_end(16);
    let preview = preview::Preview::new(config.clone());
    page.append(&preview.area);

    // Effect and renderer pickers.
    let picker = |labels: Vec<(String, usize)>| -> (gtk::Box, Vec<gtk::ToggleButton>) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("linked");
        row.add_css_class("effect-row");
        row.set_homogeneous(true);
        let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
        for (label, _) in labels {
            let b = gtk::ToggleButton::with_label(&label);
            if let Some(first) = buttons.first() {
                b.set_group(Some(first));
            }
            row.append(&b);
            buttons.push(b);
        }
        (row, buttons)
    };
    // Eight effects in two rows, all in one radio group.
    let short = |e: &Effect| match e {
        Effect::Leaves => "Leaves",
        Effect::Blossom => "Blossom",
        Effect::Comets => "Comets",
        Effect::Lights => "Lights",
        Effect::Hearts => "Hearts",
        other => other.title(),
    };
    let (effect_row_a, mut ebuttons) = picker(Effect::ALL[..4].iter().enumerate().map(|(i, e)| (short(e).to_string(), i)).collect());
    let (effect_row_b, ebuttons_b) = picker(Effect::ALL[4..].iter().enumerate().map(|(i, e)| (short(e).to_string(), i)).collect());
    for b in &ebuttons_b {
        b.set_group(Some(&ebuttons[0]));
    }
    ebuttons.extend(ebuttons_b);
    let (renderer_row, rbuttons) = picker(Renderer::ALL.iter().enumerate().map(|(i, r)| (r.title().to_string(), i)).collect());
    page.append(&effect_row_a);
    page.append(&effect_row_b);
    page.append(&renderer_row);

    let intensity = slider(0.0, 100.0, 1.0);
    let opacity = slider(10.0, 100.0, 1.0);
    let wind = slider(-100.0, 100.0, 1.0);
    let pixel_size = slider(1.0, 10.0, 1.0);
    let pixel_levels = slider(2.0, 16.0, 1.0);
    let main_group = adw::PreferencesGroup::new();
    main_group.add(&scale_row("Intensity", &intensity));
    main_group.add(&scale_row("Opacity", &opacity));
    main_group.add(&scale_row("Wind", &wind));
    let element_row = adw::ComboRow::builder().title("Flower element").subtitle("What the flowers are made of").build();
    element_row.set_model(Some(&gtk::StringList::new(&Element::ALL.map(|e| e.title()))));
    main_group.add(&element_row);
    page.append(&main_group);

    let pixel_group = adw::PreferencesGroup::builder().title("Pixel art").build();
    pixel_group.add(&scale_row("Pixel size", &pixel_size));
    pixel_group.add(&scale_row("Colours", &pixel_levels));
    page.append(&pixel_group);

    let enabled = switch_row("Display", "Show the effect on every monitor; remembered between sessions");
    let melt = switch_row("Melt and wither", "Piles slowly disappear on their own");
    let ground = switch_row("Bottom of the screen", "Collect along the screen's lower edge");
    let windows = switch_row("Windows", "Collect on top of windows");
    let layers = switch_row("Bars and panels", "Waybar, Quickshell and other shell surfaces");
    let bar_mode = adw::ComboRow::builder().title("Bars hold things on").subtitle("Transparent bars: only their visible modules").build();
    bar_mode.set_model(Some(&gtk::StringList::new(&BarMode::ALL.map(|m| m.title()))));
    let glow_button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
    glow_button.set_valign(gtk::Align::Center);
    let glow_row = adw::ActionRow::builder().title("Glow colour").subtitle("Flowers glow and sparkle in this colour").build();
    glow_row.add_suffix(&glow_button);
    let interact = switch_row("React to the cursor", "Push snow aside, brush flowers");
    let over_fs = switch_row("Over fullscreen", "Keep drawing above fullscreen windows");
    let behaviour = adw::PreferencesGroup::new();
    for r in [&enabled, &melt, &ground, &windows, &layers] {
        behaviour.add(r);
    }
    behaviour.add(&bar_mode);
    behaviour.add(&glow_row);
    for r in [&interact, &over_fs] {
        behaviour.add(r);
    }
    page.append(&behaviour);

    let clear = gtk::Button::with_label("Clear everything");
    clear.set_halign(gtk::Align::Center);
    page.append(&clear);

    let scroller = gtk::ScrolledWindow::builder().child(&page).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();
    toolbar.set_content(Some(&scroller));
    let window = adw::ApplicationWindow::builder().application(app).title("Windfall Controller").default_width(460).default_height(900).resizable(false).content(&toolbar).build();

    let ui = Rc::new(Ui {
        updating: Cell::new(false),
        config: RefCell::new(config.clone()),
        send: send.clone(),
        provider,
        preview,
        effect_buttons: Effect::ALL.iter().copied().zip(ebuttons).collect(),
        renderer_buttons: Renderer::ALL.iter().copied().zip(rbuttons).collect(),
        enabled,
        intensity,
        opacity,
        wind,
        pixel_size,
        pixel_levels,
        pixel_group,
        melt,
        ground,
        windows,
        layers,
        bar_mode,
        glow_row,
        element_row,
        glow_button,
        interact,
        over_fs,
        banner,
    });

    // Widget -> daemon.
    for (e, b) in &ui.effect_buttons {
        let (ui, e) = (ui.clone(), *e);
        b.connect_toggled(move |b| if b.is_active() { ui.patch(Patch { effect: Some(e), ..Default::default() }) });
    }
    for (r, b) in &ui.renderer_buttons {
        let (ui, r) = (ui.clone(), *r);
        b.connect_toggled(move |b| if b.is_active() { ui.patch(Patch { renderer: Some(r), ..Default::default() }) });
    }
    macro_rules! scale {
        ($w:ident, $field:ident, $conv:expr) => {{
            let ui2 = ui.clone();
            ui.$w.connect_value_changed(move |s| {
                let v = s.value();
                ui2.patch(Patch { $field: Some($conv(v)), ..Default::default() })
            });
        }};
    }
    scale!(intensity, intensity, |v: f64| (v / 100.0) as f32);
    scale!(opacity, opacity, |v: f64| (v / 100.0) as f32);
    scale!(wind, wind, |v: f64| (v / 100.0) as f32);
    scale!(pixel_size, pixel_size, |v: f64| v as u32);
    scale!(pixel_levels, pixel_levels, |v: f64| v as u32);
    macro_rules! toggle {
        ($w:ident, $field:ident) => {{
            let ui2 = ui.clone();
            ui.$w.connect_active_notify(move |s| ui2.patch(Patch { $field: Some(s.is_active()), ..Default::default() }));
        }};
    }
    toggle!(enabled, enabled);
    toggle!(melt, melt);
    toggle!(ground, ground);
    toggle!(windows, windows);
    toggle!(layers, layers);
    toggle!(interact, interact);
    {
        let ui2 = ui.clone();
        ui.element_row.connect_selected_notify(move |r| {
            if let Some(e) = Element::ALL.get(r.selected() as usize) {
                ui2.patch(Patch { element: Some(*e), ..Default::default() })
            }
        });
    }
    {
        let ui2 = ui.clone();
        ui.glow_button.connect_rgba_notify(move |b| {
            let c = b.rgba();
            let hex = windfall_core::palette::hex([c.red(), c.green(), c.blue()]);
            ui2.patch(Patch { glow_color: Some(hex), ..Default::default() })
        });
    }
    {
        let ui2 = ui.clone();
        ui.bar_mode.connect_selected_notify(move |r| {
            if let Some(m) = BarMode::ALL.get(r.selected() as usize) {
                ui2.patch(Patch { bar_mode: Some(*m), ..Default::default() })
            }
        });
    }
    toggle!(over_fs, over_fullscreen);
    {
        let send = send.clone();
        clear.connect_clicked(move |_| {
            let _ = send.send(Request::Clear);
        });
    }
    ui.show(&config);

    // Requests go out on one worker thread, in order.
    std::thread::spawn(move || {
        for req in recv {
            let _ = ipc::request(&req);
        }
    });

    // Daemon -> widgets: a subscriber thread feeds the main loop.
    let (tx, rx) = async_channel::unbounded::<Option<Config>>();
    std::thread::spawn(move || loop {
        if ensure_daemon() {
            if let Ok(mut stream) = ipc::connect() {
                if ipc::send(&mut stream, &Request::Subscribe).is_ok() {
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        if let Ok(Response::Ok { state }) = serde_json::from_str(&line) {
                            if tx.send_blocking(Some(state.config)).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
        // Lost it (or never had it): say so, then try again.
        if tx.send_blocking(None).is_err() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
    {
        let ui = ui.clone();
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                match msg {
                    Some(c) => {
                        ui.banner.set_revealed(false);
                        ui.show(&c);
                    }
                    None => ui.banner.set_revealed(true),
                }
            }
        });
    }
    ui.banner.connect_button_clicked(|_| {
        let _ = std::process::Command::new("windfalld").spawn();
    });
    window.present();
}
