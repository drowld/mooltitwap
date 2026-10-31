# mooltitwap — Decisions

_Newest first. Each entry: date · what we decided · why · alternative rejected._

## 2026-10-31 · The Soft Systems preset system (v2.40.0)

Vortex's preset system (`Vortex/docs/PRESET_SYSTEM.md`) is the new design
language for presets; the owner approved the mockup of it in Afterecho and
asked for it built as mocked.
- **The library and file format are Vortex's** (`presets.rs`, ported): format
  2 with plugin id, kind, tags, facets, notes; atomic writes; nothing
  overwritten silently; `library.json` for favourites, recent, the view and
  Try. Factory presets are served from the binary, read-only (`factory.rs`).
- **No blobs**: everything an Afterecho preset holds is a parameter. Freeze
  stays out of presets (unchanged since v2.35).
- **Facets: Time (Sync | Free, from `sync_mode`) and Cross (Mix | Ping |
  Diffuse, from `crossfeed_mode`).** The mock said "Loop"; the page calls
  that row Cross, so the filter does too.
- **The sheet covers the field and the lane**; the Time, Colour and Output
  cards stay in view and move while you try presets. Wide: collections and
  facets on the left, the list, an inspector column on the right. Tall: the
  inspector under the list, holding Init and Save (spec §6.2).
- **"In this preset"** (an Afterecho addition to the spec): six lines read
  from the preset (taps and spacing, time, Cross and feedback, Tone and
  colour, Motion, Duck and Mix). Offered to the spec for Vortex and Tide.
- **Picks are ink in both looks**, as everywhere on the page (v2.33), not the
  spec's accent-in-light colour law.
- **Folders**: presets move to `~/Library/Application Support/Soft
  Systems/Afterecho/Presets`; the old `Mooltitwap/presets` files are copied
  once and never deleted. The look (`appearance.json`) stays where it was.
- **The loaded preset is saved with the host session** (`preset-session`),
  so a reopened project names it.
- **Edited compares floats on their step grid**: the taps' Q default (0.707)
  sits off its 0.01 grid, so a freshly loaded preset read as edited before.
- **No factory presets yet**: the list (names, sounds) waits for the owner.
- Rejected: keeping Tide's spotlight palette (`palette_scope.rs`, removed) —
  the spec makes one browser for every plug-in.

## 2026-10-29 · Motion: one source moves every tap (v2.39.0)

The last Phase 4 function, built as proposed and approved by the owner (it
fills the lane's Motion tab, empty since v2.29).
- **One source moves every live tap's Pan, Gain, Cutoff and Reso** around the
  values set for them, each with its own depth (0 = still). Pan moves up to
  hard L or R either side, Gain only dips (up to 24 dB, so Motion never
  boosts or clips), Cutoff ±4 octaves, Q up to ×/÷4 (`motion.rs`, shared by
  the engine and the page).
- **Not time.** Moving delay times is the pitch wobble removed in v2.17.5
  ("not so sensitive for that plugin") and it breaks Fade and Jump. Rejected:
  a small Time depth in Repitch only.
- **Sources:** Sine, Tri, Ramp, Square, S&H (a new random value each cycle,
  each tap its own), Glide (Tide's Glider: random values eased together),
  and Env, the dry input's level (10 ms up, 200 ms down, Tide's curve), so
  playing harder moves the taps. Tide's Smooth was left out: it's close to
  Glide.
- **Rate follows Time's Sync,** as the feedback cycle does: 0.05–8 Hz in Free,
  note values from 8 bars to 1/16 (dotted and triplet too) in Sync, where
  the cycle sits at the song's position while the host plays. Two params
  (`motion_rate`, `motion_div`) so each reads right in a host; one knob shows
  whichever runs.
- **Scatter** spreads the taps' phases round the cycle (0: together, 100 %: a
  whole cycle across them, so the movement travels through the cluster).
  Env ignores it.
- **The engine re-aims the taps every 32 samples** and the per-tap glides
  (v2.37) smooth the steps. A Square at full depth on Pan, Gain and Cutoff
  over a held tone: 1.32× the still tone's sharpest bend; with the glides
  off, 544× at a square edge. Ping's repeats follow the moved pan too.
- **The field shows it:** each tap's dot is where Motion has it now, its set
  value (what a drag moves) a ring joined to it; Bars show an ink dot
  against the bar. The engine reports each tap's motion through
  `PetalParams::motion_now` (plain atomics). The Motion tab: the source as a
  vertical list, then Rate, Scatter, a "Taps now" strip and the four depths;
  in Tall it takes the whole lane, as the Tap tab does.
- **Off is off:** with every depth at 0, the 21-case comparison with v2.38.0
  is bit-identical. Presets store Motion.
