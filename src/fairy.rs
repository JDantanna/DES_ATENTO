//! A fadinha do app, desenhada em código: orbe brilhante com quatro asas que batem.

use eframe::egui::{Color32, Painter, Pos2, Shape, Stroke, vec2};

/// `flap` vai de -1 a 1 (abertura das asas); `glow` de 0 a 1 (brilho extra).
pub fn draw(p: &Painter, c: Pos2, size: f32, flap: f32, glow: f32) {
    // (ângulo em rad, comprimento, largura), relativos ao tamanho
    let wings = [(-2.36f32, 0.50f32, 0.20f32), (-0.78, 0.50, 0.20), (2.2, 0.34, 0.15), (0.94, 0.34, 0.15)];
    let open = 0.7 + 0.3 * flap;
    for (ang, len, wid) in wings {
        let dir = vec2(ang.cos(), ang.sin());
        let perp = vec2(-dir.y, dir.x);
        let l = size * len * open;
        let w = size * wid;
        let pts: Vec<Pos2> = (0..28)
            .map(|k| {
                let th = std::f32::consts::TAU * k as f32 / 28.0;
                c + dir * (l * (0.5 + 0.5 * th.cos())) + perp * (0.5 * w * th.sin())
            })
            .collect();
        p.add(Shape::convex_polygon(
            pts,
            Color32::from_rgba_unmultiplied(205, 235, 255, 150),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(235, 248, 255, 200)),
        ));
    }
    let r = size * 0.2;
    for i in (1..=5).rev() {
        let k = i as f32 / 5.0;
        let alpha = ((14.0 + 30.0 * glow) * (1.1 - k)) as u8;
        p.circle_filled(c, r * (1.0 + 1.5 * k), Color32::from_rgba_unmultiplied(130, 205, 255, alpha));
    }
    p.circle_filled(c, r, Color32::from_rgb(190, 230, 255));
    p.circle_filled(c + vec2(-r * 0.2, -r * 0.2), r * 0.55, Color32::WHITE);
}
