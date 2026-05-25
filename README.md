# Mooltitwap

A multitap delay garden in Rust, inspired by Rainbow Circuit's **Petal** (Max for Live).

Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) and [egui](https://github.com/emilk/egui). Bundles as VST3 + CLAP on macOS.

## Architecture

Two-layer design:

- **Layer 1 — Topology engine** (`topology.rs`): pure-math tap position calculator. Spacing modes (Linear, Exponential, Logarithmic, Euclidean), XY shape pad, independent L/R or linked.
- **Layer 2 — Tap processors** (`delay.rs`): per-tap delay reads with varispeed pitch (slewed delay times, crossfaded grain heads) and per-tap gain / pan.

Separate feedback delay loop, state-variable filter at the wet bus, soft-clip saturator at input, peak limiter on the output.

## Build

```sh
cargo xtask bundle mooltitwap --release
```

Bundles land in `target/bundled/`. Copy to:

- `~/Library/Audio/Plug-Ins/VST3/mooltitwap.vst3`
- `~/Library/Audio/Plug-Ins/CLAP/mooltitwap.clap`

## Status

v2.1.0 — DSP + egui GUI working. Diffusion / crossfeed / per-tap filter still stubs.

## Credits

Inspired by Rainbow Circuit's [Petal](https://maxforlive.com).
