use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId, Sense, vec2};

use crate::theme::{ACCENT, BG};
use crate::tracker::{Icon, Tracker};

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

const WIDGET_ID: &str = "coach_widget";
const WIDGET_W: f32 = 340.0;

pub struct CoachView {
    range: Range,
    widget: bool,
    widget_list: bool,
    /// Altura atual da janela do widget (o redimensionamento só é pedido quando ela muda).
    widget_h: f32,
    /// Zoom da interface quando a janela do widget foi dimensionada pela última vez.
    widget_zoom: f32,
    /// O widget acabou de ser aberto (o app principal vai para a bandeja).
    just_opened: bool,
    /// Duplo clique no widget: abrir o app principal.
    open_app: bool,
    /// Texturas dos ícones, por caminho do executável.
    icons: HashMap<String, egui::TextureHandle>,
}

const ICON: f32 = 20.0;

/// Textura do ícone do programa, criada na primeira vez que aparece.
fn icon_tex(cache: &mut HashMap<String, egui::TextureHandle>, ctx: &egui::Context, icon: Option<&Arc<Icon>>) -> Option<egui::TextureId> {
    let icon = icon?;
    let tex = cache.entry(icon.key.clone()).or_insert_with(|| {
        let img = egui::ColorImage::from_rgba_unmultiplied([icon.w, icon.h], &icon.rgba);
        ctx.load_texture(format!("app:{}", icon.key), img, egui::TextureOptions::LINEAR)
    });
    Some(tex.id())
}

