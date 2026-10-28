//! Epoché visual language — color tokens, fonts and style, ported from the
//! user's design kit (`Epoche UI.dc.html` + `epoche-kit.js`).
//!
//! Kept in sync with cumulus-rs's `theme.rs` so the two plugins share one visual
//! language. y-flanger uses a subset of the tokens/helpers; the rest are kept for
//! parity (hence the module-wide dead-code allow).
#![allow(dead_code, deprecated)]

use egui::{Color32, FontData, FontDefinitions, FontFamily};

// ---- palette: the 13 theme colors are selected at RUNTIME from a process-
// global active palette (dark default). The public UPPERCASE accessors are
// zero-arg fns reading `active()`, so every call site is `INK()` / `PAPER()` …
// and re-themes live. DARK = the original kit values; LIGHT = the Inertia
// palette. See THEME_MIGRATION_PLAN.md. ----
#[derive(Clone, Copy)]
pub struct Palette {
    pub paper: Color32,
    pub frame: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub inset: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub soft: Color32,
    pub faint: Color32,
    pub wave: Color32,
    pub metalhi: Color32,
    pub m: Color32,
    /// base color for the hairline helpers (light flips this dark)
    pub line_base: Color32,
    pub line_a: u8,
    pub line2_a: u8,
    /// indexed accent hues for color-coding (Inertia: teal coral violet amber
    /// green magenta) — `g`/`m` alias accents[4]/[0].
    pub accents: [Color32; 6],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Inertia dark palette (cool blue-grey).
pub const DARK: Palette = Palette {
    paper: rgb(0x12, 0x14, 0x17),
    frame: rgb(0x19, 0x1C, 0x20),
    panel: rgb(0x20, 0x24, 0x29),
    panel2: rgb(0x26, 0x2B, 0x31), // Inertia inset (faint bg)
    inset: rgb(0x17, 0x1A, 0x1E),  // Inertia metal-lo (deeper well)
    ink: rgb(0xEC, 0xE8, 0xDF),
    ink2: rgb(0xB9, 0xB5, 0xAB),
    soft: rgb(0x88, 0x8D, 0x94),
    faint: rgb(0x56, 0x5C, 0x64),
    wave: rgb(0x4E, 0x55, 0x5E), // lifted ~12% toward white — the old 0x3A grey read too dark on the frame
    metalhi: rgb(0x2C, 0x32, 0x3A),
    m: rgb(0x58, 0xC5, 0xC2), // accent teal  (= accents[0])
    line_base: rgb(0xEC, 0xE8, 0xDF),
    line_a: 28,
    line2_a: 48,
    accents: [
        rgb(0x58, 0xC5, 0xC2), // 0 teal
        rgb(0xE8, 0x87, 0x62), // 1 coral
        rgb(0xA0, 0x86, 0xE8), // 2 violet
        rgb(0xE4, 0xB2, 0x5C), // 3 amber
        rgb(0x78, 0xC8, 0x86), // 4 green
        rgb(0xE0, 0x80, 0xC4), // 5 magenta
    ],
};

/// Warm-graphite dark variant — neutrals lean warm-neutral to agree with the
/// cream ink (the cool `DARK` sets warm text on a blue-cool charcoal). Accents
/// unchanged; only the surface/ink/faint neutrals differ.
pub const DARK_WARM: Palette = Palette {
    paper: rgb(0x14, 0x13, 0x10),
    frame: rgb(0x1D, 0x1B, 0x17),
    panel: rgb(0x24, 0x22, 0x1D),
    panel2: rgb(0x2A, 0x28, 0x23),
    inset: rgb(0x17, 0x15, 0x0F),
    ink: rgb(0xED, 0xE8, 0xDD),
    ink2: rgb(0xB9, 0xB3, 0xA6),
    soft: rgb(0x8B, 0x85, 0x78),
    faint: rgb(0x58, 0x53, 0x49),
    wave: rgb(0x51, 0x4B, 0x40),
    metalhi: rgb(0x33, 0x2E, 0x27),
    m: rgb(0x58, 0xC5, 0xC2),
    line_base: rgb(0xED, 0xE8, 0xDD),
    line_a: 28,
    line2_a: 48,
    accents: [
        rgb(0x58, 0xC5, 0xC2), // 0 teal
        rgb(0xE8, 0x87, 0x62), // 1 coral
        rgb(0xA0, 0x86, 0xE8), // 2 violet
        rgb(0xE4, 0xB2, 0x5C), // 3 amber
        rgb(0x78, 0xC8, 0x86), // 4 green
        rgb(0xE0, 0x80, 0xC4), // 5 magenta
    ],
};

/// Ink-black dark variant — near-neutral, deeper panels for higher contrast
/// (accents pop harder, panels recede). Accents unchanged.
pub const DARK_INK: Palette = Palette {
    paper: rgb(0x0D, 0x0D, 0x0F),
    frame: rgb(0x15, 0x15, 0x17),
    panel: rgb(0x1C, 0x1C, 0x20),
    panel2: rgb(0x22, 0x22, 0x28),
    inset: rgb(0x0F, 0x0F, 0x11),
    ink: rgb(0xEC, 0xE8, 0xDF),
    ink2: rgb(0xB7, 0xB4, 0xAC),
    soft: rgb(0x86, 0x86, 0x8E),
    faint: rgb(0x52, 0x52, 0x5A),
    wave: rgb(0x4A, 0x4A, 0x52),
    metalhi: rgb(0x28, 0x28, 0x2E),
    m: rgb(0x58, 0xC5, 0xC2),
    line_base: rgb(0xEC, 0xE8, 0xDF),
    line_a: 28,
    line2_a: 48,
    accents: [
        rgb(0x58, 0xC5, 0xC2), // 0 teal
        rgb(0xE8, 0x87, 0x62), // 1 coral
        rgb(0xA0, 0x86, 0xE8), // 2 violet
        rgb(0xE4, 0xB2, 0x5C), // 3 amber
        rgb(0x78, 0xC8, 0x86), // 4 green
        rgb(0xE0, 0x80, 0xC4), // 5 magenta
    ],
};

/// OLED: the Ink palette on true black (pixels off on OLED screens); cards are
/// lifted just enough to keep the layout readable.
pub const DARK_OLED: Palette = Palette {
    paper: rgb(0x00, 0x00, 0x00),
    frame: rgb(0x0A, 0x0A, 0x0B),
    panel: rgb(0x12, 0x12, 0x14),
    panel2: rgb(0x1B, 0x1B, 0x1F),
    inset: rgb(0x00, 0x00, 0x00),
    ink: rgb(0xEC, 0xE8, 0xDF),
    ink2: rgb(0xB7, 0xB4, 0xAC),
    soft: rgb(0x86, 0x86, 0x8E),
    faint: rgb(0x52, 0x52, 0x5A),
    wave: rgb(0x4A, 0x4A, 0x52),
    metalhi: rgb(0x28, 0x28, 0x2E),
    m: rgb(0x58, 0xC5, 0xC2),
    line_base: rgb(0xEC, 0xE8, 0xDF),
    line_a: 28,
    line2_a: 48,
    accents: [
        rgb(0x58, 0xC5, 0xC2), // 0 teal
        rgb(0xE8, 0x87, 0x62), // 1 coral
        rgb(0xA0, 0x86, 0xE8), // 2 violet
        rgb(0xE4, 0xB2, 0x5C), // 3 amber
        rgb(0x78, 0xC8, 0x86), // 4 green
        rgb(0xE0, 0x80, 0xC4), // 5 magenta
    ],
};

/// Afterecho's dark page (2026-10-28, "Mono"): neutral greys with no hue bias,
/// surfaces stepping clearly from page to card to well; ink does the work and
/// colour is kept for the sound (the taps and their repeats).
pub const MONO: Palette = Palette {
    paper: rgb(0x12, 0x12, 0x12),
    frame: rgb(0x16, 0x16, 0x16),
    panel: rgb(0x1C, 0x1C, 0x1C),
    panel2: rgb(0x2A, 0x2A, 0x2A),
    inset: rgb(0x0B, 0x0B, 0x0B),
    ink: rgb(0xEE, 0xEE, 0xEC),
    ink2: rgb(0xB5, 0xB5, 0xB2),
    soft: rgb(0x85, 0x85, 0x83),
    faint: rgb(0x52, 0x52, 0x50),
    wave: rgb(0x4A, 0x4A, 0x4A),
    metalhi: rgb(0x2A, 0x2A, 0x2A),
    m: rgb(0x58, 0xC5, 0xC2),
    line_base: rgb(0xEE, 0xEE, 0xEC),
    line_a: 26,
    line2_a: 54,
    accents: [
        rgb(0x58, 0xC5, 0xC2), // 0 teal
        rgb(0xE8, 0x87, 0x62), // 1 coral
        rgb(0xA0, 0x86, 0xE8), // 2 violet
        rgb(0xE4, 0xB2, 0x5C), // 3 amber
        rgb(0x78, 0xC8, 0x86), // 4 green
        rgb(0xE0, 0x80, 0xC4), // 5 magenta
    ],
};

/// Afterecho's light page (2026-10-28, "Paper"): warm paper surfaces that step
/// clearly from page to card, near-black ink. Replaces the Inertia light values.
pub const LIGHT: Palette = Palette {
    paper: rgb(0xE7, 0xE4, 0xDD),
    frame: rgb(0xEE, 0xEB, 0xE5),
    panel: rgb(0xF6, 0xF4, 0xEF),
    panel2: rgb(0xFF, 0xFF, 0xFF),
    inset: rgb(0xDE, 0xDA, 0xD1),
    ink: rgb(0x1E, 0x1D, 0x1B),
    ink2: rgb(0x4B, 0x48, 0x44),
    soft: rgb(0x8B, 0x86, 0x7D),
    faint: rgb(0xB9, 0xB4, 0xAA),
    wave: rgb(0xC9, 0xC3, 0xB6),
    metalhi: rgb(0xFC, 0xFB, 0xF8),
    m: rgb(0x28, 0x7C, 0x7C), // accent teal  (= accents[0]) — darkened for light-bg legibility
    line_base: rgb(0x1E, 0x1D, 0x1B),
    line_a: 26,
    line2_a: 56,
    // light accents darkened from Inertia's vivid values to clear ~4.5:1 on the
    // near-white panels (used as 7.5-10px text: tile numbers, matrix/tab labels).
    accents: [
        rgb(0x28, 0x7C, 0x7C), // 0 teal
        rgb(0xAE, 0x5A, 0x40), // 1 coral
        rgb(0x7C, 0x5C, 0xCC), // 2 violet (already passes ~4.5)
        rgb(0x8E, 0x63, 0x14), // 3 amber
        rgb(0x3A, 0x7E, 0x4E), // 4 green
        rgb(0xAE, 0x4F, 0x90), // 5 magenta
    ],
};

static ACTIVE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // 0=dark 1=light
/// Which dark palette variant to use (0 cool · 1 warm · 2 ink · 3 OLED · 4 mono) —
/// only consulted when ACTIVE==0. Process-global; survives engine rebuilds.
static DARK_VARIANT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
#[inline]
fn active() -> Palette {
    if ACTIVE.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        match DARK_VARIANT.load(std::sync::atomic::Ordering::Relaxed) {
            1 => DARK_WARM,
            2 => DARK_INK,
            3 => DARK_OLED,
            4 => MONO,
            _ => DARK,
        }
    } else {
        LIGHT
    }
}
/// Select the active theme (process-global; survives engine rebuilds).
pub fn set_theme(dark: bool) {
    ACTIVE.store(if dark { 0 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}
pub fn is_dark() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Relaxed) == 0
}
/// The last dark palette variant (mono).
pub const DARK_VARIANTS: u8 = 5;
/// Select the dark palette variant (0 cool · 1 warm · 2 ink · 3 OLED · 4 mono; clamped).
pub fn set_dark_variant(i: u8) {
    DARK_VARIANT.store(i.min(DARK_VARIANTS - 1), std::sync::atomic::Ordering::Relaxed);
}
pub fn dark_variant() -> u8 {
    DARK_VARIANT.load(std::sync::atomic::Ordering::Relaxed)
}

