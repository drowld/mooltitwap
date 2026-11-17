//! Motion on the shared crate `softsystems-mod` (beta pass 8, owner 2026-11-13/14): six fixed
//! slots, ten targets, the routes in a `Patch`. Replaces v2.42's own matrix (`matrix.rs`) and
//! v2.39's shapes (`motion.rs`).
//!
//! * **Slots** (`LINEUP`): LFO 1–3, Follow, Step 1–2. Each slot's kind can be changed from its
//!   editor's name (`ALLOWED`: LFO, Steps, Follow). No MIDI, so no envelopes, note triggers or
//!   Expressions.
//! * **The taps are read-heads** (the crate's `set_voice_offset`): every slot is per voice, one
//!   voice per tap, and Scatter spreads the taps round the slot's cycle (0: all together, 1: a
//!   whole cycle across the live taps), so the movement travels through them. The four tap
//!   targets (Pan, Gain, Cutoff, Reso) read each tap's own voice; the six whole-echo targets
//!   read the lead voice, tap 1, which sits at the cycle's start.
//! * **Amounts by the crate's rule:** a route moves its knob by a share of its whole travel
//!   (+25 % is a quarter of the way), clamped once at the knob's ends. A tap's knob is its own
//!   value; Colour has no knob of its own, so its base is the middle (×1) and a route scales
//!   every colour's amount from ×0 to ×2.
//! * **Host params:** the slots' continuous settings are the crate's `ModParams` ("Mod 1
//!   Rate"…), plus Depth, Time and Scatter. Everything else (kinds, shapes, steps, triggers,
//!   units, the routes) is the `Patch`, persisted with the session and saved in a preset as its
//!   "mod" blob.
//! * **Threads:** the editor edits the patch under its lock and bumps `Shared::version`; the
//!   audio thread takes the patch at the next block it can lock it (`try_lock`, never waits).

use crate::NUM_TAPS;
use nice_plug::prelude::*;
use softsystems_mod::globals::{depth_text, time_from_knob, time_text, DEPTH_DEFAULT};
use softsystems_mod::{
    Engine, Law, Matrix, MatrixMonitor, Modulator, Monitor, Patch, Shape, SlotKind, Slots, Target, Targets, TimeSig,
    Transport,
};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

pub const N_SLOTS: usize = 6;
/// The six slots, in order: v2.43's sources.
pub const LINEUP: [SlotKind; N_SLOTS] =
    [SlotKind::Lfo, SlotKind::Lfo, SlotKind::Lfo, SlotKind::Follow, SlotKind::Steps, SlotKind::Steps];
/// What a slot can become from its kind menu.
pub const ALLOWED: [SlotKind; 3] = [SlotKind::Lfo, SlotKind::Steps, SlotKind::Follow];
/// The follower's input: the dry input as it arrives (both sides together).
pub const FOLLOW_INPUTS: [&str; 1] = ["Input"];
/// The three LFOs' shapes in a new patch (v2.42's defaults).
const LFO_SHAPES: [Shape; 3] = [Shape::Sine, Shape::Triangle, Shape::Glide];

// Targets, in grid order. Ids are stable: saved routes key on them.
pub const T_PAN: usize = 0;
pub const T_GAIN: usize = 1;
pub const T_CUTOFF: usize = 2;
pub const T_RESO: usize = 3;
/// The first four move each tap; the rest the whole echo.
pub const TAP_TARGETS: usize = 4;
pub const T_COLOUR: usize = 4;
pub const T_FDBK: usize = 5;
pub const T_CROSS: usize = 6;
pub const T_TONE: usize = 7;
pub const T_MIX: usize = 8;
pub const T_DUCK: usize = 9;
pub const N_TGT: usize = 10;
pub const TGT_IDS: [&str; N_TGT] = ["pan", "gain", "cutoff", "reso", "colour", "fdbk", "cross", "tone", "mix", "duck"];
pub const TGT_NAMES: [&str; N_TGT] = ["Pan", "Gain", "Cutoff", "Reso", "Colour", "Feedback", "X-feed", "Tone", "Mix", "Duck"];

/// The plug-in's own routes at most: every slot into every target.
pub const CAPACITY: usize = N_SLOTS * N_TGT;

