use crate::params::NUM_TAPS;

/// A single-channel ring buffer for delay. Reads at fractional sample
/// positions using cubic Hermite interpolation. Writes are sample-by-sample.
pub struct DelayLine {
    buffer: Vec<f32>,
    write_idx: usize,
    capacity: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        // round up to next power of two for cheap masking
        let capacity = max_samples.next_power_of_two().max(2);
        Self {
            buffer: vec![0.0; capacity],
            write_idx: 0,
            capacity,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        let mask = self.capacity - 1;
        self.buffer[self.write_idx & mask] = x;
        self.write_idx = (self.write_idx + 1) & mask;
    }

    /// Read at a fractional delay in samples. `delay_samples` must be >= 1.0
    /// and < capacity - 2 (to leave room for cubic neighbors).
    #[inline]
    pub fn read(&self, delay_samples: f32) -> f32 {
        let mask = self.capacity - 1;
        let d = delay_samples.max(1.0).min(self.capacity as f32 - 3.0);
        let d_int = d as usize;
        let frac = d - d_int as f32;

        // read_pos = write_idx - d_int (in modular arithmetic over capacity)
        let base = (self.write_idx + self.capacity - d_int - 1) & mask;
        let i0 = (base + self.capacity - 1) & mask;
        let i1 = base;
        let i2 = (base + 1) & mask;
        let i3 = (base + 2) & mask;

        let y0 = self.buffer[i0];
        let y1 = self.buffer[i1];
        let y2 = self.buffer[i2];
        let y3 = self.buffer[i3];

        cubic_hermite(y0, y1, y2, y3, frac)
    }
}

#[inline]
fn cubic_hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + c0
}

/// Per-tap slewed delay time. Smoothing the delay time = varispeed pitch.
/// Internal target updates instantly; the smoothed value drifts toward it
/// at a controlled rate (Doppler shift = pitch).
pub struct SlewedTime {
    current: f32,
    target: f32,
    /// Rate of change per sample. Larger = faster glide = smaller pitch shift
    /// when moving to a new target.
    rate: f32,
}

impl SlewedTime {
    pub fn new() -> Self {
        Self { current: 0.0, target: 0.0, rate: 1.0 }
    }

    /// Set the natural slew rate, in samples per sample. 1.0 = no slewing.
    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.max(0.0001);
    }

    pub fn set_target(&mut self, t: f32) {
        self.target = t.max(0.0);
    }

    pub fn snap_to(&mut self, t: f32) {
        self.current = t.max(0.0);
        self.target = self.current;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        let diff = self.target - self.current;
        if diff.abs() < 1e-4 {
            self.current = self.target;
        } else {
            let step = diff.signum() * self.rate.min(diff.abs());
            self.current += step;
        }
        self.current
    }

    pub fn current(&self) -> f32 { self.current }
}

impl Default for SlewedTime {
    fn default() -> Self { Self::new() }
}

/// One grain in the PSOLA-style pitch shifter.
#[derive(Default, Clone, Copy)]
struct PsolaGrain {
    active: bool,
    /// Constant delay (in delay-line samples) captured at grain start.
    /// While active the grain reads at this delay every sample, which gives
    /// natural-rate playback of consecutive source samples (since write_pos
    /// also advances 1 per sample).
    delay_samples: f32,
    /// Age in samples since grain start.
    age: f32,
    /// Total grain duration in samples (≈ 2 × period).
    duration: f32,
}

/// Max simultaneous grains in the PSOLA shifter. With grain duration ≈ 2T
/// and output epoch interval = T/pitch_factor, the average grain density is
/// roughly 2·pitch_factor. 6 is safe up to ~3× pitch up.
const PSOLA_MAX_GRAINS: usize = 6;

