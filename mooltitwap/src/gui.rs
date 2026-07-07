//! Mooltitwap GUI — port of the Petal v2 mockup design.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke,
        StrokeKind, Ui, Vec2,
    },
    EguiState,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::params::{
    snap_to_div, ChannelMode, FilterType, PetalParams, SpacingMode, SyncMode, NUM_TAPS, SYNC_DIVS,
};

/// Read the live tempo published by the audio thread.
#[inline]
fn read_tempo(tempo_atomic: &Arc<AtomicU32>) -> f32 {
    let raw = tempo_atomic.load(Ordering::Relaxed);
    let t = f32::from_bits(raw);
    if t.is_finite() && t > 1.0 { t } else { 120.0 }
}

pub const WINDOW_W: u32 = 1080;
// 660 ≈ the 1280×780 Inertia frame ratio; also gives the left card enough
// height for all its sections (the old 560 clipped SATURATION off-screen).
pub const WINDOW_H: u32 = 660;
const HEADER_H: f32 = 48.0;
const LEFT_W: f32 = 224.0;
const RIGHT_W: f32 = 284.0;
const SHAPING_H: f32 = 118.0;
const RIGHT_FOOTER_H: f32 = 70.0;
/// Body page margin (kit `.body` padding) and card gutter.
const PAGE_M: f32 = 14.0;
const CARD_GAP: f32 = 10.0;
/// Extra space above cards so the border chip can straddle the card border.
const CHIP_OVERHANG: f32 = 8.0;

// ─── Palette — Inertia dark tokens, taken from the ORIGINAL design kit
// (/Volumes/Plexicon/CODING/Inertia/Inertia/inertia-ui.css, html[data-theme=
// "dark"]). NOTE this differs from cumulus's port: in the real kit INSET is
// the LIGHTEST surface in dark mode — wells/buttons/viz sit on surfaces
// lighter than their panel and pop forward instead of receding. ─────────────
mod c {
    use nih_plug_egui::egui::Color32;
    /// Host page behind the plugin frame (rarely visible).
    pub const PAPER:   Color32 = Color32::from_rgb(0x10, 0x12, 0x15);
    /// The plugin frame background — window bg, title-chip backing.
    pub const FRAME:   Color32 = Color32::from_rgb(0x18, 0x1B, 0x1F);
    /// Card bodies — one step up from FRAME.
    pub const PANEL:   Color32 = Color32::from_rgb(0x1E, 0x22, 0x27);
    /// Hover wash / titlebar.
    pub const PANEL2:  Color32 = Color32::from_rgb(0x21, 0x26, 0x2C);
    /// Wells: buttons, fields, viz surface, knob faces — the LIGHTEST fill.
    pub const INSET:   Color32 = Color32::from_rgb(0x26, 0x2B, 0x32);
    /// Neutral viz ink (waveform grey).
    pub const WAVE:    Color32 = Color32::from_rgb(0x36, 0x3C, 0x44);
    /// Knob bevel highlight + pointer under-shadow.
    pub const METALHI: Color32 = Color32::from_rgb(0x2C, 0x32, 0x3A);
    /// Knob bevel dark edge.
    pub const METALLO: Color32 = Color32::from_rgb(0x17, 0x1A, 0x1E);
    pub const INK:     Color32 = Color32::from_rgb(0xEC, 0xE8, 0xDF);
    /// Secondary readable text (idle button labels).
    pub const INK2:    Color32 = Color32::from_rgb(0xB9, 0xB5, 0xAB);
    /// Labels, dimmed values.
    pub const SOFT:    Color32 = Color32::from_rgb(0x86, 0x8B, 0x92);
    /// Section titles, ticks, inactive tabs.
    pub const FAINT:   Color32 = Color32::from_rgb(0x56, 0x5C, 0x64);
    /// Ink @20% — the strong hairline: well/button borders, knob track.
    /// (premultiplied components = what from_rgba_unmultiplied(INK, a) yields)
    pub const LINE:      Color32 = Color32::from_rgba_premultiplied(114, 112, 107, 51);
    /// Ink @12% — card borders, dividers.
    pub const LINE_SOFT: Color32 = Color32::from_rgba_premultiplied(90, 88, 84, 31);
    /// Ink @7% — fine grids inside visualizers.
    pub const HAIR:      Color32 = Color32::from_rgba_premultiplied(69, 67, 65, 18);
    /// Accent A — Inertia violet, mooltitwap's identity hue (taps, time domain).
    pub const ACCENT:   Color32 = Color32::from_rgb(0xA0, 0x86, 0xE8);
    pub const ACCENT_2: Color32 = Color32::from_rgb(0xC9, 0xB8, 0xF2);
    /// Accent B — Inertia coral, color-codes the feedback domain.
    pub const ACCENT_B: Color32 = Color32::from_rgb(0xE8, 0x87, 0x62);

    /// Alpha-fade an opaque token (kit `--*Dim` / `--*Glow` are made this way).
    pub fn with_alpha(c: Color32, a: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
    }
    /// Soft glow tint for accent-filled active controls (kit `--aGlow` ≈ 34%).
    pub fn glow(c: Color32) -> Color32 { with_alpha(c, 87) }
}

/// Audio-taper skew for all gain sliders/grids: cube-root (≈ 0.333) maps
/// 75 % slider position → 0 dB, matching the feel of a DAW volume fader.
///   pos 0.75 → 0 dB   pos 0.50 → −8 dB   pos 0.25 → −18 dB   pos 0.40 → −11 dB
/// This replaces the old skew=2 which put 0 dB at 95 % and −49 dB at 40 %.
const GAIN_SKEW: f32 = 0.333;

/// dB value → normalised [0,1] grid/slider position, using GAIN_SKEW.
/// +6 dB → 1.0   0 dB → ≈0.75   -24 dB → ≈0.15   -60 dB → 0.0
#[inline]
fn gain_db_to_pos(db: f32) -> f32 {
    let raw = ((db + 60.0) / 66.0).clamp(0.0, 1.0);
    raw.powf(1.0 / GAIN_SKEW)
}
/// Inverse of gain_db_to_pos.
#[inline]
fn pos_to_gain_db(pos: f32) -> f32 {
    -60.0 + pos.clamp(0.0, 1.0).powf(GAIN_SKEW) * 66.0
}

const RR_ZERO: CornerRadius = CornerRadius { nw: 0, ne: 0, sw: 0, se: 0 };

/// Scale factor passed through all draw functions. `S(1.0)` = native 1080×560
/// design size. `S(1.5)` = 150% zoom etc. Multiply any fixed pixel value by
/// `s.f(x)`; build a scaled CornerRadius with `s.rr(r)`; scale a Stroke width
/// with `s.f(w)`.
#[derive(Clone, Copy)]
pub struct S(pub f32);
impl S {
    /// Scale a single f32 (px size, padding, font size, stroke width).
    #[inline] pub fn f(self, x: f32) -> f32 { x * self.0 }
    /// Build a scaled CornerRadius from a base radius.
    #[inline] pub fn rr(self, r: f32) -> CornerRadius {
        let v = (r * self.0).round().clamp(0.0, 255.0) as u8;
        CornerRadius { nw: v, ne: v, sw: v, se: v }
    }
}

pub struct EditorState {
    pub viz_mode: VizMode,
    pub right_tab: RightTab,
    pub drag_tap: Option<usize>,
    pub drag_kind: DragKind,
    /// Accumulated drag pixels for the tap-count drag-on-number widget.
    pub tap_count_drag_accum: f32,
    /// Tap count at the moment a drag-on-number started (absolute reference).
    pub tap_count_drag_start: i32,
    /// Accumulated drag pixels for the FB TIME drag widget.
    pub fb_time_drag_accum: f32,
    /// Normalised feedback_time value at the start of the current drag.
    pub fb_time_drag_start: f32,
    /// Right-panel vertical scroll offset (px) when n_active rows overflow.
    pub right_panel_scroll: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VizMode { Stems, Grid }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RightTab { Pan, Gain, Filter, XFeed, Diffuse }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DragKind { None, GridBall }

impl Default for EditorState {
    fn default() -> Self {
        Self {
            viz_mode: VizMode::Stems,
            right_tab: RightTab::Pan,
            drag_tap: None,
            drag_kind: DragKind::None,
            tap_count_drag_accum: 0.0,
            tap_count_drag_start: 8,
            fb_time_drag_accum: 0.0,
            fb_time_drag_start: 0.0,
            right_panel_scroll: 0.0,
        }
    }
}

/// Register the Epoché font pairing (Space Grotesk prop / Spline Sans Mono
/// mono) — same faces as cumulus/y-flanger so the family reads as one.
fn install_fonts(ctx: &egui::Context) {
    use nih_plug_egui::egui::{FontData, FontDefinitions};
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "grotesk".into(),
        FontData::from_static(include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf")).into(),
    );
    fonts.font_data.insert(
        "splinemono".into(),
        FontData::from_static(include_bytes!("../assets/fonts/SplineSansMono.ttf")).into(),
    );
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "grotesk".into());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "splinemono".into());
    ctx.set_fonts(fonts);
}

pub fn create_editor(
    params: Arc<PetalParams>,
    egui_state: Arc<EguiState>,
    tempo_bpm_shared: Arc<AtomicU32>,
) -> Option<Box<dyn Editor>> {
    let editor_state = std::sync::Mutex::new(EditorState::default());
    create_egui_editor(
        egui_state.clone(),
        editor_state,
        |ctx, _| install_fonts(ctx),
        move |egui_ctx, setter, state| {
            // NOTE: we deliberately do NOT call `set_pixels_per_point` here.
            // egui-baseview's renderer tessellates using its own internal ppp
            // (set from window scale events), so overriding egui_ctx's ppp
            // creates a mismatch that produces blurry/misaligned output.
            // Instead we keep ppp at native and scale all our drawing by
            // `scale = avail.width() / WINDOW_W` (computed inside draw_plugin).
            let tempo_bpm = read_tempo(&tempo_bpm_shared);
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                let mut st = state.lock().unwrap();
                draw_plugin(ui, &params, setter, &mut st, tempo_bpm, egui_state.as_ref());
            });
        },
    )
}

// ─── Top-level draw ──────────────────────────────────────────────────────
fn draw_plugin(ui: &mut Ui, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState, tempo_bpm: f32, egui_state: &EguiState) {
    let avail = ui.available_rect_before_wrap();
    // Compute the global scale factor from the actual window size. This is
    // the single source of truth — every fixed pixel value in the drawing
    // code is multiplied by `s.f(...)` to scale with the window.
    let s = S(avail.width() / WINDOW_W as f32);

    // Sessions saved before the 560→660 height change restore a too-short
    // window; snap the height back to the design aspect (sizes only ever
    // change via the zoom buttons, so the ratio is otherwise always exact).
    let (cur_w, cur_h) = egui_state.size();
    let want_h = (cur_w as f32 * WINDOW_H as f32 / WINDOW_W as f32).round() as u32;
    if cur_h != want_h {
        egui_state.request_size(cur_w, want_h);
    }

    ui.painter().rect_filled(avail, RR_ZERO, c::FRAME);

    let header_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), s.f(HEADER_H)));
    draw_header(ui, header_rect, egui_state, s);

    // Three chip-on-border cards (kit .sec) floating on the FRAME background.
    let m = s.f(PAGE_M);
    let gap = s.f(CARD_GAP);
    let cards_top = header_rect.bottom() + s.f(CHIP_OVERHANG) + s.f(4.0);
    let cards_bot = avail.max.y - m;
    let left_card = Rect::from_min_max(
        Pos2::new(avail.min.x + m, cards_top),
        Pos2::new(avail.min.x + m + s.f(LEFT_W), cards_bot),
    );
    let right_card = Rect::from_min_max(
        Pos2::new(avail.max.x - m - s.f(RIGHT_W), cards_top),
        Pos2::new(avail.max.x - m, cards_bot),
    );
    let center_card = Rect::from_min_max(
        Pos2::new(left_card.max.x + gap, cards_top),
        Pos2::new(right_card.min.x - gap, cards_bot),
    );

    card(ui, left_card, s);
    card(ui, center_card, s);
    card(ui, right_card, s);

    card_chip(ui, left_card, "DELAY", c::ACCENT, s);
    if let Some(i) = card_chip_tabs(ui, center_card, "VIEW", &[
        ("STEMS", st.viz_mode == VizMode::Stems),
        ("GRID", st.viz_mode == VizMode::Grid),
    ], s) {
        st.viz_mode = if i == 0 { VizMode::Stems } else { VizMode::Grid };
    }
    if let Some(i) = card_chip_tabs(ui, right_card, "TAPS", &[
        ("PAN",     st.right_tab == RightTab::Pan),
        ("GAIN",    st.right_tab == RightTab::Gain),
        ("FILTER",  st.right_tab == RightTab::Filter),
        ("XFEED",   st.right_tab == RightTab::XFeed),
        ("DIFFUSE", st.right_tab == RightTab::Diffuse),
    ], s) {
        st.right_tab = [RightTab::Pan, RightTab::Gain, RightTab::Filter,
                        RightTab::XFeed, RightTab::Diffuse][i];
    }

    let body_inset = Vec2::new(s.f(2.0), s.f(10.0));
    draw_left(ui, Rect::from_min_max(left_card.min + body_inset,
                                     left_card.max - Vec2::new(body_inset.x, s.f(2.0))),
              params, setter, tempo_bpm, s);
    draw_center(ui, Rect::from_min_max(center_card.min + body_inset,
                                       center_card.max - Vec2::new(body_inset.x, s.f(2.0))),
                params, setter, st, tempo_bpm, s);
    draw_right(ui, Rect::from_min_max(right_card.min + body_inset,
                                      right_card.max - Vec2::new(body_inset.x, s.f(2.0))),
               params, setter, st, tempo_bpm, s);
}

/// Kit `.sec` card: PANEL fill, 14px radius, soft ink hairline border.
fn card(ui: &mut Ui, rect: Rect, s: S) {
    let p = ui.painter();
    p.rect_filled(rect, s.rr(14.0), c::PANEL);
    p.rect_stroke(rect, s.rr(14.0), Stroke::new(s.f(1.0), c::LINE_SOFT), StrokeKind::Inside);
}

