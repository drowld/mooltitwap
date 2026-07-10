# mooltitwap — Decisions

_Newest first. Each entry: date · what we decided · why · alternative rejected._

## 2026-07-07 · Port to nice-plug + epoche-ui instead of hand-restyling (Path B)
Migrated the framework from nih-plug → nice-plug + nice-plug-egui and rebuilt the
editor on the shared **epoche-ui** kit (`epo::knob/slider/chip/card`, `theme::`).
**Why:** two hand-built restyles (v2.14 flat-token, v2.15 kit-CSS) were rejected as
"ghosty"/"wrong" — approximating ~15 widgets compounds error. Using cumulus's actual
widget code makes the look correct by construction, and the roadmap (resize/presets/
menu/theme) already exists there to copy. **Rejected:** continuing Path A (hand-port
palette+widgets on nih-plug/egui 0.31) — would still diverge and rebuild every roadmap
feature from scratch. **Cost paid:** nice-plug is a near-verbatim nih-plug fork, so
params/lib were mechanical renames; the real work was the ui-driven gui.rs rewrite +
egui 0.34 bump. Visual target is **running cumulus**, not the Inertia kit CSS (they differ).

## 2026-07-07 · Per-tab tap colors via new `Accent::Raw(Color32)` in epoche-ui
Each right-panel tab has an identity hue (PAN teal / GAIN green / FILTER amber /
XFEED coral / DIFFUSE violet); tap rows gradient bright→light within it. Added an
additive `Accent::Raw` variant to epoche-ui (committed `d43c2fa`) since epo widgets
only took the fixed Accent enum. **Rejected:** the 6-hue `Accent::Head` ring (repeats
every 6 over 16 taps) and a single global violet gradient (user asked for per-tab).

## 2026-07-07 · Theme choice persisted via `#[persist] theme_dark` param
Dark/light survives session reload through a persisted `Arc<AtomicBool>` param +
seed-once/apply-if-changed shadow pattern in the update closure (ported from cumulus).
**Rejected:** GUI-only state (would reset to dark every reopen).

## 2026-07-07 · Viz x-axis range floors lowered (adapt in both modes)
`pick_nice_range` floors dropped from 8 beats / 2 s to 1 beat / 250 ms so short
clusters fill the view. **Why:** the old floor meant sync-mode clusters never adapted.
