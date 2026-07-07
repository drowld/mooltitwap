//! Mooltitwap GUI — rebuilt on the shared Epoché UI kit (epoche-ui) so it
//! renders with the same widgets/theme as cumulus. Layout is ui-driven (every
//! epo widget allocates from the egui cursor); the custom delay visualizers
//! are painted into allocated rects using theme:: colors.

use nice_plug::prelude::*;
use nice_plug_egui::{create_egui_editor, EguiState};
use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use epoche_ui::epo::{self, Accent};
use epoche_ui::theme;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::params::{
    ChannelMode, CrossfeedMode, DelayMode, DiffuseRouting, FilterType, PetalParams, SpacingMode,
    SyncMode, NUM_TAPS,
};

/// Native design width; the zoom factor is window_width / BASE_W (kit contract).
pub const WINDOW_W: u32 = 1080;
pub const WINDOW_H: u32 = 636;
const BASE_W: f32 = 1080.0;
/// Shared card body height (logical px, pre-scale) so the three cards align.
const CARD_BODY_H: f32 = 486.0;

#[inline]
fn read_tempo(tempo_atomic: &Arc<AtomicU32>) -> f32 {
    let t = f32::from_bits(tempo_atomic.load(Ordering::Relaxed));
    if t.is_finite() && t > 1.0 { t } else { 120.0 }
}

// ─── Editor state (GUI-only) ─────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VizMode { Stems, Grid }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RightTab { Pan, Gain, Filter, XFeed, Diffuse }

pub struct EditorState {
    pub viz_mode: VizMode,
    pub right_tab: RightTab,
    /// Tap currently being dragged in the Grid view (None = idle).
    pub drag_tap: Option<usize>,
}
impl Default for EditorState {
    fn default() -> Self {
        Self { viz_mode: VizMode::Stems, right_tab: RightTab::Gain, drag_tap: None }
    }
}

// ─── Audio-taper gain mapping (cube-root: 75% pos = 0 dB) ────────────────
const GAIN_SKEW: f32 = 0.333;
#[inline]
fn gain_db_to_pos(db: f32) -> f32 {
    ((db + 60.0) / 66.0).clamp(0.0, 1.0).powf(1.0 / GAIN_SKEW)
}
#[inline]
fn pos_to_gain_db(pos: f32) -> f32 {
    -60.0 + pos.clamp(0.0, 1.0).powf(GAIN_SKEW) * 66.0
}

/// Lighten a color toward white by `amt` (0 = unchanged, 1 = white).
fn lighten(c: Color32, amt: f32) -> Color32 {
    let f = |v: u8| (v as f32 + (255.0 - v as f32) * amt).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Identity hue for each right-panel tab (the Inertia accent ring).
fn tab_hue(tab: RightTab) -> Color32 {
    theme::accent(match tab {
        RightTab::Pan => 0,     // teal
        RightTab::Gain => 4,    // green
        RightTab::Filter => 3,  // amber
        RightTab::XFeed => 1,   // coral
        RightTab::Diffuse => 2, // violet
    })
}

/// One color per tab, gradient bright (tap 0) → light (tap n-1).
fn tab_grad(tab: RightTab, i: usize, n_active: usize) -> Color32 {
    let t = if n_active <= 1 { 0.0 } else { i as f32 / (n_active - 1) as f32 };
    lighten(tab_hue(tab), t * 0.6)
}

/// Visualizer per-tap gradient — mooltitwap's violet signature, bright → light.
fn tap_color(i: usize, n_active: usize) -> Color32 {
    let t = if n_active <= 1 { 0.0 } else { i as f32 / (n_active - 1) as f32 };
    lighten(theme::accent(2), t * 0.55)
}

// ─── Editor entry ────────────────────────────────────────────────────────
pub fn create_editor(
    params: Arc<PetalParams>,
    egui_state: Arc<EguiState>,
    tempo_bpm_shared: Arc<AtomicU32>,
) -> Option<Box<dyn Editor>> {
    let egui_state_size = egui_state.clone();
    create_egui_editor(
        egui_state,
        EditorState::default(),
        Default::default(),
        |ctx, _queue, _st| {
            theme::set_touch(false);
            theme::set_theme(true); // dark
            theme::install_fonts(ctx);
        },
        move |ui, setter, _queue, st| {
            // Snap the persisted window height to the design ratio (keeps the
            // content-fit layout correct after older sessions / width changes).
            let (cur_w, cur_h) = egui_state_size.size();
            let want_h = (cur_w as f32 * WINDOW_H as f32 / WINDOW_W as f32).round() as u32;
            if cur_h.abs_diff(want_h) > 2 {
                egui_state_size.set_requested_size((cur_w, want_h));
            }
            theme::apply_zoom(ui.ctx(), (cur_w as f32 / BASE_W).clamp(0.5, 2.5));
            let tempo = read_tempo(&tempo_bpm_shared);
            draw_plugin(ui, &params, setter, st, tempo);
        },
    )
}

// ─── Top-level layout ──────────────────────────────────────────────────────
fn draw_plugin(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    tempo: f32,
) {
    let s = theme::ui_scale(ui);
    ui.painter().rect_filled(ui.max_rect(), 0.0, theme::PAPER());

    let margin = theme::SP_XL * s;
    draw_header(ui, params, s);
    ui.add_space(theme::SP_MD * s);

    let avail = ui.available_width() - 2.0 * margin;
    let gap = theme::CARD_GAP * s;
    let left_w = 240.0 * s;
    let right_w = 300.0 * s;
    let center_w = (avail - left_w - right_w - 2.0 * gap).max(200.0 * s);
    let body_h = CARD_BODY_H * s;

    // Read mutable state out of `st` into locals so each card body closure
    // captures only what it needs (avoids nested &mut st borrows).
    let mut viz_mode = st.viz_mode;
    let mut right_tab = st.right_tab;
    let mut drag_tap = st.drag_tap;

    ui.horizontal_top(|ui| {
        ui.add_space(margin);
        ui.spacing_mut().item_spacing.x = gap;

        epo::card(ui, "DELAY", theme::INK2(), left_w, &mut |ui| {
            ui.set_min_height(body_h);
            draw_left_body(ui, params, setter, tempo);
        });

        let (vc, _) = epo::card_tabs(
            ui, "VIEW", &["STEMS", "GRID"],
            if viz_mode == VizMode::Stems { 0 } else { 1 },
            center_w, body_h,
            &mut |ui| draw_center_body(ui, params, setter, viz_mode, tempo, &mut drag_tap),
            None,
        );
        if let Some(i) = vc { viz_mode = if i == 0 { VizMode::Stems } else { VizMode::Grid }; }

        let tabs = ["PAN", "GAIN", "FILTER", "XFEED", "DIFFUSE"];
        let active_tab = match right_tab {
            RightTab::Pan => 0, RightTab::Gain => 1, RightTab::Filter => 2,
            RightTab::XFeed => 3, RightTab::Diffuse => 4,
        };
        let (rc, _) = epo::card_tabs(
            ui, "TAPS", &tabs, active_tab, right_w, body_h,
            &mut |ui| draw_right_body(ui, params, setter, right_tab),
            None,
        );
        if let Some(i) = rc {
            right_tab = [RightTab::Pan, RightTab::Gain, RightTab::Filter,
                         RightTab::XFeed, RightTab::Diffuse][i];
        }
    });

    st.drag_tap = drag_tap;
    st.viz_mode = viz_mode;
    st.right_tab = right_tab;

    ui.add_space(theme::SP_MD * s);
    draw_footer(ui, params, setter, tempo, margin, s);
}

// ─── Header ──────────────────────────────────────────────────────────────
fn draw_header(ui: &mut egui::Ui, _params: &Arc<PetalParams>, s: f32) {
    ui.add_space(theme::SP_SM * s);
    ui.horizontal(|ui| {
        ui.add_space(theme::SP_LG * s);
        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
        let _ = epo::menu_button(ui);
        ui.label(RichText::new("mooltitwap").size(19.0 * s).color(theme::INK()).strong());
        ui.label(RichText::new("·").size(19.0 * s).color(theme::accent(2)));
        ui.label(
            RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                .family(FontFamily::Monospace).size(11.0 * s).color(theme::SOFT()),
        );
        // centered preset pill
        let pill_w = theme::PILL_MIN_W * s;
        let pad = ((ui.available_width() - pill_w) * 0.5 - theme::SP_LG * s).max(0.0);
        ui.add_space(pad);
        let _ = epo::preset_slot(ui, "DEFAULT", theme::PILL_MIN_W, theme::TBBTN_H, "preset");
    });
    ui.add_space(theme::SP_XS * s);
    let r = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [Pos2::new(r.left(), y), Pos2::new(r.right(), y)],
        Stroke::new(1.0 * s, theme::line()),
    );
}