/// Kit `.sec-h` chip: a FRAME-backed uppercase title sitting ON the card's
/// top border, cutting a notch into it.
fn card_chip(ui: &mut Ui, card: Rect, title: &str, col: Color32, s: S) {
    let p = ui.painter();
    let font = FontId::new(s.f(9.5), FontFamily::Proportional);
    let galley = p.layout_no_wrap(title.to_owned(), font, col);
    let chip = Rect::from_min_size(
        Pos2::new(card.min.x + s.f(18.0), card.min.y - galley.size().y * 0.55),
        galley.size() + Vec2::new(s.f(18.0), 0.0),
    );
    p.rect_filled(chip, RR_ZERO, c::FRAME);
    p.galley(chip.min + Vec2::new(s.f(9.0), 0.0), galley, col);
}

/// Interactive variant of the border chip: `SECTION · TAB · TAB …`.
/// Section label stays SOFT; the active tab is the bright INK one; dots are
/// faint mono. Returns the clicked tab index.
fn card_chip_tabs(ui: &mut Ui, card: Rect, section: &str,
                  items: &[(&str, bool)], s: S) -> Option<usize> {
    let font = FontId::new(s.f(9.5), FontFamily::Proportional);
    let dot_font = FontId::new(s.f(8.5), FontFamily::Monospace);
    let cy = card.min.y;
    let mut clicked = None;

    // Measure total width first so the FRAME backing covers the whole row.
    let sec_g = ui.painter().layout_no_wrap(section.to_owned(), font.clone(), c::SOFT);
    let mut total = sec_g.size().x;
    let mut galleys = Vec::with_capacity(items.len());
    for (lab, _) in items {
        let g = ui.painter().layout_no_wrap((*lab).to_owned(), font.clone(), c::FAINT);
        total += s.f(16.0) + g.size().x; // dot cell + label
        galleys.push(g);
    }
    let chip = Rect::from_min_size(
        Pos2::new(card.min.x + s.f(18.0), cy - sec_g.size().y * 0.55),
        Vec2::new(total + s.f(18.0), sec_g.size().y),
    );
    ui.painter().rect_filled(chip, RR_ZERO, c::FRAME);

    let mut x = chip.min.x + s.f(9.0);
    ui.painter().galley(Pos2::new(x, chip.min.y), sec_g.clone(), c::SOFT);
    x += sec_g.size().x;
    for (i, ((lab, active), g)) in items.iter().zip(galleys).enumerate() {
        ui.painter().text(Pos2::new(x + s.f(8.0), cy), Align2::CENTER_CENTER,
                          "\u{00B7}", dot_font.clone(), c::FAINT);
        x += s.f(16.0);
        let trect = Rect::from_min_size(
            Pos2::new(x - s.f(3.0), cy - g.size().y * 0.5 - s.f(5.0)),
            g.size() + Vec2::new(s.f(6.0), s.f(10.0)),
        );
        let resp = ui.interact(trect, ui.id().with((section, "ctab", i)), Sense::click());
        if resp.clicked() { clicked = Some(i); }
        let col = if *active { c::INK }
            else if resp.hovered() { c::INK2 }
            else { c::FAINT };
        ui.painter().galley(Pos2::new(x, chip.min.y), g.clone(), col);
        let _ = lab;
        x += g.size().x;
    }
    clicked
}

// ─── Header ──────────────────────────────────────────────────────────────
fn draw_header(ui: &mut Ui, rect: Rect, egui_state: &EguiState, s: S) {
    {
        let p = ui.painter();
        p.rect_filled(rect, RR_ZERO, c::PANEL2);
        p.line_segment([Pos2::new(rect.min.x, rect.max.y), Pos2::new(rect.max.x, rect.max.y)],
                       Stroke::new(s.f(1.0), c::LINE_SOFT));

        let mark_center = Pos2::new(rect.min.x + s.f(22.0), rect.center().y);
        draw_brand_mark(p, mark_center, s.f(9.0), s);
        // Epoché wordmark idiom: lowercase name · painted accent dot · mono meta.
        let name_x = mark_center.x + s.f(14.0);
        p.text(Pos2::new(name_x, rect.center().y), Align2::LEFT_CENTER,
               "mooltitwap", FontId::new(s.f(15.0), FontFamily::Proportional), c::INK);
        p.circle_filled(Pos2::new(name_x + s.f(87.0), rect.center().y + s.f(2.0)),
                        s.f(2.2), c::ACCENT);
        // Build version, sourced from Cargo.toml at compile time so it can
        // never go stale. If you don't see the version you expect, the DAW
        // is loading a cached plugin from a different path.
        let ver = concat!("v", env!("CARGO_PKG_VERSION"));
        p.text(Pos2::new(name_x + s.f(94.0), rect.center().y), Align2::LEFT_CENTER,
               ver, FontId::new(s.f(9.0), FontFamily::Monospace), c::SOFT);

        // preset pill (placeholder)
        let pill_rect = Rect::from_center_size(rect.center(), Vec2::new(s.f(240.0), s.f(26.0)));
        p.rect_filled(pill_rect, s.rr(9.0), c::INSET);
        p.rect_stroke(pill_rect, s.rr(9.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
        p.text(pill_rect.center(), Align2::CENTER_CENTER, "DEFAULT",
               FontId::new(s.f(10.0), FontFamily::Proportional), c::INK);
    }

    // ── Zoom selector ────────────────────────────────────────────────────
    let (cur_w, _) = egui_state.size();
    let cur_scale = cur_w as f32 / WINDOW_W as f32;
    let zooms: &[(f32, &str)] = &[
        (0.75, "75"),
        (1.00, "100"),
        (1.25, "125"),
        (1.50, "150"),
        (2.00, "200"),
    ];
    let btn_w = s.f(30.0);
    let btn_h = s.f(22.0);
    let gap = s.f(2.0);
    let total_w = btn_w * zooms.len() as f32 + gap * (zooms.len() as f32 - 1.0);
    let mut x = rect.max.x - s.f(12.0) - total_w;
    for (factor, label) in zooms.iter() {
        let r = Rect::from_min_size(Pos2::new(x, rect.center().y - btn_h * 0.5),
                                    Vec2::new(btn_w, btn_h));
        let active = (cur_scale - factor).abs() < 0.01;
        let resp = ui.interact(r, ui.id().with(("zoom", label)), Sense::click());
        // Kit `.seg-host`: inset wells, active segment = solid INK + dark text.
        let (bg, fg) = if active {
            (c::INK, c::FRAME)
        } else if resp.hovered() {
            (c::PANEL2, c::INK)
        } else {
            (c::INSET, c::SOFT)
        };
        {
            let p = ui.painter();
            p.rect_filled(r, s.rr(6.0), bg);
            if !active {
                p.rect_stroke(r, s.rr(6.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
            }
            p.text(r.center(), Align2::CENTER_CENTER, *label,
                   FontId::new(s.f(9.0), FontFamily::Proportional), fg);
        }
        if resp.clicked() && !active {
            let target_w = (WINDOW_W as f32 * factor).round() as u32;
            let target_h = (WINDOW_H as f32 * factor).round() as u32;
            egui_state.request_size(target_w, target_h);
        }
        x += btn_w + gap;
    }
}

fn draw_brand_mark(p: &egui::Painter, center: Pos2, r: f32, s: S) {
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
            p.line_segment([w[0], w[1]], Stroke::new(s.f(1.0), c::ACCENT));
        }
    }
    p.circle_filled(center, s.f(1.6), c::INK);
}

// ─── LEFT PANEL ──────────────────────────────────────────────────────────
fn draw_left(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, tempo_bpm: f32, s: S) {
    // Hero: Time L knob – link button – Time R knob
    let hero_h = s.f(88.0);
    let hero_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), hero_h));
    {
        let p = ui.painter();
        p.line_segment([Pos2::new(rect.min.x, hero_rect.max.y), Pos2::new(rect.max.x, hero_rect.max.y)],
                       Stroke::new(s.f(1.0), c::LINE_SOFT));
    }

    let knob_size = s.f(50.0);
    let cy = hero_rect.min.y + s.f(30.0);
    let lknob_center = Pos2::new(hero_rect.center().x - s.f(56.0), cy);
    let rknob_center = Pos2::new(hero_rect.center().x + s.f(56.0), cy);

    let time_l_val = params.base_time_l.value();
    let time_r_val = if params.time_linked.value() { time_l_val } else { params.base_time_r.value() };
    let l_label = params.format_base_time(time_l_val, tempo_bpm);
    let r_label = params.format_base_time(time_r_val, tempo_bpm);

    knob_at(ui, lknob_center, knob_size, &params.base_time_l, setter, s);
    if params.time_linked.value() {
        knob_at(ui, rknob_center, knob_size, &params.base_time_l, setter, s);
    } else {
        knob_at(ui, rknob_center, knob_size, &params.base_time_r, setter, s);
    }

    let ms_mode = params.channel_mode.value() == ChannelMode::MidSide;
    let (l_name, r_name) = if ms_mode { ("TIME M", "TIME S") } else { ("TIME L", "TIME R") };

    {
        let p = ui.painter();
        p.text(Pos2::new(lknob_center.x, lknob_center.y + s.f(30.0)), Align2::CENTER_TOP,
               &l_label, FontId::new(s.f(11.0), FontFamily::Monospace), c::INK);
        p.text(Pos2::new(lknob_center.x, lknob_center.y + s.f(44.0)), Align2::CENTER_TOP,
               l_name, FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
        p.text(Pos2::new(rknob_center.x, rknob_center.y + s.f(30.0)), Align2::CENTER_TOP,
               &r_label, FontId::new(s.f(11.0), FontFamily::Monospace), c::INK);
        p.text(Pos2::new(rknob_center.x, rknob_center.y + s.f(44.0)), Align2::CENTER_TOP,
               r_name, FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    }

    let link_center = Pos2::new(hero_rect.center().x, cy);
    let link_r = s.f(11.0);
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
        // Kit `.btn.on`: accent fill + white glyph + soft glow when linked.
        let (fill, border, ink) = if linked {
            (c::ACCENT, c::ACCENT, Color32::WHITE)
        } else {
            (c::INSET, c::LINE, c::SOFT)
        };
        p.circle_filled(link_center, link_r, fill);
        p.circle_stroke(link_center, link_r, Stroke::new(s.f(1.0), border));
        if linked {
            p.circle_stroke(link_center, link_r + s.f(1.5), Stroke::new(s.f(1.0), c::glow(c::ACCENT)));
        }
        p.text(link_center, Align2::CENTER_CENTER, "🔗",
               FontId::new(s.f(10.0), FontFamily::Proportional), ink);
    }

    let mut y = hero_rect.max.y;
    y = section(ui, rect, y, "SYNC", s.f(30.0), s, |ui, sec| {
        let cw = sec.width() / 2.0 - s.f(1.0);
        let r0 = Rect::from_min_size(sec.min, Vec2::new(cw, s.f(26.0)));
        let r1 = Rect::from_min_size(Pos2::new(sec.min.x + cw + s.f(2.0), sec.min.y), Vec2::new(cw, s.f(26.0)));
        let current = params.sync_mode.value();
        if seg_button(ui, r0, "Free", current == SyncMode::Free, s) {
            setter.begin_set_parameter(&params.sync_mode);
            setter.set_parameter(&params.sync_mode, SyncMode::Free);
            setter.end_set_parameter(&params.sync_mode);
        }
        if seg_button(ui, r1, "Sync", current == SyncMode::Sync, s) {
            setter.begin_set_parameter(&params.sync_mode);
            setter.set_parameter(&params.sync_mode, SyncMode::Sync);
            setter.end_set_parameter(&params.sync_mode);
        }
    });

    y = section(ui, rect, y, "DELAY MODE", s.f(30.0), s, |ui, sec| {
        use crate::params::DelayMode;
        let labels = ["Repitch", "Fade", "Jump"];
        let modes = [DelayMode::Repitch, DelayMode::Fade, DelayMode::Jump];
        let cw = sec.width() / 3.0 - s.f(1.0);
        let current = params.delay_mode.value();
        for (i, lab) in labels.iter().enumerate() {
            let r = Rect::from_min_size(
                Pos2::new(sec.min.x + i as f32 * (cw + s.f(2.0)), sec.min.y),
                Vec2::new(cw, s.f(26.0)),
            );
            if seg_button(ui, r, lab, current == modes[i], s) {
                setter.begin_set_parameter(&params.delay_mode);
                setter.set_parameter(&params.delay_mode, modes[i]);
                setter.end_set_parameter(&params.delay_mode);
            }
        }
    });

    y = section(ui, rect, y, "CHANNEL MODE", s.f(30.0), s, |ui, sec| {
        let labels = ["Stereo", "M / S", "Mono"];
        let modes = [ChannelMode::Stereo, ChannelMode::MidSide, ChannelMode::Mono];
        let cw = sec.width() / 3.0 - s.f(1.0);
        let current = params.channel_mode.value();
        for (i, lab) in labels.iter().enumerate() {
            let r = Rect::from_min_size(
                Pos2::new(sec.min.x + i as f32 * (cw + s.f(2.0)), sec.min.y),
                Vec2::new(cw, s.f(26.0)),
            );
            if seg_button(ui, r, lab, current == modes[i], s) {
                setter.begin_set_parameter(&params.channel_mode);
                setter.set_parameter(&params.channel_mode, modes[i]);
                setter.end_set_parameter(&params.channel_mode);
            }
        }
    });

    y = section(ui, rect, y, "LEVELS", s.f(76.0), s, |ui, sec| {
        let knob_s = s.f(38.0);
        let third = sec.width() / 3.0;
        let ky = sec.min.y + s.f(22.0);
        knob_at(ui, Pos2::new(sec.min.x + third * 0.5, ky), knob_s, &params.input_gain, setter, s);
        knob_at(ui, Pos2::new(sec.min.x + third * 1.5, ky), knob_s, &params.dry_wet, setter, s);
        knob_at(ui, Pos2::new(sec.min.x + third * 2.5, ky), knob_s, &params.output_gain, setter, s);
        let p = ui.painter();
        let centers = [
            (Pos2::new(sec.min.x + third * 0.5, ky), "INPUT", format!("{:+.1} dB", params.input_gain.value())),
            (Pos2::new(sec.min.x + third * 1.5, ky), "DRY/WET", format!("{:.0}%", params.dry_wet.value() * 100.0)),
            (Pos2::new(sec.min.x + third * 2.5, ky), "OUTPUT", format!("{:+.1} dB", params.output_gain.value())),
        ];
        for (c, lbl, val) in centers {
            p.text(Pos2::new(c.x, c.y + knob_s * 0.5 + s.f(4.0)), Align2::CENTER_TOP, lbl,
                   FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
            p.text(Pos2::new(c.x, c.y + knob_s * 0.5 + s.f(16.0)), Align2::CENTER_TOP, &val,
                   FontId::new(s.f(10.0), FontFamily::Monospace), c::INK);
        }
    });

    y = section(ui, rect, y, "WOBBLE", s.f(76.0), s, |ui, sec| {
        let knob_s = s.f(38.0);
        let ky = sec.min.y + s.f(22.0);
        let half = sec.width() / 2.0;
        let cx_a = sec.min.x + half * 0.5;
        let cx_b = sec.min.x + half * 1.5;
        knob_at(ui, Pos2::new(cx_a, ky), knob_s, &params.wobble_amount, setter, s);
        knob_at(ui, Pos2::new(cx_b, ky), knob_s, &params.wobble_rate, setter, s);
        let p = ui.painter();
        p.text(Pos2::new(cx_a, ky + knob_s * 0.5 + s.f(4.0)), Align2::CENTER_TOP, "AMOUNT",
               FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
        p.text(Pos2::new(cx_a, ky + knob_s * 0.5 + s.f(16.0)), Align2::CENTER_TOP,
               &format!("{:.0}", params.wobble_amount.value() * 100.0),
               FontId::new(s.f(10.0), FontFamily::Monospace), c::INK);
        p.text(Pos2::new(cx_b, ky + knob_s * 0.5 + s.f(4.0)), Align2::CENTER_TOP, "RATE",
               FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
        p.text(Pos2::new(cx_b, ky + knob_s * 0.5 + s.f(16.0)), Align2::CENTER_TOP,
               &format!("{:.2} Hz", params.wobble_rate.value()),
               FontId::new(s.f(10.0), FontFamily::Monospace), c::INK);
    });

    let _ = section(ui, rect, y, "SATURATION", s.f(30.0), s, |ui, sec| {
        let row = Rect::from_min_size(sec.min, Vec2::new(sec.width(), s.f(20.0)));
        let resp = ui.allocate_rect(row, Sense::click());
        let on = params.soft_clip.value();
        if resp.clicked() {
            setter.begin_set_parameter(&params.soft_clip);
            setter.set_parameter(&params.soft_clip, !on);
            setter.end_set_parameter(&params.soft_clip);
        }
        let p = ui.painter();
        p.text(Pos2::new(row.min.x, row.center().y), Align2::LEFT_CENTER, "SOFT CLIP",
               FontId::new(s.f(10.0), FontFamily::Proportional), if on { c::INK } else { c::SOFT });
        let sw = Rect::from_min_size(
            Pos2::new(row.max.x - s.f(30.0), row.center().y - s.f(8.0)),
            Vec2::new(s.f(28.0), s.f(16.0)),
        );
        // Kit `.pill` toggle: FAINT track off / ACCENT track on, white knob dot.
        p.rect_filled(sw, s.rr(8.0), if on { c::ACCENT } else { c::FAINT });
        let kx = if on { sw.max.x - s.f(7.0) } else { sw.min.x + s.f(7.0) };
        p.circle_filled(Pos2::new(kx, sw.center().y), s.f(6.0), Color32::WHITE);
    });
}

// ─── Section helper ──────────────────────────────────────────────────────
fn section<F: FnOnce(&mut Ui, Rect)>(
    ui: &mut Ui, panel: Rect, y: f32, title: &str, body_h: f32, s: S, body_fn: F,
) -> f32 {
    let pad_x = s.f(14.0);
    let pad_top = s.f(10.0);
    let title_h = s.f(16.0);
    let p = ui.painter();
    p.text(Pos2::new(panel.min.x + pad_x, y + pad_top), Align2::LEFT_TOP, title,
           FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);
    let body_rect = Rect::from_min_max(
        Pos2::new(panel.min.x + pad_x, y + pad_top + title_h),
        Pos2::new(panel.max.x - pad_x, y + pad_top + title_h + body_h),
    );
    body_fn(ui, body_rect);
    let total = pad_top + title_h + body_h + s.f(8.0);
    let line_y = y + total;
    let p = ui.painter();
    p.line_segment([Pos2::new(panel.min.x, line_y), Pos2::new(panel.max.x, line_y)],
                   Stroke::new(s.f(1.0), c::LINE_SOFT));
    line_y
}

// ─── Segmented button — epo `segmented` style: the active segment is a
// solid INK fill with dark text; idle segments are INSET wells with a
// hairline. State reads as grey-value hierarchy, never accent hue. ─────────
fn seg_button(ui: &mut Ui, rect: Rect, label: &str, active: bool, s: S) -> bool {
    let resp = ui.allocate_rect(rect, Sense::click());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, s.rr(6.0), c::INK);
    } else {
        p.rect_filled(rect, s.rr(6.0), c::INSET);
        p.rect_stroke(rect, s.rr(6.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
    }
    let text_col = if active { c::FRAME }
        else if resp.hovered() { c::INK }
        else { c::INK2 };
    p.text(rect.center(), Align2::CENTER_CENTER, label,
           FontId::new(s.f(10.0), FontFamily::Proportional), text_col);
    resp.clicked()
}

// ─── Knob widget ────────────────────────────────────────────────────────
fn knob_at<P: Param>(ui: &mut Ui, center: Pos2, size: f32, param: &P, setter: &ParamSetter, s: S)
where P::Plain: Into<f32> + Copy {
    knob_at_accent(ui, center, size, param, setter, s, c::ACCENT)
}

fn knob_at_accent<P: Param>(ui: &mut Ui, center: Pos2, size: f32, param: &P,
                            setter: &ParamSetter, s: S, accent: Color32)
where P::Plain: Into<f32> + Copy {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let mut norm = param.modulated_normalized_value();
    if resp.drag_started() { setter.begin_set_parameter(param); }
    if resp.dragged() {
        // Shift = fine adjust (8× slower for precise tuning)
        let fine = ui.input(|i| i.modifiers.shift);
        let mult = if fine { 0.0006 } else { 0.005 };
        let delta = -resp.drag_delta().y * mult;
        norm = (norm + delta).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, norm);
    }
    if resp.drag_stopped() { setter.end_set_parameter(param); }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, param.default_normalized_value());
        setter.end_set_parameter(param);
    }
    paint_knob(ui.painter(), center, size, norm, s, accent);
}

