//! The shifters under test. All of them read a shared delay line the way
//! Afterecho's taps do: one write per sample per channel, then each tap reads
//! at its own delay. A tap's detune is implemented *inside its read*, so the
//! tap time absorbs whatever look-behind the shifter needs.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const TAU: f32 = std::f32::consts::TAU;

pub fn cents_to_ratio(cents: f32) -> f32 {
    2f32.powf(cents / 1200.0)
}

// ─── Delay line ─────────────────────────────────────────────────────────────

/// Afterecho's `DelayLine` (power-of-two ring, 4-point Lagrange read), from
/// `mooltitwap/src/delay.rs`, with both its current read and a corrected one,
/// plus raw access for the analysis passes and
/// an optional low-passed twin that WSOLA's coarse search uses. The twin is a
/// per-channel cost shared by every tap of that channel.
pub struct DelayLine {
    buf: Vec<f32>,
    lp: Vec<f32>,
    lp_on: bool,
    lp_c: [f32; 5],
    lp_z: [f32; 2],
    write_idx: usize,
    mask: usize,
    /// Total samples written (absolute index of the next write).
    pub written: u64,
}

impl DelayLine {
    pub fn new(max_samples: usize, with_lowpass: bool, sr: f32) -> Self {
        let cap = max_samples.next_power_of_two().max(2);
        Self {
            buf: vec![0.0; cap],
            lp: if with_lowpass { vec![0.0; cap] } else { Vec::new() },
            lp_on: with_lowpass,
            lp_c: lowpass_coefs(1500.0, 0.707, sr),
            lp_z: [0.0; 2],
            write_idx: 0,
            mask: cap - 1,
            written: 0,
        }
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.write_idx] = x;
        if self.lp_on {
            let [b0, b1, b2, a1, a2] = self.lp_c;
            let y = b0 * x + self.lp_z[0];
            self.lp_z[0] = b1 * x - a1 * y + self.lp_z[1];
            self.lp_z[1] = b2 * x - a2 * y;
            self.lp[self.write_idx] = y;
        }
        self.write_idx = (self.write_idx + 1) & self.mask;
        self.written += 1;
    }

    /// Fractional delay, 4-point Lagrange, interpolating toward the *older*
    /// sample: `read(10.25)` is x[t − 10.25]. This is the read the plug-in
    /// needs once SOF-60 is fixed; B and C use it.
    #[inline]
    pub fn read(&self, delay_samples: f32) -> f32 {
        let cap = self.mask + 1;
        let d = delay_samples.max(2.0).min(cap as f32 - 3.0);
        let d_ceil = d.ceil();
        let t = d_ceil - d;
        let base = (self.write_idx + cap - 1 - d_ceil as usize) & self.mask;
        let i0 = (base + cap - 1) & self.mask;
        let i2 = (base + 1) & self.mask;
        let i3 = (base + 2) & self.mask;
        lagrange4(self.buf[i0], self.buf[base], self.buf[i2], self.buf[i3], t)
    }

    /// The plug-in's read as it is today (and as it was under the old
    /// shifter): it interpolates toward the *newer* sample, so `read(10.25)`
    /// returns x[t − 9.75] — SOF-60. A head moving at ratio r gets a
    /// 2-sample sawtooth error cycling at fs·|r − 1|. Only A uses it.
    #[inline]
    pub fn read_legacy(&self, delay_samples: f32) -> f32 {
        let cap = self.mask + 1;
        let d = delay_samples.max(1.0).min(cap as f32 - 3.0);
        let d_int = d as usize;
        let frac = d - d_int as f32;
        let base = (self.write_idx + cap - d_int - 1) & self.mask;
        let i0 = (base + cap - 1) & self.mask;
        let i2 = (base + 1) & self.mask;
        let i3 = (base + 2) & self.mask;
        lagrange4(self.buf[i0], self.buf[base], self.buf[i2], self.buf[i3], frac)
    }

    /// `out[k] = x[t - start_delay + k*stride]`, t = newest sample.
    /// Reads the low-passed twin when `lowpassed`.
    pub fn copy(&self, lowpassed: bool, start_delay: usize, stride: usize, out: &mut [f32]) {
        let src = if lowpassed { &self.lp } else { &self.buf };
        let cap = self.mask + 1;
        let idx0 = (self.write_idx + cap - 1 - start_delay) & self.mask;
        for (k, o) in out.iter_mut().enumerate() {
            *o = src[(idx0 + k * stride) & self.mask];
        }
    }

    /// Sample at an absolute index (must be already written and still held).
    #[inline]
    pub fn abs(&self, i: i64) -> f32 {
        self.buf[(i as usize) & self.mask]
    }
}

