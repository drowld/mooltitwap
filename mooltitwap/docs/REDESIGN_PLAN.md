# Afterecho redesign plan

How to bring Afterecho (crate `mooltitwap`) onto the Tide redesign: new UI on desktop and iPad, an engine clean-up, and new functions.

Written 2026-10-27, from [REDESIGN_PLAYBOOK.md](../../../y-flanger/REDESIGN_PLAYBOOK.md) (the lessons from Tide) and the state of this repo. Nothing here is built yet. Section 5 lists the decisions that are yours to make.

---

## 0. Where Afterecho stands today

- **Version 2.27.0, branch `nice-plug-port`.** The last session was on 2026-07-14.
- **Most of the work isn't committed.** Everything from v2.17.2 to v2.27.0 (about 3½ months of work) is still only in the working tree:
  - 12 modified files;
  - 7 new ones, including `gui_v3.rs`, `palette_v3.rs`, `tape.rs` and `tokens.rs`.
- **It still compiles** against today's shared code: nice-plug on `feat/auv3-host-resize` and epoche-ui on `main`. I only ran `cargo check` on the desktop lib, which gave 22 warnings. I didn't run the plug-in and didn't try an iOS build.
- **Two editors live side by side:**
  - v2 (`gui.rs`, 1080 × 660);
  - v3 (`gui_v3.rs`, a fixed 1194 × 834 canvas), chosen by the `V3_PREVIEW` switch.
- **iPad:**
  - The AUv3 was built and installed in July as `com.tlkm.afterecho`. That free-team profile expired around 2026-07-18.
  - The canvas has a fixed size; the host can't resize it.
  - Buttons and knobs take several fingers at once. Everything you drag still follows one pointer: the PAN/GAIN tracks, the GRID/STEMS dots and the XY pads.
- **Engine:** 16 taps, 78 parameter IDs. Feedback comes from the main buffer and is saturated inside the loop. It has Tape (DEGRADE), SATURATE, DIFFUSE, XFEED and FB CURVE.
- **Untested:** nothing since v2.18 has been checked by ear in a DAW or by finger on the iPad.

### Two things I found that change the plan

1. **Tide's design system has split off from the shared kit.** Tide has its own copies of `epo.rs`, `theme.rs` and `tokens.rs`, and doesn't use `Dev/epoche-ui`. Afterecho uses epoche-ui. Everything §4 of the playbook describes lives only in Tide's copy:
   - `touch_interact`, `tap_or_click` and `knob_drive`;
   - finger claims that respect layers and disabled controls;
   - the cell grammar and the restyled menus and browser.

   Decision D1 is about this.
2. **Presets don't define the whole sound yet.** `presets::apply` leaves any parameter a preset doesn't mention at its *current* value. Tape, SATURATE and FB CURVE were added after the early presets, so loading those presets keeps whatever Tape or Saturate setting was there before. The playbook's rule fixes this: missing parameters load at their default.

---

## 1. What the Tide lessons mean for Afterecho

| Tide principle | Afterecho translation |
|---|---|
| **The picture is the control** | The **tap field** becomes the big picture. GRID and STEMS merge into one field: X is always time, and a **Y-axis picker** chooses Pan, Gain or Cutoff. Dot size shows a second value. The 16-row lists become the precise editor, not the main one. |
| **Show the modulation** | Afterecho's motion is its **echo tail**. An **Echo lane** draws the real repeats: cycle time, gain per repeat, and the FB CURVE bend. Drag ↕ for feedback and ↔ for FB time. It replaces the footer and the ECHO strip sketched on 2026-07-11, and answers your note that "FB TIME feels unintuitive". If modulation comes back (§4), it draws here too. |
| **Last touched** | You already have a `LAST_TOUCHED` atomic and a tap panel in the SHAPE card. They become one **last-touched card**: a big value and a fine slider, or the selected tap's mini strip. |
| **Name above, control, value below** | One cell grammar for every knob, toggle, segmented control and track, with the same cell height so rows line up across cards. |
| **Flat, one type family, fewer words** | Karla for text and Inconsolata for values; Space Grotesk and Spline Sans Mono go (Tide removed them). Menus get a 16 px radius and 8 px row gaps. Drop words the context already says. |
| **Plain names** | "Afterecho" is already plain. The απόηχος wordmark goes, as «παλίρροια» did for Tide. Vendor: **Soft Systems**. |
| **Mock first** | Both page shapes go on the design canvas at real iPad size before any code. |

