//! Scope-first page — the redesign on the "Afterecho redesign" canvas
//! (2026-10-27). The tap field is the picture, the Echo lane shows the
//! repeats, and a right column holds Time & taps · Colour ·
//! Output. Two page shapes share it (see [`Shape`]); every rect is in logical
//! px, scaled by `s` at use. Built on the Tide-derived UI layer in
//! [`crate::ui`], not on epoche-ui; modelled on Tide's `editor_scope.rs`.
//!
//! First pass: page frame, header, Settings menu and the right column, wired to
//! the real parameters. The tap field, the Echo lane drawing and the new
//! functions come next.
#![allow(deprecated)]

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};
use nice_plug::prelude::*;
use nice_plug_egui::EguiState;
use std::path::PathBuf;
use std::sync::Arc;

use crate::gui::{gain_db_to_pos, pos_to_gain_db, EditorState, VizData};
use crate::bounce;
use crate::params::{ChannelMode, CrossfeedMode, FilterSlope, FilterType, AfterechoParams, SpacingMode, SyncMode, TapParams, NUM_TAPS, REPEATS_LOOP};
use crate::topology;
use crate::ui::epo;
use crate::ui::theme::{self, G, INK, INK2, INSET, PANEL, PAPER, SOFT};
use softsystems_ui::controls;
use softsystems_ui::header;
use softsystems_ui::options::{self, Env, Product, Zoom};
use softsystems_ui::tokens::{accents, presets as ss_presets, Accent, CustomAccent};
use softsystems_ui::{Appearance, Theme};

mod motion_tab;
mod preset_sheet;

/// Element sizes (logical px), the same on every page shape — Tide's, so the
/// plug-ins match.
pub(crate) mod g {
    pub const HEAD_Y: f32 = softsystems_ui::tokens::PAGE_MARGIN;
    pub const HEAD_H: f32 = softsystems_ui::header::HEADER_H;
    pub const R_CARD: f32 = softsystems_ui::tokens::R_CARD;
    pub const R_CTL: f32 = softsystems_ui::tokens::R_CTL;
    /// A control cell: its name, the control band, then its value. A knob
    /// fills the band; pills (`CTL_H`) sit centred in it.
    pub const CTL_H: f32 = softsystems_ui::tokens::CTL_H;
    pub const LBL_H: f32 = 13.0;
    /// Knob diameter, between Tide's 28 and the τόπος sheet's 48. On touch a
    /// knob answers within 44 px of its centre whatever its size.
    pub const DIAL: f32 = 36.0;
    pub const BAND: f32 = DIAL;
    /// Air between the name, the control and the value (owner, 2026-11-09:
    /// "the knobs don't breathe"; was 1 px).
    pub const AIR: f32 = 6.0;
    pub const CELL_H: f32 = BAND + 2.0 * AIR + 2.0 * LBL_H;
}

/// Where the regions sit on the page (logical px). Rects are
/// `(x, y, w, h)`; the right-column cards are `(y, h)` at `right_x`.
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pad: f32,
    picture: (f32, f32, f32, f32),
    lane: (f32, f32, f32, f32),
    right_x: f32,
    right_w: f32,
    time: (f32, f32),
    colour: (f32, f32),
    output: (f32, f32),
}

/// The one page, on desktop and in iPad host panels (owner, 2026-11-09:
/// Afterecho always runs in a host, never full screen). Cards are sized to their rows,
/// as tight as Tide's. The shared frame (owner, 2026-11-02): a 16 px page
/// margin round the outer blocks (the header strip is one) and 12 px between
/// blocks.
const LAYOUT: Layout = {
    // the crate's frame: PAGE_MARGIN round the outer blocks, BLOCK_GAP between them
    const M: f32 = softsystems_ui::tokens::PAGE_MARGIN;
    const GAP: f32 = softsystems_ui::tokens::BLOCK_GAP;
    const W: f32 = 1366.0;
    const LEFT_W: f32 = 806.0;
    const TOP: f32 = M + g::HEAD_H + GAP; // 72: the first row of cards
    const PIC_H: f32 = 320.0;
    const LANE_Y: f32 = TOP + PIC_H + GAP; // 404
    const LANE_H: f32 = 246.0;
    const COLOUR_H: f32 = 117.0;
    Layout {
    w: W,
    h: LANE_Y + LANE_H + M, // 666
    pad: M,
    // the field ends with the Time card, the lane starts with Colour
    picture: (M, TOP, LEFT_W, PIC_H),
    lane: (M, LANE_Y, LEFT_W, LANE_H),
    right_x: M + LEFT_W + GAP, // 834
    right_w: W - (M + LEFT_W + GAP) - M, // 516
    time: (TOP, PIC_H),
    colour: (LANE_Y, COLOUR_H),
    output: (LANE_Y + COLOUR_H + GAP, 117.0), // 533
    }
};

/// Afterecho, as the shared Soft Systems UI sees it: its own palette (Mono
/// dark, Paper light) after Soft Systems, its accent, dark first.
pub const PRODUCT: Product = Product {
    name: "Afterecho",
    slug: "afterecho",
    // "1.0.0-beta.1 · a1b2c3d" in the ☰ footer, and the beta pop-up's build (build.rs)
    version: env!("AFTERECHO_BUILD"),
    build: env!("AFTERECHO_BUILD_ID"),
    accent: accents::AFTERECHO,
    presets: &[ss_presets::AFTERECHO],
    dark_default: true,
    manual: None,
    support: None,
};


/// ☰ → Accent: Afterecho's own first, then five of the shared ones; the
/// user's own colour is the seventh swatch.
const ACCENTS: [Accent; 6] = [accents::AFTERECHO, accents::GREEN, accents::CYAN, accents::BLUE, accents::AMBER, accents::VIOLET];

/// Text size: `px` at the design text size, following ☰ → Text size.
fn fs(px: f32, s: f32) -> f32 {
    px * s * softsystems_ui::t().text
}

fn r(o: Pos2, s: f32, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(o + Vec2::new(x, y) * s, Vec2::new(w, h) * s)
}

fn text(p: &egui::Painter, pos: Pos2, align: Align2, t: &str, px: f32, s: f32, col: Color32) -> Rect {
    p.text(pos, align, t, FontId::proportional(fs(px, s)), col)
}

fn mono(p: &egui::Painter, pos: Pos2, align: Align2, t: &str, px: f32, s: f32, col: Color32) -> Rect {
    p.text(pos, align, t, FontId::monospace(fs(px, s)), col)
}

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

/// The accent file Afterecho wrote before the crate kept the accent (until 2026-11-17):
/// read once into `Appearance::accent`/`custom`, then removed.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct OldAccent {
    on: usize,
    custom: CustomAccent,
}

impl OldAccent {
    fn file() -> PathBuf {
        softsystems_ui::appearance::product_dir(&PRODUCT).join("accent.json")
    }

    /// Move an old `accent.json` into `look` (and save it); nothing when there's none.
    fn import(look: &mut Appearance) {
        Self::import_at(&Self::file(), &softsystems_ui::appearance::file(&PRODUCT), look);
    }

    fn import_at(old_file: &std::path::Path, look_file: &std::path::Path, look: &mut Appearance) {
        let Some(old) = std::fs::read_to_string(old_file).ok().and_then(|t| serde_json::from_str::<OldAccent>(&t).ok()) else {
            return;
        };
        look.accent = old.on.min(ACCENTS.len()) as u8;
        look.custom = old.custom;
        if look.write(look_file).is_ok() {
            let _ = std::fs::remove_file(old_file);
        }
    }
}

/// A fresh install's look: Afterecho's own palette, dark.
fn default_look() -> Appearance {
    Appearance { palette: 1, ..Appearance::default_for(&PRODUCT) }
}

/// Editor-only state of this page (lives in [`EditorState`]).
#[derive(Default)]
pub struct ScopeState {
    inited: bool,
    /// Every ☰ view setting (theme, palette, text size, accent, own colour, the size a new
    /// instance opens at), saved per computer (the crate's `appearance.json`).
    look: Appearance,
    /// A preview run (`AFTERECHO_PREVIEW`): the look is never saved.
    preview: bool,
    /// The ☰ Options panel.
    options: options::Panel,
    /// The beta's badge, banner and pop-up (a new one per window: the pop-up shows on open).
    beta: softsystems_ui::beta::BetaUi,
    /// The banner's height with its gap, logical px, last frame: the cards and the window
    /// move down by it in the beta's last 7 days (0 otherwise).
    banner_h: f32,
    /// Param id of the control last touched. Nothing reads it since the
    /// last-touched card went (v2.42); the cells still record it.
    focus: Option<String>,
    /// The tap selected in the picture.
    sel: Option<usize>,
    /// Picture: Y axis 0 Pan · 1 Gain · 2 Cutoff; view 0 Field · 1 Bars.
    y_axis: usize,
    view: usize,
    /// The Time view (v2.43, above Pan on the Y list): each tap on its own
    /// row, drags move only its time. The value views move only values.
    time_view: bool,
    /// Lane tab: 0 Echo · 1 Motion · 2 Tap (the selected tap's parameters).
    lane_tab: usize,
    /// The tab the lane returns to when the Tap tab closes.
    lane_back: usize,
    /// The tap the Tap tab opens on when nothing is selected.
    last_tap: usize,
    /// The Motion tab: the open slot, its editors' state, the Routes sheet.
    motion: motion_tab::MotionUi,
    /// Preview: route a few slots on the first frame, to see the matrix.
    mx_demo: bool,
    /// Preview only (`armed`): slot 1 armed, to see the rings.
    arm_demo: bool,
    /// Colour card tab: 0 Diffuse · 1 Tape · 2 Saturate.
    colour_tab: usize,
    /// The Taps menu (pattern tools, v2.37): open, and its button (a click
    /// there isn't "outside" the menu).
    pat_open: bool,
    pat_btn: Option<Rect>,
    /// The last shape applied, so the range can re-spread it: Alternate's
    /// flip, Random's seed.
    pat_shape: Option<PatShape>,
    pat_flip: bool,
    pat_seed: u32,
    /// Each axis's range on the menu's slider (0 left … 1 right; unset =
    /// the axis's default), the pin being dragged, and each axis's undo
    /// history (every tap's value before a pattern).
    pat_range: [Option<[f32; 2]>; 7],
    pat_pin: Option<usize>,
    pat_undo: [Vec<[f32; NUM_TAPS]>; 7],
    /// Preview: apply a shape on the first frame, to see it without clicking.
    pat_demo: bool,
    /// Preview only (`euclid`): Euclid, 5 taps in 8 steps, Swing at 0.7.
    euclid_demo: bool,
    /// Preview only (`swing`): Swing at 0.7, the spacing as it is.
    swing_demo: bool,
    /// Preview only (`sync`): Time synced.
    sync_demo: bool,
    /// Preview only (`over`): Feedback at 104 %, tap 1's Gain at +3 dB (the stops' over-range).
    over_demo: bool,
    /// The preset browser's state (the loaded preset, Try, the sheet's view).
    pub(crate) presets: preset_sheet::PresetUi,
    /// Preview only (`nobeta`): the beta pop-up isn't drawn, to see the page under it.
    no_beta: bool,
}

/// The pattern menu's shapes: where each tap sits between the range's ends.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PatShape {
    Alternate,
    RampDown,
    RampUp,
    Random,
}

impl PatShape {
    const ALL: [PatShape; 4] = [PatShape::Alternate, PatShape::RampDown, PatShape::RampUp, PatShape::Random];
}

/// Where each of `n` taps sits for `shape`, 0 (the range's top end, as the
/// field shows it) … 1 (its bottom end). Alternate starts at the top unless
/// flipped; Random is the same for the same seed.
fn pattern_t(shape: PatShape, flip: bool, n: usize, seed: u32) -> Vec<f32> {
    let mut x = seed.max(1);
    (0..n)
        .map(|i| {
            let f = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match shape {
                PatShape::Alternate => ((i % 2 == 1) != flip) as u8 as f32,
                PatShape::RampDown => f,
                PatShape::RampUp => 1.0 - f,
                PatShape::Random => (x >> 8) as f32 / 16_777_216.0,
            }
        })
        .collect()
}

/// The values a shape gives `n` taps on `axis` between the slider's pins.
fn pattern_values(axis: Axis, shape: PatShape, flip: bool, n: usize, seed: u32, range: [f32; 2]) -> Vec<f32> {
    let (a, b) = (axis.u_norm(range[0]), axis.u_norm(range[1]));
    let (top, bot) = (a.min(b), a.max(b));
    pattern_t(shape, flip, n, seed).into_iter().map(|t| axis.value(top + (bot - top) * t)).collect()
}

impl ScopeState {
    /// The look at page zoom `s`: the palette and text size picked in ☰, with
    /// the accent picked there (one of `ACCENTS`, or the user's own colour).
    fn theme(&self, s: f32) -> Theme {
        match ACCENTS.get(self.look.accent as usize) {
            Some(&accent) => Theme::of(&Product { accent, ..PRODUCT }, &self.look, s),
            None => Theme::of_custom(&PRODUCT, &self.look, &self.look.custom, s),
        }
    }

    /// Save the look (never in a preview).
    fn save_look(&self) {
        if !self.preview {
            let _ = self.look.save(&PRODUCT);
        }
    }

    /// Select tap `i` and show all its parameters in the lane.
    fn open_tap(&mut self, i: usize) {
        self.sel = Some(i);
        if self.lane_tab != 2 {
            self.lane_back = self.lane_tab;
            self.lane_tab = 2;
        }
    }

