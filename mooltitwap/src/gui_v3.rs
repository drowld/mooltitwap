//! απόηχος (Afterecho) v3 editor — PAN page preview.
//!
//! Faithful port of the owner's Figma frame "Multitap view pan"
//! (2388×1668 @2x → all geometry here is Figma px ÷ 2, × the zoom scale `s`).
//! Column grid: 14 | left 245 | 12 | center 516 | 12 | right 379 | 16.
//! Touch-correctness is code-side by design (owner decision 2026-07-11):
//! hit areas extend past painted pixels (full-cell tap pills, padded chips).

use egui::{Align2, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use epoche_ui::epo::{self, Accent};
use epoche_ui::theme;
use nice_plug::prelude::*;
use std::sync::Arc;

use crate::gui::{compute_viz_data, draw_xy_pad, gain_db_to_pos, pos_to_gain_db,
                 set_enum, tap_color, EditorState, RightTab};
use crate::params::{ChannelMode, CrossfeedMode, DelayMode, DiffuseRouting, FbCurve, FilterSlope,
                    FilterType, PetalParams, SyncMode, TapeType, NUM_TAPS};
use crate::tokens as tk;

const HEADER_H: f32 = 55.5;
const CARD_Y: f32 = 67.5;
const CARD_H: f32 = 751.5;
const LEFT_X: f32 = 14.0;
const LEFT_W: f32 = 245.0;
const CENTER_X: f32 = 271.0;
const CENTER_W: f32 = 516.0;
const VIZ_H: f32 = 547.5;
const STRIP_Y: f32 = 627.0;
const STRIP_H: f32 = 192.0;
const RIGHT_X: f32 = 799.0;
const RIGHT_W: f32 = 379.0;
// tk::knob() / tk::btn_min_w() / tk::ctl_h() / tk::r_ctl() moved to tokens.rs (v2.22: two selectable
// sizing sheets — the original Figma numbers and the τόπος proposal — so the
// owner can A/B them live via hamburger ▸ SIZING).

/// Multitouch-aware hit test for a discrete tap target (button / chip / pill /
/// icon). On a touch build (iPad) it claims the synthesized pointer over `hit`
/// — so the press can't fall through to a control behind it or drag-scroll a
/// parent ScrollArea — and then detects the tap from its OWN claimed finger via
/// `epo::tap_touch`, so several targets can be worked at once (egui synthesizes
/// only ONE pointer, from the first finger down). On desktop it's an ordinary
/// pointer click, byte-for-byte the old behavior. Returns (fired-this-frame,
/// response); the response still carries `hovered()` for the desktop highlight.
fn tap_or_click(ui: &mut egui::Ui, hit: Rect, id: egui::Id, enabled: bool) -> (bool, egui::Response) {
    let sense = if !enabled { Sense::hover() }
        else if theme::touch() { Sense::click_and_drag() }
        else { Sense::click() };
    let resp = ui.interact(hit, id, sense);
    let fired = enabled && if theme::touch() { epo::tap_touch(ui, hit, id) } else { resp.clicked() };
    (fired, resp)
}

/// The v3 button — painted to the Figma component spec, centered in `cell`,
/// with the WHOLE cell as the hit area. Returns clicked (false when disabled).
/// Multitouch-safe on the iPad via [`tap_or_click`].
fn v3_button(ui: &mut egui::Ui, cell: Rect, salt: (&str, usize), label: &str,
             on: bool, enabled: bool, s: f32) -> bool {
    v3_button_glyph(ui, cell, salt, label, None, on, enabled, s)
}

/// Mini filter-response curve (LP ¯\ · BP /¯\ · HP /¯) — the glanceable
/// filter-mode cue (owner idea 2026-07-13: LP/BP/HP was text-only; the curve
/// shape reads without reading). Drawn to fill `r`, y-down (0.25 = passband).
fn filter_glyph(p: &egui::Painter, r: Rect, ft: FilterType, color: egui::Color32, s: f32) {
    let pt = |x: f32, y: f32| Pos2::new(r.min.x + x * r.width(), r.min.y + y * r.height());
    let pts: &[(f32, f32)] = match ft {
        FilterType::LowPass => &[(0.0, 0.25), (0.55, 0.25), (1.0, 1.0)],
        FilterType::HighPass => &[(0.0, 1.0), (0.45, 0.25), (1.0, 0.25)],
        FilterType::BandPass => &[(0.0, 1.0), (0.35, 0.25), (0.65, 0.25), (1.0, 1.0)],
    };
    for w in pts.windows(2) {
        p.line_segment([pt(w[0].0, w[0].1), pt(w[1].0, w[1].1)], Stroke::new(1.2 * s, color));
    }
}

/// [`v3_button`] with an optional filter-response glyph painted before the
/// label — used by every filter TYPE control so the mode is visible at a
/// glance (tap panel + FILTER-page blocks).
fn v3_button_glyph(ui: &mut egui::Ui, cell: Rect, salt: (&str, usize), label: &str,
                   glyph: Option<FilterType>, on: bool, enabled: bool, s: f32) -> bool {
    let (clicked, resp) = tap_or_click(ui, cell, ui.id().with(salt), enabled);
    let p = ui.painter();
    let gw = if glyph.is_some() { 19.0 * s } else { 0.0 }; // 15 glyph + 4 gap
    let text_w = p.layout_no_wrap(label.to_string(),
        FontId::new(tk::font_body() * s, FontFamily::Proportional), theme::SOFT()).size().x;
    let w = (tk::btn_min_w() * s).max(text_w + gw + 2.0 * tk::pad_h() * s);
    let btn = Rect::from_center_size(cell.center(), Vec2::new(w, tk::ctl_h() * s));
    let a = |c: egui::Color32, mul: f32| theme::with_alpha(c, (255.0 * mul * if enabled { 1.0 } else { 0.4 }) as u8);
    if resp.hovered() && enabled {
        p.rect_filled(btn.expand(2.0 * s), (tk::r_ctl() + 2.0) * s, theme::with_alpha(theme::INK(), 8));
    }
    if on { p.rect_filled(btn, tk::r_ctl() * s, a(theme::INK(), 0.055)); }
    let stroke_c = if on { a(theme::INK(), 0.85) } else { a(theme::SOFT(), 0.6) };
    p.rect_stroke(btn, tk::r_ctl() * s, Stroke::new(tk::stroke_outline() * s, stroke_c), StrokeKind::Inside);
    let text_c = if on { a(theme::INK(), 1.0) } else { a(theme::SOFT(), 1.0) };
    if let Some(ft) = glyph {
        // glyph + text as one centered block: glyph first, label after
        let gr = Rect::from_center_size(
            Pos2::new(btn.center().x - (text_w + gw) * 0.5 + 7.5 * s, btn.center().y),
            Vec2::new(15.0 * s, 9.0 * s));
        filter_glyph(p, gr, ft, text_c, s);
        p.text(Pos2::new(btn.center().x + gw * 0.5, btn.center().y), Align2::CENTER_CENTER, label,
               FontId::new(tk::font_body() * s, FontFamily::Proportional), text_c);
    } else {
        p.text(btn.center(), Align2::CENTER_CENTER, label,
               FontId::new(tk::font_body() * s, FontFamily::Proportional), text_c);
    }
    clicked
}

/// A non-interactive, greyed knob placeholder (label · faint disc · "—"). Used
/// for the PAN knob in Mono, where panning has no meaning (owner 2026-07-14).
fn disabled_knob(p: &egui::Painter, cell: Rect, label: &str, s: f32) {
    let d = tk::knob() * 0.85 * s;
    let cx = cell.center().x;
    let disc = Pos2::new(cx, cell.center().y - 3.0 * s);
    let faint = theme::with_alpha(theme::FAINT(), 175);
    p.text(Pos2::new(cx, cell.min.y + 2.0 * s), Align2::CENTER_TOP, label,
           FontId::new(9.0 * s, FontFamily::Proportional), faint);
    p.circle_filled(disc, d * 0.5, theme::with_alpha(theme::INSET(), 200));
    p.circle_stroke(disc, d * 0.5, Stroke::new(theme::STROKE_HAIR * s, theme::line2()));
    p.text(Pos2::new(cx, cell.max.y - 2.0 * s), Align2::CENTER_BOTTOM, "\u{2013}",
           FontId::new(11.0 * s, FontFamily::Monospace), faint);
}

/// Snapped `time_offset` for a dragged tap (SNAP chip on): quantizes the
/// tap's combined position (topology pos + offset, in cluster-span units) to
/// the nearest 1/NUM_TAPS — i.e. its ABSOLUTE time to multiples of the base
/// time, since span = NUM_TAPS · base (owner idea 2026-07-13: taps land on
/// musical divisions). The nominal position is recovered from the tap's
/// current viz time and currently-applied offset.
fn snap_offset(cur_time_secs: f32, cur_off: f32, raw_off: f32, span_secs: f32) -> f32 {
    let pos = cur_time_secs / span_secs.max(1e-6) - cur_off;
    let snapped = ((pos + raw_off) * NUM_TAPS as f32).round() / NUM_TAPS as f32;
    snapped - pos
}

/// v3_button bound to a BoolParam (toggle semantics).
fn v3_toggle(ui: &mut egui::Ui, cell: Rect, salt: (&str, usize), label: &str,
             param: &BoolParam, setter: &ParamSetter, enabled: bool, s: f32) {
    if v3_button(ui, cell, salt, label, param.value(), enabled, s) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !param.value());
        setter.end_set_parameter(param);
    }
}

