use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
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

/// Per-tap filter slope. Each step roughly doubles the roll-off rate by
/// cascading more SVF stages: 12 dB = 1×, 24 dB = 2×, 48 dB = 4×.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FilterSlope {
    #[id = "db12"]
    Db12,
    #[id = "db24"]
    Db24,
    #[id = "db48"]
    Db48,
}

/// How the global Crossfeed knob blends between channels.
/// - Mix: post-tap stereo blend (good for centering / widening).
/// - PingPong: crossfeed lives in the feedback path → echoes alternate L↔R.
/// - Diffuse: feedback runs through a cascade of allpass filters
///   (ChowMatrix-style), smearing echoes into a reverb-like tail.
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

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CrossfeedMode {
    #[id = "mix"]
    Mix,
    #[id = "pingpong"]
    PingPong,
    #[id = "diffuse"]
    Diffuse,
}

#[derive(Params)]
pub struct PetalParams {
    /// Persists the editor window size across sessions.
    #[persist = "editor"]
    pub editor_state: Arc<EguiState>,

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
    /// Repitch / Fade / Jump — how the delay handles tap-time changes.
    #[id = "delay_mode"]
    pub delay_mode: EnumParam<DelayMode>,
    /// Global snap toggle for the Grid view horizontal drag.
    #[id = "tap_snap"]
    pub tap_snap: BoolParam,

    #[id = "channel_mode"]
    pub channel_mode: EnumParam<ChannelMode>,

    /// How many of the NUM_TAPS taps are "live". GUI hides the rest.
    #[id = "num_taps_active"]
    pub num_taps_active: IntParam,

    // ---- Per-tap (Layer 2) ----
    #[nested(array, group = "tap")]
    pub taps: [TapParams; NUM_TAPS],

    // ---- Feedback ----
    #[id = "feedback"]
    pub feedback: FloatParam,
    /// Normalised 0..1 feedback time. In Free mode maps to 10..8000 ms
    /// (exponential). In Sync mode snaps to the nearest SYNC_DIVS entry.
    /// Independent of the tap layout — you can space the echo cluster
    /// anywhere regardless of where the taps sit.
    #[id = "feedback_time"]
    pub feedback_time: FloatParam,
    /// Whether feedback_time uses the global tempo grid (Sync) or free ms (Free).
    #[id = "feedback_time_mode"]
    pub feedback_time_mode: EnumParam<SyncMode>,
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
    /// Wet-path allpass cascade depth [0, 1].  0 = bypass, 1 = full 64 stages.
    /// Smears each tap's transients into a diffuse, reverb-like bloom without
    /// altering the frequency response. Runs after the tap mix, before dry/wet.
    #[id = "diffuse_amount"]
    pub diffuse_amount: FloatParam,
    /// Where the DIFFUSE tab's effect is inserted into the signal path.
    #[id = "diffuse_routing"]
    pub diffuse_routing: EnumParam<DiffuseRouting>,
    /// Scales the Schroeder allpass stage delays. 1.0 = nominal 12–55 ms;
    /// 0.25 = tight & bright (less low-frequency phase smear → less "muffled");
    /// 2.0 = lush, plate-like.
    #[id = "diffuse_size"]
    pub diffuse_size: FloatParam,

    // ---- Filter ----
    #[id = "filter_type"]
    pub filter_type: EnumParam<FilterType>,
    #[id = "filter_cutoff"]
    pub filter_cutoff: FloatParam,
    #[id = "filter_q"]
    pub filter_q: FloatParam,

    // ---- Wobble (LFO) ----
    #[id = "wobble_rate"]
    pub wobble_rate: FloatParam,
    #[id = "wobble_amount"]
    pub wobble_amount: FloatParam,

    // ---- I/O ----
    #[id = "input_gain"]
    pub input_gain: FloatParam,
    #[id = "soft_clip"]
    pub soft_clip: BoolParam,
    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
    #[id = "output_gain"]
    pub output_gain: FloatParam,
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
}

