//! DEGRADE — physically-modeled tape degradation, ported from **RE-DEEMER**
//! (`~/re-deemer/crates/te2-dsp`, ISC license — attribute in any release
//! notes). This is the "channel-effect" scoping merge-rs's TAPE plan
//! describes: the tape chain without the reel/transport/motor physics.
//!
//! Chain per channel (per host sample):
//!   pre-emphasis → ×drive → [2× oversampled J-A hysteresis] → ×makeup
//!   → self-erasure LP → repro EQ (de-emph · head bump · gap loss · spacing)
//!   → dry/wet mix
//!
//! Ported verbatim: `magnetics.rs` (Jiles–Atherton + RK4, bias blend, after
//! Chowdhury DAFx-19), `heads_eq.rs` (RecordEq/ReproEq), the 47-tap halfband
//! from `oversample.rs`, and the RBJ biquad/one-pole utilities. Cut: noise
//! (hiss), dropouts, wow/flutter, tape stocks (WEAR stands in for aging).

// ─── filters (re-deemer `filters/biquad.rs` subset) ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoeffs {
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// RBJ cookbook lowpass.
    pub fn lowpass(sample_rate: f32, fc: f32, q: f32) -> Self {
        let fc = fc.clamp(10.0, sample_rate * 0.49);
        let w0 = std::f32::consts::TAU * fc / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos) * 0.5) / a0,
            b1: (1.0 - cos) / a0,
            b2: ((1.0 - cos) * 0.5) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// RBJ cookbook peaking EQ, gain in dB.
    pub fn peaking(sample_rate: f32, fc: f32, q: f32, gain_db: f32) -> Self {
        let fc = fc.clamp(10.0, sample_rate * 0.49);
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * fc / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// RBJ cookbook high shelf, gain in dB.
    pub fn high_shelf(sample_rate: f32, fc: f32, slope: f32, gain_db: f32) -> Self {
        let fc = fc.clamp(10.0, sample_rate * 0.49);
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * fc / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha)) / a0,
            b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos)) / a0,
            b2: (a * ((a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha)) / a0,
            a1: (2.0 * ((a - 1.0) - (a + 1.0) * cos)) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha) / a0,
        }
    }
}

/// Transposed direct form II biquad.
#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    #[inline]
    pub fn process(&mut self, c: &BiquadCoeffs, x: f32) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// One-pole lowpass/highpass building block.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    state: f32,
}

impl OnePole {
    pub fn coeff(sample_rate: f32, fc: f32) -> f32 {
        let fc = fc.clamp(0.01, sample_rate * 0.49);
        1.0 - (-std::f32::consts::TAU * fc / sample_rate).exp()
    }

