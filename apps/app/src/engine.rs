use crate::audio::{Denoise, NoiseSettings, Sink};
use crate::protocol::*;
use crate::softcam::Softcam;
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const PORT: u16 = 47_777;
const VIDEO_START_WIDTH: u32 = 1920;
const VIDEO_START_HEIGHT: u32 = 1080;

#[derive(Clone)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    /// BGR24, which is what DirectShow calls RGB24.
    pub pixels: Arc<Vec<u8>>,
    pub seq: u64,
}

#[derive(Debug, Clone)]
pub struct Shared {
    pub pin: String,
    pub port: u16,
    pub local_ips: Vec<String>,
    pub status: String,
    pub connected: bool,
    pub phone_name: String,
    pub caps: Option<Caps>,
    pub selected_camera: String,
    pub selected_resolution: Option<ResolutionInfo>,
    pub selected_fps: u32,
    pub stats: Stats,
    /// Decoded output size after rotation.
    pub video_width: u32,
    pub video_height: u32,
    /// Encoded size reported by the phone, before rotation.
    pub input_width: u32,
    pub input_height: u32,
    pub phone_rotation: u32,
    pub phone_mirror: bool,
    /// Extra clockwise rotation chosen from the desktop UI.
    pub rotation_offset: u32,
    pub decoder_error: String,
    pub vcam_enabled: bool,
    pub vcam_active: bool,
    pub vcam_connected: bool,
    pub vcam_error: String,
    pub latency_ms: f32,
    pub received_kbps: f32,
    pub logs: Vec<String>,
    pub adb_busy: bool,
    /// User-selected camera stream state. When false the phone keeps its
    /// camera device closed and sends no video.
    pub video_enabled: bool,
    pub phone_mic: bool,
    /// True when the phone reaches the PC through the USB cable.
    pub usb_connected: bool,
    pub torch: bool,
    pub zoom: f32,
    /// Last time the desktop sent a control, used to ignore stale phone stats.
    pub last_control_at: Option<Instant>,
    /// Theme requested from the command line, applied by the interface.
    pub pending_theme: Option<String>,
}

impl Shared {
    fn new(pin: String, local_ips: Vec<String>) -> Self {
        Self {
            pin,
            port: PORT,
            local_ips,
            status: "Waiting for phone".to_string(),
            connected: false,
            phone_name: String::new(),
            caps: None,
            selected_camera: String::new(),
            selected_resolution: None,
            selected_fps: 30,
            stats: Stats::default(),
            video_width: 0,
            video_height: 0,
            input_width: 0,
            input_height: 0,
            phone_rotation: 0,
            phone_mirror: false,
            rotation_offset: 0,
            decoder_error: String::new(),
            vcam_enabled: true,
            vcam_active: false,
            vcam_connected: false,
            vcam_error: String::new(),
            latency_ms: 0.0,
            received_kbps: 0.0,
            logs: Vec::new(),
            adb_busy: false,
            video_enabled: true,
            phone_mic: true,
            usb_connected: false,
            torch: false,
            zoom: 1.0,
            last_control_at: None,
            pending_theme: None,
        }
    }
}

pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    pub monitor_sink: Sink,
    pub mic_sink: Sink,
    pub noise: NoiseSettings,
    control_tx: Arc<Mutex<Option<Sender<Frame>>>>,
    vcam_enabled: Arc<AtomicBool>,
    decoder_tx: Sender<DecoderMsg>,
    pub latest: Arc<Mutex<Option<PreviewFrame>>>,
    /// The gain that is applied to the microphone before it is handed to the
    /// virtual microphone and to the speaker monitor.
    mic_gain: Arc<AtomicU32>,
    _decoder_thread: thread::JoinHandle<()>,
    _vcam_thread: thread::JoinHandle<()>,
}

