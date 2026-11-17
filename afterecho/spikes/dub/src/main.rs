//! Afterecho Dub: offline listening test for the Colour tab's Dub
//! (softsystems-fx `dub_loop`, Cumulus's Dub Delay feedback path).
//!
//!   cargo run --release            # reels and metrics into renders/
//!
//! One feedback loop, as Afterecho's: the loop reads the line, the pass goes
//! through the candidate, × Feedback, Afterecho's loop limit 2·tanh(x/2), and
//! back in. The candidates: the plain loop, the nearest Afterecho can do today
//! (Loop Tone dark), and Dub at three settings. Nothing here
//! is used by the plug-in.

mod material;
mod wav;

use material::SR;
use realfft::RealFftPlanner;
use softsystems_fx::dub_loop::{DubLoop, Params};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

fn secs(s: f32) -> usize {
    (s * SR) as usize
}

/// Afterecho's loop limit (`LoopColour::apply`).
fn limit(x: f32) -> f32 {
    2.0 * (x * 0.5).tanh()
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    /// Loop Tone at −0.5 (a one-pole low-pass at ~3.5 kHz on each pass): the
    /// nearest Afterecho has today. (Saturate on FB adds gain by design, so at
    /// its default Drive a 65 % loop runs away; it's measured apart.)
    Today,
    Dub(Params),
}

fn dub(intensity: f32, hp: f32, lp: f32, ap_hz: f32, stages: usize) -> Kind {
    Kind::Dub(Params { hp_hz: hp, lp_hz: lp, intensity, ap_hz, ap_stages: stages, color: 0.4 })
}

fn kinds() -> Vec<(Kind, &'static str)> {
    vec![
        (Kind::Today, "today (Loop Tone −0.5)"),
        (dub(0.3, 100.0, 4000.0, 700.0, 1), "Dub light (I 0.3, 100 Hz–4 kHz, 1 swirl)"),
        (dub(0.6, 150.0, 2500.0, 600.0, 2), "Dub medium (I 0.6, 150 Hz–2.5 kHz, 2 swirls)"),
        (dub(1.0, 200.0, 1500.0, 500.0, 3), "Dub heavy (I 1.0, 200 Hz–1.5 kHz, 3 swirls)"),
    ]
}

/// One channel's pass processor.
enum Pass {
    Plain,
    Today { lp: f32, g: f32 },
    Dub(Box<DubLoop>),
}

impl Pass {
    fn new(kind: Kind) -> Self {
        match kind {
            Kind::Plain => Pass::Plain,
            Kind::Today => {
                let hz = 20_000.0 * 0.03f32.powf(0.5);
                let g = (std::f32::consts::PI * hz / SR).tan();
                Pass::Today { lp: 0.0, g: g / (1.0 + g) }
            }
            Kind::Dub(p) => {
                let mut d = DubLoop::new(SR);
                d.set(&p);
                // its controls glide 5 ms from the defaults: settle on silence
                for _ in 0..secs(0.05) {
                    d.process(0.0);
                }
                Pass::Dub(Box::new(d))
            }
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        match self {
            Pass::Plain => x,
            Pass::Today { lp, g } => {
                // the TPT one-pole of `filter::LoopTone`, then Saturate's blend
                let v = (x - *lp) * *g;
                let y = v + *lp;
                *lp = y + v;
                y
            }
            Pass::Dub(d) => d.process(x),
        }
    }
}

/// A 375 ms stereo loop at `fb`, the dry signal and the wet together (half
/// each). Returns (L, R).
fn render(x: &[f32], kind: Kind, fb: f32, tail: f32) -> (Vec<f32>, Vec<f32>) {
    let n = x.len() + secs(tail);
    let len = secs(0.6);
    let cycle = secs(0.375);
    let (mut bl, mut br) = (vec![0.0f32; len], vec![0.0f32; len]);
    let (mut pl, mut pr) = (Pass::new(kind), Pass::new(kind));
    let mut w = 0usize;
    let (mut ol, mut or) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let dry = x.get(i).copied().unwrap_or(0.0);
        let r = (w + len - cycle) % len;
        let (tl, tr) = (bl[r], br[r]);
        let (fl, fr) = (limit(fb * pl.process(tl)), limit(fb * pr.process(tr)));
        bl[w] = dry + fl;
        br[w] = dry * 0.98 + fr;
        w = (w + 1) % len;
        ol.push(0.5 * dry + 0.5 * tl);
        or.push(0.5 * dry + 0.5 * tr);
    }
    (ol, or)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32
}