/// Per-tap delay reader with three pitch modes:
///
/// 1. **TAPE BEND** (`read_tape_bend`): single read head at the slewed
///    nominal delay time. Pitch is encoded by changing the nominal delay
///    (caller sets `set_nominal` to `base_delay * 2^(-semis/12)`); the slew
///    of `SlewedTime` produces the Doppler bend during transition. Steady
///    state = no pitch, just a delayed read. No clicks, no grain artifacts.
///
/// 2. **PSOLA** (`read_psola`): pitch-period-synchronous granular shifter.
///    Takes an externally estimated pitch period and places grains at output
///    epochs spaced `period / pitch_factor` apart. Each grain reads at a
///    constant delay (captured at grain start) and lasts ~2·period samples
///    with a Hann window. Adjacent grains overlap and OLA. Because grain
///    spacing aligns with pitch periods, the algorithm preserves the local
///    waveform shape and avoids the phase cancellation that plagues the
///    fixed-window dual-head approach.
///
/// 3. **Hann dual-head** (`read_continuous_hann`): the older fixed-window
///    shifter. Kept as a fallback when the pitch tracker reports unpitched
///    content (the PSOLA path needs a valid period to work).
///
/// Pitch param changes are smoothed by a 5 ms one-pole lowpass to suppress
/// the click that an instant pitch_factor jump would create.
pub struct TapReader {
    nominal: SlewedTime,
    /// Target pitch factor (2^(semis/12)) set by set_pitch_semitones.
    pitch_target: f32,
    /// Smoothed pitch factor — actually applied to the drift computation.
    pitch_current: f32,
    /// One-pole LP coefficient for pitch smoothing.
    pitch_smooth_coef: f32,
    /// Grain phasor [0,1) for the Hann fallback shifter.
    phase: f32,
    /// Hann window length in samples (fallback only).
    window_samples: f32,
    /// PSOLA grain pool.
    psola_grains: [PsolaGrain; PSOLA_MAX_GRAINS],
    /// Samples until the next PSOLA grain trigger (fractional, in output time).
    psola_next_trigger_in: f32,
}

impl TapReader {
    pub fn new() -> Self {
        Self {
            nominal: SlewedTime::new(),
            pitch_target: 1.0,
            pitch_current: 1.0,
            pitch_smooth_coef: 0.005,
            phase: 0.0,
            window_samples: 2880.0,
            psola_grains: [PsolaGrain::default(); PSOLA_MAX_GRAINS],
            psola_next_trigger_in: 0.0,
        }
    }

    pub fn set_window_ms(&mut self, ms: f32, sample_rate: f32) {
        self.window_samples = (ms * 0.001 * sample_rate).max(64.0);
        // 5ms pitch smoothing to suppress clicks when pitch param changes.
        let tc = (0.005 * sample_rate).max(1.0);
        self.pitch_smooth_coef = 1.0 - (-1.0_f32 / tc).exp();
    }

    pub fn set_nominal(&mut self, delay_samples: f32) {
        self.nominal.set_target(delay_samples);
    }

    pub fn snap_to(&mut self, delay_samples: f32) {
        self.nominal.snap_to(delay_samples);
        self.phase = 0.0;
        self.pitch_current = self.pitch_target;
        for g in self.psola_grains.iter_mut() { g.active = false; }
        self.psola_next_trigger_in = 0.0;
    }

    pub fn set_pitch_semitones(&mut self, semis: f32) {
        self.pitch_target = 2.0_f32.powf(semis / 12.0);
    }

    pub fn set_nominal_slew_rate(&mut self, rate: f32) {
        self.nominal.set_rate(rate);
    }

    /// TAPE BEND mode — single read at slewed nominal.
    #[inline]
    pub fn read_tape_bend(&mut self, delay: &DelayLine) -> f32 {
        let n = self.nominal.tick();
        delay.read(n.max(1.0))
    }