- Not measured yet: CPU on the iPad while Cutoff or Reso move (the filters
  are recomputed then).

## 2026-10-29 · Euclidean spacing and Swing (v2.38.0)

The fifth Phase 4 function, built as proposed and approved by the owner; it
answers the plan's open Swing question (yes, with Euclid).
- **Euclid is real now** (it was a copy of Lin): Steps (2–16, in Time-knob
  steps) and Rotate; the Taps knob's taps spread over the steps as evenly as
  possible, E(k, n) by Bresenham's spread (`topology::euclid_steps`). The dry
  note is step 0, so a tap that lands there sounds at step n, the next
  pattern's start (as Lin's last tap does). 3 of 8 gives the tresillo, 4 of
  16 four on the floor.
- **Taps beyond Steps are silent** (`PetalParams::taps_heard`, shared by the
  engine, the Echo lane and the page, which hides them); the ring says how
  many ("of 8 · 4 silent"). Rejected: keeping Steps ≥ Taps, which couples two
  knobs and fights automation.
- **In Auto the cycle is the whole pattern** (Steps × Time, the longer side
  when L/R is split) through a floor on the cluster span
  (`PetalParams::pattern_secs`), so a turned pattern whose last tap lands
  early still loops on its length.
- **The ring replaces the Shape pad in Euclid** (the pad has nothing to shape
  there): the steps round a circle, the dry note's step ringed at the top,
  the taps as dots in their colours, "5 of 16" in the middle, Steps and
  Rotate knobs under it. Dragging round the ring turns the pattern; the
  angle travelled counts in steps, past a whole turn too.
- **Swing**, 50 % (Straight) to 75 % as on an MPC: a tap on an odd step (the
  "e" of each beat) lands late by (2·swing − 1) of a step. In Lin that's
  every other tap, moved toward the next one by that share of their gap, so a
  bent Shape still swings. Lin and Euclid only; greyed in Exp and Log, which
  have no grid. It sits in the Time group's slot under L/R, which nothing else
  uses, so nothing on the card moves.
- **Straight changes nothing:** with Swing at 50 % and Lin, the 21-case
  comparison with v2.37.0 is bit-identical. Presets saved with Euclid picked
  (which sounded like Lin) now sound Euclidean.
- Steps stop at 16 because the tap grid is 16 × Time; longer patterns come
  with the deferred 32-tap update (or use a longer Time).

## 2026-10-29 · Pattern tools, Reso and Type on the field, per-tap glides (v2.37.0)

The fourth Phase 4 function, built from the second interactive mock the owner
approved, plus their addition: clicking Alternate again flips it.
- **The field's title is the "Taps" menu button** (a drawn chevron; the UI font
  may lack "▾"). The menu acts on the picked Y axis: Shape (Alternate, Ramp
  down, Ramp up, Random), Range (two pins), Reset, Undo. Alternate clicked
  again flips which taps take which end; Random re-rolls. The mock's
  Spread/Tilt became these four, which read the same on every axis.
