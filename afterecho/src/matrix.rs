//! Motion's matrix (v2.42): six sources (LFO 1–3, Follow, Step 1–2 since
//! v2.43), each routed to any of ten targets
//! with its own depth. Four targets move each tap (the v2.39 Motion ones);
//! six move the whole echo. Pure maths, shared by the engine and the page.
//!
//! A route's depth is a cell, −1…1; Amount and Polarity scale every cell
//! (`AfterechoParams::mx_grid`). The sums are clamped to −1…1 and then converted
//! the v2.39 way for the taps (`motion.rs`), so a single route at a depth
//! sounds exactly as v2.39's Motion did at that depth.
//!
//! Still not time: moving delay times is pitch wobble (removed in v2.17.5).

pub const SOURCES: usize = 6;
pub const TARGETS: usize = 10;
/// The first four targets move each tap; the rest the whole echo.
pub const TAP_TARGETS: usize = 4;
pub const LFOS: usize = 3;
pub const FOLLOW: usize = 3;
/// Step 1; Step 2 is the row after it.
pub const STEP: usize = 4;

pub const SRC_NAMES: [&str; SOURCES] = ["LFO 1", "LFO 2", "LFO 3", "Follow", "Step 1", "Step 2"];
pub const TGT_NAMES: [&str; TARGETS] = ["Pan", "Gain", "Cutoff", "Reso", "Colour", "Fdbk", "Cross", "Tone", "Mix", "Duck"];

/// Depths, `[source][target]`.
pub type Grid = [[f32; TARGETS]; SOURCES];

/// The whole-echo targets, indices into the global mods.
pub const COLOUR: usize = 0;
pub const FEEDBACK: usize = 1;
pub const CROSS: usize = 2;
pub const TONE: usize = 3;
pub const MIX: usize = 4;
pub const DUCK: usize = 5;

/// Does anything move target `t`?
pub fn col_used(g: &Grid, t: usize) -> bool {
    g.iter().any(|row| row[t] != 0.0)
}

pub fn any_tap(g: &Grid) -> bool {
    (0..TAP_TARGETS).any(|t| col_used(g, t))
}

pub fn any_global(g: &Grid) -> bool {
    (TAP_TARGETS..TARGETS).any(|t| col_used(g, t))
}

/// One tap's movement from the sources' values `s` (−1…1): Pan, Cutoff and
/// Reso as the routes' clamped sum; Gain as how far it dips, 0…1 (a route
/// dips by its depth × (1 − s)/2, as v2.39 did, so Motion never boosts).
pub fn tap_mods(g: &Grid, s: &[f32; SOURCES]) -> [f32; TAP_TARGETS] {
    let sum = |t: usize| (0..SOURCES).map(|r| g[r][t] * s[r]).sum::<f32>().clamp(-1.0, 1.0);
    let dip = (0..SOURCES).map(|r| g[r][1] * (1.0 - s[r]) * 0.5).sum::<f32>().clamp(0.0, 1.0);
    [sum(0), dip, sum(2), sum(3)]
}

/// The whole echo's movement, one clamped sum per whole-echo target.
pub fn global_mods(g: &Grid, s: &[f32; SOURCES]) -> [f32; TARGETS - TAP_TARGETS] {
    core::array::from_fn(|i| (0..SOURCES).map(|r| g[r][TAP_TARGETS + i] * s[r]).sum::<f32>().clamp(-1.0, 1.0))
}

/// Gain (dB) dipped by `dip` (0…1): up to 24 dB.
pub fn gain_db(base: f32, dip: f32) -> f32 {
    base - 24.0 * dip
}

/// Colour: every colour's amount scaled by 1 + m, so one that's off stays off.
pub fn colour(amount: f32, m: f32) -> f32 {
    (amount * (1.0 + m)).clamp(0.0, 1.0)
}

/// Feedback: ±50 points, within its own range.
pub fn feedback(fb: f32, m: f32) -> f32 {
    (fb + 0.5 * m).clamp(0.0, 1.05)
}

/// Crossfeed: ±50 points.
pub fn cross(c: f32, m: f32) -> f32 {
    (c + 0.5 * m).clamp(0.0, 1.0)
}