/// Inertia A5 knob (kit `knobSVG`): 270° sweep from −135°, neutral hairline
/// track ring, accent value arc over a soft glow copy, INSET disc face (the
/// LIGHT well — this is what makes it read physical) with a faked bevel
/// (drop shadow + METALHI rim arc), accent pointer over a METALHI shadow.
fn paint_knob(p: &egui::Painter, center: Pos2, size: f32, value_norm: f32, s: S, accent: Color32) {
    // 0..1 → point on the 270° arc (0 = −135° = lower-left, x=sin / y=−cos).
    let arc_pts = |v0: f32, v1: f32, r: f32| -> Vec<Pos2> {
        let a0 = (-135.0 + 270.0 * v0).to_radians();
        let a1 = (-135.0 + 270.0 * v1).to_radians();
        (0..=24).map(|i| {
            let t = a0 + (a1 - a0) * i as f32 / 24.0;
            Pos2::new(center.x + r * t.sin(), center.y - r * t.cos())
        }).collect()
    };
    let r = size * 0.5 - s.f(2.0);
    // glow copy under the value arc (kit blurs a 3px copy at 40% — approximate
    // with a wider translucent stroke)
    p.add(egui::Shape::line(arc_pts(0.0, value_norm.max(0.004), r),
                            Stroke::new(s.f(3.6), c::with_alpha(accent, 70))));
    p.add(egui::Shape::line(arc_pts(0.0, 1.0, r), Stroke::new(s.f(2.0), c::LINE)));
    p.add(egui::Shape::line(arc_pts(0.0, value_norm.max(0.004), r),
                            Stroke::new(s.f(2.4), accent)));

    let cr = r - 5.0 * s.0.max(0.8);
    p.circle_filled(center + Vec2::new(0.0, s.f(1.2)), cr, Color32::from_black_alpha(110));
    p.circle_filled(center, cr, c::INSET);
    p.circle_stroke(center, cr, Stroke::new(s.f(1.0), c::LINE));
    // The inner ring + bevel arc are concentric mush below ~8px face radius
    // (the 22px filter-row knobs) — skip them there.
    if cr >= s.f(8.0) {
        p.circle_stroke(center, cr - s.f(2.0), Stroke::new(s.f(1.0), c::with_alpha(c::INK, 14)));
        // Faked radial bevel (kit: metal-hi top-left → inset → metal-lo
        // bottom-right): a light arc upper-left, a dark arc lower-right.
        p.add(egui::Shape::line(arc_pts(0.08, 0.42, cr - s.f(1.0)),
                                Stroke::new(s.f(1.2), c::with_alpha(c::METALHI, 200))));
        p.add(egui::Shape::line(arc_pts(0.58, 0.92, cr - s.f(1.0)),
                                Stroke::new(s.f(1.2), c::with_alpha(c::METALLO, 200))));
    }

    let ang = (-135.0 + 270.0 * value_norm).to_radians();
    let (sin, cos) = (ang.sin(), ang.cos());
    let p0 = Pos2::new(center.x + cr * 0.16 * sin, center.y - cr * 0.16 * cos);
    // Floor the pointer tip so it stays readable on tiny knobs (at cr=4 the
    // epo formula cr−3 leaves a 0.36px pointer).
    let tip = (cr - s.f(3.0)).max(cr * 0.85);
    let p1 = Pos2::new(center.x + tip * sin, center.y - tip * cos);
    p.line_segment([p0 + Vec2::new(0.0, s.f(0.7)), p1 + Vec2::new(0.0, s.f(0.7))],
                   Stroke::new(s.f(2.4), c::METALHI));
    p.line_segment([p0, p1], Stroke::new(s.f(1.9), accent));
}

/// Per-tap colour gradient: tap 0 = darkest violet (= ACCENT #A086E8), tap
/// n−1 = lightest (= ACCENT_2 #C9B8F2). Single-hue on purpose: the gradient
/// encodes tap ORDER, which a 6-hue ring couldn't do over 16 taps.
fn tap_color(i: usize, n_active: usize) -> Color32 {
    let t = if n_active <= 1 { 0.0 } else { i as f32 / (n_active - 1) as f32 };
    let r = (0xa0_u8 as f32 + t * (0xc9_u8 - 0xa0_u8) as f32).round() as u8;
    let g = (0x86_u8 as f32 + t * (0xb8_u8 - 0x86_u8) as f32).round() as u8;
    let b = (0xe8_u8 as f32 + t * (0xf2_u8 - 0xe8_u8) as f32).round() as u8;
    Color32::from_rgb(r, g, b)
}

