use eframe::egui::{self, Color32};

pub const ACCENT: Color32 = Color32::from_rgb(110, 142, 251);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = ACCENT;
    visuals.hyperlink_color = ACCENT;
    visuals.panel_fill = Color32::from_rgb(20, 22, 30);
    visuals.window_fill = Color32::from_rgb(26, 28, 38);
    ctx.set_visuals(visuals);
}
