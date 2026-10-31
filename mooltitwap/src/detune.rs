//! Per-tap detune (v2.45): a tap can sound up to a semitone up or down. The
//! shift happens inside the tap's read of the delay line, so the tap time
//! absorbs whatever the shifter needs to look back, and the repeats keep the
//! tap's own detune (it doesn't stack pass after pass).
//!
//! Light is WSOLA (from the v2.44 listening test, `spikes/detune/`): Hann
//! grains played at the shift ratio, each new grain moved within ±10 ms of
//! where it should start to where the past best matches what the current
//! grain is about to play, so the crossfade joins two waveforms in phase.
//! A tap's channels (and Ping's bounce lines) splice at the same moments, so
//! the image holds. The cost: the tap's time wanders up to ±10 ms.
//!
//! Clean is a phase vocoder with identity phase locking (Laroche & Dolson):
//! clean on dense chords, steady timing, about 3× Light's CPU, and it needs
//! about 90 ms of look-back, so shorter taps use Light.

use crate::delay::DelayLine;
use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Detune range either way, in cents.
pub const MAX_CENTS: f32 = 100.0;

const GRAIN_MS: f32 = 80.0;
const SEARCH_MS: f32 = 10.0;
const CORR_MS: f32 = 10.0;

pub fn ratio(cents: f32) -> f32 {
    2f32.powf(cents / 1200.0)
}

/// Where a detuned tap reads this sample: up to three grains, each a delay
/// and a window weight. Every line the tap reads is read through the same
/// grains.
#[derive(Clone, Copy, Default)]
pub struct Grains {
    d: [f32; 3],
    w: [f32; 3],
}

impl crate::delay::ReadLine for Grains {
    #[inline]
    fn read_line(&self, line: &DelayLine) -> f32 {
        let mut y = 0.0;
        for k in 0..3 {
            if self.w[k] != 0.0 {
                y += self.w[k] * line.read(self.d[k]);
            }
        }
        y
    }
}

#[derive(Clone, Copy, Default)]
struct Grain {
    /// The delay it reads now; it moves by 1 − ratio a sample.
    d: f32,
    age: usize,
    active: bool,
}

/// One tap's WSOLA (Light). All buffers are made up front: nothing allocates
/// while it runs.
pub struct Wsola {
    len: usize,
    hop: usize,
    win: Vec<f32>,
    grains: [Grain; 3],
    newest: Option<usize>,
    countdown: usize,
    search: i64,
    corr: usize,
    r_buf: Vec<f32>,
    c_buf: Vec<f32>,
}

impl Wsola {
    pub fn new(sr: f32) -> Self {
        let mut len = (GRAIN_MS * 0.001 * sr).round() as usize;
        len -= len % 2;
        let hop = len / 2;
        let search = (SEARCH_MS * 0.001 * sr).round() as i64;
        let corr = (CORR_MS * 0.001 * sr).round() as usize;
        Self {
            len,
            hop,
            // Hann at overlap 2 sums to 1
            win: (0..len).map(|n| 0.5 - 0.5 * (core::f32::consts::TAU * n as f32 / len as f32).cos()).collect(),
            grains: [Grain::default(); 3],
            newest: None,
            countdown: 0,
            search,
            corr,
            r_buf: vec![0.0; corr],
            c_buf: vec![0.0; 2 * search as usize + 1 + corr],
        }
    }

    /// Start over (the next sample starts a grain on the tap time), with its
    /// grain cycle `phase` (0…1) in: taps start theirs at different moments,
    /// so their searches don't land in the same audio block.
    pub fn restart(&mut self, phase: f32) {
        self.grains = [Grain::default(); 3];
        self.newest = None;
        self.countdown = (phase.fract() * self.hop as f32) as usize + 1;
    }