    /// Clear the selection; the lane leaves the Tap tab.
    fn deselect(&mut self) {
        self.sel = None;
        if self.lane_tab == 2 {
            self.lane_tab = self.lane_back;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame
// ─────────────────────────────────────────────────────────────────────────────

/// One editor frame of the scope page: publish the look, zoom, then draw.
pub fn frame(
    ui: &mut egui::Ui,
    params: &Arc<AfterechoParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    egui_state: &EguiState,
    tempo: f32,
    licence: &softsystems_licence::Licence,
) {
    let ios = cfg!(target_os = "ios");
    let mut open_presets = false;
    {
        let sc = &mut st.scope;
        if !sc.inited {
            sc.inited = true;
            sc.preview = !ios && std::env::var_os("AFTERECHO_PREVIEW").is_some();
            let file = softsystems_ui::appearance::file(&PRODUCT);
            if sc.preview {
                // a preview reads the look but never writes it
                sc.look = Appearance::read(&file).unwrap_or_else(default_look);
            } else {
                sc.look = Appearance::load_or_import(&PRODUCT, || Some(default_look()));
                OldAccent::import(&mut sc.look);
            }
            // Desktop previews, to check a state without clicking to it:
            // AFTERECHO_PREVIEW=bars,menu,time|gain|cutoff|reso|type|slope|detune,sel3,tap|motion,mxdemo,step|step2|follow,presets,light,tape,pattern,
            // saveas|trying|deleting (with AFTERECHO_PRESET_DIR, a folder of presets to show)
            // (the Bars view, the Time view or a Y axis, a selected
            // tap, the Tap or Motion tab and its source, the preset browser, the light page, Colour's Tape
            // tab, the Taps menu with Ramp down applied). A preview never saves
            // the look it shows.
            if let (false, Ok(v)) = (ios, std::env::var("AFTERECHO_PREVIEW")) {
                for t in v.split(',').map(str::trim) {
                    match t {
                        "bars" => sc.view = 1,
                        "gain" => sc.y_axis = 1,
                        "cutoff" => sc.y_axis = 2,
                        "reso" => sc.y_axis = 3,
                        "type" => sc.y_axis = 4,
                        "slope" => sc.y_axis = 5,
                        "detune" => sc.y_axis = 6,
                        "menu" => sc.options.set_open(true),
                        "pattern" => (sc.pat_open, sc.pat_demo) = (true, true),
                        "euclid" => sc.euclid_demo = true,
                        "swing" => sc.swing_demo = true,
                        "sync" => sc.sync_demo = true,
                        "over" => sc.over_demo = true,
                        "tap" => sc.lane_tab = 2,
                        "motion" => sc.lane_tab = 1,
                        "echo" => sc.lane_tab = 0,
                        "armed" => sc.arm_demo = true,
                        "mxdemo" => (sc.lane_tab, sc.mx_demo) = (1, true),
                        "step" => (sc.lane_tab, sc.motion.sel) = (1, 4),
                        "step2" => (sc.lane_tab, sc.motion.sel) = (1, 5),
                        "follow" => (sc.lane_tab, sc.motion.sel) = (1, 3),
                        "routes" => (sc.lane_tab, sc.motion.routes) = (1, true),
                        "time" => sc.time_view = true,
                        "presets" => open_presets = true,
                        "saveas" => (open_presets, sc.presets.preview) = (true, 1),
                        "trying" => (open_presets, sc.presets.preview) = (true, 2),
                        "deleting" => (open_presets, sc.presets.preview) = (true, 3),
                        "light" => sc.look.dark = false,
                        "nobeta" => sc.no_beta = true,
                        "tape" => sc.colour_tab = 1,
                        "dub" => sc.colour_tab = 3,
                        "sat" => sc.colour_tab = 2,
                        _ => {
                            if let Some(n) = t.strip_prefix("sel").and_then(|n| n.parse::<usize>().ok()) {
                                sc.sel = n.checked_sub(1);
                            }
                        }
                    }
                }
                // a preview's presets live in their own folder, never the user's
                if let Some(d) = std::env::var_os("AFTERECHO_PRESET_DIR") {
                    sc.presets.dir = Some(PathBuf::from(d));
                }
            }
        }
    }
    if open_presets {
        st.preset_open = true;
    }
    // Desktop: zoom follows the window width and the height keeps the page's
    // aspect. iOS/AUv3: the host sizes the view (`with_host_resize`); the
    // page is fitted and centred in it.
    let vp = ui.ctx().screen_rect();
    let lay = &LAYOUT;
    let zoom = if ios {
        (vp.width() / lay.w).min(vp.height() / (lay.h + st.scope.banner_h)).clamp(0.3, 3.0)
    } else {
        (egui_state.size().0 as f32 / lay.w).clamp(0.5, 2.5)
    };
    // The shared theme, first thing: palette, accent, text size, zoom.
    softsystems_ui::theme::begin_frame(ui.ctx(), st.scope.theme(zoom));
    if !ios {
        let (w, h) = egui_state.size();
        let want_h = (w as f32 * (lay.h + st.scope.banner_h) / lay.w).round() as u32;
        if h.abs_diff(want_h) > 2 {
            egui_state.set_requested_size((w, want_h));
        }
    }

    ui.painter().rect_filled(vp, 0.0, PAPER());
    let s = zoom;
    let o = if ios {
        vp.min + ((vp.size() - Vec2::new(lay.w, lay.h + st.scope.banner_h) * s) * 0.5).max(Vec2::ZERO)
    } else {
        vp.min
    };
    // a preset that loads (the box's ‹ ›, the sheet) is one of the licence's check points
    let before = st.scope.presets.session.loaded().map(|p| p.id.clone());
    // one licence snapshot a frame, for the beta interface and the ☰ panel (a release: no beta)
    let lic = options::LicenceView::of(licence);
    let beta = softsystems_ui::beta::BetaView::of(&lic);
    page(ui, o, s, lay, params, setter, st, tempo, beta.as_ref());
    if st.scope.presets.session.loaded().map(|p| &p.id) != before.as_ref() {
        licence.check_state_load();
    }
    // the beta's pop-up over the page; its Activate opens the ☰ panel on Licence
    let page_rect = Rect::from_min_size(o, Vec2::new(lay.w, lay.h + st.scope.banner_h) * s);
    let sc = &mut st.scope;
    if !sc.no_beta {
        sc.beta.overlay(ui, page_rect, beta.as_ref(), &PRODUCT, &mut sc.options, ui.id().with("sc_beta"));
    }
    let lic = st.scope.options.is_open().then_some(lic);
    options_panel(ui, o, s, lay, params, setter, &mut st.scope, egui_state, zoom, licence, lic);
    ui.ctx().request_repaint();
}

#[allow(clippy::too_many_arguments)]
fn page(
    ui: &mut egui::Ui,
    o: Pos2,
    s: f32,
    l: &Layout,
    params: &Arc<AfterechoParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    tempo: f32,
    beta: Option<&softsystems_ui::beta::BetaView>,
) {
    header(ui, o, s, l, params, setter, st, beta);
    // the beta's last 7 days: a banner where the cards begin, the cards under it (the
    // window grows by as much, `frame`)
    let br = r(o, s, l.pad, l.time.0, l.w - 2.0 * l.pad, 0.0);
    st.scope.banner_h = st.scope.beta.banner(ui, br, beta, &PRODUCT, ui.id().with("sc_beta")) / s;
    let o = o + Vec2::new(0.0, st.scope.banner_h * s);
    // The preset browser covers the whole page under the header (v2.46;
    // before, the cards stayed in view beside it).
    st.scope.presets.track(st.preset_open);
    let presets_open = st.preset_open;
    if presets_open {
        preset_sheet::sheet(ui, o, s, l, params, setter, st);
    } else {
        if std::mem::take(&mut st.scope.mx_demo) {
            mx_demo(params);
        }
        if std::mem::take(&mut st.scope.arm_demo) {
            motion_tab::arm_demo(ui.ctx());
        }
        motion_tab::follow_wiring(ui.ctx(), &mut st.scope);
        // Motion's Routes sheet covers the picture and the lane
        if st.scope.motion.routes {
            motion_tab::routes_sheet(ui, o, s, l, params, setter, &mut st.scope);
        } else {
            picture(ui, o, s, l, params, setter, &mut st.scope, tempo);
            lane(ui, o, s, l, params, setter, &mut st.scope, tempo);
        }
        time_card(ui, o, s, l, params, setter, &mut st.scope.focus, tempo);
        colour_card(ui, o, s, l, params, setter, &mut st.scope);
        output_card(ui, o, s, l, params, setter, &mut st.scope.focus);
    }
    if !presets_open {
        taps_menu(ui, s, params, setter, &mut st.scope);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Small widgets (Tide's cell grammar: name above, control, value below)
// ─────────────────────────────────────────────────────────────────────────────

fn set_norm<P: Param>(setter: &ParamSetter, p: &P, v: f32) {
    setter.begin_set_parameter(p);
    setter.set_parameter_normalized(p, v);
    setter.end_set_parameter(p);
}

/// Index of an enum param's current variant, `n` variants in order.
fn enum_index<E: Enum + PartialEq>(p: &EnumParam<E>, n: usize) -> usize {
    (p.unmodulated_normalized_value() * (n - 1) as f32).round() as usize
}

/// Segmented control filling `rect`; returns the newly clicked index.
pub(crate) fn seg(ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, enabled: bool) -> Option<usize> {
    seg_ex(ui, rect, labels, active, salt, &[enabled; 8][..labels.len()])
}

/// [`seg`] with each option enabled on its own. Every option is laid out as
/// text on a hairline; the pick is ink, underlined, so each choice reads as its
/// own before it's touched.
#[allow(clippy::too_many_arguments)]
fn seg_ex(
    ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, enabled: &[bool],
) -> Option<usize> {
    seg_draw(ui, rect, labels, Some(active), salt, enabled, false)
}

/// [`seg`] for actions (the pattern menu's shapes): nothing need be picked,
/// and a click on the pick counts too (Alternate flips, Random re-rolls).
fn seg_again(ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: Option<usize>, salt: &str) -> Option<usize> {
    seg_draw(ui, rect, labels, active, salt, &[], true)
}

fn seg_draw(
    ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: Option<usize>, salt: &str, enabled: &[bool], again: bool,
) -> Option<usize> {
    let id = ui.id().with(("seg", salt));
    let active_i = active.unwrap_or(usize::MAX);
    let live = enabled.first().copied().unwrap_or(true);
    // the shared well segment; it returns the current one too
    let picked = if enabled.iter().all(|&e| e == live) {
        controls::seg(ui, rect, labels, active_i, id, live)
    } else {
        controls::seg_live(ui, rect, labels, active_i, id, enabled)
    };
    picked.filter(|&i| again || Some(i) != active)
}

/// A button (the shared chip): outlined on the card colour, filled when `on`.
/// True when tapped.
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, on: bool, salt: &str) -> bool {
    controls::chip(ui, rect, label, on, ui.id().with(("btn", salt)))
}

/// Split a cell into (control row, name centre, value centre). The name sits
/// above the control and the value below it, so the name stays readable while
/// a finger is on the control.
fn cell_parts(cell: Rect, s: f32) -> (Rect, Pos2, Pos2) {
    let lab = Pos2::new(cell.center().x, cell.top() + g::LBL_H * 0.5 * s);
    let ctl = Rect::from_min_size(
        Pos2::new(cell.left(), cell.top() + (g::LBL_H + g::AIR) * s),
        Vec2::new(cell.width(), g::BAND * s),
    );
    let val = Pos2::new(cell.center().x, ctl.bottom() + (g::AIR + g::LBL_H * 0.5) * s);
    (ctl, lab, val)
}

/// Lay out `rows` rows of `cols` equal cells inside `inner`, rows spread evenly.
fn cell_at(inner: Rect, cols: usize, rows: usize, row: usize, col: usize, span: usize, s: f32) -> Rect {
    let gap = 8.0 * s;
    let cw = (inner.width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let ch = g::CELL_H * s;
    let free = (inner.height() - ch * rows as f32).max(0.0) / (rows as f32 + 1.0);
    let x = inner.left() + col as f32 * (cw + gap);
    let y = inner.top() + free + row as f32 * (ch + free);
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(cw * span as f32 + gap * (span as f32 - 1.0), ch))
}

/// Points along a dial's sweep (270°, from 7:30), from `v0` to `v1` of it.
fn dial_arc(c: Pos2, rad: f32, v0: f32, v1: f32) -> Vec<Pos2> {
    let (a0, a1) = ((-135.0 + 270.0 * v0).to_radians(), (-135.0 + 270.0 * v1).to_radians());
    (0..=28)
        .map(|i| {
            let t = a0 + (a1 - a0) * i as f32 / 28.0;
            Pos2::new(c.x + rad * t.sin(), c.y - rad * t.cos())
        })
        .collect()
}

/// A knob cell: the crate's knob (`softsystems_ui::knob`: name, dial, value;
/// the arc in the accent). `id` is the param's id — the focus card follows it.
/// Stepped params of up to 16 positions show a dot per step.
#[allow(clippy::too_many_arguments)]
fn knob_cell<P: Param>(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &P, id: &str,
    label: &str, bip: bool, enabled: bool, disp: Option<String>, focus: &mut Option<String>,
) {
    knob_cell_m(ui, cell, s, setter, param, id, label, bip, enabled, disp, focus, None, None);
}

/// A Motion target's knob (beta pass 10): [`knob_cell`] plus a dot per route after its name,
/// the halo of its reach and live position, the armed slot's ring (a press then makes the
/// route, a drag sizes it), a tile's drop, and its routes on a right-click. `modu` is the
/// motion params, the target, and for a tap target the tap shown.
#[allow(clippy::too_many_arguments)]
fn knob_cell_m<P: Param>(
    ui: &mut egui::Ui, cell: Rect, _s: f32, setter: &ParamSetter, param: &P, id: &str,
    label: &str, bip: bool, enabled: bool, disp: Option<String>, focus: &mut Option<String>,
    modu: Option<(&crate::modulation::MotionParams, usize, Option<usize>)>, over: Option<f32>,
) -> Rect {
    use softsystems_ui::knob::{self, Kind, Knob, Reach};
    let n = param.modulated_normalized_value();
    let text = disp.unwrap_or_else(|| param.normalized_value_to_string(n, true));
    let kind = match param.step_count() {
        _ if bip => Kind::Bipolar,
        Some(c) if (1..=15).contains(&c) => Kind::Stepped(c + 1),
        _ => Kind::Plain,
    };
    let base = param.unmodulated_normalized_value();
    let marks = modu.map(|(m, t, tap)| motion_tab::marks(m, t, tap));
    let dots = marks.as_ref().map(motion_tab::route_dots).unwrap_or_default();
    let halo = marks.as_ref().and_then(|mk| motion_tab::halo(mk, base));
    let live = halo.map(|h| [h.1]);
    let mut k = Knob::new(label, &text)
        .kind(kind)
        .default(param.default_normalized_value())
        .live(enabled)
        .routes(&dots)
        .armed(modu.and_then(|_| motion_tab::armed(ui.ctx())));
    if let (Some((reach, _, col)), Some(live)) = (halo, live.as_ref()) {
        k = k.halo(Reach { reach, dots: live, col });
    }
    // a stop (normalised) on the way up, the arc and value above it in the over-range colour
    // (the crate's `Knob::over`; owner 2026-11-16: Feedback at 100 %, tap Gain at 0 dB)
    if let Some(at) = over {
        k = k.over(at);
    }
    let mut v = n;
    let out = knob::knob(ui, cell, ui.id().with(("knob", id)), &k, &mut v);
    if let Some((m, t, _)) = modu {
        motion_tab::place(ui.ctx(), t, cell);
        motion_tab::route_knob(ui.ctx(), setter, m, t, out.route_touch, out.route_drag, out.route_release, out.route_tap);
        motion_tab::routes_menu(ui, setter, m, t, base, out.menu, out.dial);
    }
    if out.began {
        setter.begin_set_parameter(param);
    }
    if out.changed {
        setter.set_parameter_normalized(param, v);
        *focus = Some(id.to_string());
    }
    if out.ended {
        setter.end_set_parameter(param);
    }
    out.dial
}

/// A BoolParam toggle cell: the button shows the state, the name sits above.
#[allow(clippy::too_many_arguments)]
fn toggle_cell(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &BoolParam, id: &str,
    label: &str, on_txt: &str, off_txt: &str, focus: &mut Option<String>,
) {
    let (ctl, lab, _) = cell_parts(cell, s);
    let b = Rect::from_center_size(ctl.center(), Vec2::new((cell.width() - 6.0 * s).min(96.0 * s), g::CTL_H * s));
    let on = param.value();
    if button(ui, b, if on { on_txt } else { off_txt }, on, id) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !on);
        setter.end_set_parameter(param);
        *focus = Some(id.to_string());
    }
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, INK2());
}

/// An enum selector cell spanning its rect: segmented, name above.
#[allow(clippy::too_many_arguments)]
fn enum_cell<E: Enum + PartialEq + Copy + 'static>(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &EnumParam<E>, id: &str,
    label: &str, names: &[&str], enabled: bool, focus: &mut Option<String>,
) {
    enum_cell_ex(ui, cell, s, setter, param, id, label, names, &[enabled; 8][..names.len()], focus);
}

/// [`enum_cell`] with each option enabled on its own (the name greys when none is).
#[allow(clippy::too_many_arguments)]
fn enum_cell_ex<E: Enum + PartialEq + Copy + 'static>(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &EnumParam<E>, id: &str,
    label: &str, names: &[&str], enabled: &[bool], focus: &mut Option<String>,
) {
    let (ctl, lab, _) = cell_parts(cell, s);
    let b = Rect::from_center_size(ctl.center(), Vec2::new(cell.width() - 8.0 * s, g::CTL_H * s));
    let n = names.len();
    let cur = enum_index(param, n);
    if let Some(i) = seg_ex(ui, b, names, cur, id, enabled) {
        set_norm(setter, param, i as f32 / (n - 1) as f32);
        *focus = Some(id.to_string());
    }
    let alpha = if enabled.contains(&true) { 255 } else { 90 };
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, theme::with_alpha(INK2(), alpha));
}

/// A small IntParam as a segmented choice (Dub's Swirls: Off, 1, 2, 3), name above.
#[allow(clippy::too_many_arguments)]
fn int_seg_cell(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &IntParam, id: &str, label: &str, names: &[&str],
    focus: &mut Option<String>,
) {
    let (ctl, lab, _) = cell_parts(cell, s);
    let b = Rect::from_center_size(ctl.center(), Vec2::new(cell.width() - 8.0 * s, g::CTL_H * s));
    let lo = param.range().unnormalize(0.0);
    let cur = (param.value() - lo).clamp(0, names.len() as i32 - 1) as usize;
    if let Some(i) = seg(ui, b, names, cur, id, true) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, lo + i as i32);
        setter.end_set_parameter(param);
        *focus = Some(id.to_string());
    }
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, INK2());
}

/// An enum as the shared stepper (Saturate's Curve, six choices; owner 2026-11-16): ‹ name ›,
/// name above. A drag up or down steps through the choices while the sound plays, so they can
/// be compared by ear; the arrows step too. As wide as its longest name and its arrows.
#[allow(clippy::too_many_arguments)]
fn enum_stepper_cell<E: Enum + PartialEq + Copy + 'static>(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &EnumParam<E>, id: &str, label: &str, names: &[&str],
    focus: &mut Option<String>,
) {
    let (ctl, lab, _) = cell_parts(cell, s);
    let widest = names.iter().map(|n| softsystems_ui::paint::text_width(ui.painter(), n, 11.0)).fold(0.0, f32::max);
    // the label sits between the two 22 px arrow zones, with a little air
    let w = (widest + (2.0 * 22.0 + 12.0) * s).min(cell.width() - 8.0 * s);
    let b = Rect::from_center_size(ctl.center(), Vec2::new(w, g::CTL_H * s));
    let n = names.len();
    let cur = enum_index(param, n);
    if let controls::Step::By(d) = controls::stepper(ui, b, names[cur], ui.id().with(("step", id)), controls::Ends::of(cur, n), true) {
        let i = (cur as i32 + d).clamp(0, n as i32 - 1) as usize;
        if i != cur {
            set_norm(setter, param, i as f32 / (n - 1) as f32);
            *focus = Some(id.to_string());
        }
    }
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, INK2());
}

/// A group's heading inside a card: small capitals, quiet, so groups read apart
/// without another box.
fn group_head(p: &egui::Painter, at: Pos2, align: Align2, t: &str, s: f32) {
    text(p, at, align, &t.to_uppercase(), 10.5, s, SOFT());
}

/// A hairline between groups.
fn rule(p: &egui::Painter, a: Pos2, b: Pos2, s: f32) {
    p.line_segment([a, b], Stroke::new(1.0 * s, theme::line()));
}

/// A card (the crate's: fill and title), and the inner rect for its cells.
fn card(ui: &egui::Ui, rect: Rect, title: &str, s: f32) -> Rect {
    softsystems_ui::paint::card(ui.painter(), rect, title);
    Rect::from_min_max(rect.min + Vec2::new(14.0, 30.0) * s, rect.max - Vec2::new(14.0, 8.0) * s)
}

/// An outlined diamond: a right-channel tap, apart from a muted (hollow) dot.
fn paint_diamond(p: &egui::Painter, c: Pos2, rad: f32, stroke: Stroke) {
    let pts = vec![c - Vec2::new(0.0, rad), c + Vec2::new(rad, 0.0), c + Vec2::new(0.0, rad), c - Vec2::new(rad, 0.0)];
    p.add(egui::Shape::closed_line(pts, stroke));
}

// ─────────────────────────────────────────────────────────────────────────────
// Header: ☰ · wordmark · channel modes · preset · A/B · light/dark
// ─────────────────────────────────────────────────────────────────────────────

/// The right items' widths, left to right: Clear, Freeze, A/B.
const HEAD_RIGHT: [f32; 3] = [72.0, 96.0, 80.0];
/// The channel modes' width.
const HEAD_TABS: f32 = 220.0;

