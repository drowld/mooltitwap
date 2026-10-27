//! Sizing tokens — the v3 editor's metric system, with two selectable sheets:
//!
//! * **V3** (default): the numbers the current editor was built from (the
//!   Figma "Afterecho" frame, 2388×1668 @2× ÷ 2).
//! * **τόπος β**: the owner's 2026-07-13 token-sheet proposal, same ÷2
//!   convention (56 @2× = 28 logical, knob 72 = 36, gaps md 24 = 12, …).
//!   Strokes interpreted as @2× canvas px like the rest of the sheet
//!   (1/1.5/2 → 0.5/0.75/1.0 logical).
//!
//! Toggled live from the hamburger menu ▸ SIZING so the two sheets can be
//! compared side by side in the running plugin. Process-global atomic, same
//! idiom as the theme atomics — the row painters read tokens without
//! threading EditorState everywhere.
//!
//! All values are LOGICAL units (multiply by the zoom scale `s` at use).

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
    if beta() { topos } else { current }
}

// ── Spacing ──────────────────────────────────────────────────────────────
/// inside cells (label ↔ body ↔ value)
pub fn space_xs() -> f32 { t(4.0, 2.0) }
/// icon ↔ text, tight internals
pub fn space_sm() -> f32 { t(8.0, 4.0) }
/// gaps in dense rows, compact toolbars
pub fn space_dense() -> f32 { t(8.0, 8.0) }
/// standard cell gap · card padding · gap between blocks
pub fn space_md() -> f32 { t(4.0, 12.0) }
/// page/plugin edge margin
pub fn space_edge() -> f32 { t(14.0, 16.0) }
/// between control groups within a row
pub fn space_group() -> f32 { t(24.0, 24.0) }

// ── Sizes ────────────────────────────────────────────────────────────────
/// ONE height for everything pill-shaped in card rows (56 @2× = 28)
pub fn ctl_h() -> f32 { t(27.0, 28.0) }
/// utility tier: toolbars, mode chips (32 @2× = 16)
pub fn ctl_compact_h() -> f32 { t(27.0, 16.0) }
/// minimum pill width (τόπος sizes by padding; keep the Figma floor)
pub fn btn_min_w() -> f32 { t(49.0, 49.0) }
/// knob body diameter. The τόπος sheet said 72 @2× (= 36), but its own cell
/// formula (152 = 24 label + 4 + **96 body** + 4 + 24 value) allots a 96
/// body band — so the knob is 96 @2× = 48: the original v3 size, exactly
/// filling the cell's body slot (owner decision 2026-07-13: keep the v3
/// knob, grid-aligned).
pub fn knob() -> f32 { t(48.0, 48.0) }
/// control cell width (128 @2× = 64)
pub fn cell_w() -> f32 { t(65.0, 64.0) }
/// control cell height (152 @2× = 76 = 24 label + 4 + 96 body + 4 + 24 value)
pub fn cell_h() -> f32 { t(75.0, 76.0) }
/// horizontal cell pitch = cell + gap
pub fn pitch_x() -> f32 { cell_w() + space_md() }
/// vertical row pitch = cell + block gap
pub fn pitch_y() -> f32 { cell_h() + 12.0 }

// ── Radius (follows nesting depth, not element size) ─────────────────────
pub fn r_card() -> f32 { t(10.0, 8.0) }
pub fn r_ctl() -> f32 { t(8.0, 6.0) }
pub fn r_compact() -> f32 { t(8.0, 4.0) }
pub fn r_chip() -> f32 { t(8.0, 3.0) }

// ── Strokes ──────────────────────────────────────────────────────────────
/// dividers, grid lines
pub fn stroke_hair() -> f32 { t(1.0, 0.5) }
/// control outlines (buttons, dropdowns, knobs)
pub fn stroke_outline() -> f32 { t(1.0, 0.75) }
/// active/selected states, indicators
pub fn stroke_emph() -> f32 { t(2.5, 1.0) }

// ── Type ─────────────────────────────────────────────────────────────────
/// tab labels, text inside compact controls (14 @2× = 7)
pub fn font_chip() -> f32 { t(10.0, 7.0) }
/// cell labels, values, text inside default controls (20 @2× = 10)
pub fn font_body() -> f32 { t(10.0, 10.0) }
/// the big A/B voice letters (28 @2× = 14)
pub fn font_voice() -> f32 { t(12.0, 14.0) }

// ── Control padding ──────────────────────────────────────────────────────
/// horizontal text padding inside a default-tier pill (24 @2× = 12)
pub fn pad_h() -> f32 { t(8.0, 12.0) }
/// horizontal padding inside a compact-tier pill (12 @2× = 6)
pub fn pad_h_compact() -> f32 { t(8.0, 6.0) }
