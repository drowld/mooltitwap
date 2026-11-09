use crate::params::FilterType;

/// The tap filter (crate adoption pass 5, v2.54): the shared TPT
/// state-variable filter from `softsystems-fx`, with Afterecho's three outputs.
pub use softsystems_fx::filter::svf::Svf;

/// One tick of `f`, the output `kind` picks.
#[inline]
pub fn svf_tick(f: &mut Svf, x: f32, kind: FilterType) -> f32 {
    let o = f.process(x);
    match kind {
        FilterType::LowPass => o.lp,
        FilterType::BandPass => o.bp,
        FilterType::HighPass => o.hp,
    }
}

/// A one-pole high-pass a few Hz up (v2.43): keeps offset from building up
/// in the loop. `r` = 1 − 2π·f/sr.
#[derive(Clone, Copy, Default)]
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    #[inline]
    pub fn process(&mut self, x: f32, r: f32) -> f32 {
        let y = x - self.x1 + r * self.y1;
        (self.x1, self.y1) = (x, y);
        y
    }
}

/// Soft tanh saturator with drive.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

/// The loop tone's low-pass cutoff left of centre: 20 kHz at 0 → 600 Hz at −1.
pub fn dark_hz(t: f32) -> f32 {
    20_000.0 * 0.03f32.powf(-t.min(0.0))
}

/// The loop tone's high-pass cutoff right of centre: 20 Hz at 0 → 1 kHz at 1.
pub fn thin_hz(t: f32) -> f32 {
    20.0 * 50.0f32.powf(t.max(0.0))
}

/// The knob position for a cutoff, the inverse of [`dark_hz`] / [`thin_hz`].
pub fn tone_at(dark: bool, hz: f32) -> f32 {
    if dark {
        -((20_000.0 / hz.max(1.0)).ln() / (1.0f32 / 0.03).ln()).clamp(0.0, 1.0)
    } else {
        ((hz.max(1.0) / 20.0).ln() / 50.0f32.ln()).clamp(0.0, 1.0)
    }
}

/// The loop tone's settings for one knob position `t` (−1..1), shared by
/// every channel of both loops.
#[derive(Clone, Copy, Default)]
pub struct ToneCoefs {
    /// How much of the filtered signal goes round: 0 at the centre (off), 1
    /// from 0.1 either side, a blend in between so the centre never jumps.
    w: f32,
    dark: bool,
    /// TPT one-pole gains, g / (1 + g): the dark low-pass, and the low-pass
    /// the thin high-pass subtracts. Each sits at its end of the range while
    /// the other side is in use.
    g_dark: f32,
    g_thin: f32,
}

impl ToneCoefs {
    pub fn new(t: f32, sample_rate: f32) -> Self {
        let g = |hz: f32| {
            let g = (core::f32::consts::PI * (hz / sample_rate).clamp(1.0e-5, 0.49)).tan();
            g / (1.0 + g)
        };
        Self { w: (t.abs() / 0.1).min(1.0), dark: t < 0.0, g_dark: g(dark_hz(t)), g_thin: g(thin_hz(t)) }
    }
}

/// Loop tone (v2.34, Tide's FB tilt): one knob on the feedback only. Left of
/// centre a one-pole low-pass darkens each pass, right of it a one-pole
/// high-pass thins it, and the centre is off (bit-exact). One pole per pass
/// adds up like tape: the n-th repeat has been through n of them. Unity gain
/// in the passband, so the loop can still self-oscillate there.
///
/// Both filters always run, so either can take over without a jump. State
/// for two channels (L/R, or Ping's odd/even).
#[derive(Clone, Default)]
pub struct LoopTone {
    dark: [f32; 2],
    thin: [f32; 2],
}

