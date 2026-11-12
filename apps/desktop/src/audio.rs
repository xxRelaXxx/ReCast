use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crate::protocol::AUDIO_RATE;

const TARGET_RATE: u32 = AUDIO_RATE;
const MAX_BUFFER_SAMPLES: usize = TARGET_RATE as usize * 2;

/// Live controls for the RNNoise microphone denoiser.
#[derive(Clone)]
pub struct NoiseSettings {
    enabled: Arc<AtomicBool>,
    strength: Arc<Mutex<f32>>,
}

impl NoiseSettings {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            strength: Arc::new(Mutex::new(0.8)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// 0.0 keeps the raw microphone, 1.0 uses the fully denoised signal.
    pub fn strength(&self) -> f32 {
        *self.strength.lock()
    }

    pub fn set_strength(&self, strength: f32) {
        *self.strength.lock() = strength.clamp(0.0, 1.0);
    }
}

impl Default for NoiseSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame-based RNNoise wrapper. Input and output are 48 kHz mono i16.
pub struct Denoise {
    settings: NoiseSettings,
    state: Box<nnnoiseless::DenoiseState<'static>>,
    pending: Vec<f32>,
}

impl Denoise {
    pub fn new(settings: NoiseSettings) -> Self {
        Self {
            settings,
            state: nnnoiseless::DenoiseState::new(),
            pending: Vec::with_capacity(nnnoiseless::DenoiseState::FRAME_SIZE),
        }
    }

    pub fn enabled(&self) -> bool {
        self.settings.enabled()
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }

    pub fn process(&mut self, input: &[i16]) -> Vec<i16> {
        let strength = self.settings.strength().clamp(0.0, 1.0);
        let mut output = Vec::with_capacity(input.len());
        let mut wet = [0.0f32; nnnoiseless::DenoiseState::FRAME_SIZE];

        for &sample in input {
            self.pending.push(sample as f32);
            if self.pending.len() < nnnoiseless::DenoiseState::FRAME_SIZE {
                continue;
            }

            self.state.process_frame(&mut wet, &self.pending);
            for (dry, clean) in self.pending.iter().zip(wet.iter()) {
                let mixed = dry * (1.0 - strength) + clean * strength;
                output.push(mixed.clamp(-32768.0, 32767.0) as i16);
            }
            self.pending.clear();
        }

        output
    }
}

#[derive(Clone)]
pub struct Sink {
    pub buffer: Arc<Mutex<VecDeque<f32>>>,
    pub enabled: Arc<AtomicBool>,
    pub volume: Arc<Mutex<f32>>,
    pub level: Arc<AtomicU32>,
}

impl Sink {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(TARGET_RATE as usize))),
            enabled: Arc::new(AtomicBool::new(true)),
            volume: Arc::new(Mutex::new(1.0)),
            level: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, volume: f32) {
        *self.volume.lock() = volume.clamp(0.0, 2.0);
    }

    pub fn volume(&self) -> f32 {
        *self.volume.lock()
    }

    pub fn push_i16_mono(&self, samples: &[i16]) {
        if !self.enabled() {
            return;
        }
        let mut peak = 0i32;
        let mut buf = self.buffer.lock();
        for &s in samples {
            peak = peak.max((s as i32).abs());
            buf.push_back(s as f32 / 32768.0);
        }
        while buf.len() > MAX_BUFFER_SAMPLES {
            buf.pop_front();
        }
        self.level
            .store((peak as f32 / 32768.0).to_bits(), Ordering::Relaxed);
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}

pub struct AudioEngine {
    monitor: Option<cpal::Stream>,
    virtual_mic: Option<cpal::Stream>,
    pub monitor_sink: Sink,
    pub mic_sink: Sink,
    pub monitor_device: String,
    pub virtual_device: String,
    host: cpal::Host,
}

impl AudioEngine {
    pub fn with_sinks(monitor_sink: Sink, mic_sink: Sink) -> Self {
        let host = cpal::default_host();
        Self {
            monitor: None,
            virtual_mic: None,
            monitor_sink,
            mic_sink,
            monitor_device: String::new(),
            virtual_device: String::new(),
            host,
        }
    }