// ─── Topology positions (mirrors topology.rs math, computed in GUI) ──────
/// Mirrors `TopologyEngine::compute` exactly (see topology.rs for comments).
/// `n_active` anchors: tap 0 = 1/N, tap n_active-1 = n_active/N. Shape only
/// affects intermediate taps. Inactive taps beyond n_active use linear defaults.
fn compute_positions(spacing: SpacingMode, shape_x: f32, shape_y: f32, n_active: usize) -> [f32; NUM_TAPS] {
    let n_active = n_active.clamp(1, NUM_TAPS);
    let first = 1.0 / NUM_TAPS as f32;
    let last  = n_active as f32 / NUM_TAPS as f32;
    let mut out = [0.0; NUM_TAPS];
    for i in 0..NUM_TAPS {
        if i < n_active {
            let t = if n_active == 1 { 0.0 }
                    else { i as f32 / (n_active - 1) as f32 };
            let base = match spacing {
                SpacingMode::Linear      => t,
                SpacingMode::Exponential => t.powf(2.0).min(1.0),
                SpacingMode::Logarithmic => t.sqrt(),
                SpacingMode::Euclidean   => t,
            };
            let shaped = sigmoid_blend(base, shape_y);
            let biased = bias_curve(shaped, shape_x);
            out[i] = first + biased * (last - first);
        } else {
            out[i] = (i + 1) as f32 / NUM_TAPS as f32;
        }
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
    let power = if x >= 0.0 { 1.0 / (1.0 + 2.0 * x) } else { 1.0 + 2.0 * (-x) };
    v.powf(power)
}

/// Cap on how many feedback echo cycles we'll ever draw, even at 100% feedback
/// (where echoes never decay so they'd theoretically go on forever).
/// Set high — the drawing loop already breaks when taps leave the visible area,
/// so this is only a safety cap for the 100 % feedback degenerate case.
const MAX_ECHO_CYCLES: usize = 64;

/// Echoes below this gain factor (linear) aren't worth drawing.
/// Lowered from -40 dB (0.01) to -60 dB (0.001) in v2.10 so moderate-feedback
/// settings show a longer decay tail in the viz, matching how long they
/// actually ring out in audio.
const ECHO_THRESHOLD: f32 = 0.001;

/// Everything `draw_stems` and `draw_grid_viz` need to render one frame.
/// Centralises the auto-range math so feedback echo extension is computed once.
pub struct VizData {
    /// Tap positions normalised to [0,1] over `grid_range_secs` — used by Stems view.
    /// Extended to fit feedback echoes.
    pub times_l: [f32; NUM_TAPS],
    pub times_r: [f32; NUM_TAPS],
    /// Active grid range in seconds — the x-axis for Stems spans 0..grid_range_secs.
    /// Extended to fit feedback echoes.
    pub grid_range_secs: f32,
    /// Tap positions normalised to [0,1] over `grid_range_secs_taps_only` — used by
    /// Grid view, which is sized for direct taps only so they fill the canvas.
    pub times_l_grid: [f32; NUM_TAPS],
    pub times_r_grid: [f32; NUM_TAPS],
    /// Grid range for the Grid view, sized to direct taps only (no feedback extension).
    pub grid_range_secs_taps_only: f32,
    /// Base sync-division length in beats (0 in Free mode). Used to label
    /// the grid in subdivision units instead of raw beats.
    pub div_beats: f32,
    /// Feedback cycle length per channel, NORMALISED to [0,1] of grid_range.
    /// One cycle = one round-trip of the feedback tap. Zero means no echoes.
    pub fb_cycle_norm_l: f32,
    pub fb_cycle_norm_r: f32,
    /// Number of feedback echo cycles to draw past the originals.
    pub n_echo_cycles: usize,
    /// Feedback amount — used as the per-cycle gain multiplier.
    pub feedback: f32,
    /// True when crossfeed_mode is PingPong, so echoes should alternate L↔R.
    pub ping_pong: bool,
}

/// Pick the smallest "nice" grid range that covers `needed_secs`.
/// Sync mode: nice step sizes in beats, minimum 2 bars (8 beats).
/// Free mode: nice step sizes in ms, minimum 2 seconds.
fn pick_nice_range(needed_secs: f32, sync_mode: SyncMode, tempo_bpm: f32) -> f32 {
    match sync_mode {
        SyncMode::Sync => {
            let beat_secs = 60.0 / tempo_bpm;
            let needed_beats = (needed_secs / beat_secs).max(8.0);
            let nice: &[f32] = &[
                8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0, 192.0, 256.0,
            ];
            let grid_beats = nice.iter().copied()
                .find(|&r| needed_beats <= r * 1.05)
                .unwrap_or(256.0);
            grid_beats * beat_secs
        }
        SyncMode::Free => {
            let nice_ms: &[f32] = &[
                2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 12000.0, 16000.0,
                24000.0, 32000.0, 48000.0, 64000.0,
            ];
            let needed_ms = (needed_secs * 1000.0).max(2000.0);
            let grid_ms = nice_ms.iter().copied()
                .find(|&r| needed_ms <= r * 1.05)
                .unwrap_or(64000.0);
            grid_ms * 0.001
        }
    }
}

/// Compute all the per-frame viz state in one pass.
///
/// Two ranges are computed:
/// - `grid_range_secs` (Stems view): auto-extended to fit feedback echoes,
///   with a 2-bar / 2-second minimum.
/// - `grid_range_secs_taps_only` (Grid view): sized for direct taps only
///   so they fill the grid canvas without cramping from feedback extension.
fn compute_viz_data(params: &PetalParams, tempo_bpm: f32) -> VizData {
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let pos_l = compute_positions(params.spacing_mode.value(),
                                   params.shape_x_l.value(), params.shape_y_l.value(), n_active);
    let pos_r = if params.shape_linked.value() {
        pos_l
    } else {
        compute_positions(params.spacing_mode.value(),
                          params.shape_x_r.value(), params.shape_y_r.value(), n_active)
    };

    let time_l_norm = params.base_time_l.value();
    let time_r_norm = if params.time_linked.value() { time_l_norm }
                      else { params.base_time_r.value() };
    let base_secs_l = params.base_time_seconds(time_l_norm, tempo_bpm);
    let base_secs_r = params.base_time_seconds(time_r_norm, tempo_bpm);

    let span_l = NUM_TAPS as f32 * base_secs_l;
    let span_r = NUM_TAPS as f32 * base_secs_r;
    let max_tap_secs = (pos_l[n_active - 1] * span_l)
        .max(pos_r[n_active - 1] * span_r)
        .max(0.001);

    // Feedback cycle length.
    // Mirror the DSP exactly (lib.rs::process):
    //  - LOCKED: cluster span = max(last-tap delay)
    //  - UNLOCKED: param value, but floored at cluster span (feedback can only
    //    sit AFTER the last direct tap, never between them).
    // Without this mirror, the viz drew echoes at the wrong x-position whenever
    // the DSP clamped — which is exactly why "1/1 lands after tap 3, but every
    // dotted division magically aligns to the last tap" looked broken.
    let feedback = params.feedback.value();
    let cluster_span_secs = max_tap_secs;
    let fb_cycle_secs = if params.feedback_time_locked.value() {
        cluster_span_secs
    } else {
        params.feedback_time_seconds(tempo_bpm).max(cluster_span_secs)
    }
    .max(1e-6);
    let fb_cycle_secs_l = fb_cycle_secs;
    let fb_cycle_secs_r = fb_cycle_secs;

    // Decide how many echo cycles to draw. Below the threshold = silent
    // enough to skip; at 100% feedback we'd never reach the threshold so cap.
    let n_echo_cycles = if feedback < 1e-3 || fb_cycle_secs < 1e-6 {
        0
    } else if feedback >= 0.999 {
        MAX_ECHO_CYCLES
    } else {
        // gain^k = threshold  →  k = log(threshold) / log(gain)
        let k = (ECHO_THRESHOLD.ln() / feedback.ln()).ceil() as usize;
        k.min(MAX_ECHO_CYCLES)
    };

    let sync_mode = params.sync_mode.value();

    // --- Stems view range: 2× tap span minimum + a READABLE number of repeats ---
    // The 2× base keeps the direct taps in the left half. The range used to fit
    // the entire tail to −60 dB (n_echo_cycles × cycle, up to 64 cycles) — at
    // 60% feedback that's 14 cycles, which squeezed the cluster + early repeats
    // into a sliver on the left and filled the rest with a sub-pixel hairline
    // forest. Cap the *displayed* window at a handful of repeats (like Cluster
    // Delay); the longer tail simply runs off-screen to the right.
    const SHOWN_ECHO_CYCLES: usize = 4;
    let shown_cycles = n_echo_cycles.min(SHOWN_ECHO_CYCLES);
    let needed_secs_stems = max_tap_secs * 2.0 + shown_cycles as f32 * fb_cycle_secs;
    let grid_range_secs = pick_nice_range(needed_secs_stems, sync_mode, tempo_bpm);

    // --- Grid view range: tight 10 % right margin so you can edit taps with
    // pixel precision.  Stems view keeps the 2× rule (for feedback room);
    // Grid is for editing and should not waste space.
    let needed_secs_grid = max_tap_secs * 1.10;
    let grid_range_secs_taps_only = pick_nice_range(needed_secs_grid, sync_mode, tempo_bpm);

    let div_beats = match sync_mode {
        SyncMode::Sync => snap_to_div(params.base_time_l.value()).0,
        SyncMode::Free => 0.0,
    };

    // Normalise tap positions into [0,1] for both ranges.
    let mut times_l = [0.0f32; NUM_TAPS];
    let mut times_r = [0.0f32; NUM_TAPS];
    let mut times_l_grid = [0.0f32; NUM_TAPS];
    let mut times_r_grid = [0.0f32; NUM_TAPS];
    for i in 0..NUM_TAPS {
        let offset = params.taps[i].time_offset.value();
        times_l[i]      = ((pos_l[i] * span_l / grid_range_secs)            + offset).clamp(0.0, 1.0);
        times_r[i]      = ((pos_r[i] * span_r / grid_range_secs)            + offset).clamp(0.0, 1.0);
        times_l_grid[i] = ((pos_l[i] * span_l / grid_range_secs_taps_only)  + offset).clamp(0.0, 1.0);
        times_r_grid[i] = ((pos_r[i] * span_r / grid_range_secs_taps_only)  + offset).clamp(0.0, 1.0);
    }

    let ping_pong = matches!(
        params.crossfeed_mode.value(),
        crate::params::CrossfeedMode::PingPong,
    );

    VizData {
        times_l,
        times_r,
        grid_range_secs,
        times_l_grid,
        times_r_grid,
        grid_range_secs_taps_only,
        div_beats,
        fb_cycle_norm_l: fb_cycle_secs_l / grid_range_secs,
        fb_cycle_norm_r: fb_cycle_secs_r / grid_range_secs,
        n_echo_cycles,
        feedback,
        ping_pong,
    }
}

// ─── CENTER ──────────────────────────────────────────────────────────────
fn draw_center(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
               st: &mut EditorState, tempo_bpm: f32, s: S) {
    // meta — channel mode + tap count + actual visible length
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let ch_label = match params.channel_mode.value() {
        ChannelMode::Stereo  => "STEREO",
        ChannelMode::MidSide => "MID/SIDE",
        ChannelMode::Mono    => "MONO",
    };
    let viz = compute_viz_data(params, tempo_bpm);
    let len_str = match params.sync_mode.value() {
        SyncMode::Sync => {
            let beats = viz.grid_range_secs * tempo_bpm / 60.0;
            if beats == beats.floor() { format!("{:.0} / 1", beats) }
            else { format!("{:.1} beat", beats) }
        }
        SyncMode::Free => {
            if viz.grid_range_secs < 1.0 { format!("{:.0} ms", viz.grid_range_secs * 1000.0) }
            else { format!("{:.2} s", viz.grid_range_secs) }
        }
    };
    // viz well — kit `.viz`: an INSET (lighter) surface with a strong hairline
    // border, radius 13, sitting inside the card.
    let viz_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + s.f(8.0), rect.min.y + s.f(2.0)),
        Pos2::new(rect.max.x - s.f(8.0), rect.max.y - s.f(SHAPING_H) - s.f(8.0)),
    );
    {
        let p = ui.painter();
        p.rect_filled(viz_rect, s.rr(13.0), c::INSET);
        p.rect_stroke(viz_rect, s.rr(13.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
        // corner caption (kit .vc.r): the live meta readout
        let meta = format!("{} · TAPS {} · LENGTH {}", ch_label, n_active, len_str);
        p.text(Pos2::new(viz_rect.max.x - s.f(13.0), viz_rect.min.y + s.f(9.0)),
               Align2::RIGHT_TOP, &meta,
               FontId::new(s.f(9.0), FontFamily::Monospace), c::SOFT);
    }
    let shaping_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, rect.max.y - s.f(SHAPING_H)), rect.max,
    );

    let is_sync = params.sync_mode.value() == SyncMode::Sync;

    match st.viz_mode {
        VizMode::Stems => draw_stems(ui, viz_rect, params, &viz, tempo_bpm, is_sync, s),
        VizMode::Grid  => draw_grid_viz(ui, viz_rect, params, setter, st, &viz, tempo_bpm, is_sync, s),
    }
    draw_shaping_strip(ui, shaping_rect, params, setter, n_active, s);
}

// ─── Dynamic grid — beat divisions in Sync, ms in Free ───────────────────
// In Sync mode, div_beats is the selected base division in beats (e.g. 0.25 for 1/16).
// Labels count in those units so "1/16 tap 1" shows as "1" not "0.25".
// Pass 0.0 for Free mode (falls back to beat labels).
fn paint_timeline_grid(
    p: &egui::Painter,
    draw_l: f32, draw_r: f32, draw_t: f32, draw_b: f32,
    grid_range_secs: f32, tempo_bpm: f32, is_sync: bool,
    show_labels: bool, label_y: f32,
    _div_beats: f32,  // kept for API compatibility; beat labels are now always absolute
    s: S,
) {
    let draw_w = draw_r - draw_l;

    if is_sync {
        let beat_secs = 60.0 / tempo_bpm;
        let total_beats = (grid_range_secs / beat_secs).max(0.001);

        let (major, minor) = if total_beats <= 0.5 { (0.125_f32, 0.0625_f32) }
            else if total_beats <= 1.0 { (0.25, 0.125) }
            else if total_beats <= 2.0 { (0.5, 0.25) }
            else if total_beats <= 4.0 { (1.0, 0.5) }
            else if total_beats <= 8.0 { (2.0, 1.0) }
            else if total_beats <= 16.0 { (4.0, 2.0) }
            else if total_beats <= 32.0 { (8.0, 4.0) }
            else if total_beats <= 64.0 { (16.0, 8.0) }
            else if total_beats <= 128.0 { (32.0, 16.0) }
            else { (64.0, 32.0) };

        let mut beat = 0.0_f32;
        let eps = minor * 0.01;
        while beat <= total_beats + eps {
            let x = draw_l + (beat / total_beats) * draw_w;
            let is_major = (beat / major - (beat / major).round()).abs() < 0.02;
            let (sw, col) = if is_major { (s.f(1.0), c::LINE_SOFT) } else { (s.f(0.5), c::HAIR) };
            p.line_segment([Pos2::new(x, draw_t), Pos2::new(x, draw_b)], Stroke::new(sw, col));
            if show_labels && is_major {
                let rounded = (beat * 100.0).round() / 100.0;
                // Use absolute beat labels so that changing from 1/4 → 1/2 → 1/1
                // produces visually different label numbers, not always 0..N.
                // (The old div-count formula produced identical labels across all
                // subdivisions because both the position and the range scaled
                // proportionally with div_beats.)
                let label = if rounded == 0.0 {
                    "0".to_string()
                } else if rounded == rounded.floor() {
                    format!("{:.0}", rounded)
                } else if (rounded * 2.0).round() == rounded * 2.0 {
                    format!("{:.1}", rounded)
                } else {
                    format!("{:.2}", rounded)
                };
                p.text(Pos2::new(x, label_y), Align2::CENTER_BOTTOM, &label,
                       FontId::new(s.f(9.0), FontFamily::Monospace), c::FAINT);
            }
            beat += minor;
        }
    } else {
        // Free mode — pick ms divisions that fit nicely in the range
        let total_ms = grid_range_secs * 1000.0;
        let (major, minor) = if total_ms <= 250.0 { (50.0_f32, 25.0_f32) }
            else if total_ms <= 500.0 { (100.0, 50.0) }
            else if total_ms <= 1000.0 { (200.0, 100.0) }
            else if total_ms <= 2000.0 { (500.0, 250.0) }
            else if total_ms <= 5000.0 { (1000.0, 500.0) }
            else if total_ms <= 10000.0 { (2000.0, 1000.0) }
            else { (4000.0, 2000.0) };

        let mut ms = 0.0_f32;
        let eps = minor * 0.01;
        while ms <= total_ms + eps {
            let x = draw_l + (ms / total_ms) * draw_w;
            let is_major = (ms / major - (ms / major).round()).abs() < 0.02;
            let (sw, col) = if is_major { (s.f(1.0), c::LINE_SOFT) } else { (s.f(0.5), c::HAIR) };
            p.line_segment([Pos2::new(x, draw_t), Pos2::new(x, draw_b)], Stroke::new(sw, col));
            if show_labels && is_major {
                let label = if ms < 1.0 { "0".to_string() }
                    else if ms < 1000.0 { format!("{:.0}", ms) }
                    else { format!("{:.1}s", ms * 0.001) };
                p.text(Pos2::new(x, label_y), Align2::CENTER_BOTTOM, &label,
                       FontId::new(s.f(9.0), FontFamily::Monospace), c::FAINT);
            }
            ms += minor;
        }
    }
}