/// User-selectable default accent (the old fixed green is index 0, still available).
/// (dark, light) pairs — the light variant is darkened for legibility on the light bg.
const ACCENT_HUES: [(Color32, Color32); 9] = [
    (rgb(0x78, 0xC8, 0x86), rgb(0x3A, 0x7E, 0x4E)), // 0 GREEN (original)
    (rgb(0x3F, 0xC1, 0xC9), rgb(0x1E, 0x82, 0x8A)), // 1 CYAN
    (rgb(0x3D, 0xBE, 0x9E), rgb(0x22, 0x84, 0x6C)), // 2 MINT
    (rgb(0x5B, 0x8D, 0xEF), rgb(0x33, 0x5E, 0xC2)), // 3 BLUE
    (rgb(0xE0, 0xA3, 0x3E), rgb(0xA6, 0x74, 0x1C)), // 4 AMBER
    (rgb(0xA0, 0x86, 0xE8), rgb(0x60, 0x49, 0xBE)), // 5 VIOLET
    (rgb(0x7E, 0x9C, 0xC9), rgb(0x3E, 0x5C, 0x8C)), // 6 SLATE (app-signature family)
    (rgb(0xD6, 0xD2, 0xC9), rgb(0x3A, 0x38, 0x34)), // 7 MONO — colorless (see `accent`)
    (rgb(0xFF, 0x5A, 0x36), rgb(0x2F, 0x5B, 0xEA)), // 8 AFTERECHO — signal red-orange on dark, cobalt on light
];
pub const ACCENT_NAMES: [&str; 9] = ["GREEN", "CYAN", "MINT", "BLUE", "AMBER", "VIOLET", "SLATE", "MONO", "AFTERECHO"];
/// Index of the colorless accent: every hue, the L/R pair included, turns grey.
pub const ACCENT_MONO: u8 = 7;
/// How many accents there are to pick from.
pub const ACCENTS: u8 = 9;
static ACCENT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Select the default accent (process-global; survives engine rebuilds).
pub fn set_accent(i: u8) {
    ACCENT.store(i.min(ACCENTS - 1), std::sync::atomic::Ordering::Relaxed);
}
pub fn accent_choice() -> u8 {
    ACCENT.load(std::sync::atomic::Ordering::Relaxed)
}
/// The chosen default accent, theme-aware — what `G()` resolves to.
pub fn accent_default() -> Color32 {
    let (d, l) = ACCENT_HUES[accent_choice().min(ACCENTS - 1) as usize];
    if is_dark() { d } else { l }
}
/// Theme-aware swatch color for accent choice `i` — for the menu picker.
pub fn accent_swatch(i: usize) -> Color32 {
    let (d, l) = ACCENT_HUES[i % ACCENT_HUES.len()];
    if is_dark() { d } else { l }
}

