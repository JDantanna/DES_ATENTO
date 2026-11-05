//! Moldura própria da janela: fundo arredondado, barra de título, botões de
//! janela e áreas de redimensionamento (a janela do Windows é sem decoração).

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Response, Sense, Stroke, vec2};
use egui::viewport::ResizeDirection as Dir;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{IsZoomed, WM_NCCALCSIZE};

pub const RADIUS: f32 = 14.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
    Refresh,
    Trash,
    /// Seta para cima: ordem crescente.
    SortUp,
    /// Seta para baixo: ordem decrescente.
    SortDown,
    /// Divisa para cima: reduzir.
    ChevronUp,
    /// Divisa para baixo: expandir.
    ChevronDown,
    /// Divisa para a direita: seção recolhida.
    ChevronRight,
    /// Seta para a esquerda: voltar.
    Back,
    Plus,
    /// Sol: tarefas de hoje.
    Today,
    /// Calendário: próximos dias.
    Upcoming,
    /// Linhas: todas as tarefas.
    All,
    /// Círculo com visto: concluídas.
    Done,
    /// Pontos que indicam que a linha pode ser arrastada.
    Grip,
    /// Sino: a tarefa tem aviso.
    Bell,
    /// Setas em círculo, pequenas: a tarefa se repete.
    Repeat,
    /// Visto: itens do checklist.
    Check,
}

pub fn window_button(ui: &mut egui::Ui, glyph: Glyph) -> Response {
    icon_button(ui, glyph, 0.0)
}

/// Botão de ícone desenhado em código. `spin` gira o ícone (em radianos; só o `Refresh`).
pub fn icon_button(ui: &mut egui::Ui, glyph: Glyph, spin: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 28.0), Sense::click());
    let hot = resp.hovered();
    let bg = match (hot, glyph) {
        (true, Glyph::Close) => Some(Color32::from_rgb(200, 60, 70)),
        (true, Glyph::Trash) => Some(Color32::from_rgba_unmultiplied(200, 60, 70, 90)),
        (true, _) => Some(Color32::from_white_alpha(22)),
        _ => None,
    };
    if let Some(bg) = bg {
        ui.painter().rect_filled(rect.shrink2(vec2(2.0, 0.0)), 7.0, bg);
    }
    let col = if hot { Color32::WHITE } else { Color32::from_gray(185) };
    paint_glyph(ui.painter(), rect.center(), glyph, col, spin);
    resp
}

