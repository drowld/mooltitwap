//! epo-* widgets — egui ports of the Inertia/Epoché design kit (from cumulus):
//! slider (cap · inset track · 4×12 handle · mono value), beveled knob, chip
//! toggles, segmented controls, readout pills, VU bars, preset slot, titlebar
//! icons, and chip-on-border cards. All widths are explicit, so layout is
//! overflow-proof by construction.
//!
//! This is a reusable widget kit ported wholesale from cumulus; some widgets and
//! tokens aren't used by the flanger yet, so dead-code is allowed module-wide.
#![allow(dead_code)]

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use nice_plug::prelude::*;

use super::theme::{self, ui_scale, FAINT, FRAME, G, INK, INK2, INSET, M, PANEL, PAPER, SOFT};
use super::tokens as tk;

use std::sync::atomic::{AtomicU8, Ordering as AOrd};

/// Live mod-feedback render style (kept from cumulus so slider/knob bodies port
/// verbatim). y-flanger has no modulation, so `mod_viz` is always None and these
/// paths stay inert; the toggle is wired to the options menu anyway.
static MODVIZ_STYLE: AtomicU8 = AtomicU8::new(0);
pub fn set_modviz_style(v: u8) {
    MODVIZ_STYLE.store(v, AOrd::Relaxed);
}
pub fn modviz_style() -> u8 {
    MODVIZ_STYLE.load(AOrd::Relaxed)
}

/// Live modulation feedback for one control, in normalized track space.
#[derive(Clone, Copy)]
pub struct ModViz {
    pub live: f32,
    pub depth: f32,
}

/// Accent role for a control. Ink = neutral/global; G/M are the kit's green/teal;
/// Hue(i) picks one of the 6 indexed Inertia hues for section color-coding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Ink,
    G,
    M,
    Hue(usize),
}

