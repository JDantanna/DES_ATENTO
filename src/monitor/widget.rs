//! Janela flutuante do Monitoramento: os quatro formatos (barra, pequeno, médio e grande), a trava de
//! tamanho, a opacidade e a lista flutuante das tarefas de hoje que abre junto dela.
//!
//! Regra de todo texto do widget: nunca termina em "…". Se o nome do programa não cabe inteiro, aparece só o
//! ícone (com o nome ao passar o mouse); se cabe, ícone e nome.

use super::*;
use crate::model::{Store, Task};
use crate::tasks::OVERDUE_TEXT;

/// Margem lateral das faixas.
const PAD: f32 = 10.0;
const GAP: f32 = 6.0;
/// Largura de cada botão do widget.
const BTN_W: f32 = 26.0;
const TASKS_TITLE: &str = "Navi · Tarefas de hoje";
/// Espaço entre o widget e a lista de tarefas.
const TASKS_GAP: f32 = 6.0;
const TASKS_HEADER_H: f32 = 40.0;
const TASK_ROW_H: f32 = 38.0;
/// Quantas tarefas cabem na lista antes de ela passar a rolar.
const TASKS_MAX_ROWS: usize = 7;
const TASKS_MIN_W: f32 = 270.0;
/// Quanto a janela da lista pode ficar transparente (ela precisa continuar legível).
const TASKS_MIN_OPACITY: f32 = 0.85;
const TASKS_BG: Color32 = Color32::from_rgb(26, 29, 42);
const TEXT: Color32 = Color32::from_gray(226);

/// Texto de uma linha só, sem cortar: se não cabe em `room`, não aparece (`None`).
pub(super) fn fit_text(p: &egui::Painter, text: &str, font: FontId, color: Color32, room: f32) -> Option<Arc<egui::Galley>> {
    if room <= 0.0 {
        return None;
    }
    let galley = p.layout_no_wrap(text.to_owned(), font, color);
    (galley.size().x <= room).then_some(galley)
}

/// O programa tem um ícone próprio para mostrar (o Navi é a fada).
fn has_icon(name: &str, tex: Option<egui::TextureId>) -> bool {
    tex.is_some() || name == APP_NAME
}

/// Desenha o ícone do programa em `rect`; `letter` põe a inicial do nome no lugar de um ícone que não existe.
pub(super) fn paint_icon(p: &egui::Painter, rect: egui::Rect, name: &str, tex: Option<egui::TextureId>, letter: bool) {
    if name == APP_NAME {
        crate::fairy::draw(p, rect.center(), rect.width() * 1.3, 0.0, 0.3);
    } else if let Some(id) = tex {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        p.image(id, rect, uv, Color32::WHITE);
    } else if letter {
        p.rect_filled(rect, 5.0, Color32::from_white_alpha(22));
        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        p.text(rect.center(), Align2::CENTER_CENTER, initial, FontId::proportional(rect.height() * 0.6), DIM);
    }
}

/// O que as peças do widget precisam saber para se desenhar.
struct Look<'a> {
    name: &'a str,
    tex: Option<egui::TextureId>,
    clock: &'a str,
    time_color: Color32,
    tasks_open: bool,
    /// Por que o programa conta (ou não) como em uso: "em uso", "aberto, sem uso", "tocando áudio"...
    state: Option<&'static str>,
    /// Fator de escala das fontes, de acordo com a largura da janela.
    k: f32,
}

impl Look<'_> {
    /// Texto ao passar o mouse no ícone, para quando o nome não cabe.
    fn tip(&self) -> String {
        match self.state {
            Some(state) => format!("{} ({state})", self.name),
            None => self.name.to_owned(),
        }
    }
}

/// O que cada formato de faixa mostra.
struct StripOpts {
    /// Nome do programa (se couber inteiro).
    name: bool,
    close: bool,
    tasks: bool,
}

/// Escreve o motivo do estado em cinza, depois do nome, se houver espaço inteiro para ele.
fn paint_state(p: &egui::Painter, look: &Look, x: f32, cy: f32, room: f32) {
    if let Some(label) = look.state
        && let Some(galley) = fit_text(p, &format!("· {label}"), FontId::proportional(12.0), DIM, room)
    {
        let h = galley.size().y;
        p.galley(egui::pos2(x, cy - h / 2.0), galley, DIM);
    }
}