- **Range replaces Amount** (the owner's idea): a shape spreads the taps
  between the pins, and moving a pin re-spreads it live. Each axis keeps its
  own range; defaults L 70 to R 70, −24 to 0 dB, 500 Hz to 20 kHz, Q 0.7 to 6,
  LP to HP. Positions are in the field's own scale, so what you see is evenly
  spaced.
- **Five Y axes: Pan, Gain, Cutoff, Reso (Q, log) and Type** (the filter's
  three types as three rows, HP on top). The Y picker moved from the toolbar
  to a vertical list down the field's left edge, like Spacing's list: five
  options don't fit Tall's toolbar, and the list sits beside the axis it
  picks. Rejected: a second toolbar row in Tall (field height lost) or a
  dropdown (hides the options, which the owner dislikes). The cost is about
  70 px of field width in both shapes. A field drag now accumulates its own
  value so a Type dot can step between rows.
- **Patterns write the live taps** (muted ones too), one gesture per tap.
  **Undo** keeps each axis's last 32 states in the page (not saved); Reset
  is undoable too. Patterns don't touch tap times.
- **Engine: per-tap glides,** because each tap's gain, pan, cutoff, Q and type
  used to change once per block, so a pattern moving every tap at once
  clicked. Now gain and pan glide through two 5 ms one-poles (no corner at
  either end); cutoff and Q in log steps (their whole range in 20 ms, the
  filters recomputed only while they move); a type change crossfades over
  10 ms from a copy of the filter still running the old type. Test: eight
  taps switching all five at once, four times over a held tone: 0.98× the
  held patterns' sharpest bend; with the glides off, 1,808×. A first version
  with straight-line gain and pan ramps reached 3.1×: their corners added up
  across taps.
- **Side effect:** a tap that's unmuted or added now comes in over the glide
  instead of at full level at once. Muting is unchanged (the tap stops).
- **Nothing moving, nothing changes:** the 21-case comparison with v2.36.0 is
  bit-identical.
- Not checked on screen yet (the Mac's screen was locked when the build was
  ready).

## 2026-10-29 · Ducking: the echoes dip while you play (v2.36.0)

The third Phase 4 function, built as proposed and approved by the owner
(Duck and Release knobs in Output, the dip follows playing level, the live
dip shown on the Duck knob).
- **Only the echoes dip:** after the taps and the output colour, before Mix.
  The dry signal is untouched, and the repeats keep going round at full
  level inside the loop, so the tail comes back as it would have been.
  Nothing in the loop changes, so self-oscillation is unaffected.
- **Duck** is the most the echoes can dip, Off to 24 dB. **Release** is how
  fast they come back, 50 ms to 2 s (default 300 ms, a one-pole time
  constant).
- **The dip follows playing level,** with Tide's envelope curve `1 − e^(−5·env)`:
  about 70 % of the depth at −12 dBFS, 27 % at −24 dBFS, next to nothing
  for quiet notes. No threshold knob. Rejected: a threshold (one more
  control for a small feature).
- **The key is the dry input as it arrives,** both sides together (the
  larger of the two), before In and the channel mode. The image never shifts,
  and In (how hard the delay is driven) doesn't change the ducking.
- **The follower:** 5 ms attack, a 20 ms hold so low notes don't make it
  flutter, then the Release. The dip is smoothed over 2 ms so it starts and
  ends without a corner. A test proves the smoother is needed: Freeze
  holds a clean sine and a note that starts with no ramp keys the ducking;
  without the smoother the dip's start is a corner 3× sharper than the
  sine's own bend, with it 1.0×.
- **The Duck knob shows the live dip** as a thin accent arc inside the dial,
  on the knob's own scale (the audio thread reports the most dip of each
  block through `PetalParams::duck_dip_db`, a plain atomic).
- **In the Output card:** In, Mix, Out, a rule, then Duck and Release, in the
  same columns as Colour's Route above. Out moved from the card's middle to
  column 2 so the two groups read apart.
- **Freeze plus Duck:** a held loop dips under your playing (tested).
- **Stored in presets** (part of the sound, unlike Freeze). **Off changes
  nothing:** the 21-case comparison with v2.35.0 is bit-identical.
- Not built: an external sidechain (no second input today, and AUv3 hosts
  differ on sidechains).

## 2026-10-29 · Freeze: hold the loop as it is (v2.35.0)

The second Phase 4 function, built as proposed and approved by the owner
(header button, the loop's colour bypassed while frozen, new input kept out,
tap to switch).
- **What it holds:** one cycle, the loop that's going round when you press
  it, at exactly 100 %. Feedback, Curve, Tone, the Diffuse mode and Diffuse,
  Tape or Saturate routed to FB are bypassed, so the loop doesn't darken,
  smear or fade. The length is latched in whole samples as the hold starts,
  so it repeats sample for sample (tested with every colour in the loop).
- **New input stays out of the delay;** the dry signal still passes, so you
  play over the held loop. The taps stay live (mute, pan, gain, filter), and
  so do Mix and the output colour.
- **Going in and out fades over 20 ms:** new input fades out, the hold fades
  in over the coloured loop, and the reverse on letting go. Both paths run
  during the fade, so neither switches (tested in Mix and Ping, plain and
  with Tone and Tape in the loop). The held loop has one soft 20 ms join where
  its newest and oldest moments meet.
- **Letting go:** the loop decays at the Feedback setting again, with the
  colour back. While fully held the feedback head waits at the latched
  length, so after letting go the cycle moves on as a time change would
  (Repitch glides it).
- **Ping:** the held bounce keeps alternating sides. The last cycle of dry
  input already in the main lines still enters the bounce once, as in Mix,
  where the held cycle includes the newest input.
- **Where it sits:** a button in the page header, between the preset box and
  A/B, ink when on. Rejected: the Echo lane's header (the plan's first
  place). It would be hidden behind the Motion and Tap tabs, and in Tall it
  only fits if the readout loses "Feedback".
- **The Echo lane shows the hold:** every repeat at full level, labelled
  "held", the readout "Frozen · holds a 53 ms loop", and the cycle edge
  reads "Held · 53 ms". The feedback and cycle drags and the Auto/Manual
  choice wait until it's let go. The audio thread reports the held cycle
  through `PetalParams::held_cycle_secs` (a plain atomic, not a param), as
  it was before the whole-sample rounding, so the readout doesn't change
  from 53 to 54 ms as the hold begins.
