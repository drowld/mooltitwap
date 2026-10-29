//! Scope-first page — the redesign on the "Afterecho redesign" canvas
//! (2026-10-27). The tap field is the picture, the Echo lane shows the
//! repeats, and a right column holds last touched · Time & taps · Colour ·
//! Output. Two page shapes share it (see [`Shape`]); every rect is in logical
//! px, scaled by `s` at use. Built on the Tide-derived UI layer in
//! [`crate::ui`], not on epoche-ui; modelled on Tide's `editor_scope.rs`.
//!
//! First pass: page frame, header, Settings menu and the right column, wired to
//! the real parameters. The tap field, the Echo lane drawing and the new
//! functions come next.
#![allow(deprecated)]

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use nice_plug::prelude::*;
use nice_plug_egui::EguiState;
use std::path::PathBuf;
use std::sync::Arc;

use crate::gui::{gain_db_to_pos, pos_to_gain_db, EditorState, VizData};
use crate::bounce;
use crate::params::{ChannelMode, CrossfeedMode, FilterType, PetalParams, SyncMode, TapParams, NUM_TAPS};
use crate::topology;
use crate::ui::epo;
use crate::ui::theme::{self, G, INK, INK2, INSET, PANEL, PAPER, SOFT};
use crate::ui::tokens as tk;

/// Element sizes (logical px), the same on every page shape — Tide's, so the
/// plug-ins match.
pub(crate) mod g {
    pub const HEAD_Y: f32 = 16.0;
    pub const HEAD_H: f32 = 38.0;
    pub const R_CARD: f32 = 10.0;
    pub const R_CTL: f32 = 6.0;
    /// A control cell: its name, the control band, then its value. A knob
    /// fills the band; pills (`CTL_H`) sit centred in it.
    pub const CTL_H: f32 = 30.0;
    pub const LBL_H: f32 = 13.0;
    /// Knob diameter, between Tide's 28 and the τόπος sheet's 48. On touch a
    /// knob answers within 44 px of its centre whatever its size.
    pub const DIAL: f32 = 36.0;
    pub const BAND: f32 = DIAL;
    pub const CELL_H: f32 = BAND + 2.0 + 2.0 * LBL_H;
}

/// The page shapes. Controls and text are the same size on both, so the
/// narrower `Tall` page simply gets a bigger zoom on a tablet. Desktop always
/// uses `Wide`; on iOS the editor picks one every frame from the space the
/// host gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Wide,
    Tall,
}

impl Shape {
    pub fn layout(self) -> &'static Layout {
        match self {
            Shape::Wide => &WIDE,
            Shape::Tall => &TALL,
        }
    }

    /// The shape whose controls come out biggest in a `space`-sized view: the
    /// tall page on an iPad full screen, the wide one in short host panels.
    pub fn pick(space: Vec2) -> Shape {
        let zoom = |l: &Layout| (space.x / l.w).min(space.y / l.h);
        if zoom(&TALL) > zoom(&WIDE) { Shape::Tall } else { Shape::Wide }
    }
}

/// Where the regions sit on one page shape (logical px). Rects are
/// `(x, y, w, h)`; the right-column cards are `(y, h)` at `right_x`.
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pad: f32,
    picture: (f32, f32, f32, f32),
    lane: (f32, f32, f32, f32),
    /// The lane's controls sit under its curves instead of beside them.
    lane_stacked: bool,
    right_x: f32,
    right_w: f32,
    focus: (f32, f32),
    time: (f32, f32),
    colour: (f32, f32),
    output: (f32, f32),
    /// Header: channel-mode tabs width, preset box width (it's centred).
    tabs_w: f32,
    preset_w: f32,
}

/// Desktop, and iPad in short host panels. Cards are sized to their rows,
/// as tight as Tide's.
const WIDE: Layout = Layout {
    w: 1366.0,
    h: 660.0,
    pad: 20.0,
    picture: (20.0, 70.0, 800.0, 330.0),
    lane: (20.0, 412.0, 800.0, 232.0),
    lane_stacked: false,
    right_x: 834.0,
    right_w: 512.0,
    focus: (70.0, 56.0),
    time: (138.0, 250.0),
    colour: (400.0, 117.0),
    output: (529.0, 115.0),
    tabs_w: 220.0,
    preset_w: 270.0,
};

/// iPad full screen: narrower, so it gets a bigger zoom. Close to 4:3, the
/// shape of most iPads (the 10.5" one included), so it fills the screen.
const TALL: Layout = Layout {
    w: 1050.0,
    h: 780.0,
    pad: 18.0,
    picture: (18.0, 70.0, 560.0, 416.0),
    lane: (18.0, 498.0, 560.0, 264.0),
    lane_stacked: true,
    right_x: 592.0,
    right_w: 440.0,
    focus: (70.0, 80.0),
    time: (162.0, 300.0),
    colour: (474.0, 140.0),
    output: (626.0, 136.0),
    tabs_w: 200.0,
    preset_w: 220.0,
};

/// Default text-size steps (Settings → Text size); one step is 6 %.
const FONT_STEPS_DEFAULT: i32 = 4;
const FONT_SCALE_DEFAULT: f32 = 1.0 + 0.06 * FONT_STEPS_DEFAULT as f32;
/// Afterecho's look: the Mono page (Paper when light) with its signature
/// accent (Tide is warm + slate), indices into the theme's variants and accent
/// hues.
const DARK_VARIANT_DEFAULT: u8 = 4;
const ACCENT_DEFAULT: u8 = 8;
/// Raised when the look's defaults change: a saved look from before takes the
/// new palette and accent once; its font and light/dark choice stay.
const LOOK_VERSION: u8 = 2;

/// Text size: `px` at the default text-size setting, following Settings → Text size.
fn fs(px: f32, s: f32) -> f32 {
    px * s * theme::font_scale() / FONT_SCALE_DEFAULT
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

/// The look, saved once per device (not with the project), so a new instance
/// in any host opens in the look last picked.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
struct Appearance {
    dark: bool,
    dark_variant: u8,
    accent: u8,
    font_face: u8,
    font_steps: i32,
    /// The look version it was saved under (0 before the field existed).
    #[serde(default)]
    look: u8,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: true,
            dark_variant: DARK_VARIANT_DEFAULT,
            accent: ACCENT_DEFAULT,
            font_face: 0,
            font_steps: FONT_STEPS_DEFAULT,
            look: LOOK_VERSION,
        }
    }
}

impl Appearance {
    /// Next to the presets folder, not inside it.
    fn file() -> PathBuf {
        let dir = crate::presets::data_dir();
        dir.parent().map(|p| p.to_path_buf()).unwrap_or(dir).join("appearance.json")
    }

    /// The saved choices, or the defaults when there's no (readable) file.
    fn load() -> Self {
        let mut a: Self = std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if a.look < LOOK_VERSION {
            a.dark_variant = DARK_VARIANT_DEFAULT;
            a.accent = ACCENT_DEFAULT;
            a.look = LOOK_VERSION;
        }
        Self {
            dark_variant: a.dark_variant.min(theme::DARK_VARIANTS - 1),
            accent: a.accent.min(theme::ACCENTS - 1),
            font_face: a.font_face.min(1),
            font_steps: a.font_steps.clamp(-5, 10),
            ..a
        }
    }

    fn save(&self) {
        let path = Self::file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }
}

/// Editor-only state of this page (lives in [`EditorState`]).
#[derive(Default)]
pub struct ScopeState {
    inited: bool,
    look: Appearance,
    look_saved: Option<Appearance>,
    font_applied: Option<u8>,
    shape: Shape,
    show_menu: bool,
    /// Hamburger hit rect — a click there doesn't count as "outside the menu".
    hamb_rect: Option<Rect>,
    /// Param id of the control last touched; the focus card follows it.
    focus: Option<String>,
    /// The tap selected in the picture.
    sel: Option<usize>,
    /// Picture: Y axis 0 Pan · 1 Gain · 2 Cutoff; view 0 Field · 1 Bars.
    y_axis: usize,
    view: usize,
    /// Field drags move only the Y value; the taps keep their time.
    lock_time: bool,
    /// Lane tab: 0 Echo · 1 Motion · 2 Tap (the selected tap's parameters).
    lane_tab: usize,
    /// The tab the lane returns to when the Tap tab closes.
    lane_back: usize,
    /// The tap the Tap tab opens on when nothing is selected.
    last_tap: usize,
    /// Colour card tab: 0 Diffuse · 1 Tape · 2 Saturate.
    colour_tab: usize,
}