// ─── LEFT card body ────────────────────────────────────────────────────────
fn draw_left_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, tempo: f32) {
    let s = theme::ui_scale(ui);
    let kd = theme::KNOB_DIAM_FX;

    // Hero: TIME L / link / TIME R (tempo-aware value strings via knob_ex `disp`)
    let l_str = params.format_base_time(params.base_time_l.value(), tempo);
    let r_val = if params.time_linked.value() { params.base_time_l.value() } else { params.base_time_r.value() };
    let r_str = params.format_base_time(r_val, tempo);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_MD * s;
        epo::knob_ex(ui, setter, &params.base_time_l, "TIME L", kd, Accent::Ink, false, None, Some(&l_str));
        epo::toggle(ui, "LINK", &params.time_linked, setter);
        if params.time_linked.value() {
            epo::knob_ex(ui, setter, &params.base_time_l, "TIME R", kd, Accent::Ink, false, None, Some(&r_str));
        } else {
            epo::knob_ex(ui, setter, &params.base_time_r, "TIME R", kd, Accent::Ink, false, None, Some(&r_str));
        }
    });
    ui.add_space(theme::SP_MD * s);

    // SYNC
    section_label(ui, "SYNC", s);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
        let cur = params.sync_mode.value();
        if epo::chip(ui, "FREE", cur == SyncMode::Free) { set_enum(setter, &params.sync_mode, SyncMode::Free); }
        if epo::chip(ui, "SYNC", cur == SyncMode::Sync) { set_enum(setter, &params.sync_mode, SyncMode::Sync); }
    });
    ui.add_space(theme::SP_SM * s);

    // DELAY MODE
    section_label(ui, "DELAY MODE", s);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
        let cur = params.delay_mode.value();
        for (lab, m) in [("REPITCH", DelayMode::Repitch), ("FADE", DelayMode::Fade), ("JUMP", DelayMode::Jump)] {
            if epo::chip(ui, lab, cur == m) { set_enum(setter, &params.delay_mode, m); }
        }
    });
    ui.add_space(theme::SP_SM * s);

    // CHANNEL MODE
    section_label(ui, "CHANNEL", s);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
        let cur = params.channel_mode.value();
        for (lab, m) in [("STEREO", ChannelMode::Stereo), ("M/S", ChannelMode::MidSide), ("MONO", ChannelMode::Mono)] {
            if epo::chip(ui, lab, cur == m) { set_enum(setter, &params.channel_mode, m); }
        }
    });
    ui.add_space(theme::SP_MD * s);

    // LEVELS
    section_label(ui, "LEVELS", s);
    let mix_str = format!("{:.0}%", params.dry_wet.value() * 100.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
        epo::knob(ui, setter, &params.input_gain, "IN", kd, Accent::Ink, false);
        epo::knob_ex(ui, setter, &params.dry_wet, "MIX", kd, Accent::Ink, false, None, Some(&mix_str));
        epo::knob(ui, setter, &params.output_gain, "OUT", kd, Accent::Ink, false);
    });
    ui.add_space(theme::SP_MD * s);

    // WOBBLE
    section_label(ui, "WOBBLE", s);
    let amt_str = format!("{:.0}%", params.wobble_amount.value() * 100.0);
    let rate_str = format!("{:.2} Hz", params.wobble_rate.value());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
        epo::knob_ex(ui, setter, &params.wobble_amount, "AMOUNT", kd, Accent::Ink, false, None, Some(&amt_str));
        epo::knob_ex(ui, setter, &params.wobble_rate, "RATE", kd, Accent::Ink, false, None, Some(&rate_str));
    });
    ui.add_space(theme::SP_MD * s);

    epo::toggle(ui, "SOFT CLIP", &params.soft_clip, setter);
}

