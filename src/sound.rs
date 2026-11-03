//! Sons da Navi como tilintares de sinos, gerados em memória (sem arquivo de áudio).
//!
//! Cada som é uma frase de poucas notas, na mesma estética do "Hey, listen!" original
//! deste app: sinos sintéticos (seno + harmônicos que decaem). Os nomes seguem as falas
//! da Navi; para usar um som numa situação do app, é só chamar `play(Sound::X)`.

use std::sync::OnceLock;

const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Hey,
    Listen,
    HeyListen,
    Hello,
    WatchOut,
}

/// (frequência Hz, início s, duração s)
type Note = (f32, f32, f32);

impl Sound {
    pub const ALL: [Sound; 5] = [Sound::Hey, Sound::Listen, Sound::HeyListen, Sound::Hello, Sound::WatchOut];

    /// Nome do som (também o nome do arquivo `.wav` exportado).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn name(self) -> &'static str {
        match self {
            Sound::Hey => "Hey",
            Sound::Listen => "Listen",
            Sound::HeyListen => "Hey, listen",
            Sound::Hello => "Hello",
            Sound::WatchOut => "Watch out",
        }
    }

    fn notes(self) -> &'static [Note] {
        match self {
            // sobe rápido: Mi6 -> Lá6
            Sound::Hey => &[(1318.5, 0.00, 0.11), (1760.0, 0.09, 0.16)],
            // "Lis-ten": Sol6, Si6, Mi7 (a última mais longa)
            Sound::Listen => &[(1568.0, 0.00, 0.11), (1975.5, 0.10, 0.12), (2637.0, 0.20, 0.45)],
            Sound::HeyListen => &[
                (1318.5, 0.00, 0.11),
                (1760.0, 0.09, 0.16),
                (1568.0, 0.42, 0.11),
                (1975.5, 0.52, 0.12),
                (2637.0, 0.62, 0.45),
            ],
            // "He-llo": curto, sobe e abre num Mi7 sustentado
            Sound::Hello => &[(1568.0, 0.00, 0.10), (2093.0, 0.11, 0.12), (2637.0, 0.23, 0.40)],
            // "Watch out!": insistente, notas repetidas e rápidas, mais agudas
            Sound::WatchOut => &[
                (1975.5, 0.00, 0.07),
                (1975.5, 0.09, 0.07),
                (1975.5, 0.18, 0.07),
                (2637.0, 0.27, 0.30),
            ],
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
    static WAVS: [OnceLock<Vec<u8>>; 5] = [const { OnceLock::new() }; 5];
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