// ─── STEMS visualizer ───────────────────────────────────────────────────
fn draw_stems(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>,
              viz: &VizData, tempo_bpm: f32, is_sync: bool, s: S) {
    let p = ui.painter();
    let pad_l = s.f(36.0);
    let pad_r = s.f(24.0);
    let pad_t = s.f(18.0);
    let pad_b = s.f(24.0);
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let mid_y = (draw_t + draw_b) * 0.5;
    let max_bar = (draw_b - draw_t) * 0.5 - s.f(10.0);
    let draw_w = draw_r - draw_l;

    let ch_mode = params.channel_mode.value();

    // L/R or M/S labels — top vs bottom half of the visualizer
    let (top_lab, bot_lab) = match ch_mode {
        ChannelMode::Stereo => ("L", "R"),
        ChannelMode::MidSide => ("MID", "SIDE"),
        ChannelMode::Mono => ("M", "M"),
    };
    p.text(Pos2::new(rect.min.x + s.f(10.0), draw_t + s.f(4.0)), Align2::LEFT_TOP, top_lab,
           FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);
    p.text(Pos2::new(rect.min.x + s.f(10.0), draw_b - s.f(4.0)), Align2::LEFT_BOTTOM, bot_lab,
           FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);

    // center dashed
    dashed_h(p, Pos2::new(draw_l, mid_y), draw_w, c::WAVE, s);

    // Dynamic grid (beats in Sync, ms in Free)
    paint_timeline_grid(p, draw_l, draw_r, draw_t, draw_b,
                        viz.grid_range_secs, tempo_bpm, is_sync,
                        true, rect.max.y - s.f(6.0), viz.div_beats, s);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

    // Adaptive bar width: scale down when taps are close together so adjacent
    // stems don't merge into a blob.  Uses the smallest pixel gap between any
    // two consecutive active taps on the L channel (cycle-0 only).
    let bar_w = {
        let mut min_gap_px = draw_w; // start wide, shrink as we find close pairs
        let mut prev_x: Option<f32> = None;
        for i in 0..n_active {
            if !params.taps[i].active.value() { continue; }
            let x = draw_l + viz.times_l[i].min(1.0) * draw_w;
            if let Some(px) = prev_x {
                let g = (x - px).abs();
                if g > 0.5 { min_gap_px = min_gap_px.min(g); }
            }
            prev_x = Some(x);
        }
        // Fill bar to 55 % of the gap so there is always a thin sliver of space
        // between adjacent stems, down to a 0.5 px hairline minimum.
        (min_gap_px * 0.55).clamp(0.5, s.f(4.0).max(0.5))
    };

    // Each tap gets drawn for cycle 0 (the direct tap) and then for each
    // feedback echo cycle k = 1..=n_echo_cycles. Cycle k shifts the time
    // by k * fb_cycle_norm and scales the gain by feedback^k. The Hann-
    // crossfaded ping-pong path is approximated by swapping pan on odd
    // cycles (close enough for visualisation purposes).
    for k in 0..=viz.n_echo_cycles {
        let cycle_gain = if k == 0 { 1.0 } else { viz.feedback.powi(k as i32) };
        if cycle_gain < ECHO_THRESHOLD { break; }
        let swap_lr = viz.ping_pong && (k % 2 == 1);

        for i in 0..n_active {
            let tap = &params.taps[i];
            let active = tap.active.value();
            // Echoes inherit the tap's active-ness. Use a cube-root mapping
            // for HEIGHT so the long tail stays visible (linear gain^k drops
            // way below 1 px after a few cycles even when the audio is still
            // clearly audible), and a softer-than-linear opacity fade for
            // depth cueing.
            let height_scale = if k == 0 { 1.0 } else { cycle_gain.cbrt() };
            let opacity_scale = if k == 0 { 1.0 } else { cycle_gain.sqrt().max(0.18) };
            let op = if active { opacity_scale } else { 0.22 };
            let tap_gain_n = gain_db_to_pos(tap.gain_db.value());
            let gain_n = (tap_gain_n * height_scale).clamp(0.0, 1.0);
            let raw_pan = tap.pan.value();
            let pan = if swap_lr { -raw_pan } else { raw_pan };

            // Echo time = direct time + k cycles. Per-channel cycle length
            // because L and R can have different base times.
            let t_l = viz.times_l[i] + k as f32 * viz.fb_cycle_norm_l;
            let t_r = viz.times_r[i] + k as f32 * viz.fb_cycle_norm_r;
            // Off-screen echo: stop drawing.
            if t_l > 1.0 && t_r > 1.0 { continue; }
            // Ping-pong: also swap WHERE the echo lands so an L-panned tap
            // emits its odd-k echoes from the R-channel time slot.
            let (t_a, t_b) = if swap_lr { (t_r, t_l) } else { (t_l, t_r) };
            let x_l = draw_l + t_a.min(1.0) * draw_w;
            let x_r = draw_l + t_b.min(1.0) * draw_w;
            let in_view_l = t_a <= 1.0;
            let in_view_r = t_b <= 1.0;

            // Per-tap gradient color: darkest purple for tap 0, lightest for tap n-1.
            let base_col = tap_color(i, n_active);
            let col = base_col.linear_multiply(op);
            // Echo bars draw thinner than direct taps so the live cluster
            // stays dominant even when repeats stack up.
            let bar_w = if k == 0 { bar_w } else { (bar_w * 0.7).max(0.5) };

            match ch_mode {
                ChannelMode::Stereo => {
                    let l_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).cos();
                    let r_w = ((pan + 1.0) * std::f32::consts::PI * 0.25).sin();
                    let h_l = gain_n * l_w * max_bar;
                    let h_r = gain_n * r_w * max_bar;
                    if in_view_l && h_l > 0.5 {
                        p.line_segment([Pos2::new(x_l, mid_y), Pos2::new(x_l, mid_y - h_l)],
                                       Stroke::new(bar_w, col));
                    }
                    if in_view_r && h_r > 0.5 {
                        p.line_segment([Pos2::new(x_r, mid_y), Pos2::new(x_r, mid_y + h_r)],
                                       Stroke::new(bar_w, col));
                    }
                }
                ChannelMode::MidSide => {
                    let mid_amt  = ((1.0 - pan) * 0.5).clamp(0.0, 1.0);
                    let side_amt = ((1.0 + pan) * 0.5).clamp(0.0, 1.0);
                    let mid_h  = gain_n * max_bar * 0.5 * mid_amt;
                    let side_h = gain_n * max_bar * 0.35 * side_amt;
                    let half_w = (bar_w * 0.5).max(0.5);
                    if in_view_l && mid_h > 0.5 {
                        let mr = Rect::from_min_max(
                            Pos2::new(x_l - half_w, mid_y - mid_h),
                            Pos2::new(x_l + half_w, mid_y + mid_h),
                        );
                        p.rect_filled(mr, s.rr(2.0), col);
                    }
                    if in_view_r && side_h > 0.5 {
                        let s_top_y = mid_y - max_bar * 0.65;
                        let s_bot_y = mid_y + max_bar * 0.65;
                        p.line_segment([Pos2::new(x_r, s_top_y - side_h),
                                        Pos2::new(x_r, s_top_y + side_h)],
                                       Stroke::new(bar_w, col));
                        p.line_segment([Pos2::new(x_r, s_bot_y - side_h),
                                        Pos2::new(x_r, s_bot_y + side_h)],
                                       Stroke::new(bar_w, col));
                    }
                }
                ChannelMode::Mono => {
                    let h = gain_n * max_bar;
                    if in_view_l && h > 0.5 {
                        p.line_segment([Pos2::new(x_l, mid_y - h * 0.5), Pos2::new(x_l, mid_y + h * 0.5)],
                                       Stroke::new(bar_w, col));
                    }
                }
            }
        }
    }
}

fn dashed_h(p: &egui::Painter, start: Pos2, width: f32, color: Color32, s: S) {
    let dash = s.f(4.0); let gap = s.f(4.0);
    let mut x = start.x;
    while x < start.x + width {
        let x2 = (x + dash).min(start.x + width);
        p.line_segment([Pos2::new(x, start.y), Pos2::new(x2, start.y)], Stroke::new(s.f(0.8), color));
        x += dash + gap;
    }
}
fn dashed_v(p: &egui::Painter, start: Pos2, height: f32, color: Color32, s: S) {
    let dash = s.f(3.0); let gap = s.f(4.0);
    let mut y = start.y;
    while y < start.y + height {
        let y2 = (y + dash).min(start.y + height);
        p.line_segment([Pos2::new(start.x, y), Pos2::new(start.x, y2)], Stroke::new(s.f(0.4), color));
        y += dash + gap;
    }
}