#[allow(non_snake_case)] #[inline] pub fn PAPER() -> Color32 { active().paper }
#[allow(non_snake_case)] #[inline] pub fn FRAME() -> Color32 { active().frame }
#[allow(non_snake_case)] #[inline] pub fn PANEL() -> Color32 { active().panel }
#[allow(non_snake_case)] #[inline] pub fn PANEL2() -> Color32 { active().panel2 }
#[allow(non_snake_case)] #[inline] pub fn INSET() -> Color32 { active().inset }
#[allow(non_snake_case)] #[inline] pub fn INK() -> Color32 { active().ink }
#[allow(non_snake_case)] #[inline] pub fn INK2() -> Color32 { active().ink2 }
#[allow(non_snake_case)] #[inline] pub fn SOFT() -> Color32 { active().soft }
#[allow(non_snake_case)] #[inline] pub fn FAINT() -> Color32 { active().faint }
#[allow(non_snake_case)] #[inline] pub fn WAVE() -> Color32 { active().wave }
#[allow(non_snake_case)] #[inline] pub fn METALHI() -> Color32 { active().metalhi }
#[allow(non_snake_case)] #[inline] pub fn G() -> Color32 { accent_default() }
#[allow(non_snake_case)] #[inline] pub fn M() -> Color32 { active().m }
/// Indexed accent hue (0 teal · 1 coral · 2 violet · 3 amber · 4 green · 5 magenta),
/// theme-aware. Wraps mod 6. Used to color-code heads + mod sources. Under the
/// MONO accent they turn grey, told apart by lightness (2 = L is the brightest,
/// 1 = R the dimmest).
#[inline]
pub fn accent(i: usize) -> Color32 {
    if accent_choice() == ACCENT_MONO {
        return match i % 6 {
            1 => SOFT(),
            2 => INK(),
            _ => INK2(),
        };
    }
    active().accents[i % 6]
}

