//! Afterecho Diffuse: offline listening test.
//!
//!   cargo run --release            # reels, single renders and metrics into renders/
//!
//! Four candidates (`dsp.rs`) in the three places Diffuse can sit:
//! In (before the delay), FB (inside the feedback loop) and Out (on the
//! wet). The delay around them is a plain stand-in for Afterecho's, enough
//! to hear the difference. Nothing here is used by the plug-in.

mod dsp;
mod material;
mod wav;

use material::SR;
use realfft::RealFftPlanner;
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

fn secs(s: f32) -> usize {
    (s * SR) as usize
}

/// A stereo delay with Diffuse at `route` (none: `kind` = '0'). Out and In:
/// three taps (250 ms left, 375 ms right, 500 ms centre) and 35 %
/// feedback; FB: one 375 ms loop at 65 % feedback, soft-clipped. The dry
/// signal is never diffused. Returns (L, R).
fn render(x: &[f32], kind: char, route: Route, amount: f32, tail: f32) -> (Vec<f32>, Vec<f32>) {
    let mut diff = (kind != '0').then(|| dsp::make(kind, amount, 1.0));
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
            if let Some(d) = diff.as_mut() {
                (il, ir) = d.process(il, ir);
            }
        }
        let (wl, wr) = if route == Route::Fb {
            // the loop: read, colour, write back
            let (mut fl, mut fr) = (read(&bl, w, secs(0.375)), read(&br, w, secs(0.375)));
            if let Some(d) = diff.as_mut() {
                (fl, fr) = d.process(fl, fr);
            }
            let (fl, fr) = ((fl * 0.65).tanh(), (fr * 0.65).tanh());
            bl[w] = il + fl;
            br[w] = ir + fr;
            (fl / 0.65, fr / 0.65)
        } else {
            let mut wl = 0.0;
            let mut wr = 0.0;
            for &(d, gl, gr, g) in &taps {
                let v = 0.5 * (read(&bl, w, d) + read(&br, w, d));
                wl += v * g * gl * 1.6;
                wr += v * g * gr * 1.6;
            }
            let fb = 0.35 * 0.5 * (read(&bl, w, secs(0.5)) + read(&br, w, secs(0.5)));
            bl[w] = il + fb;
            br[w] = ir + fb;
            if route == Route::Out {
                if let Some(d) = diff.as_mut() {
                    (wl, wr) = d.process(wl, wr);
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

/// The diffuser alone on an impulse in both channels: (L, R) impulse responses.
fn impulse(kind: char, amount: f32, secs_len: f32) -> (Vec<f32>, Vec<f32>) {
    let mut d = dsp::make(kind, amount, 1.0);
    (0..secs(secs_len))
        .map(|i| {
            let v = if i == 0 { 1.0 } else { 0.0 };
            d.process(v, v)
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
    ripple_db: f64,
    mono_ripple_db: f64,
    density: f64,
    centroid_ms: f64,
    t90_ms: f64,
    lr_corr: f64,
    loop_colour_db: f64,
    ns_per_sample: f64,
}

fn measure(kind: char, amount: f32) -> Metrics {
    // colour: the response's 1/6-octave spread, 100 Hz – 10 kHz
    let (hl, hr) = impulse(kind, amount, 1.3);
    let n = 1 << 16;
    let ripple_db = spread(&band_db(&power_spectrum(&hl, n), n, 6.0));
    let mono: Vec<f32> = hl.iter().zip(&hr).map(|(a, b)| a + b).collect();
    let mono_ripple_db = spread(&band_db(&power_spectrum(&mono, n), n, 6.0));
    // echo density (Abel & Huang): in 20 ms windows over the first 150 ms
    // after the onset, the share of samples beyond one deviation, against
    // noise's (1 = as dense as noise; low = separate echoes, a flutter)
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
    // smear: where the energy sits in time
    let e: Vec<f64> = hl.iter().map(|v| (*v as f64).powi(2)).collect();
    let tot: f64 = e.iter().sum();
    let centroid_ms = e.iter().enumerate().map(|(i, v)| i as f64 * v).sum::<f64>() / tot / SR as f64 * 1000.0;
    let mut acc = 0.0;
    let t90 = e.iter().position(|v| {
        acc += v;
        acc >= 0.9 * tot
    });
    let t90_ms = t90.unwrap_or(0) as f64 / SR as f64 * 1000.0;
    // width: white noise in both channels comes out this correlated
    let dot: f64 = hl.iter().zip(&hr).map(|(a, b)| *a as f64 * *b as f64).sum();
    let er: f64 = hr.iter().map(|v| (*v as f64).powi(2)).sum();
    let lr_corr = dot / (tot * er).sqrt();
    // ringing in the loop: a noise burst through FB, the tail's 1/3-octave
    // spread after five passes, against the plain loop's
    let mut rng = material::Rng::new(5);
    let burst: Vec<f32> = (0..secs(0.05)).map(|_| 0.3 * rng.bi()).collect();
    let tail_spread = |k: char| {
        let (l, _) = render(&burst, k, Route::Fb, amount, 3.0);
        let seg = &l[secs(1.9)..secs(1.9) + n.min(l.len() - secs(1.9))];
        spread(&band_db(&power_spectrum(seg, n), n, 3.0))
    };
    let loop_colour_db = tail_spread(kind) - tail_spread('0');
    // CPU: one stereo sample, averaged
    let mut d = dsp::make(kind, amount, 1.0);
    let mut rng = material::Rng::new(9);
    let count = 2_000_000;
    let t = Instant::now();
    let mut sink = 0.0f32;
    for _ in 0..count {
        let v = rng.bi();
        let (a, b) = d.process(v, -v);
        sink += a + b;
    }
    let ns_per_sample = t.elapsed().as_nanos() as f64 / count as f64;
    std::hint::black_box(sink);
    Metrics { ripple_db, mono_ripple_db, density, centroid_ms, t90_ms, lr_corr, loop_colour_db, ns_per_sample }
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

    // reels: plain delay, then A, B, C, D, each followed by a short silence
    let gap = vec![0.0f32; secs(0.8)];
    for amount in [0.6f32, 1.0] {
        for (mname, x) in &mats {
            for (route, rname) in ROUTES {
                let (mut rl, mut rr) = (Vec::new(), Vec::new());
                for kind in ['0', 'A', 'B', 'C', 'D'] {
                    let (mut l, mut r) = render(x, kind, route, amount, 2.5);
                    let pk = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
                    if pk > 0.9 {
                        l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= 0.9 / pk);
                    }
                    rl.extend_from_slice(&l);
                    rr.extend_from_slice(&r);
                    rl.extend_from_slice(&gap);
                    rr.extend_from_slice(&gap);
                }
                // one gain for the whole reel, so the candidates keep their levels
                let pk = rl.iter().chain(&rr).fold(0.0f32, |m, v| m.max(v.abs()));
                let g = if pk > 0.9 { 0.9 / pk } else { 1.0 };
                rl.iter_mut().chain(rr.iter_mut()).for_each(|v| *v *= g);
                let name = format!("{mname}-{rname}-amount{:.0}.wav", amount * 100.0);
                let clipped = wav::write(&reels.join(&name), SR as u32, &[&rl, &rr]);
                println!("{name}{}", if clipped > 0 { format!("  ({clipped} clipped)") } else { String::new() });
            }
        }
    }

    let mut md = String::new();
    writeln!(md, "| Candidate | Amount | Colour (dB spread, 1/6 oct) | Mono-sum colour (dB) | Echo density (1 = noise) | Smear centre (ms) | 90 % energy by (ms) | L/R correlation | Loop ringing vs plain (dB) | CPU (ns / stereo sample) |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (kind, label) in dsp::KINDS {
        for amount in [0.3f32, 0.6, 1.0] {
            let m = measure(kind, amount);
            writeln!(
                md,
                "| {kind} · {label} | {:.0} % | {:.1} | {:.1} | {:.2} | {:.1} | {:.0} | {:.2} | {:+.1} | {:.1} |",
                amount * 100.0,
                m.ripple_db,
                m.mono_ripple_db,
                m.density,
                m.centroid_ms,
                m.t90_ms,
                m.lr_corr,
                m.loop_colour_db,
                m.ns_per_sample
            )
            .unwrap();
        }
    }
    std::fs::write(out.join("metrics.md"), &md).unwrap();
    println!("\n{md}");
}
