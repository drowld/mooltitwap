//! Motion (v2.39): one source moves every tap's pan, gain, cutoff and Q
//! around the values set for them. The source is an LFO shape (the taps'
//! phases spread by Scatter) or the dry input's level. Pure maths, shared by
//! the engine and the page, so the field draws each tap where it is.
//!
//! Time isn't a target: moving delay times is pitch wobble (removed in
//! v2.17.5) and breaks Fade and Jump. Pan, gain and the filters move in every
//! mode, smoothed by the per-tap glides (v2.37).

use crate::params::MotionShape;

/// How far each target moves, 0 (still) … 1.
#[derive(Clone, Copy, Default, Debug)]
pub struct Depths {
    pub pan: f32,
    pub gain: f32,
    pub cutoff: f32,
    pub reso: f32,
}

impl Depths {
    /// Motion is off when nothing moves.
    pub fn any(&self) -> bool {
        self.pan > 0.0 || self.gain > 0.0 || self.cutoff > 0.0 || self.reso > 0.0
    }
}

/// A tap's random value for one cycle, −1…1: S&H and Glide draw each tap
/// its own, a new one each cycle, the same every time for that cycle.
fn rand(tap: usize, cycle: i64) -> f32 {
    let mut x = (tap as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (cycle as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

/// An LFO shape's value for tap `tap` at `phase` (in cycles), −1…1. Env isn't
/// an LFO: see [`env_value`].
pub fn shape_at(shape: MotionShape, phase: f64, tap: usize) -> f32 {
    let cycle = phase.floor();
    let f = (phase - cycle) as f32;
    match shape {
        MotionShape::Sine => (core::f32::consts::TAU * f).sin(),
        MotionShape::Triangle => 1.0 - 4.0 * (f - 0.5).abs(),
        MotionShape::Ramp => 2.0 * f - 1.0,
        MotionShape::Square => if f < 0.5 { 1.0 } else { -1.0 },
        MotionShape::SampleHold => rand(tap, cycle as i64),
        // the next random value eased in over the cycle (Tide's Glider)
        MotionShape::Glide => {
            let (a, b) = (rand(tap, cycle as i64), rand(tap, cycle as i64 + 1));
            let t = (1.0 - (core::f32::consts::PI * f).cos()) * 0.5;
            a + (b - a) * t
        }
        MotionShape::Env => 0.0,
    }
}

/// Env: the input's level as −1 (silence) … 1 (loud), with Tide's curve
/// (−12 dBFS already moves it most of the way).
pub fn env_value(level: f32) -> f32 {
    2.0 * (1.0 - (-5.0 * level).exp()) - 1.0
}

/// Pan moved by `m` at depth `d`: up to hard L or R either side of `base`.
pub fn pan(base: f32, m: f32, d: f32) -> f32 {
    (base + d * m).clamp(-1.0, 1.0)
}

/// Gain (dB) moved by `m` at depth `d`: it only dips, up to 24 dB at m = −1,
/// so Motion never boosts or clips.
pub fn gain_db(base: f32, m: f32, d: f32) -> f32 {
    base - 24.0 * d * (1.0 - m) * 0.5
}

/// Cutoff moved by `m` at depth `d`: ±4 octaves.
pub fn cutoff(base: f32, m: f32, d: f32) -> f32 {
    (base * 2f32.powf(4.0 * d * m)).clamp(20.0, 20_000.0)
}

/// Q moved by `m` at depth `d`: up to four times higher or lower.
pub fn reso(base: f32, m: f32, d: f32) -> f32 {
    (base * 4f32.powf(d * m)).clamp(0.1, 12.0)
}

/// Where tap `tap` of `n` sits in the cycle: Scatter spreads the taps evenly
/// round it (0: all together, 1: a whole cycle across them).
pub fn tap_phase(phase: f64, scatter: f32, tap: usize, n: usize) -> f64 {
    phase + (scatter * tap as f32 / n.max(1) as f32) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const LFOS: [MotionShape; 6] = [
        MotionShape::Sine,
        MotionShape::Triangle,
        MotionShape::Ramp,
        MotionShape::Square,
        MotionShape::SampleHold,
        MotionShape::Glide,
    ];

    #[test]
    fn every_shape_stays_in_range() {
        for shape in LFOS {
            for k in 0..2000 {
                let v = shape_at(shape, k as f64 * 0.013, k % 16);
                assert!((-1.0..=1.0).contains(&v), "{shape:?} at {k}: {v}");
            }
        }
        assert!((shape_at(MotionShape::Sine, 0.25, 0) - 1.0).abs() < 1e-6);
        assert!((shape_at(MotionShape::Triangle, 0.5, 0) - 1.0).abs() < 1e-6);
        assert_eq!(shape_at(MotionShape::Square, 0.7, 0), -1.0);
    }

    #[test]
    fn random_shapes_hold_per_cycle_and_differ_per_tap() {
        let sh = |phase, tap| shape_at(MotionShape::SampleHold, phase, tap);
        assert_eq!(sh(3.1, 2), sh(3.9, 2));
        assert_ne!(sh(3.1, 2), sh(4.1, 2));
        assert_ne!(sh(3.1, 2), sh(3.1, 5));
        // Glide starts each cycle where S&H holds and ends on the next value
        let g = |phase| shape_at(MotionShape::Glide, phase, 2);
        assert!((g(3.0) - sh(3.0, 2)).abs() < 1e-6 && (g(3.999_999) - sh(4.0, 2)).abs() < 1e-4);
    }

    #[test]
    fn scatter_spreads_the_taps_round_the_cycle() {
        // a whole cycle across 4 taps: taps 0 and 2 half a cycle apart
        let at = |tap| shape_at(MotionShape::Sine, tap_phase(0.25, 1.0, tap, 4), tap);
        assert!((at(0) + at(2)).abs() < 1e-5 && (at(0) - 1.0).abs() < 1e-5);
        // none: all together
        assert_eq!(tap_phase(0.3, 0.0, 3, 8), 0.3);
    }

    #[test]
    fn the_targets_move_as_far_as_their_depth() {
        assert_eq!(pan(0.2, 1.0, 0.5), 0.7);
        assert_eq!(pan(0.8, 1.0, 1.0), 1.0);
        assert_eq!(gain_db(-3.0, 1.0, 1.0), -3.0);
        assert_eq!(gain_db(-3.0, -1.0, 1.0), -27.0);
        assert!((cutoff(1000.0, 1.0, 0.5) - 4000.0).abs() < 0.1);
        assert!((reso(1.0, -1.0, 1.0) - 0.25).abs() < 1e-6);
        // depth 0 leaves every value exactly as set
        for m in [-1.0, -0.3, 0.6, 1.0] {
            assert_eq!((pan(0.3, m, 0.0), gain_db(-6.0, m, 0.0), cutoff(2500.0, m, 0.0), reso(0.707, m, 0.0)), (0.3, -6.0, 2500.0, 0.707));
        }
        assert!((env_value(0.0) + 1.0).abs() < 1e-6 && env_value(1.0) > 0.98);
    }
}
