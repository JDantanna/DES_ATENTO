use chrono::{Datelike, Local};
use eframe::egui::{self, Align2, Color32, FontId};

use crate::theme::ACCENT;

const HEAD_FADE_IN: f32 = 0.8;
/// Instante em que a primeira tarefa começa a surgir.
const FIRST_TASK_AT: f32 = 1.5;
const TASK_FADE_IN: f32 = 0.6;
const LEAVE_FADE: f32 = 0.35;
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
    /// Instante do clique que encerra a abertura (inicia o fade final).
    leaving: Option<f64>,
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
        Self { start: None, leaving: None, header, tasks, gap }
    }

    fn task_start(&self, i: usize) -> f32 {
        FIRST_TASK_AT + i as f32 * self.gap
    }

    /// Desenha a abertura. Retorna `true` quando terminou. Depois que todas as tarefas
    /// aparecem, a tela espera um clique (ou Enter/Esc) para seguir.
    pub fn show(&mut self, root: &mut egui::Ui) -> bool {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        let now = ctx.input(|i| i.time);
        let start = *self.start.get_or_insert(now);
        let t = (now - start) as f32;

        let advance = ctx.input(|i| {
            i.pointer.any_click() || i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)
        });
        if advance && self.leaving.is_none() {
            self.leaving = Some(now);
        }
        let master = match self.leaving {
            Some(at) => 1.0 - ((now - at) as f32 / LEAVE_FADE).clamp(0.0, 1.0),
            None => 1.0,
        };
        if master <= 0.0 {
            return true;
        }
        let all_in = t >= self.task_start(self.tasks.len() - 1) + TASK_FADE_IN;

        egui::CentralPanel::default().show(root, |ui| {
            let rect = ui.max_rect();
            let line = 42.0;
            let block = 84.0 + 24.0 + self.tasks.len() as f32 * line;
            let top = rect.center().y - block / 2.0;
            let x = rect.center().x;
            let paint = |text: &str, pos: egui::Pos2, align: Align2, size: f32, a: f32| {
                let a = (a * master).clamp(0.0, 1.0);
                ui.painter().text(
                    pos + egui::vec2(0.0, (1.0 - a) * 8.0),
                    align,
                    text,
                    FontId::proportional(size),
                    Color32::WHITE.gamma_multiply(a),
                );
            };
            // Título centralizado.
            paint(
                &format!("{}\nSuas tarefas para hoje são", self.header),
                egui::pos2(x, top),
                Align2::CENTER_TOP,
                28.0,
                smooth(t / HEAD_FADE_IN),
            );
            // Tarefas alinhadas à esquerda, num bloco centralizado pela linha mais larga.
            let widest = self
                .tasks
                .iter()
                .map(|s| ui.painter().layout_no_wrap(s.clone(), FontId::proportional(30.0), Color32::WHITE).size().x)
                .fold(0.0, f32::max);
            let left = x - widest / 2.0;
            for (i, task) in self.tasks.iter().enumerate() {
                let a = smooth((t - self.task_start(i)) / TASK_FADE_IN);
                paint(task, egui::pos2(left, top + 108.0 + i as f32 * line), Align2::LEFT_TOP, 30.0, a);
            }
            let (hint, pulse) = if all_in {
                ("clique para continuar", 0.55 + 0.35 * ((t * 3.0).sin() * 0.5 + 0.5))
            } else {
                ("clique para pular", 0.5)
            };
            ui.painter().text(
                rect.center_bottom() - egui::vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                hint,
                FontId::proportional(12.0),
                ACCENT.gamma_multiply(pulse * master),
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