impl Accent {
    fn col(self) -> Color32 {
        match self {
            Accent::Ink => INK(),
            Accent::G => G(),
            Accent::M => M(),
            Accent::Hue(i) => theme::accent(i),
        }
    }
    fn cap(self) -> Color32 {
        match self {
            Accent::Ink => INK2(),
            Accent::G => G(),
            Accent::M => M(),
            Accent::Hue(i) => theme::accent(i),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw-multitouch router (iPad). egui synthesizes its single pointer from the
// FIRST finger only, so standard widgets are single-touch. On touch platforms
// (`theme::touch()`) widgets additionally call `touch_interact`: the iOS bridge
// forwards EVERY finger as a raw `egui::Event::Touch`; each touch id is claimed
// by the widget whose rect contains its Start, and that widget alone receives
// its Move deltas until End/Cancel — so N fingers drive N controls at once.
//
// Notes:
// - A finger is only claimed by an ENABLED widget whose layer is the topmost
//   one under the finger — a tap on the options menu, the preset palette or an
//   enum picker must not also toggle / drag the control underneath it, and a
//   greyed-out (dim_if) knob must not turn.
// - A tap is a press shorter than egui's long-press threshold, so a long press
//   opens the right-click picker WITHOUT also stepping the value.
// - A widget that vanishes mid-hold (mode switch) leaks its grab entry; a later
//   Start with a reused touch id reclaims it (Start always overwrites), so the
//   map can't grow beyond the number of simultaneous fingers.
// ─────────────────────────────────────────────────────────────────────────────
use std::cell::RefCell;
use std::collections::HashMap;

struct TouchGrab {
    owner: egui::Id,
    last: Pos2,
    moved: f32, // accumulated |delta| px, for tap-vs-drag discrimination
    born: f64,  // egui time of the Start, for tap-vs-long-press discrimination
}

thread_local! {
    static TOUCH_GRABS: RefCell<HashMap<u64, TouchGrab>> = RefCell::new(HashMap::new());
}

/// One widget's view of the raw-touch stream this frame.
#[derive(Default, Clone, Copy)]
pub struct TouchResp {
    /// a finger landed in the rect this frame (grab claimed)
    pub started: bool,
    /// summed drag delta (this frame) of all fingers this widget owns
    pub delta: Vec2,
    /// last position of a finger this widget owns (absolute-pos widgets: XY pad)
    pub pos: Option<Pos2>,
    /// a finger lifted inside the rect having barely moved → a tap
    pub tapped: bool,
    /// ≥1 finger currently owned by this widget (after this frame's events)
    pub active: bool,
}

/// Process this frame's raw touches for the widget owning `rect`. Call at most
/// once per widget per frame. Returns `TouchResp::default()` off-touch.
pub fn touch_interact(ui: &egui::Ui, id: egui::Id, rect: Rect) -> TouchResp {
    let mut out = TouchResp::default();
    if !theme::touch() {
        return out;
    }
    let (events, now): (Vec<(u64, egui::TouchPhase, Pos2)>, f64) = ui.input(|i| {
        let ev = i
            .events
            .iter()
            .filter_map(|e| {
                if let egui::Event::Touch { id, phase, pos, .. } = *e {
                    Some((id.0, phase, pos))
                } else {
                    None
                }
            })
            .collect();
        (ev, i.time)
    });
    let long_press = ui.ctx().options(|o| o.input_options.max_click_duration);
    // Only registered Areas (menu, palette, pickers) report a layer; the page
    // itself is drawn on egui's bare background layer, which reads as `None`.
    let claimable = |pos: Pos2| {
        ui.is_enabled()
            && match ui.ctx().layer_id_at(pos) {
                Some(top) => top == ui.layer_id(),
                None => ui.layer_id() == egui::LayerId::background(),
            }
    };
    TOUCH_GRABS.with(|g| {
        let mut g = g.borrow_mut();
        for (tid, phase, pos) in events {
            match phase {
                egui::TouchPhase::Start => {
                    if rect.contains(pos) && claimable(pos) {
                        // overwrite any stale entry (iOS reuses touch ids)
                        g.insert(tid, TouchGrab { owner: id, last: pos, moved: 0.0, born: now });
                        out.started = true;
                        out.pos = Some(pos);
                    }
                }
                egui::TouchPhase::Move => {
                    if let Some(t) = g.get_mut(&tid) {
                        if t.owner == id {
                            let d = pos - t.last;
                            out.delta += d;
                            t.moved += d.length();
                            t.last = pos;
                            out.pos = Some(pos);
                        }
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                    let mine = g.get(&tid).map(|t| t.owner == id).unwrap_or(false);
                    if mine {
                        let (moved, held) = (g[&tid].moved, now - g[&tid].born);
                        if phase == egui::TouchPhase::End && moved < 8.0 && held < long_press && rect.contains(pos) {
                            out.tapped = true;
                        }
                        g.remove(&tid);
                    }
                }
            }
        }
        out.active = g.values().any(|t| t.owner == id);
    });
    out
}

/// Touch-aware "clicked": on touch platforms a raw-touch tap (ANY finger, not
/// just egui's synthesized first-finger pointer); on desktop the normal click.
/// This is what makes chips/buttons tappable while another finger holds a knob.
pub fn tap_or_click(ui: &egui::Ui, resp: &egui::Response) -> bool {
    if theme::touch() {
        touch_interact(ui, resp.id, resp.rect).tapped
    } else {
        resp.clicked()
    }
}

fn compact(mut v: String) -> String {
    if v.len() > 9 {
        v.truncate(9);
    }
    v
}

/// Extra slider decorations (kept from cumulus for port-compatibility).
#[derive(Default, Clone, Copy)]
pub struct SliderOpts {
    pub bip: bool,
    pub ticks: [f32; 8],
    pub n_ticks: usize,
    pub mod_band: f32,
    pub mod_viz: Option<ModViz>,
}

impl SliderOpts {
    pub fn bip() -> Self {
        Self { bip: true, ..Default::default() }
    }
}

/// The epo-slider row: `[CAP][track][value]`, total width = `total_w`.
pub fn slider<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    total_w: f32,
    cap_w: f32,
    accent: Accent,
    bip: bool,
) {
    let opts = SliderOpts { bip, ..Default::default() };
    slider_ex(ui, setter, param, label, total_w, cap_w, accent, &opts);
}

/// Slider with optional decorations.
#[allow(clippy::too_many_arguments)]
pub fn slider_ex<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    total_w: f32,
    cap_w: f32,
    accent: Accent,
    opts: &SliderOpts,
) -> egui::Response {
    let bip = opts.bip;
    let s = ui_scale(ui);
    let value_w = theme::SLIDER_VALUE_W * s;
    let gap = tk::space_sm() * s;
    let track_w = (total_w - cap_w - value_w - 2.0 * gap).max(theme::SLIDER_TRACK_MIN * s);
    let h = theme::SLIDER_H * s;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_sized(
            [cap_w, h],
            egui::Label::new(
                egui::RichText::new(label).size(tk::font_label() * s * theme::font_scale()).color(accent.cap()).strong(),
            ),
        );

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(track_w, h), Sense::click_and_drag());
        let resp_out = resp.clone();
        let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let n0 = param.modulated_normalized_value();

        if resp.double_clicked() {
            setter.begin_set_parameter(param);
            setter.set_parameter(param, param.default_plain_value());
            setter.end_set_parameter(param);
        } else {
            if resp.drag_started() {
                setter.begin_set_parameter(param);
            }
            if resp.dragged() || resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let mut n = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    if ui.input(|i| i.modifiers.shift) {
                        n = n0 + (n - n0) * 0.15;
                    }
                    if resp.clicked() {
                        setter.begin_set_parameter(param);
                    }
                    setter.set_parameter(param, param.preview_plain(n));
                    if resp.clicked() {
                        setter.end_set_parameter(param);
                    }
                }
            }
            if resp.drag_stopped() {
                setter.end_set_parameter(param);
            }
        }

        let n = param.modulated_normalized_value();
        let painter = ui.painter_at(rect);
        let cy = rect.center().y;
        let col = if ui.is_enabled() {
            accent.col()
        } else {
            theme::with_alpha(accent.col(), 90)
        };
        painter.line_segment(
            [Pos2::new(rect.left(), cy), Pos2::new(rect.right(), cy)],
            Stroke::new(theme::STROKE_TRACK * s, theme::line2()),
        );
        let (f0, f1) = if bip { (n.min(0.5), n.max(0.5)) } else { (0.0, n) };
        let x0 = rect.left() + f0 * rect.width();
        let x1 = rect.left() + f1 * rect.width();
        painter.line_segment(
            [Pos2::new(x0, cy), Pos2::new(x1, cy)],
            Stroke::new(theme::STROKE_TRACK * s, col),
        );
        if bip {
            painter.line_segment(
                [
                    Pos2::new(rect.center().x, cy - 3.5 * s),
                    Pos2::new(rect.center().x, cy + 3.5 * s),
                ],
                Stroke::new(tk::stroke_outline() * s, theme::with_alpha(SOFT(), 150)),
            );
        }
        if opts.mod_band > 0.001 {
            let b0 = (n - opts.mod_band * 0.5).clamp(0.0, 1.0);
            let b1 = (n + opts.mod_band * 0.5).clamp(0.0, 1.0);
            painter.line_segment(
                [
                    Pos2::new(rect.left() + b0 * rect.width(), cy),
                    Pos2::new(rect.left() + b1 * rect.width(), cy),
                ],
                Stroke::new(5.0 * s, theme::with_alpha(M(), 70)),
            );
        }
        for t in opts.ticks.iter().take(opts.n_ticks) {
            let tx = rect.left() + t.clamp(0.0, 1.0) * rect.width();
            painter.line_segment(
                [Pos2::new(tx, cy - 4.5 * s), Pos2::new(tx, cy + 4.5 * s)],
                Stroke::new(2.0 * s, theme::with_alpha(G(), 128)),
            );
        }
        let mvs = modviz_style();
        let to_x = |v: f32| rect.left() + v.clamp(0.0, 1.0) * rect.width();
        if let Some(mv) = opts.mod_viz {
            if mvs == 2 && mv.depth > 0.001 {
                painter.line_segment(
                    [Pos2::new(to_x(n - mv.depth), cy), Pos2::new(to_x(n + mv.depth), cy)],
                    Stroke::new(6.0 * s, theme::with_alpha(M(), 45)),
                );
            }
        }
        let hw = 4.0 * s;
        let hx = rect.left() + hw * 0.5 + n * (rect.width() - hw);
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(hx, cy), Vec2::new(hw, 12.0 * s)),
            2.0 * s,
            col,
        );
        if let Some(mv) = opts.mod_viz {
            if mvs != 3 {
                let live_x = to_x(n + mv.live);
                match mvs {
                    1 => {
                        painter.line_segment(
                            [Pos2::new(hx, cy), Pos2::new(live_x, cy)],
                            Stroke::new(theme::STROKE_TRACK * s, theme::with_alpha(M(), 165)),
                        );
                    }
                    2 => {
                        painter.rect_filled(
                            Rect::from_center_size(Pos2::new(live_x, cy), Vec2::new(3.0 * s, 10.0 * s)),
                            1.5 * s,
                            theme::with_alpha(M(), 225),
                        );
                    }
                    _ => {
                        painter.line_segment(
                            [Pos2::new(hx, cy), Pos2::new(live_x, cy)],
                            Stroke::new(tk::stroke_outline() * s, theme::with_alpha(M(), 130)),
                        );
                        painter.rect_filled(
                            Rect::from_center_size(Pos2::new(live_x, cy), Vec2::new(3.0 * s, 12.0 * s)),
                            2.0 * s,
                            theme::with_alpha(M(), 210),
                        );
                    }
                }
            }
        }

        ui.allocate_ui_with_layout(
            Vec2::new(value_w, h),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(compact(param.normalized_value_to_string(n, true)))
                        .monospace()
                        .size(tk::font_label() * s * theme::font_scale())
                        .color(SOFT()),
                );
            },
        );
        resp_out
    })
    .inner
}