/// RBJ low-pass biquad, normalised: [b0, b1, b2, a1, a2].
pub fn lowpass_coefs(f0: f32, q: f32, sr: f32) -> [f32; 5] {
    let w0 = TAU * f0 / sr;
    let (s, c) = w0.sin_cos();
    let alpha = s / (2.0 * q);
    let a0 = 1.0 + alpha;
    [
        (1.0 - c) / 2.0 / a0,
        (1.0 - c) / a0,
        (1.0 - c) / 2.0 / a0,
        -2.0 * c / a0,
        (1.0 - alpha) / a0,
    ]
}

/// 4-point Lagrange, points at -1, 0, 1, 2 (copied from the plug-in).
#[inline]
pub fn lagrange4(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let d0 = t;
    let d1 = t - 1.0;
    let d2 = t - 2.0;
    let dm1 = t + 1.0;
    let l0 = -d0 * d1 * d2 * (1.0 / 6.0);
    let l1 = dm1 * d1 * d2 * 0.5;
    let l2 = -dm1 * d0 * d2 * 0.5;
    let l3 = dm1 * d0 * d1 * (1.0 / 6.0);
    y0 * l0 + y1 * l1 + y2 * l2 + y3 * l3
}

// ─── Shifter dispatch ───────────────────────────────────────────────────────

pub enum Shifter {
    /// No detune: one plain read, what a tap costs today.
    Plain,
    DualHann(DualHann),
    Wsola(Box<Wsola>),
    Pv(Box<PhaseVocoder>),
}

impl Shifter {
    #[inline]
    pub fn process(&mut self, line: &DelayLine, nominal: f32) -> f32 {
        match self {
            Shifter::Plain => line.read(nominal),
            Shifter::DualHann(s) => s.process(line, nominal),
            Shifter::Wsola(s) => s.process(line, nominal),
            Shifter::Pv(s) => s.process(line, nominal),
        }
    }
}

// ─── A. Baseline: the old dual-head Hann shifter ────────────────────────────

/// Rebuilt from git history (`10c4bf5` `TapReader::read_continuous`, kept
/// as `read_continuous_hann` in `bf740cb`): shared phasor at 1/window per
/// sample, two heads 180° apart, Hann windows summing to 1, each head's delay
/// `nominal - drift * window * phase`, pitch smoothed by a 5 ms one-pole.
/// Default window 60 ms (the old `grain_size_ms` default). The old code read
/// through cubic Hermite with the same (SOF-60) indexing; this reads through
/// today's Lagrange with that indexing, i.e. the plug-in's `read` as it is.
pub struct DualHann {
    /// Read through the plug-in's current (SOF-60) read, as the old code did.
    pub legacy_read: bool,
    window_samples: f32,
    phase: f32,
    pitch_target: f32,
    pitch_current: f32,
    smooth: f32,
}

impl DualHann {
    pub fn new(ratio: f32, window_ms: f32, sr: f32) -> Self {
        let tc = (0.005 * sr).max(1.0);
        Self {
            legacy_read: true,
            window_samples: (window_ms * 0.001 * sr).max(64.0),
            phase: 0.0,
            pitch_target: ratio,
            pitch_current: ratio,
            smooth: 1.0 - (-1.0_f32 / tc).exp(),
        }
    }

    pub fn set_phase(&mut self, frac: f32) {
        self.phase = frac.fract();
    }