#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    st: &mut EditorState, beta: Option<&softsystems_ui::beta::BetaView>,
) {
    // the crate's header: ☰, the title and the theme button, then the slots
    let window = Rect::from_min_size(o, Vec2::new(l.w, l.h) * s);
    let h = header::head(ui, window, PRODUCT.name, st.scope.options.is_open(), ui.id().with("sc_head"));
    if h.menu {
        let open = !st.scope.options.is_open();
        st.scope.options.set_open(open);
    }
    if h.theme {
        st.scope.look.toggle_dark();
        st.scope.save_look();
    }
    let badge = st.scope.beta.badge(ui, h.badge_at(), beta, ui.id().with("sc_beta"));
    let after = h.title.right().max(badge.right());
    let sl = header::slots(&h, after, &header::Want { left: HEAD_TABS, right: &HEAD_RIGHT, preset: header::PRESET_W });

    // channel modes
    let cm = enum_index(&params.channel_mode, 3);
    if let Some(i) = seg(ui, sl.left, &["Stereo", "M/S", "Mono"], cm, "sc_chan", true) {
        set_norm(setter, &params.channel_mode, i as f32 / 2.0);
    }

    // preset box: ‹ name ›
    preset_sheet::preset_box(ui, sl.preset, params, setter, st);

    // Clear (v2.43): empty the delay now — the panic button.
    // the header's chips are bare (owner, 2026-11-17: "Bare, no outline")
    if header::chip(ui, sl.right[0], "Clear", false, ui.id().with(("btn", "sc_clear"))) {
        params.clear_request.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Freeze: a performance switch, so it sits up here in reach whatever the
    // lane shows. Ink when it holds.
    let frozen = params.freeze.value();
    if header::chip(ui, sl.right[1], "Freeze", frozen, ui.id().with(("btn", "sc_freeze"))) {
        setter.begin_set_parameter(&params.freeze);
        setter.set_parameter(&params.freeze, !frozen);
        setter.end_set_parameter(&params.freeze);
    }
    // A / B
    if let Some(i) = seg(ui, sl.right[2], &["A", "B"], st.ab_active as usize, "sc_ab", true) {
        crate::gui::ab_switch(st, params, setter, i as u8);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Picture: the tap field (X = time) or the bars (X = tap order); Y is picked
// ─────────────────────────────────────────────────────────────────────────────

/// What the picture's vertical axis shows: one per-tap parameter. Type is the
/// filter's choice of three (0 LP · 1 BP · 2 HP), shown as three rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    Pan,
    Gain,
    Cutoff,
    Reso,
    Type,
    Slope,
    Detune,
}

impl Axis {
    const ALL: [Axis; 7] = [Axis::Pan, Axis::Gain, Axis::Cutoff, Axis::Reso, Axis::Type, Axis::Slope, Axis::Detune];

    fn name(self) -> &'static str {
        match self {
            Axis::Pan => "Pan",
            Axis::Gain => "Gain",
            Axis::Cutoff => "Cutoff",
            Axis::Reso => "Reso",
            Axis::Type => "Type",
            Axis::Slope => "Slope",
            Axis::Detune => "Detune",
        }
    }

    /// The tap's float param on this axis; Type and Slope are enums (see `get`, `set`).
    fn float(self, tap: &TapParams) -> Option<&FloatParam> {
        match self {
            Axis::Pan => Some(&tap.pan),
            Axis::Gain => Some(&tap.gain_db),
            Axis::Cutoff => Some(&tap.filter_cutoff),
            Axis::Reso => Some(&tap.filter_q),
            Axis::Detune => Some(&tap.detune),
            Axis::Type | Axis::Slope => None,
        }
    }

    /// The tap's value on this axis (Type and Slope as 0 · 1 · 2).
    fn get(self, tap: &TapParams) -> f32 {
        match (self.float(tap), self) {
            (Some(p), _) => p.value(),
            (None, Axis::Slope) => enum_index(&tap.filter_slope, SLOPES) as f32,
            (None, _) => enum_index(&tap.filter_type, 3) as f32,
        }
    }

    /// A whole gesture: set the tap's value on this axis.
    fn set(self, setter: &ParamSetter, tap: &TapParams, v: f32) {
        self.begin(setter, tap);
        self.put(setter, tap, v);
        self.end(setter, tap);
    }

    fn begin(self, setter: &ParamSetter, tap: &TapParams) {
        match (self.float(tap), self) {
            (Some(p), _) => setter.begin_set_parameter(p),
            (None, Axis::Slope) => setter.begin_set_parameter(&tap.filter_slope),
            (None, _) => setter.begin_set_parameter(&tap.filter_type),
        }
    }

    /// Set within a gesture `begin` opened.
    fn put(self, setter: &ParamSetter, tap: &TapParams, v: f32) {
        match (self.float(tap), self) {
            (Some(p), _) => setter.set_parameter(p, v),
            (None, Axis::Slope) => setter.set_parameter(&tap.filter_slope, slope_of(v)),
            (None, _) => setter.set_parameter(&tap.filter_type, type_of(v)),
        }
    }

    fn end(self, setter: &ParamSetter, tap: &TapParams) {
        match (self.float(tap), self) {
            (Some(p), _) => setter.end_set_parameter(p),
            (None, Axis::Slope) => setter.end_set_parameter(&tap.filter_slope),
            (None, _) => setter.end_set_parameter(&tap.filter_type),
        }
    }

    /// A drag's value through [`gesture_set`] (one gesture per drag).
    fn gesture(self, ui: &egui::Ui, key: Id, setter: &ParamSetter, tap: &TapParams, d: &Drag, v: Option<f32>) {
        match (self.float(tap), self) {
            (Some(p), _) => gesture_set(ui, key, setter, p, d, v),
            (None, Axis::Slope) => gesture_set(ui, key, setter, &tap.filter_slope, d, v.map(slope_of)),
            (None, _) => gesture_set(ui, key, setter, &tap.filter_type, d, v.map(type_of)),
        }
    }

    /// The value a tap starts with (Reset, double-click).
    fn neutral(self) -> f32 {
        match self {
            Axis::Pan => 0.0,
            Axis::Gain => 0.0,
            Axis::Cutoff => 20_000.0,
            Axis::Reso => 0.707,
            Axis::Type | Axis::Slope | Axis::Detune => 0.0,
        }
    }

    /// Param id of tap `i` on this axis (nested ids carry the 1-based tap number).
    fn id(self, i: usize) -> String {
        let base = match self {
            Axis::Pan => "pan",
            Axis::Gain => "gain_db",
            Axis::Cutoff => "tap_filter_cutoff",
            Axis::Reso => "tap_filter_q",
            Axis::Type => "tap_filter_type",
            Axis::Slope => "tap_filter_slope",
            Axis::Detune => "tap_detune",
        };
        format!("{base}_{}", i + 1)
    }

    /// A value → 0 (top) … 1 (bottom). Pan has L / Mid at the top and gain the
    /// DAW-fader skew (0 dB at 75 % height), both as in v3; cutoff and Q are
    /// log; Type has HP on top, LP at the bottom; Slope 48 dB on top, 6 at the bottom.
    fn norm(self, v: f32) -> f32 {
        let n = match self {
            Axis::Pan => (v + 1.0) * 0.5,
            Axis::Gain => 1.0 - gain_db_to_pos(v),
            Axis::Cutoff => 1.0 - (v / 20.0).max(1.0).ln() / 1000f32.ln(),
            Axis::Reso => 1.0 - (v / 0.1).max(1.0).ln() / 120f32.ln(),
            Axis::Type => 1.0 - v / 2.0,
            Axis::Slope => 1.0 - v / (SLOPES - 1) as f32,
            Axis::Detune => 0.5 - v / 200.0,
        };
        n.clamp(0.0, 1.0)
    }

    fn value(self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        match self {
            Axis::Pan => n * 2.0 - 1.0,
            Axis::Gain => pos_to_gain_db(1.0 - n),
            Axis::Cutoff => 20.0 * 1000f32.powf(1.0 - n),
            Axis::Reso => 0.1 * 120f32.powf(1.0 - n),
            Axis::Type => ((1.0 - n) * 2.0).round(),
            Axis::Slope => ((1.0 - n) * (SLOPES - 1) as f32).round(),
            Axis::Detune => ((0.5 - n) * 2000.0).round() / 10.0,
        }
    }

    /// The pattern menu's slider runs low to high left to right, Pan L to R:
    /// a slider position → the field's 0 (top) … 1 (bottom).
    fn u_norm(self, u: f32) -> f32 {
        if self == Axis::Pan { u } else { 1.0 - u }
    }

    /// The range a shape spreads over until the pins are moved: L 70 to R 70,
    /// −24 to 0 dB, 500 Hz to 20 kHz, Q 0.7 to 6, LP to HP, 6 to 48 dB.
    fn default_range(self) -> [f32; 2] {
        let u = |v: f32| self.u_norm(self.norm(v));
        match self {
            Axis::Pan => [u(-0.7), u(0.7)],
            Axis::Gain => [u(-24.0), u(0.0)],
            Axis::Cutoff => [u(500.0), u(20_000.0)],
            Axis::Reso => [u(0.7), u(6.0)],
            Axis::Type | Axis::Slope => [0.0, 1.0],
            Axis::Detune => [u(-50.0), u(50.0)],
        }
    }

    fn text(self, v: f32, lb: Labels) -> String {
        match self {
            Axis::Pan => pan_text(v, lb.ch),
            Axis::Gain => db_text(v),
            Axis::Cutoff => hz_text(v),
            Axis::Reso => format!("Q {v:.1}"),
            Axis::Type => type_name(v).to_string(),
            Axis::Slope => format!("{} dB/oct", slope_name(v)),
            Axis::Detune => detune_text(v, lb.det_range),
        }
    }

    /// The short form above a bar: no unit for gain, the filter type for cutoff.
    fn short(self, tap: &TapParams, lb: Labels) -> String {
        let v = self.get(tap);
        match self {
            Axis::Pan => pan_text(v, lb.ch),
            Axis::Gain => db_text(v).trim_end_matches(" dB").to_string(),
            Axis::Cutoff => {
                let ty = type_name(enum_index(&tap.filter_type, 3) as f32);
                if v >= 1000.0 { format!("{ty} {:.1}k", v / 1000.0) } else { format!("{ty} {v:.0}") }
            }
            Axis::Reso => format!("{v:.1}"),
            Axis::Type => type_name(v).to_string(),
            Axis::Slope => slope_name(v).to_string(),
            Axis::Detune => unitless(detune_text(v, lb.det_range)),
        }
    }

    /// Gridlines as (norm, label).
    fn grid(self, lb: Labels) -> Vec<(f32, String)> {
        let at = |pts: &[(f32, &str)]| -> Vec<(f32, String)> {
            pts.iter().map(|&(v, t)| (self.norm(v), t.to_string())).collect()
        };
        match self {
            Axis::Pan if lb.ch == ChannelMode::MidSide => at(&[(-1.0, "Mid"), (0.0, "M·S"), (1.0, "Side")]),
            Axis::Pan => at(&[(-1.0, "L"), (0.0, "C"), (1.0, "R")]),
            Axis::Gain => at(&[(6.0, "+6"), (0.0, "0 dB"), (-12.0, "−12"), (-24.0, "−24"), (-48.0, "−48")]),
            Axis::Cutoff => at(&[(20000.0, "20k"), (5000.0, "5k"), (1000.0, "1k"), (200.0, "200"), (50.0, "50")]),
            Axis::Reso => at(&[(12.0, "Q 12"), (4.0, "4"), (1.2, "1.2"), (0.4, "0.4"), (0.1, "0.1")]),
            Axis::Type => at(&[(2.0, "HP"), (1.0, "BP"), (0.0, "LP")]),
            Axis::Slope => at(&[(7.0, "48 dB"), (5.0, "36"), (3.0, "24"), (1.0, "12"), (0.0, "6")]),
            // in cents at a one-semitone range (±100 ct), else in semitones
            Axis::Detune => [100.0, 50.0, 0.0, -50.0, -100.0]
                .into_iter()
                .map(|v| {
                    let t = if lb.det_range < 2.0 {
                        let c = (v * lb.det_range).round() as i32;
                        let sign = if c > 0 { "+" } else if c < 0 { "−" } else { "" };
                        format!("{sign}{} ct", c.abs())
                    } else {
                        detune_text(v, lb.det_range)
                    };
                    (self.norm(v), if v == 100.0 { t } else { unitless(t) })
                })
                .collect(),
        }
    }
}

/// The filter type for a Type value (0 LP · 1 BP · 2 HP).
fn type_of(v: f32) -> FilterType {
    FilterType::from_index(v.round().clamp(0.0, 2.0) as usize)
}

fn type_name(v: f32) -> &'static str {
    ["LP", "BP", "HP"][v.round().clamp(0.0, 2.0) as usize]
}

/// Slope's steps: 6 to 48 dB/oct in 6 dB steps (owner, 2026-11-16).
const SLOPES: usize = 8;
const SLOPE_DB: [&str; SLOPES] = ["6 dB", "12 dB", "18 dB", "24 dB", "30 dB", "36 dB", "42 dB", "48 dB"];
const SLOPE_NAMES: [&str; SLOPES] = ["6", "12", "18", "24", "30", "36", "42", "48"];

/// The filter slope for a Slope value (its step, 0 = 6 dB … 7 = 48 dB).
fn slope_of(v: f32) -> FilterSlope {
    FilterSlope::from_index(v.round().clamp(0.0, (SLOPES - 1) as f32) as usize)
}

/// A tap's Detune (% of the range, `range` semitones) as heard: cents
/// under a semitone, "+25 ct", "0 ct", "−50 ct"; semitones from one up,
/// "+1 st", "−7.5 st", "+12 st".
fn detune_text(pct: f32, range: f32) -> String {
    let c = (pct * range).round() as i32;
    let sign = if c > 0 { "+" } else if c < 0 { "−" } else { "" };
    let a = c.abs();
    if a < 100 {
        format!("{sign}{a} ct")
    } else {
        let st = format!("{:.2}", a as f32 / 100.0);
        format!("{sign}{} st", st.trim_end_matches('0').trim_end_matches('.'))
    }
}

/// A label without its unit, for the bars and the gridlines under the top one.
fn unitless(t: String) -> String {
    t.trim_end_matches(" ct").trim_end_matches(" st").to_string()
}

/// What an axis's labels need besides the value.
#[derive(Clone, Copy)]
struct Labels {
    ch: ChannelMode,
    /// Detune Range, semitones.
    det_range: f32,
}

impl Labels {
    fn of(params: &AfterechoParams) -> Labels {
        Labels { ch: params.channel_mode.value(), det_range: params.detune_range.value() as f32 }
    }
}

fn slope_name(v: f32) -> &'static str {
    SLOPE_NAMES[v.round().clamp(0.0, (SLOPES - 1) as f32) as usize]
}

fn pan_text(v: f32, ch: ChannelMode) -> String {
    let n = (v.abs() * 100.0).round() as i32;
    let ms = ch == ChannelMode::MidSide;
    match (n < 2, ms, v < 0.0) {
        (true, true, _) => "M·S".into(),
        (true, false, _) => "C".into(),
        (false, true, true) => format!("Mid {n}"),
        (false, true, false) => format!("Side {n}"),
        (false, false, true) => format!("L {n}"),
        (false, false, false) => format!("R {n}"),
    }
}

fn db_text(v: f32) -> String {
    let sign = if v < -0.05 { "−" } else if v > 0.05 { "+" } else { "" };
    format!("{sign}{:.1} dB", v.abs())
}

fn hz_text(v: f32) -> String {
    if v >= 1000.0 { format!("{:.1} kHz", v / 1000.0) } else { format!("{v:.0} Hz") }
}

fn secs_text(t: f32) -> String {
    if t < 1.0 { format!("{:.0} ms", t * 1000.0) } else { format!("{t:.2} s") }
}

/// Taps that are live; the rest are hidden, as in v3. Euclid places one per
/// step at most; any more are silent and hidden too.
fn n_active(params: &AfterechoParams) -> usize {
    params.taps_heard()
}

/// Tap `i` of `n`: the accent, lighter towards the last tap (v3's ramp).
fn tap_col(i: usize, n: usize) -> Color32 {
    let t = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
    let c = G();
    // later taps step away from the page: lighter on dark, darker on light,
    // so every tap keeps 3:1 on the page, the cards and the wells
    let (to, k) = if theme::is_dark() { (Color32::WHITE, 0.5 * t) } else { (INK(), 0.15 + 0.35 * t) };
    let f = |v: u8, w: u8| (v as f32 + (w as f32 - v as f32) * k).round() as u8;
    Color32::from_rgb(f(c.r(), to.r()), f(c.g(), to.g()), f(c.b(), to.b()))
}

/// A data colour as text on `ground`: itself when it reads at 4.5:1 there,
/// else ink (the shared contrast rule, migration step 11).
fn readable(col: Color32, ground: Color32) -> Color32 {
    if softsystems_ui::tokens::contrast(col, ground) >= 4.5 { col } else { INK() }
}

/// A rounded label (a dot's value, the time on an axis, a handle's readout),
/// slid sideways to stay inside `bounds`.
#[allow(clippy::too_many_arguments)]
fn pill_in(p: &egui::Painter, at: Pos2, align: Align2, t: &str, s: f32, fill: Color32, col: Color32, bounds: Rect) {
    let galley = p.layout_no_wrap(t.to_string(), FontId::proportional(fs(12.0, s)), col);
    let mut rect = align.anchor_size(at, galley.size() + Vec2::new(20.0, 8.0) * s);
    let dx = (bounds.left() - rect.left()).max(0.0) + (bounds.right() - rect.right()).min(0.0);
    rect = rect.translate(Vec2::new(dx, 0.0));
    p.rect_filled(rect, rect.height() * 0.5, fill);
    p.galley(rect.min + Vec2::new(10.0, 4.0) * s, galley, col);
}

/// One frame of a drag surface, the same on desktop (the pointer) and on touch
/// (this surface's own fingers, so several taps can move at once).
#[derive(Default)]
struct Drag {
    started: bool,
    delta: Vec2,
    /// A press is down on the surface (desktop) or a finger owns it (touch).
    pressing: bool,
    /// The press or finger let go this frame (a click counts).
    released: bool,
    clicked: bool,
    double: bool,
    pos: Option<Pos2>,
}

fn drag_at(ui: &egui::Ui, id: Id, hit: Rect) -> Drag {
    let resp = ui.interact(hit, id, Sense::click_and_drag());
    if theme::touch() {
        let t = epo::touch_interact(ui, id, hit);
        let key = id.with("held");
        let was: bool = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(false);
        ui.ctx().data_mut(|d| d.insert_temp(key, t.active));
        Drag {
            started: t.started,
            delta: t.delta,
            pressing: t.active,
            released: was && !t.active,
            clicked: t.tapped,
            double: false,
            pos: t.pos,
        }
    } else {
        Drag {
            started: resp.drag_started(),
            delta: if resp.dragged() { resp.drag_delta() } else { Vec2::ZERO },
            pressing: resp.is_pointer_button_down_on() || resp.dragged(),
            released: resp.drag_stopped() || resp.clicked(),
            clicked: resp.clicked(),
            double: resp.double_clicked(),
            pos: resp.interact_pointer_pos(),
        }
    }
}

/// Set `p` during a drag inside one host gesture: begin on the first change,
/// end on release — a press that never moves leaves no open gesture.
fn gesture_set<P: Param>(ui: &egui::Ui, key: Id, setter: &ParamSetter, p: &P, d: &Drag, v: Option<P::Plain>) {
    let begun: bool = ui.ctx().data(|m| m.get_temp(key)).unwrap_or(false);
    let set = v.is_some();
    if let Some(v) = v {
        if !begun {
            setter.begin_set_parameter(p);
        }
        setter.set_parameter(p, v);
    }
    if d.released && (begun || set) {
        setter.end_set_parameter(p);
        ui.ctx().data_mut(|m| m.insert_temp(key, false));
    } else if set && !begun {
        ui.ctx().data_mut(|m| m.insert_temp(key, true));
    }
}

/// Snapped `time_offset` for a dragged tap (Snap on): the tap's whole position
/// (topology position + offset, in cluster-span units) goes to the nearest
/// 1/(NUM_TAPS × grid), i.e. its time to a multiple of the base time over
/// Snap's grid (v2.43; 1 is v3's snap).
fn snap_offset(cur_time_secs: f32, cur_off: f32, raw_off: f32, span_secs: f32, grid: f32) -> f32 {
    let pos = cur_time_secs / span_secs.max(1e-6) - cur_off;
    let steps = NUM_TAPS as f32 * grid.max(1.0);
    let snapped = ((pos + raw_off) * steps).round() / steps;
    snapped - pos
}

