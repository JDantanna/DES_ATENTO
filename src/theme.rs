use eframe::egui::{self, Color32, CornerRadius, Theme, ThemePreference, vec2};

pub const ACCENT: Color32 = Color32::from_rgb(120, 190, 255);
pub const BG: Color32 = Color32::from_rgb(16, 18, 27);
pub const CARD: Color32 = Color32::from_rgb(23, 26, 38);

pub fn apply(ctx: &egui::Context) {
    // Sempre escuro, independente do tema do Windows.
    ctx.set_theme(ThemePreference::Dark);
    ctx.style_mut_of(Theme::Dark, |s| {
        let v = &mut s.visuals;
        // Os painéis ficam transparentes: o fundo arredondado é desenhado pela janela.
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = Color32::from_rgb(26, 29, 42);
        v.extreme_bg_color = Color32::from_rgb(12, 14, 21);
        v.faint_bg_color = CARD;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke.color = Color32::WHITE;
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        let r = CornerRadius::same(8);
        v.widgets.noninteractive.corner_radius = r;
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.corner_radius = r;
        v.widgets.active.corner_radius = r;
        v.widgets.open.corner_radius = r;
        v.widgets.inactive.bg_fill = Color32::from_rgb(34, 38, 54);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 34, 48);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 48, 68);
        v.widgets.noninteractive.bg_stroke.color = Color32::from_white_alpha(14);
        // Sem crescer no hover: os botões ficam parados dentro do próprio quadro.
        let stroke_w = v.widgets.inactive.bg_stroke.width;
        for w in [&mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.expansion = 0.0;
            w.bg_stroke.width = stroke_w;
        }
        s.spacing.item_spacing = vec2(8.0, 7.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
    });
}
