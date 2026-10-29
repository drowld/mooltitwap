//! Mooltitwap GUI — rebuilt on the shared Epoché UI kit (epoche-ui) so it
//! renders with the same widgets/theme as cumulus. Layout is ui-driven (every
//! epo widget allocates from the egui cursor); the custom delay visualizers
//! are painted into allocated rects using theme:: colors.

use nice_plug::prelude::*;
use nice_plug_egui::{create_egui_editor, EguiState};
use egui::{Align, Align2, Color32, FontFamily, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use epoche_ui::epo::{self, Accent};
use epoche_ui::theme;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::params::{
    ChannelMode, CrossfeedMode, DelayMode, DiffuseRouting, FilterSlope, FilterType, PetalParams,
    SyncMode, NUM_TAPS,
};
// Tap position/time math is shared with the DSP (topology.rs) — no drift.
use crate::topology::{self, compute_positions};
use crate::MAX_DELAY_SECONDS;

/// απόηχος (Afterecho) v3 preview: draw the owner's Figma "Multitap view pan"
/// page instead of the v2 editor. Canvas 1194×834 logical = the 2388×1668 @2x
/// Figma frame. Flip to `false` to get the v2.17 editor back.
pub const V3_PREVIEW: bool = true;

/// The redesigned scope-first page (gui_scope.rs, on the Tide-derived `ui`
/// layer). Flip to `false` to get the v3 editor back while the new page is
/// being built; both old editors go once it matches them.
pub const SCOPE_PAGE: bool = true;

/// Native design width; the zoom factor is window_width / BASE_W (kit contract).
pub const WINDOW_W: u32 = if SCOPE_PAGE { 1366 } else if V3_PREVIEW { 1194 } else { 1080 };
pub const WINDOW_H: u32 = if SCOPE_PAGE { 660 } else if V3_PREVIEW { 834 } else { 660 };
const BASE_W: f32 = WINDOW_W as f32;
/// Shared card body height (logical px, pre-scale) so the three cards align and
/// fill down to the bottom page margin. The right card carries the
/// FEEDBACK/FB-TIME/TAPS footer inside its bottom `RIGHT_FOOTER_H` band (v2.15).
const CARD_BODY_H: f32 = 564.0;
/// v2.15 body-layout metrics (logical px, pre-scale), reproduced on the epo cards.
const LEFT_W: f32 = 224.0;
const RIGHT_W: f32 = 284.0;
const PAGE_M: f32 = 14.0;
const CARD_GAP: f32 = 10.0;
/// Center card: fixed-height shaping strip pinned to the bottom; viz gets the rest.
const SHAPING_H: f32 = 118.0;
/// Right card: footer band height reserved at the bottom for FEEDBACK/FB-TIME/TAPS.
const RIGHT_FOOTER_H: f32 = 70.0;

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
    /// Options (hamburger) menu open.
    pub menu_open: bool,
    /// One-time seed of theme/font from the persisted params.
    pub inited: bool,
    /// Live dark(true)/light(false); `_applied` is the edge-detect twin so the
    /// theme is only re-applied (and persisted) when it actually changes.
    pub theme_dark: bool,
    pub theme_dark_applied: bool,
    /// Dark palette variant: 0 cool · 1 warm graphite · 2 ink (v3). `_applied` is
    /// the edge-detect twin so it's only pushed to the theme when it changes.
    pub dark_variant: u8,
    pub dark_variant_applied: u8,
    /// Font pairing: 0 = Space Grotesk / Spline Mono, 1 = Karla / Inconsolata.
    pub font_face: u8,
    pub font_face_applied: u8,
    /// v3 card/tab title size nudge (px = 10 + steps, clamp 7..48) —
    /// tweakable in the v3 hamburger menu ▸ TAB TEXT. v2.21: desktop too.
    pub title_steps: i32,
    /// v3 PAN/GAIN slider thickness steps (multiplier = 1 + 0.25·steps,
    /// clamp 1..4×) — hamburger menu ▸ SLIDERS.
    pub slider_steps: i32,
    /// Sizing-token sheet: false = the original v3/Figma numbers, true = the
    /// owner's τόπος proposal (tokens.rs) — hamburger menu ▸ SIZING, for
    /// live side-by-side comparison.
    pub sizing_beta: bool,
    /// FIELD view: the tap whose GAIN/FILTER panel is open (None = closed).
    pub field_sel: Option<usize>,
    /// v3 center-card view tab: 0 = GRID (time×pan), 1 = STEMS (time×volume),
    /// 2 = ECHO (classic read-only bars, full feedback tail). `viz_mode`
    /// stays for the legacy v2 editor.
    pub view3: u8,
    /// GRID/STEMS: snap dragged tap times to multiples of the base time
    /// (quantize, owner idea 2026-07-13) — the SNAP chip in the view toggles it.
    pub snap3: bool,
    /// High-contrast accessibility mode (hamburger ▸ CONTRAST). Session-state,
    /// pushed to the theme each frame — same idiom as `sizing_beta`.
    pub high_contrast: bool,
    /// Window resize-drag accumulator: (start_w, start_h, acc_dx) + throttle cooldown.
    pub drag_resize: Option<(f32, f32, f32)>,
    pub drag_cool: u8,
    /// Preset library (lazily loaded on first use), the currently-loaded preset
    /// name, the browser-modal open flag, and the save-as name buffer.
    pub library: Option<crate::presets::Library>,
    pub current_preset: Option<String>,
    pub browser_open: bool,
    /// Frames to ignore backdrop clicks right after opening (swallows the
    /// opening click so the browser doesn't instantly dismiss).
    pub browser_guard: u8,
    pub save_name: String,
    /// Drag-on-number state: (value at drag start, accumulated px).
    pub taps_drag: Option<(i32, f32)>,
    pub fb_drag: Option<(f32, f32)>,
    /// A/B compare: two full param snapshots + which is live (0=A, 1=B). Lets
    /// the owner toggle between two variants. Lazily seeded from live params.
    pub ab_slots: [Option<std::collections::BTreeMap<String, f32>>; 2],
    pub ab_active: u8,
    // ── v3 spotlight preset palette (ported from y-flanger) ──
    /// Values last loaded/saved — the dirty `*` = current_preset & !matches(snap).
    pub loaded_snapshot: Option<std::collections::BTreeMap<String, f32>>,
    pub preset_open: bool,      // palette popup visible
    pub preset_search: String,  // filter query
    pub preset_guard: u8,       // ignore backdrop clicks for N frames after open
    pub palette_top: f32,       // y the palette drops from
    pub palette_sel: usize,     // highlighted row
    pub palette_scope: u8,      // 0 all · 1 favorites · 2 recent
    pub palette_focus: bool,    // one-shot: focus the search field
    pub recent: Vec<String>,    // session MRU (most-recent first, capped)
    pub palette_name_open: bool, // save-as / rename inline editor active
    pub palette_name_buf: String,
    pub palette_name_focus: bool,
    pub palette_rename_from: Option<String>, // Some ⇒ rename, None ⇒ save-as
    pub palette_del_confirm: Option<String>, // two-click delete arm
    pub palette_tag_for: Option<String>,     // row whose add-tag field is open
    pub palette_tag_buf: String,
    pub palette_tag_focus: bool,
    /// The scope-first page's own state (gui_scope.rs).
    pub scope: crate::gui_scope::ScopeState,
}
impl Default for EditorState {
    fn default() -> Self {
        Self {
            // v2.23: FIELD (the editable time×pan plane, VizMode::Grid) is the
            // default center view in v3; STEMS is one chip away.
            viz_mode: VizMode::Grid, right_tab: RightTab::Gain, drag_tap: None,
            field_sel: None,
            view3: 0,
            snap3: false,
            high_contrast: false,
            menu_open: false, inited: false,
            theme_dark: true, theme_dark_applied: true,
            dark_variant: 1, dark_variant_applied: 255, // 1 = warm graphite; 255 forces first apply
            font_face: 0, font_face_applied: 0,
            title_steps: 2, // 12px tab titles (cumulus default)
            slider_steps: 2, // thicker PAN/GAIN tracks by default (1.5×) — owner 2026-07-14

            sizing_beta: false,
            drag_resize: None, drag_cool: 0,
            library: None, current_preset: None, browser_open: false, browser_guard: 0,
            save_name: String::new(), taps_drag: None, fb_drag: None,
            ab_slots: [None, None], ab_active: 0,
            loaded_snapshot: None, preset_open: false, preset_search: String::new(),
            preset_guard: 0, palette_top: 0.0, palette_sel: 0, palette_scope: 0,
            palette_focus: false, recent: Vec::new(), palette_name_open: false,
            palette_name_buf: String::new(), palette_name_focus: false,
            palette_rename_from: None, palette_del_confirm: None,
            palette_tag_for: None, palette_tag_buf: String::new(), palette_tag_focus: false,
            scope: Default::default(),
        }
    }
}

