//! Windfall's daemon: a click-through overlay with the effects, a tray icon and a control channel.
//! Linux (Hyprland) and Windows have their own backends; the simulation is shared (windfall-core).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod raster;
#[cfg(unix)]
mod capture;
#[cfg(unix)]
mod gpu;
#[cfg(unix)]
mod hypr;
#[cfg(unix)]
mod msg;
#[cfg(unix)]
mod server;
#[cfg(unix)]
mod tray;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(unix)]
    unix::main();
    #[cfg(windows)]
    win::main();
}
