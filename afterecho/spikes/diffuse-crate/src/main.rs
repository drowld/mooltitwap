//! Afterecho Diffuse against the crate's DIFFUSION and SPACES: offline
//! listening test (crate adoption pass 4).
//!
//!   cargo run --release            # reels and metrics into renders/
//!
//! Today's Diffuse is the plug-in's own `src/diffusion.rs`, compiled here
//! unchanged. The candidates from `softsystems-fx`: DIFFUSION (Cumulus's
//! diffuser) at two sizes, and SPACES in its ROOM and VAULT modes, short.
//! Each sits where Afterecho's Diffuse can: In (before the delay), FB (inside
//! the feedback loop) and Out (on the wet). The delay around them is a plain
//! stand-in for Afterecho's. Nothing here is used by the plug-in.

mod delay;
#[path = "../../../src/diffusion.rs"]
#[allow(dead_code)]
mod diffusion;
mod material;
mod wav;

use material::SR;
use realfft::RealFftPlanner;
use softsystems_fx::diffusion::{Diffusion as CrateDiffusion, Params as DiffParams};
use softsystems_fx::reverb::{Mode, Settings as SpaceSettings, Spaces};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
enum Route {
    In,
    Fb,
    Out,
}

const ROUTES: [(Route, &str); 3] = [(Route::In, "in"), (Route::Fb, "fb"), (Route::Out, "out")];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Today,
    /// The crate's DIFFUSION at this SIZE (0.2…2).
    Diff(f32),
    /// SPACES in this mode.
    Space(Mode),
}

const KINDS: [(Kind, &str); 5] = [
    (Kind::Today, "today (v2.47)"),
    (Kind::Diff(0.2), "DIFFUSION S (size 0.2)"),
    (Kind::Diff(0.5), "DIFFUSION M (size 0.5)"),
    (Kind::Space(Mode::Room), "SPACES ROOM"),
    (Kind::Space(Mode::Vault), "SPACES VAULT"),
];

/// SPACES's settings here: short and close, so it smears an echo rather than
/// putting it in a hall. Amount is the wet/dry mix.
const SPACE_DECAY_S: f32 = 1.0;
const SPACE_SIZE: f32 = 0.4;

fn secs(s: f32) -> usize {
    (s * SR) as usize
}

/// A stereo diffuser at one Amount (0…1).
enum Cand {
    Plain,
    Today(Box<(diffusion::Diffusion, diffusion::Diffusion)>, f32),
    Diff(Box<CrateDiffusion>, f32),
    Space(Box<Spaces>),
}

impl Cand {
    fn new(kind: Kind, amount: f32) -> Self {
        match kind {
            Kind::Plain => Cand::Plain,
            Kind::Today => {
                let (mut l, mut r) = (diffusion::Diffusion::new(0), diffusion::Diffusion::new(1));
                l.prepare(SR);
                r.prepare(SR);
                Cand::Today(Box::new((l, r)), amount)
            }
            Kind::Diff(size) => {
                let mut d = CrateDiffusion::new(SR);
                // Amount → density as today's (a curve on the allpass gain), wet only:
                // with no feedback the chain is an allpass, so it stays flat.
                d.set(&DiffParams { diff: amount.powf(0.6), size, fbk: 0.0, mix: 1.0, ..DiffParams::default() });
                // its SIZE and MIX glide from the defaults (200 ms): settle on silence
                let (mut a, mut b) = (0.0, 0.0);
                for _ in 0..secs(0.5) {
                    d.process(&mut a, &mut b);
                }
                Cand::Diff(Box::new(d), size)
            }
            Kind::Space(mode) => {
                let mut s = Spaces::new(SR);
                s.set(&SpaceSettings {
                    mode,
                    decay_s: SPACE_DECAY_S,
                    size: SPACE_SIZE,
                    predelay_ms: 0.0,
                    mix: amount,
                    ..SpaceSettings::default()
                });
                let (mut a, mut b) = (0.0, 0.0);
                for _ in 0..secs(0.5) {
                    s.process(&mut a, &mut b);
                }
                Cand::Space(Box::new(s))
            }
        }
    }