// ─── Audio-taper gain mapping (cube-root: 75% pos = 0 dB) ────────────────
const GAIN_SKEW: f32 = 0.333;
#[inline]
pub(crate) fn gain_db_to_pos(db: f32) -> f32 {
    ((db + 60.0) / 66.0).clamp(0.0, 1.0).powf(1.0 / GAIN_SKEW)
}
#[inline]
pub(crate) fn pos_to_gain_db(pos: f32) -> f32 {
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
pub(crate) fn tap_color(i: usize, n_active: usize) -> Color32 {
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
            // On the iPad AUv3 (v3 editor) run in touch mode: enables raw-touch
            // knobs, 44pt hit floors, and the title_size control (title_fs only
            // honors set_title_size on touch). Desktop stays pointer-mode.
            theme::set_touch(cfg!(target_os = "ios"));
            crate::ui::theme::set_touch(cfg!(target_os = "ios"));
            if SCOPE_PAGE {
                crate::ui::theme::install_fonts(ctx);
            } else {
                theme::install_fonts(ctx);
            }
        },
        move |ui, setter, _queue, st| {
            if SCOPE_PAGE {
                let tempo = read_tempo(&tempo_bpm_shared);
                crate::gui_scope::frame(ui, &params, setter, st, &egui_state_size, tempo);
                return;
            }
            // Seed theme from the persisted param once, forcing a single apply.
            if !st.inited {
                st.inited = true;
                // v3 seeds from the persisted theme; dark = warm graphite variant.
                st.theme_dark = params.theme_dark.load(Ordering::Relaxed);
                st.theme_dark_applied = !st.theme_dark;
            }
            // Apply-if-changed (before apply_zoom so the palette is correct this
            // frame) and write the choice back to the persisted param.
            if st.theme_dark != st.theme_dark_applied {
                theme::set_theme(st.theme_dark);
                params.theme_dark.store(st.theme_dark, Ordering::Relaxed);
                st.theme_dark_applied = st.theme_dark;
            }
            if st.dark_variant != st.dark_variant_applied {
                theme::set_dark_variant(st.dark_variant);
                st.dark_variant_applied = st.dark_variant;
            }
            if st.font_face != st.font_face_applied {
                let choice = if st.font_face == 1 { theme::FontChoice::KarlaInconsolata }
                             else { theme::FontChoice::GroteskMono };
                theme::apply_fonts(ui.ctx(), choice);
                st.font_face_applied = st.font_face;
            }
            // Window resize (aspect-locked to the design ratio) is driven by a
            // corner grip inside draw_plugin; only snap height here when the
            // width changed without a drag in progress.
            let (cur_w, cur_h) = egui_state_size.size();
            if st.drag_resize.is_none() {
                let want_h = (cur_w as f32 * WINDOW_H as f32 / WINDOW_W as f32).round() as u32;
                if cur_h.abs_diff(want_h) > 2 {
                    egui_state_size.set_requested_size((cur_w, want_h));
                }
            }
            theme::apply_zoom(ui.ctx(), (cur_w as f32 / BASE_W).clamp(0.5, 2.5));
            let tempo = read_tempo(&tempo_bpm_shared);
            if V3_PREVIEW {
                crate::gui_v3::draw(ui, &params, setter, st, tempo);
            } else {
                draw_plugin(ui, &params, setter, st, &egui_state_size, tempo);
            }
        },
    )
}