    #[inline]
    pub fn process(&mut self, delay: &DelayLine, n: f32) -> f32 {
        self.pitch_current += self.smooth * (self.pitch_target - self.pitch_current);
        let read = |d: f32| if self.legacy_read { delay.read_legacy(d) } else { delay.read(d) };
        if (self.pitch_current - 1.0).abs() < 1e-5 {
            return read(n.max(1.0));
        }
        let drift = self.pitch_current - 1.0;

        self.phase += 1.0 / self.window_samples;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let phase_a = self.phase;
        let phase_b = if self.phase < 0.5 { self.phase + 0.5 } else { self.phase - 0.5 };
        let win_a = 0.5 * (1.0 - (phase_a * TAU).cos());
        let win_b = 0.5 * (1.0 - (phase_b * TAU).cos());
        let head_a = n - drift * self.window_samples * phase_a;
        let head_b = n - drift * self.window_samples * phase_b;
        read(head_a.max(1.0)) * win_a + read(head_b.max(1.0)) * win_b
    }
}

// ─── B. WSOLA: splices aligned by cross-correlation ─────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct WsolaCfg {
    /// Grain (window) length.
    pub grain_ms: f32,
    /// Grains overlapping at any time: 2 (hop = grain/2) or 4 (hop = grain/4).
    pub overlap: usize,
    /// ± range searched around the ideal splice point.
    pub search_ms: f32,
    /// Length of the stretch compared at each splice.
    pub corr_ms: f32,
    /// Decimation of the coarse search pass on the low-passed twin
    /// (1 = brute force at full rate, no refinement needed).
    pub coarse: usize,
    /// Pull toward the nominal splice point: score penalty at the edge of
    /// the search range (0 = pure best match).
    pub bias: f32,
}

impl WsolaCfg {
    pub const DEFAULT: WsolaCfg =
        WsolaCfg { grain_ms: 80.0, overlap: 2, search_ms: 10.0, corr_ms: 10.0, coarse: 4, bias: 0.0 };
}

#[derive(Clone, Copy, Default)]
struct Grain {
    /// Delay at age 0. The grain reads at `d0 - age * (ratio - 1)`, i.e. it
    /// plays the past at `ratio` speed.
    d0: f32,
    age: usize,
    active: bool,
}

/// Delay-line granular shifter in the WSOLA family. Grains are Hann windows
/// read at `ratio` speed. Each new grain is ideally placed so the tap stays
/// centred on its nominal delay, then moved within ± `search` to where the
/// past best matches what the current grain is about to play (normalised
/// cross-correlation), so the crossfade joins two waveforms in phase instead
/// of two copies offset by an arbitrary time. Anchoring every splice to the
/// nominal delay means alignment errors never accumulate.
pub struct Wsola {
    cfg: WsolaCfg,
    ratio: f32,
    len: usize,
    hop: usize,
    win: Vec<f32>,
    grains: [Grain; 4],
    newest: usize,
    countdown: usize,
    search: i64,
    corr: usize,
    r_buf: Vec<f32>,
    c_buf: Vec<f32>,
    rd_buf: Vec<f32>,
    cd_buf: Vec<f32>,
    pub searches: u64,
    /// Range of (new grain start − ideal start), samples: how far the tap
    /// time wanders from its setting.
    pub wander: (f32, f32),
}

impl Wsola {
    pub fn new(ratio: f32, cfg: WsolaCfg, sr: f32) -> Self {
        let k = cfg.overlap.clamp(2, 4);
        let mut len = (cfg.grain_ms * 0.001 * sr).round() as usize;
        len -= len % k;
        let hop = len / k;
        let win = (0..len)
            .map(|n| (0.5 - 0.5 * (TAU * n as f32 / len as f32).cos()) * 2.0 / k as f32)
            .collect();
        let m = cfg.coarse.max(1);
        let search = (cfg.search_ms * 0.001 * sr).round() as i64;
        let mut corr = (cfg.corr_ms * 0.001 * sr).round() as usize;
        corr -= corr % m;
        let span = 2 * search as usize + 1;
        Self {
            cfg: WsolaCfg { coarse: m, overlap: k, ..cfg },
            ratio,
            len,
            hop,
            win,
            grains: [Grain::default(); 4],
            newest: usize::MAX,
            countdown: 0,
            search,
            corr,
            r_buf: vec![0.0; corr],
            c_buf: vec![0.0; span + corr],
            rd_buf: vec![0.0; corr / m],
            cd_buf: vec![0.0; span / m + corr / m + 2],
            searches: 0,
            wander: (0.0, 0.0),
        }
    }

