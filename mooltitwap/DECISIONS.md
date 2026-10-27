# mooltitwap — Decisions

_Newest first. Each entry: date · what we decided · why · alternative rejected._

## 2026-07-11 · Spotlight preset palette · dark-mode variants · A/B sizing (v2.18.0)
Three big items, all ported from y-flanger (which itself ported cumulus).
- **Preset system**: full spotlight command palette in new `src/palette_v3.rs`
  (~540 lines, faithful port): search filter, scope chips ALL/FAVORITES/RECENT,
  rows with favorite star + build stamp + tags, load-on-click/Enter, Save-As +
  Rename inline editors, two-click Delete, inline add/remove Tag, keyboard nav
  (↑↓/Enter/Esc/⌘S), and clickable footer keycaps (hosts eat the real shortcuts).
  Header preset pill is now the kit `epo::preset_slot` (◀ name ▶): ◀/▶ step the
  library, name opens the palette, dirty `*` via `presets::matches`. Backend
  (`presets.rs`) extended: `Preset` gains `favorite`/`build`/`tags`; added
  `rename`/`toggle_favorite`/`add_tag`/`remove_tag`/`delete`/`matches`/`index_of`;
  `save_as` gained a `build` arg + preserves favorite/tags on overwrite. State on
  EditorState (`preset_open`, `preset_search`, `palette_*`, `recent`,
  `loaded_snapshot`). NOT hand-tested live — port compiles + renders (verified by
  forced-open screenshot); load/save/rename/delete/fav/tag need a real run.
- **Dark mode**: added `DARK_WARM` (graphite) + `DARK_INK` palettes + a
  `DARK_VARIANT` atomic + `set_dark_variant`/`dark_variant` to the SHARED
  epoche-ui theme.rs (ADDITIVE — cumulus stays on the existing cool DARK unless it
  opts in). v3 now honors the persisted theme (defaults DARK) + variant 1 (warm);
  hamburger menu gains a SHADE picker COOL/WARM/INK. Kept epoche-ui's kit-exact
  cool DARK (not y-flanger's) per its documented "ghosty inset" note.
- **A/B**: resized to 31×27 with a real gap between the boxes, snug left of the
  sun (was 30px touching). Logic unchanged (capture-on-switch; simpler than
  y-flanger's per-frame Arc<Mutex> auto-capture but equivalent for the toggle).

## 2026-07-11 · iPad TOUCH MODE + v3 fixes round 6 (v2.18.0)
- **iPad now runs in touch mode**: `set_touch(cfg!(target_os="ios"))` (was always false).
  This was the root cause of "tab text resize does nothing" — `title_fs()` only honors
  `set_title_size` on touch. Also switches iPad to raw-touch knobs (`knob_rt`) + 44pt hit
  floors. LIKELY also fixes the "cutoff at 20 Hz locks the knob" report (was the pointer
  path at the param min) — needs on-device confirm. Desktop stays pointer mode.
- **Tab text size**: clamp raised 18→48px, step ±2, menu buttons enlarged (36×30, were tiny
  epo chips). Only visible on iPad (touch).
- **A/B moved** snug left of the sun icon (was right of the preset pill) — owner's placement.
- **SPACE page rebuilt on the DELAY cell grid**: 48px knobs (was 34) + 65×75 cells / pitch 69
  (8px gaps), centered in the card. Same size + spacing as DELAY. Diffuse knobs pushed below
  the routing chips (were overlapping).
- **Filter knobs** 34→40px (owner: a bit bigger).
DEFERRED: preset system like cumulus. Backend (`presets::Library`) ALREADY EXISTS and is
shared with cumulus; cumulus's editor.rs shows the UI (preset_load, browser popup, prev/next,
save, search at ~L2455+). v3 just needs the header preset-pill wired: prev/next cycles the
library, click opens a browser popup, save-as. Next-turn focus.

## 2026-07-11 · v3 polish: A/B · SPACE page · card+tab consistency · tab-size menu (v2.18.0)
- **A/B compare** (new): header A/B buttons right of the preset pill. Two full param
  snapshots in EditorState (`ab_slots`/`ab_active`) via `presets::capture`/`apply`.
  Switch = save live params → current slot, then apply target's snapshot (or seed it as a
  copy on first visit). NOT hand-tested live — logic reuses the proven preset capture/apply.
