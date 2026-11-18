use crate::config::{Config, Patch};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Get,
    Set { patch: Patch },
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

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join("snowhush.sock")
}

pub fn connect() -> std::io::Result<UnixStream> {
    UnixStream::connect(socket_path())
}

/// One request, one response.
pub fn request(req: &Request) -> std::io::Result<Response> {
    let mut stream = connect()?;
    send(&mut stream, req)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

pub fn send(stream: &mut UnixStream, req: &Request) -> std::io::Result<()> {
    let mut text = serde_json::to_string(req).map_err(std::io::Error::other)?;
    text.push('\n');
    stream.write_all(text.as_bytes())
}