/// Time ticks across `plot` for `range` seconds — beats in Sync, ms in Free
/// (v3's ladders) — labelled under the plot; `lines` draws the gridlines too.
fn time_grid(p: &egui::Painter, plot: Rect, range: f32, tempo: f32, is_sync: bool, s: f32, lines: bool) {
    let (total, major, minor) = if is_sync {
        let total = (range / (60.0 / tempo.max(1.0))).max(1e-3);
        let (ma, mi) = match total {
            t if t <= 0.5 => (0.125, 0.0625),
            t if t <= 1.0 => (0.25, 0.125),
            t if t <= 2.0 => (0.5, 0.25),
            t if t <= 4.0 => (1.0, 0.5),
            t if t <= 8.0 => (2.0, 1.0),
            t if t <= 16.0 => (4.0, 2.0),
            t if t <= 32.0 => (8.0, 4.0),
            t if t <= 64.0 => (16.0, 8.0),
            _ => (32.0, 16.0),
        };
        (total, ma, mi)
    } else {
        let total = (range * 1000.0).max(1e-3);
        let (ma, mi) = match total {
            t if t <= 25.0 => (5.0, 2.5),
            t if t <= 60.0 => (10.0, 5.0),
            t if t <= 125.0 => (20.0, 10.0),
            t if t <= 250.0 => (50.0, 25.0),
            t if t <= 500.0 => (100.0, 50.0),
            t if t <= 1000.0 => (200.0, 100.0),
            t if t <= 2000.0 => (500.0, 250.0),
            t if t <= 5000.0 => (1000.0, 500.0),
            t if t <= 10000.0 => (2000.0, 1000.0),
            _ => (4000.0, 2000.0),
        };
        (total, ma, mi)
    };
    let mut v = 0.0f32;
    while v <= total + minor * 0.01 {
        let x = plot.left() + v / total * plot.width();
        let is_major = (v / major - (v / major).round()).abs() < 0.02;
        if lines {
            let (w, col) = if is_major { (1.0, theme::line2()) } else { (0.5, theme::line()) };
            p.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], Stroke::new(w * s, col));
        }
        if is_major {
            let label = if v < 1e-4 {
                "0".to_string()
            } else if is_sync {
                let b = (v * 100.0).round() / 100.0;
                if b == b.floor() { format!("{b:.0}") } else { format!("{b}") }
            } else if v < 1000.0 {
                format!("{v:.0} ms")
            } else {
                format!("{:.1} s", v / 1000.0)
            };
            text(p, Pos2::new(x, plot.bottom() + 12.0 * s), Align2::CENTER_CENTER, &label, 10.5, s, SOFT());
        }
        v += minor;
    }
    if is_sync {
        text(p, Pos2::new(plot.right(), plot.bottom() + 24.0 * s), Align2::RIGHT_CENTER, "beats", 10.0, s, SOFT());
    }
}

/// The picked axis's gridlines, labelled left of the plot.
fn axis_grid(p: &egui::Painter, plot: Rect, axis: Axis, lb: Labels, s: f32) {
    for (n, t) in axis.grid(lb) {
        let y = plot.top() + n * plot.height();
        p.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0 * s, theme::line()));
        text(p, Pos2::new(plot.left() - 12.0 * s, y), Align2::RIGHT_CENTER, &t, 10.5, s, SOFT());
    }
}

#[allow(clippy::too_many_arguments)]
fn picture(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    sc: &mut ScopeState, tempo: f32,
) {
    let (x, y, w, h) = l.picture;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    softsystems_ui::paint::card(&p, rect, "");
    // The title opens the Taps menu (pattern tools; values only, so not in
    // the Time view); the chevron says so.
    let tb = r(o, s, x + 10.0, y + 10.0, 74.0, 28.0);
    if sc.time_view {
        sc.pat_open = false;
    }
    let tapped = controls::dropdown(ui, tb, "Taps", sc.pat_open, ui.id().with("sc_taps_menu"));
    if sc.time_view {
        // greyed: the pattern tools set values, which the Time view doesn't show
        p.rect_filled(tb, g::R_CTL * s, theme::with_alpha(PANEL(), 150));
    }
    if !sc.time_view && tapped {
        sc.pat_open = !sc.pat_open;
    }
    sc.pat_btn = Some(tb);
    let hint = match (sc.time_view, sc.view) {
        (true, _) => "drag a dot left or right",
        (false, 0) => "drag a dot up or down",
        _ => "drag across the bars",
    };
    text(&p, rect.min + Vec2::new(96.0, 24.0) * s, Align2::LEFT_CENTER, hint, 12.0, s, SOFT());

    // header, right to left: Field / Bars (the value views'), then in the
    // Time view Snap and its grid, so the view switch stays put
    let right = x + w - 12.0;
    if let Some(i) = seg(ui, r(o, s, right - 112.0, y + 10.0, 112.0, 28.0), &["Field", "Bars"], sc.view, "sc_view", !sc.time_view) {
        sc.view = i;
    }
    if sc.time_view {
        let xr = o.x + (right - 122.0) * s;
        rule(ui.painter(), Pos2::new(xr, o.y + (y + 14.0) * s), Pos2::new(xr, o.y + (y + 34.0) * s), s);
        let snap = params.tap_snap.value();
        if button(ui, r(o, s, right - 188.0, y + 10.0, 56.0, 28.0), "Snap", snap, "sc_snap") {
            setter.begin_set_parameter(&params.tap_snap);
            setter.set_parameter(&params.tap_snap, !snap);
            setter.end_set_parameter(&params.tap_snap);
        }
        let grid = (params.tap_grid.value() - 1).clamp(0, 3) as usize;
        if let Some(i) = seg(ui, r(o, s, right - 340.0, y + 10.0, 144.0, 28.0), &["1", "1/2", "1/3", "1/4"], grid, "sc_grid", snap) {
            setter.begin_set_parameter(&params.tap_grid);
            setter.set_parameter(&params.tap_grid, i as i32 + 1);
            setter.end_set_parameter(&params.tap_grid);
        }
    }
    // Y, down the left edge beside the axis it picks. Mono has no pan, so its
    // picture starts on gain. A new axis starts without a shape to re-spread.
    // Time first, then the values.
    let mono = params.channel_mode.value() == ChannelMode::Mono;
    let axis_i = if mono && sc.y_axis == 0 { 1 } else { sc.y_axis };
    let mut names = vec!["Time"];
    names.extend(Axis::ALL.map(Axis::name));
    let cur = if sc.time_view { 0 } else { axis_i + 1 };
    let live = [true, !mono, true, true, true, true, true, true];
    // every axis in view, stacked, so a glance shows them all (owner, 2026-11-07)
    let list = r(o, s, x + 14.0, y + 56.0, 72.0, 26.0 * names.len() as f32);
    let picked = controls::seg_v(ui, list, &names, cur, ui.id().with(("seg", "sc_y")), &live);
    if let Some(i) = picked.filter(|&i| i != cur) {
        sc.time_view = i == 0;
        if i > 0 {
            sc.y_axis = i - 1;
        }
        sc.pat_shape = None;
    }

    let axis = Axis::ALL[axis_i];
    if std::mem::take(&mut sc.over_demo) {
        for (p, v) in [(&params.feedback, 1.04), (&params.taps[0].gain_db, 3.0)] {
            setter.begin_set_parameter(p);
            setter.set_parameter(p, v);
            setter.end_set_parameter(p);
        }
    }
    if std::mem::take(&mut sc.sync_demo) {
        set_norm(setter, &params.sync_mode, 1.0);
    }
    if std::mem::take(&mut sc.swing_demo) {
        setter.begin_set_parameter(&params.swing);
        setter.set_parameter(&params.swing, 0.7);
        setter.end_set_parameter(&params.swing);
    }
    if std::mem::take(&mut sc.euclid_demo) {
        set_norm(setter, &params.spacing_mode, 1.0);
        setter.begin_set_parameter(&params.swing);
        setter.set_parameter(&params.swing, 0.7);
        setter.end_set_parameter(&params.swing);
        setter.begin_set_parameter(&params.num_taps_active);
        setter.set_parameter(&params.num_taps_active, 5);
        setter.end_set_parameter(&params.num_taps_active);
    }
    if std::mem::take(&mut sc.pat_demo) {
        sc.pat_shape = Some(PatShape::RampDown);
        pattern_apply(params, setter, sc, axis_i);
    }
    let plot = Rect::from_min_max(rect.min + Vec2::new(128.0, 56.0) * s, rect.max - Vec2::new(24.0, 34.0) * s);
    if sc.time_view || sc.view == 0 {
        let viz = crate::gui::compute_viz_data_ex(params, tempo, 1);
        field(ui, plot, s, params, setter, sc, &viz, axis, tempo);
    } else {
        bars(ui, plot, s, params, setter, sc, axis);
    }
}

/// The tap field: X = time on v3's tight taps-only range, Y = the picked axis
/// (↕ moves a dot's value), or in the Time view a row per tap (↔ moves its
/// time offset, onto Snap's grid unless Shift is held). Click a dot: select
/// it and open the Tap tab. Double-click resets what the view moves.
#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui, plot: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    sc: &mut ScopeState, viz: &VizData, axis: Axis, tempo: f32,
) {
    let p = ui.painter().clone();
    let is_sync = params.sync_mode.value() == SyncMode::Sync;
    // The view fits the pattern; while a dot is dragged it holds still, so the
    // dot stays under the pointer even as the pattern grows.
    let fit = viz.grid_range_secs_taps_only.max(1e-4);
    let held_id = ui.id().with("sc_field_range");
    let range = ui.ctx().data(|m| m.get_temp::<f32>(held_id)).unwrap_or(fit);
    let x_at = |secs: f32| plot.left() + (secs / range).min(1.0) * plot.width();
    let span = viz.span_l_secs.max(1e-4);
    let time_view = sc.time_view;
    let n = n_active(params);
    let snap = params.tap_snap.value();
    let grid = params.tap_grid.value().max(1) as f32;
    time_grid(&p, plot, range, tempo, is_sync, s, !time_view);
    if time_view {
        // Snap's grid, then a row per tap
        if snap {
            let stepw = span / (NUM_TAPS as f32 * grid);
            let mut k = 0;
            while (k as f32 * stepw) <= range && k < 4096 {
                let xg = x_at(k as f32 * stepw);
                let whole = k % grid as usize == 0;
                let col = if whole { theme::line2() } else { theme::line() };
                p.line_segment([Pos2::new(xg, plot.top()), Pos2::new(xg, plot.bottom())], Stroke::new(if whole { 1.0 } else { 0.5 } * s, col));
                k += 1;
            }
        }
        for i in 0..=n {
            let yr = plot.top() + i as f32 / n as f32 * plot.height();
            p.line_segment([Pos2::new(plot.left(), yr), Pos2::new(plot.right(), yr)], Stroke::new(0.5 * s, theme::line()));
        }
    } else {
        axis_grid(&p, plot, axis, Labels::of(params), s);
    }
    // a dot's height: its value, or its own row in the Time view
    let y_of = |i: usize, v: f32| {
        if time_view { plot.top() + (i as f32 + 0.5) / n as f32 * plot.height() } else { plot.top() + axis.norm(v) * plot.height() }
    };

    // A click on the background deselects; registered first so dots win overlaps.
    if ui.interact(plot, ui.id().with("sc_field_bg"), Sense::click()).clicked() {
        sc.deselect();
    }
    // Time L/R unlinked: the right channel's taps land at their own times.
    // They show as diamonds joined to their dot, under the dots, and don't
    // drag (L and R share each tap's offset).
    if !params.time_linked.value() {
        for i in 0..n {
            let tap = &params.taps[i];
            let y = y_of(i, axis.get(tap));
            let xl = x_at(viz.times_l_grid[i] * fit);
            let xr = x_at(viz.times_r[i] * viz.grid_range_secs);
            if (xr - xl).abs() < 1.0 {
                continue;
            }
            let (col, a) = (tap_col(i, n), if tap.active.value() { 1.0 } else { 0.45 });
            p.line_segment([Pos2::new(xl, y), Pos2::new(xr, y)], Stroke::new(1.0 * s, theme::with_alpha(col, (70.0 * a) as u8)));
            paint_diamond(&p, Pos2::new(xr, y), 5.5 * s, Stroke::new(1.5 * s, theme::with_alpha(col, (220.0 * a) as u8)));
        }
        let key = text(&p, Pos2::new(plot.right() - 10.0 * s, plot.top() + 12.0 * s), Align2::RIGHT_CENTER, "R taps", 11.0, s, SOFT());
        paint_diamond(&p, Pos2::new(key.left() - 10.0 * s, key.center().y), 4.5 * s, Stroke::new(1.3 * s, SOFT()));
    }
    let lock = !time_view;
    // Shift: off the grid, for fine moves
    let snap_now = snap && !ui.input(|i| i.modifiers.shift);
    let mut selected = None;
    let mut pressing = false;
    for i in 0..n {
        let tap = &params.taps[i];
        let v = axis.get(tap);
        let secs = viz.times_l_grid[i] * fit;
        let c = Pos2::new(x_at(secs), y_of(i, v));
        let hit = if theme::touch() { theme::TOUCH_MIN_HIT } else { 28.0 } * s;
        let id = ui.id().with(("sc_tap", i));
        let d = drag_at(ui, id, Rect::from_center_size(c, Vec2::splat(hit)));
        pressing |= d.pressing;
        if d.started {
            ui.ctx().data_mut(|m| m.insert_temp(held_id, range));
        }
        let virt_id = id.with("virt");
        // the value's own accumulator: Type's three rows would hold a dot still
        let yvirt_id = id.with("yvirt");
        // Whether the dot was already selected when the press began: a finger
        // starts a drag on touch-down, which selects, so the tap that follows
        // must read the state from before the press or it undoes its own select.
        let was_id = id.with("was_sel");
        if d.started {
            ui.ctx().data_mut(|m| m.insert_temp(was_id, sc.sel == Some(i)));
            sc.sel = Some(i);
            // raw (unsnapped) offset accumulator: quantizing the running value
            // directly makes the dot sticky (v3)
            ui.ctx().data_mut(|m| m.insert_temp(virt_id, tap.time_offset.value()));
            ui.ctx().data_mut(|m| m.insert_temp(yvirt_id, axis.norm(v)));
        }
        let (mut off, mut val) = (None, None);
        if d.delta != Vec2::ZERO {
            if !lock {
                let mut virt: f32 = ui.ctx().data(|m| m.get_temp(virt_id)).unwrap_or_else(|| tap.time_offset.value());
                // px → seconds on this range → fraction of the cluster span
                virt = (virt + d.delta.x / plot.width() * range / span).clamp(-0.5, 0.5);
                ui.ctx().data_mut(|m| m.insert_temp(virt_id, virt));
                let o = if snap_now { snap_offset(secs, tap.time_offset.value(), virt, span, grid) } else { virt };
                off = Some(o.clamp(-0.5, 0.5));
            } else {
                let yv: f32 = ui.ctx().data(|m| m.get_temp(yvirt_id)).unwrap_or_else(|| axis.norm(v));
                let yv = (yv + d.delta.y / plot.height()).clamp(0.0, 1.0);
                ui.ctx().data_mut(|m| m.insert_temp(yvirt_id, yv));
                val = Some(axis.value(yv));
            }
            sc.focus = Some(if lock { axis.id(i) } else { format!("time_offset_{}", i + 1) });
        }
        gesture_set(ui, id.with("off"), setter, &tap.time_offset, &d, off);
        axis.gesture(ui, id.with("val"), setter, tap, &d, val);
        // A double-click's second click resets; it doesn't toggle the selection.
        if d.clicked && !d.double {
            let was = ui.ctx().data(|m| m.get_temp(was_id)).unwrap_or(sc.sel == Some(i));
            if was {
                sc.deselect();
            } else {
                sc.open_tap(i);
            }
            sc.focus = Some(axis.id(i));
        }
        if d.released {
            ui.ctx().data_mut(|m| m.remove::<bool>(was_id));
        }
        if d.double {
            if lock {
                axis.set(setter, tap, axis.neutral());
            } else {
                setter.begin_set_parameter(&tap.time_offset);
                setter.set_parameter(&tap.time_offset, 0.0);
                setter.end_set_parameter(&tap.time_offset);
            }
        }

        let col = tap_col(i, n);
        let active = tap.active.value();
        let rad = if time_view || axis == Axis::Pan {
            (4.0 + 7.0 * gain_db_to_pos(tap.gain_db.value())) * s // size = level
        } else {
            5.0 * s
        };
        // Motion: the dot is where Motion has the tap now; its set value
        // (what a drag moves) stays as a ring.
        let cm = if time_view { c } else { Pos2::new(c.x, y_of(i, moved_value(params, axis, i, v))) };
        if (cm.y - c.y).abs() > 0.5 * s {
            p.circle_stroke(c, rad + 1.5 * s, Stroke::new(1.0 * s, theme::with_alpha(col, 120)));
            p.line_segment([c, cm], Stroke::new(1.5 * s, theme::with_alpha(col, 90)));
        }
        if !time_view && axis == Axis::Gain {
            let stem = theme::with_alpha(col, if active { 160 } else { 70 });
            p.line_segment([Pos2::new(cm.x, cm.y + rad), Pos2::new(cm.x, plot.bottom())], Stroke::new(1.5 * s, stem));
        }
        if active {
            p.circle_filled(cm, rad, col);
        } else {
            p.circle_stroke(cm, rad, Stroke::new(1.5 * s, col));
        }
        if sc.sel == Some(i) {
            selected = Some((i, c, rad));
        }
    }

    // the selected tap on top: ring, crosshair, its value and its time
    if let Some((i, c, rad)) = selected {
        p.circle_stroke(c, rad + 3.5 * s, Stroke::new(1.5 * s, INK()));
        let hair = Stroke::new(1.0 * s, theme::with_alpha(INK(), 60));
        p.line_segment([Pos2::new(c.x, plot.top()), Pos2::new(c.x, plot.bottom())], hair);
        p.line_segment([Pos2::new(plot.left(), c.y), Pos2::new(plot.right(), c.y)], hair);
        // The value goes above the dot (below near the top edge), not beside it,
        // so it never covers the neighbouring taps in a tight cluster.
        if !time_view {
            let ytxt = format!("{} {}  ↕", axis.name(), axis.text(axis.get(&params.taps[i]), Labels::of(params)));
            let (at, align) = if c.y - rad - 34.0 * s > plot.top() {
                (Pos2::new(c.x, c.y - rad - 8.0 * s), Align2::CENTER_BOTTOM)
            } else {
                (Pos2::new(c.x, c.y + rad + 8.0 * s), Align2::CENTER_TOP)
            };
            pill_in(&p, at, align, &ytxt, s, PAPER(), INK(), plot);
        }
        let arrow = if lock { "" } else { "↔  " };
        let ttxt = format!("{arrow}Tap {} · {}", i + 1, secs_text(viz.times_l_grid[i] * fit));
        pill_in(&p, Pos2::new(c.x, plot.bottom() + 12.0 * s), Align2::CENTER_CENTER, &ttxt, s, INK(), PAPER(), plot);
    }
    if !pressing {
        ui.ctx().data_mut(|m| m.remove::<f32>(held_id));
    }
}

