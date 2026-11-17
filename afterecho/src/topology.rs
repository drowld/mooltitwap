use crate::params::{SpacingMode, NUM_TAPS};

/// Layer 1: pure-math tap position calculator.
/// Given a base time, mode, shape parameters, and n_active, produces normalized tap
/// positions in [0, 1]. The DSP layer multiplies these by NUM_TAPS * base_time
/// to get actual delay times.
///
/// Cluster-Delay–style anchoring (v2.8.0):
///   - tap 0 is ALWAYS at 1/NUM_TAPS (= one step from the origin)
///   - tap n_active-1 is ALWAYS at n_active/NUM_TAPS
///   - intermediate taps are shaped between these two anchors by the XY pad
/// This keeps the outer boundary of the cluster fixed regardless of shape, so
/// the XY pad only adjusts the *density distribution* between first and last tap.
#[derive(Clone, Debug)]
pub struct TopologyEngine {
    positions: [f32; NUM_TAPS],
}

impl TopologyEngine {
    pub fn new() -> Self {
        let mut e = Self { positions: [0.0; NUM_TAPS] };
        e.compute(Spacing::LINEAR, NUM_TAPS);
        e
    }

    /// Recompute tap positions (see [`compute_positions`]).
    pub fn compute(&mut self, spacing: Spacing, n_active: usize) {
        self.positions = compute_positions(spacing, n_active);
    }

    /// Tap positions, normalized.
    /// Position[i] * (NUM_TAPS * base_time) = actual delay for tap i.
    pub fn positions(&self) -> &[f32; NUM_TAPS] {
        &self.positions
    }
}

impl Default for TopologyEngine {
    fn default() -> Self { Self::new() }
}

/// How the taps spread: the mode, the Shape pad (`shape_x` [-1, 1] biases
/// density toward earlier or later taps, `shape_y` [0, 1] morphs linear →
/// sigmoid), and since v2.38 Euclid's steps and rotation and swing.
#[derive(Clone, Copy, Debug)]
pub struct Spacing {
    pub mode: SpacingMode,
    pub shape_x: f32,
    pub shape_y: f32,
    /// Euclid: the pattern's length in steps (a step is the base time) and
    /// how many steps it's turned against the dry note.
    pub steps: usize,
    pub rotate: usize,
    /// 0.5 straight … 0.75: the 1st, 3rd, 5th… tap lands late by
    /// (2·swing − 1) of its gap to the next tap, so a bent Shape and any
    /// Euclid pattern swing (in Euclid the last tap's next is the first, a
    /// loop later). Lin and Euclid only: Exp and Log have no grid.
    pub swing: f32,
}

impl Spacing {
    pub const LINEAR: Spacing =
        Spacing { mode: SpacingMode::Linear, shape_x: 0.0, shape_y: 0.0, steps: NUM_TAPS, rotate: 0, swing: 0.5 };
}

/// Euclid: the steps (1..=n) that get a tap for `k` taps over `n` steps
/// turned by `rotate`: E(k, n), every tap as far from the next as can be
/// (Bresenham's spread). The dry note is step 0, so a tap that lands there
/// sounds at step n, the next pattern's start. Returns the steps in order and
/// how many there are (k, at most n). No allocation: it runs per block.
pub fn euclid_steps(k: usize, n: usize, rotate: usize) -> ([usize; NUM_TAPS], usize) {
    let n = n.clamp(1, NUM_TAPS);
    let k = k.clamp(1, n);
    let mut out = [0usize; NUM_TAPS];
    let mut count = 0;
    for i in 0..n {
        if (i * k) % n < k {
            let step = (i + rotate) % n;
            out[count] = if step == 0 { n } else { step };
            count += 1;
        }
    }
    out[..count].sort_unstable();
    (out, count)
}

