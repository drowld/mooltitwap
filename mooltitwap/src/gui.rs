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
    ChannelMode, DelayMode, PetalParams, SyncMode, NUM_TAPS,
};

/// Native design width; the zoom factor is window_width / BASE_W (kit contract).
pub const WINDOW_W: u32 = 1080;
pub const WINDOW_H: u32 = 720;
const BASE_W: f32 = 1080.0;

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
}
impl Default for EditorState {
    fn default() -> Self {
        Self { viz_mode: VizMode::Stems, right_tab: RightTab::Gain }
    }
}

// ─── Audio-taper gain mapping (cube-root: 75% pos = 0 dB) ────────────────
const GAIN_SKEW: f32 = 0.333;
#[inline]
fn gain_db_to_pos(db: f32) -> f32 {
    ((db + 60.0) / 66.0).clamp(0.0, 1.0).powf(1.0 / GAIN_SKEW)
}

/// Per-tap gradient: tap 0 = violet accent, tap n-1 = light violet.
fn tap_color(i: usize, n_active: usize) -> Color32 {
    let t = if n_active <= 1 { 0.0 } else { i as f32 / (n_active - 1) as f32 };
    let r = (0xa0_u8 as f32 + t * (0xc9_u8 - 0xa0_u8) as f32).round() as u8;
    let g = (0x86_u8 as f32 + t * (0xb8_u8 - 0x86_u8) as f32).round() as u8;
    let b = (0xe8_u8 as f32 + t * (0xf2_u8 - 0xe8_u8) as f32).round() as u8;
    Color32::from_rgb(r, g, b)
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
            let w = egui_state_size.size().0 as f32;
            theme::apply_zoom(ui.ctx(), (w / BASE_W).clamp(0.5, 2.5));
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

    // Read tab state out of `st` so the card body closures don't need it.
    let mut viz_mode = st.viz_mode;
    let mut right_tab = st.right_tab;

    ui.horizontal_top(|ui| {
        ui.add_space(margin);
        ui.spacing_mut().item_spacing.x = gap;

        epo::card(ui, "DELAY", theme::INK2(), left_w, &mut |ui| {
            draw_left_body(ui, params, setter, tempo);
        });

        let (vc, _) = epo::card_tabs(
            ui, "VIEW", &["STEMS", "GRID"],
            if viz_mode == VizMode::Stems { 0 } else { 1 },
            center_w, 380.0 * s,
            &mut |ui| draw_center_body(ui, params, setter, viz_mode, tempo),
            None,
        );
        if let Some(i) = vc { viz_mode = if i == 0 { VizMode::Stems } else { VizMode::Grid }; }

        let tabs = ["PAN", "GAIN", "FILTER", "XFEED", "DIFFUSE"];
        let active_tab = match right_tab {
            RightTab::Pan => 0, RightTab::Gain => 1, RightTab::Filter => 2,
            RightTab::XFeed => 3, RightTab::Diffuse => 4,
        };
        let (rc, _) = epo::card_tabs(
            ui, "TAPS", &tabs, active_tab, right_w, 380.0 * s,
            &mut |ui| draw_right_body(ui, params, setter, right_tab),
            None,
        );
        if let Some(i) = rc {
            right_tab = [RightTab::Pan, RightTab::Gain, RightTab::Filter,
                         RightTab::XFeed, RightTab::Diffuse][i];
        }
    });

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

// ─── CENTER card body — visualizer (placeholder well for first cut) ────────
fn draw_center_body(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    _setter: &ParamSetter,
    _viz_mode: VizMode,
    _tempo: f32,
) {
    let s = theme::ui_scale(ui);
    let w = ui.available_width();
    let h = 300.0 * s;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 13.0 * s, theme::INSET());
    p.rect_stroke(rect, 13.0 * s, Stroke::new(1.0 * s, theme::line2()), StrokeKind::Inside);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    // Minimal stems preview: evenly spaced tap bars (real time-domain viz ported next).
    let pad = 16.0 * s;
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + pad, rect.top() + pad),
        Pos2::new(rect.right() - pad, rect.bottom() - pad),
    );
    let mid = inner.center().y;
    for i in 0..n_active {
        let t = (i as f32 + 0.5) / n_active as f32;
        let x = inner.left() + t * inner.width();
        let gain_n = gain_db_to_pos(params.taps[i].gain_db.value());
        let hgt = gain_n * inner.height() * 0.45;
        let col = tap_color(i, n_active);
        p.line_segment([Pos2::new(x, mid), Pos2::new(x, mid - hgt)], Stroke::new(3.0 * s, col));
        p.line_segment([Pos2::new(x, mid), Pos2::new(x, mid + hgt)], Stroke::new(3.0 * s, col));
    }
    p.text(Pos2::new(rect.left() + pad, rect.top() + pad), Align2::LEFT_TOP,
           "STEMS", FontId::new(9.0 * s, FontFamily::Monospace), theme::SOFT());

    epo::hint(ui, "visualizer port in progress — bars are a placeholder");
}

// ─── RIGHT card body — per-tap controls ────────────────────────────────────
fn draw_right_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, tab: RightTab) {
    let s = theme::ui_scale(ui);
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let tw = ui.available_width();
    let cap = theme::SLIDER_CAP_2COL * s;
    match tab {
        RightTab::Gain => for i in 0..n_active {
            epo::slider(ui, setter, &params.taps[i].gain_db, &format!("{:02}", i + 1), tw, cap, Accent::Head(i), false);
        },
        RightTab::Pan => for i in 0..n_active {
            epo::slider(ui, setter, &params.taps[i].pan, &format!("{:02}", i + 1), tw, cap, Accent::Head(i), true);
        },
        RightTab::Filter => for i in 0..n_active {
            epo::slider(ui, setter, &params.taps[i].filter_cutoff, &format!("{:02}", i + 1), tw, cap, Accent::Head(i), false);
        },
        RightTab::XFeed => {
            epo::slider(ui, setter, &params.crossfeed, "XFEED", tw, cap, Accent::M, false);
        }
        RightTab::Diffuse => {
            epo::slider(ui, setter, &params.diffuse_amount, "AMOUNT", tw, cap, Accent::M, false);
            epo::slider(ui, setter, &params.diffuse_size, "SIZE", tw, cap, Accent::M, false);
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