    pub fn output_devices(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(devices) = self.host.output_devices() {
            for device in devices {
                if let Ok(desc) = device.description() {
                    names.push(desc.name().to_string());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    pub fn default_monitor_name(&self) -> String {
        self.host
            .default_output_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.name().to_string())
            .unwrap_or_default()
    }

    pub fn detect_virtual_cable(devices: &[String]) -> Option<String> {
        let needles = [
            "voice.ai audio cable",
            "cable input",
            "vb-audio",
            "vb-cable",
            "virtual cable",
            "droidcam audio",
            "steam streaming speakers",
        ];
        for needle in needles {
            if let Some(name) = devices
                .iter()
                .find(|n| n.to_lowercase().contains(needle))
            {
                return Some(name.clone());
            }
        }
        None
    }

    pub fn start_monitor(&mut self, name: &str) -> Result<(), String> {
        self.monitor = None;
        let name = if name.trim().is_empty() {
            self.default_monitor_name()
        } else {
            name.to_string()
        };
        if name.is_empty() {
            return Err("No default output device".to_string());
        }
        let stream = self.build_stream(&name, self.monitor_sink.clone())?;
        stream.play().map_err(|e| e.to_string())?;
        self.monitor = Some(stream);
        self.monitor_device = name;
        Ok(())
    }

    pub fn start_virtual_mic(&mut self, name: &str) -> Result<(), String> {
        self.virtual_mic = None;
        if name.trim().is_empty() {
            return Err("No virtual audio cable selected".to_string());
        }
        let stream = self.build_stream(name, self.mic_sink.clone())?;
        stream.play().map_err(|e| e.to_string())?;
        self.virtual_mic = Some(stream);
        self.virtual_device = name.to_string();
        Ok(())
    }

    fn build_stream(&self, device_name: &str, sink: Sink) -> Result<cpal::Stream, String> {
        let device = self
            .host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.description()
                    .map(|desc| desc.name() == device_name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("Output device not found: {device_name}"))?;

        let mut chosen: Option<(StreamConfig, SampleFormat)> = None;
        if let Ok(configs) = device.supported_output_configs() {
            for cfg in configs {
                if cfg.channels() >= 1
                    && cfg.min_sample_rate() <= TARGET_RATE
                    && cfg.max_sample_rate() >= TARGET_RATE
                {
                    let sample_format = cfg.sample_format();
                    if matches!(
                        sample_format,
                        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
                    ) {
                        let channels = cfg.channels().min(2);
                        chosen = Some((
                            StreamConfig {
                                channels,
                                sample_rate: TARGET_RATE,
                                buffer_size: cpal::BufferSize::Default,
                            },
                            sample_format,
                        ));
                        break;
                    }
                }
            }
        }

        let (config, sample_format) = match chosen {
            Some(v) => v,
            None => {
                let default = device.default_output_config().map_err(|e| e.to_string())?;
                let sample_format = default.sample_format();
                (default.config(), sample_format)
            }
        };

        let err_fn = |err| eprintln!("audio stream error: {err}");
        let channels = config.channels as usize;

        let stream = match sample_format {
            SampleFormat::F32 => {
                let sink = sink.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            fill_output(data, channels, &sink);
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| e.to_string())?
            }
            SampleFormat::I16 => {
                let sink = sink.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            let mut tmp = vec![0.0f32; data.len()];
                            fill_output(&mut tmp, channels, &sink);
                            for (dst, src) in data.iter_mut().zip(tmp) {
                                *dst = (src.clamp(-1.0, 1.0) * 32767.0) as i16;
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| e.to_string())?
            }
            SampleFormat::U16 => {
                let sink = sink.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            let mut tmp = vec![0.0f32; data.len()];
                            fill_output(&mut tmp, channels, &sink);
                            for (dst, src) in data.iter_mut().zip(tmp) {
                                *dst = ((src.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65535.0) as u16;
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| e.to_string())?
            }
            other => return Err(format!("Unsupported sample format: {other:?}")),
        };

        Ok(stream)
    }
}

fn fill_output(data: &mut [f32], channels: usize, sink: &Sink) {
    if !sink.enabled() {
        data.fill(0.0);
        return;
    }
    let volume = sink.volume();
    let mut buf = sink.buffer.lock();
    for frame in data.chunks_mut(channels) {
        let sample = buf.pop_front().unwrap_or(0.0) * volume;
        for out in frame.iter_mut() {
            *out = sample;
        }
    }
}