    /// Stagger where the grain cycle starts (spreads the search bursts of
    /// several taps over different audio blocks).
    pub fn set_phase(&mut self, frac: f32) {
        self.countdown = (frac.fract() * self.hop as f32) as usize;
    }

    #[inline]
    pub fn process(&mut self, line: &DelayLine, nominal: f32) -> f32 {
        if self.countdown == 0 {
            self.start_grain(line, nominal);
            self.countdown = self.hop;
        }
        self.countdown -= 1;
        let step = self.ratio - 1.0;
        let mut y = 0.0;
        for g in self.grains.iter_mut() {
            if g.active {
                let d = g.d0 - g.age as f32 * step;
                y += self.win[g.age] * line.read(d);
                g.age += 1;
                if g.age >= self.len {
                    g.active = false;
                }
            }
        }
        y
    }

    fn start_grain(&mut self, line: &DelayLine, nominal: f32) {
        let step = self.ratio - 1.0;
        // Start so that the grain's average delay is the nominal tap time.
        let d_ideal = nominal + step * self.len as f32 * 0.5;
        let prev = self.grains.get(self.newest).copied().filter(|g| g.active);
        let d0 = match prev {
            Some(g) => {
                let d_prev = g.d0 - g.age as f32 * step;
                let j = self.best_jump(line, d_prev, d_ideal - d_prev);
                d_prev + j
            }
            None => d_ideal,
        };
        let slot = (0..4).find(|&i| !self.grains[i].active).unwrap_or(0);
        if prev.is_some() {
            let dev = d0 - d_ideal;
            self.wander = (self.wander.0.min(dev), self.wander.1.max(dev));
        }
        self.grains[slot] = Grain { d0: d0.max(1.0), age: 0, active: true };
        self.newest = slot;
    }

    /// The jump (in delay samples, sub-sample) from the current grain to the
    /// new one that best matches waveforms, searched around `j_ideal`.
    fn best_jump(&mut self, line: &DelayLine, d_prev: f32, j_ideal: f32) -> f32 {
        let c = self.corr as i64;
        let dp = d_prev.round() as i64;
        let ji = j_ideal.round() as i64;
        // Both stretches compared must already be written: the reference reads
        // `c` samples ahead of the current grain.
        if dp < c + 4 {
            return j_ideal;
        }
        let lo = (ji - self.search).max(c + 4 - dp);
        let hi = ji + self.search;
        if lo >= hi {
            return ji.max(lo) as f32;
        }
        self.searches += 1;
        // Reference: what the current grain is about to play.
        line.copy(false, dp as usize, 1, &mut self.r_buf);
        let e_ref = energy(&self.r_buf);
        if e_ref < 1e-12 * c as f64 {
            return j_ideal;
        }
        // Pull toward the nominal splice point: on noise-like material no lag
        // matches well, and an unconstrained pick doubles or drops transients.
        let pull = self.cfg.bias as f64 / self.search.max(1) as f64;
        let m = self.cfg.coarse as i64;
        let (rhi, n) = if m <= 1 {
            (hi, (hi - lo) as usize + self.corr)
        } else {
            // Coarse: every m-th sample and lag, on the low-passed twin.
            let cd = self.corr / m as usize;
            line.copy(true, dp as usize, m as usize, &mut self.rd_buf[..cd]);
            let e_ref_d = energy(&self.rd_buf[..cd]);
            let span = (hi - lo) as usize;
            let nd = span / m as usize + cd;
            line.copy(true, (dp + hi) as usize, m as usize, &mut self.cd_buf[..nd]);
            let jc = if e_ref_d < 1e-12 * cd as f64 {
                ji
            } else {
                let cost = |i: usize| pull * ((hi - m * i as i64) - ji).abs() as f64;
                let (i0, _) = best_ncc(&self.rd_buf[..cd], &self.cd_buf[..nd], e_ref_d, cost);
                hi - m * i0 as i64
            };
            // Refine ±m at full rate and full band.
            let rhi = (jc + m).min(hi);
            let rlo = (jc - m).max(lo);
            (rhi, (rhi - rlo) as usize + self.corr)
        };
        line.copy(false, (dp + rhi) as usize, 1, &mut self.c_buf[..n]);
        let cost = |i: usize| pull * ((rhi - i as i64) - ji).abs() as f64;
        let (i0, frac) = best_ncc(&self.r_buf, &self.c_buf[..n], e_ref, cost);
        // The splice keeps the current grain's fractional position, so the
        // two waveforms sit exactly `j` apart, as compared.
        let _ = d_prev;
        (rhi - i0 as i64) as f32 - frac
    }
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|&v| (v as f64) * (v as f64)).sum()
}

