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

/// Per-tap delay reader with two pitch modes:
///
/// 1. **TAPE BEND** (`read_tape_bend`): single read head at the slewed
///    nominal delay time. Pitch is encoded by changing the nominal delay
///    (the caller sets `set_nominal` to `base_delay * 2^(-semis/12)`), and
///    the slew of `SlewedTime` produces the Doppler pitch bend during the
///    transition. Steady state = no pitch, just a delayed read. No clicks,
///    no grain artifacts — but no sustained pitch shift either.
///
/// 2. **CONTINUOUS** (`read_continuous`): proper dual-head Hann-windowed
///    pitch shifter. Two read heads share a phasor, 180° offset. Each head
///    sweeps from `nominal` outward by `drift * window_samples` over one
///    window cycle, then resets to `nominal` at the moment its Hann window
///    weight is exactly 0 (silent reset → no click). Window weights sum to
///    constant 1.0 → no amplitude modulation when drift = 0.
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
    /// Grain phasor [0,1) for the continuous-mode dual-head shifter.
    phase: f32,
    /// Hann window length in samples.
    window_samples: f32,
}

impl TapReader {
    pub fn new() -> Self {
        Self {
            nominal: SlewedTime::new(),
            pitch_target: 1.0,
            pitch_current: 1.0,
            pitch_smooth_coef: 0.005,
            phase: 0.0,
            window_samples: 2880.0, // ~60ms at 48kHz
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
    }

    pub fn set_pitch_semitones(&mut self, semis: f32) {
        self.pitch_target = 2.0_f32.powf(semis / 12.0);
    }

    pub fn set_nominal_slew_rate(&mut self, rate: f32) {
        self.nominal.set_rate(rate);
    }

    /// TAPE BEND mode — single read at slewed nominal. Pitch is implicit
    /// in the slew. No grain artifacts possible because there's no drift.
    #[inline]
    pub fn read_tape_bend(&mut self, delay: &DelayLine) -> f32 {
        let n = self.nominal.tick();
        delay.read(n.max(1.0))
    }

    /// CONTINUOUS mode — proper dual-head Hann-windowed pitch shifter.
    /// Sustains the pitch indefinitely without grain-boundary clicks.
    #[inline]
    pub fn read_continuous(&mut self, delay: &DelayLine) -> f32 {
        let n = self.nominal.tick();

        // Smooth pitch factor → target (5ms LP). Prevents param-change clicks.
        self.pitch_current +=
            self.pitch_smooth_coef * (self.pitch_target - self.pitch_current);
        let drift = self.pitch_current - 1.0;

        // Advance the shared phasor at a fixed rate (1/window_samples per sample).
        // Cycle duration is the window length, independent of drift magnitude.
        self.phase += 1.0 / self.window_samples;
        if self.phase >= 1.0 { self.phase -= 1.0; }

        let phase_a = self.phase;
        let phase_b = if self.phase < 0.5 { self.phase + 0.5 } else { self.phase - 0.5 };

        // Hann windows: 0 at phase=0 and phase=1, peak 1 at phase=0.5.
        // Sum of A and B (180° offset) = constant 1.0.
        let tau = std::f32::consts::TAU;
        let win_a = 0.5 * (1.0 - (phase_a * tau).cos());
        let win_b = 0.5 * (1.0 - (phase_b * tau).cos());

        // Head positions: at phase=0 the head is exactly at `nominal`; over
        // one window it drifts by `drift * window_samples`. When phase wraps
        // back to 0 the head resets to nominal — but the Hann window is 0
        // at that moment, so the reset is inaudible.
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