    #[inline]
    pub fn lowpass(&mut self, coeff: f32, x: f32) -> f32 {
        self.state += coeff * (x - self.state);
        self.state
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

// ─── 2× halfband oversampler (re-deemer `oversample.rs`, X2 stage only) ─────

/// Odd-index taps (k = 1, 3, 5, ... 23) of the 47-tap Kaiser (beta=9)
/// halfband; the center tap is exactly 0.5 and all other even taps are zero.
/// Passband flat to 0.18·fs, aliases suppressed >84 dB above 0.32·fs.
const ODD_TAPS: [f32; 12] = [
    3.157_664_2e-1,
    -9.868_68e-2,
    5.197_895_7e-2,
    -3.042_467_5e-2,
    1.801_273_4e-2,
    -1.034_221_7e-2,
    5.596_731e-3,
    -2.776_692_9e-3,
    1.218_266_7e-3,
    -4.450_039e-4,
    1.181_470_4e-4,
    -1.265_518_2e-5,
];

const HIST: usize = 24;

/// One halfband stage: interpolate by 2 and decimate by 2.
#[derive(Clone)]
pub struct Halfband {
    up_hist: [f32; HIST],
    down_odd_hist: [f32; HIST],
    down_even_hist: [f32; HIST],
}

impl Default for Halfband {
    fn default() -> Self {
        Self { up_hist: [0.0; HIST], down_odd_hist: [0.0; HIST], down_even_hist: [0.0; HIST] }
    }
}

impl Halfband {
    /// One input sample in, two output samples (at 2x rate) out.
    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        self.up_hist.copy_within(1.., 0);
        self.up_hist[HIST - 1] = x;
        let mut mid = 0.0f32;
        for (i, tap) in ODD_TAPS.iter().enumerate() {
            let a = self.up_hist[11 - i];
            let b = self.up_hist[12 + i];
            mid += tap * (a + b);
        }
        let on_grid = self.up_hist[12];
        [2.0 * mid, on_grid]
    }

    /// Two input samples (at 2x rate, in time order) in, one output out.
    #[inline]
    pub fn down(&mut self, x: [f32; 2]) -> f32 {
        self.down_even_hist.copy_within(1.., 0);
        self.down_even_hist[HIST - 1] = x[0];
        self.down_odd_hist.copy_within(1.., 0);
        self.down_odd_hist[HIST - 1] = x[1];
        let center = 0.5 * self.down_odd_hist[11];
        let mut fir = 0.0f32;
        for (i, tap) in ODD_TAPS.iter().enumerate() {
            let a = self.down_even_hist[11 - i];
            let b = self.down_even_hist[12 + i];
            fir += tap * (a + b);
        }
        center + fir
    }

    pub fn reset(&mut self) {
        self.up_hist = [0.0; HIST];
        self.down_odd_hist = [0.0; HIST];
        self.down_even_hist = [0.0; HIST];
    }
}

// ─── magnetics (re-deemer `tape/magnetics.rs`, verbatim) ────────────────────

/// J-A parameters relative to M_s = 1. Ratios from the DAFx-19 paper's tape
/// parameter set.
#[derive(Debug, Clone, Copy)]
pub struct MagParams {
    pub a: f64,
    pub alpha: f64,
    pub k: f64,
    pub c: f64,
    /// Signal -> field scale: how hard 0 dBFS hits the tape.
    pub drive: f64,
    /// 1.0 = ideal AC bias (anhysteretic), 0.0 = fully under-biased (raw J-A).
    pub bias: f64,
    /// Output normalization so small signals pass at unity gain.
    pub out_norm: f64,
}

impl MagParams {
    /// Baseline ferric (Type I) profile.
    pub fn type_i() -> Self {
        let a = 2.2e4 / 3.5e5;
        Self {
            a,
            alpha: 1.6e-3,
            k: 2.7e4 / 3.5e5,
            c: 0.17,
            drive: 4.0 * a,
            bias: 0.82,
            out_norm: 1.0,
        }
    }

    /// Chrome (Type II): higher coercivity, more headroom, cleaner top.
    pub fn type_ii() -> Self {
        let base = Self::type_i();
        Self { k: base.k * 1.25, drive: base.drive * 0.78, bias: 0.88, ..base }
    }

    /// Metal (Type IV): the most headroom, stays composed when slammed.
    pub fn type_iv() -> Self {
        let base = Self::type_i();
        Self { k: base.k * 1.5, drive: base.drive * 0.6, bias: 0.92, ..base }
    }

