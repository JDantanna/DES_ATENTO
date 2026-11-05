use chrono::{Datelike, Duration, Local, Months, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, Sense, vec2};

use crate::chrome;
use crate::model::{Store, tag_rgb};
use crate::tasks::{OVERDUE, OVERDUE_TEXT};
use crate::splash::MESES;
use crate::theme::ACCENT;

const DIAS_CURTOS: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
/// Abaixo desta largura, a barra do calendário usa duas linhas.
const TOOLBAR_ONE_LINE_W: f32 = 620.0;
/// Com colunas mais estreitas que isto, a semana vira uma lista de dias (as tarefas ficariam ilegíveis).
const WEEK_MIN_COL_W: f32 = 112.0;
/// Altura mínima de uma célula do mês; com menos espaço, a grade rola.
const MONTH_MIN_CELL_H: f32 = 40.0;

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Mode {
    Dia,
    Semana,
    Mes,
}

pub struct Planner {
    pub(crate) mode: Mode,
    anchor: NaiveDate,
    week_starts_sunday: bool,
    show_done: bool,
    new_title: String,
    /// Tarefa sendo arrastada e o dia de onde saiu.
    drag: Option<(u64, NaiveDate)>,
}

impl Planner {
    pub fn new() -> Self {
        Self {
            mode: Mode::Mes,
            anchor: Local::now().date_naive(),
            week_starts_sunday: false,
            show_done: true,
            new_title: String::new(),
            drag: None,
        }
    }

    /// Retorna o id de uma tarefa se o usuário clicou nela para abrir.
    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<u64> {
        let mut open = None;
        egui::Panel::top("plan_bar").show(ui, |ui| self.toolbar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.mode {
            Mode::Mes => open = self.month(ui, store),
            Mode::Semana => open = self.week(ui, store),
            Mode::Dia => open = self.day(ui, store),
        });
        open
    }

    fn col_index(&self, d: NaiveDate) -> i64 {
        if self.week_starts_sunday {
            d.weekday().num_days_from_sunday() as i64
        } else {
            d.weekday().num_days_from_monday() as i64
        }
    }

    fn week_start(&self) -> NaiveDate {
        self.anchor - Duration::days(self.col_index(self.anchor))
    }

    fn step(&mut self, dir: i32) {
        self.anchor = match self.mode {
            Mode::Dia => self.anchor + Duration::days(dir as i64),
            Mode::Semana => self.anchor + Duration::days(7 * dir as i64),
            Mode::Mes => {
                if dir > 0 {
                    self.anchor.checked_add_months(Months::new(1))
                } else {
                    self.anchor.checked_sub_months(Months::new(1))
                }
                .unwrap_or(self.anchor)
            }
        };
    }

    fn title(&self) -> String {
        match self.mode {
            Mode::Mes => format!("{} de {}", MESES[self.anchor.month0() as usize], self.anchor.year()),
            Mode::Semana => {
                let a = self.week_start();
                let b = a + Duration::days(6);
                format!("{} – {}", a.format("%d/%m"), b.format("%d/%m/%Y"))
            }
            Mode::Dia => {
                let d = self.anchor;
                format!(
                    "{}, {:02} de {}",
                    DIAS_CURTOS[d.weekday().num_days_from_monday() as usize],
                    d.day(),
                    MESES[d.month0() as usize]
                )
            }
        }
    }

    /// Avançar, voltar, ir para hoje e o período mostrado.
    fn nav(&mut self, ui: &mut egui::Ui) {
        if ui.button("‹").clicked() {
            self.step(-1);
        }
        if ui.button("Hoje").clicked() {
            self.anchor = Local::now().date_naive();
        }
        if ui.button("›").clicked() {
            self.step(1);
        }
        ui.add(egui::Label::new(egui::RichText::new(self.title()).heading()).selectable(false).truncate());
    }

