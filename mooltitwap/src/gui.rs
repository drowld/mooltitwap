//! Mooltitwap GUI — port of the Petal v2 mockup design.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke,
        StrokeKind, Ui, Vec2,
    },
    resizable_window::ResizableWindow,
    EguiState,
};
use std::sync::Arc;

use crate::params::{
    snap_to_div, ChannelMode, PetalParams, SpacingMode, SyncMode, NUM_TAPS, SYNC_DIVS,
};

pub const WINDOW_W: u32 = 1080;
pub const WINDOW_H: u32 = 560;
const HEADER_H: f32 = 40.0;
const LEFT_W: f32 = 220.0;
const RIGHT_W: f32 = 280.0;
const SHAPING_H: f32 = 118.0;
const RIGHT_FOOTER_H: f32 = 70.0;
const RIGHT_TABS_H: f32 = 44.0;
const VIZ_TABS_H: f32 = 36.0;

// ─── Palette ─────────────────────────────────────────────────────────────
mod c {
    use nih_plug_egui::egui::Color32;
    pub const BG_PLUGIN:  Color32 = Color32::from_rgb(0x0e, 0x0e, 0x13);
    pub const BG_PANEL:   Color32 = Color32::from_rgb(0x13, 0x13, 0x19);
    pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x22);
    pub const BG_ELEV:    Color32 = Color32::from_rgb(0x20, 0x20, 0x2a);
    pub const LINE:       Color32 = Color32::from_rgb(0x26, 0x26, 0x2f);
    pub const LINE_SOFT:  Color32 = Color32::from_rgb(0x1d, 0x1d, 0x24);
    pub const INK:        Color32 = Color32::from_rgb(0xe9, 0xe9, 0xf0);
    pub const INK_DIM:    Color32 = Color32::from_rgb(0x8c, 0x8c, 0x9c);
    pub const INK_FAINT:  Color32 = Color32::from_rgb(0x55, 0x55, 0x5f);
    pub const ACCENT:     Color32 = Color32::from_rgb(0xa0, 0x6c, 0xf0);
    pub const ACCENT_2:   Color32 = Color32::from_rgb(0xc0, 0x84, 0xfc);
}

const RR: CornerRadius = CornerRadius { nw: 4, ne: 4, sw: 4, se: 4 };
const RR6: CornerRadius = CornerRadius { nw: 6, ne: 6, sw: 6, se: 6 };
const RR_ZERO: CornerRadius = CornerRadius { nw: 0, ne: 0, sw: 0, se: 0 };
const RR_PILL: CornerRadius = CornerRadius { nw: 13, ne: 13, sw: 13, se: 13 };

pub struct EditorState {
    pub viz_mode: VizMode,
    pub right_tab: RightTab,
    pub drag_tap: Option<usize>,
    pub drag_kind: DragKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VizMode { Stems, Grid }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RightTab { Pitch, Pan, Gain, Filter, XFeed, Diffuse }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DragKind { None, GridBall }

impl Default for EditorState {
    fn default() -> Self {
        Self {
            viz_mode: VizMode::Stems,
            right_tab: RightTab::Pitch,
            drag_tap: None,
            drag_kind: DragKind::None,
        }
    }
}

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(WINDOW_W, WINDOW_H)
}

pub fn create_editor(
    params: Arc<PetalParams>,
    egui_state: Arc<EguiState>,
) -> Option<Box<dyn Editor>> {
    let editor_state = std::sync::Mutex::new(EditorState::default());
    create_egui_editor(
        egui_state.clone(),
        editor_state,
        |_ctx, _| {},
        move |egui_ctx, setter, state| {
            ResizableWindow::new("mooltitwap-window")
                .min_size(Vec2::new(WINDOW_W as f32, WINDOW_H as f32))
                .show(egui_ctx, egui_state.as_ref(), |ui| {
                    let mut st = state.lock().unwrap();
                    draw_plugin(ui, &params, setter, &mut st);
                });
        },
    )
}

// ─── Top-level draw ──────────────────────────────────────────────────────
fn draw_plugin(ui: &mut Ui, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let avail = ui.available_rect_before_wrap();
    ui.painter().rect_filled(avail, RR_ZERO, c::BG_PLUGIN);

    let header_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), HEADER_H));
    draw_header(ui, header_rect);

    let body_top = header_rect.bottom();
    let body_rect = Rect::from_min_max(
        Pos2::new(avail.min.x, body_top),
        Pos2::new(avail.max.x, avail.max.y),
    );
    let left_rect = Rect::from_min_max(body_rect.min,
        Pos2::new(body_rect.min.x + LEFT_W, body_rect.max.y));
    let right_rect = Rect::from_min_max(
        Pos2::new(body_rect.max.x - RIGHT_W, body_rect.min.y),
        body_rect.max,
    );
    let center_rect = Rect::from_min_max(
        Pos2::new(left_rect.max.x, body_rect.min.y),
        Pos2::new(right_rect.min.x, body_rect.max.y),
    );

    {
        let p = ui.painter();
        p.line_segment([Pos2::new(left_rect.max.x, body_rect.min.y), Pos2::new(left_rect.max.x, body_rect.max.y)],
                       Stroke::new(1.0, c::LINE));
        p.line_segment([Pos2::new(right_rect.min.x, body_rect.min.y), Pos2::new(right_rect.min.x, body_rect.max.y)],
                       Stroke::new(1.0, c::LINE));
        p.rect_filled(left_rect, RR_ZERO, c::BG_PANEL);
        p.rect_filled(right_rect, RR_ZERO, c::BG_PANEL);
    }

    draw_left(ui, left_rect, params, setter);
    draw_center(ui, center_rect, params, setter, st);
    draw_right(ui, right_rect, params, setter, st);
}

// ─── Header ──────────────────────────────────────────────────────────────
fn draw_header(ui: &mut Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, RR_ZERO, c::BG_PLUGIN);
    p.line_segment([Pos2::new(rect.min.x, rect.max.y), Pos2::new(rect.max.x, rect.max.y)],
                   Stroke::new(1.0, c::LINE));

    let mark_center = Pos2::new(rect.min.x + 22.0, rect.center().y);
    draw_brand_mark(p, mark_center, 9.0);
    p.text(Pos2::new(mark_center.x + 14.0, rect.center().y), Align2::LEFT_CENTER,
           "MOOLTITWAP", FontId::new(13.0, FontFamily::Proportional), c::INK);
    p.text(Pos2::new(mark_center.x + 14.0 + 88.0, rect.center().y), Align2::LEFT_CENTER,
           "v2.1 · multitap", FontId::new(10.0, FontFamily::Proportional), c::INK_FAINT);

    // preset pill (placeholder)
    let pill_rect = Rect::from_center_size(rect.center(), Vec2::new(240.0, 26.0));
    p.rect_filled(pill_rect, RR_PILL, c::BG_SURFACE);
    p.rect_stroke(pill_rect, RR_PILL, Stroke::new(1.0, c::LINE), StrokeKind::Middle);
    p.text(pill_rect.center(), Align2::CENTER_CENTER, "DEFAULT",
           FontId::new(11.0, FontFamily::Proportional), c::INK);

    // right-side icons (placeholder)
    let icons = ["?", "↶", "↷", "⚂", "💾"];
    let mut x = rect.max.x - 12.0;
    for icon in icons.iter().rev() {
        let r = Rect::from_min_size(Pos2::new(x - 26.0, rect.center().y - 13.0), Vec2::splat(26.0));
        p.text(r.center(), Align2::CENTER_CENTER, *icon,
               FontId::new(13.0, FontFamily::Proportional), c::INK_DIM);
        x -= 28.0;
    }
}

fn draw_brand_mark(p: &egui::Painter, center: Pos2, r: f32) {
    for angle_deg in [0.0_f32, 60.0, 120.0] {
        let a = angle_deg.to_radians();
        let cos_a = a.cos();
        let sin_a = a.sin();
        let n = 24;
        let pts: Vec<Pos2> = (0..=n).map(|i| {
            let t = (i as f32 / n as f32) * std::f32::consts::TAU;
            let x = r * t.cos();
            let y = r * 0.38 * t.sin();
            Pos2::new(center.x + cos_a * x - sin_a * y, center.y + sin_a * x + cos_a * y)
        }).collect();
        for w in pts.windows(2) {
            p.line_segment([w[0], w[1]], Stroke::new(1.0, c::ACCENT));
        }
    }
    p.circle_filled(center, 1.6, Color32::WHITE);
}