/// Desenha um ícone centrado em `c`, sem fundo nem área de clique.
pub fn paint_glyph(p: &egui::Painter, c: Pos2, glyph: Glyph, col: Color32, spin: f32) {
    let st = Stroke::new(1.4, col);
    match glyph {
        Glyph::ChevronRight => {
            p.line_segment([c + vec2(-2.5, -5.0), c + vec2(2.5, 0.0)], st);
            p.line_segment([c + vec2(-2.5, 5.0), c + vec2(2.5, 0.0)], st);
        }
        Glyph::Back => {
            p.line_segment([c + vec2(-6.0, 0.0), c + vec2(6.0, 0.0)], st);
            p.line_segment([c + vec2(-6.0, 0.0), c + vec2(-1.5, -4.5)], st);
            p.line_segment([c + vec2(-6.0, 0.0), c + vec2(-1.5, 4.5)], st);
        }
        Glyph::Plus => {
            p.line_segment([c + vec2(-5.5, 0.0), c + vec2(5.5, 0.0)], st);
            p.line_segment([c + vec2(0.0, -5.5), c + vec2(0.0, 5.5)], st);
        }
        Glyph::Today => {
            p.circle_stroke(c, 3.2, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * 5.2, c + d * 7.0], st);
            }
        }
        Glyph::Upcoming => {
            p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 0.5), vec2(13.0, 12.0)), 2.5, st, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-6.5, -2.0), c + vec2(6.5, -2.0)], st);
            p.line_segment([c + vec2(-3.0, -7.0), c + vec2(-3.0, -4.0)], st);
            p.line_segment([c + vec2(3.0, -7.0), c + vec2(3.0, -4.0)], st);
        }
        Glyph::All => {
            for y in [-4.5, 0.0, 4.5] {
                p.circle_filled(c + vec2(-5.5, y), 1.1, col);
                p.line_segment([c + vec2(-2.5, y), c + vec2(6.5, y)], st);
            }
        }
        Glyph::Done => {
            p.circle_stroke(c, 6.8, st);
            p.line_segment([c + vec2(-3.2, 0.2), c + vec2(-0.8, 2.6)], st);
            p.line_segment([c + vec2(-0.8, 2.6), c + vec2(3.4, -2.6)], st);
        }
        Glyph::Grip => {
            for x in [-2.0, 2.0] {
                for y in [-4.0, 0.0, 4.0] {
                    p.circle_filled(c + vec2(x, y), 0.95, col);
                }
            }
        }
        Glyph::Bell => {
            let pts = vec![c + vec2(-3.6, 2.4), c + vec2(-3.0, -2.0), c + vec2(-1.4, -4.0), c + vec2(1.4, -4.0), c + vec2(3.0, -2.0), c + vec2(3.6, 2.4)];
            p.add(egui::Shape::line(pts, Stroke::new(1.1, col)));
            p.line_segment([c + vec2(-4.8, 2.6), c + vec2(4.8, 2.6)], Stroke::new(1.1, col));
            p.circle_filled(c + vec2(0.0, 4.3), 0.9, col);
        }
        Glyph::Repeat => {
            let r = 4.2;
            let thin = Stroke::new(1.1, col);
            for start in [0.3f32, 0.3 + std::f32::consts::PI] {
                let pts: Vec<_> = (0..=8).map(|i| {
                    let a = start + 2.4 * i as f32 / 8.0;
                    c + vec2(a.cos(), a.sin()) * r
                }).collect();
                let tip = pts[8];
                let a = start + 2.4;
                let (dir, out) = (vec2(-a.sin(), a.cos()), vec2(a.cos(), a.sin()));
                p.add(egui::Shape::line(pts, thin));
                p.line_segment([tip + dir * 1.2, tip - dir * 1.2 + out * 2.0], thin);
                p.line_segment([tip + dir * 1.2, tip - dir * 1.2 - out * 2.0], thin);
            }
        }
        Glyph::Check => {
            let thin = Stroke::new(1.3, col);
            p.line_segment([c + vec2(-3.8, 0.2), c + vec2(-1.0, 3.0)], thin);
            p.line_segment([c + vec2(-1.0, 3.0), c + vec2(4.0, -3.0)], thin);
        }
        Glyph::Minimize => {
            p.line_segment([c + vec2(-5.0, 3.0), c + vec2(5.0, 3.0)], st);
        }
        Glyph::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), 2.0, st, egui::StrokeKind::Middle);
        }
        Glyph::Restore => {
            p.rect_stroke(Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(8.0, 8.0)), 2.0, st, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-1.0, -4.5), c + vec2(5.0, -4.5)], st);
            p.line_segment([c + vec2(5.0, -4.5), c + vec2(5.0, 2.0)], st);
        }
        Glyph::Close => {
            p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], st);
            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], st);
        }
        Glyph::Refresh => {
            // Arco de 280° com ponta de seta: o símbolo universal de atualizar.
            let r = 6.0;
            let a0 = spin - 1.0;
            let a1 = a0 + 4.9;
            let pt = |a: f32| c + vec2(a.cos(), a.sin()) * r;
            let pts: Vec<_> = (0..=18).map(|i| pt(a0 + (a1 - a0) * i as f32 / 18.0)).collect();
            p.add(egui::Shape::line(pts, st));
            let tip = pt(a1);
            let dir = vec2(-a1.sin(), a1.cos());
            let out = vec2(a1.cos(), a1.sin());
            p.add(egui::Shape::convex_polygon(
                vec![tip + dir * 4.0, tip + out * 3.4 - dir * 0.8, tip - out * 3.4 - dir * 0.8],
                col,
                Stroke::NONE,
            ));
        }
        Glyph::SortUp | Glyph::SortDown => {
            let s = if glyph == Glyph::SortUp { -1.0 } else { 1.0 };
            p.line_segment([c + vec2(0.0, -6.0 * s), c + vec2(0.0, 6.0 * s)], st);
            p.line_segment([c + vec2(-4.5, 1.5 * s), c + vec2(0.0, 6.0 * s)], st);
            p.line_segment([c + vec2(4.5, 1.5 * s), c + vec2(0.0, 6.0 * s)], st);
        }
        Glyph::ChevronUp | Glyph::ChevronDown => {
            let s = if glyph == Glyph::ChevronUp { -1.0 } else { 1.0 };
            p.line_segment([c + vec2(-5.0, -2.5 * s), c + vec2(0.0, 2.5 * s)], st);
            p.line_segment([c + vec2(5.0, -2.5 * s), c + vec2(0.0, 2.5 * s)], st);
        }
        Glyph::Trash => {
            p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], st); // tampa
            p.rect_stroke(Rect::from_min_max(c + vec2(-2.0, -6.5), c + vec2(2.0, -4.0)), 1.0, st, egui::StrokeKind::Middle); // alça
            p.add(egui::Shape::closed_line(
                vec![c + vec2(-4.6, -2.0), c + vec2(4.6, -2.0), c + vec2(3.8, 6.5), c + vec2(-3.8, 6.5)],
                st,
            )); // corpo
            let thin = Stroke::new(1.1, col);
            p.line_segment([c + vec2(-1.5, 0.0), c + vec2(-1.3, 4.5)], thin);
            p.line_segment([c + vec2(1.5, 0.0), c + vec2(1.3, 4.5)], thin);
        }
    }
}

