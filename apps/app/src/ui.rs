use crate::audio::AudioEngine;
use crate::engine::{Engine, Shared};
use crate::protocol::{CameraInfo, Control, ResolutionInfo};
use crate::theme::{ThemeConfig, ThemeKind};
use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, TextureOptions, Vec2};
use egui_phosphor::regular as ph;
use std::sync::Arc;
use std::time::Duration;

pub struct ReCastApp {
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
    toast: String,
    toast_until: Option<std::time::Instant>,
    logo: Option<egui::TextureHandle>,
    qr: Option<egui::TextureHandle>,
    qr_payload: String,
    chrome_ready: bool,
}

#[derive(Clone, Copy)]
enum ModeKind {
    Camera,
    Mic,
}

impl ReCastApp {
    pub fn new(cc: &eframe::CreationContext<'_>, engine: Arc<Engine>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

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
        // A failing virtual microphone is worth a message, otherwise the
        // gain and the voice seem to do nothing.
        let mut startup_note = String::new();
        if virtual_device.is_empty() {
            startup_note =
                "No virtual audio cable found. Install one to send the microphone to other apps."
                    .to_string();
        } else if let Err(e) = audio.start_virtual_mic(&virtual_device) {
            startup_note = format!("Virtual microphone: {e}");
        }

        let logo = Some(cc.egui_ctx.load_texture(
            "recast-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [256, 256],
                include_bytes!("../assets/icon.rgba"),
            ),
            TextureOptions::LINEAR,
        ));

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
            toast: startup_note.clone(),
            toast_until: if startup_note.is_empty() {
                None
            } else {
                Some(std::time::Instant::now() + Duration::from_secs(12))
            },
            logo,
            qr: None,
            qr_payload: String::new(),
            chrome_ready: false,
        }
    }

    fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = msg.into();
        self.toast_until = Some(std::time::Instant::now() + Duration::from_secs(6));
    }

    fn toast_alive(&self) -> bool {
        !self.toast.is_empty()
            && self
                .toast_until
                .map(|t| t > std::time::Instant::now())
                .unwrap_or(false)
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

    fn apply_caps_defaults(&mut self, shared: &Shared) {
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

    /// The title bar of the borderless window. It draws the brand, the live
    /// state, the input indicators, the window buttons and the draggable area.
    fn window_bar(&mut self, ctx: &egui::Context) {
        let shared = self.engine.shared.lock().clone();
        let live = shared.connected;
        let fg = self.strong();
        let muted = self.muted();
        let background = rgb(self.theme.background);
        let border = self.line();

        egui::TopBottomPanel::top("titlebar")
            .exact_height(44.0)
            .frame(
                egui::Frame::new()
                    .fill(background)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.add(
                            egui::Image::new(logo)
                                .fit_to_exact_size(egui::vec2(24.0, 24.0))
                                .corner_radius(4.0),
                        );
                    }
                    ui.add_space(2.0);
                    ui.label(RichText::new("ReCast").size(14.0).strong().color(fg));
                    ui.add_space(12.0);

                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        3.0,
                        if live { fg } else { Color32::from_gray(78) },
                    );
                    ui.label(
                        RichText::new(if live { "LIVE" } else { "WAITING" })
                            .size(9.0)
                            .strong()
                            .color(if live { fg } else { muted }),
                    );
                    if live && !shared.phone_name.is_empty() {
                        ui.label(RichText::new(&shared.phone_name).size(10.0).color(muted));
                    }

                    // The empty middle of the bar moves the window.
                    let reserved = 232.0;
                    let drag_width = (ui.available_width() - reserved).max(0.0);
                    let (_, drag) = ui.allocate_exact_size(
                        egui::vec2(drag_width, 30.0),
                        egui::Sense::click_and_drag(),
                    );
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.window_button(ui, ctx, WindowButton::Close);
                        self.window_button(ui, ctx, WindowButton::Maximize);
                        self.window_button(ui, ctx, WindowButton::Minimize);
                        if live {
                            ui.add_space(10.0);
                            let mic_glyph = if shared.phone_mic {
                                ph::MICROPHONE
                            } else {
                                ph::MICROPHONE_SLASH
                            };
                            let mic_color = if shared.phone_mic {
                                fg
                            } else {
                                Color32::from_gray(78)
                            };
                            ui.label(RichText::new(mic_glyph).size(15.0).color(mic_color))
                                .on_hover_text(if shared.phone_mic {
                                    "Phone microphone on"
                                } else {
                                    "Phone microphone off"
                                });
                            ui.add_space(4.0);
                            let cam_glyph = if shared.video_enabled {
                                ph::VIDEO_CAMERA
                            } else {
                                ph::VIDEO_CAMERA_SLASH
                            };
                            let cam_color = if shared.video_enabled {
                                fg
                            } else {
                                Color32::from_gray(78)
                            };
                            ui.label(RichText::new(cam_glyph).size(15.0).color(cam_color))
                                .on_hover_text(if shared.video_enabled {
                                    "Phone camera on"
                                } else {
                                    "Phone camera off"
                                });
                        }
                    });
                });
            });

        // A one pixel border gives the borderless window a clear edge on
        // every background, and separates the title bar from the content.
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("recast-window-border"),
        ));
        let stroke = egui::Stroke::new(1.0_f32, border);
        painter.rect_stroke(screen.shrink(0.5), 0.0, stroke, egui::StrokeKind::Inside);
        painter.line_segment(
            [
                Pos2::new(screen.left(), screen.top() + 44.0),
                Pos2::new(screen.right(), screen.top() + 44.0),
            ],
            stroke,
        );
    }

    fn window_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, kind: WindowButton) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(40.0, 28.0), egui::Sense::click());
        let hovered = response.hovered();
        let close = matches!(kind, WindowButton::Close);
        if hovered {
            let fill = if close {
                Color32::from_rgb(196, 43, 28)
            } else if self.theme.light {
                Color32::from_gray(220)
            } else {
                Color32::from_gray(38)
            };
            ui.painter().rect_filled(rect, 4.0, fill);
        }
        let color = if hovered && close {
            Color32::WHITE
        } else {
            self.muted()
        };
        let stroke = egui::Stroke::new(1.2_f32, color);
        let center = rect.center();
        let painter = ui.painter();
        match kind {
            WindowButton::Minimize => {
                painter.line_segment(
                    [
                        center + egui::vec2(-5.0, 3.0),
                        center + egui::vec2(5.0, 3.0),
                    ],
                    stroke,
                );
            }
            WindowButton::Maximize => {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if maximized {
                    painter.rect_stroke(
                        Rect::from_center_size(center + egui::vec2(1.5, 1.5), egui::vec2(8.0, 8.0)),
                        0.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                    painter.rect_stroke(
                        Rect::from_center_size(center - egui::vec2(1.5, 1.5), egui::vec2(8.0, 8.0)),
                        0.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.rect_stroke(
                        Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
                        0.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
            WindowButton::Close => {
                painter.line_segment(
                    [
                        center + egui::vec2(-5.0, -5.0),
                        center + egui::vec2(5.0, 5.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        center + egui::vec2(5.0, -5.0),
                        center + egui::vec2(-5.0, 5.0),
                    ],
                    stroke,
                );
            }
        }

        if response.clicked() {
            match kind {
                WindowButton::Minimize => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                }
                WindowButton::Maximize => {
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                WindowButton::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    /// The first screen: pairing before anything else.
    fn connection_screen(&mut self, ctx: &egui::Context) {
        let shared = self.engine.shared.lock().clone();
        let background = rgb(self.theme.background);

        let payload = crate::pairing::payload(&shared.local_ips, shared.port, &shared.pin);
        if self.qr_payload != payload {
            self.qr = make_qr_texture(ctx, &payload);
            self.qr_payload = payload;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(background))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Keep a gap for the scroll bar.
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 0,
                                right: 12,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                self.pairing_page(ui, &shared);
                            });
                    });
            });
    }

    fn pairing_page(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let full = ui.available_width();
        let content_w = (full - 48.0).max(320.0).min(1040.0);
        let pad = ((full - content_w) * 0.5).max(24.0);
        let fg = self.strong();
        let muted = self.muted();

        ui.horizontal_top(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(content_w);
                ui.add_space(26.0);
                ui.label(
                    RichText::new(format!("{}   PAIRING", ph::QR_CODE))
                        .size(9.0)
                        .strong()
                        .color(muted),
                );
                ui.add_space(5.0);
                ui.label(
                    RichText::new("Pair your phone")
                        .size(28.0)
                        .strong()
                        .color(fg),
                );
                ui.add_space(5.0);
                ui.label(
                    RichText::new(
                        "Scan once with ReCast Phone. The camera and microphone connect \
                         automatically over Wi-Fi.",
                    )
                    .size(11.0)
                    .color(muted),
                );
                ui.add_space(22.0);

                let wide = content_w >= 900.0;
                if wide {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(334.0);
                            self.qr_card(ui, 300.0);
                            ui.add_space(10.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Open ReCast Phone, then tap Scan QR code.")
                                        .size(10.0)
                                        .color(muted),
                                );
                            });
                        });
                        ui.add_space(40.0);
                        ui.vertical(|ui| {
                            self.pairing_details(ui, shared);
                        });
                    });
                } else {
                    let side = (content_w - 80.0).clamp(220.0, 320.0);
                    ui.vertical_centered(|ui| {
                        self.qr_card(ui, side);
                    });
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Open ReCast Phone, then tap Scan QR code.")
                                .size(10.0)
                                .color(muted),
                        );
                    });
                    ui.add_space(20.0);
                    self.pairing_details(ui, shared);
                }

                ui.add_space(28.0);
            });
        });
    }

    fn qr_card(&self, ui: &mut egui::Ui, side: f32) {
        egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(12)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(side);
                if let Some(qr) = &self.qr {
                    ui.add(
                        egui::Image::new(qr)
                            .fit_to_exact_size(egui::vec2(side, side))
                            .corner_radius(2.0),
                    );
                } else {
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                }
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Scan with ReCast Phone")
                            .size(10.0)
                            .strong()
                            .color(Color32::from_gray(35)),
                    );
                });
            });
    }

    fn pairing_details(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();
        ui.set_width(ui.available_width());

        ui.label(
            RichText::new("CONNECTION DETAILS")
                .size(9.0)
                .strong()
                .color(muted),
        );
        ui.add_space(8.0);

        self.address_card(ui, shared);
        ui.add_space(10.0);
        self.pin_card(ui, shared);
        ui.add_space(12.0);
        self.connection_status_card(ui, shared);

        ui.add_space(18.0);
        ui.label(
            RichText::new("HOW TO CONNECT")
                .size(9.0)
                .strong()
                .color(muted),
        );
        ui.add_space(6.0);
        step_row(
            ui,
            "1",
            "Install ReCastPhone.apk on the Android phone.",
            fg,
            muted,
        );
        step_row(ui, "2", "Open it and tap Scan QR code.", fg, muted);
        step_row(
            ui,
            "3",
            "Point the phone at the code on this screen.",
            fg,
            muted,
        );

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let button = egui::Button::new(
                RichText::new(format!("{}   Install phone app over USB", ph::USB)).size(11.0),
            )
            .min_size(egui::vec2(0.0, 30.0));
            if ui.add_enabled(!shared.adb_busy, button).clicked() {
                self.engine.install_phone_app();
                self.set_toast("USB setup started. Approve the prompt on the phone.");
            }
            if shared.adb_busy {
                ui.add(egui::Spinner::new().size(14.0));
            }
        });
        ui.label(
            RichText::new(
                "USB setup needs the Android platform tools and USB debugging. Wi-Fi needs \
                 both devices on the same network.",
            )
            .size(9.0)
            .color(muted),
        );

        ui.add_space(16.0);
        self.usb_card(ui, shared);

        if self.toast_alive() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("{}   {}", ph::INFO, self.toast))
                    .size(10.0)
                    .color(self.warn()),
            );
        }
    }

    /// The cable connection, which needs no network at all.
    fn usb_card(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();

        egui::Frame::new()
            .fill(self.card_fill())
            .stroke(egui::Stroke::new(1.0_f32, self.line()))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ph::USB).size(13.0).color(muted));
                    ui.label(RichText::new("USB CABLE").size(8.5).strong().color(muted));
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Connect without any network")
                        .size(13.0)
                        .strong()
                        .color(fg),
                );
                ui.label(
                    RichText::new(
                        "The camera and the microphone travel through the cable. The phone \
                         needs USB debugging enabled.",
                    )
                    .size(9.0)
                    .color(muted),
                );
                ui.add_space(8.0);
                let button = egui::Button::new(
                    RichText::new(format!("{}   Connect over USB", ph::PLUGS_CONNECTED)).size(11.0),
                )
                .min_size(egui::vec2(0.0, 30.0));
                if ui.add_enabled(!shared.adb_busy, button).clicked() {
                    self.engine.connect_over_usb();
                    self.set_toast("Opening the USB tunnel. Approve the prompt on the phone.");
                }
                if shared.adb_busy {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
    }

    fn address_card(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();
        let ip = shared
            .local_ips
            .first()
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".into());
        let address = format!("{ip}:{}", shared.port);
        let extra: Vec<String> = shared
            .local_ips
            .iter()
            .skip(1)
            .take(2)
            .map(|ip| format!("{ip}:{}", shared.port))
            .collect();
        let more = shared.local_ips.len().saturating_sub(3);

        egui::Frame::new()
            .fill(self.card_fill())
            .stroke(egui::Stroke::new(1.0_f32, self.line()))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ph::WIFI_HIGH).size(13.0).color(muted));
                    ui.label(RichText::new("PC ADDRESS").size(8.5).strong().color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let response = copy_icon(ui, muted);
                        if response.clicked() {
                            ui.ctx().copy_text(address.clone());
                            self.set_toast("Address copied.");
                        }
                        response.on_hover_text("Copy the address");
                    });
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new(&address)
                        .monospace()
                        .size(16.0)
                        .strong()
                        .color(fg),
                );
                for (index, address) in extra.iter().enumerate() {
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if index == 0 { "Also" } else { "" })
                                .size(9.0)
                                .color(muted),
                        );
                        ui.label(RichText::new(address).monospace().size(10.0).color(muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let response = copy_icon(ui, muted);
                            if response.clicked() {
                                ui.ctx().copy_text(address.clone());
                                self.set_toast("Address copied.");
                            }
                            response.on_hover_text("Copy this address");
                        });
                    });
                }
                if more > 0 {
                    ui.label(
                        RichText::new(format!("+{more} more addresses inside the QR code"))
                            .size(9.0)
                            .color(muted),
                    );
                }
            });
    }

    fn pin_card(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();
        let digits: Vec<char> = shared.pin.chars().collect();
        let pin = shared.pin.clone();

        egui::Frame::new()
            .fill(self.card_fill())
            .stroke(egui::Stroke::new(1.0_f32, self.line()))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ph::KEY).size(13.0).color(muted));
                    ui.label(RichText::new("PAIRING PIN").size(8.5).strong().color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let response = copy_icon(ui, muted);
                        if response.clicked() {
                            ui.ctx().copy_text(pin.clone());
                            self.set_toast("PIN copied.");
                        }
                        response.on_hover_text("Copy the PIN");
                    });
                });
                ui.add_space(8.0);

                let count = digits.len().max(1);
                let gap = 6.0;
                let cell_w = ((ui.available_width() - gap * (count as f32 - 1.0)) / count as f32)
                    .clamp(28.0, 44.0);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(cell_w * count as f32 + gap * (count as f32 - 1.0), 46.0),
                    egui::Sense::hover(),
                );
                for (index, digit) in digits.iter().enumerate() {
                    let cell = Rect::from_min_size(
                        rect.min + egui::vec2(index as f32 * (cell_w + gap), 0.0),
                        egui::vec2(cell_w, rect.height()),
                    );
                    ui.painter().rect(
                        cell,
                        7.0,
                        self.inset_fill(),
                        egui::Stroke::new(1.0_f32, self.line()),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        cell.center(),
                        Align2::CENTER_CENTER,
                        digit,
                        FontId::monospace(20.0),
                        fg,
                    );
                }
                ui.add_space(5.0);
                ui.label(
                    RichText::new("One-time PIN, generated when ReCast starts.")
                        .size(9.0)
                        .color(muted),
                );
            });
    }

    fn connection_status_card(&self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();
        let status = if shared.adb_busy {
            "Installing the phone app over USB…".to_string()
        } else if shared.status.is_empty() {
            "Waiting for the phone…".to_string()
        } else {
            shared.status.clone()
        };

        egui::Frame::new()
            .fill(self.card_fill())
            .stroke(egui::Stroke::new(1.0_f32, self.line()))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&status).size(11.0).color(fg));
                        ui.label(
                            RichText::new("Keep both devices on the same Wi-Fi network.")
                                .size(9.0)
                                .color(muted),
                        );
                    });
                });
                if !shared.decoder_error.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{}   {}", ph::WARNING, shared.decoder_error))
                            .size(9.5)
                            .color(self.warn()),
                    );
                }
            });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("controls")
            .resizable(false)
            .exact_width(364.0)
            .frame(
                egui::Frame::new()
                    .fill(rgb(self.theme.panel))
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // The scroll bar is drawn over the content, so the
                        // controls keep a gap on the right side.
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 0,
                                right: 12,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                let shared = self.engine.shared.lock().clone();
                                self.apply_caps_defaults(&shared);

                                self.section(ui, ph::BROADCAST, "SESSION");
                                self.session_card(ui, &shared);
                                ui.add_space(12.0);

                                let cam_sub = if shared.video_enabled {
                                    shared
                                        .selected_resolution
                                        .as_ref()
                                        .map(|r| {
                                            format!(
                                                "On · {}×{} · {} fps",
                                                r.width, r.height, shared.selected_fps
                                            )
                                        })
                                        .unwrap_or_else(|| "On · starting…".to_string())
                                } else {
                                    "Off · camera closed".to_string()
                                };
                                let mic_sub = if shared.phone_mic {
                                    "On · streaming to the PC".to_string()
                                } else {
                                    "Off · microphone released".to_string()
                                };
                                ui.horizontal(|ui| {
                                    // Both tiles get the same width, so the
                                    // pair stays even.
                                    let width = (ui.available_width() - 8.0) * 0.5;
                                    self.mode_tile(
                                        ui,
                                        ModeKind::Camera,
                                        shared.video_enabled,
                                        &cam_sub,
                                        &shared,
                                        width,
                                    );
                                    self.mode_tile(
                                        ui,
                                        ModeKind::Mic,
                                        shared.phone_mic,
                                        &mic_sub,
                                        &shared,
                                        width,
                                    );
                                });

                                self.camera_ui(ui, &shared);
                                self.output_ui(ui, &shared);
                                self.audio_ui(ui, &shared);
                                self.noise_ui(ui);

                                if self.toast_alive() {
                                    ui.add_space(14.0);
                                    ui.label(
                                        RichText::new(format!("{}   {}", ph::INFO, self.toast))
                                            .size(9.5)
                                            .color(self.warn()),
                                    );
                                }
                                ui.add_space(10.0);
                            });
                    });
            });
    }

    fn session_card(&self, ui: &mut egui::Ui, shared: &Shared) {
        let fg = self.strong();
        let muted = self.muted();
        let name = if shared.phone_name.is_empty() {
            "Android phone"
        } else {
            &shared.phone_name
        };

        egui::Frame::new()
            .fill(self.card_fill())
            .stroke(egui::Stroke::new(1.0_f32, self.line()))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (chip, _) =
                        ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                    ui.painter().rect_filled(chip, 8.0, self.inset_fill());
                    ui.painter().text(
                        chip.center(),
                        Align2::CENTER_CENTER,
                        ph::DEVICE_MOBILE,
                        FontId::proportional(15.0),
                        fg,
                    );
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(name).size(12.5).strong().color(fg));
                        ui.label(
                            RichText::new(if shared.usb_connected {
                                "Connected over USB"
                            } else {
                                "Connected over Wi-Fi"
                            })
                            .size(9.0)
                            .color(muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.pill(ui, "LIVE", true);
                    });
                });
            });
    }

    fn mode_tile(
        &mut self,
        ui: &mut egui::Ui,
        kind: ModeKind,
        enabled: bool,
        subtitle: &str,
        shared: &Shared,
        width: f32,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 92.0), egui::Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = response.hovered();
        let fg = self.strong();
        let muted = self.muted();

        let base = if self.theme.light {
            if enabled {
                Color32::WHITE
            } else {
                Color32::from_gray(240)
            }
        } else if enabled {
            Color32::from_gray(24)
        } else {
            Color32::from_gray(13)
        };
        let fill = if hovered {
            if self.theme.light {
                Color32::from_gray(246)
            } else if enabled {
                Color32::from_gray(30)
            } else {
                Color32::from_gray(19)
            }
        } else {
            base
        };
        let border = if enabled {
            if hovered {
                Color32::from_gray(130)
            } else {
                Color32::from_gray(80)
            }
        } else {
            self.line()
        };
        ui.painter().rect(
            rect,
            10.0,
            fill,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        let (icon, title) = match kind {
            ModeKind::Camera => (
                if enabled {
                    ph::VIDEO_CAMERA
                } else {
                    ph::VIDEO_CAMERA_SLASH
                },
                "Camera",
            ),
            ModeKind::Mic => (
                if enabled {
                    ph::MICROPHONE
                } else {
                    ph::MICROPHONE_SLASH
                },
                "Microphone",
            ),
        };
        let icon_color = if enabled { fg } else { Color32::from_gray(100) };
        // The icon sits on its own line so that a long name never runs into
        // the state dot in the corner.
        ui.painter().text(
            Pos2::new(rect.left() + 14.0, rect.top() + 22.0),
            Align2::LEFT_CENTER,
            icon,
            FontId::proportional(18.0),
            icon_color,
        );
        ui.painter().circle_filled(
            Pos2::new(rect.right() - 14.0, rect.top() + 22.0),
            3.5,
            if enabled {
                self.accent()
            } else {
                Color32::from_gray(70)
            },
        );
        ui.painter().text(
            Pos2::new(rect.left() + 14.0, rect.top() + 50.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.0),
            if enabled { fg } else { muted },
        );
        ui.painter().text(
            Pos2::new(rect.left() + 14.0, rect.top() + 70.0),
            Align2::LEFT_CENTER,
            subtitle,
            FontId::proportional(9.0),
            muted,
        );

        if response.clicked() {
            match kind {
                ModeKind::Camera => {
                    if enabled {
                        if !shared.phone_mic {
                            self.set_toast(
                                "Keep at least one input on. Turn the microphone on first.",
                            );
                        } else {
                            self.engine.set_video_enabled(false);
                        }
                    } else {
                        self.engine.set_video_enabled(true);
                    }
                }
                ModeKind::Mic => {
                    if enabled {
                        if !shared.video_enabled {
                            self.set_toast("Keep at least one input on. Turn the camera on first.");
                        } else {
                            self.engine.set_audio_enabled(false);
                        }
                    } else {
                        self.engine.set_audio_enabled(true);
                    }
                }
            }
        }
    }

    fn camera_ui(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        self.section(ui, ph::VIDEO_CAMERA, "CAMERA");
        let caps = shared.caps.clone();
        let muted = self.muted();

        if !shared.video_enabled {
            self.note(
                ui,
                "Camera is off. The phone camera is closed and sends no video.",
            );
        }

        if let Some(caps) = caps {
            ui.add_enabled_ui(shared.video_enabled, |ui| {
                if !caps.cameras.is_empty() {
                    ui.label(RichText::new("Camera").size(9.0).color(muted));
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
                    ui.label(RichText::new("Resolution").size(9.0).color(muted));
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
                            ui.label(RichText::new("Frame rate").size(9.0).color(muted));
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

                ui.add_space(4.0);
                let mut torch = shared.torch;
                if self.toggle(ui, ph::FLASHLIGHT, "Torch", &mut torch) {
                    self.engine.shared.lock().torch = torch;
                    self.engine
                        .send_control(Control::SetTorch { enabled: torch });
                }

                ui.add_space(2.0);
                ui.label(RichText::new("Zoom").size(9.0).color(muted));
                let mut zoom = shared.zoom;
                if ui
                    .add(egui::Slider::new(&mut zoom, 1.0..=8.0).show_value(true))
                    .changed()
                {
                    self.engine.shared.lock().zoom = zoom;
                    self.engine.send_control(Control::SetZoom { value: zoom });
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let button = egui::Button::new(
                        RichText::new(format!("{}   Rotate 90°", ph::ARROW_CLOCKWISE)).size(10.0),
                    );
                    if ui.add(button).clicked() {
                        self.engine.rotate_output(90);
                    }
                    ui.label(
                        RichText::new(format!("{}°", shared.rotation_offset))
                            .monospace()
                            .size(10.0)
                            .color(muted),
                    );
                    if shared.rotation_offset != 0 && ui.small_button("Reset").clicked() {
                        self.engine.reset_rotation();
                    }
                });

                let mut mirror = self.mirror_preview;
                if self.toggle(ui, ph::FLIP_HORIZONTAL, "Mirror preview", &mut mirror) {
                    self.mirror_preview = mirror;
                }
            });
        } else {
            ui.label(
                RichText::new("Waiting for the phone…")
                    .size(10.5)
                    .color(muted),
            );
        }
    }

    fn output_ui(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        self.section(ui, ph::BROADCAST, "VIRTUAL CAMERA");
        let fg = self.strong();
        let muted = self.muted();
        let mut enabled = self.engine.vcam_enabled();
        if self.toggle(ui, ph::VIDEO_CAMERA, "ReCast Camera", &mut enabled) {
            self.engine.set_vcam_enabled(enabled);
        }

        let (state, color) = if shared.vcam_active {
            if shared.vcam_connected {
                ("In use by another app", fg)
            } else {
                ("Ready to use", fg)
            }
        } else if enabled {
            ("Starting…", muted)
        } else {
            ("Off", Color32::from_gray(95))
        };
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.0, color);
            ui.label(RichText::new(state).size(9.5).color(color));
        });
        ui.label(
            RichText::new("Choose “ReCast Camera” in your browser, Zoom, Meet or OBS.")
                .size(9.0)
                .color(muted),
        );
        if !shared.vcam_error.is_empty() {
            ui.label(
                RichText::new(format!("{}   {}", ph::WARNING, shared.vcam_error))
                    .size(9.5)
                    .color(self.warn()),
            );
        }
    }

    fn audio_ui(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        self.section(ui, ph::FADERS, "AUDIO OUTPUT");
        let fg = self.strong();
        let muted = self.muted();
        if !shared.phone_mic {
            self.note(ui, "Microphone is off. The phone microphone is released.");
        }

        let mut virtual_mic = self.virtual_mic_enabled;
        if self.toggle(
            ui,
            ph::MICROPHONE_STAGE,
            "Virtual microphone",
            &mut virtual_mic,
        ) {
            self.virtual_mic_enabled = virtual_mic;
            self.audio.mic_sink.set_enabled(virtual_mic);
            if virtual_mic && !self.virtual_device.is_empty() {
                let dev = self.virtual_device.clone();
                if let Err(e) = self.audio.start_virtual_mic(&dev) {
                    self.set_toast(e);
                }
            }
        }
        if self.virtual_device.is_empty() {
            ui.label(
                RichText::new(format!("{}   No virtual audio cable found.", ph::WARNING))
                    .size(9.5)
                    .color(self.warn()),
            );
        }

        let mut monitor = self.monitor_enabled;
        if self.toggle(ui, ph::SPEAKER_HIGH, "Monitor on speakers", &mut monitor) {
            self.monitor_enabled = monitor;
            self.audio.monitor_sink.set_enabled(monitor);
            if monitor {
                let dev = self.monitor_device.clone();
                if let Err(e) = self.audio.start_monitor(&dev) {
                    self.set_toast(e);
                }
            }
        }

        ui.add_space(4.0);
        audio_meter(ui, "Mic level", self.audio.mic_sink.level(), self.strong());
        if self.monitor_enabled {
            audio_meter(
                ui,
                "Monitor",
                self.audio.monitor_sink.level(),
                Color32::from_gray(150),
            );
        }

        // The boost for the microphone that the other apps hear.
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(ph::MICROPHONE).size(14.0).color(fg));
            ui.label(RichText::new("Microphone gain").size(11.0).color(fg));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("100%").clicked() {
                    self.engine.set_mic_gain(1.0);
                    self.set_toast("Microphone gain set to 100%.");
                }
            });
        });
        let mut gain = self.engine.mic_gain() * 100.0;
        if ui
            .add(egui::Slider::new(&mut gain, 0.0..=300.0).suffix("%"))
            .changed()
        {
            self.engine.set_mic_gain(gain / 100.0);
        }
        ui.label(
            RichText::new("Makes the microphone louder or quieter for the apps that use it.")
                .size(9.0)
                .color(muted),
        );

        egui::CollapsingHeader::new(RichText::new("Devices and levels").size(10.5))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new("Monitor output").size(9.0).color(muted));
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

                ui.label(RichText::new("Virtual cable").size(9.0).color(muted));
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

                ui.horizontal(|ui| {
                    let button = egui::Button::new(
                        RichText::new(format!("{}   Refresh devices", ph::ARROW_CLOCKWISE))
                            .size(10.0),
                    );
                    if ui.add(button).clicked() {
                        self.device_names = self.audio.output_devices();
                    }
                });

                ui.add_space(2.0);
                let mut monitor_vol = self.audio.monitor_sink.volume();
                if ui
                    .add(egui::Slider::new(&mut monitor_vol, 0.0..=1.5).text("Monitor level"))
                    .changed()
                {
                    self.audio.monitor_sink.set_volume(monitor_vol);
                }
            });
    }

    fn noise_ui(&mut self, ui: &mut egui::Ui) {
        self.section(ui, ph::WAVEFORM, "NOISE SUPPRESSION");
        let muted = self.muted();
        let mut enabled = self.engine.noise.enabled();
        if self.toggle(ui, ph::WAVE_SINE, "RNNoise", &mut enabled) {
            self.engine.noise.set_enabled(enabled);
        }
        let mut strength = (self.engine.noise.strength() * 100.0).round() as i32;
        if ui
            .add_enabled(
                enabled,
                egui::Slider::new(&mut strength, 0..=100).suffix("%"),
            )
            .changed()
        {
            self.engine.noise.set_strength(strength as f32 / 100.0);
        }
        ui.label(
            RichText::new("Applied to the virtual microphone and speaker monitor.")
                .size(9.0)
                .color(muted),
        );
    }

    fn preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(rgb(self.theme.background)))
            .show(ctx, |ui| {
                let shared = self.engine.shared.lock().clone();
                let rect = ui.max_rect().shrink(18.0);
                let painter = ui.painter_at(rect);
                painter.rect(
                    rect,
                    10.0,
                    Color32::from_gray(5),
                    egui::Stroke::new(1.0_f32, self.line()),
                    egui::StrokeKind::Inside,
                );

                if !shared.video_enabled {
                    self.preview_message(
                        &painter,
                        rect,
                        ph::VIDEO_CAMERA_SLASH,
                        "Camera is off",
                        "The phone camera is closed. Turn the camera on to resume.",
                    );
                } else if let Some(tex) = &self.texture {
                    let size = tex.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(2.0);
                    let draw = size * scale;
                    let target = Rect::from_center_size(rect.center(), draw);
                    let uv = if self.mirror_preview {
                        Rect::from_min_max(Pos2::new(1.0, 0.0), Pos2::new(0.0, 1.0))
                    } else {
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                    };
                    painter.image(tex.id(), target, uv, Color32::WHITE);

                    if shared.video_width > 0 {
                        let text = format!(
                            "{}×{}     {:.0} ms     {:.0} kbps",
                            shared.video_width,
                            shared.video_height,
                            shared.latency_ms,
                            shared.received_kbps
                        );
                        let galley = painter.layout_no_wrap(
                            text,
                            FontId::monospace(10.0),
                            Color32::from_gray(200),
                        );
                        let pos = target.left_bottom() + Vec2::new(10.0, -10.0 - galley.size().y);
                        painter.rect_filled(
                            Rect::from_min_size(
                                pos - Vec2::new(6.0, 4.0),
                                galley.size() + Vec2::new(12.0, 8.0),
                            ),
                            4.0,
                            Color32::from_black_alpha(150),
                        );
                        painter.galley(pos, galley, Color32::WHITE);
                    }
                } else {
                    self.preview_message(
                        &painter,
                        rect,
                        ph::VIDEO_CAMERA,
                        "Starting camera…",
                        "The first frame from the phone arrives in a moment.",
                    );
                }
            });
    }

    fn preview_message(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        icon: &str,
        title: &str,
        subtitle: &str,
    ) {
        painter.text(
            rect.center() - Vec2::new(0.0, 32.0),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(30.0),
            Color32::from_gray(65),
        );
        painter.text(
            rect.center() + Vec2::new(0.0, 2.0),
            Align2::CENTER_CENTER,
            title,
            FontId::proportional(18.0),
            Color32::from_gray(165),
        );
        painter.text(
            rect.center() + Vec2::new(0.0, 26.0),
            Align2::CENTER_CENTER,
            subtitle,
            FontId::proportional(10.0),
            Color32::from_gray(105),
        );
    }

    /// A full-width row with an icon, a label and a switch.
    fn toggle(&self, ui: &mut egui::Ui, icon: &str, label: &str, value: &mut bool) -> bool {
        let enabled = ui.is_enabled();
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), sense);
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = enabled && response.hovered();
        let fg = if enabled { self.strong() } else { self.muted() };
        if hovered {
            ui.painter().rect_filled(
                rect,
                5.0,
                if self.theme.light {
                    Color32::from_gray(230)
                } else {
                    Color32::from_gray(24)
                },
            );
        }
        ui.painter().text(
            Pos2::new(rect.left() + 2.0, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            FontId::proportional(14.0),
            if enabled { fg } else { Color32::from_gray(95) },
        );
        ui.painter().text(
            Pos2::new(rect.left() + 24.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            fg,
        );

        let track = Rect::from_center_size(
            Pos2::new(rect.right() - 18.0, rect.center().y),
            egui::vec2(30.0, 18.0),
        );
        let track_fill = if *value {
            if enabled {
                self.accent()
            } else {
                Color32::from_gray(120)
            }
        } else if self.theme.light {
            Color32::from_gray(205)
        } else {
            Color32::from_gray(36)
        };
        ui.painter().rect_filled(track, 9.0, track_fill);
        let knob_x = if *value {
            track.right() - 9.0
        } else {
            track.left() + 9.0
        };
        ui.painter().circle_filled(
            Pos2::new(knob_x, track.center().y),
            6.5,
            if *value {
                self.on_accent()
            } else if enabled {
                Color32::from_gray(150)
            } else {
                Color32::from_gray(95)
            },
        );

        if enabled && response.clicked() {
            *value = !*value;
            return true;
        }
        false
    }

    fn note(&self, ui: &mut egui::Ui, text: &str) {
        egui::Frame::new()
            .fill(self.inset_fill())
            .corner_radius(7)
            .inner_margin(egui::Margin::symmetric(9, 7))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(ph::INFO).size(11.0).color(self.muted()));
                    ui.label(RichText::new(text).size(9.5).color(self.muted()));
                });
            });
    }

    fn pill(&self, ui: &mut egui::Ui, text: &str, active: bool) {
        let color = if active {
            self.on_accent()
        } else {
            self.muted()
        };
        let galley =
            ui.painter()
                .layout_no_wrap(text.to_string(), FontId::proportional(8.0), color);
        let size = galley.size() + egui::vec2(14.0, 6.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        if active {
            ui.painter()
                .rect_filled(rect, rect.height() * 0.5, self.accent());
        } else {
            ui.painter().rect_stroke(
                rect,
                rect.height() * 0.5,
                egui::Stroke::new(1.0_f32, self.line()),
                egui::StrokeKind::Inside,
            );
        }
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
    }

    fn section(&self, ui: &mut egui::Ui, icon: &str, title: &str) {
        ui.add_space(16.0);
        let muted = self.muted();
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(11.0).color(muted));
            ui.label(RichText::new(title).size(8.5).strong().color(muted));
            let remaining = ui.available_width();
            if remaining > 12.0 {
                let y = ui.cursor().center().y;
                let left = ui.cursor().left();
                ui.painter().line_segment(
                    [Pos2::new(left + 4.0, y), Pos2::new(left + remaining, y)],
                    egui::Stroke::new(1.0_f32, self.line()),
                );
            }
        });
        ui.add_space(6.0);
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

    fn accent(&self) -> Color32 {
        rgb(self.theme.accent)
    }

    fn on_accent(&self) -> Color32 {
        let [r, g, b] = self.theme.accent;
        let luma = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
        if luma > 0.55 {
            Color32::from_gray(10)
        } else {
            Color32::WHITE
        }
    }

    fn card_fill(&self) -> Color32 {
        if self.theme.light {
            Color32::WHITE
        } else {
            Color32::from_gray(12)
        }
    }

    fn inset_fill(&self) -> Color32 {
        if self.theme.light {
            Color32::from_gray(238)
        } else {
            Color32::from_gray(19)
        }
    }

    fn line(&self) -> Color32 {
        if self.theme.light {
            Color32::from_gray(214)
        } else {
            Color32::from_gray(34)
        }
    }

    /// Applies a theme that was requested from the command line.
    fn apply_pending_theme(&mut self) {
        let requested = self.engine.shared.lock().pending_theme.take();
        let Some(requested) = requested else { return };
        let kind = match requested.as_str() {
            "mono" => ThemeKind::Mono,
            "graphite" => ThemeKind::Graphite,
            "light" => ThemeKind::Light,
            "custom" => ThemeKind::Custom,
            _ => return,
        };
        if kind == ThemeKind::Custom {
            // Custom keeps the colours that are already loaded.
            self.theme.kind = kind;
        } else {
            apply_preset(&mut self.theme, kind);
            self.theme.kind = kind;
        }
        self.theme_dirty = true;
    }
}