// ─── Top-level layout ──────────────────────────────────────────────────────
fn draw_plugin(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    egui_state: &EguiState,
    tempo: f32,
) {
    let s = theme::ui_scale(ui);
    ui.painter().rect_filled(ui.max_rect(), 0.0, theme::PAPER());

    let margin = PAGE_M * s;
    draw_header(ui, params, setter, st, s);
    ui.add_space(theme::SP_MD * s);

    let avail = ui.available_width() - 2.0 * margin;
    let gap = CARD_GAP * s;
    let left_w = LEFT_W * s;
    let right_w = RIGHT_W * s;
    let center_w = (avail - left_w - right_w - 2.0 * gap).max(200.0 * s);
    let body_h = CARD_BODY_H * s;

    // Read mutable state out of `st` into locals so each card body closure
    // captures only what it needs (avoids nested &mut st borrows). The footer
    // now lives inside the right card, so its drag accumulators travel too.
    let mut viz_mode = st.viz_mode;
    let mut right_tab = st.right_tab;
    let mut drag_tap = st.drag_tap;
    let mut taps_drag = st.taps_drag;
    let mut fb_drag = st.fb_drag;

    ui.horizontal_top(|ui| {
        ui.add_space(margin);
        ui.spacing_mut().item_spacing.x = gap;

        epo::card(ui, "DELAY", theme::INK2(), left_w, &mut |ui| {
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
            &mut |ui| draw_right_body(ui, params, setter, right_tab, tempo, &mut taps_drag, &mut fb_drag),
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
    st.taps_drag = taps_drag;
    st.fb_drag = fb_drag;

    draw_menu(ui, st, margin, s);
    draw_browser(ui, params, setter, st, s);
    draw_resize_grip(ui, st, egui_state, s);
}

// ─── Preset helpers ────────────────────────────────────────────────────────
fn ensure_library(st: &mut EditorState) {
    if st.library.is_none() {
        st.library = Some(crate::presets::Library::load());
    }
}
fn preset_load(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter, name: &str) {
    ensure_library(st);
    let snap = st.library.as_ref()
        .and_then(|l| l.get(name))
        .map(|p| (p.values.clone(), p.values_format));
    if let Some((values, fmt)) = snap {
        crate::presets::apply(params, setter, &values, fmt);
        st.current_preset = Some(name.to_string());
    }
}
fn preset_step(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter, delta: i32) {
    ensure_library(st);
    let names = st.library.as_ref().map(|l| l.names()).unwrap_or_default();
    if names.is_empty() { return; }
    let cur = st.current_preset.as_ref()
        .and_then(|c| names.iter().position(|n| n == c))
        .unwrap_or(0);
    let next = (cur as i32 + delta).rem_euclid(names.len() as i32) as usize;
    let name = names[next].clone();
    preset_load(st, params, setter, &name);
}
fn preset_save(st: &mut EditorState, params: &Arc<PetalParams>, name: &str) {
    ensure_library(st);
    if let Some(lib) = st.library.as_mut() {
        if lib.save_as(name, params, env!("CARGO_PKG_VERSION")) {
            st.current_preset = Some(name.to_string());
        }
    }
}

// ─── Preset browser modal — cumulus command-palette pattern: ONE Area at
// Foreground, backdrop + panel click targets allocated inside it, scrim and
// panel hand-painted, and a guard that swallows the opening click. ─────────
fn draw_browser(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState, s: f32) {
    if !st.browser_open { return; }
    let screen = ui.ctx().screen_rect();
    let names = st.library.as_ref().map(|l| l.names()).unwrap_or_default();
    let cur = st.current_preset.clone();
    let mut do_load: Option<String> = None;
    let mut do_save = false;
    let mut do_close = false;

    let w = 460.0 * s;
    let row_h = 30.0 * s;
    let list_h = row_h * (names.len().clamp(1, 9) as f32);
    let head_h = 34.0 * s;
    let foot_h = 44.0 * s;
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - w * 0.5, screen.min.y + 72.0 * s),
        Vec2::new(w, head_h + list_h + foot_h + theme::SP_MD * s * 2.0),
    );
    let r12 = 12.0 * s;

    egui::Area::new(egui::Id::new("mtw_browser"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ui.ctx(), |ui| {
            let backdrop = ui.allocate_rect(screen, Sense::click());
            let block = ui.allocate_rect(panel, Sense::click());
            let p = ui.painter();
            p.rect_filled(screen, 0.0, theme::scrim());
            p.rect_filled(panel.translate(Vec2::new(0.0, 10.0 * s)).expand(2.0 * s), r12,
                          Color32::from_black_alpha(120));
            p.rect_filled(panel, r12, theme::FRAME());
            p.rect_stroke(panel, r12, Stroke::new(1.0 * s, theme::line2()), StrokeKind::Inside);

            // header
            p.text(Pos2::new(panel.min.x + theme::SP_MD * s, panel.min.y + head_h * 0.5),
                   Align2::LEFT_CENTER, "PRESETS",
                   FontId::new(theme::FS_HEADING * s, FontFamily::Proportional), theme::INK());
            p.text(Pos2::new(panel.max.x - theme::SP_MD * s, panel.min.y + head_h * 0.5),
                   Align2::RIGHT_CENTER, "click outside to close",
                   FontId::new(8.5 * s, FontFamily::Monospace), theme::FAINT());
            p.line_segment([Pos2::new(panel.min.x, panel.min.y + head_h),
                            Pos2::new(panel.max.x, panel.min.y + head_h)],
                           Stroke::new(1.0 * s, theme::line()));

            // preset rows (hand-painted; hover wash + accent name on select)
            if names.is_empty() {
                p.text(Pos2::new(panel.center().x, panel.min.y + head_h + row_h * 0.5),
                       Align2::CENTER_CENTER, "no presets yet — type a name below and SAVE",
                       FontId::new(9.0 * s, FontFamily::Monospace), theme::SOFT());
            }
            for (i, n) in names.iter().take(9).enumerate() {
                let rr = Rect::from_min_size(
                    Pos2::new(panel.min.x, panel.min.y + head_h + i as f32 * row_h),
                    Vec2::new(w, row_h),
                );
                let resp = ui.interact(rr, ui.id().with(("preset_row", i)), Sense::click());
                let sel = cur.as_deref() == Some(n.as_str());
                if resp.hovered() { p.rect_filled(rr.shrink2(Vec2::new(4.0 * s, 1.0 * s)), 6.0 * s, theme::sel_bg()); }
                let col = if sel { theme::G() } else if resp.hovered() { theme::INK() } else { theme::INK2() };
                p.text(Pos2::new(rr.min.x + theme::SP_MD * s, rr.center().y), Align2::LEFT_CENTER,
                       n, FontId::new(11.0 * s, FontFamily::Proportional), col);
                if sel {
                    p.text(Pos2::new(rr.max.x - theme::SP_MD * s, rr.center().y), Align2::RIGHT_CENTER,
                           "●", FontId::new(8.0 * s, FontFamily::Proportional), theme::G());
                }
                if resp.clicked() { do_load = Some(n.clone()); }
            }

            // footer: save-as field + SAVE chip (laid out as child ui over the footer band)
            let foot = Rect::from_min_size(
                Pos2::new(panel.min.x + theme::SP_MD * s, panel.max.y - foot_h - theme::SP_SM * s),
                Vec2::new(w - theme::SP_MD * s * 2.0, foot_h),
            );
            p.line_segment([Pos2::new(panel.min.x, foot.min.y), Pos2::new(panel.max.x, foot.min.y)],
                           Stroke::new(1.0 * s, theme::line()));
            let mut fui = ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(Vec2::new(0.0, 6.0 * s))));
            fui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                ui.add(egui::TextEdit::singleline(&mut st.save_name)
                    .hint_text("save as…")
                    .desired_width(w * 0.55));
                if epo::chip(ui, "SAVE", false) { do_save = true; }
            });

            // click-outside dismiss, guarded against the opening click
            if st.browser_guard > 0 {
                st.browser_guard -= 1;
            } else if backdrop.clicked() && !block.clicked() {
                do_close = true;
            }
        });

    if let Some(n) = do_load { preset_load(st, params, setter, &n); st.browser_open = false; }
    if do_save {
        let n = st.save_name.trim().to_string();
        if !n.is_empty() { preset_save(st, params, &n); st.save_name.clear(); }
    }
    if do_close { st.browser_open = false; }
}

// ─── Options menu (hamburger) — egui Area anchored under the header ────────
fn draw_menu(ui: &mut egui::Ui, st: &mut EditorState, margin: f32, s: f32) {
    if !st.menu_open { return; }
    let anchor = ui.ctx().screen_rect().min + Vec2::new(margin, 44.0 * s);
    egui::Area::new(egui::Id::new("mtw_options_menu"))
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
                    ui.label(RichText::new("OPTIONS").size(theme::FS_LABEL * s).color(theme::SOFT()).strong());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("THEME").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        if epo::chip(ui, if st.theme_dark { "DARK" } else { "LIGHT" }, true) {
                            st.theme_dark = !st.theme_dark;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("FONT").size(theme::FS_LABEL * s).color(theme::FAINT()));
                        if epo::chip(ui, "GROTESK", st.font_face == 0) { st.font_face = 0; }
                        if epo::chip(ui, "KARLA", st.font_face == 1) { st.font_face = 1; }
                    });
                    ui.add_space(theme::SP_XS * s);
                    if epo::chip(ui, "✕ CLOSE", false) { st.menu_open = false; }
                });
        });
}

// ─── Resize grip — bottom-right, aspect-locked to the design ratio ─────────
fn draw_resize_grip(ui: &mut egui::Ui, st: &mut EditorState, egui_state: &EguiState, s: f32) {
    let screen = ui.ctx().screen_rect();
    let grip = 22.0 * s;
    egui::Area::new(egui::Id::new("mtw_resize_grip"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.max.x - grip, screen.max.y - grip))
        .show(ui.ctx(), |aui| {
            let (rect, resp) = aui.allocate_exact_size(Vec2::new(grip, grip), Sense::drag());
            let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            let col = if resp.hovered() || resp.dragged() { theme::INK2() } else { theme::FAINT() };
            for i in 1..=3 {
                let off = grip * (i as f32) / 4.0;
                aui.painter().line_segment(
                    [egui::pos2(rect.max.x - off, rect.max.y - 2.0 * s), egui::pos2(rect.max.x - 2.0 * s, rect.max.y - off)],
                    Stroke::new(1.4 * s, col),
                );
            }
            if resp.drag_started() {
                let (w, h) = egui_state.size();
                st.drag_resize = Some((w as f32, h as f32, 0.0));
            }
            if let Some((w0, h0, acc0)) = st.drag_resize {
                let acc = acc0 + resp.drag_delta().x;
                st.drag_resize = Some((w0, h0, acc));
                st.drag_cool = st.drag_cool.saturating_sub(1);
                let new_w = (w0 + acc).clamp(BASE_W * 0.6, BASE_W * 2.2).round();
                let new_h = (new_w * WINDOW_H as f32 / WINDOW_W as f32).round(); // aspect lock
                let cur_w = egui_state.size().0;
                if resp.drag_stopped() {
                    egui_state.set_requested_size((new_w as u32, new_h as u32));
                    st.drag_resize = None;
                } else if st.drag_cool == 0 && (new_w as i64 - cur_w as i64).abs() >= 8 {
                    egui_state.set_requested_size((new_w as u32, new_h as u32));
                    st.drag_cool = 3;
                }
            }
        });
}