/// O que o usuário pediu com um clique neste quadro.
#[derive(Default)]
struct Acts {
    close: bool,
    toggle_tasks: bool,
    toggle_list: bool,
}

/// Botão de ícone desenhado em um ponto, sem tomar espaço do layout.
fn mini_button(ui: &mut egui::Ui, center: egui::Pos2, glyph: chrome::Glyph, tip: &str, salt: &str) -> egui::Response {
    let rect = egui::Rect::from_center_size(center, vec2(BTN_W, 24.0));
    let resp = ui.interact(rect, ui.scope_id().with(salt), Sense::click());
    let hot = resp.hovered();
    if hot {
        let bg = if glyph == chrome::Glyph::Close { Color32::from_rgb(200, 60, 70) } else { Color32::from_white_alpha(22) };
        ui.painter().rect_filled(rect.shrink2(vec2(1.0, 0.0)), 7.0, bg);
    }
    chrome::paint_glyph(ui.painter(), center, glyph, if hot { Color32::WHITE } else { Color32::from_gray(185) }, 0.0);
    resp.on_hover_text(tip)
}

/// Barra e formato pequeno: uma faixa com o ícone e o cronômetro. O pequeno também mostra o nome (se couber)
/// e o botão de fechar; os dois têm a seta das tarefas. Os botões somem quando a janela fica estreita demais.
fn strip(ui: &mut egui::Ui, full: egui::Rect, look: &Look, opts: &StripOpts, acts: &mut Acts) {
    let p = ui.painter().clone();
    let (h, cy) = (full.height(), full.center().y);
    let buttons = opts.name;
    let icon_base = if buttons { 22.0 } else { 16.0 };
    let isz = f32::min(icon_base, (h - 6.0).max(10.0));
    let inner_w = full.width() - 2.0 * PAD;

    // O cronômetro é a peça que nunca some: em janela estreita a fonte diminui.
    let factor = if buttons { 0.42 } else { 0.52 };
    let mut fs = (h * factor).clamp(11.0, 40.0);
    let layout = |fs: f32| p.layout_no_wrap(look.clock.to_owned(), FontId::monospace(fs), look.time_color);
    let mut clock = layout(fs);
    let room_for_clock = (inner_w - isz - GAP).max(24.0);
    if clock.size().x > room_for_clock {
        fs = (fs * room_for_clock / clock.size().x).max(6.0);
        clock = layout(fs);
    }
    let clock_w = clock.size().x;

    // Depois do ícone e do relógio, o que sobrar vai para os botões (fechar primeiro, depois a seta das tarefas).
    let mut spare = inner_w - (isz + GAP + clock_w);
    let show_close = opts.close && spare >= BTN_W + GAP;
    if show_close {
        spare -= BTN_W + GAP;
    }
    let show_tasks = opts.tasks && spare >= BTN_W + GAP;
    let mut right = full.right() - PAD;
    if show_close {
        if mini_button(ui, egui::pos2(right - BTN_W / 2.0, cy), chrome::Glyph::Close, "Fechar o widget e abrir o Navi", "fechar").clicked() {
            acts.close = true;
        }
        right -= BTN_W + 2.0;
    }
    if show_tasks {
        let glyph = if look.tasks_open { chrome::Glyph::ChevronUp } else { chrome::Glyph::ChevronDown };
        if mini_button(ui, egui::pos2(right - BTN_W / 2.0, cy), glyph, "Tarefas de hoje", "tarefas").clicked() {
            acts.toggle_tasks = true;
        }
        right -= BTN_W;
    }
    let zone = egui::Rect::from_min_max(egui::pos2(full.left() + PAD, full.top()), egui::pos2(right, full.bottom()));

    let has = has_icon(look.name, look.tex);
    let icon_slot = if has { isz + GAP } else { 0.0 };
    let name_room = zone.width() - icon_slot - clock_w - 2.0 * GAP;
    let name = if opts.name { fit_text(&p, look.name, FontId::proportional(14.0), TEXT, name_room) } else { None };
    let icon_at = |x: f32| egui::Rect::from_min_size(egui::pos2(x, cy - isz / 2.0), vec2(isz, isz));
    match name {
        Some(galley) => {
            // Ícone e nome à esquerda, cronômetro à direita.
            let mut x = zone.left();
            if has {
                paint_icon(&p, icon_at(x), look.name, look.tex, false);
                x += isz + GAP;
            }
            let (h, name_w) = (galley.size().y, galley.size().x);
            p.galley(egui::pos2(x, cy - h / 2.0), galley, TEXT);
            let clock_x = zone.right() - clock_w;
            paint_state(&p, look, x + name_w + 6.0, cy, clock_x - GAP - (x + name_w + 6.0));
            p.galley(egui::pos2(clock_x, cy - clock.size().y / 2.0), clock, look.time_color);
        }
        None => {
            // O nome não cabe (ou é a barra): só o ícone, junto do cronômetro, no meio do espaço livre.
            let x0 = zone.center().x - (isz + GAP + clock_w) / 2.0;
            let icon = icon_at(x0);
            paint_icon(&p, icon, look.name, look.tex, true);
            ui.interact(icon, ui.scope_id().with("icone"), Sense::hover()).on_hover_text(look.tip());
            p.galley(egui::pos2(x0 + isz + GAP, cy - clock.size().y / 2.0), clock, look.time_color);
        }
    }
}