/// Routes a patch holds at most: the matrix's, and routes into the slots' own settings
/// (beta pass 11, "mod.2.rate"): every slot into each other slot's settings is far under it.
pub const ROUTE_CAP: usize = CAPACITY + 96;

pub fn slots() -> Slots {
    // every kind per voice: the taps are its read-heads
    let per_voice = |k: SlotKind| {
        let mut m = k.modulator();
        m.common_mut().per_voice = true;
        m
    };
    let mut lfo = per_voice(SlotKind::Lfo);
    if let Modulator::Curve(c) = &mut lfo {
        c.shape = Shape::Sine;
    }
    // Steps glide a little by default (v2.43's Glide 10 %: a jump slides over ~10 ms), so a
    // jump on a filter or a gain doesn't click; Slew off leaves the crate's 0.67 ms edge
    let mut steps = per_voice(SlotKind::Steps);
    steps.common_mut().slew = true;
    steps.common_mut().slew_ms = 10.0;
    Slots::new(&LINEUP, &ALLOWED)
        .with_default(SlotKind::Lfo, lfo)
        .with_default(SlotKind::Steps, steps)
        .with_default(SlotKind::Follow, per_voice(SlotKind::Follow))
}

/// The ten targets: the taps' four, then the whole echo's six. The laws match the params'
/// (nice-plug's skew factor 2^−2 is a power of 4 here), so the readouts say what is heard.
pub fn targets() -> Targets {
    let laws = [
        Law::linear(-100.0, 100.0, ""),
        Law::linear(-60.0, 6.0, "dB"),
        Law::skew(20.0, 20_000.0, 4.0, "Hz"),
        Law::skew(0.1, 12.0, 2.0, ""),
        Law::default(),
        Law::linear(0.0, 105.0, "%"),
        Law::linear(0.0, 100.0, "%"),
        Law::default(),
        Law::linear(0.0, 100.0, "%"),
        Law::linear(0.0, 24.0, "dB"),
    ];
    let v = TGT_IDS
        .iter()
        .zip(TGT_NAMES)
        .zip(laws)
        .enumerate()
        .map(|(i, ((&id, name), law))| {
            let t = Target::new(id, name).law(law);
            if i < TAP_TARGETS { t.group("Taps").per_voice() } else { t.group("Echo") }
        })
        .collect();
    Targets::new(v)
}

/// A fresh patch: the line-up at its defaults (LFO 1–3 a sine, a triangle and a glide), no
/// routes.
pub fn default_patch() -> Patch {
    let mut p = slots().patch();
    for (m, shape) in p.modulators.iter_mut().zip(LFO_SHAPES) {
        if let Modulator::Curve(c) = m {
            c.shape = shape;
        }
    }
    p
}

/// Colour's knob position (×1) and what a modulated position means: every colour's amount
/// times 0..2, so one that's off stays off.
pub const COLOUR_BASE: f32 = 0.5;
pub fn colour(amount: f32, n: f32) -> f32 {
    (amount * 2.0 * n).clamp(0.0, 1.0)
}

/// What the editor and the audio thread share besides the params.
pub struct Shared {
    /// Each slot's value and playheads, one per tap (the audio thread's `publish`).
    pub monitor: Monitor,
    /// Each target's modulation now, Depth applied, per tap for the tap targets.
    pub matrix: MatrixMonitor,
    /// Bumped by the editor after it changed the patch.
    pub version: AtomicU32,
    /// A slot to start again (the editor's retrigger), taken by the audio thread.
    pub retrigger: [AtomicBool; N_SLOTS],
    tempo: AtomicU64,
    sig: AtomicU32,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            monitor: Monitor::with_voices(N_SLOTS, NUM_TAPS),
            matrix: MatrixMonitor::with_voices(N_TGT, NUM_TAPS),
            version: AtomicU32::new(0),
            retrigger: Default::default(),
            tempo: AtomicU64::new(Transport::DEFAULT_TEMPO.to_bits()),
            sig: AtomicU32::new(pack_sig(TimeSig::default())),
        }
    }
}

impl Shared {
    /// The editor changed the patch.
    pub fn changed(&self) {
        self.version.fetch_add(1, Relaxed);
    }

