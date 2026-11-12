use crate::audio::AudioEngine;
use crate::engine::Engine;
use crate::protocol::{CameraInfo, Control, ResolutionInfo};
use crate::theme::{ThemeConfig, ThemeKind};
use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, TextureOptions, Vec2};
use std::sync::Arc;
use std::time::Duration;

pub struct RewebcamApp {
    engine: Arc<Engine>,
    audio: AudioEngine,
    theme: ThemeConfig,
    theme_dirty: bool,
    texture: Option<egui::TextureHandle>,
    preview_rgb: Vec<u8>,
    last_frame_seq: u64,
    last_texture_at: Option<std::time::Instant>,
    selected_camera: String,
    selected_resolution: Option<ResolutionInfo>,
    selected_fps: u32,
    mirror_preview: bool,
    monitor_enabled: bool,
    virtual_mic_enabled: bool,
    monitor_device: String,
    virtual_device: String,
    device_names: Vec<String>,
    show_settings: bool,
    toast: String,
    toast_until: Option<std::time::Instant>,
}

impl RewebcamApp {
    pub fn new(cc: &eframe::CreationContext<'_>, engine: Arc<Engine>) -> Self {
        let theme = ThemeConfig::default();
        theme.apply(&cc.egui_ctx);

        let monitor_sink = engine.monitor_sink.clone();
        let mic_sink = engine.mic_sink.clone();
        let mut audio = AudioEngine::with_sinks(monitor_sink, mic_sink);
        let device_names = audio.output_devices();
        let monitor_device = audio.default_monitor_name();
        let virtual_device = AudioEngine::detect_virtual_cable(&device_names).unwrap_or_default();

        audio.monitor_sink.set_enabled(false);
        audio.mic_sink.set_enabled(!virtual_device.is_empty());
        if !monitor_device.is_empty() {
            let _ = audio.start_monitor(&monitor_device);
        }
        if !virtual_device.is_empty() {
            let _ = audio.start_virtual_mic(&virtual_device);
        }

        Self {
            engine,
            audio,
            theme,
            theme_dirty: false,
            texture: None,
            preview_rgb: Vec::new(),
            last_frame_seq: 0,
            last_texture_at: None,
            selected_camera: String::new(),
            selected_resolution: None,
            selected_fps: 30,
            mirror_preview: false,
            monitor_enabled: false,
            virtual_mic_enabled: !virtual_device.is_empty(),
            monitor_device,
            virtual_device,
            device_names,
            show_settings: false,
            toast: String::new(),
            toast_until: None,
        }
    }

    fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = msg.into();
        self.toast_until = Some(std::time::Instant::now() + Duration::from_secs(6));
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        // The phone streams at most 30 fps, so uploading faster only wastes
        // UI time on the BGR->RGB conversion.
        let now = std::time::Instant::now();
        if let Some(last) = self.last_texture_at {
            if now.duration_since(last) < Duration::from_millis(33) {
                return;
            }
        }
        let frame = {
            let guard = self.engine.latest.lock();
            match guard.as_ref() {
                Some(frame) => frame.clone(),
                None => return,
            }
        };
        if frame.seq == self.last_frame_seq {
            return;
        }
        self.last_frame_seq = frame.seq;
        self.last_texture_at = Some(now);

        let width = frame.width as usize;
        let height = frame.height as usize;

        // Large phone frames are downscaled for the preview (the pane is far
        // smaller than 1080p), which cuts both conversion and upload cost.
        let step = if width > 1280 { 2 } else { 1 };
        let out_w = width / step;
        let out_h = height / step;
        let needed = out_w * out_h * 3;
        self.preview_rgb.clear();
        self.preview_rgb.resize(needed, 0);

        let src = &frame.pixels;
        if step == 1 {
            for (dst, s) in self
                .preview_rgb
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(src.as_chunks::<3>().0.iter())
            {
                dst[0] = s[2];
                dst[1] = s[1];
                dst[2] = s[0];
            }
        } else {
            let src_row_bytes = width * 3;
            let dst_row_bytes = out_w * 3;
            for y in 0..out_h {
                let src_row = y * step * src_row_bytes;
                let dst_row = y * dst_row_bytes;
                for x in 0..out_w {
                    let i = src_row + x * step * 3;
                    let o = dst_row + x * 3;
                    self.preview_rgb[o] = src[i + 2];
                    self.preview_rgb[o + 1] = src[i + 1];
                    self.preview_rgb[o + 2] = src[i];
                }
            }
        }

