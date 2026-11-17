# Saturate and the loop limit: listening test

Crate adoption pass 3 (owner, 2026-11-09): put the shared `softsystems-fx` SATURATE into
Afterecho's Saturate stage, keeping the two curves that sound best, and try PolSat531 as the
feedback loop's own limit. PolSat531 is never a Saturate choice (owner).

Nothing here is in the plug-in. It's an offline spike: `cargo run --release` (about 40 s)
writes the reels and `renders/metrics.md`. The delay around the candidates is a plain stand-in
for Afterecho's.

## What to listen to

### Saturate: `renders/reels/sat-<material>-<route>-drive<6|18>.wav`

Each reel plays the same phrase seven times, with 0.8 s of silence between:

1. the plain delay, no Saturate;
2. today's Saturate (a blend of tanh(drive·x));
3. TUBE, 4. TAPE, 5. GRIT, 6. AMP, 7. TRANSFO (the crate's chain, 8× oversampled).

- **Material:** drums, pluck, chord, voice.
- **Route:** `in` (before the delay), `fb` (inside the loop, 375 ms at 65 %), `out` (on the wet).
- **Drive:** 6 dB (Afterecho's default) and 18 dB. Amount is 70 %, as in the factory presets.
- **Each candidate is matched in loudness** to the plain delay, so you hear the character, not
  the level. The raw gains are in the table below.

Start with `sat-drums-fb-drive18`, `sat-voice-out-drive6` and `sat-chord-in-drive18`.

### The loop limit: `renders/reels/loop-<material>-fb<95|110>.wav`

One 375 ms loop with no Saturate: first today's limit, 2·tanh(x/2), then PolSat531. At 95 %
the loop is hot; at 110 % it self-oscillates and the limit alone holds it. These are not
loudness-matched: the level a loop settles at is part of what you're judging.

## Measurements (`renders/metrics.md`)

Each curve alone at Amount 100 %, a 1 kHz sine at −6 dBFS:

| Curve | Drive | Gain (dB) | 2nd harm. (dB) | 3rd harm. (dB) | THD | Aliasing (dB) | CPU (ns / stereo sample) |
|---|---|---|---|---|---|---|---|
| today (tanh) | 6 / 18 | +4.2 / +8.2 | none | −24 / −11 | 7 / 30 % | **−43 / −18** | 9 / 11 |
| TUBE | 6 / 18 | −2.2 / −8.3 | **−27 / −24** | −19 / −11 | 12 / 33 % | −47 / −47 | 480 / 489 |
| TAPE | 6 / 18 | −1.0 / −8.1 | none | −27 / −11 | 4 / 33 % | −47 / −47 | 492 / 505 |
| GRIT | 6 / 18 | −1.5 / −7.2 | none | −25 / −11 | 5 / 31 % | −47 / −47 | 464 / 482 |
| AMP | 6 / 18 | −2.6 / −7.1 | none | −20 / −12 | 10 / 27 % | −47 / −47 | 534 / 589 |
| TRANSFO | 6 / 18 | −0.5 / −7.2 | **−33 / −39** | −28 / −11 | 5 / 33 % | −47 / −47 | 532 / 588 |

- **2nd harmonic:** only TUBE and TRANSFO add even harmonics (warmth); the rest are odd, like
  today's tanh.
- **Aliasing:** a 7 kHz sine in, what lands below 20 kHz that isn't a harmonic. Today's tanh
  folds a lot back at 18 dB (−18 dB); the crate's 8× chain stays at −47 dB (likely this
  measurement's floor).
- **Gain:** today's block gets louder with Drive (+8 dB at 18 dB). The crate's make-up rule
  holds the level; at high drive it comes out a little quieter.
- **CPU:** the crate's chain costs about 50× today's tanh. About 500 ns per stereo sample is
  about 2.4 % of one core at 48 kHz for each Saturate. On FB with Ping there are two loops, so
  two instances, about 5 %. Today's block is close to free.
- **Latency:** 20 samples (0.4 ms) for the crate's chain. On In and Out it's reported to the
  host. In the loop the cycle would read 20 samples early to keep its time, as Diffuse does.

The loop limit, on a noise burst, after 4 s:

| Limit | Feedback | Settled peak | Settled RMS |
|---|---|---|---|
| today | 95 % / 110 % | 0.16 / 0.67 | 0.03 / 0.15 |
| PolSat531 | 95 % / 110 % | 0.18 / 1.61 | 0.04 / 0.25 |

At 110 %, PolSat531 lets the loop settle about 7.6 dB higher at its peak (4 dB in RMS): its
curve stays near unity until close to its ceiling, then flattens. Today's tanh starts bending
early, so a self-oscillating loop stays quieter and rounder. Below 100 % they are nearly the same.

## What the numbers suggest (reasoned, not heard)

- **Saturate: TUBE and TAPE.** TUBE is the most different from today's sound: even harmonics,
  warm, and asymmetric. TAPE is the closest to today's odd-only tanh, but oversampled, so it
  doesn't fold back. TRANSFO is the other even-harmonic choice if TUBE is too much.
- **A naming clash:** a Saturate curve called "Tape" would sit next to Colour's Tape tab
  (RE-DEEMER). If TAPE is kept, it needs a different word in Afterecho.
- **The loop limit:** keep today's for now. PolSat531 makes a self-oscillating loop louder
  and harder, which is the opposite of what a safety limit is for. Your ears decide.
- **Cost:** two of the crate's chains in the loop is the price of real oversampling. If that's
  too much, the crate's lighter core layer (`CoreParams` + `CoreChannel`: no dynamics, tone or
  mix) still oversamples. It would need measuring.
