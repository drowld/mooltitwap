//! Afterecho Saturate and loop limit: offline listening test (crate adoption pass 3).
//!
//!   cargo run --release            # reels and metrics into renders/
//!
//! Part 1, Saturate: today's block (a dry/wet blend of tanh(drive·x)) against
//! the five curves of `softsystems-fx`'s SATURATE (TUBE, TAPE, GRIT, AMP,
//! TRANSFO; the whole chain, 8× oversampled, Mix = Amount), in the three
//! places the block sits: In, FB (inside the loop) and Out.
//!
//! Part 2, the loop limit: the loop's safety curve today, 2·tanh(x/2), against
//! `loop_limit::polsat531`, with the loop running hot and self-oscillating.
//! PolSat531 is never a Saturate choice (owner): it's only a candidate for
//! the loop's own limit.
//!
//! The delay around them is a plain stand-in for Afterecho's. Nothing here is
//! used by the plug-in.

mod material;
mod wav;

use material::SR;
use realfft::RealFftPlanner;
use softsystems_fx::loop_limit::polsat531;
use softsystems_fx::saturate::curve::Engine;
use softsystems_fx::saturate::{map, Saturate, Settings};
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

/// A Saturate candidate: today's tanh, or one of the crate's engines.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Tanh,
    Crate(Engine),
}

const KINDS: [(Kind, &str); 6] = [
    (Kind::Tanh, "today (tanh)"),
    (Kind::Crate(Engine::Tube), "TUBE"),
    (Kind::Crate(Engine::Tape), "TAPE"),
    (Kind::Crate(Engine::Grit), "GRIT"),
    (Kind::Crate(Engine::Amp), "AMP"),
    (Kind::Crate(Engine::Transformer), "TRANSFO"),
];

fn secs(s: f32) -> usize {
    (s * SR) as usize
}

/// One Saturate block, stereo, at `amount` (0…1, Afterecho's Amount) and
/// `drive_db` (Afterecho's Drive, 0…24 dB).
enum Stage {
    Tanh { g: f32, a: f32 },
    Crate(Box<Saturate<2>>),
}

impl Stage {
    fn new(kind: Kind, amount: f32, drive_db: f32) -> Stage {
        match kind {
            Kind::Tanh => Stage::Tanh { g: 10f32.powf(drive_db / 20.0), a: amount },
            Kind::Crate(engine) => {
                let mut sat = Box::new(Saturate::<2>::new(SR));
                // the crate's defaults (its make-up rule, no emphasis, tone
                // flat, dynamics off, level 0 dB), with Afterecho's two knobs
                let s = Settings { engine, drive_db, mix: amount, ..map::settings(&map::DEFAULT_CELLS) };
                // the first set after `new` snaps (no glide)
                sat.set(&s);
                Stage::Crate(sat)
            }
        }
    }

    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        match self {
            Stage::Tanh { g, a } => {
                let f = |x: f32| x * (1.0 - *a) + (x * *g).tanh() * *a;
                (f(l), f(r))
            }
            Stage::Crate(sat) => {
                let mut fr = [l, r];
                sat.process(&mut fr);
                (fr[0], fr[1])
            }
        }
    }
}

/// Today's loop limit: headroomed tanh, ceiling 2.
fn limit_today(x: f32) -> f32 {
    2.0 * (0.5 * x).tanh()
}

