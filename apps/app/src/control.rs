//! The local control channel of the desktop application.
//!
//! The `recast` command line tool sends commands here. The listener is bound
//! to the loopback interface only and every request has to carry the random
//! token that is stored in the user's profile directory.

use std::net::TcpListener;
use std::sync::Arc;

use serde_json::json;

use recast_core::control::{self, Command, Reply};

use crate::engine::Engine;

/// Starts the control channel in a background thread.
///
/// Returns an error when the port cannot be opened or the control file
/// cannot be written; the application then simply runs without a CLI.
pub fn start(engine: Arc<Engine>) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("Cannot open the control port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Cannot read the control port: {e}"))?
        .port();

    let token: String = (0..32)
        .map(|_| {
            let n = rand::random::<u8>() % 36;
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + n - 10) as char
            }
        })
        .collect();

    let file =
        recast_core::control::control_file().ok_or("LOCALAPPDATA is not available on this PC.")?;
    let endpoint = control::Endpoint {
        port,
        token: token.clone(),
    };
    let text = serde_json::to_string_pretty(&endpoint)
        .map_err(|e| format!("Cannot encode the control file: {e}"))?;
    std::fs::write(&file, text).map_err(|e| format!("Cannot write {}: {e}", file.display()))?;

    std::thread::Builder::new()
        .name("recast-control".into())
        .spawn(move || {
            control::serve(listener, token, move |command| handle(&engine, command));
        })
        .map_err(|e| format!("Cannot start the control thread: {e}"))?;

    Ok(())
}