// ─── GRID visualizer ────────────────────────────────────────────────────
// Only the *direct* taps are drawn here — feedback echoes would clutter the
// editable surface and make it harder to drag the right ball.
fn draw_grid_viz(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                 st: &mut EditorState, viz: &VizData, tempo_bpm: f32, is_sync: bool, s: S) {
    // Grid view uses the direct-taps-only range so the taps fill the canvas naturally
    // rather than being cramped on the left by feedback echo extension.
    let times_l = &viz.times_l_grid;
    let grid_range_secs = viz.grid_range_secs_taps_only;
    let div_beats = viz.div_beats;
    let p = ui.painter();
    let pad_l = s.f(40.0);
    let pad_r = s.f(24.0);
    let pad_t = s.f(18.0);
    let pad_b = s.f(28.0);
    let draw_l = rect.min.x + pad_l;
    let draw_r = rect.max.x - pad_r;
    let draw_t = rect.min.y + pad_t;
    let draw_b = rect.max.y - pad_b;
    let draw_w = draw_r - draw_l;
    let draw_h = draw_b - draw_t;

    // dB axis lines & labels — uses gain_db_to_pos() so positions match the grid balls exactly.
    // 0 dB is the primary reference (solid line); others are dashed for hierarchy.
    for &db in &[6.0_f32, 0.0, -6.0, -12.0, -18.0, -24.0] {
        let pos = gain_db_to_pos(db);
        let y = draw_t + (1.0 - pos) * draw_h;
        if db == 0.0 {
            p.line_segment([Pos2::new(draw_l, y), Pos2::new(draw_r, y)], Stroke::new(s.f(1.0), c::LINE));
        } else {
            dashed_h(p, Pos2::new(draw_l, y), draw_w, c::HAIR, s);
        }
        let label = if db > 0.0 { format!("+{:.0}", db) }
                    else if db == 0.0 { "0".to_string() }
                    else { format!("{:.0}", db) };
        p.text(Pos2::new(rect.min.x + pad_l - s.f(6.0), y), Align2::RIGHT_CENTER, &label,
               FontId::new(s.f(9.0), FontFamily::Monospace), c::FAINT);
    }
    p.text(Pos2::new(rect.min.x + pad_l - s.f(6.0), draw_t - s.f(6.0)), Align2::RIGHT_BOTTOM, "dB",
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);

    paint_timeline_grid(p, draw_l, draw_r, draw_t, draw_b,
                        grid_range_secs, tempo_bpm, is_sync, true, rect.max.y - s.f(6.0), div_beats, s);

    p.text(Pos2::new(draw_l + s.f(8.0), draw_t + s.f(6.0)), Align2::LEFT_TOP,
           "DRAG · X = TIME · Y = GAIN",
           FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);

    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let snap = params.tap_snap.value();
    let active_drag = st.drag_tap;

    for i in 0..n_active {
        let tap = &params.taps[i];
        let active = tap.active.value();
        let op = if active { 1.0 } else { 0.25 };
        let x = draw_l + times_l[i] * draw_w;
        let gain_n = gain_db_to_pos(tap.gain_db.value());
        let y = draw_t + (1.0 - gain_n) * draw_h;
        let r = s.f(9.0);

        let baseline_y = draw_b;
        let tap_col = tap_color(i, n_active);
        let p = ui.painter();
        p.line_segment([Pos2::new(x, baseline_y), Pos2::new(x, y)],
                       Stroke::new(s.f(3.0), tap_col.linear_multiply(if active { 0.45 } else { 0.1 })));

        let ball_rect = Rect::from_center_size(Pos2::new(x, y), Vec2::splat(r * 2.5));
        let resp = ui.allocate_rect(ball_rect, Sense::click_and_drag());

        if resp.drag_started() {
            st.drag_tap = Some(i);
            st.drag_kind = DragKind::GridBall;
            setter.begin_set_parameter(&tap.gain_db);
            setter.begin_set_parameter(&tap.time_offset);
        }
        if resp.dragged() && active_drag == Some(i) {
            // Vertical → gain (uses gain_db_to_pos/pos_to_gain_db so grid lines stay in sync)
            let dy = -resp.drag_delta().y / draw_h;
            let new_gain_n = (gain_n + dy).clamp(0.0, 1.0);
            setter.set_parameter(&tap.gain_db, pos_to_gain_db(new_gain_n));
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
        let col = tap_col.linear_multiply(op);
        let glow = tap_col.linear_multiply(op * (if active_drag == Some(i) { 0.30 } else { 0.12 }));
        p.circle_filled(Pos2::new(x, y), r + s.f(6.0), glow);
        p.circle_filled(Pos2::new(x, y), r, col);
        p.circle_stroke(Pos2::new(x, y), r, Stroke::new(s.f(1.0), c::with_alpha(c::INK, 64)));

        let label = format!("{}", i + 1);
        p.text(Pos2::new(x, y), Align2::CENTER_CENTER, &label,
               FontId::new(s.f(9.0), FontFamily::Monospace), c::PAPER);
    }

    // snap indicator at bottom right
    let snap_text = if snap { "SNAP 1/16  (shift: free)" } else { "FREE  (drag for snap toggle in params)" };
    ui.painter().text(Pos2::new(draw_r, draw_t + s.f(6.0)), Align2::RIGHT_TOP, snap_text,
                       FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);
}

// ─── SHAPING strip ──────────────────────────────────────────────────────
fn draw_shaping_strip(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, n_active: usize, s: S) {
    // Sits inside the center card (PANEL); just a divider above.
    let p = ui.painter();
    p.line_segment([Pos2::new(rect.min.x, rect.min.y), Pos2::new(rect.max.x, rect.min.y)],
                   Stroke::new(s.f(1.0), c::LINE_SOFT));

    let right_w = s.f(56.0);
    let pad_inner = s.f(8.0);
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
    draw_xy_pad(ui, l_rect, "L", &params.shape_x_l, &params.shape_y_l, setter, false, n_active, s);
    let (xr, yr): (&FloatParam, &FloatParam) =
        if linked { (&params.shape_x_l, &params.shape_y_l) }
        else { (&params.shape_x_r, &params.shape_y_r) };
    draw_xy_pad(ui, r_rect, "R", xr, yr, setter, linked, n_active, s);

    let sr_rect = Rect::from_min_max(
        Pos2::new(rect.max.x - right_w, rect.min.y), rect.max,
    );
    {
        let p = ui.painter();
        p.line_segment([Pos2::new(sr_rect.min.x, sr_rect.min.y), Pos2::new(sr_rect.min.x, sr_rect.max.y)],
                       Stroke::new(s.f(1.0), c::LINE_SOFT));
        p.text(Pos2::new(sr_rect.center().x, sr_rect.min.y + s.f(12.0)),
               Align2::CENTER_CENTER, "SHAPE",
               FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    }
    let link_center = Pos2::new(sr_rect.center().x, sr_rect.center().y);
    let link_rect = Rect::from_center_size(link_center, Vec2::splat(s.f(30.0)));
    let resp = ui.allocate_rect(link_rect, Sense::click());
    if resp.clicked() {
        setter.begin_set_parameter(&params.shape_linked);
        setter.set_parameter(&params.shape_linked, !linked);
        setter.end_set_parameter(&params.shape_linked);
    }
    let p = ui.painter();
    // Kit `.btn.on`: accent fill + white glyph + soft glow when linked.
    let (fill, border, ink) = if linked {
        (c::ACCENT, c::ACCENT, Color32::WHITE)
    } else {
        (c::INSET, c::LINE, c::SOFT)
    };
    p.circle_filled(link_center, s.f(15.0), fill);
    p.circle_stroke(link_center, s.f(15.0), Stroke::new(s.f(1.0), border));
    if linked {
        p.circle_stroke(link_center, s.f(16.5), Stroke::new(s.f(1.0), c::glow(c::ACCENT)));
    }
    p.text(link_center, Align2::CENTER_CENTER, "🔗",
           FontId::new(s.f(13.0), FontFamily::Proportional), ink);
    p.text(Pos2::new(sr_rect.center().x, sr_rect.max.y - s.f(10.0)),
           Align2::CENTER_BOTTOM,
           if linked { "LINKED" } else { "UNLINK" },
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
}

fn draw_xy_pad(ui: &mut Ui, rect: Rect, label: &str,
               x_param: &FloatParam, y_param: &FloatParam,
               setter: &ParamSetter, dim: bool, n_active: usize, s: S) {
    let alpha = if dim { 0.4 } else { 1.0 };
    let resp = if dim { ui.allocate_rect(rect, Sense::hover()) }
               else { ui.allocate_rect(rect, Sense::click_and_drag()) };

    let p = ui.painter();
    p.rect_filled(rect, s.rr(8.0), c::INSET);
    p.rect_stroke(rect, s.rr(8.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
    for i in 1..=2 {
        let x = rect.min.x + i as f32 * rect.width() / 3.0;
        dashed_v(p, Pos2::new(x, rect.min.y + s.f(4.0)), rect.height() - s.f(8.0), c::LINE, s);
    }
    for i in 1..=2 {
        let y = rect.min.y + i as f32 * rect.height() / 3.0;
        dashed_h(p, Pos2::new(rect.min.x + s.f(4.0), y), rect.width() - s.f(8.0), c::LINE, s);
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
    // Double-click anywhere on the pad → reset both axes to default (typically 0)
    if resp.double_clicked() && !dim {
        setter.begin_set_parameter(x_param);
        setter.set_parameter_normalized(x_param, x_param.default_normalized_value());
        setter.end_set_parameter(x_param);
        setter.begin_set_parameter(y_param);
        setter.set_parameter_normalized(y_param, y_param.default_normalized_value());
        setter.end_set_parameter(y_param);
    }

    let cx = rect.min.x + x_norm * rect.width();
    let cy = rect.min.y + (1.0 - y_norm) * rect.height();
    let accent = c::ACCENT.linear_multiply(alpha);
    p.line_segment([Pos2::new(rect.min.x, cy), Pos2::new(rect.max.x, cy)],
                   Stroke::new(s.f(0.6), accent.linear_multiply(0.4)));
    p.line_segment([Pos2::new(cx, rect.min.y), Pos2::new(cx, rect.max.y)],
                   Stroke::new(s.f(0.6), accent.linear_multiply(0.4)));
    p.circle_filled(Pos2::new(cx, cy), s.f(10.0), accent.linear_multiply(0.25));
    p.circle_filled(Pos2::new(cx, cy), s.f(5.0), accent);

    p.text(Pos2::new(rect.min.x + s.f(8.0), rect.min.y + s.f(6.0)), Align2::LEFT_TOP, label,
           FontId::new(s.f(9.0), FontFamily::Proportional), c::FAINT);
    p.text(Pos2::new(rect.min.x + s.f(8.0), rect.max.y - s.f(4.0)), Align2::LEFT_BOTTOM, "← tap 1",
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    p.text(Pos2::new(rect.max.x - s.f(8.0), rect.max.y - s.f(4.0)), Align2::RIGHT_BOTTOM,
           &format!("tap {} →", n_active),
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
}

// ─── RIGHT PANEL ────────────────────────────────────────────────────────
// The PAN/GAIN/… tab row lives on the card's top border (card_chip_tabs in
// draw_plugin); this draws only the body + footer.
fn draw_right(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState, tempo_bpm: f32, s: S) {
    let footer_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, rect.max.y - s.f(RIGHT_FOOTER_H)), rect.max,
    );
    let body_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(rect.max.x, footer_rect.min.y),
    );
    ui.painter().line_segment(
        [Pos2::new(rect.min.x, footer_rect.min.y), Pos2::new(rect.max.x, footer_rect.min.y)],
        Stroke::new(s.f(1.0), c::LINE_SOFT),
    );

    draw_right_body(ui, body_rect, params, setter, st, s);
    draw_right_footer(ui, footer_rect, params, setter, st, tempo_bpm, s);
}

fn draw_right_body(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState, s: S) {
    let pad = s.f(10.0);
    let row_h = s.f(26.0);
    let row_gap = s.f(4.0);
    let n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

    // Scroll handling — when n_active rows overflow the body height
    let body_inner_h = rect.height() - 2.0 * pad;
    let total_h = n_active as f32 * (row_h + row_gap);
    let max_scroll = (total_h - body_inner_h).max(0.0);
    // Capture mouse wheel while pointer is over the body rect
    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    let scroll_dy = ui.input(|i| i.raw_scroll_delta.y);
    if scroll_dy != 0.0 {
        if let Some(pos) = hover_pos {
            if rect.contains(pos) {
                st.right_panel_scroll = (st.right_panel_scroll - scroll_dy).clamp(0.0, max_scroll);
            }
        }
    }
    // Clamp existing scroll in case n_active shrank
    st.right_panel_scroll = st.right_panel_scroll.clamp(0.0, max_scroll);
    let scroll = st.right_panel_scroll;

    // Clip drawing to body rect so overflowing rows don't bleed into footer
    let prev_clip = ui.clip_rect();
    ui.set_clip_rect(rect);

    let row_x = rect.min.x + pad;
    let row_w = rect.width() - pad * 2.0;
    let row_y = |i: usize| rect.min.y + pad + i as f32 * (row_h + row_gap) - scroll;

    match st.right_tab {
        RightTab::Pan => {
            let ms_mode = params.channel_mode.value() == ChannelMode::MidSide;
            for i in 0..n_active {
                let y = row_y(i);
                if y + row_h < rect.min.y || y > rect.max.y { continue; }
                let r = Rect::from_min_size(Pos2::new(row_x, y), Vec2::new(row_w, row_h));
                if ms_mode {
                    tap_row_slider(ui, r, i, n_active, &params.taps[i].active, &params.taps[i].pan, setter,
                                   -1.0, 1.0, true, 1.0, s, |v| {
                        if v.abs() < 0.05 { "BAL".to_string() }
                        else if v < 0.0 { format!("M{}", ((-v) * 100.0).round() as i32) }
                        else { format!("S{}", (v * 100.0).round() as i32) }
                    });
                } else {
                    tap_row_slider(ui, r, i, n_active, &params.taps[i].active, &params.taps[i].pan, setter,
                                   -1.0, 1.0, true, 1.0, s, |v| {
                        if v.abs() < 0.05 { "C".to_string() }
                        else if v > 0.0 { format!("R{}", (v * 100.0).round() as i32) }
                        else { format!("L{}", (-v * 100.0).round() as i32) }
                    });
                }
            }
        }
        RightTab::Gain => {
            for i in 0..n_active {
                let y = row_y(i);
                if y + row_h < rect.min.y || y > rect.max.y { continue; }
                let r = Rect::from_min_size(Pos2::new(row_x, y), Vec2::new(row_w, row_h));
                // Audio-taper mapping (GAIN_SKEW ≈ 0.333 = cube-root) — 75% position = 0 dB,
                // matching DAW fader feel. Same curve used in the grid visualizer.
                tap_row_slider(ui, r, i, n_active, &params.taps[i].active, &params.taps[i].gain_db, setter,
                               -60.0, 6.0, false, GAIN_SKEW, s, |v| {
                    if v < -50.0 { "-∞ dB".to_string() } else { format!("{:+.0} dB", v) }
                });
            }
        }
        RightTab::Filter => {
            for i in 0..n_active {
                let y = row_y(i);
                if y + row_h < rect.min.y || y > rect.max.y { continue; }
                let r = Rect::from_min_size(Pos2::new(row_x, y), Vec2::new(row_w, row_h));
                tap_row_filter(ui, r, i, n_active, &params.taps[i], setter, s);
            }
        }
        RightTab::XFeed   => draw_xfeed_panel(ui, rect, params, setter, s),
        RightTab::Diffuse => draw_diffuse_panel(ui, rect, params, setter, s),
    }

    // Restore previous clip rect — so the footer and tabs paint normally
    ui.set_clip_rect(prev_clip);

    // Slim scrollbar indicator on the right edge when content overflows
    if max_scroll > 0.5 {
        let track_x = rect.max.x - s.f(4.0);
        let track_top = rect.min.y + pad;
        let track_bot = rect.max.y - pad;
        let track_h = (track_bot - track_top).max(1.0);
        let p = ui.painter();
        p.line_segment(
            [Pos2::new(track_x, track_top), Pos2::new(track_x, track_bot)],
            Stroke::new(s.f(1.0), c::LINE_SOFT),
        );
        let visible_frac = (body_inner_h / total_h).clamp(0.05, 1.0);
        let thumb_h = (track_h * visible_frac).max(s.f(12.0));
        let scroll_frac = if max_scroll > 0.0 { scroll / max_scroll } else { 0.0 };
        let thumb_top = track_top + (track_h - thumb_h) * scroll_frac;
        let thumb_rect = Rect::from_min_size(
            Pos2::new(track_x - s.f(1.5), thumb_top),
            Vec2::new(s.f(3.0), thumb_h),
        );
        p.rect_filled(thumb_rect, s.rr(2.0), c::SOFT);
    }
}

/// One per-tap filter row in the FILTER tab.
fn tap_row_filter(ui: &mut Ui, rect: Rect, idx: usize, n_active: usize, tap: &crate::params::TapParams, setter: &ParamSetter, s: S) {
    let active = tap.active.value();
    let alpha = if active { 1.0 } else { 0.4 };
    let accent = tap_color(idx, n_active);

    // Row background + tap number
    {
        let p = ui.painter();
        p.rect_filled(rect, s.rr(5.0), c::INSET.linear_multiply(alpha));
        p.rect_stroke(rect, s.rr(5.0), Stroke::new(s.f(1.0), c::LINE_SOFT), StrokeKind::Inside);
        p.text(Pos2::new(rect.min.x + s.f(8.0), rect.center().y), Align2::LEFT_CENTER,
               &format!("{:02}", idx + 1),
               FontId::new(s.f(10.0), FontFamily::Proportional), c::FAINT);
    }

    // Clickable active-toggle dot
    let dot_center = Pos2::new(rect.min.x + s.f(30.0), rect.center().y);
    let dot_rect = Rect::from_center_size(dot_center, Vec2::splat(s.f(14.0)));
    let dot_resp = ui.allocate_rect(dot_rect, Sense::click());
    if dot_resp.clicked() {
        setter.begin_set_parameter(&tap.active);
        setter.set_parameter(&tap.active, !active);
        setter.end_set_parameter(&tap.active);
    }
    {
        let p = ui.painter();
        if active { p.circle_filled(dot_center, s.f(4.0), accent); }
        else { p.circle_stroke(dot_center, s.f(4.0), Stroke::new(s.f(1.0), c::LINE)); }
    }

    // Type cycling button: LP → BP → HP → LP …
    let cur_type = tap.filter_type.value();
    let type_label = match cur_type {
        FilterType::LowPass  => "LP",
        FilterType::BandPass => "BP",
        FilterType::HighPass => "HP",
    };
    let next_type = match cur_type {
        FilterType::LowPass  => FilterType::BandPass,
        FilterType::BandPass => FilterType::HighPass,
        FilterType::HighPass => FilterType::LowPass,
    };
    let type_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + s.f(46.0), rect.center().y - s.f(9.0)),
        Vec2::new(s.f(32.0), s.f(18.0)),
    );
    let type_resp = ui.allocate_rect(type_rect, Sense::click());
    if type_resp.clicked() {
        setter.begin_set_parameter(&tap.filter_type);
        setter.set_parameter(&tap.filter_type, next_type);
        setter.end_set_parameter(&tap.filter_type);
    }
    {
        let p = ui.painter();
        p.rect_filled(type_rect, s.rr(6.0), accent.linear_multiply(alpha));
        p.rect_stroke(type_rect, s.rr(6.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
        p.text(type_rect.center(), Align2::CENTER_CENTER, type_label,
               FontId::new(s.f(9.0), FontFamily::Proportional), c::PAPER);
    }

    // Frequency knob + value
    let knob_s = s.f(22.0);
    let freq_cx = rect.min.x + s.f(92.0);
    knob_at(ui, Pos2::new(freq_cx, rect.center().y), knob_s, &tap.filter_cutoff, setter, s);
    let cutoff = tap.filter_cutoff.value();
    let cutoff_str = if cutoff >= 1000.0 { format!("{:.1}k", cutoff / 1000.0) }
                     else { format!("{:.0}", cutoff) };
    {
        let p = ui.painter();
        p.text(Pos2::new(freq_cx + s.f(14.0), rect.center().y), Align2::LEFT_CENTER,
               &cutoff_str,
               FontId::new(s.f(9.0), FontFamily::Monospace), c::INK);
    }

    // Q knob + value
    let q_cx = rect.min.x + s.f(168.0);
    knob_at(ui, Pos2::new(q_cx, rect.center().y), knob_s, &tap.filter_q, setter, s);
    {
        let p = ui.painter();
        p.text(Pos2::new(q_cx + s.f(14.0), rect.center().y), Align2::LEFT_CENTER,
               &format!("Q {:.1}", tap.filter_q.value()),
               FontId::new(s.f(9.0), FontFamily::Monospace), c::INK);
    }

    // Slope cycling button: 12 → 24 → 48 → 12 …
    use crate::params::FilterSlope;
    let cur_slope = tap.filter_slope.value();
    let (slope_label, next_slope) = match cur_slope {
        FilterSlope::Db12 => ("12",  FilterSlope::Db24),
        FilterSlope::Db24 => ("24",  FilterSlope::Db48),
        FilterSlope::Db48 => ("48",  FilterSlope::Db12),
    };
    let slope_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - s.f(38.0), rect.center().y - s.f(9.0)),
        Vec2::new(s.f(30.0), s.f(18.0)),
    );
    let slope_resp = ui.allocate_rect(slope_rect, Sense::click());
    if slope_resp.clicked() {
        setter.begin_set_parameter(&tap.filter_slope);
        setter.set_parameter(&tap.filter_slope, next_slope);
        setter.end_set_parameter(&tap.filter_slope);
    }
    {
        let p = ui.painter();
        p.rect_filled(slope_rect, s.rr(6.0), c::INSET.linear_multiply(alpha));
        p.rect_stroke(slope_rect, s.rr(6.0), Stroke::new(s.f(1.0), c::LINE), StrokeKind::Inside);
        p.text(slope_rect.center(), Align2::CENTER_CENTER, slope_label,
               FontId::new(s.f(9.0), FontFamily::Monospace), c::INK2);
    }
}


fn draw_diffuse_panel(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, s: S) {
    use crate::params::DiffuseRouting;
    let pad = s.f(14.0);

    let routing = params.diffuse_routing.value();
    let (title, desc) = match routing {
        DiffuseRouting::Input    => ("DIFFUSE \u{00B7} INPUT",
            "Schroeder allpass chain on the input. Every tap reads pre-smeared material \u{2014} a layered, washy character."),
        DiffuseRouting::Feedback => ("DIFFUSE \u{00B7} FEEDBACK",
            "Smear lives in the feedback loop. Each repeat is re-diffused, so the tail grows into a reverb (can spiral fast)."),
        DiffuseRouting::Output   => ("DIFFUSE \u{00B7} OUTPUT",
            "Cleanest option. Smears the tap-mix on the way to dry/wet \u{2014} a reverb bloom that doesn't touch the delay dynamics."),
    };

    // Title + wrapped description
    {
        let p = ui.painter();
        p.text(Pos2::new(rect.min.x + pad, rect.min.y + s.f(14.0)), Align2::LEFT_TOP,
               title, FontId::new(s.f(10.0), FontFamily::Proportional), c::FAINT);
    }
    let max_w = rect.width() - pad * 2.0;
    let font = FontId::new(s.f(9.0), FontFamily::Proportional);
    let mut y = rect.min.y + s.f(30.0);
    let words: Vec<&str> = desc.split_whitespace().collect();
    let mut line = String::new();
    for w in words {
        let trial = if line.is_empty() { w.to_string() } else { format!("{} {}", line, w) };
        let tw = ui.fonts(|f| f.layout_no_wrap(trial.clone(), font.clone(), c::SOFT)).rect.width();
        if tw > max_w && !line.is_empty() {
            ui.painter().text(Pos2::new(rect.min.x + pad, y), Align2::LEFT_TOP, &line, font.clone(), c::SOFT);
            y += s.f(13.0);
            line = w.to_string();
        } else { line = trial; }
    }
    if !line.is_empty() {
        ui.painter().text(Pos2::new(rect.min.x + pad, y), Align2::LEFT_TOP, &line, font.clone(), c::SOFT);
        y += s.f(13.0);
    }

    // Routing segmented selector (Input / Feedback / Output)
    let routings = [DiffuseRouting::Input, DiffuseRouting::Feedback, DiffuseRouting::Output];
    let labels = ["INPUT", "FB", "OUT"];
    let n = routings.len();
    let total_w = rect.width() - pad * 2.0;
    let btn_w = (total_w - s.f(2.0) * (n as f32 - 1.0)) / n as f32;
    let row_y = y + s.f(8.0);
    for (i, (r_mode, lbl)) in routings.iter().zip(labels.iter()).enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.min.x + pad + i as f32 * (btn_w + s.f(2.0)), row_y),
            Vec2::new(btn_w, s.f(22.0)),
        );
        if seg_button(ui, r, lbl, routing == *r_mode, s) {
            setter.begin_set_parameter(&params.diffuse_routing);
            setter.set_parameter(&params.diffuse_routing, *r_mode);
            setter.end_set_parameter(&params.diffuse_routing);
        }
    }

    // AMOUNT + SIZE knobs, side by side
    let knob_s = s.f(52.0);
    let knob_y = row_y + s.f(22.0 + 18.0) + knob_s * 0.5;
    let cx_amt  = rect.min.x + rect.width() * 0.33;
    let cx_size = rect.min.x + rect.width() * 0.67;
    knob_at(ui, Pos2::new(cx_amt,  knob_y), knob_s, &params.diffuse_amount, setter, s);
    knob_at(ui, Pos2::new(cx_size, knob_y), knob_s, &params.diffuse_size, setter, s);
    let v_amt  = params.diffuse_amount.value();
    let v_size = params.diffuse_size.value();
    let p = ui.painter();
    p.text(Pos2::new(cx_amt,  knob_y + knob_s * 0.5 + s.f(6.0)), Align2::CENTER_TOP,
           "AMOUNT", FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    let amt_str = if v_amt < 0.01 { "OFF".to_string() } else { format!("{:.0}%", v_amt * 100.0) };
    p.text(Pos2::new(cx_amt,  knob_y + knob_s * 0.5 + s.f(18.0)), Align2::CENTER_TOP,
           &amt_str, FontId::new(s.f(11.0), FontFamily::Monospace), c::INK);
    p.text(Pos2::new(cx_size, knob_y + knob_s * 0.5 + s.f(6.0)), Align2::CENTER_TOP,
           "SIZE", FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    p.text(Pos2::new(cx_size, knob_y + knob_s * 0.5 + s.f(18.0)), Align2::CENTER_TOP,
           &format!("{:.2}\u{00D7}", v_size),
           FontId::new(s.f(11.0), FontFamily::Monospace), c::INK);

    // Helper text — explains size's effect on tone (the main "why is it muffled?" knob)
    p.text(Pos2::new(rect.center().x, rect.max.y - s.f(22.0)), Align2::CENTER_BOTTOM,
           "8\u{00D7} Schroeder allpass",
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    p.text(Pos2::new(rect.center().x, rect.max.y - s.f(10.0)), Align2::CENTER_BOTTOM,
           "smaller SIZE = tighter & brighter",
           FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
}

fn draw_xfeed_panel(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter, s: S) {
    use crate::params::CrossfeedMode;
    let pad = s.f(14.0);

    // Mode + description
    let mode = params.crossfeed_mode.value();
    let (title, desc) = match mode {
        CrossfeedMode::Mix      => ("CROSSFEED · MIX",      "Post-tap stereo blend between channels."),
        CrossfeedMode::PingPong => ("CROSSFEED · PING-PONG","Feedback routes L\u{2194}R, echoes bounce side to side."),
        CrossfeedMode::Diffuse  => ("CROSSFEED · DIFFUSE",  "Feedback runs through an allpass cascade (ChowMatrix-style) \u{2014} smears echoes into a reverb tail."),
    };
    {
        let p = ui.painter();
        p.text(Pos2::new(rect.min.x + pad, rect.min.y + s.f(14.0)), Align2::LEFT_TOP,
               title,
               FontId::new(s.f(10.0), FontFamily::Proportional), c::FAINT);
        p.text(Pos2::new(rect.min.x + pad, rect.min.y + s.f(30.0)), Align2::LEFT_TOP,
               desc,
               FontId::new(s.f(9.0), FontFamily::Proportional), c::SOFT);
    }

    // Mode segmented selector
    let modes = [CrossfeedMode::Mix, CrossfeedMode::PingPong, CrossfeedMode::Diffuse];
    let labels = ["MIX", "PING", "DIFF"];
    let n = modes.len();
    let total_w = rect.width() - pad * 2.0;
    let btn_w = (total_w - s.f(2.0) * (n as f32 - 1.0)) / n as f32;
    let y = rect.min.y + s.f(54.0);
    for (i, (m, label)) in modes.iter().zip(labels.iter()).enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.min.x + pad + i as f32 * (btn_w + s.f(2.0)), y),
            Vec2::new(btn_w, s.f(24.0)),
        );
        if seg_button(ui, r, label, mode == *m, s) {
            setter.begin_set_parameter(&params.crossfeed_mode);
            setter.set_parameter(&params.crossfeed_mode, *m);
            setter.end_set_parameter(&params.crossfeed_mode);
        }
    }

    // Amount knob
    let knob_s = s.f(64.0);
    let center = Pos2::new(rect.center().x, rect.min.y + s.f(130.0));
    knob_at(ui, center, knob_s, &params.crossfeed, setter, s);
    {
        let p = ui.painter();
        let v = params.crossfeed.value();
        let label_top = match mode {
            CrossfeedMode::Mix      => "BLEND",
            CrossfeedMode::PingPong => "BOUNCE",
            CrossfeedMode::Diffuse  => "DEPTH",
        };
        let label_val = if v < 0.01 { "OFF".to_string() }
                        else { format!("{:.0}%", v * 100.0) };
        p.text(Pos2::new(center.x, center.y + knob_s * 0.5 + s.f(8.0)), Align2::CENTER_TOP, label_top,
               FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
        p.text(Pos2::new(center.x, center.y + knob_s * 0.5 + s.f(22.0)), Align2::CENTER_TOP, &label_val,
               FontId::new(s.f(12.0), FontFamily::Monospace), c::INK);
    }
}

/// `skew`:
/// - `1.0`  → linear mapping (default)
/// - `> 1`  → audio-taper: more resolution near the top of the range
///   (e.g. 2.0 for gain so the right half covers ~0…−13 dB and only the
///   bottom of the slider reaches −60 dB).
fn tap_row_slider<F: Fn(f32) -> String>(
    ui: &mut Ui, rect: Rect, idx: usize, n_active: usize,
    active_param: &BoolParam, value_param: &FloatParam,
    setter: &ParamSetter, min: f32, max: f32, bipolar: bool, skew: f32,
    s: S,
    formatter: F,
) {
    let active = active_param.value();
    let alpha = if active { 1.0 } else { 0.4 };
    let accent = tap_color(idx, n_active);

    let val_to_pos = |v: f32| -> f32 {
        let raw = ((v - min) / (max - min)).clamp(0.0, 1.0);
        if (skew - 1.0).abs() < 1e-4 { raw } else { raw.powf(1.0 / skew) }
    };
    let pos_to_val = |n: f32| -> f32 {
        let raw = if (skew - 1.0).abs() < 1e-4 { n } else { n.powf(skew) };
        min + raw * (max - min)
    };

    {
        let p = ui.painter();
        p.rect_filled(rect, s.rr(5.0), c::INSET.linear_multiply(alpha));
        p.rect_stroke(rect, s.rr(5.0), Stroke::new(s.f(1.0), c::LINE_SOFT), StrokeKind::Inside);
        p.text(Pos2::new(rect.min.x + s.f(8.0), rect.center().y), Align2::LEFT_CENTER,
               &format!("{:02}", idx + 1),
               FontId::new(s.f(10.0), FontFamily::Proportional), c::FAINT);
    }

    let dot_center = Pos2::new(rect.min.x + s.f(32.0), rect.center().y);
    let dot_rect = Rect::from_center_size(dot_center, Vec2::splat(s.f(14.0)));
    let dot_resp = ui.allocate_rect(dot_rect, Sense::click());
    if dot_resp.clicked() {
        setter.begin_set_parameter(active_param);
        setter.set_parameter(active_param, !active);
        setter.end_set_parameter(active_param);
    }
    {
        let p = ui.painter();
        if active { p.circle_filled(dot_center, s.f(4.0), accent); }
        else { p.circle_stroke(dot_center, s.f(4.0), Stroke::new(s.f(1.0), c::LINE)); }
    }

    let slider_left = rect.min.x + s.f(52.0);
    let slider_right = rect.max.x - s.f(56.0);
    let slider_y = rect.center().y;
    let v = value_param.value();
    let n = val_to_pos(v);
    // Epo slider: two overlaid 3px line segments (neutral baseline + colored
    // fill, from center when bipolar) and a small 4×12 rounded-rect handle.
    // No trough well, no circle thumb.
    {
        let p = ui.painter();
        let track = Stroke::new(s.f(3.0), c::LINE);
        p.line_segment([Pos2::new(slider_left, slider_y), Pos2::new(slider_right, slider_y)], track);
        let (f0, f1) = if bipolar { (n.min(0.5), n.max(0.5)) } else { (0.0, n) };
        let x0 = slider_left + f0 * (slider_right - slider_left);
        let x1 = slider_left + f1 * (slider_right - slider_left);
        p.line_segment([Pos2::new(x0, slider_y), Pos2::new(x1, slider_y)],
                       Stroke::new(s.f(3.0), accent.linear_multiply(alpha)));
        if bipolar {
            let mid_x = slider_left + (slider_right - slider_left) * 0.5;
            p.line_segment([Pos2::new(mid_x, slider_y - s.f(3.5)), Pos2::new(mid_x, slider_y + s.f(3.5))],
                           Stroke::new(s.f(1.0), c::with_alpha(c::SOFT, 150)));
        }
        let hw = s.f(4.0);
        let hx = slider_left + hw * 0.5 + n * ((slider_right - slider_left) - hw);
        p.rect_filled(Rect::from_center_size(Pos2::new(hx, slider_y), Vec2::new(hw, s.f(12.0))),
                      s.rr(2.0), accent.linear_multiply(alpha));
    }

    let hit = Rect::from_min_max(Pos2::new(slider_left - s.f(4.0), slider_y - s.f(10.0)),
                                  Pos2::new(slider_right + s.f(4.0), slider_y + s.f(10.0)));
    let resp = ui.allocate_rect(hit, Sense::click_and_drag());
    // Drag-only model: react to drag_started / dragged / drag_stopped /
    // double_clicked, NEVER to single clicked(). When the user drags the
    // TAP COUNT widget upward, new rows pop into existence under the cursor.
    // egui's release event for that drag was firing Response::clicked() on
    // whichever newly-appeared row was under the cursor — the "ghost edit"
    // bug. drag_started() only fires for a fresh press landing in this hit
    // rect, so it can't be triggered as a side-effect of an unrelated drag.
    if resp.drag_started() {
        setter.begin_set_parameter(value_param);
        if let Some(pos) = resp.interact_pointer_pos() {
            let nx = ((pos.x - slider_left) / (slider_right - slider_left)).clamp(0.0, 1.0);
            setter.set_parameter(value_param, pos_to_val(nx));
        }
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nx = ((pos.x - slider_left) / (slider_right - slider_left)).clamp(0.0, 1.0);
            setter.set_parameter(value_param, pos_to_val(nx));
        }
    }
    if resp.drag_stopped() { setter.end_set_parameter(value_param); }
    if resp.double_clicked() {
        setter.begin_set_parameter(value_param);
        setter.set_parameter_normalized(value_param, value_param.default_normalized_value());
        setter.end_set_parameter(value_param);
    }

    {
        let p = ui.painter();
        p.text(Pos2::new(rect.max.x - s.f(8.0), rect.center().y), Align2::RIGHT_CENTER,
               &formatter(v),
               FontId::new(s.f(9.0), FontFamily::Monospace), c::SOFT);
    }
}

fn draw_right_footer(ui: &mut Ui, rect: Rect, params: &Arc<PetalParams>, setter: &ParamSetter,
                     st: &mut EditorState, tempo_bpm: f32, s: S) {
    // Footer is split into three equal columns:
    //  [FEEDBACK knob + %]  |  [FB TIME value + FREE/SYNC]  |  [TAPS count]
    let third = rect.width() / 3.0;
    let col = |i: usize| Rect::from_min_max(
        Pos2::new(rect.min.x + i as f32 * third, rect.min.y),
        Pos2::new(rect.min.x + (i + 1) as f32 * third, rect.max.y),
    );
    let fb_rect = col(0);
    let ft_rect = col(1);
    let ct_rect = col(2);

    // ── Column 0: FEEDBACK amount ─────────────────────────────────────────
    // Feedback domain is color-coded coral (accent B), like the kit's Loop B.
    knob_at_accent(ui, Pos2::new(fb_rect.min.x + s.f(22.0), fb_rect.center().y), s.f(32.0),
                   &params.feedback, setter, s, c::ACCENT_B);
    {
        let p = ui.painter();
        p.text(Pos2::new(fb_rect.min.x + s.f(46.0), fb_rect.center().y - s.f(6.0)), Align2::LEFT_CENTER,
               "FEEDBACK", FontId::new(s.f(8.0), FontFamily::Proportional), c::ACCENT_B);
        p.text(Pos2::new(fb_rect.min.x + s.f(46.0), fb_rect.center().y + s.f(8.0)), Align2::LEFT_CENTER,
               &format!("{:.0}%", params.feedback.value() * 100.0),
               FontId::new(s.f(12.0), FontFamily::Monospace), c::INK);
    }

    // ── Column 1: FB TIME ─────────────────────────────────────────────────
    // LOCKED (default): FB TIME = cluster span (last active tap delay).
    //   Value display is static; shows the live cluster span in ms/s.
    // UNLOCKED: FB TIME = independent feedback_time param; drag to adjust.
    //   Right-click the lock button to toggle FREE / SYNC mode.
    use crate::params::SyncMode;
    let ft_locked = params.feedback_time_locked.value();

    // Compute cluster span for display when locked.
    // With anchoring, last active tap is always at n_active * base_time.
    let ft_n_active = (params.num_taps_active.value() as usize).clamp(1, NUM_TAPS);
    let ft_base_l = params.base_time_seconds(params.base_time_l.value(), tempo_bpm);
    let ft_base_r = if params.time_linked.value() { ft_base_l }
                    else { params.base_time_seconds(params.base_time_r.value(), tempo_bpm) };
    let cluster_span_secs = (ft_n_active as f32 * ft_base_l)
        .max(ft_n_active as f32 * ft_base_r);

    // Format the displayed value.
    let fb_time_display = if ft_locked {
        let ms = cluster_span_secs * 1000.0;
        if ms < 1000.0 { format!("{:.0}ms", ms) } else { format!("{:.2}s", cluster_span_secs) }
    } else {
        params.format_feedback_time(tempo_bpm)
    };

    {
        let p = ui.painter();
        p.line_segment([Pos2::new(ft_rect.min.x, ft_rect.min.y + s.f(8.0)),
                        Pos2::new(ft_rect.min.x, ft_rect.max.y - s.f(8.0))],
                       Stroke::new(s.f(1.0), c::LINE_SOFT));
        p.line_segment([Pos2::new(ft_rect.max.x, ft_rect.min.y + s.f(8.0)),
                        Pos2::new(ft_rect.max.x, ft_rect.max.y - s.f(8.0))],
                       Stroke::new(s.f(1.0), c::LINE_SOFT));
        p.text(Pos2::new(ft_rect.center().x, ft_rect.min.y + s.f(9.0)), Align2::CENTER_TOP,
               "FB TIME", FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    }

    // Value display area — interactive only when unlocked.
    let val_rect = Rect::from_center_size(
        Pos2::new(ft_rect.center().x, ft_rect.center().y - s.f(4.0)),
        Vec2::new(ft_rect.width() - s.f(8.0), s.f(28.0)),
    );
    let val_sense = if ft_locked { Sense::hover() } else { Sense::click_and_drag() };
    let val_resp = ui.allocate_rect(val_rect, val_sense);
    if !ft_locked {
        if val_resp.drag_started() {
            setter.begin_set_parameter(&params.feedback_time);
            st.fb_time_drag_accum = 0.0;
            st.fb_time_drag_start = params.feedback_time.value();
        }
        if val_resp.dragged() {
            st.fb_time_drag_accum -= val_resp.drag_delta().y;
            let new_norm = if params.feedback_time_mode.value() == SyncMode::Sync {
                let n_divs = crate::params::SYNC_DIVS.len() as f32 - 1.0;
                let start_idx = (st.fb_time_drag_start * n_divs).round();
                let delta_idx = (st.fb_time_drag_accum / 14.0).round();
                ((start_idx + delta_idx) / n_divs).clamp(0.0, 1.0)
            } else {
                (st.fb_time_drag_start + st.fb_time_drag_accum * 0.003).clamp(0.0, 1.0)
            };
            setter.set_parameter(&params.feedback_time, new_norm);
        }
        if val_resp.drag_stopped() {
            setter.end_set_parameter(&params.feedback_time);
            st.fb_time_drag_accum = 0.0;
        }
        if val_resp.double_clicked() {
            setter.begin_set_parameter(&params.feedback_time);
            setter.set_parameter_normalized(&params.feedback_time,
                                            params.feedback_time.default_normalized_value());
            setter.end_set_parameter(&params.feedback_time);
        }
    }
    let hovering_val = !ft_locked && (val_resp.hovered() || val_resp.dragged());
    let val_col = if ft_locked { c::SOFT }
                  else if hovering_val { c::ACCENT_2 }
                  else { c::INK };
    {
        let p = ui.painter();
        if hovering_val { p.rect_filled(val_rect, s.rr(4.0), c::PANEL2); }
        p.text(val_rect.center(), Align2::CENTER_CENTER, &fb_time_display,
               FontId::new(s.f(14.0), FontFamily::Monospace), val_col);
        if !ft_locked {
            p.text(Pos2::new(val_rect.center().x, val_rect.min.y + s.f(1.0)), Align2::CENTER_TOP,
                   "▲", FontId::new(s.f(7.0), FontFamily::Proportional),
                   c::FAINT.linear_multiply(if hovering_val { 1.0 } else { 0.5 }));
            p.text(Pos2::new(val_rect.center().x, val_rect.max.y - s.f(1.0)), Align2::CENTER_BOTTOM,
                   "▼", FontId::new(s.f(7.0), FontFamily::Proportional),
                   c::FAINT.linear_multiply(if hovering_val { 1.0 } else { 0.5 }));
        }
    }

    // Lock / unlock button at the bottom of the FB TIME column.
    let lock_h = s.f(16.0);
    let lock_w = ft_rect.width() - s.f(12.0);
    let lock_rect = Rect::from_center_size(
        Pos2::new(ft_rect.center().x, ft_rect.max.y - lock_h * 0.5 - s.f(3.0)),
        Vec2::new(lock_w, lock_h),
    );
    let lock_resp = ui.allocate_rect(lock_rect, Sense::click());
    // Left-click: toggle lock
    if lock_resp.clicked() {
        setter.begin_set_parameter(&params.feedback_time_locked);
        setter.set_parameter(&params.feedback_time_locked, !ft_locked);
        setter.end_set_parameter(&params.feedback_time_locked);
    }
    // Right-click when unlocked: cycle FREE/SYNC
    if lock_resp.secondary_clicked() && !ft_locked {
        let next_mode = match params.feedback_time_mode.value() {
            SyncMode::Free => SyncMode::Sync,
            SyncMode::Sync => SyncMode::Free,
        };
        setter.begin_set_parameter(&params.feedback_time_mode);
        setter.set_parameter(&params.feedback_time_mode, next_mode);
        setter.end_set_parameter(&params.feedback_time_mode);
    }
    {
        let p = ui.painter();
        // Kit `.btn.on`: accent fill + white text + soft outer glow (coral —
        // this button belongs to the feedback domain).
        let (btn_bg, btn_border, btn_text_col) = if ft_locked {
            (c::ACCENT_B, c::ACCENT_B, Color32::WHITE)
        } else {
            (c::INSET, c::LINE, c::INK2)
        };
        p.rect_filled(lock_rect, s.rr(6.0), btn_bg);
        p.rect_stroke(lock_rect, s.rr(6.0), Stroke::new(s.f(1.0), btn_border), StrokeKind::Inside);
        if ft_locked {
            p.rect_stroke(lock_rect.expand(s.f(1.5)), s.rr(7.0),
                          Stroke::new(s.f(1.0), c::glow(c::ACCENT_B)), StrokeKind::Outside);
        }
        let lock_label = if ft_locked {
            "LOCKED"
        } else {
            match params.feedback_time_mode.value() {
                SyncMode::Free => "FREE",
                SyncMode::Sync => "SYNC",
            }
        };
        p.text(lock_rect.center(), Align2::CENTER_CENTER, lock_label,
               FontId::new(s.f(7.5), FontFamily::Proportional), btn_text_col);
    }

    // ── Column 2: TAPS count ──────────────────────────────────────────────
    let n = params.num_taps_active.value();
    {
        let p = ui.painter();
        p.text(Pos2::new(ct_rect.center().x, ct_rect.min.y + s.f(10.0)), Align2::CENTER_TOP,
               "TAPS", FontId::new(s.f(8.0), FontFamily::Proportional), c::FAINT);
    }
    let num_rect = Rect::from_center_size(
        Pos2::new(ct_rect.center().x, ct_rect.center().y + s.f(6.0)),
        Vec2::new(s.f(50.0), s.f(34.0)),
    );
    let drag_resp = ui.allocate_rect(num_rect, Sense::click_and_drag());
    if drag_resp.drag_started() {
        setter.begin_set_parameter(&params.num_taps_active);
        st.tap_count_drag_accum = 0.0;
        st.tap_count_drag_start = n;
    }
    if drag_resp.dragged() {
        st.tap_count_drag_accum -= drag_resp.drag_delta().y;
        let target = st.tap_count_drag_start + (st.tap_count_drag_accum / 18.0).round() as i32;
        let new_n = target.clamp(1, NUM_TAPS as i32);
        if new_n != n { setter.set_parameter(&params.num_taps_active, new_n); }
    }
    if drag_resp.drag_stopped() {
        setter.end_set_parameter(&params.num_taps_active);
        st.tap_count_drag_accum = 0.0;
    }
    let scroll = ui.input(|i| i.raw_scroll_delta.y);
    if num_rect.contains(ui.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO))) && scroll != 0.0 {
        setter.begin_set_parameter(&params.num_taps_active);
        setter.set_parameter(&params.num_taps_active, (n + scroll.signum() as i32).clamp(1, NUM_TAPS as i32));
        setter.end_set_parameter(&params.num_taps_active);
    }
    let hovering = drag_resp.hovered() || drag_resp.dragged();
    {
        let p = ui.painter();
        if hovering { p.rect_filled(num_rect, s.rr(4.0), c::PANEL2); }
        p.text(num_rect.center(), Align2::CENTER_CENTER, &format!("{}", n),
               FontId::new(s.f(24.0), FontFamily::Monospace),
               if hovering { c::ACCENT_2 } else { c::INK });
        p.text(Pos2::new(num_rect.center().x, num_rect.min.y + s.f(1.0)), Align2::CENTER_TOP,
               "▲", FontId::new(s.f(7.0), FontFamily::Proportional),
               c::FAINT.linear_multiply(if hovering { 1.0 } else { 0.5 }));
        p.text(Pos2::new(num_rect.center().x, num_rect.max.y - s.f(1.0)), Align2::CENTER_BOTTOM,
               "▼", FontId::new(s.f(7.0), FontFamily::Proportional),
               c::FAINT.linear_multiply(if hovering { 1.0 } else { 0.5 }));
    }
}

#[allow(dead_code)]
fn _unused_avoid_warns() {
    let _ = SYNC_DIVS;
    let _ = snap_to_div(0.5);
}