/// PAN/GAIN track thickness multiplier — process-global like the theme
/// atomics, so the track painters (deep in the row call chain) can read it
/// without threading EditorState through every signature.
static SLIDER_THICK: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3f80_0000); // 1.0f32
fn set_slider_thick(v: f32) {
    SLIDER_THICK.store(v.clamp(1.0, 4.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}
fn slider_thick() -> f32 {
    f32::from_bits(SLIDER_THICK.load(std::sync::atomic::Ordering::Relaxed))
}

/// "Last touched tap" — right-card rows (PAN/GAIN/FILTER pages) report edits
/// here; draw() folds it into `st.field_sel` so the GRID/STEMS view rings the
/// tap being worked on (owner request). Same process-global idiom as above.
static LAST_TOUCHED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
fn note_tap_touch(i: usize) {
    LAST_TOUCHED.store(i as i32, std::sync::atomic::Ordering::Relaxed);
}
fn take_tap_touch() -> Option<usize> {
    let v = LAST_TOUCHED.swap(-1, std::sync::atomic::Ordering::Relaxed);
    (v >= 0).then_some(v as usize)
}

pub fn draw(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter,
            st: &mut EditorState, tempo: f32) {
    let s = theme::ui_scale(ui);
    // Card/tab title size (10 + steps), tweakable in the hamburger menu ▸
    // TAB TEXT. v2.21.0: works on DESKTOP too via theme::set_title_override
    // (previously touch-only).
    theme::set_title_override(true);
    theme::set_title_size((10.0 + st.title_steps as f32).clamp(7.0, 48.0));
    // PAN/GAIN slider thickness multiplier (hamburger ▸ SLIDERS).
    set_slider_thick(1.0 + 0.25 * st.slider_steps as f32);
    // Sizing-token sheet (hamburger ▸ SIZING): v3 vs τόπος β.
    tk::set_beta(st.sizing_beta);
    // Plugin accent = violet, matching the tap identity (tap_color = accent(2)).
    // So the preset palette (search / save-as / tag input), the header preset
    // name on hover, and the header accent dot all read violet instead of the
    // kit-default green (owner 2026-07-14).
    theme::set_accent(5);
    // High-contrast accessibility mode (hamburger ▸ CONTRAST), pushed each frame.
    theme::set_high_contrast(st.high_contrast);
    let p = ui.painter().clone();
    let page = ui.max_rect();
    p.rect_filled(page, 0.0, theme::PAPER());

    draw_v3_header(ui, &p, page, st, params, setter, s);

    let card = |x: f32, y: f32, w: f32, h: f32| {
        Rect::from_min_size(Pos2::new(page.min.x + x * s, page.min.y + y * s), Vec2::new(w * s, h * s))
    };
    let left = card(LEFT_X, CARD_Y, LEFT_W, CARD_H);
    let strip = card(CENTER_X, STRIP_Y, CENTER_W, STRIP_H);
    // Match the kit card (card_tabs): PANEL fill + hairline stroke, so all four
    // cards read the same. Interior wells (XY pads) stay INSET = recessed.
    for r in [left, strip] {
        p.rect_filled(r, theme::CARD_RADIUS * s, theme::PANEL());
        p.rect_stroke(r, theme::CARD_RADIUS * s, Stroke::new(theme::STROKE_HAIR * s, theme::line()), StrokeKind::Inside);
    }
    draw_card_tag(&p, left.min, "DELAY", s);
    // (the strip's tag is drawn below — it depends on whether a tap is selected)

    draw_v3_left(ui, left, params, setter, tempo, s);

    // CENTER card — GRID · STEMS as dot-separated border tabs, the same kit
    // card as TAPS (owner: "switch between them inside the tabs like we do
    // for the other elements, not with a button").
    let v_overhang = theme::CARD_CHIP_OVERHANG * s;
    let vslot = Rect::from_min_size(
        Pos2::new(page.min.x + CENTER_X * s, page.min.y + CARD_Y * s - v_overhang),
        Vec2::new(CENTER_W * s, VIZ_H * s + v_overhang));
    let mut vui = ui.new_child(egui::UiBuilder::new().max_rect(vslot)
        .layout(egui::Layout::top_down(egui::Align::Min)));
    let vbody_px = (VIZ_H - theme::CARD_CHIP_OVERHANG - 2.0 * theme::CARD_MARGIN_Y) * s;
    let vactive = (st.view3 as usize).min(2);
    // ECHO = the classic read-only L/R bar view with the full feedback tail —
    // restored as its own tab after STEMS became the editable lollipops
    // (owner: "we need the normal view, it's not visible anymore").
    let (vclicked, _) = epo::card_tabs(&mut vui, "VIEW", &["GRID", "STEMS", "ECHO"], vactive,
        CENTER_W * s, vbody_px,
        &mut |ui| draw_v3_viz_body(ui, params, setter, st, tempo, vbody_px, s), None);
    if let Some(i) = vclicked {
        st.view3 = i as u8;
        st.field_sel = None;
    }

    // SHAPE card — while a tap is selected in GRID/STEMS it hosts the tap
    // panel instead of the XY pads (owner mockup "Multitap new view",
    // 2026-07-14: the old floating popup occluded dots near the plot's bottom
    // edge). Close (×) restores the pads. Evaluated AFTER the viz body so a
    // dot click opens the panel the same frame.
    let panel_tap = if st.view3 <= 1 { st.field_sel } else { None };
    match panel_tap {
        Some(i) => {
            draw_card_tag(&p, strip.min, &format!("TAP {:02}", i + 1), s);
            draw_strip_tap_panel(ui, strip, params, setter, st, s);
        }
        None => {
            draw_card_tag(&p, strip.min, "SHAPE", s);
            draw_v3_strip(ui, strip, params, setter, s);
        }
    }

    // RIGHT card — the kit's own tab card (epo::card_tabs), exactly like cumulus:
    // dot-separated tabs on the top border, title_fs sizing, 44pt hit floors.
    // Placed in a child ui at the absolute card slot (offset up by the chip
    // overhang so the frame lands at CARD_Y).
    // Mono has no panning — drop the PAN tab entirely (owner 2026-07-14). If PAN
    // was active when the mode flipped to Mono, fall back to GAIN.
    let mono = params.channel_mode.value() == ChannelMode::Mono;
    if mono && st.right_tab == RightTab::Pan { st.right_tab = RightTab::Gain; }
    let tabs: &[&str] = if mono { &["GAIN", "FILTER", "SPACE"] } else { &["PAN", "GAIN", "FILTER", "SPACE"] };
    let tab_map: &[RightTab] = if mono {
        &[RightTab::Gain, RightTab::Filter, RightTab::XFeed]
    } else {
        &[RightTab::Pan, RightTab::Gain, RightTab::Filter, RightTab::XFeed]
    };
    // full-grid index (0 PAN·1 GAIN·2 FILTER·3 SPACE), shifted left when PAN drops
    let base: usize = match st.right_tab {
        RightTab::Pan => 0, RightTab::Gain => 1, RightTab::Filter => 2,
        RightTab::XFeed | RightTab::Diffuse => 3,
    };
    let active = if mono { base.saturating_sub(1) } else { base };
    let cur_tab = st.right_tab;
    let overhang = theme::CARD_CHIP_OVERHANG * s;
    let slot = Rect::from_min_size(
        Pos2::new(page.min.x + RIGHT_X * s, page.min.y + CARD_Y * s - overhang),
        Vec2::new(RIGHT_W * s, CARD_H * s + overhang));
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(slot)
        .layout(egui::Layout::top_down(egui::Align::Min)));
    // min_body_h fills the card: total = overhang + 2*margin_y + body.
    let body_h = CARD_H - theme::CARD_CHIP_OVERHANG - 2.0 * theme::CARD_MARGIN_Y;
    let body_px = body_h * s;
    let (clicked, _) = epo::card_tabs(&mut cui, "TAPS", tabs, active, RIGHT_W * s, body_px,
        &mut |ui| draw_v3_right_body(ui, params, setter, cur_tab, body_px, s), None);
    if let Some(i) = clicked {
        st.right_tab = tab_map[i.min(tab_map.len() - 1)];
    }
    // Selection sync: editing a tap row in the TAPS card selects that tap in
    // the GRID/STEMS view (ring + panel follow the work).
    if let Some(i) = take_tap_touch() {
        st.field_sel = Some(i);
    }

    draw_v3_menu(ui, st, page, s);

    // spotlight preset palette — drops from just below the header separator
    st.palette_top = page.min.y + (HEADER_H + 8.0) * s;
    if st.preset_open {
        crate::palette_v3::draw(ui, params, setter, st);
    }
}

/// Hamburger dropdown — a foreground Area anchored under the menu button.
/// Currently: TAB TEXT size stepper (touch only) + theme + CLOSE.
fn draw_v3_menu(ui: &mut egui::Ui, st: &mut EditorState, page: Rect, s: f32) {
    if !st.menu_open { return; }
    let anchor = page.min + Vec2::new(14.0 * s, 60.0 * s);
    egui::Area::new(egui::Id::new("v3_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(theme::PANEL())
                .stroke(Stroke::new(1.0 * s, theme::line2()))
                .corner_radius(10.0 * s)
                .inner_margin(egui::Margin::same((theme::SP_MD * s) as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(theme::SP_SM * s);
                    ui.label(egui::RichText::new("OPTIONS").size(theme::FS_LABEL * s).color(theme::SOFT()).strong());
                    // TAB TEXT stepper — big −/+ buttons (36×30) so they're easy to
                    // hit; range 7..48px (owner wanted way past 18).
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                        ui.label(egui::RichText::new("TAB TEXT").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        let (minus, _) = ui.allocate_exact_size(Vec2::new(36.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, minus, ("v3_ts", 0), "–", false, true, s) { st.title_steps = (st.title_steps - 2).max(-3); }
                        let px = (10 + st.title_steps).clamp(7, 48);
                        ui.label(egui::RichText::new(format!("{}px", px)).size(theme::FS_LABEL * s).color(theme::INK()));
                        let (plus, _) = ui.allocate_exact_size(Vec2::new(36.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, plus, ("v3_ts", 1), "+", false, true, s) { st.title_steps = (st.title_steps + 2).min(38); }
                    });
                    // SIZING sheet — the original v3 numbers vs the τόπος
                    // token proposal, switchable live for side-by-side.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                        ui.label(egui::RichText::new("SIZING").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        let (v3c, _) = ui.allocate_exact_size(Vec2::new(52.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, v3c, ("v3_sz", 0), "V3", !st.sizing_beta, true, s) { st.sizing_beta = false; }
                        let (tc, _) = ui.allocate_exact_size(Vec2::new(76.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, tc, ("v3_sz", 1), "τόπος β", st.sizing_beta, true, s) { st.sizing_beta = true; }
                    });
                    // SLIDERS thickness stepper — PAN/GAIN track weight, 1..4×.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                        ui.label(egui::RichText::new("SLIDERS").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        let (minus, _) = ui.allocate_exact_size(Vec2::new(36.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, minus, ("v3_sl", 0), "–", false, true, s) { st.slider_steps = (st.slider_steps - 1).max(0); }
                        let m = 1.0 + 0.25 * st.slider_steps as f32;
                        ui.label(egui::RichText::new(format!("{:.2}×", m)).size(theme::FS_LABEL * s).color(theme::INK()));
                        let (plus, _) = ui.allocate_exact_size(Vec2::new(36.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, plus, ("v3_sl", 1), "+", false, true, s) { st.slider_steps = (st.slider_steps + 1).min(12); }
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                        ui.label(egui::RichText::new("THEME").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        let (tc, _) = ui.allocate_exact_size(Vec2::new(64.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, tc, ("v3_thm", 0), if st.theme_dark { "DARK" } else { "LIGHT" }, true, true, s) {
                            st.theme_dark = !st.theme_dark;
                        }
                        // high-contrast accessibility toggle (works in either theme)
                        let (cc, _) = ui.allocate_exact_size(Vec2::new(88.0 * s, 30.0 * s), Sense::hover());
                        if v3_button(ui, cc, ("v3_hc", 0), "CONTRAST", st.high_contrast, true, s) {
                            st.high_contrast = !st.high_contrast;
                        }
                    });
                    // dark-palette variant picker (only meaningful in dark mode) —
                    // COOL · WARM · INK · OLED (pure black for OLED iPad panels).
                    if st.theme_dark {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                            ui.label(egui::RichText::new("SHADE").size(theme::FS_LABEL * s).color(theme::FAINT()));
                            for (i, lab) in ["COOL", "WARM", "INK", "OLED"].into_iter().enumerate() {
                                let (vc, _) = ui.allocate_exact_size(Vec2::new(44.0 * s, 30.0 * s), Sense::hover());
                                if v3_button(ui, vc, ("v3_var", i), lab, st.dark_variant == i as u8, true, s) {
                                    st.dark_variant = i as u8;
                                }
                            }
                        });
                    }
                    ui.add_space(theme::SP_XS * s);
                    let (cc, _) = ui.allocate_exact_size(Vec2::new(64.0 * s, 30.0 * s), Sense::hover());
                    if v3_button(ui, cc, ("v3_close", 0), "CLOSE", false, true, s) { st.menu_open = false; }
                });
        });
}

