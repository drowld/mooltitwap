# Per-tap detune: listening test

An offline spike for a per-tap detune of up to ±1 semitone in Afterecho. Nothing here is in the plug-in: no change to `src/`, the version or the installed builds. It renders WAV files for you to judge by ear, plus the measurements and costs behind them.

Made 2026-10-31 against v2.41.0 (`ui/scope-first`) on an M1 Pro.

## The short answer

Per-tap detune at ±1 semitone looks doable. The old shifter was never close.

- **A, the old dual-head shifter** (removed in v2.4), is not a pitch shifter at this range.
  - At ±1 semitone with its 60 ms window, the shift is smaller than its grain rate, so a sustained tone comes out as a warble *around* the target pitch.
  - On a pure sine, the strongest line stays at the original pitch, with sidebands as loud as the tone.
  - On the pad chord, it puts as much energy between the partials as on them.
  - It also reads through today's `DelayLine::read`, which has the SOF-60 bug. That adds a buzz to any moving read head (see below).
- **B, WSOLA**, is a delay-line shifter whose splices are lined up by cross-correlation.
  - **Clean** on sines, plucks and speech.
  - **No pumping** on pads: frame-to-frame spectral movement equals the ideal's.
  - **Faint noise skirts on dense static pads**: around each partial, about −20 dB at ±100 cents and −26 dB at ±25 cents.
  - **Loose timing**: the tap time wanders up to ±10 ms, and an occasional drum hit is doubled or shifted.
  - **Cost**: about the old shifter's, with no added latency.
- **C, a phase vocoder with phase locking**, is the quality reference.
  - **Clean everywhere I could measure**: pad sidebands −39 to −51 dB, steady timing.
  - **Costs 3× B.**
  - **Bursty**: its FFT frames bunch up into single audio blocks.
  - **Needs a tap time of at least ~88 ms** to land on time.

**My recommendation is B**, if the pad reels pass your ear: the old shifter's cost, no latency, and it fits Afterecho's one-line-many-reads design. If the faint skirts on pads bother you, C is the way, at 3× the CPU, with a frame scheduler and a minimum detuned-tap time. Either way, **fix SOF-60 first**.

Your ears decide. The numbers below are proxies.

## Listen to these first

| # | File | What it settles |
|---|---|---|
| 1 | `renders/reels/chord_p100.wav` | B's worst case. Dry, A, B, C, then the ideal, 0.4 s apart. Listen for shimmer or roughness on B compared with C and the ideal. |
| 2 | `renders/reels/chord_p025.wav` | The same at +25 cents: a subtle detune, likely the most-used range. |
| 3 | `renders/reels/drums_p025.wav` and `renders/reels/drums_p100.wav` | B's weak spot: doubled or early hits (flams) and loose timing. Also listen for C's smearing: softer, "phasey" hits. |
| 4 | `renders/multitap/multitap_pluck_*.wav` | How it would really be used: 6 taps at −25, +25, −50, +50, −100 and +100 cents over the dry. Compare `0-nodetune`, A, B, C and D. |
| 5 | `renders/multitap/multitap_voice_*.wav` | The same on speech. |

## What is in the folder

`renders/` is not in git, because the WAVs take 132 MB. Rebuild it with `cargo run --release -- render` (about 10 s).

- **`renders/<material>/<material>_dry.wav`**: the source.
- **`renders/<material>/<material>_<cents>_<method>.wav`**: one tap at 120 ms, re-aligned with the dry so files line up for A/B.
  - `<cents>`: `m100`, `m050`, `m025`, `p025`, `p050` or `p100` (m is down, p is up).
  - `<method>`: `A-dualhann`, `B-wsola`, `C-pvoc` or `ideal`.
  - `ideal` is the same phrase synthesised at the target pitch: what a perfect shifter would give. It exists for sine, chord and pluck; drums and voice have no ideal.