/// The bars: the same Y axis with the taps in order. Press or drag sets the bar
/// under the pointer, so a drag across draws a shape — on touch a finger can't
/// miss a narrow column. The number under a bar mutes or unmutes it.
#[allow(clippy::too_many_arguments)]
fn bars(
    ui: &mut egui::Ui, plot: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    sc: &mut ScopeState, axis: Axis,
) {
    let p = ui.painter().clone();
    axis_grid(&p, plot, axis, Labels::of(params), s);
    let n = n_active(params);
    let slot = plot.width() / n as f32;
    let bw = (slot * 0.46).clamp(5.0 * s, 36.0 * s);
    let wide = bw > 12.0 * s;

    let id = ui.id().with("sc_bars");
    let d = drag_at(ui, id, plot);
    let begun_id = id.with("begun");
    let mut begun: u32 = ui.ctx().data(|m| m.get_temp(begun_id)).unwrap_or(0);
    if d.pressing {
        if let Some(pos) = d.pos {
            let i = (((pos.x - plot.left()) / slot).floor().max(0.0) as usize).min(n - 1);
            let tap = &params.taps[i];
            if begun & (1 << i) == 0 {
                axis.begin(setter, tap);
                begun |= 1 << i;
            }
            axis.put(setter, tap, axis.value((pos.y - plot.top()) / plot.height()));
            sc.sel = Some(i);
            sc.focus = Some(axis.id(i));
        }
    }
    if d.released {
        for i in 0..NUM_TAPS {
            if begun & (1 << i) != 0 {
                axis.end(setter, &params.taps[i]);
            }
        }
        begun = 0;
    }
    ui.ctx().data_mut(|m| m.insert_temp(begun_id, begun));

    // values above every bar when they fit, else only the selected one's
    let show_all = slot >= if axis == Axis::Gain { 40.0 } else { 58.0 } * s;
    let pills = slot >= 38.0 * s;
    let step = (20.0 * s / slot).ceil().max(1.0) as usize;
    for i in 0..n {
        let tap = &params.taps[i];
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let yv = plot.top() + axis.norm(axis.get(tap)) * plot.height();
        let yb = if axis == Axis::Pan { plot.center().y } else { plot.bottom() };
        let sel = sc.sel == Some(i);
        let col = tap_col(i, n);
        let active = tap.active.value();

        let track = Rect::from_center_size(Pos2::new(cx, plot.center().y), Vec2::new(bw, plot.height()));
        p.rect_filled(track, if wide { 6.0 } else { 2.0 } * s, if sel { theme::with_alpha(INK(), 40) } else { PANEL() });
        let fw = (bw - if wide { 8.0 } else { 2.0 } * s).max(3.0 * s);
        let (top, bot) = (yv.min(yb), yv.max(yb).max(yv.min(yb) + 2.0 * s));
        let fill = Rect::from_min_max(Pos2::new(cx - fw * 0.5, top), Pos2::new(cx + fw * 0.5, bot));
        if active {
            p.rect_filled(fill, 3.0 * s, col);
        } else {
            p.rect_stroke(fill, 3.0 * s, Stroke::new(1.5 * s, col), egui::StrokeKind::Inside);
        }
        p.line_segment(
            [Pos2::new(cx - bw * 0.5 + 2.0 * s, yv), Pos2::new(cx + bw * 0.5 - 2.0 * s, yv)],
            Stroke::new(if wide { 3.0 } else { 2.0 } * s, INK()),
        );
        // Motion: where the tap is now, against the bar's set value
        let ym = plot.top() + axis.norm(moved_value(params, axis, i, axis.get(tap))) * plot.height();
        if (ym - yv).abs() > 0.5 * s {
            p.circle_filled(Pos2::new(cx, ym), 3.5 * s, INK());
        }
        if show_all || sel {
            mono(&p, Pos2::new(cx, plot.top() - 10.0 * s), Align2::CENTER_CENTER, &axis.short(tap, Labels::of(params)), 11.0, s, if sel { INK() } else { INK2() });
        }

        // the number under the bar: tap it to select the tap, tap again
        // quickly to mute or unmute it
        let num = Rect::from_center_size(Pos2::new(cx, plot.bottom() + 17.0 * s), Vec2::new(if pills { 34.0 * s } else { slot.max(4.0 * s) }, 24.0 * s));
        let resp = ui.interact(num, ui.id().with(("sc_mute", i)), Sense::click());
        if pills {
            let pr = Rect::from_center_size(num.center(), Vec2::new(34.0, 22.0) * s);
            if active {
                p.rect_filled(pr, 7.0 * s, col);
            } else {
                p.rect_stroke(pr, 7.0 * s, Stroke::new(1.5 * s, col), egui::StrokeKind::Inside);
            }
            mono(&p, pr.center(), Align2::CENTER_CENTER, &format!("{:02}", i + 1), 11.0, s, if active { PAPER() } else { col });
            if sel {
                p.rect_stroke(pr.expand(3.0 * s), 9.0 * s, Stroke::new(1.5 * s, INK()), egui::StrokeKind::Outside);
            }
        } else {
            let strip = Rect::from_center_size(Pos2::new(cx, plot.bottom() + 8.0 * s), Vec2::new(bw.max(4.0 * s), 4.0 * s));
            if active {
                p.rect_filled(strip, 2.0 * s, col);
            } else {
                p.rect_stroke(strip, 2.0 * s, Stroke::new(1.0 * s, col), egui::StrokeKind::Inside);
            }
            if i % step == 0 || sel {
                mono(&p, Pos2::new(cx, plot.bottom() + 21.0 * s), Align2::CENTER_CENTER, &format!("{}", i + 1), 10.0, s, if sel { INK() } else { SOFT() });
            }
        }
        if epo::tap_or_click(ui, &resp) {
            let now = ui.input(|inp| inp.time);
            let key = id.with("num_tap");
            let last: Option<(usize, f64)> = ui.ctx().data(|m| m.get_temp(key));
            if matches!(last, Some((j, t)) if j == i && now - t < 0.4) {
                setter.begin_set_parameter(&tap.active);
                setter.set_parameter(&tap.active, !active);
                setter.end_set_parameter(&tap.active);
                ui.ctx().data_mut(|m| m.remove::<(usize, f64)>(key));
            } else {
                sc.open_tap(i);
                ui.ctx().data_mut(|m| m.insert_temp(key, (i, now)));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lane: Echo (the real repeats) · Motion (Phase 4)
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn lane(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    sc: &mut ScopeState, tempo: f32,
) {
    let (x, y, w, h) = l.lane;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    softsystems_ui::paint::card(&p, rect, "");
    // Echo · Motion · Tap N. The Tap tab shows the selected tap (clicking a dot
    // opens it); picked with nothing selected, it selects the tap it last showed.
    let n = n_active(params);
    if let Some(i) = sc.sel {
        sc.last_tap = i;
    }
    let shown = sc.sel.unwrap_or(sc.last_tap).min(n - 1);
    let tap_tab = format!("Tap {}", shown + 1);
    if let Some(i) = seg(ui, r(o, s, x + 14.0, y + 7.0, 210.0, 26.0), &["Echo", "Motion", &tap_tab], sc.lane_tab, "sc_lane", true) {
        if i == 2 {
            sc.open_tap(shown);
        } else {
            sc.lane_tab = i;
        }
    }
    p.line_segment(
        [o + Vec2::new(x, y + 40.0) * s, o + Vec2::new(x + w, y + 40.0) * s],
        Stroke::new(1.0 * s, theme::line()),
    );
    let (curves, ctl, cols) = (r(o, s, x, y + 40.0, 400.0, h - 40.0), r(o, s, x + 420.0, y + 44.0, w - 434.0, h - 48.0), 4);
    // The Motion tab takes the whole lane (its grid needs the width). The
    // loop's own controls come back with Echo.
    let whole = sc.lane_tab == 1;
    // The loop's controls stand apart from the tab beside them.
    let xm = curves.right() + 6.0 * s;
    let div = [Pos2::new(xm, curves.top() + 10.0 * s), Pos2::new(xm, curves.bottom() - 10.0 * s)];
    if !whole {
        p.line_segment(div, Stroke::new(1.0 * s, theme::line()));
    }

    match sc.lane_tab {
        0 => {
            // The cycle: Auto follows the cluster; Manual keeps its own time, in
            // Time's unit (the feedback time follows Time's Sync since v2.19).
            // Frozen, the loop keeps its length, so the choice waits.
            let locked = params.feedback_time_locked.value();
            let free = !params.freeze.value();
            if let Some(i) = seg(ui, r(o, s, x + 232.0, y + 7.0, 140.0, 26.0), &["Auto", "Manual"], if locked { 0 } else { 1 }, "sc_cycle", free) {
                setter.begin_set_parameter(&params.feedback_time_locked);
                setter.set_parameter(&params.feedback_time_locked, i == 0);
                setter.end_set_parameter(&params.feedback_time_locked);
            }
            // room for three repeats of the loop, or for a count's (up to six)
            let n_rep = params.repeats.value();
            let cap = if n_rep < REPEATS_LOOP { (n_rep as usize).min(6) } else { 3 };
            let viz = crate::gui::compute_viz_data_ex(params, tempo, cap);
            text(&p, o + Vec2::new(x + w - 14.0, y + 20.0) * s, Align2::RIGHT_CENTER, &echo_readout(&viz), 12.0, s, INK2());
            echo(ui, curves, s, params, setter, &mut sc.focus, &viz, tempo);
        }
        1 => motion_tab::motion_panel(ui, o, s, l, params, setter, sc),
        _ => {
            let mut t = sc.sel.unwrap_or(shown).min(n - 1);
            // ‹ Tap 3 of 8 › steps through the live taps, wrapping.
            let (bw, lw) = (30.0, 92.0);
            let nx = x + w - 14.0 - bw;
            let back = button(ui, r(o, s, nx - lw - bw, y + 7.0, bw, 26.0), "‹", false, "sc_tap_prev");
            let fwd = button(ui, r(o, s, nx, y + 7.0, bw, 26.0), "›", false, "sc_tap_next");
            text(&p, r(o, s, nx - lw, y + 7.0, lw, 26.0).center(), Align2::CENTER_CENTER, &format!("Tap {} of {n}", t + 1), 12.0, s, INK2());
            let step = fwd as i32 - back as i32;
            if step != 0 {
                t = (t as i32 + step).rem_euclid(n as i32) as usize;
            }
            sc.sel = Some(t);
            let viz = crate::gui::compute_viz_data_ex(params, tempo, 1);
            let area = if whole { curves.union(ctl) } else { curves };
            tap_panel(ui, area, s, params, setter, &mut sc.focus, &viz, t);
        }
    }
    if whole {
        return;
    }

    // the loop's controls (the cycle is the lane's own handle; Feedback is
    // one too, and a knob): two rows beside the curves (Repeats, Loop).
    let finite = params.repeats.value() < REPEATS_LOOP;
    let (c_rep, c_fb, c_curve, c_tone, c_xf, c_mode) = {
        let title_w = 64.0 * s;
        let cells = Rect::from_min_max(ctl.min + Vec2::new(title_w, 0.0), ctl.max);
        let c = |row, col, span| cell_at(cells, cols, 2, row, col, span, s);
        for (row, t) in ["Repeats", "Loop"].iter().enumerate() {
            let (ctl_row, _, _) = cell_parts(c(row, 0, 1), s);
            group_head(&p, Pos2::new(ctl.left(), ctl_row.center().y), Align2::LEFT_CENTER, t, s);
        }
        let ym = (c(0, 0, 1).bottom() + c(1, 0, 1).top()) * 0.5;
        rule(&p, Pos2::new(ctl.left(), ym), Pos2::new(ctl.right(), ym), s);
        (c(0, 0, 1), c(0, 1, 1), c(0, 2, 2), c(1, 0, 1), c(1, 1, 1), c(1, 2, 2))
    };
    stepper_cell(ui, c_rep, s, setter, &params.repeats, "repeats", "Repeats", if finite { "then stops" } else { "loops" }, &mut sc.focus);
    feedback_cell(ui, c_fb, s, setter, &params.feedback, !params.freeze.value(), &mut sc.focus, &params.motion);
    // Curve shapes the loop's decay, so only ∞ has one.
    enum_cell(ui, c_curve, s, setter, &params.fb_curve, "fb_curve", "Curve", &["Natural", "Bloom", "Gate"], !finite, &mut sc.focus);
    // Left darkens each repeat, right thins it, the centre is off.
    knob_cell_m(ui, c_tone, s, setter, &params.loop_tone, "loop_tone", "Tone", true, true, None, &mut sc.focus, Some((&params.motion, crate::modulation::T_TONE, None)), None);
    // In Ping, X-feed is the bounce's width; Mono has no sides to bounce between.
    let ping = params.crossfeed_mode.value() == CrossfeedMode::PingPong;
    let mono = params.channel_mode.value() == ChannelMode::Mono;
    let xf_label = if ping { "Width" } else { "X-feed" };
    knob_cell_m(ui, c_xf, s, setter, &params.crossfeed, "crossfeed", xf_label, false, !(ping && mono), None, &mut sc.focus, Some((&params.motion, crate::modulation::T_CROSS, None)), None);
    enum_cell_ex(ui, c_mode, s, setter, &params.crossfeed_mode, "crossfeed_mode", "Cross", &["Mix", "Ping", "Diffuse"], &[true, !mono, true], &mut sc.focus);
}

/// Feedback's knob (v2.43): it stops at 100 % on the way up — past it only by dragging on —
/// and draws the self-oscillating range above 100 % in the warning colour. A Motion target
/// like the others (pass 10).
#[allow(clippy::too_many_arguments)]
fn feedback_cell(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &FloatParam, enabled: bool, focus: &mut Option<String>,
    m: &crate::modulation::MotionParams,
) {
    // the crate's stop at 100 %, the self-oscillating range above it in the over-range colour
    let modu = Some((m, crate::modulation::T_FDBK, None));
    knob_cell_m(ui, cell, s, setter, param, "feedback", "Feedback", false, enabled, None, focus, modu, Some(param.preview_normalized(1.0)));
}

/// The lane's Feedback drag stops at 100 % on the way up until it carries this far: the
/// crate's knob stop (`DETENT_PASS` of the knob's travel) in Feedback's units (0–105 %).
const FB_DETENT: f32 = 1.0 + softsystems_ui::knob::DETENT_PASS * 1.05;

/// A count stepper cell (Repeats): ‹ value ›, the name above, a word below.
/// The arrows step; dragging the value moves it like a knob.
#[allow(clippy::too_many_arguments)]
fn stepper_cell(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &IntParam, id: &str,
    label: &str, sub: &str, focus: &mut Option<String>,
) {
    let (ctl, lab, val) = cell_parts(cell, s);
    let w = (cell.width() - 4.0 * s).min(76.0 * s);
    let pill = Rect::from_center_size(ctl.center(), Vec2::new(w, g::CTL_H * s));
    let (lo, hi) = (param.range().unnormalize(0.0), param.range().unnormalize(1.0));
    let v = param.value();
    let shown = param.normalized_value_to_string(param.unmodulated_normalized_value(), false);
    // the shared stepper: ‹ › step (an end's arrow goes at the end) and the
    // value drags up and down; a fast drag can move several steps at once
    let ends = controls::Ends { down: v > lo, up: v < hi };
    if let controls::Step::By(d) = controls::stepper(ui, pill, &shown, ui.id().with(("step", id)), ends, true) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, (v + d).clamp(lo, hi));
        setter.end_set_parameter(param);
        *focus = Some(id.to_string());
    }
    let p = ui.painter();
    text(p, lab, Align2::CENTER_CENTER, label, 11.0, s, INK2());
    text(p, val, Align2::CENTER_CENTER, sub, 11.0, s, SOFT());
}

/// A value pill (Routes' Depth and Time): the crate's `knob::value_pill`, "Name value" in an
/// outline, dragged ↕ like a knob (Shift fine, double-click to its default).
#[allow(clippy::too_many_arguments)]
fn pill_drag(ui: &mut egui::Ui, rect: Rect, _s: f32, setter: &ParamSetter, param: &FloatParam, id: &str, label: &str, focus: &mut Option<String>) {
    use softsystems_ui::knob::{self, Kind, Knob};
    let n = param.unmodulated_normalized_value();
    let shown = param.normalized_value_to_string(n, true);
    // a range either side of zero (Depth's −100…+100 %) is bipolar: its default sits mid-track
    let bip = param.preview_plain(0.0) < 0.0 && param.preview_plain(1.0) > 0.0;
    let k = Knob::new(label, &shown).kind(if bip { Kind::Bipolar } else { Kind::Plain }).default(param.default_normalized_value());
    let mut v = n;
    let out = knob::value_pill(ui, rect, ui.id().with(("pill", id)), &k, &mut v);
    if out.began {
        setter.begin_set_parameter(param);
    }
    if out.changed {
        setter.set_parameter_normalized(param, v);
        *focus = Some(id.to_string());
    }
    if out.ended {
        setter.end_set_parameter(param);
    }
}

/// Where Motion has tap `i` right now on `axis` (v2.42): its set value `v`
/// moved by what the audio thread last reported for that target (0, so `v`
/// itself, while nothing routes there). Type and Slope don't move.
/// Preview only (`AFTERECHO_PREVIEW=mxdemo`): a few routes and a stepped pattern, to see the
/// tiles, the editor and the grid without clicking.
fn mx_demo(params: &AfterechoParams) {
    let m = &params.motion;
    if let Ok(mut g) = m.patch.lock() {
        for (slot, t, a) in [(0u16, "pan", 0.3), (0, "cutoff", -0.15), (1, "colour", 0.25), (2, "tone", -0.2), (4, "gain", 0.2), (1, "fdbk", 0.15), (3, "mix", 0.25), (1, "mod.0.rate", 0.2)] {
            g.routes.set(slot, t, a, crate::modulation::ROUTE_CAP);
        }
        if let softsystems_mod::Modulator::Steps(st) = &mut g.modulators[4] {
            for (i, v) in st.levels.iter_mut().enumerate() {
                *v = [1.0, 0.25, 0.6, 0.0, 0.8, 0.4, 1.0, 0.1][i % 8];
            }
        }
    }
    m.shared.changed();
}

fn moved_value(params: &AfterechoParams, axis: Axis, i: usize, v: f32) -> f32 {
    use crate::modulation::{T_CUTOFF, T_GAIN, T_PAN, T_RESO};
    let mx = &params.motion.shared.matrix;
    let tap = &params.taps[i];
    let moved = |p: &FloatParam, t: usize| p.preview_plain((p.preview_normalized(v) + mx.voice_offset(t, i)).clamp(0.0, 1.0));
    match axis {
        Axis::Pan => moved(&tap.pan, T_PAN),
        Axis::Gain => moved(&tap.gain_db, T_GAIN),
        Axis::Cutoff => moved(&tap.filter_cutoff, T_CUTOFF),
        Axis::Reso => moved(&tap.filter_q, T_RESO),
        Axis::Type | Axis::Slope | Axis::Detune => v,
    }
}

/// The Tap tab: one tap's time, place, level and filter at once (v3's tap
/// panel). The cells use the nested param ids, so the focus card follows them.
#[allow(clippy::too_many_arguments)]
fn tap_panel(
    ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    focus: &mut Option<String>, viz: &VizData, i: usize,
) {
    let tap = &params.taps[i];
    let ch = params.channel_mode.value();
    let inner = area.shrink2(Vec2::new(14.0, 4.0) * s);
    let c = |row, col| cell_at(inner, 5, 2, row, col, 1, s);
    let id = |base: &str| format!("{base}_{}", i + 1);
    let time = secs_text(viz.times_l_grid[i] * viz.grid_range_secs_taps_only);
    knob_cell(ui, c(0, 0), s, setter, &tap.time_offset, &id("time_offset"), "Time", true, true, Some(time), focus);
    knob_cell_m(ui, c(0, 1), s, setter, &tap.pan, &id("pan"), "Pan", true, ch != ChannelMode::Mono, Some(pan_text(tap.pan.value(), ch)), focus, Some((&params.motion, crate::modulation::T_PAN, Some(i))), None);
    knob_cell_m(ui, c(0, 2), s, setter, &tap.gain_db, &id("gain_db"), "Gain", false, true, Some(db_text(tap.gain_db.value())), focus, Some((&params.motion, crate::modulation::T_GAIN, Some(i))), Some(tap.gain_db.preview_normalized(0.0)));
    let det = detune_text(tap.detune.value(), params.detune_range.value() as f32);
    knob_cell(ui, c(0, 3), s, setter, &tap.detune, &id("tap_detune"), "Detune", true, true, Some(det), focus);
    // the range is every tap's (v2.46)
    stepper_cell(ui, c(0, 4), s, setter, &params.detune_range, "detune_range", "Range", "all taps", focus);
    toggle_cell(ui, c(1, 4), s, setter, &tap.active, &id("active"), "Tap", "On", "Muted", focus);
    enum_cell(ui, c(1, 0), s, setter, &tap.filter_type, &id("tap_filter_type"), "Filter", &["LP", "BP", "HP"], true, focus);
    knob_cell_m(ui, c(1, 1), s, setter, &tap.filter_cutoff, &id("tap_filter_cutoff"), "Cutoff", false, true, Some(hz_text(tap.filter_cutoff.value())), focus, Some((&params.motion, crate::modulation::T_CUTOFF, Some(i))), None);
    knob_cell_m(ui, c(1, 2), s, setter, &tap.filter_q, &id("tap_filter_q"), "Reso", false, true, Some(format!("Q {:.1}", tap.filter_q.value())), focus, Some((&params.motion, crate::modulation::T_RESO, Some(i))), None);
    // eight steps don't fit a segmented row: the shared stepper, dragged ↕ (owner, 2026-11-16)
    enum_stepper_cell(ui, c(1, 3), s, setter, &tap.filter_slope, &id("tap_filter_slope"), "Slope", &SLOPE_DB, focus);
}

/// "Feedback 60 % · −4.4 dB per repeat · 13 to −60 dB", from the same
/// per-cycle levels the lane draws (they follow the FB curve); a count, "4
/// repeats · flat · stops after 212 ms"; frozen, "Frozen · holds a 53 ms loop".
fn echo_readout(viz: &VizData) -> String {
    if viz.frozen {
        let cycle = secs_text(viz.fb_cycle_norm * viz.grid_range_secs);
        return format!("Frozen · holds a {cycle} loop");
    }
    // a count: "4 repeats · −6.0 dB each · stops after 212 ms"
    if let Some(n) = viz.repeats {
        let word = if n == 1 { "1 repeat".to_string() } else { format!("{n} repeats") };
        if viz.feedback < 1e-3 {
            return format!("{word} · silent at Feedback 0 %");
        }
        let each = if (viz.feedback - 1.0).abs() < 0.005 {
            "flat".to_string()
        } else {
            format!("{} each", db_text(20.0 * viz.feedback.log10()))
        };
        let last = secs_text(n as f32 * viz.fb_cycle_norm * viz.grid_range_secs);
        return format!("{word} · {each} · stops after {last}");
    }
    let fb = format!("Feedback {:.0} %", viz.feedback * 100.0);
    if viz.feedback < 1e-3 {
        return format!("{fb} · no repeats");
    }
    let g1 = viz.cycle_gains[1];
    if g1 >= 0.999 {
        return format!("{fb} · repeats hold · self-oscillates");
    }
    let db = db_text(20.0 * g1.max(1e-6).log10());
    // compute_viz_data stops counting at 64 cycles
    let reps = if viz.n_echo_cycles >= 64 { "64+".to_string() } else { viz.n_echo_cycles.to_string() };
    format!("{fb} · {db} per repeat · {reps} to −60 dB")
}

/// The Echo lane: one band per cycle, each tap and its repeats as stems (height
/// = level, −60 dB floor), the loop's decay as a dashed line. In Ping the lane
/// splits round an axis: what a mono source puts on the left goes up, on the
/// right down, so the bounce shows. Drag ↕ anywhere: feedback. Drag the band
/// edge ↔: the cycle (switches it to Manual).
#[allow(clippy::too_many_arguments)]
fn echo(
    ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    focus: &mut Option<String>, viz: &VizData, tempo: f32,
) {
    let p = ui.painter().clone();
    let plot = Rect::from_min_max(area.min + Vec2::new(18.0, 36.0) * s, area.max - Vec2::new(20.0, 22.0) * s);
    let ch = params.channel_mode.value();
    let ping = params.crossfeed_mode.value() == CrossfeedMode::PingPong && ch != ChannelMode::Mono;
    let width = bounce::width(ch, params.crossfeed.value());
    let (axis, reach) = if ping { (plot.center().y, plot.height() * 0.5) } else { (plot.bottom(), plot.height()) };
    p.line_segment([Pos2::new(plot.left(), axis), Pos2::new(plot.right(), axis)], Stroke::new(1.0 * s, theme::line2()));
    if ping {
        for (t, y) in [("L", axis - reach * 0.5), ("R", axis + reach * 0.5)] {
            text(&p, Pos2::new(plot.left() - 9.0 * s, y), Align2::CENTER_CENTER, t, 10.0, s, SOFT());
        }
    }
    let n = n_active(params);
    let active: Vec<usize> = (0..n).filter(|&i| params.taps[i].active.value()).collect();
    if active.is_empty() {
        return;
    }

    // Everything in seconds. The view covers the taps, the repeats worth
    // showing and at least one cycle (so the handles exist at 0 % feedback);
    // while the cycle edge is dragged it's held, so the edge stays under the pointer.
    let secs = |norm: f32| norm * viz.grid_range_secs;
    let t0 = active.iter().map(|&i| secs(viz.times_l[i])).fold(f32::INFINITY, f32::min);
    let cyc = secs(viz.fb_cycle_norm).max(1e-4);
    let held_id = ui.id().with("sc_echo_range");
    let held: Option<f32> = ui.ctx().data(|m| m.get_temp(held_id));
    let range = held.unwrap_or_else(|| viz.grid_range_secs.max((t0 + cyc) * 1.08)).max(1e-4);
    let tx = |t: f32| plot.left() + t / range * plot.width();
    // a level's height off the axis, −60 dB on it
    let lh = |db: f32| reach * (1.0 + db / 60.0).clamp(0.0, 1.0);
    let ly = |db: f32| axis - lh(db);
    let margin = (0.015 * range).min(cyc * 0.25);
    let shown = viz.n_echo_cycles.min(if viz.repeats.is_some() { 6 } else { 3 });
    let gdb = |k: usize| 20.0 * viz.cycle_gains[k].max(1e-6).log10();

    // bands: one per cycle, every other one lighter, labelled with its level
    for k in 0..=shown.max(1) {
        let b0 = if k == 0 { 0.0 } else { t0 + k as f32 * cyc - margin };
        let (x0, x1) = (tx(b0).max(plot.left()), tx(t0 + (k + 1) as f32 * cyc - margin).min(plot.right()));
        if x0 >= plot.right() {
            break;
        }
        if k % 2 == 1 {
            p.rect_filled(Rect::from_min_max(Pos2::new(x0, area.top()), Pos2::new(x1, plot.bottom())), 0.0, PANEL());
        }
        if x1 - x0 > 96.0 * s {
            let label = match k {
                0 => "Taps · 0 dB".to_string(),
                k if viz.frozen => format!("Repeat {k} · held"),
                k if viz.cycle_gains[k] < 1e-3 => format!("Repeat {k} · silent"),
                k => format!("Repeat {k} · {}", db_text(gdb(k))),
            };
            text(&p, Pos2::new(x0 + 8.0 * s, area.top() + 14.0 * s), Align2::LEFT_CENTER, &label, 11.0, s, SOFT());
        }
    }
    // the taps and their repeats; with Time L/R unlinked the right channel's
    // land at their own times (thin stems, diamond heads, as in the field)
    let r_apart = !params.time_linked.value();
    for k in 0..=shown {
        if viz.cycle_gains[k] < 1e-3 {
            break;
        }
        for &i in &active {
            let db = params.taps[i].gain_db.value() + gdb(k);
            let col = if k == 0 { tap_col(i, n) } else { theme::with_alpha(G(), (217.0 * 0.72f32.powi(k as i32 - 1)) as u8) };
            let dot = if k == 0 { 3.5 } else { 2.5 } * s;
            let x = tx(secs(viz.times_l[i]) + k as f32 * cyc);
            let xr = tx(secs(viz.times_r[i]) + k as f32 * cyc);
            if ping {
                // the left side up at the left time, the right side down at the right time
                let (gl, gr) = bounce::mono_sides(ch, params.taps[i].pan.value(), width, k);
                let side_db = |g: f32| db + 20.0 * g.max(1e-6).log10();
                if x <= plot.right() && side_db(gl) > -60.0 {
                    let tip = Pos2::new(x, axis - lh(side_db(gl)));
                    p.line_segment([Pos2::new(x, axis), tip], Stroke::new(2.0 * s, col));
                    p.circle_filled(tip, dot, col);
                }
                if xr <= plot.right() && side_db(gr) > -60.0 {
                    let tip = Pos2::new(xr, axis + lh(side_db(gr)));
                    p.line_segment([Pos2::new(xr, axis), tip], Stroke::new(2.0 * s, col));
                    if r_apart {
                        paint_diamond(&p, tip, 3.0 * s, Stroke::new(1.3 * s, col));
                    } else {
                        p.circle_filled(tip, dot, col);
                    }
                }
                continue;
            }
            let yv = ly(db);
            if x <= plot.right() {
                p.line_segment([Pos2::new(x, plot.bottom()), Pos2::new(x, yv)], Stroke::new(2.0 * s, col));
                p.circle_filled(Pos2::new(x, yv), dot, col);
            }
            if r_apart && xr <= plot.right() && (xr - x).abs() >= 1.0 {
                p.line_segment([Pos2::new(xr, plot.bottom()), Pos2::new(xr, yv)], Stroke::new(1.0 * s, col));
                paint_diamond(&p, Pos2::new(xr, yv), 3.0 * s, Stroke::new(1.3 * s, col));
            }
        }
    }
    // the loop's decay through the cycles (follows the FB curve)
    let env: Vec<Pos2> = (0..=shown.max(1))
        .take_while(|&k| k == 0 || viz.cycle_gains[k] >= 1e-3)
        .map(|k| Pos2::new(tx(t0 + k as f32 * cyc), ly(gdb(k))))
        .filter(|q| q.x <= plot.right())
        .collect();
    // a count ends: a line where the last copy's band does
    if let (Some(n), false) = (viz.repeats, viz.frozen) {
        let xs = tx(t0 + (n + 1) as f32 * cyc - margin);
        if n > 0 && viz.feedback >= 1e-3 && xs <= plot.right() {
            let stroke = Stroke::new(1.4 * s, INK());
            p.extend(egui::Shape::dashed_line(&[Pos2::new(xs, area.top() + 6.0 * s), Pos2::new(xs, plot.bottom())], stroke, 4.0 * s, 3.0 * s));
            text(&p, Pos2::new(xs + 5.0 * s, area.top() + 14.0 * s), Align2::LEFT_CENTER, "stop", 11.0, s, INK());
        }
    }
    if env.len() > 1 {
        let stroke = Stroke::new(1.4 * s, theme::with_alpha(INK(), 150));
        p.extend(egui::Shape::dashed_line(&env, stroke, 5.0 * s, 4.0 * s));
        if ping {
            let below: Vec<Pos2> = env.iter().map(|q| Pos2::new(q.x, 2.0 * axis - q.y)).collect();
            p.extend(egui::Shape::dashed_line(&below, stroke, 5.0 * s, 4.0 * s));
        }
    }

    // Frozen, the loop holds its level and length: the handles that set them
    // step aside until it's let go.
    let gx = tx(t0 + cyc - margin);
    if !viz.frozen {
        // ↕ feedback: anywhere on the lane, relative so it can pass 100 %
        let fid = ui.id().with("sc_echo_fb");
        let fd = drag_at(ui, fid, plot.expand2(Vec2::new(0.0, 14.0 * s)));
        // accumulated from the press, so the stop at 100 % can be dragged past
        let fb = params.feedback.value();
        let virt_id = fid.with("virt");
        if fd.started {
            ui.ctx().data_mut(|m| m.insert_temp(virt_id, fb));
        }
        let fv = (fd.delta.y != 0.0).then(|| {
            let virt: f32 = ui.ctx().data(|m| m.get_temp(virt_id)).unwrap_or(fb);
            let virt = (virt - fd.delta.y / plot.height() * 1.05).clamp(0.0, 1.05);
            ui.ctx().data_mut(|m| m.insert_temp(virt_id, virt));
            if fb <= 1.0 && virt > 1.0 && virt < FB_DETENT { 1.0 } else { virt }
        });
        if fv.is_some() {
            *focus = Some("feedback".into());
        }
        gesture_set(ui, fid.with("g"), setter, &params.feedback, &fd, fv);
        if fd.double {
            setter.begin_set_parameter(&params.feedback);
            setter.set_parameter(&params.feedback, params.feedback.default_plain_value());
            setter.end_set_parameter(&params.feedback);
        }

        // ↔ the cycle edge (registered last, so it wins over the lane)
        let cid = ui.id().with("sc_echo_cycle");
        let cd = drag_at(ui, cid, Rect::from_center_size(Pos2::new(gx, plot.center().y), Vec2::new(22.0 * s, plot.height() + 28.0 * s)));
        if cd.started {
            ui.ctx().data_mut(|m| m.insert_temp(held_id, range));
            if params.feedback_time_locked.value() {
                setter.begin_set_parameter(&params.feedback_time_locked);
                setter.set_parameter(&params.feedback_time_locked, false);
                setter.end_set_parameter(&params.feedback_time_locked);
            }
        }
        let cv = if cd.delta != Vec2::ZERO {
            cd.pos.map(|q| {
                let cycle = ((q.x - plot.left()) / plot.width() * range - t0 + margin).max(topology::MIN_FB_SECS);
                AfterechoParams::feedback_time_norm_for_secs(params.sync_mode.value(), cycle, tempo)
            })
        } else {
            None
        };
        if cv.is_some() {
            *focus = Some("feedback_time".into());
        }
        gesture_set(ui, cid.with("g"), setter, &params.feedback_time, &cd, cv);
        if cd.released {
            ui.ctx().data_mut(|m| m.remove::<f32>(held_id));
        }
    }

    // handles and their readouts: the feedback value lives in the lane's header,
    // the cycle's on the time axis, so neither covers the repeats
    time_grid(&p, plot, range, tempo, params.sync_mode.value() == SyncMode::Sync, s, false);
    let mid = plot.center().y;
    p.line_segment([Pos2::new(gx, area.top() + 4.0 * s), Pos2::new(gx, plot.bottom())], Stroke::new(1.4 * s, INK()));
    if viz.frozen {
        let ctxt = format!("Held · {}", secs_text(cyc));
        pill_in(&p, Pos2::new(gx, plot.bottom() + 12.0 * s), Align2::CENTER_CENTER, &ctxt, s, INK(), PAPER(), area);
        return;
    }
    p.rect_filled(Rect::from_center_size(Pos2::new(gx, mid), Vec2::new(6.0, 18.0) * s), 3.0 * s, INK());
    let h = Pos2::new(tx(t0 + cyc), ly(gdb(1)));
    p.circle_filled(h, 6.0 * s, PAPER());
    p.circle_stroke(h, 6.0 * s, Stroke::new(2.0 * s, INK()));
    let auto = params.feedback_time_locked.value();
    // Manual in Sync names the division too: "↔ Cycle 8/1 · 17.45 s".
    let div = if !auto && params.sync_mode.value() == SyncMode::Sync {
        format!("{} · ", crate::params::fb_div(params.feedback_time.value()).1)
    } else {
        String::new()
    };
    let ctxt = format!("↔ Cycle {div}{}{}", secs_text(cyc), if auto { " · auto" } else { "" });
    pill_in(&p, Pos2::new(gx, plot.bottom() + 12.0 * s), Align2::CENTER_CENTER, &ctxt, s, INK(), PAPER(), area);
    text(&p, h + Vec2::new(11.0, 0.0) * s, Align2::LEFT_CENTER, "↕", 12.0, s, INK2());
}

// ─────────────────────────────────────────────────────────────────────────────
// Right column
// ─────────────────────────────────────────────────────────────────────────────

/// Time, sync, transition mode, link, tap count, spacing and the Shape pad.
#[allow(clippy::too_many_arguments)]
fn time_card(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter,
    focus: &mut Option<String>, tempo: f32,
) {
    let rect = r(o, s, l.right_x, l.time.0, l.right_w, l.time.1);
    // Two groups under their own headings instead of one title: Time on the
    // left (columns 0–2), Pattern on the right (3–5), a hairline between.
    let inner = card(ui, rect, "", s);
    let c = |row, col, span| cell_at(inner, 6, 3, row, col, span, s);
    let p = ui.painter().clone();
    let split = c(0, 3, 1).left() - 4.0 * s;
    group_head(&p, Pos2::new(inner.left(), rect.top() + 12.0 * s), Align2::LEFT_TOP, "Time", s);
    group_head(&p, Pos2::new(split + 8.0 * s, rect.top() + 12.0 * s), Align2::LEFT_TOP, "Pattern", s);
    rule(&p, Pos2::new(split, rect.top() + 14.0 * s), Pos2::new(split, rect.bottom() - 14.0 * s), s);

    // Time: three rows whether or not L/R is linked. The right side's own time
    // and sync fill the third when it's split, so nothing else on the card
    // moves (Pattern lines up with the first row).
    let linked = params.time_linked.value();
    let left = Rect::from_min_max(inner.min, Pos2::new(c(0, 2, 1).right(), inner.bottom()));
    let t = |row, col, span| cell_at(left, 3, 3, row, col, span, s);
    let nl = params.base_time_l.unmodulated_normalized_value();
    knob_cell(ui, t(0, 0, 1), s, setter, &params.base_time_l, "base_time_l", "Time", false, true, Some(params.format_base_time(nl, tempo)), focus);
    enum_cell(ui, t(0, 1, 1), s, setter, &params.sync_mode, "sync_mode", "Sync", &["Free", "Sync"], true, focus);
    toggle_cell(ui, t(0, 2, 1), s, setter, &params.time_linked, "time_linked", "L/R", "Linked", "Split", focus);
    enum_cell(ui, t(1, 0, 3), s, setter, &params.delay_mode, "delay_mode", "Mode", &["Repitch", "Fade", "Jump"], true, focus);
    if !linked {
        let nr = params.base_time_r.unmodulated_normalized_value();
        let r_txt = AfterechoParams::format_base_time_mode(params.sync_mode_r_eff(), nr, tempo);
        knob_cell(ui, t(2, 0, 1), s, setter, &params.base_time_r, "base_time_r", "Time R", false, true, Some(r_txt), focus);
        enum_cell(ui, t(2, 1, 1), s, setter, &params.sync_mode_r, "sync_r", "Sync R", &["Free", "Sync"], true, focus);
    }
    // Swing, in the slot under L/R that nothing else uses (so nothing moves).
    // Lin and Euclid have a grid to swing; Exp and Log don't.
    let euclid = params.spacing_mode.value() == SpacingMode::Euclidean;
    let grid = euclid || params.spacing_mode.value() == SpacingMode::Linear;
    knob_cell(ui, t(2, 2, 1), s, setter, &params.swing, "swing", "Swing", false, grid, None, focus);

    // Pattern: the tap count over the list of spacings, the Shape pad beside
    // (Euclid's ring in Euclid, where the pad has nothing to shape); both
    // start level with Time's first row.
    let row0 = t(0, 0, 1);
    let taps = Rect::from_min_size(Pos2::new(c(0, 3, 1).left(), row0.top()), row0.size());
    knob_cell(ui, taps, s, setter, &params.num_taps_active, "num_taps_active", "Taps", false, true, None, focus);
    let sp_top = taps.bottom() + 10.0 * s;
    text(&p, Pos2::new(taps.center().x, sp_top + g::LBL_H * 0.5 * s), Align2::CENTER_CENTER, "Spacing", 11.0, s, INK2());
    let list = Rect::from_min_max(Pos2::new(taps.left() + 8.0 * s, sp_top + (g::LBL_H + 4.0) * s), Pos2::new(taps.right(), inner.bottom()));
    let cur = enum_index(&params.spacing_mode, 4);
    let picked = controls::seg_v(ui, list, &["Lin", "Exp", "Log", "Euclid"], cur, ui.id().with(("seg", "spacing_mode")), &[]);
    if let Some(i) = picked.filter(|&i| i != cur) {
        set_norm(setter, &params.spacing_mode, i as f32 / 3.0);
        *focus = Some("spacing_mode".into());
    }
    let pad = Rect::from_min_max(Pos2::new(c(0, 4, 1).left(), row0.top()), Pos2::new(c(0, 5, 1).right(), inner.bottom()));
    if euclid {
        euclid_ring(ui, pad, s, params, setter, focus);
    } else {
        shape_pad(ui, pad, s, params, setter, focus);
    }
}

/// Euclid's ring, in the Shape pad's place (v2.38): the pattern's steps round
/// a circle, the dry note's step at the top, the taps as dots in their
/// colours (one that lands on the dry note's step sounds at the pattern's
/// end, the same spot). Drag round it to turn the pattern; Steps and Rotate
/// under it.
fn euclid_ring(ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter, focus: &mut Option<String>) {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let p = ui.painter().clone();
    let n = params.euclid_steps.value() as usize;
    let rot = params.euclid_rotate.value() as usize;
    let heard = n_active(params);
    let asked = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let (steps, count) = topology::euclid_steps(heard, n, rot);

    text(&p, Pos2::new(area.center().x, area.top() + g::LBL_H * 0.5 * s), Align2::CENTER_CENTER, "Euclid", 11.0, s, INK2());
    let knobs = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - g::CELL_H * s), area.max);
    let half = Vec2::new(knobs.width() * 0.5, knobs.height());
    knob_cell(ui, Rect::from_min_size(knobs.min, half), s, setter, &params.euclid_steps, "euclid_steps", "Steps", false, true, None, focus);
    let rot_cell = Rect::from_min_size(knobs.min + Vec2::new(half.x, 0.0), half);
    knob_cell(ui, rot_cell, s, setter, &params.euclid_rotate, "euclid_rotate", "Rotate", false, true, None, focus);

    let ring_box = Rect::from_min_max(Pos2::new(area.left(), area.top() + (g::LBL_H + 2.0) * s), Pos2::new(area.right(), knobs.top() - 4.0 * s));
    let rad = (ring_box.width().min(ring_box.height()) * 0.5).max(16.0 * s);
    let c = ring_box.center();
    p.circle_filled(c, rad, INSET());
    let r = rad - 11.0 * s;
    p.circle_stroke(c, r, Stroke::new(1.0 * s, theme::line()));
    let at = |i: f32| {
        let a = i / n as f32 * TAU - FRAC_PI_2;
        c + Vec2::new(a.cos(), a.sin()) * r
    };
    // Swing as the DSP plays it (topology::compute_positions): the 1st, 3rd, 5th… tap moves
    // toward the next one, so its dot moves round the ring, a faint mark left on the straight
    // step
    let pos = topology::compute_positions(params.spacing(false), heard);
    let step = 1.0 / NUM_TAPS as f32;
    p.circle_stroke(at(0.0), 7.5 * s, Stroke::new(1.5 * s, INK()));
    for i in 0..n {
        match (0..count).find(|&j| steps[j] % n == i) {
            Some(j) => {
                let played = pos[j] / step;
                if (played - steps[j] as f32).abs() > 0.001 {
                    p.circle_filled(at(i as f32), 2.0 * s, theme::FAINT());
                }
                p.circle_filled(at(played), 5.0 * s, tap_col(j, heard));
            }
            None => {
                p.circle_filled(at(i as f32), 2.0 * s, theme::FAINT());
            }
        }
    }
    mono(&p, c - Vec2::new(0.0, 7.0 * s), Align2::CENTER_CENTER, &count.to_string(), 18.0, s, INK());
    let sub = if asked > heard { format!("of {n} · {} silent", asked - heard) } else { format!("of {n}") };
    text(&p, c + Vec2::new(0.0, 11.0 * s), Align2::CENTER_CENTER, &sub, 10.5, s, SOFT());

    // Drag round the ring to turn it: the angle travelled counts in steps,
    // and keeps counting past a whole turn.
    let id = ui.id().with("sc_euclid_ring");
    let d = drag_at(ui, id, Rect::from_center_size(c, Vec2::splat(rad * 2.0)));
    let key = id.with("turn");
    let angle = |pos: Pos2| (pos.y - c.y).atan2(pos.x - c.x);
    if d.started {
        if let Some(pos) = d.pos {
            ui.ctx().data_mut(|m| m.insert_temp(key, (angle(pos), 0.0f32, rot as i32)));
        }
    }
    let mut new = None;
    if d.pressing {
        if let (Some(pos), Some((last, acc, from))) = (d.pos, ui.ctx().data(|m| m.get_temp::<(f32, f32, i32)>(key))) {
            let a = angle(pos);
            let mut da = a - last;
            if da > PI {
                da -= TAU;
            } else if da < -PI {
                da += TAU;
            }
            let acc = acc + da;
            ui.ctx().data_mut(|m| m.insert_temp(key, (a, acc, from)));
            let turned = (from + (acc / (TAU / n as f32)).round() as i32).rem_euclid(n as i32);
            if turned != params.euclid_rotate.value() {
                new = Some(turned);
                *focus = Some("euclid_rotate".into());
            }
        }
    }
    gesture_set(ui, id.with("g"), setter, &params.euclid_rotate, &d, new);
    if d.released {
        ui.ctx().data_mut(|m| m.remove::<(f32, f32, i32)>(key));
    }
}