    /// How long the chain delays the sound, in samples: the loop reads this
    /// much earlier, as the plug-in does for today's Diffuse. SPACES has no
    /// single delay to take back (its tail starts at once), so none.
    fn bulk(&self) -> f32 {
        match self {
            Cand::Today(d, _) => d.0.bulk(),
            Cand::Diff(_, size) => [47.3f32, 71.9, 109.7, 167.2].iter().sum::<f32>() * 0.001 * SR * size,
            _ => 0.0,
        }
    }

    /// In and Out (`blend`: today fades in from dry over Amount's first 20 %).
    #[inline]
    fn process(&mut self, l: f32, r: f32, in_loop: bool) -> (f32, f32) {
        match self {
            Cand::Plain => (l, r),
            Cand::Today(d, a) => {
                if in_loop {
                    (d.0.process(l, *a), d.1.process(r, *a))
                } else {
                    (d.0.blend(l, *a), d.1.blend(r, *a))
                }
            }
            Cand::Diff(d, _) => {
                let (mut l, mut r) = (l, r);
                d.process(&mut l, &mut r);
                (l, r)
            }
            Cand::Space(s) => {
                let (mut l, mut r) = (l, r);
                s.process(&mut l, &mut r);
                (l, r)
            }
        }
    }
}

/// A stereo delay with the diffuser at `route`. In and Out: three taps
/// (250 ms left, 375 ms right, 500 ms centre) and 35 % feedback. FB: one
/// 375 ms loop at 65 %, soft-clipped, reading the diffuser's delay early.
/// The dry signal is never diffused. Returns (L, R).
fn render(x: &[f32], kind: Kind, route: Route, amount: f32, tail: f32) -> (Vec<f32>, Vec<f32>) {
    let mut d = Cand::new(kind, amount);
    let n = x.len() + secs(tail);
    let len = secs(0.6);
    let (mut bl, mut br) = (vec![0.0f32; len], vec![0.0f32; len]);
    let mut w = 0usize;
    let read = |b: &[f32], w: usize, d: usize| b[(w + len - d) % len];
    let cycle = secs(0.375);
    let cycle_read = (cycle as f32 - d.bulk()).round().max(1.0) as usize;
    let taps = [(secs(0.25), 0.9f32, 0.1f32, 0.8f32), (secs(0.375), 0.1, 0.9, 0.7), (secs(0.5), 0.5, 0.5, 0.6)];
    let (mut ol, mut or) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let dry = x.get(i).copied().unwrap_or(0.0);
        let (mut il, mut ir) = (dry, dry);
        if route == Route::In {
            (il, ir) = d.process(il, ir, false);
        }
        let (wl, wr) = if route == Route::Fb {
            let (fl, fr) = d.process(read(&bl, w, cycle_read), read(&br, w, cycle_read), true);
            let (fl, fr) = ((fl * 0.65).tanh(), (fr * 0.65).tanh());
            bl[w] = il + fl;
            br[w] = ir + fr;
            (fl / 0.65, fr / 0.65)
        } else {
            let mut wl = 0.0;
            let mut wr = 0.0;
            for &(t, gl, gr, g) in &taps {
                let v = 0.5 * (read(&bl, w, t) + read(&br, w, t));
                wl += v * g * gl * 1.6;
                wr += v * g * gr * 1.6;
            }
            let fb = 0.35 * 0.5 * (read(&bl, w, secs(0.5)) + read(&br, w, secs(0.5)));
            bl[w] = il + fb;
            br[w] = ir + fb;
            if route == Route::Out {
                (wl, wr) = d.process(wl, wr, false);
            }
            (wl, wr)
        };
        w = (w + 1) % len;
        ol.push(0.6 * dry + 0.6 * wl);
        or.push(0.6 * dry + 0.6 * wr);
    }
    (ol, or)
}

fn rms(l: &[f32], r: &[f32]) -> f32 {
    let e: f64 = l.iter().chain(r).map(|v| (*v as f64).powi(2)).sum();
    (e / (l.len() + r.len()).max(1) as f64).sqrt() as f32
}

