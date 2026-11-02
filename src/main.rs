#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

use coach::CoachView;
use tracker::Tracker;
use updater::{State, Updater};

use chrono::Local;
use eframe::egui;
use model::Store;
use planner::Planner;
use splash::Splash;
use tasks::TasksView;

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
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        notify::start();
        let store = Store::load();
        let today = store.today_titles(Local::now().date_naive());
        Self {
            splash: Some(Splash::new(today)),
            tab: Tab::Tarefas,
            store,
            tasks: TasksView::new(),
            planner: Planner::new(),
            coach: CoachView::new(),
            tracker: Tracker::start(),
            updater: Updater::default(),
        }
    }

    fn update_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        match self.updater.state() {
            State::Idle | State::UpToDate | State::Error(_) => {
                let st = self.updater.state();
                if ui.button("⟳ Verificar atualizações").clicked() {
                    self.updater.check(ctx);
                }
                match st {
                    State::UpToDate => {
                        ui.weak("Você está na versão mais recente");
                    }
                    State::Error(e) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                    }
                    _ => {}
                }
            }
            State::Checking => {
                ui.add(egui::Spinner::new());
                ui.weak("Verificando...");
            }
            State::Available { version, url } => {
                if ui.button(format!("⬇ Atualizar para v{version}")).clicked() {
                    self.updater.install(url, ctx);
                }
            }
            State::Downloading => {
                ui.add(egui::Spinner::new());
                ui.weak("Baixando atualização...");
            }
        }
        ui.weak(format!("v{}", updater::VERSION));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(s) = &mut self.splash {
            if s.show(ui) {
                self.splash = None;
            }
            return;
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Tarefas, "Tarefas");
                ui.selectable_value(&mut self.tab, Tab::Planejamento, "Planejamento");
                ui.selectable_value(&mut self.tab, Tab::Coach, "Coach");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.update_ui(ui);
                });
            });
        });

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
            .with_title("Des_Atento")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
                width: 64,
                height: 64,
            })
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([700.0, 440.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Des_Atento",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