/// Chip toggle (the kit's bordered pill): neutral ink wash when on.
pub fn toggle(ui: &mut egui::Ui, label: &str, param: &BoolParam, setter: &ParamSetter) {
    if chip(ui, label, param.value()) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !param.value());
        setter.end_set_parameter(param);
    }
}

/// Momentary chip: fires once per click.
pub fn momentary(ui: &mut egui::Ui, label: &str, param: &BoolParam, setter: &ParamSetter) {
    if chip(ui, label, false) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, true);
        setter.end_set_parameter(param);
        setter.begin_set_parameter(param);
        setter.set_parameter(param, false);
        setter.end_set_parameter(param);
    }
}

/// Cycling menu chip showing the current value; clicks advance.
pub fn cycle(ui: &mut egui::Ui, param: &IntParam, setter: &ParamSetter, max: i32) {
    if chip(ui, &param.to_string(), false) {
        let v = param.value();
        setter.begin_set_parameter(param);
        setter.set_parameter(param, if v >= max { 0 } else { v + 1 });
        setter.end_set_parameter(param);
    }
}

/// Knob interaction shared by every knob painter: vertical drag (accumulated
/// per gesture so stepped params track 1:1), shift = fine, double-click =
/// default, and on touch platforms its own finger via the raw-touch router.
/// Brackets every change in a host gesture. Returns true while this knob is
/// being driven (a drag or touch is live) — callers use it for "last touched".
pub fn knob_drive<P: Param>(
    ui: &egui::Ui,
    setter: &ParamSetter,
    param: &P,
    rect: Rect,
    resp: &egui::Response,
) -> bool {
    let s = ui_scale(ui);
    // Drag state: an ACCUMULATED normalized value across the whole
    // gesture (egui temp memory). Setting from `current + delta` each
    // frame is subtly wrong for STEPPED params (enum/int knobs): the
    // param snaps to a variant every frame, eating the fractional
    // remainder — so tiny per-frame deltas never sum past a step
    // boundary and the knob feels sticky (only fast flicks advanced
    // it). Accumulating from gesture start makes stepped knobs track
    // the finger/mouse 1:1; continuous knobs are unchanged.
    let acc_id = resp.id.with("drag_acc");
    let acc = ui.ctx().data_mut(|d| d.get_temp::<f32>(acc_id));
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    } else if theme::touch() {
        // Raw-multitouch path: this knob follows ITS OWN finger via the
        // touch router, so several knobs can be turned at once. (The
        // egui pointer path below is skipped — it only mirrors the
        // first finger and would double-drive this knob.)
        let t = touch_interact(ui, resp.id, rect);
        let mut cur = acc;
        if t.started && cur.is_none() {
            setter.begin_set_parameter(param);
            cur = Some(param.modulated_normalized_value());
        }
        if let Some(a) = cur {
            let a = (a - t.delta.y / (140.0 * s)).clamp(0.0, 1.0);
            if t.delta.y != 0.0 {
                setter.set_parameter(param, param.preview_plain(a));
            }
            if t.active {
                ui.ctx().data_mut(|d| d.insert_temp(acc_id, a));
            } else {
                setter.end_set_parameter(param);
                ui.ctx().data_mut(|d| d.remove::<f32>(acc_id));
            }
        }
    } else {
        if resp.drag_started() {
            setter.begin_set_parameter(param);
            ui.ctx().data_mut(|d| {
                d.insert_temp(acc_id, param.modulated_normalized_value())
            });
        }
        if resp.dragged() {
            let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
            let dn = -resp.drag_delta().y / (140.0 * s) * fine;
            let a = (acc.unwrap_or_else(|| param.modulated_normalized_value()) + dn)
                .clamp(0.0, 1.0);
            setter.set_parameter(param, param.preview_plain(a));
            ui.ctx().data_mut(|d| d.insert_temp(acc_id, a));
        }
        if resp.drag_stopped() {
            setter.end_set_parameter(param);
            ui.ctx().data_mut(|d| d.remove::<f32>(acc_id));
        }
    }
    acc_live(ui, resp)
}

/// True while `knob_drive` holds a gesture for this response's knob.
fn acc_live(ui: &egui::Ui, resp: &egui::Response) -> bool {
    resp.dragged() || resp.double_clicked()
        || ui.ctx().data(|d| d.get_temp::<f32>(resp.id.with("drag_acc")).is_some())
}

