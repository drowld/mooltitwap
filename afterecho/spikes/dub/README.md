# Dub: listening test

The first listen to softsystems-fx's `dub_loop` (Cumulus's Dub Delay feedback path, from
Molekular core_084), as Afterecho's coming Colour → Dub tab (owner, 2026-11-14; DECISIONS).
Built against nice-plug branch `claude/fx-grain-delay` (0236993), not on main yet.

Nothing here is in the plug-in. `cargo run --release` (about 15 s) writes the reels and
`renders/metrics.md`. The loop is Afterecho's: the line is read, the pass goes through the
candidate, × Feedback, then Afterecho's loop limit `2·tanh(x/2)`, and back in. **No gain
compensation** (the owner's rule): the Dub loop's ÷Q from Cumulus is gone.

## What to listen to

`renders/reels/dub-<material>-fb<65|90>.wav`, eight files. Each plays the same phrase five
times, with 0.8 s of silence between:

1. the plain loop (no colour);
2. **today:** Loop Tone at −0.5, the nearest Afterecho has (a gentle low-pass on each pass);
3. **Dub light:** Intensity 0.3, the band 100 Hz–4 kHz, 1 swirl at 700 Hz;
4. **Dub medium:** Intensity 0.6, 150 Hz–2.5 kHz, 2 swirls at 600 Hz;
5. **Dub heavy:** Intensity 1.0, 200 Hz–1.5 kHz, 3 swirls at 500 Hz.

- **Material:** drums, pluck, chord, voice.
- **Feedback:** 65 % (a usual setting) and 90 % (hot: Dub heavy self-oscillates here).
- **Each candidate is matched in loudness** to the plain loop over the whole render, so you hear
  the character. The first echo is the same in all five: as in Cumulus, the processing shapes
  each *return*, so the dub starts at the second echo.

Start with `dub-drums-fb65`, then `dub-pluck-fb90` and `dub-voice-fb65`.

## Measurements (`renders/metrics.md`)

A 30 ms noise burst into the loop at 65 %: each echo's brightness (spectral centroid) and level.

| | Echo 1 | 2 | 3 | 4 | 6 | Level at echo 6 | Settled peak, FB 90 % / 110 % | CPU (ns / stereo sample) |
|---|---|---|---|---|---|---|---|---|
| plain loop | 12.1 kHz | 12.1 | 12.1 | 12.1 | 12.1 | −18.8 dB | 0.10 / 0.88 | 2 |
| today (Loop Tone −0.5) | 12.1 | 3.6 | 1.9 | 1.4 | 0.9 | −30.6 dB | 0.03 / 0.44 | 4 |
| Dub light | 12.1 | 2.5 | 1.7 | 1.4 | 1.2 | −31.7 dB | 0.02 / 0.36 | 56 |
| Dub medium | 12.1 | 1.9 | 1.5 | 1.4 | 1.3 | −29.0 dB | 0.07 / 0.57 | 63 |
| Dub heavy | 12.1 | 1.3 | 1.2 | 1.2 | 1.3 | **−13.9 dB** | 0.21 / 0.26 | 64 |

What the numbers say (reasoned, not heard):
- **Loop Tone keeps darkening** pass after pass (one pole per pass, like tape). **Dub drops to its
  band at once** and stays there: every return sits at about 1.2–1.5 kHz, with the lows cut too.
  That's the dub sound: thin, mid-forward, smeared echoes rather than ones that just get duller.
- **Dub heavy holds on longer than the plain loop** (−13.9 dB at echo 6 against −18.8). With the
  ÷Q gone, Intensity's low-pass resonance (Q up to 1.5, about +3.5 dB at its corner) adds loop gain
  there, so the loop rings at the corner and sustains: at 90 % it self-oscillates, held by the
  PolSat at a peak of about 0.2. This is the house rule working as meant (the loop may sing, the
  saturation holds it). Cumulus divided the feedback by Q to stop exactly this. Your ears decide
  whether heavy settings ring too much; if so, the fix is the Q range, not the feedback.
- **Light and medium decay faster than plain:** the band takes energy every pass.
- **Cost:** about 64 ns a stereo sample, ~0.3 % of a core at 48 kHz. Afterecho would run two
  (the loop's L/R), or four with Ping.

## Found on the way: Saturate on FB runs away at 65 %

Afterecho's Saturate adds gain by design (`x·(1−a) + tanh(x·g)·a`, no compensation, the house
rule). On FB at its default Drive (6 dB) and Amount 70 %, a small signal comes back 1.7× louder,
so a 65 % loop runs away, held only by the loop limit. At 12 dB, 3.1×. That may be intended (it's
how "Runaway" works), but it means Saturate on FB turns any feedback above about 60 % into
self-oscillation. Worth a look in the Saturate pass (pass 6).

## The knobs Afterecho would show

Intensity, Low cut (20–2000 Hz), High cut (200–16000 Hz), Swirl (the allpass frequency, with 0–3
stages; Cumulus's Freq and Stages). The crate also has `color` (the allpass's sharpness),
Cumulus's fixed 0.4 here; it could stay fixed.