    /// Compute `out_norm` so the small-signal gain of the blended output is
    /// unity. ~10k iterations: call at initialize, never on the audio thread.
    pub fn normalized(mut self) -> Self {
        self.out_norm = 1.0;
        let mut hyst = Hysteresis::default();
        let os_rate = 192_000.0;
        let dt = 1.0 / os_rate;
        let freq = 1_000.0;
        let amp = 0.05f64;
        let n = (os_rate / freq) as usize * 10;

        let mut in_sq = 0.0f64;
        let mut out_sq = 0.0f64;
        for k in 0..n {
            let x = amp * (std::f64::consts::TAU * freq * k as f64 / os_rate).sin();
            let y = hyst.process(&self, x as f32, dt) as f64;
            if k >= n / 2 {
                in_sq += x * x;
                out_sq += y * y;
            }
        }
        let gain = (out_sq / in_sq.max(1e-30)).sqrt();
        self.out_norm = 1.0 / gain.max(1e-6);
        self
    }
}

/// Langevin function and derivative, numerically safe near zero.
#[inline]
fn langevin(x: f64) -> f64 {
    if x.abs() < 1e-4 { x / 3.0 - x * x * x / 45.0 } else { 1.0 / x.tanh() - 1.0 / x }
}

#[inline]
fn langevin_deriv(x: f64) -> f64 {
    if x.abs() < 1e-4 {
        1.0 / 3.0 - x * x / 15.0
    } else {
        let coth = 1.0 / x.tanh();
        1.0 - coth * coth + 1.0 / (x * x)
    }
}

/// Per-channel hysteresis state. Call `process()` once per oversampled sample.
#[derive(Debug, Clone, Default)]
pub struct Hysteresis {
    m: f64,
    h_prev: f64,
    m_an_prev: f64,
}

impl Hysteresis {
    /// J-A dM/dt as a function of state and field slew, eq. (5) of DAFx-19.
    #[inline]
    fn dm_dt(p: &MagParams, m: f64, h: f64, dh_dt: f64) -> f64 {
        let q = (h + p.alpha * m) / p.a;
        let m_an = langevin(q);
        let l_prime = langevin_deriv(q);
        let delta = if dh_dt >= 0.0 { 1.0 } else { -1.0 };
        let diff = m_an - m;
        let delta_m = if diff * delta >= 0.0 { 1.0 } else { 0.0 };

        let denom_irr = (1.0 - p.c) * delta * p.k - p.alpha * diff;
        let irr = if denom_irr.abs() < 1e-12 {
            0.0
        } else {
            (1.0 - p.c) * delta_m * diff / denom_irr
        };
        let rev = p.c * l_prime / p.a;
        let denom = 1.0 - p.c * p.alpha * l_prime / p.a;
        ((irr + rev) / denom.max(1e-9)) * dh_dt
    }

    /// Process one oversampled sample through the magnetic model.
    #[inline]
    pub fn process(&mut self, p: &MagParams, x: f32, dt: f64) -> f32 {
        let h = x as f64 * p.drive;
        let dh_dt = (h - self.h_prev) / dt;
        let h_half = 0.5 * (h + self.h_prev);

        // RK4 on the J-A ODE.
        let m = self.m;
        let k1 = dt * Self::dm_dt(p, m, self.h_prev, dh_dt);
        let k2 = dt * Self::dm_dt(p, m + 0.5 * k1, h_half, dh_dt);
        let k3 = dt * Self::dm_dt(p, m + 0.5 * k2, h_half, dh_dt);
        let k4 = dt * Self::dm_dt(p, m + k3, h, dh_dt);
        let mut m_next = m + (k1 + 2.0 * k2 + 2.0 * k3 + k4) / 6.0;
        if !m_next.is_finite() {
            m_next = 0.0;
        }
        self.m = m_next.clamp(-1.0, 1.0);
        self.h_prev = h;

        // Anhysteretic path (ideally biased recording).
        let mut m_an = langevin((h + p.alpha * self.m_an_prev) / p.a);
        m_an = langevin((h + p.alpha * m_an) / p.a);
        self.m_an_prev = m_an;

        let blended = p.bias * m_an + (1.0 - p.bias) * self.m;
        (blended * p.out_norm) as f32
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ─── heads EQ (re-deemer `tape/heads_eq.rs`, speed fixed at 1.0) ────────────

/// IEC cassette tape types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapeKind {
    #[default]
    I,
    II,
    IV,
}

/// EQ voicing per tape type.
#[derive(Debug, Clone, Copy)]
pub struct EqProfile {
    pub emphasis_db: f32,
    pub emphasis_fc: f32,
    pub bump_fc: f32,
    pub bump_db: f32,
    pub gap_fc: f32,
    pub spacing_db: f32,
    pub spacing_fc: f32,
}

impl EqProfile {
    pub fn for_kind(kind: TapeKind) -> Self {
        match kind {
            TapeKind::I => Self {
                emphasis_db: 6.0,
                emphasis_fc: 5_200.0,
                bump_fc: 85.0,
                bump_db: 2.6,
                gap_fc: 13_000.0,
                spacing_db: -2.5,
                spacing_fc: 9_000.0,
            },
            TapeKind::II => Self {
                emphasis_db: 4.5,
                emphasis_fc: 6_300.0,
                bump_fc: 80.0,
                bump_db: 2.2,
                gap_fc: 15_000.0,
                spacing_db: -1.8,
                spacing_fc: 10_000.0,
            },
            TapeKind::IV => Self {
                emphasis_db: 3.5,
                emphasis_fc: 7_000.0,
                bump_fc: 76.0,
                bump_db: 1.8,
                gap_fc: 16_500.0,
                spacing_db: -1.2,
                spacing_fc: 11_000.0,
            },
        }
    }
}

/// Record-side EQ: pre-emphasis shelf + dynamic self-erasure lowpass.
pub struct RecordEq {
    sample_rate: f32,
    pre_emph_coeffs: BiquadCoeffs,
    pre_emph: Biquad,
    erase_env: OnePole,
    erase_env_coeff: f32,
    erase_lp: OnePole,
    wear: f32,
}

impl RecordEq {
    pub fn new(sample_rate: f32, profile: &EqProfile) -> Self {
        let mut eq = Self {
            sample_rate,
            pre_emph_coeffs: BiquadCoeffs::IDENTITY,
            pre_emph: Biquad::default(),
            erase_env: OnePole::default(),
            erase_env_coeff: OnePole::coeff(sample_rate, 25.0),
            erase_lp: OnePole::default(),
            wear: 0.0,
        };
        eq.set_profile(profile);
        eq
    }