/// A5 beveled knob (the kit's knobSVG): 270° arc from -135°, bevel disc,
/// accent pointer. Vertical drag, shift = fine, double-click = reset.
pub fn knob<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    size: f32,
    accent: Accent,
    bip: bool,
) -> egui::Response {
    knob_ex(ui, setter, param, label, size, accent, bip, None, None)
}

/// Knob with optional live modulation feedback and display override.
#[allow(clippy::too_many_arguments)]
pub fn knob_ex<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    size: f32,
    accent: Accent,
    bip: bool,
    mod_viz: Option<ModViz>,
    disp: Option<&str>,
) -> egui::Response {
    let s = ui_scale(ui);
    let d = size * s;
    ui.allocate_ui_with_layout(
        Vec2::new(d + theme::KNOB_PAD_W * s, d + theme::KNOB_STACK_H * s),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(label).size(tk::font_body() * s * theme::font_scale()).color(accent.cap()).strong(),
            );
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(d, d), Sense::click_and_drag());
            let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical);

            knob_drive(ui, setter, param, rect, &resp);

            let n = param.modulated_normalized_value();
            let painter = ui.painter_at(rect.expand(4.0 * s));
            let c = rect.center();
            let r = d * 0.5 - 2.0 * s;
            let col = accent.col();
            let arc = |v0: f32, v1: f32, r: f32| -> Vec<Pos2> {
                let a0 = (-135.0 + 270.0 * v0).to_radians();
                let a1 = (-135.0 + 270.0 * v1).to_radians();
                (0..=24)
                    .map(|i| {
                        let t = a0 + (a1 - a0) * i as f32 / 24.0;
                        Pos2::new(c.x + r * t.sin(), c.y - r * t.cos())
                    })
                    .collect()
            };
            painter.add(egui::Shape::line(arc(0.0, 1.0, r), Stroke::new(2.0 * s, theme::line2())));
            let mvs = modviz_style();
            if let Some(mv) = mod_viz {
                if mvs == 2 && mv.depth > 0.001 {
                    painter.add(egui::Shape::line(
                        arc((n - mv.depth).clamp(0.0, 1.0), (n + mv.depth).clamp(0.0, 1.0), r),
                        Stroke::new(5.0 * s, theme::with_alpha(M(), 45)),
                    ));
                }
            }
            let (v0, v1) = if bip { (n.min(0.5), n.max(0.5)) } else { (0.0, n.max(0.004)) };
            painter.add(egui::Shape::line(arc(v0, v1, r), Stroke::new(2.4 * s, col)));
            if let Some(mv) = mod_viz {
                if mvs != 3 {
                    let live = (n + mv.live).clamp(0.0, 1.0);
                    if mvs == 2 {
                        let a = (-135.0 + 270.0 * live).to_radians();
                        painter.line_segment(
                            [
                                Pos2::new(c.x + (r - 2.0 * s) * a.sin(), c.y - (r - 2.0 * s) * a.cos()),
                                Pos2::new(c.x + (r + 2.5 * s) * a.sin(), c.y - (r + 2.5 * s) * a.cos()),
                            ],
                            Stroke::new(2.0 * s, theme::with_alpha(M(), 225)),
                        );
                    } else {
                        let (w, al) = if mvs == 1 { (3.0 * s, 170) } else { (2.4 * s, 205) };
                        painter.add(egui::Shape::line(
                            arc(n.min(live), n.max(live), r),
                            Stroke::new(w, theme::with_alpha(M(), al)),
                        ));
                    }
                }
            }
            if bip {
                painter.line_segment(
                    [Pos2::new(c.x, rect.top() - 1.0 * s), Pos2::new(c.x, rect.top() + 3.0 * s)],
                    Stroke::new(1.6 * s, theme::with_alpha(col, 140)),
                );
            }
            let cr = r - 5.0 * s.max(0.8);
            painter.circle_filled(c + Vec2::new(0.0, 1.2 * s), cr, Color32::from_black_alpha(110));
            painter.circle_filled(c, cr, INSET());
            painter.circle_stroke(c, cr, Stroke::new(tk::stroke_outline() * s, theme::line2()));
            painter.circle_stroke(c, cr - 2.0 * s, Stroke::new(tk::stroke_outline() * s, theme::with_alpha(INK(), 14)));
            painter.add(egui::Shape::line(
                arc(0.08, 0.42, cr - 1.0 * s),
                Stroke::new(1.2 * s, theme::with_alpha(theme::METALHI(), 200)),
            ));
            let ang = (-135.0 + 270.0 * n).to_radians();
            let p0 = Pos2::new(c.x + cr * 0.16 * ang.sin(), c.y - cr * 0.16 * ang.cos());
            let p1 = Pos2::new(c.x + (cr - 3.0 * s) * ang.sin(), c.y - (cr - 3.0 * s) * ang.cos());
            painter.line_segment(
                [p0 + Vec2::new(0.0, 0.7 * s), p1 + Vec2::new(0.0, 0.7 * s)],
                Stroke::new(2.4 * s, theme::METALHI()),
            );
            painter.line_segment([p0, p1], Stroke::new(1.9 * s, col));

            ui.label(
                egui::RichText::new(match disp {
                    Some(d) => d.to_string(),
                    None => compact(param.normalized_value_to_string(n, true)),
                })
                .monospace()
                .size(tk::font_body() * s * theme::font_scale())
                .color(SOFT()),
            );
            resp
        },
    )
    .inner
}

/// Raw chip: paints the kit button and reports clicks.
pub fn chip(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    chip_ex(ui, label, on, 1.0)
}

/// Half-size chip (same proportions) — for tight rows.
pub fn chip_sm(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    chip_ex(ui, label, on, 0.75)
}

/// Chip with a uniform size `scale` (1.0 = standard).
pub fn chip_ex(ui: &mut egui::Ui, label: &str, on: bool, scale: f32) -> bool {
    let resp = chip_ex_resp(ui, label, on, scale);
    tap_or_click(ui, &resp)
}