/// A stereo delay with Saturate at `route` (`None`: no Saturate). In and Out:
/// three taps (250 ms left, 375 ms right, 500 ms centre), 35 % feedback. FB:
/// one 375 ms loop at 65 %, Saturate then today's limit inside it, as in
/// Afterecho's loop. The dry signal is never saturated. Returns (L, R).
fn render(x: &[f32], kind: Option<Kind>, route: Route, amount: f32, drive_db: f32, tail: f32) -> (Vec<f32>, Vec<f32>) {
    let mut st = kind.map(|k| Stage::new(k, amount, drive_db));
    let n = x.len() + secs(tail);
    let len = secs(0.6);
    let (mut bl, mut br) = (vec![0.0f32; len], vec![0.0f32; len]);
    let mut w = 0usize;
    let read = |b: &[f32], w: usize, d: usize| b[(w + len - d) % len];
    let taps = [(secs(0.25), 0.9f32, 0.1f32, 0.8f32), (secs(0.375), 0.1, 0.9, 0.7), (secs(0.5), 0.5, 0.5, 0.6)];
    let (mut ol, mut or) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let dry = x.get(i).copied().unwrap_or(0.0);
        let (mut il, mut ir) = (dry, dry);
        if route == Route::In {
            if let Some(s) = st.as_mut() {
                (il, ir) = s.process(il, ir);
            }
        }
        let (wl, wr) = if route == Route::Fb {
            let (rl, rr) = (read(&bl, w, secs(0.375)), read(&br, w, secs(0.375)));
            let (mut fl, mut fr) = (rl * 0.65, rr * 0.65);
            if let Some(s) = st.as_mut() {
                (fl, fr) = s.process(fl, fr);
            }
            bl[w] = il + limit_today(fl);
            br[w] = ir + limit_today(fr);
            (rl, rr)
        } else {
            let (mut wl, mut wr) = (0.0, 0.0);
            for &(d, gl, gr, g) in &taps {
                let v = 0.5 * (read(&bl, w, d) + read(&br, w, d));
                wl += v * g * gl * 1.6;
                wr += v * g * gr * 1.6;
            }
            let fb = 0.35 * 0.5 * (read(&bl, w, secs(0.5)) + read(&br, w, secs(0.5)));
            bl[w] = il + fb;
            br[w] = ir + fb;
            if route == Route::Out {
                if let Some(s) = st.as_mut() {
                    (wl, wr) = s.process(wl, wr);
                }
            }
            (wl, wr)
        };
        w = (w + 1) % len;
        ol.push(0.6 * dry + 0.6 * wl);
        or.push(0.6 * dry + 0.6 * wr);
    }
    (ol, or)
}

/// One 375 ms loop at `fb` (≥ 1 self-oscillates), bounded by `limit`.
fn render_loop(x: &[f32], fb: f32, limit: fn(f32) -> f32, tail: f32) -> (Vec<f32>, Vec<f32>) {
    let n = x.len() + secs(tail);
    let d = secs(0.375);
    let mut b = vec![[0.0f32; 2]; d];
    let mut w = 0usize;
    let (mut ol, mut or) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let dry = x.get(i).copied().unwrap_or(0.0);
        let [rl, rr] = b[w];
        // a little crossfeed so the sides differ
        let (fl, fr) = (limit(fb * (0.9 * rl + 0.1 * rr)), limit(fb * (0.9 * rr + 0.1 * rl)));
        b[w] = [dry + fl, 0.8 * dry + fr];
        w = (w + 1) % d;
        ol.push(0.6 * dry + 0.5 * rl);
        or.push(0.6 * dry + 0.5 * rr);
    }
    (ol, or)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn power_spectrum(x: &[f32], n: usize) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    // Hann window
    let mut buf: Vec<f64> = (0..n)
        .map(|i| {
            let wv = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            x.get(i).copied().unwrap_or(0.0) as f64 * wv
        })
        .collect();
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    spec.iter().map(|c| c.norm_sqr()).collect()
}

/// Energy within ±3 bins of `hz`.
fn bin_energy(p: &[f64], n: usize, hz: f64) -> f64 {
    let k = (hz * n as f64 / SR as f64).round() as isize;
    (k - 3..=k + 3).filter(|&i| i > 0 && (i as usize) < p.len()).map(|i| p[i as usize]).sum()
}

struct Curve {
    gain_db: f64,
    h2_db: f64,
    h3_db: f64,
    thd_pct: f64,
    alias_db: f64,
    ns: f64,
}

