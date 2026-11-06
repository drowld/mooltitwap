//! Epoché visual language — color tokens, fonts and style, ported from the
//! user's design kit (`Epoche UI.dc.html` + `epoche-kit.js`).
//!
//! Kept in sync with cumulus-rs's `theme.rs` so the two plugins share one visual
//! language. y-flanger uses a subset of the tokens/helpers; the rest are kept for
//! parity (hence the module-wide dead-code allow).
#![allow(dead_code, deprecated)]

use egui::Color32;

// ---- colours: since v2.49 the current theme is softsystems-ui's (`t()`, set
// at the top of every frame by `begin_frame`). These accessors keep the names
// the page was written with; they read the shared palette (Afterecho's own
// preset, `presets::AFTERECHO`, has the Mono / Paper values these used to
// hold). ----
use softsystems_ui::t;

#[allow(non_snake_case)] #[inline] pub fn PAPER() -> Color32 { t().c.page }
#[allow(non_snake_case)] #[inline] pub fn FRAME() -> Color32 { t().c.frame }
#[allow(non_snake_case)] #[inline] pub fn PANEL() -> Color32 { t().c.panel }
#[allow(non_snake_case)] #[inline] pub fn PANEL2() -> Color32 { t().c.cap }
#[allow(non_snake_case)] #[inline] pub fn INSET() -> Color32 { t().c.well }
#[allow(non_snake_case)] #[inline] pub fn INK() -> Color32 { t().c.ink }
#[allow(non_snake_case)] #[inline] pub fn INK2() -> Color32 { t().c.ink2 }
#[allow(non_snake_case)] #[inline] pub fn SOFT() -> Color32 { t().c.soft }
#[allow(non_snake_case)] #[inline] pub fn FAINT() -> Color32 { t().c.faint }
#[allow(non_snake_case)] #[inline] pub fn METALHI() -> Color32 { t().c.cap }
#[allow(non_snake_case)] #[inline] pub fn G() -> Color32 { t().c.accent }
#[allow(non_snake_case)] #[inline] pub fn M() -> Color32 { t().hue(0) }

pub fn is_dark() -> bool {
    t().dark
}

/// Data hue `i` (0 teal · 1 coral · 2 violet · 3 amber · 4 green · 5 magenta),
/// theme-aware, wrapping. Colour-codes the taps' sides and the mod sources;
/// it keeps its hue whatever the accent (owner, 2026-11-02).
#[inline]
pub fn accent(i: usize) -> Color32 {
    t().hue(i)
}

/// Fill for a selected segment / pressed toggle: the accent, lightly tinted
/// over whatever it sits on.
pub fn on_fill() -> Color32 {
    with_alpha(G(), if is_dark() { 66 } else { 54 })
}

pub fn line() -> Color32 {
    t().c.line
}
pub fn line2() -> Color32 {
    t().c.line2
}
pub fn gdim() -> Color32 {
    with_alpha(G(), 38)
}
/// Neutral wash for things that are grouped, not selected.
pub fn sel_bg() -> Color32 {
    t().c.sel
}
/// Modal-backdrop veil.
pub fn scrim() -> Color32 {
    if is_dark() {
        Color32::from_black_alpha(195)
    } else {
        Color32::from_black_alpha(90)
    }
}
pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// ---- spacing scale (logical px, pre-zoom; multiply by ui_scale at paint) ----
// Base unit 4. The harmonic set {2,3,4,6,8,10,12,18}. See docs/SPACING.md.
pub const SP_HAIR: f32 = 2.0; // hairline nudge only — never inter-control
pub const SP_TIGHT: f32 = 3.0; // tab/chip/mode rows
pub const SP_XS: f32 = 4.0; // base unit; minor add_space
pub const SP_SM: f32 = 6.0; // slider cap|track|value gap
pub const SP_MD: f32 = 8.0; // primary: inter-card, section breaks, knob rows
pub const SP_LG: f32 = 10.0; // card horiz margin, switch-row gap
pub const SP_XL: f32 = 12.0; // card vert margin, knob gap, 2-col gutter, page margin
pub const SP_XXL: f32 = 18.0; // deliberate bottom-push only

// stroke widths (logical px — MUST be * ui_scale at paint)
pub const STROKE_HAIR: f32 = 1.0; // borders, grids, hold markers
pub const STROKE_LINE: f32 = 1.4; // connectors, point rings, brackets
pub const STROKE_BOLD: f32 = 1.8; // curve lines, selected handle
pub const STROKE_TRACK: f32 = 3.0; // slider track baseline + fill

