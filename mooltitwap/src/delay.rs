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

/// A per-tap delay-time slewer + pitch handling. Pitch is implemented as
/// continuous delay-time drift: if pitch != 0, the read position drifts at
/// rate (1 - 2^(pitch/12)) samples per sample. This creates varispeed Doppler
/// pitch shift, which is what gen~ multitap does via `slide`.
///
/// To prevent the delay from drifting indefinitely (which would push the tap
/// arbitrarily far from its intended position), we use two crossfaded read
/// heads when |pitch| > 0, each ramping through a window centered on the
/// nominal tap time.
pub struct TapReader {
    nominal: SlewedTime,
    pitch_factor: f32, // 2^(semis/12)
    head_a: f32,
    head_b: f32,
    fade: f32, // 0..1, crossfade position
    window_samples: f32,
}

impl TapReader {
    pub fn new() -> Self {
        Self {
            nominal: SlewedTime::new(),
            pitch_factor: 1.0,
            head_a: 0.0,
            head_b: 0.0,
            fade: 0.0,
            window_samples: 4800.0, // ~100ms at 48kHz
        }
    }

    pub fn set_window_ms(&mut self, ms: f32, sample_rate: f32) {
        self.window_samples = (ms * 0.001 * sample_rate).max(64.0);
    }

    pub fn set_nominal(&mut self, delay_samples: f32) {
        self.nominal.set_target(delay_samples);
    }

    pub fn snap_to(&mut self, delay_samples: f32) {
        self.nominal.snap_to(delay_samples);
        self.head_a = delay_samples;
        self.head_b = delay_samples + self.window_samples * 0.5;
        self.fade = 0.0;
    }

    pub fn set_pitch_semitones(&mut self, semis: f32) {
        self.pitch_factor = 2.0_f32.powf(semis / 12.0);
    }

    pub fn set_nominal_slew_rate(&mut self, rate: f32) {
        self.nominal.set_rate(rate);
    }

    /// Read one sample from the delay line at the current pitch/time.
    #[inline]
    pub fn read(&mut self, delay: &DelayLine) -> f32 {
        let nominal = self.nominal.tick();

        if (self.pitch_factor - 1.0).abs() < 1e-5 {
            // no pitch — single read at nominal
            self.head_a = nominal;
            self.head_b = nominal;
            self.fade = 0.0;
            return delay.read(nominal.max(1.0));
        }

        // Pitch shifting: heads drift relative to write head.
        // If pitch_factor > 1 (up), read faster than write -> delay decreases.
        let drift = self.pitch_factor - 1.0;
        self.head_a -= drift;
        self.head_b -= drift;

        // crossfade increment so we cycle through the window in window_samples
        let fade_step = drift.abs() / self.window_samples;
        self.fade += fade_step;
        if self.fade >= 1.0 {
            self.fade -= 1.0;
            // reset whichever head is further from nominal back to nominal
            if (self.head_a - nominal).abs() > (self.head_b - nominal).abs() {
                self.head_a = nominal;
            } else {
                self.head_b = nominal;
            }
        }

        // Clamp heads to valid range
        let a = self.head_a.max(1.0);
        let b = self.head_b.max(1.0);

        let sa = delay.read(a);
        let sb = delay.read(b);

        // equal-power crossfade
        let w = self.fade;
        let wa = (1.0 - w).sqrt();
        let wb = w.sqrt();
        sa * wa + sb * wb
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