impl ScopeState {
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
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    egui_state: &EguiState,
    tempo: f32,
) {
    let ios = cfg!(target_os = "ios");
    let mut open_presets = false;
    {
        let sc = &mut st.scope;
        if !sc.inited {
            sc.inited = true;
            sc.look = Appearance::load();
            sc.look_saved = Some(sc.look);
            // Desktop previews, to check a state without clicking to it:
            // AFTERECHO_PREVIEW=tall,bars,gain|cutoff,sel3,tap,lock,presets,light,tape
            // (the iPad page, the Bars view, the Y axis, a selected tap, the Tap
            // tab, Lock time, the preset browser, the light page, Colour's Tape
            // tab). A preview never saves the look it shows.
            if let (false, Ok(v)) = (ios, std::env::var("AFTERECHO_PREVIEW")) {
                for t in v.split(',').map(str::trim) {
                    match t {
                        "tall" => sc.shape = Shape::Tall,
                        "bars" => sc.view = 1,
                        "gain" => sc.y_axis = 1,
                        "cutoff" => sc.y_axis = 2,
                        "tap" => sc.lane_tab = 2,
                        "lock" => sc.lock_time = true,
                        "presets" => open_presets = true,
                        "light" => sc.look.dark = false,
                        "tape" => sc.colour_tab = 1,
                        _ => {
                            if let Some(n) = t.strip_prefix("sel").and_then(|n| n.parse::<usize>().ok()) {
                                sc.sel = n.checked_sub(1);
                            }
                        }
                    }
                }
                sc.look_saved = Some(sc.look);
            }
        }
        if sc.look_saved != Some(sc.look) {
            sc.look.save();
            sc.look_saved = Some(sc.look);
        }
    }
    if open_presets {
        st.preset_open = true;
    }
    {
        let sc = &mut st.scope;
        theme::set_font_scale((1.0 + 0.06 * sc.look.font_steps as f32).clamp(0.7, 1.6));
        if sc.font_applied != Some(sc.look.font_face) {
            sc.font_applied = Some(sc.look.font_face);
            let choice = if sc.look.font_face == 0 {
                theme::FontChoice::KarlaInconsolata
            } else {
                theme::FontChoice::PlexDidot
            };
            theme::apply_fonts(ui.ctx(), choice);
        }
        // Theme, variant and accent live in process-wide atomics that every
        // instance in the host shares, so publish this instance's look every
        // frame (plain stores) — otherwise a second Afterecho repaints this one.
        theme::set_theme(sc.look.dark);
        theme::set_dark_variant(sc.look.dark_variant);
        theme::set_accent(sc.look.accent);
    }

    // Desktop: zoom follows the window width and the height keeps the page's
    // aspect. iOS/AUv3: the host sizes the view (`with_host_resize`), so pick
    // the shape whose controls come out biggest there, then fit and centre it.
    let vp = ui.ctx().screen_rect();
    if ios {
        st.scope.shape = Shape::pick(vp.size());
    }
    let lay = st.scope.shape.layout();
    let zoom = if ios {
        (vp.width() / lay.w).min(vp.height() / lay.h).clamp(0.3, 3.0)
    } else {
        (egui_state.size().0 as f32 / lay.w).clamp(0.5, 2.5)
    };
    theme::apply_zoom(ui.ctx(), zoom);
    if !ios {
        let (w, h) = egui_state.size();
        let want_h = (w as f32 * lay.h / lay.w).round() as u32;
        if h.abs_diff(want_h) > 2 {
            egui_state.set_requested_size((w, want_h));
        }
    }

    ui.painter().rect_filled(vp, 0.0, PAPER());
    let s = zoom;
    let o = if ios {
        vp.min + ((vp.size() - Vec2::new(lay.w, lay.h) * s) * 0.5).max(Vec2::ZERO)
    } else {
        vp.min
    };
    page(ui, o, s, lay, params, setter, st, tempo);
    // The preset browser drops over the page, as on Tide.
    if st.preset_open {
        crate::palette_scope::draw(ui, params, setter, st);
    }
    ui.ctx().request_repaint();
}

#[allow(clippy::too_many_arguments)]
fn page(
    ui: &mut egui::Ui,
    o: Pos2,
    s: f32,
    l: &Layout,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    st: &mut EditorState,
    tempo: f32,
) {
    header(ui, o, s, l, params, setter, st);
    st.palette_top = o.y + (g::HEAD_Y + g::HEAD_H + 6.0) * s;
    picture(ui, o, s, l, params, setter, &mut st.scope, tempo);
    lane(ui, o, s, l, params, setter, &mut st.scope, tempo);
    focus_card(ui, o, s, l, params, setter, &mut st.scope.focus, tempo);
    time_card(ui, o, s, l, params, setter, &mut st.scope.focus, tempo);
    colour_card(ui, o, s, l, params, setter, &mut st.scope);
    output_card(ui, o, s, l, params, setter, &mut st.scope.focus);
    settings_menu(ui, o, s, l, &mut st.scope);
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
pub(crate) fn seg(ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, s: f32, px: f32, enabled: bool) -> Option<usize> {
    seg_ex(ui, rect, labels, active, salt, s, px, &[enabled; 8][..labels.len()])
}

/// [`seg`] with each option enabled on its own. Every option is laid out as
/// text on a hairline; the pick is ink, underlined, so each choice reads as its
/// own before it's touched.
#[allow(clippy::too_many_arguments)]
fn seg_ex(
    ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, s: f32, px: f32, enabled: &[bool],
) -> Option<usize> {
    let p = ui.painter().clone();
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0 * s, theme::line()));
    let w = rect.width() / labels.len().max(1) as f32;
    let mut picked = None;
    for (i, l) in labels.iter().enumerate() {
        let b = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * w, rect.top()), Vec2::new(w, rect.height()));
        let resp = ui.interact(b, ui.id().with((salt, i)), Sense::click());
        let enabled = enabled.get(i).copied().unwrap_or(true);
        let alpha = if enabled { 255 } else { 90 };
        let on = i == active;
        if on {
            let bar = Rect::from_min_max(Pos2::new(b.left() + 3.0 * s, b.bottom() - 2.0 * s), Pos2::new(b.right() - 3.0 * s, b.bottom()));
            p.rect_filled(bar, 0.0, theme::with_alpha(INK(), alpha));
        }
        let col = if on { INK() } else if enabled && resp.hovered() { INK2() } else { SOFT() };
        text(&p, b.center(), Align2::CENTER_CENTER, l, px, s, theme::with_alpha(col, alpha));
        if enabled && epo::tap_or_click(ui, &resp) && !on {
            picked = Some(i);
        }
    }
    picked
}

/// [`seg`] standing up: options stacked on a hairline, the pick ink with a bar
/// on its left.
fn seg_v(ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, s: f32, px: f32) -> Option<usize> {
    let p = ui.painter().clone();
    p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0 * s, theme::line()));
    let h = rect.height() / labels.len().max(1) as f32;
    let mut picked = None;
    for (i, l) in labels.iter().enumerate() {
        let b = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + i as f32 * h), Vec2::new(rect.width(), h));
        let resp = ui.interact(b, ui.id().with((salt, i)), Sense::click());
        let on = i == active;
        if on {
            p.rect_filled(Rect::from_min_max(Pos2::new(b.left(), b.top() + 3.0 * s), Pos2::new(b.left() + 2.0 * s, b.bottom() - 3.0 * s)), 0.0, INK());
        }
        let col = if on { INK() } else if resp.hovered() { INK2() } else { SOFT() };
        text(&p, Pos2::new(b.left() + 12.0 * s, b.center().y), Align2::LEFT_CENTER, l, px, s, col);
        if epo::tap_or_click(ui, &resp) && !on {
            picked = Some(i);
        }
    }
    picked
}

/// A flat button; `on` = pressed: outlined when off, filled with ink when on.
/// Returns its response (click via tap_or_click).
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, on: bool, salt: &str, s: f32, px: f32) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("btn", salt)), Sense::click());
    outline(ui.painter(), rect, on, s);
    let col = if on { PAPER() } else if resp.hovered() { INK() } else { INK2() };
    text(ui.painter(), rect.center(), Align2::CENTER_CENTER, label, px, s, col);
    resp
}

/// The frame every button shares: a hairline outline, or an ink fill when on.
fn outline(p: &egui::Painter, rect: Rect, on: bool, s: f32) {
    if on {
        p.rect_filled(rect, g::R_CTL * s, INK());
    } else {
        p.rect_stroke(rect, g::R_CTL * s, Stroke::new(1.0 * s, theme::line2()), egui::StrokeKind::Inside);
    }
}