---

## 2. The proposed page (a starting point for the mock, not final)

Group by job, not by widget:

- **Header:**
  - ☰ on the left;
  - then wordmark, preset browser (centred), A/B, and light/dark.
- **Picture:** the tap field, with the Y picker, SNAP and tap-count dragging.
- **Echo lane,** under the picture.
- **Right column of cards:**
  1. **Last touched.**
  2. **Time & shape:**
     - base time L/R, sync, link and delay mode (Repitch, Fade or Jump);
     - spacing and the XY SHAPE pad.
  3. **Colour:** DIFFUSE, Tape, SATURATE and XFEED. That may be too much for one card; the mock decides whether a segmented picker or two cards works better.
  4. **Output:** input, dry/wet, output, and channel mode (Stereo, M/S or Mono).
- **Per-tap lists** (PAN, GAIN and FILTER rows): **open; to decide in the mock.** Either a drawer under the picture on the tall page and a tab that swaps with the picture on the wide page, or no lists at all, leaving the field plus the last-touched tap strip.

**Page shapes:** start from Tide's two shapes, so the plug-ins look like a family:

| Shape | Size | Used for |
|---|---|---|
| Wide | 1366 × 616 | desktop; iPad in short host panels |
| Tall | 1050 × 780 | iPad full screen |

Afterecho is denser than Tide (16 taps), so the mock may move these sizes. Keep the same picking rule (the larger zoom wins) and the same tests.

---

## 3. Phases

Phases 1 and 2 can run side by side. Phase 3 needs both. **New functions (decision D3) have to be settled before Phase 2**, so the page has room for them.

### Phase 0 · Make it safe to start · small
- [ ] **Commit the working tree as it is,** as a v2.27.0 checkpoint on `nice-plug-port`, so 3½ months of work isn't only on disk. Decide whether to merge it into `main` (D6).
- [ ] **Settle the kit question (D1)** before writing any UI.
- [ ] **Rename the identity (D2),** before release, as Tide did:
  - vendor Soft Systems;
  - `com.softsystems.afterecho`;
  - new CLAP and VST3 IDs;
  - AU manufacturer `SfSy`, subtype `Aftr`.

  Keep the crate and folder name `mooltitwap`, as Tide kept `flanggggrrrrr`. That removes the folder rename you flagged as a problem.
- [ ] **Get the iPad installing again:** re-mint the profile, or move to the team Tide uses.
- [ ] **Record a baseline:** one listening pass of v2.27 in Ableton, plus a few bounced renders (default, high feedback, Tape on, M/S). Every DSP change after that is compared against them.

### Phase 1 · Engine clean-up (playbook §7) · medium
These don't change the sound; once smoothing has settled, the output should be bit-identical.

- [ ] **Fake-host tests:** copy Tide's `process_tests` (a `ProcessContext` with transport and an event queue). Today only `tape.rs` has tests.
- [ ] **NaN and inf:**
  - sanitise the input;
  - spot a poisoned loop (buffer write, SVF states, diffusers, tape state) and reset it;
  - clamp every filter cutoff below Nyquist.

  Today only the tape model checks.
- [x] **A preset defines the whole sound:** in `presets::apply`, parameters a preset doesn't mention load at their default. *(v2.31.0)*
- [ ] **Click-free switches:** check channel mode, delay mode, per-tap filter type and slope, tape type, and the IN/FB/OUT routing of DIFFUSE, Tape and SATURATE. Where a switch clicks, use Tide's sequence: fade out, swap, hold until the buffer refills, fade in.
- [ ] **Smoothing:** most continuous parameters already have smoothers. Check the rest (dry/wet, in/out, XFEED, the Tape and Saturate amounts, per-tap cutoff and Q), and use log smoothing for frequencies.
- [ ] **Buffer bounds test:** every tap and feedback read fits the buffer at maximum base time × 16 taps + offsets, at 192 kHz.
- [ ] **Tape's 2× oversampler:** make sure it only affects the wet path when Tape is routed to IN, and report latency if it adds any.
- [ ] **The playbook §9 audio list:**
  - 44.1 to 192 kHz;
  - block sizes 1, odd and very large;
  - `reset()`;
  - silence in gives silence out, for feedback below 1 (at 1 or more it is meant to self-oscillate).