// ─── Header ──────────────────────────────────────────────────────────────
fn draw_header(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState, s: f32) {
    let mut open_browser = false;
    let mut step = 0i32;
    ui.add_space(theme::SP_SM * s);
    ui.horizontal(|ui| {
        ui.add_space(theme::SP_LG * s);
        ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
        if epo::menu_button(ui) { st.menu_open = !st.menu_open; }
        ui.label(RichText::new("mooltitwap").size(19.0 * s).color(theme::INK()).strong());
        ui.label(RichText::new("·").size(19.0 * s).color(theme::accent(2)));
        ui.label(
            RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                .family(FontFamily::Monospace).size(11.0 * s).color(theme::SOFT()),
        );
        // centered preset pill: ◀ prev / name → browser / ▶ next.
        // Center on the true header midpoint (max_rect center), not on the
        // leftover space right of the wordmark cluster; subtract the trailing
        // item_spacing that precedes the pill.
        let pill_w = theme::PILL_MIN_W * s;
        let pad = (ui.max_rect().center().x - pill_w * 0.5 - ui.cursor().left() - theme::SP_SM * s).max(0.0);
        ui.add_space(pad);
        let label = st.current_preset.clone().unwrap_or_else(|| "INIT".to_string());
        match epo::preset_slot(ui, &label, theme::PILL_MIN_W, theme::TBBTN_H, "preset") {
            2 => open_browser = true,
            d if d != 0 => step = d,
            _ => {}
        }
    });
    if open_browser {
        st.browser_open = !st.browser_open;
        if st.browser_open {
            ensure_library(st);
            st.browser_guard = 3; // swallow the opening click
        }
    }
    if step != 0 { preset_step(st, params, setter, step); }
    ui.add_space(theme::SP_XS * s);
    let r = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [Pos2::new(r.left(), y), Pos2::new(r.right(), y)],
        Stroke::new(1.0 * s, theme::line()),
    );
}

// ─── LEFT card body (v2.15 geometry): 88px hero band + fixed-rhythm sections ──
fn draw_left_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, tempo: f32) {
    let s = theme::ui_scale(ui);
    let kd = theme::KNOB_DIAM_FX;
    let hero_kd = theme::KNOB_DIAM_INSPECT; // hero knobs larger than section knobs
    let (body, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), CARD_BODY_H * s), Sense::hover());

    // ── Hero band (88px): TIME L / LINK / TIME R as three centered columns ──
    let hero = Rect::from_min_size(body.min, Vec2::new(body.width(), 88.0 * s));
    let l_str = params.format_base_time(params.base_time_l.value(), tempo);
    let r_val = if params.time_linked.value() { params.base_time_l.value() } else { params.base_time_r.value() };
    let r_str = params.format_base_time(r_val, tempo);
    let hthird = hero.width() / 3.0;
    let hcol = |i: f32| Rect::from_min_size(Pos2::new(hero.min.x + i * hthird, hero.min.y + 8.0 * s),
                                            Vec2::new(hthird, hero.height() - 8.0 * s));
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(hcol(0.0)).layout(Layout::top_down(Align::Center)));
        epo::knob_ex(&mut c, setter, &params.base_time_l, "TIME L", hero_kd, Accent::Ink, false, None, Some(&l_str));
    }
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(hcol(1.0)).layout(Layout::top_down(Align::Center)));
        c.add_space(30.0 * s);
        epo::toggle(&mut c, "LINK", &params.time_linked, setter);
    }
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(hcol(2.0)).layout(Layout::top_down(Align::Center)));
        if params.time_linked.value() {
            epo::knob_ex(&mut c, setter, &params.base_time_l, "TIME R", hero_kd, Accent::Ink, false, None, Some(&r_str));
        } else {
            epo::knob_ex(&mut c, setter, &params.base_time_r, "TIME R", hero_kd, Accent::Ink, false, None, Some(&r_str));
        }
    }
    ui.painter().line_segment([Pos2::new(body.min.x, hero.max.y), Pos2::new(body.max.x, hero.max.y)],
                              Stroke::new(1.0 * s, theme::line()));

    // ── Sections (fixed rhythm + hairline dividers, like v2.15 `section`) ──
    let mut y = hero.max.y;
    let mut chip_row = |ui: &mut egui::Ui, br: Rect, s: f32, f: &mut dyn FnMut(&mut egui::Ui)| {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(br).layout(Layout::left_to_right(Align::Center)));
        c.spacing_mut().item_spacing.x = theme::SP_TIGHT * s;
        f(&mut c);
    };

    let (br, ny) = left_section(ui, body, y, "SYNC", 30.0 * s, s); y = ny;
    chip_row(ui, br, s, &mut |c| {
        let cur = params.sync_mode.value();
        if epo::chip(c, "FREE", cur == SyncMode::Free) { set_enum(setter, &params.sync_mode, SyncMode::Free); }
        if epo::chip(c, "SYNC", cur == SyncMode::Sync) { set_enum(setter, &params.sync_mode, SyncMode::Sync); }
    });

    let (br, ny) = left_section(ui, body, y, "DELAY MODE", 30.0 * s, s); y = ny;
    chip_row(ui, br, s, &mut |c| {
        let cur = params.delay_mode.value();
        for (lab, m) in [("REPITCH", DelayMode::Repitch), ("FADE", DelayMode::Fade), ("JUMP", DelayMode::Jump)] {
            if epo::chip(c, lab, cur == m) { set_enum(setter, &params.delay_mode, m); }
        }
    });

    let (br, ny) = left_section(ui, body, y, "CHANNEL", 30.0 * s, s); y = ny;
    chip_row(ui, br, s, &mut |c| {
        let cur = params.channel_mode.value();
        for (lab, m) in [("STEREO", ChannelMode::Stereo), ("M/S", ChannelMode::MidSide), ("MONO", ChannelMode::Mono)] {
            if epo::chip(c, lab, cur == m) { set_enum(setter, &params.channel_mode, m); }
        }
    });

    // LEVELS — IN / MIX / OUT knobs, one per column
    let mix_str = format!("{:.0}%", params.dry_wet.value() * 100.0);
    let (br, ny) = left_section(ui, body, y, "LEVELS", 76.0 * s, s); y = ny;
    knob_col(ui, br, 0, 3, |c| { epo::knob(c, setter, &params.input_gain, "IN", kd, Accent::Ink, false); });
    knob_col(ui, br, 1, 3, |c| { epo::knob_ex(c, setter, &params.dry_wet, "MIX", kd, Accent::Ink, false, None, Some(&mix_str)); });
    knob_col(ui, br, 2, 3, |c| { epo::knob(c, setter, &params.output_gain, "OUT", kd, Accent::Ink, false); });

    // SATURATION — SOFT CLIP toggle
    let (br, _ny) = left_section(ui, body, y, "SATURATION", 30.0 * s, s);
    chip_row(ui, br, s, &mut |c| epo::toggle(c, "SOFT CLIP", &params.soft_clip, setter));
}

/// v2.15 `section` rhythm: title at (x+pad, y+10), a body rect of `body_h`, then
/// a hairline divider. Returns the body rect and the new running y.
fn left_section(ui: &mut egui::Ui, body: Rect, y: f32, title: &str, body_h: f32, s: f32) -> (Rect, f32) {
    let pad_top = 10.0 * s;
    let title_h = 16.0 * s;
    let pad_x = 4.0 * s;
    ui.painter().text(Pos2::new(body.min.x + pad_x, y + pad_top), Align2::LEFT_TOP, title,
                      FontId::new(theme::FS_LABEL * s, FontFamily::Proportional), theme::FAINT());
    let br = Rect::from_min_max(Pos2::new(body.min.x + pad_x, y + pad_top + title_h),
                               Pos2::new(body.max.x - pad_x, y + pad_top + title_h + body_h));
    let line_y = y + pad_top + title_h + body_h + 8.0 * s;
    ui.painter().line_segment([Pos2::new(body.min.x, line_y), Pos2::new(body.max.x, line_y)],
                              Stroke::new(1.0 * s, theme::line()));
    (br, line_y)
}

