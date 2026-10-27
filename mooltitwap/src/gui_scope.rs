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

use crate::gui::EditorState;
use crate::params::PetalParams;
use crate::topology;
use crate::ui::epo;
use crate::ui::theme::{self, G, INK, INK2, INSET, PANEL, PAPER, SOFT};
use crate::ui::tokens as tk;

/// Element sizes (logical px), the same on every page shape — Tide's, so the
/// plug-ins match.
mod g {
    pub const HEAD_Y: f32 = 16.0;
    pub const HEAD_H: f32 = 38.0;
    pub const R_CARD: f32 = 16.0;
    pub const R_CTL: f32 = 9.0;
    /// A control cell: its name, the control row, then its value.
    pub const CTL_H: f32 = 30.0;
    pub const LBL_H: f32 = 13.0;
    pub const CELL_H: f32 = CTL_H + 2.0 + 2.0 * LBL_H;
    pub const DIAL: f32 = 28.0;
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

/// Desktop, and iPad in short host panels.
const WIDE: Layout = Layout {
    w: 1366.0,
    h: 720.0,
    pad: 20.0,
    picture: (20.0, 70.0, 800.0, 344.0),
    lane: (20.0, 426.0, 800.0, 276.0),
    lane_stacked: false,
    right_x: 834.0,
    right_w: 512.0,
    focus: (70.0, 64.0),
    time: (146.0, 250.0),
    colour: (408.0, 140.0),
    output: (560.0, 142.0),
    tabs_w: 220.0,
    preset_w: 270.0,
};

/// iPad full screen: narrower, so it gets a bigger zoom.
const TALL: Layout = Layout {
    w: 1050.0,
    h: 780.0,
    pad: 18.0,
    picture: (18.0, 70.0, 560.0, 360.0),
    lane: (18.0, 442.0, 560.0, 320.0),
    lane_stacked: true,
    right_x: 592.0,
    right_w: 440.0,
    focus: (70.0, 72.0),
    time: (154.0, 252.0),
    colour: (418.0, 160.0),
    output: (590.0, 172.0),
    tabs_w: 200.0,
    preset_w: 220.0,
};

/// Default text-size steps (Settings → Text size); one step is 6 %.
const FONT_STEPS_DEFAULT: i32 = 4;
const FONT_SCALE_DEFAULT: f32 = 1.0 + 0.06 * FONT_STEPS_DEFAULT as f32;
/// Afterecho's look: the cool dark foundation and the violet accent (Tide is
/// warm + slate), indices into the theme's variants and accent hues.
const DARK_VARIANT_DEFAULT: u8 = 0;
const ACCENT_DEFAULT: u8 = 5;

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
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: true,
            dark_variant: DARK_VARIANT_DEFAULT,
            accent: ACCENT_DEFAULT,
            font_face: 0,
            font_steps: FONT_STEPS_DEFAULT,
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
        let a: Self = std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            dark_variant: a.dark_variant.min(3),
            accent: a.accent.min(theme::ACCENT_MONO),
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
    /// Picture: Y axis 0 Pan · 1 Gain · 2 Cutoff; view 0 Field · 1 Bars.
    y_axis: usize,
    view: usize,
    /// Lane tab: 0 Echo · 1 Motion.
    lane_tab: usize,
    /// Colour card tab: 0 Diffuse · 1 Tape · 2 Saturate.
    colour_tab: usize,
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
    {
        let sc = &mut st.scope;
        if !sc.inited {
            sc.inited = true;
            sc.look = Appearance::load();
            sc.look_saved = Some(sc.look);
            // Desktop preview of the iPad page: AFTERECHO_SHAPE=tall.
            if !ios && std::env::var("AFTERECHO_SHAPE").is_ok_and(|v| v == "tall") {
                sc.shape = Shape::Tall;
            }
        }
        if sc.look_saved != Some(sc.look) {
            sc.look.save();
            sc.look_saved = Some(sc.look);
        }
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
    picture(ui, o, s, l, &mut st.scope);
    lane(ui, o, s, l, params, setter, &mut st.scope);
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
#[allow(clippy::too_many_arguments)]
fn seg(
    ui: &mut egui::Ui, rect: Rect, labels: &[&str], active: usize, salt: &str, s: f32, px: f32,
    bg: Color32, enabled: bool,
) -> Option<usize> {
    let p = ui.painter().clone();
    let alpha = if enabled { 255 } else { 90 };
    p.rect_filled(rect, g::R_CTL * s, bg);
    let inset = 2.5 * s;
    let n = labels.len().max(1) as f32;
    let w = (rect.width() - 2.0 * inset - (n - 1.0) * 2.0 * s) / n;
    let mut picked = None;
    for (i, l) in labels.iter().enumerate() {
        let b = Rect::from_min_size(
            Pos2::new(rect.left() + inset + i as f32 * (w + 2.0 * s), rect.top() + inset),
            Vec2::new(w, rect.height() - 2.0 * inset),
        );
        let resp = ui.interact(b, ui.id().with((salt, i)), Sense::click());
        let on = i == active;
        if on {
            p.rect_filled(b, (g::R_CTL - 2.0) * s, theme::with_alpha(theme::on_fill(), alpha));
        }
        let col = if on || (enabled && resp.hovered()) { INK() } else { INK2() };
        text(&p, b.center(), Align2::CENTER_CENTER, l, px, s, theme::with_alpha(col, alpha));
        if enabled && epo::tap_or_click(ui, &resp) && !on {
            picked = Some(i);
        }
    }
    picked
}

/// A flat button; `on` = pressed. Returns its response (click via tap_or_click).
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, on: bool, salt: &str, s: f32, px: f32) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("btn", salt)), Sense::click());
    let fill = if on { theme::on_fill() } else { INSET() };
    ui.painter().rect_filled(rect, g::R_CTL * s, fill);
    let col = if on || resp.hovered() { INK() } else { INK2() };
    text(ui.painter(), rect.center(), Align2::CENTER_CENTER, label, px, s, col);
    resp
}

