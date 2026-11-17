// ReCast installer.
//
// The compiled runtime is embedded in this executable. The installer asks
// where ReCast should live, copies the runtime there, registers the virtual
// camera and creates the shortcuts. Afterwards it can start the app and
// remove its own files.

#![windows_subsystem = "windows"]

mod install;
mod window;

use eframe::egui;
use egui_phosphor::regular as ph;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn window_icon() -> egui::IconData {
    const SIDE: u32 = 256;
    egui::IconData {
        rgba: include_bytes!("../../app/assets/icon.rgba").to_vec(),
        width: SIDE,
        height: SIDE,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    Destination,
    Installing,
    Finished,
}

enum Message {
    Progress(String, f32),
    Done(Result<(), String>),
}

struct Installer {
    page: Page,
    install_dir: String,
    desktop_icon: bool,
    start_menu: bool,
    autostart: bool,
    launch_after: bool,
    cleanup_after: bool,
    status: String,
    progress: f32,
    error: Option<String>,
    receiver: Option<Receiver<Message>>,
    logo: Option<egui::TextureHandle>,
    browser: Option<Browser>,
    chrome_ready: bool,
    finished: bool,
    hwnd: isize,
}

/// A small folder picker inside the installer.
struct Browser {
    path: PathBuf,
    entries: Vec<PathBuf>,
}

impl Installer {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = egui::Color32::from_rgb(10, 10, 11);
        visuals.panel_fill = egui::Color32::from_rgb(16, 16, 18);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(28, 28, 32);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(40, 40, 46);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(52, 52, 60);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 28, 32);
        visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::from_rgb(20, 20, 23);
        visuals.selection.bg_fill = egui::Color32::from_rgb(235, 235, 238);
        visuals.selection.stroke = egui::Stroke::NONE;
        visuals.hyperlink_color = egui::Color32::from_rgb(235, 235, 238);
        visuals.error_fg_color = egui::Color32::from_rgb(235, 125, 125);
        visuals.warn_fg_color = egui::Color32::from_rgb(229, 192, 123);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(12.0, 10.0);
            style.spacing.button_padding = egui::vec2(14.0, 8.0);
            style.spacing.interact_size.y = 40.0;
            for (kind, size) in [
                (egui::TextStyle::Body, 16.0),
                (egui::TextStyle::Button, 16.0),
                (egui::TextStyle::Heading, 26.0),
                (egui::TextStyle::Small, 14.0),
            ] {
                if let Some(font) = style.text_styles.get_mut(&kind) {
                    font.size = size;
                }
            }
        });

        let logo = Some(cc.egui_ctx.load_texture(
            "recast-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [256, 256],
                include_bytes!("../../app/assets/icon.rgba"),
            ),
            egui::TextureOptions::LINEAR,
        ));

        let install_dir = default_install_dir();
        Self {
            page: Page::Welcome,
            install_dir: install_dir.to_string_lossy().to_string(),
            desktop_icon: true,
            start_menu: true,
            autostart: false,
            launch_after: true,
            cleanup_after: true,
            status: String::new(),
            progress: 0.0,
            error: None,
            receiver: None,
            logo,
            browser: None,
            chrome_ready: false,
            finished: false,
            hwnd: 0,
        }
    }

    fn start_installation(&mut self) {
        let raw = self.install_dir.trim().trim_matches('"');
        let install_dir = PathBuf::from(raw);
        if raw.is_empty() || !install_dir.is_absolute() {
            self.error = Some(
                "Enter a full folder path. Any drive works, for example X:\\ReCast."
                    .to_string(),
            );
            return;
        }
        let options = install::Options {
            install_dir,
            desktop_icon: self.desktop_icon,
            start_menu: self.start_menu,
            autostart: self.autostart,
        };
        let (sender, receiver) = channel::<Message>();
        self.receiver = Some(receiver);
        self.page = Page::Installing;
        self.progress = 0.0;
        self.status = "Starting".to_string();
        self.error = None;

        std::thread::spawn(move || {
            let mut report = |text: &str, value: f32| {
                let _ = sender.send(Message::Progress(text.to_string(), value));
            };
            let result = install::install(&options, &mut report);
            let _ = sender.send(Message::Done(result));
        });
    }

    fn finish(&mut self) {
        let dir = PathBuf::from(self.install_dir.trim());
        if self.launch_after {
            if let Err(error) = install::launch(&dir) {
                self.status = error;
            }
        }
        if self.cleanup_after {
            let installer = std::env::current_exe().unwrap_or_default();
            install::cleanup(&installer, &dir);
        }
        self.finished = true;
        std::process::exit(0);
    }

    fn open_browser(&mut self) {
        let typed = PathBuf::from(self.install_dir.trim().trim_matches('"'));
        self.browser = Some(Browser::new(existing_folder(&typed)));
    }
}