/// Place one knob centered in column `i` of `n` within `br`.
fn knob_col(ui: &mut egui::Ui, br: Rect, i: usize, n: usize, f: impl FnOnce(&mut egui::Ui)) {
    let cw = br.width() / n as f32;
    let r = Rect::from_min_size(Pos2::new(br.min.x + i as f32 * cw, br.min.y), Vec2::new(cw, br.height()));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::top_down(Align::Center)));
    f(&mut c);
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
    // Allocate the full shared body height so this card bottom-aligns with the
    // others; the shaping strip is a fixed SHAPING_H band pinned to the bottom,
    // and the viz well takes the rest (v2.15 split — was inverted in the port).
    let (body, _) = ui.allocate_exact_size(Vec2::new(w, CARD_BODY_H * s), Sense::hover());
    let rect = Rect::from_min_max(
        Pos2::new(body.min.x + 8.0 * s, body.min.y + 2.0 * s),
        Pos2::new(body.max.x - 8.0 * s, body.max.y - SHAPING_H * s - 8.0 * s));
    let p = ui.painter().clone();
    // kit `.viz` well: INSET fill + strong hairline border.
    p.rect_filled(rect, 13.0 * s, theme::INSET());
    p.rect_stroke(rect, 13.0 * s, Stroke::new(1.0 * s, theme::line2()), StrokeKind::Inside);

    // corner caption
    let ch_label = match params.channel_mode.value() {
        ChannelMode::Stereo => "STEREO", ChannelMode::MidSide => "MID/SIDE", ChannelMode::Mono => "MONO",
    };
    // Caption shows the range of the axis actually drawn in this view.
    let shown_range = match viz_mode {
        VizMode::Stems => viz.grid_range_secs,
        VizMode::Grid => viz.grid_range_secs_taps_only,
    };
    let len_str = match params.sync_mode.value() {
        SyncMode::Sync => {
            // Musical length, not raw beats-as-"N/1" (which read like bars).
            // Round away fp error so integer beat counts format as integers.
            let beats = (shown_range * tempo / 60.0 * 1000.0).round() / 1000.0;
            if beats >= 4.0 && (beats / 4.0).fract() == 0.0 {
                format!("{:.0} BARS", beats / 4.0)
            } else if beats.fract() == 0.0 {
                format!("{:.0} BEATS", beats)
            } else {
                format!("{:.1} BEATS", beats)
            }
        }
        SyncMode::Free => {
            if shown_range < 1.0 { format!("{:.0} ms", shown_range * 1000.0) }
            else { format!("{:.2} s", shown_range) }
        }
    };
    p.text(Pos2::new(rect.max.x - 13.0 * s, rect.min.y + 9.0 * s), Align2::RIGHT_TOP,
           &format!("{} · TAPS {} · {}", ch_label, n_active, len_str),
           FontId::new(9.0 * s, FontFamily::Monospace), theme::SOFT());

    match viz_mode {
        VizMode::Stems => draw_stems(&p, rect, params, &viz, tempo, is_sync, s),
        VizMode::Grid => draw_grid_viz(ui, rect, params, setter, &viz, tempo, is_sync, drag_tap, s),
    }

    // ── Shaping strip (fixed SHAPING_H band pinned to the bottom): SHAPE L pad ·
    //    SHAPE R pad · 56px link column, matching v2.15 draw_shaping_strip. ──────
    let strip = Rect::from_min_max(Pos2::new(body.min.x, body.max.y - SHAPING_H * s), body.max);
    ui.painter().line_segment([Pos2::new(strip.min.x, strip.min.y), Pos2::new(strip.max.x, strip.min.y)],
                              Stroke::new(1.0 * s, theme::line()));
    let link_w = 56.0 * s;
    let pad_in = 8.0 * s;
    let pad_avail = (strip.width() - link_w) * 0.5;
    let l_rect = Rect::from_min_size(Pos2::new(strip.min.x + pad_in, strip.min.y + pad_in),
                                     Vec2::new(pad_avail - pad_in * 2.0, strip.height() - pad_in * 2.0));
    let r_rect = Rect::from_min_size(Pos2::new(strip.min.x + pad_avail + pad_in, strip.min.y + pad_in),
                                     Vec2::new(pad_avail - pad_in * 2.0, strip.height() - pad_in * 2.0));
    let link_rect = Rect::from_min_max(Pos2::new(strip.max.x - link_w, strip.min.y), strip.max);
    let linked = params.shape_linked.value();
    draw_xy_pad(ui, l_rect, "shape_l", "SHAPE L", &params.shape_x_l, &params.shape_y_l, setter, false, n_active, s);
    let (xr, yr) = if linked { (&params.shape_x_l, &params.shape_y_l) } else { (&params.shape_x_r, &params.shape_y_r) };
    draw_xy_pad(ui, r_rect, "shape_r", "SHAPE R", xr, yr, setter, linked, n_active, s);
    draw_shape_link(ui, link_rect, &params.shape_linked, setter, linked, s);
}

/// SHAPE-link toggle column at the right of the shaping strip: a "SHAPE"
/// caption, a chip-styled LINK button, and a LINKED/UNLINK state sublabel.
/// Toggles `shape_linked` so SHAPE R mirrors SHAPE L (parity with the pre-port
/// editor). Hand-painted because it shares the manual-rect strip with the pads;
/// the chip styling mirrors the kit's neutral button (as `epo::toggle` "LINK").
fn draw_shape_link(ui: &mut egui::Ui, rect: Rect, param: &BoolParam, setter: &ParamSetter, linked: bool, s: f32) {
    let btn_w = (rect.width() - 6.0 * s).min(40.0 * s);
    let btn = Rect::from_center_size(rect.center(), Vec2::new(btn_w, theme::BTN_H * s));
    let resp = ui.interact(btn, ui.id().with("shape_link"), Sense::click());
    if resp.clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !linked);
        setter.end_set_parameter(param);
    }
    let (bg, stroke, fg) = if linked {
        (theme::with_alpha(theme::INK(), 28), theme::INK(), theme::INK())
    } else if resp.hovered() {
        (theme::INSET(), theme::with_alpha(theme::INK(), 90), theme::INK())
    } else {
        (theme::INSET(), theme::line2(), theme::INK2())
    };
    let p = ui.painter();
    // column separator, mirroring the pre-port divider
    p.line_segment([Pos2::new(rect.min.x, rect.min.y + 4.0 * s), Pos2::new(rect.min.x, rect.max.y - 4.0 * s)],
                   Stroke::new(1.0 * s, theme::line()));
    p.text(Pos2::new(rect.center().x, rect.min.y + 12.0 * s), Align2::CENTER_CENTER, "SHAPE",
           FontId::new(theme::FS_LABEL * s, FontFamily::Proportional), theme::FAINT());
    p.rect_filled(btn, theme::BTN_RADIUS * s, bg);
    p.rect_stroke(btn, theme::BTN_RADIUS * s, Stroke::new(1.0 * s, stroke), StrokeKind::Inside);
    p.text(btn.center(), Align2::CENTER_CENTER, "LINK",
           FontId::new((theme::BTN_FS - 2.0) * s, FontFamily::Proportional), fg);
    p.text(Pos2::new(rect.center().x, rect.max.y - 10.0 * s), Align2::CENTER_CENTER,
           if linked { "LINKED" } else { "UNLINK" },
           FontId::new(theme::FS_KNOB * s, FontFamily::Proportional), theme::FAINT());
}

