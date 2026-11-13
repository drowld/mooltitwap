# Afterecho

A Soft Systems delay of up to 16 taps, in Rust (`afterecho/`), first inspired by Rainbow Circuit's **Petal** (Max for Live). Ids: `com.softsystems.afterecho`.

Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) and [egui](https://github.com/emilk/egui). Bundles as VST3 + CLAP on macOS.

## Architecture

Two-layer design:

- **Layer 1 — Topology engine** (`topology.rs`): pure-math tap position calculator. Spacing modes (Linear, Exponential, Logarithmic, Euclidean), XY shape pad, independent L/R or linked.
- **Layer 2 — Tap processors** (`delay.rs`): per-tap delay reads with varispeed pitch (slewed delay times, crossfaded grain heads) and per-tap gain / pan.

Separate feedback delay loop, state-variable filter at the wet bus, soft-clip saturator at input, peak limiter on the output.

## Build

```sh
cargo nice-plug bundle afterecho --release
```

Bundles land in `target/bundled/`. Copy to:

- `~/Library/Audio/Plug-Ins/VST3/Afterecho.vst3`
- `~/Library/Audio/Plug-Ins/CLAP/Afterecho.clap`

## Status

See `afterecho/TASKS.md` and `afterecho/docs/HANDOFF.md`.

## Credits

Inspired by Rainbow Circuit's [Petal](https://maxforlive.com).