    pub fn set_wear(&mut self, wear: f32) {
        self.wear = wear.clamp(0.0, 1.0);
    }

    pub fn set_profile(&mut self, profile: &EqProfile) {
        self.pre_emph_coeffs = BiquadCoeffs::high_shelf(
            self.sample_rate,
            profile.emphasis_fc,
            0.9,
            profile.emphasis_db,
        );
    }

    /// Before the magnetics.
    #[inline]
    pub fn pre(&mut self, x: f32) -> f32 {
        self.pre_emph.process(&self.pre_emph_coeffs, x)
    }

    /// After the magnetics: level-dependent HF self-erasure.
    #[inline]
    pub fn post(&mut self, x: f32) -> f32 {
        let env = self.erase_env.lowpass(self.erase_env_coeff, x.abs());
        let excess = (env - (0.55 - 0.20 * self.wear)).max(0.0);
        let fc = 20_000.0 * (1.0 - 0.55 * self.wear) / (1.0 + 6.0 * excess);
        let coeff = OnePole::coeff(self.sample_rate, fc);
        self.erase_lp.lowpass(coeff, x)
    }

    pub fn reset(&mut self) {
        self.pre_emph.reset();
        self.erase_env.reset();
        self.erase_lp.reset();
    }
}

/// Repro-side EQ: de-emphasis, head bump, gap loss, spacing loss.
pub struct ReproEq {
    sample_rate: f32,
    profile: EqProfile,
    wear_gap_mul: f32,
    de_emph_coeffs: BiquadCoeffs,
    de_emph: Biquad,
    bump_coeffs: BiquadCoeffs,
    bump: Biquad,
    gap_coeffs: BiquadCoeffs,
    gap: Biquad,
    spacing_coeffs: BiquadCoeffs,
    spacing: Biquad,
}

impl ReproEq {
    pub fn new(sample_rate: f32, profile: &EqProfile) -> Self {
        let mut eq = Self {
            sample_rate,
            profile: *profile,
            wear_gap_mul: 1.0,
            de_emph_coeffs: BiquadCoeffs::IDENTITY,
            de_emph: Biquad::default(),
            bump_coeffs: BiquadCoeffs::IDENTITY,
            bump: Biquad::default(),
            gap_coeffs: BiquadCoeffs::IDENTITY,
            gap: Biquad::default(),
            spacing_coeffs: BiquadCoeffs::IDENTITY,
            spacing: Biquad::default(),
        };
        eq.set_profile(profile);
        eq.update_speed_sections();
        eq
    }

    pub fn set_profile(&mut self, profile: &EqProfile) {
        self.profile = *profile;
        self.de_emph_coeffs = BiquadCoeffs::high_shelf(
            self.sample_rate,
            profile.emphasis_fc,
            0.9,
            -profile.emphasis_db,
        );
        self.spacing_coeffs = BiquadCoeffs::high_shelf(
            self.sample_rate,
            profile.spacing_fc,
            0.8,
            profile.spacing_db,
        );
        self.update_speed_sections();
    }