// ─── LEFT PANEL ──────────────────────────────────────────────────────────
fn draw_left(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter) {
    // Hero: Time L knob – link button – Time R knob
    let hero_h = 96.0;
    let hero_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), hero_h));
    {
        let p = ui.painter();
        p.line_segment([Pos2::new(rect.min.x, hero_rect.max.y), Pos2::new(rect.max.x, hero_rect.max.y)],
                       Stroke::new(1.0, c::LINE_SOFT));
    }

    let knob_size = 50.0;
    let cy = hero_rect.min.y + 30.0;
    let lknob_center = Pos2::new(hero_rect.center().x - 56.0, cy);
    let rknob_center = Pos2::new(hero_rect.center().x + 56.0, cy);

    let tempo_bpm = 120.0; // placeholder for display only
    let time_l_val = params.base_time_l.value();
    let time_r_val = if params.time_linked.value() { time_l_val } else { params.base_time_r.value() };
    let l_label = params.format_base_time(time_l_val, tempo_bpm);
    let r_label = params.format_base_time(time_r_val, tempo_bpm);

    knob_at(ui, lknob_center, knob_size, &params.base_time_l, setter);
    // If linked, also drive R from L drag
    if params.time_linked.value() {
        knob_display_only(ui, rknob_center, knob_size, time_l_val);
    } else {
        knob_at(ui, rknob_center, knob_size, &params.base_time_r, setter);
    }

    // labels
    {
        let p = ui.painter();
        p.text(Pos2::new(lknob_center.x, lknob_center.y + 30.0), Align2::CENTER_TOP,
               &l_label, FontId::new(11.0, FontFamily::Proportional), c::INK);
        p.text(Pos2::new(lknob_center.x, lknob_center.y + 44.0), Align2::CENTER_TOP,
               "TIME L", FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
        p.text(Pos2::new(rknob_center.x, rknob_center.y + 30.0), Align2::CENTER_TOP,
               &r_label, FontId::new(11.0, FontFamily::Proportional), c::INK);
        p.text(Pos2::new(rknob_center.x, rknob_center.y + 44.0), Align2::CENTER_TOP,
               "TIME R", FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
    }

    // link button
    let link_center = Pos2::new(hero_rect.center().x, cy);
    let link_r = 11.0;
    let link_rect = Rect::from_center_size(link_center, Vec2::splat(link_r * 2.0));
    let link_resp = ui.allocate_rect(link_rect, Sense::click());
    if link_resp.clicked() {
        setter.begin_set_parameter(&params.time_linked);
        setter.set_parameter(&params.time_linked, !params.time_linked.value());
        setter.end_set_parameter(&params.time_linked);
    }
    {
        let p = ui.painter();
        let linked = params.time_linked.value();
        let (fill, ink) = if linked { (c::ACCENT, Color32::WHITE) } else { (c::BG_SURFACE, c::INK_DIM) };
        p.circle_filled(link_center, link_r, fill);
        if !linked {
            p.circle_stroke(link_center, link_r, Stroke::new(1.0, c::LINE));
        }
        p.text(link_center, Align2::CENTER_CENTER, "🔗",
               FontId::new(10.0, FontFamily::Proportional), ink);
    }

    // Sections
    let mut y = hero_rect.max.y;
    y = section(ui, rect, y, "SYNC", 30.0, |ui, sec| {
        let cw = sec.width() / 2.0 - 1.0;
        let r0 = Rect::from_min_size(sec.min, Vec2::new(cw, 26.0));
        let r1 = Rect::from_min_size(Pos2::new(sec.min.x + cw + 2.0, sec.min.y), Vec2::new(cw, 26.0));
        let current = params.sync_mode.value();
        if seg_button(ui, r0, "Free", current == SyncMode::Free, true) {
            setter.begin_set_parameter(&params.sync_mode);
            setter.set_parameter(&params.sync_mode, SyncMode::Free);
            setter.end_set_parameter(&params.sync_mode);
        }
        if seg_button(ui, r1, "Sync", current == SyncMode::Sync, true) {
            setter.begin_set_parameter(&params.sync_mode);
            setter.set_parameter(&params.sync_mode, SyncMode::Sync);
            setter.end_set_parameter(&params.sync_mode);
        }
    });

    y = section(ui, rect, y, "CHANNEL MODE", 30.0, |ui, sec| {
        let labels = ["Stereo", "M / S", "Mono"];
        let modes = [ChannelMode::Stereo, ChannelMode::MidSide, ChannelMode::Mono];
        let cw = sec.width() / 3.0 - 1.0;
        let current = params.channel_mode.value();
        for (i, lab) in labels.iter().enumerate() {
            let r = Rect::from_min_size(
                Pos2::new(sec.min.x + i as f32 * (cw + 2.0), sec.min.y),
                Vec2::new(cw, 26.0),
            );
            if seg_button(ui, r, lab, current == modes[i], true) {
                setter.begin_set_parameter(&params.channel_mode);
                setter.set_parameter(&params.channel_mode, modes[i]);
                setter.end_set_parameter(&params.channel_mode);
            }
        }
    });

    y = section(ui, rect, y, "LEVELS", 76.0, |ui, sec| {
        let knob_s = 38.0;
        let third = sec.width() / 3.0;
        let ky = sec.min.y + 22.0;
        let inp = nih_plug::util::db_to_gain(params.input_gain.value());
        knob_at(ui, Pos2::new(sec.min.x + third * 0.5, ky), knob_s, &params.input_gain, setter);
        knob_at(ui, Pos2::new(sec.min.x + third * 1.5, ky), knob_s, &params.dry_wet,    setter);
        knob_at(ui, Pos2::new(sec.min.x + third * 2.5, ky), knob_s, &params.output_gain, setter);
        let _ = inp;
        // label + value below each
        let p = ui.painter();
        let centers = [
            (Pos2::new(sec.min.x + third * 0.5, ky), "INPUT", format!("{:+.1} dB", params.input_gain.value())),
            (Pos2::new(sec.min.x + third * 1.5, ky), "DRY/WET", format!("{:.0}%", params.dry_wet.value() * 100.0)),
            (Pos2::new(sec.min.x + third * 2.5, ky), "OUTPUT", format!("{:+.1} dB", params.output_gain.value())),
        ];
        for (c, lbl, val) in centers {
            p.text(Pos2::new(c.x, c.y + knob_s * 0.5 + 4.0), Align2::CENTER_TOP, lbl,
                   FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
            p.text(Pos2::new(c.x, c.y + knob_s * 0.5 + 16.0), Align2::CENTER_TOP, &val,
                   FontId::new(10.0, FontFamily::Proportional), c::INK);
        }
    });

    y = section(ui, rect, y, "CHARACTER", 76.0, |ui, sec| {
        let knob_s = 38.0;
        let ky = sec.min.y + 22.0;
        let cx_l = sec.min.x + sec.width() * 0.25;
        let cx_r = sec.min.x + sec.width() * 0.75;
        knob_at(ui, Pos2::new(cx_l, ky), knob_s, &params.wobble_amount, setter);
        knob_at(ui, Pos2::new(cx_r, ky), knob_s, &params.wobble_rate, setter);
        let p = ui.painter();
        p.text(Pos2::new(cx_l, ky + knob_s * 0.5 + 4.0), Align2::CENTER_TOP, "WOBBLE",
               FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
        p.text(Pos2::new(cx_l, ky + knob_s * 0.5 + 16.0), Align2::CENTER_TOP,
               &format!("{:.0}", params.wobble_amount.value() * 100.0),
               FontId::new(10.0, FontFamily::Proportional), c::INK);
        p.text(Pos2::new(cx_r, ky + knob_s * 0.5 + 4.0), Align2::CENTER_TOP, "RATE",
               FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
        p.text(Pos2::new(cx_r, ky + knob_s * 0.5 + 16.0), Align2::CENTER_TOP,
               &format!("{:.2} Hz", params.wobble_rate.value()),
               FontId::new(10.0, FontFamily::Proportional), c::INK);
    });

    // Soft Clip — gets its own row with extra breathing room
    let _ = section(ui, rect, y, "SATURATION", 30.0, |ui, sec| {
        let row = Rect::from_min_size(sec.min, Vec2::new(sec.width(), 20.0));
        let resp = ui.allocate_rect(row, Sense::click());
        let on = params.soft_clip.value();
        if resp.clicked() {
            setter.begin_set_parameter(&params.soft_clip);
            setter.set_parameter(&params.soft_clip, !on);
            setter.end_set_parameter(&params.soft_clip);
        }
        let p = ui.painter();
        p.text(Pos2::new(row.min.x, row.center().y), Align2::LEFT_CENTER, "SOFT CLIP",
               FontId::new(10.0, FontFamily::Proportional), if on { c::INK } else { c::INK_DIM });
        let sw = Rect::from_min_size(
            Pos2::new(row.max.x - 30.0, row.center().y - 8.0),
            Vec2::new(28.0, 16.0),
        );
        p.rect_filled(sw, CornerRadius { nw: 8, ne: 8, sw: 8, se: 8 },
                      if on { c::ACCENT } else { c::BG_SURFACE });
        p.rect_stroke(sw, CornerRadius { nw: 8, ne: 8, sw: 8, se: 8 },
                      Stroke::new(1.0, if on { c::ACCENT } else { c::LINE }), StrokeKind::Middle);
        let kx = if on { sw.max.x - 7.0 } else { sw.min.x + 7.0 };
        p.circle_filled(Pos2::new(kx, sw.center().y), 6.0, Color32::WHITE);
    });
}

// ─── Section helper ──────────────────────────────────────────────────────
fn section<F: FnOnce(&mut Ui, Rect)>(
    ui: &mut Ui, panel: Rect, y: f32, title: &str, body_h: f32, body_fn: F,
) -> f32 {
    let pad_x = 14.0;
    let pad_top = 10.0;
    let title_h = 16.0;
    let p = ui.painter();
    p.text(Pos2::new(panel.min.x + pad_x, y + pad_top), Align2::LEFT_TOP, title,
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
    let body_rect = Rect::from_min_max(
        Pos2::new(panel.min.x + pad_x, y + pad_top + title_h),
        Pos2::new(panel.max.x - pad_x, y + pad_top + title_h + body_h),
    );
    body_fn(ui, body_rect);
    let total = pad_top + title_h + body_h + 10.0;
    let line_y = y + total;
    let p = ui.painter();
    p.line_segment([Pos2::new(panel.min.x, line_y), Pos2::new(panel.max.x, line_y)],
                   Stroke::new(1.0, c::LINE_SOFT));
    line_y
}

// ─── Segmented button ────────────────────────────────────────────────────
fn seg_button(ui: &mut Ui, rect: Rect, label: &str, active: bool, accent_fill: bool) -> bool {
    let resp = ui.allocate_rect(rect, Sense::click());
    let p = ui.painter();
    let fill = if active {
        if accent_fill { c::ACCENT } else { c::BG_ELEV }
    } else { c::BG_SURFACE };
    let text_col = if active {
        if accent_fill { Color32::WHITE } else { c::INK }
    } else { c::INK_DIM };
    p.rect_filled(rect, RR, fill);
    p.rect_stroke(rect, RR, Stroke::new(1.0, c::LINE), StrokeKind::Middle);
    p.text(rect.center(), Align2::CENTER_CENTER, label,
           FontId::new(10.0, FontFamily::Proportional), text_col);
    resp.clicked()
}

// ─── Knob widget ────────────────────────────────────────────────────────
fn knob_at<P: Param>(ui: &mut Ui, center: Pos2, size: f32, param: &P, setter: &ParamSetter)
where P::Plain: Into<f32> + Copy {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let mut norm = param.modulated_normalized_value();
    if resp.drag_started() { setter.begin_set_parameter(param); }
    if resp.dragged() {
        let delta = -resp.drag_delta().y * 0.005;
        norm = (norm + delta).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, norm);
    }
    if resp.drag_stopped() { setter.end_set_parameter(param); }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, param.default_normalized_value());
        setter.end_set_parameter(param);
    }
    paint_knob(ui.painter(), center, size, norm);
}

fn knob_display_only(ui: &mut Ui, center: Pos2, size: f32, norm: f32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    let _ = ui.allocate_rect(rect, Sense::hover());
    paint_knob(ui.painter(), center, size, norm);
}

fn paint_knob(p: &egui::Painter, center: Pos2, size: f32, value_norm: f32) {
    let r_outer = size * 0.5 - 2.0;
    let r_inner = size * 0.5 - 6.0;
    let start = -135.0_f32.to_radians() - std::f32::consts::FRAC_PI_2;
    let end   =  135.0_f32.to_radians() - std::f32::consts::FRAC_PI_2;
    let cur = start + (end - start) * value_norm;

    arc(p, center, r_outer, start, end, Stroke::new(2.0, c::BG_ELEV));
    if value_norm > 0.001 {
        arc(p, center, r_outer, start, cur, Stroke::new(2.0, c::ACCENT));
    }
    p.circle_filled(center, r_inner, Color32::from_rgb(0x22, 0x22, 0x2c));
    p.circle_filled(center + Vec2::new(0.0, -r_inner * 0.15), r_inner * 0.85,
                    Color32::from_rgb(0x2a, 0x2a, 0x36));
    p.circle_stroke(center, r_inner, Stroke::new(0.8, c::LINE));

    let ind_inner = r_inner - 4.0;
    let ind_outer = r_inner + 2.0;
    let dx = cur.cos();
    let dy = cur.sin();
    p.line_segment([
        Pos2::new(center.x + dx * ind_inner, center.y + dy * ind_inner),
        Pos2::new(center.x + dx * ind_outer, center.y + dy * ind_outer),
    ], Stroke::new(1.6, Color32::WHITE));
}

fn arc(p: &egui::Painter, center: Pos2, r: f32, start: f32, end: f32, stroke: Stroke) {
    let segments = ((end - start).abs() * r * 0.5).max(8.0) as usize;
    let mut pts: Vec<Pos2> = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a = start + (end - start) * t;
        pts.push(Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r));
    }
    for w in pts.windows(2) { p.line_segment([w[0], w[1]], stroke); }
}