- [ ] **MIDI and transport:** there's no MIDI input today. Add it only if a new function needs it (for example, retriggering). The tempo falls back to 120; check what happens with no transport and on loop wraps.

### Phase 2 · Mock both pages · medium
Canvas: https://claude.ai/artifact/6osu4gEG6f8RzfBqfsCr7Z.
- **First pass, 2026-10-27:** Wide (Field and List views), the tall page and the wide page on iPad boards at real size, and today's v2.27.0 for comparison.
- **Second pass, same day:**
  - the desktop page grew to 1366 × 720;
  - the palette is Epoché's cool foundation with the violet accent (Tide is warm with slate);
  - **Bars** replaces the List rows: the same Y axis with the taps in order;
  - the Echo lane is drawn as one band per cycle.

  The earlier list and lane styles stay in Tweaks to compare.
- **Third pass, same day:** the functions you picked are on the page:
  - the Pattern menu (Spread, Tilt, Reset; it works in Play);
  - Echo and Motion tabs in the lane;
  - Freeze, and a Tone row (Damp, Low cut);
  - Euclid spacing (Steps, Rotate; the Shape pad is off in Euclid);
  - Duck and Release in Output.

  A new row of boards shows 16 and 32 taps: Bars and Field on desktop, and 32 on iPad drawn by finger.
- **Fix, version 9:** every control cell was losing its layout, and the SVG axis, value and band labels didn't draw. Two runtime behaviours caused it (below). Checked by rendering every board locally.
- **Two canvas runtime gotchas, for any future mock:**
  - It drops a `style` that *starts* with a `{{hole}}`, so put whole-declaration holes at the end.
  - It wraps every text hole in an HTML `<span>`, which never draws inside an SVG `<text>`. Put labels in HTML over the SVG instead.
  - Tide's canvas uses `<text>{{…}}</text>` in the same way, so its labels are probably invisible too. Not checked.
- **To settle before building:** the kit's `theme.rs` `DARK` and `design/palette.json` `dark_cool` differ slightly (paper `#101215` against `#121417`). Pick the canonical one.

- [x] Put Wide and Tall on the design canvas, the tall one on a real iPad board (1112 × 834 pt).
- [ ] Decide:
  - the Y picker;
  - Echo lane gestures;
  - where the per-tap lists go (D5);
  - what last-touched shows;
  - how the right column is split.
- [ ] Measure in points: dial about 29 pt or more, text about 11 pt or more, whole-cell touch targets (aiming for 44 pt).

### Phase 3 · Build the new page · large
**Progress, 2026-10-27 (v2.28.0, branch `ui/scope-first`):** the first pass is built and checked in the standalone, on both shapes.
- Done:
  - the page frame;
  - the header;
  - Tide's Settings menu;
  - Last touched;
  - Time & taps, with the Shape pad;
  - Colour;
  - Output;
  - the lane's Cross and Loop rows.
- **v2.29.0:** the tap field (Field and Bars, the Y picker, dragging and drawing) and the Echo lane (bands, repeats, the decay line, feedback and cycle handles) are built. The cycle is Auto · Manual (see DECISIONS.md).
- **v2.30.0, from the first hand test (2026-10-28):** Lock time next to Snap; cycles up to 8/1 in Sync and 20 s in Free; the Tap tab (every per-tap parameter, filter type, Reso and Slope included), a proposed answer to **D5** (v3's way: no per-tap lists, a panel for the clicked tap), to confirm once the owner has tried it. Also fixes a touch bug where tapping a dot didn't keep it selected.
- **v2.31.0:** the header's preset name opens Tide's preset browser, ported as `palette_scope.rs`; a preset now defines the whole sound; and the R taps are drawn (field and Echo lane) when Time L/R is unlinked. Ping-pong isn't drawn: for a mono source the DSP's Ping changes nothing (see DECISIONS.md), so whether Ping should alternate a mono source is now a sound question for the owner.
- **v2.32.0:** Ping bounces a mono source. It runs as its own loop of odd and even repeats; X-feed is the width, and each tap places the repeats so a spread pattern swings as a whole. Mode changes crossfade the two loops, with no click. The Echo lane draws Ping as two sides, left up and right down. Ping is greyed out in Mono. See DECISIONS.md.
- **v2.33.0:** a new look the owner picked in a look lab: Mono and Paper, soft corners, underlined choices, ink controls with colour kept for the sound, Time | Pattern grouping; 36 px knobs; a spacing pass (the field fits the pattern, cards sized to their rows). See DECISIONS.md.
- Next: a second spacing pass (the owner: "some spacing still isn't perfect"), then the new functions (Phase 4).

- [ ] **A new page module** in the style of Tide's `editor_scope.rs`:
  - absolute layout in logical px through `r(o, s, x, y, w, h)`;
  - element sizes in `mod g`;
  - a region table per shape;
  - `Shape::pick`, with fit and no-overlap tests.
- [ ] **iOS host resize:** `EguiState::from_size(..).with_host_resize()`.
- [ ] **The touch layer (D1) on every control,** above all the drag surfaces that still follow one pointer: tap dots, XY pads and tracks. On touch, the whole cell is the hit area.
- [x] **Preset browser:** restyle `palette_v3.rs` to Tide's design (segmented All / Favorites / Recent, tinted selection). *(v2.31.0: Tide's browser ported as `palette_scope.rs`; palette_v3 keeps only the glue once v3 retires.)*
- [ ] **Retire the old editors:** remove v2 (`gui.rs`) and v3 (`gui_v3.rs`) and the `V3_PREVIEW` switch once the new page matches them. There's no "old UI" option, as with Tide.