fn section_label(ui: &mut egui::Ui, text: &str, s: f32) {
    ui.label(RichText::new(text).size(theme::FS_LABEL * s).color(theme::FAINT()).strong());
    ui.add_space(theme::SP_XS * s);
}

// ─── CENTER card body — the delay visualizer (stems / grid) ────────────────
fn draw_center_body(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    viz_mode: VizMode,
    tempo: f32,
    drag_tap: &mut Option<usize>,
) {
    let s = theme::ui_scale(ui);
    let viz = compute_viz_data(params, tempo);
    let is_sync = params.sync_mode.value() == SyncMode::Sync;
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

    let w = ui.available_width();
    let h = 300.0 * s;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter().clone();
    // kit `.viz` well: INSET fill + strong hairline border.
    p.rect_filled(rect, 13.0 * s, theme::INSET());
    p.rect_stroke(rect, 13.0 * s, Stroke::new(1.0 * s, theme::line2()), StrokeKind::Inside);

    // corner caption
    let ch_label = match params.channel_mode.value() {
        ChannelMode::Stereo => "STEREO", ChannelMode::MidSide => "MID/SIDE", ChannelMode::Mono => "MONO",
    };
    let len_str = match params.sync_mode.value() {
        SyncMode::Sync => {
            let beats = viz.grid_range_secs * tempo / 60.0;
            if beats == beats.floor() { format!("{:.0}/1", beats) } else { format!("{:.1} beat", beats) }
        }
        SyncMode::Free => {
            if viz.grid_range_secs < 1.0 { format!("{:.0} ms", viz.grid_range_secs * 1000.0) }
            else { format!("{:.2} s", viz.grid_range_secs) }
        }
    };
    p.text(Pos2::new(rect.max.x - 13.0 * s, rect.min.y + 9.0 * s), Align2::RIGHT_TOP,
           &format!("{} · TAPS {} · {}", ch_label, n_active, len_str),
           FontId::new(9.0 * s, FontFamily::Monospace), theme::SOFT());

    match viz_mode {
        VizMode::Stems => draw_stems(&p, rect, params, &viz, tempo, is_sync, s),
        VizMode::Grid => draw_grid_viz(ui, rect, params, setter, &viz, tempo, is_sync, drag_tap, s),
    }

    // ── Shaping strip: two XY pads (per-channel topology shaping) ──────────
    ui.add_space(theme::SP_SM * s);
    let strip_h = (ui.available_height() - theme::SP_SM * s).max(96.0 * s);
    let (strip, _) = ui.allocate_exact_size(Vec2::new(w, strip_h), Sense::hover());
    let pad_gap = theme::SP_SM * s;
    let pad_w = (strip.width() - pad_gap) * 0.5;
    let l_rect = Rect::from_min_size(strip.min, Vec2::new(pad_w, strip_h));
    let r_rect = Rect::from_min_size(Pos2::new(strip.min.x + pad_w + pad_gap, strip.min.y), Vec2::new(pad_w, strip_h));
    let linked = params.shape_linked.value();
    draw_xy_pad(ui, l_rect, "shape_l", "SHAPE L", &params.shape_x_l, &params.shape_y_l, setter, false, n_active, s);
    let (xr, yr) = if linked { (&params.shape_x_l, &params.shape_y_l) } else { (&params.shape_x_r, &params.shape_y_r) };
    draw_xy_pad(ui, r_rect, "shape_r", "SHAPE R", xr, yr, setter, linked, n_active, s);
}