/// Like `chip_ex` but returns the full [`egui::Response`] (senses click AND drag)
/// so callers can attach a `context_menu` or read drag deltas — used by the
/// editor's enum/int selectors. Visuals are identical to `chip`.
pub fn chip_ex_resp(ui: &mut egui::Ui, label: &str, on: bool, scale: f32) -> egui::Response {
    let s = ui_scale(ui) * scale;
    // Match cumulus: full-size chips render 2pt smaller than BTN_FS (8pt); the
    // half-size variant keeps BTN_FS so it doesn't shrink twice.
    let fs = if scale >= 1.0 { tk::font_chip_text() } else { tk::font_btn() };
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(fs * s * theme::font_scale()),
        INK2(),
    );
    let size = Vec2::new(galley.size().x + tk::pad_x() * s, tk::ctl_h() * s);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let hot = resp.hovered();
    let painter = ui.painter_at(rect);
    let (bg, stroke, fg) = if on {
        (theme::on_fill(), theme::G(), INK())
    } else if hot {
        (INSET(), theme::with_alpha(INK(), 90), INK())
    } else {
        (INSET(), theme::line2(), INK2())
    };
    let bw = if on { tk::stroke_emph() } else { tk::stroke_outline() };
    painter.rect_filled(rect, tk::r_ctl() * s, bg);
    painter.rect_stroke(rect, tk::r_ctl() * s, Stroke::new(bw * s, stroke), egui::StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() * 0.5, galley, fg);
    resp
}

/// Like `chip()` but the ACTIVE state is a SOLID accent fill + PAPER() text +
/// a soft outer accent glow. Returns clicked().
pub fn chip_accent(ui: &mut egui::Ui, label: &str, on: bool, accent: Color32) -> bool {
    let s = ui_scale(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(tk::font_btn() * s * theme::font_scale()),
        INK2(),
    );
    let size = Vec2::new(galley.size().x + tk::pad_x() * s, tk::ctl_h() * s);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    let painter = ui.painter_at(rect.expand(2.0 * s));
    let (bg, stroke, fg) = if on {
        (accent, accent, PAPER())
    } else if hot {
        (INSET(), accent, INK())
    } else {
        (INSET(), theme::line2(), SOFT())
    };
    if on {
        painter.rect_stroke(
            rect.expand(1.5 * s),
            tk::r_ctl() * s,
            Stroke::new(tk::stroke_outline() * s, theme::with_alpha(accent, 90)),
            egui::StrokeKind::Outside,
        );
    }
    painter.rect_filled(rect, tk::r_ctl() * s, bg);
    painter.rect_stroke(
        rect,
        tk::r_ctl() * s,
        Stroke::new(tk::stroke_outline() * s, stroke),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, fg);
    tap_or_click(ui, &resp)
}

/// Like `chip_accent` but PASTEL: translucent accent wash + accent border +
/// accent-colored text. `chip_accent`'s solid fill + PAPER() text drowns short
/// labels (POL ±); here the label stays readable on both themes.
pub fn chip_accent_soft(ui: &mut egui::Ui, label: &str, on: bool, accent: Color32) -> bool {
    let s = ui_scale(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional((tk::font_btn() - 2.0) * s * theme::font_scale()),
        INK2(),
    );
    let size = Vec2::new(galley.size().x + tk::pad_x() * s, tk::ctl_h() * s);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    let painter = ui.painter_at(rect);
    let (bg, stroke, fg) = if on {
        (theme::with_alpha(accent, 42), accent, accent)
    } else if hot {
        (INSET(), accent, INK())
    } else {
        (INSET(), theme::line2(), SOFT())
    };
    painter.rect_filled(rect, tk::r_ctl() * s, bg);
    painter.rect_stroke(
        rect,
        tk::r_ctl() * s,
        Stroke::new(tk::stroke_outline() * s, stroke),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, fg);
    tap_or_click(ui, &resp)
}

/// BoolParam toggle backed by `chip_accent` (colored active state).
pub fn toggle_accent(
    ui: &mut egui::Ui,
    label: &str,
    param: &BoolParam,
    setter: &ParamSetter,
    accent: Color32,
) {
    if chip_accent(ui, label, param.value(), accent) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !param.value());
        setter.end_set_parameter(param);
    }
}

/// Bitwig/Epoché segmented control core: paints `labels` as a well of segments
/// with `active` highlighted (INK() fill + FRAME() text), and returns the index
/// of a segment if it was clicked this frame. Width is summed explicitly. The
/// `salt` keeps interact-ids unique when several segmented controls share a view.
pub fn segmented_idx(ui: &mut egui::Ui, labels: &[&str], active: usize, salt: &str) -> Option<usize> {
    let s = ui_scale(ui);
    let pad = theme::SEG_PAD * s;
    let item_px = theme::SEG_ITEM_PAD_X * s;
    let font = egui::FontId::proportional(tk::font_btn() * s * theme::font_scale());
    let galleys: Vec<_> = labels
        .iter()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), SOFT()))
        .collect();
    let seg_ws: Vec<f32> = galleys.iter().map(|g| g.size().x + 2.0 * item_px).collect();
    let total_w = pad * 2.0 + seg_ws.iter().sum::<f32>() + pad * (labels.len() as f32 - 1.0);
    let total_h = tk::ctl_h() * s;
    let seg_h = total_h - 2.0 * pad;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tk::r_ctl() * s, INSET());
    painter.rect_stroke(
        rect,
        tk::r_ctl() * s,
        Stroke::new(tk::stroke_outline() * s, theme::line2()),
        egui::StrokeKind::Inside,
    );
    let mut clicked = None;
    let mut x = rect.left() + pad;
    for (i, (g, w)) in galleys.into_iter().zip(seg_ws.iter()).enumerate() {
        let seg = Rect::from_min_size(Pos2::new(x, rect.top() + pad), Vec2::new(*w, seg_h));
        let id = ui.id().with((salt, "seg", i));
        let resp = ui.interact(seg, id, Sense::click());
        let active = i == active;
        let hot = resp.hovered();
        let fg = if active { FRAME() } else if hot { INK() } else { SOFT() };
        if active {
            painter.rect_filled(seg, theme::SEG_RADIUS * s, INK());
        }
        painter.galley(seg.center() - g.size() * 0.5, g, fg);
        if tap_or_click(ui, &resp) && !active {
            clicked = Some(i);
        }
        x += *w + pad;
    }
    clicked
}