    /// Opções e a escolha entre dia, semana e mês, da direita para a esquerda.
    fn modes(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Opções", |ui| {
                ui.checkbox(&mut self.week_starts_sunday, "Semana começa no domingo");
                ui.checkbox(&mut self.show_done, "Mostrar concluídas");
            });
            ui.selectable_value(&mut self.mode, Mode::Mes, "Mês");
            ui.selectable_value(&mut self.mode, Mode::Semana, "Semana");
            ui.selectable_value(&mut self.mode, Mode::Dia, "Dia");
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        if ui.available_width() >= TOOLBAR_ONE_LINE_W {
            ui.horizontal(|ui| {
                self.nav(ui);
                self.modes(ui);
            });
        } else {
            ui.horizontal(|ui| self.nav(ui));
            ui.horizontal(|ui| self.modes(ui));
        }
        ui.add_space(4.0);
    }

    fn tasks_on(&self, store: &Store, d: NaiveDate) -> Vec<usize> {
        let mut v: Vec<usize> = store
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.occurs_on(d) && (self.show_done || !t.done))
            .map(|(i, _)| i)
            .collect();
        v.sort_by_key(|&i| (store.tasks[i].done, store.tasks[i].time, store.tasks[i].id));
        v
    }

    /// O mês ocupa toda a área; numa janela muito baixa, a grade rola em vez de sair da tela.
    fn month(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<u64> {
        let avail = ui.available_size();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.month_grid(ui, store, avail)).inner
    }

    fn month_grid(&mut self, ui: &mut egui::Ui, store: &mut Store, avail: egui::Vec2) -> Option<u64> {
        let first = self.anchor.with_day(1).unwrap();
        let next = first.checked_add_months(Months::new(1)).unwrap();
        let days = (next - first).num_days();
        let offset = self.col_index(first);
        let rows = ((offset + days + 6) / 7) as usize;
        let grid_start = first - Duration::days(offset);
        let today = Local::now().date_naive();
        let now = Local::now().naive_local();

        let order: [usize; 7] = if self.week_starts_sunday { [6, 0, 1, 2, 3, 4, 5] } else { [0, 1, 2, 3, 4, 5, 6] };
        let head_h = 22.0;
        let cell_w = avail.x / 7.0;
        let cell_h = ((avail.y - head_h) / rows as f32).max(MONTH_MIN_CELL_H);
        let (area, _) = ui.allocate_exact_size(vec2(avail.x, head_h + cell_h * rows as f32), Sense::hover());
        let painter = ui.painter_at(area);

        for (c, &name) in order.iter().enumerate() {
            let r = egui::Rect::from_min_size(area.min + vec2(c as f32 * cell_w, 0.0), vec2(cell_w, head_h));
            painter.text(r.center(), Align2::CENTER_CENTER, DIAS_CURTOS[name], FontId::proportional(13.0), Color32::GRAY);
        }

        let mut open = None;
        let mut goto_day = None;
        let mut cells = Vec::with_capacity(rows * 7);
        for row in 0..rows {
            for col in 0..7 {
                let d = grid_start + Duration::days((row * 7 + col) as i64);
                let rect = egui::Rect::from_min_size(
                    area.min + vec2(col as f32 * cell_w, head_h + row as f32 * cell_h),
                    vec2(cell_w, cell_h),
                )
                .shrink(1.5);
                cells.push((rect, d));
                let in_month = d.month() == first.month();
                let bg = if d == today {
                    ACCENT.gamma_multiply(0.18)
                } else if in_month {
                    Color32::from_rgb(28, 30, 40)
                } else {
                    Color32::from_rgb(22, 24, 32)
                };
                painter.rect_filled(rect, 4.0, bg);

                let num_rect = egui::Rect::from_min_size(rect.min, vec2(rect.width(), 20.0));
                let resp = ui.interact(num_rect, ui.scope_id().with(("dia", d)), Sense::click());
                if resp.clicked() {
                    goto_day = Some(d);
                }
                let num_color = if d == today {
                    ACCENT
                } else if in_month {
                    Color32::LIGHT_GRAY
                } else {
                    Color32::DARK_GRAY
                };
                painter.text(
                    num_rect.left_center() + vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    d.day().to_string(),
                    FontId::proportional(13.0),
                    if resp.hovered() { Color32::WHITE } else { num_color },
                );

                let list = self.tasks_on(store, d);
                let fit = (((rect.height() - 22.0) / 17.0) as usize).max(1);
                let shown = if list.len() > fit { fit - 1 } else { list.len() };
                for (k, &i) in list.iter().take(shown).enumerate() {
                    let t = &store.tasks[i];
                    let chip = egui::Rect::from_min_size(
                        rect.min + vec2(3.0, 22.0 + k as f32 * 17.0),
                        vec2(rect.width() - 6.0, 15.0),
                    );
                    let [r, g, b] = t.color;
                    let base = Color32::from_rgb(r, g, b);
                    let r = ui.interact(chip, ui.scope_id().with(("chip", t.id, d)), Sense::click_and_drag());
                    if r.drag_started() {
                        self.drag = Some((t.id, d));
                    }
                    let lifted = self.drag.is_some_and(|(id, from)| id == t.id && from == d);
                    // Atrasada: chip vermelho cheio e texto sublinhado.
                    let overdue = !lifted && t.overdue(now);
                    let fill = if t.done || lifted {
                        Color32::from_gray(50)
                    } else if overdue {
                        OVERDUE.gamma_multiply(if r.hovered() { 1.0 } else { 0.85 })
                    } else {
                        base.gamma_multiply(if r.hovered() { 0.7 } else { 0.45 })
                    };
                    painter.rect_filled(chip, 3.0, fill);
                    let label = match t.time {
                        Some(h) => format!("{} {}", h.format("%H:%M"), t.title),
                        None => t.title.clone(),
                    };
                    let color = if t.done { Color32::GRAY } else { Color32::WHITE };
                    let galley = painter.layout_no_wrap(label, FontId::proportional(11.0), color);
                    let at = chip.left_center() + vec2(4.0, -galley.size().y / 2.0);
                    let clipped = painter.with_clip_rect(chip);
                    if overdue {
                        let y = at.y + galley.size().y;
                        clipped.line_segment([egui::pos2(at.x, y), egui::pos2(at.x + galley.size().x, y)], egui::Stroke::new(1.0, color));
                    }
                    clipped.galley(at, galley, color);
                    if r.clicked() {
                        open = Some(t.id);
                    }
                }
                if list.len() > shown {
                    painter.text(
                        rect.min + vec2(6.0, 22.0 + shown as f32 * 17.0 + 7.0),
                        Align2::LEFT_CENTER,
                        format!("+{} mais", list.len() - shown),
                        FontId::proportional(11.0),
                        Color32::GRAY,
                    );
                }
            }
        }
        self.finish_drag(ui, store, &cells);
        if let Some(d) = goto_day {
            self.anchor = d;
            self.mode = Mode::Dia;
        }
        open
    }

    fn week(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<u64> {
        let start = self.week_start();
        let today = Local::now().date_naive();
        let mut open = None;
        let mut dirty = false;
        let mut cells = Vec::with_capacity(7);
        let area = ui.available_rect_before_wrap();
        let col_w = area.width() / 7.0;
        let day_head = |d: NaiveDate| {
            let name = DIAS_CURTOS[d.weekday().num_days_from_monday() as usize];
            let head = egui::RichText::new(format!("{name} {}", d.day())).strong();
            egui::Label::new(if d == today { head.color(ACCENT) } else { head }).selectable(false).truncate()
        };
        if col_w >= WEEK_MIN_COL_W {
            // Cada dia tem sua coluna, com limite próprio: nada passa para a coluna ao lado.
            for c in 0..7 {
                let d = start + Duration::days(c as i64);
                let col = egui::Rect::from_min_size(area.min + vec2(c as f32 * col_w, 0.0), vec2(col_w, area.height())).shrink2(vec2(2.0, 0.0));
                cells.push((col, d));
                let bg = if d == today { ACCENT.gamma_multiply(0.10) } else { Color32::from_rgb(23, 26, 38) };
                ui.painter().rect_filled(col, 8.0, bg);
                chrome::region(ui, col.shrink(6.0), ("coluna", c), |ui| {
                    ui.add(day_head(d));
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt(("semana", c)).auto_shrink([false, false]).show(ui, |ui| {
                        self.cards(ui, store, d, true, &mut open, &mut dirty);
                    });
                });
            }
            ui.advance_cursor_after_rect(area);
        } else {
            // Sem largura para sete colunas: os dias ficam um embaixo do outro.
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for c in 0..7 {
                    let d = start + Duration::days(c as i64);
                    let day = ui.scope(|ui| {
                        ui.set_width(ui.available_width());
                        ui.add(day_head(d));
                        if self.tasks_on(store, d).is_empty() {
                            ui.weak("Nenhuma tarefa");
                        }
                        self.cards(ui, store, d, true, &mut open, &mut dirty);
                    });
                    cells.push((day.response.rect, d));
                    ui.separator();
                }
            });
        }
        store.dirty |= dirty;
        self.finish_drag(ui, store, &cells);
        open
    }

    /// Acompanha o arrasto de uma tarefa: mostra o que está sendo levado, destaca o dia sob o mouse
    /// e, ao soltar, muda a data (uma tarefa recorrente leva a série inteira junto).
    fn finish_drag(&mut self, ui: &egui::Ui, store: &mut Store, cells: &[(egui::Rect, NaiveDate)]) {
        let Some((id, from)) = self.drag else { return };
        let ctx = ui.ctx();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.drag = None;
            return;
        }
        let pos = ctx.pointer_latest_pos();
        let target = pos.and_then(|p| cells.iter().find(|(r, _)| r.contains(p)).copied());
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some((r, _)) = target {
            ui.painter().rect_stroke(r.shrink(1.0), 4.0, egui::Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
        }
        if let (Some(p), Some(t)) = (pos, store.tasks.iter().find(|t| t.id == id)) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("navi_drag")));
            let galley = painter.layout_no_wrap(t.title.clone(), FontId::proportional(12.0), Color32::WHITE);
            let [r, g, b] = t.color;
            let rect = egui::Rect::from_min_size(p + vec2(12.0, 8.0), galley.size() + vec2(14.0, 8.0));
            painter.rect_filled(rect, 4.0, Color32::from_rgb(r, g, b).gamma_multiply(0.9));
            painter.galley(rect.min + vec2(7.0, 4.0), galley, Color32::WHITE);
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.drag = None;
            if let Some((_, to)) = target.filter(|(_, to)| *to != from) {
                if let Some(t) = store.tasks.iter_mut().find(|t| t.id == id) {
                    t.date = t.date.map(|d| d + (to - from));
                    t.set_anchor();
                    store.dirty = true;
                }
            }
        }
    }

    /// Cancela um arrasto em andamento (ao sair da aba).
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    fn day(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<u64> {
        let mut open = None;
        let mut dirty = false;
        let d = self.anchor;
        ui.horizontal(|ui| {
            let width = crate::tasks::NEW_TASK_W.min(ui.available_width() - 70.0).max(80.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.new_title).hint_text("Nova tarefa para este dia...").desired_width(width));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Criar").clicked() || enter) && !self.new_title.trim().is_empty() {
                store.add(self.new_title.trim().to_owned(), Some(d));
                self.new_title.clear();
                r.request_focus();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.tasks_on(store, d).is_empty() {
                ui.weak("Nenhuma tarefa neste dia.");
            }
            self.cards(ui, store, d, false, &mut open, &mut dirty);
        });
        store.dirty |= dirty;
        open
    }

    /// Cartões das tarefas de um dia; com `can_drag`, o título deles pode ser arrastado para outro dia.
    fn cards(&mut self, ui: &mut egui::Ui, store: &mut Store, d: NaiveDate, can_drag: bool, open: &mut Option<u64>, dirty: &mut bool) {
        let colors = &store.tag_colors;
        let now = Local::now().naive_local();
        for i in self.tasks_on(store, d) {
            let t = &mut store.tasks[i];
            let [r, g, b] = t.color;
            let overdue = t.overdue(now);
            let fill = if overdue { OVERDUE.gamma_multiply(0.5) } else { Color32::from_rgb(r, g, b).gamma_multiply(if t.done { 0.12 } else { 0.28 }) };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(5.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        // Como na lista de tarefas: concluir uma recorrente a reagenda para a próxima data.
                        if ui.checkbox(&mut t.done, "").changed() {
                            t.roll_over();
                            *dirty = true;
                        }
                        let mut text = egui::RichText::new(&t.title);
                        if t.done {
                            text = text.strikethrough().weak();
                        } else if overdue {
                            text = text.underline().strong().color(OVERDUE_TEXT);
                        }
                        let sense = if can_drag { Sense::click_and_drag() } else { Sense::click() };
                        // Título longo quebra a linha dentro do cartão, sem invadir a coluna ao lado.
                        let title = ui.add(egui::Label::new(text).selectable(false).sense(sense).wrap());
                        let title = title.on_hover_cursor(if can_drag { egui::CursorIcon::Grab } else { egui::CursorIcon::PointingHand });
                        if title.drag_started() {
                            self.drag = Some((t.id, d));
                        }
                        if title.clicked() {
                            *open = Some(t.id);
                        }
                    });
                    let mut meta = String::new();
                    if let Some(h) = t.time {
                        meta.push_str(&h.format("%H:%M").to_string());
                    }
                    let (ok, total) = t.subtasks_done();
                    if total > 0 {
                        meta.push_str(&format!("  {ok}/{total}"));
                    }
                    if !meta.is_empty() || !t.tags.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if !meta.is_empty() {
                                ui.label(egui::RichText::new(meta.trim()).small().color(Color32::from_gray(170)));
                            }
                            for tag in &t.tags {
                                let [r, g, b] = tag_rgb(colors, tag);
                                ui.label(egui::RichText::new(format!("#{tag}")).small().color(Color32::from_rgb(r, g, b)));
                            }
                        });
                    }
                });
            ui.add_space(3.0);
        }
    }
}