impl Default for TapParams {
    fn default() -> Self {
        Self {
            active: BoolParam::new("Active", true),
            gain_db: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(10.0)),
            pan: FloatParam::new("Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
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
            .with_smoother(SmoothingStyle::Logarithmic(30.0)),
            filter_q: FloatParam::new(
                "Tap Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 16.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(30.0)),
            filter_slope: EnumParam::new("Tap Slope", FilterSlope::Db12),
        }
    }
}

impl Default for PetalParams {
    fn default() -> Self {
        let time_default = 0.25; // normalized → ~125 ms in Free mode, 1/4 in Sync mode
        Self {
            editor_state: EguiState::from_size(crate::gui::WINDOW_W, crate::gui::WINDOW_H),
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
            delay_mode: EnumParam::new("Delay Mode", DelayMode::Repitch),
            tap_snap: BoolParam::new("Snap", true),

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
            .with_smoother(SmoothingStyle::Linear(20.0)),
            // Default: Sync mode, 1/4 note (index 8 of 17 divs → 8/17 ≈ 0.47)
            feedback_time: FloatParam::new(
                "Feedback Time",
                8.0 / (SYNC_DIVS.len() - 1) as f32,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            feedback_time_mode: EnumParam::new("Feedback Time Mode", SyncMode::Sync),
            feedback_time_locked: BoolParam::new("Feedback Time Locked", true),

            crossfeed: FloatParam::new("Crossfeed", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            crossfeed_mode: EnumParam::new("Crossfeed Mode", CrossfeedMode::Mix),

            diffuse_amount: FloatParam::new(
                "Diffuse Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
            diffuse_routing: EnumParam::new("Diffuse Routing", DiffuseRouting::Output),
            diffuse_size: FloatParam::new(
                "Diffuse Size",
                1.0,
                FloatRange::Skewed { min: 0.25, max: 2.0, factor: FloatRange::skew_factor(-0.5) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),

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

            wobble_rate: FloatParam::new(
                "Wobble Rate",
                0.5,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Logarithmic(20.0)),
            wobble_amount: FloatParam::new(
                "Wobble Amount",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),

            input_gain: FloatParam::new(
                "Input Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 12.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            soft_clip: BoolParam::new("Soft Clip", false),
            dry_wet: FloatParam::new(
                "Dry/Wet",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_gain: FloatParam::new(
                "Output Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 12.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

// ─── Time conversion helpers ──────────────────────────────────────────────
impl PetalParams {
    /// Convert a base-time param value (0..1) to seconds, depending on sync mode and tempo.
    pub fn base_time_seconds(&self, normalized: f32, tempo_bpm: f32) -> f32 {
        match self.sync_mode.value() {
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

    /// Display string for a base-time param value, depending on sync mode.
    pub fn format_base_time(&self, normalized: f32, tempo_bpm: f32) -> String {
        match self.sync_mode.value() {
            SyncMode::Free => {
                let secs = self.base_time_seconds(normalized, tempo_bpm);
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

    /// Feedback delay time in seconds. Derives from `feedback_time` param
    /// (0..1 normalised) interpreted via `feedback_time_mode`.
    pub fn feedback_time_seconds(&self, tempo_bpm: f32) -> f32 {
        match self.feedback_time_mode.value() {
            SyncMode::Free => {
                // Exponential 10 ms..8000 ms — same feel as the base-time knob.
                let ms = 10.0_f32 * 800.0_f32.powf(self.feedback_time.value().clamp(0.0, 1.0));
                ms * 0.001
            }
            SyncMode::Sync => {
                let (beats, _) = snap_to_div(self.feedback_time.value());
                (60.0 / tempo_bpm) * beats
            }
        }
    }

    /// Human-readable feedback time string shown in the GUI footer.
    pub fn format_feedback_time(&self, tempo_bpm: f32) -> String {
        match self.feedback_time_mode.value() {
            SyncMode::Free => {
                let ms = self.feedback_time_seconds(tempo_bpm) * 1000.0;
                if ms < 100.0 { format!("{:.0}ms", ms) }
                else if ms < 1000.0 { format!("{:.0}ms", ms) }
                else { format!("{:.2}s", ms * 0.001) }
            }
            SyncMode::Sync => {
                let (_, label) = snap_to_div(self.feedback_time.value());
                label.to_string()
            }
        }
    }
}
