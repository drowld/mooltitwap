# Mooltitwap — Developer Handoff

**What it is:** A 16-tap multitap delay "garden" (VST3 + CLAP + standalone), inspired
by Rainbow Circuit's *Petal* and Cluster Delay. Rust.

---

## ⭐ CURRENT STATE — v2.39.0 · 2026-10-29 · branch `ui/scope-first`

**Worked on (2026-10-28 → 29):** the scope-first page (new look, spacing, Bars
tap/double-tap mute, preset browser restyle), Ping as a mono bounce (v2.32), then all
six Phase 4 functions: Loop tone (v2.34), Freeze (v2.35), Ducking (v2.36), Pattern
tools + Reso/Type axes + per-tap glides (v2.37), Euclid + Swing (v2.38), Motion
(v2.39). Each has a DECISIONS.md entry.

**Current state:** v2.39.0 committed locally (`b88320d`, nothing pushed) and installed
on desktop (CLAP + VST3). 56 unit tests pass; every feature, when off, is bit-identical
to the version before it (21-render A/B). **Nothing heard by ear yet** — the owner is
testing it all at once. The iPad is still on v2.34.0.

**Problems / open:**
- `DelayLine::read` interpolates fractional delays backwards (SOF-60): small jumps in
  Repitch glides; the feedback cycle runs one sample long. Not fixed.
- Motion's CPU on the iPad not measured (filters recomputed while Cutoff/Reso move).
- iPad installs refuse while the iPad is locked; screenshots need the Mac unlocked, and
  the standalone's window can open off-screen (find it with CGWindowList `.optionAll`).

**Next session:** fold in the owner's hand-test notes, then fix SOF-60 (own version +
A/B). **Estimated next task:** SOF-60.