/// A candidate alone at Amount 100 %: a 1 kHz sine at −6 dBFS (gain, 2nd and
/// 3rd harmonic, THD), then a 7 kHz sine (what folds back below 20 kHz that
/// isn't a harmonic: aliasing), then its CPU.
fn measure_curve(kind: Kind, drive_db: f32) -> Curve {
    let n = 1 << 15;
    let run = |hz: f32| -> Vec<f32> {
        let mut st = Stage::new(kind, 1.0, drive_db);
        let skip = secs(0.1);
        (0..skip + n)
            .map(|i| {
                let v = 0.5 * (std::f32::consts::TAU * hz * i as f32 / SR).sin();
                st.process(v, v).0
            })
            .skip(skip)
            .collect()
    };
    let y = run(1000.0);
    let p = power_spectrum(&y, n);
    let h = |k: f64| bin_energy(&p, n, 1000.0 * k);
    let f1 = h(1.0);
    let harm: f64 = (2..=20).map(|k| h(k as f64)).sum();
    let gain_db = 20.0 * (rms(&y) as f64 / (0.5 / 2f64.sqrt())).log10();
    // aliasing: 7 kHz, everything below 20 kHz that isn't 7 or 14 kHz
    let y = run(7000.0);
    let p = power_spectrum(&y, n);
    let f7 = bin_energy(&p, n, 7000.0);
    let hz_bin = SR as f64 / n as f64;
    let mut alias = 0.0;
    for (i, v) in p.iter().enumerate() {
        let f = i as f64 * hz_bin;
        if f > 50.0 && f < 20_000.0 && (f - 7000.0).abs() > 4.0 * hz_bin && (f - 14_000.0).abs() > 4.0 * hz_bin {
            alias += v;
        }
    }
    // CPU: one stereo sample, averaged
    let mut st = Stage::new(kind, 0.7, drive_db);
    let mut rng = material::Rng::new(9);
    let count = 1_000_000;
    let t = Instant::now();
    let mut sink = 0.0f32;
    for _ in 0..count {
        let v = 0.3 * rng.bi();
        let (a, b) = st.process(v, -v);
        sink += a + b;
    }
    let ns = t.elapsed().as_nanos() as f64 / count as f64;
    std::hint::black_box(sink);
    Curve {
        gain_db,
        h2_db: 10.0 * (h(2.0) / f1 + 1e-30).log10(),
        h3_db: 10.0 * (h(3.0) / f1 + 1e-30).log10(),
        thd_pct: 100.0 * (harm / f1).sqrt(),
        alias_db: 10.0 * (alias / f7 + 1e-30).log10(),
        ns,
    }
}

