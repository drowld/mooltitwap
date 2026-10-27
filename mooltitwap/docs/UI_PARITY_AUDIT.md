# Mooltitwap Editor — UI Placement Audit

_New ui-driven editor (`src/gui.rs`, v2.17.1) vs the pre-port manual-rect reference
(`be061d8`, v2.15). Generated 2026-07-10 by a region-by-region old↔new comparison
(6 regions mapped, each adversarially verified restyle-vs-misplacement, then
synthesized). Two high findings independently re-verified by hand._

> ✅ **RESOLVED 2026-07-10 (v2.17.2).** All 9 findings below were fixed and each was
> visually verified in the running standalone (default, FILTER, XFEED and GRID views
> screenshotted). H2 (per-tap mute) was confirmed an oversight by the owner and
> restored. See DECISIONS.md 2026-07-10.

## Verdict
The nice-plug + epoche-ui port faithfully preserved control **order and grouping**
everywhere (the DELAY card sequence is identical, the TAPS/XFEED/DIFFUSE order is
intact). But it **dropped two reachable controls entirely** and shifted several
elements out of their hand-tuned positions as auto-layout byproducts. Counts:
**2 high · 5 medium · 2 low.**

---

## HIGH — dropped controls (functionality lost, not just moved)

### H1 · CENTER — SHAPE link toggle removed
- **Old:** a 56px right column in the shaping strip held a 🔗 button that toggled
  `params.shape_linked` (`be061d8:gui.rs:1471–1507`); the two XY pads split
  `(width − 56)/2`.
- **Now:** `draw_center_body` (`src/gui.rs:636–646`) draws only the two pads at 50/50.
  `shape_linked` is **read-only repo-wide** (reads at `gui.rs:643, 782`; no setter).
  It defaults to `true` (`params.rs:368`), so **SHAPE R is permanently dimmed and
  cannot be unlinked from the UI.**
- **Fix:** In `draw_center_body`, reserve `link_w = 56.0*s` on the right, set
  `pad_w = (strip.width() − link_w − 2*pad_gap)*0.5`, and add a clickable toggle wired
  to `begin/set/end_set_parameter(&params.shape_linked)` (mirror the OLD 1478–1507
  block, or reuse the TIME `epo::toggle` pattern already used at `src/gui.rs:519`).

### H2 · TAPS — per-tap active/mute toggle removed
- **Old:** every tap row had a clickable dot toggling `tap.active`
  (`tap_row_slider` `be061d8:2000–2012` for PAN/GAIN; `tap_row_filter:1725–1727` for
  FILTER).
- **Now:** `draw_right_body` (`src/gui.rs:1085–1165`) emits no per-tap active control.
  `tap.active` is **read only** (opacity dimming at `935, 959, 1033`); only the global
  `num_taps_active` count is settable (footer). **Per-tap mute is unreachable.**
- **Fix:** add a small clickable mute affordance per tap wired to `params.taps[i].active`
  — for FILTER prepend a clickable dot / `chip_sm` in the row `ui.horizontal`; for
  GAIN/PAN wrap `epo::slider` in a `ui.horizontal` led by the toggle.
- **Note:** verify this wasn't a deliberate simplification — nothing in DECISIONS.md
  records dropping it, so it reads as an oversight, but it's the owner's call.

---

## MEDIUM — real mispositioning / lost information

### M1 · HEADER — preset pill biased right of true centre
The "centered preset pill" is centered in the leftover space *right of the left
cluster*, not on the header midpoint (`src/gui.rs:481` pads by
`((available_width − pill_w)*0.5 − SP_LG)` where `available_width` already excludes the
left cluster). Old centred on `rect.center()` (`be061d8:383`). **Fix:** center against
full header width — capture `full_w`/`left0` before the left cluster, then
`pad = ((full_w − pill_w)*0.5 − consumed).max(0.0)`.