/// Right-card body (inside the epo::card_tabs frame): dispatch to the page.
/// card_tabs' body ui has no real height budget while it's still measuring, so
/// we CLAIM a `body_px`-tall region up front and lay each page out inside it.
fn draw_v3_right_body(ui: &mut egui::Ui, params: &Arc<PetalParams>,
                      setter: &ParamSetter, page: RightTab, body_px: f32, s: f32) {
    let width = ui.available_width();
    let (region, _) = ui.allocate_exact_size(Vec2::new(width, body_px), Sense::hover());
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(region)
        .layout(egui::Layout::top_down(egui::Align::Min)));
    cui.set_clip_rect(region.expand(1.0 * s));
    match page {
        RightTab::Pan | RightTab::Gain => draw_v3_tap_rows(&mut cui, region, params, setter, page, s),
        RightTab::Filter => draw_v3_filter_blocks(&mut cui, region, params, setter, s),
        _ => draw_v3_space(&mut cui, region, params, setter, s),
    }
}

// ─── RIGHT card — SPACE page: crossfeed + diffusion merged (owner's tab) ────
// Uses the SAME 65×75 cell grid / 8px gaps (pitch 69) and 48px knobs as the
// DELAY card, centered in the wider right card, so spacing stays steady.
fn draw_v3_space(ui: &mut egui::Ui, region: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, s: f32) {
    let p = ui.painter().clone();
    // origin so an N-cell group (token pitch) sits centered in the card
    let group_x = |n: usize| region.center().x
        - (n as f32 * tk::cell_w() + (n as f32 - 1.0) * tk::space_md()) * 0.5 * s;
    let cell = |gx: f32, i: usize, row_y: f32| {
        Rect::from_min_size(Pos2::new(gx + i as f32 * tk::pitch_x() * s, region.min.y + row_y * s),
                            Vec2::new(tk::cell_w() * s, tk::cell_h() * s))
    };
    let label = |r: Rect, text: &str| {
        p.text(Pos2::new(r.center().x, r.min.y + 5.0 * s), Align2::CENTER_TOP, text,
               FontId::new(tk::font_body() * s, FontFamily::Proportional), theme::FAINT());
    };
    let knob_in = |ui: &mut egui::Ui, r: Rect, param: &FloatParam, lab: &str, disp: &str| {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Center)));
        epo::knob_ex(&mut c, setter, param, lab, tk::knob() * s, Accent::Ink, false, None, Some(disp));
    };
    // section title, centered over an N-cell group
    let sec = |n: usize, row_y: f32, text: &str| {
        p.text(Pos2::new(region.center().x, region.min.y + row_y * s), Align2::CENTER_TOP, text,
               FontId::new(theme::FS_LABEL * s, FontFamily::Proportional), theme::FAINT());
    };

    // Five sections on a compact pitch (sec title → chips row +18 → knob row
    // +48 → next divider). Total ≈ 690 of the ~722-unit body.
    let divider = |row_y: f32| {
        let dy = region.min.y + row_y * s;
        p.line_segment([Pos2::new(region.min.x + 8.0 * s, dy), Pos2::new(region.max.x - 8.0 * s, dy)], Stroke::new(tk::stroke_hair() * s, theme::line()));
    };
    let gx3 = group_x(3);
    // chip centered in the cell's control band (like the DELAY mode chips)
    let chip_cell = |cl: Rect| Rect::from_center_size(Pos2::new(cl.center().x, cl.min.y + 22.0 * s), Vec2::new(60.0 * s, tk::ctl_h() * s));

    // ── XFEED: 3 mode chips + BLEND knob ──
    sec(3, 6.0, "XFEED MODE");
    let cf_mode = params.crossfeed_mode.value();
    for (i, (lab, m)) in [("MIX", CrossfeedMode::Mix), ("PING", CrossfeedMode::PingPong), ("DIFF", CrossfeedMode::Diffuse)].into_iter().enumerate() {
        let bcell = chip_cell(cell(gx3, i, 24.0));
        if v3_button(ui, bcell, ("v3xf", i), lab, cf_mode == m, true, s) { set_enum(setter, &params.crossfeed_mode, m); }
    }
    let cf = params.crossfeed.value();
    let cf_str = if cf < 0.01 { "OFF".into() } else { format!("{:.0}%", cf * 100.0) };
    let cf_lab = match cf_mode { CrossfeedMode::Mix => "BLEND", CrossfeedMode::PingPong => "BOUNCE", CrossfeedMode::Diffuse => "DEPTH" };
    knob_in(ui, cell(group_x(1), 0, 68.0), &params.crossfeed, cf_lab, &cf_str);

    divider(148.0);

    // ── DIFFUSE: 3 routing chips + AMOUNT & SIZE knobs ──
    sec(3, 156.0, "DIFFUSE ROUTING");
    let route = params.diffuse_routing.value();
    for (i, (lab, m)) in [("IN", DiffuseRouting::Input), ("FB", DiffuseRouting::Feedback), ("OUT", DiffuseRouting::Output)].into_iter().enumerate() {
        let bcell = chip_cell(cell(gx3, i, 174.0));
        if v3_button(ui, bcell, ("v3df", i), lab, route == m, true, s) { set_enum(setter, &params.diffuse_routing, m); }
    }
    let da = params.diffuse_amount.value();
    let da_str = if da < 0.01 { "OFF".into() } else { format!("{:.0}%", da * 100.0) };
    let ds_str = format!("{:.2}\u{00D7}", params.diffuse_size.value());
    let gx2 = group_x(2);
    knob_in(ui, cell(gx2, 0, 222.0), &params.diffuse_amount, "AMOUNT", &da_str);
    knob_in(ui, cell(gx2, 1, 222.0), &params.diffuse_size, "SIZE", &ds_str);

    divider(302.0);

    // ── DEGRADE: routing chips + tape TYPE chips + DRIVE / WEAR / AMOUNT ──
    // Physically-modeled tape (re-deemer port: J-A hysteresis + heads EQ +
    // self-erasure). FB routing = generation loss: every repeat is another
    // tape generation, like copies of copies.
    sec(3, 310.0, "DEGRADE ROUTING");
    let dg_route = params.degrade_routing.value();
    for (i, (lab, m)) in [("IN", DiffuseRouting::Input), ("FB", DiffuseRouting::Feedback), ("OUT", DiffuseRouting::Output)].into_iter().enumerate() {
        let bcell = chip_cell(cell(gx3, i, 328.0));
        if v3_button(ui, bcell, ("v3dg", i), lab, dg_route == m, true, s) { set_enum(setter, &params.degrade_routing, m); }
    }
    let dg_type = params.degrade_type.value();
    for (i, (lab, m)) in [("I FE", TapeType::I), ("II CR", TapeType::II), ("IV MT", TapeType::IV)].into_iter().enumerate() {
        let bcell = chip_cell(cell(gx3, i, 372.0));
        if v3_button(ui, bcell, ("v3dgt", i), lab, dg_type == m, true, s) { set_enum(setter, &params.degrade_type, m); }
    }
    let dg_amt = params.degrade_amount.value();
    let dg_amt_str = if dg_amt < 0.01 { "OFF".into() } else { format!("{:.0}%", dg_amt * 100.0) };
    let dg_drv_str = format!("{:+.1} dB", params.degrade_drive.value());
    let dg_wear_str = format!("{:.0}%", params.degrade_wear.value() * 100.0);
    knob_in(ui, cell(gx3, 0, 420.0), &params.degrade_drive, "DRIVE", &dg_drv_str);
    knob_in(ui, cell(gx3, 1, 420.0), &params.degrade_wear, "WEAR", &dg_wear_str);
    knob_in(ui, cell(gx3, 2, 420.0), &params.degrade_amount, "AMOUNT", &dg_amt_str);

    divider(505.0);

    // ── SATURATE: 3 routing chips + DRIVE / AMOUNT knobs ──
    // Playable tanh drive stage; the loop's fixed safety tanh is separate.
    sec(3, 513.0, "SATURATE ROUTING");
    let st_route = params.sat_routing.value();
    for (i, (lab, m)) in [("IN", DiffuseRouting::Input), ("FB", DiffuseRouting::Feedback), ("OUT", DiffuseRouting::Output)].into_iter().enumerate() {
        let bcell = chip_cell(cell(gx3, i, 531.0));
        if v3_button(ui, bcell, ("v3st", i), lab, st_route == m, true, s) { set_enum(setter, &params.sat_routing, m); }
    }
    let st_amt = params.sat_amount.value();
    let st_amt_str = if st_amt < 0.01 { "OFF".into() } else { format!("{:.0}%", st_amt * 100.0) };
    let st_drv_str = format!("{:.1} dB", params.sat_drive.value());
    knob_in(ui, cell(gx2, 0, 579.0), &params.sat_drive, "DRIVE", &st_drv_str);
    knob_in(ui, cell(gx2, 1, 579.0), &params.sat_amount, "AMOUNT", &st_amt_str);
    let _ = label;
}

/// A/B compare: save the live params into the current slot, then load the
/// target (apply its snapshot, or seed it as a copy of the current on first use).
pub(crate) fn ab_switch(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter, target: u8) {
    if st.ab_active == target { return; }
    st.ab_slots[st.ab_active as usize] = Some(crate::presets::capture(params));
    if let Some(snap) = st.ab_slots[target as usize].clone() {
        crate::presets::apply(params, setter, &snap, crate::presets::VALUES_FORMAT_PLAIN, &mut crate::presets::Sent::default());
    } else {
        // first visit to this slot — start it as a copy of what's live now
        st.ab_slots[target as usize] = st.ab_slots[st.ab_active as usize].clone();
    }
    st.ab_active = target;
}

