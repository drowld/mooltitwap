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
- [ ] **A preset defines the whole sound:** in `presets::apply`, parameters a preset doesn't mention load at their default.
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
- [ ] Put Wide and Tall on the design canvas, the tall one on a real iPad board (1112 × 834 pt).
- [ ] Decide:
  - the Y picker;
  - Echo lane gestures;
  - where the per-tap lists go (D5);
  - what last-touched shows;
  - how the right column is split.
- [ ] Measure in points: dial about 29 pt or more, text about 11 pt or more, whole-cell touch targets (aiming for 44 pt).

### Phase 3 · Build the new page · large
- [ ] **A new page module** in the style of Tide's `editor_scope.rs`:
  - absolute layout in logical px through `r(o, s, x, y, w, h)`;
  - element sizes in `mod g`;
  - a region table per shape;
  - `Shape::pick`, with fit and no-overlap tests.
- [ ] **iOS host resize:** `EguiState::from_size(..).with_host_resize()`.
- [ ] **The touch layer (D1) on every control,** above all the drag surfaces that still follow one pointer: tap dots, XY pads and tracks. On touch, the whole cell is the hit area.
- [ ] **Preset browser:** restyle `palette_v3.rs` to Tide's design (segmented All / Favorites / Recent, tinted selection).
- [ ] **Retire the old editors:** remove v2 (`gui.rs`) and v3 (`gui_v3.rs`) and the `V3_PREVIEW` switch once the new page matches them. There's no "old UI" option, as with Tide.

### Phase 4 · New functions · size depends on D3
**This needs your list.** Ideas already on the backlog, or reusable from Tide:
- **Modulation** of tap positions, the SHAPE XY pad or the filter. Tide's LFO shapes (Smooth, S&H, Glider…), tempo rate, envelope follower and retrigger are documented in [DSP_FOR_REUSE.md §3](../../../y-flanger/DSP_FOR_REUSE.md). Wobble was removed in v2.17.5, partly because it fought Fade mode. Any new modulation needs an answer for Fade mode.
- **Ducker:** a sidechain envelope follower that ducks the wet signal.
- **Real Euclidean spacing:** it's a copy of Linear today.
- **Feedback cycle as AUTO, FREE or SYNC,** from the ECHO strip sketch (it lives in the Echo lane).
- **FB tilt** (low-pass or high-pass on the feedback only), [DSP_FOR_REUSE.md §5](../../../y-flanger/DSP_FOR_REUSE.md).
- **SATURATE:** keep it or remove it (your open question from July).

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
| **D1** | **The kit.** Move Tide's `epo`, `theme` and `tokens` changes into `Dev/epoche-ui` (additive, so cumulus doesn't break), or copy them into Afterecho? | **Move them into epoche-ui.** A third diverging copy makes every later fix three fixes. It costs one merge pass, and cumulus needs a check afterwards. |
| **D2** | **Identity.** Change the IDs to Soft Systems now? This breaks Ableton sessions that use the dev builds. | **Yes, now.** Nothing has shipped, and the IDs are permanent once on the App Store. Keep the crate name. |
| **D3** | **New functions.** Which ones? | Your call. Settle them before the mock. |
| **D4** | **Audio scope.** Only the clean-up (Phase 1), or sound changes too (SATURATE, FB curve, modulation)? | Do the clean-up first. Make sound changes one at a time against the baseline renders. |
| **D5** | **Per-tap lists.** Keep them as a page, or rely on the field plus last-touched? | Decide in the mock. Rows are the easier touch target for 16 taps. |
| **D6** | **Branches.** Merge `nice-plug-port` into `main` before starting, or start the redesign on a new branch from it? | Commit, merge into `main`, then work on `ui/scope-first`, as Tide does. |