// ─── XY shaping pad (rect-driven; interacts via ui.interact so it composes
// inside an already-allocated strip). ──────────────────────────────────────
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_xy_pad(ui: &mut egui::Ui, rect: Rect, salt: &str, label: &str,
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
    pub feedback: f32,
    /// Cumulative echo level after k cycles ([0] = 1.0), following the FB
    /// CURVE law (Natural = feedbackᵏ; Bloom/Gate mirror lib.rs's
    /// level-dependent loop gain with the bar level as the env proxy) — so
    /// the drawn tail visibly changes shape with the curve chips.
    pub cycle_gains: [f32; MAX_ECHO_CYCLES + 1],
    /// PingPong mode with a non-zero crossfeed amount.
    pub ping_pong: bool,
    /// d = 1 − 2·crossfeed. The fb crossfeed is a BLEND, not a hard swap: after
    /// k cycles the own-channel weight is (1+dᵏ)/2, swapped weight (1−dᵏ)/2.
    pub pp_own_d: f32,
    /// Freeze holds the loop (v2.35): every repeat at full level, the cycle
    /// at the length the audio thread latched.
    pub frozen: bool,
}

// Pick the smallest "nice" x-axis range covering `needed_secs`. The old
// floors (2 bars / 2 s) meant short clusters never filled the view — now the
// range adapts down to 1 beat / 250 ms so the content always fills.
fn pick_nice_range(needed_secs: f32, sync_mode: SyncMode, tempo_bpm: f32) -> f32 {
    match sync_mode {
        SyncMode::Sync => {
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

/// Default: reserve x-range for up to 4 echo cycles (the classic ECHO view).
pub(crate) fn compute_viz_data(params: &PetalParams, tempo_bpm: f32) -> VizData {
    compute_viz_data_ex(params, tempo_bpm, 4)
}

/// `shown_cap` = how many echo cycles the x-range reserves room for. The v3
/// GRID/STEMS editing views pass 1 (only the first repeat is ghosted there —
/// reserving 4 squeezed the editable taps into the left edge).
pub(crate) fn compute_viz_data_ex(params: &PetalParams, tempo_bpm: f32, shown_cap: usize) -> VizData {
    let n_active = params.taps_heard();
    let pos_l = compute_positions(params.spacing(false), n_active);
    let pos_r = compute_positions(params.spacing(true), n_active);

    let time_l_norm = params.base_time_l.value();
    let time_r_norm = if params.time_linked.value() { time_l_norm } else { params.base_time_r.value() };
    let base_secs_l = params.base_time_seconds(time_l_norm, tempo_bpm);
    let base_secs_r = PetalParams::base_time_seconds_mode(params.sync_mode_r_eff(), time_r_norm, tempo_bpm);
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
    if frozen {
        cycle_gains = [1.0; MAX_ECHO_CYCLES + 1];
        n_echo_cycles = MAX_ECHO_CYCLES;
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

    let crossfeed = params.crossfeed.value().min(1.0);
    let ping_pong = matches!(params.crossfeed_mode.value(), crate::params::CrossfeedMode::PingPong)
        && crossfeed > 1e-4;
    VizData {
        times_l, times_r, grid_range_secs, times_l_grid, grid_range_secs_taps_only,
        span_l_secs: span_l,
        fb_cycle_norm: fb_cycle_secs / grid_range_secs,
        n_echo_cycles, feedback, cycle_gains, ping_pong,
        pp_own_d: 1.0 - 2.0 * crossfeed,
        frozen,
    }
}

// ─── Timeline grid (beats in Sync, ms in Free) ─────────────────────────────
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_timeline_grid(
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

pub(crate) fn dashed_h(p: &egui::Painter, start: Pos2, width: f32, color: Color32, s: f32) {
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
pub(crate) fn draw_stems(p: &egui::Painter, rect: Rect, params: &Arc<PetalParams>, viz: &VizData, tempo_bpm: f32, is_sync: bool, s: f32) {
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
    // Bar width scales with the smallest gap between direct taps: chunky
    // rounded pills when there's room, hairlines when the cluster is dense.
    let bar_w = {
        let mut min_gap_px = draw_w;
        let mut prev_x: Option<f32> = None;
        for i in 0..n_active {
            if !params.taps[i].active.value() { continue; }
            let x = draw_l + viz.times_l[i].min(1.0) * draw_w;
            if let Some(px) = prev_x { let g = (x - px).abs(); if g > 0.5 { min_gap_px = min_gap_px.min(g); } }
            prev_x = Some(x);
        }
        (min_gap_px * 0.6).clamp(1.5 * s, 8.0 * s)
    };
    // Rounded pill bar: |x| centered, from y0 (top) to y1 (bottom).
    let bar = |x: f32, y0: f32, y1: f32, w: f32, col: Color32| {
        if y1 - y0 < 0.5 { return; }
        let hw = (w * 0.5).max(0.5);
        p.rect_filled(
            Rect::from_min_max(Pos2::new(x - hw, y0), Pos2::new(x + hw, y1)),
            hw, // full rounding = pill caps
            col,
        );
    };

    for k in 0..=viz.n_echo_cycles {
        // Curve-shaped cumulative level (Natural = feedbackᵏ as before).
        let cycle_gain = viz.cycle_gains[k.min(MAX_ECHO_CYCLES)];
        if cycle_gain < ECHO_THRESHOLD { break; }
        // Ping-pong crossfeeds the fb CONTENT between channels as a blend
        // (fb_l' = fb_l·(1-cf) + fb_r·cf), not a hard swap: after k cycles the
        // own-channel weight is (1+dᵏ)/2 with d = 1-2cf. Draw each echo as two
        // weighted copies — straight and L/R-time-swapped. At cf=1 this
        // degenerates to the classic alternating full swap. Pan weights are
        // per-output-channel in the DSP and do NOT swap with the content.
        let own_w = if viz.ping_pong && k > 0 { (1.0 + viz.pp_own_d.powi(k as i32)) * 0.5 } else { 1.0 };
        for i in 0..n_active {
            let tap = &params.taps[i];
            let active = tap.active.value();
            let pan = tap.pan.value();
            let t_l = viz.times_l[i] + k as f32 * viz.fb_cycle_norm;
            let t_r = viz.times_r[i] + k as f32 * viz.fb_cycle_norm;
            if t_l > 1.0 && t_r > 1.0 { continue; }
            let bw = if k == 0 { bar_w } else { (bar_w * 0.6).max(1.0) };
            for (t_a, t_b, w) in [(t_l, t_r, own_w), (t_r, t_l, 1.0 - own_w)] {
                if w < 0.02 { continue; }
                let g = cycle_gain * w;
                let height_scale = if k == 0 { 1.0 } else { g.cbrt() };
                let opacity_scale = if k == 0 { 1.0 } else { (g.sqrt() * 0.7).max(0.12) };
                let op = if active { opacity_scale } else { 0.22 * w };
                let gain_n = (gain_db_to_pos(tap.gain_db.value()) * height_scale).clamp(0.0, 1.0);
                let x_l = draw_l + t_a.min(1.0) * draw_w;
                let x_r = draw_l + t_b.min(1.0) * draw_w;
                let (in_l, in_r) = (t_a <= 1.0, t_b <= 1.0);
                let col = tap_color(i, n_active).linear_multiply(op);
                match ch_mode {
                    ChannelMode::Stereo => {
                        let l_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).cos();
                        let r_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).sin();
                        let (h_l, h_r) = (gain_n * l_w * max_bar, gain_n * r_w * max_bar);
                        if in_l { bar(x_l, mid_y - h_l, mid_y, bw, col); }
                        if in_r { bar(x_r, mid_y, mid_y + h_r, bw, col); }
                    }
                    ChannelMode::MidSide => {
                        let mid_amt = ((1.0 - pan) * 0.5).clamp(0.0, 1.0);
                        let side_amt = ((1.0 + pan) * 0.5).clamp(0.0, 1.0);
                        let mid_h = gain_n * max_bar * 0.5 * mid_amt;
                        let side_h = gain_n * max_bar * 0.35 * side_amt;
                        if in_l { bar(x_l, mid_y - mid_h, mid_y + mid_h, bw, col); }
                        if in_r {
                            let (st_y, sb_y) = (mid_y - max_bar * 0.65, mid_y + max_bar * 0.65);
                            bar(x_r, st_y - side_h, st_y + side_h, bw, col);
                            bar(x_r, sb_y - side_h, sb_y + side_h, bw, col);
                        }
                    }
                    ChannelMode::Mono => {
                        let h = gain_n * max_bar;
                        if in_l { bar(x_l, mid_y - h * 0.5, mid_y + h * 0.5, bw, col); }
                    }
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
    // "dB" header above the amplitude scale (parity with the pre-port grid viz).
    p.text(Pos2::new(rect.min.x + pad_l - 6.0 * s, draw_t - 6.0 * s), Align2::RIGHT_BOTTOM, "dB",
           FontId::new(8.0 * s, FontFamily::Monospace), theme::FAINT());
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
            // px → offset units: time_offset is a fraction of the cluster SPAN
            // (16 × base time) while the axis is grid_range wide — convert so
            // the ball tracks the pointer and the audio tracks the ball.
            let dx = resp.drag_delta().x / draw_w
                * (viz.grid_range_secs_taps_only / viz.span_l_secs.max(1e-6));
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

// ─── RIGHT card body — per-tap controls + in-card footer (v2.15 geometry) ────
fn draw_right_body(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter,
                   tab: RightTab, tempo: f32,
                   taps_drag: &mut Option<(i32, f32)>, fb_drag: &mut Option<(f32, f32)>) {
    let s = theme::ui_scale(ui);
    // Allocate the full shared card-body rect (so this card bottom-aligns with
    // DELAY/VIEW), then carve a RIGHT_FOOTER_H band off the bottom for the
    // FEEDBACK/FB-TIME/TAPS footer — the footer lives INSIDE the card, as v2.15.
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), CARD_BODY_H * s), Sense::hover());
    let foot_h = RIGHT_FOOTER_H * s;
    let content = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.max.y - foot_h));
    let foot = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - foot_h), rect.max);

    // divider above the footer band
    ui.painter().line_segment(
        [Pos2::new(rect.min.x, foot.min.y), Pos2::new(rect.max.x, foot.min.y)],
        Stroke::new(1.0 * s, theme::line()));

    // tab content (tap rows / knob panels) in the upper area, clipped so long
    // tap lists never bleed into the footer band. The tap list scrolls: rows
    // got taller (roomier rhythm) so 16 taps overflow the card by design.
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    cui.set_clip_rect(content);
    cui.add_space(theme::SP_XS * s);
    egui::ScrollArea::vertical()
        .id_salt("tap_rows")
        .max_height(content.height() - theme::SP_XS * s)
        .auto_shrink([false, false])
        .show(&mut cui, |ui| {
            draw_tap_content(ui, params, setter, tab, s);
        });

    draw_incard_footer(ui, foot, params, setter, tempo, taps_drag, fb_drag, s);
}

// One row per tap for PAN/GAIN/FILTER, or the XFEED/DIFFUSE knob panels.
fn draw_tap_content(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, tab: RightTab, s: f32) {
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let tw = ui.available_width();
    let grad = |i| Accent::Raw(tab_grad(tab, i, n_active));
    // Roomier ~36px row rhythm (taller slider rows + wider gaps); 16 taps
    // intentionally overflow the card and scroll (owner request, 2026-07-11).
    let tap_rows = matches!(tab, RightTab::Pan | RightTab::Gain | RightTab::Filter);
    if tap_rows {
        ui.spacing_mut().item_spacing.y = theme::SP_MD * s;
        theme::set_slider_size(1.4);
        theme::set_slider_gap(6.0);
    }
    match tab {
        RightTab::Gain => for i in 0..n_active {
            let tap = &params.taps[i];
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                // per-tap mute — the tap number doubles as the active toggle
                epo::toggle_sm(ui, &format!("{:02}", i + 1), &tap.active, setter);
                let w = ui.available_width();
                epo::slider(ui, setter, &tap.gain_db, "", w, 0.0, grad(i), false);
            });
        },
        RightTab::Pan => for i in 0..n_active {
            let tap = &params.taps[i];
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SP_SM * s;
                epo::toggle_sm(ui, &format!("{:02}", i + 1), &tap.active, setter);
                let w = ui.available_width();
                epo::slider(ui, setter, &tap.pan, "", w, 0.0, grad(i), true);
            });
        },
        RightTab::Filter => {
            // Two-line rows (v2.17.6): identity + type/slope chips on top,
            // full-width CUTOFF + Q sliders below. The old 5-control single
            // row was the cramped one at RIGHT_W=284; taller rows + scroll
            // (v2.17.5) pay for the extra height.
            ui.spacing_mut().item_spacing.y = theme::SP_TIGHT * s;
            for i in 0..n_active {
                if i > 0 { ui.add_space(theme::SP_LG * s); }
                let tap = &params.taps[i];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SP_XS * s;
                    // per-tap mute — the tap number doubles as the active toggle
                    epo::toggle_sm(ui, &format!("{:02}", i + 1), &tap.active, setter);
                    // type-cycle chip (LP → BP → HP)
                    let (tl, tn) = match tap.filter_type.value() {
                        FilterType::LowPass => ("LP", FilterType::BandPass),
                        FilterType::BandPass => ("BP", FilterType::HighPass),
                        FilterType::HighPass => ("HP", FilterType::LowPass),
                    };
                    if epo::chip_sm(ui, tl, false) { set_enum(setter, &tap.filter_type, tn); }
                    // slope-cycle chip (12 → 24 → 48 dB/oct)
                    let (sl, sn) = match tap.filter_slope.value() {
                        FilterSlope::Db12 => ("12", FilterSlope::Db24),
                        FilterSlope::Db24 => ("24", FilterSlope::Db48),
                        FilterSlope::Db48 => ("48", FilterSlope::Db12),
                    };
                    if epo::chip_sm(ui, sl, false) { set_enum(setter, &tap.filter_slope, sn); }
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SP_XS * s;
                    let rem = ui.available_width();
                    let q_w = (rem * 0.34).clamp(72.0 * s, 120.0 * s);
                    let cut_w = rem - q_w - theme::SP_XS * s;
                    let mut co = epo::SliderOpts::default();
                    co.value_w = 52.0 * s; // "20000 Hz" fits on the full-width line
                    epo::slider_ex(ui, setter, &tap.filter_cutoff, "", cut_w, 0.0, grad(i), &co);
                    let mut qo = epo::SliderOpts::default();
                    qo.value_w = 26.0 * s;
                    epo::slider_ex(ui, setter, &tap.filter_q, "Q", q_w, 12.0 * s, grad(i), &qo);
                });
            }
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
            ui.add_space(theme::SP_XL * s);
            let v = params.crossfeed.value();
            let amt_str = if v < 0.01 { "OFF".to_string() } else { format!("{:.0}%", v * 100.0) };
            // caption reflects the crossfeed mode (parity with the pre-port panel)
            let xf_lab = match params.crossfeed_mode.value() {
                CrossfeedMode::Mix => "BLEND",
                CrossfeedMode::PingPong => "BOUNCE",
                CrossfeedMode::Diffuse => "DEPTH",
            };
            ui.horizontal(|ui| {
                ui.add_space((tw - theme::KNOB_DIAM_INSPECT - theme::KNOB_PAD_W * s) * 0.5);
                epo::knob_ex(ui, setter, &params.crossfeed, xf_lab, theme::KNOB_DIAM_INSPECT, acc, false, None, Some(&amt_str));
            });
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
            ui.add_space(theme::SP_XL * s);
            let v_amt = params.diffuse_amount.value();
            let amt_str = if v_amt < 0.01 { "OFF".to_string() } else { format!("{:.0}%", v_amt * 100.0) };
            let size_str = format!("{:.2}\u{00D7}", params.diffuse_size.value());
            let cell = theme::KNOB_DIAM_INSPECT + theme::KNOB_PAD_W * s;
            ui.horizontal(|ui| {
                ui.add_space((tw - 2.0 * cell - theme::KNOB_GAP * s).max(0.0) * 0.5);
                ui.spacing_mut().item_spacing.x = theme::KNOB_GAP * s;
                epo::knob_ex(ui, setter, &params.diffuse_amount, "AMOUNT", theme::KNOB_DIAM_INSPECT, acc, false, None, Some(&amt_str));
                epo::knob_ex(ui, setter, &params.diffuse_size, "SIZE", theme::KNOB_DIAM_INSPECT, acc, false, None, Some(&size_str));
            });
            ui.add_space(theme::SP_MD * s);
            epo::hint(ui, "8× Schroeder allpass · smaller SIZE = tighter & brighter");
        }
    }
    // Slider ergonomics are GLOBAL — restore so the footer/XFEED sliders keep
    // their normal height.
    if tap_rows {
        theme::set_slider_size(1.0);
        theme::set_slider_gap(0.0);
    }
}

