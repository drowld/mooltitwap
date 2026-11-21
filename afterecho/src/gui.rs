//! The editor's entry: the scope-first page (`gui_scope.rs`), its window
//! size, A/B compare, and the timeline maths the page draws from.
//! (v2.50: the v2 and v3 editors are gone.)

use nice_plug::prelude::*;
use nice_plug_egui::{create_egui_editor, EguiState};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::params::{AfterechoParams, SyncMode, NUM_TAPS};
use crate::topology::{self, compute_positions};
use crate::MAX_DELAY_SECONDS;

pub const WINDOW_W: u32 = 1366;
pub const WINDOW_H: u32 = 666;

/// A new instance's window: the size last picked in ☰ on this computer (the crate's
/// `Appearance::size`), else the design size. A saved project replaces it with its own
/// (`#[persist]`). On iOS the host sizes the view.
pub fn new_editor_state() -> std::sync::Arc<nice_plug_egui::EguiState> {
    let z = if cfg!(target_os = "ios") {
        None
    } else {
        softsystems_ui::Appearance::load(&crate::gui_scope::PRODUCT).size
    };
    let z = z.unwrap_or(1.0);
    let (w, h) = ((WINDOW_W as f32 * z).round() as u32, (WINDOW_H as f32 * z).round() as u32);
    nice_plug_egui::EguiState::from_size(w, h).with_host_resize()
}

fn read_tempo(tempo_atomic: &Arc<AtomicU32>) -> f32 {
    let t = f32::from_bits(tempo_atomic.load(Ordering::Relaxed));
    if t.is_finite() && t > 1.0 { t } else { 120.0 }
}

/// GUI-only state.
#[derive(Default)]
pub struct EditorState {
    /// A/B compare: two full param snapshots and which is live (0 = A).
    pub ab_slots: [Option<BTreeMap<String, f32>>; 2],
    pub ab_active: u8,
    /// The preset sheet is open.
    pub preset_open: bool,
    /// The page's own state.
    pub scope: crate::gui_scope::ScopeState,
}

/// A/B compare: save the live params into the current slot, then load the
/// target (its snapshot, or a copy of what's live on first use).
pub(crate) fn ab_switch(st: &mut EditorState, params: &Arc<AfterechoParams>, setter: &ParamSetter, target: u8) {
    if st.ab_active == target { return; }
    st.ab_slots[st.ab_active as usize] = Some(crate::presets::capture(params));
    if let Some(snap) = st.ab_slots[target as usize].clone() {
        crate::presets::apply(params, setter, &snap, crate::presets::VALUES_FORMAT_PLAIN, &mut crate::presets::Sent::default());
    } else {
        st.ab_slots[target as usize] = st.ab_slots[st.ab_active as usize].clone();
    }
    st.ab_active = target;
}

// ─── A tap's Gain on the field and its dot size: the level knob's own scale ──
// (−60…+12 dB straight in dB, 0 dB at 5/6; owner 2026-11-21, was a cube-root
// taper over −60…+6 with 0 dB at 75 %)
#[inline]
pub(crate) fn gain_db_to_pos(db: f32) -> f32 {
    softsystems_ui::knob::level::norm(db)
}
#[inline]
pub(crate) fn pos_to_gain_db(pos: f32) -> f32 {
    softsystems_ui::knob::level::db(pos)
}

pub fn create_editor(
    params: Arc<AfterechoParams>,
    egui_state: Arc<EguiState>,
    tempo_bpm_shared: Arc<AtomicU32>,
    licence: softsystems_licence::Licence,
) -> Option<Box<dyn Editor>> {
    let egui_state_size = egui_state.clone();
    create_egui_editor(
        egui_state,
        EditorState::default(),
        Default::default(),
        {
            let licence = licence.clone();
            move |ctx, _queue, _st| {
                // each time the window opens
                licence.check_editor_open();
                crate::ui::theme::set_touch(cfg!(target_os = "ios"));
                crate::ui::theme::install_fonts(ctx);
            }
        },
        move |ui, setter, _queue, st| {
            let tempo = read_tempo(&tempo_bpm_shared);
            crate::gui_scope::frame(ui, &params, setter, st, &egui_state_size, tempo, &licence);
        },
    )
}

// ─── Topology mirror (matches topology.rs / lib.rs exactly) ────────────────
const MAX_ECHO_CYCLES: usize = 64;
const ECHO_THRESHOLD: f32 = 0.001;