// ─── Topology positions (mirrors topology.rs math, computed in GUI) ──────
fn compute_positions(spacing: SpacingMode, shape_x: f32, shape_y: f32) -> [f32; NUM_TAPS] {
    let mut out = [0.0; NUM_TAPS];
    for i in 0..NUM_TAPS {
        let n = (i + 1) as f32 / NUM_TAPS as f32;
        let base = match spacing {
            SpacingMode::Linear => n,
            SpacingMode::Exponential => n.powf(2.0).min(1.0),
            SpacingMode::Logarithmic => n.sqrt(),
            SpacingMode::Euclidean => n,
        };
        let shaped = sigmoid_blend(base, shape_y);
        out[i] = bias_curve(shaped, shape_x);
    }
    out
}
fn sigmoid_blend(x: f32, y: f32) -> f32 {
    if y <= 0.0 { return x; }
    let s = x * x * (3.0 - 2.0 * x);
    x * (1.0 - y) + s * y
}
fn bias_curve(v: f32, x: f32) -> f32 {
    if x.abs() < 1e-6 { return v; }
    let power = if x >= 0.0 { 1.0 / (1.0 + 4.0 * x) } else { 1.0 + 4.0 * (-x) };
    v.powf(power)
}

// Returns (positions_l, positions_r) normalized to fit in [0, 1] of the
// visible timeline, scaled by relative time_l vs time_r.
fn tap_times_normalized(params: &PetalParams) -> ([f32; NUM_TAPS], [f32; NUM_TAPS]) {
    let pos_l = compute_positions(params.spacing_mode.value(),
                                   params.shape_x_l.value(), params.shape_y_l.value());
    let pos_r = if params.shape_linked.value() {
        pos_l
    } else {
        compute_positions(params.spacing_mode.value(),
                          params.shape_x_r.value(), params.shape_y_r.value())
    };
    // Scale positions by time L/R so different L/R times spread visually
    let time_l = params.base_time_l.value();
    let time_r = if params.time_linked.value() { time_l } else { params.base_time_r.value() };
    // Normalize by max to fit timeline width
    let max_time = time_l.max(time_r).max(0.001);
    let scale_l = time_l / max_time;
    let scale_r = time_r / max_time;
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let max_pos_l = pos_l[n_active - 1].max(0.001);
    let max_pos_r = pos_r[n_active - 1].max(0.001);
    let mut out_l = [0.0; NUM_TAPS];
    let mut out_r = [0.0; NUM_TAPS];
    for i in 0..NUM_TAPS {
        let offset = params.taps[i].time_offset.value();
        out_l[i] = (pos_l[i] / max_pos_l * scale_l + offset).clamp(0.0, 1.0);
        out_r[i] = (pos_r[i] / max_pos_r * scale_r + offset).clamp(0.0, 1.0);
    }
    (out_l, out_r)
}