fn app_icon(ui: &mut egui::Ui, tex: Option<egui::TextureId>) {
    match tex {
        Some(id) => {
            ui.image((id, vec2(ICON, ICON)));
        }
        None => {
            ui.allocate_space(vec2(ICON, ICON));
        }
    }
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

impl CoachView {
    pub fn new() -> Self {
        Self {
            range: Range::Hoje,
            widget: false,
            widget_list: false,
            widget_h: 0.0,
            widget_zoom: 1.0,
            just_opened: false,
            open_app: false,
            icons: HashMap::new(),
        }
    }

    pub fn take_opened(&mut self) -> bool {
        std::mem::take(&mut self.just_opened)
    }

    pub fn take_open_app(&mut self) -> bool {
        std::mem::take(&mut self.open_app)
    }

    pub fn show(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        ui.ctx().request_repaint_after(Duration::from_secs(2));
        let n = self.range.days();
        let report = tracker.report(n);
        let total: u64 = report.per_day.iter().map(|(_, s)| s).sum();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.range, Range::Hoje, "Hoje");
                ui.selectable_value(&mut self.range, Range::Semana, "7 dias");
                ui.selectable_value(&mut self.range, Range::Mes, "30 dias");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if fairy_button(ui, self.widget) {
                        self.widget = !self.widget;
                        self.just_opened = self.widget;
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let now = &report.now;
                if now.idle {
                    ui.weak("Ocioso (sem tempo contado)");
                } else if let Some(cur) = now.current.as_deref() {
                    let tex = icon_tex(&mut self.icons, ui.ctx(), now.icon.as_ref());
                    app_icon(ui, tex);
                    ui.weak(format!("Agora: {cur}"));
                } else {
                    ui.weak("Agora: –");
                }
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("Total: {}", fmt(total))).heading());
            ui.add_space(8.0);

            if n > 1 {
                chart(ui, &report.per_day);
                ui.add_space(10.0);
            }

            if report.apps.is_empty() {
                ui.weak("Ainda sem dados. Use outros programas e volte aqui.");
            }
            let max = report.apps.first().map(|a| a.secs).unwrap_or(1).max(1);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for app in &report.apps {
                    let tex = icon_tex(&mut self.icons, ui.ctx(), app.icon.as_ref());
                    ui.horizontal(|ui| {
                        app_icon(ui, tex);
                        ui.add(
                            egui::ProgressBar::new(app.secs as f32 / max as f32)
                                .fill(ACCENT.gamma_multiply(0.6))
                                .desired_width(ui.available_width())
                                .text(format!("{}   {}", app.name, fmt(app.secs))),
                        );
                    });
                }
            });
        });
    }

    /// Janela flutuante; deve ser chamada a cada quadro, qualquer que seja a aba.
    pub fn floating(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        if !self.widget {
            self.widget_h = 0.0;
            return;
        }
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_secs(1));
        let data = tracker.widget();
        let name = if data.now.idle { "Ocioso".to_owned() } else { data.now.current.clone().unwrap_or_else(|| "–".to_owned()) };
        let cur_tex = icon_tex(&mut self.icons, &ctx, data.now.icon.as_ref());
        let rows_tex: Vec<Option<egui::TextureId>> =
            data.session.iter().map(|r| icon_tex(&mut self.icons, &ctx, r.icon.as_ref())).collect();
        let height = if self.widget_list { 176.0 + data.session.len().min(7) as f32 * 26.0 + 8.0 } else { 150.0 };

        let id = egui::ViewportId::from_hash_of(WIDGET_ID);
        // Só pede novo tamanho quando ele ou a escala muda: um comando a cada quadro mantém a janela repintando sem parar.
        let zoom = ctx.zoom_factor();
        let first = self.widget_h == 0.0;
        if (!first && (self.widget_h - height).abs() > f32::EPSILON) || (zoom - self.widget_zoom).abs() > 1e-3 {
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(vec2(WIDGET_W, height)));
        }
        self.widget_h = height;
        // No primeiro quadro a janela ainda não existe; o comando só vale a partir do próximo.
        if !first {
            self.widget_zoom = zoom;
        }

        let builder = egui::ViewportBuilder::default()
            .with_title("Navi · Coach")
            .with_inner_size([WIDGET_W, height])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false);
        let mut close = false;
        let mut toggle_list = false;
        let mut open_app = false;
        let open = self.widget_list;
        // Sem nada sendo rastreado (ocioso ou sem programa), o widget fica vermelho.
        let tracking = !data.now.idle && data.now.current.is_some();
        let (fill, stroke, time_color) = if tracking {
            (BG, Color32::from_white_alpha(22), ACCENT)
        } else {
            (Color32::from_rgb(58, 20, 24), Color32::from_rgb(214, 70, 70), Color32::from_rgb(255, 120, 120))
        };
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            let full = ui.max_rect();
            ui.painter().rect_filled(full, 14.0, fill);
            ui.painter().rect_stroke(full, 14.0, egui::Stroke::new(if tracking { 1.0 } else { 1.5 }, stroke), egui::StrokeKind::Inside);

            let drag = ui.interact(full, ui.scope_id().with("widget_drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                open_app = true;
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if cur_tex.is_some() {
                        app_icon(ui, cur_tex);
                    }
                    ui.label(egui::RichText::new(&name).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::chrome::close_button(ui).clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(egui::RichText::new(fmt_long(data.app_secs, true)).size(19.0).color(time_color));
                ui.weak(format!("Hoje, no total: {}", fmt_long(data.total, false)));
                ui.add_space(2.0);
                let label = format!("{} Programas desta sessão ({})", if open { "–" } else { "+" }, data.session.len());
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    toggle_list = true;
                }
                if open {
                    egui::ScrollArea::vertical().max_height(7.0 * 26.0).show(ui, |ui| {
                        if data.session.is_empty() {
                            ui.weak("Nenhum programa registrado ainda.");
                        }
                        for (row, tex) in data.session.iter().zip(&rows_tex) {
                            ui.horizontal(|ui| {
                                app_icon(ui, *tex);
                                ui.label(egui::RichText::new(&row.name).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.weak(fmt_long(row.secs, false));
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
        self.open_app |= open_app;
        if close {
            self.widget = false;
        }
    }
}
fn chart(ui: &mut egui::Ui, days: &[(chrono::NaiveDate, u64)]) {
    let today = Local::now().date_naive();
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
        let is_today = *d == today;
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

/// Botão da fadinha: as asas batem quando o mouse passa por cima. Retorna `true` ao clicar.
fn fairy_button(ui: &mut egui::Ui, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 44.0), Sense::click());
    let amp = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.18);
    let time = ui.input(|i| i.time) as f32;
    let flap = (time * 22.0).sin() * amp;
    if amp > 0.0 {
        ui.ctx().request_repaint();
    }
    let glow = if active { 1.0 } else { amp };
    // Em hover ela voa em oito, subindo e descendo, enquanto bate as asas.
    let sway = vec2((time * 5.0).sin() * 5.0, (time * 10.0).sin() * 4.0) * amp;
    crate::fairy::draw(ui.painter(), rect.center() + vec2(0.0, 2.0) + sway, 40.0 + 6.0 * amp, flap, glow);
    resp.on_hover_text(if active { "Fechar widget" } else { "Abrir widget" }).clicked()
}