- **`renders/reels/<material>_<cents>.wav`**: dry, A, B, C, then the ideal, 0.4 s silence between. The quickest way to compare.
- **`renders/multitap/multitap_<pluck|voice>_<version>.wav`**, stereo, loudness-matched to each other. The six taps are:

  | tap time | 110 ms | 220 ms | 330 ms | 440 ms | 550 ms | 660 ms |
  |---|---|---|---|---|---|---|
  | detune (cents) | −25 | +25 | −50 | +50 | −100 | +100 |
  | gain (dB) | −4 | −5 | −6 | −7 | −9 | −10 |

  - Taps are panned alternately left and right.
  - Feedback is 0.35 with a 770 ms cycle, re-read from the main line as the plug-in does, so the detune never stacks up on repeats.
  - `0-nodetune` uses the same taps with no detune.
  - `D-shiftloop-p4hz` is the optional comparison: no tap detune, a +4 Hz frequency shift on the feedback instead (Tide's Hilbert pair). Each repeat moves 4 Hz further. It is inharmonic and not per-tap. REDESIGN_PLAN has this as "Out", and this render is only there so you can confirm that.
- **`renders/metrics.md`** and **`renders/metrics.csv`**: every number for every file.
- **`chord_p100_spectra.svg`**: the picture of the flutter (the chord at +100 cents: ideal, A, B, C).
  - Each partial should be a single clean line.
  - A smears each one into a comb of lines 16.7 Hz apart.
  - B keeps the lines but grows skirts.
  - C stays clean.

### The material, and what each exposes

| Material | What it is | Listen for |
|---|---|---|
| `sine` | A3 = 220 Hz, 4 s | Warble or flutter (A); any roughness. |
| `chord` | Static pad: C3 G3 E4 B4 D5, band-limited saws, 5 s | Flutter and pumping (A); shimmer or roughness around notes (B). The hardest case for B: no one splice point suits five pitches. |
| `pluck` | 8-note guitar-like phrase (additive strings, pick click) | Attack clarity, ringing tails, doubled picks. |
| `drums` | 2 bars at 120 BPM: kick, snare, hats | Doubled or flammed hits and loose timing (B); softened, smeared or phasey hits (C). |
| `voice` | Speech from macOS `say` (Samantha and Daniel) | Graininess, roughness, "robot" (no voice sample is in the repo; internal listening only). |

## Methods

The three methods all read Afterecho's delay line the way a tap does: one write per sample, then each tap reads at its own delay. Any look-behind a shifter needs is hidden inside the tap time.

### A: old dual-head Hann (baseline)

Rebuilt from git: `10c4bf5`, `read_continuous`, kept as `read_continuous_hann` in `bf740cb`.

- Two heads 180° apart on one phasor, 60 ms Hann windows (the old default).
- It reads through the plug-in's `read` as it is today, SOF-60 included, as the old code did.
- The later v2.3.5 shifter ("dual-head + WSOLA/AMDF alignment", per the comment in `src/delay.rs`) was never committed and has no tag, so it can't be rebuilt.

### B: WSOLA

`src/dsp.rs` `Wsola`; settings: 80 ms Hann grains, overlap 2, search ±10 ms, compare 10 ms.

- Grains read the past at the detune ratio.
- At each splice the new grain is placed around where it keeps the tap on its time. It is then moved within ±10 ms to where the past best matches what the current grain is about to play (normalised cross-correlation, sub-sample peak), so the crossfade joins two waveforms in phase.
- The search is coarse-to-fine: every 4th sample on a low-passed copy of the line, then ±4 samples at full rate. That makes it 2.4× cheaper than brute force, with the same metrics (compared at 40 ms grains in the sweep).
- In practice the current grain's own continuation always matches perfectly. So B slides at the detune ratio and only makes a real (aligned) splice when the drift leaves the ±10 ms window. That gives few splices, which suits sustained tones, but the tap time wanders up to ±10 ms.
- I picked these settings from a sweep (`cargo run --release -- tune`; also `-- tune 2` and `-- wander`):
  - Overlap 4 is worse at small detunes on chords and doubles the CPU.
  - Grains from 60 to 100 ms barely differ.
  - A ±6 ms search halves the wander and is kinder to drums, but pad skirts rise to −14…−21 dB.
  - A "pull toward the set time" didn't stop the wander and roughened sines, so it's off.

### C: phase vocoder

`src/dsp.rs` `PhaseVocoder`.

- Time-stretch by the ratio with Laroche–Dolson identity phase locking, then resample.
- N = 4096, overlap 4, frames read straight from the delay line, early enough to land on the tap time.
- N = 2048 is measured too: half the latency, but pad sidebands of −23 to −28 dB, about B's.

## Measurements

These are proxies for what the ears hear.

- **"Off-partial energy"**: the energy a render puts more than 3.7 Hz away from the ideal's partials, relative to the energy on them, from a 2.7 s spectrum. It catches flutter, warble and skirts. Lower is better; the ideal is −∞.
- **"Pre-hit"**: the level change in the 25 ms before each drum hit. Positive means early, doubled or smeared hits.

Values are across −100, −50, −25, +25, +50 and +100 cents.

| | A old | B WSOLA | C PV 4096 | (C at N=2048) |
|---|---|---|---|---|
| Sine: off-pitch energy (dB; the ideal measures −67 to −87, the floor) | **−6 to −26** | −58 to −87 (= ideal except −58 at −25 c) | −67 to −87 (= ideal) | −67 to −83 |
| Sine: strongest line | at the *original* pitch (warble) | within 0.05 cent of target | within 0.05 cent | within 0.05 cent |
| Chord: off-partial energy (dB) | **+3 to −2** | −20 (±100 c), −22 (±50 c), −26 (±25 c) | −39 to −51 | −23 to −28 |
| Chord: spectral flux vs ideal | 3.0–3.3 vs 1.85–2.06 (pumping) | = ideal | = ideal | = ideal |
| Pluck: off-partial energy (dB) | −31 to −35 | −38 to −46 | −37 to −47 | — |
| Pluck: log-spectral distance to ideal (dB) | 7.4–8.6 | 1.3–2.1 | 1.1–1.9 | — |
| Drums: hit level (dB) | −0.5 to −1.7 | −0.6 to −2.3 | −0.5 to −0.8 | −0.4 to −0.7 |
| Drums: pre-hit (dB) | −0.8 to −1.0 | **+0.4 to +3.7** | −0.2 to −0.6 | −0.4 to −0.6 |
| Tap timing | constant 1.5–1.7 ms off (old code didn't centre the heads) | **wanders ±10 ms** (drums at +100 c: 7.8 ms early) | constant ≤1.6 ms off | — |
| Level vs dry | −0.2 to −2.6 dB (cancellation) | within 0.3 dB | within 0.3 dB | — |

A with the read fixed (measured, not rendered) is still a warble: chord off-partial energy +3 to −2 dB. So A's flutter is the algorithm; SOF-60 adds a buzz on top. Second-difference spikes on its sine renders sit 35 dB above the dry's.

No render clips. B and C show no clicks or dropouts. The one dropout flag is B's sine at −50 cents, which ends 10 ms early because of the wander.

## CPU and latency

Release build, 48 kHz, 128-sample blocks, best of 3, one M1 Pro core. The 16 × 2 figure runs on 22 s lines, with taps spread from 60 ms to 1.9 s and detunes spread over ±100 cents. **The iPad is not measured.**

| | 1 tap, 1 channel (ns/sample) | 16 taps × 2 ch (% of one core) | Worst 128-sample block (% of its budget) | Latency | Shortest tap that works |
|---|---|---|---|---|---|
| Plain read (today's tap) | 9.0 | 0.8 % | 2.8 % | none | — |
| A old dual-head | 15.3 | 2.4 % | 6.2 % | none | ~4 ms |
| **B WSOLA** | **14.9** | **2.2 %** | **5.5 %** (10.7 % if grain cycles aren't staggered across taps) | **none** (in the tap time) | **~23 ms**; shorter taps lose the search and sound like A |
| B, brute-force search | 35.9 | 5.7 % | 12.4 % (66.6 % unstaggered) | none | ~23 ms |
| **C PV, N=4096** | **44.5** | **7.0 %** | **43 %** (frames of many taps land in one block) | **83–88 ms** | shorter taps come late by the difference |
| C PV, N=2048 | 43.3 | 6.6 % | 26 % | 42–44 ms | same |

Memory:

- **B**: about 25 KB per tap-channel, plus a low-passed copy of each main line for the coarse search. That copy is the same size as the line (8 MB per channel at 48 kHz for the 32 s lines), or a quarter of that if stored decimated.
- **C**: about 200 KB per tap-channel at N=4096 (≈6.5 MB for 32).

The C worst-block figure matters on iPad with small host buffers. Every frame costs ~45 µs. If all 32 frames land in one 32-sample block (0.67 ms), that block overruns, so C would need a scheduler that computes frames ahead and caps them per block.

## Found on the way: SOF-60 makes any moving read head buzz

`DelayLine::read` interpolates toward the newer sample, so `read(10.25)` returns x[t − 9.75]. That's SOF-60 in TASKS.

- For a head moving at ratio r, this gives a 2-sample sawtooth error repeating at 48 kHz × |r − 1|: about 2.8 kHz at ±1 semitone.
- In this spike it produced spurs at 2.86 kHz ± the tone (−50 dB each, −37 dB in total) until I gave B a corrected read (`read` in `src/dsp.rs`). With the correction, B's sine became as clean as the ideal and its pluck error fell from 8.5 to 2 dB.
- The old shifter (`bf740cb`) has the same indexing. So what you heard in v2.3 carried this buzz on top of the flutter, and the earlier verdict on "alignment" (v2.3.5) may have mixed the two.
- **Any shifter needs SOF-60 fixed first.** Repitch glides have the same error today.

## What was tested, and what wasn't

**Tested** (all offline, in this spike):
- The renders.
- The proxy metrics above.
- Level, clicks and dropouts on every file.
- Sample-level timing on the drums.
- CPU and worst-block time on this Mac.
- The direction of D's shift: +4 Hz lands at 1004 Hz within 0.007 cent.

**Not tested:**
- **Listening.** I can't hear; the metrics only point.
- **The iPad's CPU.**
- **Other sample rates**: everything is at 48 kHz, and B's settings are in ms so should carry over.
- **Changing the detune while it plays** (only static detunes were rendered). A smooths the ratio over 5 ms. B and C as written assume a fixed ratio: B would have to integrate its read position per sample, and C would change the ratio per frame.
- **Very short taps** (< 23 ms for B, < 88 ms for C).
- **Stereo-linked splice points**: taps are mono here. In the plug-in, L and R would splice at different moments, which could smear the image; that needs a test.
- **Real recorded instruments**: everything except the voice is synthesised.
- **The plug-in's real wet chain** (filters, diffusion, Tape) around a detuned tap.

**Decisions this leaves for you:**
- B or C, or neither.
- Whether a ±10 ms timing wander is acceptable on synced taps (a ±6 ms search halves it, at some pad quality).
- Whether detuned taps get a minimum time.

If one goes in, it's a deviation from "no per-tap pitch" (HANDOFF §8, the params.rs note, REDESIGN_PLAN "Not recommended"), so it goes in DECISIONS.md with these numbers.

## Reproduce

```
cd mooltitwap/spikes/detune
cargo run --release -- render     # WAVs + renders/metrics.{md,csv}
cargo run --release -- bench      # the CPU table
cargo run --release -- tune       # the settings sweep (tune 2: second round; wander: tap-time wander vs pull)
python3 plot_spectra.py renders chord_p100_spectra.svg   # needs numpy
```

This is a standalone crate with its own `[workspace]`, so it never touches the plug-in's build or `Cargo.lock`. Voice needs macOS `say`; without it, voice is skipped.