// container framing (logical px)
pub const CARD_GAP: f32 = 8.0; // gap between the 3 bottom cards (== SP_MD)
pub const CARD_MARGIN_X: f32 = 10.0; // card frame inner margin, horizontal
pub const CARD_MARGIN_Y: f32 = 12.0; // card frame inner margin, vertical
pub const CARD_MARGIN_TOTAL_X: f32 = 22.0; // 2*CARD_MARGIN_X + 2px border; body = w - this
pub const CARD_BODY_INSET: f32 = 4.0; // extra slack so sliders never kiss the border
pub const CARD_RADIUS: f32 = 10.0; // card / wave corners
pub const CARD_CHIP_OVERHANG: f32 = 5.0; // top space reserved for the title chip
// ============================================================================
// BUTTON CLASS — the single source of truth for EVERY chip / toggle / pill /
// segment. Change these four and all buttons restyle together. Height + font
// are uniform across buttons; only WIDTH varies with content.
// ============================================================================
pub const BTN_H: f32 = 24.0; // uniform button height (== the readout pills / VOICES)
pub const BTN_PAD_X: f32 = 22.0; // horizontal text padding, total (~11px/side)
pub const BTN_RADIUS: f32 = 8.0; // corner radius
pub const BTN_FS: f32 = 10.0; // label/value font size

// slider row geometry (logical px)
pub const SLIDER_H: f32 = 17.0; // row height
pub const SLIDER_VALUE_W: f32 = 40.0; // right-aligned mono value column (snug to the track)
pub const SLIDER_TRACK_MIN: f32 = 24.0; // overflow floor for computed track width
pub const SLIDER_CAP_2COL: f32 = 36.0; // cap width inside 2-col slider grids

// knob cell geometry (logical px)
pub const KNOB_DIAM_INSPECT: f32 = 48.0; // head inspector knobs
pub const KNOB_DIAM_FX: f32 = 40.0; // FX tab knobs
pub const KNOB_PAD_W: f32 = 14.0; // side padding added to diameter for cell width
pub const KNOB_STACK_H: f32 = 26.0; // label-above + value-below vertical budget
pub const KNOB_GAP: f32 = 12.0; // horizontal gap between knob cells (== SP_XL)

// tile (head grid) geometry (logical px)
pub const TILE_H: f32 = 34.0;
pub const TILE_GAP: f32 = 5.0;
pub const TILE_RADIUS: f32 = 7.0; // == CHIP_RADIUS
pub const TILE_PAD: f32 = 7.0; // unified tile interior padding (was the 5/7 mess)

// font sizes by role (Space Grotesk prop / Spline Sans Mono mono)
pub const FS_KNOB: f32 = 8.0;
pub const FS_LABEL: f32 = 8.5; // slider cap, card title, hint — the workhorse
pub const FS_VALUE: f32 = 9.0; // sample name, ruler-adjacent status
pub const FS_TILE: f32 = 10.0; // tile head number
pub const FS_HEADING: f32 = 12.0;
pub const FS_TOOLBAR: f32 = 13.0; // pop-out toolbar title

// ---- titlebar / control-strip geometry (Epoché two-row header) ----
pub const TITLEBAR_H: f32 = 50.0; // row 1 height
pub const STRIP_H: f32 = 46.0; // row 2 (control/master) height
// titlebar icon buttons (hamburger / gear) — Epoché 32x28 inset box
pub const TBBTN_W: f32 = 32.0;
pub const TBBTN_H: f32 = 28.0;
pub const TBBTN_RADIUS: f32 = 8.0; // icon box + sample/readout well radius
// preset-slot pill
pub const PILL_RADIUS: f32 = 9.0;
pub const PILL_PAD_X: f32 = 13.0;
pub const PILL_MIN_W: f32 = 300.0; // preset + pool slots — wide enough to read the name
// segmented play-mode control
pub const SEG_PAD: f32 = 2.0; // outer inset + inter-segment gap
pub const SEG_RADIUS: f32 = 6.0; // active-segment corner
pub const SEG_ITEM_PAD_X: f32 = 11.0;
// twin VU bars
pub const VU_BAR_W: f32 = 7.0;
pub const VU_BAR_H: f32 = 26.0;
pub const VU_BAR_GAP: f32 = 4.0;
pub const VU_RADIUS: f32 = 3.0;