    /// One sample of a tap at `nominal` samples, shifting by `ratio`. `src`
    /// fills a buffer from the line the splices are matched on: `out[k]` is
    /// the sample `start − k` back (`start` back, then newer).
    #[inline]
    pub fn step(&mut self, nominal: f32, ratio: f32, src: &impl Fn(usize, &mut [f32])) -> Grains {
        if self.newest.is_none() {
            // just restarted: two grains already under way, as if it had been
            // running, so it starts at full level (Hann at overlap 2 sums to 1)
            let c = self.countdown.min(self.hop);
            let step = ratio - 1.0;
            for (slot, age) in [(0, self.hop - c), (1, self.len - c)] {
                let d = nominal + step * (self.len as f32 * 0.5 - age as f32);
                self.grains[slot] = Grain { d: d.max(1.0), age, active: true };
            }
            self.newest = Some(0);
        }
        if self.countdown == 0 {
            self.start_grain(nominal, ratio, src);
            self.countdown = self.hop;
        }
        self.countdown -= 1;
        let mut out = Grains::default();
        let mut k = 0;
        for g in self.grains.iter_mut().filter(|g| g.active) {
            out.d[k] = g.d.max(1.0);
            out.w[k] = self.win[g.age];
            k += 1;
            g.d -= ratio - 1.0;
            g.age += 1;
            if g.age >= self.len {
                g.active = false;
            }
        }
        out
    }

    fn start_grain(&mut self, nominal: f32, ratio: f32, src: &impl Fn(usize, &mut [f32])) {
        // start where the grain's average delay is the tap time
        let d_ideal = nominal + (ratio - 1.0) * self.len as f32 * 0.5;
        let prev = self.newest.map(|i| self.grains[i]).filter(|g| g.active);
        let d0 = match prev {
            Some(g) => g.d + self.best_jump(g.d, d_ideal - g.d, src),
            None => d_ideal,
        };
        let slot = (0..3).find(|&i| !self.grains[i].active).unwrap_or(0);
        self.grains[slot] = Grain { d: d0.max(1.0), age: 0, active: true };
        self.newest = Some(slot);
    }

    /// The jump (delay samples, sub-sample) from the current grain to the new
    /// one that best matches waveforms, searched around `j_ideal`.
    fn best_jump(&mut self, d_prev: f32, j_ideal: f32, src: &impl Fn(usize, &mut [f32])) -> f32 {
        let c = self.corr as i64;
        let dp = d_prev.round() as i64;
        let ji = j_ideal.round() as i64;
        // what's compared must be written already: the reference reads `c`
        // samples newer than the current grain
        if dp < c + 4 {
            return j_ideal;
        }
        let lo = (ji - self.search).max(c + 4 - dp);
        let hi = ji + self.search;
        if lo >= hi {
            return ji.max(lo) as f32;
        }
        // reference: what the current grain is about to play
        src(dp as usize, &mut self.r_buf);
        let e_ref = energy(&self.r_buf);
        if e_ref < 1e-12 * c as f64 {
            return j_ideal;
        }
        let n = (hi - lo) as usize + self.corr;
        src((dp + hi) as usize, &mut self.c_buf[..n]);
        let (i0, frac) = best_ncc(&self.r_buf, &self.c_buf[..n], e_ref);
        (hi - i0 as i64) as f32 - frac
    }
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|&v| (v as f64) * (v as f64)).sum()
}

