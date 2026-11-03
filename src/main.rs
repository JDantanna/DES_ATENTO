#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chrome;
mod coach;
mod ics;
mod model;
mod notify;
mod planner;
mod splash;
mod tasks;
mod theme;
mod tracker;
mod updater;

use chrono::Local;
use coach::CoachView;
use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use model::Store;
use planner::Planner;
use splash::Splash;
use tasks::TasksView;
use theme::ACCENT;
use tracker::Tracker;
use updater::{State, Updater};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tarefas,
    Planejamento,
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
    logo: egui::TextureHandle,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        notify::start();
        let store = Store::load();
        let today = store.today_titles(Local::now().date_naive());
        let logo = cc.egui_ctx.load_texture(
            "navi",
            egui::ColorImage::from_rgba_unmultiplied([64, 64], include_bytes!("../assets/icon64.rgba")),
            egui::TextureOptions::LINEAR,
        );
        Self {
            splash: Some(Splash::new(today)),
            tab: Tab::Tarefas,
            store,
            tasks: TasksView::new(),
            planner: Planner::new(),
            coach: CoachView::new(),
            tracker: Tracker::start(),
            updater: Updater::default(),
            logo,
        }
    }

    fn update_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.weak(format!("v{}", updater::VERSION));
        match self.updater.state() {
            State::Checking => {
                ui.weak("Verificando...");
            }
            State::Downloading => {
                ui.weak("Baixando atualização...");
            }
            State::Available { version, url } => {
                if ui.button(format!("Atualizar para v{version}")).clicked() {
                    self.updater.install(url, ctx);
                }
            }
            st => {
                if ui.add(egui::Button::new("Buscar atualizações").frame(false)).clicked() {
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
                ui.image((self.logo.id(), vec2(24.0, 24.0)));
                ui.label(egui::RichText::new("Navi").strong().size(16.0));
                ui.add_space(14.0);
                for (tab, label) in [(Tab::Tarefas, "Tarefas"), (Tab::Planejamento, "Planejamento"), (Tab::Coach, "Coach")] {
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
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let g = if maximized { chrome::Glyph::Restore } else { chrome::Glyph::Maximize };
                    if chrome::window_button(ui, g).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if chrome::window_button(ui, chrome::Glyph::Minimize).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    ui.add_space(8.0);
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

        self.title_bar(ui, maximized);

        match self.tab {
            Tab::Tarefas => self.tasks.show(ui, &mut self.store),
            Tab::Planejamento => {
                if let Some(id) = self.planner.show(ui, &mut self.store) {
                    self.tasks.open(id);
                    self.tab = Tab::Tarefas;
                }
            }
            Tab::Coach => self.coach.show(ui, &self.tracker),
        }

        self.coach.floating(ui, &self.tracker);

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