// ─── CENTER ──────────────────────────────────────────────────────────────
fn draw_center(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let p = ui.painter();
    p.rect_filled(rect, RR_ZERO, c::BG_PLUGIN);

    let tabs_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), VIZ_TABS_H));
    p.rect_filled(tabs_rect, RR_ZERO, c::BG_PANEL);
    p.line_segment([Pos2::new(tabs_rect.min.x, tabs_rect.max.y), Pos2::new(tabs_rect.max.x, tabs_rect.max.y)],
                   Stroke::new(1.0, c::LINE));

    let tab_w = 90.0;
    let stems_r = Rect::from_min_size(Pos2::new(tabs_rect.min.x + 16.0, tabs_rect.min.y), Vec2::new(tab_w, VIZ_TABS_H));
    let grid_r  = Rect::from_min_size(Pos2::new(stems_r.max.x + 8.0, tabs_rect.min.y), Vec2::new(tab_w, VIZ_TABS_H));
    if viz_tab(ui, stems_r, "STEMS", st.viz_mode == VizMode::Stems) { st.viz_mode = VizMode::Stems; }
    if viz_tab(ui, grid_r,  "GRID",  st.viz_mode == VizMode::Grid)  { st.viz_mode = VizMode::Grid; }

    // meta
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let meta = format!("STEREO    TAPS  {}    LENGTH  4 / 1", n_active);
    ui.painter().text(
        Pos2::new(tabs_rect.max.x - 16.0, tabs_rect.center().y),
        Align2::RIGHT_CENTER, &meta,
        FontId::new(10.0, FontFamily::Proportional), c::INK_DIM,
    );

    // viz area + shaping strip
    let viz_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, tabs_rect.max.y),
        Pos2::new(rect.max.x, rect.max.y - SHAPING_H),
    );
    let shaping_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, viz_rect.max.y), rect.max,
    );

    let (times_l, times_r) = tap_times_normalized(params);

    match st.viz_mode {
        VizMode::Stems => draw_stems(ui, viz_rect, params, &times_l, &times_r),
        VizMode::Grid  => draw_grid_viz(ui, viz_rect, params, setter, st, &times_l),
    }
    draw_shaping_strip(ui, shaping_rect, params, setter);
}

fn viz_tab(ui: &mut Ui, rect: Rect, label: &str, active: bool) -> bool {
    let resp = ui.allocate_rect(rect, Sense::click());
    let p = ui.painter();
    p.text(rect.center(), Align2::CENTER_CENTER, label,
           FontId::new(10.0, FontFamily::Proportional),
           if active { c::INK } else { c::INK_DIM });
    if active {
        p.line_segment([Pos2::new(rect.min.x, rect.max.y - 1.0), Pos2::new(rect.max.x, rect.max.y - 1.0)],
                       Stroke::new(2.0, c::ACCENT));
    }
    resp.clicked()
}

// ─── Grid background helper (16th notes) ─────────────────────────────────
fn paint_timeline_grid(p: &egui::Painter, draw_l: f32, draw_r: f32, draw_t: f32, draw_b: f32, show_labels: bool, label_y: f32) {
    let draw_w = draw_r - draw_l;
    // 16th note subdivisions (16 over 4 beats)
    for i in 0..=16 {
        let x = draw_l + (i as f32 / 16.0) * draw_w;
        let (sw, col) = if i % 4 == 0 { (1.0, c::LINE) } else { (0.5, c::LINE_SOFT) };
        p.line_segment([Pos2::new(x, draw_t), Pos2::new(x, draw_b)], Stroke::new(sw, col));
        if show_labels && i % 4 == 0 {
            p.text(Pos2::new(x, label_y), Align2::CENTER_BOTTOM,
                   &format!("{}/1", i / 4),
                   FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
        }
    }
}

// ─── STEMS visualizer ───────────────────────────────────────────────────
fn draw_stems(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>,
              times_l: &[f32; NUM_TAPS], times_r: &[f32; NUM_TAPS]) {
    let p = ui.painter();
    let pad_l = 36.0;
    let pad_r = 24.0;
    let pad_t = 18.0;
    let pad_b = 24.0;
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let mid_y = (draw_t + draw_b) * 0.5;
    let max_bar = (draw_b - draw_t) * 0.5 - 10.0;
    let draw_w = draw_r - draw_l;

    let ch_mode = params.channel_mode.value();

    // L/R or M/S labels
    let (top_lab, bot_lab) = match ch_mode {
        ChannelMode::Stereo => ("L", "R"),
        ChannelMode::MidSide => ("M", "S"),
        ChannelMode::Mono => ("M", "M"),
    };
    p.text(Pos2::new(rect.min.x + 10.0, draw_t + 4.0), Align2::LEFT_TOP, top_lab,
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
    p.text(Pos2::new(rect.min.x + 10.0, draw_b - 4.0), Align2::LEFT_BOTTOM, bot_lab,
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);

    // center dashed
    dashed_h(p, Pos2::new(draw_l, mid_y), draw_w, c::LINE);

    // 16th grid
    paint_timeline_grid(p, draw_l, draw_r, draw_t, draw_b, true, rect.max.y - 6.0);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let op = if active { 1.0 } else { 0.22 };
        let gain_n = 1.0 - (tap.gain_db.value().clamp(-60.0, 6.0) - 6.0) / -66.0;
        let pan = tap.pan.value();

        let x_l = draw_l + times_l[i] * draw_w;
        let x_r = draw_l + times_r[i] * draw_w;

        match ch_mode {
            ChannelMode::Stereo => {
                let l_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).cos();
                let r_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).sin();
                let h_l = gain_n * l_w * max_bar;
                let h_r = gain_n * r_w * max_bar;
                let col = c::ACCENT.linear_multiply(op);
                p.line_segment([Pos2::new(x_l, mid_y - 1.0), Pos2::new(x_l, mid_y - h_l)], Stroke::new(2.2, col));
                p.circle_filled(Pos2::new(x_l, mid_y - h_l), 3.0, col);
                p.line_segment([Pos2::new(x_r, mid_y + 1.0), Pos2::new(x_r, mid_y + h_r)], Stroke::new(2.2, col));
                p.circle_filled(Pos2::new(x_r, mid_y + h_r), 3.0, col);
                let pitch = tap.pitch_semitones.value();
                if pitch != 0 && active {
                    let sign = if pitch > 0 { "+" } else { "" };
                    p.text(Pos2::new(x_l, mid_y - h_l - 10.0), Align2::CENTER_BOTTOM,
                           &format!("{}{}", sign, pitch),
                           FontId::new(10.0, FontFamily::Proportional), col);
                }
            }
            ChannelMode::MidSide => {
                // Mid bar centered (small ±20% height), Side bars spaced 30% above/below
                let mid_h = gain_n * max_bar * 0.4;
                let side_h = gain_n * max_bar * 0.3 * pan.abs(); // |pan| drives side amount
                let col = c::ACCENT.linear_multiply(op);
                // Mid: centered horizontal bar
                let mr = Rect::from_min_max(
                    Pos2::new(x_l - 2.5, mid_y - mid_h * 0.5),
                    Pos2::new(x_l + 2.5, mid_y + mid_h * 0.5),
                );
                p.rect_filled(mr, RR, col);
                // Side bars top and bottom
                if side_h > 1.0 {
                    let s_top_y = mid_y - max_bar * 0.6;
                    let s_bot_y = mid_y + max_bar * 0.6;
                    p.line_segment([Pos2::new(x_l, s_top_y - side_h * 0.5), Pos2::new(x_l, s_top_y + side_h * 0.5)],
                                   Stroke::new(2.2, col));
                    p.line_segment([Pos2::new(x_l, s_bot_y - side_h * 0.5), Pos2::new(x_l, s_bot_y + side_h * 0.5)],
                                   Stroke::new(2.2, col));
                }
            }
            ChannelMode::Mono => {
                let h = gain_n * max_bar;
                let col = c::ACCENT.linear_multiply(op);
                // Single bar centered (mid_y), going both ways smaller
                p.line_segment([Pos2::new(x_l, mid_y - h * 0.5), Pos2::new(x_l, mid_y + h * 0.5)],
                               Stroke::new(2.2, col));
                p.circle_filled(Pos2::new(x_l, mid_y - h * 0.5), 3.0, col);
                p.circle_filled(Pos2::new(x_l, mid_y + h * 0.5), 3.0, col);
            }
        }
    }
}

