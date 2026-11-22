//! Windfall's daemon: a click-through overlay with the effects, a tray icon and a control channel.
//! Linux (Wayland and X11) and Windows have their own backends; the simulation is shared (windfall-core).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(unix)]
mod capture;
#[cfg(unix)]
mod gpu;
#[cfg(unix)]
mod hypr;
#[cfg(unix)]
mod msg;
mod raster;
#[cfg(unix)]
mod server;
#[cfg(unix)]
mod tray;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod win;
#[cfg(unix)]
mod x11;

fn main() {
    #[cfg(unix)]
    if std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("x11"))
        || (std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some())
    {
        x11::main();
    } else {
        unix::main();
    }
    #[cfg(windows)]
    win::main();
}
