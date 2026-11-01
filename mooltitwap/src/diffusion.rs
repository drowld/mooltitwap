//! Diffuse (v2.47): six short Schroeder allpasses in series, each one's
//! delay drifting slowly, different on each side.
//!
//! Chosen by ear from the v2.46 listening test (`spikes/diffuse/`, candidate
//! C). It replaces v2.7–v2.46's eight long allpasses (12.5–53 ms, g = 0.6,
//! the same on both sides, a dry blend for Amount), which fluttered (separate
//! echoes rather than a wash), added no width, and put up to a quarter second
//! between the sound and its smear: in the loop, every pass came round that
//! much late.
//!
//! - Amount is the allpass gain (0 → 0.75): how dense the smear is, never a
//!   blend with the dry signal, so the response stays flat (each stage is an
//!   allpass) and nothing combs.
//! - The chain always delays by its stages' sum (`bulk`, ~31 ms × Size; at
//!   Amount 0 it is just that delay). The loops read that much earlier, so
//!   the cycle stays the one you set. On In and Out it can't be taken back,
//!   so `blend` fades the chain in from dry over the first 20 % of Amount.
//! - The sides use different delays with the same sum, so a mono source comes
//!   out wide and its mono sum keeps its timing.
//! - Each stage's delay drifts (±0.12 ms × Size at 0.4–1.3 Hz, scaled by
//!   Amount), so a loop running for minutes never settles into one ringing
//!   pattern. Size glides (30 ms), so turning it doesn't click.

use crate::delay::DelayLine;

const STAGES: usize = 6;

/// Stage delays (ms) at Size 1, per side; both sum to 30.61 ms.
const DELAY_MS: [[f32; STAGES]; 2] = [
    [1.31, 2.27, 3.53, 5.17, 7.43, 10.9],
    [1.43, 2.11, 3.79, 4.93, 7.91, 10.44],
];
const RATE_HZ: [f32; STAGES] = [0.37, 0.53, 0.71, 0.89, 1.13, 1.31];
const DRIFT_MS: f32 = 0.12;
const G_MAX: f32 = 0.75;

/// Largest Size the buffers hold (the param's max).
const MAX_SIZE: f32 = 2.0;

/// Amount → allpass gain.
fn gain(amount: f32) -> f32 {
    G_MAX * amount.clamp(0.0, 1.0).powf(0.6)
}

struct Stage {
    v: DelayLine,
    /// Delay at Size 1, whole samples.
    base: f32,
    /// The drift's sine and cosine, turned a step each sample.
    sin: f32,
    cos: f32,
    /// The fractional read's last output (a first-order allpass).
    frac_prev: f32,
}

pub struct Diffusion {
    side: usize,
    stages: [Stage; STAGES],
    /// Each stage's drift step: (sin, cos) of its angle a sample.
    turn: [(f32, f32); STAGES],
    drift: f32,
    size_now: f32,
    size_to: f32,
    size_glide: f32,
}

impl Diffusion {
    /// `side`: 0 for a left (or odd / Mid) line, 1 for a right one.
    pub fn new(side: usize) -> Self {
        Self {
            side: side.min(1),
            stages: core::array::from_fn(|_| Stage { v: DelayLine::new(4), base: 1.0, sin: 0.0, cos: 1.0, frac_prev: 0.0 }),
            turn: [(0.0, 1.0); STAGES],
            drift: 0.0,
            size_now: 1.0,
            size_to: 1.0,
            size_glide: 1.0,
        }
    }

    pub fn prepare(&mut self, sr: f32) {
        let ms = DELAY_MS[self.side];
        let mut base: [f32; STAGES] = core::array::from_fn(|k| (ms[k] * 0.001 * sr).round());
        // the sides' sums match in whole samples
        let other: f32 = DELAY_MS[1 - self.side].iter().map(|m| (m * 0.001 * sr).round()).sum();
        if self.side == 1 {
            base[STAGES - 1] += other - base.iter().sum::<f32>();
        }
        self.drift = DRIFT_MS * 0.001 * sr;
        for (k, st) in self.stages.iter_mut().enumerate() {
            let cap = (base[k] * MAX_SIZE + self.drift * MAX_SIZE + 8.0) as usize;
            st.v = DelayLine::new(cap);
            st.base = base[k];
            // the sides drift out of step
            let ph = core::f32::consts::TAU * (0.17 * k as f32 + 0.5 * self.side as f32);
            (st.sin, st.cos) = ph.sin_cos();
        }
        self.turn = core::array::from_fn(|k| (core::f32::consts::TAU * RATE_HZ[k] / sr).sin_cos());
        self.size_glide = 1.0 - (-1.0 / (0.03 * sr)).exp();
    }

    /// The Size to glide to (0.25…2).
    pub fn set_size(&mut self, size: f32) {
        self.size_to = size.clamp(0.25, MAX_SIZE);
    }