/// Segmented toggle bound to an IntParam (index == value).
pub fn segmented(ui: &mut egui::Ui, param: &IntParam, setter: &ParamSetter, labels: &[&str]) {
    if let Some(i) = segmented_idx(ui, labels, param.value().max(0) as usize, "int") {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, i as i32);
        setter.end_set_parameter(param);
    }
}

/// Non-interactive labeled readout pill: INSET() fill, line2() border. Inline
/// [label][value]: label SOFT(), value mono INK(). Width computed from galleys.
pub fn readout_chip(ui: &mut egui::Ui, label: &str, value: &str) -> egui::Response {
    let s = ui_scale(ui);
    let lab_font = egui::FontId::proportional(tk::font_btn() * s * theme::font_scale());
    let val_font = egui::FontId::monospace(tk::font_btn() * s * theme::font_scale());
    let lg = ui.painter().layout_no_wrap(label.to_string(), lab_font, SOFT());
    let vg = ui.painter().layout_no_wrap(value.to_string(), val_font, INK());
    let gap = tk::space_sm() * s;
    let pad_x = 11.0 * s;
    let size = Vec2::new(lg.size().x + gap + vg.size().x + 2.0 * pad_x, tk::ctl_h() * s);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tk::r_ctl() * s, INSET());
    painter.rect_stroke(
        rect,
        tk::r_ctl() * s,
        Stroke::new(tk::stroke_outline() * s, theme::line2()),
        egui::StrokeKind::Inside,
    );
    let lx = rect.left() + pad_x;
    painter.galley(Pos2::new(lx, rect.center().y - lg.size().y * 0.5), lg.clone(), SOFT());
    let vx = lx + lg.size().x + gap;
    painter.galley(Pos2::new(vx, rect.center().y - vg.size().y * 0.5), vg, INK());
    resp
}

/// Twin vertical VU bars. `peak` is 0..1; continuous green fill from the bottom.
pub fn vu_meter(ui: &mut egui::Ui, peak_l: f32, peak_r: f32) -> egui::Response {
    let s = ui_scale(ui);
    let bw = theme::VU_BAR_W * s;
    let bh = theme::VU_BAR_H * s;
    let gap = theme::VU_BAR_GAP * s;
    let r = theme::VU_RADIUS * s;
    let peaks = [peak_l.clamp(0.0, 1.0), peak_r.clamp(0.0, 1.0)];
    let size = Vec2::new(bw * 2.0 + gap, bh);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    for i in 0..2usize {
        let x0 = rect.left() + i as f32 * (bw + gap);
        let bar = Rect::from_min_size(Pos2::new(x0, rect.top()), Vec2::new(bw, bh));
        painter.rect_filled(bar, r, INSET());
        let fh = bh * peaks[i];
        if fh > 0.0 {
            let fill = Rect::from_min_max(Pos2::new(x0, bar.bottom() - fh), bar.max);
            painter.rect_filled(fill, r, G());
        }
        painter.rect_stroke(
            bar,
            r,
            Stroke::new(tk::stroke_outline() * s, theme::line2()),
            egui::StrokeKind::Inside,
        );
    }
    resp
}

/// Framed `◀ name ▶` pill. Returns -1 on ◀, +1 on ▶, 2 on the name, else 0.
pub fn preset_slot(ui: &mut egui::Ui, name: &str, min_w: f32, h_logical: f32, salt: &str) -> i32 {
    let s = ui_scale(ui);
    let h = h_logical * s;
    let w = (min_w * s).max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tk::r_pill() * s, INSET());
    painter.rect_stroke(
        rect,
        tk::r_pill() * s,
        Stroke::new(tk::stroke_outline() * s, theme::line2()),
        egui::StrokeKind::Inside,
    );
    let pad = theme::PILL_PAD_X * s;
    let arrow = egui::FontId::proportional(10.0 * s * theme::font_scale());
    let aw = 14.0 * s;
    let la = Rect::from_min_size(Pos2::new(rect.left() + pad * 0.4, rect.top()), Vec2::new(aw, h));
    let lresp = ui.interact(la, ui.id().with((salt, "ps_l")), Sense::click());
    painter.text(
        Pos2::new(la.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        "\u{25C0}",
        arrow.clone(),
        if lresp.hovered() { INK() } else { FAINT() },
    );
    let ra = Rect::from_min_size(Pos2::new(rect.right() - pad * 0.4 - aw, rect.top()), Vec2::new(aw, h));
    let rresp = ui.interact(ra, ui.id().with((salt, "ps_r")), Sense::click());
    painter.text(
        Pos2::new(ra.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        "\u{25B6}",
        arrow,
        if rresp.hovered() { INK() } else { FAINT() },
    );
    let ca = Rect::from_min_max(Pos2::new(la.right(), rect.top()), Pos2::new(ra.left(), rect.bottom()));
    let cresp = ui
        .interact(ca, ui.id().with((salt, "ps_c")), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(10.5 * s * theme::font_scale()),
        if cresp.hovered() { G() } else { INK() },
    );
    if tap_or_click(ui, &lresp) {
        -1
    } else if tap_or_click(ui, &rresp) {
        1
    } else if tap_or_click(ui, &cresp) {
        2
    } else {
        0
    }
}

/// Shared titlebar icon-button box (INSET() fill, line2() -> G() on hover).
fn tb_box(ui: &mut egui::Ui) -> (Rect, egui::Response, bool) {
    let s = ui_scale(ui);
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(theme::TBBTN_W * s, theme::TBBTN_H * s), Sense::click());
    let hot = resp.hovered();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme::TBBTN_RADIUS * s, INSET());
    painter.rect_stroke(
        rect,
        theme::TBBTN_RADIUS * s,
        Stroke::new(tk::stroke_outline() * s, if hot { G() } else { theme::line2() }),
        egui::StrokeKind::Inside,
    );
    (rect, resp, hot)
}