        let image = egui::ColorImage::from_rgb([out_w, out_h], &self.preview_rgb);
        let size = [out_w, out_h];
        if let Some(tex) = self.texture.as_mut() {
            if tex.size() == size {
                tex.set(image, TextureOptions::LINEAR);
                return;
            }
        }
        self.texture = Some(ctx.load_texture("preview", image, TextureOptions::LINEAR));
    }

    fn apply_caps_defaults(&mut self, shared: &crate::engine::Shared) {
        if let Some(caps) = &shared.caps {
            // Follow the camera selected on the phone, including changes made
            // from the phone UI (engine mirrors them from the stats frames).
            if !shared.selected_camera.is_empty() && self.selected_camera != shared.selected_camera
            {
                self.selected_camera = shared.selected_camera.clone();
                self.selected_resolution = None;
            }
            let Some(cam) = caps
                .cameras
                .iter()
                .find(|c| c.id == self.selected_camera)
                .or_else(|| caps.cameras.first())
            else {
                return;
            };
            if self.selected_camera != cam.id {
                self.selected_camera = cam.id.clone();
            }
            if let Some(r) = &shared.selected_resolution {
                let different = self
                    .selected_resolution
                    .as_ref()
                    .map(|x| x.width != r.width || x.height != r.height)
                    .unwrap_or(true);
                if different {
                    self.selected_resolution = Some(r.clone());
                }
                if self.selected_fps != shared.selected_fps {
                    self.selected_fps = shared.selected_fps;
                }
            }
            let valid = self
                .selected_resolution
                .as_ref()
                .map(|r| {
                    cam.resolutions
                        .iter()
                        .any(|x| x.width == r.width && x.height == r.height)
                })
                .unwrap_or(false);
            if !valid {
                self.selected_resolution = preferred_resolution(cam);
                if let Some(r) = &self.selected_resolution {
                    self.selected_fps = r.fps.first().copied().unwrap_or(30);
                }
            }
        }
    }

    fn send_camera_change(&mut self) {
        self.engine.send_control(Control::SetCamera {
            id: self.selected_camera.clone(),
        });
        self.engine.shared.lock().selected_camera = self.selected_camera.clone();
    }

    fn send_resolution_change(&mut self) {
        if let Some(r) = &self.selected_resolution {
            self.engine.send_control(Control::SetResolution {
                width: r.width,
                height: r.height,
                fps: self.selected_fps,
            });
            self.engine.shared.lock().selected_resolution = Some(r.clone());
            self.engine.shared.lock().selected_fps = self.selected_fps;
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let shared = self.engine.shared.lock().clone();
        let live = shared.connected;
        let fg = self.strong();
        let muted = self.muted();

        egui::TopBottomPanel::top("topbar")
            .exact_height(46.0)
            .frame(
                egui::Frame::new()
                    .fill(rgb(self.theme.background))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("REWEBCAM").size(16.0).strong().color(fg));
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(if live { "●" } else { "○" })
                            .size(9.0)
                            .color(if live { fg } else { muted }),
                    );
                    ui.label(
                        RichText::new(if live { "LIVE" } else { "WAITING" })
                            .size(10.0)
                            .color(if live { fg } else { muted }),
                    );
                    if live && !shared.phone_name.is_empty() {
                        ui.label(RichText::new(&shared.phone_name).size(10.0).color(muted));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Theme").clicked() {
                            self.show_settings = true;
                        }
                        ui.add_space(10.0);
                        let vcam = shared.vcam_active;
                        ui.label(
                            RichText::new(if vcam { "VIRTUAL CAMERA" } else { "CAMERA OFF" })
                                .size(9.0)
                                .color(if vcam { fg } else { muted }),
                        );
                        ui.label(
                            RichText::new(if vcam { "●" } else { "○" })
                                .size(9.0)
                                .color(if vcam { fg } else { muted }),
                        );
                    });
                });
            });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("controls")
            .resizable(false)
            .exact_width(312.0)
            .frame(
                egui::Frame::new()
                    .fill(rgb(self.theme.panel))
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let shared = self.engine.shared.lock().clone();
                        self.apply_caps_defaults(&shared);
                        self.connection_ui(ui, &shared);
                        self.camera_ui(ui, &shared);
                        self.output_ui(ui, &shared);
                        self.audio_ui(ui);
                        self.noise_ui(ui);

                        let toast_alive = !self.toast.is_empty()
                            && self
                                .toast_until
                                .map(|t| t > std::time::Instant::now())
                                .unwrap_or(false);
                        if toast_alive {
                            ui.add_space(12.0);
                            let text = self.toast.clone();
                            ui.label(RichText::new(text).size(10.0).color(self.warn()));
                        }
                    });
            });
    }

    fn connection_ui(&mut self, ui: &mut egui::Ui, shared: &crate::engine::Shared) {
        self.section(ui, "CONNECTION");
        if shared.connected {
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(9.0).color(self.strong()));
                ui.label(
                    RichText::new(&shared.phone_name)
                        .size(12.0)
                        .color(self.strong()),
                );
            });
            ui.label(RichText::new(&shared.status).size(10.0).color(self.muted()));
        } else {
            let ip = shared
                .local_ips
                .first()
                .cloned()
                .unwrap_or_else(|| "127.0.0.1".into());
            let address = format!("{ip}:{}", shared.port);
            self.copy_row(ui, "Address", &address);
            self.copy_row(ui, "PIN", &shared.pin);
            ui.add_space(4.0);
            if ui
                .add_enabled(!shared.adb_busy, egui::Button::new("USB setup"))
                .clicked()
            {
                self.engine.install_phone_app();
                self.set_toast("USB setup started. Approve the prompt on the phone.");
            }
            if shared.adb_busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Starting…").size(10.0).color(self.muted()));
                });
            }
            ui.label(
                RichText::new("Enter these in the phone app.")
                    .size(10.0)
                    .color(self.muted()),
            );
        }
        if !shared.decoder_error.is_empty() {
            ui.label(
                RichText::new(&shared.decoder_error)
                    .size(10.0)
                    .color(self.warn()),
            );
        }
    }

    fn copy_row(&mut self, ui: &mut egui::Ui, label: &str, value: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(10.0).color(self.muted()));
            let value = value.to_string();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(value.clone());
                    self.set_toast(format!("{label} copied."));
                }
                ui.label(
                    RichText::new(&value)
                        .monospace()
                        .size(11.0)
                        .color(self.strong()),
                );
            });
        });
    }

    fn camera_ui(&mut self, ui: &mut egui::Ui, shared: &crate::engine::Shared) {
        self.section(ui, "CAMERA");
        let caps = shared.caps.clone();
        if let Some(caps) = caps {
            if !caps.cameras.is_empty() {
                let current_label = caps
                    .cameras
                    .iter()
                    .find(|c| c.id == self.selected_camera)
                    .map(|c| c.label.clone())
                    .unwrap_or_else(|| "No camera".into());
                egui::ComboBox::from_id_salt("camera_combo")
                    .selected_text(current_label)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for cam in &caps.cameras {
                            if ui
                                .selectable_label(self.selected_camera == cam.id, &cam.label)
                                .clicked()
                                && self.selected_camera != cam.id
                            {
                                self.selected_camera = cam.id.clone();
                                self.selected_resolution = preferred_resolution(cam);
                                if let Some(r) = &self.selected_resolution {
                                    self.selected_fps = r.fps.first().copied().unwrap_or(30);
                                }
                                self.send_camera_change();
                                self.send_resolution_change();
                            }
                        }
                    });
            }

            let cam = caps.cameras.iter().find(|c| c.id == self.selected_camera);
            if let Some(cam) = cam {
                let res_label = self
                    .selected_resolution
                    .as_ref()
                    .map(|r| r.label())
                    .unwrap_or_else(|| "—".into());
                egui::ComboBox::from_id_salt("res_combo")
                    .selected_text(res_label)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for r in &cam.resolutions {
                            let selected = self
                                .selected_resolution
                                .as_ref()
                                .map(|x| x.width == r.width && x.height == r.height)
                                .unwrap_or(false);
                            if ui.selectable_label(selected, r.label()).clicked() {
                                self.selected_resolution = Some(r.clone());
                                self.selected_fps = r.fps.first().copied().unwrap_or(30);
                                self.send_resolution_change();
                            }
                        }
                    });

                if let Some(r) = self.selected_resolution.clone() {
                    if r.fps.len() > 1 {
                        let mut fps = self.selected_fps;
                        egui::ComboBox::from_id_salt("fps_combo")
                            .selected_text(format!("{fps} fps"))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for &v in &r.fps {
                                    ui.selectable_value(&mut fps, v, format!("{v} fps"));
                                }
                            });
                        if fps != self.selected_fps {
                            self.selected_fps = fps;
                            self.send_resolution_change();
                        }
                    }
                }
            }
        } else {
            ui.label(
                RichText::new("Waiting for phone")
                    .size(11.0)
                    .color(self.muted()),
            );
        }

        ui.add_space(6.0);
        let mut mic = shared.phone_mic;
        if ui.checkbox(&mut mic, "Phone microphone").changed() {
            self.engine.shared.lock().phone_mic = mic;
            self.engine.send_control(Control::SetMic { enabled: mic });
        }
        let mut torch = shared.torch;
        if ui.checkbox(&mut torch, "Torch").changed() {
            self.engine.shared.lock().torch = torch;
            self.engine
                .send_control(Control::SetTorch { enabled: torch });
        }
        let mut zoom = shared.zoom;
        if ui
            .add(egui::Slider::new(&mut zoom, 1.0..=8.0).text("Zoom"))
            .changed()
        {
            self.engine.shared.lock().zoom = zoom;
            self.engine.send_control(Control::SetZoom { value: zoom });
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Rotation").size(11.0));
            if ui.button("+90°").clicked() {
                self.engine.rotate_output(90);
            }
            ui.label(
                RichText::new(format!("{}°", shared.rotation_offset))
                    .monospace()
                    .size(10.0)
                    .color(self.muted()),
            );
            if shared.rotation_offset != 0 && ui.small_button("Reset").clicked() {
                self.engine.reset_rotation();
            }
        });
        ui.checkbox(&mut self.mirror_preview, "Mirror preview");
    }

    fn output_ui(&mut self, ui: &mut egui::Ui, shared: &crate::engine::Shared) {
        self.section(ui, "OUTPUT");
        let mut enabled = self.engine.vcam_enabled();
        if ui.checkbox(&mut enabled, "Virtual camera").changed() {
            self.engine.set_vcam_enabled(enabled);
        }
        let state = if shared.vcam_active {
            if shared.vcam_connected {
                "In use"
            } else {
                "Ready"
            }
        } else if enabled {
            "Starting…"
        } else {
            "Off"
        };
        ui.label(RichText::new(state).size(10.0).color(self.muted()));
        if !shared.vcam_error.is_empty() {
            ui.label(
                RichText::new(&shared.vcam_error)
                    .size(10.0)
                    .color(self.warn()),
            );
        }
    }

    fn audio_ui(&mut self, ui: &mut egui::Ui) {
        self.section(ui, "AUDIO");
        if ui
            .checkbox(&mut self.virtual_mic_enabled, "Virtual microphone")
            .changed()
        {
            self.audio.mic_sink.set_enabled(self.virtual_mic_enabled);
            if self.virtual_mic_enabled && !self.virtual_device.is_empty() {
                let dev = self.virtual_device.clone();
                if let Err(e) = self.audio.start_virtual_mic(&dev) {
                    self.set_toast(e);
                }
            }
        }
        if self.virtual_device.is_empty() {
            ui.label(
                RichText::new("No virtual audio cable found.")
                    .size(10.0)
                    .color(self.warn()),
            );
        }
        if ui
            .checkbox(&mut self.monitor_enabled, "Monitor on speakers")
            .changed()
        {
            self.audio.monitor_sink.set_enabled(self.monitor_enabled);
            if self.monitor_enabled {
                let dev = self.monitor_device.clone();
                if let Err(e) = self.audio.start_monitor(&dev) {
                    self.set_toast(e);
                }
            }
        }

        audio_meter(ui, "Mic", self.audio.mic_sink.level(), self.strong());
        if self.monitor_enabled {
            audio_meter(
                ui,
                "Monitor",
                self.audio.monitor_sink.level(),
                Color32::from_gray(150),
            );
        }

        egui::CollapsingHeader::new(RichText::new("Devices").size(11.0))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new("Monitor").size(10.0).color(self.muted()));
                let mut monitor_dev = self.monitor_device.clone();
                egui::ComboBox::from_id_salt("monitor_dev")
                    .selected_text(short_device(&monitor_dev))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for name in &self.device_names {
                            ui.selectable_value(&mut monitor_dev, name.clone(), short_device(name));
                        }
                    });
                if monitor_dev != self.monitor_device {
                    self.monitor_device = monitor_dev;
                    let dev = self.monitor_device.clone();
                    if let Err(e) = self.audio.start_monitor(&dev) {
                        self.set_toast(e);
                    }
                }

                ui.label(
                    RichText::new("Virtual cable")
                        .size(10.0)
                        .color(self.muted()),
                );
                let mut virtual_dev = self.virtual_device.clone();
                egui::ComboBox::from_id_salt("virtual_dev")
                    .selected_text(short_device(&virtual_dev))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for name in &self.device_names {
                            ui.selectable_value(&mut virtual_dev, name.clone(), short_device(name));
                        }
                    });
                if virtual_dev != self.virtual_device {
                    self.virtual_device = virtual_dev;
                    if !self.virtual_device.is_empty() {
                        let dev = self.virtual_device.clone();
                        if let Err(e) = self.audio.start_virtual_mic(&dev) {
                            self.set_toast(e);
                        }
                    }
                }
                if ui.small_button("Refresh devices").clicked() {
                    self.device_names = self.audio.output_devices();
                }

                let mut mic_vol = self.audio.mic_sink.volume();
                if ui
                    .add(egui::Slider::new(&mut mic_vol, 0.0..=1.5).text("Mic level"))
                    .changed()
                {
                    self.audio.mic_sink.set_volume(mic_vol);
                }
                if self.monitor_enabled {
                    let mut monitor_vol = self.audio.monitor_sink.volume();
                    if ui
                        .add(egui::Slider::new(&mut monitor_vol, 0.0..=1.5).text("Monitor level"))
                        .changed()
                    {
                        self.audio.monitor_sink.set_volume(monitor_vol);
                    }
                }
            });
    }

    fn noise_ui(&mut self, ui: &mut egui::Ui) {
        self.section(ui, "NOISE SUPPRESSION");
        let mut enabled = self.engine.noise.enabled();
        if ui.checkbox(&mut enabled, "RNNoise").changed() {
            self.engine.noise.set_enabled(enabled);
        }
        let mut strength = (self.engine.noise.strength() * 100.0).round() as i32;
        if ui
            .add_enabled(
                enabled,
                egui::Slider::new(&mut strength, 0..=100).text("Strength"),
            )
            .changed()
        {
            self.engine.noise.set_strength(strength as f32 / 100.0);
        }
        ui.label(
            RichText::new("Applied to the virtual microphone and monitor.")
                .size(9.0)
                .color(self.muted()),
        );
    }

    fn preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(rgb(self.theme.background)))
            .show(ctx, |ui| {
                let rect = ui.max_rect().shrink(18.0);
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, Color32::from_gray(6));

                if let Some(tex) = &self.texture {
                    let size = tex.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(1.0);
                    let draw = size * scale;
                    let target = Rect::from_center_size(rect.center(), draw);
                    let uv = if self.mirror_preview {
                        Rect::from_min_max(Pos2::new(1.0, 0.0), Pos2::new(0.0, 1.0))
                    } else {
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                    };
                    painter.image(tex.id(), target, uv, Color32::WHITE);

                    let shared = self.engine.shared.lock().clone();
                    let text = format!(
                        "{}×{}   {:.0} ms   {:.0} kbps",
                        shared.video_width,
                        shared.video_height,
                        shared.latency_ms,
                        shared.received_kbps
                    );
                    let galley = painter.layout_no_wrap(
                        text,
                        FontId::monospace(10.0),
                        Color32::from_gray(190),
                    );
                    let pos = target.left_bottom() + Vec2::new(10.0, -10.0 - galley.size().y);
                    painter.rect_filled(
                        Rect::from_min_size(
                            pos - Vec2::new(6.0, 4.0),
                            galley.size() + Vec2::new(12.0, 8.0),
                        ),
                        3.0,
                        Color32::from_black_alpha(130),
                    );
                    painter.galley(pos, galley, Color32::WHITE);
                } else {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "No video",
                        FontId::proportional(18.0),
                        Color32::from_gray(120),
                    );
                }
            });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = self.show_settings;
        egui::Window::new("Theme")
            .open(&mut open)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                let mut kind = self.theme.kind;
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(kind.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for k in [
                            ThemeKind::Mono,
                            ThemeKind::Graphite,
                            ThemeKind::Light,
                            ThemeKind::Custom,
                        ] {
                            ui.selectable_value(&mut kind, k, k.label());
                        }
                    });
                if kind != self.theme.kind {
                    self.theme.kind = kind;
                    apply_preset(&mut self.theme, kind);
                    self.theme_dirty = true;
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Accent");
                    if ui.color_edit_button_srgb(&mut self.theme.accent).changed() {
                        self.theme.kind = ThemeKind::Custom;
                        self.theme_dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Background");
                    if ui
                        .color_edit_button_srgb(&mut self.theme.background)
                        .changed()
                    {
                        self.theme.kind = ThemeKind::Custom;
                        self.theme_dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Panels");
                    if ui.color_edit_button_srgb(&mut self.theme.panel).changed() {
                        self.theme.kind = ThemeKind::Custom;
                        self.theme_dirty = true;
                    }
                });
                if ui
                    .checkbox(&mut self.theme.light, "Light background")
                    .changed()
                {
                    self.theme.kind = ThemeKind::Custom;
                    self.theme_dirty = true;
                }
                if ui
                    .add(egui::Slider::new(&mut self.theme.roundness, 0..=16).text("Corners"))
                    .changed()
                {
                    self.theme_dirty = true;
                }
            });
        self.show_settings = open;
    }

    fn section(&self, ui: &mut egui::Ui, title: &str) {
        if ui.available_height() > 0.0 {
            ui.add_space(12.0);
        }
        ui.label(RichText::new(title).size(9.0).strong().color(self.muted()));
        ui.add_space(3.0);
    }

    fn strong(&self) -> Color32 {
        if self.theme.light {
            Color32::from_gray(20)
        } else {
            Color32::from_gray(235)
        }
    }

    fn muted(&self) -> Color32 {
        if self.theme.light {
            Color32::from_gray(100)
        } else {
            Color32::from_gray(140)
        }
    }

    fn warn(&self) -> Color32 {
        if self.theme.light {
            Color32::from_rgb(170, 45, 45)
        } else {
            Color32::from_rgb(235, 125, 125)
        }
    }
}