/// The diffuser alone on an impulse in both channels (wet only for SPACES).
fn impulse(kind: Kind, amount: f32, secs_len: f32) -> (Vec<f32>, Vec<f32>) {
    let kind_wet = match kind {
        Kind::Space(m) => (Kind::Space(m), 1.0),
        k => (k, amount),
    };
    let mut d = Cand::new(kind_wet.0, kind_wet.1);
    (0..secs(secs_len))
        .map(|i| {
            let v = if i == 0 { 1.0 } else { 0.0 };
            d.process(v, v, true)
        })
        .unzip()
}

fn power_spectrum(x: &[f32], n: usize) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<f64> = (0..n).map(|i| x.get(i).copied().unwrap_or(0.0) as f64).collect();
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    spec.iter().map(|c| c.norm_sqr()).collect()
}

/// Band levels (dB) in fractional-octave bands from 100 Hz to 10 kHz.
fn band_db(p: &[f64], n: usize, per_oct: f32) -> Vec<f64> {
    let hz = SR as f64 / n as f64;
    let mut out = Vec::new();
    let mut f = 100.0f64;
    let step = 2f64.powf(1.0 / per_oct as f64);
    while f * step <= 10_000.0 {
        let (a, b) = ((f / hz) as usize, ((f * step) / hz) as usize);
        let e: f64 = p[a..b.max(a + 1)].iter().sum::<f64>() / (b.max(a + 1) - a) as f64;
        out.push(10.0 * (e + 1e-30).log10());
        f *= step;
    }
    out
}

fn spread(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
}

struct Metrics {
    mono_ripple_db: f64,
    density: f64,
    centroid_ms: f64,
    t90_ms: f64,
    lr_corr: f64,
    loop_colour_db: f64,
    loop_late_ms: f64,
    ns_per_sample: f64,
}