/// The Shape pad: X bends where the taps bunch up, Y how hard (shape_x −1…1,
/// shape_y 0…1 with 1 at the top, as in v3). Ticks show the real tap positions.
fn shape_pad(ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter, focus: &mut Option<String>) {
    let p = ui.painter().clone();
    text(&p, Pos2::new(area.center().x, area.top() + g::LBL_H * 0.5 * s), Align2::CENTER_CENTER, "Shape", 11.0, s, INK2());
    let side = (area.height() - 2.0 * (g::LBL_H + 2.0) * s).min(area.width());
    let pad = Rect::from_center_size(Pos2::new(area.center().x, area.top() + (g::LBL_H + 2.0) * s + side * 0.5), Vec2::splat(side));
    p.rect_filled(pad, g::R_CTL * s, INSET());

    // The dot and the drag both map to the same inset rect, so the dot stays
    // whole at the edges and stays under the finger.
    let inner = pad.shrink(9.0 * s);
    let (xp, yp) = (&params.shape_x_l, &params.shape_y_l);
    let resp = ui.interact(pad, ui.id().with("sc_shape"), Sense::click_and_drag());
    let pos = if theme::touch() {
        epo::touch_interact(ui, resp.id, pad).pos
    } else if resp.dragged() || resp.clicked() {
        resp.interact_pointer_pos()
    } else {
        None
    };
    if resp.drag_started() || resp.clicked() {
        setter.begin_set_parameter(xp);
        setter.begin_set_parameter(yp);
    }
    if let Some(at) = pos {
        let nx = ((at.x - inner.left()) / inner.width()).clamp(0.0, 1.0);
        let ny = ((at.y - inner.top()) / inner.height()).clamp(0.0, 1.0);
        setter.set_parameter(xp, nx * 2.0 - 1.0);
        setter.set_parameter(yp, 1.0 - ny);
        *focus = Some("shape_x_l".into());
    }
    if resp.drag_stopped() || resp.clicked() {
        setter.end_set_parameter(xp);
        setter.end_set_parameter(yp);
    }

    let (sx, sy) = (xp.value(), yp.value());
    let dot = Pos2::new(inner.left() + (sx + 1.0) * 0.5 * inner.width(), inner.top() + (1.0 - sy) * inner.height());
    let hair = Stroke::new(1.0 * s, theme::with_alpha(INK(), 60));
    p.line_segment([Pos2::new(dot.x, pad.top()), Pos2::new(dot.x, pad.bottom())], hair);
    p.line_segment([Pos2::new(pad.left(), dot.y), Pos2::new(pad.right(), dot.y)], hair);
    // where the taps actually sit (first … last active), along the top edge
    let n = n_active(params);
    let pos = topology::compute_positions(topology::Spacing { shape_x: sx, shape_y: sy, ..params.spacing(false) }, n);
    let (first, last) = (pos[0], pos[n - 1].max(pos[0] + 1e-6));
    for &t in pos.iter().take(n) {
        let x = inner.left() + inner.width() * ((t - first) / (last - first)).clamp(0.0, 1.0);
        p.line_segment([Pos2::new(x, pad.top() + 3.0 * s), Pos2::new(x, pad.top() + 8.0 * s)], Stroke::new(2.0 * s, G()));
    }
    p.circle_filled(dot, 7.0 * s, PAPER());
    p.circle_stroke(dot, 7.0 * s, Stroke::new(2.0 * s, INK()));
    p.circle_filled(dot, 3.0 * s, INK());
    let linked = if params.shape_linked.value() { "Linked" } else { "L" };
    mono(&p, Pos2::new(area.center().x, pad.bottom() + (2.0 + g::LBL_H * 0.5) * s), Align2::CENTER_CENTER, linked, 11.5, s, INK());
}