// ─── XY shaping pad (rect-driven; interacts via ui.interact so it composes
// inside an already-allocated strip). ──────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn draw_xy_pad(ui: &mut egui::Ui, rect: Rect, salt: &str, label: &str,
               x_param: &FloatParam, y_param: &FloatParam, setter: &ParamSetter,
               dim: bool, n_active: usize, s: f32) {
    let alpha = if dim { 0.4 } else { 1.0 };
    let sense = if dim { Sense::hover() } else { Sense::click_and_drag() };
    let resp = ui.interact(rect, ui.id().with(salt), sense);
    let p = ui.painter();
    p.rect_filled(rect, 8.0 * s, theme::INSET());
    p.rect_stroke(rect, 8.0 * s, Stroke::new(1.0 * s, theme::line2()), StrokeKind::Inside);
    for i in 1..=2 {
        let x = rect.min.x + i as f32 * rect.width() / 3.0;
        dashed_v(p, Pos2::new(x, rect.min.y + 4.0 * s), rect.height() - 8.0 * s, theme::line(), s);
        let y = rect.min.y + i as f32 * rect.height() / 3.0;
        dashed_h(p, Pos2::new(rect.min.x + 4.0 * s, y), rect.width() - 8.0 * s, theme::line(), s);
    }

    let x_norm = (x_param.value() + 1.0) * 0.5;
    let y_norm = y_param.value();
    if !dim {
        if resp.drag_started() { setter.begin_set_parameter(x_param); setter.begin_set_parameter(y_param); }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let nx = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                let ny = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
                setter.set_parameter(x_param, nx * 2.0 - 1.0);
                setter.set_parameter(y_param, 1.0 - ny);
            }
        }
        if resp.drag_stopped() { setter.end_set_parameter(x_param); setter.end_set_parameter(y_param); }
        if resp.double_clicked() {
            for pp in [x_param, y_param] {
                setter.begin_set_parameter(pp);
                setter.set_parameter_normalized(pp, pp.default_normalized_value());
                setter.end_set_parameter(pp);
            }
        }
    }

    let cx = rect.min.x + x_norm * rect.width();
    let cy = rect.min.y + (1.0 - y_norm) * rect.height();
    let accent = theme::accent(2).linear_multiply(alpha);
    p.line_segment([Pos2::new(rect.min.x, cy), Pos2::new(rect.max.x, cy)], Stroke::new(0.6 * s, accent.linear_multiply(0.4)));
    p.line_segment([Pos2::new(cx, rect.min.y), Pos2::new(cx, rect.max.y)], Stroke::new(0.6 * s, accent.linear_multiply(0.4)));
    p.circle_filled(Pos2::new(cx, cy), 10.0 * s, accent.linear_multiply(0.25));
    p.circle_filled(Pos2::new(cx, cy), 5.0 * s, accent);
    p.text(Pos2::new(rect.min.x + 8.0 * s, rect.min.y + 6.0 * s), Align2::LEFT_TOP, label,
           FontId::new(8.5 * s, FontFamily::Proportional), theme::FAINT());
    p.text(Pos2::new(rect.min.x + 8.0 * s, rect.max.y - 4.0 * s), Align2::LEFT_BOTTOM, "← tap 1",
           FontId::new(8.0 * s, FontFamily::Monospace), theme::FAINT());
    p.text(Pos2::new(rect.max.x - 8.0 * s, rect.max.y - 4.0 * s), Align2::RIGHT_BOTTOM,
           &format!("tap {} →", n_active), FontId::new(8.0 * s, FontFamily::Monospace), theme::FAINT());
}

// ─── Topology mirror (matches topology.rs / lib.rs exactly) ────────────────
fn compute_positions(spacing: SpacingMode, shape_x: f32, shape_y: f32, n_active: usize) -> [f32; NUM_TAPS] {
    let n_active = n_active.clamp(1, NUM_TAPS);
    let first = 1.0 / NUM_TAPS as f32;
    let last = n_active as f32 / NUM_TAPS as f32;
    let mut out = [0.0; NUM_TAPS];
    for i in 0..NUM_TAPS {
        if i < n_active {
            let t = if n_active == 1 { 0.0 } else { i as f32 / (n_active - 1) as f32 };
            let base = match spacing {
                SpacingMode::Linear => t,
                SpacingMode::Exponential => t.powf(2.0).min(1.0),
                SpacingMode::Logarithmic => t.sqrt(),
                SpacingMode::Euclidean => t,
            };
            out[i] = first + bias_curve(sigmoid_blend(base, shape_y), shape_x) * (last - first);
        } else {
            out[i] = (i + 1) as f32 / NUM_TAPS as f32;
        }
    }
    out
}
fn sigmoid_blend(x: f32, y: f32) -> f32 {
    if y <= 0.0 { return x; }
    let sm = x * x * (3.0 - 2.0 * x);
    x * (1.0 - y) + sm * y
}
fn bias_curve(v: f32, x: f32) -> f32 {
    if x.abs() < 1e-6 { return v; }
    let power = if x >= 0.0 { 1.0 / (1.0 + 2.0 * x) } else { 1.0 + 2.0 * (-x) };
    v.powf(power)
}

const MAX_ECHO_CYCLES: usize = 64;
const ECHO_THRESHOLD: f32 = 0.001;

pub struct VizData {
    pub times_l: [f32; NUM_TAPS],
    pub times_r: [f32; NUM_TAPS],
    pub grid_range_secs: f32,
    pub times_l_grid: [f32; NUM_TAPS],
    pub grid_range_secs_taps_only: f32,
    pub fb_cycle_norm_l: f32,
    pub fb_cycle_norm_r: f32,
    pub n_echo_cycles: usize,
    pub feedback: f32,
    pub ping_pong: bool,
}

fn pick_nice_range(needed_secs: f32, sync_mode: SyncMode, tempo_bpm: f32) -> f32 {
    match sync_mode {
        SyncMode::Sync => {
            let beat_secs = 60.0 / tempo_bpm;
            let needed_beats = (needed_secs / beat_secs).max(8.0);
            let nice: &[f32] = &[8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0, 192.0, 256.0];
            nice.iter().copied().find(|&r| needed_beats <= r * 1.05).unwrap_or(256.0) * beat_secs
        }
        SyncMode::Free => {
            let nice_ms: &[f32] = &[2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 12000.0, 16000.0, 24000.0, 32000.0, 48000.0, 64000.0];
            let needed_ms = (needed_secs * 1000.0).max(2000.0);
            nice_ms.iter().copied().find(|&r| needed_ms <= r * 1.05).unwrap_or(64000.0) * 0.001
        }
    }
}