/// Settings gear button (painted cog). Click returns true.
pub fn gear_button(ui: &mut egui::Ui) -> bool {
    let s = ui_scale(ui);
    let (rect, resp, hot) = tb_box(ui);
    let painter = ui.painter_at(rect);
    let col = if hot { INK() } else { INK2() };
    let c = rect.center();
    let r_out = 6.0 * s;
    let r_in = 2.4 * s;
    for i in 0..8u32 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let d = Vec2::new(a.cos(), a.sin());
        painter.line_segment(
            [c + d * (r_out * 0.7), c + d * r_out],
            Stroke::new(tk::stroke_emph() * s, col),
        );
    }
    painter.circle_stroke(c, r_out * 0.62, Stroke::new(tk::stroke_emph() * s, col));
    painter.circle_filled(c, r_in, col);
    resp.clicked()
}

/// Vertical hairline divider, inset top/bottom.
pub fn vrule(ui: &mut egui::Ui, h: f32) {
    let s = ui_scale(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(tk::stroke_outline() * s, h * s), Sense::hover());
    let painter = ui.painter_at(rect);
    let inset = theme::SP_XS * s;
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + inset),
            Pos2::new(rect.center().x, rect.bottom() - inset),
        ],
        Stroke::new(tk::stroke_outline() * s, theme::line()),
    );
}

/// Chip-on-border card: PANEL() body, hairline border, title chip sitting on
/// the top border (the kit's signature frame).
pub fn card(
    ui: &mut egui::Ui,
    title: &str,
    title_col: Color32,
    w: f32,
    body: &mut dyn FnMut(&mut egui::Ui),
) {
    let s = ui_scale(ui);
    ui.allocate_ui_with_layout(
        Vec2::new(w, 10.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(w);
            ui.set_max_width(w);
            ui.add_space(theme::CARD_CHIP_OVERHANG * s);
            let frame = egui::Frame::new()
                .fill(PANEL())
                .stroke(Stroke::new(tk::stroke_outline() * s, theme::line()))
                .corner_radius(theme::CARD_RADIUS * s)
                .inner_margin(egui::Margin::symmetric(
                    (theme::CARD_MARGIN_X * s) as i8,
                    (theme::CARD_MARGIN_Y * s) as i8,
                ));
            let resp = frame.show(ui, |ui| {
                ui.set_min_width(w - theme::CARD_MARGIN_TOTAL_X * s);
                body(ui);
            });
            let top = resp.response.rect.top();
            let chip_pos = Pos2::new(resp.response.rect.left() + theme::SP_XL * s, top);
            let galley = ui.painter().layout_no_wrap(
                title.to_uppercase(),
                egui::FontId::proportional(tk::font_label() * s * theme::font_scale()),
                title_col,
            );
            let chip_rect = Rect::from_min_size(
                chip_pos - Vec2::new(4.0 * s, galley.size().y * 0.55),
                galley.size() + Vec2::new(8.0 * s, 0.0),
            );
            ui.painter().rect_filled(chip_rect, 0.0, theme::FRAME());
            ui.painter()
                .galley(chip_rect.min + Vec2::new(4.0 * s, 0.0), galley, title_col);
        },
    );
}

/// Like `card`, but the body is forced to fill `outer_h` (logical px) via
/// `set_min_height`, so several cards in a row can be made the same height. The
/// title chip is identical to `card`.
pub fn card_h(
    ui: &mut egui::Ui,
    title: &str,
    title_col: Color32,
    w: f32,
    outer_h: f32,
    body: &mut dyn FnMut(&mut egui::Ui),
) {
    let s = ui_scale(ui);
    let min_body =
        (outer_h - theme::CARD_CHIP_OVERHANG * s - 2.0 * theme::CARD_MARGIN_Y * s).max(0.0);
    ui.allocate_ui_with_layout(
        Vec2::new(w, 10.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(w);
            ui.set_max_width(w);
            ui.add_space(theme::CARD_CHIP_OVERHANG * s);
            let frame = egui::Frame::new()
                .fill(PANEL())
                .stroke(Stroke::new(tk::stroke_outline() * s, theme::line()))
                .corner_radius(theme::CARD_RADIUS * s)
                .inner_margin(egui::Margin::symmetric(
                    (theme::CARD_MARGIN_X * s) as i8,
                    (theme::CARD_MARGIN_Y * s) as i8,
                ));
            let resp = frame.show(ui, |ui| {
                ui.set_min_width(w - theme::CARD_MARGIN_TOTAL_X * s);
                ui.set_min_height(min_body);
                body(ui);
            });
            let top = resp.response.rect.top();
            let chip_pos = Pos2::new(resp.response.rect.left() + theme::SP_XL * s, top);
            let galley = ui.painter().layout_no_wrap(
                title.to_uppercase(),
                egui::FontId::proportional(tk::font_label() * s * theme::font_scale()),
                title_col,
            );
            let chip_rect = Rect::from_min_size(
                chip_pos - Vec2::new(4.0 * s, galley.size().y * 0.55),
                galley.size() + Vec2::new(8.0 * s, 0.0),
            );
            ui.painter().rect_filled(chip_rect, 0.0, theme::FRAME());
            ui.painter()
                .galley(chip_rect.min + Vec2::new(4.0 * s, 0.0), galley, title_col);
        },
    );
}

