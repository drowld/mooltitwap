//! Listen-proxy measurements. They point at artefacts; the ears decide.

use crate::material::SR;
use realfft::RealFftPlanner;

fn biquad_lp(x: &[f64], f0: f64, q: f64) -> Vec<f64> {
    let w0 = std::f64::consts::TAU * f0 / SR as f64;
    let (s, c) = w0.sin_cos();
    let alpha = s / (2.0 * q);
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = ((1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0);
    let (a1, a2) = (-2.0 * c / a0, (1.0 - alpha) / a0);
    let (mut z1, mut z2) = (0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = b0 * v + z1;
            z1 = b1 * v - a1 * y + z2;
            z2 = b2 * v - a2 * y;
            y
        })
        .collect()
}

/// Amplitude envelope sqrt(2·LP(x²)), 4th-order Butterworth at 100 Hz, so
/// amplitude wobble up to ~100 Hz (flutter, pumping) shows and the carrier
/// does not. Returned at 1 kHz.
pub fn envelope(x: &[f32]) -> Vec<f32> {
    let sq: Vec<f64> = x.iter().map(|&v| v as f64 * v as f64).collect();
    let y = biquad_lp(&biquad_lp(&sq, 100.0, 0.5412), 100.0, 1.3066);
    y.iter().step_by(48).map(|&v| (2.0 * v.max(0.0)).sqrt() as f32).collect()
}

pub struct EnvStats {
    /// RMS of the fast (>~4 Hz) envelope wobble relative to the mean level, dB.
    pub flutter_db: f32,
}

/// Envelope wobble over [a, b) (samples).
pub fn env_stats(x: &[f32], a: usize, b: usize) -> EnvStats {
    let env = envelope(x);
    let (a, b) = (a / 48, (b / 48).min(env.len()));
    let e = &env[a..b];
    // Trend: centred 250 ms moving average.
    let w = 125;
    let mut num = 0.0f64;
    let mut cnt = 0usize;
    let mean = e.iter().map(|&v| v as f64).sum::<f64>() / e.len() as f64;
    for i in w..e.len().saturating_sub(w) {
        let tr = e[i - w..i + w].iter().map(|&v| v as f64).sum::<f64>() / (2 * w) as f64;
        let d = e[i] as f64 - tr;
        num += d * d;
        cnt += 1;
    }
    let fl = (num / cnt.max(1) as f64).sqrt() / mean.max(1e-12);
    EnvStats { flutter_db: 20.0 * (fl.max(1e-9) as f32).log10() }
}

fn hann_spectrum(x: &[f32], a: usize, n: usize) -> Vec<f64> {
    thread_local! {
        static PLANNER: std::cell::RefCell<RealFftPlanner<f64>> = std::cell::RefCell::new(RealFftPlanner::new());
    }
    let fft = PLANNER.with(|p| p.borrow_mut().plan_fft_forward(n));
    let mut buf: Vec<f64> = (0..n)
        .map(|i| {
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            x.get(a + i).copied().unwrap_or(0.0) as f64 * w
        })
        .collect();
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    spec.iter().map(|c| c.norm_sqr()).collect()
}

pub struct SineStats {
    /// Measured pitch vs. target, cents.
    pub pitch_err_c: f32,
    /// Energy away from the tone (sidebands, noise) relative to the tone, dB.
    pub spur_db: f32,
}

/// Pitch and spurs of a sustained sine, from a 2.7 s Hann FFT starting at `a`.
pub fn sine_stats(x: &[f32], a: usize, target_hz: f32) -> SineStats {
    let n = 131_072;
    let p = hann_spectrum(x, a, n);
    let k = (1..p.len() - 1).max_by(|&i, &j| p[i].partial_cmp(&p[j]).unwrap()).unwrap();
    let (l, c, r) = (p[k - 1].ln(), p[k].ln(), p[k + 1].ln());
    let delta = 0.5 * (l - r) / (l - 2.0 * c + r);
    let hz = (k as f64 + delta) * SR as f64 / n as f64;
    let inside: f64 = p[k.saturating_sub(10)..=(k + 10).min(p.len() - 1)].iter().sum();
    let total: f64 = p.iter().sum();
    SineStats {
        pitch_err_c: (1200.0 * (hz / target_hz as f64).log2()) as f32,
        spur_db: (10.0 * ((total - inside).max(1e-30) / inside).log10()) as f32,
    }
}