- **Not stored in presets or A/B** (`presets::NOT_STORED`): loading a preset
  doesn't let go of a held loop, and holding doesn't mark a preset edited. It
  is a normal automatable param, so a session saved while frozen reopens
  frozen, with an empty loop (the delay's contents aren't saved).
- **Off changes nothing:** the 21-case comparison with v2.34.0 is
  bit-identical.
- **Found while testing, not changed here:** `DelayLine::read` interpolates
  fractional delays backwards (`read(10.25)` returns 9.75 samples back and
  jumps 2 samples at each whole one), so gliding delays get small jumps.
  And the feedback head reads before the write, so each cycle comes round
  one sample later than its length. Reported to the owner as a separate fix.

## 2026-10-28 · Loop tone: one Tone knob on the feedback (v2.34.0)

The first Phase 4 function. Until now nothing filtered the loop unless Tape,
Diffuse or Saturate was routed to it, so repeats stayed as bright as the first
pass. The owner approved one knob in the lane's Loop row, beside Curve.
- **One bipolar knob, as Tide's FB tilt:** left of centre a low-pass darkens
  each repeat (20 kHz → 600 Hz), right of centre a high-pass thins it (20 Hz →
  1 kHz), both exponential; the centre is off. It reads "LP 3.5 kHz",
  "HP 140 Hz" or "Off". Rejected: separate low-cut and high-cut knobs. They
  would do both at once, but they don't fit the Loop row without squeezing
  Curve into a cycling pill, which the owner dislikes.
- **One pole (6 dB/oct) per pass, so it adds up:** the n-th repeat has been
  through n poles, as in a tape echo.
- **Ranges are wider than Tide's** (low-pass 10 → 1.77 kHz, high-pass 40 →
  320 Hz there): a flanger's loop goes round hundreds of times a second, a
  delay's a few times, so each pass needs to do more.
- **Off at the centre, bit for bit**, checked by rendering 21 cases (every
  mode, colour routing, channel mode and curve, self-oscillation, mode
  switching) on v2.33.2 and v2.34.0: identical. A Tone of −0.01 changes them,
  so the comparison can see the knob.
- **The centre never jumps.** Both filters always run, each parked at its end
  of the range while the other side is used, and within 0.1 of the centre the
  filtered signal is blended in. Tide switches straight from off to a
  10 kHz low-pass or a 40 Hz high-pass; a one-pole high-pass still shifts a
  bass note's phase by 10–20°, so that switch would click. The knob also
  glides (a unit per 20 ms) so stepped automation doesn't click.
- **Where it sits in the loop:** after Saturate and before the safety soft
  clip, like a tape echo's playback losing highs after the tape saturates.
  The soft clip stays the last stage and the only bound on the loop. No gain
  compensation: the passband stays at unity, so feedback above 100 % still
  self-oscillates (tested at both ends of the knob, in Mix and in Ping).
- **Ping's bounce loop has its own tone filters**, as it has its own Diffuse
  and Tape, so switching modes mid-tail stays click-free (tested dark and
  thin). A loop that stops getting input keeps running 0.1 s so the filters
  ring out.
- In the tall shape, the lane's single control row gains a sixth column for
  it.

## 2026-10-28 · Time & taps holds still when L/R splits (v2.33.2)