/// The lag `i0` maximising the normalised cross-correlation between `r` and
/// `c[i0 .. i0 + r.len()]`, plus a parabolic sub-sample offset of the peak.
fn best_ncc(r: &[f32], c: &[f32], e_ref: f64) -> (usize, f32) {
    let n = r.len();
    let lags = c.len() - n + 1;
    let mut e = energy(&c[..n]);
    // (ncc, i0, ncc at i0 − 1, ncc at i0 + 1)
    let mut best = (f64::MIN, 0usize, f64::MIN, f64::MIN);
    let mut prev = f64::MIN;
    for i0 in 0..lags {
        if i0 > 0 {
            let (out, inn) = (c[i0 - 1] as f64, c[i0 + n - 1] as f64);
            e += inn * inn - out * out;
        }
        let ncc = dot(r, &c[i0..i0 + n]) as f64 / (e_ref * e.max(0.0) + 1e-20).sqrt();
        if i0 == best.1 + 1 {
            best.3 = ncc;
        }
        if ncc > best.0 {
            best = (ncc, i0, prev, f64::MIN);
        }
        prev = ncc;
    }
    let (a, b, c) = (best.2, best.0, best.3);
    let frac = if a > f64::MIN && c > f64::MIN && a - 2.0 * b + c < 0.0 {
        (0.5 * (a - c) / (a - 2.0 * b + c)).clamp(-0.5, 0.5) as f32
    } else {
        0.0
    };
    (best.1, frac)
}

/// Dot product with 8 accumulators, so it vectorises.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut acc = [0f32; 8];
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
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

/// One channel of a tap's phase vocoder (Clean): time-stretch by the ratio,
/// then resample by it. Analysis frames are read from the tap's lines early
/// enough that the output lands on the tap time; each frame's place follows
/// the tap time as it moves. Nothing allocates while it runs.
pub struct Pv {
    n: usize,
    hs: usize,
    win: Vec<f32>,
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    frame: Vec<f32>,
    aux: Vec<f32>,
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
    prev_ana: i64,
    /// Samples since the restart: the newest input's index.
    now: i64,
}