fn dashed_h(p: &egui::Painter, start: Pos2, width: f32, color: Color32) {
    let dash = 4.0; let gap = 4.0;
    let mut x = start.x;
    while x < start.x + width {
        let x2 = (x + dash).min(start.x + width);
        p.line_segment([Pos2::new(x, start.y), Pos2::new(x2, start.y)], Stroke::new(0.8, color));
        x += dash + gap;
    }
}
fn dashed_v(p: &egui::Painter, start: Pos2, height: f32, color: Color32) {
    let dash = 3.0; let gap = 4.0;
    let mut y = start.y;
    while y < start.y + height {
        let y2 = (y + dash).min(start.y + height);
        p.line_segment([Pos2::new(start.x, y), Pos2::new(start.x, y2)], Stroke::new(0.4, color));
        y += dash + gap;
    }
}

// ─── GRID visualizer ────────────────────────────────────────────────────
fn draw_grid_viz(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                 st: &mut EditorState, times_l: &[f32; NUM_TAPS]) {
    let p = ui.painter();
    let pad_l = 40.0;
    let pad_r = 24.0;
    let pad_t = 18.0;
    let pad_b = 28.0;
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let draw_w = draw_r - draw_l;
    let draw_h = draw_b - draw_t;

    // dB axis lines & labels
    for &g in &[1.0_f32, 0.75, 0.5, 0.25, 0.0] {
        let y = draw_t + (1.0 - g) * draw_h;
        if g != 0.0 && g != 1.0 {
            dashed_h(p, Pos2::new(draw_l, y), draw_w, c::LINE_SOFT);
        } else {
            p.line_segment([Pos2::new(draw_l, y), Pos2::new(draw_r, y)], Stroke::new(1.0, c::LINE));
        }
        let label = if g == 0.0 { "-∞".to_string() } else if g == 1.0 { "0".to_string() } else { format!("{}", ((g - 1.0) * 24.0) as i32) };
        p.text(Pos2::new(rect.min.x + pad_l - 6.0, y), Align2::RIGHT_CENTER, &label,
               FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
    }
    p.text(Pos2::new(rect.min.x + pad_l - 6.0, draw_t - 6.0), Align2::RIGHT_BOTTOM, "dB",
           FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);

    paint_timeline_grid(p, draw_l, draw_r, draw_t, draw_b, true, rect.max.y - 6.0);

    p.text(Pos2::new(draw_l + 8.0, draw_t + 6.0), Align2::LEFT_TOP,
           "DRAG · X = TIME · Y = GAIN",
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let snap = params.tap_snap.value();
    let active_drag = st.drag_tap;

    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let op = if active { 1.0 } else { 0.25 };
        let x = draw_l + times_l[i] * draw_w;
        let gain_n = 1.0 - (tap.gain_db.value().clamp(-60.0, 6.0) - 6.0) / -66.0;
        let y = draw_t + (1.0 - gain_n) * draw_h;
        let pitch = tap.pitch_semitones.value();
        let r = 8.0 + (pitch.abs() as f32) * 0.4;

        let baseline_y = draw_b;
        let p = ui.painter();
        p.line_segment([Pos2::new(x, baseline_y), Pos2::new(x, y)],
                       Stroke::new(1.0, c::ACCENT.linear_multiply(if active { 0.45 } else { 0.1 })));

        let ball_rect = Rect::from_center_size(Pos2::new(x, y), Vec2::splat(r * 2.5));
        let resp = ui.allocate_rect(ball_rect, Sense::click_and_drag());

        if resp.drag_started() {
            st.drag_tap = Some(i);
            st.drag_kind = DragKind::GridBall;
            setter.begin_set_parameter(&tap.gain_db);
            setter.begin_set_parameter(&tap.time_offset);
        }
        if resp.dragged() && active_drag == Some(i) {
            // Vertical → gain
            let dy = -resp.drag_delta().y / draw_h;
            let new_gain_n = (gain_n + dy).clamp(0.0, 1.0);
            let db = -60.0 + new_gain_n * 66.0;
            setter.set_parameter(&tap.gain_db, db);
            // Horizontal → time offset (snap if enabled and shift not held)
            let dx = resp.drag_delta().x / draw_w;
            if dx.abs() > 1e-5 {
                let mut new_off = tap.time_offset.value() + dx;
                if snap && !ui.input(|i| i.modifiers.shift) {
                    // snap to 1/16
                    let step = 1.0 / 16.0;
                    new_off = (new_off / step).round() * step;
                }
                new_off = new_off.clamp(-0.5, 0.5);
                setter.set_parameter(&tap.time_offset, new_off);
            }
        }
        if resp.drag_stopped() && active_drag == Some(i) {
            setter.end_set_parameter(&tap.gain_db);
            setter.end_set_parameter(&tap.time_offset);
            st.drag_tap = None;
            st.drag_kind = DragKind::None;
        }

        let p = ui.painter();
        let col = c::ACCENT.linear_multiply(op);
        let glow = c::ACCENT.linear_multiply(op * (if active_drag == Some(i) { 0.30 } else { 0.12 }));
        p.circle_filled(Pos2::new(x, y), r + 6.0, glow);
        p.circle_filled(Pos2::new(x, y), r, col);
        p.circle_stroke(Pos2::new(x, y), r, Stroke::new(1.0, Color32::from_white_alpha(64)));

        let label = if pitch == 0 { format!("{}", i + 1) }
                    else if pitch > 0 { format!("+{}", pitch) }
                    else { format!("{}", pitch) };
        let fs = if pitch == 0 { 9.0 } else { 10.0 };
        p.text(Pos2::new(x, y), Align2::CENTER_CENTER, &label,
               FontId::new(fs, FontFamily::Proportional), Color32::WHITE);
    }

    // snap indicator at bottom right
    let snap_text = if snap { "SNAP 1/16  (shift: free)" } else { "FREE  (drag for snap toggle in params)" };
    ui.painter().text(Pos2::new(draw_r, draw_t + 6.0), Align2::RIGHT_TOP, snap_text,
                       FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
}

// ─── SHAPING strip ──────────────────────────────────────────────────────
fn draw_shaping_strip(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter) {
    let p = ui.painter();
    p.rect_filled(rect, RR_ZERO, c::BG_PANEL);
    p.line_segment([Pos2::new(rect.min.x, rect.min.y), Pos2::new(rect.max.x, rect.min.y)],
                   Stroke::new(1.0, c::LINE));

    let right_w = 56.0;
    let pad_inner = 8.0;
    let pad_avail_w = (rect.width() - right_w) * 0.5;
    let l_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + pad_inner, rect.min.y + pad_inner),
        Vec2::new(pad_avail_w - pad_inner * 2.0, rect.height() - pad_inner * 2.0),
    );
    let r_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + pad_avail_w + pad_inner, rect.min.y + pad_inner),
        Vec2::new(pad_avail_w - pad_inner * 2.0, rect.height() - pad_inner * 2.0),
    );

    let linked = params.shape_linked.value();
    draw_xy_pad(ui, l_rect, "L", &params.shape_x_l, &params.shape_y_l, setter, false);
    let (xr, yr): (&FloatParam, &FloatParam) =
        if linked { (&params.shape_x_l, &params.shape_y_l) }
        else { (&params.shape_x_r, &params.shape_y_r) };
    draw_xy_pad(ui, r_rect, "R", xr, yr, setter, linked);

    let sr_rect = Rect::from_min_max(
        Pos2::new(rect.max.x - right_w, rect.min.y), rect.max,
    );
    {
        let p = ui.painter();
        p.line_segment([Pos2::new(sr_rect.min.x, sr_rect.min.y), Pos2::new(sr_rect.min.x, sr_rect.max.y)],
                       Stroke::new(1.0, c::LINE));
        p.text(Pos2::new(sr_rect.center().x, sr_rect.min.y + 12.0),
               Align2::CENTER_CENTER, "SHAPE",
               FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
    }
    let link_center = Pos2::new(sr_rect.center().x, sr_rect.center().y);
    let link_rect = Rect::from_center_size(link_center, Vec2::splat(30.0));
    let resp = ui.allocate_rect(link_rect, Sense::click());
    if resp.clicked() {
        setter.begin_set_parameter(&params.shape_linked);
        setter.set_parameter(&params.shape_linked, !linked);
        setter.end_set_parameter(&params.shape_linked);
    }
    let p = ui.painter();
    let (fill, ink) = if linked { (c::ACCENT, Color32::WHITE) } else { (c::BG_SURFACE, c::INK_DIM) };
    p.circle_filled(link_center, 15.0, fill);
    if !linked { p.circle_stroke(link_center, 15.0, Stroke::new(1.0, c::LINE)); }
    p.text(link_center, Align2::CENTER_CENTER, "🔗",
           FontId::new(13.0, FontFamily::Proportional), ink);
    p.text(Pos2::new(sr_rect.center().x, sr_rect.max.y - 10.0),
           Align2::CENTER_BOTTOM,
           if linked { "LINKED" } else { "UNLINK" },
           FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
}

fn draw_xy_pad(ui: &mut Ui, rect: Rect, label: &str,
               x_param: &FloatParam, y_param: &FloatParam,
               setter: &ParamSetter, dim: bool) {
    let alpha = if dim { 0.4 } else { 1.0 };
    let resp = if dim { ui.allocate_rect(rect, Sense::hover()) }
               else { ui.allocate_rect(rect, Sense::click_and_drag()) };

    let p = ui.painter();
    p.rect_filled(rect, RR6, c::BG_SURFACE);
    p.rect_stroke(rect, RR6, Stroke::new(1.0, c::LINE), StrokeKind::Middle);
    for i in 1..=2 {
        let x = rect.min.x + i as f32 * rect.width() / 3.0;
        dashed_v(p, Pos2::new(x, rect.min.y + 4.0), rect.height() - 8.0, c::LINE);
    }
    for i in 1..=2 {
        let y = rect.min.y + i as f32 * rect.height() / 3.0;
        dashed_h(p, Pos2::new(rect.min.x + 4.0, y), rect.width() - 8.0, c::LINE);
    }

    let x_norm = (x_param.value() + 1.0) * 0.5;
    let y_norm = y_param.value();

    if resp.drag_started() && !dim {
        setter.begin_set_parameter(x_param);
        setter.begin_set_parameter(y_param);
    }
    if resp.dragged() && !dim {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nx = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            let ny = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
            setter.set_parameter(x_param, nx * 2.0 - 1.0);
            setter.set_parameter(y_param, 1.0 - ny);
        }
    }
    if resp.drag_stopped() && !dim {
        setter.end_set_parameter(x_param);
        setter.end_set_parameter(y_param);
    }

    let cx = rect.min.x + x_norm * rect.width();
    let cy = rect.min.y + (1.0 - y_norm) * rect.height();
    let accent = c::ACCENT.linear_multiply(alpha);
    p.line_segment([Pos2::new(rect.min.x, cy), Pos2::new(rect.max.x, cy)],
                   Stroke::new(0.6, accent.linear_multiply(0.4)));
    p.line_segment([Pos2::new(cx, rect.min.y), Pos2::new(cx, rect.max.y)],
                   Stroke::new(0.6, accent.linear_multiply(0.4)));
    p.circle_filled(Pos2::new(cx, cy), 10.0, accent.linear_multiply(0.25));
    p.circle_filled(Pos2::new(cx, cy), 5.0, accent);

    p.text(Pos2::new(rect.min.x + 8.0, rect.min.y + 6.0), Align2::LEFT_TOP, label,
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
    p.text(Pos2::new(rect.min.x + 8.0, rect.max.y - 4.0), Align2::LEFT_BOTTOM, "← tap 1",
           FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
    p.text(Pos2::new(rect.max.x - 8.0, rect.max.y - 4.0), Align2::RIGHT_BOTTOM, "tap 8 →",
           FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
}

// ─── RIGHT PANEL ────────────────────────────────────────────────────────
fn draw_right(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let tabs_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), RIGHT_TABS_H));
    let footer_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, rect.max.y - RIGHT_FOOTER_H), rect.max,
    );
    let body_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, tabs_rect.max.y),
        Pos2::new(rect.max.x, footer_rect.min.y),
    );

    {
        let p = ui.painter();
        p.line_segment([Pos2::new(rect.min.x, tabs_rect.max.y), Pos2::new(rect.max.x, tabs_rect.max.y)],
                       Stroke::new(1.0, c::LINE));
        p.line_segment([Pos2::new(rect.min.x, footer_rect.min.y), Pos2::new(rect.max.x, footer_rect.min.y)],
                       Stroke::new(1.0, c::LINE));
    }

    let tabs = [
        ("PITCH",   RightTab::Pitch),
        ("PAN",     RightTab::Pan),
        ("GAIN",    RightTab::Gain),
        ("FILTER",  RightTab::Filter),
        ("XFEED",   RightTab::XFeed),
        ("DIFFUSE", RightTab::Diffuse),
    ];
    let tab_w = tabs_rect.width() / tabs.len() as f32;
    for (i, (lab, kind)) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(tabs_rect.min.x + i as f32 * tab_w, tabs_rect.min.y),
            Vec2::new(tab_w, RIGHT_TABS_H),
        );
        let active = st.right_tab == *kind;
        let resp = ui.allocate_rect(r, Sense::click())
            .on_hover_text_at_pointer(*lab);
        if resp.clicked() { st.right_tab = *kind; }
        let p = ui.painter();
        let icon_col = if active { c::ACCENT } else { c::INK_DIM };
        let icon_y = r.min.y + 14.0;
        draw_tab_icon(p, Pos2::new(r.center().x, icon_y), *kind, icon_col);
        // small permanent label below icon
        p.text(Pos2::new(r.center().x, r.max.y - 6.0), Align2::CENTER_BOTTOM, *lab,
               FontId::new(7.5, FontFamily::Proportional), icon_col);
        if active {
            let inset = r.width() * 0.2;
            p.line_segment(
                [Pos2::new(r.min.x + inset, r.max.y - 1.0), Pos2::new(r.max.x - inset, r.max.y - 1.0)],
                Stroke::new(2.0, c::ACCENT),
            );
        }
    }

    draw_right_body(ui, body_rect, params, setter, st);
    draw_right_footer(ui, footer_rect, params, setter);
}