// ─── Header: hamburger · απόηχος · A/B · preset pill · sun ─────────────────
fn draw_v3_header(ui: &mut egui::Ui, p: &egui::Painter, page: Rect, st: &mut EditorState,
                  params: &Arc<PetalParams>, setter: &ParamSetter, s: f32) {
    let hx = page.min.x;
    let hy = page.min.y;
    let w = page.width();
    // hamburger (3 lines) — painted 20px wide, hit area the full 44pt square
    let ham_hit = Rect::from_min_size(Pos2::new(hx + 10.0 * s, hy + 6.0 * s), Vec2::splat(44.0 * s));
    let (ham_clicked, ham) = tap_or_click(ui, ham_hit, ui.id().with("v3_ham"), true);
    let hc = if ham.hovered() { theme::INK() } else { theme::SOFT() };
    for k in 0..3 {
        let y = hy + 21.0 * s + k as f32 * 6.5 * s;
        p.line_segment([Pos2::new(hx + 22.0 * s, y), Pos2::new(hx + 42.0 * s, y)], Stroke::new(1.6 * s, hc));
    }
    if ham_clicked { st.menu_open = !st.menu_open; }

    // title + accent dot + version
    p.text(Pos2::new(hx + 56.0 * s, hy + HEADER_H * 0.5 * s), Align2::LEFT_CENTER, "απόηχος",
           FontId::new(21.0 * s, FontFamily::Proportional), theme::INK());
    p.circle_filled(Pos2::new(hx + 148.0 * s, hy + 36.0 * s), 2.4 * s, theme::accent_default());
    p.text(Pos2::new(hx + 158.0 * s, hy + HEADER_H * 0.5 * s + 3.0 * s), Align2::LEFT_CENTER,
           &format!("v{} · v3 preview", env!("CARGO_PKG_VERSION")),
           FontId::new(8.5 * s, FontFamily::Monospace), theme::FAINT());

    // preset pill (◀ name ▶) — kit widget; ◀/▶ step, name opens the palette.
    // Dirty `*` when the loaded preset has diverged from live params.
    let pill_w = 207.0;
    let pill_x = hx + w * 0.5 - pill_w * 0.5 * s;
    let pill_rect = Rect::from_min_size(Pos2::new(pill_x, hy + (HEADER_H * 0.5 - 13.5) * s), Vec2::new(pill_w * s, 27.0 * s));
    let dirty = crate::palette_v3::is_dirty(st, params);
    let pname = st.current_preset.clone().unwrap_or_else(|| "INIT".into());
    let plabel = if dirty { format!("{pname}  *") } else { pname };
    let mut pcu = ui.new_child(egui::UiBuilder::new().max_rect(pill_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    match epo::preset_slot(&mut pcu, &plabel, pill_w, 27.0, "v3_preset") {
        2 => {
            st.preset_open = !st.preset_open;
            if st.preset_open {
                st.preset_search.clear();
                st.palette_sel = 0;
                st.palette_focus = true;
                st.preset_guard = 2;
            }
        }
        d if d != 0 => crate::palette_v3::preset_step(st, params, setter, d),
        _ => {}
    }

    // sun (theme toggle) — painted 16px, hit 44pt
    let sun_c = Pos2::new(hx + w - 30.0 * s, hy + HEADER_H * 0.5 * s);

    // A/B compare — two 30×27 bordered boxes left of the sun with a clear ~22px
    // gap so they read as two distinct buttons, not a joined segmented control.
    let ab_y = sun_c.y;
    let b_cell = Rect::from_center_size(Pos2::new(sun_c.x - 52.0 * s, ab_y), Vec2::new(30.0 * s, 27.0 * s));
    if v3_button(ui, b_cell, ("v3_ab", 1), "B", st.ab_active == 1, true, s) { ab_switch(st, params, setter, 1); }
    let a_cell = Rect::from_center_size(Pos2::new(sun_c.x - 104.0 * s, ab_y), Vec2::new(30.0 * s, 27.0 * s));
    if v3_button(ui, a_cell, ("v3_ab", 0), "A", st.ab_active == 0, true, s) { ab_switch(st, params, setter, 0); }

    let sun_hit = Rect::from_center_size(sun_c, Vec2::splat(44.0 * s));
    let (sun_clicked, sun) = tap_or_click(ui, sun_hit, ui.id().with("v3_sun"), true);
    let sc = if sun.hovered() { theme::INK() } else { theme::SOFT() };
    p.circle_stroke(sun_c, 5.0 * s, Stroke::new(1.4 * s, sc));
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let (d0, d1) = (7.5 * s, 10.0 * s);
        p.line_segment([Pos2::new(sun_c.x + a.cos() * d0, sun_c.y + a.sin() * d0),
                        Pos2::new(sun_c.x + a.cos() * d1, sun_c.y + a.sin() * d1)], Stroke::new(1.2 * s, sc));
    }
    if sun_clicked { st.theme_dark = !st.theme_dark; }

    p.line_segment([Pos2::new(hx, hy + HEADER_H * s), Pos2::new(hx + w, hy + HEADER_H * s)],
                   Stroke::new(1.0 * s, theme::line()));
}

/// Small label tag straddling the card's top-left edge (the Figma card title).
// Card title chip, matched to epo::card_tabs' section title: same title_fs
// PROPORTIONAL font + SOFT colour, painted on a PANEL-filled chip that cuts the
// card's top-border hairline (no box stroke) — so single-title cards read the
// same as the tabbed right card.
fn draw_card_tag(p: &egui::Painter, card_min: Pos2, label: &str, s: f32) {
    let font = FontId::new(theme::title_fs() * s, FontFamily::Proportional);
    let up = label.to_uppercase();
    let galley = p.layout_no_wrap(up.clone(), font.clone(), theme::SOFT());
    let w = galley.size().x + theme::SP_MD * 2.0 * s;
    let h = galley.size().y + 4.0 * s;
    let r = Rect::from_min_size(Pos2::new(card_min.x + theme::SP_XL * s, card_min.y - h * 0.5),
                                Vec2::new(w, h));
    // Fill with the PAGE background (not the card) so the tag reads as cut out of
    // the page — it "fades into" the backdrop rather than sitting on a chip.
    p.rect_filled(r, 3.0 * s, theme::PAPER());
    p.text(r.center(), Align2::CENTER_CENTER, &up, font, theme::SOFT());
}

// ─── LEFT card — Figma cell grid: 65×75 cells in a 219 column at x 32 ──────
fn draw_v3_left(ui: &mut egui::Ui, card: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                tempo: f32, s: f32) {
    // Center the 3-cell control grid in the card (owner 2026-07-14: the DELAY
    // card sat left-anchored at a fixed 13px inset while the 3-cell group is only
    // ~203 wide, leaving ~16px more dead space on the right — the middle control
    // rode 8px left of center). Every other card centers its groups via group_x
    // (region.center − groupWidth/2); match that so DELAY reads justify-center too.
    // grp_w = 2·pitch + cell = the exact span of a 3-cell row in either sheet.
    let col_w = (2.0 * tk::pitch_x() + tk::cell_w()) * s;
    let col_x = card.center().x - col_w * 0.5;
    // Cell grid from the sizing tokens (v3 sheet: 65-wide cells, 4px gaps,
    // pitch 69; τόπος sheet: 64 + 12 = 76).
    let cell = |row_y: f32, i: usize| {
        Rect::from_min_size(Pos2::new(col_x + i as f32 * tk::pitch_x() * s, card.min.y + row_y * s),
                            Vec2::new(tk::cell_w() * s, tk::cell_h() * s))
    };
    let full_row = |row_y: f32| {
        Rect::from_min_size(Pos2::new(col_x, card.min.y + row_y * s), Vec2::new(col_w, tk::cell_h() * s))
    };
    let knob_in = |ui: &mut egui::Ui, r: Rect, f: &mut dyn FnMut(&mut egui::Ui)| {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r)
            .layout(egui::Layout::top_down(egui::Align::Center)));
        f(&mut c);
    };
    let label = |ui: &egui::Ui, r: Rect, text: &str| {
        ui.painter().text(Pos2::new(r.center().x, r.min.y + 5.0 * s), Align2::CENTER_TOP, text,
                          FontId::new(tk::font_body() * s, FontFamily::Proportional), theme::FAINT());
    };
    let divider = |ui: &egui::Ui, row_y: f32| {
        let y = card.min.y + row_y * s + 7.0 * s;
        ui.painter().line_segment(
            [Pos2::new(card.min.x + 4.0 * s, y), Pos2::new(card.min.x + 241.0 * s, y)],
            Stroke::new(tk::stroke_hair() * s, theme::line()));
    };
    // Row ladder: rows one pitch apart, dividers one pitch below the previous
    // row, the next row 19 under a divider — reproduces the Figma ladder
    // (20/107/194/281 · 368 · 387 · 474 · 493/580/667) exactly at pitch 87.
    let pv = tk::pitch_y();
    let r0 = 20.0;
    let (r1, r2, r3) = (r0 + pv, r0 + 2.0 * pv, r0 + 3.0 * pv);
    let d1 = r3 + pv;
    let r4 = d1 + 19.0;
    let d2 = r4 + pv;
    let r5 = d2 + 19.0;
    let (r6, r7) = (r5 + pv, r5 + 2.0 * pv);
    // (20) TIME L · LINK · TIME R
    let l_str = params.format_base_time(params.base_time_l.value(), tempo);
    let r_val = if params.time_linked.value() { params.base_time_l.value() } else { params.base_time_r.value() };
    let r_str = crate::params::PetalParams::format_base_time_mode(params.sync_mode_r_eff(), r_val, tempo);
    knob_in(ui, cell(r0, 0), &mut |c| {
        epo::knob_ex(c, setter, &params.base_time_l, "Time L", tk::knob(), Accent::Ink, false, None, Some(&l_str));
    });
    v3_toggle(ui, cell(r0, 1), ("v3_link", 0), "Link", &params.time_linked, setter, true, s);
    knob_in(ui, cell(r0, 2), &mut |c| {
        if params.time_linked.value() {
            epo::knob_ex(c, setter, &params.base_time_l, "Time R", tk::knob(), Accent::Ink, false, None, Some(&r_str));
        } else {
            epo::knob_ex(c, setter, &params.base_time_r, "Time R", tk::knob(), Accent::Ink, false, None, Some(&r_str));
        }
    });

    // (107) SYNC L · SYNC R — per-channel; R follows L while time is linked
    let linked = params.time_linked.value();
    let cl = cell(r1, 0);
    label(ui, cl, "Sync L");
    let sync_l_on = params.sync_mode.value() == SyncMode::Sync;
    if v3_button(ui, cl, ("v3_sync", 0), if sync_l_on { "ON" } else { "OFF" }, sync_l_on, true, s) {
        set_enum(setter, &params.sync_mode, if sync_l_on { SyncMode::Free } else { SyncMode::Sync });
    }
    let cr = cell(r1, 2);
    label(ui, cr, "Sync R");
    let sync_r_on = params.sync_mode_r_eff() == SyncMode::Sync;
    if v3_button(ui, cr, ("v3_sync", 1), if sync_r_on { "ON" } else { "OFF" }, sync_r_on, !linked, s) {
        set_enum(setter, &params.sync_mode_r, if sync_r_on { SyncMode::Free } else { SyncMode::Sync });
    }

    // (194) DELAY MODE — one button per cell (Figma grid alignment)
    label(ui, full_row(r2), "Delay mode");
    let cur = params.delay_mode.value();
    for (i, (lab, m)) in [("Repitch", DelayMode::Repitch), ("Fade", DelayMode::Fade), ("Jump", DelayMode::Jump)].into_iter().enumerate() {
        if v3_button(ui, cell(r2, i), ("v3_dmode", i), lab, cur == m, true, s) {
            set_enum(setter, &params.delay_mode, m);
        }
    }

    // (281) CHANNEL MODE
    label(ui, full_row(r3), "Delay channel mode");
    let cur = params.channel_mode.value();
    for (i, (lab, m)) in [("Stereo", ChannelMode::Stereo), ("M/S", ChannelMode::MidSide), ("Mono", ChannelMode::Mono)].into_iter().enumerate() {
        if v3_button(ui, cell(r3, i), ("v3_cmode", i), lab, cur == m, true, s) {
            set_enum(setter, &params.channel_mode, m);
        }
    }

    divider(ui, d1);

    // (387) IN · MIX · OUT
    let mix_str = format!("{:.0}%", params.dry_wet.value() * 100.0);
    knob_in(ui, cell(r4, 0), &mut |c| { epo::knob(c, setter, &params.input_gain, "In", tk::knob(), Accent::Ink, false); });
    knob_in(ui, cell(r4, 1), &mut |c| { epo::knob_ex(c, setter, &params.dry_wet, "Mix", tk::knob(), Accent::Ink, false, None, Some(&mix_str)); });
    knob_in(ui, cell(r4, 2), &mut |c| { epo::knob(c, setter, &params.output_gain, "Out", tk::knob(), Accent::Ink, false); });

    // (SATURATION toggle removed v2.21.0 — superseded by the SPACE page's
    // SATURATE block routed to IN; the freed rows host the FB CURVE chips.)

    divider(ui, d2);

    // (493) FEEDBACK · FEEDB TIME · LOCK
    let fb_str = format!("{:.0}%", params.feedback.value() * 100.0);
    let locked = params.feedback_time_locked.value();
    let ft_str = if locked { "= taps".to_string() } else { params.format_feedback_time(tempo) };
    knob_in(ui, cell(r5, 0), &mut |c| {
        epo::knob_ex(c, setter, &params.feedback, "Feedback", tk::knob(), Accent::Ink, false, None, Some(&fb_str));
    });
    knob_in(ui, cell(r5, 1), &mut |c| {
        epo::knob_ex(c, setter, &params.feedback_time, "Feedb time", tk::knob(), Accent::Ink, false, None, Some(&ft_str));
    });
    let lock_cell = cell(r5, 2);
    label(ui, lock_cell, "Lock feedb");
    let lk_on = params.feedback_time_locked.value();
    if v3_button(ui, lock_cell, ("v3_lock", 0), if lk_on { "ON" } else { "OFF" }, lk_on, true, s) {
        setter.begin_set_parameter(&params.feedback_time_locked);
        setter.set_parameter(&params.feedback_time_locked, !lk_on);
        setter.end_set_parameter(&params.feedback_time_locked);
    }

    // (580) FEEDBACK CURVE — decay shape of the repeats, one chip per cell
    // (owner: "under the feedback, between the feedback and the taps count").
    label(ui, full_row(r6), "Feedback curve");
    let curve = params.fb_curve.value();
    for (i, (lab, m)) in [("Natural", FbCurve::Natural), ("Bloom", FbCurve::Bloom), ("Gate", FbCurve::Gate)].into_iter().enumerate() {
        if v3_button(ui, cell(r6, i), ("v3_fbc", i), lab, curve == m, true, s) {
            set_enum(setter, &params.fb_curve, m);
        }
    }

    // (667) TAPS COUNT — slider (owner: "tap count will be a slider")
    let r = full_row(r7);
    label(ui, r, "Taps count");
    let sr = Rect::from_min_size(Pos2::new(r.min.x, r.min.y + 30.0 * s), Vec2::new(r.width(), 30.0 * s));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(sr)
        .layout(egui::Layout::top_down(egui::Align::Min)));
    epo::slider(&mut c, setter, &params.num_taps_active, "", r.width(), 0.0, Accent::Ink, false);
}