/// Index `i0` maximising the normalised cross-correlation between `r` and
/// `c[i0 .. i0 + r.len()]` minus `cost(i0)`, plus a parabolic sub-sample
/// offset of the peak.
fn best_ncc(r: &[f32], c: &[f32], e_ref: f64, cost: impl Fn(usize) -> f64) -> (usize, f32) {
    let n = r.len();
    let lags = c.len() - n + 1;
    let mut e = energy(&c[..n]);
    // (score, i0, ncc at i0 − 1, ncc at i0, ncc at i0 + 1): the sub-sample
    // fit uses the raw NCC so the pull never moves the peak itself.
    let mut best = (f64::MIN, 0usize, f64::MIN, 0.0, f64::MIN);
    let mut prev_ncc = f64::MIN;
    for i0 in 0..lags {
        if i0 > 0 {
            let out = c[i0 - 1] as f64;
            let inn = c[i0 + n - 1] as f64;
            e += inn * inn - out * out;
        }
        let ncc = dot(r, &c[i0..i0 + n]) as f64 / (e_ref * e.max(0.0) + 1e-20).sqrt();
        if i0 == best.1 + 1 {
            best.4 = ncc;
        }
        let score = ncc - cost(i0);
        if score > best.0 {
            best = (score, i0, prev_ncc, ncc, f64::MIN);
        }
        prev_ncc = ncc;
    }
    let (a, b, c) = (best.2, best.3, best.4);
    let frac = if a > f64::MIN && c > f64::MIN && a - 2.0 * b + c < 0.0 {
        (0.5 * (a - c) / (a - 2.0 * b + c)).clamp(-0.5, 0.5) as f32
    } else {
        0.0
    };
    (best.1, frac)
}

/// Dot product with 8 independent accumulators so it vectorises.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut acc = [0f32; 8];
    let ca = a.chunks_exact(8);
    let cb = b.chunks_exact(8);
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for (x, y) in ra.iter().zip(rb) {
        s += x * y;
    }
    s
}

// ─── C. Phase vocoder with identity phase locking (quality reference) ───────

/// Time-stretch by `ratio` with a phase vocoder (Hann, overlap 4) using
/// Laroche & Dolson's identity phase locking (each spectral peak carries the
/// bins around it with their analysis phase relationships), then resample by
/// `ratio`. Analysis frames are read straight from the delay line, early
/// enough that the output lands on the tap time; that needs a tap time of at
/// least one frame (N samples), which is this method's latency.
pub struct PhaseVocoder {
    n: usize,
    hs: usize,
    ratio: f32,
    win: Vec<f32>,
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    frame: Vec<f32>,
    spec: Vec<Complex<f32>>,
    prev_spec: Vec<Complex<f32>>,
    syn: Vec<Complex<f32>>,
    prev_syn: Vec<Complex<f32>>,
    scratch_f: Vec<Complex<f32>>,
    scratch_i: Vec<Complex<f32>>,
    out: Vec<f32>,
    mag: Vec<f32>,
    peaks: Vec<usize>,
    rots: Vec<Complex<f32>>,
    ring: Vec<f32>,
    rmask: usize,
    frames: u64,
    read_pos: f64,
    ana_pos: f64,
    prev_ana: i64,
    started: bool,
    /// Frames that had to read samples not yet written (tap shorter than N).
    pub late_frames: u64,
}