fn draw_tab_icon(p: &egui::Painter, center: Pos2, kind: RightTab, color: Color32) {
    let r = 8.0;
    let st = Stroke::new(1.4, color);
    match kind {
        RightTab::Pitch => {
            p.line_segment([Pos2::new(center.x - r, center.y + r * 0.5), Pos2::new(center.x - r, center.y - r * 0.2)], st);
            p.line_segment([Pos2::new(center.x - r, center.y - r * 0.2), Pos2::new(center.x, center.y - r * 0.2)], st);
            p.line_segment([Pos2::new(center.x, center.y - r * 0.2), Pos2::new(center.x, center.y - r)], st);
            p.line_segment([Pos2::new(center.x, center.y - r), Pos2::new(center.x + r, center.y - r)], st);
            p.circle_filled(Pos2::new(center.x + r, center.y - r), 1.6, color);
        }
        RightTab::Pan => {
            p.line_segment([Pos2::new(center.x - r, center.y), Pos2::new(center.x - r * 0.5, center.y)], st);
            p.line_segment([Pos2::new(center.x - r * 0.5, center.y), Pos2::new(center.x - r * 0.25, center.y - r * 0.5)], st);
            p.line_segment([Pos2::new(center.x - r * 0.25, center.y - r * 0.5), Pos2::new(center.x - r * 0.25, center.y + r * 0.5)], st);
            p.line_segment([Pos2::new(center.x + r, center.y), Pos2::new(center.x + r * 0.5, center.y)], st);
            p.line_segment([Pos2::new(center.x + r * 0.5, center.y), Pos2::new(center.x + r * 0.25, center.y - r * 0.5)], st);
            p.line_segment([Pos2::new(center.x + r * 0.25, center.y - r * 0.5), Pos2::new(center.x + r * 0.25, center.y + r * 0.5)], st);
            p.circle_filled(center, 1.2, color);
        }
        RightTab::Gain => {
            for (i, h) in [3.0_f32, 5.0, 8.0, 11.0].iter().enumerate() {
                let x = center.x - r + i as f32 * (r * 0.5);
                p.rect_filled(Rect::from_min_size(Pos2::new(x, center.y + r - h), Vec2::new(2.0, *h)),
                              RR_ZERO, color);
            }
        }
        RightTab::Filter => {
            let n = 24;
            let pts: Vec<Pos2> = (0..=n).map(|i| {
                let t = i as f32 / n as f32;
                let x = center.x - r + t * r * 2.0;
                let y = if t < 0.55 { center.y - r * 0.4 }
                        else { let k = (t - 0.55) / 0.45;
                               center.y - r * 0.4 + k * k * r * 1.2 };
                Pos2::new(x, y)
            }).collect();
            for w in pts.windows(2) { p.line_segment([w[0], w[1]], st); }
        }
        RightTab::XFeed => {
            p.line_segment([Pos2::new(center.x - r, center.y - r * 0.5), Pos2::new(center.x + r, center.y - r * 0.5)], st);
            p.line_segment([Pos2::new(center.x + r, center.y - r * 0.5), Pos2::new(center.x + r * 0.5, center.y - r)], st);
            p.line_segment([Pos2::new(center.x + r, center.y + r * 0.5), Pos2::new(center.x - r, center.y + r * 0.5)], st);
            p.line_segment([Pos2::new(center.x - r, center.y + r * 0.5), Pos2::new(center.x - r * 0.5, center.y + r)], st);
        }
        RightTab::Diffuse => {
            p.circle_filled(center, 2.0, color);
            for (dx, dy, op) in [
                (-6.0, -4.0, 0.8_f32), (6.0, -3.0, 0.8),
                (-5.0, 4.0, 0.6), (5.0, 5.0, 0.6),
                (-9.0, 0.0, 0.4), (9.0, 1.0, 0.4),
            ] {
                p.circle_filled(Pos2::new(center.x + dx, center.y + dy), 1.4, color.linear_multiply(op));
            }
        }
    }
}