impl eframe::App for RewebcamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.theme_dirty {
            self.theme.apply(ctx);
            self.theme_dirty = false;
        }
        self.update_texture(ctx);
        self.top_bar(ctx);
        self.right_panel(ctx);
        self.preview(ctx);
        self.settings_window(ctx);
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}

fn rgb(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn apply_preset(theme: &mut ThemeConfig, kind: ThemeKind) {
    match kind {
        ThemeKind::Mono => {
            theme.accent = [232, 232, 232];
            theme.background = [8, 8, 8];
            theme.panel = [16, 16, 16];
            theme.light = false;
            theme.roundness = 4;
        }
        ThemeKind::Graphite => {
            theme.accent = [200, 200, 200];
            theme.background = [24, 24, 24];
            theme.panel = [34, 34, 34];
            theme.light = false;
            theme.roundness = 6;
        }
        ThemeKind::Light => {
            theme.accent = [24, 24, 24];
            theme.background = [242, 242, 242];
            theme.panel = [255, 255, 255];
            theme.light = true;
            theme.roundness = 4;
        }
        ThemeKind::Custom => {}
    }
}

fn preferred_resolution(cam: &CameraInfo) -> Option<ResolutionInfo> {
    cam.resolutions
        .iter()
        .find(|r| r.width == 1920 && r.height == 1080)
        .or_else(|| {
            cam.resolutions
                .iter()
                .find(|r| r.width == 1280 && r.height == 720)
        })
        .or_else(|| cam.resolutions.first())
        .cloned()
}

fn short_device(name: &str) -> String {
    if name.len() > 34 {
        format!("…{}", &name[name.len() - 33..])
    } else if name.is_empty() {
        "—".to_string()
    } else {
        name.to_string()
    }
}

fn audio_meter(ui: &mut egui::Ui, label: &str, level: f32, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(9.0)
                .color(Color32::from_gray(140)),
        );
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width() - 4.0, 6.0),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, 3.0, ui.visuals().widgets.inactive.bg_fill);
        let mut filled = rect;
        filled.set_width(rect.width() * level.clamp(0.0, 1.0));
        ui.painter().rect_filled(filled, 3.0, color);
    });
}
