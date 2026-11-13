//! Sizing tokens — dual-sheet, live-toggleable (port of Afterecho's
//! `tokens.rs`, per `TOPOS_TOKENS.md`):
//!
//! * **V3** (default): the numbers Tide's editor is built from today
//!   (theme.rs consts + the measured `mod fig` layout).
//! * **τόπος β**: the 2026-07-13 token-sheet proposal, @2× px ÷ 2 = logical
//!   (control 56 → 28, radius 12 → 6, strokes 1/1.5/2 → 0.5/0.75/1.0, type
//!   body 20 → 10, chip/tab 14 → 7, knob 96 → 48).
//!
//! Toggled live from the hamburger ▸ SIZING so both sheets compare in the
//! running plugin. Process-global atomic (same idiom as the theme atomics) so
//! widget painters read tokens without threading UiState through.
//!
//! Scope note: this is the stage-1 port — pill controls (height / radius /
//! stroke / font / padding), knob default, type, and the two spacings the
//! widgets consume. The absolute `mod fig` card/knob POSITIONS are not
//! re-pitched; the toggle compares how CONTROLS render, not the page grid.
//!
//! All values LOGICAL units (multiply by the zoom scale `s` at use).

use std::sync::atomic::{AtomicBool, Ordering};

static BETA: AtomicBool = AtomicBool::new(false);

pub fn set_beta(v: bool) {
    BETA.store(v, Ordering::Relaxed);
}

pub fn beta() -> bool {
    BETA.load(Ordering::Relaxed)
}

/// Pick current-sheet vs τόπος-sheet value.
#[inline]
fn t(current: f32, topos: f32) -> f32 {
    if beta() {
        topos
    } else {
        current
    }
}

// ── Sizes ────────────────────────────────────────────────────────────────
/// ONE height for everything pill-shaped (chips, toggles, enum selectors).
/// τόπος `control/default` 56 @2× = 28.
pub fn ctl_h() -> f32 {
    t(24.0, 28.0)
}
/// Knob body diameter DEFAULT (the live KNOBS stepper still fine-tunes).
/// V3 = editor.rs KD_DEFAULT (50). τόπος `knob` 96 @2× = 48 (the corrected
/// value — fills the cell body band).
#[allow(dead_code)] // part of the τόπος sheet; the scope layout sizes its own dials
pub fn knob_default() -> f32 {
    t(50.0, 48.0)
}

// ── Radius (follows nesting depth, not element size) ─────────────────────
/// pill/chip controls. τόπος `radius/control` 12 @2× = 6.
pub fn r_ctl() -> f32 {
    t(8.0, 6.0)
}
/// preset slot pill. Same τόπος tier as the other controls.
pub fn r_pill() -> f32 {
    t(9.0, 6.0)
}

// ── Strokes (τόπος: @2× px like everything else) ─────────────────────────
/// control outlines + hairlines. τόπος `stroke/outline` 1.5 @2× = 0.75.
pub fn stroke_outline() -> f32 {
    t(1.0, 0.75)
}
/// active/selected states, indicators. τόπος `stroke/emphasis` 2 @2× = 1.0.
pub fn stroke_emph() -> f32 {
    t(1.4, 1.0)
}

// ── Type ─────────────────────────────────────────────────────────────────
/// knob labels/values + card-chip text. τόπος `type/body` 20 @2× = 10.
pub fn font_body() -> f32 {
    t(8.0, 10.0)
}
/// card titles (chip-on-border tabs). The τόπος sheet said `type/chip` 14 @2×
/// = 7, but 7 read far too small on the rendered cards (owner, 2026-07-13) —
/// doubled to 14 logical. V3 keeps its original 8.5.
pub fn font_label() -> f32 {
    t(8.5, 14.0)
}
/// text inside standard buttons (menu chips, titlebar). τόπος body = 10.
pub fn font_btn() -> f32 {
    t(10.0, 10.0)
}
/// full-size card-chip text (V3 renders BTN_FS−2 = 8). τόπος body = 10.
pub fn font_chip_text() -> f32 {
    t(8.0, 10.0)
}

// ── Padding / spacing ────────────────────────────────────────────────────
/// horizontal text padding inside a pill, TOTAL (both sides).
/// τόπος `control padding` 24 @2× = 12 per side → 24 total.
pub fn pad_x() -> f32 {
    t(22.0, 24.0)
}
/// icon ↔ text, tight internals. τόπος `space/sm` 8 @2× = 4.
pub fn space_sm() -> f32 {
    t(6.0, 4.0)
}
/// standard gap · card padding. τόπος `space/md` 24 @2× = 12.
#[allow(dead_code)] // part of the τόπος sheet; no consumer since the settings-menu rebuild
pub fn space_md() -> f32 {
    t(8.0, 12.0)
}