impl Engine {
    pub fn new() -> Self {
        let pin = std::env::var("RECAST_PIN")
            .ok()
            .filter(|p| p.len() == 6 && p.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or_else(|| format!("{:06}", rand::random::<u32>() % 900_000 + 100_000));
        let local_ips = collect_local_ips();
        let shared = Arc::new(Mutex::new(Shared::new(pin.clone(), local_ips)));
        let monitor_sink = Sink::new();
        let mic_sink = Sink::new();
        let noise = NoiseSettings::new();
        let latest_for_vcam: Arc<Mutex<Option<PreviewFrame>>> = Arc::new(Mutex::new(None));
        let vcam_enabled = Arc::new(AtomicBool::new(true));

        let (decoder_tx, decoder_rx) = unbounded();
        let decoder_thread = {
            let shared = shared.clone();
            let latest = latest_for_vcam.clone();
            thread::Builder::new()
                .name("recast-decoder".into())
                .spawn(move || decoder_loop(decoder_rx, shared, latest))
                .expect("decoder thread")
        };

        let vcam_thread = {
            let shared = shared.clone();
            let latest = latest_for_vcam.clone();
            let enabled = vcam_enabled.clone();
            thread::Builder::new()
                .name("recast-vcam".into())
                .spawn(move || vcam_loop(latest, enabled, shared))
                .expect("vcam thread")
        };

        let control_tx = Arc::new(Mutex::new(None));
        let mic_gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        {
            let shared = shared.clone();
            let control_tx = control_tx.clone();
            let decoder_tx = decoder_tx.clone();
            let monitor_sink = monitor_sink.clone();
            let mic_sink = mic_sink.clone();
            let noise = noise.clone();
            let pin = pin.clone();
            let mic_gain = mic_gain.clone();
            thread::Builder::new()
                .name("recast-listener".into())
                .spawn(move || {
                    listener_loop(
                        shared,
                        control_tx,
                        decoder_tx,
                        monitor_sink,
                        mic_sink,
                        noise,
                        pin,
                        mic_gain,
                    )
                })
                .expect("listener thread");
        }

        Self {
            shared,
            monitor_sink,
            mic_sink,
            noise,
            control_tx,
            vcam_enabled,
            decoder_tx,
            latest: latest_for_vcam,
            mic_gain,
            _decoder_thread: decoder_thread,
            _vcam_thread: vcam_thread,
        }
    }

    /// The gain for the microphone, 1.0 is the level the phone records.
    pub fn mic_gain(&self) -> f32 {
        f32::from_bits(self.mic_gain.load(Ordering::Relaxed))
    }

    pub fn set_mic_gain(&self, gain: f32) {
        self.mic_gain
            .store(gain.clamp(0.0, 4.0).to_bits(), Ordering::Relaxed);
    }

    pub fn send_control(&self, control: Control) {
        let frame = match Frame::json(T_CONTROL, 0, 0, &control) {
            Ok(f) => f,
            Err(e) => {
                self.log(format!("control encode error: {e}"));
                return;
            }
        };
        {
            let tx = self.control_tx.lock();
            if let Some(tx) = tx.as_ref() {
                let _ = tx.send(frame);
            }
        }
        let mut s = self.shared.lock();
        s.last_control_at = Some(Instant::now());
    }

    pub fn set_vcam_enabled(&self, enabled: bool) {
        self.vcam_enabled.store(enabled, Ordering::Relaxed);
        let mut s = self.shared.lock();
        s.vcam_enabled = enabled;
        if !enabled {
            s.vcam_active = false;
            s.vcam_connected = false;
        }
    }

    pub fn vcam_enabled(&self) -> bool {
        self.vcam_enabled.load(Ordering::Relaxed)
    }

    /// Enables or disables the phone camera. The phone closes the camera
    /// device when this is false, so microphone-only sessions do not power or
    /// stream the camera module.
    pub fn set_video_enabled(&self, enabled: bool) {
        let changed = {
            let mut s = self.shared.lock();
            if s.video_enabled == enabled {
                false
            } else {
                s.video_enabled = enabled;
                true
            }
        };
        if !changed {
            return;
        }
        self.send_control(Control::SetVideo { enabled });
        if !enabled {
            let _ = self.decoder_tx.send(DecoderMsg::Stop);
            *self.latest.lock() = None;
            let mut s = self.shared.lock();
            s.video_width = 0;
            s.video_height = 0;
            s.input_width = 0;
            s.input_height = 0;
        }
    }

    /// Enables or disables the phone microphone without touching the camera.
    pub fn set_audio_enabled(&self, enabled: bool) {
        let changed = {
            let mut s = self.shared.lock();
            if s.phone_mic == enabled {
                false
            } else {
                s.phone_mic = enabled;
                true
            }
        };
        if changed {
            self.send_control(Control::SetMic { enabled });
        }
    }

    /// Adds a clockwise rotation on top of the rotation reported by the
    /// phone. The decoder is restarted immediately and the phone is asked for
    /// a fresh keyframe so the new orientation appears without a long wait.
    pub fn rotate_output(&self, delta: u32) {
        self.set_rotation_offset(|current| (current + delta) % 360);
    }

    pub fn reset_rotation(&self) {
        self.set_rotation_offset(|_| 0);
    }

    fn set_rotation_offset(&self, change: impl FnOnce(u32) -> u32) {
        let (width, height, rotation, mirror, connected) = {
            let mut s = self.shared.lock();
            s.rotation_offset = change(s.rotation_offset);
            (
                s.input_width,
                s.input_height,
                (s.phone_rotation + s.rotation_offset) % 360,
                s.phone_mirror,
                s.connected,
            )
        };
        if connected && width > 0 && height > 0 {
            let _ = self.decoder_tx.send(DecoderMsg::Config {
                width,
                height,
                rotation,
                mirror,
            });
            self.send_control(Control::RequestKeyframe);
        }
    }

    pub fn log(&self, message: impl Into<String>) {
        let mut s = self.shared.lock();
        push_log(&mut s, message.into());
    }

    pub fn install_phone_app(&self) {
        if self.shared.lock().adb_busy {
            return;
        }
        self.shared.lock().adb_busy = true;
        let shared = self.shared.clone();
        let pin = shared.lock().pin.clone();
        thread::spawn(move || {
            let result = adb_setup(&pin, &shared);
            let mut s = shared.lock();
            s.adb_busy = false;
            match result {
                Ok(msg) => {
                    s.status = msg.clone();
                    push_log(&mut s, msg);
                }
                Err(err) => {
                    s.status = err.clone();
                    push_log(&mut s, err);
                }
            }
        });
    }

    /// Connects the phone over the USB cable, so no network is needed.
    pub fn connect_over_usb(&self) {
        if self.shared.lock().adb_busy {
            return;
        }
        self.shared.lock().adb_busy = true;
        let shared = self.shared.clone();
        let pin = shared.lock().pin.clone();
        thread::spawn(move || {
            let result = usb_connect(&pin, &shared);
            let mut s = shared.lock();
            s.adb_busy = false;
            match result {
                Ok(msg) => {
                    s.status = msg.clone();
                    push_log(&mut s, msg);
                }
                Err(err) => {
                    s.status = err.clone();
                    push_log(&mut s, err);
                }
            }
        });
    }
}

fn collect_local_ips() -> Vec<String> {
    let mut candidates: Vec<(u8, String)> = Vec::new();
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in interfaces {
            if let std::net::IpAddr::V4(v4) = ip {
                if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() {
                    let s = v4.to_string();
                    if !candidates.iter().any(|(_, known)| known == &s) {
                        candidates.push((address_rank(&v4, &name), s));
                    }
                }
            }
        }
    }
    // The phone tries the addresses in this order. Real Wi-Fi and Ethernet
    // interfaces come first so pairing is immediate even when the PC has
    // virtual adapters from a VPN, a VM or WSL.
    candidates.sort_by_key(|(rank, _)| *rank);
    let ips: Vec<String> = candidates.into_iter().map(|(_, ip)| ip).collect();
    if ips.is_empty() {
        vec!["127.0.0.1".to_string()]
    } else {
        ips
    }
}