impl Pv {
    pub fn new(sr: f32) -> Self {
        // about 85 ms of frame at any rate (4096 at 48 kHz)
        let n = ((0.085 * sr) as usize).next_power_of_two().max(1024);
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(n);
        let c2r = planner.plan_fft_inverse(n);
        let nb = n / 2 + 1;
        let ring_len = (4 * n).next_power_of_two();
        Self {
            n,
            hs: n / 4,
            win: (0..n).map(|i| 0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / n as f32).cos()).collect(),
            scratch_f: r2c.make_scratch_vec(),
            scratch_i: c2r.make_scratch_vec(),
            r2c,
            c2r,
            frame: vec![0.0; n],
            aux: vec![0.0; n],
            spec: vec![Complex::default(); nb],
            prev_spec: vec![Complex::default(); nb],
            syn: vec![Complex::default(); nb],
            prev_syn: vec![Complex::default(); nb],
            out: vec![0.0; n],
            mag: vec![0.0; nb],
            peaks: Vec::with_capacity(nb),
            rots: Vec::with_capacity(nb),
            ring: vec![0.0; ring_len],
            rmask: ring_len - 1,
            frames: 0,
            read_pos: 0.0,
            prev_ana: 0,
            now: 0,
        }
    }

    /// The shortest tap time (samples) at which it lands on time.
    pub fn min_delay(&self, ratio: f32) -> f32 {
        self.n as f32 + 8.0 + (self.n as f32 / 2.0) * (1.0 - 1.0 / ratio).abs()
    }

    /// Start over, its frame grid `phase` (0…1) in (taps apart, so their
    /// FFTs don't land in the same audio block).
    pub fn restart(&mut self, phase: f32) {
        self.ring.fill(0.0);
        self.frames = 0;
        // three frames in, so the overlap-add is whole from the first sample
        self.read_pos = (3.0 + phase.fract() as f64) * self.hs as f64;
        self.now = 0;
        self.prev_ana = 0;
    }

    /// One sample of a tap at `nominal` samples, shifting by `ratio`.
    /// `fill(back, out, aux)` fills `out` with the tap's input, `out[k]`
    /// being `back − k` samples back (`aux` is scratch for it).
    #[inline]
    pub fn step(&mut self, nominal: f32, ratio: f32, fill: &impl Fn(usize, &mut [f32], &mut [f32])) -> f32 {
        let r = ratio as f64;
        while self.read_pos + 3.0 > (self.frames * self.hs as u64) as f64 {
            // where this frame's input starts so the tap sits on its time
            let centre = (self.n as f64 / 2.0) * (1.0 - 1.0 / r);
            let ahead = ((self.frames * self.hs as u64) as f64 - self.read_pos) / r;
            let a = (self.now as f64 + ahead - nominal as f64 - centre).round() as i64;
            // never past the newest sample
            let a = a.min(self.now - self.n as i64 + 1);
            self.synth_frame(a, fill);
        }
        let ip = self.read_pos.floor();
        let frac = (self.read_pos - ip) as f32;
        let i = ip as i64;
        let at = |k: i64| self.ring[(k as usize) & self.rmask];
        let y = lagrange4_fwd(at(i - 1), at(i), at(i + 1), at(i + 2), frac);
        self.read_pos += r;
        self.now += 1;
        y
    }

    fn synth_frame(&mut self, a: i64, fill: &impl Fn(usize, &mut [f32], &mut [f32])) {
        let n = self.n;
        let nb = n / 2 + 1;
        fill((self.now - a).max(0) as usize, &mut self.frame, &mut self.aux);
        for (x, w) in self.frame.iter_mut().zip(&self.win) {
            *x *= w;
        }
        if self.r2c.process_with_scratch(&mut self.frame, &mut self.spec, &mut self.scratch_f).is_err() {
            return;
        }
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
            if m > floor && m > self.mag[k - 1] && m >= self.mag[k + 1] && m > self.mag[k - 2] && m >= self.mag[k + 2] {
                self.peaks.push(k);
            }
        }
        // One rotation per peak: synthesis phase = the last synthesis phase +
        // instantaneous frequency × the synthesis hop; the bins round a peak
        // keep their analysis phase offsets from it.
        let ha = ((a - self.prev_ana) as f32).max(1.0);
        let hs = self.hs as f32;
        self.rots.clear();
        for &p in &self.peaks {
            let prev_syn = self.prev_syn[p];
            let ps2 = prev_syn.norm_sqr();
            let cur = self.spec[p];
            let rot = if self.frames == 0 || ps2 < self.mag[p] * 1e-6 || ps2 < 1e-30 {
                Complex::new(1.0, 0.0)
            } else {
                let omega = core::f32::consts::TAU * p as f32 / n as f32;
                let d = cur * self.prev_spec[p].conj();
                let dev = princarg(d.im.atan2(d.re) - omega * ha);
                let adv = (omega + dev / ha) * hs;
                prev_syn / ps2.sqrt() * Complex::from_polar(1.0, adv) * (cur.conj() / self.mag[p].sqrt())
            };
            self.rots.push(rot);
        }
        if self.peaks.is_empty() {
            self.syn.copy_from_slice(&self.spec);
        } else {
            let mut start = 0;
            for (pi, &p) in self.peaks.iter().enumerate() {
                let end = match self.peaks.get(pi + 1) {
                    Some(&q) => (p..=q).min_by(|&x, &y| self.mag[x].total_cmp(&self.mag[y])).unwrap_or(q),
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
        if self.c2r.process_with_scratch(&mut self.syn, &mut self.out, &mut self.scratch_i).is_err() {
            return;
        }
        // overlap-add into the stretched ring (Hann² at overlap 4 sums to 1.5)
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
    }
}

#[inline]
fn princarg(x: f32) -> f32 {
    x - core::f32::consts::TAU * (x / core::f32::consts::TAU).round()
}

/// 4-point Lagrange forward in time (the ring runs oldest to newest).
#[inline]
fn lagrange4_fwd(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let (d0, d1, d2, dm1) = (t, t - 1.0, t - 2.0, t + 1.0);
    -d0 * d1 * d2 * (1.0 / 6.0) * y0 + dm1 * d1 * d2 * 0.5 * y1 - dm1 * d0 * d2 * 0.5 * y2 + dm1 * d0 * d1 * (1.0 / 6.0) * y3
}
