//! The four Diffuse candidates. Each takes Amount (0…1) and Size (× its
//! nominal delays) at construction, and runs on a stereo pair.

use crate::material::SR;

const TAU: f32 = std::f32::consts::TAU;

/// A Schroeder allpass in lattice form, v(n) = x − g·v(n−D), y = g·v + v(n−D),
/// with a fractional (and optionally slowly moving) delay.
struct Ap {
    buf: Vec<f32>,
    w: usize,
    d: f32,
    g: f32,
    depth: f32,
    rate: f32,
    phase: f32,
    frac_prev: f32,
}

impl Ap {
    fn new(ms: f32, g: f32, depth_ms: f32, rate_hz: f32, phase: f32) -> Self {
        // a still stage reads whole samples, so it stays exactly allpass
        let depth = depth_ms * 0.001 * SR;
        let d = if depth > 0.0 { ms * 0.001 * SR } else { (ms * 0.001 * SR).round() }.max(2.0);
        let len = (d + depth + 4.0).ceil() as usize;
        Ap { buf: vec![0.0; len], w: 0, d, g, depth, rate: rate_hz / SR, phase, frac_prev: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let mut d = self.d;
        if self.depth > 0.0 {
            d += self.depth * (TAU * self.phase).sin();
            self.phase = (self.phase + self.rate).fract();
        }
        // v, d samples back (the newest written is 1 back): 4-point
        // Lagrange, so a moving stage loses little top
        let n = self.buf.len();
        if self.depth == 0.0 {
            // a still stage: one whole-sample read, as the plug-in does
            let v_d = self.buf[(self.w + n - d as usize) % n];
            let v = x - self.g * v_d;
            self.buf[self.w] = v;
            self.w = (self.w + 1) % n;
            return self.g * v + v_d;
        }
        // a moving stage: whole samples, then the fraction through a
        // first-order allpass (Thiran), as the plug-in does since v2.47 (a
        // Lagrange read here took the top off every pass)
        let k = (d - 0.5).floor().max(1.0);
        let eta = d - k;
        let a = (1.0 - eta) / (1.0 + eta);
        let at = |k: usize| self.buf[(self.w + n - k) % n];
        let v_d = a * at(k as usize) + at(k as usize + 1) - a * self.frac_prev;
        self.frac_prev = v_d;
        let v = x - self.g * v_d;
        self.buf[self.w] = v;
        self.w = (self.w + 1) % n;
        self.g * v + v_d
    }
}

pub trait Diffuser {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32);
}

/// A — today's plug-in (`diffusion.rs`): eight allpasses of 12.5–53 ms in
/// series, g = 0.6, the same on both sides; Amount blends dry with them.
pub struct Current {
    l: Vec<Ap>,
    r: Vec<Ap>,
    amount: f32,
}

impl Current {
    pub fn new(amount: f32, size: f32) -> Self {
        let ms = [12.5, 17.8, 23.7, 29.1, 37.5, 41.7, 47.3, 53.1];
        // the plug-in rounds each stage to whole samples
        let mk = || ms.iter().map(|&m| Ap::new((m * size * 0.001 * SR).round().max(1.0) / SR * 1000.0, 0.6, 0.0, 0.0, 0.0)).collect();
        Current { l: mk(), r: mk(), amount }
    }
}

impl Diffuser for Current {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut wl, mut wr) = (l, r);
        for ap in &mut self.l {
            wl = ap.process(wl);
        }
        for ap in &mut self.r {
            wr = ap.process(wr);
        }
        let a = self.amount;
        (l * (1.0 - a) + wl * a, r * (1.0 - a) + wr * a)
    }
}

/// B — dense: six short allpasses (1.3–11 ms), different on each side, so
/// it widens; Amount is the allpass gain (never a dry blend, so its
/// response stays flat at every setting).
pub struct Dense {
    l: Vec<Ap>,
    r: Vec<Ap>,
}

const DENSE_L: [f32; 6] = [1.31, 2.27, 3.53, 5.17, 7.43, 10.9];
/// The same total as the left (30.61 ms), so at low Amount the sides sum
/// to mono without a comb.
const DENSE_R: [f32; 6] = [1.43, 2.11, 3.79, 4.93, 7.91, 10.44];

/// Amount → allpass gain for B and C.
fn dense_g(amount: f32) -> f32 {
    0.75 * amount.clamp(0.0, 1.0).powf(0.6)
}

