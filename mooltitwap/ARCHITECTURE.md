# mooltitwap — Architecture

_As of v2.17.3 (2026-07-10), post nice-plug/epoche-ui port + v2.15 layout-parity pass._

## Overview
16-tap multitap delay "garden" (VST3 + CLAP + standalone) built on **nice-plug**
(a nih-plug fork at `../../Dev/nice-plug`) with an **egui** editor that renders
through the shared **epoche-ui** kit (`../../Dev/epoche-ui` — theme + `epo` painted
widgets + Space Grotesk/Spline Mono fonts). egui pinned to 0.34.1.

## Data flow
`process()` (lib.rs): encode channel mode → input gain/soft-clip/input-diffuse →
read feedback from the **main ring buffer** at `cluster_span` back, scale by
`feedback`, re-inject at write point → read N taps (per-tap SVF + gain + pan) →
sum → output-diffuse → dry/wet → decode → output gain → limiter. Feedback is a
time-aligned repeat of the whole cluster (never a tap-sum). Host tempo published
to the GUI via an `AtomicU32`.

## Key modules (src/)
- `lib.rs` — Plugin/Clap/Vst3 impls, DSP process loop, state; `nice_export_*` macros.
- `params.rs` — `PetalParams` + per-tap `TapParams[16]` (`#[nested(array)]`), enums,
  time helpers; `#[persist]` editor_state + theme_dark.
- `gui.rs` — entire editor. Header (menu/wordmark/centered preset pill) · 3 `epo::card`/
  `card_tabs` (DELAY / VIEW+viz / TAPS+tabs). **Layout reproduces v2.15 absolute
  geometry inside the kit card bodies** (v2.17.3): each body `allocate_exact_size`s the
  shared `CARD_BODY_H` rect, then places epo widgets at computed sub-rects via
  `ui.new_child(UiBuilder::max_rect(..))` — LEFT uses a fixed `section` rhythm (88px hero
  + dividers, `left_section`/`knob_col`), CENTER pins a `SHAPING_H`=118 strip to the
  bottom with the viz well above, and the **footer lives INSIDE the right card**'s bottom
  `RIGHT_FOOTER_H`=70 band (`draw_incard_footer`, 3 columns). Custom painters for the
  visualizers: stems, grid, XY pads, timeline grid, shape-link — recolored via `theme::`
  + `theme::ui_scale(ui)`. Menu/resize-grip/preset-browser are foreground `egui::Area`s.
- `presets.rs` — named-preset system (GUI-thread only): capture/apply all params via
  `param_map()` + `raw_context.raw_*`, JSON files in
  `~/Library/Application Support/Mooltitwap/presets`.
- `delay.rs` · `diffusion.rs` · `filter.rs` · `topology.rs` — pure DSP, no framework
  deps. **topology.rs is the shared position/time math** (v2.17.4): `compute_positions`,
  `tap_delay_secs`, `tap_fits`, `cluster_span_secs`, `MIN_FB_SECS` — called by BOTH
  `lib.rs` (×sr) and `gui.rs::compute_viz_data`, so viz and audio cannot drift.
  Wobble (params + per-sample LFO) was removed entirely in v2.17.5.

## Scaling
`zoom = window_width / 1080`, applied via `theme::apply_zoom` each frame; painters
recover it with `theme::ui_scale(ui)`. Window height snaps to the design ratio
(1080×660). Resize grip is aspect-locked.