fn default_install_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("ReCast"),
        None => PathBuf::from("ReCast"),
    }
}

/// Every drive letter Windows has assigned, including substituted drives such as X:.
/// This does not open the drive, so a slow or empty volume cannot hide the others.
fn drive_roots() -> Vec<PathBuf> {
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    (0..26u32)
        .filter(|bit| mask & (1u32 << bit) != 0)
        .map(|bit| {
            let letter = (b'A' + bit as u8) as char;
            PathBuf::from(format!("{letter}:\\"))
        })
        .collect()
}

/// The nearest folder that exists, so a path like `X:\ReCast` opens on `X:\`
/// instead of falling back to AppData on C:.
fn existing_folder(path: &std::path::Path) -> PathBuf {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(path).to_path_buf()
    };
    loop {
        if current.is_dir() {
            return current;
        }
        match current.parent() {
            Some(parent) if parent != current.as_path() => current = parent.to_path_buf(),
            _ => break,
        }
    }
    PathBuf::from(r"C:\")
}

impl Browser {
    fn new(path: PathBuf) -> Self {
        let mut entries = Vec::new();
        if let Ok(read) = std::fs::read_dir(&path) {
            for entry in read.flatten() {
                if entry.path().is_dir() {
                    entries.push(entry.path());
                }
            }
        }
        entries.sort();
        Self { path, entries }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 700.0])
            .with_min_inner_size([820.0, 600.0])
            .with_title("Install ReCast")
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Install ReCast",
        options,
        Box::new(|cc| Ok(Box::new(Installer::new(cc)))),
    )
}

impl eframe::App for Installer {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.chrome_ready {
            self.chrome_ready = window::apply_native_chrome(frame);
        }
        if self.hwnd == 0 {
            self.hwnd = window::native_hwnd(frame).unwrap_or(0);
            if self.hwnd != 0 {
                window::enable_move_and_resize(self.hwnd);
            }
        }

        if let Some(receiver) = &self.receiver {
            let mut done = false;
            while let Ok(message) = receiver.try_recv() {
                match message {
                    Message::Progress(text, value) => {
                        self.status = text;
                        self.progress = value;
                    }
                    Message::Done(result) => {
                        match result {
                            Ok(()) => {
                                self.status = "ReCast is installed".to_string();
                                self.progress = 1.0;
                                self.page = Page::Finished;
                            }
                            Err(error) => {
                                self.error = Some(error);
                                self.page = Page::Destination;
                            }
                        }
                        done = true;
                    }
                }
            }
            if done {
                self.receiver = None;
            }
        }

        let background = egui::Color32::from_rgb(10, 10, 11);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(background))
            .show(ctx, |ui| self.draw(ui));

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

impl Installer {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let fg = egui::Color32::from_rgb(235, 235, 238);
        let muted = egui::Color32::from_rgb(140, 140, 150);
        let faint = egui::Color32::from_rgb(95, 95, 104);