impl Dense {
    pub fn new(amount: f32, size: f32, moving: bool) -> Self {
        let g = dense_g(amount);
        // C: each stage's delay drifts ±0.12 ms × size at its own slow rate
        let rates = [0.37, 0.53, 0.71, 0.89, 1.13, 1.31];
        let mk = |ms: &[f32; 6], ph: f32| {
            ms.iter()
                .zip(rates)
                .enumerate()
                .map(|(k, (&m, rate))| {
                    let depth = if moving { 0.12 * size } else { 0.0 };
                    Ap::new(m * size, g, depth, rate, ph + 0.17 * k as f32)
                })
                .collect()
        };
        Dense { l: mk(&DENSE_L, 0.0), r: mk(&DENSE_R, 0.5) }
    }
}

impl Diffuser for Dense {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut wl, mut wr) = (l, r);
        for ap in &mut self.l {
            wl = ap.process(wl);
        }
        for ap in &mut self.r {
            wr = ap.process(wr);
        }
        (wl, wr)
    }
}

/// D — a small feedback delay network (8 lines, 3.1–19.7 ms, Hadamard
/// mix, gentle damping) after two of B's allpasses: a short, dense,
/// decaying smear. Amount sets its decay (60 → 260 ms) and blends it in.
pub struct Fdn {
    pre_l: Vec<Ap>,
    pre_r: Vec<Ap>,
    lines: Vec<Vec<f32>>,
    w: [usize; 8],
    g: [f32; 8],
    lp: [f32; 8],
    mix: f32,
}

impl Fdn {
    pub fn new(amount: f32, size: f32) -> Self {
        let g_ap = 0.6;
        let pre = |ms: &[f32; 6]| vec![Ap::new(ms[1] * size, g_ap, 0.0, 0.0, 0.0), Ap::new(ms[3] * size, g_ap, 0.0, 0.0, 0.0)];
        let t60 = 0.06 + 0.2 * amount;
        let ms = [3.1f32, 4.3, 5.9, 7.7, 9.8, 12.7, 15.9, 19.7];
        let lens = ms.map(|m| ((m * size * 0.001 * SR).round() as usize).max(2));
        Fdn {
            pre_l: pre(&DENSE_L),
            pre_r: pre(&DENSE_R),
            lines: lens.iter().map(|&n| vec![0.0; n]).collect(),
            w: [0; 8],
            g: lens.map(|n| 10f32.powf(-3.0 * n as f32 / SR / t60)),
            lp: [0.0; 8],
            mix: amount.clamp(0.0, 1.0).sqrt(),
        }
    }
}

impl Diffuser for Fdn {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut pl, mut pr) = (l, r);
        for ap in &mut self.pre_l {
            pl = ap.process(pl);
        }
        for ap in &mut self.pre_r {
            pr = ap.process(pr);
        }
        let o: [f32; 8] = core::array::from_fn(|k| self.lines[k][self.w[k]]);
        // damping: one pole at ~9 kHz in each line
        for k in 0..8 {
            self.lp[k] += (o[k] - self.lp[k]) * 0.7;
        }
        // fast Walsh–Hadamard, scaled to stay orthogonal
        let mut h = self.lp;
        let mut span = 1;
        while span < 8 {
            for i in (0..8).step_by(2 * span) {
                for j in i..i + span {
                    let (a, b) = (h[j], h[j + span]);
                    h[j] = a + b;
                    h[j + span] = a - b;
                }
            }
            span *= 2;
        }
        let s = 1.0 / 8f32.sqrt();
        for k in 0..8 {
            let inp = if k % 2 == 0 { pl } else { pr };
            let n = self.lines[k].len();
            self.lines[k][self.w[k]] = 0.5 * inp + self.g[k] * h[k] * s;
            self.w[k] = (self.w[k] + 1) % n;
        }
        let wl = 0.5 * (o[0] + o[2] + o[4] + o[6]) + 0.25 * pl;
        let wr = 0.5 * (o[1] + o[3] + o[5] + o[7]) + 0.25 * pr;
        let m = self.mix;
        (l * (1.0 - m) + wl * m, r * (1.0 - m) + wr * m)
    }
}

pub fn make(kind: char, amount: f32, size: f32) -> Box<dyn Diffuser> {
    match kind {
        'A' => Box::new(Current::new(amount, size)),
        'B' => Box::new(Dense::new(amount, size, false)),
        'C' => Box::new(Dense::new(amount, size, true)),
        _ => Box::new(Fdn::new(amount, size)),
    }
}

pub const KINDS: [(char, &str); 4] = [
    ('A', "today: 8 long allpasses, dry blend"),
    ('B', "dense: 6 short allpasses, L≠R, Amount = gain"),
    ('C', "dense + slow movement"),
    ('D', "small FDN smear"),
];
