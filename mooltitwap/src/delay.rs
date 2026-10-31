use crate::params::{DelayMode, NUM_TAPS};

/// Equal-power crossfade window length for DelayMode::Fade (samples).
/// ~21 ms at 48 kHz — long enough to hide click artifacts from large jumps,
/// short enough that the user perceives parameter changes as immediate.
pub const FADE_SAMPLES: usize = 1024;

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

    /// The largest magnitude the line holds.
    #[cfg(test)]
    pub fn buffer_peak(&self) -> f32 {
        self.buffer.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        let mask = self.capacity - 1;
        self.buffer[self.write_idx & mask] = x;
        self.write_idx = (self.write_idx + 1) & mask;
    }

    /// Read at a fractional delay in samples using 4-point Lagrange
    /// interpolation, after this sample's write (`read(1.0)` is the sample
    /// before the newest). `delay_samples` must be >= 1.0 and < capacity - 2.
    ///
    /// v2.44 (SOF-60): the four points run newest to oldest, so the fraction
    /// moves the read *back*: `read(10.25)` is 10.25 samples back. Before, it
    /// interpolated toward the newer sample (9.75 back) and jumped 2 samples
    /// at each whole one, so gliding delays (Repitch) stepped.
    ///
    /// Why Lagrange vs cubic Hermite: at a fractional offset of 0.5, cubic
    /// Hermite has measurable HF rolloff (~1 dB near Nyquist), while at offset
    /// 0.0 it's flat. With two read heads at different fractional offsets
    /// (any dual-head pitch shifter), this creates a timbral modulation at
    /// the grain rate → audible "detune" at integer pitch ratios. 4-point
    /// Lagrange has a much flatter passband across all fractional offsets,
    /// which essentially eliminates that artifact. Cost: 3 extra multiplies
    /// per sample.
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

        #[cfg(test)]
        if LEGACY_READ.load(std::sync::atomic::Ordering::Relaxed) {
            let i3 = (base + 2) & mask;
            return lagrange4(self.buffer[i0], self.buffer[i1], self.buffer[i2], self.buffer[i3], frac);
        }
        // i2 is one newer than i1, i0 one older: the fraction runs from i1
        // toward the older i0, with i2 and the one before i0 as the outer points
        let i_older2 = (base + self.capacity - 2) & mask;
        lagrange4(self.buffer[i2], self.buffer[i1], self.buffer[i0], self.buffer[i_older2], frac)
    }

    /// A read made before this sample's write (the feedback loops' and
    /// Repeats' reads): the newest sample is already one old, so `delay`
    /// lands exactly `delay` samples before the sample about to be written.
    /// Before v2.44 these used `read`, and every cycle came round a sample late.
    #[inline]
    pub fn read_before_write(&self, delay_samples: f32) -> f32 {
        #[cfg(test)]
        if LEGACY_READ.load(std::sync::atomic::Ordering::Relaxed) {
            return self.read(delay_samples);
        }
        self.read(delay_samples - 1.0)
    }
}

/// Tests only: read the way v2.43 and before did, for before/after renders.
#[cfg(test)]
pub static LEGACY_READ: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 4-point Lagrange interpolation, sample points at x = -1, 0, 1, 2.
/// `t` is the fractional offset in [0, 1] between y1 and y2.
///
/// Frequency response is significantly flatter across all fractional offsets
/// than cubic Hermite — passband ripple drops from ~1 dB to well under 0.1 dB
/// for normal audio bandwidths. This is the same interpolator used by
/// chowdsp_PitchShifter (`DelayLineInterpolationTypes::Lagrange3rd`) in
/// Chowdhury's ChowMatrix.
#[inline]
fn lagrange4(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    // Lagrange basis with denominators precomputed:
    //   L0(t) = -t·(t-1)·(t-2) / 6      = ((-t³ + 3t² - 2t)) / 6
    //   L1(t) =  (t+1)·(t-1)·(t-2) / 2  = (( t³ - 2t² - t + 2)) / 2
    //   L2(t) = -(t+1)·t·(t-2) / 2      = ((-t³ + t² + 2t)) / 2
    //   L3(t) =  (t+1)·t·(t-1) / 6      = (( t³ - t)) / 6
    // Evaluated with Horner-style nested form for speed.
    let d0 = t;
    let d1 = t - 1.0;
    let d2 = t - 2.0;
    let dm1 = t + 1.0;

    let l0 = -d0 * d1 * d2 * (1.0 / 6.0);
    let l1 =  dm1 * d1 * d2 * 0.5;
    let l2 = -dm1 * d0 * d2 * 0.5;
    let l3 =  dm1 * d0 * d1 * (1.0 / 6.0);

    y0 * l0 + y1 * l1 + y2 * l2 + y3 * l3
}

// ─── Pitch shifter removed in v2.4.0 ────────────────────────────────────────
// A granular Hann pitch shifter (chowdsp-style dual-head + WSOLA/AMDF alignment)
// lived here through v2.3.5. It had audible artifacts on sustained tones that
// couldn't be eliminated without either a phase vocoder (heavy per-tap CPU) or
// PSOLA + a pitch tracker (broken on polyphony). Both out of scope for a delay.
// The full implementation is preserved in git history at the v2.3.5 tag if
// pitch shifting is ever revisited as a separate plugin.

/// Per-tap slewed delay time. Smoothing the delay time produces a tape-style
/// glide when the nominal delay moves (sync-division change, wobble LFO,
/// per-tap time offset drag) — same mechanism that produces the characteristic
/// Doppler bend of tape echoes.
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

