use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, vec2};

use crate::theme::{self, ACCENT, BG};
use crate::tracker::{Icon, Tracker};
use crate::chrome;

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
const WIDGET_TITLE: &str = "Navi · Coach";
/// Largura de referência do widget (as fontes escalam em relação a ela).
const WIDGET_W: f32 = 340.0;
const COMPACT_H: f32 = 190.0;
const LIST_H: f32 = 400.0;
const MIN_W: f32 = 230.0;
const MIN_H: f32 = 150.0;

pub struct CoachView {
    range: Range,
    widget: bool,
    widget_list: bool,
    /// Altura atual da janela do widget (o redimensionamento só é pedido quando ela muda).
    widget_h: f32,
    /// Largura atual da janela do widget (o usuário pode redimensionar).
    widget_w: f32,
    /// O widget está se fechando (esmaecendo).
    closing: bool,
    /// O widget acabou de abrir e deve surgir esmaecendo desde o zero.
    widget_fresh: bool,
    /// A moldura quadrada do Windows já foi removida da janela do widget.
    frame_fixed: bool,
    /// Zoom da interface quando a janela do widget foi dimensionada pela última vez.
    widget_zoom: f32,
    /// O widget acabou de ser aberto (o app principal vai para a bandeja).
    just_opened: bool,
    /// Duplo clique no widget: abrir o app principal.
    open_app: bool,
    /// Texturas dos ícones, por caminho do executável.
    icons: HashMap<String, egui::TextureHandle>,
    /// Cor de fundo escolhida para cada programa (pelo nome exibido).
    colors: AppColors,
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

/// Faixas de tempo, da maior para a menor, como fração do programa mais usado: (mínimo, cor).
const BANDS: [(f32, Color32); 4] = [
    (0.75, Color32::from_rgb(232, 92, 92)),
    (0.50, Color32::from_rgb(240, 160, 66)),
    (0.25, Color32::from_rgb(88, 190, 130)),
    (0.0, Color32::from_rgb(96, 140, 236)),
];

fn band_color(frac: f32) -> Color32 {
    BANDS.iter().find(|(min, _)| frac >= *min).map_or(BANDS[3].1, |b| b.1)
}

/// Legenda das cores das barras, com os tempos de cada faixa (relativos ao programa mais usado).
fn legend(ui: &mut egui::Ui, max: u64) {
    let at = |f: f32| fmt((max as f32 * f) as u64);
    let labels = [
        format!("mais de {}", at(0.75)),
        format!("{} a {}", at(0.5), at(0.75)),
        format!("{} a {}", at(0.25), at(0.5)),
        format!("até {}", at(0.25)),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for ((_, color), label) in BANDS.iter().zip(labels) {
            let (sq, _) = ui.allocate_exact_size(vec2(11.0, 11.0), Sense::hover());
            ui.painter().rect_filled(sq, 3.0, *color);
            ui.weak(label);
            ui.add_space(8.0);
        }
    });
}

const PRESETS: [[u8; 3]; 8] =
    [[220, 70, 70], [235, 140, 60], [225, 190, 60], [90, 190, 120], [70, 170, 200], [90, 120, 230], [160, 100, 220], [210, 100, 170]];

type AppColors = BTreeMap<String, [u8; 3]>;

fn colors_path() -> std::path::PathBuf {
    crate::model::data_dir().join("app_colors.json")
}

