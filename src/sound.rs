//! "Hey, listen!" como um tilintar de sinos, gerado em memória (sem arquivo de áudio).
//!
//! São só as notas da frase: "Hey!" sobe rápido e "Lis-ten!" sobe de novo em duas
//! notas, a última mais longa. Cada nota é um sino sintético (seno + harmônicos que
//! decaem).

use std::sync::OnceLock;

const RATE: u32 = 22_050;

/// (frequência Hz, início s, duração s)
const NOTES: [(f32, f32, f32); 5] = [
    (1318.5, 0.00, 0.11), // Mi6   "Hey"
    (1760.0, 0.09, 0.16), // Lá6
    (1568.0, 0.42, 0.11), // Sol6  "Lis"
    (1975.5, 0.52, 0.12), // Si6   "ten"
    (2637.0, 0.62, 0.45), // Mi7
];

fn build() -> Vec<u8> {
    let total = NOTES.iter().map(|n| n.1 + n.2).fold(0.0, f32::max) + 0.35;
    let n = (total * RATE as f32) as usize;
    let mut mix = vec![0.0f32; n];
    for (freq, at, dur) in NOTES {
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

/// Toca o som sem bloquear. O buffer é estático porque o Windows lê dele de forma assíncrona.
pub fn hey_listen() {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(build);
    unsafe {
        PlaySoundW(wav.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