    /// Land on the Size at once (a reset, the first block).
    pub fn snap_size(&mut self) {
        self.size_now = self.size_to;
    }

    pub fn reset(&mut self) {
        for st in &mut self.stages {
            st.v.reset();
            st.frac_prev = 0.0;
        }
        self.size_now = self.size_to;
    }

    /// How long the chain delays the sound, in samples (its stages' sum at
    /// the current Size): the loops read this much earlier.
    pub fn bulk(&self) -> f32 {
        self.stages.iter().map(|st| st.base).sum::<f32>() * self.size_now
    }

    /// One sample through the chain at `amount` (0…1; 0 is a pure delay of
    /// `bulk` samples).
    #[inline]
    pub fn process(&mut self, x: f32, amount: f32) -> f32 {
        if self.size_now != self.size_to {
            self.size_now += (self.size_to - self.size_now) * self.size_glide;
            if (self.size_to - self.size_now).abs() < 1e-5 {
                self.size_now = self.size_to;
            }
        }
        let g = gain(amount);
        let depth = self.drift * self.size_now * g / G_MAX;
        let mut y = x;
        for (st, (ts, tc)) in self.stages.iter_mut().zip(self.turn) {
            let d = st.base * self.size_now + depth * st.sin;
            // turn the drift on, keeping it on the unit circle
            let (s, c) = (st.sin * tc + st.cos * ts, st.cos * tc - st.sin * ts);
            let norm = 1.5 - 0.5 * (s * s + c * c);
            (st.sin, st.cos) = (s * norm, c * norm);
            // v(n−d): whole samples back, then the fraction through a
            // first-order allpass (Thiran), which keeps every frequency's
            // level as the delay moves (a Lagrange read here took half an
            // impulse's energy, the top, every pass). At a whole-sample delay
            // it's exact.
            let k = (d - 0.5).floor().max(1.0);
            let eta = d - k;
            let a = (1.0 - eta) / (1.0 + eta);
            let (s0, s1) = (st.v.back(k as usize), st.v.back(k as usize + 1));
            let v_d = a * s0 + s1 - a * st.frac_prev;
            st.frac_prev = v_d;
            // lattice: v(n) = y − g·v(n−d), out = g·v(n) + v(n−d)
            let v = y - g * v_d;
            st.v.write(v);
            y = g * v + v_d;
        }
        y
    }

    /// In and Out: the chain fades in from the dry signal over the first
    /// 20 % of Amount (there's no loop to read early there).
    #[inline]
    pub fn blend(&mut self, x: f32, amount: f32) -> f32 {
        let m = (amount * 5.0).clamp(0.0, 1.0);
        let y = self.process(x, amount);
        x + (y - x) * m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_keeps_the_energy_and_sits_at_its_delay() {
        // an impulse, still and drifting: all its energy comes out (an
        // allpass), centred within 1 ms of where the loops take it back
        for amount in [0.0f32, 0.3, 0.6, 1.0] {
            for side in [0, 1] {
                let mut d = Diffusion::new(side);
                d.prepare(48_000.0);
                let h: Vec<f32> = (0..96_000).map(|i| d.process(if i == 0 { 1.0 } else { 0.0 }, amount)).collect();
                let e: f64 = h.iter().map(|v| (*v as f64).powi(2)).sum();
                let c = h.iter().enumerate().map(|(i, v)| i as f64 * (*v as f64).powi(2)).sum::<f64>() / e;
                assert!((e - 1.0).abs() < 0.005, "amount {amount}: energy {e}");
                assert!((c - d.bulk() as f64).abs() < 48.0, "amount {amount}: centre {c} vs {}", d.bulk());
            }
        }
        // the sides delay alike, so the mono sum keeps its timing
        let (mut l, mut r) = (Diffusion::new(0), Diffusion::new(1));
        l.prepare(48_000.0);
        r.prepare(48_000.0);
        assert_eq!(l.bulk(), r.bulk());
    }

    /// CPU of a stereo chain (not a check): `cargo test … diffuse_cpu -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn diffuse_cpu() {
        let (mut l, mut r) = (Diffusion::new(0), Diffusion::new(1));
        l.prepare(48_000.0);
        r.prepare(48_000.0);
        let n = 4_800_000;
        let t = std::time::Instant::now();
        let mut sink = 0.0f32;
        let mut x = 0.1f32;
        for _ in 0..n {
            x = -x * 0.999;
            sink += l.process(x, 0.6) + r.process(-x, 0.6);
        }
        let ns = t.elapsed().as_nanos() as f64 / n as f64;
        println!("diffuse: {ns:.1} ns a stereo sample, {:.2} % of a core at 48 kHz ({sink})", ns * 48_000.0 / 1e7);
    }
}
