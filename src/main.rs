#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appcache;
mod appname;
mod backup;
#[cfg(test)]
mod capturas;
mod chrome;
mod datepicker;
mod fairy;
mod ics;
mod model;
mod monitor;
mod notify;
mod planner;
mod splash;
mod tasks;
mod theme;
mod tray;
mod tracker;
mod updater;

use chrono::Local;
use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use model::{Settings, Store};
use monitor::MonitorView;
use planner::Planner;
use splash::Splash;
use tasks::TasksView;
use theme::ACCENT;
use tracker::Tracker;
use updater::{State, Updater};

/// Tamanho mínimo da janela, em pontos da interface (acompanha a escala escolhida).
const MIN_WINDOW: egui::Vec2 = vec2(480.0, 420.0);
/// Abaixo desta largura, as abas descem para uma segunda linha.
const TABS_INLINE_W: f32 = 740.0;
/// O mesmo, com o botão de atualizar (mais largo) à mostra.
const TABS_INLINE_BUSY_W: f32 = 940.0;
/// Gravação das tarefas: espera este tempo depois da primeira mudança, para não gravar a cada tecla.
const SAVE_DELAY_SECS: f64 = 1.0;
/// O mesmo para as preferências do widget (opacidade, formato, trava).
const PREFS_SAVE_DELAY_SECS: f64 = 0.6;
/// Quanto o fechamento do app espera pelo último backup (a unidade da nuvem pode estar lenta).
const EXIT_BACKUP_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Tab {
    Tarefas,
    Calendario,
    Monitoramento,
}