/// Log-spectral distance to the ideal render, dB (2048-point frames,
/// 40 Hz–12 kHz, frames within 40 dB of the loudest, floor 60 dB under each
/// frame's peak).
pub fn lsd_db(out: &[f32], ideal: &[f32]) -> f32 {
    let n = 2048;
    let hop = 512;
    let len = out.len().min(ideal.len());
    let k_lo = (40.0 * n as f32 / SR) as usize;
    let k_hi = (12_000.0 * n as f32 / SR) as usize;
    let mut frames = Vec::new();
    let mut a = 0;
    while a + n <= len {
        let po = hann_spectrum(out, a, n);
        let pi = hann_spectrum(ideal, a, n);
        let ei: f64 = pi.iter().sum();
        frames.push((po, pi, ei));
        a += hop;
    }
    let emax = frames.iter().map(|f| f.2).fold(0.0, f64::max);
    let mut acc = 0.0;
    let mut cnt = 0;
    for (po, pi, ei) in &frames {
        if *ei < emax * 1e-4 {
            continue;
        }
        // Floor 60 dB under the ideal frame's loudest bin: differences below
        // that are inaudible next to the partials.
        let floor = pi.iter().cloned().fold(0.0, f64::max) * 1e-6;
        let mut s = 0.0;
        for k in k_lo..k_hi {
            let d = 10.0 * ((po[k] + floor).log10() - (pi[k] + floor).log10());
            s += d * d;
        }
        acc += (s / (k_hi - k_lo) as f64).sqrt();
        cnt += 1;
    }
    (acc / cnt.max(1) as f64) as f32
}

fn frame_db(x: &[f32], frame: usize) -> Vec<f32> {
    x.chunks(frame)
        .map(|c| {
            let e = c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32;
            10.0 * (e + 1e-12).log10()
        })
        .collect()
}

pub struct DrumStats {
    /// Correlation of the 2 ms level envelopes (dry vs out), best of ±6 ms.
    pub env_corr: f32,
    /// Mean change of the hit peaks, dB (negative = softened attacks).
    pub attack_db: f32,
    /// Mean change of the level just before each hit, dB (positive = smear /
    /// pre-echo / doubled hits).
    pub pre_db: f32,
}

pub fn drum_stats(dry: &[f32], out: &[f32], onsets: &[usize]) -> DrumStats {
    let f = 96; // 2 ms
    let (d, o) = (frame_db(dry, f), frame_db(out, f));
    let dmax = d.iter().cloned().fold(f32::MIN, f32::max);
    let clamp = |v: f32| v.max(dmax - 60.0);
    let mut best = -1.0f32;
    for sh in -3i32..=3 {
        let pairs: Vec<(f32, f32)> = (3..d.len().min(o.len()) - 3)
            .map(|i| (clamp(d[i]), clamp(o[(i as i32 + sh) as usize])))
            .collect();
        best = best.max(pearson(&pairs));
    }
    let pk = |x: &[f32], a: usize, b: usize| {
        let s = &x[a.min(x.len())..b.min(x.len())];
        let fr = frame_db(s, 48);
        fr.into_iter().fold(-120.0f32, f32::max)
    };
    let pre = |x: &[f32], a: usize, b: usize| {
        let s = &x[a.min(x.len())..b.min(x.len())];
        10.0 * (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32 + 1e-12).log10()
    };
    let (mut att, mut prv) = (0.0, 0.0);
    let mut cnt = 0;
    for &on in onsets {
        if on < 2_400 {
            continue;
        }
        att += pk(out, on - 96, on + 384) - pk(dry, on - 96, on + 384);
        prv += pre(out, on - 1_200, on - 144) - pre(dry, on - 1_200, on - 144);
        cnt += 1;
    }
    let c = cnt.max(1) as f32;
    DrumStats { env_corr: best, attack_db: att / c, pre_db: prv / c }
}