/// Like `card`, but the title region is a dot-separated interactive tab row.
/// Returns Some(index) on the frame a tab is clicked.
#[allow(clippy::too_many_arguments)]
pub fn card_tabs(
    ui: &mut egui::Ui,
    section: &str,
    tabs: &[&str],
    active: usize,
    w: f32,
    body: &mut dyn FnMut(&mut egui::Ui),
    tab_col: Option<&dyn Fn(usize) -> Color32>,
) -> Option<usize> {
    let s = ui_scale(ui);
    let mut clicked = None;
    ui.allocate_ui_with_layout(
        Vec2::new(w, 10.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(w);
            ui.set_max_width(w);
            ui.add_space(theme::CARD_CHIP_OVERHANG * s);
            let frame = egui::Frame::new()
                .fill(PANEL())
                .stroke(Stroke::new(tk::stroke_outline() * s, theme::line()))
                .corner_radius(theme::CARD_RADIUS * s)
                .inner_margin(egui::Margin::symmetric(
                    (theme::CARD_MARGIN_X * s) as i8,
                    (theme::CARD_MARGIN_Y * s) as i8,
                ));
            let fresp = frame.show(ui, |ui| {
                ui.set_min_width(w - theme::CARD_MARGIN_TOTAL_X * s);
                body(ui);
            });
            let painter = ui.painter().clone();
            let title_font = egui::FontId::proportional(tk::font_label() * s * theme::font_scale() * 1.24);
            let dot_font = egui::FontId::monospace(tk::font_body() * s * theme::font_scale());
            let lx = fresp.response.rect.left() + theme::SP_XL * s;
            let top = fresp.response.rect.top();
            let measure =
                |t: &str, f: &egui::FontId| painter.layout_no_wrap(t.to_string(), f.clone(), SOFT()).size();
            let sec_up = section.to_uppercase();
            let sec_sz = measure(&sec_up, &title_font);
            let dot_sz = measure("·", &dot_font);
            let tab_ups: Vec<String> = tabs.iter().map(|t| t.to_uppercase()).collect();
            let tab_szs: Vec<Vec2> = tab_ups.iter().map(|t| measure(t, &title_font)).collect();
            let gap = tk::space_sm() * s;
            let row_h = sec_sz.y;
            let ty = top - row_h * 0.55;
            let mut total = sec_sz.x;
            for tsz in &tab_szs {
                total += gap + dot_sz.x + gap + tsz.x;
            }
            let pad = theme::SP_XS * s;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(lx - pad, ty), Vec2::new(total + 2.0 * pad, row_h)),
                0.0,
                FRAME(),
            );
            let mut x = lx;
            painter.text(Pos2::new(x, ty), egui::Align2::LEFT_TOP, &sec_up, title_font.clone(), SOFT());
            x += sec_sz.x;
            for (i, tup) in tab_ups.iter().enumerate() {
                x += gap;
                painter.text(Pos2::new(x, ty), egui::Align2::LEFT_TOP, "·", dot_font.clone(), FAINT());
                x += dot_sz.x + gap;
                let trect = Rect::from_min_size(Pos2::new(x, ty), Vec2::new(tab_szs[i].x, row_h));
                let id = ui.id().with((section, "ctab", i));
                let resp = ui.interact(trect, id, Sense::click());
                let col = if i == active {
                    tab_col.map(|f| f(i)).unwrap_or_else(INK)
                } else if resp.hovered() {
                    INK2()
                } else {
                    FAINT()
                };
                painter.text(Pos2::new(x, ty), egui::Align2::LEFT_TOP, tup, title_font.clone(), col);
                if resp.clicked() {
                    clicked = Some(i);
                }
                x += tab_szs[i].x;
            }
        },
    );
    clicked
}

/// Small dim caption.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    let s = ui_scale(ui);
    ui.label(egui::RichText::new(text).size(8.5 * s * theme::font_scale()).color(FAINT()));
}

// Headless egui frames with synthetic touches: who gets the tap?
#[cfg(test)]
mod touch_tests {
    use super::*;

    const UNDER: Rect = Rect { min: Pos2::new(10.0, 10.0), max: Pos2::new(110.0, 60.0) };
    const OPEN: Rect = Rect { min: Pos2::new(200.0, 200.0), max: Pos2::new(260.0, 240.0) };

    fn touch(id: u64, phase: egui::TouchPhase, pos: Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        }
    }

    /// What covers UNDER: nothing, a menu whose spot there is a touch widget, or
    /// a menu whose spot there is passive (a label, the panel background).
    #[derive(Clone, Copy, PartialEq)]
    enum Menu { None, Widget, Passive }

    /// One frame: a page widget over UNDER (optionally disabled), an open page
    /// widget at OPEN, and the `menu` cover. Returns (under, open, in_menu) taps.
    fn frame(ctx: &egui::Context, time: f64, events: Vec<egui::Event>, menu: Menu, disabled: bool) -> (bool, bool, bool) {
        let mut taps = (false, false, false);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            ui.add_enabled_ui(!disabled, |ui| {
                taps.0 = touch_interact(ui, egui::Id::new("under"), UNDER).tapped;
            });
            taps.1 = touch_interact(ui, egui::Id::new("open"), OPEN).tapped;
            if menu != Menu::None {
                egui::Area::new(egui::Id::new("menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(UNDER.min)
                    .show(ui.ctx(), |ui| {
                        let (r, _) = ui.allocate_exact_size(UNDER.size(), Sense::click());
                        if menu == Menu::Widget {
                            taps.2 = touch_interact(ui, egui::Id::new("in_menu"), r).tapped;
                        }
                    });
            }
        });
        taps
    }

    /// Frame 1 lays the scene out (egui reads layers from the previous frame),
    /// frame 2 delivers a press at `pos`, frame 3 the release `held` seconds later.
    fn tap(pos: Pos2, menu: Menu, disabled: bool, held: f64) -> (bool, bool, bool) {
        theme::set_touch(true);
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, vec![], menu, disabled);
        frame(&ctx, 0.1, vec![touch(7, egui::TouchPhase::Start, pos)], menu, disabled);
        frame(&ctx, 0.1 + held, vec![touch(7, egui::TouchPhase::End, pos)], menu, disabled)
    }

    #[test]
    fn tap_reaches_an_uncovered_page_widget() {
        assert_eq!(tap(UNDER.center(), Menu::None, false, 0.1), (true, false, false));
        assert_eq!(tap(OPEN.center(), Menu::Passive, false, 0.1), (false, true, false));
    }

    #[test]
    fn tap_on_a_menu_does_not_fall_through() {
        assert_eq!(tap(UNDER.center(), Menu::Widget, false, 0.1), (false, false, true));
        assert_eq!(tap(UNDER.center(), Menu::Passive, false, 0.1), (false, false, false));
    }

    #[test]
    fn disabled_widget_ignores_touch() {
        assert_eq!(tap(UNDER.center(), Menu::None, true, 0.1), (false, false, false));
    }

    #[test]
    fn long_press_is_not_a_tap() {
        assert_eq!(tap(UNDER.center(), Menu::None, false, 1.0), (false, false, false));
    }
}
