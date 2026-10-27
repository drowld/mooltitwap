# TASKS — mooltitwap
_State & detail live in docs/HANDOFF.md. This is the live todo list (role.md format)._

## In Progress
- **iPad/AUv3 build path OPEN (v2.18.0)** — runs fullscreen on the iPad Pro 11 simulator as a real AUv3. Build+install: `cargo nice-plug bundle-ios mooltitwap` (booted sim, ad-hoc, no creds). Config: `impl AuPlugin` + `nice_export_auv3!` in lib.rs, `bundler.toml` at workspace root, target-cfg deps in Cargo.toml (iOS=auv3 / no standalone / no opengl), main.rs iOS-stubbed. **INSTALLED on the physical iPad (com.tlkm.afterecho, team 6ABFQC732N).** Reusable path in DECISIONS.md 2026-07-11: xcodegen mint project at `scratchpad/afmint/` → `xcodebuild -allowProvisioningUpdates` → `cargo nice-plug bundle-ios mooltitwap --device`. Free-team profiles expire ~2026-07-18 → re-mint then. First launch: unlock iPad + trust developer (Settings ▸ General ▸ VPN & Device Management). NEXT: hand-test true multitouch on the hand-painted pan/gain tracks + knob_rt in an AU host (AUM/Cubasis/GarageBand)
- **απόηχος v3 port — PAN page preview shipped (v2.18.0)**: new `src/gui_v3.rs` behind `gui.rs::V3_PREVIEW=true` (flip false → v2.17 editor returns intact). Canvas 1194×834 (Figma ÷2), light theme seeded, all PAN-page controls live (left panel, stems viz, XY pads, scroll rows w/ full-cell mute hits + hand-painted pan sliders). Owner round-2 feedback applied: 48px knobs, per-cell centered chips, inactive taps HIDDEN not dimmed, pads centered, **NEW `sync_r` param** — per-channel L/R sync (DSP `base_time_seconds_mode` + viz updated; R follows L when time-linked, chip disabled). Round 3: custom `v3_button` to Figma spec (49×27, r8, full-cell hits) replaces epo chips. Round 4: tabs now use the KIT's `epo::card_tabs` (cumulus-style, proper size — my hand-painted chips were wrong); **FILTER page shipped as 2-up squared blocks** (## / type / slope chips + CUT & Q sliders, 8 scrolling rows, normalized API for skewed cutoff/Q); tap rows tightened pitch 54→50. Desktop bundle FILENAME is now `Afterecho.*` (bundler.toml name) but Plugin::NAME + VST3/CLAP IDs unchanged → DAW still shows "Mooltitwap", sessions intact; stale mooltitwap.vst3/clap removed from install dir. NOT yet in v3: SPACE page (XFEED+DIFFUSE merged), hamburger menu contents, preset pill interaction, resize grip, Grid view, Plugin::NAME rename. v2 editor has NO Sync-R control (param defaults Free = old behavior). Round 5: FILTER CUT/Q now KNOBS (were sliders) matching Figma; tap rows tightened 50→46; renamed to Afterecho (display). ("DELAY spacing" confusion resolved — owner meant the tap rows, done.)

## Rename to Afterecho (in progress)
- [x] Display rename: `Plugin::NAME`→"Afterecho", CLAP desc, bundle files Afterecho.* (IDs kept → sessions safe)
- [ ] DEFERRED coordinated rename: struct `Mooltitwap`, crate/folder `mooltitwap`, `bundle mooltitwap` cmds, bundler.toml `[mooltitwap]`, docs, git. Invasive — owner flagged folder rename as a problem. Do as one careful pass
- [ ] Consider whether to also change VST3_CLASS_ID/CLAP_ID (NO unless we accept breaking every existing session)