/// Linha de cima dos formatos médio e grande: ícone e nome à esquerda, botões à direita.
fn header_row(ui: &mut egui::Ui, rect: egui::Rect, look: &Look, acts: &mut Acts) {
    let p = ui.painter().clone();
    let cy = rect.center().y;
    let mut right = rect.right();
    if rect.width() >= 70.0 {
        if mini_button(ui, egui::pos2(right - BTN_W / 2.0, cy), chrome::Glyph::Close, "Fechar o widget e abrir o Navi", "fechar").clicked() {
            acts.close = true;
        }
        right -= BTN_W + 2.0;
    }
    if rect.width() >= 130.0 {
        let glyph = if look.tasks_open { chrome::Glyph::ChevronUp } else { chrome::Glyph::ChevronDown };
        if mini_button(ui, egui::pos2(right - BTN_W / 2.0, cy), glyph, "Tarefas de hoje", "tarefas").clicked() {
            acts.toggle_tasks = true;
        }
        right -= BTN_W;
    }
    let isz = 20.0;
    let has = has_icon(look.name, look.tex);
    let slot = if has { isz + GAP } else { 0.0 };
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left(), cy - isz / 2.0), vec2(isz, isz));
    match fit_text(&p, look.name, FontId::proportional(15.0), TEXT, right - 4.0 - rect.left() - slot) {
        Some(galley) => {
            if has {
                paint_icon(&p, icon, look.name, look.tex, false);
            }
            let (h, name_w) = (galley.size().y, galley.size().x);
            p.galley(egui::pos2(rect.left() + slot, cy - h / 2.0), galley, TEXT);
            let x = rect.left() + slot + name_w + 6.0;
            paint_state(&p, look, x, cy, right - 4.0 - x);
        }
        None => {
            paint_icon(&p, icon, look.name, look.tex, true);
            ui.interact(icon, ui.scope_id().with("icone"), Sense::hover()).on_hover_text(look.tip());
        }
    }
}

/// Uma linha da lista de programas da sessão: ícone e nome à esquerda (o nome some se não couber), tempo à direita.
fn session_row(ui: &mut egui::Ui, row: &AppRow, tex: Option<egui::TextureId>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let p = ui.painter().clone();
    let cy = rect.center().y;
    let time = p.layout_no_wrap(clock(row.secs), FontId::monospace(12.0), DIM);
    let time_w = time.size().x;
    p.galley(egui::pos2(rect.right() - time_w, cy - time.size().y / 2.0), time, DIM);
    let isz = 18.0;
    let has = has_icon(&row.name, tex);
    let slot = if has { isz + GAP } else { 0.0 };
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left(), cy - isz / 2.0), vec2(isz, isz));
    match fit_text(&p, &row.name, FontId::proportional(13.5), TEXT, rect.width() - time_w - 10.0 - slot) {
        Some(galley) => {
            if has {
                paint_icon(&p, icon, &row.name, tex, false);
            }
            let h = galley.size().y;
            p.galley(egui::pos2(rect.left() + slot, cy - h / 2.0), galley, TEXT);
        }
        None => {
            paint_icon(&p, icon, &row.name, tex, true);
            resp.on_hover_text(&row.name);
        }
    }
}

