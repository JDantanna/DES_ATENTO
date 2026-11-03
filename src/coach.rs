use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId, Sense, vec2};

use crate::theme::{ACCENT, BG};
use crate::tracker::Tracker;

#[derive(PartialEq, Clone, Copy)]
enum Range {
    Hoje,
    Semana,
    Mes,
}

impl Range {
    fn days(self) -> i64 {
        match self {
            Range::Hoje => 1,
            Range::Semana => 7,
            Range::Mes => 30,
        }
    }
}

pub struct CoachView {
    range: Range,
    widget: bool,
    widget_list: bool,
}

/// Formato curto, para relatório e gráficos: `1h 05m`.
pub fn fmt(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs % 3600 / 60);
    match (h, m) {
        (0, 0) if secs == 0 => "0m".to_owned(),
        (0, 0) => "<1m".to_owned(),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m:02}m"),
    }
}

/// Tempo por extenso: `1 hora, 23 minutos e 5 segundos`.
pub fn fmt_long(secs: u64, with_secs: bool) -> String {
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let unit = |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(unit(h, "hora", "horas"));
    }
    if m > 0 {
        parts.push(unit(m, "minuto", "minutos"));
    }
    if (s > 0 && with_secs) || parts.is_empty() {
        parts.push(unit(s, "segundo", "segundos"));
    }
    match parts.len() {
        1 => parts.remove(0),
        _ => {
            let last = parts.pop().unwrap();
            format!("{} e {last}", parts.join(", "))
        }
    }
}

fn key(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl CoachView {
    pub fn new() -> Self {
        Self { range: Range::Hoje, widget: false, widget_list: false }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        let snap = tracker.snapshot();
        let (usage, current, idle) = (snap.usage, snap.current, snap.idle);
        let today = Local::now().date_naive();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.range, Range::Hoje, "Hoje");
                ui.selectable_value(&mut self.range, Range::Semana, "7 dias");
                ui.selectable_value(&mut self.range, Range::Mes, "30 dias");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.widget, "Widget flutuante");
                });
            });
            ui.add_space(6.0);
            ui.weak(if idle {
                "Ocioso (sem tempo contado)".to_owned()
            } else {
                format!("Agora: {}", current.as_deref().unwrap_or("—"))
            });
            ui.add_space(6.0);

            let n = self.range.days();
            let mut per_app: BTreeMap<String, u64> = BTreeMap::new();
            let mut per_day: Vec<(chrono::NaiveDate, u64)> = Vec::new();
            for i in (0..n).rev() {
                let d = today - chrono::Duration::days(i);
                let mut total = 0;
                if let Some(apps) = usage.days.get(&key(d)) {
                    for (a, s) in apps {
                        *per_app.entry(a.clone()).or_default() += s;
                        total += s;
                    }
                }
                per_day.push((d, total));
            }
            let total: u64 = per_day.iter().map(|(_, s)| s).sum();
            ui.label(egui::RichText::new(format!("Total: {}", fmt(total))).heading());
            ui.add_space(8.0);

            if n > 1 {
                chart(ui, &per_day);
                ui.add_space(10.0);
            }

            let mut apps: Vec<(String, u64)> = per_app.into_iter().collect();
            apps.sort_by(|a, b| b.1.cmp(&a.1));
            if apps.is_empty() {
                ui.weak("Ainda sem dados. Use outros programas e volte aqui.");
            }
            let max = apps.first().map(|a| a.1).unwrap_or(1).max(1);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (name, secs) in apps {
                    ui.add(
                        egui::ProgressBar::new(secs as f32 / max as f32)
                            .fill(ACCENT.gamma_multiply(0.6))
                            .desired_width(ui.available_width())
                            .text(format!("{name}   {}", fmt(secs))),
                    );
                }
            });
        });
    }

    /// Janela flutuante; deve ser chamada a cada quadro, qualquer que seja a aba.
    pub fn floating(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        if !self.widget {
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        let snap = tracker.snapshot();
        let today = snap.usage.days.get(&key(Local::now().date_naive()));
        let app_secs = snap.current.as_deref().and_then(|c| today.and_then(|t| t.get(c))).copied().unwrap_or(0);
        let total: u64 = today.map(|t| t.values().sum()).unwrap_or(0);
        let name = if snap.idle { "Ocioso".to_owned() } else { snap.current.clone().unwrap_or_else(|| "—".to_owned()) };

        let mut session: Vec<(&String, &u64)> = snap.session.iter().collect();
        session.sort_by(|a, b| b.1.cmp(a.1));
        let rows = session.len().min(7) as f32;
        let height = if self.widget_list { 176.0 + rows * 26.0 + 8.0 } else { 150.0 };

        let builder = egui::ViewportBuilder::default()
            .with_title("Navi · Coach")
            .with_inner_size([340.0, height])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false);
        let mut close = false;
        let mut toggle_list = false;
        let open = self.widget_list;
        ui.ctx().show_viewport_immediate(egui::ViewportId::from_hash_of("coach_widget"), builder, |ui, _class| {
            let full = ui.max_rect();
            ui.painter().rect_filled(full, 14.0, BG);
            ui.painter().rect_stroke(full, 14.0, egui::Stroke::new(1.0, Color32::from_white_alpha(22)), egui::StrokeKind::Inside);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(340.0, height)));

            let drag = ui.interact(full, ui.scope_id().with("widget_drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&name).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::chrome::close_button(ui).clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(egui::RichText::new(fmt_long(app_secs, true)).size(19.0).color(ACCENT));
                ui.weak(format!("Hoje, no total: {}", fmt_long(total, false)));
                ui.add_space(2.0);
                let label = format!("{} Programas desta sessão ({})", if open { "−" } else { "+" }, session.len());
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    toggle_list = true;
                }
                if open {
                    egui::ScrollArea::vertical().max_height(7.0 * 26.0).show(ui, |ui| {
                        if session.is_empty() {
                            ui.weak("Nenhum programa registrado ainda.");
                        }
                        for (app, secs) in &session {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(app.as_str()).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.weak(fmt_long(**secs, false));
                                });
                            });
                        }
                    });
                }
            });
        });
        if toggle_list {
            self.widget_list = !self.widget_list;
        }
        if close {
            self.widget = false;
        }
    }
}