pub(crate) struct App {
    pub(crate) splash: Option<Splash>,
    pub(crate) tab: Tab,
    store: Store,
    pub(crate) tasks: TasksView,
    pub(crate) planner: Planner,
    pub(crate) monitor: MonitorView,
    tracker: Tracker,
    updater: Updater,
    tray: Option<tray::Tray>,
    /// Mostra a pergunta "sair ou ir para a bandeja?" ao fechar.
    ask_close: bool,
    /// A saída já foi confirmada: o pedido de fechar passa direto.
    quitting: bool,
    pub(crate) settings: Settings,
    pub(crate) backup: backup::Backup,
    /// Backup a colocar no lugar dos dados quando o app terminar de fechar.
    restore_pending: Option<backup::Prepared>,
    /// Desde quando há mudanças nas tarefas ainda não gravadas (no relógio da interface).
    dirty_since: Option<f64>,
    /// Quando as preferências do widget mudaram pela última vez e ainda não foram gravadas (relógio da interface).
    prefs_dirty_at: Option<f64>,
    scale_dirty: bool,
    /// Etapa da volta da bandeja com esmaecer (0 = nenhuma).
    restore_stage: u8,
    /// A moldura quadrada do Windows já foi removida da janela principal.
    frame_fixed: bool,
    /// A janela principal estava fora de cena (minimizada ou na bandeja) no quadro anterior.
    was_away: bool,
    /// Escala para a qual o tamanho mínimo da janela foi calculado.
    min_size_zoom: f32,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        cc.egui_ctx.set_zoom_factor(settings.scale);
        notify::start();
        let store = Store::load();
        let day = Local::now().date_naive();
        let today = store.today_titles(day);
        let behind = store.pending_before(day);
        Self::with(store, settings, Some(Splash::new(today, behind)))
    }

    pub(crate) fn with(store: Store, settings: Settings, splash: Option<Splash>) -> Self {
        Self {
            splash,
            tab: Tab::Tarefas,
            store,
            tasks: TasksView::new(),
            planner: Planner::new(),
            monitor: MonitorView::new(settings.widget_prefs()),
            tracker: Tracker::start(),
            updater: Updater::default(),
            tray: None,
            ask_close: false,
            quitting: false,
            settings,
            backup: backup::Backup::new(),
            restore_pending: None,
            dirty_since: None,
            prefs_dirty_at: None,
            scale_dirty: false,
            restore_stage: 0,
            frame_fixed: false,
            was_away: false,
            min_size_zoom: 1.0,
        }
    }

    /// Atualização: `wide` diz se há espaço para a versão e as mensagens por extenso.
    fn update_ui(&mut self, ui: &mut egui::Ui, wide: bool) {
        let ctx = ui.ctx().clone();
        if wide {
            ui.weak(format!("v{}", updater::VERSION));
        }
        match self.updater.state() {
            State::Checking | State::Downloading => {
                // O ícone gira enquanto trabalha.
                let spin = ui.input(|i| i.time) as f32 * 5.0;
                chrome::icon_button(ui, chrome::Glyph::Refresh, spin);
                if wide {
                    ui.weak(if matches!(self.updater.state(), State::Checking) { "Verificando..." } else { "Baixando atualização..." });
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(33));
            }
            State::Available { version, url } => {
                if ui.button(format!("Atualizar para v{version}")).clicked() {
                    self.updater.install(url, ctx);
                }
            }
            st => {
                if chrome::icon_button(ui, chrome::Glyph::Refresh, 0.0).on_hover_text("Buscar atualizações").clicked() {
                    self.updater.check(ctx);
                }
                match st {
                    State::UpToDate if wide => {
                        ui.weak("Na versão mais recente");
                    }
                    State::Error(e) => {
                        ui.add(egui::Label::new(egui::RichText::new(e).color(Color32::LIGHT_RED)).truncate());
                    }
                    _ => {}
                }
            }
        }
    }

    fn go_tray(&mut self, ctx: &egui::Context) {
        if self.tray.is_none() {
            self.tray = tray::enter(ctx, include_bytes!("../assets/icon64.rgba"));
        }
        // Sem bandeja disponível, ao menos minimiza.
        if self.tray.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        } else if !self.settings.tray_notice_shown {
            // Só na primeira vez: avisa que o Navi continua rodando na bandeja.
            self.settings.tray_notice_shown = true;
            self.settings.save();
            notify::toast("O Navi continua aberto", "Ele está na bandeja do sistema. Clique no ícone para abrir.");
        }
    }

    /// Traz a janela principal de volta (da bandeja ou da barra de tarefas) e a põe em foco.
    /// Da bandeja ela reaparece esmaecendo: primeiro um quadro transparente, só então a janela é mostrada.
    fn restore(&mut self, ctx: &egui::Context) {
        if self.tray.is_some() {
            self.restore_stage = 1;
            ctx.request_repaint();
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Opacidade da janela principal neste quadro (0 a 1) e as etapas de volta da bandeja.
    fn fade(&mut self, ctx: &egui::Context, id: egui::Id) -> f32 {
        let hold = self.restore_stage == 1;
        match self.restore_stage {
            1 => {
                ctx.animate_value_with_time(id, 0.0, 0.0);
                self.restore_stage = 2;
                ctx.request_repaint();
            }
            2 => {
                if self.tray.take().is_some() {
                    tray::leave();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.restore_stage = 0;
            }
            _ => {}
        }
        if hold { 0.0 } else { ctx.animate_value_with_time(id, 1.0, theme::FADE_SECS) }
    }

    /// Mantém a escala dentro dos limites e a grava quando muda (menu, Ctrl +/-).
    fn sync_scale(&mut self, ctx: &egui::Context) {
        let z = ctx.zoom_factor();
        let clamped = z.clamp(*model::SCALE_RANGE.start(), *model::SCALE_RANGE.end());
        if (clamped - z).abs() > 1e-3 {
            ctx.set_zoom_factor(clamped);
        }
        if (clamped - self.settings.scale).abs() > 1e-3 {
            self.settings.scale = clamped;
            self.scale_dirty = true;
        }
        if self.scale_dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.scale_dirty = false;
            self.settings.save();
        }
    }

    /// O tamanho mínimo é em pontos: ao mudar a escala, ele é refeito e a janela cresce se ficou pequena demais.
    fn sync_min_size(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        if (zoom - self.min_size_zoom).abs() > 1e-3 {
            self.min_size_zoom = zoom;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(MIN_WINDOW));
            let size = ctx.content_rect().size();
            if size.x < MIN_WINDOW.x || size.y < MIN_WINDOW.y {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.max(MIN_WINDOW)));
            }
        }
    }

    fn scale_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.menu_button("Aa", |ui| {
            let z = ctx.zoom_factor();
            ui.label("Tamanho da interface");
            ui.horizontal(|ui| {
                let set = |v: f32| ctx.set_zoom_factor(((v * 20.0).round() / 20.0).clamp(*model::SCALE_RANGE.start(), *model::SCALE_RANGE.end()));
                if ui.button("−").clicked() {
                    set(z - 0.1);
                }
                ui.label(format!("{:.0}%", z * 100.0));
                if ui.button("+").clicked() {
                    set(z + 0.1);
                }
                if ui.button("100%").clicked() {
                    set(1.0);
                }
            });
            ui.weak("Atalhos: Ctrl + / Ctrl − / Ctrl 0");
        })
        .response
        .on_hover_text("Tamanho da interface");
    }

    /// A fada do cabeçalho, sem o fundo do ícone; só cresce um pouco quando o mouse passa por cima.
    fn logo_ui(ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
        let grow = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.15);
        fairy::draw(ui.painter(), rect.center(), 30.0 * (1.0 + 0.25 * grow), 0.0, 0.3 + 0.7 * grow);
    }

    /// O widget acompanha a janela principal: abre quando ela sai de cena, por qualquer caminho
    /// (botão, Win+D, barra de tarefas ou bandeja), e fecha quando ela volta.
    /// Com o widget aberto o Navi fica só na bandeja: uma janela minimizada de outro jeito que não o botão
    /// (Win+D, o botão da barra de tarefas) também vai para lá, e some da barra de tarefas.
    fn sync_widget(&mut self, ctx: &egui::Context) {
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let away = self.tray.is_some() || minimized;
        if away != self.was_away {
            self.was_away = away;
            if away {
                self.monitor.open_widget();
                if self.tray.is_none() {
                    self.go_tray(ctx);
                }
            } else {
                self.monitor.close_widget();
            }
        }
    }

    /// Grava as preferências do widget só depois de um instante sem mudar (arrastar a opacidade muda a cada quadro).
    fn prefs_tick(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        if let Some(prefs) = self.monitor.take_prefs_change() {
            self.settings.set_widget_prefs(prefs);
            self.prefs_dirty_at = Some(time);
        }
        if let Some(at) = self.prefs_dirty_at {
            if time - at >= PREFS_SAVE_DELAY_SECS {
                self.prefs_dirty_at = None;
                self.settings.save();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(PREFS_SAVE_DELAY_SECS));
            }
        }
    }

    /// Grava as mudanças pendentes das tarefas agora.
    fn save_store(&mut self) {
        self.dirty_since = None;
        if self.store.dirty {
            self.store.save();
        }
    }

    /// Faz o backup com os dados em disco já atualizados.
    fn start_backup(&mut self, ctx: &egui::Context, dir: std::path::PathBuf) {
        self.save_store();
        self.tracker.save_now();
        self.backup.start(dir, ctx);
    }

    /// Recolhe o resultado do backup em andamento e confere se é hora de outro.
    fn backup_tick(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.backup.take_finished() {
            self.settings.last_backup = Some(at);
            self.settings.save();
        }
        let Some(dir) = self.settings.backup_dir.clone() else { return };
        let time = ctx.input(|i| i.time);
        let now = Local::now().naive_local();
        if self.backup.check_now(time) && !self.backup.running() && backup::due(self.settings.last_backup, backup::tasks_changed_at(), now) {
            self.start_backup(ctx, dir);
        }
        // Parado na bandeja, o app só acorda para a próxima conferência.
        ctx.request_repaint_after(self.backup.until_check(time));
    }

    fn backup_requests(&mut self, ctx: &egui::Context) {
        match self.backup.window(ctx, &mut self.settings) {
            Some(backup::Request::BackupNow) => {
                if let Some(dir) = self.settings.backup_dir.clone() {
                    self.start_backup(ctx, dir);
                }
            }
            Some(backup::Request::Restore(prepared)) => {
                // Só substitui os dados depois de ter uma cópia dos atuais; a troca em si acontece ao fechar.
                self.save_store();
                self.tracker.save_now();
                match backup::safety_copy(&prepared, &self.settings, &model::data_dir(), Local::now().naive_local()) {
                    Ok(_) => {
                        self.restore_pending = Some(prepared);
                        self.quit(ctx);
                    }
                    Err(e) => self.backup.set_error(format!("Não foi possível copiar os dados atuais, então nada foi alterado. {e}")),
                }
            }
            None => {}
        }
    }

    /// Último passo ao fechar para restaurar: troca os arquivos e abre o app de novo.
    fn finish_restore(&mut self, prepared: backup::Prepared) {
        self.tracker.freeze();
        let data = model::data_dir();
        if backup::commit(&prepared, &data).is_ok() {
            // A pasta de backup é desta máquina: o backup restaurado pode ter vindo de outro computador.
            let mut restored = Settings::load();
            restored.backup_dir = self.settings.backup_dir.take();
            restored.save();
        }
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).spawn();
        }
    }

    /// Ao fechar, as tarefas mudadas desde o último backup vão para a nuvem, sem prender o fechamento.
    fn final_backup(&mut self) {
        let Some(dir) = self.settings.backup_dir.clone() else { return };
        if self.backup.running() || !backup::changed_since(self.settings.last_backup, backup::tasks_changed_at()) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let now = Local::now().naive_local();
            let _ = tx.send(backup::create(&dir, &model::data_dir(), now).map(|_| now));
        });
        if let Ok(Ok(at)) = rx.recv_timeout(EXIT_BACKUP_WAIT) {
            self.settings.last_backup = Some(at);
            self.settings.save();
        }
    }

    /// Encerra o app de verdade (sem passar pela pergunta).
    fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        enum Choice {
            Quit,
            Tray,
            Cancel,
        }
        let mut choice = None;
        egui::Window::new("Fechar o Navi")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Quer mesmo sair?");
                ui.weak("Saindo, o monitoramento para e os avisos de tarefas deixam de aparecer.");
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Sair").clicked() {
                        choice = Some(Choice::Quit);
                    }
                    if ui.button("Ir para a bandeja").clicked() {
                        choice = Some(Choice::Tray);
                    }
                    if ui.add(egui::Button::new("Cancelar").frame(false)).clicked() {
                        choice = Some(Choice::Cancel);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(Choice::Cancel);
        }
        match choice {
            Some(Choice::Quit) => self.quit(ctx),
            Some(Choice::Tray) => {
                self.ask_close = false;
                self.go_tray(ctx);
            }
            Some(Choice::Cancel) => self.ask_close = false,
            None => {}
        }
    }

    /// Botões das abas; `fill` os estica para ocupar a linha toda.
    fn tabs(&mut self, ui: &mut egui::Ui, fill: bool) {
        const TABS: [(Tab, &str); 3] = [(Tab::Tarefas, "Tarefas"), (Tab::Calendario, "Calendário"), (Tab::Monitoramento, "Monitoramento")];
        let width = if fill { (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0 } else { 0.0 };
        for (tab, label) in TABS {
            let sel = self.tab == tab;
            let text = egui::RichText::new(label).size(14.5).color(if sel { Color32::WHITE } else { Color32::from_gray(160) });
            let bg = if sel { ACCENT.gamma_multiply(0.22) } else { Color32::TRANSPARENT };
            let button = egui::Button::new(text).fill(bg).stroke(Stroke::NONE).corner_radius(9).min_size(vec2(width, 0.0));
            if ui.add(button).clicked() {
                self.tab = tab;
            }
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui, maximized: bool) {
        let ctx = ui.ctx().clone();
        let width = ui.available_width();
        let busy = matches!(self.updater.state(), State::Available { .. } | State::Downloading | State::Error(_));
        let inline = width >= if busy { TABS_INLINE_BUSY_W } else { TABS_INLINE_W };
        egui::Panel::top("bar").frame(egui::Frame::NONE).show(ui, |ui| {
            ui.set_height(44.0);
            let rect = ui.max_rect();
            let drag = ui.interact(rect, ui.scope_id().with("titlebar"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            ui.horizontal_centered(|ui| {
                ui.add_space(14.0);
                Self::logo_ui(ui);
                ui.add(egui::Label::new(egui::RichText::new("Navi").strong().size(16.0)).selectable(false));
                ui.add_space(14.0);
                if inline {
                    self.tabs(ui, false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if chrome::window_button(ui, chrome::Glyph::Close).clicked() {
                        self.ask_close = true;
                    }
                    let g = if maximized { chrome::Glyph::Restore } else { chrome::Glyph::Maximize };
                    if chrome::window_button(ui, g).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    // Minimizar manda o Navi para a bandeja (o widget abre e o app sai da barra de tarefas).
                    let min = chrome::window_button(ui, chrome::Glyph::Minimize).on_hover_text("Minimizar para a bandeja");
                    if min.clicked() {
                        self.go_tray(&ctx);
                    }
                    ui.add_space(8.0);
                    self.scale_menu(ui);
                    ui.add_space(2.0);
                    let health = self.backup.health(&self.settings);
                    let hint = match health {
                        backup::Health::Ok => "Backup",
                        backup::Health::Off => "Backup desligado",
                        backup::Health::Failing => "O backup está com problema",
                    };
                    let cloud = chrome::icon_button(ui, chrome::Glyph::Cloud, 0.0).on_hover_text(hint);
                    let dot = match health {
                        backup::Health::Ok => None,
                        backup::Health::Off => Some(Color32::from_rgb(240, 180, 90)),
                        backup::Health::Failing => Some(Color32::from_rgb(235, 80, 80)),
                    };
                    if let Some(dot) = dot {
                        ui.painter().circle_filled(cloud.rect.right_top() + vec2(-9.0, 7.0), 3.5, dot);
                    }
                    if cloud.clicked() {
                        self.backup.open = !self.backup.open;
                    }
                    ui.add_space(2.0);
                    self.update_ui(ui, width >= 1080.0);
                });
            });
        });
        if !inline {
            let frame = egui::Frame::NONE.inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 6 });
            egui::Panel::top("abas").frame(frame).show(ui, |ui| {
                ui.horizontal(|ui| self.tabs(ui, true));
            });
        }
    }

    /// A interface inteira da janela principal, em um quadro.
    pub(crate) fn root(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.sync_scale(&ctx);
        self.sync_min_size(&ctx);
        if !self.frame_fixed {
            self.frame_fixed = chrome::flatten_window("Navi");
        }
        let fade = self.fade(&ctx, ui.scope_id().with("app_fade"));
        ui.set_opacity(fade);
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let radius = if maximized { 0.0 } else { chrome::RADIUS };
        let full = ui.max_rect();
        ui.painter().rect_filled(full, radius, theme::BG);
        ui.painter().rect_stroke(full, radius, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
        if !maximized {
            chrome::resize_handles(ui, full);
        }

        if let Some(s) = &mut self.splash {
            if s.show(ui) {
                self.splash = None;
            }
            return;
        }

        match tray::take_request() {
            Some(tray::Request::Restore) => self.restore(&ctx),
            Some(tray::Request::Quit) => self.quit(&ctx),
            None => {}
        }
        if self.updater.state() == State::Closing {
            self.quit(&ctx);
        }
        // Alt+F4 e afins também passam pela pergunta (menos quando a saída já foi confirmada).
        if !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ask_close = true;
        }

        self.title_bar(ui, maximized);

        match self.tab {
            Tab::Tarefas => self.tasks.show(ui, &mut self.store),
            Tab::Calendario => {
                if let Some(id) = self.planner.show(ui, &mut self.store) {
                    self.tasks.open(id);
                    self.tab = Tab::Tarefas;
                }
            }
            Tab::Monitoramento => self.monitor.show(ui, &self.tracker),
        }
        if self.tab != Tab::Calendario {
            self.planner.cancel_drag();
        }
        if self.tab != Tab::Tarefas {
            self.tasks.cancel_drag();
        }

        self.sync_widget(&ctx);
        self.monitor.floating(ui, &self.tracker);
        self.monitor.tasks_window(ui, &mut self.store);
        self.monitor.panel(ui, &self.tracker);
        if self.monitor.take_opened() {
            self.go_tray(&ctx);
        }
        if self.monitor.take_open_app() {
            self.restore(&ctx);
        }
        self.prefs_tick(&ctx);
        if self.ask_close {
            self.close_dialog(&ctx);
        }

        self.backup_requests(&ctx);
        self.backup_tick(&ctx);

        if self.store.dirty {
            let time = ctx.input(|i| i.time);
            let since = *self.dirty_since.get_or_insert(time);
            if time - since >= SAVE_DELAY_SECS {
                self.save_store();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAVE_DELAY_SECS));
            }
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.root(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.prefs_dirty_at.take().is_some() {
            self.settings.save();
        }
        self.save_store();
        self.tracker.save_now();
        match self.restore_pending.take() {
            Some(prepared) => self.finish_restore(prepared),
            None => self.final_backup(),
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Navi")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
                width: 64,
                height: 64,
            })
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([1000.0, 640.0])
            .with_min_inner_size(MIN_WINDOW),
        ..Default::default()
    };
    eframe::run_native("Navi", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
