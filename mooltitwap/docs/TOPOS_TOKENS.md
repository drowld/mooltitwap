# τόπος — design tokens

**Status:** proposal, live-A/B-testable in Afterecho since v2.22.1
(hamburger ▸ SIZING ▸ V3 / τόπος β). This file is the portable spec — copy it
into any plugin repo (cumulus, merge, y-flanger, …) and port `tokens.rs`
alongside it.

**Reference implementation:** `mooltitwap/src/tokens.rs` (dual-sheet, runtime
toggle). One correction was made against the original sheet during
implementation — see *Corrections* below. With it, the system closes
perfectly: **23 numbers govern the whole plugin.**

---

## Units convention

All values in the tables are **@2× canvas px** (the Figma frames are 2388×1668
@2×). Code works in **logical units = px ÷ 2**, multiplied by the runtime zoom
scale `s` at draw time.

> iPad ÷2 rule: real points = px ÷ 2. 56 = 28 pt, 32 = 16 pt — compacts are
> small for fingers, so keep ≥16 px gaps around them and make the **cell** the
> touch target, not the drawn pill (hit area > visual, à la Cumulus).

---

## Spacing

| Token | @2× | logical | Job |
|---|---|---|---|
| `space/xs` | **4** | 2 | inside cells (label ↔ body ↔ value) |
| `space/sm` | **8** | 4 | icon ↔ text, tight internals |
| `space/dense` | **16** | 8 | gaps in dense rows (TRIGGER tier), compact toolbars |
| `space/md` | **24** | 12 | standard cell gap · card padding · gap between blocks |
| `space/edge` | **32** | 16 | page/plugin edge margin |
| `space/group` | **48** | 24 | between control groups within a row |

## Sizes

| Token | @2× | logical | Job |
|---|---|---|---|
| `control/default` | **56** | 28 | buttons, dropdowns, pill readouts in card rows — ONE height for everything pill-shaped |
| `control/compact` | **32** | 16 | utility tier: editor toolbars (MSEG 1-Shot/Hold/Loop), mode chips, Set A/B |
| `knob` | **96** ✱ | 48 | knob body diameter = the cell's body band (✱ corrected, see below) |
| `cell/width` | **128** | 64 | the control cell (or 112 for a tighter variant — pick one) |
| `cell/height` | **152** | 76 | = 24 label + 4 + 96 body + 4 + 24 value |
| `header` | **48** | 24 | card header row height |
| `tab` | **24** | 12 | chip-on-border title; overhang = **−12** (half its height) |

Derived: `pitch_x = cell/width + space/md` (152 @2× → 76 logical),
`pitch_y = cell/height + space/md` (176 @2× → 88 logical).

## Radius

| Token | @2× | logical | Job |
|---|---|---|---|
| `radius/card` | **16** | 8 | cards, plugin contour |
| `radius/control` | **12** | 6 | the 56-height controls |
| `radius/compact` | **8** | 4 | the 32-height controls, small inner elements |
| `radius/chip` | **6** | 3 | tabs/chips |

Rules: radius follows **nesting depth**, not element size. Concentric
corners: **inner = outer − gap**.

## Strokes

| Token | @2× | logical | Job |
|---|---|---|---|
| `stroke/hairline` | **1** | 0.5 | dividers, grid lines |
| `stroke/outline` | **1.5** | 0.75 | control outlines (buttons, dropdowns, knobs) |
| `stroke/emphasis` | **2** | 1.0 | active/selected states, knob indicator |

## Type

| Token | @2× | logical | Job |
|---|---|---|---|
| `type/chip` | **14** | 7 | tab labels, text inside **compact (32)** buttons |
| `type/body` | **20** | 10 | cell labels, values, text inside **default (56)** buttons |
| `type/voice` | **28** | 14 | the big A/B voice letters |

(All Karla Light, per current design.)

## Control padding

| Tier | @2× H / V | logical H / V |
|---|---|---|
| default (56) | 24 / 12 | 12 / 6 |
| compact (32) | 12 / 6 | 6 / 3 |

---

## The two governing rules

1. **Context picks the size, never the element**: card row → 56 tier;
   toolbar/editor → 32 tier. No per-button decisions.
2. **Hit area > visual**: the cell (or a ≥44 pt expansion) is the touch
   target, never the drawn pill. Keep ≥16 px (@2×) gaps around compact-tier
   controls.

---

## Corrections & clarifications (from the Afterecho implementation, 2026-07-13)

1. **`knob` is 96, not 72.** The original sheet said 72, but its own cell
   formula (`cell/height 152 = 24 + 4 + **96 body** + 4 + 24`) allots a 96
   body band — a 72 knob leaves 12 pt of dead slack in every cell. With
   knob = 96 the formula closes exactly (logical: 12 + 2 + 48 + 2 + 12 = 76),
   and it happens to equal the pre-τόπος v3 knob, which the owner prefers.
2. **Strokes are @2× px like everything else** → 0.5 / 0.75 / 1.0 logical.
   (If they were ever meant 1:1-logical, that's a 3-line change in
   `tokens.rs` — but @2× is what ships.)
3. **Pill minimum width**: τόπος sizes pills by text + padding only; the
   Afterecho port keeps the Figma 98 @2× (49 logical) minimum-width floor so
   short labels ("ON") don't collapse into squares. Recommended for other
   ports too.

---

## Porting recipe (what Afterecho did — `src/tokens.rs`)

- **One module, one fn per token**, each returning the logical value:
  `fn t(current, topos) -> f32` picks the active sheet from a process-global
  `AtomicBool`. UI code calls `tk::knob()`, `tk::ctl_h()`, `tk::space_md()`, …
  and multiplies by the zoom scale `s` at use.
- **Runtime toggle** in the options/hamburger menu (`SIZING ▸ V3 / τόπος β`)
  sets the atomic once per frame from editor state → both sheets compare live
  in one build, no restart. Delete the old sheet once a winner is picked.
- **Compute row ladders, don't hardcode them.** Afterecho's DELAY card:
  rows one `pitch_y` apart, a divider one pitch below the previous row, the
  next row +19 (logical) under the divider. This reproduced the hand-placed
  Figma ladder exactly at the old pitch and reflows automatically at the new
  one.
- **Tokenize in this order** (highest visual leverage first): pill controls
  (height/radius/stroke/font/padding) → cell grids & pitches → knob diameter →
  dividers/hairlines → type. Kit-internal chrome (epo knob label/value type,
  card_tabs) is stage 2 — do it **in epoche-ui itself** so every plugin
  inherits it.
- **Figma side**: enter these as Figma variables (`space/*`, `control/*`,
  `radius/*`, `stroke/*`, `type/*`) at the @2× values and bind while
  componentizing; this table is then `theme.rs` almost line for line.

## Current per-plugin state

| Plugin | Status |
|---|---|
| **Afterecho** (mooltitwap) | dual-sheet `tokens.rs`, live toggle, card grids + all pill controls tokenized; kit internals pending |
| **cumulus / merge / y-flanger** | not started — port `tokens.rs` + this doc |
