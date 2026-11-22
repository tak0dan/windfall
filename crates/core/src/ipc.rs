use crate::config::{Config, Patch};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Unix socket on Linux; on Windows a TCP connection to the daemon on the loopback interface.
#[cfg(unix)]
pub type Stream = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub type Stream = std::net::TcpStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Get,
    Set {
        patch: Patch,
    },
    Toggle,
    /// Blow away everything that has piled up.
    Clear,
    /// Keep the connection open and receive a `State` line on every change.
    Subscribe,
    Quit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub config: Config,
    pub version: String,
    pub monitors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok { state: State },
    Error { message: String },
}

#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("windfall.sock")
}

/// Where the Windows daemon writes the port it listens on.
#[cfg(windows)]
pub fn port_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("windfall").join("port")
}

#[cfg(windows)]
pub fn port() -> std::io::Result<u16> {
    std::fs::read_to_string(port_path())?
        .trim()
        .parse()
        .map_err(|_| std::io::Error::other("bad port file"))
}

pub fn connect() -> std::io::Result<Stream> {
    #[cfg(unix)]
    {
        Stream::connect(socket_path())
    }
    #[cfg(windows)]
    {
        let s = Stream::connect(("127.0.0.1", port()?))?;
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        Ok(s)
    }
}

/// One request, one response.
pub fn request(req: &Request) -> std::io::Result<Response> {
    let mut stream = connect()?;
    send(&mut stream, req)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

pub fn send(stream: &mut Stream, req: &Request) -> std::io::Result<()> {
    let mut text = serde_json::to_string(req).map_err(std::io::Error::other)?;
    text.push('\n');
    stream.write_all(text.as_bytes())
}