// ─── CENTER — stems viz well ────────────────────────────────────────────────
/// Center-card body (inside the GRID · STEMS card_tabs frame).
/// GRID = time × pan, STEMS = time × volume — both fully draggable; the tap
/// panel shows whichever knob is NOT on the view's axes.
fn draw_v3_viz_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter,
                    st: &mut EditorState, tempo: f32, body_px: f32, s: f32) {
    let width = ui.available_width();
    let (region, _) = ui.allocate_exact_size(Vec2::new(width, body_px), Sense::hover());
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(region)
        .layout(egui::Layout::top_down(egui::Align::Min)));
    cui.set_clip_rect(region.expand(1.0 * s));
    let well = region.shrink(2.0 * s);
    let p = cui.painter().clone();
    p.rect_stroke(well, 8.0 * s, Stroke::new(tk::stroke_hair() * s, theme::line()), StrokeKind::Inside);
    // Editing views reserve x-range for ONE echo cycle (the single ghost);
    // ECHO keeps the classic 4-cycle reserve for the full tail.
    let viz = if st.view3 == 2 { compute_viz_data(params, tempo) }
        else { crate::gui::compute_viz_data_ex(params, tempo, 1) };
    let is_sync = params.sync_mode.value() == SyncMode::Sync;
    match st.view3 {
        0 => draw_tap_field(&mut cui, well, params, setter, st, &viz, tempo, is_sync, s),
        1 => draw_tap_stems(&mut cui, well, params, setter, st, &viz, tempo, is_sync, s),
        // ECHO — the classic read-only bar view (full feedback tail).
        _ => crate::gui::draw_stems(&p, well, params, &viz, tempo, is_sync, s),
    }
}

/// FIELD view — the taps as draggable objects on a time × pan plane.
/// X = position on the time grid (drag writes the tap's `time_offset`);
/// Y = pan, L up / R down (relabels MID/SIDE in M/S mode — follows the
/// channel-mode change). Dot size tracks tap gain. Click a tap → GAIN +
/// FILTER panel, bottom-center. Double-click resets offset + pan.
fn draw_tap_field(ui: &mut egui::Ui, well: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                  st: &mut EditorState, viz: &crate::gui::VizData, tempo: f32, is_sync: bool, s: f32) {
    let _ = tempo;
    let p = ui.painter().clone();
    let (pad_l, pad_r, pad_t, pad_b) = (36.0 * s, 24.0 * s, 18.0 * s, 24.0 * s);
    let draw_l = well.min.x + pad_l;
    let draw_r = well.max.x - pad_r;
    let draw_t = well.min.y + pad_t;
    let draw_b = well.max.y - pad_b;
    let mid_y = (draw_t + draw_b) * 0.5;
    let half_h = (draw_b - draw_t) * 0.5 - 14.0 * s;
    let draw_w = draw_r - draw_l;
    let ch_mode = params.channel_mode.value();

    // Background click = deselect. Registered FIRST so the tap dots and the
    // panel (registered later) win pointer overlaps.
    let bg = ui.interact(well, ui.id().with("v3_field_bg"), Sense::click());
    if bg.clicked() { st.field_sel = None; }

    let (top_lab, bot_lab) = match ch_mode {
        ChannelMode::Stereo => ("L", "R"),
        ChannelMode::MidSide => ("MID", "SIDE"),
        ChannelMode::Mono => ("M", "M"),
    };
    p.text(Pos2::new(well.min.x + 10.0 * s, draw_t + 4.0 * s), Align2::LEFT_TOP, top_lab,
           FontId::new(9.0 * s, FontFamily::Proportional), theme::FAINT());
    p.text(Pos2::new(well.min.x + 10.0 * s, draw_b - 4.0 * s), Align2::LEFT_BOTTOM, bot_lab,
           FontId::new(9.0 * s, FontFamily::Proportional), theme::FAINT());
    // Y-axis grid: pan quarter-lines (faint) + the dashed center; axis names
    // so the plane reads at a glance.
    for frac in [-0.5f32, 0.5] {
        let y = mid_y + frac * half_h;
        p.line_segment([Pos2::new(draw_l, y), Pos2::new(draw_r, y)],
                       Stroke::new(tk::stroke_hair() * s, theme::with_alpha(theme::line(), 110)));
    }
    p.text(Pos2::new(well.min.x + 10.0 * s, mid_y), Align2::LEFT_CENTER, "PAN",
           FontId::new(8.0 * s, FontFamily::Proportional), theme::FAINT());
    p.text(Pos2::new(draw_r, draw_t + 4.0 * s), Align2::RIGHT_TOP, "TIME →",
           FontId::new(8.0 * s, FontFamily::Proportional), theme::FAINT());
    crate::gui::dashed_h(&p, Pos2::new(draw_l, mid_y), draw_w, theme::WAVE(), s);
    // Tight taps-only range (same as the old v2 Grid view) for precise placing.
    crate::gui::paint_timeline_grid(&p, draw_l, draw_r, draw_t, draw_b,
        viz.grid_range_secs_taps_only, tempo, is_sync, well.max.y - 6.0 * s, s);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let span = viz.span_l_secs.max(1e-4);
    let range = viz.grid_range_secs_taps_only.max(1e-4);

    // SNAP — quantize dragged taps to base-time multiples (shared with STEMS).
    let snap_cell = Rect::from_center_size(Pos2::new(draw_r - 30.0 * s, draw_t + 30.0 * s),
                                           Vec2::new(60.0 * s, 27.0 * s));
    if v3_button(ui, snap_cell, ("v3_snap", 0), "SNAP", st.snap3, true, s) { st.snap3 = !st.snap3; }

    // First feedback repeat, ghosted in WITHOUT extending the x-range (owner
    // 2026-07-14): the repeat shows only if it already lands in view — the
    // taps-only range stays tight so temporal mapping precision is unchanged.
    // Same recipe as STEMS: cycle_gains[1] level, faint dots; size follows the
    // repeat's level like the real dots' size follows gain.
    if viz.n_echo_cycles >= 1 {
        let cg = viz.cycle_gains[1];
        if cg >= 0.001 {
            let drop_db = 20.0 * cg.log10();
            // fb cycle: fraction of the echo range → seconds → THIS view's range
            let fb_dx = viz.fb_cycle_norm * viz.grid_range_secs / range;
            for i in 0..n_active {
                let tap = &params.taps[i];
                if !tap.active.value() { continue; }
                let t = viz.times_l_grid[i] + fb_dx;
                if t > 1.0 { continue; }
                let gpos = gain_db_to_pos(tap.gain_db.value() + drop_db).clamp(0.0, 1.0);
                if gpos <= 0.001 { continue; }
                let gc = Pos2::new(draw_l + t * draw_w, mid_y + tap.pan.value() * half_h);
                p.circle_filled(gc, (4.0 + 7.0 * gpos) * s,
                                tap_color(i, n_active).linear_multiply(0.25));
            }
        }
    }

    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let x = draw_l + viz.times_l_grid[i].min(1.0) * draw_w;
        let pan = tap.pan.value();
        let y = mid_y + pan * half_h; // pan −1 (L / MID) at the top
        let c = Pos2::new(x, y);
        let gain_n = gain_db_to_pos(tap.gain_db.value()).clamp(0.0, 1.0);
        let r_vis = (4.0 + 7.0 * gain_n) * s;
        let dim = if active { 1.0 } else { 0.35 };
        let col = tap_color(i, n_active).linear_multiply(dim);

        // Hit area ≥ visual (44pt floor on touch). (The tap panel lives in the
        // SHAPE card now — nothing floats over the plot anymore.)
        {
            let hit = if theme::touch() { theme::TOUCH_MIN_HIT } else { 28.0 } * s;
            let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(hit)),
                                   ui.id().with(("v3_fld", i)), Sense::click_and_drag());
            let virt_id = ui.id().with(("v3_fld_virt", i));
            if resp.drag_started() {
                setter.begin_set_parameter(&tap.time_offset);
                setter.begin_set_parameter(&tap.pan);
                st.field_sel = Some(i);
                // raw (unsnapped) offset accumulator for this drag — quantizing
                // the incremental value directly would make the dot sticky.
                ui.memory_mut(|m| m.data.insert_temp(virt_id, tap.time_offset.value()));
            }
            if resp.dragged() {
                let d = resp.drag_delta();
                // px → seconds on this range → fraction of the cluster span
                let d_off = d.x / draw_w * range / span;
                let mut virt: f32 = ui.memory(|m| m.data.get_temp(virt_id))
                    .unwrap_or_else(|| tap.time_offset.value());
                virt = (virt + d_off).clamp(-0.5, 0.5);
                ui.memory_mut(|m| m.data.insert_temp(virt_id, virt));
                let off = if st.snap3 {
                    snap_offset(viz.times_l_grid[i] * range, tap.time_offset.value(), virt, span)
                } else { virt };
                setter.set_parameter(&tap.time_offset, off.clamp(-0.5, 0.5));
                setter.set_parameter(&tap.pan, (pan + d.y / half_h).clamp(-1.0, 1.0));
            }
            if resp.drag_stopped() {
                setter.end_set_parameter(&tap.time_offset);
                setter.end_set_parameter(&tap.pan);
            }
            if resp.clicked() {
                st.field_sel = if st.field_sel == Some(i) { None } else { Some(i) };
            }
            if resp.double_clicked() {
                setter.begin_set_parameter(&tap.time_offset);
                setter.set_parameter(&tap.time_offset, 0.0);
                setter.end_set_parameter(&tap.time_offset);
                setter.begin_set_parameter(&tap.pan);
                setter.set_parameter(&tap.pan, 0.0);
                setter.end_set_parameter(&tap.pan);
            }
            if resp.hovered() {
                p.circle_stroke(c, r_vis + 5.0 * s, Stroke::new(1.0 * s, theme::with_alpha(theme::INK(), 60)));
            }
        }

        // (no guide bar to the baseline — owner 2026-07-13: GRID dots float free)
        p.circle_filled(c, r_vis, col);
        if st.field_sel == Some(i) {
            p.circle_stroke(c, r_vis + 3.0 * s, Stroke::new(1.0 * s, theme::INK()));
        }
        p.text(Pos2::new(x, y - r_vis - 4.0 * s), Align2::CENTER_BOTTOM, format!("{:02}", i + 1),
               FontId::new(8.0 * s, FontFamily::Monospace),
               theme::with_alpha(theme::SOFT(), (160.0 * dim) as u8));
    }
}