fn handle(engine: &Arc<Engine>, command: Command) -> Reply {
    match command {
        Command::Status => Reply::ok(status(engine)),

        Command::SetCamera { id } => {
            let (cameras, current) = {
                let s = engine.shared.lock();
                let cameras = s
                    .caps
                    .as_ref()
                    .map(|c| {
                        c.cameras
                            .iter()
                            .map(|cam| {
                                json!({
                                    "id": cam.id,
                                    "label": cam.label,
                                    "facing": cam.facing,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (cameras, s.selected_camera.clone())
            };

            // Accept the id, the label or the facing direction.
            let target = {
                let s = engine.shared.lock();
                let caps = match s.caps.as_ref() {
                    Some(caps) => caps,
                    None => return Reply::error("The phone has not sent its camera list yet."),
                };
                let by_id = caps.cameras.iter().find(|c| c.id == id);
                let by_label = caps
                    .cameras
                    .iter()
                    .find(|c| c.label.eq_ignore_ascii_case(&id));
                let by_facing = caps
                    .cameras
                    .iter()
                    .find(|c| c.facing.eq_ignore_ascii_case(&id));
                by_id.or(by_label).or(by_facing).map(|c| c.id.clone())
            };

            let Some(target) = target else {
                let names: Vec<String> = cameras
                    .iter()
                    .filter_map(|c| c.get("label").and_then(|l| l.as_str()).map(String::from))
                    .collect();
                return Reply::error(format!(
                    "Unknown camera '{id}'. Available: {}.",
                    names.join(", ")
                ));
            };

            if target == current {
                return Reply::ok(json!({ "camera": target }));
            }

            engine.send_control(crate::protocol::Control::SetCamera { id: target.clone() });
            {
                let mut s = engine.shared.lock();
                s.selected_camera = target.clone();
            }
            Reply::ok(json!({ "camera": target }))
        }

        Command::SetResolution { width, height, fps } => {
            let (resolution, allowed) = {
                let s = engine.shared.lock();
                let caps = match s.caps.as_ref() {
                    Some(caps) => caps,
                    None => return Reply::error("The phone has not sent its camera list yet."),
                };
                let cam = caps
                    .cameras
                    .iter()
                    .find(|c| c.id == s.selected_camera)
                    .or_else(|| caps.cameras.first());
                let Some(cam) = cam else {
                    return Reply::error("The phone reported no cameras.");
                };
                let allowed: Vec<String> = cam
                    .resolutions
                    .iter()
                    .map(|r| format!("{}x{}", r.width, r.height))
                    .collect();
                let resolution = cam
                    .resolutions
                    .iter()
                    .find(|r| r.width == width && r.height == height)
                    .cloned();
                (resolution, allowed)
            };

            let Some(resolution) = resolution else {
                return Reply::error(format!(
                    "The camera does not support {width}x{height}. Available: {}.",
                    allowed.join(", ")
                ));
            };

            let fps = if resolution.fps.contains(&fps) {
                fps
            } else {
                resolution.fps.first().copied().unwrap_or(30)
            };

            engine.send_control(crate::protocol::Control::SetResolution { width, height, fps });
            {
                let mut s = engine.shared.lock();
                s.selected_resolution = Some(resolution);
                s.selected_fps = fps;
            }
            Reply::ok(json!({ "width": width, "height": height, "fps": fps }))
        }

        Command::SetMic { enabled } => {
            engine.send_control(crate::protocol::Control::SetMic { enabled });
            engine.shared.lock().phone_mic = enabled;
            Reply::ok(json!({ "mic": enabled }))
        }

        Command::SetTorch { enabled } => {
            engine.send_control(crate::protocol::Control::SetTorch { enabled });
            engine.shared.lock().torch = enabled;
            Reply::ok(json!({ "torch": enabled }))
        }

        Command::SetZoom { value } => {
            let value = value.clamp(1.0, 8.0);
            engine.send_control(crate::protocol::Control::SetZoom { value });
            engine.shared.lock().zoom = value;
            Reply::ok(json!({ "zoom": value }))
        }

        Command::Rotate { degrees } => {
            let degrees = degrees % 360;
            if degrees == 0 {
                engine.reset_rotation();
            } else {
                engine.rotate_output(degrees);
            }
            Reply::ok(json!({ "rotation": engine.shared.lock().rotation_offset }))
        }

        Command::SetTheme { kind } => {
            let known = ["mono", "graphite", "light", "custom"];
            if !known.contains(&kind.as_str()) {
                return Reply::error(format!(
                    "Unknown theme '{kind}'. Available: {}.",
                    known.join(", ")
                ));
            }
            engine.shared.lock().pending_theme = Some(kind.clone());
            Reply::ok(json!({ "theme": kind }))
        }

        Command::UsbSetup => {
            if engine.shared.lock().adb_busy {
                return Reply::error("A USB setup is already running.");
            }
            engine.install_phone_app();
            Reply::ok(json!({ "started": true }))
        }
    }
}

fn status(engine: &Arc<Engine>) -> serde_json::Value {
    let s = engine.shared.lock();

    let address = s
        .local_ips
        .first()
        .map(|ip| format!("{ip}:{}", s.port))
        .unwrap_or_else(|| format!("127.0.0.1:{}", s.port));

    let camera = s
        .caps
        .as_ref()
        .and_then(|caps| {
            caps.cameras
                .iter()
                .find(|c| c.id == s.selected_camera)
                .map(|c| c.label.clone())
        })
        .unwrap_or_default();

    let resolution = s
        .selected_resolution
        .as_ref()
        .map(|r| format!("{}x{} @ {} fps", r.width, r.height, s.selected_fps))
        .unwrap_or_default();

    let cameras: Vec<serde_json::Value> = s
        .caps
        .as_ref()
        .map(|caps| {
            caps.cameras
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "label": c.label,
                        "facing": c.facing,
                        "resolutions": c.resolutions.iter().map(|r| json!({
                            "width": r.width,
                            "height": r.height,
                            "fps": r.fps,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    json!({
        "connected": s.connected,
        "phone_name": s.phone_name,
        "status": s.status,
        "address": address,
        "pin": s.pin,
        "port": s.port,
        "camera": camera,
        "cameras": cameras,
        "resolution": resolution,
        "latency_ms": s.latency_ms,
        "kbps": s.received_kbps,
        "mic": s.phone_mic,
        "torch": s.torch,
        "zoom": s.zoom,
        "rotation": s.rotation_offset,
        "virtual_camera": s.vcam_active,
        "virtual_camera_connected": s.vcam_connected,
        "decoder_error": s.decoder_error,
    })
}