fn compute_viz_data(params: &PetalParams, tempo_bpm: f32) -> VizData {
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let pos_l = compute_positions(params.spacing_mode.value(), params.shape_x_l.value(), params.shape_y_l.value(), n_active);
    let pos_r = if params.shape_linked.value() { pos_l }
        else { compute_positions(params.spacing_mode.value(), params.shape_x_r.value(), params.shape_y_r.value(), n_active) };

    let time_l_norm = params.base_time_l.value();
    let time_r_norm = if params.time_linked.value() { time_l_norm } else { params.base_time_r.value() };
    let base_secs_l = params.base_time_seconds(time_l_norm, tempo_bpm);
    let base_secs_r = params.base_time_seconds(time_r_norm, tempo_bpm);
    let span_l = NUM_TAPS as f32 * base_secs_l;
    let span_r = NUM_TAPS as f32 * base_secs_r;
    let max_tap_secs = (pos_l[n_active - 1] * span_l).max(pos_r[n_active - 1] * span_r).max(0.001);

    let feedback = params.feedback.value();
    let cluster_span_secs = max_tap_secs;
    let fb_cycle_secs = if params.feedback_time_locked.value() { cluster_span_secs }
        else { params.feedback_time_seconds(tempo_bpm).max(cluster_span_secs) }.max(1e-6);

    let n_echo_cycles = if feedback < 1e-3 || fb_cycle_secs < 1e-6 { 0 }
        else if feedback >= 0.999 { MAX_ECHO_CYCLES }
        else { ((ECHO_THRESHOLD.ln() / feedback.ln()).ceil() as usize).min(MAX_ECHO_CYCLES) };

    let sync_mode = params.sync_mode.value();
    // Show the cluster + a readable handful of repeats (not the full -60 dB tail).
    const SHOWN_ECHO_CYCLES: usize = 4;
    let shown = n_echo_cycles.min(SHOWN_ECHO_CYCLES);
    let grid_range_secs = pick_nice_range(max_tap_secs * 2.0 + shown as f32 * fb_cycle_secs, sync_mode, tempo_bpm);
    let grid_range_secs_taps_only = pick_nice_range(max_tap_secs * 1.10, sync_mode, tempo_bpm);

    let mut times_l = [0.0f32; NUM_TAPS];
    let mut times_r = [0.0f32; NUM_TAPS];
    let mut times_l_grid = [0.0f32; NUM_TAPS];
    for i in 0..NUM_TAPS {
        let offset = params.taps[i].time_offset.value();
        times_l[i] = ((pos_l[i] * span_l / grid_range_secs) + offset).clamp(0.0, 1.0);
        times_r[i] = ((pos_r[i] * span_r / grid_range_secs) + offset).clamp(0.0, 1.0);
        times_l_grid[i] = ((pos_l[i] * span_l / grid_range_secs_taps_only) + offset).clamp(0.0, 1.0);
    }

    let ping_pong = matches!(params.crossfeed_mode.value(), crate::params::CrossfeedMode::PingPong);
    VizData {
        times_l, times_r, grid_range_secs, times_l_grid, grid_range_secs_taps_only,
        fb_cycle_norm_l: fb_cycle_secs / grid_range_secs,
        fb_cycle_norm_r: fb_cycle_secs / grid_range_secs,
        n_echo_cycles, feedback, ping_pong,
    }
}

// ─── Timeline grid (beats in Sync, ms in Free) ─────────────────────────────
#[allow(clippy::too_many_arguments)]
fn paint_timeline_grid(
    p: &egui::Painter, draw_l: f32, draw_r: f32, draw_t: f32, draw_b: f32,
    grid_range_secs: f32, tempo_bpm: f32, is_sync: bool, label_y: f32, s: f32,
) {
    let draw_w = draw_r - draw_l;
    if is_sync {
        let beat_secs = 60.0 / tempo_bpm;
        let total_beats = (grid_range_secs / beat_secs).max(0.001);
        let (major, minor) = if total_beats <= 0.5 { (0.125_f32, 0.0625_f32) }
            else if total_beats <= 1.0 { (0.25, 0.125) } else if total_beats <= 2.0 { (0.5, 0.25) }
            else if total_beats <= 4.0 { (1.0, 0.5) } else if total_beats <= 8.0 { (2.0, 1.0) }
            else if total_beats <= 16.0 { (4.0, 2.0) } else if total_beats <= 32.0 { (8.0, 4.0) }
            else if total_beats <= 64.0 { (16.0, 8.0) } else if total_beats <= 128.0 { (32.0, 16.0) }
            else { (64.0, 32.0) };
        let mut beat = 0.0_f32;
        let eps = minor * 0.01;
        while beat <= total_beats + eps {
            let x = draw_l + (beat / total_beats) * draw_w;
            let is_major = (beat / major - (beat / major).round()).abs() < 0.02;
            let (sw, col) = if is_major { (1.0 * s, theme::line2()) } else { (0.5 * s, theme::line()) };
            p.line_segment([Pos2::new(x, draw_t), Pos2::new(x, draw_b)], Stroke::new(sw, col));
            if is_major {
                let rounded = (beat * 100.0).round() / 100.0;
                let label = if rounded == 0.0 { "0".to_string() }
                    else if rounded == rounded.floor() { format!("{:.0}", rounded) }
                    else if (rounded * 2.0).round() == rounded * 2.0 { format!("{:.1}", rounded) }
                    else { format!("{:.2}", rounded) };
                p.text(Pos2::new(x, label_y), Align2::CENTER_BOTTOM, &label,
                       FontId::new(9.0 * s, FontFamily::Monospace), theme::FAINT());
            }
            beat += minor;
        }
    } else {
        let total_ms = grid_range_secs * 1000.0;
        let (major, minor) = if total_ms <= 250.0 { (50.0_f32, 25.0_f32) }
            else if total_ms <= 500.0 { (100.0, 50.0) } else if total_ms <= 1000.0 { (200.0, 100.0) }
            else if total_ms <= 2000.0 { (500.0, 250.0) } else if total_ms <= 5000.0 { (1000.0, 500.0) }
            else if total_ms <= 10000.0 { (2000.0, 1000.0) } else { (4000.0, 2000.0) };
        let mut ms = 0.0_f32;
        let eps = minor * 0.01;
        while ms <= total_ms + eps {
            let x = draw_l + (ms / total_ms) * draw_w;
            let is_major = (ms / major - (ms / major).round()).abs() < 0.02;
            let (sw, col) = if is_major { (1.0 * s, theme::line2()) } else { (0.5 * s, theme::line()) };
            p.line_segment([Pos2::new(x, draw_t), Pos2::new(x, draw_b)], Stroke::new(sw, col));
            if is_major {
                let label = if ms < 1.0 { "0".to_string() }
                    else if ms < 1000.0 { format!("{:.0}", ms) } else { format!("{:.1}s", ms * 0.001) };
                p.text(Pos2::new(x, label_y), Align2::CENTER_BOTTOM, &label,
                       FontId::new(9.0 * s, FontFamily::Monospace), theme::FAINT());
            }
            ms += minor;
        }
    }
}

