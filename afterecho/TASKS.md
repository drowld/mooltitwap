# TASKS — Afterecho
_State lives in docs/HANDOFF.md, the why in DECISIONS.md, the plan in docs/REDESIGN_PLAN.md. Branch `ui/scope-first`, local commits only (push on major milestones, owner's call)._

## In Progress
- [ ] **Owner hand test (v2.34–v2.47)** on desktop, all at once: Phase 4 (Loop tone, Freeze, Ducking, Pattern tools, Euclid + Swing, Motion), Ping (v2.32), the new look (v2.33), the preset browser (v2.40), v2.42 (Repeats, the Motion matrix, Hanken Grotesk) and v2.43 (Time view, Grid, two Steps with Glide, ear safety), v2.44 (fractional delay reads: Repitch glides, cycles a sample tighter), v2.45 (per-tap detune, Light/Clean), v2.46 (Detune Range, In/Out at 0 dB, full-page presets), v2.47 (Diffuse rebuilt; Cross → Diffuse with it), v2.48 (presets on the shared store: browse, Try/Back, Save as… once). v2.47.0 installed 2026-11-01. Step clicks: owner confirmed fixed (2026-11-01). Unit/A-B tested + screenshots; nothing heard or clicked by hand yet. Linear SOF-57.
- [ ] **v2.49: Afterecho on softsystems-ui** (owner's go 2026-11-06, branch `ui/softsystems-ui`, uncommitted): shared theme, fonts, ☰ Options panel, 16/12 frame, controls, the shared preset sheet, the contrast rule in a test. Lists by length are in (Y axis and LFO shape dropdowns, Spacing the vertical segment). "In this preset" is back, under the sheet's list. Left: owner check, then merge on his word. Linear SOF-224.
- [ ] **Beta readiness: everything from the crates** (crates hub, 2026-11-13: Afterecho is 4th to beta after Vortex, Tide and Strata; date: owner). One pass at a time, each built, tested and shown, committed on the owner's word. Branch `ui/softsystems-ui`. Done so far: presets (v2.48), UI (v2.49), pass 1 compatibility removed (v2.50), pass 2 cards and shape list (v2.52–2.53), the crate's SVF (v2.54).
  Beta-critical:
  1. [x] Name and ids (v2.55.0, SOF-92): `com.softsystems.afterecho` (CLAP, bundles, iOS), VST3 `SoftSystemsAftr1`, AU manufacturer `SfSy`, vendor Soft Systems; crate, folder, `Afterecho` struct and `AfterechoParams` renamed. 100 tests, bundles load, iOS check. Not committed, not installed.
  2. [x] Licence (v2.56.0): `softsystems-licence` wired by hand: `licence!("afterecho")` (the beta date comes with pass 3), the demo gate last on the output, checks at load, editor open and preset load, the ☰ panel's Licence section (Activate, offline, sign out). 100 tests, iOS check, screenshot. Not activated against the server, not committed.
  3. [x] Beta build (1.0.0-beta.1): `beta` feature on by default, `licence!("afterecho", beta_ends = …)`, release hardening (lto fat, codegen-units 1, path remaps in `.cargo/config.toml`, `@rpath/Afterecho` install name in `build.rs`). CLAP and VST3 pass `check-release-binary.py --beta`. **Blocker before any tester build: `BETA_ENDS` is a placeholder (2026-12-31), the owner sets the date.** The tests run the crate's `DemoGate::off()` (dev-dependency with `dev`), and the standalone passes the check too (the Licensing chat's fixes, nice-plug eaf0e7e). Gap sent: CFBundleVersion "1.0.0-beta.1" (bundler). Not committed.
  4. [ ] Beta UI and title: `paint::wordmark`, header laid out from its rect, `BetaUi` badge, banner and pop-up. Waits for nice-plug main: the hub merges claude/ui-beta → ui-halo → ui-wordmark.
  After:
  5. [x] The shared knob (owner 2026-11-13: the arc in the accent): every knob cell is `softsystems_ui::knob::knob` (name, dial, value; stepped params up to 16 positions show their dots), host gestures bracketed. Feedback keeps its own drag for the stop at 100 % and draws the crate's dial (crate gap: `Knob::detent` / `.over`, proposed by the UI chat). No halo yet (comes with the modulation passes); cells ≥ ~54 px to check then. 100 tests, iOS check, screenshots. Not committed.
  6. [ ] Saturate (`spikes/saturate/`, waiting for ears).
  7. [ ] Diffuse (`spikes/diffuse-crate/`, waiting for ears).
  8. [ ] Modulation engine on softsystems-mod: LFO 1–3, Follow, Step 1–2; taps as voices (Scatter).
  9. [ ] The matrix: targets, routes, Depth / Time (replaces SOF-244's bugs).
  10. [ ] Grid, halo, knob menu, tiles.
  11. [ ] Editors: curve, steps, follow; mod-to-mod routing.
  - Optional: the tape's 2× oversampler on the crate's half-band.
  Release gate: SPACES tables, TUBE and TRANSFO constants need the owner's clearance before a public build.
- [ ] **Factory presets: tune by ear** — the 18 approved presets (v2.41.0, `factory.rs`) are written and tested against their descriptions; none heard yet. Linear SOF-147.

## Next
1. [ ] Fold in the hand-test feedback. Linear SOF-84. First notes (2026-11-01) in v2.46/v2.47: preset sheet over the whole page, In/Out with 0 dB at the top, accent ring no longer cropped, Detune Range 1–12 st (and short detuned taps fixed), Diffuse rebuilt. Same ring crop in Morpheus, merge, cumulus, phasma, y-flanger (their own repos).
2. [ ] iPad: install v2.47.0 (still on v2.34.0; needs the iPad unlocked), hand-test touch, measure CPU with Motion's Cutoff/Reso moving and with detune (Light and Clean, 16 taps). Linear SOF-85.
3. [ ] Second spacing pass (owner: "spacing still not perfect"). Linear SOF-86.
4. [ ] Restyle the Settings menu to the new look (still old epo chips). **On hold:** the ☰ panel will come from softsystems-ui (below). Linear SOF-87.
5. [ ] Matrix audit (crates hub, 2026-11-05, read from code, nothing run; 7 bugs, none high): cell drag sticks on slow drags (no accumulator, `gui_scope.rs:2211`); no clear on iPad cells; negative Gain depth / Polarity < 0 silent on Gain with no hint; the Motion tick grid restarts every block, so Step Glide runs fast with block sizes not a multiple of 32 (`lib.rs:1048`); Amount/Polarity unsmoothed; the grid shows raw cells, not the depth heard; a hidden Env LFO shape is dead. Most go with the shared routing (after Vortex); owner (2026-11-05): wait for the shared routing, fix none before. List: the hub's `matrix-audit-bugs-by-plugin.md`. Linear SOF-244.
5. [ ] Touch layer on the scope page: multitouch for drag surfaces (dots, pads, ring, range pins) — REDESIGN_PLAN Phase 3 / D1. Linear SOF-88.
7. [ ] Phase 5: hosts (AUM, Drambo, Logic iPad, Cubasis; Ableton, Reaper, Bitwig, FL; pluginval), release list (bundle IDs, App Group, 60 fps cap), licences (RE-DEEMER notice, GPL vs Soft Systems plan). Linear SOF-90.
8. [ ] Verify Fade mode thoroughly with feedback interaction. Linear SOF-91.
- Later (after release): 32 taps. Linear SOF-93 (Backlog).

## Open questions (owner)
- Keep SATURATE?
- Merge / push `ui/scope-first` (owner's call).

## Blocked
- iPad installs: devicectl refuses while the iPad is locked or "not unlocked recently" — unlock first.

## Done (recent)
- [x] v2.52.0 Crate pass 2: the crate's cards; LFO shapes as icons and names. 100 tests, iOS check, screenshots.
- [x] v2.51.0 One page shape: Tall removed (owner: Afterecho always runs in a host panel). 100 tests, iOS check, screenshots. Commit 8c1ab85.
- [x] v2.50.0 Old editors and every compatibility path removed (crate adoption pass 1); 101 tests, iOS check, screenshots. Commit 7014573. (SOF-89)
- [x] v2.48.0 Presets on the shared softsystems-presets crate (`presets.rs` 1,533 → ~530 lines); parity-checked on copies of the real folders; loads send only changed params; factory `author` "Soft Systems". Not committed, not installed. The edited check is the crate's since its float-grid fix (nice-plug 780e520). (SOF-205)
- [x] v2.47.0 Diffuse rebuilt from the listening test (candidate C): six short drifting allpasses, wide, the loops read early by its delay, Cross → Diffuse too (SOF-199); with v2.46: Detune Range 1–12 st, In/Out 0 dB at the top, full-page preset sheet, accent ring (SOF-84 notes)
- [x] v2.45.0 Per-tap detune ±100 ct (Tap tab knob, Y axis), Settings → Sound → Detune Light (WSOLA) / Clean (phase vocoder) (SOF-156)
- [x] v2.44.0 Fractional delay reads fixed (Repitch glides no longer step; loops come round on time); count→∞ handover dip fixed (SOF-60)
- [x] v2.43.0 Time view (Lock time removed) + Snap Grid; field/lane aligned with the cards; LFO 1–3 + Step 1–2 with Glide; smooth cutoff/Q glide; Ear guard, Feedback stop at 100 %, NaN guard, loop DC blockers, Clear (SOF-157)
- [x] v2.42.0 Repeats (1–16 or ∞, new default 4; old sessions/presets load as ∞) + Feedback knob; Motion matrix (LFO 1–4, Follow, Step × 10 targets, Amount/Polarity); Field/Bars pinned right; Slope Y axis; last-touched card gone; Hanken Grotesk default. (SOF-155)
- [x] v2.41.0 First factory set: 18 presets written, read-only (SOF-147 stays open for the ear pass)
- [x] v2.40.0 Soft Systems preset system: library + sheet over field/lane, Time/Cross facets, Try/Keep/Back, "In this preset", folder move, session name (SOF-146)
- [x] v2.39.0 Motion: one source moves every tap's Pan/Gain/Cutoff/Reso (SOF-83)
- [x] v2.38.0 Euclid (Steps, Rotate, ring) + Swing (SOF-82)
- [x] v2.37.0 Pattern tools, Reso/Type Y axes, per-tap glides + type crossfade (SOF-69)
- [x] v2.36.0 Ducking (SOF-68)
- [x] v2.35.0 Freeze (SOF-59)
- [x] v2.34.0 Loop tone (SOF-58)
- [x] v2.33.0–2.33.2 new look (Mono/Paper, underlined choices, ink), spacing passes, Bars tap/double-tap mute, preset browser restyle, Time holds still on L/R split
- [x] v2.32.0 Ping bounces a mono source (two loops, click-free mode switches)
- [x] v2.30–2.31 Lock time, longer cycles, Tap tab, preset browser (`palette_scope.rs`, replaced in v2.40), whole-sound presets, R taps drawn
- _Older (v2.17–v2.29): see DECISIONS.md and git log._
