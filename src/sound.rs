//! Sons do app: tilintares de sino compostos para o Navi, gerados em memória (sem arquivo de áudio).
//!
//! Cada som é uma frase curta de notas, com sinos sintéticos (seno + harmônicos que decaem).
//! Para usar um som numa situação do app, é só chamar `play(Sound::X)`.

use std::sync::OnceLock;

const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// Arpejo ascendente: abertura do app.
    BemVindo,
    /// Três notas subindo: aviso de tarefa.
    Lembrete,
    /// Dois toques, "ding-dong": aviso de tarefa.
    Chamada,
    /// Cascata descendente: aviso de tarefa.
    Brisa,
    /// Notas repetidas e insistentes: algo no horário.
    Urgente,
    /// Quinta ascendente suave: tarefa concluída.
    Concluido,
}

/// (frequência Hz, início s, duração s)
type Note = (f32, f32, f32);

impl Sound {
    pub const ALL: [Sound; 6] =
        [Sound::BemVindo, Sound::Lembrete, Sound::Chamada, Sound::Brisa, Sound::Urgente, Sound::Concluido];

    /// Nome do som (também o nome do arquivo `.wav` exportado).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn name(self) -> &'static str {
        match self {
            Sound::BemVindo => "Bem-vindo",
            Sound::Lembrete => "Lembrete",
            Sound::Chamada => "Chamada",
            Sound::Brisa => "Brisa",
            Sound::Urgente => "Urgente",
            Sound::Concluido => "Concluido",
        }
    }

    fn notes(self) -> &'static [Note] {
        match self {
            // Dó6 Mi6 Sol6 Dó7 (pentatônica, sobe e abre)
            Sound::BemVindo => &[(1046.5, 0.00, 0.12), (1318.5, 0.11, 0.12), (1568.0, 0.22, 0.12), (2093.0, 0.33, 0.45)],
            // Lá5 Ré6 Lá6
            Sound::Lembrete => &[(880.0, 0.00, 0.14), (1174.7, 0.14, 0.14), (1760.0, 0.28, 0.45)],
            // Mi6 depois Si5: dois toques espaçados
            Sound::Chamada => &[(1318.5, 0.00, 0.30), (987.8, 0.38, 0.55)],
            // Mi7 Dó7 Lá6 Sol6 Mi6: desce em cascata
            Sound::Brisa => &[
                (2637.0, 0.00, 0.08),
                (2093.0, 0.09, 0.08),
                (1760.0, 0.18, 0.08),
                (1568.0, 0.27, 0.08),
                (1318.5, 0.36, 0.40),
            ],
            // Sol6 três vezes e Dó7 por cima
            Sound::Urgente => &[(1568.0, 0.00, 0.08), (1568.0, 0.14, 0.08), (1568.0, 0.28, 0.08), (2093.0, 0.42, 0.40)],
            // Dó6 e Sol6
            Sound::Concluido => &[(1046.5, 0.00, 0.14), (1568.0, 0.16, 0.45)],
        }
    }
}
fn build(sound: Sound) -> Vec<u8> {
    let notes = sound.notes();
    let total = notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max) + 0.35;
    let n = (total * RATE as f32) as usize;
    let mut mix = vec![0.0f32; n];
    for &(freq, at, dur) in notes {
        let ring = dur + 0.3;
        let first = (at * RATE as f32) as usize;
        for k in 0..(ring * RATE as f32) as usize {
            let Some(slot) = mix.get_mut(first + k) else { break };
            let t = k as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let env = attack * (-t * 7.0).exp();
            let w = std::f32::consts::TAU * freq * t;
            *slot += env * (w.sin() + 0.35 * (2.0 * w).sin() + 0.15 * (3.0 * w).sin());
        }
    }
    let peak = mix.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let pcm: Vec<u8> = mix.iter().flat_map(|v| ((v / peak * 0.55 * 32767.0) as i16).to_le_bytes()).collect();

    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

/// WAV do som, gerado uma vez. O buffer é estático porque o Windows lê dele de forma assíncrona.
pub fn wav(sound: Sound) -> &'static [u8] {
    static WAVS: [OnceLock<Vec<u8>>; 6] = [const { OnceLock::new() }; 6];
    let i = Sound::ALL.iter().position(|s| *s == sound).unwrap_or(0);
    WAVS[i].get_or_init(|| build(sound))
}

/// Toca o som sem bloquear.
pub fn play(sound: Sound) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    unsafe {
        PlaySoundW(wav(sound).as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

/// Som de aviso: sorteia entre Lembrete, Chamada e Brisa, sem repetir o anterior.
pub fn notification() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LAST: AtomicUsize = AtomicUsize::new(usize::MAX);
    const POOL: [Sound; 3] = [Sound::Lembrete, Sound::Chamada, Sound::Brisa];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    let mut i = nanos % POOL.len();
    if i == LAST.load(Ordering::Relaxed) {
        i = (i + 1) % POOL.len();
    }
    LAST.store(i, Ordering::Relaxed);
    play(POOL[i]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grava `assets/sounds/<nome>.wav` para ouvir fora do app: `cargo test export_sounds`.
    #[test]
    fn export_sounds() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("sounds");
        std::fs::create_dir_all(&dir).unwrap();
        for s in Sound::ALL {
            let file = format!("{}.wav", s.name().replace(", ", " "));
            std::fs::write(dir.join(file), wav(s)).unwrap();
        }
    }
}