/// STEMS view — time × volume lollipops. Drag X = time offset, drag Y = tap
/// gain (dB, DAW-fader skew: 75% height = 0 dB). Fixed dot size — height
/// already encodes level (owner: size-by-gain not needed here). Echo repeats
/// ghost in from the same per-cycle FB-CURVE levels the old bar view used.
fn draw_tap_stems(ui: &mut egui::Ui, well: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                  st: &mut EditorState, viz: &crate::gui::VizData, tempo: f32, is_sync: bool, s: f32) {
    let _ = tempo;
    let p = ui.painter().clone();
    let (pad_l, pad_r, pad_t, pad_b) = (36.0 * s, 40.0 * s, 18.0 * s, 24.0 * s);
    let draw_l = well.min.x + pad_l;
    let draw_r = well.max.x - pad_r;
    let draw_t = well.min.y + pad_t;
    let draw_b = well.max.y - pad_b;
    let draw_w = draw_r - draw_l;
    let plot_h = (draw_b - draw_t) - 10.0 * s;

    let bg = ui.interact(well, ui.id().with("v3_stems_bg"), Sense::click());
    if bg.clicked() { st.field_sel = None; }

    // dB gridlines (right-side labels, like the owner's mock): positions from
    // the same DAW-fader skew the GAIN rows use, so 0 dB sits at 75% height.
    for (db, lab) in [(6.0f32, "+6"), (0.0, "0 dB"), (-12.0, "−12"), (-24.0, "−24")] {
        let y = draw_b - gain_db_to_pos(db).clamp(0.0, 1.0) * plot_h;
        p.line_segment([Pos2::new(draw_l, y), Pos2::new(draw_r, y)],
                       Stroke::new(tk::stroke_hair() * s, theme::with_alpha(theme::line(), 110)));
        p.text(Pos2::new(draw_r + 4.0 * s, y), Align2::LEFT_CENTER, lab,
               FontId::new(7.5 * s, FontFamily::Proportional), theme::FAINT());
    }
    p.text(Pos2::new(well.min.x + 10.0 * s, draw_t + 4.0 * s), Align2::LEFT_TOP, "VOL",
           FontId::new(8.0 * s, FontFamily::Proportional), theme::FAINT());
    // Echo-inclusive range (repeats stay visible), like the old bar view.
    crate::gui::paint_timeline_grid(&p, draw_l, draw_r, draw_t, draw_b,
        viz.grid_range_secs, tempo, is_sync, well.max.y - 6.0 * s, s);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let span = viz.span_l_secs.max(1e-4);
    let range = viz.grid_range_secs.max(1e-4);

    // SNAP — same toggle as GRID (quantize dragged taps to base-time multiples).
    let snap_cell = Rect::from_center_size(Pos2::new(draw_r - 30.0 * s, draw_t + 30.0 * s),
                                           Vec2::new(60.0 * s, 27.0 * s));
    if v3_button(ui, snap_cell, ("v3_snap", 1), "SNAP", st.snap3, true, s) { st.snap3 = !st.snap3; }

    // Ghost the FIRST repeat only (owner 2026-07-13: the full echo tail
    // buried the editable dots at high feedback — "everything is hidden").
    // One faint cycle keeps the feedback landing point readable without
    // cluttering the editing surface.
    if viz.n_echo_cycles >= 1 {
        let cg = viz.cycle_gains[1];
        if cg >= 0.001 {
            let drop_db = 20.0 * cg.log10();
            for i in 0..n_active {
                let tap = &params.taps[i];
                if !tap.active.value() { continue; }
                let t = viz.times_l[i] + viz.fb_cycle_norm;
                if t > 1.0 { continue; }
                let pos = gain_db_to_pos(tap.gain_db.value() + drop_db).clamp(0.0, 1.0);
                if pos <= 0.001 { continue; }
                let c = Pos2::new(draw_l + t * draw_w, draw_b - pos * plot_h);
                p.circle_filled(c, 2.5 * s, tap_color(i, n_active).linear_multiply(0.25));
            }
        }
    }

    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let x = draw_l + viz.times_l[i].min(1.0) * draw_w;
        let gain_pos = gain_db_to_pos(tap.gain_db.value()).clamp(0.0, 1.0);
        let y = draw_b - gain_pos * plot_h;
        let c = Pos2::new(x, y);
        let dim = if active { 1.0 } else { 0.35 };
        let col = tap_color(i, n_active).linear_multiply(dim);
        let r_vis = 5.0 * s; // fixed — the stem height IS the level

        {
            let hit = if theme::touch() { theme::TOUCH_MIN_HIT } else { 28.0 } * s;
            let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(hit)),
                                   ui.id().with(("v3_stm", i)), Sense::click_and_drag());
            let virt_id = ui.id().with(("v3_stm_virt", i));
            if resp.drag_started() {
                setter.begin_set_parameter(&tap.time_offset);
                setter.begin_set_parameter(&tap.gain_db);
                st.field_sel = Some(i);
                // raw (unsnapped) offset accumulator — see draw_tap_field
                ui.memory_mut(|m| m.data.insert_temp(virt_id, tap.time_offset.value()));
            }
            if resp.dragged() {
                let d = resp.drag_delta();
                let d_off = d.x / draw_w * range / span;
                let mut virt: f32 = ui.memory(|m| m.data.get_temp(virt_id))
                    .unwrap_or_else(|| tap.time_offset.value());
                virt = (virt + d_off).clamp(-0.5, 0.5);
                ui.memory_mut(|m| m.data.insert_temp(virt_id, virt));
                let off = if st.snap3 {
                    snap_offset(viz.times_l[i] * range, tap.time_offset.value(), virt, span)
                } else { virt };
                setter.set_parameter(&tap.time_offset, off.clamp(-0.5, 0.5));
                let new_pos = (gain_pos - d.y / plot_h).clamp(0.0, 1.0);
                setter.set_parameter(&tap.gain_db, pos_to_gain_db(new_pos));
            }
            if resp.drag_stopped() {
                setter.end_set_parameter(&tap.time_offset);
                setter.end_set_parameter(&tap.gain_db);
            }
            if resp.clicked() {
                st.field_sel = if st.field_sel == Some(i) { None } else { Some(i) };
            }
            if resp.double_clicked() {
                setter.begin_set_parameter(&tap.time_offset);
                setter.set_parameter(&tap.time_offset, 0.0);
                setter.end_set_parameter(&tap.time_offset);
                setter.begin_set_parameter(&tap.gain_db);
                setter.set_parameter(&tap.gain_db, 0.0);
                setter.end_set_parameter(&tap.gain_db);
            }
            if resp.hovered() {
                p.circle_stroke(c, r_vis + 5.0 * s, Stroke::new(1.0 * s, theme::with_alpha(theme::INK(), 60)));
            }
        }

        // the stem: dot down to the baseline
        p.line_segment([Pos2::new(x, y + r_vis), Pos2::new(x, draw_b)],
                       Stroke::new(1.5 * s, theme::with_alpha(col, (150.0 * dim) as u8)));
        p.circle_filled(c, r_vis, col);
        if st.field_sel == Some(i) {
            p.circle_stroke(c, r_vis + 3.0 * s, Stroke::new(1.0 * s, theme::INK()));
        }
        p.text(Pos2::new(x, y - r_vis - 4.0 * s), Align2::CENTER_BOTTOM, format!("{:02}", i + 1),
               FontId::new(8.0 * s, FontFamily::Monospace),
               theme::with_alpha(theme::SOFT(), (160.0 * dim) as u8));
    }
}