fn pearson(p: &[(f32, f32)]) -> f32 {
    let n = p.len() as f64;
    let (mx, my) = p.iter().fold((0.0, 0.0), |a, &(x, y)| (a.0 + x as f64, a.1 + y as f64));
    let (mx, my) = (mx / n, my / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for &(x, y) in p {
        let (dx, dy) = (x as f64 - mx, y as f64 - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    (sxy / (sxx * syy).sqrt().max(1e-12)) as f32
}

/// Energy the output puts away from the ideal's partials, relative to the
/// energy on them, dB (2.7 s Hann FFT from `a`; a bin counts as "on a
/// partial" within ±10 bins = ±3.7 Hz of any ideal peak above −70 dB).
/// Catches sidebands from amplitude/phase wobble on complex tones.
pub fn spur_vs_ideal(out: &[f32], ideal: &[f32], a: usize) -> f32 {
    let n = 131_072;
    let po = hann_spectrum(out, a, n);
    let pi = hann_spectrum(ideal, a, n);
    let thr = pi.iter().cloned().fold(0.0, f64::max) * 1e-7;
    let mut mask = vec![false; pi.len()];
    for k in 0..pi.len() {
        if pi[k] > thr {
            for j in k.saturating_sub(10)..=(k + 10).min(pi.len() - 1) {
                mask[j] = true;
            }
        }
    }
    let (mut on, mut off) = (0.0, 0.0);
    for k in 1..po.len() {
        if mask[k] {
            on += po[k];
        } else {
            off += po[k];
        }
    }
    (10.0 * (off.max(1e-30) / on.max(1e-30)).log10()) as f32
}

/// Mean frame-to-frame change of the log spectrum over [a, b), dB (2048-pt
/// Hann frames, hop 256, 40 Hz–8 kHz, floor 50 dB under the frame peak).
/// A static chord should barely move; flutter and pumping make it move.
pub fn flux_db(x: &[f32], a: usize, b: usize) -> f32 {
    let n = 2048;
    let k_lo = (40.0 * n as f32 / SR) as usize;
    let k_hi = (8_000.0 * n as f32 / SR) as usize;
    let mut prev: Option<Vec<f64>> = None;
    let (mut acc, mut cnt) = (0.0, 0);
    let mut i = a;
    while i + n <= b.min(x.len()) {
        let p = hann_spectrum(x, i, n);
        let floor = p.iter().cloned().fold(0.0, f64::max) * 1e-5;
        let l: Vec<f64> = p[k_lo..k_hi].iter().map(|&v| 10.0 * (v + floor).log10()).collect();
        if let Some(q) = &prev {
            let s: f64 = l.iter().zip(q).map(|(a, b)| (a - b) * (a - b)).sum();
            acc += (s / l.len() as f64).sqrt();
            cnt += 1;
        }
        prev = Some(l);
        i += 256;
    }
    (acc / cnt.max(1) as f64) as f32
}

/// Lag of `b` relative to `a` (ms, positive = late) from 1 ms level
/// envelopes, searched over ±40 ms.
pub fn best_lag_ms(a: &[f32], b: &[f32]) -> f32 {
    let (ea, eb) = (frame_db(a, 48), frame_db(b, 48));
    let mx = ea.iter().cloned().fold(f32::MIN, f32::max);
    let cl = |v: f32| v.max(mx - 50.0);
    let mut best = (f32::MIN, 0i32);
    for sh in -40i32..=40 {
        let pairs: Vec<(f32, f32)> = (40..ea.len().min(eb.len()) - 40)
            .map(|i| (cl(ea[i]), cl(eb[(i as i32 + sh) as usize])))
            .collect();
        let r = pearson(&pairs);
        if r > best.0 {
            best = (r, sh);
        }
    }
    best.1 as f32
}