/// Fill for a selected segment / pressed toggle: the accent, lightly tinted
/// over whatever it sits on, so the on-state reads in every theme (a neutral
/// step-up was nearly invisible on most of them).
pub fn on_fill() -> Color32 {
    with_alpha(accent_default(), if is_dark() { 66 } else { 54 })
}

pub fn line() -> Color32 {
    let p = active();
    with_alpha(p.line_base, p.line_a) // --line
}
pub fn line2() -> Color32 {
    let p = active();
    with_alpha(p.line_base, p.line2_a) // --line2
}
pub fn gdim() -> Color32 {
    with_alpha(accent_default(), 38) // --gdim .15 (follows the chosen accent)
}
/// Neutral wash (ink-based) for things that are grouped, not selected — the
/// palette's tag chips. Selection itself uses `on_fill`.
pub fn sel_bg() -> Color32 {
    with_alpha(INK2(), 30)
}
/// Modal-backdrop veil — deep over the dark UI, gentle over the light one (a
/// fixed 76% black buries a light page; a soft dim keeps the modal focused).
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

/// Font pairings. Karla is Tide's type; Plex is on trial (owner, 2026-10-26)
/// to compare in the plugin before deciding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontChoice {
    /// Karla for text + Inconsolata for numbers (Karla's digits aren't
    /// fixed-width, so readouts would jitter in it); the Greek wordmark falls
    /// back to egui's built-in font — Karla has no Greek.
    KarlaInconsolata,
    /// IBM Plex Sans for everything — its digits are fixed-width and it has
    /// Greek — with the wordmark in GFS Didot.
    PlexDidot,
}