impl LoopTone {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn process(&mut self, c: &ToneCoefs, ch: usize, x: f32) -> f32 {
        let v = (x - self.dark[ch]) * c.g_dark;
        let lp = v + self.dark[ch];
        self.dark[ch] = lp + v;
        let v = (x - self.thin[ch]) * c.g_thin;
        let low = v + self.thin[ch];
        self.thin[ch] = low + v;
        if c.w == 0.0 {
            return x;
        }
        let y = if c.dark { lp } else { x - low };
        x + c.w * (y - x)
    }
}

/// Naive peak limiter. Lookahead-free, just a soft knee + attack/release env.
/// Catches the >99% feedback runaway. Not a mastering tool.
pub struct PeakLimiter {
    env: f32,
    attack_coef: f32,
    release_coef: f32,
    ceiling: f32,
}

impl PeakLimiter {
    pub fn new(sample_rate: f32) -> Self {
        let attack_ms = 1.0;
        let release_ms = 100.0;
        Self {
            env: 0.0,
            attack_coef: (-1.0 / (attack_ms * 0.001 * sample_rate)).exp(),
            release_coef: (-1.0 / (release_ms * 0.001 * sample_rate)).exp(),
            ceiling: 0.95,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.attack_coef = (-1.0 / (0.001 * sample_rate)).exp();
        self.release_coef = (-1.0 / (0.1 * sample_rate)).exp();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let abs = x.abs();
        let target = if abs > self.ceiling { abs / self.ceiling } else { 1.0 };
        let coef = if target > self.env { self.attack_coef } else { self.release_coef };
        self.env = target + coef * (self.env - target);
        x / self.env.max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{tone_from_text, tone_text};

    fn noise(n: usize) -> Vec<f32> {
        let mut seed = 7u32;
        (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / 16_777_216.0 - 0.5
            })
            .collect()
    }

    #[test]
    fn the_tone_is_off_at_the_centre() {
        let c = ToneCoefs::new(0.0, 48_000.0);
        let mut tone = LoopTone::default();
        for x in noise(4800) {
            assert_eq!(tone.process(&c, 0, x).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn the_tone_reaches_its_ends() {
        assert!((dark_hz(-1.0) - 600.0).abs() < 0.5 && (dark_hz(0.0) - 20_000.0).abs() < 0.5);
        assert!((thin_hz(1.0) - 1000.0).abs() < 0.5 && (thin_hz(0.0) - 20.0).abs() < 1e-3);
        // a sine well inside each side's stopband loses most of its level
        let sine = |hz: f32| (0..9600).map(move |n| (2.0 * core::f32::consts::PI * hz * n as f32 / 48_000.0).sin());
        let rms_through = |t: f32, hz: f32| {
            let (c, mut tone) = (ToneCoefs::new(t, 48_000.0), LoopTone::default());
            let out: Vec<f32> = sine(hz).map(|x| tone.process(&c, 0, x)).collect();
            (out[4800..].iter().map(|y| y * y).sum::<f32>() / 4800.0).sqrt() * 2f32.sqrt()
        };
        assert!(rms_through(-1.0, 6000.0) < 0.15, "LP 600 Hz at 6 kHz: {}", rms_through(-1.0, 6000.0));
        assert!(rms_through(1.0, 100.0) < 0.15, "HP 1 kHz at 100 Hz: {}", rms_through(1.0, 100.0));
        assert!(rms_through(-1.0, 60.0) > 0.98 && rms_through(1.0, 12_000.0) > 0.98);
    }

    #[test]
    fn the_tone_text_reads_back() {
        for t in [-1.0, -0.62, -0.1, 0.0, 0.07, 0.45, 1.0] {
            let s = tone_text(t);
            let back = tone_from_text(&s).unwrap_or_else(|| panic!("{s:?} didn't read back"));
            assert!((back - t).abs() < 0.01, "{t} → {s:?} → {back}");
        }
        assert_eq!(tone_text(0.0), "Off");
        assert_eq!(tone_from_text("hp 1k"), Some(1.0));
        assert_eq!(tone_from_text("loud"), None);
    }
}