/// Free-function form of `TopologyEngine::compute` — the single copy of the
/// anchoring/shaping math, shared by the DSP (via the engine) and the GUI
/// visualizer (directly), so the two can never drift.
///
/// `n_active` — the number of live taps; positions [0..n_active) are anchored
/// to [1/N, n_active/N]; positions [n_active..NUM_TAPS) continue linearly. In
/// Euclid the live taps sit on their pattern's steps instead.
pub fn compute_positions(sp: Spacing, n_active: usize) -> [f32; NUM_TAPS] {
    let n_active = n_active.clamp(1, NUM_TAPS);
    let step = 1.0 / NUM_TAPS as f32;
    let late = 2.0 * sp.swing - 1.0;
    let mut positions = [0.0; NUM_TAPS];
    if sp.mode == SpacingMode::Euclidean {
        let (steps, count) = euclid_steps(n_active, sp.steps, sp.rotate);
        for (i, p) in positions.iter_mut().enumerate() {
            *p = if i < count { steps[i] as f32 * step } else { (i + 1) as f32 * step };
        }
        // Swing as in Lin (owner, 2026-11-16; before, only taps on odd steps moved, so an
        // on-the-beat pattern such as the default 8 in 16 never swung): the 1st, 3rd, 5th… tap
        // moves toward the next one, the last toward the pattern's first hit one loop later.
        if late != 0.0 && count > 0 {
            let first_again = (sp.steps.max(1) as f32 * step) + positions[0];
            for i in (0..count).step_by(2) {
                let next = if i + 1 < count { positions[i + 1] } else { first_again };
                positions[i] += late * (next - positions[i]);
            }
        }
        return positions;
    }
    let (mode, shape_x, shape_y) = (sp.mode, sp.shape_x, sp.shape_y);
    let first = 1.0 / NUM_TAPS as f32;
    let last  = n_active as f32 / NUM_TAPS as f32;

    for i in 0..NUM_TAPS {
        if i < n_active {
            // t ∈ [0, 1] normalized within the active window
            let t = if n_active == 1 {
                0.0
            } else {
                i as f32 / (n_active - 1) as f32
            };

            let base = match mode {
                SpacingMode::Linear      => t,
                SpacingMode::Exponential => t.powf(2.0).min(1.0),
                SpacingMode::Logarithmic => t.sqrt(),
                SpacingMode::Euclidean   => unreachable!(),
            };

            let shaped = sigmoid_blend(base, shape_y);
            let biased = bias_curve(shaped, shape_x);

            // Map shaped [0,1] back to [first, last].
            // sigmoid_blend and bias_curve both preserve {0,1}, so the anchors
            // are guaranteed: biased=0 → first, biased=1 → last.
            positions[i] = first + biased * (last - first);
        } else {
            // Inactive taps beyond the active window: linear continuation
            positions[i] = (i + 1) as f32 / NUM_TAPS as f32;
        }
    }
    // Swing: the taps on odd steps (the 1st, 3rd, 5th…) move toward the next.
    if mode == SpacingMode::Linear && late != 0.0 {
        for i in (0..n_active).step_by(2) {
            let next = if i + 1 < n_active {
                positions[i + 1]
            } else {
                2.0 * positions[i] - if i > 0 { positions[i - 1] } else { 0.0 }
            };
            positions[i] += late * (next - positions[i]);
        }
    }
    positions
}

/// One tap's nominal delay in SECONDS: (position + offset) × span.
/// `time_offset` is a fraction of the cluster span (NUM_TAPS × base time),
/// NOT of the visualizer's display range. Shared by lib.rs (× sample rate)
/// and gui.rs::compute_viz_data.
pub fn tap_delay_secs(pos: f32, time_offset: f32, span_secs: f32) -> f32 {
    (pos + time_offset) * span_secs
}

/// True if a tap's L/R delays fit the ring buffer (95% guard). Mirrors the
/// DSP's out-of-range silencing: over-budget taps are muted, not clamped.
pub fn tap_fits(t_l_secs: f32, t_r_secs: f32, max_delay_secs: f32) -> bool {
    let guard = max_delay_secs * 0.95;
    t_l_secs <= guard && t_r_secs <= guard
}

/// Spiral-protection floor for the feedback time (10 ms).
pub const MIN_FB_SECS: f32 = 0.010;

/// Cluster span in seconds: max delay across active, in-range taps (offsets
/// included), floored at MIN_FB_SECS and at `pattern_secs` (Euclid's whole
/// pattern, v2.38, so a pattern whose last tap lands early still loops on
/// its length). This is the feedback-cycle length when FB TIME is locked —
/// shared by lib.rs and the echo visualizer so echo bars land where the
/// audio does.
pub fn cluster_span_secs(
    t_l: &[f32; NUM_TAPS],
    t_r: &[f32; NUM_TAPS],
    in_range: &[bool; NUM_TAPS],
    n_active: usize,
    pattern_secs: f32,
) -> f32 {
    (0..n_active.min(NUM_TAPS))
        .filter(|&i| in_range[i])
        .map(|i| t_l[i].max(t_r[i]))
        .fold(MIN_FB_SECS.max(pattern_secs), f32::max)
}