/// The fonts (Hanken Grotesk, the wordmark in its SemiBold): softsystems-ui's,
/// installed once at editor creation.
pub fn install_fonts(ctx: &egui::Context) {
    softsystems_ui::fonts::install(ctx);
}

/// The text-size factor the page's sizes were written against: 1.24 at the
/// design size (the old default, "+4" steps of 6 %), following the ☰ Text
/// size choice (`t().text`, 1.0 = design size).
pub fn font_scale() -> f32 {
    softsystems_ui::t().text * 1.24
}

// ---- live slider geometry (iPad ergonomics; tunable from the menu). `size` is a
// multiplier on the slider row height + handle (1.0 = design default); `gap` is
// extra logical px added to each row's height so adjacent rows sit further apart.
// Both read inside epo::slider_ex, so no per-call-site plumbing. ----
static SLIDER_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn slider_size() -> f32 {
    let b = SLIDER_SIZE.load(std::sync::atomic::Ordering::Relaxed);
    if b == 0 {
        1.0
    } else {
        f32::from_bits(b)
    }
}
pub fn set_slider_size(v: f32) {
    SLIDER_SIZE.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}
// 0 bits == 0.0 f32, so the zero-initialised default is already "no extra gap".
static SLIDER_GAP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn slider_gap() -> f32 {
    f32::from_bits(SLIDER_GAP.load(std::sync::atomic::Ordering::Relaxed))
}
pub fn set_slider_gap(v: f32) {
    SLIDER_GAP.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

// ---- live inline mod-matrix row height (iPad ergonomics; tunable from the
// menu). A multiplier on the matrix cell/row height (1.0 = design 18px). Taller
// rows = fewer mis-taps when dialing routes on-device. Read inside the inline
// matrix grid, so no per-call-site plumbing. 0 bits == the 1.0 default. ----
static MATRIX_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn matrix_scale() -> f32 {
    let b = MATRIX_SCALE.load(std::sync::atomic::Ordering::Relaxed);
    if b == 0 {
        1.0
    } else {
        f32::from_bits(b)
    }
}
pub fn set_matrix_scale(v: f32) {
    MATRIX_SCALE.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Card/tab title font size (logical px, pre-`ui_scale`). One source of truth so
/// every card header AND every tab label matches. A notch smaller on touch — the
/// iPad font bump makes titles read a bit large. Multiply by `ui_scale` at use.
// Live-tunable card/tab title size on touch (OPTIONS ▸ TITLE TEXT). 0 bits = the
// 10px default; independent of the body TEXT nudge so titles hold a steady size.
static TITLE_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn title_size() -> f32 {
    let b = TITLE_SIZE.load(std::sync::atomic::Ordering::Relaxed);
    if b == 0 {
        10.0
    } else {
        f32::from_bits(b)
    }
}
pub fn set_title_size(v: f32) {
    TITLE_SIZE.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}
pub fn title_fs() -> f32 {
    if touch() {
        title_size()
    } else {
        FS_LABEL * font_scale()
    }
}

// ---- touch mode (iPad) — process-global, set once at editor `create`. When
// true, widgets expand their interaction rects to a finger-friendly floor and
// offer tap-to-open value editors. Defaults false so desktop is unchanged. ----
static TOUCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[inline]
pub fn touch() -> bool {
    TOUCH.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn set_touch(v: bool) {
    TOUCH.store(v, std::sync::atomic::Ordering::Relaxed);
}
/// Apple-HIG minimum touch target, in logical px (multiply by `ui_scale` at use).
pub const TOUCH_MIN_HIT: f32 = 44.0;
/// Readout / drag-pill touch height — smaller than the 44pt hit floor so dense
/// control rows and the titlebar strip stay tight (the pill is still an easy
/// vertical-drag target). The matrix footer + control-row heights track this.
pub const READOUT_TOUCH_H: f32 = 34.0;

/// Custom painters recover the zoom factor from the scaled Body font size.
pub fn ui_scale(ui: &egui::Ui) -> f32 {
    let body = |s: &egui::Style| {
        s.text_styles
            .get(&egui::TextStyle::Body)
            .map(|f| f.size)
            .unwrap_or(12.0)
    };
    body(ui.style()) / body(&egui::Style::default())
}
