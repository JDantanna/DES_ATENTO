use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId};

use crate::theme::ACCENT;

const HEAD_FADE_IN: f32 = 0.8;
/// Instante em que a primeira tarefa começa a surgir.
const FIRST_TASK_AT: f32 = 1.5;
const TASK_FADE_IN: f32 = 0.6;
const END_HOLD: f32 = 1.6;
const FADE_OUT: f32 = 0.7;
const MAX_LINES: usize = 8;
pub const DIAS: [&str; 7] = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
];
pub const MESES: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro",
    "outubro", "novembro", "dezembro",
];

pub struct Splash {
    start: Option<f64>,
    header: String,
    tasks: Vec<String>,
    /// Intervalo entre o surgimento de uma tarefa e a seguinte.
    gap: f32,
}

impl Splash {
    pub fn new(mut tasks: Vec<String>) -> Self {
        let d = Local::now().date_naive();
        let header = format!(
            "Hoje é {}, {:02} de {} de {}.",
            DIAS[d.weekday().num_days_from_monday() as usize],
            d.day(),
            MESES[d.month0() as usize],
            d.year()
        );
        if tasks.is_empty() {
            tasks.push("Nenhuma tarefa por enquanto.".to_owned());
        }
        if tasks.len() > MAX_LINES {
            let extra = tasks.len() - (MAX_LINES - 1);
            tasks.truncate(MAX_LINES - 1);
            tasks.push(format!("e mais {extra}..."));
        }
        let gap = (0.5f32).min(3.5 / tasks.len() as f32);
        Self { start: None, header, tasks, gap }
    }

    fn task_start(&self, i: usize) -> f32 {
        FIRST_TASK_AT + i as f32 * self.gap
    }

    fn fade_out_at(&self) -> f32 {
        self.task_start(self.tasks.len() - 1) + TASK_FADE_IN + END_HOLD
    }

    /// Desenha a abertura. Retorna `true` quando terminou (ou foi pulada).
    pub fn show(&mut self, root: &mut egui::Ui) -> bool {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        let now = ctx.input(|i| i.time);
        let start = *self.start.get_or_insert(now);
        let t = (now - start) as f32;

        let skip = ctx.input(|i| {
            i.pointer.any_click() || i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)
        });
        let out_at = self.fade_out_at();
        if skip || t >= out_at + FADE_OUT {
            return true;
        }
        let master = 1.0 - ((t - out_at) / FADE_OUT).clamp(0.0, 1.0);

        egui::CentralPanel::default().show(root, |ui| {
            let rect = ui.max_rect();
            let line = 42.0;
            let block = 84.0 + 24.0 + self.tasks.len() as f32 * line;
            let top = rect.center().y - block / 2.0;
            let x = rect.center().x;
            let paint = |text: &str, y: f32, size: f32, a: f32, color: Color32| {
                let a = (a * master).clamp(0.0, 1.0);
                let rise = (1.0 - a) * 8.0;
                ui.painter().text(
                    egui::pos2(x, y + rise),
                    Align2::CENTER_TOP,
                    text,
                    FontId::proportional(size),
                    color.gamma_multiply(a),
                );
            };
            paint(
                &format!("{}\nSuas tarefas para hoje são", self.header),
                top,
                28.0,
                smooth(t / HEAD_FADE_IN),
                Color32::WHITE,
            );
            for (i, task) in self.tasks.iter().enumerate() {
                let a = smooth((t - self.task_start(i)) / TASK_FADE_IN);
                paint(task, top + 108.0 + i as f32 * line, 30.0, a, Color32::WHITE);
            }
            ui.painter().text(
                rect.center_bottom() - egui::vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "clique para pular",
                FontId::proportional(12.0),
                ACCENT.gamma_multiply(0.5 * master),
            );
        });
        ctx.request_repaint();
        false
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}