// ─── In-card footer (v2.15): FEEDBACK · FB TIME · TAPS as three columns in the
//     bottom band of the right card. Each column stacks single widgets, so
//     top_down(Center) centers them horizontally without a sizing pass. ─────────
#[allow(clippy::too_many_arguments)]
fn draw_incard_footer(ui: &mut egui::Ui, foot: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                      tempo: f32, taps_drag: &mut Option<(i32, f32)>, fb_drag: &mut Option<(f32, f32)>, s: f32) {
    let locked = params.feedback_time_locked.value();
    // Locked: FB TIME follows the cluster span (last active tap) — display it live.
    let fb_time_str = if locked {
        let n = (params.num_taps_active.value() as f32).max(1.0);
        let base_l = params.base_time_seconds(params.base_time_l.value(), tempo);
        let base_r = if params.time_linked.value() { base_l }
                     else { params.base_time_seconds(params.base_time_r.value(), tempo) };
        let span = (n * base_l).max(n * base_r);
        if span < 1.0 { format!("{:.0} ms", span * 1000.0) } else { format!("{:.2} s", span) }
    } else {
        params.format_feedback_time(tempo)
    };

    let third = foot.width() / 3.0;
    let col = |i: f32| Rect::from_min_size(Pos2::new(foot.min.x + i * third, foot.min.y), Vec2::new(third, foot.height()));
    {
        let p = ui.painter();
        for i in [1.0_f32, 2.0] {
            let x = foot.min.x + i * third;
            p.line_segment([Pos2::new(x, foot.min.y + 8.0 * s), Pos2::new(x, foot.max.y - 8.0 * s)],
                           Stroke::new(1.0 * s, theme::line()));
        }
    }

    // col 0 — FEEDBACK knob (coral, its domain)
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(col(0.0)).layout(Layout::top_down(Align::Center)));
        c.add_space(2.0 * s);
        let fb_str = format!("{:.0}%", params.feedback.value() * 100.0);
        epo::knob_ex(&mut c, setter, &params.feedback, "FEEDBACK", theme::KNOB_DIAM_FX, Accent::Head(1), false, None, Some(&fb_str));
    }
    // col 1 — FB TIME readout + mode (unlocked) + lock, stacked
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(col(1.0)).layout(Layout::top_down(Align::Center)));
        c.add_space(if locked { 12.0 } else { 6.0 } * s);
        fb_time_group(&mut c, params, setter, fb_drag, s, locked, &fb_time_str);
    }
    // col 2 — TAPS count
    {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(col(2.0)).layout(Layout::top_down(Align::Center)));
        c.add_space(22.0 * s);
        taps_group(&mut c, params, setter, taps_drag, s);
    }
}