impl eframe::App for ReCastApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.chrome_ready {
            self.chrome_ready = crate::window::apply_native_chrome(frame);
        }
        crate::window::resize_edges(ctx);

        self.apply_pending_theme();
        if self.theme_dirty {
            self.theme.apply(ctx);
            self.theme_dirty = false;
        }

        self.window_bar(ctx);

        let (connected, video_on) = {
            let shared = self.engine.shared.lock();
            (shared.connected, shared.video_enabled)
        };
        if connected {
            if video_on {
                self.update_texture(ctx);
            } else {
                self.texture = None;
                self.last_frame_seq = 0;
            }
            self.right_panel(ctx);
            self.preview(ctx);
        } else {
            self.connection_screen(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}

#[derive(Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// A small borderless icon button, used for copy actions.
fn copy_icon(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(ph::COPY).size(13.0).color(color))
            .frame(false)
            .min_size(egui::vec2(22.0, 22.0)),
    )
}

/// One numbered instruction on the pairing screen.
fn step_row(ui: &mut egui::Ui, number: &str, text: &str, fg: Color32, muted: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        ui.painter().rect_stroke(
            rect,
            9.0,
            egui::Stroke::new(1.0_f32, muted),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            number,
            FontId::proportional(9.0),
            fg,
        );
        ui.label(RichText::new(text).size(11.0).color(fg));
    });
}