/// Split a cell into (control row, name centre, value centre). The name sits
/// above the control and the value below it, so the name stays readable while
/// a finger is on the control.
fn cell_parts(cell: Rect, s: f32) -> (Rect, Pos2, Pos2) {
    let lab = Pos2::new(cell.center().x, cell.top() + g::LBL_H * 0.5 * s);
    let ctl = Rect::from_min_size(
        Pos2::new(cell.left(), cell.top() + (g::LBL_H + 1.0) * s),
        Vec2::new(cell.width(), g::CTL_H * s),
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

fn paint_dial(p: &egui::Painter, c: Pos2, rad: f32, n: f32, bip: bool, s: f32, alpha: u8) {
    let arc = |v0: f32, v1: f32| -> Vec<Pos2> {
        let (a0, a1) = ((-135.0 + 270.0 * v0).to_radians(), (-135.0 + 270.0 * v1).to_radians());
        (0..=28)
            .map(|i| {
                let t = a0 + (a1 - a0) * i as f32 / 28.0;
                Pos2::new(c.x + rad * t.sin(), c.y - rad * t.cos())
            })
            .collect()
    };
    let sw = 3.0 * s;
    p.add(egui::Shape::line(arc(0.0, 1.0), Stroke::new(sw, theme::with_alpha(theme::line2(), alpha))));
    let (v0, v1) = if bip { (n.min(0.5), n.max(0.5)) } else { (0.0, n.max(0.004)) };
    p.add(egui::Shape::line(arc(v0, v1), Stroke::new(sw, theme::with_alpha(G(), alpha))));
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
    let resp = ui.interact(dial, ui.id().with(("knob", id)), Sense::click_and_drag());
    if enabled {
        let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if epo::knob_drive(ui, setter, param, dial, &resp) {
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
    let (ctl, lab, _) = cell_parts(cell, s);
    let b = Rect::from_center_size(ctl.center(), Vec2::new(cell.width(), g::CTL_H * s));
    let n = names.len();
    let cur = enum_index(param, n);
    if let Some(i) = seg(ui, b, names, cur, id, s, 12.0, INSET(), enabled) {
        set_norm(setter, param, i as f32 / (n - 1) as f32);
        *focus = Some(id.to_string());
    }
    let alpha = if enabled { 255 } else { 90 };
    text(ui.painter(), lab, Align2::CENTER_CENTER, label, 11.0, s, theme::with_alpha(INK2(), alpha));
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
    ui.painter().rect_filled(hb, 11.0 * s, theme::FRAME());
    let open = st.scope.show_menu;
    for k in [-1.0f32, 0.0, 1.0] {
        let yy = hb.center().y + k * 5.0 * s;
        ui.painter().line_segment(
            [Pos2::new(hb.center().x - 8.0 * s, yy), Pos2::new(hb.center().x + 8.0 * s, yy)],
            Stroke::new(1.6 * s, if hresp.hovered() || open { INK() } else { INK2() }),
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
    if let Some(i) = seg(ui, r(o, s, l.pad + 176.0, y, l.tabs_w, h), &["Stereo", "M/S", "Mono"], cm, "sc_chan", s, 13.0, theme::FRAME(), true) {
        set_norm(setter, &params.channel_mode, i as f32 / 2.0);
    }

    // preset box, centred on the page: ‹ name ›
    let pb = r(o, s, (l.w - l.preset_w) * 0.5, y, l.preset_w, h);
    ui.painter().rect_filled(pb, 11.0 * s, INSET());
    let prev = Rect::from_min_size(pb.min, Vec2::new(38.0 * s, pb.height()));
    let next = Rect::from_min_size(Pos2::new(pb.right() - 38.0 * s, pb.top()), Vec2::new(38.0 * s, pb.height()));
    let mid = Rect::from_min_max(Pos2::new(prev.right(), pb.top()), Pos2::new(next.left(), pb.bottom()));
    let rp = ui.interact(prev, ui.id().with("sc_prev"), Sense::click());
    let rn = ui.interact(next, ui.id().with("sc_next"), Sense::click());
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

    // A / B
    let ab_x = l.w - l.pad - 38.0 - 10.0 - 80.0;
    if let Some(i) = seg(ui, r(o, s, ab_x, y, 80.0, h), &["A", "B"], st.ab_active as usize, "sc_ab", s, 13.0, theme::FRAME(), true) {
        crate::gui_v3::ab_switch(st, params, setter, i as u8);
    }

    // light / dark
    let tb = r(o, s, l.w - l.pad - 38.0, y, 38.0, h);
    let tresp = ui.interact(tb, ui.id().with("sc_theme"), Sense::click());
    ui.painter().rect_filled(tb, 11.0 * s, theme::FRAME());
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
// Picture and lane (frames this pass; their drawings come next)
// ─────────────────────────────────────────────────────────────────────────────

fn picture(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, sc: &mut ScopeState) {
    let (x, y, w, h) = l.picture;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    p.rect_filled(rect, g::R_CARD * s, theme::FRAME());
    text(&p, rect.min + Vec2::new(16.0, 24.0) * s, Align2::LEFT_CENTER, "Taps", 12.0, s, INK());
    let vx = x + w - 12.0 - 120.0;
    if let Some(i) = seg(ui, r(o, s, vx, y + 10.0, 120.0, 28.0), &["Field", "Bars"], sc.view, "sc_view", s, 12.0, PAPER(), true) {
        sc.view = i;
    }
    let yx = vx - 8.0 - 190.0;
    text(&p, r(o, s, yx - 16.0, y + 10.0, 12.0, 28.0).center(), Align2::CENTER_CENTER, "Y", 12.0, s, SOFT());
    if let Some(i) = seg(ui, r(o, s, yx, y + 10.0, 190.0, 28.0), &["Pan", "Gain", "Cutoff"], sc.y_axis, "sc_y", s, 12.0, PAPER(), true) {
        sc.y_axis = i;
    }
    text(&p, rect.center(), Align2::CENTER_CENTER, "Tap field — next pass", 12.0, s, SOFT());
}

fn lane(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, sc: &mut ScopeState) {
    let (x, y, w, h) = l.lane;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    p.rect_filled(rect, g::R_CARD * s, theme::FRAME());
    if let Some(i) = seg(ui, r(o, s, x + 14.0, y + 7.0, 150.0, 26.0), &["Echo", "Motion"], sc.lane_tab, "sc_lane", s, 13.0, PAPER(), true) {
        sc.lane_tab = i;
    }
    p.line_segment(
        [o + Vec2::new(x, y + 40.0) * s, o + Vec2::new(x + w, y + 40.0) * s],
        Stroke::new(1.0 * s, theme::line()),
    );
    let (curves, ctl, cols) = if l.lane_stacked {
        (r(o, s, x, y + 40.0, w, 136.0), r(o, s, x + 14.0, y + 176.0, w - 28.0, h - 180.0), 5)
    } else {
        (r(o, s, x, y + 40.0, 470.0, h - 40.0), r(o, s, x + 482.0, y + 44.0, w - 496.0, h - 48.0), 3)
    };
    let placeholder = if sc.lane_tab == 0 { "Echo lane — next pass" } else { "Motion — next pass" };
    text(&p, curves.center(), Align2::CENTER_CENTER, placeholder, 12.0, s, SOFT());

    // The loop's controls (real params). Feedback is a knob until the lane's
    // drag handles exist.
    let title_w = 52.0 * s;
    let cells = Rect::from_min_max(ctl.min + Vec2::new(title_w, 0.0), ctl.max);
    let c = |row, col, span| cell_at(cells, cols, 2, row, col, span, s);
    for (row, t) in ["Cross", "Loop"].iter().enumerate() {
        let (ctl_row, _, _) = cell_parts(c(row, 0, 1), s);
        text(&p, Pos2::new(ctl.left(), ctl_row.center().y), Align2::LEFT_CENTER, t, 12.0, s, INK2());
    }
    knob_cell(ui, c(0, 0, 1), s, setter, &params.crossfeed, "crossfeed", "X-feed", false, true, None, &mut sc.focus);
    enum_cell(ui, c(0, 1, 2), s, setter, &params.crossfeed_mode, "crossfeed_mode", "Mode", &["Mix", "Ping", "Diffuse"], true, &mut sc.focus);
    knob_cell(ui, c(1, 0, 1), s, setter, &params.feedback, "feedback", "Feedback", false, true, None, &mut sc.focus);
    enum_cell(ui, c(1, 1, 2), s, setter, &params.fb_curve, "fb_curve", "Curve", &["Natural", "Bloom", "Gate"], true, &mut sc.focus);
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
    match id.as_str() {
        "base_time_l" => val = params.format_base_time(n, tempo),
        "base_time_r" => val = PetalParams::format_base_time_mode(params.sync_mode_r_eff(), n, tempo),
        _ => {}
    }
    let mid = l.focus.1 * 0.5;
    let p = ui.painter();
    text(p, rect.min + Vec2::new(16.0, mid - 11.0) * s, Align2::LEFT_CENTER, &format!("{name} · last touched"), 11.0, s, INK2());
    mono(p, rect.min + Vec2::new(16.0, mid + 11.0) * s, Align2::LEFT_CENTER, &val, 22.0, s, INK());
    let track = Rect::from_min_size(rect.min + Vec2::new(236.0, mid - 2.0) * s, Vec2::new(l.right_w - 256.0, 4.0) * s);
    p.rect_filled(track, 2.0 * s, theme::line2());
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * n, track.height())), 2.0 * s, G());
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
    let inner = card(ui, rect, "Time & taps", s);
    // 6 columns × 3 rows; the Shape pad takes columns 4–5 of rows 1–2.
    let c = |row, col, span| cell_at(inner, 6, 3, row, col, span, s);
    let nl = params.base_time_l.unmodulated_normalized_value();
    knob_cell(ui, c(0, 0, 1), s, setter, &params.base_time_l, "base_time_l", "Time", false, true, Some(params.format_base_time(nl, tempo)), focus);
    enum_cell(ui, c(0, 1, 1), s, setter, &params.sync_mode, "sync_mode", "Sync", &["Free", "Sync"], true, focus);
    enum_cell(ui, c(0, 2, 2), s, setter, &params.delay_mode, "delay_mode", "Mode", &["Repitch", "Fade", "Jump"], true, focus);
    toggle_cell(ui, c(0, 4, 1), s, setter, &params.time_linked, "time_linked", "Time L/R", "Linked", "Free", focus);
    knob_cell(ui, c(0, 5, 1), s, setter, &params.num_taps_active, "num_taps_active", "Taps", false, true, None, focus);
    enum_cell(ui, c(1, 0, 4), s, setter, &params.spacing_mode, "spacing_mode", "Spacing", &["Lin", "Exp", "Log", "Euclid"], true, focus);
    // R side: only live when L/R is unlinked.
    let linked = params.time_linked.value();
    let nr = params.base_time_r.unmodulated_normalized_value();
    let r_txt = PetalParams::format_base_time_mode(params.sync_mode_r_eff(), nr, tempo);
    knob_cell(ui, c(2, 0, 1), s, setter, &params.base_time_r, "base_time_r", "Time R", false, !linked, Some(r_txt), focus);
    enum_cell(ui, c(2, 1, 1), s, setter, &params.sync_mode_r, "sync_r", "Sync R", &["Free", "Sync"], !linked, focus);
    let pad_area = c(1, 4, 2).union(c(2, 4, 2));
    shape_pad(ui, pad_area, s, params, setter, focus);
}

/// The Shape pad: X bends where the taps bunch up, Y how hard (shape_x −1…1,
/// shape_y 0…1 with 1 at the top, as in v3). Ticks show the real tap positions.
fn shape_pad(ui: &mut egui::Ui, area: Rect, s: f32, params: &Arc<PetalParams>, setter: &ParamSetter, focus: &mut Option<String>) {
    let p = ui.painter().clone();
    text(&p, Pos2::new(area.center().x, area.top() + g::LBL_H * 0.5 * s), Align2::CENTER_CENTER, "Shape", 11.0, s, INK2());
    let side = (area.height() - 2.0 * (g::LBL_H + 2.0) * s).min(area.width());
    let pad = Rect::from_center_size(Pos2::new(area.center().x, area.top() + (g::LBL_H + 2.0) * s + side * 0.5), Vec2::splat(side));
    p.rect_filled(pad, 8.0 * s, INSET());

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
    let hair = Stroke::new(1.0 * s, theme::with_alpha(G(), 80));
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
    p.circle_filled(dot, 3.0 * s, G());
    let linked = if params.shape_linked.value() { "Linked" } else { "L" };
    mono(&p, Pos2::new(area.center().x, pad.bottom() + (2.0 + g::LBL_H * 0.5) * s), Align2::CENTER_CENTER, linked, 11.5, s, INK());
}

/// Diffuse · Tape · Saturate on one card; a dot marks the blocks that are on.
fn colour_card(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, sc: &mut ScopeState) {
    let rect = r(o, s, l.right_x, l.colour.0, l.right_w, l.colour.1);
    let inner = card(ui, rect, "Colour", s);
    let tabs = Rect::from_min_size(Pos2::new(rect.right() - 254.0 * s, rect.top() + 7.0 * s), Vec2::new(240.0, 26.0) * s);
    if let Some(i) = seg(ui, tabs, &["Diffuse", "Tape", "Saturate"], sc.colour_tab, "sc_colour", s, 12.0, INSET(), true) {
        sc.colour_tab = i;
    }
    let on = [params.diffuse_amount.value() > 0.0, params.degrade_amount.value() > 0.0, params.sat_amount.value() > 0.0];
    let seg_w = (tabs.width() - 5.0 * s - 4.0 * s) / 3.0;
    for (i, &is_on) in on.iter().enumerate() {
        let c = Pos2::new(tabs.left() + 2.5 * s + i as f32 * (seg_w + 2.0 * s) + seg_w - 8.0 * s, tabs.center().y);
        ui.painter().circle_filled(c, 2.5 * s, if is_on { G() } else { theme::line2() });
    }
    let body = Rect::from_min_max(Pos2::new(inner.left(), tabs.bottom()), inner.max);
    let c = |col, span| cell_at(body, 6, 1, 0, col, span, s);
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
    let c = |col| cell_at(inner, 6, 1, 0, col, 1, s);
    knob_cell(ui, c(0), s, setter, &params.input_gain, "input_gain", "In", true, true, None, focus);
    knob_cell(ui, c(1), s, setter, &params.dry_wet, "dry_wet", "Mix", false, true, None, focus);
    knob_cell(ui, c(2), s, setter, &params.output_gain, "output_gain", "Out", true, true, None, focus);
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
                                for (i, name) in ["Cool", "Warm", "Ink", "OLED"].iter().enumerate() {
                                    if epo::chip(ui, name, sc.look.dark_variant == i as u8) {
                                        sc.look.dark_variant = i as u8;
                                    }
                                }
                            });
                        });
                        menu_row(ui, "Accent", s, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0 * s;
                                for i in 0..theme::ACCENT_NAMES.len() as u8 {
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
            // header: tabs end before the preset box, which ends before A/B
            let tabs_end = l.pad + 176.0 + l.tabs_w;
            let preset = ((l.w - l.preset_w) * 0.5, (l.w + l.preset_w) * 0.5);
            assert!(tabs_end < preset.0 && preset.1 < l.w - l.pad - 38.0 - 10.0 - 80.0);
        }
    }
}