fn chart(ui: &mut egui::Ui, days: &[(chrono::NaiveDate, u64)]) {
    let h = 110.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let p = ui.painter_at(rect);
    let max = days.iter().map(|d| d.1).max().unwrap_or(0).max(1);
    let slot = rect.width() / days.len() as f32;
    let plot_h = h - 28.0;
    for (i, (d, s)) in days.iter().enumerate() {
        let x = rect.min.x + i as f32 * slot;
        let bh = plot_h * *s as f32 / max as f32;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x + slot * 0.15, rect.min.y + 14.0 + plot_h - bh),
            egui::pos2(x + slot * 0.85, rect.min.y + 14.0 + plot_h),
        );
        let is_today = *d == Local::now().date_naive();
        p.rect_filled(bar, 2.0, if is_today { ACCENT } else { ACCENT.gamma_multiply(0.45) });
        if days.len() <= 8 || i % 5 == 0 || is_today {
            p.text(
                egui::pos2(x + slot / 2.0, rect.max.y - 7.0),
                Align2::CENTER_CENTER,
                format!("{:02}/{:02}", d.day(), d.month()),
                FontId::proportional(10.0),
                Color32::GRAY,
            );
        }
        if *s > 0 && days.len() <= 8 {
            p.text(
                egui::pos2(x + slot / 2.0, bar.min.y - 7.0),
                Align2::CENTER_CENTER,
                fmt(*s),
                FontId::proportional(10.0),
                Color32::LIGHT_GRAY,
            );
        }
    }
}