fn load_colors() -> AppColors {
    std::fs::read_to_string(colors_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_colors(colors: &AppColors) {
    let _ = std::fs::create_dir_all(crate::model::data_dir());
    if let Ok(json) = serde_json::to_string(colors) {
        let _ = std::fs::write(colors_path(), json);
    }
}

/// Conteúdo do menu de botão direito: escolher (ou tirar) a cor de fundo de um programa.
fn color_menu(ui: &mut egui::Ui, name: &str, colors: &mut AppColors, changed: &mut bool) {
    ui.label(egui::RichText::new(format!("Cor de fundo: {name}")).strong());
    ui.horizontal(|ui| {
        for c in PRESETS {
            let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            ui.painter().circle_filled(r.center(), 8.0, Color32::from_rgb(c[0], c[1], c[2]));
            if resp.hovered() {
                ui.painter().circle_stroke(r.center(), 9.5, Stroke::new(1.5, Color32::WHITE));
            }
            if resp.clicked() {
                colors.insert(name.to_owned(), c);
                *changed = true;
                ui.close();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Outra cor:");
        let mut rgb = colors.get(name).copied().unwrap_or([110, 142, 251]);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            colors.insert(name.to_owned(), rgb);
            *changed = true;
        }
    });
    if colors.contains_key(name) && ui.button("Sem cor").clicked() {
        colors.remove(name);
        *changed = true;
        ui.close();
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

/// Relógio `HH:MM:SS`.
pub fn clock(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

impl CoachView {
    pub fn new() -> Self {
        Self {
            range: Range::Hoje,
            widget: false,
            widget_list: false,
            widget_h: 0.0,
            widget_w: WIDGET_W,
            closing: false,
            widget_fresh: false,
            frame_fixed: false,
            widget_zoom: 1.0,
            just_opened: false,
            open_app: false,
            icons: HashMap::new(),
            colors: load_colors(),
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
                    if fairy_button(ui, self.widget && !self.closing) {
                        if self.widget && !self.closing {
                            self.closing = true;
                        } else if !self.widget {
                            self.widget = true;
                            self.widget_fresh = true;
                            self.just_opened = true;
                        }
                    }
                });
            });
            ui.add_space(6.0);
            self.status_banner(ui, &report.now);
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
            if !report.apps.is_empty() {
                legend(ui, max);
                ui.add_space(4.0);
            }
            let mut changed = false;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for app in &report.apps {
                    let tex = icon_tex(&mut self.icons, ui.ctx(), app.icon.as_ref());
                    // Fundo na cor escolhida para o programa (botão direito para mudar); a barra mostra o tempo.
                    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                    if let Some([r, g, b]) = self.colors.get(&app.name) {
                        ui.painter().rect_filled(row, 6.0, Color32::from_rgb(*r, *g, *b).gamma_multiply(0.5));
                    }
                    let bar = band_color(app.secs as f32 / max as f32);
                    let layout = egui::Layout::left_to_right(egui::Align::Center);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(row.shrink2(vec2(4.0, 1.0))).layout(layout), |ui| {
                        app_icon(ui, tex);
                        ui.add(
                            egui::ProgressBar::new(app.secs as f32 / max as f32)
                                .fill(bar.gamma_multiply(0.8))
                                .desired_width(ui.available_width())
                                .text(format!("{}   {}", app.name, fmt(app.secs))),
                        );
                    });
                    resp.on_hover_text("Botão direito: cor de fundo").context_menu(|ui| color_menu(ui, &app.name, &mut self.colors, &mut changed));
                }
            });
            if changed {
                save_colors(&self.colors);
            }
        });
    }

    /// Faixa no topo do Coach: mostra que o rastreador está rodando (mesmo com o widget fechado) ou pausado.
    fn status_banner(&mut self, ui: &mut egui::Ui, now: &crate::tracker::Now) {
        let tracking = !now.idle && now.current.is_some();
        let tint = if tracking { Color32::from_rgb(80, 200, 120) } else { Color32::from_rgb(230, 170, 60) };
        let t = ui.input(|i| i.time) as f32;
        if tracking {
            // Pulso suave, em ~10 quadros por segundo, só enquanto esta aba está aberta.
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        egui::Frame::new()
            .fill(tint.gamma_multiply(0.12))
            .stroke(Stroke::new(1.0, tint.gamma_multiply(0.45)))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    let pulse = if tracking { 0.5 + 0.5 * (t * 3.0).sin() } else { 0.0 };
                    ui.painter().circle_filled(dot.center(), 5.0 + 3.5 * pulse, tint.gamma_multiply(0.35 * (1.0 - pulse)));
                    ui.painter().circle_filled(dot.center(), 4.5, tint);
                    ui.label(egui::RichText::new(if tracking { "Rastreando" } else { "Pausado" }).strong().color(tint));
                    if tracking {
                        let tex = icon_tex(&mut self.icons, ui.ctx(), now.icon.as_ref());
                        app_icon(ui, tex);
                        ui.label(now.current.as_deref().unwrap_or(""));
                    } else {
                        ui.weak("sem atividade; o tempo não está sendo contado");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let widget_on = self.widget && !self.closing;
                        ui.weak(if widget_on { "widget aberto" } else { "widget fechado: o rastreamento continua em segundo plano" });
                    });
                });
            });
    }

    /// Janela flutuante; deve ser chamada a cada quadro, qualquer que seja a aba.
    pub fn floating(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        if !self.widget {
            self.widget_h = 0.0;
            self.frame_fixed = false;
            return;
        }
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_secs(1));
        // Abre e fecha com o mesmo esmaecer suave do resto do app.
        let fade_id = ui.scope_id().with("widget_fade");
        if std::mem::take(&mut self.widget_fresh) {
            ctx.animate_value_with_time(fade_id, 0.0, 0.0);
        }
        let fade = ctx.animate_value_with_time(fade_id, if self.closing { 0.0 } else { 1.0 }, theme::FADE_SECS);
        if self.closing && fade <= 0.01 {
            self.widget = false;
            self.closing = false;
            return;
        }

        let data = tracker.widget();
        let name = if data.now.idle { "Ocioso".to_owned() } else { data.now.current.clone().unwrap_or_else(|| "–".to_owned()) };
        let cur_tex = icon_tex(&mut self.icons, &ctx, data.now.icon.as_ref());
        let rows_tex: Vec<Option<egui::TextureId>> =
            data.session.iter().map(|r| icon_tex(&mut self.icons, &ctx, r.icon.as_ref())).collect();
        let height = if self.widget_list { LIST_H } else { COMPACT_H };

        let id = egui::ViewportId::from_hash_of(WIDGET_ID);
        // Só pede novo tamanho quando a lista abre/fecha ou a escala muda: um comando a cada quadro
        // mantém a janela repintando sem parar, e o tamanho que o usuário escolheu arrastando não pode ser desfeito.
        let zoom = ctx.zoom_factor();
        let first = self.widget_h == 0.0;
        if (!first && (self.widget_h - height).abs() > f32::EPSILON) || (zoom - self.widget_zoom).abs() > 1e-3 {
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(vec2(self.widget_w, height)));
        }
        self.widget_h = height;
        // No primeiro quadro a janela ainda não existe; o comando só vale a partir do próximo.
        if !first {
            self.widget_zoom = zoom;
        }

        let builder = egui::ViewportBuilder::default()
            .with_title(WIDGET_TITLE)
            .with_inner_size([self.widget_w, height])
            .with_min_inner_size([MIN_W, MIN_H])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(true);
        let mut close = false;
        let mut toggle_list = false;
        let mut open_app = false;
        let mut size = None;
        let open = self.widget_list;
        // Sem nada sendo rastreado (ocioso ou sem programa), o widget fica vermelho.
        let tracking = !data.now.idle && data.now.current.is_some();
        let tracked_name = if tracking { data.now.current.clone() } else { None };
        // Cor de fundo escolhida para o programa em uso (botão direito no widget ou no Coach).
        let program = tracked_name.as_deref().and_then(|n| self.colors.get(n)).copied();
        let mut colors = std::mem::take(&mut self.colors);
        let mut colors_changed = false;
        let (fill, time_color) = if tracking {
            (program.map_or(BG, |[r, g, b]| BG.lerp_to_gamma(Color32::from_rgb(r, g, b), 0.5)), ACCENT)
        } else {
            (Color32::from_rgb(58, 20, 24), Color32::from_rgb(255, 120, 120))
        };
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            ui.set_opacity(fade);
            let full = ui.max_rect();
            size = Some(full.size());

            let drag = ui.interact(full, ui.scope_id().with("widget_drag"), Sense::click_and_drag());
            // Com o mouse em cima, o widget clareia um pouco, como se uma luz o iluminasse.
            let glow = ui.ctx().animate_bool_with_time(ui.scope_id().with("widget_glow"), drag.contains_pointer(), 0.2);
            let lit = fill.lerp_to_gamma(Color32::from_rgb(170, 190, 255), 0.13 * glow);
            ui.painter().rect_filled(full, chrome::RADIUS, lit);
            let edge = if tracking {
                Stroke::new(1.0, Color32::from_white_alpha(22 + (60.0 * glow) as u8))
            } else {
                Stroke::new(1.5, Color32::from_rgb(214, 70, 70).lerp_to_gamma(Color32::from_rgb(255, 130, 130), glow))
            };
            ui.painter().rect_stroke(full, chrome::RADIUS, edge, egui::StrokeKind::Inside);
            chrome::resize_handles(ui, full);

            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                open_app = true;
            }
            drag.context_menu(|ui| match &tracked_name {
                Some(n) => color_menu(ui, n, &mut colors, &mut colors_changed),
                None => {
                    ui.weak("Nada sendo rastreado agora.");
                }
            });
            // A fonte do relógio acompanha a largura da janela.
            let k = (full.width() / WIDGET_W).clamp(0.7, 2.4);
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if cur_tex.is_some() {
                        app_icon(ui, cur_tex);
                    }
                    ui.add(egui::Label::new(egui::RichText::new(&name).strong()).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if chrome::close_button(ui).clicked() {
                            close = true;
                        }
                    });
                });
                ui.vertical_centered(|ui| {
                    ui.add_space(2.0 * k);
                    ui.label(egui::RichText::new(clock(data.app_secs)).monospace().strong().size(34.0 * k).color(time_color));
                    ui.weak(format!("Hoje, no total: {}", clock(data.total)));
                    ui.add_space(2.0);
                    let label = format!("{} Programas desta sessão ({})", if open { "–" } else { "+" }, data.session.len());
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        toggle_list = true;
                    }
                });
                if open {
                    egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                        if data.session.is_empty() {
                            ui.weak("Nenhum programa registrado ainda.");
                        }
                        for (row, tex) in data.session.iter().zip(&rows_tex) {
                            ui.horizontal(|ui| {
                                app_icon(ui, *tex);
                                ui.add(egui::Label::new(egui::RichText::new(&row.name).strong()).truncate());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.weak(clock(row.secs));
                                });
                            });
                        }
                    });
                }
            });
        });
        self.colors = colors;
        if colors_changed {
            save_colors(&self.colors);
        }
        if let Some(s) = size {
            self.widget_w = s.x.max(MIN_W);
        }
        // A janela só existe a partir do primeiro quadro; então tira a moldura quadrada do Windows.
        if !first && !self.frame_fixed {
            self.frame_fixed = chrome::flatten_window(WIDGET_TITLE);
        }
        if toggle_list {
            self.widget_list = !self.widget_list;
        }
        if open_app {
            self.open_app = true;
            self.closing = true;
        }
        // Fechar o widget também abre o app.
        if close {
            self.open_app = true;
            self.closing = true;
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
    let flap = (time * 13.0).sin() * amp;
    if amp > 0.0 {
        ui.ctx().request_repaint();
    }
    let glow = if active { 1.0 } else { amp };
    // Em hover ela voa em oito, subindo e descendo, enquanto bate as asas.
    let sway = vec2((time * 5.0).sin() * 5.0, (time * 10.0).sin() * 4.0) * amp;
    crate::fairy::draw(ui.painter(), rect.center() + vec2(0.0, 2.0) + sway, 40.0 + 6.0 * amp, flap, glow);
    resp.on_hover_text(if active { "Fechar widget" } else { "Abrir widget" }).clicked()
}