pub struct VizData {
    pub times_l: [f32; NUM_TAPS],
    pub times_r: [f32; NUM_TAPS],
    pub grid_range_secs: f32,
    pub times_l_grid: [f32; NUM_TAPS],
    pub grid_range_secs_taps_only: f32,
    /// Cluster span (NUM_TAPS × base time, L channel) in seconds — the unit
    /// `time_offset` is expressed in. Grid-view drags convert px → offset with it.
    pub span_l_secs: f32,
    /// Feedback cycle as a fraction of grid_range_secs (single value: the DSP
    /// has one fb read head shared by both channels).
    pub fb_cycle_norm: f32,
    pub n_echo_cycles: usize,
    /// Repeats as a count (v2.42), or None for ∞ (the loop).
    pub repeats: Option<usize>,
    pub feedback: f32,
    /// Cumulative echo level after k cycles ([0] = 1.0), following the FB
    /// CURVE law (Natural = feedbackᵏ; Bloom/Gate mirror lib.rs's
    /// level-dependent loop gain with the bar level as the env proxy) — so
    /// the drawn tail visibly changes shape with the curve chips.
    pub cycle_gains: [f32; MAX_ECHO_CYCLES + 1],
    /// Freeze holds the loop (v2.35): every repeat at full level, the cycle
    /// at the length the audio thread latched.
    pub frozen: bool,
}

// Pick the smallest "nice" x-axis range covering `needed_secs`. The old
// floors (2 bars / 2 s) meant short clusters never filled the view — now the
// range adapts down to 1 beat / 250 ms so the content always fills.
fn pick_nice_range(needed_secs: f32, sync_mode: SyncMode, tempo_bpm: f32) -> f32 {
    match sync_mode {
        SyncMode::Sync | SyncMode::Dotted | SyncMode::Triplet => {
            let beat_secs = 60.0 / tempo_bpm;
            let needed_beats = (needed_secs / beat_secs).max(1.0);
            let nice: &[f32] = &[
                1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0,
                64.0, 96.0, 128.0, 192.0, 256.0,
            ];
            nice.iter().copied().find(|&r| needed_beats <= r * 1.05).unwrap_or(256.0) * beat_secs
        }
        SyncMode::Free => {
            // Down to 20 ms, so a short pattern spreads across the view instead
            // of sitting in its first fifth (the floor was 250 ms).
            let nice_ms: &[f32] = &[
                20.0, 30.0, 40.0, 50.0, 60.0, 80.0, 100.0, 125.0, 150.0, 200.0,
                250.0, 375.0, 500.0, 750.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0,
                6000.0, 8000.0, 12000.0, 16000.0, 24000.0, 32000.0, 48000.0, 64000.0,
            ];
            let needed_ms = (needed_secs * 1000.0).max(20.0);
            nice_ms.iter().copied().find(|&r| needed_ms <= r * 1.05).unwrap_or(64000.0) * 0.001
        }
    }
}