/// Lower is better: physical private LAN first, virtual adapters last.
fn address_rank(ip: &std::net::Ipv4Addr, interface: &str) -> u8 {
    let name = interface.to_ascii_lowercase();
    let virtual_adapter = [
        "vethernet",
        "virtualbox",
        "vmware",
        "hyper-v",
        "loopback",
        "tailscale",
        "zerotier",
        "docker",
        "wsl",
        "npcap",
        "vpn",
    ]
    .iter()
    .any(|word| name.contains(word));

    let network = if ip.octets()[0] == 192 && ip.octets()[1] == 168 {
        0
    } else if ip.octets()[0] == 10 {
        1
    } else if ip.octets()[0] == 172 && (16..=31).contains(&ip.octets()[1]) {
        2
    } else {
        4
    };

    network + if virtual_adapter { 8 } else { 0 }
}

fn push_log(shared: &mut Shared, message: String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let line = format!("[{:10.1}] {}", secs % 100000.0, message);
    eprintln!("{line}");
    shared.logs.push(line);
    if shared.logs.len() > 240 {
        shared.logs.drain(0..40);
    }
}

fn listener_loop(
    shared: Arc<Mutex<Shared>>,
    control_tx: Arc<Mutex<Option<Sender<Frame>>>>,
    decoder_tx: Sender<DecoderMsg>,
    monitor_sink: Sink,
    mic_sink: Sink,
    noise: NoiseSettings,
    pin: String,
    mic_gain: Arc<AtomicU32>,
) {
    let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
        Ok(l) => l,
        Err(e) => {
            let mut s = shared.lock();
            let msg = format!("Cannot listen on port {PORT}: {e}");
            s.status = msg.clone();
            push_log(&mut s, msg);
            return;
        }
    };
    {
        let mut s = shared.lock();
        push_log(
            &mut s,
            format!("Listening on port {PORT}. Pairing PIN: {pin}"),
        );
    }

    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                push_log(&mut shared.lock(), format!("accept error: {e}"));
                continue;
            }
        };
        if shared.lock().connected {
            let mut s = stream;
            let _ = write_frame(
                &mut s,
                &Frame::json(
                    T_HELLO_ACK,
                    0,
                    0,
                    &HelloAck {
                        ok: false,
                        message: "Another phone is already connected".to_string(),
                    },
                )
                .unwrap(),
            );
            continue;
        }
        let shared2 = shared.clone();
        let control_tx2 = control_tx.clone();
        let decoder_tx2 = decoder_tx.clone();
        let monitor2 = monitor_sink.clone();
        let mic2 = mic_sink.clone();
        let noise2 = noise.clone();
        let pin2 = pin.clone();
        let shared3 = shared.clone();
        let mic_gain2 = mic_gain.clone();
        // A phone that connects through the USB tunnel reaches the PC over
        // the loopback address.
        {
            let mut s = shared3.lock();
            s.usb_connected = stream
                .peer_addr()
                .map(|addr| addr.ip().is_loopback())
                .unwrap_or(false);
        }
        thread::Builder::new()
            .name("recast-client".into())
            .spawn(move || {
                if let Err(e) = handle_client(
                    stream,
                    shared2,
                    control_tx2,
                    decoder_tx2,
                    monitor2,
                    mic2,
                    noise2,
                    pin2,
                    mic_gain2,
                ) {
                    push_log(&mut shared3.lock(), format!("connection ended: {e}"));
                }
            })
            .ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_client(
    mut stream: TcpStream,
    shared: Arc<Mutex<Shared>>,
    control_tx: Arc<Mutex<Option<Sender<Frame>>>>,
    decoder_tx: Sender<DecoderMsg>,
    monitor_sink: Sink,
    mic_sink: Sink,
    noise: NoiseSettings,
    pin: String,
    mic_gain: Arc<AtomicU32>,
) -> std::io::Result<()> {
    let mut denoise = Denoise::new(noise);
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;

    let hello_frame = match read_frame(&mut stream)? {
        Some(f) => f,
        None => return Ok(()),
    };
    if hello_frame.typ != T_HELLO {
        let _ = write_frame(
            &mut stream,
            &Frame::json(
                T_HELLO_ACK,
                0,
                0,
                &HelloAck {
                    ok: false,
                    message: "Expected HELLO".into(),
                },
            )
            .unwrap(),
        );
        return Ok(());
    }
    let hello: Hello = serde_json::from_slice(&hello_frame.payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    if hello.pin.trim() != pin {
        let _ = write_frame(
            &mut stream,
            &Frame::json(
                T_HELLO_ACK,
                0,
                0,
                &HelloAck {
                    ok: false,
                    message: "Wrong PIN".into(),
                },
            )
            .unwrap(),
        );
        return Ok(());
    }

    write_frame(
        &mut stream,
        &Frame::json(
            T_HELLO_ACK,
            0,
            0,
            &HelloAck {
                ok: true,
                message: "Connected".into(),
            },
        )
        .unwrap(),
    )?;
    stream.set_read_timeout(None)?;

    {
        let mut s = shared.lock();
        s.connected = true;
        let phone_name = if hello.device.is_empty() {
            "Android phone".to_string()
        } else {
            hello.device.clone()
        };
        s.phone_name = phone_name.clone();
        s.status = format!("{phone_name} connected");
        s.decoder_error.clear();
        push_log(&mut s, format!("Phone connected: {phone_name}"));
    }

    let (out_tx, out_rx) = unbounded::<Frame>();
    *control_tx.lock() = Some(out_tx.clone());
    let writer_stream = stream.try_clone()?;
    thread::Builder::new()
        .name("recast-writer".into())
        .spawn(move || {
            let mut writer = writer_stream;
            while let Ok(frame) = out_rx.recv() {
                if write_frame(&mut writer, &frame).is_err() {
                    break;
                }
            }
        })
        .ok();

    // Apply the mode selected on the desktop before the phone starts, so a
    // microphone-only session never opens the camera even for a moment.
    let (video_enabled, mic_enabled) = {
        let mut s = shared.lock();
        s.last_control_at = Some(Instant::now());
        (s.video_enabled, s.phone_mic)
    };
    let _ = out_tx.send(
        Frame::json(
            T_CONTROL,
            0,
            0,
            &Control::SetVideo {
                enabled: video_enabled,
            },
        )
        .unwrap(),
    );
    let _ = out_tx.send(
        Frame::json(
            T_CONTROL,
            0,
            0,
            &Control::SetMic {
                enabled: mic_enabled,
            },
        )
        .unwrap(),
    );
    let _ = out_tx.send(Frame::json(T_CONTROL, 0, 0, &Control::Start).unwrap());

    // Latency ping loop for this session.
    {
        let out = out_tx.clone();
        let shared = shared.clone();
        thread::spawn(move || loop {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0);
            let frame = Frame {
                typ: T_PING,
                flags: 0,
                pts_us: 0,
                payload: now.to_le_bytes().to_vec(),
            };
            if out.send(frame).is_err() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
            if !shared.lock().connected {
                break;
            }
        });
    }

    let mut received_window_start = std::time::Instant::now();
    let mut received_bytes = 0u64;
    let mut dump_file = std::env::var("RECAST_DUMP_STREAM")
        .ok()
        .and_then(|p| std::fs::File::create(p).ok());

    loop {
        let frame = match read_frame(&mut stream) {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::ConnectionReset {
                    break;
                }
                let mut s = shared.lock();
                push_log(&mut s, format!("read error: {e}"));
                break;
            }
        };

        match frame.typ {
            T_CAPS => {
                if let Ok(caps) = serde_json::from_slice::<Caps>(&frame.payload) {
                    let mut s = shared.lock();
                    let first = caps.cameras.first().cloned();
                    let phone_camera = caps.camera_id.clone();
                    let phone_width = caps.width;
                    let phone_height = caps.height;
                    let phone_fps = caps.fps;
                    let phone_mic = caps.mic;
                    let phone_video = caps.video;
                    s.caps = Some(caps);

                    // Prefer what the phone is already using. This avoids
                    // reconfiguring the camera right after connecting, which
                    // can make some devices fail to open a capture session.
                    let camera_id = if !phone_camera.is_empty() {
                        phone_camera
                    } else if !s.selected_camera.is_empty() {
                        s.selected_camera.clone()
                    } else {
                        first.as_ref().map(|c| c.id.clone()).unwrap_or_default()
                    };
                    s.selected_camera = camera_id.clone();

                    let cam = s
                        .caps
                        .as_ref()
                        .and_then(|c| c.cameras.iter().find(|x| x.id == camera_id))
                        .cloned()
                        .or(first);
                    if let Some(cam) = cam {
                        let reported = if phone_width > 0 && phone_height > 0 {
                            cam.resolutions
                                .iter()
                                .find(|r| r.width == phone_width && r.height == phone_height)
                                .cloned()
                        } else {
                            None
                        };
                        let res = reported.or_else(|| choose_resolution(&cam));
                        if let Some(r) = &res {
                            s.selected_resolution = Some(r.clone());
                            if phone_fps > 0 && r.fps.contains(&phone_fps) {
                                s.selected_fps = phone_fps;
                            } else {
                                s.selected_fps = r.fps.first().copied().unwrap_or(30);
                            }
                        }
                    }
                    if let Some(mic) = phone_mic {
                        s.phone_mic = mic;
                    }
                    if let Some(video) = phone_video {
                        s.video_enabled = video;
                    }
                }
            }
            T_VIDEO_CONFIG => {
                if let Ok(cfg) = serde_json::from_slice::<VideoConfig>(&frame.payload) {
                    let rotation = (cfg.rotation + shared.lock().rotation_offset) % 360;
                    let (out_w, out_h) = if rotation == 90 || rotation == 270 {
                        (cfg.height, cfg.width)
                    } else {
                        (cfg.width, cfg.height)
                    };
                    {
                        let mut s = shared.lock();
                        s.input_width = cfg.width;
                        s.input_height = cfg.height;
                        s.phone_rotation = cfg.rotation;
                        s.phone_mirror = cfg.mirror;
                        s.video_width = out_w;
                        s.video_height = out_h;
                        s.decoder_error.clear();
                        push_log(
                            &mut s,
                            format!(
                                "Video: {}x{} @ {} fps, rotation {}°",
                                cfg.width, cfg.height, cfg.fps, rotation
                            ),
                        );
                    }
                    let _ = decoder_tx.send(DecoderMsg::Config {
                        width: cfg.width,
                        height: cfg.height,
                        rotation,
                        mirror: cfg.mirror,
                    });
                }
            }
            T_VIDEO => {
                received_bytes += frame.payload.len() as u64;
                if let Some(file) = dump_file.as_mut() {
                    let _ = file.write_all(&frame.payload);
                }
                let _ = decoder_tx.send(DecoderMsg::Data {
                    data: frame.payload,
                });
            }
            T_AUDIO => {
                let samples = pcm_bytes_to_i16(&frame.payload);
                // The gain is applied before the signal is split, so the
                // virtual microphone and the monitor hear the same level.
                let gain = f32::from_bits(mic_gain.load(Ordering::Relaxed));
                let boosted: Vec<i16> = if (gain - 1.0).abs() < 0.001 {
                    samples
                } else {
                    samples
                        .iter()
                        .map(|&s| (s as f32 * gain).clamp(-32768.0, 32767.0) as i16)
                        .collect()
                };
                if denoise.enabled() {
                    let clean = denoise.process(&boosted);
                    if !clean.is_empty() {
                        monitor_sink.push_i16_mono(&clean);
                        mic_sink.push_i16_mono(&clean);
                    }
                } else {
                    denoise.reset();
                    monitor_sink.push_i16_mono(&boosted);
                    mic_sink.push_i16_mono(&boosted);
                }
            }
            T_STATS => {
                if let Ok(stats) = serde_json::from_slice::<Stats>(&frame.payload) {
                    let mut video_stopped = false;
                    {
                        let mut s = shared.lock();
                        // Phone-side changes are reflected in the desktop UI, but
                        // ignore echoes that may still describe the old settings
                        // for a moment after a desktop control was sent.
                        let quiet = s
                            .last_control_at
                            .is_none_or(|t| t.elapsed() > Duration::from_millis(1800));
                        if quiet {
                            if !stats.camera_id.is_empty() {
                                s.selected_camera = stats.camera_id.clone();
                            }
                            if stats.width > 0 && stats.height > 0 {
                                let camera_id = s.selected_camera.clone();
                                let cam = s.caps.as_ref().and_then(|c| {
                                    c.cameras.iter().find(|x| x.id == camera_id).cloned()
                                });
                                if let Some(cam) = cam {
                                    if let Some(r) = cam
                                        .resolutions
                                        .iter()
                                        .find(|r| {
                                            r.width == stats.width && r.height == stats.height
                                        })
                                        .cloned()
                                    {
                                        s.selected_resolution = Some(r.clone());
                                        s.selected_fps = if stats.fps > 0.0 {
                                            (stats.fps.round() as u32)
                                                .min(r.fps.iter().copied().max().unwrap_or(30))
                                                .max(r.fps.iter().copied().min().unwrap_or(1))
                                        } else {
                                            r.fps.first().copied().unwrap_or(30)
                                        };
                                    }
                                }
                            }
                            if let Some(video) = stats.video {
                                if s.video_enabled != video {
                                    s.video_enabled = video;
                                    video_stopped = !video;
                                }
                            }
                            s.phone_mic = stats.mic;
                            s.torch = stats.torch;
                            if stats.zoom >= 1.0 && stats.zoom <= 8.0 {
                                s.zoom = stats.zoom;
                            }
                        }
                        s.stats = stats;
                    }
                    if video_stopped {
                        let _ = decoder_tx.send(DecoderMsg::Stop);
                    }
                }
            }
            T_PING => {
                let _ = out_tx.send(Frame {
                    typ: T_PONG,
                    flags: 0,
                    pts_us: 0,
                    payload: frame.payload,
                });
            }
            T_PONG => {
                if frame.payload.len() == 8 {
                    let sent = u64::from_le_bytes(frame.payload[..8].try_into().unwrap());
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0);
                    if now > sent {
                        shared.lock().latency_ms =
                            ((now - sent) as f32 / 1000.0).clamp(0.0, 5000.0);
                    }
                }
            }
            T_ERROR => {
                if let Ok(err) = serde_json::from_slice::<ErrorMsg>(&frame.payload) {
                    push_log(&mut shared.lock(), format!("phone error: {}", err.message));
                }
            }
            _ => {}
        }

        if received_window_start.elapsed() >= Duration::from_secs(2) {
            let kbps = (received_bytes as f32 * 8.0 / 1000.0)
                / received_window_start.elapsed().as_secs_f32().max(0.001);
            shared.lock().received_kbps = kbps;
            received_window_start = std::time::Instant::now();
            received_bytes = 0;
        }
    }

    {
        let mut s = shared.lock();
        s.connected = false;
        s.status = "Phone disconnected".to_string();
        s.video_width = 0;
        s.video_height = 0;
        s.latency_ms = 0.0;
        s.received_kbps = 0.0;
        push_log(&mut s, "Phone disconnected".to_string());
    }
    *control_tx.lock() = None;
    let _ = decoder_tx.send(DecoderMsg::Stop);
    Ok(())
}

