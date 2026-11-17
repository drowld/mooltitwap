# Continuous tap-filter slope: a measured spike

Owner, 2026-11-16: since the tap filters are digital, could Slope be continuous, say 6 to
48 dB/oct, so it isn't stepped in the Bars view? Nothing here is in the plug-in. Run
`python3 slope.py` (numpy only). It writes `metrics.md` and the `response-*.svg` curves. The SVF is
modelled exactly (the TPT SVF is the analog prototype through the prewarped bilinear transform),
at a 1 kHz cutoff, 48 kHz, Q 0.707 and Q 4. Measured, not heard.

## Today

Slope is 12, 24 or 48: 1, 2 or 4 of the crate's SVFs in a chain, **each with the tap's full Q**.

## Two ways to make it continuous

**A. Blend two orders** (run both chains and mix them, or fade the next stage in: the same maths).
It doesn't give in-between slopes. Past the cutoff the lower order wins, so "18 dB" still falls
at about 12 dB/oct, only lower. At Q 4 and 21 dB it makes a 4 dB notch with a shelf after it.
**Rejected.**

**C. Whole stages plus a fractional tilt.** 6 dB steps come from whole stages (a one-pole for an
odd step, SVFs for pairs). The fraction between comes from a tilt: five staggered one-pole
zero/pole pairs that slope −6·α dB/oct past the cutoff (Oustaloup's approximation). Its results:
- **Real in-between slopes:**
  - HP, Q 0.707: 9 → 8.0–8.7 dB/oct, 15 → 14.5–14.9, 21 → 19.8–20.7, 30 → 28.9–29.9, 42 → 40.7–41.9.
  - LP: the same shape but a little behind (9 → 4.6–7.1), because the tilt's top corners pass
    Nyquist. In the plug-in they'd be kept under it.
- **No notches, no pass-band bumps,** at either Q.
- **Cost:** five one-pole sections per tap and channel, about as much as one more SVF. The worst
  case (48) costs what it does today.

## Found on the way: Resonance stacks with Slope (today, in the plug-in)

Every stage gets the full Q, so the peaks add up:

| Q | 12 dB | 24 dB | 48 dB |
|---|---|---|---|
| 4 | +12 dB | +24 dB | **+48 dB** |
| 12 (max) | +21.6 | +43 | **about +86** |

A continuous Slope makes this worse: Resonance would jump up and down as Slope moves (at Q 4:
+12 at 12 dB, +9 at 18, +24 at 24), because the one-pole has no Q. The usual fix, and the
recommendation, is to put Q on one stage only and give the others a fixed Butterworth Q, so the
peak follows Resonance whatever the Slope. That changes how today's 24 and 48 dB settings with
high Resonance sound, so it's the owner's decision.

## If the owner goes ahead

- **The parameter:** Slope becomes a continuous parameter (6–48 dB/oct), so it can be drawn in
  Bars and moved by Motion.
  - It needs a new id; presets map 12 / 24 / 48 to their values.
  - Host automation recorded on the old Slope stops working. The beta is the cheapest time to
    make that change.
- **Band-pass:** it has no clean "slope". Each stage adds 6 dB/oct on both sides, so the tilt
  would go on both sides, or band-pass would keep whole steps.
- **Before building it:** listen to a sweep of 6 → 48 at Q 0.7 and Q 4 for zipper noise or level
  jumps as stages come in. A stage that switches in at a whole step needs a short crossfade, the
  way the filter type does today.