/// Split a cell into (control row, name centre, value centre). The name sits
/// above the control and the value below it, so the name stays readable while
/// a finger is on the control.
fn cell_parts(cell: Rect, s: f32) -> (Rect, Pos2, Pos2) {
    let lab = Pos2::new(cell.center().x, cell.top() + g::LBL_H * 0.5 * s);
    let ctl = Rect::from_min_size(
        Pos2::new(cell.left(), cell.top() + (g::LBL_H + 1.0) * s),
        Vec2::new(cell.width(), g::BAND * s),
    );
    let val = Pos2::new(cell.center().x, ctl.bottom() + (1.0 + g::LBL_H * 0.5) * s);
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

fn paint_dial(p: &egui::Painter, c: Pos2, rad: f32, n: f32, bip: bool, s: f32, alpha: u8) {
    let arc = |v0: f32, v1: f32| dial_arc(c, rad, v0, v1);
    let sw = 3.0 * s;
    p.add(egui::Shape::line(arc(0.0, 1.0), Stroke::new(sw, theme::with_alpha(theme::line2(), alpha))));
    let (v0, v1) = if bip { (n.min(0.5), n.max(0.5)) } else { (0.0, n.max(0.004)) };
    p.add(egui::Shape::line(arc(v0, v1), Stroke::new(sw, theme::with_alpha(INK(), alpha))));
    let cap = rad - sw * 1.3;
    p.circle_filled(c, cap, theme::with_alpha(theme::PANEL2(), alpha));
    let a = (-135.0 + 270.0 * n).to_radians();
    let (sa, ca) = a.sin_cos();
    p.line_segment(
        [c + Vec2::new(sa, -ca) * cap * 0.2, c + Vec2::new(sa, -ca) * cap * 0.8],
        Stroke::new(2.0 * s, theme::with_alpha(INK(), alpha)),
    );
}

/// A knob cell. `id` is the param's id — the focus card follows it.
#[allow(clippy::too_many_arguments)]
fn knob_cell<P: Param>(
    ui: &mut egui::Ui, cell: Rect, s: f32, setter: &ParamSetter, param: &P, id: &str,
    label: &str, bip: bool, enabled: bool, disp: Option<String>, focus: &mut Option<String>,
) {
    let (ctl, lab, val) = cell_parts(cell, s);
    let dial = Rect::from_center_size(ctl.center(), Vec2::splat(g::DIAL * s));
    let hit = if theme::touch() { dial.expand(((theme::TOUCH_MIN_HIT - g::DIAL) * 0.5).max(0.0) * s) } else { dial };
    let resp = ui.interact(hit, ui.id().with(("knob", id)), Sense::click_and_drag());
    if enabled {
        let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if epo::knob_drive(ui, setter, param, hit, &resp) {
            *focus = Some(id.to_string());
        }
    }
    let n = param.modulated_normalized_value();
    let alpha = if enabled { 255 } else { 90 };
    let p = ui.painter();
    paint_dial(p, dial.center(), g::DIAL * 0.5 * s, n, bip, s, alpha);
    text(p, lab, Align2::CENTER_CENTER, label, 11.0, s, theme::with_alpha(INK2(), alpha));
    let v = disp.unwrap_or_else(|| param.normalized_value_to_string(n, true));
    mono(p, val, Align2::CENTER_CENTER, &v, 11.5, s, theme::with_alpha(INK(), alpha));
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
    let resp = button(ui, b, if on { on_txt } else { off_txt }, on, id, s, 12.0);
    if epo::tap_or_click(ui, &resp) {
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
    if let Some(i) = seg_ex(ui, b, names, cur, id, s, 12.0, enabled) {
        set_norm(setter, param, i as f32 / (n - 1) as f32);
        *focus = Some(id.to_string());
    }
    let alpha = if enabled.contains(&true) { 255 } else { 90 };
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, theme::with_alpha(INK2(), alpha));
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

/// A card: fill, title, and the inner rect for its cells.
fn card(ui: &egui::Ui, rect: Rect, title: &str, s: f32) -> Rect {
    let p = ui.painter();
    p.rect_filled(rect, g::R_CARD * s, PANEL());
    text(p, rect.min + Vec2::new(16.0, 10.0) * s, Align2::LEFT_TOP, title, 13.0, s, INK2());
    Rect::from_min_max(rect.min + Vec2::new(14.0, 30.0) * s, rect.max - Vec2::new(14.0, 8.0) * s)
}

/// Horizontal drag on a slider: touch claims its own finger, desktop uses the pointer.
fn slider_drive(ui: &egui::Ui, resp: &egui::Response, rect: Rect) -> Option<f32> {
    if theme::touch() {
        epo::touch_interact(ui, resp.id, rect).pos.map(|p| p.x)
    } else if resp.dragged() || resp.clicked() {
        resp.interact_pointer_pos().map(|p| p.x)
    } else {
        None
    }
}

/// An outlined diamond: a right-channel tap, apart from a muted (hollow) dot.
fn paint_diamond(p: &egui::Painter, c: Pos2, rad: f32, stroke: Stroke) {
    let pts = vec![c - Vec2::new(0.0, rad), c + Vec2::new(rad, 0.0), c + Vec2::new(0.0, rad), c - Vec2::new(rad, 0.0)];
    p.add(egui::Shape::closed_line(pts, stroke));
}

fn paint_cross(p: &egui::Painter, c: Pos2, rad: f32, col: Color32, s: f32) {
    let st = Stroke::new(1.5 * s, col);
    p.line_segment([c + Vec2::new(-rad, -rad), c + Vec2::new(rad, rad)], st);
    p.line_segment([c + Vec2::new(-rad, rad), c + Vec2::new(rad, -rad)], st);
}

// ─────────────────────────────────────────────────────────────────────────────
// Header: ☰ · wordmark · channel modes · preset · A/B · light/dark
// ─────────────────────────────────────────────────────────────────────────────

fn header(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter,
    st: &mut EditorState,
) {
    let (y, h) = (g::HEAD_Y, g::HEAD_H);

    // ☰ settings
    let hb = r(o, s, l.pad, y, 38.0, h);
    let hresp = ui.interact(hb, ui.id().with("sc_hamb"), Sense::click());
    let open = st.scope.show_menu;
    outline(ui.painter(), hb, open, s);
    for k in [-1.0f32, 0.0, 1.0] {
        let yy = hb.center().y + k * 5.0 * s;
        ui.painter().line_segment(
            [Pos2::new(hb.center().x - 8.0 * s, yy), Pos2::new(hb.center().x + 8.0 * s, yy)],
            Stroke::new(1.6 * s, if open { PAPER() } else if hresp.hovered() { INK() } else { INK2() }),
        );
    }
    if epo::tap_or_click(ui, &hresp) {
        st.scope.show_menu = !open;
    }
    st.scope.hamb_rect = Some(hb);

    // wordmark (its own family, so the UI font can change without it)
    ui.painter().text(
        o + Vec2::new(l.pad + 52.0, y + h * 0.5) * s,
        Align2::LEFT_CENTER,
        "Afterecho",
        FontId::new(22.0 * s, egui::FontFamily::Name(theme::WORDMARK.into())),
        INK(),
    );

    // channel modes
    let cm = enum_index(&params.channel_mode, 3);
    if let Some(i) = seg(ui, r(o, s, l.pad + 176.0, y, l.tabs_w, h), &["Stereo", "M/S", "Mono"], cm, "sc_chan", s, 13.0, true) {
        set_norm(setter, &params.channel_mode, i as f32 / 2.0);
    }

    // preset box, centred on the page: ‹ name ›
    let pb = r(o, s, (l.w - l.preset_w) * 0.5, y, l.preset_w, h);
    outline(ui.painter(), pb, false, s);
    let prev = Rect::from_min_size(pb.min, Vec2::new(38.0 * s, pb.height()));
    let next = Rect::from_min_size(Pos2::new(pb.right() - 38.0 * s, pb.top()), Vec2::new(38.0 * s, pb.height()));
    let mid = Rect::from_min_max(Pos2::new(prev.right(), pb.top()), Pos2::new(next.left(), pb.bottom()));
    let rp = ui.interact(prev, ui.id().with("sc_prev"), Sense::click());
    let rn = ui.interact(next, ui.id().with("sc_next"), Sense::click());
    let rm = ui.interact(mid, ui.id().with("sc_preset"), Sense::click());
    for (rect, glyph, resp) in [(prev, "‹", &rp), (next, "›", &rn)] {
        text(ui.painter(), rect.center(), Align2::CENTER_CENTER, glyph, 18.0, s, if resp.hovered() { INK() } else { INK2() });
    }
    let name = st.current_preset.clone().unwrap_or_else(|| "Init".into());
    let sub = match (st.current_preset.is_some(), crate::palette_v3::is_dirty(st, params)) {
        (false, _) => "Default",
        (true, false) => "User",
        (true, true) => "User · edited",
    };
    text(ui.painter(), mid.center() - Vec2::new(0.0, 7.0 * s), Align2::CENTER_CENTER, &name, 14.0, s, INK());
    text(ui.painter(), mid.center() + Vec2::new(0.0, 9.0 * s), Align2::CENTER_CENTER, sub, 11.0, s, INK2());
    if epo::tap_or_click(ui, &rp) {
        crate::palette_v3::preset_step(st, params, setter, -1);
    }
    if epo::tap_or_click(ui, &rn) {
        crate::palette_v3::preset_step(st, params, setter, 1);
    }
    // The name opens the browser (search, favourites, save-as).
    if epo::tap_or_click(ui, &rm) {
        st.preset_open = !st.preset_open;
        if st.preset_open {
            st.preset_search.clear();
            st.palette_sel = 0;
            st.palette_focus = true;
            st.preset_guard = 2;
        }
    }

    // A / B
    let ab_x = l.w - l.pad - 38.0 - 10.0 - 80.0;
    if let Some(i) = seg(ui, r(o, s, ab_x, y, 80.0, h), &["A", "B"], st.ab_active as usize, "sc_ab", s, 13.0, true) {
        crate::gui_v3::ab_switch(st, params, setter, i as u8);
    }

    // Freeze: a performance switch, so it sits up here in reach whatever the
    // lane shows. Ink when it holds.
    let frozen = params.freeze.value();
    let fresp = button(ui, r(o, s, ab_x - 16.0 - 96.0, y, 96.0, h), "Freeze", frozen, "sc_freeze", s, 13.0);
    if epo::tap_or_click(ui, &fresp) {
        setter.begin_set_parameter(&params.freeze);
        setter.set_parameter(&params.freeze, !frozen);
        setter.end_set_parameter(&params.freeze);
    }

    // light / dark
    let tb = r(o, s, l.w - l.pad - 38.0, y, 38.0, h);
    let tresp = ui.interact(tb, ui.id().with("sc_theme"), Sense::click());
    outline(ui.painter(), tb, false, s);
    let col = if tresp.hovered() { INK() } else { INK2() };
    let c = tb.center();
    ui.painter().circle_stroke(c, 4.0 * s, Stroke::new(1.4 * s, col));
    for i in 0..8 {
        let (sa, ca) = (i as f32 * std::f32::consts::TAU / 8.0).sin_cos();
        ui.painter().line_segment([c + Vec2::new(sa, ca) * 6.4 * s, c + Vec2::new(sa, ca) * 8.6 * s], Stroke::new(1.4 * s, col));
    }
    if epo::tap_or_click(ui, &tresp) {
        st.scope.look.dark = !st.scope.look.dark;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Picture: the tap field (X = time) or the bars (X = tap order); Y is picked
// ─────────────────────────────────────────────────────────────────────────────

/// What the picture's vertical axis shows: one per-tap parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    Pan,
    Gain,
    Cutoff,
}

impl Axis {
    const ALL: [Axis; 3] = [Axis::Pan, Axis::Gain, Axis::Cutoff];

    fn name(self) -> &'static str {
        match self {
            Axis::Pan => "Pan",
            Axis::Gain => "Gain",
            Axis::Cutoff => "Cutoff",
        }
    }

    fn param(self, tap: &TapParams) -> &FloatParam {
        match self {
            Axis::Pan => &tap.pan,
            Axis::Gain => &tap.gain_db,
            Axis::Cutoff => &tap.filter_cutoff,
        }
    }

    /// Param id of tap `i` on this axis (nested ids carry the 1-based tap number).
    fn id(self, i: usize) -> String {
        let base = match self {
            Axis::Pan => "pan",
            Axis::Gain => "gain_db",
            Axis::Cutoff => "tap_filter_cutoff",
        };
        format!("{base}_{}", i + 1)
    }

    /// A value → 0 (top) … 1 (bottom). Pan has L / Mid at the top and gain the
    /// DAW-fader skew (0 dB at 75 % height), both as in v3; cutoff is log.
    fn norm(self, v: f32) -> f32 {
        let n = match self {
            Axis::Pan => (v + 1.0) * 0.5,
            Axis::Gain => 1.0 - gain_db_to_pos(v),
            Axis::Cutoff => 1.0 - (v / 20.0).max(1.0).ln() / 1000f32.ln(),
        };
        n.clamp(0.0, 1.0)
    }

    fn value(self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        match self {
            Axis::Pan => n * 2.0 - 1.0,
            Axis::Gain => pos_to_gain_db(1.0 - n),
            Axis::Cutoff => 20.0 * 1000f32.powf(1.0 - n),
        }
    }

    fn text(self, v: f32, ch: ChannelMode) -> String {
        match self {
            Axis::Pan => pan_text(v, ch),
            Axis::Gain => db_text(v),
            Axis::Cutoff => hz_text(v),
        }
    }

    /// The short form above a bar: no unit for gain, the filter type for cutoff.
    fn short(self, tap: &TapParams, ch: ChannelMode) -> String {
        let v = self.param(tap).value();
        match self {
            Axis::Pan => pan_text(v, ch),
            Axis::Gain => db_text(v).trim_end_matches(" dB").to_string(),
            Axis::Cutoff => {
                let ty = match tap.filter_type.value() {
                    FilterType::LowPass => "LP",
                    FilterType::BandPass => "BP",
                    FilterType::HighPass => "HP",
                };
                if v >= 1000.0 { format!("{ty} {:.1}k", v / 1000.0) } else { format!("{ty} {v:.0}") }
            }
        }
    }

    /// Gridlines as (norm, label).
    fn grid(self, ch: ChannelMode) -> Vec<(f32, String)> {
        let at = |pts: &[(f32, &str)]| -> Vec<(f32, String)> {
            pts.iter().map(|&(v, t)| (self.norm(v), t.to_string())).collect()
        };
        match self {
            Axis::Pan if ch == ChannelMode::MidSide => at(&[(-1.0, "Mid"), (0.0, "M·S"), (1.0, "Side")]),
            Axis::Pan => at(&[(-1.0, "L"), (0.0, "C"), (1.0, "R")]),
            Axis::Gain => at(&[(6.0, "+6"), (0.0, "0 dB"), (-12.0, "−12"), (-24.0, "−24"), (-48.0, "−48")]),
            Axis::Cutoff => at(&[(20000.0, "20k"), (5000.0, "5k"), (1000.0, "1k"), (200.0, "200"), (50.0, "50")]),
        }
    }
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

/// Taps that are live; the rest are hidden, as in v3.
fn n_active(params: &PetalParams) -> usize {
    (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS)
}

/// Tap `i` of `n`: the accent, lighter towards the last tap (v3's ramp).
fn tap_col(i: usize, n: usize) -> Color32 {
    let t = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
    let c = G();
    let f = |v: u8| (v as f32 + (255.0 - v as f32) * 0.5 * t).round() as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
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
/// 1/NUM_TAPS, i.e. its time to a multiple of the base time. Same as v3.
fn snap_offset(cur_time_secs: f32, cur_off: f32, raw_off: f32, span_secs: f32) -> f32 {
    let pos = cur_time_secs / span_secs.max(1e-6) - cur_off;
    let snapped = ((pos + raw_off) * NUM_TAPS as f32).round() / NUM_TAPS as f32;
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
fn axis_grid(p: &egui::Painter, plot: Rect, axis: Axis, ch: ChannelMode, s: f32) {
    for (n, t) in axis.grid(ch) {
        let y = plot.top() + n * plot.height();
        p.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0 * s, theme::line()));
        text(p, Pos2::new(plot.left() - 12.0 * s, y), Align2::RIGHT_CENTER, &t, 10.5, s, SOFT());
    }
}

#[allow(clippy::too_many_arguments)]
fn picture(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter,
    sc: &mut ScopeState, tempo: f32,
) {
    let (x, y, w, h) = l.picture;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    p.rect_filled(rect, g::R_CARD * s, theme::FRAME());
    text(&p, rect.min + Vec2::new(16.0, 24.0) * s, Align2::LEFT_CENTER, "Taps", 12.0, s, INK());
    if !l.lane_stacked {
        let hint = match (sc.view, sc.lock_time) {
            (0, false) => "drag a dot",
            (0, true) => "drag a dot up or down",
            _ => "drag across the bars",
        };
        text(&p, rect.min + Vec2::new(54.0, 24.0) * s, Align2::LEFT_CENTER, hint, 12.0, s, SOFT());
    }

    // header, right to left: Snap · Lock time (field only) · Field / Bars · Y picker
    let mut right = x + w - 12.0;
    if sc.view == 0 {
        let snap = params.tap_snap.value();
        let resp = button(ui, r(o, s, right - 56.0, y + 10.0, 56.0, 28.0), "Snap", snap, "sc_snap", s, 12.0);
        if epo::tap_or_click(ui, &resp) {
            setter.begin_set_parameter(&params.tap_snap);
            setter.set_parameter(&params.tap_snap, !snap);
            setter.end_set_parameter(&params.tap_snap);
        }
        right -= 64.0;
        // Lock time: a tap that sits right keeps its time while its value moves.
        let resp = button(ui, r(o, s, right - 84.0, y + 10.0, 84.0, 28.0), "Lock time", sc.lock_time, "sc_lock", s, 12.0);
        if epo::tap_or_click(ui, &resp) {
            sc.lock_time = !sc.lock_time;
        }
        right -= 100.0;
        let xr = o.x + (right + 8.0) * s;
        rule(ui.painter(), Pos2::new(xr, o.y + (y + 14.0) * s), Pos2::new(xr, o.y + (y + 34.0) * s), s);
    }
    if let Some(i) = seg(ui, r(o, s, right - 112.0, y + 10.0, 112.0, 28.0), &["Field", "Bars"], sc.view, "sc_view", s, 12.0, true) {
        sc.view = i;
    }
    right -= 128.0;
    let xr = o.x + (right + 8.0) * s;
    rule(ui.painter(), Pos2::new(xr, o.y + (y + 14.0) * s), Pos2::new(xr, o.y + (y + 34.0) * s), s);
    // Mono has no pan, so its picture starts on gain.
    let mono = params.channel_mode.value() == ChannelMode::Mono;
    let axis_i = if mono && sc.y_axis == 0 { 1 } else { sc.y_axis };
    if let Some(i) = seg_ex(ui, r(o, s, right - 180.0, y + 10.0, 180.0, 28.0), &["Pan", "Gain", "Cutoff"], axis_i, "sc_y", s, 12.0, &[!mono, true, true]) {
        sc.y_axis = i;
    }
    text(&p, r(o, s, right - 198.0, y + 10.0, 12.0, 28.0).center(), Align2::CENTER_CENTER, "Y", 12.0, s, SOFT());

    let axis = Axis::ALL[axis_i];
    let plot = Rect::from_min_max(rect.min + Vec2::new(56.0, 56.0) * s, rect.max - Vec2::new(24.0, 34.0) * s);
    if sc.view == 0 {
        let viz = crate::gui::compute_viz_data_ex(params, tempo, 1);
        field(ui, plot, s, params, setter, sc, &viz, axis, tempo);
    } else {
        bars(ui, plot, s, params, setter, sc, axis);
    }
}

/// The tap field: X = time on v3's tight taps-only range, Y = the picked axis.
/// Drag a dot: ↔ moves its time offset (unless Lock time), ↕ its value.
/// Click it: select it and open the Tap tab. Double-click resets what can move.
#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui, plot: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter,
    sc: &mut ScopeState, viz: &VizData, axis: Axis, tempo: f32,
) {
    let p = ui.painter().clone();
    let ch = params.channel_mode.value();
    let is_sync = params.sync_mode.value() == SyncMode::Sync;
    // The view fits the pattern; while a dot is dragged it holds still, so the
    // dot stays under the pointer even as the pattern grows.
    let fit = viz.grid_range_secs_taps_only.max(1e-4);
    let held_id = ui.id().with("sc_field_range");
    let range = ui.ctx().data(|m| m.get_temp::<f32>(held_id)).unwrap_or(fit);
    let x_at = |secs: f32| plot.left() + (secs / range).min(1.0) * plot.width();
    let span = viz.span_l_secs.max(1e-4);
    time_grid(&p, plot, range, tempo, is_sync, s, true);
    axis_grid(&p, plot, axis, ch, s);

    // A click on the background deselects; registered first so dots win overlaps.
    if ui.interact(plot, ui.id().with("sc_field_bg"), Sense::click()).clicked() {
        sc.deselect();
    }
    let n = n_active(params);
    // Time L/R unlinked: the right channel's taps land at their own times.
    // They show as diamonds joined to their dot, under the dots, and don't
    // drag (L and R share each tap's offset).
    if !params.time_linked.value() {
        for i in 0..n {
            let tap = &params.taps[i];
            let y = plot.top() + axis.norm(axis.param(tap).value()) * plot.height();
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
    let snap = params.tap_snap.value();
    let lock = sc.lock_time;
    let mut selected = None;
    let mut pressing = false;
    for i in 0..n {
        let tap = &params.taps[i];
        let prm = axis.param(tap);
        let secs = viz.times_l_grid[i] * fit;
        let c = Pos2::new(x_at(secs), plot.top() + axis.norm(prm.value()) * plot.height());
        let hit = if theme::touch() { theme::TOUCH_MIN_HIT } else { 28.0 } * s;
        let id = ui.id().with(("sc_tap", i));
        let d = drag_at(ui, id, Rect::from_center_size(c, Vec2::splat(hit)));
        pressing |= d.pressing;
        if d.started {
            ui.ctx().data_mut(|m| m.insert_temp(held_id, range));
        }
        let virt_id = id.with("virt");
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
        }
        let (mut off, mut val) = (None, None);
        if d.delta != Vec2::ZERO {
            if !lock {
                let mut virt: f32 = ui.ctx().data(|m| m.get_temp(virt_id)).unwrap_or_else(|| tap.time_offset.value());
                // px → seconds on this range → fraction of the cluster span
                virt = (virt + d.delta.x / plot.width() * range / span).clamp(-0.5, 0.5);
                ui.ctx().data_mut(|m| m.insert_temp(virt_id, virt));
                let o = if snap { snap_offset(secs, tap.time_offset.value(), virt, span) } else { virt };
                off = Some(o.clamp(-0.5, 0.5));
            }
            val = Some(axis.value(axis.norm(prm.value()) + d.delta.y / plot.height()));
            sc.focus = Some(axis.id(i));
        }
        gesture_set(ui, id.with("off"), setter, &tap.time_offset, &d, off);
        gesture_set(ui, id.with("val"), setter, prm, &d, val);
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
            setter.begin_set_parameter(prm);
            setter.set_parameter(prm, prm.default_plain_value());
            setter.end_set_parameter(prm);
            if !lock {
                setter.begin_set_parameter(&tap.time_offset);
                setter.set_parameter(&tap.time_offset, 0.0);
                setter.end_set_parameter(&tap.time_offset);
            }
        }

        let col = tap_col(i, n);
        let active = tap.active.value();
        let rad = if axis == Axis::Pan {
            (4.0 + 7.0 * gain_db_to_pos(tap.gain_db.value())) * s // size = level
        } else {
            5.0 * s
        };
        if axis == Axis::Gain {
            let stem = theme::with_alpha(col, if active { 160 } else { 70 });
            p.line_segment([Pos2::new(c.x, c.y + rad), Pos2::new(c.x, plot.bottom())], Stroke::new(1.5 * s, stem));
        }
        if active {
            p.circle_filled(c, rad, col);
        } else {
            p.circle_stroke(c, rad, Stroke::new(1.5 * s, col));
        }
        if sc.sel == Some(i) {
            selected = Some((i, c, rad));
        }
    }

    // the selected tap on top: ring, crosshair, its value and its time
    if let Some((i, c, rad)) = selected {
        let prm = axis.param(&params.taps[i]);
        p.circle_stroke(c, rad + 3.5 * s, Stroke::new(1.5 * s, INK()));
        let hair = Stroke::new(1.0 * s, theme::with_alpha(INK(), 60));
        p.line_segment([Pos2::new(c.x, plot.top()), Pos2::new(c.x, plot.bottom())], hair);
        p.line_segment([Pos2::new(plot.left(), c.y), Pos2::new(plot.right(), c.y)], hair);
        // The value goes above the dot (below near the top edge), not beside it,
        // so it never covers the neighbouring taps in a tight cluster.
        let ytxt = format!("{} {}  ↕", axis.name(), axis.text(prm.value(), ch));
        let (at, align) = if c.y - rad - 34.0 * s > plot.top() {
            (Pos2::new(c.x, c.y - rad - 8.0 * s), Align2::CENTER_BOTTOM)
        } else {
            (Pos2::new(c.x, c.y + rad + 8.0 * s), Align2::CENTER_TOP)
        };
        pill_in(&p, at, align, &ytxt, s, PAPER(), INK(), plot);
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
    ui: &mut egui::Ui, plot: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter,
    sc: &mut ScopeState, axis: Axis,
) {
    let p = ui.painter().clone();
    let ch = params.channel_mode.value();
    axis_grid(&p, plot, axis, ch, s);
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
            let prm = axis.param(&params.taps[i]);
            if begun & (1 << i) == 0 {
                setter.begin_set_parameter(prm);
                begun |= 1 << i;
            }
            setter.set_parameter(prm, axis.value((pos.y - plot.top()) / plot.height()));
            sc.sel = Some(i);
            sc.focus = Some(axis.id(i));
        }
    }
    if d.released {
        for i in 0..NUM_TAPS {
            if begun & (1 << i) != 0 {
                setter.end_set_parameter(axis.param(&params.taps[i]));
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
        let prm = axis.param(tap);
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let yv = plot.top() + axis.norm(prm.value()) * plot.height();
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
        if show_all || sel {
            mono(&p, Pos2::new(cx, plot.top() - 10.0 * s), Align2::CENTER_CENTER, &axis.short(tap, ch), 11.0, s, if sel { INK() } else { INK2() });
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
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter,
    sc: &mut ScopeState, tempo: f32,
) {
    let (x, y, w, h) = l.lane;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    p.rect_filled(rect, g::R_CARD * s, theme::FRAME());
    // Echo · Motion · Tap N. The Tap tab shows the selected tap (clicking a dot
    // opens it); picked with nothing selected, it selects the tap it last showed.
    let n = n_active(params);
    if let Some(i) = sc.sel {
        sc.last_tap = i;
    }
    let shown = sc.sel.unwrap_or(sc.last_tap).min(n - 1);
    let tap_tab = format!("Tap {}", shown + 1);
    if let Some(i) = seg(ui, r(o, s, x + 14.0, y + 7.0, 210.0, 26.0), &["Echo", "Motion", &tap_tab], sc.lane_tab, "sc_lane", s, 13.0, true) {
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
    let (curves, ctl, cols) = if l.lane_stacked {
        (r(o, s, x, y + 40.0, w, 136.0), r(o, s, x + 14.0, y + 176.0, w - 28.0, h - 180.0), 6)
    } else {
        (r(o, s, x, y + 40.0, 470.0, h - 40.0), r(o, s, x + 490.0, y + 44.0, w - 504.0, h - 48.0), 3)
    };
    // On the tall page the Tap tab takes the whole lane: its eight controls
    // need two full rows, and the loop's own come back with Echo.
    let tap_whole = l.lane_stacked && sc.lane_tab >= 2;
    // The loop's controls stand apart from the tab above or beside them.
    let div = if l.lane_stacked {
        [Pos2::new(ctl.left(), curves.bottom()), Pos2::new(ctl.right(), curves.bottom())]
    } else {
        let xm = curves.right() + 6.0 * s;
        [Pos2::new(xm, curves.top() + 10.0 * s), Pos2::new(xm, curves.bottom() - 10.0 * s)]
    };
    if !tap_whole {
        p.line_segment(div, Stroke::new(1.0 * s, theme::line()));
    }

    match sc.lane_tab {
        0 => {
            // The cycle: Auto follows the cluster; Manual keeps its own time, in
            // Time's unit (the feedback time follows Time's Sync since v2.19).
            // Frozen, the loop keeps its length, so the choice waits.
            let locked = params.feedback_time_locked.value();
            let free = !params.freeze.value();
            if let Some(i) = seg(ui, r(o, s, x + 232.0, y + 7.0, 140.0, 26.0), &["Auto", "Manual"], if locked { 0 } else { 1 }, "sc_cycle", s, 12.0, free) {
                setter.begin_set_parameter(&params.feedback_time_locked);
                setter.set_parameter(&params.feedback_time_locked, i == 0);
                setter.end_set_parameter(&params.feedback_time_locked);
            }
            let viz = crate::gui::compute_viz_data_ex(params, tempo, 3);
            text(&p, o + Vec2::new(x + w - 14.0, y + 20.0) * s, Align2::RIGHT_CENTER, &echo_readout(&viz, l.lane_stacked), 12.0, s, INK2());
            echo(ui, curves, s, params, setter, &mut sc.focus, &viz, tempo);
        }
        1 => {
            text(&p, curves.center(), Align2::CENTER_CENTER, "Motion — next pass", 12.0, s, SOFT());
        }
        _ => {
            let mut t = sc.sel.unwrap_or(shown).min(n - 1);
            // ‹ Tap 3 of 8 › steps through the live taps, wrapping.
            let (bw, lw) = (30.0, 92.0);
            let nx = x + w - 14.0 - bw;
            let prev = button(ui, r(o, s, nx - lw - bw, y + 7.0, bw, 26.0), "‹", false, "sc_tap_prev", s, 15.0);
            let next = button(ui, r(o, s, nx, y + 7.0, bw, 26.0), "›", false, "sc_tap_next", s, 15.0);
            text(&p, r(o, s, nx - lw, y + 7.0, lw, 26.0).center(), Align2::CENTER_CENTER, &format!("Tap {} of {n}", t + 1), 12.0, s, INK2());
            let (back, fwd) = (epo::tap_or_click(ui, &prev), epo::tap_or_click(ui, &next));
            let step = fwd as i32 - back as i32;
            if step != 0 {
                t = (t as i32 + step).rem_euclid(n as i32) as usize;
            }
            sc.sel = Some(t);
            let viz = crate::gui::compute_viz_data_ex(params, tempo, 1);
            let area = if tap_whole { curves.union(ctl) } else { curves };
            tap_panel(ui, area, s, params, setter, &mut sc.focus, &viz, t);
        }
    }
    if tap_whole {
        return;
    }

    // the loop's controls; Feedback and the cycle are the lane's own handles.
    // Under the curves they share one row; beside them, two (Cross, Loop).
    let (c_xf, c_mode, c_curve, c_tone) = if l.lane_stacked {
        let c = |col, span| cell_at(ctl, cols, 1, 0, col, span, s);
        (c(0, 1), c(1, 2), c(4, 2), c(3, 1))
    } else {
        let title_w = 52.0 * s;
        let cells = Rect::from_min_max(ctl.min + Vec2::new(title_w, 0.0), ctl.max);
        let c = |row, col, span| cell_at(cells, cols, 2, row, col, span, s);
        for (row, t) in ["Cross", "Loop"].iter().enumerate() {
            let (ctl_row, _, _) = cell_parts(c(row, 0, 1), s);
            group_head(&p, Pos2::new(ctl.left(), ctl_row.center().y), Align2::LEFT_CENTER, t, s);
        }
        let ym = (c(0, 0, 1).bottom() + c(1, 0, 1).top()) * 0.5;
        rule(&p, Pos2::new(ctl.left(), ym), Pos2::new(ctl.right(), ym), s);
        // each row reads knob then selector, so the columns line up
        (c(0, 0, 1), c(0, 1, 2), c(1, 1, 2), c(1, 0, 1))
    };
    // In Ping, X-feed is the bounce's width; Mono has no sides to bounce between.
    let ping = params.crossfeed_mode.value() == CrossfeedMode::PingPong;
    let mono = params.channel_mode.value() == ChannelMode::Mono;
    let xf_label = if ping { "Width" } else { "X-feed" };
    knob_cell(ui, c_xf, s, setter, &params.crossfeed, "crossfeed", xf_label, false, !(ping && mono), None, &mut sc.focus);
    enum_cell_ex(ui, c_mode, s, setter, &params.crossfeed_mode, "crossfeed_mode", "Mode", &["Mix", "Ping", "Diffuse"], &[true, !mono, true], &mut sc.focus);
    enum_cell(ui, c_curve, s, setter, &params.fb_curve, "fb_curve", "Curve", &["Natural", "Bloom", "Gate"], true, &mut sc.focus);
    // Left darkens each repeat, right thins it, the centre is off.
    knob_cell(ui, c_tone, s, setter, &params.loop_tone, "loop_tone", "Tone", true, true, None, &mut sc.focus);
}

/// The Tap tab: one tap's time, place, level and filter at once (v3's tap
/// panel). The cells use the nested param ids, so the focus card follows them.
#[allow(clippy::too_many_arguments)]
fn tap_panel(
    ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter,
    focus: &mut Option<String>, viz: &VizData, i: usize,
) {
    let tap = &params.taps[i];
    let ch = params.channel_mode.value();
    let inner = area.shrink2(Vec2::new(14.0, 4.0) * s);
    let c = |row, col| cell_at(inner, 4, 2, row, col, 1, s);
    let id = |base: &str| format!("{base}_{}", i + 1);
    let time = secs_text(viz.times_l_grid[i] * viz.grid_range_secs_taps_only);
    knob_cell(ui, c(0, 0), s, setter, &tap.time_offset, &id("time_offset"), "Time", true, true, Some(time), focus);
    knob_cell(ui, c(0, 1), s, setter, &tap.pan, &id("pan"), "Pan", true, ch != ChannelMode::Mono, Some(pan_text(tap.pan.value(), ch)), focus);
    knob_cell(ui, c(0, 2), s, setter, &tap.gain_db, &id("gain_db"), "Gain", false, true, Some(db_text(tap.gain_db.value())), focus);
    toggle_cell(ui, c(0, 3), s, setter, &tap.active, &id("active"), "Tap", "On", "Muted", focus);
    enum_cell(ui, c(1, 0), s, setter, &tap.filter_type, &id("tap_filter_type"), "Filter", &["LP", "BP", "HP"], true, focus);
    knob_cell(ui, c(1, 1), s, setter, &tap.filter_cutoff, &id("tap_filter_cutoff"), "Cutoff", false, true, Some(hz_text(tap.filter_cutoff.value())), focus);
    knob_cell(ui, c(1, 2), s, setter, &tap.filter_q, &id("tap_filter_q"), "Reso", false, true, Some(format!("Q {:.1}", tap.filter_q.value())), focus);
    enum_cell(ui, c(1, 3), s, setter, &tap.filter_slope, &id("tap_filter_slope"), "Slope", &["12", "24", "48"], true, focus);
}

/// "Feedback 60 % · −4.4 dB per repeat · 13 to −60 dB", from the same
/// per-cycle levels the lane draws (they follow the FB curve); frozen,
/// "Frozen · holds a 53 ms loop".
fn echo_readout(viz: &VizData, short: bool) -> String {
    if viz.frozen {
        let cycle = secs_text(viz.fb_cycle_norm * viz.grid_range_secs);
        return if short { format!("Frozen · {cycle}") } else { format!("Frozen · holds a {cycle} loop") };
    }
    let fb = format!("Feedback {:.0} %", viz.feedback * 100.0);
    if viz.feedback < 1e-3 {
        return format!("{fb} · no repeats");
    }
    let g1 = viz.cycle_gains[1];
    if g1 >= 0.999 {
        return format!("{fb} · {}", if short { "holds" } else { "repeats hold · self-oscillates" });
    }
    let db = db_text(20.0 * g1.max(1e-6).log10());
    if short {
        return format!("{fb} · {db}");
    }
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
    ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter,
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
    let shown = viz.n_echo_cycles.min(3);
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
        let fb = params.feedback.value();
        let fv = (fd.delta.y != 0.0).then(|| (fb - fd.delta.y / plot.height() * 1.05).clamp(0.0, 1.05));
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
                PetalParams::feedback_time_norm_for_secs(params.sync_mode.value(), cycle, tempo)
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

/// The last control touched, big, with a fine slider. Follows `focus` (a param
/// id); defaults to Mix.
#[allow(clippy::too_many_arguments)]
fn focus_card(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter,
    focus: &mut Option<String>, tempo: f32,
) {
    let rect = r(o, s, l.right_x, l.focus.0, l.right_w, l.focus.1);
    ui.painter().rect_filled(rect, g::R_CARD * s, PANEL());
    let id = focus.clone().unwrap_or_else(|| "dry_wet".into());
    let Some((_, ptr, _)) = params.param_map().into_iter().find(|(pid, _, _)| *pid == id) else { return };
    // SAFETY: ptr comes from these params, which outlive the editor frame.
    let (name, n, mut val) = unsafe {
        let n = ptr.unmodulated_normalized_value();
        (ptr.name().to_string(), n, ptr.normalized_value_to_string(n, true))
    };
    // Read exactly like the control that was touched.
    let mut name = name;
    match id.as_str() {
        "base_time_l" => val = params.format_base_time(n, tempo),
        "base_time_r" => val = PetalParams::format_base_time_mode(params.sync_mode_r_eff(), n, tempo),
        "feedback_time" => val = params.format_feedback_time(tempo),
        "crossfeed" if params.crossfeed_mode.value() == CrossfeedMode::PingPong => name = "Bounce width".into(),
        _ => {}
    }
    if let Some((i, axis)) = (0..NUM_TAPS).find_map(|i| Axis::ALL.iter().find(|a| a.id(i) == id).map(|a| (i, *a))) {
        name = format!("Tap {} · {}", i + 1, axis.name());
        // SAFETY: as above.
        val = axis.text(unsafe { ptr.unmodulated_plain_value() }, params.channel_mode.value());
    }
    // The Tap tab's other controls, named and read as the tab shows them.
    let tap_ctl = ["time_offset", "tap_filter_q", "tap_filter_type", "tap_filter_slope", "active"]
        .into_iter()
        .find_map(|b| Some((b, id.strip_prefix(b)?.strip_prefix('_')?.parse::<usize>().ok()?.checked_sub(1)?)))
        .filter(|&(_, i)| i < NUM_TAPS);
    if let Some((base, i)) = tap_ctl {
        let tap = &params.taps[i];
        let (label, v) = match base {
            "time_offset" => {
                let viz = crate::gui::compute_viz_data_ex(params, tempo, 1);
                ("Time", secs_text(viz.times_l_grid[i] * viz.grid_range_secs_taps_only))
            }
            "tap_filter_q" => ("Reso", format!("Q {:.1}", tap.filter_q.value())),
            "tap_filter_type" => ("Filter", ["Low-pass", "Band-pass", "High-pass"][enum_index(&tap.filter_type, 3)].into()),
            "tap_filter_slope" => ("Slope", ["12 dB/oct", "24 dB/oct", "48 dB/oct"][enum_index(&tap.filter_slope, 3)].into()),
            _ => ("Tap", if tap.active.value() { "On" } else { "Muted" }.into()),
        };
        name = format!("Tap {} · {label}", i + 1);
        val = v;
    }
    let mid = l.focus.1 * 0.5;
    let p = ui.painter();
    text(p, rect.min + Vec2::new(16.0, mid - 11.0) * s, Align2::LEFT_CENTER, &format!("{name} · last touched"), 11.0, s, INK2());
    mono(p, rect.min + Vec2::new(16.0, mid + 11.0) * s, Align2::LEFT_CENTER, &val, 22.0, s, INK());
    let track = Rect::from_min_size(rect.min + Vec2::new(236.0, mid - 2.0) * s, Vec2::new(l.right_w - 256.0, 4.0) * s);
    p.rect_filled(track, 2.0 * s, theme::line2());
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * n, track.height())), 2.0 * s, INK());
    p.circle_filled(Pos2::new(track.left() + track.width() * n, track.center().y), 8.0 * s, INK());
    let hit = track.expand2(Vec2::new(10.0, 16.0) * s);
    let resp = ui.interact(hit, ui.id().with("sc_focus_slider"), Sense::click_and_drag());
    if let Some(px) = slider_drive(ui, &resp, hit) {
        let v = ((px - track.left()) / track.width()).clamp(0.0, 1.0);
        // SAFETY: as above.
        unsafe {
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter.raw_context.raw_set_parameter_normalized(ptr, v);
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
}

/// Time, sync, transition mode, link, tap count, spacing and the Shape pad.
#[allow(clippy::too_many_arguments)]
fn time_card(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter,
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
        let r_txt = PetalParams::format_base_time_mode(params.sync_mode_r_eff(), nr, tempo);
        knob_cell(ui, t(2, 0, 1), s, setter, &params.base_time_r, "base_time_r", "Time R", false, true, Some(r_txt), focus);
        enum_cell(ui, t(2, 1, 1), s, setter, &params.sync_mode_r, "sync_r", "Sync R", &["Free", "Sync"], true, focus);
    }

    // Pattern: the tap count over the list of spacings, the Shape pad beside;
    // both start level with Time's first row.
    let row0 = t(0, 0, 1);
    let taps = Rect::from_min_size(Pos2::new(c(0, 3, 1).left(), row0.top()), row0.size());
    knob_cell(ui, taps, s, setter, &params.num_taps_active, "num_taps_active", "Taps", false, true, None, focus);
    let sp_top = taps.bottom() + 10.0 * s;
    text(&p, Pos2::new(taps.center().x, sp_top + g::LBL_H * 0.5 * s), Align2::CENTER_CENTER, "Spacing", 11.0, s, INK2());
    let list = Rect::from_min_max(Pos2::new(taps.left() + 8.0 * s, sp_top + (g::LBL_H + 4.0) * s), Pos2::new(taps.right(), inner.bottom()));
    let cur = enum_index(&params.spacing_mode, 4);
    if let Some(i) = seg_v(ui, list, &["Lin", "Exp", "Log", "Euclid"], cur, "spacing_mode", s, 12.0) {
        set_norm(setter, &params.spacing_mode, i as f32 / 3.0);
        *focus = Some("spacing_mode".into());
    }
    let pad = Rect::from_min_max(Pos2::new(c(0, 4, 1).left(), row0.top()), Pos2::new(c(0, 5, 1).right(), inner.bottom()));
    shape_pad(ui, pad, s, params, setter, focus);
}

/// The Shape pad: X bends where the taps bunch up, Y how hard (shape_x −1…1,
/// shape_y 0…1 with 1 at the top, as in v3). Ticks show the real tap positions.
fn shape_pad(ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter, focus: &mut Option<String>) {
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
    let n = params.num_taps_active.value().max(1) as usize;
    let pos = topology::compute_positions(params.spacing_mode.value(), sx, sy, n);
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
fn colour_card(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, sc: &mut ScopeState) {
    let rect = r(o, s, l.right_x, l.colour.0, l.right_w, l.colour.1);
    let inner = card(ui, rect, "Colour", s);
    let tabs = Rect::from_min_size(Pos2::new(rect.right() - 254.0 * s, rect.top() + 7.0 * s), Vec2::new(240.0, 26.0) * s);
    if let Some(i) = seg(ui, tabs, &["Diffuse", "Tape", "Saturate"], sc.colour_tab, "sc_colour", s, 12.0, true) {
        sc.colour_tab = i;
    }
    // each stage's on/off dot sits just after its name
    let on = [params.diffuse_amount.value() > 0.0, params.degrade_amount.value() > 0.0, params.sat_amount.value() > 0.0];
    let seg_w = tabs.width() / 3.0;
    for (i, (&is_on, name)) in on.iter().zip(["Diffuse", "Tape", "Saturate"]).enumerate() {
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
            knob_cell(ui, c(0, 1), s, setter, &params.diffuse_amount, "diffuse_amount", "Amount", false, true, None, f);
            knob_cell(ui, c(1, 1), s, setter, &params.diffuse_size, "diffuse_size", "Size", false, true, None, f);
            enum_cell(ui, c(4, 2), s, setter, &params.diffuse_routing, "diffuse_routing", "Route", &routes, true, f);
        }
        1 => {
            knob_cell(ui, c(0, 1), s, setter, &params.degrade_amount, "degrade_amount", "Amount", false, true, None, f);
            knob_cell(ui, c(1, 1), s, setter, &params.degrade_drive, "degrade_drive", "Drive", false, true, None, f);
            knob_cell(ui, c(2, 1), s, setter, &params.degrade_wear, "degrade_wear", "Wear", false, true, None, f);
            enum_cell(ui, c(3, 1), s, setter, &params.degrade_type, "degrade_type", "Type", &["I", "II", "IV"], true, f);
            enum_cell(ui, c(4, 2), s, setter, &params.degrade_routing, "degrade_routing", "Route", &routes, true, f);
        }
        _ => {
            knob_cell(ui, c(0, 1), s, setter, &params.sat_amount, "sat_amount", "Amount", false, true, None, f);
            knob_cell(ui, c(1, 1), s, setter, &params.sat_drive, "sat_drive", "Drive", false, true, None, f);
            enum_cell(ui, c(4, 2), s, setter, &params.sat_routing, "sat_routing", "Route", &routes, true, f);
        }
    }
}

fn output_card(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, focus: &mut Option<String>) {
    let rect = r(o, s, l.right_x, l.output.0, l.right_w, l.output.1);
    let inner = card(ui, rect, "Output", s);
    // Levels on the left; ducking after a rule, in the columns Colour's
    // Route uses above it.
    let c = |col| cell_at(inner, 6, 1, 0, col, 1, s);
    knob_cell(ui, c(0), s, setter, &params.input_gain, "input_gain", "In", true, true, None, focus);
    knob_cell(ui, c(1), s, setter, &params.dry_wet, "dry_wet", "Mix", false, true, None, focus);
    knob_cell(ui, c(2), s, setter, &params.output_gain, "output_gain", "Out", true, true, None, focus);
    let xr = c(4).left() - 4.0 * s;
    rule(ui.painter(), Pos2::new(xr, inner.top() + 8.0 * s), Pos2::new(xr, inner.bottom() - 4.0 * s), s);
    knob_cell(ui, c(4), s, setter, &params.duck, "duck", "Duck", false, true, None, focus);
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
// Settings menu (Tide's: label column, sections split by hairlines, 16 px
// radius, fixed 8 px row gaps)
// ─────────────────────────────────────────────────────────────────────────────

fn settings_menu(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, sc: &mut ScopeState) {
    if !sc.show_menu {
        return;
    }
    let anchor = o + Vec2::new(l.pad, g::HEAD_Y + g::HEAD_H + 8.0) * s;
    let fsc = theme::font_scale();
    let area = egui::Area::new(Id::new("afterecho_settings"))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(PANEL())
                .stroke(Stroke::new(tk::stroke_outline() * s, theme::line2()))
                .corner_radius(16.0 * s)
                .inner_margin(egui::Margin::symmetric((16.0 * s) as i8, (14.0 * s) as i8))
                .show(ui, |ui| {
                    ui.set_width(300.0 * s);
                    ui.spacing_mut().item_spacing = Vec2::new(6.0 * s, 8.0 * s);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Settings").size(tk::font_label() * s * fsc).color(INK()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rr, resp) = ui.allocate_exact_size(Vec2::splat(18.0 * s), Sense::click());
                            let col = if resp.hovered() { INK() } else { SOFT() };
                            paint_cross(ui.painter(), rr.center(), 4.0 * s, col, s);
                            if epo::tap_or_click(ui, &resp) {
                                sc.show_menu = false;
                            }
                        });
                    });

                    menu_section(ui, "Appearance", s);
                    menu_rows(ui, s, |ui| {
                        menu_row(ui, "Theme", s, |ui| {
                            ui.horizontal(|ui| {
                                for (i, name) in [(4u8, "Mono"), (0, "Cool"), (1, "Warm"), (2, "Ink"), (3, "OLED")] {
                                    if epo::chip(ui, name, sc.look.dark_variant == i) {
                                        sc.look.dark_variant = i;
                                    }
                                }
                            });
                        });
                        menu_row(ui, "Accent", s, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0 * s;
                                for i in std::iter::once(theme::ACCENTS - 1).chain(0..theme::ACCENTS - 1) {
                                    let sz = 18.0 * s;
                                    let (rr, resp) = ui.allocate_exact_size(Vec2::splat(sz), Sense::click());
                                    let p = ui.painter_at(rr.expand(2.0 * s));
                                    p.circle_filled(rr.center(), sz * 0.5, theme::accent_swatch(i as usize));
                                    if sc.look.accent == i {
                                        p.circle_stroke(rr.center(), sz * 0.5 + 1.5 * s, Stroke::new(1.5 * s, INK()));
                                    }
                                    if epo::tap_or_click(ui, &resp.on_hover_text(theme::ACCENT_NAMES[i as usize])) {
                                        sc.look.accent = i;
                                    }
                                }
                            });
                        });
                        menu_row(ui, "Font (trial)", s, |ui| {
                            ui.horizontal(|ui| {
                                if epo::chip(ui, "Karla", sc.look.font_face == 0) {
                                    sc.look.font_face = 0;
                                }
                                if epo::chip(ui, "Plex", sc.look.font_face == 1) {
                                    sc.look.font_face = 1;
                                }
                            });
                        });
                        menu_row(ui, "Text size", s, |ui| {
                            match menu_stepper(ui, s, &format!("{:+}", sc.look.font_steps), sc.look.font_steps == FONT_STEPS_DEFAULT) {
                                Step::Dec => sc.look.font_steps = (sc.look.font_steps - 1).max(-5),
                                Step::Inc => sc.look.font_steps = (sc.look.font_steps + 1).min(10),
                                Step::Reset => sc.look.font_steps = FONT_STEPS_DEFAULT,
                                Step::None => {}
                            }
                        });
                    });

                    // The preset folder lives inside the app sandbox on iPad, so
                    // there is no folder to reveal there.
                    if !cfg!(target_os = "ios") {
                        menu_section(ui, "Presets", s);
                        if epo::chip(ui, "Open preset folder", false) {
                            reveal_presets();
                            sc.show_menu = false;
                        }
                    }

                    ui.add_space(2.0 * s);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(concat!("Afterecho ", env!("CARGO_PKG_VERSION")))
                                .size(tk::font_body() * 0.9 * s * fsc)
                                .color(theme::FAINT()),
                        );
                    });
                });
        });
    // A click outside the panel closes it — except on the hamburger, which owns
    // open/close itself (a position test; egui can re-run a frame's passes).
    let on_hamburger = ui
        .ctx()
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|pos| sc.hamb_rect.is_some_and(|hr| hr.contains(pos)));
    if !on_hamburger && area.response.clicked_elsewhere() {
        sc.show_menu = false;
    }
}