**How things were checked (reuse):** an A/B test renders 21 cases (every mode, colour
routing, channel mode, curve, self-oscillation, mode switching) on a worktree of HEAD
and on the working tree, compared byte for byte; click tests compare the sharpest
second difference against held baselines, each with a control that disables the fix.
Previews: `AFTERECHO_PREVIEW=tall,bars,gain|cutoff|reso|type,sel3,tap|motion,lock,presets,light,tape,pattern`
(params can't be set by token: flip a default, build, shoot, restore).

## PRIOR STATE — v2.27.0 · 2026-07-14 · branch `nice-plug-port`

**The plugin is απόηχος / Afterecho** (display name; the `mooltitwap` crate/folder
rename is deferred — see TASKS). It runs as **VST3 + CLAP on desktop AND as a real AUv3
on the physical iPad** (Bussien team `6ABFQC732N`, bundle `com.tlkm.afterecho`).
v2.27.0 is installed to `~/Library/Audio/Plug-Ins` (CLAP+VST3) and pushed+launched
on the iPad.
> **All of §1–§11 far below predate v2.18** and describe the nih-plug era; trust
> ARCHITECTURE.md + DECISIONS.md for anything structural. This block + DECISIONS.md
> were the source of truth for v2.18→v2.27.

### What changed 2026-07-14 (v2.25.0 → v2.27.0)
All editor/UX; no DSP changes. **Still desktop-screenshot-verified only** — the iPad
multitouch feel and a DAW-by-ear pass remain the standing gate.

**Multitouch (v2.25.0)** — every discrete tap target now works with several fingers at
once on the iPad. `gui_v3::tap_or_click` wraps the touch/desktop split (kit `chip_rt`
pattern: on touch, claim the synth pointer over the cell + detect the tap from its OWN
`epo::tap_touch` finger; desktop keeps the ordinary `clicked()`). Routed through it:
`v3_button` (all chips/toggles/modes/A-B/curve/lock/sync/link), both mute pills, close-×,
hamburger, sun. **Desktop behavior is byte-identical** (only `theme::touch()` builds change).
*Not yet multitouch (still `ui.interact`, the separate handoff item):* PAN/GAIN tracks,
GRID/STEMS tap dots, XY SHAPE pads — those are drag surfaces needing a raw-touch
horizontal-drag helper.

**DELAY card centering (v2.25.0)** — `draw_v3_left` was left-anchored at a fixed 13px
inset; now it centers the 3-cell grid on the card like every other card (`col_x =
card.center − grp_w/2`, `grp_w = 2·pitch + cell`).

**GRID/STEMS viz overhaul (v2.26.0)** — the three owner viz/UX ideas landed:
- **Tap panel moved into the SHAPE card** (`draw_strip_tap_panel`) — selecting a dot fills
  the SHAPE card (tag reads `TAP NN`) instead of a popup floating over the plot (killed the
  bottom-edge occlusion). `‹ ›` step through active taps (wrap); mute · VOL · **PAN** · CUT ·
  Q; TYPE + SLOPE are single **click-to-cycle** buttons. × restores the XY pads.
- **SNAP chip** (both views, `EditorState::snap3`) quantizes a dragged tap's time to base-time
  multiples (`snap_offset`: quantize `pos+offset` to 1/NUM_TAPS since span = NUM_TAPS·base).
  A per-drag raw-offset accumulator (`ui.memory` `*_virt`) avoids stickiness.
- **Filter-mode glyph** — `filter_glyph` (LP ¯\ · BP /¯\ · HP /¯) painted on every TYPE
  button (`v3_button_glyph`), tap panel + FILTER page.
- **Feedback ghost in GRID** — first repeat ghosted faint, drawn only if it lands in the
  existing taps-only range (no x-range rescale, so temporal mapping is unchanged).

**M/S PAN + Mono (v2.27.0)** — `draw_pan_track` is channel-mode aware: **M/S** shows a
MID(left)↔SIDE(right) balance (fill from the left, no center detent, readout `MID/M·S/SIDE`);
Stereo unchanged (bipolar L/C/R). **Mono drops the PAN tab entirely** (tab list is
`GAIN·FILTER·SPACE`, right_tab coerces Pan→Gain) and **greys the panel PAN knob**
(`disabled_knob` placeholder). Panel PAN readout is mode-aware too.

**Accent → violet (v2.27.0)** — `gui_v3::draw` calls `theme::set_accent(5)` so `G()` /
`accent_default()` = violet (`#a086e8` = `accent(2)`, the tap color). Fixes the preset
palette (search icon, save-as icon+label, tag-input border), the header preset name on
hover, and the header accent dot — all were the kit-default green.

**OLED + high-contrast (v2.27.0, kit change)** — `epoche-ui/theme.rs` gained (additive,
cumulus-safe): `DARK_OLED` palette = pure-black paper (SHADE ▸ **OLED**, dark_variant 3,
clamp bumped 2→3); a `HIGH_CONTRAST` atomic + `boost()` (brighter ink family + stronger
hairlines) with `set_high_contrast`, wired to a hamburger **CONTRAST** toggle
(`EditorState::high_contrast`, pushed each frame). Both are **session-state, not persisted**
(like `dark_variant`/`sizing_beta`) — they reset on reload.

**New:** `EditorState::{snap3, high_contrast}`; `gui_v3::{tap_or_click, v3_button_glyph,
filter_glyph, disabled_knob, snap_offset, draw_strip_tap_panel}`; theme `DARK_OLED` +
high-contrast. **Default change:** `slider_steps` 0→2 (PAN/GAIN tracks 1.5× by default).
**Removed:** `field_panel_rect` / `draw_field_panel` (the old floating popup).
**Uncommitted in the kit:** `set_title_override` (v2.21) + OLED/high-contrast (v2.27).

### What changed this session (v2.18.1 → v2.24.2) — see DECISIONS.md for the "why" on each
**DSP / feedback**
- **v2.18.1** — fixed the "digital mash" on delay-mode changes: base-time/sync/tempo
  moves now route through `TapReader::set_nominal` so Repitch/Fade/Jump actually govern
  the transition (the old v2.12 "Guard 1" hard-snapped every reader every block). Repitch
  glide is now bounded (`max(0.02, dist/100ms)`). Feedback head follows the same rules →
  FB TIME responds fast instead of gliding for ~90 s.
- **v2.18.2** — **in-loop soft saturation** (`2·tanh(x/2)` just before the buffer write;
  the loop's only bound, no gain-comp per project rule) → feedback ≥1.0 self-oscillates
  bounded instead of detonating. With that, the **unlocked-FB-TIME cluster-span floor was
  removed** (short FB times are now a usable dub-wash zone).
- **v2.19.0** — **FB TIME follows the main delay SYNC** (`feedback_time_seconds` reads
  `sync_mode`; the separate `feedback_time_mode` param is deprecated-but-kept).
- **v2.20.0 / v2.21.0** — **FEEDBACK CURVE** (Natural / Bloom / Gate): envelope-follower
  inside the loop bends the effective loop gain by level (Bloom = comp-in-loop plateau,
  Gate = expander-in-loop hold-then-cut). Chips live on the **DELAY card**, between
  FEEDBACK and TAPS COUNT (v2.21 moved them off the SPACE page). Viz echo bars follow the
  curve (`cycle_gains[]` in `compute_viz_data`).
- **SATURATE block** (v2.20) — playable tanh DRIVE stage, routable IN/FB/OUT like DIFFUSE.
- **DEGRADE block** — **v2.19 shipped a bitcrush (wrong algo), v2.21 replaced it with a
  real TAPE model** ported from **RE-DEEMER** (`~/re-deemer`, ISC) into `src/tape.rs`:
  J-A hysteresis (RK4, 2× oversampled, Chowdhury DAFx-19) + record/repro heads EQ +
  HF self-erasure. Params: TYPE I/II/IV · DRIVE (−12..+24 dB, full makeup) · WEAR ·
  AMOUNT · routing (FB routing = generation loss). 3 unit tests pass. `src/crush.rs`
  deleted. The DELAY-card SATURATION toggle was removed (`soft_clip` param deprecated).

**GUI / layout**
- **SPACE page** now has five sections: XFEED · DIFFUSE · DEGRADE · SATURATE (+ FB CURVE
  moved out to the DELAY card in v2.21).
- **Center card is now a `card_tabs` card: VIEW · GRID · STEMS · ECHO** (v2.24):
  - **GRID** = time × pan plane. Drag a dot: X = `time_offset`, Y = pan (L top / R bottom,
    relabels MID/SIDE in M/S mode). Dot **size = tap gain**. No baseline bar (v2.24.2).
  - **STEMS** = time × volume lollipops. Drag Y = gain (DAW-fader skew, dB gridlines),
    X = time. Fixed dot size. Ghosts only the **first** feedback repeat (v2.24.1).
  - **ECHO** = the classic read-only L/R bar view w/ full feedback tail (`draw_stems`).
  - **Tap panel**: click a dot → bottom-center inline card (mute pill · [GAIN in GRID /
    PAN in STEMS — whichever isn't on the axes] · CUT · Q · LP/BP/HP · 12/24/48 · ×).
  - **Selection sync**: editing a tap row in the TAPS card (mute/PAN/GAIN track/FILTER
    chips) selects that tap in the view (via `LAST_TOUCHED` atomic → `st.field_sel`).
    *Not yet:* CUT/Q knob drags in FILTER blocks (epo knobs return nothing hookable).
  - State: `EditorState::view3` (u8: 0 GRID / 1 STEMS / 2 ECHO). `viz_mode` is now
    legacy-v2-editor only. `compute_viz_data_ex(.., shown_cap)` parameterizes the echo
    x-range reserve (editing views pass 1, ECHO/v2 pass 4).
- **TAB TEXT** now works on **desktop** too (epoche-ui `set_title_override`, additive;
  cumulus unchanged). **SLIDERS** hamburger stepper scales PAN/GAIN track thickness (1–4×).
- **τόπος sizing beta** (v2.22) — `src/tokens.rs` holds two selectable sizing sheets (the
  original v3/Figma numbers vs the owner's τόπος proposal), toggled live via hamburger ▸
  **SIZING (V3 / τόπος β)**. Spec exported to **`docs/TOPOS_TOKENS.md`** (portable to other
  plugins). Knob token corrected to 96@2×=48 (= v3 size; the sheet's `72` contradicted its
  own cell formula). Both are session-state, not persisted.

**New files:** `src/tape.rs`, `src/tokens.rs`, `docs/TOPOS_TOKENS.md`. **New params:**
`degrade_{amount,drive,wear,type}`, `degrade_routing`, `sat_{amount,drive}`, `sat_routing`,
`fb_curve`; deprecated (kept for state-compat): `soft_clip`, `feedback_time_mode`.
**New EditorState:** `slider_steps`, `sizing_beta`, `field_sel`, `view3`.

### ⭐ owner-directed viz/UX ideas (2026-07-13) — ✅ ALL DONE in v2.26.0–2.27.0
1. ✅ **GRID tap-panel occlusion** → panel now lives in the SHAPE card
   (`draw_strip_tap_panel`), not floating over the plot.
2. ✅ **Snap-to-grid / quantize** → SNAP chip in GRID/STEMS (`snap3` + `snap_offset`).
3. ✅ **Filter-mode visual cue** → response-curve glyph on every TYPE button
   (`filter_glyph` / `v3_button_glyph`).
See the "What changed 2026-07-14" changelog above. Only hand-testing on the iPad + in a
DAW is left for these.

### iOS / AUv3 (see DECISIONS 2026-07-11)
`impl AuPlugin` + `nice_export_auv3!` (both `#[cfg(target_os="ios")]`); `bundler.toml` at
workspace root; target-split deps in Cargo.toml (iOS = `auv3`, no `standalone`/`opengl`).
iPad runs in **touch mode** (`set_touch(cfg!(target_os="ios"))`).
**Install recipe (learned the hard way v2.24):**
- `cargo nice-plug bundle-ios mooltitwap --device` — **always `--device`** (without it you
  build the *simulator* app). Its own install step errors on a Wi-Fi-tunnel iPad (USB-only
  discovery) — **ignore that error**, then push directly:
- `xcrun devicectl device install app --device 4594E238-8C6F-5D0F-A274-7766D3D606DE
  "…/target/ios/device/Afterecho.app"` then `… process launch … com.tlkm.afterecho`
  (launch makes iOS re-register the AUv3). Verify the pushed version with
  `strings …/MooltitwapAU.framework/MooltitwapAU | grep 2.24`.
- Profiles minted via `scratchpad/afmint` xcodegen + `xcodebuild -allowProvisioningUpdates`;
  **free-team profiles expire ~2026-07-18 → re-mint then.**

**Verified by build + standalone screenshots (and iPad install) only. Still almost nothing
is hand-tested by ear on the iPad or in a DAW** — the standing gate (TASKS.md).

### ⭐ NEXT PHASE (owner-directed) — CUSTOM MULTITOUCH GUI, like cumulus
Goal: make the iPad truly multitouch — several fingers moving different controls at once
(cumulus does this for "almost every element"). egui delivers ONE pointer, so any element
built on `ui.interact` is single-touch. The kit already has the raw-touch plumbing:
- iOS `UITouch` events → `nice-plug-egui/src/editor_ios.rs` (`touches_began/moved/ended/
  cancelled` + `dispatch_touch`).
- `theme::touch()` gate; `epo::tap_touch(ui, hit_rect, id)` claims ONE finger for a tap;
  `epo::knob_rt(...)` is a raw-touch knob (own hit-test, claims a finger, vertical drag).
- `theme::TOUCH_MIN_HIT = 44.0`.

**What's already multitouch in v3**: knobs (`epo::knob_ex`→`knob_rt`), the
`preset_slot`/`card_tabs` taps, AND — since **v2.25.0** — every `v3_button` (chips/toggles/
modes/A-B/curve/lock/sync/link), both mute pills, close-×, hamburger + sun, all via
`gui_v3::tap_or_click` (the kit `chip_rt` pattern).
**What is NOT yet (single-pointer `ui.interact`)** and is the remaining port target — all
DRAG surfaces (need a raw-touch "claim a finger, drag" helper, unlike the tap buttons):
- the hand-painted PAN / GAIN tracks + FILTER-block sliders + SPACE has knobs (ok) — the
  tracks need a raw-touch "claim a finger, drag horizontally" like `knob_rt` does vertically.
- **GRID / STEMS tap dots** (`draw_tap_field` / `draw_tap_stems`, `ui.interact` per dot)
  — raw-touch claim so several taps drag at once (this view benefits most from multitouch).
- XY SHAPE pads (`draw_xy_pad`) — raw-touch claim so both pads move at once.
Plan: study cumulus's custom widgets (its editor.rs uses `tap_touch`/raw-touch pervasively),
add a raw-touch horizontal-slider + toggle helper to the kit (or v3), swap v3's
`ui.interact` call sites over. Test with 2+ fingers on the iPad.

**Also outstanding**: finish the multitouch port — the remaining DRAG surfaces (PAN/GAIN
tracks, FILTER-block sliders, GRID/STEMS tap dots, XY SHAPE pads) need a raw-touch
horizontal-drag helper (the tap buttons are done, v2.25); hand-test everything on iPad + in
a DAW (multitouch feel, snap drag, M/S pan, OLED/contrast, palette round-trip, A/B,
DEGRADE-tape by ear, FB CURVE feel); merge `nice-plug-port`→`main`; the
`mooltitwap`→`afterecho` crate/folder rename; ECHO-strip feedback-footer redesign (still
just sketched); optionally **persist** the session-state theme toggles (OLED/`dark_variant`/
`high_contrast`/`sizing_beta` all reset on reload — would need new persist params).
**Uncommitted in the shared kit (epoche-ui):** `set_title_override` (v2.21) + the additive
`DARK_OLED` palette and `HIGH_CONTRAST`/`boost()` (v2.27) — not yet committed there.

---

## ⭐ PRIOR STATE — v2.17.1 · 2026-07-07 · branch `nice-plug-port`

The plugin was **ported off nih-plug onto nice-plug + nice-plug-egui + epoche-ui**
(egui 0.31→0.34) and the editor fully rebuilt on the shared Epoché widget kit so it
matches the **cumulus** plugin. See ARCHITECTURE.md (current) + DECISIONS.md (why).

- Framework/editor port: **done**, builds clean, bundles CLAP/VST3/standalone
  (`cargo nice-plug bundle mooltitwap --release`), installed to `~/Library/Audio/Plug-Ins`.
- Editor: header (hamburger menu w/ persisted dark-light + font), 3 epo cards, real
  stems/grid viz + XY pads (theme-colored), per-tab gradient tap colors, full
  PAN/GAIN/FILTER/XFEED/DIFFUSE tabs, footer (feedback + drag TAPS + FB-TIME lock/drag),
  aspect-locked resize grip, named preset system (`src/presets.rs`).
- git: `main` holds the pristine pre-port v2.15 checkpoint (`be061d8`); all port work is
  on **`nice-plug-port`**. epoche-ui got an additive `Accent::Raw` (committed `d43c2fa`).

**Verification workflow:** `cargo build --release --bin mooltitwap`, launch the
standalone, find the window via a Swift `CGWindowListCopyWindowInfo` snippet,
`screencapture -x -o -l <id>`. `cliclick` is unreliable (window on a negative-Y
display) — to shoot a specific tab/menu/modal, temporarily flip its default in code,
build, shoot, revert.

> ⚠️ **§1–§11 below predate the nice-plug port** (v2.13-era, nih-plug/nih_plug_egui,
> old manual-rect gui.rs). The DSP sections (§2–§4 signal architecture, feedback,
> delay modes, diffusion) are still accurate; the framework/build/GUI-scaling notes
> are historical. Trust ARCHITECTURE.md for current structure.

---

## 0. TL;DR / Quick start

```bash
cd "/Users/emmanuelbussien/Petal vst/mooltitwap"

# Compile-check fast (catches type errors)
cargo build

# Build the actual plugin bundles (release)
cargo xtask bundle mooltitwap --release
# → produces ../target/bundled/mooltitwap.{clap,vst3}

# Install to the user plugin folders (NOT /Library — that needs sudo)
cp -rf "../target/bundled/mooltitwap.clap" "$HOME/Library/Audio/Plug-Ins/CLAP/"
cp -rf "../target/bundled/mooltitwap.vst3" "$HOME/Library/Audio/Plug-Ins/VST3/"
```

The version string shown in the GUI header comes from `CARGO_PKG_VERSION` at compile
time — if you don't see the version you expect after a build, the DAW is loading a
**cached** copy from another path (or you forgot to bundle+install).

**Bump the version in `Cargo.toml` on every change you want to A/B in the DAW** — the
user relies on the header version to confirm the new build loaded.

---

## 1. Repository layout

```
Petal vst/                      ← git repo root (NOTE: project lives in a subdir)
├── mooltitwap/                 ← the actual crate
│   ├── Cargo.toml              ← version lives here
│   ├── src/
│   │   ├── lib.rs        (612) ← Plugin trait, DSP process loop, state
│   │   ├── params.rs     (543) ← all nih-plug params + enums + time helpers
│   │   ├── gui.rs       (2235) ← entire egui GUI (one file, no submodules)
│   │   ├── delay.rs      (303) ← DelayLine, SlewedTime, TapReader (3 delay modes)
│   │   ├── diffusion.rs  (147) ← Schroeder allpass diffuser (Airwindows-style)
│   │   ├── topology.rs    (92) ← tap-position math (Layer 1)
│   │   └── filter.rs      (92) ← SVF, soft_clip, PeakLimiter
│   ├── docs/
│   │   ├── HANDOFF.md           ← this file
│   │   └── RESIZE_IMPLEMENTATION.md  ← notes on the GUI scaling system
│   └── patches/
│       └── nih_plug_egui/      ← LOCAL PATCHED COPY of nih_plug_egui (see §9)
└── target/bundled/             ← build output
```

### ⚠️ Git state — READ THIS
- The repo's **last commit is `a6132da` "Mooltitwap v2.1.0"**. Everything from
  v2.2 through **v2.13 is uncommitted in the working tree.** `git status` shows
  `src/diffusion.rs`, `docs/`, `patches/` as untracked and the rest as modified.
- **The user has explicitly asked NOT to push to GitHub** except on "major
  introductions, not small fixes." Honor that. Do not `git push` without being
  asked. Committing locally is fine if asked; pushing is gated.
- Practically: treat the working tree as the source of truth, not git history.
  `git log` will mislead you — the commit messages describe an old pitch-shifter
  architecture that **no longer exists** (removed in v2.4.0).

---

## 2. Signal architecture (the mental model)

Mooltitwap is conceptually two layers feeding a single shared delay buffer:

### Layer 1 — Topology (`topology.rs`)
Pure math. Given `(spacing_mode, shape_x, shape_y, n_active)` it produces
`positions[0..16]`, each a normalized multiplier in roughly `[0,1]`. The DSP turns
`position[i]` into an actual delay: `delay_i = position[i] * (NUM_TAPS * base_time)`.

**Anchoring (v2.8+, important):** with `n_active` taps,
- `position[0]` is **always** `1/NUM_TAPS` (first tap fixed),
- `position[n_active-1]` is **always** `n_active/NUM_TAPS` (last active tap fixed),
- intermediate taps are shaped between those two anchors by the XY pad.

This mirrors Cluster Delay: the cluster's outer boundary stays fixed; the XY pad
only redistributes density *between* first and last tap. `sigmoid_blend` and
`bias_curve` both preserve the endpoints `{0,1}`, which is what guarantees the
anchors. **`gui.rs::compute_positions` is a hand-duplicated copy of this math** —
if you change `topology.rs`, change `compute_positions` to match or the viz drifts
from the audio.

### Layer 2 — Taps + feedback (`lib.rs::process`)
One ring buffer per channel (`main_l`, `main_r`). Each sample:

1. Encode channel mode (Stereo / Mid-Side / Mono).
2. Apply input gain, optional soft-clip, optional **input-routed** diffusion.
3. **Read feedback** from the main buffer at `cluster_span` samples back
   (`fb_time_slew.tick()`), scale by `feedback`, optionally crossfeed/diffuse it.
4. **Write** `input + feedback` into the main buffer.
5. **Read all N taps** from the main buffer at their (mode-dependent) delays,
   per-tap SVF filter, per-tap gain, per-tap pan → sum into `tap_out_l/r`.
6. Optional **output-routed** diffusion, dry/wet, channel-mode decode, output gain,
   limiter.

### ⭐ The feedback architecture is the heart of the plugin (v2.11 rewrite)
**This is the single most important thing to understand.** Earlier versions
(v2.7–v2.10) summed the N tap outputs into a separate feedback buffer. That was
**wrong**: each tap reads a different point in time, so summing them collapsed N
moments into one, which then got re-tapped → exponential smearing. It also needed a
`/N` normalization hack that crushed the feedback level.

**The correct model (Cluster Delay's, now ours):** feedback is read straight from
the **main buffer** at `cluster_span` back and re-injected at the write point. The
buffer already contains the cluster shape; re-injecting it produces a *perfect,
time-aligned repeat* of the whole pattern, attenuated by `feedback` each cycle.
Consequences:
- `feedback` param IS the literal per-cycle gain. 0.6 → −4.4 dB/cycle (matches
  Cluster Delay). ≥1.0 → self-oscillation. Range goes to 1.05.
- Feedback is **independent of tap layout** — moving the XY pad or wobbling no
  longer perturbs what recirculates (this killed the old "ringing" artifact).
- Muting a tap silences it in the output but does NOT remove it from the
  recirculated cluster (correct — matches Cluster Delay).

If you ever touch the feedback path, **do not** go back to summing tap outputs.

---

## 3. Delay transition modes (v2.13, `delay.rs::TapReader`)

A delay's read pointer moving = pitch shift. When tap times change (XY pad, wobble,
time offset, tap-count, base-time) the reader has to get from old to new delay. Three
user-selectable modes (`params::DelayMode`, GUI left panel "DELAY MODE"):

- **Repitch** (default): `SlewedTime` glides the read position. Tape-bend Doppler.
  Wobble drives the same mechanism. This is the classic sound.
- **Fade**: dual read heads + equal-power (cos/sin) crossfade over `FADE_SAMPLES`
  (1024 ≈ 21 ms). **Pitch-neutral** — best for editing during playback. Wobble is
  **bypassed** in this mode in `lib.rs` (`wobble_mod = 1.0`) because per-sample
  mod_scale changes would re-trigger fades endlessly.
- **Jump**: snaps instantly. Click on big jumps, zero pitch artifact.

`TapReader::set_mode` is idempotent (no-op if unchanged) and re-aligns internal
state on an actual switch so changing modes mid-playback is silent.

### Snap guards (`lib.rs`, the "topology_jump" logic)
Independent of delay mode, certain **discrete** events must snap (never glide), or
you get multi-second pitch sweeps:
- `base_changed` — base time / tempo / sync-division changed between blocks.
- `n_active_changed` — tap count changed (re-anchors ALL intermediate taps).
- `newly_active` — a tap went inactive→active.
- `needs_initial_snap` — first block after `reset()`.

On any of these, all tap readers AND `fb_time_slew` snap. This is what fixed the
"change tap count → mangled/repitched" bug. The Repitch glide is only for
*continuous* parameter motion, not these step events.

---

## 4. Diffusion (`diffusion.rs`, v2.9 rewrite + v2.10 SIZE)

8 chained **Schroeder allpass delay sections** (not the old ChowMatrix 1st-order IIR
— that just phase-rotated at 200 Hz and was inaudible as a tail). Prime-ish delays
12.5–53.1 ms, `g = 0.6`. Produces a real reverb-like bloom.

- `depth` (the AMOUNT knob) = wet/dry mix, 0 = bypass.
- `set_size(size)` scales all stage delays at runtime (allocation-free; buffers are
  sized for `MAX_SIZE = 2.0` at `prepare`). 0.25× = tight & bright (fixes "muffled"),
  2.0× = lush plate.

**Two independent diffuser instances** exist per channel:
- `diff_l/r` — used ONLY by `CrossfeedMode::Diffuse` (legacy crossfeed mode).
- `diff_wet_l/r` — used by the DIFFUSE tab, with **routing** (`DiffuseRouting`):
  - `Input` — diffuse the dry input before the buffer (layered).
  - `Feedback` — diffuse the recirculating signal (tail grows each cycle).
  - `Output` — diffuse the tap-mix before dry/wet (cleanest; default).

The SIZE/AMOUNT knobs and routing live in the DIFFUSE right-panel tab
(`gui.rs::draw_diffuse_panel`).

---

## 5. Parameters (`params.rs`)

`PetalParams` (the `#[derive(Params)]` struct) + per-tap `TapParams[16]` (nested
array, `#[nested(array, group="tap")]`). Key ones:

| Param | Notes |
|---|---|
| `base_time_l/r` (0..1) | Free mode → exp 1–2000 ms; Sync → snaps to `SYNC_DIVS` |
| `time_linked` | L=R |
| `sync_mode` | Free / Sync |
| `delay_mode` | Repitch / Fade / Jump (v2.13) |
| `spacing_mode` | Linear / Exp / Log / Euclidean(=linear placeholder) |
| `shape_x/y_l/r` + `shape_linked` | XY pad → topology shaping |
| `num_taps_active` (1..16) | how many taps are live |
| `feedback` (0..1.05) | literal per-cycle gain; >1 self-oscillates |
| `feedback_time` + `feedback_time_mode` | independent FB time (when unlocked) |
| `feedback_time_locked` (default true) | locked → FB time = cluster span |
| `crossfeed` + `crossfeed_mode` | Mix / PingPong / Diffuse |
| `diffuse_amount` + `diffuse_routing` + `diffuse_size` | DIFFUSE tab |
| `channel_mode` | Stereo / MidSide / Mono |
| per-tap: `active, gain_db, pan, time_offset, filter_{type,cutoff,q,slope}` | |
| `wobble_rate/amount`, `input_gain, output_gain, dry_wet, soft_clip` | |

**Time helpers on `PetalParams`:** `base_time_seconds`, `format_base_time`,
`feedback_time_seconds`, `format_feedback_time`. `SYNC_DIVS` is the
`(beats, label)` table; `snap_to_div(norm)` snaps a 0..1 value to the nearest entry.

`GAIN_SKEW = 0.333` (cube-root) maps gain slider position → dB so 75% = 0 dB
(DAW-fader feel). `gain_db_to_pos` / `pos_to_gain_db` in gui.rs.

---

## 6. GUI (`gui.rs`) — one big file

### Scaling system (critical convention)
There is **no `set_pixels_per_point`** — egui-baseview tessellates with its own ppp;
overriding it blurs everything. Instead a single scale factor
`s = S(window_width / 1080)` is threaded through every draw fn, and **every fixed
pixel value must be wrapped in `s.f(x)`** (and corner radii in `s.rr(r)`, vec2 in
`s.v(v)`). Zoom buttons (75/100/125/150/200%) call `egui_state.request_size(...)`.

**Scaling status:** left panel, right-panel tabs, right-panel body (tap rows,
sliders, filter rows), and footer are all scaled (v2.10). **Still NOT scaled** (uses
raw px, looks right at 100% only): `draw_center` interior — `draw_stems`,
`draw_grid_viz`, `draw_shaping_strip`/`draw_xy_pad`, and the DIFFUSE/XFEED panel
bodies. Threading `s` into those is the main outstanding GUI task. See
`RESIZE_IMPLEMENTATION.md`.

### Layout
`draw_plugin` → header (40px) + body. Body = left panel (220px) | center | right
panel (280px). Center = viz-tabs + visualizer (Stems/Grid) + shaping strip (XY pads).
Right = icon tabs (PAN/GAIN/FILTER/XFEED/DIFFUSE) + scrollable body + 3-column footer
(FEEDBACK | FB TIME | TAPS).

### Visualizer data (`compute_viz_data` → `VizData`)
Computed once per frame. Centralizes the auto-range math. Two ranges:
- `grid_range_secs` (Stems view) — 2× tap span + feedback echo extension.
- `grid_range_secs_taps_only` (Grid view) — `max_tap_secs * 1.10` (tight, for
  precise editing; v2.13).

**The viz MUST mirror DSP feedback math** (`fb_cycle_secs` uses the same
locked/unlocked/cluster-span floor logic as `lib.rs`) or echoes draw in the wrong
place. This was a real bug (the "1/1 lands after tap 3 but dotted variants align to
last tap" report). Echo bar **heights** use cube-root of cycle gain so the tail
stays visible; threshold is −60 dB.

`tap_color(i, n_active)` — purple gradient `#a06cf0` → `#d4a8ff` across active taps,
used everywhere (stems, grid balls, slider fills, dots) for visual consistency.

### FB TIME footer widget
Locked (default): displays the live cluster span, non-interactive. Unlocked:
drag the value to set; the LOCK/FREE/SYNC button below toggles lock on left-click and
cycles Free/Sync on **right-click** when unlocked.

### Known GUI bugs / quirks (unfixed)
- **"Ghost edit" on sliders** — v2.13 went drag-only (removed click-to-set) to stop
  released TAP-COUNT drags from bumping a newly-appeared row. User reports it can
  STILL happen occasionally (a drag started on the TAP number, released over a row).
  Root cause is egui routing the release to a row that didn't exist at press time.
  A robust fix likely needs gating on `is_pointer_button_down_on()` at press, or
  capturing pointer to the tap-count widget for the whole drag. Low priority per user.
- **Free-mode grid** caps at 64 s (`pick_nice_range` nice list ends at 64000 ms).
  Bump the list if longer ranges are needed.

---

## 7. Version history (what each bump did)

- **v2.6** thicker stems, per-tap purple gradient, echo cycle cap → 64.
- **v2.7** independent feedback-time control (replaced feedback_tap), DIFFUSE tab.
- **v2.8** ChowMatrix diffusion attempt, FB-TIME lock, spiral protection, bar-width
  scaling, 2× viz rule, subdivision-label fix, **XY pad first+last anchoring**.
- **v2.9** **feedback /N normalization** + Airwindows Schroeder diffusion + diffuse
  routing (Input/FB/Output).
- **v2.10** diffusion SIZE knob, echo viz cube-root height + −60 dB threshold,
  GUI scaling pass (right panel + footer + tap rows).
- **v2.11** ⭐ **feedback architecture rewrite** — read-from-main-buffer instead of
  tap-sum. Removed separate fb buffers + the /N hack. Clean cluster repeats.
- **v2.12** snap on tap-count change (fixed "mangled" bug), grid margin 2×→1.10×,
  slider-drag-ownership attempt.
- **v2.13** DelayMode Repitch/Fade/Jump, grid 28s-cap removed, slider drag-only.

---

## 8. Things that are intentionally GONE (don't "re-add" them)
- **Per-tap pitch shifting** (semitones, PSOLA, dual-head granular) — removed v2.4.
  Had unfixable flutter/AM artifacts. Git history/commit messages still reference it;
  ignore them. Tape-bend (Repitch mode) is the only pitch behavior now, and it's a
  side-effect of delay-time glide, not a dedicated shifter.
- **The old tap-sum feedback bus + /N normalization** — removed v2.11. See §2.
- **Separate `fb_l`/`fb_r` delay lines** — removed v2.11; feedback reads from `main`.

---

## 9. Build gotchas (learned the hard way)
- **`cargo clean` after a version bump if the binary shows the old version.** Cargo
  sometimes reuses the cached artifact and the GUI shows a stale version string.
- **`cp -r` MERGES into an existing bundle dir** — always `rm -rf` (or `cp -rf` which
  overwrites file-by-file but can leave orphans). The install commands in §0 use
  `cp -rf`; if you see weird behavior, `rm -rf` the destination bundle first.
- **`/Library/Audio/Plug-Ins` needs sudo** (no terminal password available in this
  env). Install to **`$HOME/Library/Audio/Plug-Ins`** instead — both CLAP and VST3
  are scanned there.
- **`patches/nih_plug_egui`** is a local patched copy (referenced by `Cargo.toml` as
  a path dependency). Don't replace it with the upstream crate without checking what
  was patched (resize handling — see RESIZE_IMPLEMENTATION.md).
- Hex float literals like `0xa0_f32` don't exist in Rust — cast: `0xa0_u8 as f32`.

---

## 10. Outstanding work / backlog (priority order)
1. ~~Wobble → Ducker~~ **DONE (v2.17.5): Wobble removed** (params + LFO + UI).
   Ducker (sidechain envelope follower ducking wet by dry level) remains optional —
   left-panel space is free for it.
2. **Scale `draw_center` interior** (stems/grid/shaping) with `s` — currently only
   correct at 100% zoom.
3. **Robust slider ghost-edit fix** (see §6).
4. **Free-mode grid range** beyond 64 s (see §6).
5. **Verify Fade mode** thoroughly — it's new (v2.13) and lightly tested. Check
   feedback interaction (the feedback read still uses `fb_time_slew`, separate from
   per-tap readers — Fade mode does not affect the feedback head).
6. Euclidean spacing mode is a placeholder (= linear). Implement real Euclidean
   rhythm distribution if desired.

---

## 11. How the user works / communication notes
- The user is the developer-owner, hands-on, tests every build in Ableton by ear and
  by looking at the rendered waveform (bounces Mooltitwap vs Cluster Delay side by
  side — Cluster Delay is the reference target for feel & level).
- **Always bump the version** so they can confirm the loaded build.
- They give detailed perceptual bug reports ("rings", "muffled", "mangled",
  "repitched"). Translate those to DSP root causes — usually it's a read pointer
  moving when it shouldn't (→ snap/Fade), or a level/architecture issue.
- Reference plugins they cite: **Cluster Delay** (behavior target), **Ableton Delay**
  (Repitch/Fade/Jump modes), **Airwindows Galactic/MatrixVerb** (diffusion feel),
  **ChowMatrix** (interpolation: we use 4-point Lagrange, matching chowdsp).
- Don't push to GitHub unprompted (see §1).