/// Formatos médio e grande: cabeçalho, cronômetro grande, total do dia e a lista de programas da sessão
/// (no grande ela fica sempre à vista; no médio abre e fecha).
fn card(ui: &mut egui::Ui, full: egui::Rect, look: &Look, data: &crate::tracker::WidgetData, rows_tex: &[Option<egui::TextureId>], big: bool, list_open: bool, acts: &mut Acts) {
    egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::same(14))).show(ui, |ui| {
        let (_, header) = ui.allocate_space(vec2(ui.available_width(), 24.0));
        header_row(ui, header, look, acts);
        ui.vertical_centered(|ui| {
            ui.add_space(2.0 * look.k);
            let base = if big { 44.0 } else { 34.0 };
            let size = (base * look.k).min(full.height() * 0.3).min(ui.available_width() / 4.9).max(10.0);
            ui.label(egui::RichText::new(look.clock).monospace().size(size).color(look.time_color));
            ui.weak(format!("Hoje, no total: {}", clock(data.total)));
            ui.add_space(2.0);
            if big {
                ui.weak(format!("Programas desta sessão ({})", data.session.len()));
            } else {
                let label = format!("{} Programas desta sessão ({})", if list_open { "–" } else { "+" }, data.session.len());
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    acts.toggle_list = true;
                }
            }
        });
        if big || list_open {
            egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                if data.session.is_empty() {
                    ui.weak("Nenhum programa registrado ainda.");
                }
                for (row, tex) in data.session.iter().zip(rows_tex) {
                    session_row(ui, row, *tex);
                }
            });
        }
    });
}

