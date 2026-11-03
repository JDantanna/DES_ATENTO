//! Moldura própria da janela: fundo arredondado, barra de título, botões de
//! janela e áreas de redimensionamento (a janela do Windows é sem decoração).

use eframe::egui::{self, Color32, CursorIcon, Rect, Response, Sense, Stroke, vec2};
use egui::viewport::ResizeDirection as Dir;

pub const RADIUS: f32 = 14.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

pub fn window_button(ui: &mut egui::Ui, glyph: Glyph) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 28.0), Sense::click());
    let hot = resp.hovered();
    let bg = match (hot, glyph) {
        (true, Glyph::Close) => Some(Color32::from_rgb(200, 60, 70)),
        (true, _) => Some(Color32::from_white_alpha(22)),
        _ => None,
    };
    if let Some(bg) = bg {
        ui.painter().rect_filled(rect.shrink2(vec2(2.0, 0.0)), 7.0, bg);
    }
    let c = rect.center();
    let col = if hot { Color32::WHITE } else { Color32::from_gray(185) };
    let st = Stroke::new(1.4, col);
    let p = ui.painter();
    match glyph {
        Glyph::Minimize => {
            p.line_segment([c + vec2(-5.0, 3.0), c + vec2(5.0, 3.0)], st);
        }
        Glyph::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), 2.0, st, egui::StrokeKind::Middle);
        }
        Glyph::Restore => {
            p.rect_stroke(Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(8.0, 8.0)), 2.0, st, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-1.0, -4.5), c + vec2(5.0, -4.5)], st);
            p.line_segment([c + vec2(5.0, -4.5), c + vec2(5.0, 2.0)], st);
        }
        Glyph::Close => {
            p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], st);
            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], st);
        }
    }
    resp
}

pub fn close_button(ui: &mut egui::Ui) -> Response {
    window_button(ui, Glyph::Close)
}

/// Faixas finas nas bordas e cantos que iniciam o redimensionamento.
pub fn resize_handles(ui: &mut egui::Ui, full: Rect) {
    let (e, c) = (5.0, 12.0);
    let zones = [
        (Rect::from_min_max(full.left_top() + vec2(c, 0.0), full.right_top() + vec2(-c, e)), Dir::North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(full.left_bottom() + vec2(c, -e), full.right_bottom() + vec2(-c, 0.0)), Dir::South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(full.left_top() + vec2(0.0, c), full.left_bottom() + vec2(e, -c)), Dir::West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(full.right_top() + vec2(-e, c), full.right_bottom() + vec2(0.0, -c)), Dir::East, CursorIcon::ResizeHorizontal),
        (Rect::from_min_size(full.left_top(), vec2(c, c)), Dir::NorthWest, CursorIcon::ResizeNwSe),
        (Rect::from_min_size(full.right_top() + vec2(-c, 0.0), vec2(c, c)), Dir::NorthEast, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(full.left_bottom() + vec2(0.0, -c), vec2(c, c)), Dir::SouthWest, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(full.right_bottom() - vec2(c, c), vec2(c, c)), Dir::SouthEast, CursorIcon::ResizeNwSe),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let r = ui.interact(rect, ui.scope_id().with(("resize", i)), Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if r.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