- **SPACE page** (XFEED+DIFFUSE merged, owner's shared tab): XFEED mode chips MIX/PING/DIFF
  + BLEND/BOUNCE/DEPTH knob (mode-dependent label); DIFFUSE routing IN/FB/OUT + AMOUNT +
  SIZE knobs. All 5 right-card pages now exist (PAN/GAIN/FILTER/SPACE).
- **Card + title consistency**: the hand-painted left/viz/strip cards now use PANEL fill +
  hairline stroke like `epo::card_tabs` (interior wells stay INSET); `draw_card_tag` uses
  the SAME `title_fs` proportional font + SOFT colour as the tabs (was 8.5px mono w/ box).
  So DELAY/STEMS/SHAPE titles match the TAPS·PAN·… tabs.
- **Tab text size**: `EditorState.title_steps` (default 2 = 12px on touch, cumulus parity);
  v3 calls `theme::set_title_size(10+steps)` each frame + a hamburger menu ▸ TAB TEXT −/+
  stepper. NOTE: `title_fs()` only honors this on TOUCH (iPad); desktop uses FS_LABEL, so
  the control is a no-op on the standalone — it's for iPad tuning.
- **Spacing**: DELAY cell pitch 77→69 (owner tightened 24→8px gaps in Figma); tap rows 46→45.

## 2026-07-11 · Renamed to Afterecho (display) · filter knobs · tighter rows (v2.18.0)
- **Rename**: `Plugin::NAME` "Mooltitwap"→**"Afterecho"**, CLAP_DESCRIPTION drops "Multitap".
  Owner: retire "Mooltitwap"/"multitap" branding entirely. **VST3_CLASS_ID + CLAP_ID kept
  UNCHANGED** → existing DAW sessions still recall the plugin (identity is the UID, not the
  name). Desktop bundle file is `Afterecho.{vst3,clap,app}`. The struct/crate/folder are
  still `mooltitwap` internally — renaming those is invasive (workspace, all `bundle
  mooltitwap` commands, xtask, bundler.toml `[mooltitwap]`, git) so it's DEFERRED to a
  coordinated pass; owner flagged the folder rename as "a bit of a problem".
- **Filter CUT/Q are KNOBS, not sliders** — the Figma filter block's bottom row is two
  `Pitch` knob-cells (same component as the DELAY Time knobs). Rebuilt each 2-up block as
  `[## · TYPE · SLOPE]` chips + two `epo::knob_ex` knobs (34px, tap-gradient accent, custom
  "20.0k"/"0.7" readouts). Skewed taper handled natively by knob_ex. Block h 116→122.
- **Tap rows** tightened again 50→46 (owner wanted "a tiny bit closer").
- **Color**: owner shared `y-flanger/design/dark-modes.html` (Epoché dark study — Warm
  graphite ★ foundation + 8 leveled accents). Noted for a future DARK-MODE pass; v3 stays
  light for now to match the light Figma.

## 2026-07-11 · v3 right card uses the kit's card_tabs · FILTER page · tighter rows (v2.18.0)
Owner feedback pass on the v3 editor. (1) **Tabs were wrong** — I'd hand-painted tiny
tag-chips; cumulus uses the kit's `epo::card_tabs` (dot-separated on the top border,
`title_fs` sizing, 44pt hit floors). Switched the right card to `epo::card_tabs`
("TAPS" + PAN/GAIN/FILTER/SPACE) placed via a child ui at the absolute card slot.
Gotcha: inside card_tabs the body ui has NO height budget while it measures, so a
naive `ScrollArea::max_height(ui.max_rect().height())` collapses to ~50px — FIX: claim
a `body_px`-tall region with `allocate_exact_size` first, lay the page out in a child
ui at that rect. (2) **FILTER page** built as **2-up squared blocks** (iPad-friendly,
matches the owner's Figma filter view): each tap = a bordered block with `[## · TYPE ·
SLOPE]` chips + CUT & Q sliders; 16 taps → 8 scrolling rows. Cutoff/Q use the
normalized param API (`unmodulated_normalized_value` / `set_parameter_normalized`) so
the handle follows the skewed taper. (3) **Tighter tap rows** — pitch 54→50 (Figma
edit removed the 8px gap; item_spacing 0). NOTE: DELAY-card row positions measured
identical to before — the owner felt they'd tightened it but the "Standard" frame's y
values match; left as-is (flag if a specific delta was intended).

## 2026-07-11 · Afterecho INSTALLED on the physical iPad (v2.18.0)
Real-device install works. Two corrections vs the first attempt: (1) bundle id is
**`com.tlkm.afterecho`** (vendor prefix `com.tlkm` on the ACTIVE signing **team
6ABFQC732N** — same as cumulus/flanger; `com.bussien.*` / `DX5H5JSYV8` was wrong).
(2) Free-team profiles must be Xcode-minted per bundle id (no wildcard). Reusable
mint recipe (no Apple-ID password needed while the Xcode session is valid): an
**xcodegen** app+app-extension project (`scratchpad/afmint/project.yml`, targets set
to `com.tlkm.afterecho` + `.AUExt`, `DEVELOPMENT_TEAM 6ABFQC732N`, automatic signing)
→ `xcodebuild -allowProvisioningUpdates build -destination generic/platform=iOS`
mints both profiles into `~/Library/Developer/Xcode/UserData/Provisioning Profiles/`.
Then `cargo nice-plug bundle-ios mooltitwap --device` signs + installs via devicectl.
**Free-team profiles expire in 7 days** — re-run the mint step when the install stops
launching. First launch needs the device UNLOCKED + a one-time "trust developer"
approval (Settings ▸ General ▸ VPN & Device Management). Pattern mirrors the prior
y-flanger `ProfMint` project (kept at its old scratchpad path).

## 2026-07-11 · AUv3/iOS build path opened — runs on iPad (v2.18.0)
Wired mooltitwap for iOS AUv3 and got the v3 editor running fullscreen on the
iPad Pro 11" **simulator** (first iOS build of this plugin; no sibling did it).
Changes: `impl AuPlugin` (AU_SUBTYPE `Aftr`, AU_MANUFACTURER `Busn`, type auto
`aufx`) + `nice_export_auv3!` in lib.rs; `bundler.toml` at workspace root
(name "Afterecho", ios_bundle_id com.bussien.afterecho); main.rs stubbed under
`cfg(target_os="ios")`. **Key gotcha — the xtask iOS build (`cargo nice-plug
bundle-ios`) passes NO --features, so desktop/iOS deps MUST split by
`[target.'cfg(...)'.dependencies]` in Cargo.toml**: desktop keeps `standalone`
+ egui `opengl`; iOS uses `auv3` and drops BOTH (baseview/jack/cpal are
desktop-only, egui-baseview has no UIKit backend — nice-plug-egui's own iOS
renderer takes over). The whole v3 editor (fixed 1194×834 canvas, egui closures,
EguiState resize calls) compiled for aarch64-apple-ios-sim UNCHANGED — no cfg
needed in gui.rs/gui_v3.rs. Install: `cargo nice-plug bundle-ios mooltitwap`
(booted sim, ad-hoc signed, zero credentials). **Real device** = same command
`--device` but needs an Apple Developer Team ID + provisioning profile (owner's
to provide). **Rejected:** per-package `[features]` default toggles (both targets
get default features under the xtask, so only target-cfg deps differentiate).

## 2026-07-11 · απόηχος v3 preview as a const-gated parallel editor (v2.18.0)
First page (PAN) of the owner's Figma redesign ("Mooltitwap" file, frame "Multitap
view pan", 2388×1668 @2x) ported as `src/gui_v3.rs` behind `gui::V3_PREVIEW` — the
v2.17 editor stays fully intact and returns by flipping one const. Geometry = Figma
px ÷ 2 on a 1194×834 canvas (ratio 1.432); WINDOW/BASE consts are const-if switched.
Touch-correctness implemented code-side per the owner's division of labor (Figma =
look only): tap pills paint 49×27 but hit the full 65×50 cell, header icons hit 44pt
squares, pan sliders are hand-painted (track/center-tick/fill-from-center/handle)
with whole-cell drag + double-click reset. Reused (made pub(crate), not duplicated):
compute_viz_data, draw_stems, draw_xy_pad, tap_color, set_enum. **Rejected:**
replacing the v2 editor outright (uncommitted work, only 1 of 4 pages exists) and
branching (working tree is the source of truth in this repo's workflow). Preset
pill + hamburger are display-only in the preview; plugin rename/IDs deferred.

## 2026-07-11 · Wobble removed · roomier tap rows · musical axis caption (v2.17.5)
Owner UI-redesign pass. **Wobble is gone entirely** (params `wobble_rate`/`wobble_amount`,
the per-sample LFO, and the left-panel section) — owner: "not so sensitive for that
plugin"; this also retires the Fade-mode wobble bypass and lets `set_nominal` move to
block level (it was per-sample only for wobble). The Ducker replacement from the old
backlog was NOT built — left-panel space is free for the coming redesign. **SATURATION
kept** for now (owner unsure, not a directive — flag, don't delete). **Per-tap rows**:
taller rhythm via the kit's live-ergonomics hooks (`theme::set_slider_size(1.4)` +
`set_slider_gap(6)`, scoped to the tap tabs and restored after) + `SP_MD` row gap; 16
taps now intentionally overflow and the card body scrolls (egui ScrollArea). **Value
steps** (per-tap): PAN displays −100..+100 in steps of 1 (stored −1..1, step 0.01),
GAIN 0.1 dB, CUTOFF 1 Hz integer, Q 0.1 with **max lowered 16 → 12** (owner: "12 or
something arbitrary"; presets with Q>12 clamp). XFEED/DIFFUSE value formatting deferred
(owner: "not so important right now"). **Viz axis**: the Stems range headroom dropped
2× → 1.10× — the 2× was a v2.8 relic from before echoes explicitly extended the range,
and it's why 8 taps at 1/1 showed a 64-beat axis; the Sync caption now reads "N BARS"/
"N BEATS" instead of formatting raw beats as "64/1" (which read like 64 bars).
NOTE: `spacing_mode` (LINEAR/EXP/LOG) has no UI in v2.15 either — hidden param, left as-is.

## 2026-07-11 · Viz mirrors DSP via shared topology math (v2.17.4)
Deep-analysed Stems/Grid against lib.rs and found three viz-only bugs that **predate
the port** (identical math in v2.15 `main` — so fixing them is a bug fix, not a parity
deviation; the DSP is untouched and presets sound the same). (1) **time_offset unit
mismatch**: DSP treats it as a fraction of the cluster SPAN (`nom=(pos+off)·span`),
the viz added it as a fraction of the display RANGE — so the timeline lied for offset
taps, Stems and Grid disagreed with each other (different ranges), turning FEEDBACK
made offset taps slide sideways, and Grid snap never landed on gridlines. (2) The viz
cluster span ignored offsets/mutes/in-range (DSP includes all three) → echo bars in
the wrong place when locked. (3) Grid-view corner caption showed the Stems range.
**Fix:** moved the single copy of the position/time math into topology.rs
(`compute_positions`, `tap_delay_secs`, `tap_fits`, `cluster_span_secs`, `MIN_FB_SECS`)
and made BOTH lib.rs and gui.rs::compute_viz_data call it — the hand-duplicated
`gui.rs::compute_positions` (a documented drift hazard) is gone. Grid drag now converts
px→offset by `range/span` so ball, pointer, and audio agree; range picking uses the true
drawn extent (offsets included, mute-agnostic). **Ping-pong viz** upgraded from a hard
odd-cycle swap to the exact blend: own-channel weight `(1+dᵏ)/2, d=1−2·crossfeed`, drawn
as two weighted copies (degenerates to the old full swap at crossfeed=1, nothing at 0 —
previously it swapped even with the knob at 0); pan no longer negates (DSP swaps content,
not pan weights). **Rejected:** keeping the viz-local copies in sync by hand (that rule
already failed silently for years) and gating the swap at crossfeed>0.5 (arbitrary pop).
Deferred: edge-ghosting/arrows for out-of-range taps (they still clamp to the border).

## 2026-07-10 · Full layout parity with v2.15 body geometry (v2.17.3)
After v2.17.2 the owner confirmed the editor still read as differently *aligned/
spaced/positioned* than pre-port v2.15 — because the port swapped hand-tuned absolute
pixel rects for kit auto-layout. Reshaped the whole body to reproduce v2.15 geometry
while keeping the epo widget *style* and port features (menu/resize/presets):
card widths back to LEFT_W=224 / RIGHT_W=284, window 660, `CARD_BODY_H` up to 564;
**footer moved back INSIDE the right card** (RIGHT_FOOTER_H=70 band, 3 columns) — which
also reclaims the height that restores the center viz proportions; LEFT panel rebuilt
with the exact v2.15 `section` rhythm (88px hero + 10/16/body/8 sections at body 30/76 +
hairline dividers); CENTER split inverted (shaping fixed 118 pinned to bottom, viz well
inset gets the rest); SHAPE-link column back to 56px. **Technique:** reproduce old
absolute rects INSIDE the epo card bodies via `ui.allocate_exact_size` + child uis at
computed rects (`new_child(UiBuilder::max_rect(..))`); footer columns stack single
widgets so `top_down(Center)` centers them (no sizing pass needed). **Rejected:** keeping
the full-width footer (would permanently steal the viz height) and reverting to the old
hand-painted widgets (style mandate keeps epo). Verified visually across all tabs/views.

## 2026-07-10 · UI placement parity pass vs pre-port v2.15 (v2.17.2)
Audited the ui-driven editor against the manual-rect reference (`be061d8`) region by
region and fixed 9 placement regressions (docs/UI_PARITY_AUDIT.md). Two were dropped
controls restored: the **SHAPE link toggle** (a 46px column at the right of the
shaping strip; `shape_linked` had no UI setter) and **per-tap mute** (the tap number
is now an `epo::toggle_sm` bound to `taps[i].active` on PAN/GAIN/FILTER rows). The
rest were geometry: preset pill centered on the true header midpoint, hero TIME knobs
enlarged to `KNOB_DIAM_INSPECT`, restored "dB" grid caption + mode-dependent crossfeed
caption (BLEND/BOUNCE/DEPTH), SATURATION section label, and an even 3-thirds footer.
**Footer centering:** egui auto-layout won't horizontally center a hugging row, so each
footer group is measured in a `UiBuilder::sizing_pass().invisible()` child ui (writes
gated by an `act` flag) then placed with a computed leading spacer. **Rejected:**
`top_down(Align::Center)` (left-aligned the rows) and hand-duplicating epo width
formulas (fragile). Verified visually in the standalone across all tabs/views.

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

## 2026-07-12 · v2.18.1 — delay-time changes go through the mode machinery (Guard-1 removed)
The v2.12 "Guard 1" snapped ALL tap readers whenever `base_samples` changed >0.5
samples between blocks. Dragging the TIME knob (or changing sync division/tempo)
changed it every block → `snap_to()` every block, which bypasses `TapReader`'s
mode logic entirely — Repitch/Fade/Jump all collapsed into identical per-block
hard snapping ("digital mash", mode buttons appeared to do nothing). **Fix:**
base-time/sync/tempo changes now route through `set_nominal()` so the selected
mode governs the transition (Ableton-like). Snap guards remain ONLY for discrete
re-anchoring: tap-count change, newly-active tap, and post-reset.
**Guard-1's original reason** (12-second Doppler ringing on sync jumps at the fixed
0.02 samp/samp tape rate) is solved differently: the Repitch glide rate is now
`max(0.02, remaining_distance / 100 ms)` recomputed per block — small moves keep the
classic constant-rate tape bend, big jumps become a fast bounded sweep (~0.5 s worst
case). **Rejected:** keeping the snap only for sync mode (still swallows the mode
selector) and a pure one-pole time smoother (loses the constant-detune tape feel on
small moves).

## 2026-07-12 · v2.18.1 — feedback head follows the same transition rules (fixes "FB time broken")
`fb_time_slew` glided at the fixed 0.02 samp/samp rate on FB-TIME changes — moving
1/16 → 1/1 at 120 BPM ≈ 90k samples of travel ≈ **94 seconds** to arrive, so the
knob read as dead. Now: Jump mode → snap; otherwise the same bounded glide as the
taps; snap kept on tap-count change/reset. Also fixed a dead condition: the fb-snap
checked `needs_initial_snap` *after* it had already been cleared that block.
NOTE (unchanged, by design): unlocked FB TIME is still floored at the cluster span —
the lower part of the knob is intentionally inert; and `feedback_time_mode`
(Free/Sync) is not yet exposed in the v3 GUI (stuck on its Sync default).

## 2026-07-13 · v2.18.2 — in-loop feedback saturation; unlocked FB TIME floor removed
The "unlocked FB TIME can't go below the cluster span" floor (v2.x) existed because
short feedback cycles "fed back infinitely, really loud". Root cause was NOT the short
cycle: the loop had **no saturation at all** (`write(x + raw_fb·feedback)`, tanh only
on the input when SATURATION is on) while the feedback knob goes to 1.05 → unbounded
exponential runaway whose doubling period = the fb cycle, so short cycles merely
detonated *faster*. **Fix per the project-wide rule** ("feedback paths must be allowed
to self-oscillate and are soft-saturated inside the loop; never gain-compensate"):
the recirculating signal is now headroomed-tanh saturated just before the buffer
write — `2·tanh(x/2)`, transparent at normal levels (−0.2 dB at 0.5 amp), ceiling ≈2.0
— so feedback ≥ 1.0 settles into bounded tape-style self-oscillation at ANY cycle
length. With runaway fixed, the cluster-span floor is removed: unlocked FB TIME is
free above the 10 ms squeal floor; short settings are now a usable dub-wash/comb zone
(the old "feedback racing the taps" sound is reachable, but only by explicit unlock —
LOCKED behavior is unchanged). Viz mirror updated in gui.rs::compute_viz_data.
**Rejected:** keeping the floor with saturation added (whole knob range should work);
saturating the write sum x+fb (would color the dry input even at zero feedback).

## 2026-07-13 · v2.19.0 — FB TIME follows the main SYNC; DEGRADE block (merge-rs CRUSH port)
**FB sync**: `feedback_time_seconds`/`format_feedback_time` now read the main delay's
`sync_mode` (synced delay → synced FB divisions, free → 10–8000 ms). The separate
`feedback_time_mode` param is deprecated-but-kept (old sessions restore silently; the
v3 GUI never exposed it — it was stuck on Sync). **Rejected:** removing the param
(state-compat churn) and exposing a second Free/Sync toggle (owner: one sync source).
**DEGRADE**: behavior-parity port of merge-rs `crush.rs` (mid-tread bit quantizer →
fractional S&H decimator, NO anti-alias filter — fold-back is the character) into
`src/crush.rs`, self-contained (AnimaLfo/Lcg/LinSmoother inlined). Params: BITS 1–16,
RATE 100 Hz–48 kHz (FULL = transparent), AMOUNT (internal dry/wet), routing IN/FB/OUT
on the same grammar as DIFFUSE. FB routing = generation loss (each repeat re-crushes,
before the loop's safety tanh so grit stays bounded). WOBBLE/LINK exist in the module
but aren't exposed yet (depth 0, linked). Presets/A-B pick the params up via param_map.

## 2026-07-13 · v2.20.0 — SATURATE block + FEEDBACK CURVE (Bloom/Gate); SPACE page compacted
**SATURATE**: playable tanh drive stage (DRIVE 0–24 dB into tanh, AMOUNT dry/wet, no
gain compensation per the loop rule), routable IN/FB/OUT like DIFFUSE/DEGRADE; distinct
from the always-on safety tanh. **FEEDBACK CURVE**: a recirculating loop is inherently
exponential, so non-natural decay shapes bend the effective loop gain by level via an
envelope follower inside the loop (5 ms att / 150 ms rel, linked max|L,R| of the raw
buffer signal): **Bloom** = compressor-in-loop (gain × clamp((0.35/env)^0.4, 0.5, 1.15)
→ tail plateaus, sustains, then dies; boost capped so fb<1 still decays), **Gate** =
expander-in-loop (full gain above env 0.1, ×(env/T)² below → hold-then-cut; FEEDBACK
sets hold length). **Rejected:** uncapped bloom boost (any fb → infinite sustain),
per-channel envelopes (skews the stereo image). SPACE page re-pitched to fit 5 sections
(XFEED/DIFFUSE/DEGRADE/SATURATE/FB CURVE) in the 722-unit body; verified by standalone
screenshot. Constants P=0.35/T=0.1/curve exponents are first-guess — tune by ear.

## 2026-07-13 · v2.21.0 — DEGRADE is now TAPE (re-deemer port); FB CURVE on the DELAY card; viz follows the curve
**DEGRADE rebuilt**: the v2.19 bitcrush was the wrong algorithm — owner wanted the
tape emulation from **RE-DEEMER** (`~/re-deemer`, ISC — attribute in release notes),
the same source merge-rs's TAPE-slot plan scopes. `src/tape.rs` ports verbatim:
J-A hysteresis + RK4 (Chowdhury DAFx-19) at 2× oversampling (47-tap Kaiser halfband),
record pre-emphasis + level-dependent HF self-erasure, repro EQ (de-emph/head bump/
gap loss/spacing), tape types I/II/IV, and the wear laws (bias sag, lower erasure
knee, fallen gap corner, hotter operating point). **Cut** (channel-effect scoping):
reel/transport/motor, hiss, dropouts, wow/flutter. Params: DRIVE −12..+24 dB (full
makeup — character not level), WEAR, TYPE chips, AMOUNT, routing. crush.rs deleted;
`degrade_bits`/`degrade_rate` params replaced by `degrade_drive`/`degrade_wear`/
`degrade_type` (only ~1 day old, no compat concern). MagParams normalization (~10k
iters × 3 types) runs in `initialize`, never per block. Verified by 3 unit tests:
small-signal ≈ unity, hot drive compresses, full wear bounded.
**FB CURVE moved to the DELAY card** (Natural/Bloom/Gate chips between FEEDBACK and
TAPS COUNT, owner request) — the SPACE section is gone; the DELAY-card SATURATION
toggle is **removed** (superseded by SATURATE routed IN; `soft_clip` param deprecated,
DSP no longer reads it, v2 legacy editor still shows it inert).
**Viz**: echo-bar decay now follows the selected curve — `compute_viz_data` builds a
per-cycle level array mirroring lib.rs's Bloom/Gate laws (prev level as env proxy)
instead of feedbackᵏ.

## 2026-07-13 · v2.21.0 — TAB TEXT on desktop + SLIDERS thickness (hamburger)
epoche-ui gained additive `set_title_override` so Afterecho's DESKTOP build honors
the TAB TEXT stepper (previously touch-only; cumulus unchanged — override defaults
off). New hamburger stepper **SLIDERS** (1..4×, 0.25 steps) scales the PAN/GAIN row
track/fill strokes + handle width via a process-global atomic (same idiom as the
theme atomics — avoids threading EditorState into the row painters). Both are
session-state, not persisted (matches title_steps precedent).

## 2026-07-13 · v2.22.0 — τόπος sizing tokens as a live A/B beta (hamburger ▸ SIZING)
The owner proposed a consolidated 23-number token sheet ("τόπος"). Implemented as
`src/tokens.rs`: every token is a fn returning the CURRENT v3/Figma value or the
τόπος value (@2× sheet px ÷ 2 → logical), switched by a process-global atomic wired
to a hamburger SIZING toggle (V3 / τόπος β) — both sheets compare live in the same
build, no restart. gui_v3's metric constants (KNOB_D/BTN_*), the DELAY-card cell
grid + row ladder (now computed: rows one pitch apart, +19 after dividers —
reproduces the Figma 20…667 ladder exactly at pitch 87), the SPACE grid, v3_button
(font/pad/radius/stroke), and dividers all read tokens. Key β deltas in logical
units: knob 48→36, cell gap 4→12 (pitch 69→76 / 87→88), control 27→28 r8→r6,
strokes 1/1/2.5→0.5/0.75/1, chip type 10→7. **Interpretation choice**: the sheet's
stroke values (1/1.5/2) read as @2× canvas px like every other number → halved to
logical; flag if they were meant 1:1. **Not yet tokenized** (stage 2 once a sheet
wins): epo kit internals (knob label/value type, card_tabs chrome), header, viz/XY
wells, FILTER blocks, footer strip, palette — the beta covers the card grids and
all pill controls, enough to judge the direction. Verified by standalone
screenshots of both modes; both fit the existing card geometry.

## 2026-07-13 · v2.22.1 — τόπος knob token corrected to 96 @2× (= 48, the v3 size)
The sheet's `knob: 72` was never actually discussed and contradicted the sheet's own
cell formula (`cell/height 152 = 24 + 4 + **96 body** + 4 + 24`) — a 72 knob leaves
12pt of dead slack in every cell's body band. Owner prefers the original knobs, so
the token is 96 @2× = 48 logical: identical to v3, and it exactly fills the τόπος
body slot (÷2: 12+2+48+2+12 = 76 = cell/height ✓). The β now differs from v3 only
in gaps (×3), pill height/radius/stroke, and type-chip size.

## 2026-07-13 · v2.23.0 — FIELD view: taps as draggable objects on a time × pan plane
Owner proposal: replace the read-only stems with an interactive "panner/distance"
surface. Implemented as the new DEFAULT center view (VizMode::Grid repurposed;
STEMS stays one chip away, toggle top-right of the well — same live-A/B culture as
SIZING). **Mapping:** X = tap time on the taps-only grid (drag writes per-tap
`time_offset`, converted px → seconds → span-fraction, clamped ±0.5); Y = pan,
L up / R down matching the stems convention, axis labels follow channel mode
(L/R · MID/SIDE · M). Dot size tracks tap gain (gain_db_to_pos); a faint vertical
guide drops from each dot to the timeline; double-click resets offset + pan; the
selected tap gets an emphasis ring. **Tap panel** (click a dot): bottom-center
inline card with mute pill (tap number), GAIN/CUT/Q knobs, filter TYPE (LP/BP/HP)
+ SLOPE (12/24/48) chips, × close; background click deselects. Pointer-overlap
rules: well background registered first, dots skip hit-testing under the open
panel, panel swallows its own clicks. Reuses paint_timeline_grid/dashed_h (now
pub(crate)) so the axis matches stems exactly. **Known limits (v1):** dots follow
the L-channel times (unlinked R times not shown), echo repeats aren't ghosted in
FIELD (use STEMS), and dots are single-pointer `ui.interact` — multitouch claim
is part of the planned raw-touch port. Card tag follows the view (FIELD/STEMS).

## 2026-07-13 · v2.24.0 — GRID · STEMS as center-card tabs; interactive STEMS; selection sync
Owner's Figma frames formalized the two-view system:
**GRID** = time × pan (kept from v2.23, renamed; + pan quarter-gridlines, PAN/TIME →
axis captions, dot size still tracks gain — owner likes it there).
**STEMS** = time × volume lollipops (NEW, replaces the read-only bar view in v3):
drag Y = tap gain through the DAW-fader skew (0 dB = 75% height, dashed dB gridlines
at +6/0/−12/−24 with right-side labels per the mock), drag X = time offset, fixed dot
size (height already encodes level — owner). Echo repeats ghost in from the same
per-cycle FB-CURVE levels (gain_db + 20·log10(cycle_gain)), so the decay-shape viz
survived the bar view's retirement. Echo-inclusive range in STEMS, taps-only in GRID.
**Tabs, not buttons** (owner): the center card is now an epo::card_tabs card —
"VIEW · GRID · STEMS" on the border, exactly like the TAPS card; the hand-painted
card + toggle chips are gone. **Panel symmetry** (from the mock): the tap panel's
first knob is whichever param is NOT on the view's axes — GAIN in GRID, PAN in STEMS.
**Selection sync** (owner): editing a tap row in the TAPS card (mute pill, PAN/GAIN
track drags, FILTER type/slope chips) selects that tap in the center view — via a
LAST_TOUCHED process-global (same idiom as SLIDER_THICK) folded into st.field_sel
once per frame; CUT/Q knob edits in FILTER blocks don't report yet (epo knobs return
nothing hookable — kit change, stage 2). v2's draw_stems bar view is untouched
(legacy editor only).

## 2026-07-13 · v2.24.1 — STEMS ghosts trimmed to the first repeat; ring + panel polish
Owner feedback on v2.24: the full echo-ghost tail buried the editable dots at high
feedback ("everything is hidden at the beginning"). Fixes: (1) STEMS ghosts only the
FIRST repeat, fixed faint alpha 0.25 — enough to see where the feedback lands, no
clutter; (2) `SHOWN_ECHO_CYCLES` 4→1, so the x-range no longer reserves room for four
echo cycles that aren't drawn (this was also squeezing the direct taps into the left
edge); (3) selection ring thinned to 1 px stroke, radius +4→+3 (was covering the tap
number); (4) the tap-panel knob cells are now 74 tall (= the label+knob+value stack)
so the top-down layout sits truly centered on the panel's midline — with the old 92
cell the slack pooled below and the knobs rode high. Note: the iPad push needed
`xcrun devicectl device install app` directly — nice-plug's bundle-ios installer only
finds USB devices and the iPad was on the Wi-Fi tunnel.

## 2026-07-13 · v2.24.2 — GRID dots float free; classic view restored as ECHO tab
(1) GRID view: removed the vertical guide bar from each dot to the baseline (owner:
dots don't need it — the time gridlines suffice). (2) The classic read-only L/R bar
view (full feedback tail) had silently vanished in v2.24 when STEMS became the
editable lollipops; restored as a third center tab: **GRID · STEMS · ECHO**. New
`EditorState::view3` (u8) drives the v3 tabs — `viz_mode` now belongs to the legacy
v2 editor only. (3) `compute_viz_data` split: `compute_viz_data_ex(params, tempo,
shown_cap)` parameterizes the echo-cycle x-range reserve — editing views (GRID/
STEMS) pass 1, ECHO and the v2 editor keep the classic 4 — so v2.24.1's range fix
no longer clips ECHO's tail. iPad note: `bundle-ios` WITHOUT `--device` builds the
SIMULATOR app; always `--device` (ignore its USB-only install error when on the
Wi-Fi tunnel) then `xcrun devicectl device install app` — verify with `strings` on
the framework binary if unsure what got pushed.
