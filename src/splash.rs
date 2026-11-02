use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId};

use crate::theme::ACCENT;

const FADE_IN: f32 = 0.8;
const HOLD: f32 = 1.2;
const FADE_OUT: f32 = 0.6;
const TASK_FADE_IN: f32 = 0.5;
const TASK_HOLD: f32 = 0.9;
const TASK_FADE_OUT: f32 = 0.4;

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
    /// Fala final de cada etapa: cabeçalho primeiro, depois uma por tarefa.
    tasks: Vec<String>,
}

impl Splash {
    pub fn new(tasks: Vec<String>) -> Self {
        let d = Local::now().date_naive();
        let header = format!(
            "Hoje é {}, {:02} de {} de {}.",
            DIAS[d.weekday().num_days_from_monday() as usize],
            d.day(),
            MESES[d.month0() as usize],
            d.year()
        );
        Self { start: None, header, tasks }
    }

    fn total(&self) -> f32 {
        let tasks = if self.tasks.is_empty() {
            TASK_FADE_IN + TASK_HOLD + TASK_FADE_OUT
        } else {
            self.tasks.len() as f32 * (TASK_FADE_IN + TASK_HOLD + TASK_FADE_OUT)
        };
        FADE_IN + HOLD + FADE_OUT + tasks
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
        if skip || t >= self.total() {
            return true;
        }

        let (text, alpha, sub) = if t < FADE_IN + HOLD + FADE_OUT {
            (
                format!("{}\nSuas tarefas para hoje são", self.header),
                phase(t, FADE_IN, HOLD, FADE_OUT),
                false,
            )
        } else {
            let t2 = t - (FADE_IN + HOLD + FADE_OUT);
            let step = TASK_FADE_IN + TASK_HOLD + TASK_FADE_OUT;
            let idx = (t2 / step) as usize;
            let local = t2 - idx as f32 * step;
            let label = self
                .tasks
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "Nenhuma tarefa por enquanto.".to_owned());
            (label, phase(local, TASK_FADE_IN, TASK_HOLD, TASK_FADE_OUT), true)
        };

        egui::CentralPanel::default().show(root, |ui| {
            let rect = ui.max_rect();
            let a = (alpha * 255.0) as u8;
            let (font, color) = if sub {
                (FontId::proportional(34.0), Color32::from_white_alpha(a))
            } else {
                (FontId::proportional(28.0), Color32::from_white_alpha(a))
            };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
            ui.painter().text(
                rect.center_bottom() - egui::vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "clique para pular",
                FontId::proportional(12.0),
                ACCENT.gamma_multiply(0.5),
            );
        });
        ctx.request_repaint();
        false
    }
}

/// Fade-in / permanência / fade-out suavizado, retorna opacidade 0..=1.
fn phase(t: f32, fin: f32, hold: f32, fout: f32) -> f32 {
    let x = if t < fin {
        t / fin
    } else if t < fin + hold {
        1.0
    } else {
        1.0 - ((t - fin - hold) / fout)
    };
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