fn reveal_presets() {
    let dir = crate::presets::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&dir).spawn();
    }
}

/// Section header: small soft caps with a hairline above it.
fn menu_section(ui: &mut egui::Ui, t: &str, s: f32) {
    ui.add_space(4.0 * s);
    let (rr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0 * s), Sense::hover());
    ui.painter().line_segment([rr.left_center(), rr.right_center()], Stroke::new(tk::stroke_outline() * s, theme::line()));
    ui.add_space(2.0 * s);
    ui.label(
        RichText::new(t.to_uppercase())
            .size(tk::font_body() * 0.85 * s * theme::font_scale())
            .extra_letter_spacing(0.8 * s)
            .color(SOFT()),
    );
}

/// One menu section's rows, stacked with a uniform gap.
fn menu_rows(ui: &mut egui::Ui, s: f32, body: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0 * s;
        body(ui);
    });
}

/// A `label · control` row; the label sits in a fixed-width slot so every row's
/// control starts on the same x. (Not an `egui::Grid`: a Grid requests a
/// discard pass on first layout, which replays the hamburger click.)
fn menu_row(ui: &mut egui::Ui, t: &str, s: f32, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (rr, _) = ui.allocate_exact_size(Vec2::new(100.0 * s, tk::ctl_h() * s), Sense::hover());
        ui.painter().text(rr.left_center(), Align2::LEFT_CENTER, t, FontId::proportional(tk::font_body() * s * theme::font_scale()), INK2());
        control(ui);
    });
}

