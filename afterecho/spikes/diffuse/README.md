# Diffuse: listening test

An offline spike for Afterecho's Colour → Diffuse (owner, 2026-11-01: "so-so"). Nothing here is in the plug-in. It renders reels for you to judge by ear, plus the measurements behind them. Linear SOF-199.

Made 2026-11-01 against v2.46.0 (`ui/scope-first`) on an M1 Pro. Run with `cargo run --release` (about 4 s).

## What to listen to

`renders/reels/<material>-<route>-amount<60|100>.wav`. Each reel plays the same phrase five times, with 0.8 s of silence between:

1. **The plain delay**, no Diffuse.
2. **A: today's Diffuse.**
3. **B: dense.**
4. **C: dense, slowly moving.**
5. **D: small FDN.**

- **Material:** drums, pluck (guitar-like), chord (a held pad), voice.
- **Route:**
  - **in:** before the delay.
  - **fb:** inside the feedback loop, 375 ms at 65 %.
  - **out:** on the wet. The taps are at 250, 375 and 500 ms.
- **Amount:** 60 % (what the factory presets use) and 100 %.

The delay around the candidates is a plain stand-in for Afterecho's. One gain is applied to each whole reel, so the candidates keep their relative levels.

I'd start with `drums-fb-amount60`, `pluck-out-amount60` and `voice-in-amount100`.

## The candidates

| | What it is | Amount does |
|---|---|---|
| **A** | Today's `diffusion.rs`: eight allpasses of 12.5–53 ms in series, g = 0.6, the same on both sides | blends dry with the chain |
| **B** | Six short allpasses (1.3–11 ms). The left and right delays differ, with the same total. | sets the allpass gain (0 → 0.75) |
| **C** | B, with each stage's delay drifting ±0.12 ms at 0.4–1.3 Hz | as B |
| **D** | Two of B's allpasses into an 8-line feedback delay network (3–20 ms, Hadamard, light damping) | sets the decay (60 → 260 ms) and blends it in |

## Measurements

From `renders/metrics.md`. Each candidate was measured alone on an impulse at Size 1.

| | Amount | Echo density (1 = like noise) | Energy centre (ms) | 90 % of energy by (ms) | L/R correlation | Mono-sum colour (dB) | Ringing in the loop vs plain (dB) |
|---|---|---|---|---|---|---|---|
| A | 30 / 60 / 100 % | 0.22 / 0.22 / 0.22 | 40 / 179 / 263 | 214 / 375 / 406 | 1.00 | 0.4 / 0.5 / 0.0 | −2 / −3 / −1 |
| B | 30 / 60 / 100 % | 0.35 / 0.60 / 0.81 | 31 / 31 / 31 | 42 / 48 / 62 | 0.03 / 0.01 / 0.01 | 5.9 / 4.6 / 8.0 | −1 / −1 / −1 |
| C | 30 / 60 / 100 % | 0.46 / 0.80 / 0.90 | 31 / 31 / 31 | 41 / 49 / 64 | 0.22 / 0.05 / 0.07 | 5.2 / 10.9 / 8.2 | −1 / −1 / −1 |
| D | 30 / 60 / 100 % | 0.75 / 0.83 / 0.88 | 6 / 15 / 22 | 21 / 33 / 41 | 0.68 / 0.25 / 0.04 | 5.9 / 12.7 / 16.8 | **+36 / +29 / +22** |

What the columns mean:
- **Echo density** (Abel & Huang): how much the first 150 ms sounds like noise rather than separate echoes. Low means a flutter.
- **Energy centre** and **90 % by**: how far the diffuser delays and smears the sound.
- **L/R correlation**: how alike the sides are for a mono input. 1 means it adds no width.
- **Mono-sum colour**: the 1/6-octave spread of L + R.
- **Ringing in the loop**: the 1/3-octave spread of the feedback tail after about five passes, compared with the plain loop. High means metallic.

_Updated the same day: C's moving delays now read through a first-order allpass (Thiran), as in the plug-in's port (v2.47). The first run read them with Lagrange interpolation, which took about half an impulse's energy (the top) on every pass. The C row and the reels are from the fixed version._

## What the numbers say about today's Diffuse (A)

These are reasoned from the measurements, not heard.

- **It flutters.** Its density stays at 0.22 at every Amount: eight long allpasses make a train of separate 12–53 ms echoes, not a wash.
- **It drags the sound late.** At 100 % the energy centre is 263 ms after the input. In the feedback loop, each pass then comes round about a quarter of a second late, so repeats stop landing on the set time. At 60 % it is still 179 ms.
- **It adds no width.** Both sides run the same chain.
- **Its colour is fine.** The dry blend I suspected barely colours it (under 0.5 dB at 1/6 octave). Its long delays put the comb teeth too close together to hear as tone. They are heard as the flutter above.

## The alternatives

- **B and C** turn into a wash fast (90 % of the energy by 42–62 ms), are dense (C up to 0.95), and are wide. They add no ringing in the loop.
  - **The cost of width:** summed to mono, they colour by 5–11 dB at 1/6 octave, as any stereo reverb does.
  - **A fixed delay:** they put about 31 ms × Size before the sound, at every Amount. In the plug-in, the loop's read can be moved earlier by that much so repeats stay on time. On Out, the wet comes 31 ms later when Diffuse turns on.
  - **C vs B:** C is denser than B at the same Amount and never sits still, which is what you'd want in a loop that runs for minutes. It costs nearly 2× B.
- **D** is dense and short, but rings metallic in the feedback loop (+22 to +36 dB). That's a small FDN's few modes building up pass after pass. I'd drop it unless the Out reels win you over.

**CPU:** every candidate costs under 0.1 µs per stereo sample in this spike (A 64 ns, B 45–54 ns, C 86 ns, D 25 ns), which is under 0.5 % of a core at 48 kHz. This spike's code isn't tuned (each read wraps with a modulo), so the plug-in's would cost less.

## My recommendation

**C**, with B as the cheaper fallback, if the reels agree with the numbers. A port would bring:

- **Amount as density** (the allpass gain), not a dry blend. Amount 0 is off, with a 20 ms crossfade when it switches.
- **Different left and right delays with equal totals**, so the mono sum keeps its timing.
- **Size** stays as today (0.25–2×).
- **In the feedback route, the loop reads earlier by the chain's delay**, so the cycle you set is the cycle you hear.

Two questions only your ears answer:
1. Does the width B and C add suit every route, or should In and FB stay narrower?
2. Is today's long flutter (A) a sound worth keeping as a second mode?

**Cross → Diffuse uses the same chain**, in the loop at 100 % (the `diff_l`/`diff_r` in `lib.rs`; its comment still says "ChowMatrix-style", which it hasn't been since v2.7). So it has A's flutter and its quarter-second drag per pass too, and a replacement changes both. The `fb` reels at 100 % are the closest to what that mode does.