    pub fn tempo(&self) -> f64 {
        f64::from_bits(self.tempo.load(Relaxed))
    }

    pub fn sig(&self) -> TimeSig {
        let v = self.sig.load(Relaxed);
        TimeSig { num: (v >> 8) as u8, den: v as u8 }
    }
}

fn pack_sig(s: TimeSig) -> u32 {
    ((s.num as u32) << 8) | s.den as u32
}

#[derive(Params)]
pub struct MotionParams {
    /// Every route times this, −100..+100 % (negative turns them all over: v2.42's Polarity).
    #[id = "mx_depth"]
    pub depth: FloatParam,
    /// Every slot's rate times this: the knob 0..1, ×0.25..×4 (the crate's law).
    #[id = "mx_time"]
    pub time: FloatParam,
    /// How far apart the taps read each slot: 0 all together … 1 a whole cycle across them.
    #[id = "mx_scatter"]
    pub scatter: FloatParam,
    /// The six slots' continuous settings, "Mod 1 Rate" … "Mod 6 Gain".
    #[nested(group = "Modulators")]
    pub mods: softsystems_mod::ModParams,
    /// The slots' kinds and switches, the steps, and the routes.
    #[persist = "mod_patch"]
    pub patch: Arc<Mutex<Patch>>,
    /// Not a param.
    pub shared: Arc<Shared>,
}

impl Default for MotionParams {
    fn default() -> Self {
        Self {
            depth: FloatParam::new("Depth", DEPTH_DEFAULT, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(Arc::new(depth_text))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim().trim_end_matches('%').trim().replace('\u{2212}', "-");
                    s.parse::<f32>().ok().map(|v| v / 100.0)
                })),
            time: FloatParam::new("Time", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| time_text(time_from_knob(v)))),
            scatter: FloatParam::new("Scatter", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            mods: softsystems_mod::ModParams::new(&slots()),
            patch: Arc::new(Mutex::new(default_patch())),
            shared: Arc::new(Shared::default()),
        }
    }
}

/// Motion on the audio thread.
pub struct ModRt {
    engine: Engine,
    matrix: Matrix,
    targets: Targets,
    /// The modulators as the engine has them (the patch's, with the host's settings).
    mods: Vec<Modulator>,
    seen: u32,
    /// Scatter and the live tap count the read-heads were last placed for.
    heads: (f32, usize),
    /// Which targets a route reaches (one nothing reaches stays on its knob).
    targeted: [bool; N_TGT],
}

/// Which targets the routes reach.
fn targeted(routes: &softsystems_mod::Routes) -> [bool; N_TGT] {
    let mut t = [false; N_TGT];
    for r in routes.iter().filter(|r| r.amount != 0.0) {
        if let Some(i) = TGT_IDS.iter().position(|id| *id == r.target) {
            t[i] = true;
        }
    }
    t
}

impl ModRt {
    /// Off the audio thread (it allocates): the engine and matrix for the patch as it is.
    pub fn new(sample_rate: f32, p: &MotionParams) -> Self {
        let targets = targets();
        let patch = {
            let mut g = p.patch.lock().unwrap_or_else(|e| e.into_inner());
            slots().load(&mut g);
            g.clone()
        };
        let mut mods = patch.modulators.clone();
        p.mods.pull(&mut mods, |_, _| {});
        let mut engine = Engine::new(sample_rate, NUM_TAPS, &mods);
        // routes into the slots' own settings (pass 11); the matrix leaves them alone
        engine.set_mod_routes(&patch.routes);
        let mut matrix = Matrix::new(&targets, NUM_TAPS, ROUTE_CAP);
        matrix.load(&patch.routes, &targets);
        let targeted = targeted(&patch.routes);
        Self { engine, matrix, targets, mods, seen: p.shared.version.load(Relaxed), heads: (f32::NAN, 0), targeted }
    }

    pub fn reset(&mut self) {
        self.engine.reset();
    }

    /// Whether any route runs (none: Motion leaves every knob as set).
    pub fn routed(&self) -> bool {
        self.targeted.iter().any(|&t| t)
    }

    /// Whether a route reaches target `t`.
    pub fn reaches(&self, t: usize) -> bool {
        self.targeted.get(t).copied().unwrap_or(false)
    }

