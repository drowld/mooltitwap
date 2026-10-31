# mooltitwap (Afterecho) — Architecture

_As of v2.41.0 (2026-10-31), branch `ui/scope-first`. The why behind each piece is in DECISIONS.md._

## Overview
16-tap multitap delay (VST3 + CLAP + standalone on desktop, AUv3 on iPad) on **nice-plug**
(nih-plug fork, `../../Dev/nice-plug`) with an **egui 0.34** editor. The live page is the
scope-first page (`gui_scope.rs`) on a local copy of Tide's UI layer (`src/ui/`: `epo`,
`theme`, `tokens`); the old v2/v3 editors still compile (epoche-ui) and are to be retired.

## Data flow (`lib.rs::process`, per sample)
1. Encode channel mode (Stereo / M-S / Mono) → In gain → input colour (Diffuse/Tape/Saturate on In).
2. Ducking's and Motion's followers read the raw dry input.
3. Feedback: read the main lines at the cycle (Auto = cluster span, floored at Euclid's
   whole pattern; Manual = its own time). Two loops share it by Ping's share: the main
   loop (L/R) and the bounce (`ping_odd`/`ping_even`, mono, odd/even repeats), crossfaded.
4. Each loop's colour: crossfeed Diffuse → Diffuse/Tape/Saturate on FB → Loop tone →
   safety soft clip (the only bound; no gain compensation). Freeze crossfades to the held
   loop (unity, latched whole-sample length) and keeps new input out.
5. Write input + feedback. Taps read the main and bounce lines: per-tap pan, SVF cascade
   (type crossfade on change), gain — all gliding (two 5 ms one-poles; cutoff/Q in log
   steps). Motion re-aims pan/gain/cutoff/Q every 32 samples.
6. Crossfeed Mix → output colour → Ducking (wet only) → dry/wet → decode → Out → limiter.

## Key modules (src/)
- `lib.rs` — plugin impls, `process`, engine state, tests (incl. click and A/B-style checks).
- `params.rs` — `PetalParams` (+ `TapParams[16]`), enums (`MotionShape`, `MOTION_DIVS`…),
  helpers shared by engine and page (`spacing`, `taps_heard`, `pattern_secs`,
  `motion_depths`, `motion_hz`), and plain atomics the engine writes for the page:
  `held_cycle_secs`, `duck_dip_db`, `motion_now[16]` (tempo is a separate `Arc<AtomicU32>`).
- `topology.rs` — tap positions (`Spacing`, `compute_positions`, `euclid_steps`, swing),
  `tap_delay_secs`, `cluster_span_secs`: one copy for engine and page.
- `bounce.rs` — Ping placement (width, swing of panned taps). `motion.rs` — Motion shapes
  and target maths. `filter.rs` — `Svf`, `LoopTone`, `soft_clip`, `PeakLimiter`.
- `delay.rs` — `DelayLine` (Lagrange read; fractional reads are backwards, SOF-60),
  `TapReader` (Repitch / Fade / Jump), `SlewedTime`. `diffusion.rs`, `tape.rs` (RE-DEEMER port).
- `presets.rs` — the Soft Systems preset library (Vortex's port): the file (format 2,
  plain discrete values, Time/Cross facets), `Library` (factory from the binary + user
  files in `Soft Systems/Afterecho/Presets`, `library.json`, one-time copy from
  `Mooltitwap/presets`), `PresetSession` (persisted with the host). `NOT_STORED`
  (Freeze) stays out of presets and A/B. `factory.rs` — the factory set as changes
  from Init.
- `gui.rs` — `create_editor` (routes to the scope page when `SCOPE_PAGE`),
  `compute_viz_data_ex` (shared viz maths), the old v2 editor. `gui_v3.rs`, `palette_v3.rs` — old v3.
- `gui_scope.rs` — the page: header (Freeze, A/B), the field/Bars with the Y list and the
  Taps menu (pattern tools), the lane (Echo / Motion / Tap tabs, Cross and Loop rows),
  the Time card (Time | Pattern, Euclid ring, Swing), Colour, Output (Duck), Settings.
  `gui_scope/preset_sheet.rs` — the header's preset box and the preset sheet, drawn in
  place of the field and the lane while open (state in `PresetUi`: Try, edited, session).

## Layout and scaling
Two page shapes, absolute layouts in logical px: Wide 1366×660 and Tall 1050×780
(`Shape::pick` chooses on iPad). Zoom = window width ÷ page width; height keeps the
aspect. The page repaints every frame.