/// FB-TIME footer column (stacked): readout (drag-to-edit when unlocked), the
/// mode chip (unlocked only), then the lock chip. Parent is `top_down(Center)`
/// so each stacked widget centers horizontally.
fn fb_time_group(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter,
                 fb_drag: &mut Option<(f32, f32)>, s: f32, locked: bool, fb_time_str: &str) {
    ui.spacing_mut().item_spacing.y = theme::SP_TIGHT * s;
    let fb_resp = epo::readout_chip(ui, "FB TIME", fb_time_str);
    if !locked {
        let fb_resp = fb_resp.interact(Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if fb_resp.drag_started() {
            setter.begin_set_parameter(&params.feedback_time);
            *fb_drag = Some((params.feedback_time.value(), 0.0));
        }
        if let Some((v0, acc0)) = *fb_drag {
            let acc = acc0 - fb_resp.drag_delta().y;
            *fb_drag = Some((v0, acc));
            let new_norm = if params.feedback_time_mode.value() == SyncMode::Sync {
                let n_divs = crate::params::SYNC_DIVS.len() as f32 - 1.0;
                let start_idx = (v0 * n_divs).round();
                ((start_idx + (acc / (14.0 * s)).round()) / n_divs).clamp(0.0, 1.0)
            } else {
                (v0 + acc * 0.003).clamp(0.0, 1.0)
            };
            setter.set_parameter(&params.feedback_time, new_norm);
            if fb_resp.drag_stopped() {
                setter.end_set_parameter(&params.feedback_time);
                *fb_drag = None;
            }
        }
        let mode = params.feedback_time_mode.value();
        let lab = if mode == SyncMode::Sync { "SYNC" } else { "FREE" };
        if epo::chip_sm(ui, lab, false) {
            let next = if mode == SyncMode::Sync { SyncMode::Free } else { SyncMode::Sync };
            set_enum(setter, &params.feedback_time_mode, next);
        }
    }
    if epo::chip_sm(ui, if locked { "LOCK" } else { "free" }, locked) {
        setter.begin_set_parameter(&params.feedback_time_locked);
        setter.set_parameter(&params.feedback_time_locked, !locked);
        setter.end_set_parameter(&params.feedback_time_locked);
    }
}

/// TAPS footer column: the tap-count readout with drag-on-number + scroll.
fn taps_group(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter,
              taps_drag: &mut Option<(i32, f32)>, s: f32) {
    let taps_resp = epo::readout_chip(ui, "TAPS", &format!("{}", params.num_taps_active.value()))
        .interact(Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let n = params.num_taps_active.value();
    if taps_resp.drag_started() {
        setter.begin_set_parameter(&params.num_taps_active);
        *taps_drag = Some((n, 0.0));
    }
    if let Some((n0, acc0)) = *taps_drag {
        let acc = acc0 - taps_resp.drag_delta().y;
        *taps_drag = Some((n0, acc));
        let target = (n0 + (acc / (18.0 * s)).round() as i32).clamp(1, NUM_TAPS as i32);
        if target != n { setter.set_parameter(&params.num_taps_active, target); }
        if taps_resp.drag_stopped() {
            setter.end_set_parameter(&params.num_taps_active);
            *taps_drag = None;
        }
    }
    if taps_resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 4.0 {
            setter.begin_set_parameter(&params.num_taps_active);
            setter.set_parameter(&params.num_taps_active, (n + scroll.signum() as i32).clamp(1, NUM_TAPS as i32));
            setter.end_set_parameter(&params.num_taps_active);
        }
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────
pub(crate) fn set_enum<T: PartialEq + Copy>(setter: &ParamSetter, param: &EnumParam<T>, v: T)
where
    T: nice_plug::params::enums::Enum,
{
    setter.begin_set_parameter(param);
    setter.set_parameter(param, v);
    setter.end_set_parameter(param);
}
