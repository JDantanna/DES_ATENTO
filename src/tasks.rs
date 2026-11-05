//! Aba de Tarefas: menu de listas à esquerda, lista agrupada por data no meio e detalhes à direita.
//! A disposição acompanha a largura da janela (ver [`arrange`]): o menu vira uma fila de botões e,
//! em janela estreita, os detalhes tomam o lugar da lista.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::chrome::{self, Glyph};
use crate::datepicker;
use crate::ics;
use crate::model::{Repeat, Store, Subtask, TagColors, Task, tag_rgb};
use crate::splash::{DIAS, MESES};
use crate::theme::{ACCENT, CARD};

/// Vermelho das tarefas atrasadas (preenchimento e texto).
pub const OVERDUE: Color32 = Color32::from_rgb(210, 60, 60);
pub const OVERDUE_TEXT: Color32 = Color32::from_rgb(255, 130, 130);
/// Largura do campo de nova tarefa em repouso; ao receber o foco, ele ocupa a linha.
pub const NEW_TASK_W: f32 = 240.0;
/// Cinza dos detalhes de cada tarefa.
const DIM: Color32 = Color32::from_gray(150);

const SIDEBAR_W: f32 = 196.0;
const LIST_MIN_W: f32 = 300.0;
const DETAIL_MIN_W: f32 = 300.0;
const DETAIL_START_W: f32 = 370.0;
const GAP: f32 = 10.0;
/// Altura da linha com e sem a linha de detalhes.
const ROW_H: f32 = 52.0;
const ROW_PLAIN_H: f32 = 40.0;
/// Onde começa o texto da linha (depois da alça e do círculo de concluir).
const ROW_TEXT_X: f32 = 44.0;
const ICON_SLOT: f32 = 26.0;
/// Largura dos rótulos no painel de detalhes.
const PROP_W: f32 = 66.0;
/// Por quanto tempo o aviso "Desfazer" fica na tela.
const UNDO_SECS: f64 = 6.0;
const DIAS_CURTOS: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];

/// Quais partes da aba cabem na largura disponível.
#[derive(Debug, PartialEq, Clone, Copy)]
struct Arrange {
    sidebar: bool,
    list: bool,
    detail_w: f32,
}

fn arrange(width: f32, detail_open: bool, wanted: f32) -> Arrange {
    if !detail_open {
        return Arrange { sidebar: width >= SIDEBAR_W + GAP + LIST_MIN_W, list: true, detail_w: 0.0 };
    }
    if width < LIST_MIN_W + GAP + DETAIL_MIN_W {
        return Arrange { sidebar: false, list: false, detail_w: width };
    }
    let detail_w = wanted.clamp(DETAIL_MIN_W, width - LIST_MIN_W - GAP);
    Arrange { sidebar: width - detail_w - GAP >= SIDEBAR_W + GAP + LIST_MIN_W, list: true, detail_w }
}

#[derive(PartialEq, Clone, Copy)]
enum Filter {
    Hoje,
    Proximos,
    Todas,
    Concluidas,
}

const FILTERS: [(Filter, &str, Glyph); 4] = [
    (Filter::Hoje, "Hoje", Glyph::Today),
    (Filter::Proximos, "Próximos", Glyph::Upcoming),
    (Filter::Todas, "Todas", Glyph::All),
    (Filter::Concluidas, "Concluídas", Glyph::Done),
];

/// Seções da lista, na ordem em que aparecem.
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Group {
    Atrasadas,
    Hoje,
    Amanha,
    Semana,
    Depois,
    SemData,
    Concluidas,
}

impl Group {
    const ALL: [Group; 7] = [Group::Atrasadas, Group::Hoje, Group::Amanha, Group::Semana, Group::Depois, Group::SemData, Group::Concluidas];

    fn label(self) -> &'static str {
        match self {
            Group::Atrasadas => "Atrasadas",
            Group::Hoje => "Hoje",
            Group::Amanha => "Amanhã",
            Group::Semana => "Próximos 7 dias",
            Group::Depois => "Mais adiante",
            Group::SemData => "Sem data",
            Group::Concluidas => "Concluídas",
        }
    }
}

fn group_of(t: &Task, today: NaiveDate, now: NaiveDateTime) -> Group {
    if t.done {
        return Group::Concluidas;
    }
    if t.overdue(now) {
        return Group::Atrasadas;
    }
    match t.date.map(|d| (d - today).num_days()) {
        None => Group::SemData,
        Some(..=0) => Group::Hoje,
        Some(1) => Group::Amanha,
        Some(2..=7) => Group::Semana,
        Some(_) => Group::Depois,
    }
}

fn in_filter(filter: Filter, group: Group, t: &Task, today: NaiveDate) -> bool {
    match filter {
        Filter::Hoje => match group {
            Group::Atrasadas | Group::Hoje => true,
            Group::Concluidas => t.occurs_on(today),
            _ => false,
        },
        Filter::Proximos => matches!(group, Group::Amanha | Group::Semana | Group::Depois),
        Filter::Todas => true,
        Filter::Concluidas => group == Group::Concluidas,
    }
}

/// Número ao lado de cada lista: o que falta fazer (ou, em Concluídas, o que já foi feito).
fn count(store: &Store, filter: Filter, today: NaiveDate, now: NaiveDateTime) -> usize {
    store.tasks.iter().filter(|t| (filter == Filter::Concluidas || !t.done) && in_filter(filter, group_of(t, today, now), t, today)).count()
}

/// Texto de data e hora à direita da linha: só o que a seção ainda não diz.
fn when_text(t: &Task, group: Group) -> String {
    let time = t.time.map(|h| h.format("%H:%M").to_string());
    let day = match (group, t.date) {
        (Group::Hoje | Group::Amanha, _) | (_, None) => None,
        (Group::Semana, Some(d)) => Some(DIAS_CURTOS[d.weekday().num_days_from_monday() as usize].to_owned()),
        (_, Some(d)) => Some(d.format("%d/%m").to_string()),
    };
    match (day, time) {
        (Some(d), Some(h)) => format!("{d} {h}"),
        (Some(d), None) => d,
        (None, Some(h)) => h,
        (None, None) => String::new(),
    }
}

fn date_label(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "Hoje".to_owned(),
        1 => "Amanhã".to_owned(),
        2..=6 => DIAS[d.weekday().num_days_from_monday() as usize].to_owned(),
        _ if d.year() == today.year() => d.format("%d/%m").to_string(),
        _ => d.format("%d/%m/%Y").to_string(),
    }
}

/// O que foi reconhecido no campo de nova tarefa: "Reunião amanhã 14h #trabalho".
#[derive(Debug, PartialEq, Default)]
struct Quick {
    title: String,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    tags: Vec<String>,
}

fn quick_time(word: &str) -> Option<NaiveTime> {
    let (hour, minute, colon) = match word.split_once(':') {
        Some((h, m)) => (h, m, true),
        None => {
            let (h, m) = word.split_once('h')?;
            (h, m, false)
        }
    };
    let minute = match (minute.len(), colon) {
        (0, false) => 0,
        (2, _) => minute.parse().ok()?,
        _ => return None,
    };
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute, 0)
}

fn quick_date(word: &str, today: NaiveDate) -> Option<NaiveDate> {
    match word {
        "hoje" => return Some(today),
        "amanhã" | "amanha" => return Some(today + Duration::days(1)),
        _ => {}
    }
    let name = word.strip_suffix("-feira").unwrap_or(word);
    let weekday = match name {
        "segunda" => Some(0),
        "terça" | "terca" => Some(1),
        "quarta" => Some(2),
        "quinta" => Some(3),
        "sexta" => Some(4),
        "sábado" | "sabado" => Some(5),
        "domingo" => Some(6),
        _ => None,
    };
    if let Some(target) = weekday {
        // Sempre a próxima: "sexta" dita numa sexta é a da semana que vem.
        let now = today.weekday().num_days_from_monday() as i64;
        let ahead = (target - now - 1).rem_euclid(7) + 1;
        return Some(today + Duration::days(ahead));
    }
    let mut parts = word.split('/');
    let (day, month) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    match (parts.next(), parts.next()) {
        (None, _) => {
            let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
            if this_year >= today { Some(this_year) } else { NaiveDate::from_ymd_opt(today.year() + 1, month, day) }
        }
        (Some(y), None) => {
            let year: i32 = y.parse().ok()?;
            NaiveDate::from_ymd_opt(if year < 100 { 2000 + year } else { year }, month, day)
        }
        _ => None,
    }
}

