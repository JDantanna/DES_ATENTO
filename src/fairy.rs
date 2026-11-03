//! A fadinha do app, desenhada em código: orbe brilhante, dois pés e duas asas compridas em forma de folha.
//! As asas batem como nas referências: abertas em V largo, depois fechadas quase na vertical.

use eframe::egui::{Color32, Painter, Pos2, Shape, Stroke, vec2};

/// `flap` vai de -1 a 1 (-1 asas fechadas, 1 abertas); `glow` de 0 a 1 (brilho extra).
pub fn draw(p: &Painter, c: Pos2, size: f32, flap: f32, glow: f32) {
    let r = size * 0.17;
    // Ângulo de cada asa em relação à vertical: ~10° fechada, ~65° aberta.
    let open = 0.5 + 0.5 * flap;
    let theta = 0.17 + 0.96 * open;
    let (len, half) = (size * 0.66, size * 0.07);
    for side in [-1.0f32, 1.0] {
        let dir = vec2(side * theta.sin(), -theta.cos());
        let perp = vec2(-dir.y, dir.x);
        // Folha: estreita na base, mais larga a ~35% do comprimento, ponta fina.
        let edge = |t: f32| half * (std::f32::consts::PI * t.powf(0.75)).sin();
        let mut pts: Vec<Pos2> = (0..=14).map(|k| k as f32 / 14.0).map(|t| c + dir * (len * t) + perp * edge(t)).collect();
        pts.extend((0..=14).rev().map(|k| k as f32 / 14.0).map(|t| c + dir * (len * t) - perp * edge(t)));
        p.add(Shape::convex_polygon(
            pts,
            Color32::from_rgba_unmultiplied(214, 238, 255, 190),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(240, 250, 255, 210)),
        ));
    }
    // Pés
    for side in [-1.0f32, 1.0] {
        p.circle_filled(c + vec2(side * r * 0.62, r * 1.18), r * 0.42, Color32::from_rgb(190, 228, 250));
    }
    for i in (1..=5).rev() {
        let k = i as f32 / 5.0;
        let alpha = ((14.0 + 30.0 * glow) * (1.1 - k)) as u8;
        p.circle_filled(c, r * (1.0 + 1.5 * k), Color32::from_rgba_unmultiplied(130, 205, 255, alpha));
    }
    p.circle_filled(c, r, Color32::from_rgb(190, 230, 255));
    p.circle_filled(c + vec2(-r * 0.2, -r * 0.2), r * 0.55, Color32::WHITE);
}