fn measure(kind: Kind, amount: f32) -> Metrics {
    let (hl, hr) = impulse(kind, amount, 1.5);
    let n = 1 << 16;
    let mono: Vec<f32> = hl.iter().zip(&hr).map(|(a, b)| a + b).collect();
    let mono_ripple_db = spread(&band_db(&power_spectrum(&mono, n), n, 6.0));
    // echo density (Abel & Huang) over the first 150 ms after the onset
    let onset = hl.iter().position(|v| v.abs() > 1e-4).unwrap_or(0);
    let (win, hop) = (secs(0.02), secs(0.005));
    let mut ned = Vec::new();
    let mut s = onset;
    while s + win <= (onset + secs(0.15)).min(hl.len()) {
        let w = &hl[s..s + win];
        let sd = (w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / win as f64).sqrt();
        if sd > 0.0 {
            ned.push(w.iter().filter(|v| (v.abs() as f64) > sd).count() as f64 / win as f64 / 0.3173);
        }
        s += hop;
    }
    let density = ned.iter().sum::<f64>() / ned.len().max(1) as f64;
    let e: Vec<f64> = hl.iter().map(|v| (*v as f64).powi(2)).collect();
    let tot: f64 = e.iter().sum();
    let centroid_ms = e.iter().enumerate().map(|(i, v)| i as f64 * v).sum::<f64>() / tot / SR as f64 * 1000.0;
    let mut acc = 0.0;
    let t90 = e.iter().position(|v| {
        acc += v;
        acc >= 0.9 * tot
    });
    let t90_ms = t90.unwrap_or(0) as f64 / SR as f64 * 1000.0;
    let dot: f64 = hl.iter().zip(&hr).map(|(a, b)| *a as f64 * *b as f64).sum();
    let er: f64 = hr.iter().map(|v| (*v as f64).powi(2)).sum();
    let lr_corr = dot / (tot * er).sqrt();
    // how late the loop's energy comes round after the take-back (0 = on time)
    let bulk_ms = Cand::new(kind, amount).bulk() as f64 / SR as f64 * 1000.0;
    let loop_late_ms = centroid_ms - bulk_ms;
    // ringing in the loop: a noise burst through FB, the tail's 1/3-octave
    // spread after five passes, against the plain loop's
    let mut rng = material::Rng::new(5);
    let burst: Vec<f32> = (0..secs(0.05)).map(|_| 0.3 * rng.bi()).collect();
    let tail_spread = |k: Kind| {
        let (l, _) = render(&burst, k, Route::Fb, amount, 3.0);
        let seg = &l[secs(1.9)..secs(1.9) + n.min(l.len() - secs(1.9))];
        spread(&band_db(&power_spectrum(seg, n), n, 3.0))
    };
    let loop_colour_db = tail_spread(kind) - tail_spread(Kind::Plain);
    // CPU: one stereo sample, averaged
    let mut d = Cand::new(kind, amount);
    let mut rng = material::Rng::new(9);
    let count = 2_000_000;
    let t = Instant::now();
    let mut sink = 0.0f32;
    for _ in 0..count {
        let v = rng.bi();
        let (a, b) = d.process(v, -v, true);
        sink += a + b;
    }
    let ns_per_sample = t.elapsed().as_nanos() as f64 / count as f64;
    std::hint::black_box(sink);
    Metrics { mono_ripple_db, density, centroid_ms, t90_ms, lr_corr, loop_colour_db, loop_late_ms, ns_per_sample }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = root.join("renders");
    let reels = out.join("reels");
    std::fs::create_dir_all(&reels).unwrap();

    let mut mats: Vec<(&str, Vec<f32>)> = vec![("drums", material::drums().0), ("pluck", material::pluck(1.0)), ("chord", material::chord(1.0))];
    if let Some(v) = material::voice(&root.join("material")) {
        mats.push(("voice", v));
    }
    for (_, x) in &mut mats {
        material::normalise(x, 0.5);
    }

    // reels: plain delay, then each candidate, each matched in loudness to the
    // plain delay, with a short silence between
    let gap = vec![0.0f32; secs(0.8)];
    for amount in [0.6f32, 1.0] {
        for (mname, x) in &mats {
            for (route, rname) in ROUTES {
                let (pl, pr) = render(x, Kind::Plain, route, amount, 2.5);
                let target = rms(&pl, &pr);
                let (mut rl, mut rr) = (pl, pr);
                rl.extend_from_slice(&gap);
                rr.extend_from_slice(&gap);
                for (kind, _) in KINDS {
                    let (mut l, mut r) = render(x, kind, route, amount, 2.5);
                    let g = target / rms(&l, &r).max(1e-9);
                    l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= g);
                    rl.extend_from_slice(&l);
                    rr.extend_from_slice(&r);
                    rl.extend_from_slice(&gap);
                    rr.extend_from_slice(&gap);
                }
                let pk = rl.iter().chain(&rr).fold(0.0f32, |m, v| m.max(v.abs()));
                let g = if pk > 0.9 { 0.9 / pk } else { 1.0 };
                rl.iter_mut().chain(rr.iter_mut()).for_each(|v| *v *= g);
                let name = format!("diff-{mname}-{rname}-amount{:.0}.wav", amount * 100.0);
                let clipped = wav::write(&reels.join(&name), SR as u32, &[&rl, &rr]);
                println!("{name}{}", if clipped > 0 { format!("  ({clipped} clipped)") } else { String::new() });
            }
        }
    }

    let mut md = String::new();
    writeln!(md, "Each candidate alone on an impulse (SPACES wet only), Afterecho's Size 1.\n").unwrap();
    writeln!(md, "| Candidate | Amount | Echo density (1 = noise) | Energy centre (ms) | 90 % by (ms) | L/R correlation | Mono-sum colour (dB) | Loop late after take-back (ms) | Loop ringing vs plain (dB) | CPU (ns / stereo sample) |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (kind, label) in KINDS {
        for amount in [0.3f32, 0.6, 1.0] {
            let m = measure(kind, amount);
            writeln!(
                md,
                "| {label} | {:.0} % | {:.2} | {:.0} | {:.0} | {:.2} | {:.1} | {:+.0} | {:+.1} | {:.0} |",
                amount * 100.0,
                m.density,
                m.centroid_ms,
                m.t90_ms,
                m.lr_corr,
                m.mono_ripple_db,
                m.loop_late_ms,
                m.loop_colour_db,
                m.ns_per_sample
            )
            .unwrap();
        }
    }
    std::fs::write(out.join("metrics.md"), &md).unwrap();
    println!("\n{md}");
}