/// Renders a QR code into an egui texture, with a four-module quiet zone.
fn make_qr_texture(ctx: &egui::Context, data: &str) -> Option<egui::TextureHandle> {
    use qrcode::{Color as QrColor, EcLevel, QrCode};

    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    let modules = code.width();
    let colors = code.to_colors();
    let scale = 8usize;
    let quiet = 4usize;
    let side = (modules + quiet * 2) * scale;
    let mut rgba = vec![255u8; side * side * 4];
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] != QrColor::Dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let px = (x + quiet) * scale + dx;
                    let py = (y + quiet) * scale + dy;
                    let index = (py * side + px) * 4;
                    rgba[index] = 0;
                    rgba[index + 1] = 0;
                    rgba[index + 2] = 0;
                    rgba[index + 3] = 255;
                }
            }
        }
    }
    Some(ctx.load_texture(
        "pairing-qr",
        egui::ColorImage::from_rgba_unmultiplied([side, side], &rgba),
        TextureOptions::NEAREST,
    ))
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
        let width = (ui.available_width() - 4.0).max(20.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 3.0, ui.visuals().widgets.inactive.bg_fill);
        let mut filled = rect;
        filled.set_width(rect.width() * level.clamp(0.0, 1.0));
        ui.painter().rect_filled(filled, 3.0, color);
    });
}