fn choose_resolution(cam: &CameraInfo) -> Option<ResolutionInfo> {
    for (w, h) in [
        (VIDEO_START_WIDTH, VIDEO_START_HEIGHT),
        (1280, 720),
        (854, 480),
    ] {
        if let Some(r) = cam
            .resolutions
            .iter()
            .find(|r| r.width == w && r.height == h)
        {
            return Some(r.clone());
        }
    }
    cam.resolutions.first().cloned()
}

fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

enum DecoderMsg {
    Config {
        width: u32,
        height: u32,
        rotation: u32,
        mirror: bool,
    },
    Data {
        data: Vec<u8>,
    },
    Stop,
}

fn decoder_loop(
    rx: Receiver<DecoderMsg>,
    shared: Arc<Mutex<Shared>>,
    latest: Arc<Mutex<Option<PreviewFrame>>>,
) {
    let generation = Arc::new(AtomicU64::new(0));
    let mut child: Option<Child> = None;
    let mut stdin: Option<std::process::ChildStdin> = None;
    let mut current_size = (0u32, 0u32);
    let mut current_rotation = 0u32;
    let mut current_mirror = false;

    while let Ok(msg) = rx.recv() {
        match msg {
            DecoderMsg::Config {
                width,
                height,
                rotation,
                mirror,
            } => {
                if width == 0 || height == 0 {
                    continue;
                }
                let rotation = rotation % 360;
                if current_size == (width, height)
                    && current_rotation == rotation
                    && current_mirror == mirror
                    && child.is_some()
                {
                    continue;
                }
                current_size = (width, height);
                current_rotation = rotation;
                current_mirror = mirror;
                let gen = generation.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(mut old) = child.take() {
                    let _ = old.kill();
                    let _ = old.wait();
                }
                stdin = None;
                match start_ffmpeg(
                    width,
                    height,
                    rotation,
                    mirror,
                    gen,
                    generation.clone(),
                    shared.clone(),
                    latest.clone(),
                ) {
                    Ok((c, si)) => {
                        child = Some(c);
                        stdin = Some(si);
                        shared.lock().decoder_error.clear();
                        push_log(
                            &mut shared.lock(),
                            format!(
                                "FFmpeg decoder started for {}x{} (rotation {}°, mirror {})",
                                width, height, rotation, mirror
                            ),
                        );
                    }
                    Err(e) => {
                        let mut s = shared.lock();
                        s.decoder_error = e.clone();
                        push_log(&mut s, e);
                    }
                }
            }
            DecoderMsg::Data { data } => {
                if let Some(writer) = stdin.as_mut() {
                    if writer.write_all(&data).is_err() {
                        let mut s = shared.lock();
                        s.decoder_error = "FFmpeg stdin closed".to_string();
                        push_log(&mut s, "FFmpeg stdin closed".to_string());
                        stdin = None;
                    }
                }
            }
            DecoderMsg::Stop => {
                if let Some(mut old) = child.take() {
                    let _ = old.kill();
                    let _ = old.wait();
                }
                stdin = None;
                current_size = (0, 0);
                *latest.lock() = None;
                shared.lock().decoder_error.clear();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn start_ffmpeg(
    width: u32,
    height: u32,
    rotation: u32,
    mirror: bool,
    generation: u64,
    generation_shared: Arc<AtomicU64>,
    shared: Arc<Mutex<Shared>>,
    latest: Arc<Mutex<Option<PreviewFrame>>>,
) -> Result<(Child, std::process::ChildStdin), String> {
    let ffmpeg = find_ffmpeg()
        .ok_or_else(|| "ffmpeg.exe not found next to ReCast or in tools/ffmpeg".to_string())?;

    let (out_w, out_h) = if rotation == 90 || rotation == 270 {
        (height, width)
    } else {
        (width, height)
    };

    let mut filters: Vec<&str> = Vec::new();
    match rotation {
        90 => filters.push("transpose=1"),
        180 => {
            filters.push("hflip");
            filters.push("vflip");
        }
        270 => filters.push("transpose=2"),
        _ => {}
    }
    if mirror {
        filters.push("hflip");
    }

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-fflags".into(),
        "nobuffer".into(),
        "-flags".into(),
        "low_delay".into(),
        "-probesize".into(),
        "32".into(),
        "-analyzeduration".into(),
        "0".into(),
        "-f".into(),
        "h264".into(),
        "-i".into(),
        "pipe:0".into(),
        "-an".into(),
    ];
    if !filters.is_empty() {
        args.push("-vf".into());
        args.push(filters.join(","));
    }
    args.extend([
        // With a tiny probe the raw H.264 demuxer cannot estimate the frame
        // rate and invents a huge one; the default vsync mode then drops
        // almost every decoded frame. We only need pixels, not timestamps.
        "-fps_mode".into(),
        "passthrough".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "bgr24".into(),
        "-flush_packets".into(),
        "1".into(),
        "pipe:1".into(),
    ]);

    let mut child = Command::new(ffmpeg)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("cannot start ffmpeg: {e}"))?;

    let stdin = child.stdin.take().ok_or("ffmpeg stdin missing")?;
    let stdout = child.stdout.take().ok_or("ffmpeg stdout missing")?;
    if let Some(stderr) = child.stderr.take() {
        let shared_err = shared.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if line.to_lowercase().contains("error")
                    || line.contains("Invalid")
                    || line.contains("missing")
                {
                    push_log(&mut shared_err.lock(), format!("ffmpeg: {line}"));
                }
            }
        });
    }

    let frame_size = out_w as usize * out_h as usize * 3;
    thread::Builder::new()
        .name("recast-frames".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut seq = 0u64;
            let mut buf = vec![0u8; frame_size];
            loop {
                if generation_shared.load(Ordering::SeqCst) != generation {
                    break;
                }
                match reader.read_exact(&mut buf) {
                    Ok(()) => {
                        seq += 1;
                        let frame = PreviewFrame {
                            width: out_w,
                            height: out_h,
                            pixels: Arc::new(buf.clone()),
                            seq,
                        };
                        *latest.lock() = Some(frame.clone());
                        let mut s = shared.lock();
                        s.video_width = out_w;
                        s.video_height = out_h;
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| format!("frame reader: {e}"))?;

    Ok((child, stdin))
}

fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RECAST_FFMPEG") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for rel in ["ffmpeg.exe", "bin/ffmpeg.exe", "tools/ffmpeg.exe"] {
                let p = dir.join(rel);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ffmpeg.exe");
    if dev.is_file() {
        return Some(dev);
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
        .join("ffmpeg")
        .join("ffmpeg.exe");
    if workspace.is_file() {
        return Some(workspace);
    }
    None
}

/// The virtual camera keeps one fixed format for the whole session. A stable
/// 1280x720@30 device is what browsers and meeting apps expect; recreating the
/// filter whenever the phone changes resolution makes an open consumer stall.
const VCAM_WIDTH: u32 = 1280;
const VCAM_HEIGHT: u32 = 720;
const VCAM_FPS: f32 = 30.0;

fn vcam_loop(
    latest: Arc<Mutex<Option<PreviewFrame>>>,
    enabled: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
) {
    let mut cam: Option<Softcam> = None;
    let mut last_error = String::new();
    let mut last_send = std::time::Instant::now();
    let placeholder = placeholder(VCAM_WIDTH, VCAM_HEIGHT);

    loop {
        if !enabled.load(Ordering::Relaxed) {
            cam = None;
            {
                let mut s = shared.lock();
                s.vcam_active = false;
                s.vcam_connected = false;
            }
            thread::sleep(Duration::from_millis(120));
            continue;
        }

        let frame = latest.lock().clone();
        let source_len = |w: u32, h: u32| w as usize * h as usize * 3;
        let pixels = match frame {
            Some(f) if f.width == VCAM_WIDTH && f.height == VCAM_HEIGHT => f.pixels,
            Some(f) if f.pixels.len() >= source_len(f.width, f.height) => Arc::new(scale_bgr(
                &f.pixels,
                f.width,
                f.height,
                VCAM_WIDTH,
                VCAM_HEIGHT,
            )),
            _ => placeholder.clone(),
        };

        if cam.is_none() {
            thread::sleep(Duration::from_millis(30));
            match Softcam::new(VCAM_WIDTH, VCAM_HEIGHT, VCAM_FPS) {
                Ok(c) => {
                    cam = Some(c);
                    last_error.clear();
                    {
                        let mut s = shared.lock();
                        s.vcam_active = true;
                        s.vcam_error.clear();
                        push_log(
                            &mut s,
                            format!(
                                "Virtual camera ready: {}x{} @ {VCAM_FPS:.0} fps",
                                VCAM_WIDTH, VCAM_HEIGHT
                            ),
                        );
                    }
                }
                Err(e) => {
                    if e != last_error {
                        last_error = e.clone();
                        let mut s = shared.lock();
                        s.vcam_error = e.clone();
                        s.vcam_active = false;
                        push_log(&mut s, e);
                    }
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        }

        if let Some(c) = cam.as_ref() {
            let target = Duration::from_micros(1_000_000 / VCAM_FPS as u64);
            let elapsed = last_send.elapsed();
            if elapsed < target {
                thread::sleep(target - elapsed);
            }
            last_send = std::time::Instant::now();
            if pixels.len() >= source_len(VCAM_WIDTH, VCAM_HEIGHT) {
                c.send_rgb(&pixels);
            }
            let connected = c.is_connected();
            let mut s = shared.lock();
            s.vcam_connected = connected;
            s.vcam_active = true;
        }
    }
}

/// Nearest-neighbour BGR24 scaling for the fixed virtual-camera format.
fn scale_bgr(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut dst = vec![0u8; dw as usize * dh as usize * 3];
    if sw == 0 || sh == 0 || src.len() < sw as usize * sh as usize * 3 {
        return dst;
    }
    let src_stride = sw as usize * 3;
    let dst_stride = dw as usize * 3;
    for y in 0..dh as usize {
        let sy = y * sh as usize / dh as usize;
        let src_row = sy * src_stride;
        let dst_row = y * dst_stride;
        for x in 0..dw as usize {
            let sx = x * sw as usize / dw as usize;
            let si = src_row + sx * 3;
            let di = dst_row + x * 3;
            dst[di] = src[si];
            dst[di + 1] = src[si + 1];
            dst[di + 2] = src[si + 2];
        }
    }
    dst
}

/// A calm dark slate that is delivered while no phone frame is available, so
/// applications always see a live camera instead of a frozen or black device.
fn placeholder(w: u32, h: u32) -> Arc<Vec<u8>> {
    let mut v = vec![0u8; w as usize * h as usize * 3];
    for y in 0..h {
        let t = (y * 40 / h.max(1)) as u8;
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;
            v[i] = 8 + t / 3;
            v[i + 1] = 9 + t / 3;
            v[i + 2] = 11 + t / 2;
        }
    }
    Arc::new(v)
}

fn adb_setup(pin: &str, shared: &Arc<Mutex<Shared>>) -> Result<String, String> {
    let adb = find_adb().ok_or_else(|| {
        "adb.exe not found. Install Android platform-tools or use the Wi-Fi connection.".to_string()
    })?;
    let apk = find_apk().ok_or_else(|| {
        "ReCastPhone.apk not found next to ReCast. Build the Android app first.".to_string()
    })?;

    push_log(&mut shared.lock(), format!("Using adb: {}", adb.display()));
    let devices = run_cmd(&adb, &["devices"])?;
    push_log(&mut shared.lock(), devices.trim().to_string());
    if !devices.lines().any(|l| l.ends_with("\tdevice")) {
        return Err(
            "No authorized ADB device. On the phone: enable Developer options, USB debugging, then approve this PC."
                .to_string(),
        );
    }

    let _ = run_cmd(&adb, &["reverse", "tcp:47777", "tcp:47777"])?;
    push_log(&mut shared.lock(), "USB reverse tunnel ready".to_string());
    let install = run_cmd(
        &adb,
        &["install", "-r", apk.to_str().ok_or("bad APK path")?],
    )?;
    push_log(&mut shared.lock(), install.trim().to_string());

    let _ = run_cmd(
        &adb,
        &[
            "shell",
            "am",
            "start",
            "-n",
            "com.recast.phone/.MainActivity",
            "--es",
            "host",
            "127.0.0.1",
            "--es",
            "pin",
            pin,
            "--ez",
            "auto",
            "true",
        ],
    )?;
    Ok("Phone app launched over USB. Waiting for stream…".to_string())
}

/// Connects the phone through the USB cable, without any network.
///
/// The tunnel is opened with `adb reverse`, so the phone reaches the PC over
/// the loopback address and the whole stream runs through the cable.
fn usb_connect(pin: &str, shared: &Arc<Mutex<Shared>>) -> Result<String, String> {
    let adb = find_adb().ok_or_else(|| {
        "adb.exe not found. Install the Android platform tools to use the USB connection."
            .to_string()
    })?;
    let devices = run_cmd(&adb, &["devices"])?;
    push_log(&mut shared.lock(), devices.trim().to_string());
    if !devices.lines().any(|l| l.ends_with("\tdevice")) {
        return Err(
            "No authorized ADB device. On the phone: enable Developer options, USB debugging, then approve this PC."
                .to_string(),
        );
    }

    let _ = run_cmd(&adb, &["reverse", "tcp:47777", "tcp:47777"])?;
    push_log(&mut shared.lock(), "USB tunnel ready".to_string());

    let _ = run_cmd(
        &adb,
        &[
            "shell",
            "am",
            "start",
            "-n",
            "com.recast.phone/.MainActivity",
            "--es",
            "host",
            "127.0.0.1",
            "--es",
            "pin",
            pin,
            "--ez",
            "auto",
            "true",
        ],
    )?;
    Ok("Connected over USB. The stream runs through the cable.".to_string())
}

fn find_adb() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RECAST_ADB") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("adb.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let fallback = PathBuf::from(r"C:\platform-tools\adb.exe");
    if fallback.is_file() {
        return Some(fallback);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local)
            .join("Android")
            .join("Sdk")
            .join("platform-tools")
            .join("adb.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn find_apk() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["ReCastPhone.apk", "recast-phone.apk"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("phone")
        .join("ReCastPhone.apk");
    if dev.is_file() {
        return Some(dev);
    }
    None
}

fn run_cmd(program: &PathBuf, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!("adb {} failed: {}", args.join(" "), text.trim()));
    }
    Ok(text)
}

trait CreationFlags {
    fn creation_flags(&mut self, flags: u32) -> &mut Self;
}

impl CreationFlags for Command {
    fn creation_flags(&mut self, flags: u32) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            CommandExt::creation_flags(self, flags);
        }
        #[cfg(not(windows))]
        let _ = flags;
        self
    }
}