Splitting L/R used to re-space the Time group from two rows to three, and the
Pattern group (lined up with Time's first row) jumped up with it. The owner:
the Pattern side "should already be well placed". Time is now always laid out
in three rows; Time R and Sync R fill the third when L/R is split and leave it
empty when linked. Nothing on the card moves. Rejected: spreading two rows
while linked (it looked airier, but every split moved the whole card).

## 2026-10-28 · Room around dividers · Bars: tap selects, double-tap mutes · the preset browser in the new look (v2.33.1)

From the owner's first look at v2.33.0 ("some of the things need to breathe a
tiny bit more"):
- **Selectors sit 4 px inside their cells**, so neighbouring underlines and the
  dividers between groups (Colour's Type | Route, Time | Pattern) get air.
- **The lane's Cross and Loop controls start 14 px past their divider** (they
  sat 6 px from it). The field's toolbar has 16 px around each divider; its
  selectors are a little narrower so the tall page still fits.
- **Bars view: one tap on a tap's number selects it (and opens its Tap tab), a
  second tap within 0.4 s mutes or unmutes it.** A single tap used to mute, which
  made selecting from the numbers impossible. The double tap is timed by the page
  so it works on touch, where egui reports no double click.
- **The preset browser follows the page:** All / Favorites / Recent is the
  page's underlined selector, the selected row is an ink wash, icons and the
  favourite star are ink, and radii match the page (10 panel, 6 controls).

## 2026-10-28 · A new look: Mono and Paper, soft corners, underlined choices, ink controls, grouped cards · 36 px knobs · a spacing pass (v2.33.0)

**The owner picked the look in a lab** (an interactive mock of the page with
switchable palette, corners, choice style, highlight and grouping): Soft corners,
Mono for dark and Paper for light, Underline choices, Ink, Sub-headers.
- **Mono (dark) and Paper (light)** replace the cool charcoal and the Inertia
  light values in Afterecho's copy of the theme (`ui/theme.rs`). Their surfaces
  step clearly from page to card to well; the old three were nearly one value.
  The other dark variants stay in Settings → Theme.
- **The accent is the owner's to pick** (Settings → Accent). The default is a
  new signature pair: signal red-orange on Mono, cobalt on Paper.
- **Ink, not colour, for the controls.** Knob arcs, picks, toggles and the
  focus slider are ink. The accent is kept for the sound: the taps and their
  repeats, and the Shape pad's tap ticks. Before, green was on everything, so
  nothing stood out.
- **Underlined choices.** Every option is laid out as text on a hairline and
  the pick is ink, underlined. The old filled pick in a dark well read as one
  box that cycles; the owner asked whether Curve should be laid out, and it was.
  Buttons are outlined, and filled with ink when on.
- **Soft corners:** cards 16 → 10, controls 9 → 6.
- **Grouped cards.** Time & taps is now Time (Time, Sync, L/R, Mode, and Time R
  and Sync R when split) and Pattern (Taps, Spacing as a vertical list, the
  Shape pad), each under its own heading, with a hairline between. The lane's
  Cross and Loop get headings and a hairline; Colour, a hairline before Route;
  the field's toolbar, hairlines between its groups. L/R reads Linked / Split
  (it said Free, like Sync).
- **A saved look from before takes the new palette and accent once** (a look
  version in `appearance.json`); its font and light/dark choice stay.

**36 px knobs** (Tide's are 28): between Tide's size and the τόπος sheet's 48,
after a 44 px try felt heavy. Rows of cells grow from 58 to 72 px tall. On touch
a knob answers within 44 px of its centre whatever its drawn size. Tide's knobs
measure about 23 px on screen (its window opens at 1120 wide), but its cards are
dense; ours had 38–46 px of empty height around single rows.

**The spacing pass:**
- The wide page is 1366 × 660 (was 720) and its cards are sized to their rows.
  The tall page stays 1050 × 780: the owner's iPad is the 10.5" (4:3), where the
  width sets the zoom, so a shorter page would only add blank bands.
- The taps field fits the pattern: Free mode's time axis goes down to 20 ms
  (it never showed less than 250 ms, so the default 53 ms pattern sat in its
  first fifth), with finer grid steps. While a dot is dragged the view holds
  still.
- Output's knobs sit on columns 0, 2 and 4 of the right column's grid.
- The Echo lane's feedback readout moved to its header and the cycle's to the
  time axis, so neither covers the repeats.
- On the tall page the lane's controls share one row (the field grew from 336
  to 416 px) and the Tap tab takes the whole lane.

**Not settled:** spacing still isn't right everywhere (the owner's words). On
the tall page the Time group's rows sit far apart. Plates instead of headings
were on the table and are not built.

**Rejected:** a cycling Curve button (it hides two of three options and costs
taps); colour by area ("Two hues"), held back for now; accent-filled picks
(loud, one-colour page); 44 px knobs (heavy, and they pushed the tall Tap tab out
of its strip).

## 2026-10-28 · Ping bounces a mono source: its own loop, X-feed as width, each tap places the repeats (v2.32.0)

**Ping now bounces, like a classic ping-pong.** The owner asked for it and said
Afterecho may differ from Petal. Petal has no ping-pong at all: its two
feedback combs never cross channels.
- The loop runs in two mono lines beside the main ones (`bounce.rs`): odd
  repeats bound left, even repeats bound right. The main lines' mono sum (the
  mid in M/S) starts each bounce; each repeat comes back as the next one on the
  other side. In Ping the main lines hold the dry input only.
- **Each tap places the repeats itself,** at its own pan squeezed toward the
  repeat's side: position = ±width + (1 − width)·pan, constant power. A spread
  pattern swings as a whole; at 100 % every repeat is hard left, then hard
  right, at full level.
- **X-feed is the width in Ping.** The knob reads Width; the focus card reads
  Bounce width. At 0 % a mono source sounds exactly like plain feedback
  (tested). A stereo source's repeats are mono in Ping at any width; the taps'
  own pass keeps its stereo.
- The loop's colour (Diffuse, Tape and Saturate when routed to FB, then the soft
  clip) runs in the bounce loop too, with its own Diffuse and Tape. No gain
  compensation: above 100 % it self-oscillates, bounded by the soft clip
  (tested).