        let full = ui.max_rect();
        let header_height = window::TITLE_BAR_HEIGHT as f32;
        // Fallback when the native caption hit-test is not installed yet.
        let header = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), header_height));
        let drag = ui.interact(header, ui.id().with("window-drag"), egui::Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.horizontal(|ui| {
            ui.set_min_height(header_height);
            ui.add_space(24.0);
            if let Some(logo) = &self.logo {
                ui.add(
                    egui::Image::new(logo)
                        .fit_to_exact_size(egui::vec2(32.0, 32.0))
                        .corner_radius(7.0),
                );
            }
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("ReCast")
                    .size(20.0)
                    .strong()
                    .color(fg),
            );
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(format!("Setup {VERSION}"))
                    .size(15.0)
                    .color(faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(ph::X).size(18.0).color(muted))
                            .frame(false)
                            .min_size(egui::vec2(44.0, 36.0)),
                    )
                    .clicked()
                {
                    std::process::exit(0);
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 36,
                        right: 36,
                        top: 22,
                        bottom: 28,
                    })
                    .show(ui, |ui| {
                        match self.page {
                            Page::Welcome => self.draw_welcome(ui, fg, muted, faint),
                            Page::Destination => self.draw_destination(ui, fg, muted, faint),
                            Page::Installing => self.draw_installing(ui, fg, muted),
                            Page::Finished => self.draw_finished(ui, fg, muted),
                        }
                    });
            });
    }

    fn draw_welcome(&mut self, ui: &mut egui::Ui, fg: egui::Color32, muted: egui::Color32, faint: egui::Color32) {
        ui.label(
            egui::RichText::new("Install ReCast")
                .size(30.0)
                .strong()
                .color(fg),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "ReCast turns an Android phone into the camera and microphone of this \
                 computer. Everything runs on your own machine.",
            )
            .size(16.0)
            .color(muted),
        );
        ui.add_space(28.0);

        for (index, (title, text)) in [
            ("The runtime is copied to your computer", "The app, the virtual camera and the phone app."),
            ("The virtual camera is registered", "It appears as “ReCast Camera” in Windows."),
            ("Shortcuts are created", "On the desktop and in the start menu, if you want them."),
        ]
        .iter()
        .enumerate()
        {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let (circle, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().circle_stroke(
                    circle.center(),
                    8.0,
                    egui::Stroke::new(1.0_f32, muted),
                );
                ui.painter().text(
                    circle.center(),
                    egui::Align2::CENTER_CENTER,
                    (index + 1).to_string(),
                    egui::FontId::proportional(13.0),
                    fg,
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(*title).size(16.0).color(fg));
                    ui.label(egui::RichText::new(*text).size(14.0).color(faint));
                });
            });
            ui.add_space(14.0);
        }

        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{}   Next", ph::CARET_RIGHT)).size(16.0),
                    )
                    .min_size(egui::vec2(140.0, 42.0)),
                )
                .clicked()
            {
                self.page = Page::Destination;
            }
        });
    }

    fn draw_destination(
        &mut self,
        ui: &mut egui::Ui,
        fg: egui::Color32,
        muted: egui::Color32,
        faint: egui::Color32,
    ) {
        ui.label(
            egui::RichText::new("Where should ReCast live?")
                .size(28.0)
                .strong()
                .color(fg),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Choose any folder on any drive. The folder is created if it does not exist. \
                 The path below is only a suggestion, and it does not have to stay in AppData.",
            )
            .size(16.0)
            .color(muted),
        );
        ui.add_space(18.0);

        ui.label(egui::RichText::new("Folder").size(14.0).color(faint));
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(22, 22, 26))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(96, 96, 108)))
            .corner_radius(8)
            .inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 8,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.install_dir)
                        .desired_width(ui.available_width())
                        .hint_text("X:\\ReCast")
                        .font(egui::FontId::proportional(18.0))
                        .text_color(fg)
                        .frame(false)
                        .min_size(egui::vec2(0.0, 28.0)),
                );
            });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{}   Browse", ph::FOLDER)).size(16.0),
                    )
                    .min_size(egui::vec2(150.0, 42.0)),
                )
                .clicked()
            {
                let typed = PathBuf::from(self.install_dir.trim().trim_matches('"'));
                if let Some(path) = window::pick_folder(self.hwnd, &existing_folder(&typed)) {
                    self.install_dir = path.to_string_lossy().to_string();
                    self.error = None;
                }
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Folder list").size(16.0))
                        .min_size(egui::vec2(140.0, 42.0)),
                )
                .clicked()
            {
                self.open_browser();
            }
        });
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("DRIVES")
                .size(13.0)
                .strong()
                .color(faint),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for drive in drive_roots() {
                let letter = drive.to_string_lossy().to_string();
                let short = letter.trim_end_matches('\\').to_string();
                let chosen = self
                    .install_dir
                    .trim()
                    .to_ascii_uppercase()
                    .starts_with(&short.to_ascii_uppercase());
                let text_color = if chosen {
                    egui::Color32::from_rgb(10, 10, 11)
                } else {
                    fg
                };
                let mut button = egui::Button::new(
                    egui::RichText::new(&short).size(16.0).color(text_color),
                )
                .min_size(egui::vec2(72.0, 40.0));
                if chosen {
                    button = button.fill(egui::Color32::from_rgb(235, 235, 238));
                }
                let response = ui.add(button);
                if response.clicked() {
                    self.install_dir = drive.join("ReCast").to_string_lossy().to_string();
                    self.error = None;
                    if self.browser.is_some() {
                        self.browser = Some(Browser::new(drive));
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Every drive Windows can see is listed, including X:. \
                 Click one to start at that drive, or type a full path such as X:\\ReCast.",
            )
            .size(14.0)
            .color(faint),
        );

        ui.add_space(18.0);
        ui.label(
            egui::RichText::new("SHORTCUTS AND START")
                .size(13.0)
                .strong()
                .color(faint),
        );
        ui.add_space(6.0);

        check_row(
            ui,
            ph::DESKTOP,
            "Create a desktop shortcut",
            &mut self.desktop_icon,
            fg,
            muted,
        );
        check_row(
            ui,
            ph::LIST_BULLETS,
            "Add ReCast to the start menu",
            &mut self.start_menu,
            fg,
            muted,
        );
        check_row(
            ui,
            ph::POWER,
            "Start ReCast when Windows starts",
            &mut self.autostart,
            fg,
            muted,
        );

        if let Some(error) = &self.error {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(format!("{}   {error}", ph::WARNING))
                    .size(15.0)
                    .color(egui::Color32::from_rgb(235, 125, 125)),
            );
        }

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{}   Back", ph::CARET_LEFT)).size(16.0),
                    )
                    .min_size(egui::vec2(110.0, 42.0)),
                )
                .clicked()
            {
                self.page = Page::Welcome;
            }
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{}   Install", ph::DOWNLOAD_SIMPLE)).size(16.0),
                    )
                    .min_size(egui::vec2(150.0, 42.0)),
                )
                .clicked()
            {
                self.start_installation();
            }
        });

        self.draw_browser(ui, fg, muted);
    }

    fn draw_installing(&mut self, ui: &mut egui::Ui, fg: egui::Color32, muted: egui::Color32) {
        ui.label(
            egui::RichText::new("Installing ReCast")
                .size(28.0)
                .strong()
                .color(fg),
        );
        ui.add_space(8.0);
        ui.label(egui::RichText::new(&self.status).size(16.0).color(muted));
        ui.add_space(18.0);
        self.draw_progress_bar(ui, self.progress, fg);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(format!("{:.0}%", self.progress * 100.0))
                .monospace()
                .size(15.0)
                .color(muted),
        );
    }

    /// A bar of vertical strokes that fill up as the installation runs.
    fn draw_progress_bar(&self, ui: &mut egui::Ui, progress: f32, fg: egui::Color32) {
        let width = ui.available_width();
        let height = 46.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let bars = 28;
        let gap = 4.0;
        let bar_width = ((rect.width() - gap * (bars as f32 - 1.0)) / bars as f32).max(2.0);
        let filled = (progress * bars as f32).round() as usize;

        for index in 0..bars {
            let x = rect.left() + index as f32 * (bar_width + gap);
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(bar_width, rect.height()),
            );
            let color = if index < filled {
                fg
            } else {
                egui::Color32::from_rgb(28, 28, 32)
            };
            ui.painter().rect_filled(cell, 2.0, color);
        }
    }

    fn draw_finished(&mut self, ui: &mut egui::Ui, fg: egui::Color32, muted: egui::Color32) {
        ui.label(
            egui::RichText::new("ReCast is installed")
                .size(28.0)
                .strong()
                .color(fg),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Choose what should happen now.")
                .size(16.0)
                .color(muted),
        );
        ui.add_space(18.0);

        check_row(
            ui,
            ph::PLAY,
            "Start ReCast now",
            &mut self.launch_after,
            fg,
            muted,
        );
        check_row(
            ui,
            ph::TRASH,
            "Remove the installer and its files",
            &mut self.cleanup_after,
            fg,
            muted,
        );

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "The cleanup keeps only the installed program in the folder you chose. \
                 The camera stays registered for your user.",
            )
            .size(15.0)
            .color(muted),
        );

        ui.add_space(20.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{}   Finish", ph::CHECK)).size(16.0),
                )
                .min_size(egui::vec2(150.0, 42.0)),
            )
            .clicked()
        {
            self.finish();
        }
    }

    /// A small folder picker.
    fn draw_browser(&mut self, ui: &mut egui::Ui, fg: egui::Color32, muted: egui::Color32) {
        let Some(browser) = &self.browser else { return };
        let path = browser.path.clone();
        let entries = browser.entries.clone();

        egui::Window::new("Choose the folder")
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(640.0, 460.0))
            .min_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ui.ctx(), |ui| {
                ui.label(
                    egui::RichText::new(path.to_string_lossy().to_string())
                        .size(16.0)
                        .color(muted),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    for drive in drive_roots() {
                        let letter = drive.to_string_lossy().to_string();
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(letter.trim_end_matches('\\')).size(14.0),
                                )
                                .min_size(egui::vec2(48.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.browser = Some(Browser::new(drive));
                        }
                    }
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{}   ..", ph::ARROW_UP)).size(15.0),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            if let Some(parent) = parent {
                                self.browser = Some(Browser::new(parent.to_path_buf()));
                            }
                        }
                        for entry in &entries {
                            let name = entry
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(format!(
                                            "{}   {name}",
                                            ph::FOLDER
                                        ))
                                        .size(15.0)
                                        .color(fg),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                let next = Browser::new(entry.clone());
                                self.browser = Some(next);
                            }
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Use this folder").size(16.0))
                                .min_size(egui::vec2(160.0, 40.0)),
                        )
                        .clicked()
                    {
                        self.install_dir = path.to_string_lossy().to_string();
                        self.browser = None;
                    }
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Cancel").size(16.0))
                                .min_size(egui::vec2(100.0, 40.0)),
                        )
                        .clicked()
                    {
                        self.browser = None;
                    }
                });
            });
    }
}

/// A row with an icon, a label and a checkbox.
fn check_row(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    value: &mut bool,
    fg: egui::Color32,
    muted: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(18.0).color(muted));
        ui.label(egui::RichText::new(label).size(16.0).color(fg));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
            let box_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
            if *value {
                ui.painter().rect_filled(box_rect, 4.0, fg);
                ui.painter().text(
                    box_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    ph::CHECK,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(10, 10, 11),
                );
            } else {
                ui.painter().rect_stroke(
                    box_rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(70, 70, 78)),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                *value = !*value;
            }
            response.on_hover_cursor(egui::CursorIcon::PointingHand);
        });
    });
    ui.add_space(6.0);
}