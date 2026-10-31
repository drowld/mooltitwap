# TASKS — mooltitwap (Afterecho)
_State lives in docs/HANDOFF.md, the why in DECISIONS.md, the plan in docs/REDESIGN_PLAN.md. Branch `ui/scope-first`, local commits only (push on major milestones, owner's call)._

## In Progress
- [ ] **Owner hand test (v2.34–v2.41)** on desktop, all at once: Phase 4 (Loop tone, Freeze, Ducking, Pattern tools, Euclid + Swing, Motion), Ping (v2.32), the new look (v2.33) and the preset browser (v2.40). Installed 2026-10-31 (v2.41.0, `b2930f2`). Unit/A-B tested + screenshots; nothing heard or clicked by hand yet. Linear SOF-57.
- [ ] **Factory presets: tune by ear** — the 18 approved presets (v2.41.0, `factory.rs`) are written and tested against their descriptions; none heard yet. Linear SOF-147.

## Next
1. [ ] **Fix fractional delay reads** (found 2026-10-29): `DelayLine::read` interpolates backwards (2-sample jump at each whole sample) → small jumps in Repitch glides; feedback cycle runs 1 sample long. Changes glide sound: own version + A/B. Linear SOF-60.
2. [ ] Fold in the hand-test feedback. Linear SOF-84.
3. [ ] iPad: install v2.41.0 (still on v2.34.0; needs the iPad unlocked), hand-test touch, measure CPU with Motion's Cutoff/Reso moving. Linear SOF-85.
4. [ ] Second spacing pass (owner: "spacing still not perfect"; Tall Time group loose). Linear SOF-86.
5. [ ] Restyle the Settings menu to the new look (still old epo chips). Linear SOF-87.
6. [ ] Touch layer on the scope page: multitouch for drag surfaces (dots, pads, ring, range pins) — REDESIGN_PLAN Phase 3 / D1. Linear SOF-88.
7. [ ] Retire the old editors: `gui.rs` v2 drawing, `gui_v3.rs`, `palette_v3.rs` glue, `V3_PREVIEW` (Phase 3). Linear SOF-89.
8. [ ] Phase 5: hosts (AUM, Drambo, Logic iPad, Cubasis; Ableton, Reaper, Bitwig, FL; pluginval), release list (bundle IDs, App Group, 60 fps cap), licences (RE-DEEMER notice, GPL vs Soft Systems plan). Linear SOF-90.
9. [ ] Verify Fade mode thoroughly with feedback interaction. Linear SOF-91.
10. [ ] Coordinated rename to Afterecho (struct, crate/folder, bundler key, docs; IDs stay unless owner accepts broken sessions). Linear SOF-92.
- Later (after release): 32 taps. Linear SOF-93 (Backlog).

## Open questions (owner)
- Keep SATURATE?
- Rename the Linear project "Mooltitwap" → "Afterecho"?
- Merge / push `ui/scope-first` (owner's call).

## Blocked
- iPad installs: devicectl refuses while the iPad is locked or "not unlocked recently" — unlock first.

## Done (recent)
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
