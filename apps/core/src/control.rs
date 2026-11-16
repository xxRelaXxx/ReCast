//! The local control channel between the `recast` command line tool and the
//! running desktop application.
//!
//! The desktop app opens a TCP listener on the loopback interface only and
//! writes the port and a random token to
//! `%LOCALAPPDATA%\ReCast\control.json`. The CLI reads that file, connects and
//! exchanges one JSON object per line.
//!
//! The channel is local-only and authenticated with a token that lives in the
//! user's profile directory, so it is not reachable from the network.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Name of the file that describes the running app.
pub const CONTROL_FILE: &str = "control.json";

/// A command the CLI can send to the desktop app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Report the current state.
    Status,
    /// Switch the phone camera.
    SetCamera { id: String },
    /// Change the stream resolution and frame rate.
    SetResolution { width: u32, height: u32, fps: u32 },
    /// Mute or unmute the phone microphone.
    SetMic { enabled: bool },
    /// Turn the phone torch on or off.
    SetTorch { enabled: bool },
    /// Set the digital zoom, 1.0 to 8.0.
    SetZoom { value: f32 },
    /// Rotate the output by a number of degrees.
    Rotate { degrees: u32 },
    /// Switch the interface theme.
    SetTheme { kind: String },
    /// Install the phone app over USB, open the tunnel and start the app.
    UsbSetup,
}

/// A command that waits for the interface thread to apply it.
#[derive(Debug)]
pub struct PendingCommand {
    pub command: Command,
    pub reply: std::sync::mpsc::Sender<Reply>,
}

/// The answer of the desktop app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl Reply {
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            error: String::new(),
            data,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: message.into(),
            data: serde_json::Value::Null,
        }
    }
}

/// Path of the control file.
pub fn control_file() -> Option<PathBuf> {
    Some(super::paths::install_dir()?.join(CONTROL_FILE))
}

/// The connection details of the running app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
}

/// Reads the control file, if the app is running.
pub fn read_endpoint() -> Option<Endpoint> {
    let path = control_file()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Accepts control connections on the loopback interface.
///
/// Every connection is answered with exactly one JSON line. The handler is
/// called on a worker thread, so it must not block for long.
pub fn serve<F>(listener: std::net::TcpListener, token: String, handler: F) -> !
where
    F: Fn(Command) -> Reply + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let handler = handler.clone();
        let token = token.clone();
        std::thread::spawn(move || {
            let peer = stream
                .peer_addr()
                .map(|a| a.ip().to_string())
                .unwrap_or_default();
            // Only the local machine may talk to the control channel.
            if !peer.starts_with("127.") && peer != "::1" {
                let mut stream = stream;
                let _ = writeln!(
                    stream,
                    "{}",
                    serde_json::to_string(&Reply::error("Local connections only.")).unwrap()
                );
                return;
            }

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                return;
            }

            let reply = match serde_json::from_str::<serde_json::Value>(line.trim()) {
                Ok(value) => {
                    if value.get("token").and_then(|t| t.as_str()) != Some(token.as_str()) {
                        Reply::error("Wrong token.")
                    } else {
                        match value.get("cmd").cloned() {
                            Some(cmd) => match serde_json::from_value::<Command>(cmd) {
                                Ok(command) => handler(command),
                                Err(e) => Reply::error(format!("Unknown command: {e}")),
                            },
                            None => Reply::error("No command in the request."),
                        }
                    }
                }
                Err(e) => Reply::error(format!("Malformed request: {e}")),
            };

            let mut stream = stream;
            let _ = writeln!(
                stream,
                "{}",
                serde_json::to_string(&reply).unwrap_or_else(|_| {
                    "{\"ok\":false,\"error\":\"Cannot encode the answer.\"}".to_string()
                })
            );
            let _ = stream.flush();
        });
    }
    std::process::exit(0)
}

/// Sends one command to the running app.
pub fn send(command: &Command) -> Result<Reply, String> {
    let endpoint = read_endpoint().ok_or_else(|| {
        "ReCast is not running. Start desktop\\ReCast.exe and try again.".to_string()
    })?;

    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port))
        .map_err(|e| format!("Cannot reach the running ReCast app: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;

    let request = serde_json::json!({
        "token": endpoint.token,
        "cmd": command,
    });
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("Cannot send the command: {e}"))?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .map_err(|e| format!("Cannot read the answer: {e}"))?;
    if response.trim().is_empty() {
        return Err("The running ReCast app closed the connection.".to_string());
    }
    serde_json::from_str(response.trim()).map_err(|e| format!("Unexpected answer: {e}"))
}
