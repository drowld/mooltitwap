//! Test material. Everything is synthesised except the voice, which comes from
//! macOS `say` (internal listening only). The pitched generators take a
//! `ratio` so the same phrase can be rendered at the target pitch: the
//! "ideal" output a perfect shifter would give (same envelopes, every
//! frequency × ratio).

use std::path::Path;
use std::process::Command;

pub const SR: f32 = 48_000.0;

pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    /// Uniform in [-1, 1).
    pub fn bi(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn secs(s: f32) -> usize {
    (s * SR) as usize
}

fn fade(x: &mut [f32], fade_in: f32, fade_out: f32) {
    let (a, b) = (secs(fade_in).max(1), secs(fade_out).max(1));
    let n = x.len();
    for i in 0..a.min(n) {
        x[i] *= i as f32 / a as f32;
    }
    for i in 0..b.min(n) {
        x[n - 1 - i] *= i as f32 / b as f32;
    }
}

/// Sustained sine, A3 = 220 Hz, 4 s.
#[allow(dead_code)]
pub fn sine(ratio: f32) -> Vec<f32> {
    let f = 220.0 * ratio as f64;
    let mut x: Vec<f32> = (0..secs(4.0))
        .map(|i| (std::f64::consts::TAU * f * i as f64 / SR as f64).sin() as f32)
        .collect();
    fade(&mut x, 0.03, 0.03);
    x
}

/// Static pad chord (C3 G3 E4 B4 D5, band-limited saws, soft top), 5 s.
pub fn chord(ratio: f32) -> Vec<f32> {
    let notes = [130.81f64, 196.00, 329.63, 493.88, 587.33];
    let n = secs(5.0);
    let mut x = vec![0.0f32; n];
    let mut rng = Rng::new(7);
    for &f0 in &notes {
        let f0 = f0 * ratio as f64;
        let mut h = 1;
        while f0 * h as f64 * 1.0 < 9_000.0 {
            let fh = f0 * h as f64;
            let amp = (1.0 / h as f64) / (1.0 + (fh / 2_500.0).powi(2)).sqrt();
            let ph = rng.bi() as f64 * std::f64::consts::PI;
            let w = std::f64::consts::TAU * fh / SR as f64;
            for (i, v) in x.iter_mut().enumerate() {
                *v += (amp * (w * i as f64 + ph).sin()) as f32;
            }
            h += 1;
        }
    }
    fade(&mut x, 0.3, 0.5);
    x
}

/// Guitar-like phrase: additive plucked strings (pluck position, per-partial
/// decay, slight stiffness) with a pick click. 8 notes, ≈5 s.
pub fn pluck(ratio: f32) -> Vec<f32> {
    let notes = [164.81f64, 246.94, 329.63, 392.00, 493.88, 440.00, 392.00, 329.63];
    let step = 0.42;
    let n = secs(step * notes.len() as f32 + 1.6);
    let mut x = vec![0.0f32; n];
    let mut rng = Rng::new(11);
    for (k, &f0) in notes.iter().enumerate() {
        let start = secs(step * k as f32);
        let len = n - start;
        let f0 = f0 * ratio as f64;
        let b = 2e-4;
        let mut h = 1;
        loop {
            let hf = h as f64;
            let fh = f0 * hf * (1.0 + b * hf * hf).sqrt();
            if fh > 10_000.0 {
                break;
            }
            // Decay depends on the *source* partial frequency so the ideal
            // keeps the same envelope as the dry phrase.
            let fsrc = fh / ratio as f64;
            let tau = 1.6 / (1.0 + fsrc / 900.0);
            let amp = (hf * std::f64::consts::PI * 0.18).sin().abs() / hf.powf(1.1);
            let w = std::f64::consts::TAU * fh / SR as f64;
            let ph = rng.bi() as f64 * 0.3;
            for i in 0..len {
                let t = i as f64 / SR as f64;
                let env = (-t / tau).exp() * (t / 0.002).min(1.0);
                x[start + i] += (amp * env * (w * i as f64 + ph).sin()) as f32;
            }
            h += 1;
        }
        // Pick click: a few ms of differentiated noise (unpitched).
        let mut prev = 0.0;
        for i in 0..secs(0.006) {
            let t = i as f32 / SR;
            let nz = rng.bi();
            x[start + i] += 0.25 * (nz - prev) * (-t / 0.0012).exp();
            prev = nz;
        }
    }
    fade(&mut x, 0.0, 0.3);
    x
}

/// Two-bar drum loop at 120 BPM (kick, snare, hats). Returns the onsets.
pub fn drums() -> (Vec<f32>, Vec<usize>) {
    let step = 0.125; // 16th notes at 120 BPM
    let n = secs(step * 32.0 + 0.5);
    let mut x = vec![0.0f32; n];
    let mut rng = Rng::new(23);
    let mut onsets = Vec::new();
    let kicks = [0, 6, 8, 16, 22, 24, 27];
    let snares = [4, 12, 20, 28];
    let open = [14, 30];
    for s in 0..32 {
        let start = secs(step * s as f32);
        let mut hit = false;
        if kicks.contains(&s) {
            hit = true;
            let mut ph = 0.0f64;
            for i in 0..secs(0.5).min(n - start) {
                let t = i as f64 / SR as f64;
                let f = 45.0 + 110.0 * (-t / 0.03).exp();
                ph += std::f64::consts::TAU * f / SR as f64;
                let mut v = 0.9 * (-t / 0.28).exp() * ph.sin();
                if t < 0.001 {
                    v += 0.3 * rng.bi() as f64;
                }
                x[start + i] += v as f32;
            }
        }
        if snares.contains(&s) {
            hit = true;
            let mut prev = 0.0;
            for i in 0..secs(0.4).min(n - start) {
                let t = i as f32 / SR;
                let tone = 0.45
                    * (-t / 0.06).exp()
                    * ((TAU32 * 185.0 * t).sin() + 0.5 * (TAU32 * 330.0 * t).sin());
                let nz = rng.bi();
                let noise = 0.55 * (-t / 0.14).exp() * (nz - 0.6 * prev);
                prev = nz;
                x[start + i] += tone + noise;
            }
        }
        if s % 2 == 0 || open.contains(&s) {
            hit = true;
            let (tau, amp) = if open.contains(&s) { (0.18, 0.18) } else { (0.025, 0.22) };
            let (mut p1, mut p2) = (0.0, 0.0);
            for i in 0..secs(0.5).min(n - start) {
                let t = i as f32 / SR;
                let nz = rng.bi();
                // Second difference: a crude high-pass for a metallic hiss.
                let hp = nz - 2.0 * p1 + p2;
                p2 = p1;
                p1 = nz;
                x[start + i] += amp * 0.5 * hp * (-t / tau).exp();
            }
        }
        if hit {
            onsets.push(start);
        }
    }
    (x, onsets)
}

const TAU32: f32 = std::f32::consts::TAU;

/// Speech from macOS `say` (two voices), cached under `material/`.
pub fn voice(dir: &Path) -> Option<Vec<f32>> {
    let lines = [
        ("Samantha", "voice_a.wav", "Every echo comes back a little different."),
        ("Daniel", "voice_b.wav", "Higher, lower, and then it slowly fades away."),
    ];
    std::fs::create_dir_all(dir).ok()?;
    let mut out = Vec::new();
    for (v, file, text) in lines {
        let p = dir.join(file);
        if !p.exists() {
            let ok = Command::new("say")
                .args(["-v", v, "-o"])
                .arg(&p)
                .args(["--file-format=WAVE", "--data-format=LEI16@48000", text])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return None;
            }
        }
        let (sr, mut x) = crate::wav::read_mono(&p)?;
        if sr != SR as u32 {
            return None;
        }
        out.append(&mut x);
        out.extend(std::iter::repeat(0.0).take(secs(0.3)));
    }
    Some(out)
}

/// Scale so the peak sits at `peak`; returns the gain used.
pub fn normalise(x: &mut [f32], peak: f32) -> f32 {
    let m = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let g = if m > 0.0 { peak / m } else { 1.0 };
    x.iter_mut().for_each(|v| *v *= g);
    g
}
