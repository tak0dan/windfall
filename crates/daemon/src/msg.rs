use crate::hypr::Snapshot;
use snowhush_core::config::Patch;
use snowhush_core::ipc::{Request, Response};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

/// Everything that reaches the main loop from other threads.
pub enum Msg {
    Snapshot(Snapshot),
    Patch(Patch),
    Toggle,
    Clear,
    OpenGui,
    Quit,
    Ipc { req: Request, reply: mpsc::Sender<Response>, subscriber: Option<UnixStream> },
}