    pub fn set_wear(&mut self, wear: f32) {
        self.wear_gap_mul = 1.0 - 0.62 * wear.clamp(0.0, 1.0);
        self.update_speed_sections();
    }

    /// re-deemer's `set_speed`, fixed at nominal speed 1.0 (no transport here).
    fn update_speed_sections(&mut self) {
        let bump_fc = self.profile.bump_fc.clamp(25.0, 400.0);
        self.bump_coeffs = BiquadCoeffs::peaking(self.sample_rate, bump_fc, 1.1, self.profile.bump_db);
        let gap_fc = (self.profile.gap_fc * self.wear_gap_mul).clamp(1_200.0, 0.45 * self.sample_rate);
        self.gap_coeffs = BiquadCoeffs::lowpass(self.sample_rate, gap_fc, 0.6);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = self.de_emph.process(&self.de_emph_coeffs, x);
        let x = self.bump.process(&self.bump_coeffs, x);
        let x = self.gap.process(&self.gap_coeffs, x);
        self.spacing.process(&self.spacing_coeffs, x)
    }

    pub fn reset(&mut self) {
        self.de_emph.reset();
        self.bump.reset();
        self.gap.reset();
        self.spacing.reset();
    }
}

// ─── the stereo DEGRADE wrapper ─────────────────────────────────────────────

pub struct TapeDegradeParams {
    pub kind: TapeKind,
    /// Drive into the tape in dB (0 = the calibrated operating point).
    pub drive_db: f32,
    /// Aging 0..1 — sags bias (more hysteresis grit), lowers the self-erasure
    /// knee and the gap-loss corner, drives the operating point hotter.
    pub wear: f32,
    /// Dry/wet 0..1. 0 = bypass.
    pub mix: f32,
}

pub struct TapeDegrade {
    sample_rate: f32,
    /// Pre-normalized MagParams per tape type ([I, II, IV]) — the ~10k-iter
    /// calibration runs once at construction, never on the audio thread.
    mags: [MagParams; 3],
    kind: TapeKind,
    /// Bias-sagged copy of the active type's params (wear-adjusted).
    mag: MagParams,
    hysteresis: [Hysteresis; 2],
    os: [Halfband; 2],
    record_eq: [RecordEq; 2],
    repro_eq: [ReproEq; 2],
    drive_in: f32,
    drive_out: f32,
    mix: f32,
    applied_wear: f32,
}

impl TapeDegrade {
    pub fn new(sample_rate: f32) -> Self {
        let mags = [
            MagParams::type_i().normalized(),
            MagParams::type_ii().normalized(),
            MagParams::type_iv().normalized(),
        ];
        let profile = EqProfile::for_kind(TapeKind::I);
        Self {
            sample_rate,
            mags,
            kind: TapeKind::I,
            mag: mags[0],
            hysteresis: [Hysteresis::default(), Hysteresis::default()],
            os: [Halfband::default(), Halfband::default()],
            record_eq: [RecordEq::new(sample_rate, &profile), RecordEq::new(sample_rate, &profile)],
            repro_eq: [ReproEq::new(sample_rate, &profile), ReproEq::new(sample_rate, &profile)],
            drive_in: 1.0,
            drive_out: 1.0,
            mix: 0.0,
            applied_wear: -1.0,
        }
    }

    fn mag_index(kind: TapeKind) -> usize {
        match kind {
            TapeKind::I => 0,
            TapeKind::II => 1,
            TapeKind::IV => 2,
        }
    }

    /// Per-block parameter push (control rate — filter design happens here).
    pub fn set_params(&mut self, p: &TapeDegradeParams) {
        let wear = p.wear.clamp(0.0, 1.0);
        if p.kind != self.kind {
            self.kind = p.kind;
            let profile = EqProfile::for_kind(p.kind);
            for ch in 0..2 {
                self.record_eq[ch].set_profile(&profile);
                self.repro_eq[ch].set_profile(&profile);
            }
            self.applied_wear = -1.0; // force wear re-apply on the new type
        }
        if (wear - self.applied_wear).abs() > 1e-3 {
            self.applied_wear = wear;
            // A worn machine's bias sags: more hysteresis grit (re-deemer
            // engine.rs wear application, stock trims folded into WEAR).
            self.mag = self.mags[Self::mag_index(self.kind)];
            self.mag.bias = (self.mag.bias * (1.0 - 0.35 * wear as f64)).clamp(0.2, 1.0);
            for ch in 0..2 {
                self.record_eq[ch].set_wear(wear);
                self.repro_eq[ch].set_wear(wear);
            }
        }
        // Worn oxide is driven relatively harder and gives back less —
        // verbatim re-deemer law (stock drive_mul → the DRIVE knob). Full
        // makeup: DRIVE sets how hard the tape saturates, not the level.
        let user_drive = 10.0f32.powf(p.drive_db / 20.0);
        self.drive_in = user_drive * (1.0 + 0.5 * wear);
        self.drive_out = (1.0 / self.drive_in.max(1e-3)) * (1.0 - 0.12 * wear);
        self.mix = p.mix.clamp(0.0, 1.0);
    }

