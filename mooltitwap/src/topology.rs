use crate::params::{SpacingMode, NUM_TAPS};

/// Layer 1: pure-math tap position calculator.
/// Given a base time, mode, and shape parameters, produces normalized tap
/// positions in [0, 1]. The DSP layer multiplies these by NUM_TAPS * base_time
/// to get actual delay times.
#[derive(Clone, Debug)]
pub struct TopologyEngine {
    positions: [f32; NUM_TAPS],
}

impl TopologyEngine {
    pub fn new() -> Self {
        let mut e = Self { positions: [0.0; NUM_TAPS] };
        e.compute(SpacingMode::Linear, 0.0, 0.0);
        e
    }

    /// Recompute tap positions. `shape_x` in [-1, 1] biases toward tap1 (-1) or
    /// tap8 (+1). `shape_y` in [0, 1] morphs linear → sigmoid.
    pub fn compute(&mut self, mode: SpacingMode, shape_x: f32, shape_y: f32) {
        for i in 0..NUM_TAPS {
            let n = (i + 1) as f32 / NUM_TAPS as f32; // 1/8 .. 8/8
            let base = match mode {
                SpacingMode::Linear => n,
                SpacingMode::Exponential => (n.powf(2.0)).min(1.0),
                SpacingMode::Logarithmic => n.sqrt(),
                SpacingMode::Euclidean => n, // placeholder — same as linear for now
            };

            let shaped = sigmoid_blend(base, shape_y);
            let biased = bias_curve(shaped, shape_x);
            self.positions[i] = biased;
        }
    }

    /// Tap positions, normalized to roughly [0, 1].
    /// Position N is the multiplier applied to (NUM_TAPS * base_time) for tap N.
    pub fn positions(&self) -> &[f32; NUM_TAPS] {
        &self.positions
    }
}

impl Default for TopologyEngine {
    fn default() -> Self { Self::new() }
}

/// Blend between linear (y=0) and sigmoid (y=1) curves.
fn sigmoid_blend(x: f32, y: f32) -> f32 {
    if y <= 0.0 { return x; }
    // smoothstep-like sigmoid: 3x^2 - 2x^3 centered around 0.5
    let s = x * x * (3.0 - 2.0 * x);
    x * (1.0 - y) + s * y
}

/// Bias the curve toward 0 (x<0) or 1 (x>0). x in [-1, 1].
fn bias_curve(v: f32, x: f32) -> f32 {
    if x.abs() < 1e-6 { return v; }
    // power-curve bias: positive x compresses values upward, negative downward
    let power = if x >= 0.0 {
        1.0 / (1.0 + 2.0 * x)
    } else {
        1.0 + 2.0 * (-x)
    };
    v.powf(power)
}