fn dashed_h(p: &egui::Painter, start: Pos2, width: f32, color: Color32, s: f32) {
    let (dash, gap) = (4.0 * s, 4.0 * s);
    let mut x = start.x;
    while x < start.x + width {
        let x2 = (x + dash).min(start.x + width);
        p.line_segment([Pos2::new(x, start.y), Pos2::new(x2, start.y)], Stroke::new(0.8 * s, color));
        x += dash + gap;
    }
}
fn dashed_v(p: &egui::Painter, start: Pos2, height: f32, color: Color32, s: f32) {
    let (dash, gap) = (3.0 * s, 4.0 * s);
    let mut y = start.y;
    while y < start.y + height {
        let y2 = (y + dash).min(start.y + height);
        p.line_segment([Pos2::new(start.x, y), Pos2::new(start.x, y2)], Stroke::new(0.4 * s, color));
        y += dash + gap;
    }
}

// ─── STEMS visualizer ──────────────────────────────────────────────────────
fn draw_stems(p: &egui::Painter, rect: Rect, params: &Arc<PetalParams>, viz: &VizData, tempo_bpm: f32, is_sync: bool, s: f32) {
    let (pad_l, pad_r, pad_t, pad_b) = (36.0 * s, 24.0 * s, 18.0 * s, 24.0 * s);
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let mid_y = (draw_t + draw_b) * 0.5;
    let max_bar = (draw_b - draw_t) * 0.5 - 10.0 * s;
    let draw_w = draw_r - draw_l;
    let ch_mode = params.channel_mode.value();

    let (top_lab, bot_lab) = match ch_mode {
        ChannelMode::Stereo => ("L", "R"), ChannelMode::MidSide => ("MID", "SIDE"), ChannelMode::Mono => ("M", "M"),
    };
    p.text(Pos2::new(rect.min.x + 10.0 * s, draw_t + 4.0 * s), Align2::LEFT_TOP, top_lab,
           FontId::new(9.0 * s, FontFamily::Proportional), theme::FAINT());
    p.text(Pos2::new(rect.min.x + 10.0 * s, draw_b - 4.0 * s), Align2::LEFT_BOTTOM, bot_lab,
           FontId::new(9.0 * s, FontFamily::Proportional), theme::FAINT());

    dashed_h(p, Pos2::new(draw_l, mid_y), draw_w, theme::WAVE(), s);
    paint_timeline_grid(p, draw_l, draw_r, draw_t, draw_b, viz.grid_range_secs, tempo_bpm, is_sync, rect.max.y - 6.0 * s, s);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let bar_w = {
        let mut min_gap_px = draw_w;
        let mut prev_x: Option<f32> = None;
        for i in 0..n_active {
            if !params.taps[i].active.value() { continue; }
            let x = draw_l + viz.times_l[i].min(1.0) * draw_w;
            if let Some(px) = prev_x { let g = (x - px).abs(); if g > 0.5 { min_gap_px = min_gap_px.min(g); } }
            prev_x = Some(x);
        }
        (min_gap_px * 0.55).clamp(0.5, (4.0 * s).max(0.5))
    };

    for k in 0..=viz.n_echo_cycles {
        let cycle_gain = if k == 0 { 1.0 } else { viz.feedback.powi(k as i32) };
        if cycle_gain < ECHO_THRESHOLD { break; }
        let swap_lr = viz.ping_pong && (k % 2 == 1);
        for i in 0..n_active {
            let tap = &params.taps[i];
            let active = tap.active.value();
            let height_scale = if k == 0 { 1.0 } else { cycle_gain.cbrt() };
            let opacity_scale = if k == 0 { 1.0 } else { cycle_gain.sqrt().max(0.18) };
            let op = if active { opacity_scale } else { 0.22 };
            let gain_n = (gain_db_to_pos(tap.gain_db.value()) * height_scale).clamp(0.0, 1.0);
            let raw_pan = tap.pan.value();
            let pan = if swap_lr { -raw_pan } else { raw_pan };
            let t_l = viz.times_l[i] + k as f32 * viz.fb_cycle_norm_l;
            let t_r = viz.times_r[i] + k as f32 * viz.fb_cycle_norm_r;
            if t_l > 1.0 && t_r > 1.0 { continue; }
            let (t_a, t_b) = if swap_lr { (t_r, t_l) } else { (t_l, t_r) };
            let x_l = draw_l + t_a.min(1.0) * draw_w;
            let x_r = draw_l + t_b.min(1.0) * draw_w;
            let (in_l, in_r) = (t_a <= 1.0, t_b <= 1.0);
            let col = tap_color(i, n_active).linear_multiply(op);
            let bw = if k == 0 { bar_w } else { (bar_w * 0.7).max(0.5) };
            match ch_mode {
                ChannelMode::Stereo => {
                    let l_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).cos();
                    let r_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).sin();
                    let (h_l, h_r) = (gain_n * l_w * max_bar, gain_n * r_w * max_bar);
                    if in_l && h_l > 0.5 { p.line_segment([Pos2::new(x_l, mid_y), Pos2::new(x_l, mid_y - h_l)], Stroke::new(bw, col)); }
                    if in_r && h_r > 0.5 { p.line_segment([Pos2::new(x_r, mid_y), Pos2::new(x_r, mid_y + h_r)], Stroke::new(bw, col)); }
                }
                ChannelMode::MidSide => {
                    let mid_amt = ((1.0 - pan) * 0.5).clamp(0.0, 1.0);
                    let side_amt = ((1.0 + pan) * 0.5).clamp(0.0, 1.0);
                    let mid_h = gain_n * max_bar * 0.5 * mid_amt;
                    let side_h = gain_n * max_bar * 0.35 * side_amt;
                    let half_w = (bw * 0.5).max(0.5);
                    if in_l && mid_h > 0.5 {
                        p.rect_filled(Rect::from_min_max(Pos2::new(x_l - half_w, mid_y - mid_h), Pos2::new(x_l + half_w, mid_y + mid_h)), 2.0 * s, col);
                    }
                    if in_r && side_h > 0.5 {
                        let (st_y, sb_y) = (mid_y - max_bar * 0.65, mid_y + max_bar * 0.65);
                        p.line_segment([Pos2::new(x_r, st_y - side_h), Pos2::new(x_r, st_y + side_h)], Stroke::new(bw, col));
                        p.line_segment([Pos2::new(x_r, sb_y - side_h), Pos2::new(x_r, sb_y + side_h)], Stroke::new(bw, col));
                    }
                }
                ChannelMode::Mono => {
                    let h = gain_n * max_bar;
                    if in_l && h > 0.5 { p.line_segment([Pos2::new(x_l, mid_y - h * 0.5), Pos2::new(x_l, mid_y + h * 0.5)], Stroke::new(bw, col)); }
                }
            }
        }
    }
}