/// Diffuse · Tape · Saturate on one card; a dot marks the blocks that are on.
fn colour_card(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState) {
    let rect = r(o, s, l.right_x, l.colour.0, l.right_w, l.colour.1);
    let inner = card(ui, rect, "Colour", s);
    const NAMES: [&str; 4] = ["Diffuse", "Tape", "Saturate", "Dub"];
    let tabs = Rect::from_min_size(Pos2::new(rect.right() - 314.0 * s, rect.top() + 7.0 * s), Vec2::new(300.0, 26.0) * s);
    if let Some(i) = seg(ui, tabs, &NAMES, sc.colour_tab, "sc_colour", true) {
        sc.colour_tab = i;
    }
    // each stage's on/off dot sits just after its name
    let on = [
        params.diffuse_amount.value() > 0.0,
        params.degrade_amount.value() > 0.0,
        params.sat_amount.value() > 0.0,
        params.dub_intensity.value() > 0.0,
    ];
    let seg_w = tabs.width() / NAMES.len() as f32;
    for (i, (&is_on, name)) in on.iter().zip(NAMES).enumerate() {
        let tw = ui.painter().layout_no_wrap(name.into(), FontId::proportional(fs(12.0, s)), INK()).size().x;
        let c = Pos2::new(tabs.left() + (i as f32 + 0.5) * seg_w + tw * 0.5 + 7.0 * s, tabs.center().y);
        ui.painter().circle_filled(c, 2.5 * s, if is_on { INK() } else { theme::line2() });
    }
    let body = Rect::from_min_max(Pos2::new(inner.left(), tabs.bottom()), inner.max);
    let c = |col, span| cell_at(body, 6, 1, 0, col, span, s);
    let xr = c(4, 1).left() - 4.0 * s;
    rule(ui.painter(), Pos2::new(xr, body.top() + 8.0 * s), Pos2::new(xr, body.bottom() - 4.0 * s), s);
    let f = &mut sc.focus;
    let routes = ["In", "FB", "Out"];
    match sc.colour_tab {
        0 => {
            knob_cell_m(ui, c(0, 1), s, setter, &params.diffuse_amount, "diffuse_amount", "Amount", false, true, None, f, Some((&params.motion, crate::modulation::T_COLOUR, None)), None);
            knob_cell(ui, c(1, 1), s, setter, &params.diffuse_size, "diffuse_size", "Size", false, true, None, f);
            enum_cell(ui, c(4, 2), s, setter, &params.diffuse_routing, "diffuse_routing", "Route", &routes, true, f);
        }
        1 => {
            knob_cell_m(ui, c(0, 1), s, setter, &params.degrade_amount, "degrade_amount", "Amount", false, true, None, f, Some((&params.motion, crate::modulation::T_COLOUR, None)), None);
            knob_cell(ui, c(1, 1), s, setter, &params.degrade_drive, "degrade_drive", "Drive", false, true, None, f);
            knob_cell(ui, c(2, 1), s, setter, &params.degrade_wear, "degrade_wear", "Wear", false, true, None, f);
            enum_cell(ui, c(3, 1), s, setter, &params.degrade_type, "degrade_type", "Type", &["I", "II", "IV"], true, f);
            enum_cell(ui, c(4, 2), s, setter, &params.degrade_routing, "degrade_routing", "Route", &routes, true, f);
        }
        3 => {
            // Dub (beta pass 12): always in the loop, so no Route; the swirl's centre greys
            // with no allpass
            let col = Some((&params.motion, crate::modulation::T_COLOUR, None));
            knob_cell_m(ui, c(0, 1), s, setter, &params.dub_intensity, "dub_intensity", "Intensity", false, true, None, f, col, None);
            knob_cell(ui, c(1, 1), s, setter, &params.dub_low, "dub_low", "Low cut", false, true, Some(hz_text(params.dub_low.value())), f);
            knob_cell(ui, c(2, 1), s, setter, &params.dub_high, "dub_high", "High cut", false, true, Some(hz_text(params.dub_high.value())), f);
            let swirls = params.dub_swirls.value() > 0;
            knob_cell(ui, c(3, 1), s, setter, &params.dub_swirl, "dub_swirl", "Swirl", false, swirls, Some(hz_text(params.dub_swirl.value())), f);
            int_seg_cell(ui, c(4, 2), s, setter, &params.dub_swirls, "dub_swirls", "Swirls", &["Off", "1", "2", "3"], f);
        }
        _ => {
            knob_cell_m(ui, c(0, 1), s, setter, &params.sat_amount, "sat_amount", "Amount", false, true, None, f, Some((&params.motion, crate::modulation::T_COLOUR, None)), None);
            knob_cell(ui, c(1, 1), s, setter, &params.sat_drive, "sat_drive", "Drive", false, true, None, f);
            let curves = ["Classic", "Tube", "Smooth", "Grit", "Amp", "Transfo"];
            enum_stepper_cell(ui, c(2, 2), s, setter, &params.sat_curve, "sat_curve", "Curve", &curves, f);
            enum_cell(ui, c(4, 2), s, setter, &params.sat_routing, "sat_routing", "Route", &routes, true, f);
        }
    }
}

