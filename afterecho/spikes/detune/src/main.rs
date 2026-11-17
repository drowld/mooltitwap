//! Afterecho per-tap detune: offline listening test.
//!
//!   cargo run --release -- render   # WAVs + metrics into renders/
//!   cargo run --release -- bench    # CPU and worst-block cost, 16 taps × 2 ch
//!   cargo run --release -- tune     # parameter sweep used to pick the settings
//!                                   # (`tune 2`: second round, WSOLA ×2 only)
//!   cargo run --release -- wander   # how far B's tap time wanders (and the pull)
//!   cargo run --release -- probe D  # raw f32 dumps of sine/chord ±100 c into D
//!   cargo run --release -- fscheck  # direction check of the loop shifter (D)
//!
//! Nothing here is used by the plug-in.

mod dsp;
mod material;
mod metrics;
mod wav;

use dsp::{cents_to_ratio, DelayLine, DualHann, FreqShifter, PhaseVocoder, Shifter, Wsola, WsolaCfg};
use material::SR;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

const TAU: f32 = std::f32::consts::TAU;
const CENTS: [f32; 6] = [-100.0, -50.0, -25.0, 25.0, 50.0, 100.0];
/// Tap time used for the single-tap renders (long enough for every method).
const TAP_S: f32 = 0.12;

#[derive(Clone, Copy, Debug)]
enum Method {
    Plain,
    DualHann { window_ms: f32, legacy: bool },
    Wsola(WsolaCfg),
    Pv { n: usize },
}

impl Method {
    fn make(&self, ratio: f32) -> Shifter {
        match *self {
            Method::Plain => Shifter::Plain,
            Method::DualHann { window_ms, legacy } => {
                let mut d = DualHann::new(ratio, window_ms, SR);
                d.legacy_read = legacy;
                Shifter::DualHann(d)
            }
            Method::Wsola(cfg) => Shifter::Wsola(Box::new(Wsola::new(ratio, cfg, SR))),
            Method::Pv { n } => Shifter::Pv(Box::new(PhaseVocoder::new(ratio, n))),
        }
    }
    fn needs_lowpass(&self) -> bool {
        matches!(self, Method::Wsola(c) if c.coarse > 1)
    }
    fn label(&self) -> String {
        match self {
            Method::Plain => "plain read".into(),
            Method::DualHann { window_ms, legacy } => {
                format!("dual-head Hann {window_ms} ms{}", if *legacy { "" } else { ", fixed read" })
            }
            Method::Wsola(c) => format!(
                "WSOLA {} ms ×{} search ±{} ms corr {} ms {}",
                c.grain_ms,
                c.overlap,
                c.search_ms,
                c.corr_ms,
                if c.coarse > 1 { format!("coarse/{}", c.coarse) } else { "brute".into() }
            ),
            Method::Pv { n } => format!("phase vocoder N={n}"),
        }
    }
}