/// Loop tone (−1 low-pass … 1 high-pass): the whole way either side.
pub fn tone(t: f32, m: f32) -> f32 {
    (t + m).clamp(-1.0, 1.0)
}

/// Mix: ±50 points.
pub fn mix(w: f32, m: f32) -> f32 {
    (w + 0.5 * m).clamp(0.0, 1.0)
}

/// Duck (dB): ±12 dB of depth.
pub fn duck(d: f32, m: f32) -> f32 {
    (d + 12.0 * m).clamp(0.0, 24.0)
}

/// Step's value for tap `tap` of `n` at `pos` (in steps): Scatter starts each
/// tap further along, up to the whole length across the taps.
pub fn step_at(steps: &[f32; 16], len: usize, pos: f64, scatter: f32, tap: usize, n: usize) -> f32 {
    let len = len.clamp(1, 16);
    let at = pos + (scatter * len as f32 * tap as f32 / n.max(1) as f32) as f64;
    steps[(at.floor() as i64).rem_euclid(len as i64) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(cells: &[(usize, usize, f32)]) -> Grid {
        let mut g = [[0.0; TARGETS]; SOURCES];
        for &(r, t, v) in cells {
            g[r][t] = v;
        }
        g
    }

    #[test]
    fn one_route_moves_as_v2_39_motion_did() {
        use crate::motion;
        let g = grid(&[(0, 0, 0.5), (0, 1, 0.8), (0, 2, 0.6), (0, 3, 1.0)]);
        for m in [-1.0, -0.4, 0.0, 0.7, 1.0] {
            let mut s = [0.0; SOURCES];
            s[0] = m;
            let [pan, dip, cut, q] = tap_mods(&g, &s);
            assert!((motion::pan(0.1, pan, 1.0) - motion::pan(0.1, m, 0.5)).abs() < 1e-6);
            assert!((gain_db(-3.0, dip) - (-3.0 - 24.0 * 0.8 * (1.0 - m) * 0.5)).abs() < 1e-5);
            assert!((motion::cutoff(1000.0, cut, 1.0) - motion::cutoff(1000.0, m, 0.6)).abs() < 0.01);
            assert!((motion::reso(0.707, q, 1.0) - motion::reso(0.707, m, 1.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn routes_add_up_and_stay_in_range() {
        let g = grid(&[(0, 0, 1.0), (1, 0, 1.0), (3, 4, 1.0), (5, 4, -0.5)]);
        let s = [1.0, 1.0, 0.0, 1.0, 0.0, 1.0];
        assert_eq!(tap_mods(&g, &s)[0], 1.0);
        assert!((global_mods(&g, &s)[COLOUR] - 0.5).abs() < 1e-6);
        // gain never boosts, whatever the routes say
        let g = grid(&[(0, 1, -1.0)]);
        assert_eq!(tap_mods(&g, &[-1.0, 0.0, 0.0, 0.0, 0.0, 0.0])[1], 0.0);
    }

    #[test]
    fn whole_echo_targets_hold_their_ranges() {
        assert_eq!(colour(0.0, 1.0), 0.0);
        assert_eq!(colour(0.6, 1.0), 1.0);
        assert_eq!(feedback(0.9, 1.0), 1.05);
        assert_eq!(cross(0.2, -1.0), 0.0);
        assert_eq!(tone(0.5, 1.0), 1.0);
        assert_eq!(mix(0.5, 0.5), 0.75);
        assert_eq!(duck(6.0, -1.0), 0.0);
    }

    #[test]
    fn step_plays_its_steps_in_turn() {
        let mut st = [0.0; 16];
        for (i, v) in st.iter_mut().enumerate() {
            *v = i as f32 / 10.0;
        }
        assert_eq!(step_at(&st, 4, 0.5, 0.0, 0, 8), 0.0);
        assert_eq!(step_at(&st, 4, 5.2, 0.0, 0, 8), 0.1);
        // a whole length across 4 taps: tap 2 is two steps on
        assert_eq!(step_at(&st, 4, 0.0, 1.0, 2, 4), 0.2);
    }
}