## ⭐ NEXT PHASE — custom multitouch GUI (owner-directed, like cumulus)
- [ ] Convert v3's single-pointer `ui.interact` elements to raw-touch so 2+ fingers work at once on iPad. Kit plumbing exists: `epo::tap_touch(ui, hit_rect, id)` (one-finger tap), `epo::knob_rt` (raw-touch knob), iOS touch dispatch in nice-plug-egui `editor_ios.rs`. Targets: `v3_button` (all chips/toggles/mute pills/A-B) → tap_touch; hand-painted PAN/GAIN tracks + FILTER/SPACE need a raw-touch horizontal-slider helper (claim a finger, drag X — mirror knob_rt); XY SHAPE pads → raw-touch claim (both at once). Study cumulus editor.rs (uses tap_touch/raw-touch pervasively). Already multitouch: all knobs (knob_ex→knob_rt), preset_slot, card_tabs.

## v3 — preset palette shipped (hand-test next)
- [x] Tag field now validates on Return/tap-away (was: soft-keyboard Enter never reached the global key read → tags never committed). Fixed via TextEdit `lost_focus()`, Esc still cancels
- [x] Card tags (DELAY/STEMS/SHAPE) fill PAPER (page bg) so they fade into the backdrop; A/B gap widened (~22px) so they don't read as joined
- [x] Full spotlight preset palette ported (src/palette_v3.rs): search, ALL/FAV/RECENT, save-as/rename/delete/fav/tags, keyboard + footer keycaps. Header pill = epo::preset_slot
- [ ] HAND-TEST the palette: save a preset (disk icon), load (click/Enter), rename (✎), delete (✕✕), favorite (star), add/remove tag, ◀/▶ step, dirty * appears on edit. Desktop + iPad
- [ ] Dark mode: toggle DARK/LIGHT (sun or menu), switch SHADE COOL/WARM/INK in the menu — confirm on iPad
- [ ] A/B now 31×27 w/ gap — confirm placement reads right

## v3 — hand-test on iPad (touch mode now ON)
- [ ] Tab TEXT −/+ now resizes tabs on iPad (touch fix) — confirm it works + pick a good default size
- [ ] Cutoff→20 Hz knob lock: likely fixed by touch/raw-touch switch — CONFIRM on device
- [ ] A/B compare: tweak A, tweak B, toggle to confirm recall (logic sound, not tested live)
- [ ] Raw-touch knobs feel (all knobs switched to knob_rt on iPad) + 44pt hit floors

## Deferred design passes
- [ ] DARK MODE for v3 — apply Epoché dark palette (see `y-flanger/design/dark-modes.html`: Warm graphite ★ foundation + leveled accents). v3 currently light-only
- [ ] v3: preset-pill wiring (prev/next/save), resize grip, Grid view, fuller hamburger menu (theme/font/zoom like v2)
- [x] v3 SPACE page (XFEED+DIFFUSE merged) — done; hamburger menu exists w/ TAB TEXT + theme

## Open questions (owner)
- SATURATION (soft clip): owner unsure it's needed — kept for now, decide during the redesign
- Feedback / FB TIME UX feels unintuitive to the owner — redesign discussion pending
- XFEED + DIFFUSE value formatting (percent readouts) — deprioritized by owner