/// `shown_cap` = how many echo cycles the x-range reserves room for. The v3
/// GRID/STEMS editing views pass 1 (only the first repeat is ghosted there —
/// reserving 4 squeezed the editable taps into the left edge).
pub(crate) fn compute_viz_data_ex(params: &AfterechoParams, tempo_bpm: f32, shown_cap: usize) -> VizData {
    let n_active = params.taps_heard();
    let pos_l = compute_positions(params.spacing(false), n_active);
    let pos_r = compute_positions(params.spacing(true), n_active);

    let time_l_norm = params.base_time_l.value();
    let time_r_norm = if params.time_linked.value() { time_l_norm } else { params.base_time_r.value() };
    let base_secs_l = params.base_time_seconds(time_l_norm, tempo_bpm);
    let base_secs_r = AfterechoParams::base_time_seconds_mode(params.sync_mode_r_eff(), time_r_norm, tempo_bpm);
    let span_l = NUM_TAPS as f32 * base_secs_l;
    let span_r = NUM_TAPS as f32 * base_secs_r;

    // Seconds-domain tap times via the SAME topology helpers the DSP uses —
    // time_offset is a fraction of the cluster span, not the display range.
    let mut t_l = [0.0f32; NUM_TAPS];
    let mut t_r = [0.0f32; NUM_TAPS];
    let mut in_range = [false; NUM_TAPS];
    for i in 0..NUM_TAPS {
        let off = params.taps[i].time_offset.value();
        t_l[i] = topology::tap_delay_secs(pos_l[i], off, span_l);
        t_r[i] = topology::tap_delay_secs(pos_r[i], off, span_r);
        in_range[i] = params.taps[i].active.value() && i < n_active
            && topology::tap_fits(t_l[i], t_r[i], MAX_DELAY_SECONDS);
    }

    let feedback = params.feedback.value();
    // Feedback cycle — mirrors lib.rs exactly: LOCKED = cluster span over
    // active, in-range taps (offsets included); UNLOCKED = free, 10 ms floor
    // only (v2.18.2: the cluster-span floor is gone — in-loop saturation now
    // bounds short-cycle feedback instead).
    let cluster_span_secs =
        topology::cluster_span_secs(&t_l, &t_r, &in_range, n_active, params.pattern_secs(base_secs_l, base_secs_r));
    // Frozen, the loop keeps the length it had when the hold began (the audio
    // thread reports it; until it does, the params' cycle stands in).
    let frozen = params.freeze.value();
    let held = f32::from_bits(params.held_cycle_secs.load(Ordering::Relaxed));
    let fb_cycle_secs = if frozen && held > 0.0 { held }
        else if params.feedback_time_locked.value() { cluster_span_secs }
        else {
            params.feedback_time_seconds(tempo_bpm)
                .clamp(topology::MIN_FB_SECS, MAX_DELAY_SECONDS * 0.95)
        };

    // Per-cycle cumulative levels following the FB CURVE (mirrors lib.rs's
    // constants: Bloom P=0.35 / clamp 0.5..1.15 / exp 0.4, Gate T=0.1). The
    // DSP's envelope follower tracks the real signal; here the previous
    // cycle's level stands in for it, which is exact for an isolated cluster.
    let fb_curve = params.fb_curve.value();
    let mut cycle_gains = [0.0f32; MAX_ECHO_CYCLES + 1];
    cycle_gains[0] = 1.0;
    let mut n_echo_cycles = 0usize;
    // a count of repeats (v2.42): that many, each `feedback` times the last,
    // then nothing (not capped at 0 dB: above 100 % they grow)
    let n_rep = params.repeats.value();
    let repeats = (n_rep < crate::params::REPEATS_LOOP).then_some(n_rep as usize);
    if frozen {
        cycle_gains = [1.0; MAX_ECHO_CYCLES + 1];
        n_echo_cycles = MAX_ECHO_CYCLES;
    } else if let Some(n) = repeats {
        if feedback >= 1e-3 && fb_cycle_secs >= 1e-6 {
            for k in 1..=n {
                cycle_gains[k] = feedback.powi(k as i32);
                if cycle_gains[k] < ECHO_THRESHOLD {
                    break;
                }
                n_echo_cycles = k;
            }
        }
    } else if feedback >= 1e-3 && fb_cycle_secs >= 1e-6 {
        let mut level = 1.0f32;
        for k in 1..=MAX_ECHO_CYCLES {
            let g = match fb_curve {
                crate::params::FbCurve::Natural => feedback,
                crate::params::FbCurve::Bloom => {
                    let bend = (0.35 / level.max(1e-4)).powf(0.4);
                    feedback * bend.clamp(0.5, 1.15)
                }
                crate::params::FbCurve::Gate => {
                    if level >= 0.1 { feedback }
                    else {
                        let t = level / 0.1;
                        feedback * t * t
                    }
                }
            };
            level = (level * g).min(1.0);
            cycle_gains[k] = level;
            if level < ECHO_THRESHOLD { break; }
            n_echo_cycles = k;
        }
    }

    let sync_mode = params.sync_mode.value();
    // Drawn extent: ALL taps up to n_active (muted taps still draw, dimmed),
    // offsets included — so offset taps don't pile up past the right edge.
    let max_drawn_secs = (0..n_active).map(|i| t_l[i].max(t_r[i])).fold(0.001f32, f32::max);
    // Show the cluster + a readable handful of repeats (not the full -60 dB tail).
    let shown = n_echo_cycles.min(shown_cap);
    // 1.10× headroom (was 2× — a v2.8 relic from before echoes explicitly
    // extended the range; it made 8 taps at 1/1 show a 16-bar axis).
    let grid_range_secs = pick_nice_range(max_drawn_secs * 1.10 + shown as f32 * fb_cycle_secs, sync_mode, tempo_bpm);
    let grid_range_secs_taps_only = pick_nice_range(max_drawn_secs * 1.10, sync_mode, tempo_bpm);

    let mut times_l = [0.0f32; NUM_TAPS];
    let mut times_r = [0.0f32; NUM_TAPS];
    let mut times_l_grid = [0.0f32; NUM_TAPS];
    for i in 0..NUM_TAPS {
        times_l[i] = (t_l[i] / grid_range_secs).clamp(0.0, 1.0);
        times_r[i] = (t_r[i] / grid_range_secs).clamp(0.0, 1.0);
        times_l_grid[i] = (t_l[i] / grid_range_secs_taps_only).clamp(0.0, 1.0);
    }

    VizData {
        times_l, times_r, grid_range_secs, times_l_grid, grid_range_secs_taps_only,
        span_l_secs: span_l,
        fb_cycle_norm: fb_cycle_secs / grid_range_secs,
        n_echo_cycles, repeats, feedback, cycle_gains,
        frozen,
    }
}