- **Mono greys Ping out** (and the Width knob): one channel can't bounce. In
  M/S the repeats still bounce left and right. They go in as mid and side, and
  each tap's mid/side balance applies on top.
- **The Echo lane draws Ping as two sides:** what a mono source puts on the left
  goes up from the axis, the right goes down.
- Cost: two more 32 s lines (+16 MB at 48 kHz, +32 MB at 96 kHz), two to four
  more reads per active tap, and a second Diffuse and Tape when routed to FB.

**Changing the mode doesn't click.** Tested: a held tone through spread taps,
switched Mix → Ping → Diffuse → Ping → Mix, is no sharper than the same tone
held in each mode. That holds plain, with Diffuse in the loop, and with Tape in
the loop. With the fades disabled the test fails at 277×.
- The two loops share what goes round by the bounce's share, which moves over
  20 ms (smoothstep). While it moves, both run and their inputs crossfade, so no
  loop's input, and no Diffuse or Tape inside it, ever jumps. A loop left
  without input runs on for 3 s while its Diffuse and Tape ring out, then
  stops; it starts again from a cleared state.
- Taking over: the main loop's next pass crosses to the odd line. The old tail
  then bounces as one block, its edges 20 ms pan sweeps.
- Handing back: the taps first glide their repeats to centre (30 ms, two
  one-poles so the glide has no corner). Then the main loop takes the bounce's
  last pass on and the repeats' stereo spread fades back in.
- Mix's output blend and Diffuse's loop allpass now fade with the mode as well
  (20 ms). Diffuse never runs while the loop is the bounce: Ping waits for it to
  fade out, and it waits for the hand-back. It starts from a cleared state, its
  input faded in, so no old tail leaks back.
- Taps now pan before their filter so the bounced repeats join each side ahead
  of it. **Mix, Diffuse, the loop colour and M/S render the same as v2.31**
  (A/B against the committed DSP: largest difference 7.5e-8, 130 dB below
  peak).

**What changes for old sessions: Ping only.** A stereo source's repeats used to
swap sides; now its mono sum bounces. A source panned to one side gives repeats
6 dB quieter (half of a one-sided signal). At X-feed 0 % a stereo source's
repeats are mono instead of stereo.

**Rejected:**
- Writing the bounce into the main lines (the first proposal). Taps pan by
  turning one line down, so a tap panned right went silent on every left repeat:
  an L, R, L, R pattern lost half its notes (simulated).
- A separate Bounce mode beside Ping: two near-identical modes.
- Keeping the stereo spread in the repeats and bouncing only the mid: a source
  panned to one side then puts anti-phase on the far side.
- Switching loops in one step with only output fades: the old tail flipped
  sides with a click every cycle, and Diffuse or Tape in the loop clicked at
  each handover (44× and 104× in the test).
- Petal's two loop lengths: they give width, not a bounce. Kept as a later idea
  (a swung bounce).

**Left as is:** the v2 editor's ping-pong drawing (`gui.rs`, `pp_own_d`) still
models the old blend. v2 isn't shown and retires in Phase 3.

## 2026-10-28 · The preset browser on the new page · a preset defines the whole sound (v2.31.0)

**The header's preset name opens Tide's preset browser.** Search,
All · Favorites · Recent, rows with favourite, tags, rename and delete, save-as,
init, and the iPad enlargement.
- Ported as-is from Tide (`y-flanger/src/editor.rs`, `draw_command_palette`) into
  `palette_scope.rs`. The only changes are the type names and `state.library`. It
  draws over the page the way Tide's does, after the page and the menu.
- The load / save / init glue stays in `palette_v3.rs`, shared with the v3 editor.
  palette_v3's own drawing goes when the v3 editor retires.
- Rejected: restyling palette_v3 in place. It's drawn on epoche-ui, which the new
  page left. Tide's version is the design target and already runs on the same UI
  layer as this page.

**A preset defines the whole sound.** This is the plan's Phase 1 item, using
Tide's rule.
- A param a preset doesn't mention loads at its default.
- The edited marker compares missing params with their default, so an older
  preset doesn't read as edited the moment it loads.
- The effect: presets saved before Tape, Saturate or the FB curve existed now
  load those at their defaults, instead of keeping whatever was set.
