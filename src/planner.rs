use chrono::{Datelike, Duration, Local, Months, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, Sense, vec2};

use crate::model::Store;
use crate::splash::MESES;
use crate::theme::ACCENT;

const DIAS_CURTOS: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Dia,
    Semana,
    Mes,
}

pub struct Planner {
    mode: Mode,
    anchor: NaiveDate,
    week_starts_sunday: bool,
    show_done: bool,
    new_title: String,
}

impl Planner {
    pub fn new() -> Self {
        Self {
            mode: Mode::Mes,
            anchor: Local::now().date_naive(),
            week_starts_sunday: false,
            show_done: true,
            new_title: String::new(),
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

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("‹").clicked() {
                self.step(-1);
            }
            if ui.button("Hoje").clicked() {
                self.anchor = Local::now().date_naive();
            }
            if ui.button("›").clicked() {
                self.step(1);
            }
            ui.label(egui::RichText::new(self.title()).heading());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Opções", |ui| {
                    ui.checkbox(&mut self.week_starts_sunday, "Semana começa no domingo");
                    ui.checkbox(&mut self.show_done, "Mostrar concluídas");
                });
                ui.selectable_value(&mut self.mode, Mode::Mes, "Mês");
                ui.selectable_value(&mut self.mode, Mode::Semana, "Semana");
                ui.selectable_value(&mut self.mode, Mode::Dia, "Dia");
            });
        });
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

    fn month(&mut self, ui: &mut egui::Ui, store: &Store) -> Option<u64> {
        let first = self.anchor.with_day(1).unwrap();
        let next = first.checked_add_months(Months::new(1)).unwrap();
        let days = (next - first).num_days();
        let offset = self.col_index(first);
        let rows = ((offset + days + 6) / 7) as usize;
        let grid_start = first - Duration::days(offset);
        let today = Local::now().date_naive();

        let order: [usize; 7] = if self.week_starts_sunday { [6, 0, 1, 2, 3, 4, 5] } else { [0, 1, 2, 3, 4, 5, 6] };
        let avail = ui.available_size();
        let head_h = 22.0;
        let cell_w = avail.x / 7.0;
        let cell_h = ((avail.y - head_h) / rows as f32).max(70.0);
        let (area, _) = ui.allocate_exact_size(vec2(avail.x, head_h + cell_h * rows as f32), Sense::hover());
        let painter = ui.painter_at(area);

        for (c, &name) in order.iter().enumerate() {
            let r = egui::Rect::from_min_size(area.min + vec2(c as f32 * cell_w, 0.0), vec2(cell_w, head_h));
            painter.text(r.center(), Align2::CENTER_CENTER, DIAS_CURTOS[name], FontId::proportional(13.0), Color32::GRAY);
        }

        let mut open = None;
        let mut goto_day = None;
        for row in 0..rows {
            for col in 0..7 {
                let d = grid_start + Duration::days((row * 7 + col) as i64);
                let rect = egui::Rect::from_min_size(
                    area.min + vec2(col as f32 * cell_w, head_h + row as f32 * cell_h),
                    vec2(cell_w, cell_h),
                )
                .shrink(1.5);
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
                    let r = ui.interact(chip, ui.scope_id().with(("chip", t.id)), Sense::click());
                    let fill = if t.done { Color32::from_gray(50) } else { base.gamma_multiply(if r.hovered() { 0.7 } else { 0.45 }) };
                    painter.rect_filled(chip, 3.0, fill);
                    let label = match t.time {
                        Some(h) => format!("{} {}", h.format("%H:%M"), t.title),
                        None => t.title.clone(),
                    };
                    painter.with_clip_rect(chip).text(
                        chip.left_center() + vec2(4.0, 0.0),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(11.0),
                        if t.done { Color32::GRAY } else { Color32::WHITE },
                    );
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
        ui.columns(7, |cols| {
            for (c, col) in cols.iter_mut().enumerate() {
                let d = start + Duration::days(c as i64);
                let name = DIAS_CURTOS[d.weekday().num_days_from_monday() as usize];
                let head = egui::RichText::new(format!("{name} {}", d.day())).strong();
                col.label(if d == today { head.color(ACCENT) } else { head });
                col.separator();
                egui::ScrollArea::vertical().id_salt(("semana", c)).show(col, |ui| {
                    self.cards(ui, store, d, &mut open, &mut dirty);
                });
            }
        });
        store.dirty |= dirty;
        open
    }

    fn day(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<u64> {
        let mut open = None;
        let mut dirty = false;
        let d = self.anchor;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.new_title)
                    .hint_text("Nova tarefa para este dia...")
                    .desired_width(ui.available_width() - 70.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Criar").clicked() || enter) && !self.new_title.trim().is_empty() {
                store.add(self.new_title.trim().to_owned(), Some(d));
                self.new_title.clear();
                r.request_focus();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.tasks_on(store, d).is_empty() {
                ui.weak("Nenhuma tarefa neste dia.");
            }
            self.cards(ui, store, d, &mut open, &mut dirty);
        });
        store.dirty |= dirty;
        open
    }

    fn cards(&self, ui: &mut egui::Ui, store: &mut Store, d: NaiveDate, open: &mut Option<u64>, dirty: &mut bool) {
        for i in self.tasks_on(store, d) {
            let t = &mut store.tasks[i];
            let [r, g, b] = t.color;
            egui::Frame::new()
                .fill(Color32::from_rgb(r, g, b).gamma_multiply(if t.done { 0.12 } else { 0.28 }))
                .corner_radius(5.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        *dirty |= ui.checkbox(&mut t.done, "").changed();
                        let mut text = egui::RichText::new(&t.title);
                        if t.done {
                            text = text.strikethrough().weak();
                        }
                        if ui.link(text).clicked() {
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
                    for tag in &t.tags {
                        meta.push_str(&format!("  #{tag}"));
                    }
                    if !meta.is_empty() {
                        ui.weak(meta.trim());
                    }
                });
            ui.add_space(3.0);
        }
    }
}
