# mooltitwap — Architecture

_As of v2.17.1 (2026-07-07), post nice-plug/epoche-ui port._

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
- `gui.rs` — entire editor, **ui-driven** (epo widgets allocate from the egui cursor,
  no manual-rect layout). Header (menu/wordmark/preset pill) · 3 chip-on-border cards
  (DELAY / VIEW+viz / TAPS+tabs) · footer (feedback/FB-time/taps). Custom painters kept
  for the visualizers: stems, grid, XY pads, timeline grid — recolored via `theme::`
  fns + `theme::ui_scale(ui)`. Menu/resize-grip/preset-browser are foreground `egui::Area`s.
- `presets.rs` — named-preset system (GUI-thread only): capture/apply all params via
  `param_map()` + `raw_context.raw_*`, JSON files in
  `~/Library/Application Support/Mooltitwap/presets`.
- `delay.rs` · `diffusion.rs` · `filter.rs` · `topology.rs` — pure DSP, no framework
  deps (untouched by the port).

## Scaling
`zoom = window_width / 1080`, applied via `theme::apply_zoom` each frame; painters
recover it with `theme::ui_scale(ui)`. Window height snaps to the design ratio
(1080×656). Resize grip is aspect-locked.
