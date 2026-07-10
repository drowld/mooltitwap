# TASKS — mooltitwap
_State & detail live in docs/HANDOFF.md. This is the live todo list (role.md format)._

## In Progress
- (nothing active — landed at v2.17.1 on branch `nice-plug-port`)

## Next
- [ ] DAW hand-test the v2.17.1 interactions (TAPS drag/scroll, FB TIME unlock+drag, preset browser open→click-outside→reopen, XFEED/DIFFUSE knobs) — verified only by code + screenshot, not live
- [ ] Merge `nice-plug-port` → `main` once hand-tested (decision left to owner)
- [ ] Replace Wobble with Ducker (sidechain envelope follower) — never started
- [ ] Preset browser polish: tags / search / delete / rename / favorites (MVP only ships prev-next + save-as)
- [ ] Per-tap FILTER Q/slope layout is dense-but-works — revisit if it reads cramped
- [ ] Verify Fade mode thoroughly with feedback interaction
- [ ] Implement real Euclidean rhythm spacing

## Done (recent)
- [x] v2.17.1 — restored TAPS/FB-TIME editing, viz range adapts in sync+free, rounded pill bars, XFEED/DIFFUSE knobs, preset-browser z-order fix, footer un-clipped
- [x] v2.17.0 — hamburger menu + persisted dark/light, aspect-locked resize grip, named preset system (src/presets.rs)
- [x] v2.16.x — full editor rebuilt on epo widgets: cards, real stems/grid viz, XY pads, grid drag, per-tab gradient tap colors, FILTER/XFEED/DIFFUSE tabs
- [x] **Framework port nih-plug → nice-plug + nice-plug-egui + epoche-ui** (egui 0.31→0.34); DSP modules unchanged
- [x] Local git checkpoint of the whole v2.2–v2.15 working tree before the port (`be061d8` on main)
