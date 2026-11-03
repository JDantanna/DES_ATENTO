use chrono::{Local, NaiveDate, NaiveTime};
use eframe::egui::{self, Color32, RichText};

use crate::ics;
use crate::model::{Store, Subtask, Task};

#[derive(PartialEq, Clone, Copy)]
enum Filter {
    Hoje,
    Pendentes,
    Todas,
    Concluidas,
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
        }
    }

    /// Seleciona uma tarefa vinda de outra aba, garantindo que apareça na lista.
    pub fn open(&mut self, id: u64) {
        self.selected = Some(id);
        self.filter = Filter::Todas;
        self.tag_filter = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        egui::Panel::left("lista")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| self.list(ui, store));
        egui::CentralPanel::default().show(ui, |ui| self.editor(ui, store));
    }

    fn list(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        let today = Local::now().date_naive();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.new_title)
                    .hint_text("Nova tarefa...")
                    .desired_width(ui.available_width() - 70.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Criar").clicked() || enter) && !self.new_title.trim().is_empty() {
                let title = self.new_title.trim().to_owned();
                let date = (self.filter == Filter::Hoje).then_some(today);
                self.selected = Some(store.add(title, date));
                self.new_title.clear();
                r.request_focus();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.filter, Filter::Hoje, "Hoje");
            ui.selectable_value(&mut self.filter, Filter::Pendentes, "Pendentes");
            ui.selectable_value(&mut self.filter, Filter::Todas, "Todas");
            ui.selectable_value(&mut self.filter, Filter::Concluidas, "Concluídas");
            ui.menu_button("⇅ Calendário", |ui| self.ics_menu(ui, store));
        });
        if let Some(m) = &self.msg {
            ui.weak(m);
        }
        let tags = store.all_tags();
        if !tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for t in &tags {
                    let on = self.tag_filter.as_ref() == Some(t);
                    if ui.selectable_label(on, format!("#{t}")).clicked() {
                        self.tag_filter = if on { None } else { Some(t.clone()) };
                    }
                }
            });
        }
        ui.separator();

        let mut idx: Vec<usize> = store
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| match self.filter {
                Filter::Hoje => t.date == Some(today),
                Filter::Pendentes => !t.done,
                Filter::Todas => true,
                Filter::Concluidas => t.done,
            })
            .filter(|(_, t)| self.tag_filter.as_ref().is_none_or(|f| t.tags.contains(f)))
            .map(|(i, _)| i)
            .collect();
        idx.sort_by_key(|&i| {
            let t = &store.tasks[i];
            (t.done, t.date, t.time, t.id)
        });

        let mut dirty = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if idx.is_empty() {
                ui.weak("Nada por aqui.");
            }
            for i in idx {
                let t = &mut store.tasks[i];
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut t.done, "").changed() {
                        dirty = true;
                    }
                    let [r, g, b] = t.color;
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4.5, Color32::from_rgb(r, g, b));
                    let mut title = RichText::new(&t.title);
                    if t.done {
                        title = title.strikethrough().weak();
                    }
                    if ui.selectable_label(self.selected == Some(t.id), title).clicked() {
                        self.selected = Some(t.id);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut info = String::new();
                        if let Some(h) = t.time {
                            info.push_str(&h.format("%H:%M ").to_string());
                        }
                        if let (Some(d), true) = (t.date, self.filter != Filter::Hoje) {
                            info.push_str(&d.format("%d/%m").to_string());
                        }
                        let (ok, total) = t.subtasks_done();
                        if total > 0 {
                            info.push_str(&format!(" ☑{ok}/{total}"));
                        }
                        if !info.is_empty() {
                            ui.weak(info.trim());
                        }
                    });
                });
            }
        });
        store.dirty |= dirty;
    }

    fn ics_menu(&mut self, ui: &mut egui::Ui, store: &mut Store) {
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

    fn sync_buffers(&mut self, t: &Task) {
        if self.buf_for != Some(t.id) {
            self.buf_for = Some(t.id);
            self.date_buf = t.date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default();
            self.time_buf = t.time.map(|h| h.format("%H:%M").to_string()).unwrap_or_default();
            self.tag_buf.clear();
            self.sub_buf.clear();
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        let Some(id) = self.selected else {
            ui.weak("Selecione ou crie uma tarefa.");
            return;
        };
        let Some(pos) = store.tasks.iter().position(|t| t.id == id) else {
            self.selected = None;
            return;
        };
        self.sync_buffers(&store.tasks[pos]);
        let today = Local::now().date_naive();
        let mut changed = false;
        let mut delete = false;
        let t = &mut store.tasks[pos];

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut t.done, "").changed();
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut t.title)
                            .font(egui::TextStyle::Heading)
                            .desired_width(ui.available_width() - 90.0),
                    )
                    .changed();
                changed |= ui.color_edit_button_srgb(&mut t.color).changed();
                if ui.button("🗑").on_hover_text("Excluir tarefa").clicked() {
                    delete = true;
                }
            });
            ui.add_space(8.0);

            ui.label("Data e hora");
            ui.horizontal(|ui| {
                let parsed = NaiveDate::parse_from_str(self.date_buf.trim(), "%d/%m/%Y").ok();
                let bad = !self.date_buf.trim().is_empty() && parsed.is_none();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.date_buf)
                        .hint_text("dd/mm/aaaa")
                        .desired_width(100.0)
                        .text_color_opt(bad.then_some(Color32::LIGHT_RED)),
                );
                if r.changed() && (parsed.is_some() || self.date_buf.trim().is_empty()) {
                    t.date = parsed;
                    changed = true;
                }
                if ui.small_button("Hoje").clicked() {
                    t.date = Some(today);
                    self.date_buf = today.format("%d/%m/%Y").to_string();
                    changed = true;
                }
                let bad = !self.time_buf.is_empty() && NaiveTime::parse_from_str(&self.time_buf, "%H:%M").is_err();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.time_buf)
                        .hint_text("HH:MM (24h)")
                        .desired_width(90.0)
                        .text_color_opt(bad.then_some(Color32::LIGHT_RED)),
                );
                if r.changed() {
                    let digits: String = self.time_buf.chars().filter(char::is_ascii_digit).take(4).collect();
                    self.time_buf = format_time_digits(&digits);
                    let parsed = NaiveTime::parse_from_str(&self.time_buf, "%H:%M").ok();
                    if parsed.is_some() || self.time_buf.is_empty() {
                        t.time = parsed;
                        changed = true;
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Aviso");
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
                if t.notify.is_some() && (t.date.is_none() || t.time.is_none()) {
                    ui.weak("defina data e hora");
                }
            });

            ui.add_space(8.0);
            ui.label("Tags");
            ui.horizontal_wrapped(|ui| {
                let mut rm = None;
                for (i, tag) in t.tags.iter().enumerate() {
                    if ui.small_button(format!("#{tag}  ✕")).clicked() {
                        rm = Some(i);
                    }
                }
                if let Some(i) = rm {
                    t.tags.remove(i);
                    changed = true;
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.tag_buf).hint_text("nova tag").desired_width(90.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let tag = self.tag_buf.trim().trim_start_matches('#').to_owned();
                    if !tag.is_empty() && !t.tags.contains(&tag) {
                        t.tags.push(tag);
                        changed = true;
                    }
                    self.tag_buf.clear();
                    r.request_focus();
                }
            });

            ui.add_space(8.0);
            let (ok, total) = t.subtasks_done();
            ui.horizontal(|ui| {
                ui.label("Checklist");
                if total > 0 {
                    ui.add(egui::ProgressBar::new(ok as f32 / total as f32).desired_width(120.0).text(format!("{ok}/{total}")));
                }
            });
            let mut rm = None;
            for (i, s) in t.subtasks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut s.done, "").changed();
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut s.title).desired_width(ui.available_width() - 30.0))
                        .changed();
                    if ui.small_button("✕").clicked() {
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
            ui.label("Informações");
            changed |= ui
                .add(egui::TextEdit::multiline(&mut t.notes).desired_rows(6).desired_width(f32::INFINITY))
                .changed();
        });

        store.dirty |= changed;
        if delete {
            store.remove(id);
            self.selected = None;
            self.buf_for = None;
        }
    }
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