fn draw_right_body(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let pad = 10.0;
    let row_h = 26.0;
    let row_gap = 4.0;
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

    match st.right_tab {
        RightTab::Pitch => {
            for i in 0..n_active {
                let r = Rect::from_min_size(
                    Pos2::new(rect.min.x + pad, rect.min.y + pad + i as f32 * (row_h + row_gap)),
                    Vec2::new(rect.width() - pad * 2.0, row_h),
                );
                tap_row_pitch(ui, r, i, &params.taps[i], setter);
            }
        }
        RightTab::Pan => {
            for i in 0..n_active {
                let r = Rect::from_min_size(
                    Pos2::new(rect.min.x + pad, rect.min.y + pad + i as f32 * (row_h + row_gap)),
                    Vec2::new(rect.width() - pad * 2.0, row_h),
                );
                tap_row_slider(ui, r, i, &params.taps[i].active, &params.taps[i].pan, setter,
                               -1.0, 1.0, true, |v| {
                    if v.abs() < 0.05 { "C".to_string() }
                    else if v > 0.0 { format!("R{}", (v * 100.0).round() as i32) }
                    else { format!("L{}", (-v * 100.0).round() as i32) }
                });
            }
        }
        RightTab::Gain => {
            for i in 0..n_active {
                let r = Rect::from_min_size(
                    Pos2::new(rect.min.x + pad, rect.min.y + pad + i as f32 * (row_h + row_gap)),
                    Vec2::new(rect.width() - pad * 2.0, row_h),
                );
                tap_row_slider(ui, r, i, &params.taps[i].active, &params.taps[i].gain_db, setter,
                               -60.0, 6.0, false, |v| format!("{:.0} dB", v));
            }
        }
        RightTab::Filter => stub_panel(ui, rect, "Filter", "Per-tap SVF coming soon."),
        RightTab::XFeed  => stub_panel(ui, rect, "Crossfeed", "Route tap outputs into adjacent taps or the opposite stereo channel."),
        RightTab::Diffuse => stub_panel(ui, rect, "Diffusion", "Allpass network per tap — blurs transients into smear."),
    }
}

fn stub_panel(ui: &mut Ui, rect: Rect, title: &str, desc: &str) {
    let p = ui.painter();
    p.text(Pos2::new(rect.min.x + 14.0, rect.min.y + 14.0), Align2::LEFT_TOP,
           title.to_ascii_uppercase(),
           FontId::new(10.0, FontFamily::Proportional), c::INK_FAINT);
    let max_w = rect.width() - 28.0;
    let font = FontId::new(10.0, FontFamily::Proportional);
    let mut y = rect.min.y + 36.0;
    let words: Vec<&str> = desc.split_whitespace().collect();
    let mut line = String::new();
    for w in words {
        let trial = if line.is_empty() { w.to_string() } else { format!("{} {}", line, w) };
        let tw = ui.fonts(|f| f.layout_no_wrap(trial.clone(), font.clone(), c::INK_DIM)).rect.width();
        if tw > max_w && !line.is_empty() {
            p.text(Pos2::new(rect.min.x + 14.0, y), Align2::LEFT_TOP, &line, font.clone(), c::INK_DIM);
            y += 14.0;
            line = w.to_string();
        } else { line = trial; }
    }
    if !line.is_empty() {
        p.text(Pos2::new(rect.min.x + 14.0, y), Align2::LEFT_TOP, &line, font, c::INK_DIM);
    }
}

fn tap_row_pitch(ui: &mut Ui, rect: Rect, idx: usize, tap: &crate::params::TapParams, setter: &ParamSetter) {
    let active = tap.active.value();
    let alpha = if active { 1.0 } else { 0.4 };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius { nw: 5, ne: 5, sw: 5, se: 5 }, c::BG_SURFACE.linear_multiply(alpha));
    p.rect_stroke(rect, CornerRadius { nw: 5, ne: 5, sw: 5, se: 5 }, Stroke::new(1.0, c::LINE_SOFT), StrokeKind::Middle);
    p.text(Pos2::new(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER,
           &format!("{:02}", idx + 1),
           FontId::new(10.0, FontFamily::Proportional), c::INK_FAINT);

    let dot_center = Pos2::new(rect.min.x + 32.0, rect.center().y);
    let dot_rect = Rect::from_center_size(dot_center, Vec2::splat(14.0));
    let resp = ui.allocate_rect(dot_rect, Sense::click());
    if resp.clicked() {
        setter.begin_set_parameter(&tap.active);
        setter.set_parameter(&tap.active, !active);
        setter.end_set_parameter(&tap.active);
    }
    let p = ui.painter();
    if active { p.circle_filled(dot_center, 4.0, c::ACCENT); }
    else { p.circle_stroke(dot_center, 4.0, Stroke::new(1.0, c::LINE)); }

    let minus = Rect::from_min_size(Pos2::new(rect.min.x + 50.0, rect.center().y - 9.0), Vec2::new(18.0, 18.0));
    if click_box(ui, minus, "−") {
        let v = tap.pitch_semitones.value();
        setter.begin_set_parameter(&tap.pitch_semitones);
        setter.set_parameter(&tap.pitch_semitones, (v - 1).max(-12));
        setter.end_set_parameter(&tap.pitch_semitones);
    }
    let val_rect = Rect::from_min_size(
        Pos2::new(minus.max.x, rect.center().y - 9.0),
        Vec2::new(rect.max.x - minus.max.x - 50.0, 18.0),
    );
    let pitch = tap.pitch_semitones.value();
    let label = if pitch > 0 { format!("+{}", pitch) } else { format!("{}", pitch) };
    let p = ui.painter();
    p.text(val_rect.center(), Align2::CENTER_CENTER, &label,
           FontId::new(11.0, FontFamily::Proportional),
           if pitch != 0 { c::ACCENT_2 } else { c::INK });

    let plus = Rect::from_min_size(Pos2::new(val_rect.max.x, rect.center().y - 9.0), Vec2::new(18.0, 18.0));
    if click_box(ui, plus, "+") {
        let v = tap.pitch_semitones.value();
        setter.begin_set_parameter(&tap.pitch_semitones);
        setter.set_parameter(&tap.pitch_semitones, (v + 1).min(12));
        setter.end_set_parameter(&tap.pitch_semitones);
    }
    let p = ui.painter();
    p.text(Pos2::new(rect.max.x - 12.0, rect.center().y), Align2::RIGHT_CENTER, "st",
           FontId::new(9.0, FontFamily::Proportional), c::INK_FAINT);
}