    #[inline]
    fn process_ch(&mut self, ch: usize, x: f32) -> f32 {
        let dt_os = 1.0 / (2.0 * self.sample_rate as f64);
        let emphasized = self.record_eq[ch].pre(x);
        let driven = emphasized * self.drive_in;
        // 2× oversampled hysteresis.
        let [a, b] = self.os[ch].up(driven);
        let ya = self.hysteresis[ch].process(&self.mag, a, dt_os);
        let yb = self.hysteresis[ch].process(&self.mag, b, dt_os);
        let magnetized = self.os[ch].down([ya, yb]) * self.drive_out;
        // Self-erasure of hot highs, then the repro chain.
        let shaped = self.record_eq[ch].post(magnetized);
        self.repro_eq[ch].process(shaped)
    }

    #[inline]
    pub fn process(&mut self, l: &mut f32, r: &mut f32) {
        let mix = self.mix;
        let wet_l = self.process_ch(0, *l);
        let wet_r = self.process_ch(1, *r);
        *l = *l * (1.0 - mix) + wet_l * mix;
        *r = *r * (1.0 - mix) + wet_r * mix;
    }

    pub fn reset(&mut self) {
        for ch in 0..2 {
            self.hysteresis[ch].reset();
            self.os[ch].reset();
            self.record_eq[ch].reset();
            self.repro_eq[ch].reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a 1 kHz sine through the full chain and return the RMS gain over
    /// the settled half.
    fn measure(drive_db: f32, wear: f32, amp: f32) -> f64 {
        let sr = 48_000.0f32;
        let mut t = TapeDegrade::new(sr);
        t.set_params(&TapeDegradeParams { kind: TapeKind::I, drive_db, wear, mix: 1.0 });
        let n = 48_000;
        let mut in_sq = 0.0f64;
        let mut out_sq = 0.0f64;
        for k in 0..n {
            let x = amp * (std::f32::consts::TAU * 1_000.0 * k as f32 / sr).sin();
            let (mut l, mut r) = (x, x);
            t.process(&mut l, &mut r);
            assert!(l.is_finite() && r.is_finite(), "output must stay finite");
            assert!(l.abs() < 4.0, "runaway output {l}");
            if k > n / 2 {
                in_sq += (x as f64) * (x as f64);
                out_sq += (l as f64) * (l as f64);
            }
        }
        (out_sq / in_sq.max(1e-30)).sqrt()
    }

    #[test]
    fn small_signal_passes_near_unity() {
        // The EQ chain (bump/gap/spacing) colors but shouldn't grossly change
        // level at -26 dBFS and neutral drive.
        let gain = measure(0.0, 0.0, 0.05);
        assert!((0.5..1.5).contains(&gain), "small-signal gain {gain:.3}");
    }

    #[test]
    fn hot_drive_compresses() {
        // Slamming the tape (+18 dB drive at 0 dBFS) must compress vs the
        // small-signal gain — tape, not a wire.
        let g_soft = measure(0.0, 0.0, 0.05);
        let g_hot = measure(18.0, 0.0, 1.0);
        assert!(g_hot < g_soft * 0.75, "hot {g_hot:.3} soft {g_soft:.3}");
    }

    #[test]
    fn wear_darkens() {
        // Full wear must not blow up and must still pass signal.
        let gain = measure(6.0, 1.0, 0.3);
        assert!(gain > 0.1 && gain < 1.5, "worn gain {gain:.3}");
    }
}
