use nih_plug::prelude::*;

pub const NUM_TAPS: usize = 16;

/// Beat divisions for sync mode (sorted shortest -> longest).
pub const SYNC_DIVS: &[(f32, &str)] = &[
    (1.0/32.0, "1/32"),
    (1.0/24.0, "1/16T"),
    (1.0/16.0, "1/16"),
    (1.0/16.0 * 1.5, "1/16."),
    (1.0/12.0, "1/8T"),
    (1.0/8.0,  "1/8"),
    (1.0/8.0 * 1.5, "1/8."),
    (1.0/6.0,  "1/4T"),
    (1.0/4.0,  "1/4"),
    (1.0/4.0 * 1.5, "1/4."),
    (1.0/3.0,  "1/2T"),
    (1.0/2.0,  "1/2"),
    (1.0/2.0 * 1.5, "1/2."),
    (1.0,      "1/1"),
    (1.5,      "1/1."),
    (2.0,      "2/1"),
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

#[derive(Params)]
pub struct PetalParams {
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
    #[id = "feedback_tap"]
    pub feedback_tap: IntParam,

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
    #[id = "pitch_semitones"]
    pub pitch_semitones: IntParam,
    #[id = "gain_db"]
    pub gain_db: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    /// Per-tap time offset around the topology-derived position, in fraction of
    /// the full delay span (−0.5 .. +0.5). Drag a Grid ball horizontally to set.
    #[id = "time_offset"]
    pub time_offset: FloatParam,
}

impl Default for TapParams {
    fn default() -> Self {
        Self {
            active: BoolParam::new("Active", true),
            pitch_semitones: IntParam::new("Pitch", 0, IntRange::Linear { min: -12, max: 12 }),
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
        }
    }
}

impl Default for PetalParams {
    fn default() -> Self {
        let time_default = 0.25; // normalized → ~125 ms in Free mode, 1/4 in Sync mode
        Self {
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
            feedback_tap: IntParam::new(
                "Feedback Size",
                8,
                IntRange::Linear { min: 1, max: NUM_TAPS as i32 },
            ),

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
}