/// The loop at `fb` on a noise burst: the level it settles at (peak, RMS)
/// and how bright the settled tail is (spectral centroid), after 4 s.
fn measure_loop(fb: f32, limit: fn(f32) -> f32) -> (f32, f32, f64) {
    let mut rng = material::Rng::new(5);
    let burst: Vec<f32> = (0..secs(0.05)).map(|_| 0.3 * rng.bi()).collect();
    let (l, _) = render_loop(&burst, fb, limit, 6.0);
    let seg = &l[secs(4.0)..secs(4.0) + secs(1.0)];
    let pk = seg.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let n = 1 << 15;
    let p = power_spectrum(seg, n);
    let hz = SR as f64 / n as f64;
    let tot: f64 = p.iter().sum();
    let centroid = p.iter().enumerate().map(|(i, v)| i as f64 * hz * v).sum::<f64>() / tot.max(1e-30);
    // the wet's 0.5 gain taken back out, so these are the loop's own levels
    (pk / 0.5, rms(seg) / 0.5, centroid)
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
    let gap = vec![0.0f32; secs(0.8)];

    // Part 1: plain delay, today's tanh, then the crate's five curves. Each
    // candidate is matched in loudness (RMS) to the plain delay, so what
    // differs is the character, not the level; metrics.md has the raw gains.
    let amount = 0.7; // the factory presets' Amount
    for drive_db in [6.0f32, 18.0] {
        for (mname, x) in &mats {
            for (route, rname) in ROUTES {
                let (mut rl, mut rr) = (Vec::new(), Vec::new());
                let mut reference = 0.0;
                let cands: Vec<Option<Kind>> = std::iter::once(None).chain(KINDS.iter().map(|&(k, _)| Some(k))).collect();
                for kind in cands {
                    let (mut l, mut r) = render(x, kind, route, amount, drive_db, 2.5);
                    let level = rms(&l).hypot(rms(&r));
                    if kind.is_none() {
                        reference = level;
                    } else if level > 0.0 {
                        let g = reference / level;
                        l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= g);
                    }
                    rl.extend_from_slice(&l);
                    rr.extend_from_slice(&r);
                    rl.extend_from_slice(&gap);
                    rr.extend_from_slice(&gap);
                }
                let pk = rl.iter().chain(&rr).fold(0.0f32, |m, v| m.max(v.abs()));
                let g = if pk > 0.9 { 0.9 / pk } else { 1.0 };
                rl.iter_mut().chain(rr.iter_mut()).for_each(|v| *v *= g);
                let name = format!("sat-{mname}-{rname}-drive{drive_db:.0}.wav");
                let clipped = wav::write(&reels.join(&name), SR as u32, &[&rl, &rr]);
                println!("{name}{}", if clipped > 0 { format!("  ({clipped} clipped)") } else { String::new() });
            }
        }
    }

    // Part 2: the loop limit, today's then PolSat531, at 95 % (hot) and
    // 110 % (self-oscillating). Not loudness-matched: the level a loop
    // settles at is part of what's being judged.
    let limits: [(fn(f32) -> f32, &str); 2] = [(limit_today, "today 2·tanh(x/2)"), (polsat531, "PolSat531")];
    for fb in [0.95f32, 1.1] {
        for (mname, x) in mats.iter().filter(|(m, _)| *m != "chord") {
            let (mut rl, mut rr) = (Vec::new(), Vec::new());
            for (limit, _) in limits {
                let (l, r) = render_loop(x, fb, limit, 6.0);
                rl.extend_from_slice(&l);
                rr.extend_from_slice(&r);
                rl.extend_from_slice(&gap);
                rr.extend_from_slice(&gap);
            }
            let pk = rl.iter().chain(&rr).fold(0.0f32, |m, v| m.max(v.abs()));
            let g = if pk > 0.9 { 0.9 / pk } else { 1.0 };
            rl.iter_mut().chain(rr.iter_mut()).for_each(|v| *v *= g);
            let name = format!("loop-{mname}-fb{:.0}.wav", fb * 100.0);
            wav::write(&reels.join(&name), SR as u32, &[&rl, &rr]);
            println!("{name}");
        }
    }

    // metrics
    let mut md = String::new();
    writeln!(md, "## Saturate, alone at Amount 100 %\n").unwrap();
    writeln!(md, "| Curve | Drive | Gain at −6 dBFS (dB) | 2nd harmonic (dB) | 3rd harmonic (dB) | THD (%) | Aliasing, 7 kHz in (dB) | CPU (ns / stereo sample) |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|").unwrap();
    for (kind, label) in KINDS {
        for drive in [6.0f32, 18.0] {
            let c = measure_curve(kind, drive);
            writeln!(md, "| {label} | {drive:.0} dB | {:+.1} | {:.0} | {:.0} | {:.1} | {:.0} | {:.0} |", c.gain_db, c.h2_db, c.h3_db, c.thd_pct, c.alias_db, c.ns).unwrap();
        }
    }
    writeln!(md, "\nLatency: today 0 samples; the crate's chain 20 samples (its 8× filters).\n").unwrap();
    writeln!(md, "## The loop limit\n").unwrap();
    writeln!(md, "| Limit | Feedback | Settled peak | Settled RMS | Tail brightness (centroid, Hz) |").unwrap();
    writeln!(md, "|---|---|---|---|---|").unwrap();
    for (limit, label) in limits {
        for fb in [0.95f32, 1.1] {
            let (pk, r, c) = measure_loop(fb, limit);
            writeln!(md, "| {label} | {:.0} % | {pk:.3} | {r:.3} | {c:.0} |", fb * 100.0).unwrap();
        }
    }
    std::fs::write(out.join("metrics.md"), &md).unwrap();
    print!("{md}");
}