/// Tags valem em qualquer lugar do texto; data e hora, só no fim, para que
/// "Ler o jornal de hoje cedo" continue sendo só um título.
fn parse_quick(text: &str, today: NaiveDate) -> Quick {
    let mut tags: Vec<String> = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    for word in text.split_whitespace() {
        match word.strip_prefix('#') {
            Some(tag) if !tag.is_empty() => {
                if !tags.iter().any(|t| t == tag) {
                    tags.push(tag.to_owned());
                }
            }
            _ => words.push(word),
        }
    }
    let (mut date, mut time) = (None, None);
    while let Some(last) = words.last().map(|w| w.to_lowercase()) {
        let (found_time, found_date) = (quick_time(&last), quick_date(&last, today));
        if time.is_none() && found_time.is_some() {
            time = found_time;
        } else if date.is_none() && found_date.is_some() {
            date = found_date;
        } else {
            break;
        }
        words.pop();
        // "depois de amanhã" e as ligações: "às 14h", "dia 05/11", "para sexta".
        if last.starts_with("amanh") && words.len() >= 2 && words[words.len() - 2].eq_ignore_ascii_case("depois") && words[words.len() - 1].eq_ignore_ascii_case("de") {
            words.truncate(words.len() - 2);
            date = date.map(|d| d + Duration::days(1));
        }
        if words.last().is_some_and(|w| matches!(w.to_lowercase().as_str(), "às" | "as" | "dia" | "para" | "pra" | "na" | "no")) {
            words.pop();
        }
    }
    if words.is_empty() {
        // Sem sobrar título, o texto inteiro é o título: uma tarefa chamada "hoje" é uma escolha válida.
        return Quick { title: text.trim().to_owned(), ..Default::default() };
    }
    Quick { title: words.join(" "), date, time, tags }
}

/// Etiqueta colorida de uma tag; `on` a destaca.
fn tag_chip(ui: &mut egui::Ui, tag: &str, rgb: [u8; 3], on: bool, suffix: &str) -> egui::Response {
    let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let text = RichText::new(format!("#{tag}{suffix}")).size(11.0).color(Color32::WHITE);
    let fill = c.gamma_multiply(if on { 0.75 } else { 0.28 });
    // Compacta: pouco respiro interno e altura mínima baixa, mas ainda legível.
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(5.0, 0.0);
        ui.add(egui::Button::new(text).fill(fill).stroke(Stroke::new(1.0, c.gamma_multiply(0.85))).corner_radius(8).min_size(vec2(0.0, 16.0)))
    })
    .inner
}

/// Linhas de tags lado a lado, com pouco espaço entre elas.
fn tight(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
}

/// Item do menu lateral: ícone ou ponto colorido, nome e contagem à direita.
fn nav_item(ui: &mut egui::Ui, glyph: Option<Glyph>, dot: Option<Color32>, label: &str, n: usize, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 8.0, ACCENT.gamma_multiply(0.22));
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, Color32::from_white_alpha(10));
    }
    let color = if on { Color32::WHITE } else { Color32::from_gray(205) };
    let icon = pos2(rect.left() + 17.0, rect.center().y);
    if let Some(g) = glyph {
        chrome::paint_glyph(p, icon, g, if on { ACCENT } else { Color32::from_gray(175) }, 0.0);
    }
    if let Some(d) = dot {
        p.circle_filled(icon, 4.5, d);
    }
    let mut right = rect.right() - 10.0;
    if n > 0 {
        let num = p.layout_no_wrap(n.to_string(), FontId::proportional(12.0), DIM);
        right -= num.size().x;
        let h = num.size().y;
        p.galley(pos2(right, rect.center().y - h / 2.0), num, DIM);
        right -= 8.0;
    }
    let name = line_job(label, 13.5, color, right - rect.left() - 34.0);
    let name = p.layout_job(name);
    let h = name.size().y;
    p.galley(pos2(rect.left() + 34.0, rect.center().y - h / 2.0), name, color);
    resp
}

/// Texto de uma linha só, terminado em "…" se não couber em `width`.
fn line_job(text: &str, size: f32, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), color);
    job.wrap.max_width = width.max(8.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    job
}

/// Cabeçalho de seção, clicável para recolher.
fn group_header(ui: &mut egui::Ui, group: Group, n: usize, closed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, Color32::from_white_alpha(7));
    }
    let tint = if group == Group::Atrasadas { OVERDUE_TEXT } else { Color32::from_gray(190) };
    let glyph = if closed { Glyph::ChevronRight } else { Glyph::ChevronDown };
    chrome::paint_glyph(p, pos2(rect.left() + 12.0, rect.center().y), glyph, DIM, 0.0);
    let name = p.layout_no_wrap(group.label().to_owned(), FontId::proportional(13.0), tint);
    let (w, h) = (name.size().x, name.size().y);
    p.galley(pos2(rect.left() + 24.0, rect.center().y - h / 2.0), name, tint);
    p.text(pos2(rect.left() + 32.0 + w, rect.center().y), Align2::LEFT_CENTER, n.to_string(), FontId::proportional(12.0), DIM);
    resp
}

/// O círculo de concluir. No mouse, antecipa o visto.
fn paint_check(p: &egui::Painter, c: egui::Pos2, color: Color32, done: bool, hot: bool) {
    if done {
        p.circle_filled(c, 9.0, Color32::from_gray(90));
        chrome::paint_glyph(p, c, Glyph::Check, Color32::WHITE, 0.0);
    } else {
        p.circle_stroke(c, 8.5, Stroke::new(1.6, color));
        if hot {
            p.circle_filled(c, 8.0, color.gamma_multiply(0.28));
            chrome::paint_glyph(p, c, Glyph::Check, Color32::WHITE, 0.0);
        }
    }
}

/// Linha de propriedade do painel de detalhes: rótulo à esquerda, controles à direita (quebrando linha se preciso).
fn prop(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(PROP_W, 24.0), Sense::hover());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(12.5), DIM);
        ui.horizontal_wrapped(add);
    });
    ui.add_space(4.0);
}

/// O que uma linha pediu neste quadro; aplicado depois de desenhar a lista.
enum Act {
    Open(u64),
    Toggle(u64),
    Delete(u64),
    Notes(u64),
    Move(u64, Option<NaiveDate>),
}

/// Última mudança, que ainda pode ser desfeita.
struct Undo {
    text: String,
    task: Task,
    index: usize,
    at: f64,
}

pub struct TasksView {
    selected: Option<u64>,
    filter: Filter,
    tag_filter: Option<String>,
    new_title: String,
    date_buf: String,
    time_buf: String,
    tag_buf: String,
    sub_buf: String,
    buf_for: Option<u64>,
    msg: Option<String>,
    tags_open: bool,
    /// Áreas onde um clique não desfaz o filtro de tag (as tags, as linhas, os detalhes).
    keep_filter: Vec<Rect>,
    /// Tarefa cuja linha está sendo arrastada para mudar a ordem.
    row_drag: Option<u64>,
    /// Mês mostrado no calendário em popup da data (`None` = fechado).
    cal_month: Option<NaiveDate>,
    /// Tarefas com as informações recolhidas no rodapé da linha (por padrão ficam abertas).
    collapsed: HashSet<u64>,
    /// Seções recolhidas.
    closed_groups: HashSet<Group>,
    /// Texto em edição de cada tag no painel de tags.
    tag_edit: HashMap<String, String>,
    /// Largura escolhida para o painel de detalhes.
    detail_w: f32,
    undo: Option<Undo>,
    /// Ctrl+N: o campo de nova tarefa recebe o foco no próximo quadro.
    focus_new: bool,
}

impl TasksView {
    pub fn new() -> Self {
        Self {
            selected: None,
            filter: Filter::Hoje,
            tag_filter: None,
            new_title: String::new(),
            date_buf: String::new(),
            time_buf: String::new(),
            tag_buf: String::new(),
            sub_buf: String::new(),
            buf_for: None,
            msg: None,
            tags_open: false,
            keep_filter: Vec::new(),
            row_drag: None,
            cal_month: None,
            collapsed: HashSet::new(),
            closed_groups: HashSet::from([Group::Concluidas]),
            tag_edit: HashMap::new(),
            detail_w: DETAIL_START_W,
            undo: None,
            focus_new: false,
        }
    }

