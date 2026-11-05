#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appcache;
mod appname;
mod chrome;
mod coach;
mod datepicker;
mod fairy;
mod ics;
mod model;
mod notify;
mod planner;
mod splash;
mod tasks;
mod theme;
mod tray;
mod tracker;
mod updater;

use chrono::Local;
use coach::CoachView;
use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use model::{Settings, Store};
use planner::Planner;
use splash::Splash;
use tasks::TasksView;
use theme::ACCENT;
use tracker::Tracker;
use updater::{State, Updater};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tarefas,
    Calendario,
    Coach,
}

struct App {
    splash: Option<Splash>,
    tab: Tab,
    store: Store,
    tasks: TasksView,
    planner: Planner,
    coach: CoachView,
    tracker: Tracker,
    updater: Updater,
    tray: Option<tray::Tray>,
    ask_minimize: bool,
    /// Mostra a pergunta "sair ou ir para a bandeja?" ao fechar.
    ask_close: bool,
    /// A saída já foi confirmada: o pedido de fechar passa direto.
    quitting: bool,
    settings: Settings,
    scale_dirty: bool,
    /// Etapa da volta da bandeja com esmaecer (0 = nenhuma).
    restore_stage: u8,
    /// A moldura quadrada do Windows já foi removida da janela principal.
    frame_fixed: bool,
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
        Self {
            splash: Some(Splash::new(today, behind)),
            tab: Tab::Tarefas,
            store,
            tasks: TasksView::new(),
            planner: Planner::new(),
            coach: CoachView::new(settings.widget_small),
            tracker: Tracker::start(),
            updater: Updater::default(),
            tray: None,
            ask_minimize: false,
            ask_close: false,
            quitting: false,
            settings,
            scale_dirty: false,
            restore_stage: 0,
            frame_fixed: false,
        }
    }

    fn update_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.weak(format!("v{}", updater::VERSION));
        match self.updater.state() {
            State::Checking | State::Downloading => {
                // O ícone gira enquanto trabalha.
                let spin = ui.input(|i| i.time) as f32 * 5.0;
                chrome::icon_button(ui, chrome::Glyph::Refresh, spin);
                ui.weak(if matches!(self.updater.state(), State::Checking) { "Verificando..." } else { "Baixando atualização..." });
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
                    State::UpToDate => {
                        ui.weak("Na versão mais recente");
                    }
                    State::Error(e) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
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

    /// Aplica a escolha de minimizar (bandeja ou só minimizar).
    fn minimize_to(&mut self, ctx: &egui::Context, to_tray: bool) {
        if to_tray {
            self.go_tray(ctx);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
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
                ui.weak("Saindo, o Coach para de rastrear e os avisos de tarefas deixam de aparecer.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
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
                self.minimize_to(ctx, true);
            }
            Some(Choice::Cancel) => self.ask_close = false,
            None => {}
        }
    }

    fn minimize_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        egui::Window::new("Minimizar o Navi")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Para onde o Navi deve ir?");
                ui.weak("Pergunto só desta vez. Clique com o botão direito no botão de minimizar para mudar.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Bandeja do sistema").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Só minimizar").clicked() {
                        choice = Some(false);
                    }
                    if ui.add(egui::Button::new("Cancelar").frame(false)).clicked() {
                        self.ask_minimize = false;
                    }
                });
            });
        if let Some(to_tray) = choice {
            self.ask_minimize = false;
            self.settings.minimize_to_tray = Some(to_tray);
            self.settings.save();
            self.minimize_to(ctx, to_tray);
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui, maximized: bool) {
        let ctx = ui.ctx().clone();
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
                for (tab, label) in [(Tab::Tarefas, "Tarefas"), (Tab::Calendario, "Calendário"), (Tab::Coach, "Coach")] {
                    let sel = self.tab == tab;
                    let text = egui::RichText::new(label).size(14.5).color(if sel { Color32::WHITE } else { Color32::from_gray(160) });
                    let fill = if sel { ACCENT.gamma_multiply(0.22) } else { Color32::TRANSPARENT };
                    if ui.add(egui::Button::new(text).fill(fill).stroke(Stroke::NONE).corner_radius(9)).clicked() {
                        self.tab = tab;
                    }
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
                    let min = chrome::window_button(ui, chrome::Glyph::Minimize).on_hover_text("Clique com o botão direito para escolher de novo");
                    if min.clicked() {
                        match self.settings.minimize_to_tray {
                            Some(to_tray) => self.minimize_to(&ctx, to_tray),
                            None => self.ask_minimize = true,
                        }
                    }
                    if min.secondary_clicked() {
                        self.ask_minimize = true;
                    }
                    ui.add_space(8.0);
                    self.scale_menu(ui);
                    ui.add_space(4.0);
                    self.update_ui(ui);
                });
            });
        });
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.sync_scale(&ctx);
        if !self.frame_fixed {
            self.frame_fixed = chrome::flatten_window("Navi");
        }
        let fade = self.fade(&ctx, ui.scope_id().with("app_fade"));
        ui.set_opacity(fade);
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
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
            Some(tray::Request::Restore) => self.restore(ui.ctx()),
            Some(tray::Request::Quit) => self.quit(&ctx),
            None => {}
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
            Tab::Coach => self.coach.show(ui, &self.tracker),
        }
        if self.tab != Tab::Calendario {
            self.planner.cancel_drag();
        }
        if self.tab != Tab::Tarefas {
            self.tasks.cancel_drag();
        }

        self.coach.floating(ui, &self.tracker);
        self.coach.panel(ui, &self.tracker);
        if self.coach.take_opened() {
            let ctx = ui.ctx().clone();
            self.go_tray(&ctx);
        }
        if self.coach.take_open_app() {
            self.restore(ui.ctx());
        }
        if let Some(small) = self.coach.take_format_change() {
            self.settings.widget_small = small;
            self.settings.save();
        }
        if self.ask_minimize {
            let ctx = ui.ctx().clone();
            self.minimize_dialog(&ctx);
        }
        if self.ask_close {
            let ctx = ui.ctx().clone();
            self.close_dialog(&ctx);
        }

        if self.store.dirty {
            self.store.save();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.tracker.save_now();
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
            .with_min_inner_size([760.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native("Navi", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
