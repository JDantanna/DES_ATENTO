use std::collections::HashMap;

use chrono::{Local, NaiveDate, NaiveTime};
use eframe::egui::{self, Color32, RichText};

use crate::ics;
use crate::chrome::{self, Glyph};
use crate::model::{Repeat, Store, Subtask, Task, tag_rgb};
use crate::theme::ACCENT;

const ROW_H: f32 = 42.0;
/// Largura do campo de nova tarefa (basta para o título; o resto vai no editor).
pub const NEW_TASK_W: f32 = 200.0;
/// Cinza dos detalhes de cada tarefa.
const DIM: Color32 = Color32::from_gray(140);

/// Etiqueta colorida de uma tag; `on` a destaca (filtro ativo).
fn tag_chip(ui: &mut egui::Ui, tag: &str, rgb: [u8; 3], on: bool, suffix: &str) -> egui::Response {
    let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let text = RichText::new(format!("#{tag}{suffix}")).size(11.0).color(Color32::WHITE);
    let fill = c.gamma_multiply(if on { 0.75 } else { 0.28 });
    // Compacta: pouco respiro interno e altura mínima baixa, mas ainda legível.
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(5.0, 0.0);
        ui.add(
            egui::Button::new(text)
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, c.gamma_multiply(0.85)))
                .corner_radius(8)
                .min_size(egui::vec2(0.0, 16.0)),
        )
    })
    .inner
}