    /// Seleciona uma tarefa vinda de outra aba, garantindo que apareça na lista.
    pub fn open(&mut self, id: u64) {
        self.selected = Some(id);
        self.filter = Filter::Todas;
        self.tag_filter = None;
        self.closed_groups.clear();
    }

    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        let today = Local::now().date_naive();
        let now = Local::now().naive_local();
        // Esc fecha os detalhes; Ctrl+N vai direto para o campo de nova tarefa.
        if self.selected.is_some() && self.cal_month.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && ui.memory(|m| m.focused().is_none()) {
            self.selected = None;
        }
        if ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.focus_new = true;
            if !arrange(ui.available_width(), self.selected.is_some(), self.detail_w).list {
                self.selected = None;
            }
        }
        self.keep_filter.clear();

        let frame = egui::Frame::NONE.inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 12 });
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            // Textos não capturam o mouse: clicar ou arrastar neles vale para o que está por baixo.
            ui.style_mut().interaction.selectable_labels = false;
            let full = ui.available_rect_before_wrap();
            let open = self.selected.is_some();
            let slide = ui.ctx().animate_bool_with_time(ui.scope_id().with("detalhes_abrindo"), open, 0.16);
            let plan = arrange(full.width(), open, self.detail_w);
            // Os detalhes entram deslizando; em janela estreita, simplesmente tomam o lugar da lista.
            let detail_now = if plan.list { plan.detail_w * slide } else { plan.detail_w };
            let mut left = full.left();
            if plan.sidebar {
                let rect = Rect::from_min_size(full.min, vec2(SIDEBAR_W, full.height()));
                ui.painter().rect_filled(rect, 12.0, CARD);
                chrome::region(ui, rect.shrink(8.0), "menu", |ui| self.sidebar(ui, store, today, now));
                left += SIDEBAR_W + GAP;
            }
            if plan.list {
                let right = full.right() - if detail_now > 0.0 { detail_now + GAP } else { 0.0 };
                let rect = Rect::from_min_max(pos2(left, full.top()), pos2(right.max(left + 1.0), full.bottom()));
                chrome::region(ui, rect, "lista", |ui| self.list(ui, store, today, now, !plan.sidebar));
            }
            if open && detail_now > 0.5 {
                let rect = Rect::from_min_size(pos2(full.right() - detail_now, full.top()), vec2(plan.detail_w, full.height()));
                self.keep_filter.push(rect.intersect(full));
                ui.painter().rect_filled(rect, 12.0, CARD);
                if plan.list {
                    // A borda esquerda dos detalhes ajusta a largura deles.
                    let edge = Rect::from_min_max(pos2(rect.left() - GAP, rect.top()), pos2(rect.left(), rect.bottom()));
                    let grip = ui.interact(edge, ui.scope_id().with("divisoria"), Sense::drag());
                    if grip.hovered() || grip.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if grip.dragged() {
                        self.detail_w = (plan.detail_w - grip.drag_delta().x).clamp(DETAIL_MIN_W, (full.width() - LIST_MIN_W - GAP).max(DETAIL_MIN_W));
                    }
                }
                let back = !plan.list;
                chrome::region(ui, rect.shrink(14.0), "detalhes", |ui| self.editor(ui, store, back));
            }
            ui.advance_cursor_after_rect(full);
        });
        self.tags_window(ui.ctx(), store);
        self.undo_toast(ui, store);
        // O filtro de tag vale só enquanto não se clica em outro lugar; mexer nas tarefas filtradas não conta.
        if self.tag_filter.is_some() && ui.input(|i| i.pointer.primary_clicked()) {
            let pos = ui.input(|i| i.pointer.interact_pos());
            if !pos.is_some_and(|p| self.keep_filter.iter().any(|r| r.contains(p))) {
                self.tag_filter = None;
            }
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, store: &Store, today: NaiveDate, now: NaiveDateTime) {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (filter, label, glyph) in FILTERS {
            if nav_item(ui, Some(glyph), None, label, count(store, filter, today, now), self.filter == filter).clicked() {
                self.filter = filter;
            }
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("Tags").size(12.5).color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let edit = egui::Button::new(RichText::new("Editar").size(12.0).color(DIM)).frame(false);
                if ui.add(edit).on_hover_text("Trocar cores e nomes das tags").clicked() {
                    self.tags_open = true;
                }
            });
        });
        let tags = store.all_tags();
        if tags.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Use #tag ao criar uma tarefa.").size(12.0).color(DIM));
            });
        }
        egui::ScrollArea::vertical().id_salt("menu_tags").auto_shrink([false, false]).show(ui, |ui| {
            for tag in &tags {
                let [r, g, b] = tag_rgb(&store.tag_colors, tag);
                let on = self.tag_filter.as_ref() == Some(tag);
                let item = nav_item(ui, None, Some(Color32::from_rgb(r, g, b)), tag, store.tag_count(tag), on);
                self.keep_filter.push(item.rect);
                if item.clicked() {
                    self.tag_filter = if on { None } else { Some(tag.clone()) };
                }
            }
        });
    }

    /// As listas em uma fila de botões, para quando não há espaço para o menu lateral.
    fn pills(&mut self, ui: &mut egui::Ui, store: &Store, today: NaiveDate, now: NaiveDateTime) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (filter, label, _) in FILTERS {
                let n = count(store, filter, today, now);
                let text = if n > 0 && filter != Filter::Concluidas { format!("{label} {n}") } else { label.to_owned() };
                if ui.selectable_label(self.filter == filter, text).clicked() {
                    self.filter = filter;
                }
            }
            let tags = store.all_tags();
            let name = self.tag_filter.as_ref().map_or_else(|| "Tags".to_owned(), |t| format!("#{t}"));
            let menu = ui.menu_button(name, |ui| {
                for tag in &tags {
                    let on = self.tag_filter.as_ref() == Some(tag);
                    let item = ui.selectable_label(on, format!("#{tag}"));
                    self.keep_filter.push(item.rect);
                    if item.clicked() {
                        self.tag_filter = if on { None } else { Some(tag.clone()) };
                        ui.close();
                    }
                }
                if ui.button("Editar tags").clicked() {
                    self.tags_open = true;
                    ui.close();
                }
            });
            self.keep_filter.push(menu.response.rect);
        });
        ui.add_space(4.0);
    }

    /// Título da lista, resumo e ações.
    fn header(&mut self, ui: &mut egui::Ui, store: &mut Store, today: NaiveDate, now: NaiveDateTime) {
        let pending = count(store, self.filter, today, now);
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let (title, mut sub) = match self.filter {
            Filter::Hoje => {
                let d = today;
                let day = format!("{}, {} de {}", DIAS[d.weekday().num_days_from_monday() as usize], d.day(), MESES[d.month0() as usize]);
                ("Hoje", format!("{day} · {}", plural(pending, "pendente", "pendentes")))
            }
            Filter::Proximos => ("Próximos", plural(pending, "tarefa", "tarefas")),
            Filter::Todas => ("Todas", plural(pending, "pendente", "pendentes")),
            Filter::Concluidas => ("Concluídas", plural(pending, "tarefa", "tarefas")),
        };
        if let Some(tag) = &self.tag_filter {
            sub = format!("#{tag} · {sub}");
        }
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(title).size(20.0).strong()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("•••", |ui| self.more_menu(ui, store)).response.on_hover_text("Tags, importar e exportar");
                // Ordenar pela hora; depois dá para ajustar à mão, arrastando as linhas.
                if chrome::icon_button(ui, Glyph::SortDown, 0.0).on_hover_text("Ordenar por hora: decrescente").clicked() {
                    store.sort_by_time(true);
                }
                if chrome::icon_button(ui, Glyph::SortUp, 0.0).on_hover_text("Ordenar por hora: crescente").clicked() {
                    store.sort_by_time(false);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(sub).size(12.5).color(DIM)).truncate());
                });
            });
        });
        if self.filter == Filter::Hoje {
            // Progresso do dia: o que já foi feito do que estava marcado para hoje.
            let done = store.tasks.iter().filter(|t| t.done && t.occurs_on(today)).count();
            let total = done + pending;
            if total > 0 {
                let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
                ui.painter().rect_filled(bar, 1.5, Color32::from_white_alpha(14));
                let fill = Rect::from_min_size(bar.min, vec2(bar.width() * done as f32 / total as f32, bar.height()));
                ui.painter().rect_filled(fill, 1.5, ACCENT);
            }
        }
        if let Some(m) = &self.msg {
            ui.label(RichText::new(m).size(12.0).color(DIM));
        }
    }

    fn more_menu(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        if ui.button("Editar tags").clicked() {
            self.tags_open = true;
            ui.close();
        }
        ui.separator();
        ui.weak("Google Calendar, Outlook, Apple...");
        if ui.button("Importar arquivo .ics").clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new().add_filter("Calendário", &["ics"]).pick_file() {
                self.msg = Some(match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let items = ics::parse(&text);
                        let n = items.len();
                        let added = ics::import_into(store, items);
                        format!("Importados {added} de {n} eventos.")
                    }
                    Err(e) => format!("Não foi possível ler o arquivo: {e}"),
                });
            }
        }
        if ui.button("Exportar tarefas com data (.ics)").clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new().add_filter("Calendário", &["ics"]).set_file_name("navi.ics").save_file() {
                self.msg = Some(match std::fs::write(&path, ics::export(store)) {
                    Ok(()) => "Exportado.".to_owned(),
                    Err(e) => format!("Falha ao salvar: {e}"),
                });
            }
        }
    }

    /// Campo de nova tarefa: compacto em repouso, ocupa a linha ao receber o foco e entende
    /// data, hora e tags no próprio texto. Enter cria e o foco fica nele, para emendar outra.
    fn quick_add(&mut self, ui: &mut egui::Ui, store: &mut Store, today: NaiveDate) {
        let id = ui.scope_id().with("nova_tarefa");
        let focused = ui.memory(|m| m.has_focus(id));
        let wide = focused || !self.new_title.is_empty();
        let grow = ui.ctx().animate_bool_with_time(id.with("largura"), wide, 0.14);
        let avail = ui.available_width();
        let width = egui::lerp(NEW_TASK_W.min(avail)..=avail, grow);
        let border = if focused { ACCENT.gamma_multiply(0.75) } else { Color32::from_white_alpha(22) };
        let frame = egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(9)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width((width - 22.0).max(60.0));
                ui.horizontal(|ui| {
                    let (icon, _) = ui.allocate_exact_size(vec2(14.0, 18.0), Sense::hover());
                    chrome::paint_glyph(ui.painter(), icon.center(), Glyph::Plus, if focused { ACCENT } else { DIM }, 0.0);
                    let hint = if wide { "Ex.: Reunião amanhã 14h #trabalho" } else { "Nova tarefa" };
                    let edit = egui::TextEdit::singleline(&mut self.new_title)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .hint_text(hint)
                        .desired_width(ui.available_width());
                    ui.add(edit).on_hover_text("Enter cria a tarefa. Atalho: Ctrl+N")
                })
                .inner
            });
        self.keep_filter.push(frame.response.rect);
        let field = frame.inner;
        if std::mem::take(&mut self.focus_new) {
            field.request_focus();
        }
        let quick = parse_quick(&self.new_title, today);
        // Mostra o que foi entendido antes de criar.
        let mut seen: Vec<String> = Vec::new();
        if let Some(d) = quick.date {
            seen.push(date_label(d, today));
        }
        if let Some(h) = quick.time {
            seen.push(h.format("%H:%M").to_string());
        }
        seen.extend(quick.tags.iter().map(|t| format!("#{t}")));
        if !seen.is_empty() {
            ui.label(RichText::new(seen.join("  ·  ")).size(12.0).color(ACCENT));
        }
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !quick.title.is_empty() {
            let default_date = match self.filter {
                Filter::Hoje => Some(today),
                Filter::Proximos => Some(today + Duration::days(1)),
                _ => quick.time.map(|_| today),
            };
            let id = store.add(quick.title, quick.date.or(default_date));
            if let Some(t) = store.tasks.iter_mut().find(|t| t.id == id) {
                t.time = quick.time;
                t.tags = quick.tags;
                if let Some(tag) = &self.tag_filter {
                    if !t.tags.contains(tag) {
                        t.tags.push(tag.clone());
                    }
                }
                t.set_anchor();
            }
            self.new_title.clear();
            field.request_focus();
        }
    }

    fn list(&mut self, ui: &mut egui::Ui, store: &mut Store, today: NaiveDate, now: NaiveDateTime, pills: bool) {
        if pills {
            self.pills(ui, store, today, now);
        }
        self.header(ui, store, today, now);
        ui.add_space(2.0);
        self.quick_add(ui, store, today);
        ui.add_space(4.0);

        let mut groups: Vec<(Group, Vec<usize>)> = Group::ALL.iter().map(|g| (*g, Vec::new())).collect();
        for (i, t) in store.tasks.iter().enumerate() {
            let group = group_of(t, today, now);
            if in_filter(self.filter, group, t, today) && self.tag_filter.as_ref().is_none_or(|f| t.tags.contains(f)) {
                if let Some(slot) = groups.iter_mut().find(|g| g.0 == group) {
                    slot.1.push(i);
                }
            }
        }
        groups.retain(|g| !g.1.is_empty());
        // Em Concluídas tudo é da mesma seção: o cabeçalho só repetiria o título.
        let headed = self.filter != Filter::Concluidas;

        let mut acts = Vec::new();
        let mut rows: Vec<(Rect, u64, Group)> = Vec::new();
        let scroll = egui::ScrollArea::vertical().id_salt("linhas").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if groups.is_empty() {
                self.empty_state(ui);
            }
            for (group, indices) in &groups {
                if headed {
                    let closed = self.closed_groups.contains(group);
                    let head = group_header(ui, *group, indices.len(), closed);
                    self.keep_filter.push(head.rect);
                    if head.clicked() && !self.closed_groups.remove(group) {
                        self.closed_groups.insert(*group);
                    }
                    if closed {
                        continue;
                    }
                }
                for &i in indices {
                    self.task_row(ui, &store.tasks[i], *group, &store.tag_colors, today, &mut acts, &mut rows);
                }
                ui.add_space(6.0);
            }
        });
        self.finish_row_drag(ui, store, &rows, scroll.inner_rect);
        let time = ui.input(|i| i.time);
        for act in acts {
            self.apply(store, act, time);
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui) {
        let (title, hint) = match (self.filter, self.tag_filter.is_some()) {
            (_, true) => ("Nenhuma tarefa com esta tag aqui.", "Clique fora das tags para ver todas."),
            (Filter::Hoje, _) => ("Nada para hoje.", "Escreva acima para criar a primeira tarefa."),
            (Filter::Proximos, _) => ("Nada nos próximos dias.", "Tente: Dentista sexta 15h"),
            (Filter::Todas, _) => ("Nenhuma tarefa ainda.", "Escreva acima para criar a primeira."),
            (Filter::Concluidas, _) => ("Nada concluído ainda.", "O que você concluir aparece aqui."),
        };
        ui.add_space(36.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(title).size(15.0));
            ui.label(RichText::new(hint).size(12.5).color(DIM));
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn task_row(&mut self, ui: &mut egui::Ui, t: &Task, group: Group, colors: &TagColors, today: NaiveDate, acts: &mut Vec<Act>, rows: &mut Vec<(Rect, u64, Group)>) {
        let id = t.id;
        let width = ui.available_width();
        let overdue = group == Group::Atrasadas;
        let selected = self.selected == Some(id);
        let (ok, total) = t.subtasks_done();
        let has_meta = total > 0 || t.repeat != Repeat::None || t.notify.is_some() || !t.tags.is_empty();
        let base_h = if has_meta { ROW_H } else { ROW_PLAIN_H };
        // Informações da tarefa: aparecem no rodapé da linha (até 4 linhas) e podem ser recolhidas pela seta.
        let notes = t.notes.trim();
        let has_notes = !notes.is_empty();
        let notes_open = has_notes && !self.collapsed.contains(&id);
        let footer = notes_open.then(|| {
            let mut job = egui::text::LayoutJob::simple(notes.to_owned(), FontId::proportional(12.0), DIM, (width - ROW_TEXT_X - 12.0).max(60.0));
            job.wrap.max_rows = 4;
            job.wrap.overflow_character = Some('…');
            ui.painter().layout_job(job)
        });
        let height = base_h + footer.as_ref().map_or(0.0, |g| g.size().y + 6.0);

        let (row, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        rows.push((row, id, group));
        self.keep_filter.push(row);
        let hot = resp.contains_pointer();
        let glow = ui.ctx().animate_bool_with_time(ui.scope_id().with(("brilho", id)), hot || selected, 0.12);
        let p = ui.painter();
        if overdue {
            // Atrasada: preenchida de vermelho.
            p.rect_filled(row, 8.0, OVERDUE.gamma_multiply(0.26 + 0.12 * glow));
        } else if selected {
            p.rect_filled(row, 8.0, ACCENT.gamma_multiply(0.18));
        } else if glow > 0.0 {
            p.rect_filled(row, 8.0, Color32::from_white_alpha((11.0 * glow) as u8));
        }
        if selected {
            p.rect_filled(Rect::from_min_size(row.min + vec2(0.0, 8.0), vec2(3.0, height - 16.0)), 1.5, ACCENT);
        }

        let title_y = row.top() + if has_meta { 17.0 } else { base_h / 2.0 };
        if hot {
            chrome::paint_glyph(p, pos2(row.left() + 8.0, title_y), Glyph::Grip, Color32::from_gray(110), 0.0);
        }
        let check_rect = Rect::from_center_size(pos2(row.left() + 26.0, title_y), vec2(26.0, 26.0));
        let check = ui.interact(check_rect, ui.scope_id().with(("concluir", id)), Sense::click());
        let [r, g, b] = t.color;
        let ring = if overdue { OVERDUE_TEXT } else { Color32::from_rgb(r, g, b) };
        paint_check(p, check_rect.center(), ring, t.done, check.hovered());
        if check.clicked() {
            acts.push(Act::Toggle(id));
        }
        check.on_hover_text(if t.done { "Reabrir" } else { "Concluir" });

        // Da direita para a esquerda: excluir, recolher as informações, data e hora.
        let text_left = row.left() + ROW_TEXT_X;
        let mut right = row.right() - 4.0 - ICON_SLOT;
        if hot {
            let cell = Rect::from_min_size(pos2(right, title_y - 12.0), vec2(ICON_SLOT, 24.0));
            let trash = ui.interact(cell, ui.scope_id().with(("excluir", id)), Sense::click());
            if trash.hovered() {
                p.rect_filled(cell, 6.0, Color32::from_rgba_unmultiplied(200, 60, 70, 90));
            }
            chrome::paint_glyph(p, cell.center(), Glyph::Trash, if trash.hovered() { Color32::WHITE } else { Color32::from_gray(170) }, 0.0);
            if trash.on_hover_text("Excluir").clicked() {
                acts.push(Act::Delete(id));
            }
        }
        if has_notes {
            right -= ICON_SLOT;
            let cell = Rect::from_min_size(pos2(right, title_y - 12.0), vec2(ICON_SLOT, 24.0));
            let arrow = ui.interact(cell, ui.scope_id().with(("seta", id)), Sense::click());
            let color = if arrow.hovered() { Color32::WHITE } else { Color32::from_gray(160) };
            chrome::paint_glyph(p, cell.center(), if notes_open { Glyph::ChevronUp } else { Glyph::ChevronDown }, color, 0.0);
            if arrow.on_hover_text(if notes_open { "Recolher informações" } else { "Mostrar informações" }).clicked() {
                acts.push(Act::Notes(id));
            }
        }
        let when = when_text(t, group);
        if !when.is_empty() {
            let color = if overdue { OVERDUE_TEXT } else { DIM };
            let galley = p.layout_no_wrap(when, FontId::proportional(12.5), color);
            right -= galley.size().x + 2.0;
            let h = galley.size().y;
            p.galley(pos2(right, title_y - h / 2.0), galley, color);
            right -= 8.0;
        }

        let (color, underline, strike) = if t.done {
            (DIM, Stroke::NONE, Stroke::new(1.0, DIM))
        } else if overdue {
            (OVERDUE_TEXT, Stroke::new(1.0, OVERDUE_TEXT), Stroke::NONE)
        } else {
            (Color32::from_gray(235), Stroke::NONE, Stroke::NONE)
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(&t.title, 0.0, egui::TextFormat { font_id: FontId::proportional(14.5), color, underline, strikethrough: strike, ..Default::default() });
        job.wrap.max_width = (right - text_left).max(20.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let title = p.layout_job(job);
        let (h, cut) = (title.size().y, title.elided);
        p.galley(pos2(text_left, title_y - h / 2.0), title, color);
        // Título cortado: o texto inteiro aparece ao passar o mouse.
        if cut {
            resp.clone().on_hover_text(t.title.as_str());
        }

        if has_meta {
            let y = title_y + 18.0;
            let clip = p.with_clip_rect(Rect::from_min_max(pos2(text_left, y - 9.0), pos2(row.right() - 8.0, y + 9.0)));
            let mut x = text_left;
            let icon = |x: &mut f32, glyph: Glyph, text: Option<String>| {
                chrome::paint_glyph(&clip, pos2(*x + 5.0, y), glyph, DIM, 0.0);
                *x += 13.0;
                if let Some(text) = text {
                    let galley = clip.layout_no_wrap(text, FontId::proportional(11.5), DIM);
                    let (w, h) = (galley.size().x, galley.size().y);
                    clip.galley(pos2(*x, y - h / 2.0), galley, DIM);
                    *x += w;
                }
                *x += 8.0;
            };
            if total > 0 {
                icon(&mut x, Glyph::Check, Some(format!("{ok}/{total}")));
            }
            if t.repeat != Repeat::None {
                icon(&mut x, Glyph::Repeat, None);
            }
            if t.notify.is_some() {
                icon(&mut x, Glyph::Bell, None);
            }
            for tag in &t.tags {
                let [r, g, b] = tag_rgb(colors, tag);
                let c = Color32::from_rgb(r, g, b);
                let galley = clip.layout_no_wrap(format!("#{tag}"), FontId::proportional(11.5), c);
                let (w, h) = (galley.size().x, galley.size().y);
                clip.galley(pos2(x, y - h / 2.0), galley, c);
                x += w + 8.0;
            }
        }
        if let Some(g) = footer {
            p.galley(pos2(text_left, row.top() + base_h - 2.0), g, DIM);
        }

        if resp.drag_started() {
            self.row_drag = Some(id);
        }
        if resp.clicked() {
            acts.push(Act::Open(id));
        }
        resp.context_menu(|ui| {
            if ui.button("Abrir detalhes").clicked() {
                acts.push(Act::Open(id));
                ui.close();
            }
            ui.separator();
            for (label, date) in [("Para hoje", Some(today)), ("Para amanhã", Some(today + Duration::days(1))), ("Sem data", None)] {
                if ui.button(label).clicked() {
                    acts.push(Act::Move(id, date));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Excluir").clicked() {
                acts.push(Act::Delete(id));
                ui.close();
            }
        });
    }

    fn apply(&mut self, store: &mut Store, act: Act, time: f64) {
        match act {
            Act::Open(id) => self.selected = Some(id),
            Act::Notes(id) => {
                if !self.collapsed.remove(&id) {
                    self.collapsed.insert(id);
                }
            }
            Act::Toggle(id) => {
                let Some(index) = store.tasks.iter().position(|t| t.id == id) else { return };
                let task = store.tasks[index].clone();
                let t = &mut store.tasks[index];
                t.done = !t.done;
                let rolled = t.roll_over();
                let text = match (rolled, t.done, t.date) {
                    (true, _, Some(d)) => format!("Próxima data: {}", d.format("%d/%m")),
                    (_, true, _) => "Tarefa concluída".to_owned(),
                    _ => "Tarefa reaberta".to_owned(),
                };
                store.dirty = true;
                self.undo = Some(Undo { text, task, index, at: time });
            }
            Act::Delete(id) => self.delete(store, id, time),
            Act::Move(id, date) => {
                if let Some(t) = store.tasks.iter_mut().find(|t| t.id == id) {
                    t.date = date;
                    t.set_anchor();
                    store.dirty = true;
                }
            }
        }
    }

    /// Exclui e deixa a opção de desfazer: por isso a lixeira não precisa pedir confirmação.
    fn delete(&mut self, store: &mut Store, id: u64, time: f64) {
        let Some(index) = store.tasks.iter().position(|t| t.id == id) else { return };
        let task = store.tasks[index].clone();
        store.remove(id);
        if self.selected == Some(id) {
            self.selected = None;
            self.buf_for = None;
        }
        self.undo = Some(Undo { text: "Tarefa excluída".to_owned(), task, index, at: time });
    }

    fn undo_toast(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        let Some(undo) = &self.undo else { return };
        let left = UNDO_SECS - (ui.input(|i| i.time) - undo.at);
        if left <= 0.0 {
            self.undo = None;
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
        let text = undo.text.clone();
        let mut clicked = false;
        let area = egui::Area::new(ui.scope_id().with("desfazer"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(text.as_str());
                        clicked = ui.button("Desfazer").clicked();
                    });
                });
            });
        self.keep_filter.push(area.response.rect);
        if clicked && let Some(undo) = self.undo.take() {
            store.restore(undo.task, undo.index);
        }
    }

    /// Painel para trocar a cor, renomear ou remover as tags.
    fn tags_window(&mut self, ctx: &egui::Context, store: &mut Store) {
        if !self.tags_open {
            return;
        }
        let tags = store.all_tags();
        self.tag_edit.retain(|k, _| tags.contains(k));
        let (mut recolor, mut rename, mut delete) = (None, None, None);
        let mut open = true;
        let tall = ctx.content_rect().height() * 0.6;
        egui::Window::new("Tags").open(&mut open).collapsible(false).default_width(360.0).show(ctx, |ui| {
            if tags.is_empty() {
                ui.weak("Nenhuma tag ainda. Use #tag ao criar uma tarefa.");
            }
            egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| {
                for tag in &tags {
                    ui.horizontal(|ui| {
                        let mut rgb = tag_rgb(&store.tag_colors, tag);
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            recolor = Some((tag.clone(), rgb));
                        }
                        let buf = self.tag_edit.entry(tag.clone()).or_insert_with(|| tag.clone());
                        let r = ui.add(egui::TextEdit::singleline(buf).desired_width(150.0));
                        if r.lost_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) || buf.trim().is_empty() {
                                *buf = tag.clone();
                            } else if buf.trim() != tag.as_str() {
                                rename = Some((tag.clone(), buf.clone()));
                            }
                        }
                        ui.weak(format!("{} tarefas", store.tag_count(tag)));
                        if chrome::icon_button(ui, Glyph::Trash, 0.0).on_hover_text("Remover de todas as tarefas").clicked() {
                            delete = Some(tag.clone());
                        }
                    });
                }
            });
        });
        self.tags_open = open;
        if let Some((tag, rgb)) = recolor {
            store.set_tag_color(&tag, rgb);
        }
        if let Some((from, to)) = rename {
            store.rename_tag(&from, &to);
            self.tag_edit.remove(&from);
            if self.tag_filter.as_ref() == Some(&from) {
                self.tag_filter = Some(to.trim().trim_start_matches('#').trim().to_owned());
            }
        }
        if let Some(tag) = delete {
            store.delete_tag(&tag);
            self.tag_edit.remove(&tag);
            if self.tag_filter.as_ref() == Some(&tag) {
                self.tag_filter = None;
            }
        }
    }

    /// Cancela um arrasto em andamento (ao sair da aba).
    pub fn cancel_drag(&mut self) {
        self.row_drag = None;
    }

    /// Acompanha o arrasto de uma linha: mostra a linha sendo levada e onde ela vai cair; ao soltar, reordena.
    /// A seção de uma tarefa vem da data, então a ordem manual só muda dentro da própria seção.
    fn finish_row_drag(&mut self, ui: &egui::Ui, store: &mut Store, rows: &[(Rect, u64, Group)], area: Rect) {
        let Some(id) = self.row_drag else { return };
        let ctx = ui.ctx();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.row_drag = None;
            return;
        }
        let pos = ctx.pointer_latest_pos();
        let own = rows.iter().find(|r| r.1 == id).map(|r| r.2);
        let target = pos
            .filter(|p| area.contains(*p))
            .and_then(|p| rows.iter().find(|(r, _, _)| r.y_range().contains(p.y)).copied())
            .filter(|(_, t, g)| *t != id && Some(*g) == own);
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some((r, tid, _)) = target {
            let at = |id| store.tasks.iter().position(|t| t.id == id);
            // Subindo, cai acima da linha de destino; descendo, abaixo.
            let y = if at(id) > at(tid) { r.top() } else { r.bottom() };
            ui.painter().line_segment([pos2(r.left() + 4.0, y), pos2(r.right() - 4.0, y)], Stroke::new(2.0, ACCENT));
        }
        if let (Some(p), Some(t)) = (pos, store.tasks.iter().find(|t| t.id == id)) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("navi_row_drag")));
            let galley = painter.layout_no_wrap(t.title.clone(), FontId::proportional(13.0), Color32::WHITE);
            let [r, g, b] = t.color;
            let rect = Rect::from_min_size(p + vec2(12.0, 8.0), galley.size() + vec2(14.0, 8.0));
            painter.rect_filled(rect, 5.0, Color32::from_rgb(r, g, b).gamma_multiply(0.9));
            painter.galley(rect.min + vec2(7.0, 4.0), galley, Color32::WHITE);
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.row_drag = None;
            if let Some((_, tid, _)) = target {
                store.move_task(id, tid);
            }
        }
    }

    fn sync_buffers(&mut self, t: &Task) {
        if self.buf_for != Some(t.id) {
            self.buf_for = Some(t.id);
            self.date_buf = t.date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default();
            self.time_buf = t.time.map(|h| h.format("%H:%M").to_string()).unwrap_or_default();
            self.tag_buf.clear();
            self.sub_buf.clear();
            self.cal_month = None;
        } else if parse_date(&self.date_buf).is_some() && parse_date(&self.date_buf) != t.date {
            // A data mudou fora da caixa (concluir uma recorrente, arrastar no calendário).
            self.date_buf = t.date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default();
        }
    }

    /// Painel de detalhes. `back`: ele ocupa o lugar da lista, e o botão de fechar vira "voltar".
    fn editor(&mut self, ui: &mut egui::Ui, store: &mut Store, back: bool) {
        let Some(id) = self.selected else { return };
        let Some(pos) = store.tasks.iter().position(|t| t.id == id) else {
            self.selected = None;
            return;
        };
        self.sync_buffers(&store.tasks[pos]);
        let today = Local::now().date_naive();
        let mut changed = false;
        let mut delete = false;
        let mut close = false;
        let all_tags = store.all_tags();
        let colors = &store.tag_colors;
        let t = &mut store.tasks[pos];

        ui.horizontal(|ui| {
            if back && chrome::icon_button(ui, Glyph::Back, 0.0).on_hover_text("Voltar para a lista (Esc)").clicked() {
                close = true;
            }
            if ui.checkbox(&mut t.done, "Concluída").changed() {
                t.roll_over();
                changed = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !back && chrome::icon_button(ui, Glyph::Close, 0.0).on_hover_text("Fechar (Esc)").clicked() {
                    close = true;
                }
                if chrome::icon_button(ui, Glyph::Trash, 0.0).on_hover_text("Excluir tarefa").clicked() {
                    delete = true;
                }
                changed |= ui.color_edit_button_srgb(&mut t.color).on_hover_text("Cor da tarefa").changed();
            });
        });
        // O título ocupa a largura toda e quebra a linha, para um título longo ficar inteiro à vista. Enter só confirma.
        let width = ui.available_width();
        let title = ui.add(
            egui::TextEdit::multiline(&mut t.title)
                .font(egui::TextStyle::Heading)
                .desired_rows(1)
                .desired_width(width)
                .return_key(None::<egui::KeyboardShortcut>),
        );
        if title.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            title.surrender_focus();
        }
        if title.changed() {
            // Colar um texto com várias linhas não pode criar um título de várias linhas.
            if t.title.contains(['\n', '\r']) {
                t.title = t.title.split(['\n', '\r']).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
            }
            changed = true;
        }
        ui.separator();

        egui::ScrollArea::vertical().id_salt("detalhes_rolagem").auto_shrink([false, false]).show(ui, |ui| {
            prop(ui, "Data", |ui| {
                let date_id = ui.scope_id().with("caixa_data");
                // Vermelho só com a data inteira digitada, ou ao sair da caixa: durante a digitação ela ainda está incompleta.
                let typing = ui.memory(|m| m.has_focus(date_id)) && self.date_buf.len() < 10;
                let bad = !self.date_buf.is_empty() && !typing && parse_date(&self.date_buf).is_none();
                let r = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.date_buf)
                            .id(date_id)
                            .hint_text("dd/mm/aaaa")
                            .desired_width(92.0)
                            .text_color_opt(bad.then_some(Color32::LIGHT_RED)),
                    )
                    .on_hover_text("Digite só os números, ou escolha no calendário");
                if r.clicked() {
                    self.cal_month = Some(t.date.unwrap_or(today).with_day(1).unwrap_or(today));
                }
                if r.changed() {
                    let digits: String = self.date_buf.chars().filter(char::is_ascii_digit).take(8).collect();
                    let masked = format_date_digits(&digits);
                    if masked != self.date_buf {
                        self.date_buf = masked;
                        cursor_to_end(ui.ctx(), date_id, &self.date_buf);
                    }
                    let parsed = parse_date(&self.date_buf);
                    if parsed.is_some() || self.date_buf.is_empty() {
                        t.date = parsed;
                        t.set_anchor();
                        changed = true;
                    }
                }
                // "05/11" ou "05/11/26": ao sair da caixa, completa o ano.
                if r.lost_focus() {
                    if let Some(d) = complete_date(&self.date_buf, today) {
                        self.date_buf = d.format("%d/%m/%Y").to_string();
                        t.date = Some(d);
                        t.set_anchor();
                        changed = true;
                    }
                }
                if let Some(mut month) = self.cal_month {
                    let id = ui.scope_id().with("calendario");
                    let outcome = datepicker::show(ui.ctx(), id, r.rect, &mut month, t.date, today);
                    self.cal_month = Some(month);
                    match outcome {
                        datepicker::Outcome::Open => {}
                        datepicker::Outcome::Close => self.cal_month = None,
                        datepicker::Outcome::Clear => {
                            t.date = None;
                            self.date_buf.clear();
                            self.cal_month = None;
                            changed = true;
                        }
                        datepicker::Outcome::Pick(d) => {
                            t.date = Some(d);
                            t.set_anchor();
                            self.date_buf = d.format("%d/%m/%Y").to_string();
                            self.cal_month = None;
                            changed = true;
                        }
                    }
                }
                for (label, day) in [("Hoje", today), ("Amanhã", today + Duration::days(1))] {
                    if ui.small_button(label).clicked() {
                        t.date = Some(day);
                        t.set_anchor();
                        self.date_buf = day.format("%d/%m/%Y").to_string();
                        changed = true;
                    }
                }
            });

            prop(ui, "Hora", |ui| {
                let time_id = ui.scope_id().with("caixa_hora");
                let typing = ui.memory(|m| m.has_focus(time_id)) && self.time_buf.len() < 5;
                let bad = !self.time_buf.is_empty() && !typing && NaiveTime::parse_from_str(&self.time_buf, "%H:%M").is_err();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.time_buf)
                        .id(time_id)
                        .hint_text("HH:MM")
                        .desired_width(60.0)
                        .text_color_opt(bad.then_some(Color32::LIGHT_RED)),
                );
                if r.changed() {
                    let digits: String = self.time_buf.chars().filter(char::is_ascii_digit).take(4).collect();
                    let masked = format_time_digits(&digits);
                    if masked != self.time_buf {
                        self.time_buf = masked;
                        cursor_to_end(ui.ctx(), time_id, &self.time_buf);
                    }
                    let parsed = NaiveTime::parse_from_str(&self.time_buf, "%H:%M").ok();
                    if parsed.is_some() || self.time_buf.is_empty() {
                        t.time = parsed;
                        changed = true;
                    }
                }
            });

            prop(ui, "Aviso", |ui| {
                let label = match t.notify {
                    None => "Sem aviso".to_owned(),
                    Some(0) => "No horário".to_owned(),
                    Some(m) => format!("{m} min antes"),
                };
                egui::ComboBox::from_id_salt("notify").selected_text(label).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut t.notify, None, "Sem aviso").changed();
                    changed |= ui.selectable_value(&mut t.notify, Some(0), "No horário").changed();
                    for m in [5, 15, 30, 60] {
                        changed |= ui.selectable_value(&mut t.notify, Some(m), format!("{m} min antes")).changed();
                    }
                });
                changed |= ui
                    .checkbox(&mut t.notify_day_before, "Também no dia anterior")
                    .on_hover_text("Na mesma hora, um dia antes. Sem hora marcada, avisa às 9h.")
                    .changed();
                if (t.notify.is_some() && (t.date.is_none() || t.time.is_none())) || (t.notify_day_before && t.date.is_none()) {
                    ui.weak("defina data e hora");
                }
            });

            prop(ui, "Repetir", |ui| {
                egui::ComboBox::from_id_salt("repeat").selected_text(t.repeat.label()).show_ui(ui, |ui| {
                    for r in Repeat::ALL {
                        if ui.selectable_value(&mut t.repeat, r, r.label()).changed() {
                            t.set_anchor();
                            changed = true;
                        }
                    }
                });
                if t.repeat != Repeat::None {
                    ui.label("a cada");
                    changed |= ui.add(egui::DragValue::new(&mut t.every).range(1..=365)).changed();
                    ui.label(t.repeat.unit(t.every));
                    if t.date.is_none() {
                        ui.weak("defina a data");
                    }
                }
            });
            if t.repeat != Repeat::None {
                prop(ui, "Termina", |ui| {
                    let label = match t.repeat_times {
                        None => "Nunca".to_owned(),
                        Some(n) => format!("Após {n} {}", if n == 1 { "vez" } else { "vezes" }),
                    };
                    egui::ComboBox::from_id_salt("repeat_end").selected_text(label).show_ui(ui, |ui| {
                        changed |= ui.selectable_value(&mut t.repeat_times, None, "Nunca").changed();
                        if ui.selectable_label(t.repeat_times.is_some(), "Após um número de vezes").clicked() {
                            if t.repeat_times.is_none() {
                                t.repeat_times = Some(3);
                                changed = true;
                            }
                            ui.close();
                        }
                    });
                    if let Some(n) = &mut t.repeat_times {
                        changed |= ui.add(egui::DragValue::new(n).range(1..=999)).changed();
                        ui.weak("ocorrências, contando esta");
                    }
                });
            }

            prop(ui, "Tags", |ui| {
                tight(ui);
                let mut rm = None;
                for (i, tag) in t.tags.iter().enumerate() {
                    if tag_chip(ui, tag, tag_rgb(colors, tag), true, "  ×").clicked() {
                        rm = Some(i);
                    }
                }
                if let Some(i) = rm {
                    t.tags.remove(i);
                    changed = true;
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.tag_buf).hint_text("nova tag").desired_width(80.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let tag = self.tag_buf.trim().trim_start_matches('#').to_owned();
                    if !tag.is_empty() && !t.tags.contains(&tag) {
                        t.tags.push(tag);
                        changed = true;
                    }
                    self.tag_buf.clear();
                    r.request_focus();
                }
                // Tags que já existem em outras tarefas, para reaproveitar com um clique.
                for tag in all_tags.iter().filter(|g| !t.tags.contains(g)).cloned().collect::<Vec<_>>() {
                    if tag_chip(ui, &tag, tag_rgb(colors, &tag), false, "").clicked() {
                        t.tags.push(tag);
                        changed = true;
                    }
                }
            });

            ui.add_space(6.0);
            let (ok, total) = t.subtasks_done();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Checklist").size(12.5).color(DIM));
                if total > 0 {
                    ui.add(egui::ProgressBar::new(ok as f32 / total as f32).desired_width(110.0).text(format!("{ok}/{total}")));
                }
            });
            let mut rm = None;
            for (i, s) in t.subtasks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut s.done, "").changed();
                    changed |= ui.add(egui::TextEdit::singleline(&mut s.title).desired_width((ui.available_width() - 44.0).max(40.0))).changed();
                    if ui.small_button("×").clicked() {
                        rm = Some(i);
                    }
                });
            }
            if let Some(i) = rm {
                t.subtasks.remove(i);
                changed = true;
            }
            let r = ui.add(egui::TextEdit::singleline(&mut self.sub_buf).hint_text("Adicionar item e Enter").desired_width(f32::INFINITY));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let title = self.sub_buf.trim().to_owned();
                if !title.is_empty() {
                    t.subtasks.push(Subtask { title, done: false });
                    changed = true;
                }
                self.sub_buf.clear();
                r.request_focus();
            }

            ui.add_space(8.0);
            ui.label(RichText::new("Informações").size(12.5).color(DIM));
            changed |= ui.add(egui::TextEdit::multiline(&mut t.notes).desired_rows(5).desired_width(f32::INFINITY)).changed();
        });

        store.dirty |= changed;
        if close {
            self.selected = None;
        }
        if delete {
            let time = ui.input(|i| i.time);
            self.delete(store, id, time);
        }
    }
}

