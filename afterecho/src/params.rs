use nice_plug::prelude::*;
use nice_plug_egui::EguiState;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

pub const NUM_TAPS: usize = 16;

/// Per-tap spacing in standard music notation. Each entry is
/// (beats_per_tap_step, label).
///
/// In 4/4: 1/4 = 1 beat (quarter note), 1/8 = 0.5 beats (eighth note),
/// 1/2 = 2 beats, 1/1 = 4 beats (whole note = one bar), 4/1 = 16 beats = 4 bars.
///
/// Selecting "1/8" means each tap fires one 1/8 note after the previous one:
/// tap 1 at 1×(1/8), tap 2 at 2×(1/8), tap N at N×(1/8).
pub const SYNC_DIVS: &[(f32, &str)] = &[
    (0.125,    "1/32"),    // 1/32 note = 1/8 beat
    (1.0/6.0,  "1/16T"),   // 1/16 triplet
    (0.25,     "1/16"),    // 1/16 note = 1/4 beat
    (0.375,    "1/16."),   // dotted 1/16
    (1.0/3.0,  "1/8T"),    // 1/8 triplet
    (0.5,      "1/8"),     // 1/8 note = half a beat
    (0.75,     "1/8."),    // dotted 1/8
    (2.0/3.0,  "1/4T"),    // 1/4 triplet
    (1.0,      "1/4"),     // quarter note = 1 beat
    (1.5,      "1/4."),    // dotted 1/4
    (4.0/3.0,  "1/2T"),    // 1/2 triplet
    (2.0,      "1/2"),     // half note = 2 beats
    (3.0,      "1/2."),    // dotted 1/2
    (4.0,      "1/1"),     // whole note = 4 beats (1 bar)
    (6.0,      "1/1."),    // dotted whole
    (8.0,      "2/1"),     // 2 bars
    (12.0,     "2/1."),    // dotted 2/1
    (16.0,     "4/1"),     // 4 bars
];

/// Helper: snap a normalized 0..1 value to the nearest sync-division index and return (beats, label)
pub fn snap_to_div(norm: f32) -> (f32, &'static str) {
    let i = ((norm * (SYNC_DIVS.len() - 1) as f32).round() as usize).min(SYNC_DIVS.len() - 1);
    SYNC_DIVS[i]
}

/// Feedback-cycle divisions past the end of SYNC_DIVS (v2.30). They continue
/// its index scale (value × 17), so they sit above 1.0 and a saved cycle keeps
/// its division.
pub const FB_LONG_DIVS: &[(f32, &str)] = &[
    (24.0,     "4/1."),    // dotted 4/1 = 6 bars
    (32.0,     "8/1"),     // 8 bars
];

/// Top of `feedback_time`: 20 s in Free (10 ms × 800^n), 8/1 in Sync. It was
/// 1.0 (8 s, 4/1) before v2.30; the range grew past 1 instead of remapping,
/// so saved sessions (plain values) read back unchanged.
pub const FEEDBACK_TIME_MAX: f32 = 1.137_074; // ln(2000) / ln(800)

/// The feedback cycle's division for a `feedback_time` value: SYNC_DIVS, then FB_LONG_DIVS.
pub fn fb_div(norm: f32) -> (f32, &'static str) {
    let last = SYNC_DIVS.len() - 1;
    let i = (norm.max(0.0) * last as f32).round() as usize;
    if i <= last {
        SYNC_DIVS[i]
    } else {
        FB_LONG_DIVS[(i - last - 1).min(FB_LONG_DIVS.len() - 1)]
    }
}

/// Repeats: a count up to 16, then the feedback loop (∞).
pub const REPEATS_LOOP: i32 = 17;
pub const REPEATS_DEFAULT: i32 = 4;

/// The loop tone as its filter: "Off", "LP 3.5 kHz", "HP 140 Hz".
pub fn tone_text(t: f32) -> String {
    let (kind, hz) = match t {
        t if t < 0.0 => ("LP", crate::filter::dark_hz(t)),
        t if t > 0.0 => ("HP", crate::filter::thin_hz(t)),
        _ => return "Off".into(),
    };
    if hz >= 1000.0 { format!("{kind} {:.1} kHz", hz / 1000.0) } else { format!("{kind} {hz:.0} Hz") }
}

