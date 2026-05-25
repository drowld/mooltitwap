use crate::params::FilterType;

/// TPT state-variable filter (Vadim Zavalishin form). Stable up to Nyquist.
#[derive(Default, Clone)]
pub struct Svf {
    s1: f32,
    s2: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Svf {
    pub fn new() -> Self { Self::default() }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let f = (cutoff_hz / sample_rate).clamp(1.0e-5, 0.49);
        let g = (core::f32::consts::PI * f).tan();
        let k = 1.0 / q.max(0.01);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        self.g = g;
        self.k = k;
        self.a1 = a1;
        self.a2 = a2;
        self.a3 = a3;
    }

    #[inline]
    pub fn process(&mut self, x: f32, kind: FilterType) -> f32 {
        let v3 = x - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;
        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;
        match kind {
            FilterType::LowPass => v2,
            FilterType::BandPass => v1,
            FilterType::HighPass => x - self.k * v1 - v2,
        }
    }
}

/// Soft tanh saturator with drive.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    x.tanh()
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