/// Blend between linear (y=0) and sigmoid (y=1) curves.
/// Preserves {0, 1}.
fn sigmoid_blend(x: f32, y: f32) -> f32 {
    if y <= 0.0 { return x; }
    let s = x * x * (3.0 - 2.0 * x);
    x * (1.0 - y) + s * y
}

/// Bias the curve toward 0 (x<0) or 1 (x>0). x in [-1, 1].
/// Preserves {0, 1}.
fn bias_curve(v: f32, x: f32) -> f32 {
    if x.abs() < 1e-6 { return v; }
    let power = if x >= 0.0 { 1.0 / (1.0 + 2.0 * x) } else { 1.0 + 2.0 * (-x) };
    v.powf(power)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(k: usize, n: usize, r: usize) -> Vec<usize> {
        let (s, c) = euclid_steps(k, n, r);
        s[..c].to_vec()
    }

    #[test]
    fn euclid_spreads_the_taps_evenly() {
        // the tresillo, its first hit moved to the next downbeat
        assert_eq!(steps(3, 8, 0), [3, 6, 8]);
        assert_eq!(steps(4, 16, 0), [4, 8, 12, 16]);
        assert_eq!(steps(5, 16, 0), [4, 7, 10, 13, 16]);
        // turned a step, and turns wrap
        assert_eq!(steps(3, 8, 1), [1, 4, 7]);
        assert_eq!(steps(3, 8, 9), steps(3, 8, 1));
        // every step, and never more taps than steps
        assert_eq!(steps(8, 8, 0), (1..=8).collect::<Vec<_>>());
        assert_eq!(steps(12, 8, 0).len(), 8);
    }

    #[test]
    fn euclid_puts_each_tap_on_its_step() {
        let sp = Spacing { mode: SpacingMode::Euclidean, steps: 8, rotate: 1, ..Spacing::LINEAR };
        let pos = compute_positions(sp, 3);
        assert_eq!(&pos[..3], &[1.0, 4.0, 7.0].map(|st| st / NUM_TAPS as f32));
    }

    #[test]
    fn swing_pushes_the_odd_steps_late() {
        let step = 1.0 / NUM_TAPS as f32;
        // Euclid, steps 3, 6, 8 at 75 %: the 1st and 3rd taps go halfway to the next, the 3rd's
        // next being the 3 again a loop (8 steps) later
        let sp = Spacing { mode: SpacingMode::Euclidean, steps: 8, swing: 0.75, ..Spacing::LINEAR };
        let got = compute_positions(sp, 3);
        let want = [4.5, 6.0, 9.5].map(|x| x * step);
        assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{:?}", &got[..3]);
        // the default 8 in 16 (every tap on an even step) swings too: 2→3, 4, 6→7, 8…
        let even = Spacing { mode: SpacingMode::Euclidean, steps: 16, swing: 0.75, ..Spacing::LINEAR };
        let (plain, swung) = (compute_positions(Spacing { swing: 0.5, ..even }, 8), compute_positions(even, 8));
        for i in 0..8 {
            let moved = (swung[i] - plain[i]) / step;
            assert!((moved - if i % 2 == 0 { 1.0 } else { 0.0 }).abs() < 1e-5, "tap {i} moved {moved} steps");
        }
        // Lin, straight: the taps on steps 1…4; swung, the 1st and 3rd move half a step
        assert_eq!(&compute_positions(Spacing::LINEAR, 4)[..4], &[1.0, 2.0, 3.0, 4.0].map(|x| x * step));
        let lin = compute_positions(Spacing { swing: 0.75, ..Spacing::LINEAR }, 4);
        let want = [1.5, 2.0, 3.5, 4.0].map(|x| x * step);
        assert!(lin.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{lin:?}");
        // a bent Shape still swings: an odd-step tap goes halfway to the next
        let bent = Spacing { shape_x: 0.4, shape_y: 0.7, ..Spacing::LINEAR };
        let (plain, swung) = (compute_positions(bent, 6), compute_positions(Spacing { swing: 0.75, ..bent }, 6));
        assert!((swung[2] - (plain[2] + 0.5 * (plain[3] - plain[2]))).abs() < 1e-6 && swung[3] == plain[3]);
        // Exp and Log have no grid: swing leaves them be
        for mode in [SpacingMode::Exponential, SpacingMode::Logarithmic] {
            let sp = Spacing { mode, ..Spacing::LINEAR };
            assert_eq!(compute_positions(sp, 7), compute_positions(Spacing { swing: 0.75, ..sp }, 7));
        }
    }
}
