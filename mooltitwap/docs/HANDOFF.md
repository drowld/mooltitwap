# Mooltitwap — Developer Handoff

**What it is:** A 16-tap multitap delay "garden" (VST3 + CLAP + standalone), inspired
by Rainbow Circuit's *Petal* and Cluster Delay. Rust.

---

## ⭐ CURRENT STATE — v2.17.1 · 2026-07-07 · branch `nice-plug-port`

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

**Next:** hand-test the v2.17.1 interactions in a DAW (TAPS drag/scroll, FB-TIME
unlock+drag, preset browser open→click-out→reopen, XFEED/DIFFUSE knobs — verified only
by code+screenshot); then merge `nice-plug-port`→`main`; then the Wobble→Ducker swap
(never started). Full list in TASKS.md.

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
1. **Wobble → Ducker.** The user wants Wobble *removed entirely* and its left-panel
   space reused for a **ducker** (sidechain-style envelope follower that ducks the
   wet signal by the dry input's level). Wobble currently still exists and still
   sounds "ringy" in Repitch/Jump (expected — it's modulating delay time). Fade mode
   bypasses it. Plan: gut Wobble, add Ducker DSP + UI.
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