/// Spectral centroid (Hz) and level (dB) of `x`.
fn centroid_db(x: &[f32]) -> (f64, f64) {
    let n = x.len().next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<f64> = (0..n).map(|i| x.get(i).copied().unwrap_or(0.0) as f64).collect();
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    let p: Vec<f64> = spec.iter().map(|c| c.norm_sqr()).collect();
    let tot: f64 = p.iter().sum();
    let hz = SR as f64 / n as f64;
    let c = p.iter().enumerate().map(|(k, v)| k as f64 * hz * v).sum::<f64>() / tot.max(1e-30);
    let e = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    (c, 10.0 * (e + 1e-30).log10())
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

    // reels: plain, today, Dub light / medium / heavy, each matched in
    // loudness to the plain loop, 0.8 s of silence between
    let gap = vec![0.0f32; secs(0.8)];
    for fb in [0.65f32, 0.9] {
        for (mname, x) in &mats {
            let (pl, pr) = render(x, Kind::Plain, fb, 4.0);
            let target = rms(&pl).max(rms(&pr));
            let (mut rl, mut rr) = (pl, pr);
            rl.extend_from_slice(&gap);
            rr.extend_from_slice(&gap);
            for (kind, _) in kinds() {
                let (mut l, mut r) = render(x, kind, fb, 4.0);
                let g = target / rms(&l).max(rms(&r)).max(1e-9);
                l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= g);
                rl.extend_from_slice(&l);
                rr.extend_from_slice(&r);
                rl.extend_from_slice(&gap);
                rr.extend_from_slice(&gap);
            }
            let pk = rl.iter().chain(&rr).fold(0.0f32, |m, v| m.max(v.abs()));
            let g = if pk > 0.9 { 0.9 / pk } else { 1.0 };
            rl.iter_mut().chain(rr.iter_mut()).for_each(|v| *v *= g);
            let name = format!("dub-{mname}-fb{:.0}.wav", fb * 100.0);
            let clipped = wav::write(&reels.join(&name), SR as u32, &[&rl, &rr]);
            println!("{name}{}", if clipped > 0 { format!("  ({clipped} clipped)") } else { String::new() });
        }
    }

    // metrics: a 30 ms noise burst into the loop at 65 %; each echo's
    // brightness and level, pass by pass
    let mut rng = material::Rng::new(3);
    let burst: Vec<f32> = (0..secs(0.03)).map(|_| 0.4 * rng.bi()).collect();
    let mut md = String::new();
    writeln!(md, "A 30 ms noise burst into a 375 ms loop at Feedback 65 %: each echo's spectral centroid (Hz) and level against the first echo (dB).\n").unwrap();
    writeln!(md, "| Candidate | Echo 1 | Echo 2 | Echo 3 | Echo 4 | Echo 6 | Level at echo 6 | Settled peak at FB 90 % / 110 % | CPU (ns / stereo sample) |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|").unwrap();
    let mut all = vec![(Kind::Plain, "plain loop")];
    all.extend(kinds());
    for (kind, label) in all {
        let (l, _) = render(&burst, kind, 0.65, 3.0);
        let echo = |k: usize| {
            // the wet half of the output around echo k (k cycles after the burst)
            let a = secs(0.375) * k;
            centroid_db(&l[a..a + secs(0.1)])
        };
        let e: Vec<(f64, f64)> = [1, 2, 3, 4, 6].iter().map(|&k| echo(k)).collect();
        let peak = |fb: f32| {
            let (l, r) = render(&burst, kind, fb, 6.0);
            let tail = &l[l.len() - secs(1.0)..];
            let tr = &r[r.len() - secs(1.0)..];
            tail.iter().chain(tr).fold(0.0f32, |m, v| m.max(v.abs())) * 2.0
        };
        let (p90, p110) = (peak(0.9), peak(1.1));
        let mut pl = Pass::new(kind);
        let mut pr = Pass::new(kind);
        let mut rng = material::Rng::new(9);
        let count = 2_000_000;
        let t = Instant::now();
        let mut sink = 0.0f32;
        for _ in 0..count {
            let v = rng.bi();
            sink += pl.process(v) + pr.process(-v);
        }
        let ns = t.elapsed().as_nanos() as f64 / count as f64;
        std::hint::black_box(sink);
        writeln!(
            md,
            "| {label} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:+.1} | {:.2} / {:.2} | {:.0} |",
            e[0].0, e[1].0, e[2].0, e[3].0, e[4].0, e[4].1 - e[0].1, p90, p110, ns
        )
        .unwrap();
    }
    std::fs::write(out.join("metrics.md"), &md).unwrap();
    println!("\n{md}");
}