/// Selected-tap panel — fills the SHAPE card while a tap is selected in
/// GRID/STEMS (owner mockup "Multitap new view", 2026-07-14; the old floating
/// popup occluded dots near the plot's bottom edge). ‹ › step through the
/// active taps (wrap), mute · VOL · PAN · CUT · Q all live here (PAN added so
/// "most controls" are in one place), and TYPE / SLOPE are single
/// click-to-cycle buttons like the FILTER-page blocks. Close (×) restores the
/// SHAPE pads.
fn draw_strip_tap_panel(ui: &mut egui::Ui, card: Rect, params: &Arc<PetalParams>,
                        setter: &ParamSetter, st: &mut EditorState, s: f32) {
    let Some(sel) = st.field_sel else { return; };
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let i = sel.min(n_active - 1);
    let tap = &params.taps[i];
    let p = ui.painter().clone();
    let cy = card.center().y;

    // close × — top-right corner of the card
    let xc = Rect::from_center_size(Pos2::new(card.max.x - 18.0 * s, card.min.y + 16.0 * s),
                                    Vec2::splat(28.0 * s));
    let (x_clicked, xr) = tap_or_click(ui, xc, ui.id().with("v3_sp_x"), true);
    p.text(xc.center(), Align2::CENTER_CENTER, "×",
           FontId::new(15.0 * s, FontFamily::Proportional),
           if xr.hovered() { theme::INK() } else { theme::SOFT() });
    if x_clicked { st.field_sel = None; return; }

    // ‹ › — previous / next active tap (wraps), 44pt targets at the card edges
    // (owner: step through taps here instead of clicking every dot). Painted
    // chevrons, no font dependence.
    let mut step = 0i32;
    for (k, x) in [(0usize, card.min.x + 24.0 * s), (1usize, card.max.x - 24.0 * s)] {
        let cell = Rect::from_center_size(Pos2::new(x, cy), Vec2::splat(44.0 * s));
        let (clicked, resp) = tap_or_click(ui, cell, ui.id().with(("v3_sp_step", k)), true);
        let c = if resp.hovered() { theme::INK() } else { theme::SOFT() };
        let dir = if k == 0 { -1.0f32 } else { 1.0 };
        let (apex, base_x) = (cell.center().x + dir * 3.5 * s, cell.center().x - dir * 3.5 * s);
        p.line_segment([Pos2::new(base_x, cy - 7.0 * s), Pos2::new(apex, cy)], Stroke::new(1.6 * s, c));
        p.line_segment([Pos2::new(apex, cy), Pos2::new(base_x, cy + 7.0 * s)], Stroke::new(1.6 * s, c));
        if clicked { step = if k == 0 { -1 } else { 1 }; }
    }
    if step != 0 {
        st.field_sel = Some((i as i32 + step).rem_euclid(n_active as i32) as usize);
    }

    // centered control row: [## mute] · VOL · PAN · CUT · Q · [TYPE] [SLOPE]
    // (logical units: pill 44 · gap 8 · 4 knob cells @56/pitch 60 · gap 8 ·
    // two 54 buttons — 412 total, inside the 420 between the arrows)
    let (pill_w, kcell, kpitch, btn_w, gap) = (44.0, 56.0, 60.0, 54.0, 8.0);
    let group_w = pill_w + gap + (3.0 * kpitch + kcell) + gap + btn_w + gap + btn_w;
    let x0 = card.center().x - group_w * 0.5 * s;

    // mute pill (tap number)
    let pill = Rect::from_center_size(Pos2::new(x0 + pill_w * 0.5 * s, cy), Vec2::new(pill_w * s, 44.0 * s));
    if v3_button(ui, pill, ("v3_sp_m", i), &format!("{:02}", i + 1), tap.active.value(), true, s) {
        setter.begin_set_parameter(&tap.active);
        setter.set_parameter(&tap.active, !tap.active.value());
        setter.end_set_parameter(&tap.active);
    }

    // knobs — height matched to the label+knob+value stack (≈74) so the
    // top-down child sits visually centered on cy.
    let ch = params.channel_mode.value();
    let mono = ch == ChannelMode::Mono;
    let gain_str = format!("{:+.1} dB", tap.gain_db.value());
    let pan_v = (tap.pan.value() * 100.0).round() as i32;
    // PAN readout follows the channel mode: L/C/R in Stereo, MID/SIDE balance in
    // M/S (matches the PAN-tab slider + the GRID axis).
    let pan_str = match ch {
        ChannelMode::MidSide => {
            if pan_v <= -100 { "MID".to_string() } else if pan_v >= 100 { "SIDE".to_string() }
            else if pan_v == 0 { "M·S".to_string() }
            else if pan_v < 0 { format!("MID {}", -pan_v) } else { format!("SIDE {}", pan_v) }
        }
        _ => if pan_v == 0 { "C".to_string() }
             else if pan_v < 0 { format!("L {}", -pan_v) } else { format!("R {}", pan_v) },
    };
    let cut = tap.filter_cutoff.value();
    let cut_str = if cut >= 1000.0 { format!("{:.1} kHz", cut / 1000.0) } else { format!("{:.0} Hz", cut) };
    let q_str = format!("{:.2}", tap.filter_q.value());
    let knobs: [(&FloatParam, &str, &String); 4] = [
        (&tap.gain_db, "VOL", &gain_str),
        (&tap.pan, "PAN", &pan_str),
        (&tap.filter_cutoff, "CUT", &cut_str),
        (&tap.filter_q, "Q", &q_str),
    ];
    for (j, (param, lab, disp)) in knobs.into_iter().enumerate() {
        let kx = x0 + (pill_w + gap + kcell * 0.5 + j as f32 * kpitch) * s;
        let cell = Rect::from_center_size(Pos2::new(kx, cy), Vec2::new(kcell * s, 74.0 * s));
        // Mono has no panning — show the PAN knob greyed + inert.
        if mono && lab == "PAN" {
            disabled_knob(&p, cell, "PAN", s);
            continue;
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(cell)
            .layout(egui::Layout::top_down(egui::Align::Center)));
        epo::knob_ex(&mut c, setter, param, lab, tk::knob() * 0.85 * s, Accent::Ink, false, None, Some(disp));
    }

    // TYPE + SLOPE — single cycle-buttons (owner: "click and it toggles"),
    // TYPE carries the filter-curve glyph so the mode reads at a glance.
    let bx0 = x0 + (pill_w + gap + 3.0 * kpitch + kcell + gap) * s;
    let type_cell = Rect::from_center_size(Pos2::new(bx0 + btn_w * 0.5 * s, cy), Vec2::new(btn_w * s, 44.0 * s));
    let ft = tap.filter_type.value();
    let (tl, tn) = match ft {
        FilterType::LowPass => ("LP", FilterType::BandPass),
        FilterType::BandPass => ("BP", FilterType::HighPass),
        FilterType::HighPass => ("HP", FilterType::LowPass),
    };
    if v3_button_glyph(ui, type_cell, ("v3_sp_t", 0), tl, Some(ft), false, true, s) {
        set_enum(setter, &tap.filter_type, tn);
    }
    let slope_cell = Rect::from_center_size(
        Pos2::new(bx0 + (btn_w + gap + btn_w * 0.5) * s, cy), Vec2::new(btn_w * s, 44.0 * s));
    let (sl, sn) = match tap.filter_slope.value() {
        FilterSlope::Db12 => ("12 dB", FilterSlope::Db24),
        FilterSlope::Db24 => ("24 dB", FilterSlope::Db48),
        FilterSlope::Db48 => ("48 dB", FilterSlope::Db12),
    };
    if v3_button(ui, slope_cell, ("v3_sp_s", 0), sl, false, true, s) {
        set_enum(setter, &tap.filter_slope, sn);
    }
}

// ─── CENTER bottom — SHAPE L/R pads + link column ──────────────────────────
fn draw_v3_strip(ui: &mut egui::Ui, card: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, s: f32) {
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    // symmetric: margin 27.5 | pad 178 | 12 | link col 81 | 12 | pad 178 | 27.5
    let pad = |x: f32| Rect::from_min_size(
        Pos2::new(card.min.x + x * s, card.min.y + 12.0 * s), Vec2::new(178.0 * s, 168.0 * s));
    let (l_rect, r_rect) = (pad(27.5), pad(310.5));
    let linked = params.shape_linked.value();
    draw_xy_pad(ui, l_rect, "v3_shape_l", "SHAPE L", &params.shape_x_l, &params.shape_y_l, setter, false, n_active, s);
    let (xr, yr) = if linked { (&params.shape_x_l, &params.shape_y_l) } else { (&params.shape_x_r, &params.shape_y_r) };
    draw_xy_pad(ui, r_rect, "v3_shape_r", "SHAPE R", xr, yr, setter, linked, n_active, s);

    // middle column: Link toggle between the pads, centered on the pad midline
    let mid = Rect::from_center_size(
        Pos2::new(card.center().x, l_rect.center().y),
        Vec2::new(81.0 * s, 64.0 * s));
    ui.painter().text(Pos2::new(mid.center().x, mid.min.y), Align2::CENTER_TOP, "Link",
                      FontId::new(10.0 * s, FontFamily::Proportional), theme::FAINT());
    let btn_cell = Rect::from_min_max(Pos2::new(mid.min.x, mid.min.y + 16.0 * s), mid.max);
    v3_toggle(ui, btn_cell, ("v3_shape_link", 0), "ON", &params.shape_linked, setter, true, s);
}

// ─── RIGHT card — PAN / GAIN tap rows (h50 · pitch 50 · scroll) ────────────
fn draw_v3_tap_rows(ui: &mut egui::Ui, region: Rect, params: &Arc<PetalParams>,
                    setter: &ParamSetter, page: RightTab, s: f32) {
    let width = region.width();
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    egui::ScrollArea::vertical()
        .id_salt("v3_tap_rows")
        .max_height(region.height())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0; // pitch = row height (owner: tighter)
            // inactive taps are HIDDEN, not dimmed (owner decision 2026-07-11)
            for i in 0..n_active {
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, 45.0 * s), Sense::hover());
                let dim = draw_tap_pill(ui, row, params, i, n_active, setter, s);
                match page {
                    RightTab::Gain => draw_gain_track(ui, row, params, setter, i, n_active, dim, s),
                    _ => draw_pan_track(ui, row, params, setter, i, n_active, dim, s),
                }
            }
        });
}

/// Tap-number pill (= mute toggle). Painted to the button spec (49×27, r8),
/// hit area = the full leading cell. Returns the row's dim factor.
fn draw_tap_pill(ui: &mut egui::Ui, row: Rect, params: &Arc<PetalParams>,
                 i: usize, n_active: usize, setter: &ParamSetter, s: f32) -> f32 {
    let tap = &params.taps[i];
    let active = tap.active.value();
    let dim = if active { 1.0 } else { 0.45 };
    let p = ui.painter().clone();
    let cell = Rect::from_min_size(row.min, Vec2::new(60.0 * s, row.height()));
    let pill = Rect::from_center_size(cell.center(), Vec2::new(tk::btn_min_w() * s, tk::ctl_h() * s));
    let (m_clicked, resp) = tap_or_click(ui, cell, ui.id().with(("v3_mute", i)), true);
    if m_clicked {
        note_tap_touch(i);
        setter.begin_set_parameter(&tap.active);
        setter.set_parameter(&tap.active, !active);
        setter.end_set_parameter(&tap.active);
    }
    let pc = tap_color(i, n_active).linear_multiply(dim);
    if resp.hovered() {
        p.rect_filled(pill.expand(2.0 * s), (tk::r_ctl() + 2.0) * s, theme::with_alpha(theme::INK(), 10));
    }
    p.rect_stroke(pill, tk::r_ctl() * s, Stroke::new(1.2 * s, if active { pc } else { theme::line2() }),
                  StrokeKind::Inside);
    p.text(pill.center(), Align2::CENTER_CENTER, &format!("{:02}", i + 1),
           FontId::new(10.0 * s, FontFamily::Monospace),
           theme::with_alpha(theme::INK(), (255.0 * dim) as u8));
    dim
}

/// Row track (relative): leading pill cell 60 · value column 48 · track between.
fn row_track(row: Rect, s: f32) -> (Rect, f32) {
    let val_w = 48.0 * s;
    let track = Rect::from_min_max(Pos2::new(row.min.x + 64.0 * s, row.min.y),
                                   Pos2::new(row.max.x - val_w, row.max.y));
    (track, row.max.x - 4.0 * s)
}

