//! Desenha as telas sem abrir janela, em várias larguras, para conferir o layout onde não há monitor (no CI).
//! Com a variável `NAVI_CAPTURAS` apontando para uma pasta, grava cada tela como PNG; sem ela, só confere
//! que nenhuma tela quebra em nenhum tamanho.

use std::collections::HashMap;

use chrono::{Duration, Local, NaiveTime};
use eframe::egui::{self, Color32, TextureId, epaint::Primitive};

use crate::model::{Repeat, Settings, Store, Subtask};
use crate::planner::Mode;
use crate::splash::Splash;
use crate::{App, Tab, theme};

type Atlas = HashMap<TextureId, (usize, usize, Vec<Color32>)>;

/// Tarefas de exemplo, com os casos que costumam quebrar o layout: títulos longos, muitas tags, informações.
fn sample() -> (Store, u64) {
    let today = Local::now().date_naive();
    let mut s = Store::default();
    let at = |h, m| NaiveTime::from_hms_opt(h, m, 0);
    let late = s.add("Pagar o boleto do condomínio".into(), Some(today - Duration::days(2)));
    let main = s.add("Reunião de planejamento do trimestre com a equipe inteira e os fornecedores".into(), Some(today));
    let call = s.add("Ligar para o dentista".into(), Some(today));
    let plain = s.add("Comprar pão".into(), Some(today));
    let gym = s.add("Academia".into(), Some(today + Duration::days(1)));
    let week = s.add("Entregar o relatório".into(), Some(today + Duration::days(4)));
    let later = s.add("Renovar o passaporte".into(), Some(today + Duration::days(30)));
    s.add("Ideias para o fim de semana".into(), None);
    let done = s.add("Responder os e-mails".into(), Some(today));
    for t in &mut s.tasks {
        if t.id == late {
            t.tags = vec!["casa".into()];
            t.time = at(9, 0);
        } else if t.id == main {
            t.time = at(14, 30);
            t.notify = Some(15);
            t.tags = vec!["trabalho".into(), "urgente".into(), "planejamento-trimestral-2026".into()];
            t.notes = "Levar os números do último trimestre.\nConfirmar a sala e avisar quem participa remotamente.".into();
            t.subtasks = vec![Subtask { title: "Pauta".into(), done: true }, Subtask { title: "Slides".into(), done: false }];
            t.color = [236, 112, 99];
        } else if t.id == call {
            t.time = at(16, 0);
            t.tags = vec!["saúde".into()];
            t.color = [88, 196, 141];
        } else if t.id == plain {
            t.time = None;
        } else if t.id == gym {
            t.time = at(7, 0);
            t.repeat = Repeat::Daily;
        } else if t.id == week || t.id == later {
            t.tags = vec!["trabalho".into()];
        } else if t.id == done {
            t.done = true;
        }
        t.set_anchor();
    }
    (s, main)
}

fn frame(app: &mut App, ctx: &egui::Context, atlas: &mut Atlas, size: egui::Vec2, time: f64) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        time: Some(time),
        predicted_dt: 1.0 / 30.0,
        focused: true,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| app.root(ui));
    for (id, deltas) in &out.textures_delta.set {
        for delta in deltas {
            let egui::ImageData::Color(image) = &delta.image;
            let [w, h] = image.size;
            match delta.pos {
                None => {
                    atlas.insert(*id, (w, h, image.pixels.clone()));
                }
                Some([x0, y0]) => {
                    let tex = atlas.get_mut(id).expect("atualização de textura que não existe");
                    for y in 0..h {
                        for x in 0..w {
                            tex.2[(y0 + y) * tex.0 + x0 + x] = image.pixels[y * w + x];
                        }
                    }
                }
            }
        }
    }
    out
}

