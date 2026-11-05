use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, vec2};

use crate::theme::{self, ACCENT, BG};
use crate::tracker::{AppRow, Icon, Tracker};
use crate::chrome::{self, Glyph};

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
/// Altura do formato pequeno (faixa com o programa e o cronômetro).
const STRIP_H: f32 = 56.0;
/// Altura do formato médio, com a lista de programas fechada e aberta.
const COMPACT_H: f32 = 190.0;
const LIST_H: f32 = 400.0;
const MIN_W: f32 = 230.0;
const MIN_H: f32 = 48.0;
/// Nome com que o próprio app aparece no rastreador.
const APP_NAME: &str = "Navi";

pub struct CoachView {
    range: Range,
    widget: bool,
    widget_list: bool,
    /// Altura atual da janela do widget (o redimensionamento só é pedido quando ela muda).
    widget_h: f32,
    /// Largura atual da janela do widget (o usuário pode redimensionar).
    widget_w: f32,
    /// Tamanho com que a janela do widget foi criada.
    widget_start: egui::Vec2,
    /// Formato pequeno (faixa) em vez do médio.
    small: bool,
    /// O formato mudou e precisa ser gravado nas preferências.
    format_changed: bool,
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
    /// Painel do programa aberto com o botão direito.
    panel: Option<Panel>,
    /// Programa cujo ícone será escolhido em seguida, no seletor de arquivo.
    pending_pick: Option<String>,
    /// Erro a mostrar no Coach (imagem de ícone que não pôde ser lida).
    notice: Option<String>,
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

/// Ícone do programa; o próprio Navi aparece só como a fada, sem o fundo do ícone do executável.
fn program_icon(ui: &mut egui::Ui, name: &str, tex: Option<egui::TextureId>) {
    if name == APP_NAME {
        let (rect, _) = ui.allocate_exact_size(vec2(ICON, ICON), Sense::hover());
        crate::fairy::draw(ui.painter(), rect.center(), ICON * 1.3, 0.0, 0.3);
    } else {
        app_icon(ui, tex);
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
            ui.label(egui::RichText::new(label).size(12.0).color(DIM));
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

const PANEL_TITLE: &str = "Navi · Programa";
const PANEL_W: f32 = 340.0;
/// Altura de cada linha de programa no Coach.
const ROW_H: f32 = 44.0;
/// Cinza dos textos secundários do Coach, com contraste suficiente sobre o fundo escuro.
const DIM: Color32 = Color32::from_gray(160);

/// Painel de um programa (nome, ícone, cor), aberto com o botão direito. É uma janela de verdade
/// porque um menu ficaria cortado dentro da janela pequena do widget.
struct Panel {
    /// Nome do programa; vazio quando não há nada rastreado (só o formato do widget aparece).
    name: String,
    /// Texto em edição do novo nome.
    buf: String,
    /// Aberto pelo widget: mostra também a escolha do formato.
    from_widget: bool,
    at: egui::Pos2,
    /// Seletor de cor livre aberto.
    picker: bool,
    /// A janela já recebeu o foco: perdê-lo depois disso a fecha.
    seen_focus: bool,
    /// Quadros desde a abertura (a moldura do Windows só pode ser tirada com a janela já criada).
    frames: u8,
}

impl Panel {
    fn new(name: String, from_widget: bool, at: egui::Pos2) -> Self {
        Self { buf: name.clone(), name, from_widget, at, picker: false, seen_focus: false, frames: 0 }
    }

    fn editable(&self) -> bool {
        !self.name.is_empty() && self.name != APP_NAME
    }

    fn height(&self) -> f32 {
        let mut h = 66.0;
        if self.editable() {
            h += 128.0;
        }
        if !self.name.is_empty() {
            h += 92.0;
        }
        if self.picker {
            h += 215.0;
        }
        if self.from_widget {
            h += 62.0;
        }
        h
    }
}

/// Onde abrir o painel: junto do ponteiro, sem sair da tela.
fn panel_pos(ui: &egui::Ui) -> egui::Pos2 {
    ui.input(|i| {
        let vp = i.viewport();
        let origin = vp.outer_rect.map_or(egui::Pos2::ZERO, |r| r.min);
        let p = origin + i.pointer.latest_pos().unwrap_or_default().to_vec2() + vec2(8.0, 8.0);
        match vp.monitor_size {
            Some(m) if p.x < m.x && p.y < m.y => egui::pos2(p.x.min(m.x - PANEL_W - 8.0), p.y.min(m.y - 420.0).max(0.0)),
            _ => p,
        }
    })
}

/// Título pequeno de uma seção do painel.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).size(12.0).color(DIM));
}

/// Linha de um programa: ícone e nome à esquerda, tempo à direita, barra de proporção embaixo.
fn app_row(ui: &mut egui::Ui, app: &AppRow, tex: Option<egui::TextureId>, max: u64, total: u64, color: Option<[u8; 3]>) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(row) {
        return resp;
    }
    let p = ui.painter();
    if let Some([r, g, b]) = color {
        let c = Color32::from_rgb(r, g, b);
        p.rect_filled(row, 8.0, c.gamma_multiply(0.3));
        p.rect_filled(egui::Rect::from_min_size(row.min, vec2(3.0, row.height())), 1.5, c);
    }
    if resp.hovered() {
        p.rect_filled(row, 8.0, Color32::from_white_alpha(10));
    }
    let inner = row.shrink2(vec2(10.0, 0.0));
    let icon = egui::Rect::from_min_size(egui::pos2(inner.left(), row.top() + 7.0), vec2(ICON, ICON));
    match tex {
        Some(id) => egui::Image::new((id, vec2(ICON, ICON))).paint_at(ui, icon),
        // Sem ícone: a inicial do nome, para a linha não ficar com um buraco.
        None => {
            p.rect_filled(icon, 5.0, Color32::from_white_alpha(18));
            let letter = app.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            p.text(icon.center(), Align2::CENTER_CENTER, letter, FontId::proportional(12.0), DIM);
        }
    }

    let strong = ui.visuals().strong_text_color();
    let line_y = icon.center().y;
    let time = p.layout_no_wrap(fmt(app.secs), FontId::proportional(14.0), strong);
    let time_x = inner.right() - time.size().x;
    let time_h = time.size().y;
    p.galley(egui::pos2(time_x, line_y - time_h / 2.0), time, strong);

    let share = app.secs as f32 * 100.0 / total.max(1) as f32;
    let share = if share < 1.0 { "<1%".to_owned() } else { format!("{share:.0}%") };
    let share = p.layout_no_wrap(share, FontId::proportional(12.0), DIM);
    let share_x = time_x - 12.0 - share.size().x;
    let share_h = share.size().y;
    p.galley(egui::pos2(share_x, line_y - share_h / 2.0), share, DIM);

    let text_x = icon.right() + 10.0;
    let mut job = egui::text::LayoutJob::simple_singleline(app.name.clone(), FontId::proportional(14.0), strong);
    job.wrap.max_width = (share_x - 12.0 - text_x).max(20.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let name = p.layout_job(job);
    let name_h = name.size().y;
    p.galley(egui::pos2(text_x, line_y - name_h / 2.0), name, strong);

    let track = egui::Rect::from_min_max(egui::pos2(text_x, row.top() + 31.0), egui::pos2(inner.right(), row.top() + 35.0));
    p.rect_filled(track, 2.0, Color32::from_white_alpha(14));
    let frac = app.secs as f32 / max.max(1) as f32;
    let fill = egui::Rect::from_min_size(track.min, vec2((track.width() * frac).max(3.0), track.height()));
    p.rect_filled(fill, 2.0, band_color(frac));
    resp
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
    pub fn new(small: bool) -> Self {
        Self {
            range: Range::Hoje,
            widget: false,
            widget_list: false,
            widget_h: 0.0,
            widget_w: WIDGET_W,
            widget_start: vec2(WIDGET_W, COMPACT_H),
            small,
            format_changed: false,
            closing: false,
            widget_fresh: false,
            frame_fixed: false,
            widget_zoom: 1.0,
            just_opened: false,
            open_app: false,
            icons: HashMap::new(),
            colors: load_colors(),
            panel: None,
            pending_pick: None,
            notice: None,
        }
    }

    pub fn take_opened(&mut self) -> bool {
        std::mem::take(&mut self.just_opened)
    }

    pub fn take_open_app(&mut self) -> bool {
        std::mem::take(&mut self.open_app)
    }

    /// O novo formato do widget, se ele acabou de mudar (`true` = pequeno).
    pub fn take_format_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.format_changed).then_some(self.small)
    }