/// Per-tap delay reader supporting three transition modes:
///
/// * **Repitch** (default, classic tape) — `SlewedTime` glides the read
///   position; gliding a delay-buffer read = Doppler pitch shift. Wobble
///   feeds the same mechanism for its modulation.
/// * **Fade** — keep the old read position alive while a new one fades in
///   over `FADE_SAMPLES` with an equal-power (cos/sin) crossfade. Pitch
///   neutral. Wobble bypassed (it would re-trigger fades every sample).
/// * **Jump** — snap to the new delay. Audible click on big jumps but zero
///   pitch artifact. Best paired with snap-on-discrete-event externally.
pub struct TapReader {
    mode: DelayMode,
    // Repitch state
    nominal: SlewedTime,
    // Fade / Jump state
    target_delay: f32,
    current_delay: f32,   // for Fade: the "old" position being faded out
    fade_counter: usize,  // 0 = just started; >= FADE_SAMPLES = done
}

impl TapReader {
    pub fn new() -> Self {
        Self {
            mode: DelayMode::Repitch,
            nominal: SlewedTime::new(),
            target_delay: 1.0,
            current_delay: 1.0,
            fade_counter: FADE_SAMPLES, // start in "done" state (no fade)
        }
    }

    /// Switch transition mode. When changing modes, the new mode's state is
    /// seeded from the current target so there's no discontinuity.
    /// Idempotent: calling with the same mode is a no-op (safe to call every
    /// block as the GUI param value is polled).
    pub fn set_mode(&mut self, mode: DelayMode) {
        if mode as u8 == self.mode as u8 {
            return;
        }
        // Re-align all state to a single value so the mode switch is silent.
        let t = self.target_delay.max(1.0);
        self.nominal.snap_to(t);
        self.current_delay = t;
        self.fade_counter = FADE_SAMPLES;
        self.mode = mode;
    }

    /// Request a new nominal delay. Behavior depends on mode.
    pub fn set_nominal(&mut self, delay_samples: f32) {
        let target = delay_samples.max(1.0);
        match self.mode {
            DelayMode::Jump => {
                self.target_delay = target;
                self.current_delay = target;
                self.nominal.snap_to(target);
            }
            DelayMode::Repitch => {
                self.target_delay = target;
                self.nominal.set_target(target);
            }
            DelayMode::Fade => {
                // Only restart on meaningful changes — otherwise wobble-like
                // continuous tiny variations would re-trigger every sample.
                if (target - self.target_delay).abs() > 1.0 {
                    // Whatever target we had IS the "old" head from now on.
                    // If a previous fade is still mid-flight a slight click
                    // is possible — acceptable tradeoff for arbitrary rapid
                    // parameter changes.
                    self.current_delay = self.target_delay;
                    self.target_delay = target;
                    self.fade_counter = 0;
                }
            }
        }
    }

    pub fn snap_to(&mut self, delay_samples: f32) {
        let t = delay_samples.max(1.0);
        self.nominal.snap_to(t);
        self.target_delay = t;
        self.current_delay = t;
        self.fade_counter = FADE_SAMPLES;
    }

    pub fn set_nominal_slew_rate(&mut self, rate: f32) {
        self.nominal.set_rate(rate);
    }

    /// Where the reader is *right now*: Repitch = the slewed (in-flight)
    /// position, Fade/Jump = the committed target. The DSP uses this to
    /// bound the Repitch glide time (rate ∝ remaining distance).
    pub fn current_pos(&self) -> f32 {
        match self.mode {
            DelayMode::Repitch => self.nominal.current(),
            _ => self.target_delay,
        }
    }

    /// The longest delay this reader may read before it settles (samples).
    pub fn reach(&self) -> f32 {
        match self.mode {
            DelayMode::Repitch => self.nominal.current().max(self.nominal.target),
            _ => self.current_delay.max(self.target_delay),
        }
    }

    /// Advance one sample and say where to read. Taken once per sample; the
    /// heads then read any line (Ping's bounce lines are read where the main
    /// line is).
    #[inline]
    pub fn heads(&mut self) -> Heads {
        match self.mode {
            DelayMode::Jump => Heads::one(self.target_delay.max(1.0)),
            DelayMode::Repitch => Heads::one(self.nominal.tick().max(1.0)),
            DelayMode::Fade => {
                if self.fade_counter < FADE_SAMPLES {
                    // Equal-power cos/sin crossfade. As `t` goes 0 → 1, the
                    // old head fades cos→0 and the new head fades 0→sin.
                    let t = self.fade_counter as f32 / FADE_SAMPLES as f32;
                    let arg = t * core::f32::consts::FRAC_PI_2;
                    let heads = Heads {
                        a: self.current_delay.max(1.0),
                        wa: arg.cos(),
                        b: self.target_delay.max(1.0),
                        wb: arg.sin(),
                    };
                    self.fade_counter += 1;
                    if self.fade_counter >= FADE_SAMPLES {
                        self.current_delay = self.target_delay;
                    }
                    heads
                } else {
                    Heads::one(self.target_delay.max(1.0))
                }
            }
        }
    }
}

/// Where a tap reads this sample: one head, or two while Fade crossfades.
#[derive(Clone, Copy, PartialEq)]
pub struct Heads {
    a: f32,
    wa: f32,
    b: f32,
    wb: f32,
}

impl Heads {
    fn one(delay: f32) -> Self {
        Self { a: delay, wa: 1.0, b: delay, wb: 0.0 }
    }

    #[inline]
    pub fn read(&self, line: &DelayLine) -> f32 {
        let y = line.read(self.a) * self.wa;
        if self.wb == 0.0 { y } else { y + line.read(self.b) * self.wb }
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