fn tap_row_slider<F: Fn(f32) -> String>(
    ui: &mut Ui, rect: Rect, idx: usize,
    active_param: &BoolParam, value_param: &FloatParam,
    setter: &ParamSetter, min: f32, max: f32, bipolar: bool, formatter: F,
) {
    let active = active_param.value();
    let alpha = if active { 1.0 } else { 0.4 };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius { nw: 5, ne: 5, sw: 5, se: 5 }, c::BG_SURFACE.linear_multiply(alpha));
    p.rect_stroke(rect, CornerRadius { nw: 5, ne: 5, sw: 5, se: 5 }, Stroke::new(1.0, c::LINE_SOFT), StrokeKind::Middle);
    p.text(Pos2::new(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER,
           &format!("{:02}", idx + 1),
           FontId::new(10.0, FontFamily::Proportional), c::INK_FAINT);

    let dot_center = Pos2::new(rect.min.x + 32.0, rect.center().y);
    if active { p.circle_filled(dot_center, 4.0, c::ACCENT.linear_multiply(alpha)); }
    else { p.circle_stroke(dot_center, 4.0, Stroke::new(1.0, c::LINE)); }

    let slider_left = rect.min.x + 52.0;
    let slider_right = rect.max.x - 56.0;
    let slider_y = rect.center().y;
    let slider_rect = Rect::from_min_max(
        Pos2::new(slider_left, slider_y - 3.0),
        Pos2::new(slider_right, slider_y + 3.0),
    );
    let p = ui.painter();
    p.rect_filled(slider_rect, CornerRadius { nw: 3, ne: 3, sw: 3, se: 3 }, c::BG_ELEV);
    p.rect_stroke(slider_rect, CornerRadius { nw: 3, ne: 3, sw: 3, se: 3 },
                  Stroke::new(1.0, c::LINE), StrokeKind::Middle);
    let v = value_param.value();
    let n = ((v - min) / (max - min)).clamp(0.0, 1.0);
    if bipolar {
        let mid_x = slider_left + (slider_right - slider_left) * 0.5;
        let cur_x = slider_left + n * (slider_right - slider_left);
        let (fl, fr) = (mid_x.min(cur_x), mid_x.max(cur_x));
        let fr_rect = Rect::from_min_max(Pos2::new(fl, slider_y - 3.0), Pos2::new(fr, slider_y + 3.0));
        p.rect_filled(fr_rect, CornerRadius { nw: 3, ne: 3, sw: 3, se: 3 }, c::ACCENT.linear_multiply(alpha));
    } else {
        let fr_rect = Rect::from_min_max(
            slider_rect.min,
            Pos2::new(slider_left + n * (slider_right - slider_left), slider_rect.max.y),
        );
        p.rect_filled(fr_rect, CornerRadius { nw: 3, ne: 3, sw: 3, se: 3 }, c::ACCENT.linear_multiply(alpha));
    }
    let tx = slider_left + n * (slider_right - slider_left);
    p.circle_filled(Pos2::new(tx, slider_y), 5.0, Color32::WHITE);
    p.circle_stroke(Pos2::new(tx, slider_y), 5.0, Stroke::new(1.0, c::ACCENT.linear_multiply(alpha)));

    let hit = Rect::from_min_max(Pos2::new(slider_left - 4.0, slider_y - 10.0),
                                  Pos2::new(slider_right + 4.0, slider_y + 10.0));
    let resp = ui.allocate_rect(hit, Sense::click_and_drag());
    if resp.drag_started() { setter.begin_set_parameter(value_param); }
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nx = ((pos.x - slider_left) / (slider_right - slider_left)).clamp(0.0, 1.0);
            setter.set_parameter(value_param, min + nx * (max - min));
        }
    }
    if resp.drag_stopped() { setter.end_set_parameter(value_param); }

    let p = ui.painter();
    p.text(Pos2::new(rect.max.x - 8.0, rect.center().y), Align2::RIGHT_CENTER,
           &formatter(v),
           FontId::new(10.0, FontFamily::Proportional), c::INK);
}

fn click_box(ui: &mut Ui, rect: Rect, label: &str) -> bool {
    let resp = ui.allocate_rect(rect, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, RR, c::BG_ELEV);
    p.rect_stroke(rect, RR, Stroke::new(1.0, c::LINE), StrokeKind::Middle);
    p.text(rect.center(), Align2::CENTER_CENTER, label,
           FontId::new(11.0, FontFamily::Proportional), c::INK_DIM);
    resp.clicked()
}

fn draw_right_footer(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter) {
    let half = rect.width() * 0.5;
    let fb_rect = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + half, rect.max.y));
    knob_at(ui, Pos2::new(fb_rect.min.x + 28.0, fb_rect.center().y), 36.0, &params.feedback, setter);
    let p = ui.painter();
    p.text(Pos2::new(fb_rect.min.x + 56.0, fb_rect.center().y - 6.0), Align2::LEFT_CENTER,
           "FEEDBACK", FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
    p.text(Pos2::new(fb_rect.min.x + 56.0, fb_rect.center().y + 8.0), Align2::LEFT_CENTER,
           &format!("{:.0}%", params.feedback.value() * 100.0),
           FontId::new(13.0, FontFamily::Proportional), c::INK);

    // tap count: − [n] +
    let ct_rect = Rect::from_min_max(Pos2::new(rect.min.x + half, rect.min.y), rect.max);
    let n = params.num_taps_active.value();
    let label_y = ct_rect.center().y - 18.0;
    p.text(Pos2::new(ct_rect.max.x - 12.0, label_y), Align2::RIGHT_CENTER, "TAPS",
           FontId::new(8.0, FontFamily::Proportional), c::INK_FAINT);
    p.text(Pos2::new(ct_rect.max.x - 12.0, ct_rect.center().y + 2.0), Align2::RIGHT_CENTER,
           &format!("{}", n),
           FontId::new(22.0, FontFamily::Proportional), c::ACCENT_2);
    // − / + buttons
    let plus = Rect::from_min_size(Pos2::new(ct_rect.max.x - 44.0, ct_rect.max.y - 24.0), Vec2::new(20.0, 20.0));
    let minus = Rect::from_min_size(Pos2::new(plus.min.x - 22.0, plus.min.y), Vec2::new(20.0, 20.0));
    if click_box(ui, minus, "−") {
        setter.begin_set_parameter(&params.num_taps_active);
        setter.set_parameter(&params.num_taps_active, (n - 1).max(1));
        setter.end_set_parameter(&params.num_taps_active);
    }
    if click_box(ui, plus, "+") {
        setter.begin_set_parameter(&params.num_taps_active);
        setter.set_parameter(&params.num_taps_active, (n + 1).min(NUM_TAPS as i32));
        setter.end_set_parameter(&params.num_taps_active);
    }
}

#[allow(dead_code)]
fn _unused_avoid_warns() {
    let _ = SYNC_DIVS;
    let _ = snap_to_div(0.5);
}