    pub fn show(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        ui.ctx().request_repaint_after(Duration::from_secs(2));
        let n = self.range.days();
        let report = tracker.report(n);
        let total: u64 = report.per_day.iter().map(|(_, s)| s).sum();

        egui::CentralPanel::default().show(ui, |ui| {
            // Textos não capturam o mouse: o clique e o botão direito valem para a linha inteira.
            ui.style_mut().interaction.selectable_labels = false;
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
            ui.add_space(4.0);
            self.status_banner(ui, &report.now);
            if let Some(notice) = &self.notice {
                ui.colored_label(Color32::LIGHT_RED, notice);
            }
            ui.add_space(10.0);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // O número principal primeiro; o período e a quantidade de programas em segundo plano.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(fmt(total)).size(28.0).strong());
                    let period = match self.range {
                        Range::Hoje => "hoje",
                        Range::Semana => "nos últimos 7 dias",
                        Range::Mes => "nos últimos 30 dias",
                    };
                    let count = report.apps.len();
                    let apps = if count == 1 { "programa" } else { "programas" };
                    ui.label(egui::RichText::new(format!("{period} · {count} {apps}")).color(DIM));
                });
                ui.add_space(8.0);

                if n > 1 {
                    chart(ui, &report.per_day);
                    ui.add_space(10.0);
                }

                if report.apps.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Ainda sem dados neste período.").color(DIM));
                        ui.label(egui::RichText::new("Use outros programas e o tempo aparece aqui.").color(DIM));
                    });
                    return;
                }
                let max = report.apps.first().map_or(1, |a| a.secs).max(1);
                legend(ui, max);
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                for app in &report.apps {
                    let tex = icon_tex(&mut self.icons, ui.ctx(), app.icon.as_ref());
                    let resp = app_row(ui, app, tex, max, total, self.colors.get(&app.name).copied());
                    if resp.secondary_clicked() {
                        self.panel = Some(Panel::new(app.name.clone(), false, panel_pos(ui)));
                    }
                    resp.on_hover_text("Botão direito: renomear, trocar o ícone e escolher a cor");
                }
            });
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
        let frame = egui::Frame::new()
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let widget_on = self.widget && !self.closing;
                        let text = if widget_on { "widget aberto" } else { "em segundo plano" };
                        ui.label(egui::RichText::new(text).size(12.0).color(DIM));
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if tracking {
                                let tex = icon_tex(&mut self.icons, ui.ctx(), now.icon.as_ref());
                                let name = now.current.as_deref().unwrap_or("");
                                program_icon(ui, name, tex);
                                ui.add(egui::Label::new(name).truncate());
                            } else {
                                ui.add(egui::Label::new(egui::RichText::new("sem atividade; o tempo não está sendo contado").color(DIM)).truncate());
                            }
                        });
                    });
                });
            });
        let area = ui.interact(frame.response.rect, ui.scope_id().with("faixa_status"), Sense::click());
        if tracking && area.secondary_clicked() {
            self.panel = Some(Panel::new(now.current.clone().unwrap_or_default(), false, panel_pos(ui)));
        }
        area.on_hover_text(if tracking {
            "O tempo deste programa está sendo contado, com o widget aberto ou não. Botão direito: renomear, ícone e cor."
        } else {
            "Sem atividade há alguns minutos: o tempo volta a contar quando você voltar."
        });
    }

    /// Janela do painel do programa; deve ser chamada a cada quadro, qualquer que seja a aba.
    pub fn panel(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        // O seletor de arquivo bloqueia a interface: só abre depois que o painel fechou.
        if let Some(name) = self.pending_pick.take() {
            let file = rfd::FileDialog::new()
                .set_title(format!("Ícone de {name}"))
                .add_filter("Imagem", &["png", "jpg", "jpeg", "ico", "bmp"])
                .pick_file();
            if let Some(path) = file {
                self.notice = tracker.set_custom_icon(&name, &path).err().map(|e| format!("Não foi possível usar a imagem: {e}"));
            }
        }
        let Some(mut p) = self.panel.take() else { return };
        let ctx = ui.ctx().clone();
        let id = egui::ViewportId::from_hash_of("coach_painel_programa");
        let builder = egui::ViewportBuilder::default()
            .with_title(PANEL_TITLE)
            .with_inner_size([PANEL_W, p.height()])
            .with_position(p.at)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false)
            .with_active(true);
        let mut close = false;
        let mut rename = None;
        let mut pick_icon = false;
        let mut reset_icon = false;
        let mut set_small = None;
        let mut colors_changed = false;
        let small = self.small;
        let custom = !p.name.is_empty() && tracker.has_custom_icon(&p.name);
        let colors = &mut self.colors;
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            ui.style_mut().interaction.selectable_labels = false;
            let full = ui.max_rect();
            let drag = ui.interact(full, ui.scope_id().with("painel_arrastar"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.painter().rect_filled(full, chrome::RADIUS, Color32::from_rgb(26, 29, 42));
            ui.painter().rect_stroke(full, chrome::RADIUS, Stroke::new(1.0, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
            // Fecha com Esc ou ao clicar fora, como um menu.
            match ui.input(|i| i.viewport().focused) {
                Some(true) => p.seen_focus = true,
                Some(false) if p.seen_focus => close = true,
                _ => {}
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= chrome::close_button(ui).clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let title = if p.name.is_empty() { "Widget" } else { p.name.as_str() };
                            ui.add(egui::Label::new(egui::RichText::new(title).strong().size(15.0)).truncate());
                        });
                    });
                });
                if p.editable() {
                    section(ui, "Nome");
                    let name = p.name.clone();
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut p.buf).desired_width(PANEL_W - 120.0));
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let new = p.buf.trim();
                        let valid = !new.is_empty() && new != name;
                        if ui.add_enabled(valid, egui::Button::new("Salvar")).clicked() || (enter && valid) {
                            rename = Some(new.to_owned());
                        }
                    });
                    section(ui, "Ícone");
                    ui.horizontal(|ui| {
                        pick_icon |= ui.button("Escolher imagem...").clicked();
                        if custom {
                            reset_icon |= ui.button("Usar o original").clicked();
                        }
                    });
                }
                if !p.name.is_empty() {
                    section(ui, "Cor de fundo");
                    let current = colors.get(&p.name).copied();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for c in PRESETS {
                            let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                            ui.painter().circle_filled(r.center(), 9.0, Color32::from_rgb(c[0], c[1], c[2]));
                            if resp.hovered() || current == Some(c) {
                                ui.painter().circle_stroke(r.center(), 10.5, Stroke::new(1.5, Color32::WHITE));
                            }
                            if resp.clicked() {
                                colors.insert(p.name.clone(), c);
                                colors_changed = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.selectable_label(p.picker, "Outra cor").clicked() {
                            p.picker = !p.picker;
                        }
                        if current.is_some() && ui.button("Sem cor").clicked() {
                            colors.remove(&p.name);
                            colors_changed = true;
                        }
                    });
                    if p.picker {
                        let [r, g, b] = current.unwrap_or([110, 142, 251]);
                        let mut c = Color32::from_rgb(r, g, b);
                        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                            colors.insert(p.name.clone(), [c.r(), c.g(), c.b()]);
                            colors_changed = true;
                        }
                    }
                }
                if p.from_widget {
                    section(ui, "Formato do widget");
                    ui.horizontal(|ui| {
                        if ui.selectable_label(small, "Pequeno").clicked() {
                            set_small = Some(true);
                        }
                        if ui.selectable_label(!small, "Médio").clicked() {
                            set_small = Some(false);
                        }
                    });
                }
            });
        });
        if colors_changed {
            save_colors(&self.colors);
        }
        if let Some(s) = set_small {
            self.set_small(s);
        }
        if let Some(new) = rename {
            tracker.rename_app(&p.name, &new);
            if let Some(c) = self.colors.remove(&p.name) {
                self.colors.insert(new.clone(), c);
                save_colors(&self.colors);
            }
            p.buf = new.clone();
            p.name = new;
        }
        if reset_icon {
            tracker.reset_icon(&p.name);
        }
        if pick_icon {
            self.pending_pick = Some(p.name.clone());
            ctx.request_repaint();
            return;
        }
        // Tira a moldura do Windows assim que a janela existe.
        p.frames = p.frames.saturating_add(1);
        if p.frames == 2 {
            chrome::flatten_window(PANEL_TITLE);
        }
        if !close {
            self.panel = Some(p);
        }
    }

    /// Muda o formato do widget, que volta ao tamanho padrão daquele formato.
    fn set_small(&mut self, small: bool) {
        if small != self.small {
            self.small = small;
            self.format_changed = true;
            self.widget_w = WIDGET_W;
        }
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
        let small = self.small;
        let height = if small {
            STRIP_H
        } else if self.widget_list {
            LIST_H
        } else {
            COMPACT_H
        };

        let id = egui::ViewportId::from_hash_of(WIDGET_ID);
        // Só pede novo tamanho quando o formato, a lista ou a escala muda: um comando a cada quadro
        // mantém a janela repintando sem parar, e o tamanho que o usuário escolheu arrastando não pode ser desfeito.
        let zoom = ctx.zoom_factor();
        let first = self.widget_h == 0.0;
        if (!first && (self.widget_h - height).abs() > f32::EPSILON) || (zoom - self.widget_zoom).abs() > 1e-3 {
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(vec2(self.widget_w, height)));
        }
        self.widget_h = height;
        // No primeiro quadro a janela ainda não existe; o comando só vale a partir do próximo.
        if first {
            self.widget_start = vec2(self.widget_w, height);
        } else {
            self.widget_zoom = zoom;
        }

        // O tamanho aqui é só o inicial e fica fixo: o egui reenvia o tamanho sempre que este valor muda,
        // o que brigaria com o redimensionamento feito pelo usuário.
        let builder = egui::ViewportBuilder::default()
            .with_title(WIDGET_TITLE)
            .with_inner_size(self.widget_start)
            .with_min_inner_size([MIN_W, MIN_H])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(true);
        let mut close = false;
        let mut toggle_list = false;
        let mut open_app = false;
        let mut size = None;
        let mut set_small = None;
        let open = self.widget_list;
        // Sem nada sendo rastreado (ocioso ou sem programa), o widget fica vermelho.
        let tracking = !data.now.idle && data.now.current.is_some();
        let tracked_name = if tracking { data.now.current.clone() } else { None };
        // Cor de fundo escolhida para o programa em uso (botão direito no widget ou no Coach).
        let program = tracked_name.as_deref().and_then(|n| self.colors.get(n)).copied();
        let mut open_panel = None;
        let (fill, time_color) = if tracking {
            (program.map_or(BG, |[r, g, b]| BG.lerp_to_gamma(Color32::from_rgb(r, g, b), 0.5)), ACCENT)
        } else {
            (Color32::from_rgb(58, 20, 24), Color32::from_rgb(255, 120, 120))
        };
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            ui.set_opacity(fade);
            // Textos não capturam o mouse: arrastar, duplo clique e botão direito valem no widget inteiro.
            ui.style_mut().interaction.selectable_labels = false;
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
            if drag.secondary_clicked() {
                open_panel = Some(panel_pos(ui));
            }
            // A fonte do relógio acompanha o tamanho da janela.
            let k = (full.width() / WIDGET_W).clamp(0.7, 2.4);
            let time_text = |size: f32| egui::RichText::new(clock(data.app_secs)).monospace().strong().size(size).color(time_color);
            let margin = if small { egui::Margin::symmetric(12, 0) } else { egui::Margin::same(14) };
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(margin)).show(ui, |ui| {
                // Formato pequeno: uma faixa só com o programa e o cronômetro.
                if small {
                    ui.horizontal_centered(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            close |= chrome::close_button(ui).clicked();
                            if chrome::icon_button(ui, Glyph::ChevronDown, 0.0).on_hover_text("Formato médio").clicked() {
                                set_small = Some(false);
                            }
                            ui.label(time_text((22.0 * k).min(full.height() * 0.6)));
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                if cur_tex.is_some() || name == APP_NAME {
                                    program_icon(ui, &name, cur_tex);
                                }
                                ui.add(egui::Label::new(egui::RichText::new(&name).strong()).truncate());
                            });
                        });
                    });
                    return;
                }
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= chrome::close_button(ui).clicked();
                        if chrome::icon_button(ui, Glyph::ChevronUp, 0.0).on_hover_text("Formato pequeno").clicked() {
                            set_small = Some(true);
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if cur_tex.is_some() || name == APP_NAME {
                                program_icon(ui, &name, cur_tex);
                            }
                            ui.add(egui::Label::new(egui::RichText::new(&name).strong()).truncate());
                        });
                    });
                });
                ui.vertical_centered(|ui| {
                    ui.add_space(2.0 * k);
                    ui.label(time_text((34.0 * k).min(full.height() * 0.3)));
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
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.weak(clock(row.secs));
                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        app_icon(ui, *tex);
                                        ui.add(egui::Label::new(egui::RichText::new(&row.name).strong()).truncate());
                                    });
                                });
                            });
                        }
                    });
                }
            });
        });
        if let Some(s) = size {
            self.widget_w = s.x.max(MIN_W);
        }
        if let Some(s) = set_small {
            self.set_small(s);
        }
        // A janela só existe a partir do primeiro quadro; então tira a moldura quadrada do Windows.
        if !first && !self.frame_fixed {
            self.frame_fixed = chrome::flatten_window(WIDGET_TITLE);
        }
        if toggle_list {
            self.widget_list = !self.widget_list;
        }
        if let Some(at) = open_panel {
            self.panel = Some(Panel::new(tracked_name.unwrap_or_default(), true, at));
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
                FontId::proportional(11.0),
                DIM,
            );
        }
        if *s > 0 && days.len() <= 8 {
            p.text(
                egui::pos2(x + slot / 2.0, bar.min.y - 7.0),
                Align2::CENTER_CENTER,
                fmt(*s),
                FontId::proportional(11.0),
                Color32::from_gray(210),
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