    /// PSOLA-style pitch-period-synchronous shifter.
    ///
    /// `period_samples` is the externally detected pitch period of the source
    /// signal. When the period is valid (the tracker reports a confident
    /// fundamental), grains are placed at output epochs of `period /
    /// pitch_factor`, each lasting 2·period samples with a Hann window.
    /// Adjacent grains overlap and OLA. Because grain hops align with the
    /// source's pitch periods, the local waveform is preserved across grains
    /// → no phase cancellation between grains, no perceived detune.
    ///
    /// When `period_samples <= 0` (unpitched / noisy / silent input), falls
    /// back to the fixed-window dual-head Hann shifter.
    #[inline]
    pub fn read_psola(&mut self, delay: &DelayLine, period_samples: f32) -> f32 {
        let n = self.nominal.tick();

        // Smooth the pitch_factor (kills param-change clicks)
        self.pitch_current +=
            self.pitch_smooth_coef * (self.pitch_target - self.pitch_current);
        let pf = self.pitch_current.max(0.0625); // clamp to 4 oct down

        if (pf - 1.0).abs() < 1e-5 {
            // No pitch shift — just read at nominal.
            return delay.read(n.max(1.0));
        }

        // Bail to Hann fallback when no usable period is reported
        if period_samples < 32.0 {
            return self.read_continuous_hann(delay);
        }

        // Clamp period to musical range
        let period = period_samples.clamp(32.0, 2400.0);
        // Output epoch spacing in output-time samples
        let out_step = (period / pf).max(8.0);
        // Grain duration = 2 periods so adjacent grains have 50% overlap at pf=1
        let grain_dur = 2.0 * period;

        // Step the trigger counter and fire a new grain at each epoch
        self.psola_next_trigger_in -= 1.0;
        while self.psola_next_trigger_in <= 0.0 {
            self.psola_next_trigger_in += out_step;
            // Find an inactive slot or evict the oldest
            let slot = self.allocate_grain_slot();
            let g = &mut self.psola_grains[slot];
            g.active = true;
            g.delay_samples = n;
            g.age = 0.0;
            g.duration = grain_dur;
        }

        // Sum all active grains with Hann window weights. Window sum is *not*
        // guaranteed to be 1.0 for arbitrary pitch_factor, so normalize by it.
        let tau = std::f32::consts::TAU;
        let mut output = 0.0_f32;
        let mut win_sum = 0.0_f32;
        for g in self.psola_grains.iter_mut() {
            if !g.active { continue; }
            if g.age >= g.duration {
                g.active = false;
                continue;
            }
            let phase = g.age / g.duration;
            let w = 0.5 * (1.0 - (phase * tau).cos());
            // Constant delay = natural-rate playback through delay buffer
            let s = delay.read(g.delay_samples.max(1.0));
            output += s * w;
            win_sum += w;
            g.age += 1.0;
        }
        if win_sum > 1e-4 {
            output / win_sum
        } else {
            // Coverage hole — fall back to direct read
            delay.read(n.max(1.0))
        }
    }

    fn allocate_grain_slot(&mut self) -> usize {
        // Prefer inactive slot
        for i in 0..PSOLA_MAX_GRAINS {
            if !self.psola_grains[i].active { return i; }
        }
        // All active — evict the one furthest along its life
        let mut best = 0;
        let mut best_ratio = -1.0_f32;
        for i in 0..PSOLA_MAX_GRAINS {
            let g = &self.psola_grains[i];
            let r = g.age / g.duration.max(1.0);
            if r > best_ratio { best_ratio = r; best = i; }
        }
        best
    }

    /// Fixed-window dual-head Hann shifter (fallback used by PSOLA when
    /// no pitch is detected). Same algorithm as before.
    #[inline]
    fn read_continuous_hann(&mut self, delay: &DelayLine) -> f32 {
        let n = self.nominal.current();
        let drift = self.pitch_current - 1.0;

        self.phase += 1.0 / self.window_samples;
        if self.phase >= 1.0 { self.phase -= 1.0; }

        let phase_a = self.phase;
        let phase_b = if self.phase < 0.5 { self.phase + 0.5 } else { self.phase - 0.5 };

        let tau = std::f32::consts::TAU;
        let win_a = 0.5 * (1.0 - (phase_a * tau).cos());
        let win_b = 0.5 * (1.0 - (phase_b * tau).cos());

        let head_a = n - drift * self.window_samples * phase_a;
        let head_b = n - drift * self.window_samples * phase_b;

        let sa = delay.read(head_a.max(1.0));
        let sb = delay.read(head_b.max(1.0));

        sa * win_a + sb * win_b
    }
}

impl Default for TapReader {
    fn default() -> Self { Self::new() }
}

/// One channel's worth of taps.
pub struct ChannelTaps {
    pub delay: DelayLine,
    pub readers: [TapReader; NUM_TAPS],
}

impl ChannelTaps {
    pub fn new(max_samples: usize) -> Self {
        Self {
            delay: DelayLine::new(max_samples),
            readers: core::array::from_fn(|_| TapReader::new()),
        }
    }

    pub fn reset(&mut self) {
        self.delay.reset();
        for r in self.readers.iter_mut() {
            r.snap_to(1.0);
        }
    }
}