// ─── GRID visualizer — draggable tap balls (X = time offset, Y = gain) ─────
#[allow(clippy::too_many_arguments)]
fn draw_grid_viz(ui: &mut egui::Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                 viz: &VizData, tempo_bpm: f32, is_sync: bool, drag_tap: &mut Option<usize>, s: f32) {
    let times_l = &viz.times_l_grid;
    let (pad_l, pad_r, pad_t, pad_b) = (40.0 * s, 24.0 * s, 18.0 * s, 28.0 * s);
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let (draw_w, draw_h) = (draw_r - draw_l, draw_b - draw_t);
    let p = ui.painter().clone();

    for &db in &[6.0_f32, 0.0, -6.0, -12.0, -18.0, -24.0] {
        let y = draw_t + (1.0 - gain_db_to_pos(db)) * draw_h;
        if db == 0.0 { p.line_segment([Pos2::new(draw_l, y), Pos2::new(draw_r, y)], Stroke::new(1.0 * s, theme::line2())); }
        else { dashed_h(&p, Pos2::new(draw_l, y), draw_w, theme::line(), s); }
        let label = if db > 0.0 { format!("+{:.0}", db) } else if db == 0.0 { "0".into() } else { format!("{:.0}", db) };
        p.text(Pos2::new(rect.min.x + pad_l - 6.0 * s, y), Align2::RIGHT_CENTER, &label,
               FontId::new(9.0 * s, FontFamily::Monospace), theme::FAINT());
    }
    paint_timeline_grid(&p, draw_l, draw_r, draw_t, draw_b, viz.grid_range_secs_taps_only, tempo_bpm, is_sync, rect.max.y - 6.0 * s, s);
    p.text(Pos2::new(draw_l + 8.0 * s, draw_t + 6.0 * s), Align2::LEFT_TOP,
           "DRAG · X = TIME · Y = GAIN", FontId::new(9.0 * s, FontFamily::Proportional), theme::FAINT());

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let snap = params.tap_snap.value();
    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let op = if active { 1.0 } else { 0.25 };
        let x = draw_l + times_l[i] * draw_w;
        let gain_n = gain_db_to_pos(tap.gain_db.value());
        let y = draw_t + (1.0 - gain_n) * draw_h;
        let r = 9.0 * s;

        let resp = ui.interact(Rect::from_center_size(Pos2::new(x, y), Vec2::splat(r * 2.5)),
                               ui.id().with(("gridball", i)), Sense::click_and_drag());
        if resp.drag_started() {
            *drag_tap = Some(i);
            setter.begin_set_parameter(&tap.gain_db);
            setter.begin_set_parameter(&tap.time_offset);
        }
        if resp.dragged() && *drag_tap == Some(i) {
            let dy = -resp.drag_delta().y / draw_h;
            setter.set_parameter(&tap.gain_db, pos_to_gain_db((gain_n + dy).clamp(0.0, 1.0)));
            let dx = resp.drag_delta().x / draw_w;
            if dx.abs() > 1e-5 {
                let mut new_off = tap.time_offset.value() + dx;
                if snap && !ui.input(|inp| inp.modifiers.shift) {
                    let step = 1.0 / 16.0;
                    new_off = (new_off / step).round() * step;
                }
                setter.set_parameter(&tap.time_offset, new_off.clamp(-0.5, 0.5));
            }
        }
        if resp.drag_stopped() && *drag_tap == Some(i) {
            setter.end_set_parameter(&tap.gain_db);
            setter.end_set_parameter(&tap.time_offset);
            *drag_tap = None;
        }

        let tap_col = tap_color(i, n_active);
        let glow = tap_col.linear_multiply(op * (if *drag_tap == Some(i) { 0.30 } else { 0.12 }));
        p.line_segment([Pos2::new(x, draw_b), Pos2::new(x, y)],
                       Stroke::new(3.0 * s, tap_col.linear_multiply(if active { 0.45 } else { 0.1 })));
        p.circle_filled(Pos2::new(x, y), r + 6.0 * s, glow);
        p.circle_filled(Pos2::new(x, y), r, tap_col.linear_multiply(op));
        p.circle_stroke(Pos2::new(x, y), r, Stroke::new(1.0 * s, theme::with_alpha(theme::INK(), 64)));
        p.text(Pos2::new(x, y), Align2::CENTER_CENTER, &format!("{}", i + 1),
               FontId::new(9.0 * s, FontFamily::Monospace), theme::PAPER());
    }
}