/// Reads [`tone_text`] back ("off", "lp 3.5 khz", "hp 140", "HP 1k").
pub fn tone_from_text(s: &str) -> Option<f32> {
    let s = s.trim().to_ascii_lowercase();
    if s == "off" {
        return Some(0.0);
    }
    let (dark, rest) = match (s.strip_prefix("lp"), s.strip_prefix("hp")) {
        (Some(r), _) => (true, r),
        (_, Some(r)) => (false, r),
        _ => return None,
    };
    let rest = rest.trim();
    let (num, k) = if let Some(n) = rest.strip_suffix("khz").or(rest.strip_suffix('k')) {
        (n, 1000.0)
    } else {
        (rest.strip_suffix("hz").unwrap_or(rest), 1.0)
    };
    let hz = num.trim().parse::<f32>().ok()? * k;
    Some(crate::filter::tone_at(dark, hz))
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SpacingMode {
    #[id = "linear"]
    Linear,
    #[id = "exponential"]
    Exponential,
    #[id = "logarithmic"]
    Logarithmic,
    #[id = "euclidean"]
    Euclidean,
}

/// How the delay reacts when tap-position parameters change at runtime.
///
/// - `Repitch` (default): smooth glide between old and new delay time
///   (Ableton-style "Repitch"). Sounds like tape-bend. Wobble works.
/// - `Fade`: equal-power crossfade between old and new read positions
///   over ~21 ms — pitch-neutral. Best for editing during playback.
///   Wobble is bypassed in this mode (it would constantly re-trigger fades).
/// - `Jump`: instantly snaps to the new delay. Audible click on big jumps,
///   but zero pitch artifact. Best when automating discrete steps.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum DelayMode {
    #[id = "repitch"]
    Repitch,
    #[id = "fade"]
    Fade,
    #[id = "jump"]
    Jump,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SyncMode {
    #[id = "free"]
    Free,
    #[id = "sync"]
    Sync,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ChannelMode {
    #[id = "stereo"]
    Stereo,
    #[id = "midside"]
    MidSide,
    #[id = "mono"]
    Mono,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FilterType {
    #[id = "lp"]
    LowPass,
    #[id = "bp"]
    BandPass,
    #[id = "hp"]
    HighPass,
}

/// How detuned taps are shifted (v2.45).
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum DetuneQuality {
    #[id = "light"]
    Light,
    #[id = "clean"]
    Clean,
}

/// Per-tap filter slope, 6 to 48 dB/oct in 6 dB steps (owner, 2026-11-16; before, 12, 24 and
/// 48): SVF stages (12 dB each) and a one-pole for an odd step (`filter::TapLayout`). The order
/// is the slope's, so the index is `dB / 6 − 1`. Ids are stable (sessions store them); presets
/// store the index, so files from before are moved by `presets::Store::migrate`.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FilterSlope {
    #[id = "db6"]
    Db6,
    #[id = "db12"]
    Db12,
    #[id = "db18"]
    Db18,
    #[id = "db24"]
    Db24,
    #[id = "db30"]
    Db30,
    #[id = "db36"]
    Db36,
    #[id = "db42"]
    Db42,
    #[id = "db48"]
    Db48,
}

/// How the global Crossfeed knob blends between channels.
/// - Mix: post-tap stereo blend (good for centering / widening).
/// - PingPong: crossfeed lives in the feedback path → echoes alternate L↔R.
/// - Diffuse: feedback runs through Diffuse's chain (`diffusion.rs`), Cross
///   its density, smearing echoes into a reverb-like tail.
/// Where in the signal path the DIFFUSE tab's diffuser is inserted.
/// - `Input`: applied to the dry input before it enters the delay buffer.
///   Each tap reads pre-diffused signal, so the cluster sounds layered.
/// - `Feedback`: applied to the feedback path. Each feedback cycle is
///   re-diffused, so repeats accumulate progressively more smear.
/// - `Output`: applied to the tap-sum before dry/wet (cleanest; doesn't
///   interact with the delay / feedback dynamics).
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum DiffuseRouting {
    #[id = "input"]
    Input,
    #[id = "feedback"]
    Feedback,
    #[id = "output"]
    Output,
}

/// Saturate's curve (beta pass 6): today's blended tanh, or one of the shared crate's five
/// (softsystems-fx SATURATE, 8× oversampled, level held by its make-up). All six stay until
/// the owner has picked by ear (DECISIONS 2026-11-16). The crate's TAPE is "Smooth" here, so
/// it doesn't read as Colour's Tape tab.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SatCurve {
    #[id = "classic"]
    Classic,
    #[id = "tube"]
    Tube,
    #[id = "smooth"]
    Smooth,
    #[id = "grit"]
    Grit,
    #[id = "amp"]
    Amp,
    #[id = "transfo"]
    Transfo,
}

/// IEC cassette tape types for the DEGRADE block (v2.21.0, re-deemer port).
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum TapeType {
    /// Ferric: saturates earliest, dullest top — the classic worn-cassette lo-fi.
    #[id = "type_i"]
    I,
    /// Chrome: more headroom, brighter.
    #[id = "type_ii"]
    II,
    /// Metal: most headroom, stays composed when slammed.
    #[id = "type_iv"]
    IV,
}

/// Decay shape of successive feedback repeats (v2.20.0). A recirculating
/// loop is inherently exponential (constant per-cycle gain), so non-natural
/// curves are made by letting an envelope follower inside the loop bend the
/// effective gain by level.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FbCurve {
    /// Constant per-cycle gain — the classic exponential decay.
    #[id = "natural"]
    Natural,
    /// Compressor-in-the-loop: loud repeats are tamed, quiet ones get a
    /// (capped) lift — the tail plateaus near a target level, sustains, then
    /// dies. Dub-machine "held" decay.
    #[id = "bloom"]
    Bloom,
    /// Expander-in-the-loop: repeats hold close to full level until they dip
    /// under the gate threshold, then the tail is cut fast. Gated-reverb
    /// style decay; FEEDBACK sets how long the hold lasts.
    #[id = "gate"]
    Gate,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CrossfeedMode {
    #[id = "mix"]
    Mix,
    #[id = "pingpong"]
    PingPong,
    #[id = "diffuse"]
    Diffuse,
}

use softsystems_ui::knob::level;

/// In and Out: the shared level knob (nice-plug 627ebb7, owner 2026-11-20: "i want it global"),
/// −60…+12 dB straight in dB, so the normalised value is `level::norm` of the dB and 0 dB sits
/// at 3 o'clock. At the bottom the plug-in is silent ("−∞ dB").
const GAIN_RANGE: FloatRange = FloatRange::Linear { min: level::MIN_DB, max: level::MAX_DB };

/// A level's text, "−∞ dB" at the bottom (the crate's `level::text`).
fn level_v2s() -> Arc<dyn Fn(f32) -> String + Send + Sync> {
    Arc::new(|db| level::text(level::norm(db)))
}

/// A typed level: "+6", "−3.5 dB", "-inf"; the bottom for −∞.
fn level_s2v() -> Arc<dyn Fn(&str) -> Option<f32> + Send + Sync> {
    Arc::new(|s| {
        let t = s.trim().trim_end_matches("dB").trim().replace('\u{2212}', "-");
        if t.contains('\u{221e}') || t.eq_ignore_ascii_case("-inf") {
            return Some(level::MIN_DB);
        }
        t.parse::<f32>().ok().map(|db| db.clamp(level::MIN_DB, level::MAX_DB))
    })
}

#[derive(Params)]
pub struct AfterechoParams {
    /// Persists the editor window size across sessions.
    #[persist = "editor"]
    pub editor_state: Arc<EguiState>,

    /// The loaded preset (name, kind, edited), saved with the host's session
    /// so a reopened project names it (the preset spec §10). GUI thread only.
    #[persist = "preset-session"]
    pub preset_session: Arc<std::sync::Mutex<crate::presets::PresetSession>>,

    // ---- Topology (Layer 1) ----
    /// Free mode: time in ms. Sync mode: time as 0..1 normalized over SYNC_DIVS.
    #[id = "base_time_l"]
    pub base_time_l: FloatParam,
    #[id = "base_time_r"]
    pub base_time_r: FloatParam,
    #[id = "time_linked"]
    pub time_linked: BoolParam,

    #[id = "spacing_mode"]
    pub spacing_mode: EnumParam<SpacingMode>,
    /// Euclid (v2.38): the pattern's length in Time-knob steps; the taps
    /// spread over them as evenly as possible (see `topology::euclid_steps`).
    #[id = "euclid_steps"]
    pub euclid_steps: IntParam,
    /// Euclid: the pattern turned this many steps against the dry note.
    #[id = "euclid_rotate"]
    pub euclid_rotate: IntParam,
    /// Swing (v2.38), 0.5 straight … 0.75: taps on odd steps land late, in
    /// Lin and Euclid.
    #[id = "swing"]
    pub swing: FloatParam,
    #[id = "shape_x_l"]
    pub shape_x_l: FloatParam,
    #[id = "shape_y_l"]
    pub shape_y_l: FloatParam,
    #[id = "shape_x_r"]
    pub shape_x_r: FloatParam,
    #[id = "shape_y_r"]
    pub shape_y_r: FloatParam,
    #[id = "shape_linked"]
    pub shape_linked: BoolParam,

    #[id = "sync_mode"]
    pub sync_mode: EnumParam<SyncMode>,
    /// Per-channel sync for TIME R (v3 design). When `time_linked` is on,
    /// R follows L's mode — use `sync_mode_r_eff()`, not this field, in DSP.
    #[id = "sync_r"]
    pub sync_mode_r: EnumParam<SyncMode>,
    /// Repitch / Fade / Jump — how the delay handles tap-time changes.
    #[id = "delay_mode"]
    pub delay_mode: EnumParam<DelayMode>,
    /// Global snap toggle for the Grid view horizontal drag.
    #[id = "tap_snap"]
    pub tap_snap: BoolParam,
    /// Snap's grid (v2.43): the tap spacing divided by 1…4, so a tap can sit
    /// on halves, thirds or quarters of it.
    #[id = "tap_grid"]
    pub tap_grid: IntParam,
    /// How detuned taps are shifted (v2.45): Light (WSOLA) or Clean (phase
    /// vocoder, about 3× the CPU; taps under ~90 ms use Light).
    #[id = "detune_quality"]
    pub detune_quality: EnumParam<DetuneQuality>,
    /// What a tap's Detune at ±100 % means, in semitones (v2.46).
    #[id = "detune_range"]
    pub detune_range: IntParam,

    #[id = "channel_mode"]
    pub channel_mode: EnumParam<ChannelMode>,

    /// How many of the NUM_TAPS taps are "live". GUI hides the rest.
    #[id = "num_taps_active"]
    pub num_taps_active: IntParam,

    // ---- Per-tap (Layer 2) ----
    #[nested(array, group = "tap")]
    pub taps: [TapParams; NUM_TAPS],

    // ---- Feedback ----
    /// How much each repeat keeps (0 … 105 %).
    #[id = "feedback"]
    pub feedback: FloatParam,
    /// How many times the pattern repeats (v2.42): 1…16 copies, then a clean
    /// stop; `REPEATS_LOOP` (∞) is the feedback loop. New instances start on
    /// a count; sessions and presets from before load as ∞ (see `migrate`).
    #[id = "repeats"]
    pub repeats: IntParam,
    /// Feedback time, 0..FEEDBACK_TIME_MAX. In Free mode maps to 10 ms..20 s
    /// (exponential). In Sync mode snaps to the nearest division (`fb_div`).
    /// Independent of the tap layout — you can space the echo cluster
    /// anywhere regardless of where the taps sit.
    #[id = "feedback_time"]
    pub feedback_time: FloatParam,
    /// When true, FB TIME is locked to the cluster span (= last active tap's delay).
    /// Unlock to set an independent FB TIME. Persisted so the DAW remembers the lock state.
    #[id = "fb_time_locked"]
    pub feedback_time_locked: BoolParam,

    /// Crossfeed amount: 0 = no crossfeed, 1 = full effect. The mode below
    /// decides what "crossfeed" does (mix / ping-pong / diffuse).
    #[id = "crossfeed"]
    pub crossfeed: FloatParam,
    #[id = "crossfeed_mode"]
    pub crossfeed_mode: EnumParam<CrossfeedMode>,

    // ---- Output diffusion ----
    /// Diffuse's density [0, 1]: its allpasses' gain (v2.47, `diffusion.rs`).
    /// 0 = off on In and Out (the chain fades in over the first 20 %), a plain
    /// delay the loops take back on FB. Smears each repeat into a wide wash
    /// without altering the frequency response.
    #[id = "diffuse_amount"]
    pub diffuse_amount: FloatParam,
    /// Where the DIFFUSE tab's effect is inserted into the signal path.
    #[id = "diffuse_routing"]
    pub diffuse_routing: EnumParam<DiffuseRouting>,
    /// Scales the six allpass delays (1.0 = 1.3–11 ms, ~31 ms in all;
    /// 0.25 tight, 2.0 long). Glides, so turning it doesn't click.
    #[id = "diffuse_size"]
    pub diffuse_size: FloatParam,

    // ---- Degrade (physically-modeled tape, ported from RE-DEEMER; the
    //      v2.19 bitcrush was the wrong algorithm — owner wanted tape) ----
    /// Dry/wet of the degrade block. 0 = bypass.
    #[id = "degrade_amount"]
    pub degrade_amount: FloatParam,
    /// Drive into the tape in dB (0 = calibrated operating point). Full
    /// makeup gain: sets how hard the tape saturates, not the level.
    #[id = "degrade_drive"]
    pub degrade_drive: FloatParam,
    /// Tape aging 0..1: sagged bias (hysteresis grit), earlier/harder HF
    /// self-erasure, fallen gap-loss corner, hotter operating point.
    #[id = "degrade_wear"]
    pub degrade_wear: FloatParam,
    /// IEC tape type: I ferric / II chrome / IV metal.
    #[id = "degrade_type"]
    pub degrade_type: EnumParam<TapeType>,
    /// Where the degrade block is inserted (same grammar as DIFFUSE).
    /// Feedback routing = generation loss: every repeat degrades further.
    #[id = "degrade_routing"]
    pub degrade_routing: EnumParam<DiffuseRouting>,

    // ---- Saturate (playable drive block; the loop always has its own
    //      fixed safety tanh — this one is the *characterful* stage) ----
    /// Dry/wet of the saturate block. 0 = bypass.
    #[id = "sat_amount"]
    pub sat_amount: FloatParam,
    /// Drive into the tanh stage, in dB (0..24).
    #[id = "sat_drive"]
    pub sat_drive: FloatParam,
    /// Where the saturate block is inserted (same grammar as DIFFUSE).
    #[id = "sat_routing"]
    pub sat_routing: EnumParam<DiffuseRouting>,
    /// Saturate's curve (beta pass 6).
    #[id = "sat_curve"]
    pub sat_curve: EnumParam<SatCurve>,

    // ---- Dub (beta pass 12): Cumulus's Dub Delay feedback path, always in the loop ----
    /// How hard each pass is driven and how much the low-pass rings; 0 is off.
    #[id = "dub_intensity"]
    pub dub_intensity: FloatParam,
    /// The band each pass is cut to: its high-pass…
    #[id = "dub_low"]
    pub dub_low: FloatParam,
    /// …and its resonant low-pass.
    #[id = "dub_high"]
    pub dub_high: FloatParam,
    /// The allpasses' centre: where each pass smears.
    #[id = "dub_swirl"]
    pub dub_swirl: FloatParam,
    /// How many allpasses, 0–3.
    #[id = "dub_swirls"]
    pub dub_swirls: IntParam,

    // ---- Feedback decay curve ----
    /// Decay shape of the feedback repeats: Natural / Bloom / Gate.
    #[id = "fb_curve"]
    pub fb_curve: EnumParam<FbCurve>,
    /// Loop tone (v2.34), on the feedback only: below 0 a low-pass darkens
    /// each repeat, above 0 a high-pass thins it, 0 is off. See
    /// `filter::LoopTone`.
    #[id = "loop_tone"]
    pub loop_tone: FloatParam,

    /// Freeze (v2.35): hold the loop as it is, at exactly 100 %. New input
    /// stays out of the delay; Feedback, Curve and the loop's colour are
    /// bypassed. A performance switch, so presets and A/B leave it alone.
    #[id = "freeze"]
    pub freeze: BoolParam,
    /// The held loop's length in seconds (f32 bits) while Freeze holds, else
    /// 0. Written by the audio thread for the page to draw; not a param and
    /// not saved.
    pub held_cycle_secs: AtomicU32,
    /// Ear guard (v2.43): echoes that stay very loud for a couple of seconds
    /// ease down at the output (never inside the loop, which may still
    /// self-oscillate). On by default; a safety setting, so presets and A/B
    /// leave it alone.
    #[id = "ear_guard"]
    pub ear_guard: BoolParam,
    /// How far the guard has the echoes down (dB, f32 bits, the most in the
    /// last block), for the page. Not a param, not saved.
    pub guard_db: AtomicU32,
    /// Clear (v2.43): the page asks the audio thread to empty the delay
    /// (the echoes fade out over 10 ms first). Not a param.
    pub clear_request: AtomicBool,

    // ---- Filter ----
    #[id = "filter_type"]
    pub filter_type: EnumParam<FilterType>,
    #[id = "filter_cutoff"]
    pub filter_cutoff: FloatParam,
    #[id = "filter_q"]
    pub filter_q: FloatParam,

    // ---- Wobble (LFO) ----

    // ---- I/O ----
    #[id = "input_gain"]
    pub input_gain: FloatParam,
    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
    #[id = "output_gain"]
    pub output_gain: FloatParam,

    // ---- Ducking (v2.36): the echoes dip while the dry input is loud ----
    /// The most the echoes dip, in dB (0 = off). How far they actually dip
    /// follows how loud the dry input is.
    #[id = "duck"]
    pub duck: FloatParam,
    /// How fast the echoes come back once the input goes quiet (ms).
    #[id = "duck_release"]
    pub duck_release: FloatParam,
    /// The echoes' dip right now in dB (f32 bits, the most in the last
    /// block). Written by the audio thread for the Duck knob; not a param,
    /// not saved.
    pub duck_dip_db: AtomicU32,

    // ---- Motion (beta pass 8): the shared modulation crate ----
    /// Six slots (LFO, Steps or Follow), Depth, Time, Scatter and the routes
    /// (`modulation.rs`).
    #[nested(group = "Motion")]
    pub motion: crate::modulation::MotionParams,
}

#[derive(Params)]
pub struct TapParams {
    #[id = "active"]
    pub active: BoolParam,
    // NOTE: per-tap pitch (semitones + bend/continuous mode toggle) was removed in
    // v2.4.0. The granular pitch shifter introduced audible flutter/AM on sustained
    // tones (textbook artifact of any dual-head Hann shifter without phase locking)
    // and the tape-bend fallback only Doppler-shifted *during* delay-time changes,
    // not in steady state. Re-introducing pitch would require either a phase vocoder
    // (heavy CPU per tap) or PSOLA + a pitch tracker (breaks on polyphony). Both
    // are out of scope for a multitap delay. The shifter code lives in git history
    // up to v2.3.5 if we ever want to revisit it as a separate plugin.
    #[id = "gain_db"]
    pub gain_db: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    /// Per-tap time offset around the topology-derived position, in fraction of
    /// the full delay span (−0.5 .. +0.5). Drag a Grid ball horizontally to set.
    #[id = "time_offset"]
    pub time_offset: FloatParam,

    // ---- Per-tap filter ----
    #[id = "tap_filter_type"]
    pub filter_type: EnumParam<FilterType>,
    #[id = "tap_filter_cutoff"]
    pub filter_cutoff: FloatParam,
    #[id = "tap_filter_q"]
    pub filter_q: FloatParam,
    #[id = "tap_filter_slope"]
    pub filter_slope: EnumParam<FilterSlope>,
    /// Detune (v2.45): the tap's pitch, ±100 cents (`detune.rs`).
    #[id = "tap_detune"]
    pub detune: FloatParam,
}

impl Default for TapParams {
    fn default() -> Self {
        Self {
            active: BoolParam::new("Active", true),
            gain_db: FloatParam::new(
                "Gain",
                0.0,
                GAIN_RANGE, // the shared level knob, as In and Out (owner 2026-11-21)
            )
            .with_step_size(0.1)
            .with_value_to_string(level_v2s())
            .with_string_to_value(level_s2v())
            .with_smoother(SmoothingStyle::Linear(10.0)),
            // Displayed/edited as -100..+100 in steps of 1 (stored -1..1).
            pan: FloatParam::new("Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_step_size(0.01)
                .with_value_to_string(Arc::new(|v| format!("{:.0}", v * 100.0)))
                .with_string_to_value(Arc::new(|s| s.trim().parse::<f32>().ok().map(|v| v / 100.0)))
                .with_smoother(SmoothingStyle::Linear(10.0)),
            time_offset: FloatParam::new(
                "Time Offset",
                0.0,
                FloatRange::Linear { min: -0.5, max: 0.5 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0)),
            filter_type: EnumParam::new("Tap Filter Type", FilterType::LowPass),
            filter_cutoff: FloatParam::new(
                "Tap Cutoff",
                20000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_step_size(1.0)
            .with_value_to_string(Arc::new(|v| format!("{:.0}", v)))
            .with_smoother(SmoothingStyle::Logarithmic(30.0)),
            filter_q: FloatParam::new(
                "Tap Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 12.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_step_size(0.1)
            .with_value_to_string(Arc::new(|v| format!("{:.1}", v)))
            .with_smoother(SmoothingStyle::Logarithmic(30.0)),
            filter_slope: EnumParam::new("Tap Slope", FilterSlope::Db12),
            // % of Detune Range (v2.46; v2.45 had cents at a fixed ±100, the
            // same at the default range of 1 semitone)
            detune: FloatParam::new("Tap Detune", 0.0, FloatRange::Linear { min: -100.0, max: 100.0 })
                .with_unit(" %")
                .with_step_size(0.1)
                .with_value_to_string(Arc::new(|v| if v.abs() < 0.05 { "0".into() } else { format!("{v:+.1}") }))
                .with_string_to_value(Arc::new(|s| s.trim().trim_end_matches('%').trim().parse().ok())),
        }
    }
}

impl Default for AfterechoParams {
    fn default() -> Self {
        let time_default = 0.25; // normalized → ~125 ms in Free mode, 1/4 in Sync mode
        Self {
            // The page follows the host's bounds on iOS and picks its shape
            // there (gui_scope.rs `Shape::pick`).
            editor_state: crate::gui::new_editor_state(),
            preset_session: Arc::new(std::sync::Mutex::new(Default::default())),
            base_time_l: FloatParam::new(
                "Base Time L",
                time_default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            base_time_r: FloatParam::new(
                "Base Time R",
                time_default,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            time_linked: BoolParam::new("Time Link", true),

            spacing_mode: EnumParam::new("Spacing", SpacingMode::Linear),
            euclid_steps: IntParam::new("Euclid Steps", 16, IntRange::Linear { min: 2, max: NUM_TAPS as i32 }),
            euclid_rotate: IntParam::new("Euclid Rotate", 0, IntRange::Linear { min: 0, max: NUM_TAPS as i32 - 1 }),
            swing: FloatParam::new("Swing", 0.5, FloatRange::Linear { min: 0.5, max: 0.75 })
                .with_value_to_string(Arc::new(|v| if v < 0.505 { "Straight".into() } else { format!("{:.0} %", v * 100.0) }))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim().to_ascii_lowercase();
                    if s == "straight" {
                        return Some(0.5);
                    }
                    s.trim_end_matches('%').trim().parse::<f32>().ok().map(|v| if v > 1.0 { v / 100.0 } else { v })
                })),

            shape_x_l: FloatParam::new("Shape X L", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            shape_y_l: FloatParam::new("Shape Y L", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            shape_x_r: FloatParam::new("Shape X R", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            shape_y_r: FloatParam::new("Shape Y R", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            shape_linked: BoolParam::new("Shape Link", true),

            sync_mode: EnumParam::new("Sync Mode", SyncMode::Free),
            sync_mode_r: EnumParam::new("Sync Mode R", SyncMode::Free),
            delay_mode: EnumParam::new("Delay Mode", DelayMode::Repitch),
            tap_snap: BoolParam::new("Snap", true),
            detune_quality: EnumParam::new("Detune Quality", DetuneQuality::Light),
            detune_range: IntParam::new("Detune Range", 1, IntRange::Linear { min: 1, max: crate::detune::MAX_SEMITONES })
                .with_value_to_string(Arc::new(|v| format!("{v} st")))
                .with_string_to_value(Arc::new(|s| s.trim().trim_start_matches('±').trim_end_matches("st").trim().parse().ok())),
            tap_grid: IntParam::new("Snap Grid", 1, IntRange::Linear { min: 1, max: 4 })
                .with_value_to_string(Arc::new(|v| if v <= 1 { "1".into() } else { format!("1/{v}") })),

            channel_mode: EnumParam::new("Channel Mode", ChannelMode::Stereo),

            num_taps_active: IntParam::new(
                "Tap Count",
                8, // default 8 active even with max 16
                IntRange::Linear { min: 1, max: NUM_TAPS as i32 },
            ),

            taps: core::array::from_fn(|_| TapParams::default()),

            feedback: FloatParam::new(
                "Feedback",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.05 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            repeats: IntParam::new("Repeats", REPEATS_DEFAULT, IntRange::Linear { min: 1, max: REPEATS_LOOP })
                .with_value_to_string(Arc::new(|v| if v >= REPEATS_LOOP { "∞".into() } else { v.to_string() }))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim();
                    if s == "∞" || s.eq_ignore_ascii_case("inf") { Some(REPEATS_LOOP) } else { s.parse().ok() }
                })),
            // Default: Sync mode, 1/4 note (index 8 of 17 divs → 8/17 ≈ 0.47)
            feedback_time: FloatParam::new(
                "Feedback Time",
                8.0 / (SYNC_DIVS.len() - 1) as f32,
                FloatRange::Linear { min: 0.0, max: FEEDBACK_TIME_MAX },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            feedback_time_locked: BoolParam::new("Feedback Time Locked", true),

            crossfeed: FloatParam::new("Crossfeed", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage())
                .with_smoother(SmoothingStyle::Linear(20.0)),
            crossfeed_mode: EnumParam::new("Crossfeed Mode", CrossfeedMode::Mix),

            diffuse_amount: FloatParam::new(
                "Diffuse Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            diffuse_routing: EnumParam::new("Diffuse Routing", DiffuseRouting::Output),
            diffuse_size: FloatParam::new(
                "Diffuse Size",
                1.0,
                FloatRange::Skewed { min: 0.25, max: 2.0, factor: FloatRange::skew_factor(-0.5) },
            )
            .with_value_to_string(Arc::new(|v| format!("{v:.2}×")))
            .with_string_to_value(Arc::new(|s| s.trim().trim_end_matches(['×', 'x']).trim().parse().ok()))
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),

            degrade_amount: FloatParam::new(
                "Degrade Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            degrade_drive: FloatParam::new(
                "Degrade Drive",
                0.0,
                FloatRange::Linear { min: -12.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            degrade_wear: FloatParam::new(
                "Degrade Wear",
                0.25,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            degrade_type: EnumParam::new("Degrade Tape Type", TapeType::I),
            degrade_routing: EnumParam::new("Degrade Routing", DiffuseRouting::Output),

            sat_amount: FloatParam::new(
                "Saturate Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            sat_drive: FloatParam::new(
                "Saturate Drive",
                6.0,
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            sat_routing: EnumParam::new("Saturate Routing", DiffuseRouting::Output),
            sat_curve: EnumParam::new("Saturate Curve", SatCurve::Classic),
            // the listening spike's "Dub medium" band and swirl (not heard yet, TASKS)
            dub_intensity: FloatParam::new("Dub Intensity", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage())
                .with_smoother(SmoothingStyle::Linear(20.0)),
            dub_low: FloatParam::new("Dub Low Cut", 150.0, FloatRange::Skewed { min: 20.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
                .with_string_to_value(formatters::s2v_f32_hz_then_khz())
                .with_smoother(SmoothingStyle::Linear(20.0)),
            dub_high: FloatParam::new("Dub High Cut", 2500.0, FloatRange::Skewed { min: 200.0, max: 16000.0, factor: FloatRange::skew_factor(-2.0) })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
                .with_string_to_value(formatters::s2v_f32_hz_then_khz())
                .with_smoother(SmoothingStyle::Linear(20.0)),
            dub_swirl: FloatParam::new("Dub Swirl", 600.0, FloatRange::Skewed { min: 20.0, max: 16000.0, factor: FloatRange::skew_factor(-2.0) })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
                .with_string_to_value(formatters::s2v_f32_hz_then_khz())
                .with_smoother(SmoothingStyle::Linear(20.0)),
            dub_swirls: IntParam::new("Dub Swirls", 2, IntRange::Linear { min: 0, max: 3 }),

            fb_curve: EnumParam::new("Feedback Curve", FbCurve::Natural),
            loop_tone: FloatParam::new("Loop Tone", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(Arc::new(tone_text))
                .with_string_to_value(Arc::new(tone_from_text)),
            freeze: BoolParam::new("Freeze", false),
            held_cycle_secs: AtomicU32::new(0),
            ear_guard: BoolParam::new("Ear Guard", true),
            guard_db: AtomicU32::new(0),
            clear_request: AtomicBool::new(false),

            filter_type: EnumParam::new("Filter Type", FilterType::LowPass),
            filter_cutoff: FloatParam::new(
                "Cutoff",
                20000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),
            filter_q: FloatParam::new(
                "Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 16.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),

            input_gain: FloatParam::new(
                "Input Gain",
                0.0,
                GAIN_RANGE,
            )
            .with_value_to_string(level_v2s())
            .with_string_to_value(level_s2v())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            dry_wet: FloatParam::new(
                "Dry/Wet",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage())
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_gain: FloatParam::new(
                "Output Gain",
                0.0,
                GAIN_RANGE,
            )
            .with_value_to_string(level_v2s())
            .with_string_to_value(level_s2v())
            .with_smoother(SmoothingStyle::Linear(20.0)),

            duck: FloatParam::new("Duck", 0.0, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_value_to_string(Arc::new(|v| if v < 0.05 { "Off".into() } else { format!("−{v:.1} dB") }))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim().to_ascii_lowercase();
                    if s == "off" {
                        return Some(0.0);
                    }
                    let n = s.trim_end_matches("db").trim().trim_start_matches(['-', '−']).trim();
                    n.parse::<f32>().ok()
                })),
            duck_release: FloatParam::new(
                "Duck Release",
                300.0,
                FloatRange::Skewed { min: 50.0, max: 2000.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_value_to_string(Arc::new(|v| if v < 1000.0 { format!("{v:.0} ms") } else { format!("{:.2} s", v / 1000.0) }))
            .with_string_to_value(Arc::new(|s| {
                let s = s.trim().to_ascii_lowercase();
                if let Some(ms) = s.strip_suffix("ms") {
                    ms.trim().parse().ok()
                } else if let Some(secs) = s.strip_suffix('s') {
                    secs.trim().parse::<f32>().ok().map(|v| v * 1000.0)
                } else {
                    s.parse().ok()
                }
            })),
            duck_dip_db: AtomicU32::new(0),

            motion: crate::modulation::MotionParams::default(),
        }
    }
}

// ─── Time conversion helpers ──────────────────────────────────────────────
impl AfterechoParams {
    /// How the taps spread on one side: the mode, that side's Shape pad (the
    /// left one while they're linked), Euclid's steps and rotation, swing.
    pub fn spacing(&self, right: bool) -> crate::topology::Spacing {
        let (x, y) = if right && !self.shape_linked.value() {
            (&self.shape_x_r, &self.shape_y_r)
        } else {
            (&self.shape_x_l, &self.shape_y_l)
        };
        crate::topology::Spacing {
            mode: self.spacing_mode.value(),
            shape_x: x.value(),
            shape_y: y.value(),
            steps: self.euclid_steps.value() as usize,
            rotate: self.euclid_rotate.value() as usize,
            swing: self.swing.value(),
        }
    }

    /// The taps heard: Euclid places at most one per step, and any more stay
    /// silent (v2.38).
    pub fn taps_heard(&self) -> usize {
        let n = (self.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
        if self.spacing_mode.value() == SpacingMode::Euclidean { n.min(self.euclid_steps.value() as usize) } else { n }
    }

    /// The whole pattern's length, the shortest the Auto cycle may be: in
    /// Euclid, its steps on the longer side's grid, so the rhythm loops on
    /// the pattern even when its last tap lands early. 0 otherwise.
    pub fn pattern_secs(&self, base_l: f32, base_r: f32) -> f32 {
        if self.spacing_mode.value() == SpacingMode::Euclidean {
            self.euclid_steps.value() as f32 * base_l.max(base_r)
        } else {
            0.0
        }
    }

    /// Convert a base-time param value (0..1) to seconds, depending on sync mode and tempo.
    /// Effective sync mode for the R channel: follows L while time is linked.
    pub fn sync_mode_r_eff(&self) -> SyncMode {
        if self.time_linked.value() { self.sync_mode.value() } else { self.sync_mode_r.value() }
    }

    /// Base time in seconds under an EXPLICIT sync mode (per-channel since v3).
    pub fn base_time_seconds_mode(mode: SyncMode, normalized: f32, tempo_bpm: f32) -> f32 {
        match mode {
            SyncMode::Free => {
                // Exponential 1..2000 ms — small values get more resolution.
                let ms = 1.0_f32 * 2000.0_f32.powf(normalized.clamp(0.0, 1.0));
                ms * 0.001
            }
            SyncMode::Sync => {
                let (beats, _) = snap_to_div(normalized);
                (60.0 / tempo_bpm) * beats
            }
        }
    }

    /// L-channel (and pre-v3 global) base time in seconds.
    pub fn base_time_seconds(&self, normalized: f32, tempo_bpm: f32) -> f32 {
        Self::base_time_seconds_mode(self.sync_mode.value(), normalized, tempo_bpm)
    }

    /// Display string for a base-time value under an explicit sync mode.
    pub fn format_base_time_mode(mode: SyncMode, normalized: f32, tempo_bpm: f32) -> String {
        match mode {
            SyncMode::Free => {
                let secs = Self::base_time_seconds_mode(mode, normalized, tempo_bpm);
                let ms = secs * 1000.0;
                if ms < 10.0 { format!("{:.2} ms", ms) }
                else if ms < 100.0 { format!("{:.1} ms", ms) }
                else { format!("{:.0} ms", ms) }
            }
            SyncMode::Sync => {
                let (_, label) = snap_to_div(normalized);
                label.to_string()
            }
        }
    }

    /// Display string for a base-time param value (L / global mode).
    pub fn format_base_time(&self, normalized: f32, tempo_bpm: f32) -> String {
        Self::format_base_time_mode(self.sync_mode.value(), normalized, tempo_bpm)
    }

    /// Feedback delay time in seconds. Derives from `feedback_time` param
    /// (0..1 normalised), interpreted via the MAIN delay's `sync_mode`
    /// (v2.19.0: synced delay → synced feedback divisions, free delay →
    /// free ms).
    pub fn feedback_time_seconds(&self, tempo_bpm: f32) -> f32 {
        Self::feedback_time_seconds_mode(self.sync_mode.value(), self.feedback_time.value(), tempo_bpm)
    }

    /// `feedback_time_seconds` under an explicit sync mode.
    pub fn feedback_time_seconds_mode(mode: SyncMode, normalized: f32, tempo_bpm: f32) -> f32 {
        match mode {
            SyncMode::Free => {
                // Exponential 10 ms..20 s — the base-time knob's curve, run on past 8 s.
                let ms = 10.0_f32 * 800.0_f32.powf(normalized.clamp(0.0, FEEDBACK_TIME_MAX));
                ms * 0.001
            }
            SyncMode::Sync => {
                let (beats, _) = fb_div(normalized);
                (60.0 / tempo_bpm) * beats
            }
        }
    }

    /// The `feedback_time` value whose cycle is closest to `secs` — the inverse
    /// of `feedback_time_seconds_mode`, for dragging the Echo lane's cycle edge.
    pub fn feedback_time_norm_for_secs(mode: SyncMode, secs: f32, tempo_bpm: f32) -> f32 {
        match mode {
            SyncMode::Free => ((secs * 100.0).max(1.0).ln() / 800.0_f32.ln()).clamp(0.0, FEEDBACK_TIME_MAX),
            SyncMode::Sync => {
                let beats = secs * tempo_bpm / 60.0;
                let nearest = SYNC_DIVS
                    .iter()
                    .chain(FB_LONG_DIVS)
                    .enumerate()
                    .min_by(|a, b| (a.1 .0 - beats).abs().total_cmp(&(b.1 .0 - beats).abs()))
                    .map_or(0, |(i, _)| i);
                nearest as f32 / (SYNC_DIVS.len() - 1) as f32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_time_inverse_round_trips() {
        // Free: any time in range comes back within a hair.
        for ms in [10.0f32, 37.0, 250.0, 1000.0, 3333.0, 8000.0, 12500.0, 20000.0] {
            let n = AfterechoParams::feedback_time_norm_for_secs(SyncMode::Free, ms / 1000.0, 120.0);
            let back = AfterechoParams::feedback_time_seconds_mode(SyncMode::Free, n, 120.0) * 1000.0;
            assert!((back - ms).abs() < ms * 1e-3, "{ms} ms came back as {back}");
        }
        // Sync: every division, the long ones too, maps back onto itself.
        for (i, &(beats, label)) in SYNC_DIVS.iter().chain(FB_LONG_DIVS).enumerate() {
            let secs = beats * 60.0 / 120.0;
            let n = AfterechoParams::feedback_time_norm_for_secs(SyncMode::Sync, secs, 120.0);
            assert!(n <= FEEDBACK_TIME_MAX, "division {i} lands past the range");
            assert_eq!(fb_div(n).1, label, "division {i}");
        }
    }

    #[test]
    fn longer_cycles_leave_saved_ones_alone() {
        // A cycle saved before v2.30 (0..1) keeps its division and its time.
        for i in 0..SYNC_DIVS.len() {
            let n = i as f32 / (SYNC_DIVS.len() - 1) as f32;
            assert_eq!(fb_div(n), snap_to_div(n), "division {i}");
        }
        let free = |n| AfterechoParams::feedback_time_seconds_mode(SyncMode::Free, n, 120.0);
        assert!((free(1.0) - 8.0).abs() < 1e-3);
        // The new top: 20 s free, 8 bars synced.
        assert!((free(FEEDBACK_TIME_MAX) - 20.0).abs() < 0.01);
        assert_eq!(fb_div(FEEDBACK_TIME_MAX).1, "8/1");
        assert_eq!(AfterechoParams::default().feedback_time.value(), 8.0 / 17.0);
    }

    #[test]
    fn repeats_read_as_a_count_or_the_loop() {
        let r = &AfterechoParams::default().repeats;
        assert_eq!(r.value(), REPEATS_DEFAULT);
        assert_eq!(r.normalized_value_to_string(1.0, false), "∞");
        assert_eq!(r.normalized_value_to_string(r.preview_normalized(4), false), "4");
        assert_eq!(r.string_to_normalized_value("∞"), Some(1.0));
    }
}