## Next
- [ ] **← NEXT UP:** DAW hand-test v2.17.5 (one pass covers .3/.4/.5): taller tap rows + scroll, stepped values drag feel (steps quantize host automation — if 0.1 dB GAIN feels notchy, switch to display-only formatting), wobble gone, Sync caption BARS/BEATS
- [ ] ECHO strip (feedback footer redesign, sketch agreed 2026-07-11): CYCLE segmented AUTO/FREE/SYNC replaces LOCK + right-click; FEEDBACK readout "−X dB/repeat · ~N echoes" + red >100% arc zone; dashed cycle marker in Stems viz. UI-only — `feedback_time_locked`+`feedback_time_mode` already encode the states
- [ ] DAW hand-test v2.17.4 viz fixes: drag a tap in GRID (ball tracks pointer AND audio lands on the shown gridline), snap lands on base-time multiples, mute the last tap / offset a tap with FB locked and check echoes still align, PING at 0% crossfeed must not alternate sides
- [ ] Ghost/arrow indicator for taps clamped at the viz right edge (out-of-range) — deferred from v2.17.4
- [ ] DAW hand-test v2.17.3 in Ableton by ear/waveform — esp. the reshaped layout (in-card footer drag targets: TAPS drag/scroll, FB TIME unlock+drag; SHAPE link; per-tap mute), preset browser open→click-outside→reopen, XFEED/DIFFUSE knobs, and resize/zoom (aspect-lock at 1080×660) — verified by code + screenshot, not live
- [ ] Layout polish nits if any read off in the DAW: left-panel chip rows are hug-width (not full-width segmented like v2.15 — kept epo chip style); FILTER row is dense at RIGHT_W=284
- [ ] Merge `nice-plug-port` → `main` once hand-tested (decision left to owner)
- [ ] Ducker (sidechain envelope follower) — optional; Wobble itself was removed in v2.17.5, left-panel space is free
- [ ] Preset browser polish: tags / search / delete / rename / favorites (MVP only ships prev-next + save-as)
- [ ] **απόηχος / Afterecho redesign port** (owner's Figma "Mooltitwap" file, canvas 2388×1668 @2x = iPad Pro 11, ratio 1.432): new columns 490/1032/758 + 24px gutters, tap rows h100/pitch108 w/ scroll fade, FILTER blocks h245, taps-count as slider. IMPLEMENTATION NOTES — touch-correctness is code-side, the Figma file needs NO further edits (owner decision 2026-07-11): menu items render 96px tall (mockup shows 47), tab strip becomes discrete chips w/ padded hits, row-leading tap pill gets a full-cell (130×100) hit area — interact on the cell rect, paint the pill; rename touches plugin name + CLAP/VST3 IDs + preset folder
- [ ] Verify Fade mode thoroughly with feedback interaction
- [ ] Implement real Euclidean rhythm spacing

## Done (recent)
- [x] v2.17.6 — FILTER tab two-line rows: chips (mute/type/slope) on top, full-width CUTOFF + Q sliders below; un-cramps the old 5-control row at RIGHT_W=284
- [x] v2.17.5 — Wobble removed entirely (params+DSP+UI; Ducker NOT built), per-tap rows taller + scrollable, per-tap value steps (PAN ±100/1, GAIN 0.1 dB, CUTOFF 1 Hz, Q 0.1 max 12), Stems axis headroom 2×→1.1×, Sync caption in BARS/BEATS (was "64/1"-style raw beats). See DECISIONS.md 2026-07-11
- [x] v2.17.4 — viz now mirrors DSP: time_offset unit fix (span not range — timeline/drag/snap/cross-view all agree), echo cycle uses the DSP's true cluster span (offsets+mutes+in-range, 10 ms floor), Grid caption shows its own axis range, ping-pong viz is the exact (1+dᵏ)/2 blend gated on crossfeed. Position/time math single-sourced in topology.rs (gui.rs::compute_positions deleted). See DECISIONS.md 2026-07-11
- [x] v2.17.3 — full layout parity with v2.15 body geometry: card widths 224/284, window 660, CARD_BODY_H 564, footer moved back inside the right card, LEFT panel rebuilt on the v2.15 section rhythm + dividers, CENTER viz/shaping split inverted (tall viz again), SHAPE-link column 56px. See DECISIONS.md 2026-07-10
- [x] v2.17.2 — UI placement parity vs pre-port v2.15: restored SHAPE link toggle + per-tap mute (2 dropped controls), preset-pill centering, hero-knob size, dB/crossfeed captions, SATURATION label, even 3-thirds footer (measured centering). See docs/UI_PARITY_AUDIT.md
- [x] v2.17.1 — restored TAPS/FB-TIME editing, viz range adapts in sync+free, rounded pill bars, XFEED/DIFFUSE knobs, preset-browser z-order fix, footer un-clipped
- [x] v2.17.0 — hamburger menu + persisted dark/light, aspect-locked resize grip, named preset system (src/presets.rs)
- [x] v2.16.x — full editor rebuilt on epo widgets: cards, real stems/grid viz, XY pads, grid drag, per-tab gradient tap colors, FILTER/XFEED/DIFFUSE tabs
- [x] **Framework port nih-plug → nice-plug + nice-plug-egui + epoche-ui** (egui 0.31→0.34); DSP modules unchanged
- [x] Local git checkpoint of the whole v2.2–v2.15 working tree before the port (`be061d8` on main)
