use eframe::egui;
use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
    Mono,
    Graphite,
    Light,
    Custom,
}

impl ThemeKind {
    pub fn label(self) -> &'static str {
        match self {
            ThemeKind::Mono => "Mono",
            ThemeKind::Graphite => "Graphite",
            ThemeKind::Light => "Light",
            ThemeKind::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub kind: ThemeKind,
    pub accent: [u8; 3],
    pub background: [u8; 3],
    pub panel: [u8; 3],
    pub light: bool,
    pub roundness: u8,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            kind: ThemeKind::Mono,
            accent: [232, 232, 232],
            background: [8, 8, 8],
            panel: [16, 16, 16],
            light: false,
            roundness: 4,
        }
    }
}

impl ThemeConfig {
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = if self.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };

        let accent = Color32::from_rgb(self.accent[0], self.accent[1], self.accent[2]);
        let bg = Color32::from_rgb(self.background[0], self.background[1], self.background[2]);
        let panel = Color32::from_rgb(self.panel[0], self.panel[1], self.panel[2]);

        // Text on accent-colored widgets must contrast with the accent itself.
        let accent_luma = (0.2126 * self.accent[0] as f32
            + 0.7152 * self.accent[1] as f32
            + 0.0722 * self.accent[2] as f32)
            / 255.0;
        let accent_text = if accent_luma > 0.55 {
            Color32::from_gray(10)
        } else {
            Color32::from_gray(245)
        };
        let text = if self.light {
            Color32::from_gray(25)
        } else {
            Color32::from_gray(225)
        };
        let muted_text = if self.light {
            Color32::from_gray(105)
        } else {
            Color32::from_gray(150)
        };
        let inline_bg = if self.light {
            Color32::from_gray(235)
        } else {
            Color32::from_gray(28)
        };
        let hover_bg = if self.light {
            Color32::from_gray(215)
        } else {
            Color32::from_gray(42)
        };
        let stroke = if self.light {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(60)
        };

        visuals.panel_fill = panel;
        visuals.window_fill = panel;
        visuals.faint_bg_color = bg;
        visuals.extreme_bg_color = if self.light {
            Color32::from_gray(238)
        } else {
            Color32::from_gray(10)
        };
        visuals.code_bg_color = bg;
        visuals.window_stroke = egui::Stroke::new(1.0_f32, stroke);
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent_text);
        visuals.hyperlink_color = accent;

        visuals.widgets.noninteractive.bg_fill = panel;
        visuals.widgets.noninteractive.weak_bg_fill = panel;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, stroke);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, muted_text);

        visuals.widgets.inactive.bg_fill = inline_bg;
        visuals.widgets.inactive.weak_bg_fill = inline_bg;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, stroke);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);

        visuals.widgets.hovered.bg_fill = hover_bg;
        visuals.widgets.hovered.weak_bg_fill = hover_bg;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, muted_text);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, text);

        visuals.widgets.active.bg_fill = accent;
        visuals.widgets.active.weak_bg_fill = accent;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, accent_text);

        visuals.window_corner_radius = egui::CornerRadius::same(self.roundness);
        visuals.menu_corner_radius = egui::CornerRadius::same(self.roundness);

        let mut style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.slider_width = 160.0;
        ctx.set_style(style);
    }
}