enum Step {
    None,
    Dec,
    Inc,
    Reset,
}

/// `[−] value [+]  Reset` — the value sits in a fixed-width slot so the row never
/// reflows as it changes; Reset fades out when already at the default.
fn menu_stepper(ui: &mut egui::Ui, s: f32, value: &str, at_default: bool) -> Step {
    let mut out = Step::None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0 * s;
        if epo::chip(ui, "−", false) {
            out = Step::Dec;
        }
        let (rr, _) = ui.allocate_exact_size(Vec2::new(38.0 * s, tk::ctl_h() * s), Sense::hover());
        ui.painter().text(rr.center(), Align2::CENTER_CENTER, value, FontId::proportional(tk::font_body() * s * theme::font_scale()), INK());
        if epo::chip(ui, "+", false) {
            out = Step::Inc;
        }
        ui.add_space(4.0 * s);
        ui.add_enabled_ui(!at_default, |ui| {
            if at_default {
                ui.multiply_opacity(0.35);
            }
            if epo::chip_sm(ui, "Reset", false) {
                out = Step::Reset;
            }
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_shape_with_the_bigger_controls() {
        // iPad full screen, landscape and portrait: tall.
        assert_eq!(Shape::pick(Vec2::new(1112.0, 834.0)), Shape::Tall);
        assert_eq!(Shape::pick(Vec2::new(834.0, 1112.0)), Shape::Tall);
        // Desktop-shaped and short host panels: wide.
        assert_eq!(Shape::pick(Vec2::new(1366.0, 720.0)), Shape::Wide);
        assert_eq!(Shape::pick(Vec2::new(1112.0, 500.0)), Shape::Wide);
    }

    #[test]
    fn layouts_fit_their_page_without_overlapping() {
        for l in [&WIDE, &TALL] {
            let rect = |(x, y, w, h): (f32, f32, f32, f32)| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
            let right = |(y, h): (f32, f32)| rect((l.right_x, y, l.right_w, h));
            let regions = [
                rect(l.picture),
                rect(l.lane),
                right(l.focus),
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
            // under the curves), and the Tap tab two (on the tall page it
            // takes the whole lane)
            let fits = |room: f32, rows: f32| room >= rows * g::CELL_H;
            assert!(fits(l.time.1 - 38.0, 3.0), "Time & taps, {}×{}", l.w, l.h);
            assert!(fits(l.colour.1 - 41.0, 1.0) && fits(l.output.1 - 38.0, 1.0));
            let (lane_h, ctl_rows, ctl_h, tap_h) = if l.lane_stacked {
                (l.lane.3, 1.0, l.lane.3 - 180.0, l.lane.3 - 48.0)
            } else {
                (l.lane.3, 2.0, l.lane.3 - 48.0, l.lane.3 - 48.0)
            };
            assert!(fits(ctl_h, ctl_rows) && fits(tap_h, 2.0), "lane {lane_h}, {}×{}", l.w, l.h);
            // header: tabs end before the preset box, which ends before A/B
            let tabs_end = l.pad + 176.0 + l.tabs_w;
            let preset = ((l.w - l.preset_w) * 0.5, (l.w + l.preset_w) * 0.5);
            assert!(tabs_end < preset.0 && preset.1 < l.w - l.pad - 38.0 - 10.0 - 80.0);
        }
    }
}
