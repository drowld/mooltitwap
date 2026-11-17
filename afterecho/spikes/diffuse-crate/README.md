# Diffuse against the crate's DIFFUSION and SPACES: listening test

Crate adoption pass 4 (owner, 2026-11-09): try the shared `softsystems-fx` DIFFUSION and SPACES
(ROOM, VAULT) as Afterecho's Diffuse, against today's (v2.47, candidate C of the first test).

Nothing here is in the plug-in. It's an offline spike: `cargo run --release` (about 10 s) writes
the reels and `renders/metrics.md`. Today's Diffuse is the plug-in's own `src/diffusion.rs`,
compiled here unchanged. The delay around the candidates is a plain stand-in for Afterecho's.

## What to listen to

`renders/reels/diff-<material>-<route>-amount<60|100>.wav`. Each reel plays the same phrase six
times, with 0.8 s of silence between:

1. the plain delay, no Diffuse;
2. today's Diffuse (six short drifting allpasses, ~31 ms);
3. DIFFUSION S: Cumulus's diffuser at its smallest size (four allpasses, 9–33 ms each);
4. DIFFUSION M: the same at size 0.5 (24–84 ms each);
5. SPACES ROOM, 6. SPACES VAULT: the reverb, short (decay 1 s, size 0.4, no predelay).

- **Material:** drums, pluck, chord, voice.
- **Route:** `in` (before the delay), `fb` (inside the loop, 375 ms at 65 %), `out` (on the wet).
- **Amount:** 60 % (the factory presets) and 100 %.
- **Each candidate is matched in loudness** to the plain delay.

How Amount drives each one (my choices, easy to change):
- today and DIFFUSION: the density (allpass gain), wet only. With no feedback, DIFFUSION's chain
  is an allpass too, so it doesn't colour.
- SPACES: the wet/dry mix (equal power).

In the loop, today's and DIFFUSION's delay is taken back (the loop reads that much earlier, as
the plug-in does now), so the cycle keeps its time. SPACES has no single delay to take back: its
energy lands 77–92 ms after the sound, so each pass comes round that much late.

Start with `diff-drums-fb-amount60`, `diff-pluck-out-amount60` and `diff-voice-in-amount100`.

## Measurements (`renders/metrics.md`)

Each candidate alone on an impulse (SPACES wet only):

| | Amount | Echo density (1 = noise) | Energy centre (ms) | 90 % by (ms) | L/R corr. | Loop late (ms) | Loop ringing vs plain (dB) | CPU (ns) |
|---|---|---|---|---|---|---|---|---|
| today | 30 / 60 / 100 % | 0.54 / 0.87 / 0.91 | 31 | 41 / 49 / 63 | 0.05 / 0.02 / 0.03 | 0 | −0.5 / −0.9 / −0.8 | 48–71 |
| DIFFUSION S | 30 / 60 / 100 % | **0.10 / 0.17 / 0.27** | 71–78 | 99 / 113 / 132 | 0.00 | −2 / −4 / −8 | −2.2 / −2.3 / −2.4 | 115–132 |
| DIFFUSION M | 30 / 60 / 100 % | **0.03 / 0.04 / 0.05** | 177–194 | 251 / 280 / 331 | 0.00–0.05 | −4 / −10 / −21 | −1.6 / −1.5 / −1.4 | 118–139 |
| SPACES ROOM | 30 / 60 / 100 % | 0.72 | 77 | 155 | −0.01 | **+77** | **+7.7 / +18 / +30** | 91–96 |
| SPACES VAULT | 30 / 60 / 100 % | 0.51 | 92 | 172 | −0.01 | **+92** | **+2.7 / +13 / +26** | 72–77 |

- **Echo density:** how much the first 150 ms sounds like noise rather than separate echoes. Low
  is a flutter. This is what the first test held against v2.46's long allpasses (0.22).
- **Loop late:** after the take-back, how far the loop's energy lands from the cycle you set.
- **Loop ringing:** the feedback tail's 1/3-octave spread after about five passes, against the
  plain loop. High means it rings or booms (a reverb inside a 65 % loop builds up its own modes).
- **CPU** (M1 Pro): all of them are cheap; DIFFUSION is about twice today's.
- **Memory:** DIFFUSION allocates about 0.5 MB and SPACES about 1.5 MB per instance, at 48 kHz.
  Afterecho runs up to six Diffuse chains (Cross, the wet pair, the bounce pair).

## What the numbers suggest (reasoned, not heard)

- **DIFFUSION** is built for long washes (Cumulus's cloud); as a short Diffuse its stages are
  far apart, so it flutters: echo density 0.10–0.27 at its smallest, lower than v2.46's chain
  that the first test rejected. It shines with its own feedback (a freeze), which Afterecho's loop
  already does.
- **SPACES** is dense and wide, but in the loop each pass lands 77–92 ms late and the tail rings.
  On In and Out, where there is no loop, it's a real candidate: a short room on the echoes.
- **So:** keep today's Diffuse for FB and Cross. If one of the crate's is worth having, it's
  SPACES ROOM or VAULT on Out (or In), maybe as a second Diffuse character. Your ears decide.
- **Crate gap, if DIFFUSION is kept:** its `set` always glides SIZE and MIX from where they were
  (no snap), so a preset load starts with a 200 ms sweep. The spike works around it with 0.5 s
  of silence. That would go to the crates hub.