/// Rasterizador mínimo: triângulos com cor por vértice e textura, com a mesma mistura do renderizador real.
fn rasterize(ctx: &egui::Context, out: egui::FullOutput, atlas: &Atlas, width: usize, height: usize) -> Vec<u8> {
    let mut px = vec![[0.0f32, 0.0, 0.0, 255.0]; width * height];
    for prim in ctx.tessellate(out.shapes, out.pixels_per_point) {
        let Primitive::Mesh(mesh) = prim.primitive else { continue };
        let Some((tw, th, tex)) = atlas.get(&mesh.texture_id) else { continue };
        let clip = prim.clip_rect;
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [mesh.vertices[tri[0] as usize], mesh.vertices[tri[1] as usize], mesh.vertices[tri[2] as usize]];
            let area = (b.pos.x - a.pos.x) * (c.pos.y - a.pos.y) - (b.pos.y - a.pos.y) * (c.pos.x - a.pos.x);
            if area.abs() < 1e-6 {
                continue;
            }
            let x0 = a.pos.x.min(b.pos.x).min(c.pos.x).max(clip.left()).max(0.0).floor() as usize;
            let y0 = a.pos.y.min(b.pos.y).min(c.pos.y).max(clip.top()).max(0.0).floor() as usize;
            let x1 = (a.pos.x.max(b.pos.x).max(c.pos.x).min(clip.right()).ceil().max(0.0) as usize).min(width);
            let y1 = (a.pos.y.max(b.pos.y).max(c.pos.y).min(clip.bottom()).ceil().max(0.0) as usize).min(height);
            for y in y0..y1 {
                for x in x0..x1 {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let wa = ((b.pos.x - fx) * (c.pos.y - fy) - (b.pos.y - fy) * (c.pos.x - fx)) / area;
                    let wb = ((c.pos.x - fx) * (a.pos.y - fy) - (c.pos.y - fy) * (a.pos.x - fx)) / area;
                    let wc = 1.0 - wa - wb;
                    if wa < 0.0 || wb < 0.0 || wc < 0.0 || !clip.contains(egui::pos2(fx, fy)) {
                        continue;
                    }
                    let mix = |pa: u8, pb: u8, pc: u8| pa as f32 * wa + pb as f32 * wb + pc as f32 * wc;
                    let u = a.uv.x * wa + b.uv.x * wb + c.uv.x * wc;
                    let v = a.uv.y * wa + b.uv.y * wb + c.uv.y * wc;
                    let texel = tex[((v * *th as f32) as usize).min(th - 1) * tw + ((u * *tw as f32) as usize).min(tw - 1)];
                    let src = [
                        mix(a.color.r(), b.color.r(), c.color.r()) * texel.r() as f32 / 255.0,
                        mix(a.color.g(), b.color.g(), c.color.g()) * texel.g() as f32 / 255.0,
                        mix(a.color.b(), b.color.b(), c.color.b()) * texel.b() as f32 / 255.0,
                        mix(a.color.a(), b.color.a(), c.color.a()) * texel.a() as f32 / 255.0,
                    ];
                    let dst = &mut px[y * width + x];
                    let keep = 1.0 - src[3] / 255.0;
                    for i in 0..3 {
                        dst[i] = src[i] + dst[i] * keep;
                    }
                }
            }
        }
    }
    px.iter().flat_map(|p| [p[0] as u8, p[1] as u8, p[2] as u8, 255]).collect()
}

/// Vários quadros seguidos (as animações terminam e os tamanhos se acomodam) e a imagem do último.
fn capture(app: &mut App, ctx: &egui::Context, atlas: &mut Atlas, name: &str, w: usize, h: usize, clock: &mut f64) {
    let size = egui::vec2(w as f32, h as f32);
    let mut last = None;
    // A abertura leva vários segundos para mostrar todas as linhas.
    for _ in 0..if name == "abertura" { 48 } else { 6 } {
        *clock += 0.25;
        last = Some(frame(app, ctx, atlas, size, *clock));
    }
    let out = last.expect("ao menos um quadro");
    let Some(dir) = std::env::var_os("NAVI_CAPTURAS") else { return };
    let pixels = rasterize(ctx, out, atlas, w, h);
    let path = std::path::Path::new(&dir).join(format!("{w}x{h}_{name}.png"));
    image::save_buffer(&path, &pixels, w as u32, h as u32, image::ColorType::Rgba8).expect("gravar a captura");
}

#[test]
fn telas_em_todos_os_tamanhos() {
    if let Some(dir) = std::env::var_os("NAVI_CAPTURAS") {
        std::fs::create_dir_all(dir).expect("criar a pasta das capturas");
    }
    // Os dados de exemplo nunca podem tocar nas tarefas de verdade de quem roda o teste.
    // SAFETY: nenhum outro teste depende de APPDATA, e o acesso às variáveis de ambiente passa pela mesma trava da biblioteca padrão.
    unsafe { std::env::set_var("APPDATA", std::env::temp_dir().join("navi_capturas")) };
    let mut clock = 10.0;
    for (w, h) in [(1280, 780), (1000, 640), (760, 560), (480, 420)] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut atlas = Atlas::new();
        let (mut store, main) = sample();
        store.dirty = false;
        let day = Local::now().date_naive();
        let splash = Splash::new(store.today_titles(day), store.pending_before(day));
        let mut app = App::with(store, Settings::default(), Some(splash));

        capture(&mut app, &ctx, &mut atlas, "abertura", w, h, &mut clock);
        app.splash = None;
        capture(&mut app, &ctx, &mut atlas, "tarefas", w, h, &mut clock);
        app.tasks.open(main);
        capture(&mut app, &ctx, &mut atlas, "tarefas_detalhes", w, h, &mut clock);
        app.tab = Tab::Calendario;
        for (mode, name) in [(Mode::Mes, "mes"), (Mode::Semana, "semana"), (Mode::Dia, "dia")] {
            app.planner.mode = mode;
            capture(&mut app, &ctx, &mut atlas, name, w, h, &mut clock);
        }
        app.tab = Tab::Monitoramento;
        capture(&mut app, &ctx, &mut atlas, "monitoramento", w, h, &mut clock);
    }
}