impl MonitorView {
    /// Janela flutuante; deve ser chamada a cada quadro, qualquer que seja a aba.
    pub fn floating(&mut self, ui: &mut egui::Ui, tracker: &Tracker) {
        if !self.widget {
            self.widget_h = 0.0;
            self.frame_fixed = false;
            self.widget_outer = None;
            self.set_tasks_open(false);
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
        self.fade = fade;
        if self.closing && fade <= 0.01 {
            self.widget = false;
            self.closing = false;
            self.set_tasks_open(false);
            return;
        }

        let data = tracker.widget();
        // Com um programa na frente (em uso ou só aberto) aparece o nome dele e o motivo do estado; sem ninguém
        // no computador, "Sem atividade".
        let name = data.now.current.clone().unwrap_or_else(|| "Sem atividade".to_owned());
        let state_label = data.now.current.is_some().then(|| data.now.state.label());
        let cur_tex = icon_tex(&mut self.icons, &ctx, data.now.icon.as_ref());
        let rows_tex: Vec<Option<egui::TextureId>> =
            data.session.iter().map(|r| icon_tex(&mut self.icons, &ctx, r.icon.as_ref())).collect();
        let format = self.prefs.format;
        let (locked, opacity) = (self.prefs.locked, self.prefs.opacity);
        let base = format_size(format);
        let list_open = format == WidgetFormat::Medio && self.widget_list;
        let height = if list_open { LIST_H } else { base.y };

        let id = egui::ViewportId::from_hash_of(WIDGET_ID);
        // Só pede novo tamanho quando o formato, a lista ou a escala muda: um comando a cada quadro
        // mantém a janela repintando sem parar, e o tamanho que o usuário escolheu arrastando não pode ser desfeito.
        let zoom = ctx.zoom_factor();
        let first = self.widget_h == 0.0;
        let resize = std::mem::take(&mut self.resize_now);
        if (!first && ((self.widget_h - height).abs() > f32::EPSILON || resize)) || (zoom - self.widget_zoom).abs() > 1e-3 {
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
        // o que brigaria com o redimensionamento feito pelo usuário. Já a trava de tamanho muda de verdade.
        let builder = egui::ViewportBuilder::default()
            .with_title(WIDGET_TITLE)
            .with_inner_size(self.widget_start)
            .with_min_inner_size(MIN_SIZE)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            // O widget não tem botão na barra de tarefas: com ele aberto, o Navi fica só na bandeja.
            .with_taskbar(false)
            // Surge sem roubar o foco de quem estiver em uso quando o Navi é minimizado.
            .with_active(false)
            .with_resizable(!locked);
        // Sem nada sendo rastreado (ocioso ou sem programa), o widget fica vermelho.
        let tracking = !data.now.idle && data.now.current.is_some();
        let tracked_name = if tracking { data.now.current.clone() } else { None };
        // Cor de fundo escolhida para o programa em uso (botão direito no widget ou no Monitoramento).
        let program = tracked_name.as_deref().and_then(|n| self.colors.get(n)).copied();
        let (fill, time_color) = if tracking {
            (program.map_or(BG, |[r, g, b]| BG.lerp_to_gamma(Color32::from_rgb(r, g, b), 0.5)), ACCENT)
        } else {
            (Color32::from_rgb(58, 20, 24), Color32::from_rgb(255, 120, 120))
        };
        let clock_text = clock(data.app_secs);
        let tasks_open = self.tasks_open;
        let mut acts = Acts::default();
        let mut open_app = false;
        let mut open_panel = None;
        let mut size = None;
        let mut outer = None;
        let mut ppp = self.widget_ppp;
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            ui.set_opacity((fade * opacity).clamp(0.0, 1.0));
            ppp = ui.ctx().pixels_per_point();
            // Textos não capturam o mouse: arrastar, duplo clique e botão direito valem no widget inteiro.
            ui.style_mut().interaction.selectable_labels = false;
            let full = ui.max_rect();
            size = Some(full.size());
            outer = ui.input(|i| i.viewport().outer_rect);

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
            // Trancado: sem alças de redimensionar (a janela também deixa de ser redimensionável).
            if !locked {
                chrome::resize_handles(ui, full);
            }

            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                open_app = true;
            }
            if drag.secondary_clicked() {
                open_panel = Some(panel_pos(ui));
            }
            // A fonte acompanha o tamanho da janela.
            let k = (full.width() / base.x).clamp(0.7, 2.4);
            let look = Look { name: &name, tex: cur_tex, clock: &clock_text, time_color, tasks_open, state: state_label, k };
            match format {
                // A barra só mostra ícone e cronômetro, mais a seta das tarefas.
                WidgetFormat::Barra => strip(ui, full, &look, &StripOpts { name: false, close: false, tasks: true }, &mut acts),
                WidgetFormat::Pequeno => strip(ui, full, &look, &StripOpts { name: true, close: true, tasks: true }, &mut acts),
                WidgetFormat::Medio | WidgetFormat::Grande => {
                    card(ui, full, &look, &data, &rows_tex, format == WidgetFormat::Grande, list_open, &mut acts)
                }
            }
        });
        if let Some(s) = size {
            self.widget_w = s.x.max(MIN_SIZE.x);
        }
        if outer.is_some() {
            self.widget_outer = outer;
        }
        self.widget_ppp = ppp;
        // A janela só existe a partir do primeiro quadro; então tira a moldura quadrada do Windows
        // e a faz não tomar o foco quando clicam nela.
        if !first && !self.frame_fixed {
            self.frame_fixed = chrome::flatten_window(WIDGET_TITLE);
            if self.frame_fixed {
                chrome::no_activate(WIDGET_TITLE);
            }
        }
        if acts.toggle_list {
            self.widget_list = !self.widget_list;
        }
        if acts.toggle_tasks {
            self.set_tasks_open(!self.tasks_open);
        }
        if let Some(at) = open_panel {
            self.panel = Some(Panel::new(tracked_name.unwrap_or_default(), true, at));
        }
        // Fechar o widget, ou dar dois cliques nele, abre o app.
        if open_app || acts.close {
            self.open_app = true;
            self.closing = true;
            self.set_tasks_open(false);
        }
    }

    /// Lista flutuante das tarefas de hoje, encostada no widget. Deve ser chamada a cada quadro.
    /// Mostra as pendentes e as atrasadas, ao vivo; o círculo conclui (e, clicado de novo, desfaz).
    pub fn tasks_window(&mut self, ui: &mut egui::Ui, store: &mut Store) {
        if !(self.widget && !self.closing && self.tasks_open) {
            return;
        }
        let ctx = ui.ctx().clone();
        // Sem saber onde o widget está (o primeiro quadro dele) a lista espera, para não aparecer no lugar errado.
        let outer = self.widget_outer.or_else(|| cfg!(test).then(|| egui::Rect::from_min_size(egui::pos2(24.0, 60.0), format_size(self.prefs.format))));
        let Some(outer) = outer else {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        };
        ctx.request_repaint_after(Duration::from_secs(1));
        let today = Local::now().date_naive();
        let now = Local::now().naive_local();

        let done_ids: Vec<u64> = self.tasks_win.done.iter().map(|d| d.before.id).collect();
        let mut lines: Vec<TaskLine> = store
            .today_pending(today, now)
            .into_iter()
            .map(|i| &store.tasks[i])
            .filter(|t| !done_ids.contains(&t.id))
            .map(|t| TaskLine::of(t, now, false))
            .collect();
        // As que acabaram de ser concluídas por aqui ficam no fim, riscadas, até a lista fechar (dá para desfazer).
        lines.extend(done_ids.iter().filter_map(|id| store.tasks.iter().find(|t| t.id == *id)).map(|t| TaskLine::of(t, now, true)));

        let shown = lines.len().min(TASKS_MAX_ROWS);
        let body = if lines.is_empty() { 44.0 } else { shown as f32 * TASK_ROW_H };
        let size = vec2(outer.width().max(TASKS_MIN_W), TASKS_HEADER_H + body + 10.0);

        // Embaixo do widget; se não couber na tela, em cima dele. Sempre dentro da área de trabalho do monitor.
        // Os pontos do egui valem de acordo com o DPI do monitor do widget, não o da janela principal.
        let area = chrome::work_area(WIDGET_TITLE, self.widget_ppp);
        let mut pos = egui::pos2(outer.left(), outer.bottom() + TASKS_GAP);
        if let Some(area) = area {
            if pos.y + size.y > area.bottom() {
                pos.y = outer.top() - size.y - TASKS_GAP;
                if pos.y < area.top() {
                    pos.y = (area.bottom() - size.y).max(area.top());
                }
            }
            pos.x = pos.x.min(area.right() - size.x).max(area.left());
        }

        let id = egui::ViewportId::from_hash_of("monitor_tarefas_hoje");
        if self.tasks_win.frames == 0 {
            self.tasks_win.start = size;
            self.tasks_win.start_pos = pos;
        } else {
            // A janela só existe a partir do primeiro quadro; depois ela acompanha o widget e o tamanho da lista.
            let moved = self.tasks_win.placed.is_none_or(|p| (p - pos).length() > 0.5);
            let resized = self.tasks_win.sized.is_none_or(|s| (s - size).length() > 0.5);
            if resized {
                ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(size));
            }
            if moved {
                ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(pos));
            }
        }
        self.tasks_win.placed = Some(pos);
        self.tasks_win.sized = Some(size);

        // O tamanho e o lugar daqui são só os iniciais e ficam fixos; o resto vai por comando (como no widget).
        let builder = egui::ViewportBuilder::default()
            .with_title(TASKS_TITLE)
            .with_inner_size(self.tasks_win.start)
            .with_position(self.tasks_win.start_pos)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false)
            // Não rouba o foco de quem está trabalhando em outro programa.
            .with_active(false);
        let alpha = (self.fade * self.prefs.opacity.max(TASKS_MIN_OPACITY)).clamp(0.0, 1.0);
        let pending = lines.iter().filter(|l| !l.done).count();
        let mut toggle = None;
        let mut close = false;
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            ui.set_opacity(alpha);
            ui.style_mut().interaction.selectable_labels = false;
            let full = ui.max_rect();
            ui.painter().rect_filled(full, chrome::RADIUS, TASKS_BG);
            ui.painter().rect_stroke(full, chrome::RADIUS, Stroke::new(1.0, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
            egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 8))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Tarefas de hoje").strong().size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if chrome::icon_button(ui, chrome::Glyph::Close, 0.0).on_hover_text("Fechar a lista").clicked() {
                            close = true;
                        }
                        let count = if pending == 1 { "1 pendente".to_owned() } else { format!("{pending} pendentes") };
                        ui.label(egui::RichText::new(count).size(12.0).color(DIM));
                    });
                });
                ui.spacing_mut().item_spacing.y = 0.0;
                if lines.is_empty() {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| ui.label(egui::RichText::new("Nada pendente para hoje.").color(DIM)));
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for line in &lines {
                        task_line(ui, line, &mut toggle);
                    }
                });
            });
        });

        if let Some(id) = toggle {
            self.toggle_task(store, id);
        }
        self.tasks_win.frames = self.tasks_win.frames.saturating_add(1);
        if self.tasks_win.frames >= 2 && !self.tasks_win.flattened {
            self.tasks_win.flattened = chrome::flatten_window(TASKS_TITLE);
            if self.tasks_win.flattened {
                chrome::no_activate(TASKS_TITLE);
            }
        }
        if close {
            self.set_tasks_open(false);
        }
    }

    /// Conclui a tarefa; uma que se repete já volta marcada para a próxima data (como na aba Tarefas).
    /// Clicar de novo numa que acabou de ser concluída aqui desfaz.
    fn toggle_task(&mut self, store: &mut Store, id: u64) {
        if let Some(at) = self.tasks_win.done.iter().position(|d| d.before.id == id) {
            let undone = self.tasks_win.done.remove(at);
            store.restore(undone.before, undone.index);
        } else if let Some(index) = store.tasks.iter().position(|t| t.id == id) {
            let before = store.tasks[index].clone();
            let task = &mut store.tasks[index];
            task.done = true;
            task.roll_over();
            store.dirty = true;
            self.tasks_win.done.push(Done { before, index });
        }
    }
}

