//! The control socket: JSON lines over `$XDG_RUNTIME_DIR/windfall.sock`.

use crate::msg::Msg;
use windfall_core::ipc::{self, Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::mpsc;
use std::time::Duration;

/// Fails with `AddrInUse` if another daemon is alive.
pub fn start(tx: calloop::channel::Sender<Msg>) -> std::io::Result<()> {
    let path = ipc::socket_path();
    if path.exists() {
        if ipc::connect().is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)?;
    std::thread::Builder::new().name("ipc".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || handle(stream, tx));
        }
    })?;
    Ok(())
}

fn handle(mut stream: std::os::unix::net::UnixStream, tx: calloop::channel::Sender<Msg>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Request>(&line) {
        Err(e) => Response::Error { message: format!("bad request: {e}") },
        Ok(req) => {
            let (rtx, rrx) = mpsc::channel();
            let subscriber = if matches!(req, Request::Subscribe) { stream.try_clone().ok() } else { None };
            if tx.send(Msg::Ipc { req, reply: rtx, subscriber }).is_err() {
                return;
            }
            rrx.recv_timeout(Duration::from_secs(2)).unwrap_or(Response::Error { message: "daemon busy".into() })
        }
    };
    let mut text = serde_json::to_string(&reply).unwrap_or_default();
    text.push('\n');
    let _ = stream.write_all(text.as_bytes());
}