### Phase 4 · New functions · size depends on D3
Proposed and decided on 2026-10-27.

**Progress, 2026-10-28:** the order is loop tone, Freeze, ducking, pattern tools, Euclidean, Motion (v2.30–v2.33.2 committed first as a checkpoint).
- **v2.34.0:** loop tone is built: one Tone knob in the Loop row, a one-pole low-pass left of centre and high-pass right of it, off at the centre (bit-identical to v2.33.2). See DECISIONS.md.
- Next: Freeze.
- **In:** loop tone, Freeze, ducking, pattern tools, Motion, real Euclidean spacing.
- **Out:** shift in the loop. You judged it would be messy and sound meh in a multitap.
- **Later, in an update after release:** 32 taps. Keep 16 for the redesign.
- **Not answered yet:** Swing (it came paired with Euclidean), and the other larger items.

Sizes are rough: S is a day or two, M about a week, L more than that.

**Small, and each fills a real gap:**

| Function | What it does | Why Afterecho needs it | Size | Where it goes |
|---|---|---|---|---|
| **Loop tone** | Low-pass and high-pass on the feedback only (Tide's FB tilt, [DSP_FOR_REUSE.md §5](../../../y-flanger/DSP_FOR_REUSE.md)). | Nothing filters the loop today unless Tape, Diffuse or Saturate is routed to FB, so repeats stay as bright as the first pass. Darkening repeats are the dub and tape sound. | S | Echo lane, Loop row |
| **Freeze** | Stops writing input and loops the current cluster at unity, bypassing the loop's saturation and colour so it doesn't wear away. | A performance control, and a good one on iPad. Afterecho's buffer already holds the whole cluster, so this is cheap. Entering and leaving need click-free fades. | S–M | Echo lane header |
| **Ducking** | Ducks the wet signal while the dry input is loud. | Long feedback stays usable in a mix. The FB curve already has an envelope follower to build on. | S | Output (the dashed slot) |
| **Pattern tools** | Act on every tap at once for the picked Y axis: Spread (alternate, ramp, random), Tilt (fade the taps out or in), Randomise (seeded, undoable), Reset. | Today, shaping 8–16 taps means 8–16 separate edits. These write the existing per-tap parameters from the GUI, so the engine doesn't change. | S–M | Picture header, for Field and Bars |

**Medium:**

| Function | What it does | Watch out for | Size |
|---|---|---|---|
| **Motion** | Tide's LFO shapes (Smooth, S&H, Glider…), tempo rate and envelope follower ([DSP_FOR_REUSE.md §3](../../../y-flanger/DSP_FOR_REUSE.md)). They drive a per-tap Scatter (each tap wanders on its own) and/or the Shape pad. This is the "show the modulation" idea from the playbook. | Wobble had to be bypassed in Fade mode, because moving a delay time retriggers the fades. In Fade and Jump, modulate pan, gain and cutoff, not time. | M |
| **Swing and real Euclidean** | Swing pushes every other tap late in Sync. Euclidean gets pulses, steps and rotate; today it's a copy of Linear. | It lives in `topology.rs`, which the visuals share, so they stay in step. | S–M |
| **Shift in the loop** | A frequency shifter on the feedback (Tide's Hilbert pair): repeats climb or fall in pitch without per-tap pitch shifting. | Keep the shift range small by default; large shifts turn into a siren. | M |

**Larger, later:**
- **32 taps:** deferred on 2026-10-27 to an update after release. If it ships, show a short warning when you go past 16, about CPU and memory at high sample rates and long times. Raise `NUM_TAPS` from 16 to 32 (M). What it costs:
  - Every per-tap array doubles: readers, 4 SVF stages per tap and channel, and the visualiser's arrays.
  - Per-tap parameters go from 128 to 256 (8 per tap). The new IDs are additive, so old sessions still load.
  - The cluster span is `NUM_TAPS × base`, and the buffer is 32 s (`MAX_DELAY_SECONDS`). 16 taps at a 2 s base already fill it. 32 taps need 64 s (about 98 MB for both channels at 192 kHz), or else the far taps drop out at long times.
  - CPU per tap doubles. Measure it on the iPad.
  - The page needs the dense rules mocked on the canvas: values only for the selected tap, numbers every fourth tap, and drawing across the bars on touch.
- **Reverse taps:** per-tap reverse windows (L). Risky in the same way the old pitch shifter was.
- **Pattern morph:** two stored tap patterns and one automatable knob between them (M–L).
- **MIDI resonator:** a note sets the base time to its period, so at short times the taps become a tuned comb (M). There's no MIDI input today.

**Not recommended:** per-tap pitch shifting. It was removed in v2.4 for flutter and AM artifacts (HANDOFF §8).

**My pick for the first pass:** loop tone, Freeze, ducking and the pattern tools, plus Motion as the one medium feature.

**Also open from July:**
- the feedback cycle as Auto, Free or Sync (already drawn in the Echo lane);
- whether to keep SATURATE.

### Phase 5 · iPad and release · medium
- [ ] **Hosts:**
  - iPad: AUM, Drambo, Logic for iPad, Cubasis;
  - desktop: Ableton, Reaper, Bitwig, FL Studio;
  - pluginval on desktop.
- [ ] **Release list (playbook §10):**
  - permanent bundle IDs;
  - an App Group for presets;
  - a 60 fps cap;
  - aufx;
  - what TestFlight needs.
- [ ] **Licences:**
  - The Tape model is ported from RE-DEEMER (ISC licence), so ship its notice.
  - `Cargo.toml` says GPL-3.0-or-later. Square that with the Soft Systems licensing plan; the question is shared with the other plug-ins.
  - Keep the Petal `.amxd` that sits in the repo root out of anything shipped.

---

## 4. What isn't covered yet
- Whether the new page is denser than Tide's sizes allow. Only the mock answers that.
- Whether nice-plug's host resize behaves in AUM, Logic and Drambo. The playbook lists this as unverified for Tide too.
- The iOS build of the current tree (I didn't try it).

---

## 5. Decisions for you

| # | Question | My recommendation |
|---|---|---|
| **D1** | **The kit.** Move Tide's `epo`, `theme` and `tokens` changes into `Dev/epoche-ui`, or copy them into Afterecho? | **Decided 2026-10-27 (owner): copy them, as Tide does.** epoche-ui is no longer used. The local copy is in `src/ui/`; see DECISIONS.md. |
| **D2** | **Identity.** Change the IDs to Soft Systems now? This breaks Ableton sessions that use the dev builds. | **Yes, now.** Nothing has shipped, and the IDs are permanent once on the App Store. Keep the crate name. |
| **D3** | **New functions.** Which ones? | **Decided 2026-10-27:** see Phase 4. 32 taps waits for an update after release; Swing and the larger items are still open. |
| **D4** | **Audio scope.** Only the clean-up (Phase 1), or sound changes too (SATURATE, FB curve, modulation)? | Do the clean-up first. Make sound changes one at a time against the baseline renders. |
| **D5** | **Per-tap lists.** Keep them as a page, or rely on the field plus last-touched? | Decide in the mock. Rows are the easier touch target for 16 taps. **Proposed in v2.30.0:** the lane's Tap tab (all of one tap's parameters, ‹ › to step), with no lists. Waiting on the owner's hand test. |
| **D6** | **Branches.** Merge `nice-plug-port` into `main` before starting, or start the redesign on a new branch from it? | Commit, merge into `main`, then work on `ui/scope-first`, as Tide does. |
