# mooltitwap (Afterecho) — Architecture

_As of v2.47.0 (2026-11-01), branch `ui/scope-first`. The why behind each piece is in DECISIONS.md._

## Overview
16-tap multitap delay (VST3 + CLAP + standalone on desktop, AUv3 on iPad) on **nice-plug**
(nih-plug fork, `../../Dev/nice-plug`) with an **egui 0.34** editor. The live page is the
scope-first page (`gui_scope.rs`) on the shared Soft Systems UI (nice-plug `softsystems-ui`,
v2.49: theme, Hanken, controls, the ☰ Options panel, the preset sheet). `src/ui/` is what's
left of Tide's layer: `theme` names the shared palette's colours for the page's painters,
`epo` keeps the knob drive and touch helpers. The old v2/v3 editors and epoche-ui
went in v2.50, with every compatibility path (old sessions, old presets, the old look).

## Data flow (`lib.rs::process`, per sample)
1. Encode channel mode (Stereo / M-S / Mono) → In gain → input colour (Diffuse/Tape/Saturate on In).
2. Ducking's and Motion's followers read the raw dry input.
3. Feedback: read the main lines at the cycle, less Diffuse's delay when it's in a loop (Auto = cluster span, floored at Euclid's
   whole pattern; Manual = its own time). Two loops share it by Ping's share: the main
   loop (L/R) and the bounce (`ping_odd`/`ping_even`, mono, odd/even repeats), crossfaded.
   Repeats as a count (v2.42) replaces the loop's read with N copies read from `dry_l/r`
   (the input as written) at k × the cycle, crossfading with the loop on a switch.
4. Each loop's colour: crossfeed Diffuse → Diffuse/Tape/Saturate on FB → Loop tone →
   safety soft clip (the only bound; no gain compensation). Freeze crossfades to the held
   loop (unity, latched whole-sample length) and keeps new input out.
5. Write input + feedback (and the input alone to `dry_l/r`). Taps read the main and
   bounce lines: per-tap pan, SVF cascade (type crossfade on change), gain — all gliding
   (two 5 ms one-poles; cutoff/Q in log steps). Every 32 samples Motion's matrix re-aims
   each tap's pan/gain/cutoff/Q and the whole echo's Colour/Feedback/Cross/Tone/Mix/Duck.
6. Crossfeed Mix → output colour → Ducking (wet only) → Ear guard (wet only) → Clear's
   fade → dry/wet → decode → Out → limiter. A non-finite sample silences the block and
   clears the delay. The loops' write-back passes 5 Hz DC blockers (not Freeze's hold).

## Key modules (src/)
- `lib.rs` — plugin impls, `process`, engine state, tests (incl. click and A/B-style checks).
- `params.rs` — `PetalParams` (+ `TapParams[16]`, `LfoParams` ×3, `StepParams` ×2, `MxRow` ×6), enums (`MotionShape`, `MOTION_DIVS`…),
  helpers shared by engine and page (`spacing`, `taps_heard`, `pattern_secs`,
  `motion_depths`, `motion_hz`), and plain atomics the engine writes for the page:
  `held_cycle_secs`, `duck_dip_db`, `motion_now[4][16]` (per tap target; tempo is a separate `Arc<AtomicU32>`).
- `topology.rs` — tap positions (`Spacing`, `compute_positions`, `euclid_steps`, swing),
  `tap_delay_secs`, `cluster_span_secs`: one copy for engine and page.
- `bounce.rs` — Ping placement (width, swing of panned taps). `motion.rs` — LFO shapes
  and the tap targets' maths. `matrix.rs` — the Motion matrix (LFO 1–3, Follow, Step 1–2 × targets, sums,
  whole-echo conversions, Step). `filter.rs` — `Svf`, `LoopTone`, `soft_clip`, `PeakLimiter`.
- `detune.rs` — per-tap detune (v2.45; a % of Detune Range, 1–12 st, v2.46): `Wsola`
  (Light: grains, a tap's sides spliced together) and `Pv` (Clean: phase vocoder, frames
  from the tap's lines), each with a `min_delay`; the engine crossfades plain ↔ shifted
  and Light ↔ Clean per tap (`TapLines`).
- `delay.rs` — `DelayLine` (Lagrange read; `read_before_write` for the loops' and Repeats' reads, v2.44),
  `TapReader` (Repitch / Fade / Jump), `SlewedTime`. `diffusion.rs` (v2.47: six short drifting
  allpasses, L≠R; the loops read early by its `bulk` delay), `tape.rs` (RE-DEEMER port).
- `presets.rs` — Afterecho's half of the shared preset store (nice-plug
  `softsystems-presets`, v2.48): `Store` implements its `Plugin` (which params a preset
  holds, plain floats, the Time/Cross facets, the factory bank); `Library` = the crate's
  (factory from the binary + user files in `Soft Systems/Afterecho/Presets`,
  `library.json`). `PresetSession` (the crate's `PresetRef`) is persisted with the host.
  `NOT_STORED` (Freeze, Ear guard) stays out of presets and A/B. `factory.rs` — the factory set as changes
  from Init.
- `gui.rs` — `create_editor` (the scope page), `EditorState`, A/B (`ab_switch`) and
  `compute_viz_data_ex` (the timeline maths the page draws from).
- `gui_scope.rs` — the page: header (Freeze, A/B), the field/Bars with the Y list and the
  Taps menu (pattern tools), the lane (Echo with Repeats and Loop rows; Motion, the
  matrix, across the whole lane; Tap), the Time card (Time | Pattern, Euclid ring,
  Swing), Colour, Output (Duck), the ☰ Options panel's own rows (Accent, Detune, the preset
  folder). `PRODUCT` describes Afterecho to the shared UI; the look is saved in
  `Soft Systems/Afterecho/appearance.json` (+ `accent.json`), imported once from
  `Mooltitwap/appearance.json`.
  `gui_scope/preset_sheet.rs` — the shared preset box and sheet (`softsystems_ui::preset_sheet`),
  over the box round all the cards while open; `PresetUi` keeps the library, the store's
  `Session` and the sheet's view, and syncs the host-session field.

## Layout and scaling
One page, an absolute layout in logical px: 1366×660 (`LAYOUT`; Tall went in v2.51).
Desktop: zoom = window width ÷ page width; height keeps the aspect. iPad host panel:
fitted and centred in the space the host gives. The page repaints every frame.