impl PhaseVocoder {
    pub fn new(ratio: f32, n: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(n);
        let c2r = planner.plan_fft_inverse(n);
        let nb = n / 2 + 1;
        let ring_len = (4 * n).next_power_of_two();
        Self {
            n,
            hs: n / 4,
            ratio,
            win: (0..n).map(|i| 0.5 - 0.5 * (TAU * i as f32 / n as f32).cos()).collect(),
            scratch_f: r2c.make_scratch_vec(),
            scratch_i: c2r.make_scratch_vec(),
            r2c,
            c2r,
            frame: vec![0.0; n],
            spec: vec![Complex::default(); nb],
            prev_spec: vec![Complex::default(); nb],
            syn: vec![Complex::default(); nb],
            prev_syn: vec![Complex::default(); nb],
            out: vec![0.0; n],
            mag: vec![0.0; nb],
            peaks: Vec::with_capacity(nb / 2),
            rots: Vec::with_capacity(nb / 2),
            ring: vec![0.0; ring_len],
            rmask: ring_len - 1,
            frames: 0,
            read_pos: 0.0,
            ana_pos: 0.0,
            prev_ana: 0,
            started: false,
            late_frames: 0,
        }
    }

    /// Smallest tap time (samples) at which this tap lands on time.
    #[allow(dead_code)]
    pub fn min_delay(&self) -> f32 {
        self.n as f32 + 4.0 - (self.n as f32 / 2.0) * (1.0 - 1.0 / self.ratio)
    }

    /// Stagger the frame grid (spreads FFT bursts of several taps).
    pub fn set_phase(&mut self, frac: f32) {
        self.read_pos = -(frac.fract() as f64) * self.hs as f64;
    }

    #[inline]
    pub fn process(&mut self, line: &DelayLine, nominal: f32) -> f32 {
        if !self.started {
            // Stretched position s maps to input ana_pos + s/ratio + (N/2)(1-1/ratio)
            // on average; start so the tap sits on `nominal`.
            let t = line.written as f64 - 1.0;
            let r = self.ratio as f64;
            let centre = (self.n as f64 / 2.0) * (1.0 - 1.0 / r);
            self.ana_pos = t - nominal as f64 - centre - self.read_pos / r;
            self.started = true;
        }
        while self.read_pos + 3.0 > (self.frames * self.hs as u64) as f64 {
            self.synth_frame(line);
        }
        let ip = self.read_pos.floor();
        let frac = (self.read_pos - ip) as f32;
        let i = ip as i64;
        let at = |k: i64| self.ring[(k as usize) & self.rmask];
        let y = lagrange4(at(i - 1), at(i), at(i + 1), at(i + 2), frac);
        self.read_pos += self.ratio as f64;
        y
    }