fn output_card(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, focus: &mut Option<String>) {
    let rect = r(o, s, l.right_x, l.output.0, l.right_w, l.output.1);
    let inner = card(ui, rect, "Output", s);
    // Levels on the left; ducking after a rule, in the columns Colour's
    // Route uses above it.
    let c = |col| cell_at(inner, 6, 1, 0, col, 1, s);
    knob_cell(ui, c(0), s, setter, &params.input_gain, "input_gain", "In", true, true, None, focus);
    knob_cell_m(ui, c(1), s, setter, &params.dry_wet, "dry_wet", "Mix", false, true, None, focus, Some((&params.motion, crate::modulation::T_MIX, None)), None);
    knob_cell(ui, c(2), s, setter, &params.output_gain, "output_gain", "Out", true, true, None, focus);
    // Ear guard (v2.43), and how far it has the echoes down right now
    toggle_cell(ui, c(3), s, setter, &params.ear_guard, "ear_guard", "Ear guard", "On", "Off", focus);
    let down = f32::from_bits(params.guard_db.load(std::sync::atomic::Ordering::Relaxed));
    if params.ear_guard.value() && down > 0.1 {
        let (_, _, val) = cell_parts(c(3), s);
        mono(ui.painter(), val, Align2::CENTER_CENTER, &format!("−{down:.1} dB"), 11.5, s, readable(theme::accent(1), PANEL()));
    }
    let xr = c(4).left() - 4.0 * s;
    rule(ui.painter(), Pos2::new(xr, inner.top() + 8.0 * s), Pos2::new(xr, inner.bottom() - 4.0 * s), s);
    knob_cell_m(ui, c(4), s, setter, &params.duck, "duck", "Duck", false, true, None, focus, Some((&params.motion, crate::modulation::T_DUCK, None)), None);
    knob_cell(ui, c(5), s, setter, &params.duck_release, "duck_release", "Release", false, true, None, focus);
    // How far the echoes dip right now: a thin arc inside the Duck dial, on
    // the knob's own scale (its full sweep is 24 dB).
    let dip = f32::from_bits(params.duck_dip_db.load(std::sync::atomic::Ordering::Relaxed));
    if dip > 0.05 {
        let (ctl, _, _) = cell_parts(c(4), s);
        let arc = dial_arc(ctl.center(), (g::DIAL * 0.5 - 5.5) * s, 0.0, (dip / 24.0).min(1.0));
        ui.painter().add(egui::Shape::line(arc, Stroke::new(2.0 * s, G())));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Taps menu: pattern tools (v2.37) — shape every tap at once on the picked axis
// ─────────────────────────────────────────────────────────────────────────────

/// Write the current shape across the live taps on axis `ai`, between its
/// range's pins. One gesture per tap.
fn pattern_apply(params: &AfterechoParams, setter: &ParamSetter, sc: &ScopeState, ai: usize) {
    let Some(shape) = sc.pat_shape else { return };
    let axis = Axis::ALL[ai];
    let range = sc.pat_range[ai].unwrap_or_else(|| axis.default_range());
    let vals = pattern_values(axis, shape, sc.pat_flip, n_active(params), sc.pat_seed, range);
    for (tap, v) in params.taps.iter().zip(vals) {
        axis.set(setter, tap, v);
    }
}

/// Keep every tap's value on axis `ai` so Undo can put it back (the last 32).
fn pattern_keep(params: &AfterechoParams, sc: &mut ScopeState, ai: usize) {
    let axis = Axis::ALL[ai];
    let undo = &mut sc.pat_undo[ai];
    undo.push(core::array::from_fn(|i| axis.get(&params.taps[i])));
    if undo.len() > 32 {
        undo.remove(0);
    }
}

/// The Taps menu, under its button: Shape (Alternate, Ramp down, Ramp up,
/// Random), Range (two pins; the shape re-spreads as they move), Reset and
/// Undo, all on the picked axis. Alternate clicked again flips, Random
/// re-rolls. A click outside closes it.
fn taps_menu(ui: &mut egui::Ui, s: f32, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState) {
    let Some(btn) = sc.pat_btn.filter(|_| sc.pat_open) else { return };
    let ch = params.channel_mode.value();
    let ai = if ch == ChannelMode::Mono && sc.y_axis == 0 { 1 } else { sc.y_axis };
    let axis = Axis::ALL[ai];
    let n = n_active(params);
    let area = egui::Area::new(Id::new("afterecho_taps_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(btn.left_bottom() + Vec2::new(0.0, 8.0 * s))
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(330.0, 206.0) * s, Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(rect, g::R_CARD * s, PANEL());
            p.rect_stroke(rect, g::R_CARD * s, Stroke::new(1.0 * s, theme::line2()), egui::StrokeKind::Inside);
            let at = |dx: f32, dy: f32, w: f32, h: f32| Rect::from_min_size(rect.min + Vec2::new(dx, dy) * s, Vec2::new(w, h) * s);

            text(&p, at(16.0, 12.0, 200.0, 20.0).left_center(), Align2::LEFT_CENTER, &format!("Taps · {}", axis.name()), 13.0, s, INK());
            text(&p, at(214.0, 12.0, 100.0, 20.0).right_center(), Align2::RIGHT_CENTER, &format!("{n} taps"), 11.0, s, SOFT());

            group_head(&p, at(16.0, 42.0, 100.0, 12.0).left_center(), Align2::LEFT_CENTER, "Shape", s);
            let cur = sc.pat_shape.and_then(|sh| PatShape::ALL.iter().position(|&x| x == sh));
            let labels = ["Alternate", "Ramp down", "Ramp up", "Random"];
            if let Some(i) = seg_again(ui, at(16.0, 50.0, 298.0, 28.0), &labels, cur, "sc_pat_shape") {
                let shape = PatShape::ALL[i];
                match shape {
                    PatShape::Alternate => sc.pat_flip = sc.pat_shape == Some(shape) && !sc.pat_flip,
                    PatShape::Random => sc.pat_seed = sc.pat_seed.wrapping_mul(7919).wrapping_add(17),
                    _ => {}
                }
                pattern_keep(params, sc, ai);
                sc.pat_shape = Some(shape);
                pattern_apply(params, setter, sc, ai);
            }

            // Range: two pins on the axis's own scale; the span between is ink.
            group_head(&p, at(16.0, 94.0, 100.0, 12.0).left_center(), Align2::LEFT_CENTER, "Range", s);
            let mut range = sc.pat_range[ai].unwrap_or_else(|| axis.default_range());
            let track = at(24.0, 104.0, 282.0, 22.0);
            let xu = |u: f32| track.left() + u * track.width();
            let cy = track.center().y;
            p.line_segment([Pos2::new(track.left(), cy), Pos2::new(track.right(), cy)], Stroke::new(3.0 * s, theme::line2()));
            p.line_segment([Pos2::new(xu(range[0]), cy), Pos2::new(xu(range[1]), cy)], Stroke::new(3.0 * s, INK()));
            for u in range {
                p.circle_filled(Pos2::new(xu(u), cy), 7.0 * s, INK());
            }
            let d = drag_at(ui, Id::new("sc_pat_range"), track.expand2(Vec2::new(10.0, 6.0) * s));
            let u_at = |x: f32| {
                let u = ((x - track.left()) / track.width()).clamp(0.0, 1.0);
                match axis {
                    Axis::Type => (u * 2.0).round() / 2.0,
                    Axis::Slope => (u * (SLOPES - 1) as f32).round() / (SLOPES - 1) as f32,
                    _ => u,
                }
            };
            if d.started {
                if let Some(pos) = d.pos {
                    let u = u_at(pos.x);
                    sc.pat_pin = Some(if (u - range[0]).abs() <= (u - range[1]).abs() { 0 } else { 1 });
                    if sc.pat_shape.is_some() {
                        pattern_keep(params, sc, ai);
                    }
                }
            }
            if let (true, Some(k), Some(pos)) = (d.pressing, sc.pat_pin, d.pos) {
                let u = u_at(pos.x);
                range[k] = if k == 0 { u.min(range[1]) } else { u.max(range[0]) };
                if sc.pat_range[ai] != Some(range) {
                    sc.pat_range[ai] = Some(range);
                    pattern_apply(params, setter, sc, ai);
                }
            }
            if d.released {
                sc.pat_pin = None;
            }
            let end = |u: f32| axis.text(axis.value(axis.u_norm(u)), Labels::of(params));
            mono(&p, at(16.0, 130.0, 150.0, 16.0).left_center(), Align2::LEFT_CENTER, &end(range[0]), 12.0, s, INK2());
            mono(&p, at(164.0, 130.0, 150.0, 16.0).right_center(), Align2::RIGHT_CENTER, &end(range[1]), 12.0, s, INK2());

            rule(&p, at(16.0, 158.0, 0.0, 0.0).min, at(314.0, 158.0, 0.0, 0.0).min, s);
            if button(ui, at(16.0, 168.0, 76.0, 26.0), "Reset", false, "sc_pat_reset") {
                pattern_keep(params, sc, ai);
                for tap in params.taps.iter().take(n) {
                    axis.set(setter, tap, axis.neutral());
                }
                sc.pat_shape = None;
            }
            let can_undo = !sc.pat_undo[ai].is_empty();
            let undo = at(100.0, 168.0, 76.0, 26.0);
            if button(ui, undo, "Undo", false, "sc_pat_undo") {
                if let Some(vals) = sc.pat_undo[ai].pop() {
                    for (tap, v) in params.taps.iter().zip(vals) {
                        axis.set(setter, tap, v);
                    }
                    sc.pat_shape = None;
                }
            }
            if !can_undo {
                p.rect_filled(undo, g::R_CTL * s, theme::with_alpha(PANEL(), 150));
            }
            text(&p, at(184.0, 168.0, 130.0, 26.0).right_center(), Align2::RIGHT_CENTER, "every tap, this axis", 11.0, s, SOFT());
        });
    // A click outside closes it, except on its own button (which toggles it).
    let on_btn = ui.ctx().input(|i| i.pointer.interact_pos()).is_some_and(|pos| btn.contains(pos));
    if !on_btn && area.response.clicked_elsewhere() {
        sc.pat_open = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings menu (Tide's: label column, sections split by hairlines, 16 px
// radius, fixed 8 px row gaps)
// ─────────────────────────────────────────────────────────────────────────────

/// The ☰ Options panel (softsystems-ui): Theme, Palette, Text size and Size,
/// then Afterecho's own rows: Accent, the Detune quality, the preset folder.
#[allow(clippy::too_many_arguments)]
fn options_panel(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState,
    egui_state: &EguiState, zoom: f32, licence: &softsystems_licence::Licence, lic: Option<options::LicenceView>,
) {
    let ios = cfg!(target_os = "ios");
    let at = o + Vec2::new(l.pad, g::HEAD_Y + g::HEAD_H + 8.0) * s;
    // the ☰ button header::head drew: the panel hangs from it
    let strip = header::strip(Rect::from_min_size(o, Vec2::new(l.w, l.h) * s));
    let sq = header::HEADER_SQ * s;
    let burger = Rect::from_center_size(Pos2::new(strip.left() + sq * 0.5, strip.center().y), Vec2::splat(sq));
    let mut reveal = false;
    // the Accent row edits copies (the panel holds the look); they go back into it below
    let (mut accent_on, mut custom) = (sc.look.accent as usize, sc.look.custom);
    let mut accent_changed = false;
    let ScopeState { look, options, .. } = sc;
    let env = Env { product: &PRODUCT, look, zoom: (!ios).then_some(Zoom { now: zoom, steps: &options::ZOOM_STEPS }), licence: lic };
    let out = options.show(ui, at, burger, env, |rows| {
        let colors: Vec<Color32> = ACCENTS.iter().map(|a| a.get(softsystems_ui::t().dark)).collect();
        accent_changed = rows.accent("Accent", &colors, &mut accent_on, &mut custom);
        rows.section("Sound");
        // how detuned taps are shifted (v2.45): saved with the sound
        let clean = params.detune_quality.value() == crate::params::DetuneQuality::Clean;
        if let Some(i) = rows.seg("Detune", &["Light", "Clean · 3× CPU"], clean as usize) {
            if (i == 1) != clean {
                set_norm(setter, &params.detune_quality, i as f32);
            }
        }
        // the preset folder lives inside the app sandbox on iPad: nothing to reveal there
        if !ios {
            rows.section("Presets");
            reveal = rows.button("Folder", "Open preset folder");
        }
    });
    if let Some(act) = out.licence {
        act.apply(licence);
    }
    if accent_changed {
        sc.look.accent = accent_on.min(ACCENTS.len()) as u8;
        sc.look.custom = custom;
    }
    // a new instance opens at the size last picked (a project reopens at its own)
    if let Some(z) = out.zoom {
        egui_state.set_requested_size(((l.w * z).round() as u32, ((l.h + sc.banner_h) * z).round() as u32));
        sc.look.size = Some(z);
    }
    // every ☰ change is saved as it happens (a host may unload without closing the editor)
    if out.look_changed || accent_changed || out.zoom.is_some() {
        sc.save_look();
    }
    if reveal {
        reveal_presets();
        sc.options.set_open(false);
    }
}

fn reveal_presets() {
    let dir = crate::presets::default_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&dir).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every palette, theme and accent Afterecho offers, as the theme each sets.
    fn every_theme() -> Vec<(String, Theme)> {
        let mut out = Vec::new();
        for palette in [0u8, 1] {
            for th in 0..3 {
                for (k, accent) in ACCENTS.iter().enumerate() {
                    let mut look = Appearance { palette, ..Appearance::default_for(&PRODUCT) };
                    look.set_theme(th);
                    let t = Theme::of(&Product { accent: *accent, ..PRODUCT }, &look, 1.0);
                    out.push((format!("palette {palette} theme {th} accent {k}"), t));
                }
            }
        }
        out
    }

    /// The shared contrast rule (owner, 2026-11-02; migration step 11): a
    /// data colour reads at 3:1 as a mark (the taps, the sources' dots,
    /// lines and bars) and 4.5:1 as text (a matrix cell's value), on the
    /// grounds it's drawn on.
    #[test]
    fn data_colours_read_on_their_grounds() {
        use softsystems_ui::tokens::contrast;
        let mut bad = Vec::new();
        for (name, th) in every_theme() {
            th.make_current();
            let c = th.c;
            let marks: Vec<(String, Color32)> = (0..6)
                .map(|i| (format!("hue {i}"), th.hue(i)))
                .chain([(String::from("first tap"), tap_col(0, 16)), (String::from("last tap"), tap_col(15, 16))])
                .collect();
            for (what, col) in &marks {
                for (ground, g) in [("page", c.page), ("card", c.panel), ("well", c.well)] {
                    let k = contrast(*col, g);
                    if k < 3.0 {
                        bad.push(format!("{name}: {what} on {ground} {k:.2}"));
                    }
                }
            }
            for i in 0..6 {
                let k = contrast(readable(th.hue(i), c.panel), c.panel);
                if k < 4.5 {
                    bad.push(format!("{name}: hue {i} as text on card {k:.2}"));
                }
            }
        }
        assert!(bad.is_empty(), "{} short:\n{}", bad.len(), bad.join("\n"));
    }

    #[test]
    fn the_layout_fits_its_page_without_overlapping() {
        {
            let l = &LAYOUT;
            let rect = |(x, y, w, h): (f32, f32, f32, f32)| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
            let right = |(y, h): (f32, f32)| rect((l.right_x, y, l.right_w, h));
            let regions = [
                rect(l.picture),
                rect(l.lane),
                right(l.time),
                right(l.colour),
                right(l.output),
            ];
            let page = Rect::from_min_size(Pos2::new(l.pad, g::HEAD_Y + g::HEAD_H), Vec2::new(l.w - 2.0 * l.pad, l.h - g::HEAD_Y - g::HEAD_H - 16.0));
            for (i, a) in regions.iter().enumerate() {
                assert!(page.contains_rect(*a), "region {i} of the {}×{} page leaves it", l.w, l.h);
                for b in regions.iter().skip(i + 1) {
                    assert!(!a.intersects(*b), "regions overlap on the {}×{} page", l.w, l.h);
                }
            }
            // every card holds its rows of cells: Time & taps three, Colour
            // (under its tabs) and Output one, the lane's controls two (one
            // beside the curves), and the Tap tab two
            let fits = |room: f32, rows: f32| room >= rows * g::CELL_H;
            assert!(fits(l.time.1 - 38.0, 3.0), "Time & taps, {}×{}", l.w, l.h);
            assert!(fits(l.colour.1 - 41.0, 1.0) && fits(l.output.1 - 38.0, 1.0));
            let (lane_h, ctl_rows, ctl_h, tap_h) = (l.lane.3, 2.0, l.lane.3 - 48.0, l.lane.3 - 48.0);
            assert!(fits(ctl_h, ctl_rows) && fits(tap_h, 2.0), "lane {lane_h}, {}×{}", l.w, l.h);
            // header: the channel modes, the preset box and Clear · Freeze · A/B all get their
            // width, with a title and a beta badge as wide as they come (about 190 px)
            let window = Rect::from_min_size(Pos2::ZERO, Vec2::new(l.w, l.h));
            let strip = header::strip(window);
            let sq = header::HEADER_SQ;
            let title = Rect::from_min_size(Pos2::new(strip.left() + sq + header::TITLE_GAP, strip.top()), Vec2::new(190.0, 20.0));
            let h = header::Head {
                strip,
                menu_rect: Rect::from_min_size(strip.min, Vec2::splat(sq)),
                theme_rect: Rect::from_min_size(Pos2::new(strip.right() - sq, strip.top()), Vec2::splat(sq)),
                title,
                menu: false,
                theme: false,
                window,
            };
            let sl = header::slots(&h, title.right(), &header::Want { left: HEAD_TABS, right: &HEAD_RIGHT, preset: header::PRESET_W });
            assert!(sl.fits, "{sl:?}");
        }
    }

    #[test]
    fn the_old_accent_file_moves_into_the_look_once() {
        let dir = std::env::temp_dir().join(format!("afterecho-accent-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (old, new) = (dir.join("accent.json"), dir.join("appearance.json"));
        std::fs::write(&old, r#"{"on": 6, "custom": {"hue": 200.0, "level": 0.4}}"#).unwrap();
        let mut look = default_look();
        OldAccent::import_at(&old, &new, &mut look);
        assert_eq!((look.accent, look.custom.hue, look.custom.level), (6, 200.0, 0.4));
        assert!(!old.exists(), "the old file goes once it's moved");
        let saved = Appearance::read(&new).unwrap();
        assert_eq!((saved.accent, saved.palette), (6, 1));
        // the custom accent (past the colours) picks the user's own colour
        assert!(saved.uses_custom(ACCENTS.len()));
        // no old file: nothing changes
        let mut again = default_look();
        OldAccent::import_at(&old, &new, &mut again);
        assert_eq!(again, default_look());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_shape_places_the_taps_between_the_ends() {
        let t = |shape, flip| pattern_t(shape, flip, 4, 5);
        assert_eq!(t(PatShape::Alternate, false), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(t(PatShape::Alternate, true), [1.0, 0.0, 1.0, 0.0]);
        let down = t(PatShape::RampDown, false);
        assert!(down[0] == 0.0 && down[3] == 1.0 && down.windows(2).all(|w| w[0] < w[1]));
        let up = t(PatShape::RampUp, false);
        assert!(up[0] == 1.0 && up[3] == 0.0);
        // Random: inside the range, the same for a seed, new for another
        let r = pattern_t(PatShape::Random, false, 16, 9);
        assert!(r.iter().all(|&x| (0.0..1.0).contains(&x)));
        assert_eq!(r, pattern_t(PatShape::Random, false, 16, 9));
        assert_ne!(r, pattern_t(PatShape::Random, false, 16, 10));
    }

    #[test]
    fn a_shape_spans_each_axis_between_its_pins() {
        let close = |a: f32, b: f32| (a - b).abs() <= 1e-3 * b.abs().max(1.0);
        // the default ranges: L 70 to R 70, −24 to 0 dB, 500 Hz to 20 kHz, Q 0.7 to 6
        let v = |axis: Axis, shape| pattern_values(axis, shape, false, 8, 3, axis.default_range());
        let pan = v(Axis::Pan, PatShape::Alternate);
        assert!(close(pan[0], -0.7) && close(pan[1], 0.7), "Pan alternates L and R: {pan:?}");
        let gain = v(Axis::Gain, PatShape::RampDown);
        assert!(close(gain[0], 0.0) && close(gain[7], -24.0), "Gain fades out from the top: {gain:?}");
        let cut = v(Axis::Cutoff, PatShape::RampUp);
        assert!(close(cut[0], 500.0) && close(cut[7], 20_000.0), "Cutoff opens up: {cut:?}");
        let q = v(Axis::Reso, PatShape::Alternate);
        assert!(close(q[0], 6.0) && close(q[1], 0.7), "Reso alternates high and low: {q:?}");
        // Type stays on its three values, HP at the top
        let ty = v(Axis::Type, PatShape::RampDown);
        assert_eq!(ty, [2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
        // a narrower range: the shape only reaches the pins
        let mid = pattern_values(Axis::Pan, PatShape::RampDown, false, 3, 1, [0.5, 0.75]);
        assert!(close(mid[0], 0.0) && close(mid[2], 0.5), "C to R 50: {mid:?}");
    }

    #[test]
    fn the_new_axes_read_back_their_own_values() {
        for q in [0.1, 0.707, 3.0, 12.0] {
            assert!((Axis::Reso.value(Axis::Reso.norm(q)) - q).abs() < 1e-3 * q, "Q {q}");
        }
        for ty in [0.0, 1.0, 2.0] {
            assert_eq!(Axis::Type.value(Axis::Type.norm(ty)), ty);
            assert_eq!(type_of(ty) as usize, ty as usize);
        }
        assert_eq!(Axis::Type.text(2.0, Labels { ch: ChannelMode::Stereo, det_range: 1.0 }), "HP");
    }

    #[test]
    fn detune_reads_in_cents_then_semitones() {
        assert_eq!(detune_text(25.0, 1.0), "+25 ct");
        assert_eq!(detune_text(-100.0, 1.0), "−1 st");
        assert_eq!(detune_text(0.0, 12.0), "0 ct");
        assert_eq!(detune_text(100.0, 12.0), "+12 st");
        assert_eq!(detune_text(-62.5, 12.0), "−7.5 st");
        assert_eq!(detune_text(4.0, 12.0), "+48 ct");
        let lb = Labels { ch: ChannelMode::Stereo, det_range: 7.0 };
        assert_eq!(Axis::Detune.grid(lb)[0].1, "+7 st");
        assert_eq!(Axis::Detune.grid(lb)[1].1, "+3.5");
        let lb = Labels { ch: ChannelMode::Stereo, det_range: 1.0 };
        let g: Vec<String> = Axis::Detune.grid(lb).into_iter().map(|(_, t)| t).collect();
        assert_eq!(g, ["+100 ct", "+50", "0", "−50", "−100"]);
    }
}