/// Family name for the παλίρροια wordmark (`FontFamily::Name`).
pub const WORDMARK: &str = "wordmark";

/// Register all font faces and select the active pairing. Call again to
/// live-switch (it re-runs ctx.set_fonts). Fonts: SIL OFL 1.1 — licences in
/// assets/fonts/ ship with the plugin.
pub fn apply_fonts(ctx: &egui::Context, choice: FontChoice) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("karla", &include_bytes!("../../assets/fonts/Karla-Medium.ttf")[..]),
        ("inconsolata", &include_bytes!("../../assets/fonts/Inconsolata-Regular.ttf")[..]),
        ("plex", &include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf")[..]),
        ("didot", &include_bytes!("../../assets/fonts/GFSDidot-Regular.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    }
    let (prop, mono, mark) = match choice {
        FontChoice::KarlaInconsolata => ("karla", "inconsolata", None),
        FontChoice::PlexDidot => ("plex", "plex", Some("didot")),
    };
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, prop.into());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, mono.into());
    // Wordmark: Didot first when chosen, then the proportional chain (which
    // ends in egui's built-in fonts — what renders the Greek under Karla).
    let mut wm: Vec<String> = mark.into_iter().map(String::from).collect();
    wm.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(FontFamily::Name(WORDMARK.into()), wm);
    ctx.set_fonts(fonts);
}

/// Install the default font pairing (Karla) at editor creation, so the first
/// frame doesn't flash another face.
pub fn install_fonts(ctx: &egui::Context) {
    apply_fonts(ctx, FontChoice::KarlaInconsolata);
}

// ---- global font-size scale (live-tunable from the menu; fonts only, NOT
// geometry). All widget text sizes multiply by this; 1.0 = design default. ----
static FONT_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn font_scale() -> f32 {
    let b = FONT_SCALE.load(std::sync::atomic::Ordering::Relaxed);
    if b == 0 {
        1.0
    } else {
        f32::from_bits(b)
    }
}
pub fn set_font_scale(v: f32) {
    FONT_SCALE.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
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

/// Rebuild Style from default each frame (no compounding), scale it, and
/// apply the Epoché visuals.
pub fn apply_zoom(ctx: &egui::Context, zoom: f32) {
    let mut style = egui::Style::default();
    let mut v = if is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = PAPER();
    v.window_fill = PAPER();
    v.extreme_bg_color = INSET();
    v.faint_bg_color = PANEL2();
    v.widgets.noninteractive.bg_fill = PANEL();
    v.widgets.noninteractive.fg_stroke.color = INK2();
    v.widgets.inactive.bg_fill = INSET();
    v.widgets.inactive.fg_stroke.color = INK2();
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, line2());
    v.widgets.hovered.bg_fill = INSET();
    v.widgets.hovered.fg_stroke.color = INK();
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, line2());
    v.widgets.active.bg_fill = INSET();
    v.widgets.active.fg_stroke.color = INK();
    // neutral selection (was green) — generic UI state is never an accent
    v.selection.bg_fill = on_fill();
    v.selection.stroke = egui::Stroke::new(1.0, INK());
    style.visuals = v;
    for f in style.text_styles.values_mut() {
        f.size *= zoom;
    }
    style.spacing.item_spacing *= zoom;
    style.spacing.button_padding *= zoom;
    style.spacing.interact_size *= zoom;
    style.spacing.combo_width *= zoom;
    style.spacing.icon_width *= zoom;
    style.spacing.icon_width_inner *= zoom;
    style.spacing.icon_spacing *= zoom;
    ctx.set_style(style);
}

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