- Not changed: `tap_snap` is still saved in presets. It's an editing preference;
  leaving it out (like Tide's XY routing) is a separate call.

**Verified:** tests cover the matching. Desktop screenshots, taken with a preview
flag, show the browser on both shapes. A real click on the header name is
untested here; the code is Tide's, line for line.

**R taps are drawn when Time L/R is unlinked.**
- In the field they're diamonds, so they can't be confused with muted taps
  (hollow circles). A faint line joins each to its dot, and a key reads "R taps".
- In the Echo lane they're thin stems with diamond heads, at their own times.
- They don't drag: L and R share each tap's `time_offset`.

**Ping-pong is not drawn: the DSP gives it nothing to show for a mono source.**
- `CrossfeedMode::PingPong` blends the feedback, fb_l' = fb_l·(1−cf) + fb_r·cf.
  A mono or centred input fills both lines identically, so the blend changes
  nothing. Only a stereo input's sides trade places, repeat by repeat.
- Each channel's repeats always land at that channel's own tap times. The v3
  ECHO view drew the swapped share at the other channel's times, which doesn't
  match the DSP.
- **Open question for the owner:** should Ping alternate a mono source too, like
  a classic ping-pong? That's a DSP change (the loop has to start lopsided), not
  a drawing task.
  *Answered in v2.32.0: yes; see the entry above.*

## 2026-10-28 · Lock time, cycles to 8 bars / 20 s, the Tap tab (v2.30.0)

From the owner's first hand test of v2.29 (iPad and desktop).

**Lock time** (field header, next to Snap). While it's on, a field drag moves only
the Y value (pan, gain or cutoff) and double-click resets only that.
- Why: "if I found a right tap position and want to just edit the cutoff, it's
  pretty hard to drag." A diagonal finger always nudged the time too.
- It's editor state, not a param: it changes how the field drags, not the sound.
- Rejected: a direction lock that guesses from the first few pixels of the drag.
  It's invisible, and it gets in the way of deliberate diagonal moves.

**The cycle reaches 8 bars in Sync and 20 s in Free** (was 4/1 and 8 s; the owner
hit the 8.7 s cap at their tempo).
- `feedback_time`'s range grew from 0..1 to 0..`FEEDBACK_TIME_MAX` (1.137); the
  mapping below 1.0 is unchanged. Sync appends 4/1. and 8/1 (`FB_LONG_DIVS`) on
  SYNC_DIVS' index scale, so `fb_div` reads every saved cycle as before. Free
  keeps 10 ms × 800^n and simply runs on to 20 s.
- Sessions store plain values, so they're unaffected. Presets stored floats
  normalized, so `feedback_time` is now stored by plain value (`PLAIN_FLOATS`).
  An old preset's normalized value equals today's plain one, so old and new
  presets read back exactly. Tests cover both.
- The cost: host automation of Feedback Time is normalized, so an old lane
  now lands 1.137× as far up the range (longer cycles). Rare, and accepted.
- The buffer already held 30.4 s, so nothing changes in the DSP. 8/1 below
  ~63 BPM runs past that and is clamped, as before.
- SYNC_DIVS itself is untouched: base time uses it too.
- Rejected: remapping 0..1 (every saved cycle would move), or a ×2 range switch
  (one more control for the same thing).

**The Tap tab** (Echo · Motion · Tap N in the lane). Clicking a dot in the field
opens it with all of that tap's parameters: Time, Pan, Gain, on/muted, filter
type, Cutoff, Reso (Q) and Slope. ‹ › steps through the taps.
- Why: the owner liked v3's tap panel, and the new page had no way to reach the
  per-tap filter type, Q or slope at all.
- Deselecting leaves the tab. Picking the tab with nothing selected selects the
  tap it last showed.
- It replaces the lane's picture only. The loop's controls stay, behind a new
  divider, so no card moves.
- Rejected: a floating popup near the dot (v3 dropped that because it covered
  dots near the bottom edge), and a strip under the field (the plot would
  change height under the finger).

**Also:**
- **Touch selection fix:** on touch a drag starts at touch-down, which selected
  the dot, and the tap that followed then deselected it. A tap now reads the
  state from before the press.
- **The cycle readout** names the division in Sync ("↔ Cycle 8/1 · 16.00 s"), and
  both handle labels flip to the left of the handle near the lane's edge.
- **Focus card:** names the Tap tab's controls ("Tap 3 · Reso").

## 2026-10-27 · Tap field, Bars and the Echo lane on the new page (v2.29.0)

**Tap field.** X is time on v3's tight taps-only range; the Y picker chooses
Pan, Gain or Cutoff.
- v3's drag rules are kept:
  - `time_offset` is in cluster-span units, clamped to ±0.5;
  - a raw accumulator keeps Snap from being sticky (Snap is the `tap_snap` param);
  - pan has −1 at the top, and gain uses the DAW-fader skew;
  - double-click resets the tap.
- The Gain view draws stems; the Pan view sizes the dots by level.
- The selected tap's value sits above its dot (below it near the top edge), so a
  tight cluster stays visible. Its time sits on the axis.

**Bars.** The same Y axis with the taps in order.
- One surface covers all the bars: press or drag sets the bar under the pointer,
  so dragging across draws a shape. On touch, a finger can't miss a narrow column.
- The number under each bar mutes or unmutes it.
- Values show above every bar when they fit, otherwise above the selected one only.

**Echo lane.** Drawn from `compute_viz_data` (the DSP mirror):
- one band per cycle, labelled with its level;
- the taps and their repeats as stems (dB, −60 floor);
- the loop's decay as a dashed line, which follows the FB curve.

Drag anywhere ↕ for feedback. It's relative, so it can pass 100 %. Drag the band
edge ↔ for the cycle: that switches the cycle to Manual, and the time range holds
still during the drag so the edge stays under the pointer.

**The cycle is Auto · Manual, not the canvas's Auto · Free · Sync.**
- Since v2.19 the feedback time follows Time's Sync, so Free / Sync here would be
  a second copy of that switch. Manual's time is in Time's unit.
- New exact inverse `PetalParams::feedback_time_norm_for_secs`, with a
  round-trip test. `feedback_time_seconds` now calls a mode-explicit twin; the
  math is unchanged.
- The lane's Feedback knob is gone: the lane's handle and Last touched replace it.

**Touch.**
- Every drag goes through Tide's per-finger layer, so several taps can move at once.
- Host gestures begin on the first change and end on release.
- Not tried on a device yet.

**Desktop previews.** `AFTERECHO_PREVIEW=tall,bars,gain|cutoff,selN` replaces
`AFTERECHO_SHAPE`. Synthetic clicks don't reach the standalone window reliably,
so states are checked by opening them directly.

_Not yet:_
- R-channel times when L/R is unlinked: the field and the lane show L, like v3.
- Ping-pong isn't drawn.
- The Motion tab is a placeholder.
- The field's first-repeat ghost is dropped, because the lane shows the repeats.

## 2026-10-27 · Scope-first page, first pass, on Tide's own UI layer (v2.28.0)

**The redesign is built on a local copy of Tide's UI layer, not on epoche-ui.**
- Owner, 2026-10-27: we stopped using epoche-ui. Tide keeps its own copy of the
  Epoché widgets, theme and tokens, and that copy is the current one: it has the
  multitouch layer (`touch_interact`, `tap_or_click`, `knob_drive`), the cell
  grammar and the Settings-menu styling.
- Copied into `src/ui/{epo,theme,tokens}.rs`, with only the paths changed. Keep
  them close to Tide's so fixes can move between the two plug-ins.
- Fonts: Karla, Inconsolata, IBM Plex and GFS Didot come with their OFL licences.
- The v2/v3 editors stay on epoche-ui until the new page replaces them. Then they
  and the `epoche-ui` dependency go together, so nothing is ported twice.

**The new page, `src/gui_scope.rs`**, is modelled on Tide's `editor_scope.rs`
and switched by `gui::SCOPE_PAGE` (`false` brings the v3 editor back).
- Region tables follow the canvas:
  - Wide is 1366 × 720 (desktop, and iPad in short host panels).
  - Tall is 1050 × 780 (iPad full screen).
  - `Shape::pick` takes the larger zoom, with tests for the picking rule and for
    every region fitting without overlap.
  - iOS uses `with_host_resize`.
  - `AFTERECHO_SHAPE=tall` previews the tall page on desktop.
- Element sizes are Tide's: dial 28, control row 30, name and value rows 13.
- The look follows Tide's model: theme, accent, font and text size are saved once
  per device in `appearance.json` beside the presets folder, not with the
  project. Afterecho's defaults are the cool foundation and the violet accent.
- Built this pass:
  - the header: ☰, wordmark, channel modes, preset ‹ › stepping, A/B, light/dark;
  - Tide's Settings menu;
  - Last touched;
  - Time & taps, including the Shape pad with the real tap positions;
  - Colour (Diffuse, Tape and Saturate on tabs);
  - Output;
  - the Echo lane's Cross and Loop rows.
- Still placeholders: the tap field and the Echo lane drawing. Feedback is a knob
  until the lane's drag handles exist.

**Display text for the global params.** The page shows each parameter's own
text, and most 0–1 parameters had none, so they printed raw values like "0.5".
- Percent: Feedback, Crossfeed, Dry/Wet, the Diffuse, Tape and Saturate amounts,
  and Tape wear.
- `×`: Diffuse size.
- One decimal: the dB parameters.
- Display only; values and saved state are unchanged. Hosts' automation lanes
  now read the same text.

_Rejected:_
- Adding Tide's pieces to epoche-ui: the owner no longer uses that crate.
- Porting the v3 editor onto the new layer first: it would be thrown away.

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