// ─── RIGHT card body — per-tap controls ────────────────────────────────────
fn draw_right_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, tab: RightTab) {
    let s = theme::ui_scale(ui);
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let tw = ui.available_width();
    let cap = theme::SLIDER_CAP_2COL * s;
    let grad = |i| Accent::Raw(tab_grad(tab, i, n_active));
    match tab {
        RightTab::Gain => for i in 0..n_active {
            epo::slider(ui, setter, &params.taps[i].gain_db, &format!("{:02}", i + 1), tw, cap, grad(i), false);
        },
        RightTab::Pan => for i in 0..n_active {
            epo::slider(ui, setter, &params.taps[i].pan, &format!("{:02}", i + 1), tw, cap, grad(i), true);
        },
        RightTab::Filter => for i in 0..n_active {
            let tap = &params.taps[i];
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_XS * s;
                // type-cycle chip (LP → BP → HP)
                let (lab, next) = match tap.filter_type.value() {
                    FilterType::LowPass => ("LP", FilterType::BandPass),
                    FilterType::BandPass => ("BP", FilterType::HighPass),
                    FilterType::HighPass => ("HP", FilterType::LowPass),
                };
                if epo::chip_sm(ui, lab, false) { set_enum(setter, &tap.filter_type, next); }
                let sw = ui.available_width();
                epo::slider(ui, setter, &tap.filter_cutoff, &format!("{:02}", i + 1), sw, cap, grad(i), false);
            });
        },
        RightTab::XFeed => {
            let acc = Accent::Raw(tab_hue(RightTab::XFeed));
            section_label(ui, "MODE", s);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
                let cur = params.crossfeed_mode.value();
                for (lab, m) in [("MIX", CrossfeedMode::Mix), ("PING", CrossfeedMode::PingPong), ("DIFF", CrossfeedMode::Diffuse)] {
                    if epo::chip(ui, lab, cur == m) { set_enum(setter, &params.crossfeed_mode, m); }
                }
            });
            ui.add_space(theme::SP_MD * s);
            epo::slider(ui, setter, &params.crossfeed, "AMT", tw, cap, acc, false);
        }
        RightTab::Diffuse => {
            let acc = Accent::Raw(tab_hue(RightTab::Diffuse));
            section_label(ui, "ROUTING", s);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
                let cur = params.diffuse_routing.value();
                for (lab, m) in [("INPUT", DiffuseRouting::Input), ("FB", DiffuseRouting::Feedback), ("OUT", DiffuseRouting::Output)] {
                    if epo::chip(ui, lab, cur == m) { set_enum(setter, &params.diffuse_routing, m); }
                }
            });
            ui.add_space(theme::SP_MD * s);
            epo::slider(ui, setter, &params.diffuse_amount, "AMT", tw, cap, acc, false);
            epo::slider(ui, setter, &params.diffuse_size, "SIZE", tw, cap, acc, false);
        }
    }
}

// ─── Footer ────────────────────────────────────────────────────────────────
fn draw_footer(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    tempo: f32,
    margin: f32,
    s: f32,
) {
    ui.horizontal(|ui| {
        ui.add_space(margin);
        ui.spacing_mut().item_spacing.x = theme::SP_LG * s;
        let fb_str = format!("{:.0}%", params.feedback.value() * 100.0);
        epo::knob_ex(ui, setter, &params.feedback, "FEEDBACK", theme::KNOB_DIAM_FX, Accent::Head(1), false, None, Some(&fb_str));
        let _ = epo::readout_chip(ui, "FB TIME", &params.format_feedback_time(tempo));
        let _ = epo::readout_chip(ui, "TAPS", &format!("{}", params.num_taps_active.value()));
    });
}

// ─── helpers ────────────────────────────────────────────────────────────────
fn set_enum<T: PartialEq + Copy>(setter: &ParamSetter, param: &EnumParam<T>, v: T)
where
    T: nice_plug::params::enums::Enum,
{
    setter.begin_set_parameter(param);
    setter.set_parameter(param, v);
    setter.end_set_parameter(param);
}
