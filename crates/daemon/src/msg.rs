use crate::hypr::Snapshot;
use windfall_core::config::Patch;
use windfall_core::shape::Segment;
use windfall_core::ipc::{Request, Response};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

/// Everything that reaches the main loop from other threads.
pub enum Msg {
    Snapshot(Snapshot),
    Patch(Patch),
    Toggle,
    Clear,
    OpenGui,
    /// The tray service is up (it may take a while at login, until the bar is there).
    TrayReady(ksni::blocking::Handle<crate::tray::Tray>),
    Quit,
    /// Blank (or restore) a monitor's overlay so a bar can be photographed without our own drawing on it.
    Hide { monitor: String, bands: Vec<[i32; 4]> },
    /// What was found of one layer surface; `None` if it could not be photographed.
    Mask { id: u64, segs: Option<Vec<Segment>> },
    /// A calibration pass for the monitor finished.
    Calibrated { monitor: String, key: Vec<(u64, [i32; 4])> },
    Ipc { req: Request, reply: mpsc::Sender<Response>, subscriber: Option<UnixStream> },
}
