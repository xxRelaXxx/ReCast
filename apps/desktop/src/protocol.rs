use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 4] = b"RWBC";
pub const HEADER_LEN: usize = 20;
pub const MAX_PAYLOAD: usize = 32 * 1024 * 1024;

pub const T_HELLO: u8 = 1;
pub const T_HELLO_ACK: u8 = 2;
pub const T_CAPS: u8 = 3;
pub const T_VIDEO_CONFIG: u8 = 4;
pub const T_VIDEO: u8 = 5;
pub const T_AUDIO: u8 = 6;
pub const T_CONTROL: u8 = 7;
pub const T_STATS: u8 = 8;
pub const T_PING: u8 = 9;
pub const T_PONG: u8 = 10;
pub const T_ERROR: u8 = 11;

// Video frame flags. The phone sets them; the desktop decoder currently
// ignores them because FFmpeg resynchronises on its own.
#[allow(dead_code)]
pub const VF_CONFIG: u8 = 0x01;
#[allow(dead_code)]
pub const VF_KEYFRAME: u8 = 0x02;

pub const AUDIO_RATE: u32 = 48_000;

#[derive(Debug, Clone)]
pub struct Frame {
    pub typ: u8,
    pub flags: u8,
    pub pts_us: i64,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn json<T: Serialize>(typ: u8, flags: u8, pts_us: i64, value: &T) -> io::Result<Self> {
        let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
        Ok(Self {
            typ,
            flags,
            pts_us,
            payload,
        })
    }
}

pub fn write_frame<W: Write>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    if frame.payload.len() > MAX_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "payload too large"));
    }
    let mut header = [0u8; HEADER_LEN];
    header[0..4].copy_from_slice(MAGIC);
    header[4] = frame.typ;
    header[5] = frame.flags;
    header[6..8].copy_from_slice(&0u16.to_le_bytes());
    header[8..12].copy_from_slice(&(frame.payload.len() as u32).to_le_bytes());
    header[12..20].copy_from_slice(&frame.pts_us.to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(&frame.payload)?;
    writer.flush()
}

pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    let mut header = [0u8; HEADER_LEN];
    let mut got = 0usize;
    while got < HEADER_LEN {
        match reader.read(&mut header[got..]) {
            Ok(0) => {
                if got == 0 {
                    return Ok(None);
                }
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "partial header"));
            }
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }

    if &header[0..4] != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic"));
    }

    let typ = header[4];
    let flags = header[5];
    let len = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "payload too large"));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    let pts_us = i64::from_le_bytes(header[12..20].try_into().unwrap());

    Ok(Some(Frame {
        typ,
        flags,
        pts_us,
        payload,
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Caps {
    #[serde(default)]
    pub cameras: Vec<CameraInfo>,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub device: String,
    /// Camera currently selected on the phone when the session starts.
    #[serde(default)]
    pub camera_id: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub fps: u32,
    /// Microphone state reported by the phone; absent on older clients.
    #[serde(default)]
    pub mic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CameraInfo {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub facing: String,
    #[serde(default)]
    pub resolutions: Vec<ResolutionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ResolutionInfo {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub fps: Vec<u32>,
}

impl ResolutionInfo {
    pub fn label(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub pin: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAck {
    pub ok: bool,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    #[serde(default)]
    pub rotation: u32,
    #[serde(default)]
    pub mirror: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    #[serde(default)]
    pub battery: Option<u32>,
    #[serde(default)]
    pub thermal: Option<i32>,
    #[serde(default)]
    pub fps: f32,
    #[serde(default)]
    pub bitrate_kbps: u32,
    #[serde(default)]
    pub camera_id: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub mic: bool,
    #[serde(default)]
    pub torch: bool,
    #[serde(default)]
    pub zoom: f32,
    #[serde(default)]
    pub dropped: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Control {
    SetCamera { id: String },
    SetResolution { width: u32, height: u32, fps: u32 },
    SetMic { enabled: bool },
    SetTorch { enabled: bool },
    SetZoom { value: f32 },
    RequestKeyframe,
    Start,
    Stop,
    Ping { id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub message: String,
}