    /// Whether a route reaches any tap target.
    pub fn moves_taps(&self) -> bool {
        self.targeted[..TAP_TARGETS].iter().any(|&t| t)
    }

    /// At each block start: the editor's patch if it changed, the host's settings, Depth, Time,
    /// the transport, and the taps' places (`n` live taps).
    pub fn begin_block(&mut self, p: &MotionParams, t: &nice_plug::prelude::Transport, n: usize) {
        let v = p.shared.version.load(Relaxed);
        if v != self.seen {
            if let Ok(g) = p.patch.try_lock() {
                self.seen = v;
                for (i, (m, new)) in self.mods.iter_mut().zip(g.modulators.iter()).enumerate() {
                    if m != new {
                        *m = *new;
                        self.engine.set_config(i, m);
                    }
                }
                self.matrix.load(&g.routes, &self.targets);
                // allocates nothing; parses the ids, only when the patch changed
                self.engine.set_mod_routes(&g.routes);
                self.targeted = targeted(&g.routes);
                // a kind change restarts a slot: place its heads again
                self.heads.1 = usize::MAX;
            }
        }
        let engine = &mut self.engine;
        let mut kinds_moved = false;
        p.mods.pull(&mut self.mods, |i, m| {
            engine.set_config(i, m);
            kinds_moved = true;
        });
        if kinds_moved {
            self.heads.1 = usize::MAX;
        }
        for (i, r) in p.shared.retrigger.iter().enumerate() {
            if r.swap(false, Relaxed) {
                self.engine.retrigger(i);
            }
        }
        let tr = transport(t);
        p.shared.tempo.store(tr.tempo.to_bits(), Relaxed);
        p.shared.sig.store(pack_sig(tr.sig), Relaxed);
        self.engine.set_transport(&tr);
        self.engine.set_time(time_from_knob(p.time.value()));
        self.matrix.set_depth(p.depth.value(), &self.engine);
        self.place_heads(p.scatter.value(), n);
    }

    /// Tap `i` of `n` reads each slot `scatter · i / n` of a cycle along; the taps past `n`
    /// aren't shown.
    fn place_heads(&mut self, scatter: f32, n: usize) {
        let n = n.clamp(1, NUM_TAPS);
        if self.heads == (scatter, n) {
            return;
        }
        self.heads = (scatter, n);
        for v in 0..NUM_TAPS {
            let at = scatter as f64 * v as f64 / n as f64;
            for slot in 0..N_SLOTS {
                self.engine.set_voice_offset(slot, v, at);
            }
            self.engine.show_voice(v, v < n);
        }
    }

    /// One host sample: the follower hears `key` (the dry input's level).
    #[inline]
    pub fn advance(&mut self, key: f32) {
        self.engine.advance(1, &[&[key]]);
    }

    /// Every target's sum now; call it where Motion re-aims.
    #[inline]
    pub fn update(&mut self) {
        self.matrix.update(&self.engine);
    }

    /// A whole-echo knob's normalized value with its routes added, clamped once.
    #[inline]
    pub fn apply(&self, target: usize, base: f32) -> f32 {
        self.matrix.apply(target, base)
    }

    /// A tap knob's normalized value with that tap's routes added, clamped once.
    #[inline]
    pub fn apply_tap(&self, target: usize, tap: usize, base: f32) -> f32 {
        self.matrix.apply_voice(target, tap, base)
    }

    /// For the editor, at the block end.
    pub fn publish(&self, shared: &Shared) {
        self.engine.publish(&shared.monitor);
        self.matrix.publish(&shared.matrix);
    }
}

/// The host's transport as the engine reads it (120 BPM, 4/4 where it says nothing).
fn transport(t: &nice_plug::prelude::Transport) -> Transport {
    let sig = match (t.time_sig_numerator, t.time_sig_denominator) {
        (Some(n), Some(d)) if (1..=255).contains(&n) && (1..=255).contains(&d) => TimeSig { num: n as u8, den: d as u8 },
        _ => TimeSig::default(),
    };
    Transport {
        playing: t.playing,
        tempo: t.tempo.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(Transport::DEFAULT_TEMPO),
        beats: t.pos_beats().filter(|v| v.is_finite()).unwrap_or(0.0),
        sig,
    }
}