/// Área de layout com limite próprio: o que passar de `rect` é cortado, em vez de invadir o vizinho.
pub fn region<R>(ui: &mut egui::Ui, rect: Rect, salt: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let builder = egui::UiBuilder::new().id_salt(salt).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min));
    let mut child = ui.new_child(builder);
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child)
}

pub fn close_button(ui: &mut egui::Ui) -> Response {
    window_button(ui, Glyph::Close)
}

/// Sem isto, o winit reserva 1 px no topo das janelas sem decoração (para ter a sombra do sistema), e o
/// Windows pinta esse pixel de preto: era a linha preta acima do widget. Aqui a janela inteira vira área útil.
/// Maximizada, a janela segue a regra do winit, que limita o tamanho à área de trabalho do monitor.
unsafe extern "system" fn no_top_line(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    unsafe {
        if msg == WM_NCCALCSIZE && wparam != 0 && IsZoomed(hwnd) == 0 {
            return 0;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

/// Tira da janela a moldura que o Windows desenha em janelas sem decoração (sombra, borda e a linha preta
/// de 1 px no topo), que aparecia atrás dos cantos arredondados. Retorna `false` se a janela ainda não existe.
/// Deve ser chamada na thread da janela (a da interface).
pub fn flatten_window(title: &str) -> bool {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::Controls::MARGINS;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let none = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 0 };
        DwmExtendFrameIntoClientArea(hwnd, &none);
        // DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED; DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND.
        let one: i32 = 1;
        DwmSetWindowAttribute(hwnd, 2, &one as *const i32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 33, &one as *const i32 as *const c_void, 4);
        // Passa a responder o cálculo da moldura e pede para o Windows refazê-lo já.
        SetWindowSubclass(hwnd, Some(no_top_line), 1, 0);
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    true
}

/// Faixas finas nas bordas e cantos que iniciam o redimensionamento.
pub fn resize_handles(ui: &mut egui::Ui, full: Rect) {
    let (e, c) = (5.0, 12.0);
    let zones = [
        (Rect::from_min_max(full.left_top() + vec2(c, 0.0), full.right_top() + vec2(-c, e)), Dir::North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(full.left_bottom() + vec2(c, -e), full.right_bottom() + vec2(-c, 0.0)), Dir::South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(full.left_top() + vec2(0.0, c), full.left_bottom() + vec2(e, -c)), Dir::West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(full.right_top() + vec2(-e, c), full.right_bottom() + vec2(0.0, -c)), Dir::East, CursorIcon::ResizeHorizontal),
        (Rect::from_min_size(full.left_top(), vec2(c, c)), Dir::NorthWest, CursorIcon::ResizeNwSe),
        (Rect::from_min_size(full.right_top() + vec2(-c, 0.0), vec2(c, c)), Dir::NorthEast, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(full.left_bottom() + vec2(0.0, -c), vec2(c, c)), Dir::SouthWest, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(full.right_bottom() - vec2(c, c), vec2(c, c)), Dir::SouthEast, CursorIcon::ResizeNwSe),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let r = ui.interact(rect, ui.scope_id().with(("resize", i)), Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if r.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