/// Depois de reformatar o texto da caixa, o cursor volta para o fim (a máscara acrescenta caracteres).
fn cursor_to_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        egui::TextEdit::store_state(ctx, id, state);
    }
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    // Exige os 10 caracteres: o chrono aceitaria "05/11/2" como o ano 2.
    if text.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(text, "%d/%m/%Y").ok()
}

/// Completa uma data curta: "05/11" fica no ano atual e "05/11/26" vira 2026.
fn complete_date(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if text.len() == 10 || digits.len() < 4 {
        return None;
    }
    let day = digits[..2].parse().ok()?;
    let month = digits[2..4].parse().ok()?;
    let year = match digits.len() {
        4 => today.year(),
        6 => 2000 + digits[4..6].parse::<i32>().ok()?,
        _ => return None,
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

/// Máscara de data: só dígitos, dia e mês dentro do possível, e as barras nos lugares certos.
fn format_date_digits(digits: &str) -> String {
    let mut d: Vec<char> = digits.chars().collect();
    // Um dia que não pode ter dois dígitos ganha o zero na frente: "5" vira "05".
    if d.first().is_some_and(|c| *c > '3') {
        d.insert(0, '0');
    }
    if d.len() >= 2 && d[0] == '3' && d[1] > '1' {
        d[1] = '1';
    }
    // O mesmo para o mês: "5" vira "05", e acima de 12 fica em 12.
    if d.len() >= 3 && d[2] > '1' {
        d.insert(2, '0');
    }
    if d.len() >= 4 && d[2] == '1' && d[3] > '2' {
        d[3] = '2';
    }
    d.truncate(8);
    let mut out = String::with_capacity(10);
    for (i, c) in d.iter().enumerate() {
        if i == 2 || i == 4 {
            out.push('/');
        }
        out.push(*c);
    }
    out
}

/// Máscara de hora 24h: só dígitos, completa o zero à esquerda e põe os dois pontos.
fn format_time_digits(digits: &str) -> String {
    let mut d: Vec<char> = digits.chars().collect();
    if d.first().is_some_and(|c| *c > '2') {
        d.insert(0, '0');
    }
    if d.len() >= 2 && (d[0] == '2' && d[1] > '3') {
        d[1] = '3';
    }
    if d.len() >= 3 && d[2] > '5' {
        d.insert(2, '0');
    }
    d.truncate(4);
    let s: String = d.iter().collect();
    if s.len() > 2 { format!("{}:{}", &s[..2], &s[2..]) } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dia(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    #[test]
    fn mascara_de_data() {
        assert_eq!(format_date_digits("0"), "0");
        assert_eq!(format_date_digits("05"), "05");
        assert_eq!(format_date_digits("051"), "05/1");
        assert_eq!(format_date_digits("0511"), "05/11");
        assert_eq!(format_date_digits("05112026"), "05/11/2026");
        assert_eq!(format_date_digits("5"), "05");
        assert_eq!(format_date_digits("0531"), "05/03/1");
        assert_eq!(format_date_digits("3911"), "31/11");
        assert_eq!(format_date_digits("0519"), "05/12");
    }

    #[test]
    fn data_curta_completa_o_ano() {
        let today = dia(11, 4);
        assert_eq!(complete_date("05/11", today), NaiveDate::from_ymd_opt(2026, 11, 5));
        assert_eq!(complete_date("05/11/27", today), NaiveDate::from_ymd_opt(2027, 11, 5));
        assert_eq!(complete_date("05/1", today), None);
        assert_eq!(complete_date("31/02", today), None);
        assert_eq!(parse_date("05/11/2"), None);
        assert_eq!(parse_date("05/11/2026"), NaiveDate::from_ymd_opt(2026, 11, 5));
    }

    #[test]
    fn nova_tarefa_entende_data_hora_e_tags() {
        // 05/11/2026 é uma quinta-feira.
        let today = dia(11, 5);
        let q = parse_quick("Reunião amanhã 14h #trabalho", today);
        assert_eq!(q.title, "Reunião");
        assert_eq!(q.date, Some(dia(11, 6)));
        assert_eq!(q.time, NaiveTime::from_hms_opt(14, 0, 0));
        assert_eq!(q.tags, ["trabalho"]);

        let q = parse_quick("Dentista dia 20/11 às 9h30", today);
        assert_eq!((q.title.as_str(), q.date), ("Dentista", Some(dia(11, 20))));
        assert_eq!(q.time, NaiveTime::from_hms_opt(9, 30, 0));

        assert_eq!(parse_quick("Prova sexta-feira", today).date, Some(dia(11, 6)));
        assert_eq!(parse_quick("Prova quinta", today).date, Some(dia(11, 12)));
        assert_eq!(parse_quick("Viagem depois de amanhã", today).date, Some(dia(11, 7)));
        assert_eq!(parse_quick("Viagem depois de amanhã", today).title, "Viagem");
        // Já passou neste ano: fica para o ano que vem.
        assert_eq!(parse_quick("Natal 01/02", today).date, NaiveDate::from_ymd_opt(2027, 2, 1));
    }

    #[test]
    fn nova_tarefa_nao_inventa_data_no_meio_do_titulo() {
        let today = dia(11, 5);
        let q = parse_quick("Ler o jornal de hoje cedo", today);
        assert_eq!((q.title.as_str(), q.date), ("Ler o jornal de hoje cedo", None));
        assert_eq!(parse_quick("hoje", today), Quick { title: "hoje".to_owned(), ..Default::default() });
        assert_eq!(parse_quick("Comprar 3 itens", today).time, None);
        assert_eq!(parse_quick("Sala 25:00", today).time, None);
        assert_eq!(parse_quick("Fração 31/02", today).date, None);
    }

    #[test]
    fn secoes_seguem_a_data() {
        let today = dia(11, 5);
        let now = today.and_hms_opt(12, 0, 0).unwrap();
        let mut store = Store::default();
        let mut with_date = |d: Option<NaiveDate>| {
            let id = store.add("x".into(), d);
            store.tasks.iter().find(|t| t.id == id).unwrap().clone()
        };
        assert_eq!(group_of(&with_date(Some(dia(11, 4))), today, now), Group::Atrasadas);
        assert_eq!(group_of(&with_date(Some(today)), today, now), Group::Hoje);
        assert_eq!(group_of(&with_date(Some(dia(11, 6))), today, now), Group::Amanha);
        assert_eq!(group_of(&with_date(Some(dia(11, 12))), today, now), Group::Semana);
        assert_eq!(group_of(&with_date(Some(dia(11, 13))), today, now), Group::Depois);
        let none = with_date(None);
        assert_eq!(group_of(&none, today, now), Group::SemData);
        assert!(in_filter(Filter::Todas, Group::SemData, &none, today));
        assert!(!in_filter(Filter::Hoje, Group::SemData, &none, today));
        assert!(!in_filter(Filter::Proximos, Group::Atrasadas, &none, today));
    }

    #[test]
    fn disposicao_acompanha_a_largura() {
        // Larga: tudo lado a lado, e a soma nunca passa da largura.
        let wide = arrange(1200.0, true, DETAIL_START_W);
        assert!(wide.sidebar && wide.list);
        assert!(SIDEBAR_W + GAP + LIST_MIN_W + GAP + wide.detail_w <= 1200.0);
        // Sem detalhes abertos, o menu lateral fica até onde cabe ao lado da lista.
        assert!(arrange(SIDEBAR_W + GAP + LIST_MIN_W, false, 0.0).sidebar);
        assert!(!arrange(SIDEBAR_W + GAP + LIST_MIN_W - 1.0, false, 0.0).sidebar);
        // Média: o menu sai primeiro e os detalhes encolhem até o mínimo.
        let medium = arrange(700.0, true, DETAIL_START_W);
        assert!(!medium.sidebar && medium.list);
        assert!(LIST_MIN_W + GAP + medium.detail_w <= 700.0);
        // Estreita: os detalhes tomam o lugar da lista.
        let narrow = arrange(480.0, true, DETAIL_START_W);
        assert_eq!(narrow, Arrange { sidebar: false, list: false, detail_w: 480.0 });
        assert!(arrange(480.0, false, DETAIL_START_W).list);
    }
}