    fn synth_frame(&mut self, line: &DelayLine) {
        let n = self.n;
        let nb = n / 2 + 1;
        let a = self.ana_pos.round() as i64;
        if a + n as i64 > line.written as i64 {
            self.late_frames += 1;
        }
        for i in 0..n {
            self.frame[i] = line.abs(a + i as i64) * self.win[i];
        }
        self.r2c
            .process_with_scratch(&mut self.frame, &mut self.spec, &mut self.scratch_f)
            .unwrap();

        let mut maxm = 0.0f32;
        for k in 0..nb {
            let m = self.spec[k].norm_sqr();
            self.mag[k] = m;
            maxm = maxm.max(m);
        }
        let floor = (maxm * 1e-10).max(1e-24);
        self.peaks.clear();
        for k in 2..nb - 2 {
            let m = self.mag[k];
            if m > floor
                && m > self.mag[k - 1]
                && m >= self.mag[k + 1]
                && m > self.mag[k - 2]
                && m >= self.mag[k + 2]
            {
                self.peaks.push(k);
            }
        }

        // One rotation per peak: synthesis phase = previous synthesis phase
        // + instantaneous frequency × synthesis hop; the bins in the peak's
        // region keep their analysis phase offsets from the peak.
        let ha = (a - self.prev_ana) as f32;
        let hs = self.hs as f32;
        self.rots.clear();
        for &p in &self.peaks {
            let prev_syn = self.prev_syn[p];
            let ps2 = prev_syn.norm_sqr();
            let cur = self.spec[p];
            let rot = if self.frames == 0 || ps2 < self.mag[p] * 1e-6 || ps2 < 1e-30 {
                Complex::new(1.0, 0.0)
            } else {
                let omega = TAU * p as f32 / n as f32;
                let d = cur * self.prev_spec[p].conj();
                let dev = princarg(d.im.atan2(d.re) - omega * ha);
                let adv = (omega + dev / ha) * hs;
                let u = prev_syn / ps2.sqrt();
                let v = cur.conj() / self.mag[p].sqrt();
                u * Complex::from_polar(1.0, adv) * v
            };
            self.rots.push(rot);
        }
        if self.peaks.is_empty() {
            self.syn.copy_from_slice(&self.spec);
        } else {
            let mut start = 0;
            for (pi, &p) in self.peaks.iter().enumerate() {
                let end = match self.peaks.get(pi + 1) {
                    Some(&q) => {
                        let mut kmin = p;
                        for k in p..=q {
                            if self.mag[k] < self.mag[kmin] {
                                kmin = k;
                            }
                        }
                        kmin
                    }
                    None => nb,
                };
                let rot = self.rots[pi];
                for k in start..end {
                    self.syn[k] = self.spec[k] * rot;
                }
                start = end;
            }
        }
        self.syn[0].im = 0.0;
        self.syn[nb - 1].im = 0.0;
        self.prev_spec.copy_from_slice(&self.spec);
        self.prev_syn.copy_from_slice(&self.syn);
        self.c2r
            .process_with_scratch(&mut self.syn, &mut self.out, &mut self.scratch_i)
            .unwrap();

        // Overlap-add into the stretched ring (Hann² at overlap 4 sums to 1.5).
        let base = self.frames as usize * self.hs;
        for i in n - self.hs..n {
            self.ring[(base + i) & self.rmask] = 0.0;
        }
        let g = 1.0 / (n as f32 * 1.5);
        for i in 0..n {
            self.ring[(base + i) & self.rmask] += self.out[i] * self.win[i] * g;
        }
        self.frames += 1;
        self.prev_ana = a;
        self.ana_pos += self.hs as f64 / self.ratio as f64;
    }
}

#[inline]
fn princarg(x: f32) -> f32 {
    x - TAU * (x / TAU).round()
}

// ─── D. Shift in the loop (comparison only) ─────────────────────────────────

/// Frequency shifter from Tide's recipe (`y-flanger/DSP_FOR_REUSE.md` §5):
/// 12-pole Hilbert pair (I = even poles, Q = odd, ~90° apart 20 Hz–20 kHz),
/// output `i·cos θ + q·sin θ`. Shifts every frequency by the same number of
/// Hz, so in a feedback loop each repeat moves further. Not a pitch shifter.
pub struct FreqShifter {
    a: [f32; 12],
    x1: [f32; 12],
    y1: [f32; 12],
    theta: f32,
    inc: f32,
}

impl FreqShifter {
    pub fn new(shift_hz: f32, sr: f32) -> Self {
        let mut a = [0.0f32; 12];
        for (j, aj) in a.iter_mut().enumerate() {
            let m = if j < 6 { 2 * j } else { 2 * (j - 6) + 1 };
            let f = 20.0 * 1000f32.powf(m as f32 / 11.0);
            let t = (std::f32::consts::PI * f / sr).tan();
            *aj = (t - 1.0) / (t + 1.0);
        }
        Self { a, x1: [0.0; 12], y1: [0.0; 12], theta: 0.0, inc: TAU * shift_hz / sr }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut stage = |j: usize, v: f32| {
            let y = self.a[j] * v + self.x1[j] - self.a[j] * self.y1[j];
            self.x1[j] = v;
            self.y1[j] = y;
            y
        };
        let mut i = x;
        for j in 0..6 {
            i = stage(j, i);
        }
        let mut q = x;
        for j in 6..12 {
            q = stage(j, q);
        }
        self.theta += self.inc;
        if self.theta > TAU {
            self.theta -= TAU;
        } else if self.theta < -TAU {
            self.theta += TAU;
        }
        i * self.theta.cos() + q * self.theta.sin()
    }
}