/// Linhas de tags lado a lado, com pouco espaço entre elas.
fn tight(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
}

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
    tags_open: bool,
    /// Áreas dos botões de tag no quadro atual (um clique fora delas desfaz o filtro).
    chip_rects: Vec<egui::Rect>,
    /// Texto em edição de cada tag no painel de tags.
    tag_edit: HashMap<String, String>,
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
            chip_rects: Vec::new(),
            tag_edit: HashMap::new(),
        }
    }

    /// Seleciona uma tarefa vinda de outra aba, garantindo que apareça na lista.
    pub fn open(&mut self, id: u64) {
        self.selected = Some(id);
        self.filter = Filter::Todas;
        self.tag_filter = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        // O editor só existe enquanto há uma tarefa sendo criada ou editada; Esc fecha.
        let editing = self.selected.is_some();
        if editing && ui.input(|i| i.key_pressed(egui::Key::Escape)) && ui.memory(|m| m.focused().is_none()) {
            self.selected = None;
        }
        if self.selected.is_some() {
            egui::Panel::left("lista").resizable(true).default_size(340.0).show(ui, |ui| self.list(ui, store));
            egui::CentralPanel::default().show(ui, |ui| self.editor(ui, store));
        } else {
            egui::CentralPanel::default().show(ui, |ui| self.list(ui, store));
        }
        self.tags_window(ui.ctx(), store);
        // O filtro de tag vale só enquanto não se clica em outro lugar: clicar em qualquer ponto fora das tags o desfaz.
        if self.tag_filter.is_some() && ui.input(|i| i.pointer.primary_clicked()) {
            let pos = ui.input(|i| i.pointer.interact_pos());
            if !pos.is_some_and(|p| self.chip_rects.iter().any(|r| r.contains(p))) {
                self.tag_filter = None;
            }
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
        egui::Window::new("Tags").open(&mut open).collapsible(false).default_width(360.0).show(ctx, |ui| {
            if tags.is_empty() {
                ui.weak("Nenhuma tag ainda. Crie tags no editor de uma tarefa.");
            }
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
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

    fn list(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        let today = Local::now().date_naive();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.new_title)
                    .hint_text("Nova tarefa...")
                    .desired_width(NEW_TASK_W),
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
            if ui.selectable_label(self.tags_open, "Tags").on_hover_text("Editar cores e nomes das tags").clicked() {
                self.tags_open = !self.tags_open;
            }
            ui.menu_button("Importar / exportar", |ui| self.ics_menu(ui, store));
        });
        if let Some(m) = &self.msg {
            ui.weak(m);
        }
        let tags = store.all_tags();
        self.chip_rects.clear();
        if !tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                tight(ui);
                for t in &tags {
                    let on = self.tag_filter.as_ref() == Some(t);
                    let chip = tag_chip(ui, t, tag_rgb(&store.tag_colors, t), on, "");
                    self.chip_rects.push(chip.rect);
                    if chip.clicked() {
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
                Filter::Hoje => t.occurs_on(today),
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
        let mut delete = None;
        let colors = &store.tag_colors;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if idx.is_empty() {
                ui.weak("Nada por aqui.");
            }
            for i in idx {
                let t = &mut store.tasks[i];
                // A linha inteira é clicável; os controles dentro dela (caixa, texto) têm prioridade.
                let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), ROW_H));
                let resp = ui.interact(row, ui.scope_id().with(("linha", t.id)), egui::Sense::click());
                let selected = self.selected == Some(t.id);
                if selected || resp.hovered() {
                    let a = if selected { 0.22 } else { 0.10 };
                    ui.painter().rect_filled(row, 7.0, ACCENT.gamma_multiply(a));
                }
                if resp.clicked() {
                    self.selected = Some(t.id);
                }
                let id = t.id;
                resp.context_menu(|ui| {
                    if ui.button("Editar").clicked() {
                        self.selected = Some(id);
                        ui.close();
                    }
                    if ui.button("Excluir").clicked() {
                        delete = Some(id);
                        ui.close();
                    }
                });
                let layout = egui::Layout::left_to_right(egui::Align::Center);
                ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(layout), |ui| {
                    ui.set_min_height(ROW_H);
                    if ui.checkbox(&mut t.done, "").changed() {
                        t.roll_over();
                        dirty = true;
                    }
                    let [r, g, b] = t.color;
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4.5, Color32::from_rgb(r, g, b));
                    // Título em cima; embaixo, uma linha de detalhes: data · hora · checklist e as tags.
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let mut title = RichText::new(&t.title);
                        if t.done {
                            title = title.strikethrough().weak();
                        }
                        ui.add(egui::Label::new(title).selectable(false).truncate());

                        let overdue = !t.done && t.date.is_some_and(|d| d < today);
                        let mut parts: Vec<(String, Color32)> = Vec::new();
                        if let (Some(d), true) = (t.date, self.filter != Filter::Hoje || overdue) {
                            let color = if overdue { Color32::from_rgb(230, 110, 110) } else { DIM };
                            parts.push((d.format("%d/%m").to_string(), color));
                        }
                        if let Some(h) = t.time {
                            parts.push((h.format("%H:%M").to_string(), DIM));
                        }
                        let (ok, total) = t.subtasks_done();
                        if total > 0 {
                            parts.push((format!("{ok}/{total} itens"), DIM));
                        }
                        if !parts.is_empty() || !t.tags.is_empty() {
                            ui.horizontal(|ui| {
                                tight(ui);
                                for (i, (text, color)) in parts.iter().enumerate() {
                                    if i > 0 {
                                        ui.label(RichText::new("·").size(11.5).color(DIM));
                                    }
                                    ui.label(RichText::new(text).size(11.5).color(*color));
                                }
                                for tag in &t.tags {
                                    let [r, g, b] = tag_rgb(colors, tag);
                                    ui.label(RichText::new(format!("#{tag}")).size(11.0).color(Color32::from_rgb(r, g, b)));
                                }
                            });
                        }
                    });
                });
            }
        });
        store.dirty |= dirty;
        if let Some(id) = delete {
            store.remove(id);
            if self.selected == Some(id) {
                self.selected = None;
                self.buf_for = None;
            }
        }
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

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut t.done, "").changed() {
                    t.roll_over();
                    changed = true;
                }
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut t.title)
                            .font(egui::TextStyle::Heading)
                            .desired_width(ui.available_width() - 130.0),
                    )
                    .changed();
                changed |= ui.color_edit_button_srgb(&mut t.color).changed();
                if chrome::icon_button(ui, Glyph::Trash, 0.0).on_hover_text("Excluir tarefa").clicked() {
                    delete = true;
                }
                if chrome::icon_button(ui, Glyph::Close, 0.0).on_hover_text("Fechar (Esc)").clicked() {
                    close = true;
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
                    t.set_anchor();
                    changed = true;
                }
                if ui.small_button("Hoje").clicked() {
                    t.date = Some(today);
                    t.set_anchor();
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

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Repetir");
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
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Termina");
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

            ui.add_space(8.0);
            ui.label("Tags");
            ui.horizontal_wrapped(|ui| {
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
            // Tags que já existem em outras tarefas, para reaproveitar com um clique.
            let unused: Vec<&String> = all_tags.iter().filter(|g| !t.tags.contains(g)).collect();
            if !unused.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    tight(ui);
                    ui.weak("Existentes:");
                    for tag in unused {
                        if tag_chip(ui, tag, tag_rgb(colors, tag), false, "").clicked() {
                            t.tags.push(tag.clone());
                            changed = true;
                        }
                    }
                });
            }

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
            ui.label("Informações");
            changed |= ui
                .add(egui::TextEdit::multiline(&mut t.notes).desired_rows(6).desired_width(f32::INFINITY))
                .changed();
        });

        store.dirty |= changed;
        if close {
            self.selected = None;
        }
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