### M2 · LEFT — hero TIME knobs lost size hierarchy
Old hero knobs were `s.f(50.0)` vs section knobs `s.f(38.0)` (~1.32× emphasis;
`be061d8:466` vs `581/602`). Now all use `kd = KNOB_DIAM_FX` (40px) — flat. **Fix:**
pass a larger diameter (e.g. `KNOB_DIAM_INSPECT`≈48px) to the three hero `knob_ex`
calls at `src/gui.rs:518, 521, 523`; leave LEVELS/WOBBLE on `kd`.

### M3 · CENTER — "dB" axis caption missing in GRID viz
Old drew a "dB" header above the amplitude scale (`be061d8:1364–1365`); the new
`draw_grid_viz` goes straight from the dB loop (`src/gui.rs:1024`) to the timeline.
**Fix:** after the loop add
`p.text(Pos2::new(rect.min.x+pad_l−6.0*s, draw_t−6.0*s), Align2::RIGHT_BOTTOM, "dB", FontId::new(8.0*s, FontFamily::Monospace), theme::FAINT())`.

### M4 · XFEED — crossfeed knob lost its mode-dependent caption
Old set the caption to BLEND / BOUNCE / DEPTH per `crossfeed_mode`
(`be061d8:1952–1956`); new hardcodes `"AMOUNT"` (`src/gui.rs:1138`) though `cur` is
already read at 1134. Diffuse kept its semantic captions, so this is inconsistent.
**Fix:** `match cur { Mix=>"BLEND", PingPong=>"BOUNCE", Diffuse=>"DEPTH" }`.

### M5 · FOOTER — even 3-column layout replaced by flush-left + right cluster
Old split the footer into three equal thirds (FEEDBACK ~1/6, FB-TIME 1/2, TAPS 5/6
with dividers; `be061d8:2082–2091, 2133–2138`). Now FEEDBACK is flush-left and
everything else packs against the right edge via one `right_to_left` block
(`src/gui.rs:1207`), leaving the center empty. **Fix:** lay three groups with flexible
spacers so they land near 1/6, 1/2, 5/6.

---

## LOW

### L1 · LEFT — SATURATION section header dropped
SOFT CLIP is the only left-card control with no `section_label`, breaking parity with
the five other groups (old wrapped it in a "SATURATION" section, `be061d8:622`).
**Fix:** add `section_label(ui, "SATURATION", s);` before `src/gui.rs:582`.

### L2 · FOOTER — FB-TIME mode chip abuts TAPS
`right_to_left` insert order renders L→R as FB-TIME · LOCK · SYNC/FREE · TAPS, so the
mode chip sits next to the unrelated TAPS control and LOCK splits the FB-TIME group.
**Fix:** reorder inserts (`src/gui.rs:1211–1255`) so FB-TIME + mode + lock stay one
adjacent sub-group, separated from TAPS by a spacer.

---

## Suggested fix order (fast wins first)
1. M4 crossfeed caption (`1138`) — one-line `match`.
2. M3 "dB" caption (`1024`) — one-line `p.text`.
3. L1 SATURATION header (`582`) — one-line `section_label`.
4. M2 hero knob size (`518/521/523`) — larger diameter constant.
5. M1 preset pill centering (`481`) — recompute against full width.
6. M5 + L2 footer (`1200–1281`) — one layout rework fixes both.
7. H1 SHAPE link toggle (`637–646`) — restore control + param setter.
8. H2 per-tap mute (`1085–1165`) — largest; per-row affordance × 3 tab views.

---

## Considered and dismissed (intentional port restyle, per DECISIONS.md 2026-07-07)
Not flagged as bugs: spirograph brand-mark → hamburger+wordmark header idiom;
viz/shaping fixed↔flex inversion (consequence of the resizable window); FILTER
cutoff/Q knobs → horizontal sliders (documented, fits one row); FILTER SLOPE chip
reorder; crossfeed knob 64→48px (shared `KNOB_DIAM_INSPECT`); XFeed tab title/desc
dropped symmetrically; mode/routing seg-buttons → hug-width chips; knob captions
moved above the knob (kit convention); Diffuse knobs centered vs 1/3–2/3 spread;
FB-TIME lock wide button → inline `chip_sm`. The FILTER Q-slider width formula is a
latent weakness that never triggers at shipped sizes (optional hardening only).
