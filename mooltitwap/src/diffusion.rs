//! Schroeder-allpass diffusion network (Airwindows-inspired).
//!
//! Replaces the v2.7 ChowMatrix-style 1st-order IIR allpass cascade.
//! That earlier approach was a phase-rotation network at 200 Hz — it didn't
//! produce audible *time*-domain smearing because the per-stage group delay
//! collapses to ~50 samples at DC and almost nothing above 1 kHz. To get the
//! lush reverb-like tail Airwindows plugins (Galactic / MatrixVerb) achieve,
//! you need actual delay lines.
//!
//! This module chains 8 Schroeder allpass sections with prime-ish delays
//! between 12 and 55 ms and a constant feedback coefficient g = 0.6. Each
//! section's transfer is H(z) = (g + z^-D) / (1 + g·z^-D) — magnitude is
//! exactly 1 (true allpass) but the impulse response is a decaying train of
//! taps spaced D samples apart. Chaining 8 produces enough density to read
//! as a small-room reverb on transient material.
//!
//! `depth` is a wet/dry mix: 0 = bypass, 1 = full diffused signal. All 8
//! stages always process when depth > 0 — at intermediate depths the dry
//! signal is blended back in. This gives a more intuitive, gain-preserving
//! control than the old "number of stages" mapping.

/// Number of Schroeder allpass stages in the chain.
const NUM_STAGES: usize = 8;

/// Per-stage delay in milliseconds. Mutually prime values reduce comb-filter
/// resonances when stages superimpose. Range 12-55 ms is in the comfortable
/// "small room" zone — long enough to hear actual smearing, short enough that
/// the impulse-response density is dense and the tail decays cleanly.
const STAGE_DELAYS_MS: [f32; NUM_STAGES] = [
    12.5, 17.8, 23.7, 29.1, 37.5, 41.7, 47.3, 53.1,
];

/// Schroeder allpass feedback coefficient. 0.6 is a classic value that
/// produces a dense, fast-decaying tail without ringing. Higher (0.7-0.8)
/// gives a metallic shimmer; lower (0.3-0.4) is more dry / less diffused.
const STAGE_FEEDBACK: f32 = 0.6;

/// Single Schroeder allpass delay section, lattice form:
///     v(n) = x(n) − g · v(n−D)
///     y(n) = g · v(n) + v(n−D)
/// Magnitude response is exactly 1 (allpass); only phase is altered.
struct AllpassDelay {
    buffer: Vec<f32>,
    write_idx: usize,
    delay_samples: usize,
}

impl AllpassDelay {
    fn new() -> Self {
        Self { buffer: Vec::new(), write_idx: 0, delay_samples: 0 }
    }

    /// `max_samples` — the largest delay this stage will ever use; the buffer
    /// is sized accordingly so SIZE changes at runtime are allocation-free.
    fn prepare(&mut self, max_samples: usize, initial_samples: usize) {
        let cap = max_samples.max(initial_samples).max(1);
        self.buffer = vec![0.0; cap];
        self.delay_samples = initial_samples.clamp(1, cap);
        self.write_idx = 0;
    }

    /// Change the active delay length at runtime (no allocation).
    fn set_delay(&mut self, samples: usize) {
        let cap = self.buffer.len().max(1);
        self.delay_samples = samples.clamp(1, cap);
        if self.write_idx >= self.delay_samples {
            self.write_idx = 0;
        }
    }

    fn reset(&mut self) {
        for v in self.buffer.iter_mut() { *v = 0.0; }
        self.write_idx = 0;
    }

    #[inline]
    fn process(&mut self, x: f32, g: f32) -> f32 {
        let v_delayed = self.buffer[self.write_idx];
        let v_n = x - g * v_delayed;
        let y_n = g * v_n + v_delayed;
        self.buffer[self.write_idx] = v_n;
        self.write_idx += 1;
        if self.write_idx >= self.delay_samples {
            self.write_idx = 0;
        }
        y_n
    }
}

/// Largest SIZE multiplier the diffuser ever supports. Buffer capacity
/// is sized for this so SIZE changes at runtime are allocation-free.
const MAX_SIZE: f32 = 2.0;

pub struct Diffusion {
    stages: [AllpassDelay; NUM_STAGES],
    sample_rate: f32,
}

impl Diffusion {
    pub fn new() -> Self {
        Self {
            stages: core::array::from_fn(|_| AllpassDelay::new()),
            sample_rate: 48_000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for (i, s) in self.stages.iter_mut().enumerate() {
            let max_samples = (STAGE_DELAYS_MS[i] * MAX_SIZE * 0.001 * sample_rate).round() as usize;
            let initial = (STAGE_DELAYS_MS[i] * 0.001 * sample_rate).round() as usize;
            s.prepare(max_samples, initial);
        }
    }

    /// Scale all stage delays by `size`. 1.0 = nominal (12–55 ms); 0.25 =
    /// tight & bright (3–14 ms, less low-frequency smear → less "muffled");
    /// 2.0 = lush plate (25–106 ms). Clamped to MAX_SIZE.
    pub fn set_size(&mut self, size: f32) {
        let size = size.clamp(0.05, MAX_SIZE);
        for (i, s) in self.stages.iter_mut().enumerate() {
            let samples = (STAGE_DELAYS_MS[i] * size * 0.001 * self.sample_rate).round() as usize;
            s.set_delay(samples.max(1));
        }
    }

    pub fn reset(&mut self) {
        for s in self.stages.iter_mut() { s.reset(); }
    }

    /// Process one sample. `depth` is a wet/dry mix in [0, 1].
    /// depth = 0 short-circuits to a bare bypass (no CPU spent on the chain).
    #[inline]
    pub fn process(&mut self, x: f32, depth: f32) -> f32 {
        let depth = depth.clamp(0.0, 1.0);
        if depth < 1e-4 { return x; }
        let mut wet = x;
        for s in self.stages.iter_mut() {
            wet = s.process(wet, STAGE_FEEDBACK);
        }
        x * (1.0 - depth) + wet * depth
    }
}

impl Default for Diffusion {
    fn default() -> Self { Self::new() }
}