fn draw_pan_track(ui: &mut egui::Ui, row: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                  i: usize, n_active: usize, dim: f32, s: f32) {
    let tap = &params.taps[i];
    let p = ui.painter().clone();
    let (track, val_x) = row_track(row, s);
    let resp = ui.interact(track, ui.id().with(("v3_pan", i)), Sense::click_and_drag());
    if resp.drag_started() { note_tap_touch(i); setter.begin_set_parameter(&tap.pan); }
    if resp.dragged() {
        if let Some(px) = resp.interact_pointer_pos() {
            let t = ((px.x - track.min.x) / track.width()).clamp(0.0, 1.0);
            setter.set_parameter(&tap.pan, t * 2.0 - 1.0);
        }
    }
    if resp.drag_stopped() { setter.end_set_parameter(&tap.pan); }
    if resp.double_clicked() {
        setter.begin_set_parameter(&tap.pan);
        setter.set_parameter(&tap.pan, 0.0);
        setter.end_set_parameter(&tap.pan);
    }
    // M/S: the same −1..+1 handle is a MID(left)↔SIDE(right) BALANCE, not a
    // bipolar L/R pan (owner 2026-07-14) — fill from the left edge, no center
    // detent, and the readout says MID/SIDE.
    let ms = params.channel_mode.value() == ChannelMode::MidSide;
    let cy = track.center().y;
    let m = slider_thick(); // hamburger ▸ SLIDERS weight
    let line_c = theme::with_alpha(theme::SOFT(), (150.0 * dim) as u8);
    p.line_segment([Pos2::new(track.min.x, cy), Pos2::new(track.max.x, cy)], Stroke::new(1.2 * m * s, line_c));
    let pan = tap.pan.value();
    let hx = track.min.x + (pan * 0.5 + 0.5) * track.width();
    let pc = tap_color(i, n_active).linear_multiply(dim);
    if ms {
        // fill from the left (MID) edge to the handle
        p.line_segment([Pos2::new(track.min.x, cy), Pos2::new(hx, cy)], Stroke::new(2.5 * m * s, pc));
    } else {
        // bipolar L/R: center detent + fill from center to handle
        p.line_segment([Pos2::new(track.center().x, cy - 6.0 * s), Pos2::new(track.center().x, cy + 6.0 * s)],
                       Stroke::new(1.0 * m * s, line_c));
        if pan.abs() > 0.005 {
            p.line_segment([Pos2::new(track.center().x, cy), Pos2::new(hx, cy)], Stroke::new(2.5 * m * s, pc));
        }
    }
    let hc = if dim >= 1.0 { theme::INK() } else { theme::SOFT() };
    p.rect_filled(Rect::from_center_size(Pos2::new(hx, cy), Vec2::new((4.0 + 1.5 * (m - 1.0)) * s, 16.0 * s)), 2.0 * s,
                  theme::with_alpha(hc, (255.0 * dim) as u8));
    let v = (pan * 100.0).round() as i32;
    let vs = if ms {
        if v <= -100 { "MID".to_string() } else if v >= 100 { "SIDE".to_string() }
        else if v == 0 { "M·S".to_string() }
        else if v < 0 { format!("MID {}", -v) } else { format!("SIDE {}", v) }
    } else if v == 0 { "C".to_string() } else if v < 0 { format!("L {}", -v) } else { format!("R {}", v) };
    p.text(Pos2::new(val_x, cy), Align2::RIGHT_CENTER, &vs,
           FontId::new(10.5 * s, FontFamily::Monospace),
           theme::with_alpha(theme::INK(), (230.0 * dim) as u8));
}

fn draw_gain_track(ui: &mut egui::Ui, row: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                   i: usize, n_active: usize, dim: f32, s: f32) {
    let tap = &params.taps[i];
    let p = ui.painter().clone();
    let (track, val_x) = row_track(row, s);
    let resp = ui.interact(track, ui.id().with(("v3_gain", i)), Sense::click_and_drag());
    if resp.drag_started() { note_tap_touch(i); setter.begin_set_parameter(&tap.gain_db); }
    if resp.dragged() {
        if let Some(px) = resp.interact_pointer_pos() {
            let t = ((px.x - track.min.x) / track.width()).clamp(0.0, 1.0);
            setter.set_parameter(&tap.gain_db, pos_to_gain_db(t));
        }
    }
    if resp.drag_stopped() { setter.end_set_parameter(&tap.gain_db); }
    if resp.double_clicked() {
        setter.begin_set_parameter(&tap.gain_db);
        setter.set_parameter(&tap.gain_db, 0.0);
        setter.end_set_parameter(&tap.gain_db);
    }
    let cy = track.center().y;
    let m = slider_thick(); // hamburger ▸ SLIDERS weight
    let line_c = theme::with_alpha(theme::SOFT(), (150.0 * dim) as u8);
    p.line_segment([Pos2::new(track.min.x, cy), Pos2::new(track.max.x, cy)], Stroke::new(1.2 * m * s, line_c));
    let db = tap.gain_db.value();
    let t = gain_db_to_pos(db).clamp(0.0, 1.0);
    let hx = track.min.x + t * track.width();
    let pc = tap_color(i, n_active).linear_multiply(dim);
    // level fill from the left edge up to the handle
    if t > 0.005 {
        p.line_segment([Pos2::new(track.min.x, cy), Pos2::new(hx, cy)], Stroke::new(2.5 * m * s, pc));
    }
    let hc = if dim >= 1.0 { theme::INK() } else { theme::SOFT() };
    p.rect_filled(Rect::from_center_size(Pos2::new(hx, cy), Vec2::new((4.0 + 1.5 * (m - 1.0)) * s, 16.0 * s)), 2.0 * s,
                  theme::with_alpha(hc, (255.0 * dim) as u8));
    let vs = if db <= -59.95 { "-inf".to_string() } else { format!("{:.1}", db) };
    p.text(Pos2::new(val_x, cy), Align2::RIGHT_CENTER, &vs,
           FontId::new(10.5 * s, FontFamily::Monospace),
           theme::with_alpha(theme::INK(), (230.0 * dim) as u8));
}

// ─── RIGHT card — FILTER page: 2-up squared blocks (iPad-friendly) ─────────
// Each tap gets a block: [## · TYPE · SLOPE] on top, CUT + Q sliders below.
// Two blocks per row so 16 taps become 8 scrolling rows with big touch targets.
fn draw_v3_filter_blocks(ui: &mut egui::Ui, region: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, s: f32) {
    let width = region.width();
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let gap = 10.0 * s;
    let block_w = (width - gap) * 0.5;
    let block_h = 122.0 * s;
    egui::ScrollArea::vertical()
        .id_salt("v3_filter_blocks")
        .max_height(region.height())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0 * s;
            let mut i = 0;
            while i < n_active {
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, block_h), Sense::hover());
                for col in 0..2 {
                    let idx = i + col;
                    if idx >= n_active { break; }
                    let x = row.min.x + col as f32 * (block_w + gap);
                    let brect = Rect::from_min_size(Pos2::new(x, row.min.y), Vec2::new(block_w, block_h));
                    draw_filter_block(ui, brect, params, setter, idx, n_active, s);
                }
                i += 2;
            }
        });
}

fn draw_filter_block(ui: &mut egui::Ui, b: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                     i: usize, n_active: usize, s: f32) {
    use crate::params::{FilterType, FilterSlope};
    let tap = &params.taps[i];
    let active = tap.active.value();
    let dim = if active { 1.0 } else { 0.5 };
    let p = ui.painter().clone();
    p.rect_filled(b, 8.0 * s, theme::INSET());
    p.rect_stroke(b, 8.0 * s, Stroke::new(1.0 * s, theme::line()), StrokeKind::Inside);

    let pad = 8.0 * s;
    let inner_l = b.min.x + pad;
    let inner_r = b.max.x - pad;
    // ── top strip: [## mute] [TYPE] [SLOPE] ──
    let strip_h = 27.0 * s;
    let strip_y = b.min.y + pad;
    let pill_w = 40.0 * s;
    let chip_gap = 6.0 * s;
    let chip_w = (inner_r - inner_l - pill_w - 2.0 * chip_gap) * 0.5;
    // mute pill
    let pill = Rect::from_min_size(Pos2::new(inner_l, strip_y), Vec2::new(pill_w, strip_h));
    let (mf_clicked, _mresp) = tap_or_click(ui, pill, ui.id().with(("v3f_mute", i)), true);
    if mf_clicked {
        note_tap_touch(i);
        setter.begin_set_parameter(&tap.active);
        setter.set_parameter(&tap.active, !active);
        setter.end_set_parameter(&tap.active);
    }
    let pc = tap_color(i, n_active).linear_multiply(dim);
    p.rect_stroke(pill, tk::r_ctl() * s, Stroke::new(1.2 * s, if active { pc } else { theme::line2() }), StrokeKind::Inside);
    p.text(pill.center(), Align2::CENTER_CENTER, &format!("{:02}", i + 1),
           FontId::new(10.0 * s, FontFamily::Monospace), theme::with_alpha(theme::INK(), (255.0 * dim) as u8));
    // type-cycle chip
    let type_cell = Rect::from_min_size(Pos2::new(inner_l + pill_w + chip_gap, strip_y), Vec2::new(chip_w, strip_h));
    let ft = tap.filter_type.value();
    let (tl, tn) = match ft {
        FilterType::LowPass => ("LP", FilterType::BandPass),
        FilterType::BandPass => ("BP", FilterType::HighPass),
        FilterType::HighPass => ("HP", FilterType::LowPass),
    };
    if v3_button_glyph(ui, type_cell, ("v3f_type", i), tl, Some(ft), false, true, s) { note_tap_touch(i); set_enum(setter, &tap.filter_type, tn); }
    // slope-cycle chip
    let slope_cell = Rect::from_min_size(Pos2::new(type_cell.max.x + chip_gap, strip_y), Vec2::new(chip_w, strip_h));
    let (sl, sn) = match tap.filter_slope.value() {
        FilterSlope::Db12 => ("12", FilterSlope::Db24),
        FilterSlope::Db24 => ("24", FilterSlope::Db48),
        FilterSlope::Db48 => ("48", FilterSlope::Db12),
    };
    if v3_button(ui, slope_cell, ("v3f_slope", i), sl, false, true, s) { note_tap_touch(i); set_enum(setter, &tap.filter_slope, sn); }

    // ── CUT + Q knobs (two per block, as in the Figma filter view) ──
    let knob_top = strip_y + strip_h + 6.0 * s;
    let knob_h = b.max.y - knob_top - pad;
    let half = (inner_r - inner_l) * 0.5;
    let acc = Accent::Raw(pc);
    let cut_hz = tap.filter_cutoff.value();
    let cut_str = if cut_hz >= 1000.0 { format!("{:.1}k", cut_hz / 1000.0) } else { format!("{:.0}", cut_hz) };
    let q_str = format!("{:.1}", tap.filter_q.value());
    let mut knob_cell = |cx: f32, param: &FloatParam, label: &str, disp: &str| {
        let cell = Rect::from_min_size(Pos2::new(cx, knob_top), Vec2::new(half, knob_h));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(cell)
            .layout(egui::Layout::top_down(egui::Align::Center)));
        epo::knob_ex(&mut c, setter, param, label, 40.0 * s, acc, false, None, Some(disp));
    };
    knob_cell(inner_l, &tap.filter_cutoff, "CUT", &cut_str);
    knob_cell(inner_l + half, &tap.filter_q, "Q", &q_str);
}