/// The three methods that get rendered.
fn methods() -> [(&'static str, Method); 3] {
    [
        ("A-dualhann", Method::DualHann { window_ms: 60.0, legacy: true }),
        ("B-wsola", Method::Wsola(WsolaCfg::DEFAULT)),
        ("C-pvoc", Method::Pv { n: 4096 }),
    ]
}

/// Measured but not rendered: A with the read fixed, to show how much of A's
/// trouble is the algorithm and how much is SOF-60; C at the cheaper,
/// lower-latency frame size.
fn extra_methods() -> [(&'static str, Method); 2] {
    [
        ("(A, fixed read)", Method::DualHann { window_ms: 60.0, legacy: false }),
        ("(C, N=2048)", Method::Pv { n: 2048 }),
    ]
}

fn set_phase(s: &mut Shifter, frac: f32) {
    match s {
        Shifter::DualHann(d) => d.set_phase(frac),
        Shifter::Wsola(w) => w.set_phase(frac),
        Shifter::Pv(p) => p.set_phase(frac),
        Shifter::Plain => {}
    }
}

/// One detuned tap at `TAP_S`, output re-aligned with the dry input.
fn run_tap(x: &[f32], m: Method, cents: f32) -> Vec<f32> {
    let d = (TAP_S * SR) as usize;
    let mut line = DelayLine::new(1 << 18, m.needs_lowpass(), SR);
    let mut sh = m.make(cents_to_ratio(cents));
    let mut out = Vec::with_capacity(x.len());
    for i in 0..x.len() + d {
        line.write(x.get(i).copied().unwrap_or(0.0));
        let y = sh.process(&line, d as f32);
        if i >= d {
            out.push(y);
        }
    }
    if let Shifter::Pv(p) = &sh {
        assert_eq!(p.late_frames, 0, "PV tap shorter than its frame");
    }
    out
}

fn cents_tag(c: f32) -> String {
    format!("{}{:03}", if c < 0.0 { "m" } else { "p" }, c.abs() as i32)
}

struct Mat {
    name: &'static str,
    dry: Vec<f32>,
    ideal: Option<fn(f32) -> Vec<f32>>,
    gain: f32,
    onsets: Vec<usize>,
    steady: Option<(usize, usize)>,
}

fn materials(dir: &Path) -> Vec<Mat> {
    let mut v = Vec::new();
    let mut add = |name, mut dry: Vec<f32>, ideal, onsets, steady| {
        let gain = material::normalise(&mut dry, 0.5);
        v.push(Mat { name, dry, ideal, gain, onsets, steady });
    };
    add("sine", material::sine(1.0), Some(material::sine as fn(f32) -> Vec<f32>), vec![], Some((24_000, 168_000)));
    add("chord", material::chord(1.0), Some(material::chord), vec![], Some((30_000, 204_000)));
    add("pluck", material::pluck(1.0), Some(material::pluck), vec![], None);
    let (drums, onsets) = material::drums();
    add("drums", drums, None, onsets, None);
    match material::voice(&dir.join("material")) {
        Some(voice) => add("voice", voice, None, vec![], None),
        None => eprintln!("voice: macOS `say` unavailable, skipped"),
    }
    v
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ─── render ─────────────────────────────────────────────────────────────────

fn render() {
    let root = root();
    let out_dir = root.join("renders");
    std::fs::create_dir_all(out_dir.join("reels")).unwrap();
    let mats = materials(&root);
    let mut table = String::new();
    let mut csv = String::from("material,cents,method,pitch_err_c,spurs_db,flutter_db,flux_db,lsd_db,env_corr,attack_db,pre_db\n");
    let gap = vec![0.0f32; (0.4 * SR) as usize];
    let mut clipped = 0;

    for m in &mats {
        let dir = out_dir.join(m.name);
        std::fs::create_dir_all(&dir).unwrap();
        clipped += wav::write(&dir.join(format!("{}_dry.wav", m.name)), SR as u32, &[&m.dry]);
        if m.name == "voice" {
            // No ideal and no useful number for speech: listening only.
            for &c in &CENTS {
                let mut reel: Vec<f32> = m.dry.clone();
                for (tag, meth) in methods() {
                    let y = run_tap(&m.dry, meth, c);
                    clipped += wav::write(&dir.join(format!("{}_{}_{}.wav", m.name, cents_tag(c), tag)), SR as u32, &[&y]);
                    reel.extend_from_slice(&gap);
                    reel.extend_from_slice(&y);
                }
                clipped += wav::write(&out_dir.join("reels").join(format!("{}_{}.wav", m.name, cents_tag(c))), SR as u32, &[&reel]);
            }
            eprintln!("voice done");
            continue;
        }
        let _ = writeln!(table, "\n### {}\n", m.name);
        let _ = writeln!(table, "| cents | method | pitch err (c) | spurs (dB) | flutter (dB) | flux (dB) | LSD to ideal (dB) | env corr | attack (dB) | pre-hit (dB) |");
        let _ = writeln!(table, "|---|---|---|---|---|---|---|---|---|---|");
        for &c in &CENTS {
            let ratio = cents_to_ratio(c);
            let ideal = m.ideal.map(|f| {
                let mut y = f(ratio);
                y.iter_mut().for_each(|v| *v *= m.gain);
                y
            });
            let mut reel: Vec<f32> = m.dry.clone();
            let mut rows: Vec<(String, Vec<f32>, bool)> = Vec::new();
            for (tag, meth) in methods() {
                rows.push((tag.to_string(), run_tap(&m.dry, meth, c), true));
            }
            if let Some(y) = &ideal {
                rows.push(("ideal".to_string(), y.clone(), true));
            }
            for (tag, meth) in extra_methods() {
                rows.push((tag.to_string(), run_tap(&m.dry, meth, c), false));
            }
            for (tag, y, rendered) in &rows {
                if *rendered {
                    let path = dir.join(format!("{}_{}_{}.wav", m.name, cents_tag(c), tag));
                    clipped += wav::write(&path, SR as u32, &[y]);
                    reel.extend_from_slice(&gap);
                    reel.extend_from_slice(y);
                }
                let mut cells = vec![String::new(); 8];
                let is_ideal = tag == "ideal";
                if m.name == "sine" {
                    let s = metrics::sine_stats(y, 28_800, 220.0 * ratio);
                    cells[0] = format!("{:+.2}", s.pitch_err_c);
                    cells[1] = format!("{:.0}", s.spur_db);
                } else if let (Some(id), false) = (&ideal, is_ideal) {
                    let a = m.steady.map(|s| s.0).unwrap_or(0);
                    cells[1] = format!("{:.0}", metrics::spur_vs_ideal(y, id, a));
                }
                if let Some((a, b)) = m.steady {
                    cells[2] = format!("{:.0}", metrics::env_stats(y, a, b).flutter_db);
                    if m.name != "sine" {
                        cells[3] = format!("{:.2}", metrics::flux_db(y, a, b));
                    }
                }
                if let (Some(id), false) = (&ideal, is_ideal) {
                    cells[4] = format!("{:.1}", metrics::lsd_db(y, id));
                }
                if !m.onsets.is_empty() {
                    let d = metrics::drum_stats(&m.dry, y, &m.onsets);
                    cells[5] = format!("{:.3}", d.env_corr);
                    cells[6] = format!("{:+.1}", d.attack_db);
                    cells[7] = format!("{:+.1}", d.pre_db);
                }
                let _ = writeln!(table, "| {:+} | {} | {} |", c, tag, cells.join(" | "));
                let _ = writeln!(csv, "{},{},{},{}", m.name, c, tag, cells.join(","));
            }
            clipped += wav::write(
                &out_dir.join("reels").join(format!("{}_{}.wav", m.name, cents_tag(c))),
                SR as u32,
                &[&reel],
            );
            eprintln!("{} {:+} done", m.name, c);
        }
    }

    // Multitap: 6 taps at different detunes over the dry signal, with the
    // plug-in's feedback (re-read from the main line, so detune never stacks).
    let mt_dir = out_dir.join("multitap");
    std::fs::create_dir_all(&mt_dir).unwrap();
    for src in ["pluck", "voice"] {
        let Some(m) = mats.iter().find(|m| m.name == src) else { continue };
        let mut list: Vec<(&str, Method)> = vec![("0-nodetune", Method::Plain)];
        list.extend(methods());
        let mut renders: Vec<(&str, (Vec<f32>, Vec<f32>))> =
            list.iter().map(|(tag, meth)| (*tag, multitap(&m.dry, *meth, *tag == "0-nodetune"))).collect();
        // D: no tap detune, the loop shifted instead (each repeat +4 Hz).
        renders.push(("D-shiftloop-p4hz", multitap_ex(&m.dry, Method::Plain, true, Some(LOOP_SHIFT_HZ))));
        // Loudness-matched (same RMS), then one shared trim so nothing clips:
        // a louder version would otherwise win the comparison.
        let rms = |(l, r): &(Vec<f32>, Vec<f32>)| {
            (l.iter().chain(r).map(|v| v * v).sum::<f32>() / (2 * l.len()) as f32).sqrt()
        };
        let target = rms(&renders[0].1);
        let gains: Vec<f32> = renders.iter().map(|r| target / rms(&r.1)).collect();
        let peak = renders
            .iter()
            .zip(&gains)
            .flat_map(|(r, g)| r.1 .0.iter().chain(&r.1 .1).map(move |v| (v * g).abs()))
            .fold(0.0f32, f32::max);
        let trim = 0.89 / peak;
        for ((tag, (l, r)), g) in renders.into_iter().zip(gains) {
            let g = g * trim;
            let l: Vec<f32> = l.iter().map(|v| v * g).collect();
            let r: Vec<f32> = r.iter().map(|v| v * g).collect();
            clipped += wav::write(&mt_dir.join(format!("multitap_{src}_{tag}.wav")), SR as u32, &[&l, &r]);
        }
        eprintln!("multitap {src} done");
    }

    std::fs::write(out_dir.join("metrics.md"), &table).unwrap();
    std::fs::write(out_dir.join("metrics.csv"), &csv).unwrap();
    println!("{table}");
    println!("clipped samples: {clipped}");
}

/// Taps of the multitap example: (ms, cents, pan −1..1, gain dB).
const TAPS: [(f32, f32, f32, f32); 6] = [
    (110.0, -25.0, -0.7, -4.0),
    (220.0, 25.0, 0.7, -5.0),
    (330.0, -50.0, -0.35, -6.0),
    (440.0, 50.0, 0.35, -7.0),
    (550.0, -100.0, -0.85, -9.0),
    (660.0, 100.0, 0.85, -10.0),
];
const FB: f32 = 0.35;
const LOOP_SHIFT_HZ: f32 = 4.0;
const SPAN_MS: f32 = 770.0;

fn multitap(x: &[f32], m: Method, no_detune: bool) -> (Vec<f32>, Vec<f32>) {
    multitap_ex(x, m, no_detune, None)
}

/// `loop_shift_hz`: frequency-shift the feedback (option D) instead of
/// detuning taps.
fn multitap_ex(x: &[f32], m: Method, no_detune: bool, loop_shift_hz: Option<f32>) -> (Vec<f32>, Vec<f32>) {
    let mut shifter = loop_shift_hz.map(|hz| FreqShifter::new(hz, SR));
    let mut line = DelayLine::new(1 << 19, m.needs_lowpass(), SR);
    let mut taps: Vec<(f32, Shifter, f32, f32)> = TAPS
        .iter()
        .enumerate()
        .map(|(i, &(ms, c, pan, db))| {
            let mut s = m.make(cents_to_ratio(if no_detune { 0.0 } else { c }));
            set_phase(&mut s, i as f32 / TAPS.len() as f32);
            let a = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let g = 10f32.powf(db / 20.0);
            (ms * 0.001 * SR, s, g * a.cos(), g * a.sin())
        })
        .collect();
    let span = SPAN_MS * 0.001 * SR;
    let n = x.len() + (3.5 * SR) as usize;
    let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let inp = x.get(i).copied().unwrap_or(0.0);
        let mut fb = line.read(span);
        if let Some(s) = shifter.as_mut() {
            fb = s.process(fb);
        }
        fb *= FB;
        line.write(inp + fb);
        let (mut yl, mut yr) = (inp, inp);
        for (d, s, gl, gr) in taps.iter_mut() {
            let y = s.process(&line, *d);
            yl += y * *gl;
            yr += y * *gr;
        }
        l.push(yl);
        r.push(yr);
    }
    (l, r)
}

// ─── bench ──────────────────────────────────────────────────────────────────

struct BenchResult {
    ns_per_sample: f64,
    core_pct: f64,
    worst_block_pct: f64,
}

/// 16 taps × 2 channels on 20 s lines, delays spread 60 ms – 1.9 s, detunes
/// spread over ±100 cents, processed in 128-sample blocks like a host would.
fn bench_16x2(m: Method, src: &[f32], stagger: bool) -> BenchResult {
    const BLOCK: usize = 128;
    let mut lines = [DelayLine::new(1 << 20, m.needs_lowpass(), SR), DelayLine::new(1 << 20, m.needs_lowpass(), SR)];
    let mut taps: Vec<Vec<(f32, Shifter)>> = (0..2)
        .map(|_| {
            (0..16)
                .map(|i| {
                    let cents = -100.0 + 200.0 * i as f32 / 15.0;
                    let mut s = m.make(cents_to_ratio(cents));
                    if stagger {
                        set_phase(&mut s, i as f32 / 16.0);
                    }
                    ((0.06 + 1.84 * i as f32 / 15.0) * SR, s)
                })
                .collect()
        })
        .collect();
    let total = (30.0 * SR) as usize;
    // Warm-up: fill the lines so every read hits real data.
    for i in 0..(2.0 * SR) as usize {
        for (ch, line) in lines.iter_mut().enumerate() {
            line.write(src[(i + ch * 999) % src.len()]);
        }
    }
    let mut sink = 0.0f32;
    let mut blocks = Vec::with_capacity(total / BLOCK);
    let t0 = Instant::now();
    let mut i = 0;
    while i < total {
        let tb = Instant::now();
        for j in i..i + BLOCK {
            for ch in 0..2 {
                lines[ch].write(src[(j + ch * 999) % src.len()]);
                let line = &lines[ch];
                for (d, s) in taps[ch].iter_mut() {
                    sink += s.process(line, *d);
                }
            }
        }
        blocks.push(tb.elapsed().as_secs_f64());
        i += BLOCK;
    }
    let el = t0.elapsed().as_secs_f64();
    std::hint::black_box(sink);
    let budget = BLOCK as f64 / SR as f64;
    let worst = blocks.iter().cloned().fold(0.0, f64::max);
    BenchResult {
        ns_per_sample: el * 1e9 / (total as f64 * 32.0),
        core_pct: 100.0 * el / (total as f64 / SR as f64),
        worst_block_pct: 100.0 * worst / budget,
    }
}

/// One tap on one channel: ns per output sample (includes the line write).
fn bench_single(m: Method, src: &[f32], cents: f32) -> f64 {
    let mut line = DelayLine::new(1 << 20, m.needs_lowpass(), SR);
    let mut s = m.make(cents_to_ratio(cents));
    for i in 0..SR as usize {
        line.write(src[i % src.len()]);
    }
    let total = (60.0 * SR) as usize;
    let mut sink = 0.0;
    let t0 = Instant::now();
    for i in 0..total {
        line.write(src[i % src.len()]);
        sink += s.process(&line, 0.5 * SR);
    }
    let el = t0.elapsed().as_secs_f64();
    std::hint::black_box(sink);
    el * 1e9 / total as f64
}

fn bench_src() -> Vec<f32> {
    let mut v = material::chord(1.0);
    v.extend(material::drums().0);
    v.extend(material::pluck(1.0));
    material::normalise(&mut v, 0.5);
    v
}

fn bench() {
    let src = bench_src();
    let mut list: Vec<(&str, Method)> = vec![("plain read (today's tap)", Method::Plain)];
    list.extend(methods());
    list.push(("B, brute-force search", Method::Wsola(WsolaCfg { coarse: 1, ..WsolaCfg::DEFAULT })));
    list.extend(extra_methods());
    println!("Measured on this machine, release build, 48 kHz, 128-sample blocks, best of 3 runs.\n");
    println!("| method | 1 tap, 1 ch (ns/sample) | 16 taps × 2 ch (% of one core) | worst 128-sample block (% of its budget) | same, grains not staggered |");
    println!("|---|---|---|---|---|");
    for (tag, m) in list {
        let single = (0..3).map(|_| bench_single(m, &src, 100.0)).fold(f64::MAX, f64::min);
        let runs: Vec<BenchResult> = (0..3).map(|_| bench_16x2(m, &src, true)).collect();
        let best = runs.iter().min_by(|a, b| a.core_pct.partial_cmp(&b.core_pct).unwrap()).unwrap();
        let worst = runs.iter().map(|r| r.worst_block_pct).fold(f64::MAX, f64::min);
        let unst = (0..3).map(|_| bench_16x2(m, &src, false).worst_block_pct).fold(f64::MAX, f64::min);
        println!(
            "| {} ({}) | {:.1} | {:.2} | {:.1} | {:.1} |",
            tag,
            m.label(),
            single,
            best.core_pct,
            worst,
            unst
        );
        let _ = best.ns_per_sample;
    }
}

// ─── tune ───────────────────────────────────────────────────────────────────

fn tune() {
    let sine = material::sine(1.0);
    let mut chord = material::chord(1.0);
    let cg = material::normalise(&mut chord, 0.5);
    let mut pluck = material::pluck(1.0);
    let pg = material::normalise(&mut pluck, 0.5);
    let (mut drums, onsets) = material::drums();
    material::normalise(&mut drums, 0.5);
    let src = bench_src();
    let ideal = |f: fn(f32) -> Vec<f32>, g: f32, c: f32| {
        let mut y = f(cents_to_ratio(c));
        y.iter_mut().for_each(|v| *v *= g);
        y
    };
    let (ca, cb) = (30_000, 204_000);
    for c in [-100.0, -25.0, 25.0, 100.0] {
        let id = ideal(material::chord, cg, c);
        println!("ideal chord {c:+}: flux {:.2}", metrics::flux_db(&id, ca, cb));
    }

    let round2 = std::env::args().nth(2).as_deref() == Some("2");
    let mut cands: Vec<Method> = Vec::new();
    if !round2 {
        cands.push(Method::DualHann { window_ms: 60.0, legacy: true });
        cands.push(Method::DualHann { window_ms: 60.0, legacy: false });
        for &(g, k) in &[(30.0, 2), (40.0, 2), (60.0, 2), (60.0, 4)] {
            for &(s, c) in &[(10.0, 10.0), (6.0, 6.0), (8.0, 12.0)] {
                for &bias in &[0.0, 0.2] {
                    cands.push(Method::Wsola(WsolaCfg { grain_ms: g, overlap: k, search_ms: s, corr_ms: c, coarse: 4, bias }));
                }
            }
        }
        cands.push(Method::Wsola(WsolaCfg { coarse: 1, ..WsolaCfg::DEFAULT }));
    } else {
        for &g in &[60.0, 80.0, 100.0] {
            for &(s, c) in &[(8.0, 10.0), (10.0, 10.0), (10.0, 15.0), (12.0, 20.0)] {
                cands.push(Method::Wsola(WsolaCfg { grain_ms: g, overlap: 2, search_ms: s, corr_ms: c, coarse: 4, bias: 0.0 }));
            }
        }
    }
    cands.push(Method::Pv { n: 2048 });
    cands.push(Method::Pv { n: 4096 });

    println!("method | ns/s | sine flut/spur/err −100 +100 | chord spur/flux −100 +100 −25 +25 | pluck spur/LSD −100 +100 | drums corr/att/pre −100 +100 | lag ms");
    for m in cands {
        let ns = bench_single(m, &src, 100.0);
        let mut sf = Vec::new();
        for c in [-100.0, 100.0] {
            let y = run_tap(&sine, m, c);
            let e = metrics::env_stats(&y, 24_000, 168_000);
            let st = metrics::sine_stats(&y, 28_800, 220.0 * cents_to_ratio(c));
            sf.push(format!("{:.0}/{:.0}/{:+.2}", e.flutter_db, st.spur_db, st.pitch_err_c));
        }
        let mut ch = Vec::new();
        for c in [-100.0, 100.0, -25.0, 25.0] {
            let y = run_tap(&chord, m, c);
            let id = ideal(material::chord, cg, c);
            ch.push(format!("{:.0}/{:.2}", metrics::spur_vs_ideal(&y, &id, ca), metrics::flux_db(&y, ca, cb)));
        }
        let mut pl = Vec::new();
        let mut lag = 0.0;
        for c in [-100.0, 100.0] {
            let y = run_tap(&pluck, m, c);
            let id = ideal(material::pluck, pg, c);
            pl.push(format!("{:.0}/{:.1}", metrics::spur_vs_ideal(&y, &id, 0), metrics::lsd_db(&y, &id)));
            lag = metrics::best_lag_ms(&id, &y);
        }
        let mut dr = Vec::new();
        for c in [-100.0, 100.0] {
            let y = run_tap(&drums, m, c);
            let d = metrics::drum_stats(&drums, &y, &onsets);
            dr.push(format!("{:.3}/{:+.1}/{:+.1}", d.env_corr, d.attack_db, d.pre_db));
        }
        let label = match m {
            Method::Wsola(c) if c.bias > 0.0 => format!("{} pull {}", m.label(), c.bias),
            _ => m.label(),
        };
        println!("{} | {:.0} | {} | {} | {} | {} | {:+}", label, ns, sf.join(" "), ch.join(" "), pl.join(" "), dr.join(" "), lag);
    }
}

/// How far B's tap time wanders, and what the pull costs on tones.
fn wander() {
    let mut chord = material::chord(1.0);
    let cg = material::normalise(&mut chord, 0.5);
    let (mut drums, onsets) = material::drums();
    material::normalise(&mut drums, 0.5);
    let sine = material::sine(1.0);
    for bias in [0.0f32, 0.05, 0.1, 0.2, 0.4] {
        for search in [10.0f32, 6.0] {
            let cfg = WsolaCfg { bias, search_ms: search, ..WsolaCfg::DEFAULT };
            let mut row = format!("pull {bias:.2} ±{search} ms |");
            for c in [-100.0f32, -25.0, 25.0, 100.0] {
                let r = cents_to_ratio(c);
                let d = (TAP_S * SR) as usize;
                let run = |x: &[f32]| {
                    let mut line = DelayLine::new(1 << 18, true, SR);
                    let mut w = Wsola::new(r, cfg, SR);
                    let mut out = Vec::new();
                    for i in 0..x.len() + d {
                        line.write(x.get(i).copied().unwrap_or(0.0));
                        let y = w.process(&line, d as f32);
                        if i >= d {
                            out.push(y);
                        }
                    }
                    (out, w.wander, w.searches)
                };
                let (yc, wc, _) = run(&chord);
                let (yd, wd, _) = run(&drums);
                let (ys, _, _) = run(&sine);
                let mut id = material::chord(r);
                id.iter_mut().for_each(|v| *v *= cg);
                let ds = metrics::drum_stats(&drums, &yd, &onsets);
                let _ = write!(
                    row,
                    " {c:+}: chord {:.0} dB wander {:+.1}..{:+.1} ms; drums {:.3}/{:+.1}/{:+.1} wander {:+.1}..{:+.1}; sine {:.0} |",
                    metrics::spur_vs_ideal(&yc, &id, 30_000),
                    wc.0 / 48.0,
                    wc.1 / 48.0,
                    ds.env_corr,
                    ds.attack_db,
                    ds.pre_db,
                    wd.0 / 48.0,
                    wd.1 / 48.0,
                    metrics::sine_stats(&ys, 28_800, 220.0 * r).spur_db
                );
            }
            println!("{row}");
        }
    }
}

/// Raw f32 dumps for ad-hoc analysis: probe <dir>.
fn probe() {
    let dir = PathBuf::from(std::env::args().nth(2).expect("probe <dir>"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut chord = material::chord(1.0);
    material::normalise(&mut chord, 0.5);
    let srcs = [("sine", material::sine(1.0)), ("chord", chord)];
    for (name, x) in &srcs {
        for (tag, m) in methods() {
            for c in [-100.0f32, 100.0] {
                let y = run_tap(x, m, c);
                let b: Vec<u8> = y.iter().flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write(dir.join(format!("{name}_{}_{tag}.f32", cents_tag(c))), b).unwrap();
            }
        }
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("render") => render(),
        Some("bench") => bench(),
        Some("tune") => tune(),
        Some("probe") => probe(),
        Some("wander") => wander(),
        Some("fscheck") => {
            // Direction check: a 1 kHz sine through the shifter must land at 1004 Hz.
            let mut fs = FreqShifter::new(LOOP_SHIFT_HZ, SR);
            let y: Vec<f32> = (0..300_000).map(|i| fs.process((TAU * 1000.0 * i as f32 / SR).sin())).collect();
            let st = metrics::sine_stats(&y, 100_000, 1000.0 + LOOP_SHIFT_HZ);
            println!("shift +{LOOP_SHIFT_HZ} Hz: off by {:+.3} cents, spurs {:.0} dB", st.pitch_err_c, st.spur_db);
        }
        _ => {
            render();
            bench();
        }
    }
}
