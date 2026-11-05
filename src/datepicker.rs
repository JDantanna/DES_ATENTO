//! Calendário em popup para escolher uma data, aberto ao clicar na caixa da data.

use chrono::{Datelike, Duration, Months, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, vec2};

use crate::splash::MESES;
use crate::theme::ACCENT;

const LETRAS: [&str; 7] = ["S", "T", "Q", "Q", "S", "S", "D"];
const CELL: egui::Vec2 = vec2(32.0, 28.0);

pub enum Outcome {
    /// Nada mudou; o popup continua aberto.
    Open,
    Pick(NaiveDate),
    Clear,
    Close,
}

/// Desenha o calendário abaixo de `anchor` (a caixa da data). `month` é o primeiro dia do mês mostrado.
pub fn show(ctx: &egui::Context, id: egui::Id, anchor: Rect, month: &mut NaiveDate, selected: Option<NaiveDate>, today: NaiveDate) -> Outcome {
    let mut outcome = Outcome::Open;
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(anchor.left_bottom() + vec2(0.0, 4.0))
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                ui.horizontal(|ui| {
                    if ui.button("‹").on_hover_text("Mês anterior").clicked() {
                        *month = month.checked_sub_months(Months::new(1)).unwrap_or(*month);
                    }
                    let title = format!("{} de {}", MESES[month.month0() as usize], month.year());
                    let (rect, _) = ui.allocate_exact_size(vec2(CELL.x * 7.0 - 64.0, CELL.y), Sense::hover());
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, title, FontId::proportional(13.5), ui.visuals().strong_text_color());
                    if ui.button("›").on_hover_text("Próximo mês").clicked() {
                        *month = month.checked_add_months(Months::new(1)).unwrap_or(*month);
                    }
                });
                ui.horizontal(|ui| {
                    for letra in LETRAS {
                        let (rect, _) = ui.allocate_exact_size(vec2(CELL.x, 20.0), Sense::hover());
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, letra, FontId::proportional(11.5), Color32::from_gray(150));
                    }
                });
                let first = *month - Duration::days(month.weekday().num_days_from_monday() as i64);
                for row in 0..6 {
                    ui.horizontal(|ui| {
                        for col in 0..7 {
                            let d = first + Duration::days(row * 7 + col);
                            if day_cell(ui, d, d.month() == month.month(), selected == Some(d), d == today) {
                                outcome = Outcome::Pick(d);
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Hoje").clicked() {
                        outcome = Outcome::Pick(today);
                    }
                    if ui.button("Limpar").clicked() {
                        outcome = Outcome::Clear;
                    }
                });
            });
        });
    // Fecha com Esc ou clicando fora (clicar na própria caixa mantém aberto, para seguir digitando).
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p))
    });
    if pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        outcome = Outcome::Close;
    }
    outcome
}

fn day_cell(ui: &mut egui::Ui, d: NaiveDate, in_month: bool, selected: bool, today: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(CELL, Sense::click());
    let cell = rect.shrink(1.0);
    if selected {
        ui.painter().rect_filled(cell, 6.0, ACCENT);
    } else if resp.hovered() {
        ui.painter().rect_filled(cell, 6.0, Color32::from_white_alpha(22));
    }
    if today && !selected {
        ui.painter().rect_stroke(cell, 6.0, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
    }
    let color = match (selected, in_month) {
        (true, _) => Color32::from_rgb(16, 18, 27),
        (false, true) => Color32::from_gray(225),
        (false, false) => Color32::from_gray(110),
    };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, d.day().to_string(), FontId::proportional(13.0), color);
    resp.clicked()
}
