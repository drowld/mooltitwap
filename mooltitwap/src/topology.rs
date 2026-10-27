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
        e.compute(SpacingMode::Linear, 0.0, 0.0, NUM_TAPS);
        e
    }

    /// Recompute tap positions.
    ///
    /// `shape_x` [-1, 1] biases density toward earlier (< 0) or later (> 0) taps.
    /// `shape_y` [0, 1] morphs linear → sigmoid distribution.
    /// `n_active` — the number of live taps; positions [0..n_active) are anchored
    /// to [1/N, n_active/N]; positions [n_active..NUM_TAPS) continue linearly.
    pub fn compute(&mut self, mode: SpacingMode, shape_x: f32, shape_y: f32, n_active: usize) {
        self.positions = compute_positions(mode, shape_x, shape_y, n_active);
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

/// Free-function form of `TopologyEngine::compute` — the single copy of the
/// anchoring/shaping math, shared by the DSP (via the engine) and the GUI
/// visualizer (directly), so the two can never drift.
pub fn compute_positions(mode: SpacingMode, shape_x: f32, shape_y: f32, n_active: usize) -> [f32; NUM_TAPS] {
    let n_active = n_active.clamp(1, NUM_TAPS);
    let first = 1.0 / NUM_TAPS as f32;
    let last  = n_active as f32 / NUM_TAPS as f32;
    let mut positions = [0.0; NUM_TAPS];

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
                SpacingMode::Euclidean   => t,   // placeholder
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
/// included), floored at MIN_FB_SECS. This is the feedback-cycle length when
/// FB TIME is locked, and the floor when unlocked — shared by lib.rs and the
/// echo visualizer so echo bars land where the audio does.
pub fn cluster_span_secs(
    t_l: &[f32; NUM_TAPS],
    t_r: &[f32; NUM_TAPS],
    in_range: &[bool; NUM_TAPS],
    n_active: usize,
) -> f32 {
    (0..n_active.min(NUM_TAPS))
        .filter(|&i| in_range[i])
        .map(|i| t_l[i].max(t_r[i]))
        .fold(MIN_FB_SECS, f32::max)
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