/// Tarefa concluída por esta lista: o estado de antes serve para desfazer.
struct Done {
    before: Task,
    index: usize,
}

/// Estado da janela da lista de tarefas.
#[derive(Default)]
pub struct TasksWin {
    /// Quadros desde que a janela abriu (a moldura do Windows só sai com a janela já criada).
    frames: u8,
    flattened: bool,
    start: egui::Vec2,
    start_pos: egui::Pos2,
    placed: Option<egui::Pos2>,
    sized: Option<egui::Vec2>,
    done: Vec<Done>,
}

impl TasksWin {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// O que a lista mostra de uma tarefa.
struct TaskLine {
    id: u64,
    title: String,
    time: Option<String>,
    overdue: bool,
    done: bool,
}

impl TaskLine {
    fn of(t: &Task, now: chrono::NaiveDateTime, done: bool) -> Self {
        Self {
            id: t.id,
            title: t.title.clone(),
            time: t.time.map(|h| h.format("%H:%M").to_string()),
            overdue: !done && t.overdue(now),
            done,
        }
    }
}

/// Uma linha da lista: círculo de concluir, título (até duas linhas) e a hora à direita.
fn task_line(ui: &mut egui::Ui, line: &TaskLine, toggle: &mut Option<u64>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), TASK_ROW_H), Sense::hover());
    let p = ui.painter().clone();
    if resp.hovered() {
        p.rect_filled(rect, 8.0, Color32::from_white_alpha(8));
    }
    let center = egui::pos2(rect.left() + 16.0, rect.center().y);
    let check = ui.interact(egui::Rect::from_center_size(center, vec2(30.0, 30.0)), ui.scope_id().with(("tarefa", line.id)), Sense::click());
    let ring = if line.overdue { OVERDUE_TEXT } else { Color32::from_gray(170) };
    if line.done {
        p.circle_filled(center, 9.0, Color32::from_gray(90));
        chrome::paint_glyph(&p, center, chrome::Glyph::Check, Color32::WHITE, 0.0);
    } else {
        p.circle_stroke(center, 8.5, Stroke::new(1.6, ring));
        if check.hovered() {
            p.circle_filled(center, 8.0, ring.gamma_multiply(0.28));
            chrome::paint_glyph(&p, center, chrome::Glyph::Check, Color32::WHITE, 0.0);
        }
    }
    if check.on_hover_text(if line.done { "Desfazer" } else { "Concluir" }).clicked() {
        *toggle = Some(line.id);
    }

    let mut right = rect.right() - 6.0;
    if let Some(time) = &line.time {
        let color = if line.overdue { OVERDUE_TEXT } else { DIM };
        let galley = p.layout_no_wrap(time.clone(), FontId::proportional(12.5), color);
        right -= galley.size().x;
        p.galley(egui::pos2(right, rect.center().y - galley.size().y / 2.0), galley, color);
        right -= 8.0;
    }
    let color = if line.done {
        DIM
    } else if line.overdue {
        OVERDUE_TEXT
    } else {
        TEXT
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &line.title,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(13.5),
            color,
            strikethrough: if line.done { Stroke::new(1.0, color) } else { Stroke::NONE },
            underline: if line.overdue { Stroke::new(1.0, color) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    job.wrap.max_width = (right - (rect.left() + 34.0)).max(30.0);
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = None;
    let galley = p.layout_job(job);
    let (cut, h) = (galley.elided, galley.size().y);
    p.galley(egui::pos2(rect.left() + 34.0, rect.center().y - h / 2.0), galley, color);
    if cut {
        resp.on_hover_text(line.title.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roda um quadro vazio só para ter as fontes carregadas.
    fn with_painter(f: impl FnOnce(&egui::Painter)) {
        let ctx = egui::Context::default();
        let mut f = Some(f);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(f) = f.take() {
                f(ui.painter());
            }
        });
    }

    #[test]
    fn nome_que_nao_cabe_some_em_vez_de_virar_reticencias() {
        with_painter(|p| {
            let font = FontId::proportional(14.0);
            let name = "Um programa com um nome bem comprido";
            let full = p.layout_no_wrap(name.to_owned(), font.clone(), TEXT).size().x;
            let shown = fit_text(p, name, font.clone(), TEXT, full + 1.0).expect("cabe inteiro");
            assert_eq!(shown.text(), name);
            assert!(fit_text(p, name, font.clone(), TEXT, full - 1.0).is_none(), "não cabe: nada de nome cortado");
            assert!(fit_text(p, name, font, TEXT, 0.0).is_none());
        });
    }

    #[test]
    fn linha_da_lista_mostra_hora_e_marca_atrasada() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let now = today.and_hms_opt(12, 0, 0).unwrap();
        let mut store = Store::default();
        let id = store.add("Pagar".into(), Some(today));
        let task = &mut store.tasks[0];
        task.time = chrono::NaiveTime::from_hms_opt(9, 5, 0);
        let line = TaskLine::of(task, now, false);
        assert_eq!((line.id, line.time.as_deref(), line.overdue), (id, Some("09:05"), true));
        // Concluída por aqui, deixa de contar como atrasada.
        assert!(!TaskLine::of(task, now, true).overdue);
    }

    #[test]
    fn concluir_e_desfazer_pela_lista() {
        let today = Local::now().date_naive();
        let mut store = Store::default();
        let id = store.add("Ligar".into(), Some(today));
        store.dirty = false;
        let mut view = MonitorView::new(crate::model::Settings::default().widget_prefs());
        view.toggle_task(&mut store, id);
        assert!(store.tasks[0].done && store.dirty);
        assert_eq!(view.tasks_win.done.len(), 1);
        view.toggle_task(&mut store, id);
        assert!(!store.tasks[0].done);
        assert!(view.tasks_win.done.is_empty());
        // Uma tarefa que se repete volta pendente na próxima data, e desfazer devolve a data de antes.
        let id = store.add("Academia".into(), Some(today));
        let at = store.tasks.iter().position(|t| t.id == id).unwrap();
        store.tasks[at].repeat = crate::model::Repeat::Daily;
        store.tasks[at].set_anchor();
        view.toggle_task(&mut store, id);
        assert!(!store.tasks[at].done && store.tasks[at].date == Some(today + chrono::Duration::days(1)));
        view.toggle_task(&mut store, id);
        assert_eq!(store.tasks[at].date, Some(today));
    }

    #[test]
    fn formato_novo_volta_ao_tamanho_padrao_e_opacidade_tem_limite() {
        let mut view = MonitorView::new(crate::model::Settings::default().widget_prefs());
        let mut prefs = view.prefs;
        prefs.format = WidgetFormat::Barra;
        prefs.opacity = 0.0;
        view.apply_prefs(prefs);
        assert_eq!(view.widget_w, format_size(WidgetFormat::Barra).x);
        assert!(view.resize_now && view.prefs_changed);
        assert!((view.prefs.opacity - 0.3).abs() < 1e-6, "abaixo de 30% o widget some");
        // Repetir a mesma escolha não manda gravar de novo.
        view.prefs_changed = false;
        view.apply_prefs(view.prefs);
        assert!(!view.prefs_changed);
    }
}
