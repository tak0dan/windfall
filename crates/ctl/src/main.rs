use windfall_core::config::{BarMode, Effect, Element, Patch, Renderer};
use windfall_core::ipc::{self, Request, Response};
use std::io::{BufRead, BufReader};

const HELP: &str = "windfallctl - control the running windfalld

usage: windfallctl <command>

  status [--json]       show the current settings
  toggle | on | off     show or hide the effect (remembered between sessions)
  effect <name>         snow | leaves | blossom | rain | flowers | comets | lights | hearts
  element <name>        what the flowers are made of: natural | lightning | fire | ice | matter | void
  lights [on|off]       floating lights over any effect, in its colours (yellow alone)
  renderer <name>       smooth | pixel
  intensity <0-100>     how much falls or grows
  opacity <10-100>      overall opacity
  pixel-size <1-10>     size of one art pixel (pixel renderer)
  bars <visible|whole>  grow on a bar's painted parts only, or on all of it
  glow <#rrggbb>        colour of the flowers' glow
  wind <-100..100>      sideways drift
  set <key>=<value>...  melt, ground, windows, layers, interact, over_fullscreen = true|false
  clear                 sweep away everything that has settled
  watch                 print the settings as JSON on every change
  quit                  stop the daemon";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("windfallctl: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn num(s: Option<&String>, lo: f32, hi: f32) -> Result<f32, String> {
    let v: f32 = s.ok_or("missing value")?.trim_end_matches('%').parse().map_err(|_| "not a number".to_string())?;
    if v < lo || v > hi {
        return Err(format!("{v} is outside {lo}..{hi}"));
    }
    Ok(v)
}

fn flag(s: &str) -> Result<bool, String> {
    match s {
        "true" | "on" | "1" | "yes" => Ok(true),
        "false" | "off" | "0" | "no" => Ok(false),
        _ => Err(format!("`{s}` is not true or false")),
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    let set = |p: Patch| Request::Set { patch: p };
    let req = match cmd {
        "-h" | "--help" | "help" => {
            println!("{HELP}");
            return Ok(());
        }
        "status" => Request::Get,
        "toggle" => Request::Toggle,
        "on" => set(Patch { enabled: Some(true), ..Default::default() }),
        "off" => set(Patch { enabled: Some(false), ..Default::default() }),
        "clear" => Request::Clear,
        "quit" => Request::Quit,
        "effect" => set(Patch { effect: Some(args.get(1).and_then(|s| Effect::parse(s)).ok_or("unknown effect")?), ..Default::default() }),
        "element" => set(Patch { element: Some(args.get(1).and_then(|s| Element::parse(s)).ok_or("unknown element")?), ..Default::default() }),
        "lights" => set(Patch { lights: Some(args.get(1).map(|v| flag(v)).transpose()?.unwrap_or(true)), ..Default::default() }),
        "renderer" => set(Patch { renderer: Some(args.get(1).and_then(|s| Renderer::parse(s)).ok_or("unknown renderer")?), ..Default::default() }),
        "intensity" => set(Patch { intensity: Some(num(args.get(1), 0.0, 100.0)? / 100.0), ..Default::default() }),
        "opacity" => set(Patch { opacity: Some(num(args.get(1), 10.0, 100.0)? / 100.0), ..Default::default() }),
        "pixel-size" => set(Patch { pixel_size: Some(num(args.get(1), 1.0, 10.0)? as u32), ..Default::default() }),
        "bars" => set(Patch { bar_mode: Some(args.get(1).and_then(|s| BarMode::parse(s)).ok_or("expected visible or whole")?), ..Default::default() }),
        "glow" => set(Patch { glow_color: Some(args.get(1).filter(|c| windfall_core::palette::parse_hex(c).is_some()).ok_or("expected a colour like #ffd84d")?.clone()), ..Default::default() }),
        "wind" => set(Patch { wind: Some(num(args.get(1), -100.0, 100.0)? / 100.0), ..Default::default() }),
        "set" => {
            let mut p = Patch::default();
            for kv in &args[1..] {
                let (k, v) = kv.split_once('=').ok_or("expected key=value")?;
                let b = flag(v)?;
                match k {
                    "lights" => p.lights = Some(b),
                    "melt" => p.melt = Some(b),
                    "ground" => p.ground = Some(b),
                    "windows" => p.windows = Some(b),
                    "layers" => p.layers = Some(b),
                    "interact" => p.interact = Some(b),
                    "over_fullscreen" => p.over_fullscreen = Some(b),
                    _ => return Err(format!("unknown key `{k}`")),
                }
            }
            set(p)
        }
        "watch" => {
            let mut stream = ipc::connect().map_err(not_running)?;
            ipc::send(&mut stream, &Request::Subscribe).map_err(|e| e.to_string())?;
            for line in BufReader::new(stream).lines() {
                println!("{}", line.map_err(|e| e.to_string())?);
            }
            return Ok(());
        }
        other => return Err(format!("unknown command `{other}` (try --help)")),
    };
    match ipc::request(&req).map_err(not_running)? {
        Response::Error { message } => Err(message),
        Response::Ok { state } => {
            if cmd == "status" {
                if args.iter().any(|a| a == "--json") {
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                } else {
                    let c = &state.config;
                    println!("windfalld {} on {}", state.version, state.monitors.join(", "));
                    println!("{}  {} / {}  intensity {:.0}%  opacity {:.0}%", if c.enabled { "on " } else { "off" }, c.effect.title(), c.renderer.title(), c.intensity * 100.0, c.opacity * 100.0);
                }
            }
            Ok(())
        }
    }
}

fn not_running(e: std::io::Error) -> String {
    format!("cannot reach windfalld ({e}); is it running? Try: systemctl --user start windfall")
}
