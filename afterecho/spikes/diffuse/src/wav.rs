//! Minimal 16-bit PCM WAV reader/writer (no dependency).

use std::fs;
use std::io::Write;
use std::path::Path;

/// Write interleaved-by-channel float audio as 16-bit PCM with TPDF dither.
/// Returns how many samples had to be clipped (should be 0).
pub fn write(path: &Path, sr: u32, channels: &[&[f32]]) -> usize {
    let nch = channels.len();
    let len = channels.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut data = Vec::with_capacity(len * nch * 2);
    let mut clipped = 0;
    let mut rng = 0x1234_5678_u32;
    for i in 0..len {
        for ch in channels {
            let x = ch.get(i).copied().unwrap_or(0.0);
            // TPDF dither, ±1 LSB.
            let d = (next_unit(&mut rng) - next_unit(&mut rng)) / 32768.0;
            let mut v = (x + d) * 32767.0;
            if !(-32768.0..=32767.0).contains(&v) {
                clipped += 1;
                v = v.clamp(-32768.0, 32767.0);
            }
            data.extend_from_slice(&(v.round() as i16).to_le_bytes());
        }
    }
    let mut f = fs::File::create(path).expect("create wav");
    let byte_rate = sr * nch as u32 * 2;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data.len() as u32).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&(nch as u16).to_le_bytes()).unwrap();
    f.write_all(&sr.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&(nch as u16 * 2).to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
    f.write_all(&data).unwrap();
    clipped
}

fn next_unit(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32) / (u32::MAX as f32)
}

/// Read a 16-bit PCM WAV, mixing all channels to mono. Returns (sr, samples).
pub fn read_mono(path: &Path) -> Option<(u32, Vec<f32>)> {
    let b = fs::read(path).ok()?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let (mut sr, mut nch, mut bits) = (0u32, 0usize, 0u16);
    let mut pos = 12;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let size = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            nch = u16::from_le_bytes(b[body + 2..body + 4].try_into().ok()?) as usize;
            sr = u32::from_le_bytes(b[body + 4..body + 8].try_into().ok()?);
            bits = u16::from_le_bytes(b[body + 14..body + 16].try_into().ok()?);
        } else if id == b"data" {
            if bits != 16 || nch == 0 {
                return None;
            }
            let end = (body + size).min(b.len());
            let frames = (end - body) / (2 * nch);
            let mut out = Vec::with_capacity(frames);
            for f in 0..frames {
                let mut s = 0.0;
                for c in 0..nch {
                    let o = body + (f * nch + c) * 2;
                    s += i16::from_le_bytes([b[o], b[o + 1]]) as f32 / 32768.0;
                }
                out.push(s / nch as f32);
            }
            return Some((sr, out));
        }
        pos = body + size + (size & 1);
    }
    None
}
