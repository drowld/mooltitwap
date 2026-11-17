use crate::params::FilterType;

/// The tap filter (crate adoption pass 5, v2.54): the shared TPT
/// state-variable filter from `softsystems-fx`, with Afterecho's three outputs.
pub use softsystems_fx::filter::svf::Svf;

pub use softsystems_fx::filter::svf::OnePole;
use crate::params::FilterSlope;

/// Which stages a tap filter runs (owner, 2026-11-16: Slope in 6 dB steps, 6 to 48 dB/oct):
/// `svfs` SVFs (12 dB/oct each) and, for an odd step, a one-pole (6 dB/oct).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapLayout {
    pub svfs: usize,
    pub pole: bool,
}

impl TapLayout {
    pub fn of(slope: FilterSlope) -> Self {
        // 6·m dB/oct: m / 2 SVFs, and a one-pole when m is odd
        let m = slope as usize + 1;
        TapLayout { svfs: m / 2, pole: m % 2 == 1 }
    }
}

/// The Q every SVF after the first runs at: Butterworth's 0.707 (k = √2). Only the first stage
/// takes the tap's Resonance, so the peak follows the knob at any Slope instead of stacking
/// (before 2026-11-16 every stage took it: +48 dB at Q 4 and 48 dB/oct; spikes/slope/).
pub const K_FLAT: f32 = core::f32::consts::SQRT_2;

/// One channel of a tap's filter: up to four SVFs and the one-pole step (two one-poles for
/// band-pass: a low-pass into a high-pass at the cutoff, 6 dB/oct on each side).
#[derive(Clone, Copy, Debug, Default)]
pub struct TapChain {
    svf: [Svf; 4],
    pole: [OnePole; 2],
}

impl TapChain {
    /// The cutoff's `g` for every stage; Resonance's `k` for the first SVF only.
    pub fn set_gk(&mut self, g: f32, k: f32) {
        for (st, f) in self.svf.iter_mut().enumerate() {
            f.set_gk(g, if st == 0 { k } else { K_FLAT });
        }
        for p in self.pole.iter_mut() {
            p.set_g(g);
        }
    }

    pub fn reset(&mut self) {
        self.svf.iter_mut().for_each(Svf::reset);
        self.pole.iter_mut().for_each(OnePole::reset);
    }

    /// Stages `to` runs that `from` didn't start from rest, not from whatever they held the
    /// last time they ran.
    pub fn wake(&mut self, from: TapLayout, to: TapLayout) {
        for f in self.svf.iter_mut().take(to.svfs).skip(from.svfs) {
            f.reset();
        }
        if to.pole && !from.pole {
            self.pole.iter_mut().for_each(OnePole::reset);
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32, kind: FilterType, lay: TapLayout) -> f32 {
        let mut y = x;
        for f in self.svf.iter_mut().take(lay.svfs) {
            y = svf_tick(f, y, kind);
        }
        if lay.pole {
            y = match kind {
                FilterType::LowPass => self.pole[0].lp(y),
                FilterType::HighPass => self.pole[0].hp(y),
                // the pair peaks at ½ at the cutoff; ×√2 puts it where an SVF band-pass at
                // the default Resonance peaks (0.707), so a band-pass step is as loud as a whole one
                FilterType::BandPass => {
                    let lp = self.pole[0].lp(y);
                    core::f32::consts::SQRT_2 * self.pole[1].hp(lp)
                }
            };
        }
        y
    }
}

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

    /// The steady gain (dB) of a tap chain at `f` Hz: a sine, measured after it settles.
    fn chain_db(kind: FilterType, slope: FilterSlope, q: f32, f: f32) -> f32 {
        let sr = 48_000.0;
        let mut svf = Svf::default();
        svf.set(1000.0, q, sr);
        let mut ch = TapChain::default();
        ch.set_gk(svf.g(), svf.k());
        let lay = TapLayout::of(slope);
        // the sine's phase in f64: f32's runs out of precision this many samples in, a noise
        // floor near −66 dB that a 48 dB/oct stop band reaches
        let w = core::f64::consts::TAU * f as f64 / sr as f64;
        let (mut peak, n) = (0.0f32, 48_000);
        for i in 0..n {
            let y = ch.tick((w * i as f64).sin() as f32, kind, lay);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    const SLOPES: [FilterSlope; 8] = [
        FilterSlope::Db6,
        FilterSlope::Db12,
        FilterSlope::Db18,
        FilterSlope::Db24,
        FilterSlope::Db30,
        FilterSlope::Db36,
        FilterSlope::Db42,
        FilterSlope::Db48,
    ];

    #[test]
    fn each_slope_rolls_off_at_its_rate() {
        // the drop over an octave past the cutoff: 2 to 4 kHz for LP, 500 to 250 Hz for HP (further
        // out a 48 dB/oct stop band nears f32's own floor)
        for (k, &slope) in SLOPES.iter().enumerate() {
            let want = 6.0 * (k + 1) as f32;
            let lp = chain_db(FilterType::LowPass, slope, 0.707, 2000.0) - chain_db(FilterType::LowPass, slope, 0.707, 4000.0);
            let hp = chain_db(FilterType::HighPass, slope, 0.707, 500.0) - chain_db(FilterType::HighPass, slope, 0.707, 250.0);
            // the bilinear transform steepens a low-pass near Nyquist a little
            assert!((lp - want).abs() < 0.12 * want + 1.0, "LP {want} dB/oct: {lp:.1}");
            assert!((hp - want).abs() < 0.08 * want + 0.5, "HP {want} dB/oct: {hp:.1}");
        }
    }

    #[test]
    fn resonance_does_not_stack_with_slope() {
        // Q 4 peaks about +12 dB on one SVF. Before 2026-11-16 every stage took Q: +24 at
        // 24 dB/oct, +48 at 48. Now only the first does, and each further stage (an SVF at
        // 0.707, or the one-pole) is −3 dB at the cutoff, as at the default Resonance: the peak
        // never passes +12.6 and comes down 3 dB a stage. 6 dB has no SVF, so no peak.
        for (k, &slope) in SLOPES.iter().enumerate().skip(1) {
            let lay = TapLayout::of(slope);
            let more = (lay.svfs - 1) as f32 + if lay.pole { 1.0 } else { 0.0 };
            for kind in [FilterType::LowPass, FilterType::HighPass] {
                let peak = [900.0, 950.0, 1000.0, 1050.0, 1100.0].map(|f| chain_db(kind, slope, 4.0, f)).into_iter().fold(f32::MIN, f32::max);
                let floor = 12.0 - 3.0 * more - 1.5;
                assert!(peak < 12.6 && peak > floor, "{} dB/oct {kind:?}: peak {peak:.1} dB", 6 * (k + 1));
            }
        }
        // and at the default Q a whole step is as it was: −3 dB at the cutoff per SVF
        let db48 = chain_db(FilterType::LowPass, FilterSlope::Db48, 0.707, 1000.0);
        assert!((db48 + 12.0).abs() < 0.5, "48 dB/oct at the cutoff: {db48:.1}");
    }

    #[test]
    fn a_band_pass_step_is_as_loud_as_a_whole_one() {
        let at = |slope| chain_db(FilterType::BandPass, slope, 0.707, 1000.0);
        for pair in SLOPES.windows(2) {
            let (a, b) = (at(pair[0]), at(pair[1]));
            // each step adds 6 dB/oct a side and loses 3 dB at the centre, as an SVF stage does
            assert!((a - b).abs() < 4.0, "{:?} {a:.1} dB, {:?} {b:.1} dB", pair[0], pair[1]);
        }
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
