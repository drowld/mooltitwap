# TASKS — mooltitwap (Afterecho)
_State lives in docs/HANDOFF.md, the why in DECISIONS.md, the plan in docs/REDESIGN_PLAN.md. Branch `ui/scope-first`, local commits only (push on major milestones, owner's call)._

## In Progress
- [ ] **Owner hand test (v2.34–v2.45)** on desktop, all at once: Phase 4 (Loop tone, Freeze, Ducking, Pattern tools, Euclid + Swing, Motion), Ping (v2.32), the new look (v2.33), the preset browser (v2.40), v2.42 (Repeats, the Motion matrix, Hanken Grotesk) and v2.43 (Time view, Grid, two Steps with Glide, ear safety), v2.44 (fractional delay reads: Repitch glides, cycles a sample tighter), v2.45 (per-tap detune, Light/Clean). v2.44.0 installed 2026-11-01 (`81de0a8`); v2.45.0 built, not installed. Step clicks: judge by ear (not provable by test). Unit/A-B tested + screenshots; nothing heard or clicked by hand yet. Linear SOF-57.
- [ ] **Factory presets: tune by ear** — the 18 approved presets (v2.41.0, `factory.rs`) are written and tested against their descriptions; none heard yet. Linear SOF-147.

## Next
1. [ ] Fold in the hand-test feedback. Linear SOF-84.
2. [ ] iPad: install v2.45.0 (still on v2.34.0; needs the iPad unlocked), hand-test touch, measure CPU with Motion's Cutoff/Reso moving and with detune (Light and Clean, 16 taps). Linear SOF-85.
3. [ ] Second spacing pass (owner: "spacing still not perfect"; Tall Time group loose). Linear SOF-86.
4. [ ] Restyle the Settings menu to the new look (still old epo chips). Linear SOF-87.
5. [ ] Touch layer on the scope page: multitouch for drag surfaces (dots, pads, ring, range pins) — REDESIGN_PLAN Phase 3 / D1. Linear SOF-88.
6. [ ] Retire the old editors: `gui.rs` v2 drawing, `gui_v3.rs`, `palette_v3.rs` glue, `V3_PREVIEW` (Phase 3). Linear SOF-89.
7. [ ] Phase 5: hosts (AUM, Drambo, Logic iPad, Cubasis; Ableton, Reaper, Bitwig, FL; pluginval), release list (bundle IDs, App Group, 60 fps cap), licences (RE-DEEMER notice, GPL vs Soft Systems plan). Linear SOF-90.
8. [ ] Verify Fade mode thoroughly with feedback interaction. Linear SOF-91.
9. [ ] Coordinated rename to Afterecho (struct, crate/folder, bundler key, docs; IDs stay unless owner accepts broken sessions). Linear SOF-92.
- Later (after release): 32 taps. Linear SOF-93 (Backlog).

## Open questions (owner)
- Keep SATURATE?
- Rename the Linear project "Mooltitwap" → "Afterecho"?
- Merge / push `ui/scope-first` (owner's call).

## Blocked
- iPad installs: devicectl refuses while the iPad is locked or "not unlocked recently" — unlock first.

## Done (recent)
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
