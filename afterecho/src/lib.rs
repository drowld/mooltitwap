mod bounce;
mod delay;
mod detune;
mod diffusion;
mod factory;
mod filter;
mod gui;
mod gui_scope;
mod modulation;
mod params;
mod presets;
mod tape;
mod topology;
mod ui;

use nice_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::delay::{ChannelTaps, DelayLine, ReadLine, SlewedTime};
use crate::detune::{Pv, Wsola};
use crate::diffusion::Diffusion;
use crate::filter::{soft_clip, DcBlock, LoopTone, PeakLimiter, Svf, TapChain, TapLayout, ToneCoefs};
use crate::modulation::{ModRt, N_TGT, T_COLOUR, T_CROSS, T_CUTOFF, T_DUCK, T_FDBK, T_GAIN, T_MIX, T_PAN, T_RESO, T_TONE};
use crate::params::{
    ChannelMode, CrossfeedMode, DelayMode, DiffuseRouting, FbCurve, AfterechoParams, SatCurve, TapeType, NUM_TAPS, REPEATS_LOOP,
};
use crate::tape::{TapeDegrade, TapeDegradeParams, TapeKind};
use softsystems_fx::dub_loop::{DubLoop, Params as DubParams};
use softsystems_fx::saturate::{self as crate_sat, curve::Engine as SatEngine, Saturate};
use crate::topology::TopologyEngine;

/// Smoothstep: the shape of every mode fade (no corner where it starts or
/// ends, so a fade can't tick).
#[inline]
fn s_curve(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// A cutoff or Q gliding in the log domain through two one-poles (v2.43,
/// as gain and pan glide), so a jump — a Step's, a knob's — starts and lands
/// without a corner. `now` is the value the filter runs at.
#[derive(Clone, Copy)]
struct LogGlide {
    to: f32,
    to_ln: f32,
    mid_ln: f32,
    now_ln: f32,
    now: f32,
}

impl LogGlide {
    fn at(v: f32) -> Self {
        Self { to: v, to_ln: v.ln(), mid_ln: v.ln(), now_ln: v.ln(), now: v }
    }

    /// One sample toward `target` (`k`: each one-pole's share a sample),
    /// landing on it exactly. True if it moved.
    #[inline]
    fn step(&mut self, target: f32, k: f32) -> bool {
        if target != self.to {
            (self.to, self.to_ln) = (target, target.ln());
        }
        if self.now == self.to && self.mid_ln == self.to_ln {
            return false;
        }
        self.mid_ln += (self.to_ln - self.mid_ln) * k;
        self.now_ln += (self.mid_ln - self.now_ln) * k;
        if (self.to_ln - self.now_ln).abs() < 1e-5 && (self.to_ln - self.mid_ln).abs() < 1e-5 {
            *self = Self::at(self.to);
        } else {
            self.now = self.now_ln.exp();
        }
        true
    }
}

/// Dub's allpass sharpness: Cumulus's fixed 0.4 (Q 1.7), not a knob (DECISIONS 2026-11-16).
const DUB_COLOR: f32 = 0.4;

/// Dub's share of each pass at Intensity `i`: it fades in over the first tenth.
fn dub_wet(i: f32) -> f32 {
    (i / 0.1).clamp(0.0, 1.0)
}

/// What the feedback path does to each pass: Diffuse, Tape and Saturate
/// when they're routed to it (set per block), and the loop tone (which
/// glides per sample).
#[derive(Clone, Copy)]
struct LoopColour {
    /// Diffuse's Amount while it's routed to the feedback path (the chain
    /// stays in the loop even at 0, where it's just a delay the loop's read
    /// takes back).
    diffuse: Option<f32>,
    tape: bool,
    sat: bool,
    /// Saturate runs one of the crate's curves (beta pass 6), not today's tanh.
    sat_crate: bool,
    /// Dub's share of each pass (beta pass 12): 0 off; it fades in over the first tenth of
    /// Intensity, so turning it on doesn't jump to its band.
    dub: f32,
    tone: ToneCoefs,
}

impl LoopColour {
    /// One pass through the loop's colour, then the soft clip that bounds it.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn apply(
        self, mut l: f32, mut r: f32, diff_l: &mut Diffusion, diff_r: &mut Diffusion, tape: &mut TapeDegrade,
        tone: &mut LoopTone, sat: &impl Fn(f32) -> f32, dub: &mut [DubLoop; 2], crate_sat: &mut Saturate<2>,
    ) -> (f32, f32) {
        // DIFFUSE routing = Feedback: smear the recirculating signal so
        // each cluster repeat picks up more allpass density.
        if let Some(a) = self.diffuse {
            l = diff_l.process(l, a);
            r = diff_r.process(r, a);
        }
        // DEGRADE routing = Feedback: generation loss — every repeat gets
        // re-crushed, like tape/BBD copies of copies. Before the loop
        // saturator so the quantization grit stays bounded.
        if self.tape {
            tape.process(&mut l, &mut r);
        }
        // SATURATE routing = Feedback: drive the recirculating signal —
        // each repeat picks up another pass of harmonics (tape-loop
        // buildup). Sits before the fixed safety tanh.
        if self.sat && self.sat_crate {
            crate_sat.process_stereo(&mut l, &mut r);
        } else if self.sat {
            l = sat(l);
            r = sat(r);
        }
        // DUB (beta pass 12): Cumulus's Dub Delay feedback path — the band, the swirl and its
        // PolSat — on every pass. No gain compensation (the owner's rule): with Intensity up
        // its low-pass peak adds loop gain, and the soft clip below holds the loop.
        if self.dub > 0.0 {
            l += (dub[0].process(l) - l) * self.dub;
            r += (dub[1].process(r) - r) * self.dub;
        }
        // Loop tone: after the drive, as a tape echo's playback loses highs
        // after the tape saturates. Off (bit-exact) at the centre.
        l = tone.process(&self.tone, 0, l);
        r = tone.process(&self.tone, 1, r);
        // In-loop soft saturation (v2.18.2) — the ONLY thing bounding the
        // loop; there is deliberately NO gain compensation (project rule:
        // feedback paths must be allowed to self-oscillate). Headroomed
        // tanh: ~transparent at normal levels (−0.2 dB at 0.5 amplitude),
        // ceiling ≈ 2.0. With feedback ≥ 1.0 the loop settles into
        // bounded, tape-style self-oscillation instead of exponential
        // runaway — which is what used to detonate at short FB times.
        (2.0 * soft_clip(l * 0.5), 2.0 * soft_clip(r * 0.5))
    }
}

/// Per-channel delay buffer capacity in seconds. Tap nominal delays exceeding
/// this are silenced (not clamped — clamping makes all over-budget taps read
/// the same sample, summing into a loud saturated cluster).
const MAX_DELAY_SECONDS: f32 = 32.0;

/// Simple autocorrelation pitch tracker. Maintains a ring buffer of recent
pub struct Afterecho {
    params: Arc<AfterechoParams>,
    /// The licence (one per process, shared by every Afterecho) and this
    /// instance's demo gate: no silences for 3 minutes, then ~2 s every 45 s
    /// while unlicensed. The gate is the audio thread's only view of it. iOS:
    /// not required, the gate does nothing.
    licence: softsystems_licence::Licence,
    gate: softsystems_licence::DemoGate,
    /// Tempo shared with the GUI thread (f32 bitcast into u32).
    /// Updated from `ctx.transport().tempo` every process block.
    tempo_bpm_shared: Arc<AtomicU32>,
    sample_rate: f32,

    // Layer 1
    topology_l: TopologyEngine,
    topology_r: TopologyEngine,

    // Layer 2 — main tap delay (per channel)
    main_l: ChannelTaps,
    main_r: ChannelTaps,

    // NOTE: v2.11.0 dropped the separate fb_l/fb_r delay lines and fb_state
    // accumulators. The new architecture (see process()) reads feedback
    // directly from main_l / main_r at cluster_span back — a single buffer
    // hosts both taps and recirculation, which is what produces clean,
    // unsmeared cluster repeats (matching Cluster Delay).

    // Ping (v2.32, see bounce.rs): the loop's odd repeats (bound left) and
    // even repeats (bound right), kept out of the main lines so each tap can
    // place them. Written every sample, with silence outside Ping.
    ping_odd: DelayLine,
    ping_even: DelayLine,
    /// The loop runs as the bounce. Follows the mode, except that handing the
    /// loop back waits `ping_leave` samples while the taps glide to centre.
    ping_on: bool,
    ping_leave: Option<usize>,
    /// The bounce's share of the loop, 0..1, moving over 20 ms toward
    /// `ping_on`. While it moves both loops run, their inputs crossfading,
    /// so neither loop's input (or its Diffuse and Tape) ever jumps.
    ping_share: f32,
    /// How deep the loop may read the bounce lines: a hand-back takes their
    /// last pass to the main lines, so anything older reads as silence.
    ping_valid: usize,
    /// Samples each loop has had no input (saturating). It keeps running that
    /// long so its Diffuse and Tape ring out, then stops, and starts again
    /// from a clean state.
    main_quiet: usize,
    bounce_quiet: usize,
    /// The bounce loop's own Diffuse (odd, even) and Tape, when they're routed
    /// to the feedback path.
    diff_odd: Diffusion,
    diff_even: Diffusion,
    tape_bounce: TapeDegrade,
    /// Saturate on the crate's curves (beta pass 6): on In, on Out, and in each loop. Boxed
    /// (a few KB each), made off the audio thread.
    sat_in: Box<Saturate<2>>,
    sat_out: Box<Saturate<2>>,
    sat_main: Box<Saturate<2>>,
    sat_bounce: Box<Saturate<2>>,
    /// Dub's loop processors (beta pass 12): the main loop's L/R, the bounce loop's odd/even.
    dub_main: [DubLoop; 2],
    dub_bounce: [DubLoop; 2],
    /// The loop tone's filters, one set per loop (main L/R, bounce odd/even),
    /// and the knob as it glides to the param (a unit per 20 ms).
    tone_main: LoopTone,
    tone_bounce: LoopTone,
    tone_t: f32,
    /// Freeze (v2.35): the hold's share of what's written back, 0..1 over
    /// 20 ms, and the loop length it holds, in whole samples (latched as it
    /// starts from nothing) so the held loop repeats bit for bit.
    freeze_w: f32,
    freeze_len: f32,
    /// Ducking (v2.36): the dry input's level (a peak follower: 5 ms attack,
    /// a 20 ms hold so low notes don't flutter, then the Release), the hold's
    /// samples left, the depth gliding to the Duck knob (dB, 24 per 20 ms),
    /// and the dip itself (dB), smoothed over 2 ms so it starts and ends
    /// without a corner.
    duck_env: f32,
    duck_hold: usize,
    duck_depth: f32,
    duck_dip: f32,
    /// Motion (beta pass 8): the shared crate's six slots and its matrix,
    /// each tap a read-head (`modulation.rs`).
    modu: ModRt,
    /// The whole-echo targets as Motion has them, gliding (5 ms) to where
    /// each 32-sample tick aims them: Feedback, Crossfeed, Mix.
    fb_now: f32,
    cross_now: f32,
    mix_now: f32,
    /// Repeats (v2.42): the dry input as written (In and its colour), for the
    /// counted copies; each copy's level, gliding (5 ms) so a count or level
    /// change doesn't click; and the count's share against the loop, 0..1
    /// over 20 ms, so switching between a count and ∞ crossfades.
    dry_l: DelayLine,
    dry_r: DelayLine,
    rep_gain: [f32; 16],
    rep_w: f32,
    /// The count last set, so a handover to ∞ keeps playing it until the
    /// loop has taken over (rather than the copies fading first).
    rep_n: usize,
    /// Safety (v2.43): the loops' DC blockers (main L/R, bounce odd/even);
    /// Ear guard's power follower and gain; Clear's fade (samples left).
    dc: [DcBlock; 4],
    guard_pow: f32,
    guard_g: f32,
    clear_left: Option<usize>,
    /// Samples since the loop last became the bounce (saturating). A tap
    /// starts swinging once the first bounced repeat has reached it.
    ping_age: usize,
    /// Samples the bounce lines may still hold repeats after the loop was
    /// handed back: the taps play them out and the main loop takes the last
    /// pass on.
    ping_live: usize,
    /// Samples since the loop was handed back, and how deep the taps may
    /// still read the bounce lines (once handed back and played out, older
    /// repeats read as silence). Both saturate.
    ping_since_off: usize,
    ping_fresh: usize,
    /// Detune (v2.45): each tap's shifters (left, right; a linked tap uses
    /// the left one for both, so its sides splice together), its ratio
    /// gliding (5 ms) to the knob, and its share against the plain read,
    /// 0..1 over 20 ms (a tap at 0 cents reads plainly and costs nothing).
    det: Vec<[Wsola; 2]>,
    det_ratio: [f32; NUM_TAPS],
    det_w: [f32; NUM_TAPS],
    /// Clean (phase vocoders, L and R) and its share against Light, 0..1
    /// over 20 ms: a tap moves to Light while its time is too short for
    /// Clean. `*_live`: that shifter is running (it restarts when it's
    /// needed again).
    pv: Vec<[Pv; 2]>,
    /// Samples a detuned tap has sat at 0 cents: it goes back to the plain
    /// read only after 300 ms, so a knob passing through 0 doesn't fade
    /// between the shifted read and the plain one (their timing differs).
    det_zero: [usize; NUM_TAPS],
    clean_w: [f32; NUM_TAPS],
    light_live: [bool; NUM_TAPS],
    clean_live: [bool; NUM_TAPS],
    /// Per tap, the gains placing the repeats ([odd→L, odd→R, even→L,
    /// even→R]), gliding toward the targets set each block through two
    /// one-poles, so a glide starts and ends without a corner.
    ping_gain: [[f32; 4]; NUM_TAPS],
    ping_gain_mid: [[f32; 4]; NUM_TAPS],
    /// Mix's output blend and Diffuse's loop allpass, faded in and out (0..1)
    /// over 20 ms so changing the mode doesn't click.
    xf_mix: f32,
    xf_diffuse: f32,
    /// Diffuse on In or Out is running (Amount above 0).
    wet_diff_on: bool,
    /// Cross → Diffuse's chain: its input's fade and how long it has run.
    x_feed: f32,
    x_warm: usize,

    // Per-tap filters (`filter::TapChain`): up to four SVFs and a one-pole, Slope's
    // `TapLayout` picking which run (6 to 48 dB/oct), Resonance on the first SVF only.
    tap_chain_l: [TapChain; NUM_TAPS],
    tap_chain_r: [TapChain; NUM_TAPS],

    /// Per-tap glides (v2.37), so a pattern that moves every tap at once
    /// (or stepped automation) doesn't click: each tap's gain and pan gains
    /// as heard, through two 5 ms one-poles (`_mid`, then `_now`) so a glide
    /// starts and ends without a corner; its cutoff and Q the same way in the
    /// log domain (`LogGlide`, v2.43; before, a fixed rate with corners that
    /// clicked under Step), the filters recomputed only while they move.
    /// Nothing changes while nothing moves.
    tap_gain_mid: [f32; NUM_TAPS],
    tap_gain_now: [f32; NUM_TAPS],
    tap_pan_mid: [(f32, f32); NUM_TAPS],
    tap_pan_now: [(f32, f32); NUM_TAPS],
    tap_cut: [LogGlide; NUM_TAPS],
    tap_q: [LogGlide; NUM_TAPS],
    /// A filter-type or Slope change crossfades over 10 ms (`tap_type_xf` 0..1)
    /// from a copy of the filter still running the old type and stages to the new.
    tap_type_now: [crate::params::FilterType; NUM_TAPS],
    tap_type_old: [crate::params::FilterType; NUM_TAPS],
    tap_layout_now: [TapLayout; NUM_TAPS],
    tap_layout_old: [TapLayout; NUM_TAPS],
    tap_type_xf: [f32; NUM_TAPS],
    tap_chain_old_l: [TapChain; NUM_TAPS],
    tap_chain_old_r: [TapChain; NUM_TAPS],

    // Cross → Diffuse's chain (`diffusion.rs`), one per side.
    // Used exclusively by CrossfeedMode::Diffuse (feedback path).
    diff_l: Diffusion,
    diff_r: Diffusion,

    // Wet-output diffusion — smears the tap mix into a bloom before dry/wet.
    // Controlled by the DIFFUSE tab's `diffuse_amount` param.
    diff_wet_l: Diffusion,
    diff_wet_r: Diffusion,

    // DEGRADE block — physically-modeled tape (re-deemer port: J-A
    // hysteresis at 2× + heads EQ + self-erasure). Stereo; inserted at the
    // routing point the `degrade_routing` param selects — Input / Feedback /
    // Output, same grammar as DIFFUSE. Feedback routing = generation loss.
    tape: TapeDegrade,

    /// Envelope follower on the raw feedback signal (linked max of L/R so
    /// the stereo image isn't skewed) — drives the FB CURVE gain bending
    /// (Bloom = compressor-in-the-loop, Gate = expander-in-the-loop).
    fb_env: f32,

    /// Smoothly tracks the target feedback delay time (samples) to avoid
    /// clicks when the user changes `feedback_time` or the tempo jumps.
    fb_time_slew: SlewedTime,

    /// True right after `reset()` until the first `process()` call snaps
    /// the tap delays to their actual nominal targets. Without this, every
    /// tap reader starts at delay = 1 (the `snap_to(1.0)` in `ChannelTaps::reset`)
    /// and slews toward its real target at the very slow tape-bend rate
    /// (~0.02 samples per audio sample), producing minutes of audible
    /// pitch drift and a loud stack of taps reading near-zero delay.
    needs_initial_snap: bool,

    /// Active-tap count from the previous block. When this changes the
    /// topology anchoring re-distributes ALL intermediate taps, so the
    /// fb_time_slew (and per-tap readers) must SNAP not glide — gliding
    /// over hundreds of samples of delay difference is what produced the
    /// "weird repitched and mangled" pitch sweep when changing tap count.
    prev_n_active: usize,
    /// Whether each tap was being actively read in the previous block.
    /// When a tap transitions inactive→active we snap it to avoid the same
    /// slow-slew Doppler artifact on first use.
    prev_tap_active: [bool; NUM_TAPS],

    // Limiter
    lim_l: PeakLimiter,
    lim_r: PeakLimiter,
}

impl Default for Afterecho {
    fn default() -> Self {
        let dummy_max = 192_000;
        // reads and checks the licence file; never waits on the network
        // The beta's last day (UTC), the day the website stops activations. PLACEHOLDER
        // until the owner sets Afterecho's date: set it before any build reaches a tester.
        #[cfg(feature = "beta")]
        let licence = softsystems_licence::licence!("afterecho", beta_ends = "2026-12-31");
        #[cfg(not(feature = "beta"))]
        let licence = softsystems_licence::licence!("afterecho");
        let gate = licence.demo_gate();
        let params = Arc::new(AfterechoParams::default());
        Self {
            params: params.clone(),
            licence,
            gate,
            tempo_bpm_shared: Arc::new(AtomicU32::new(120.0_f32.to_bits())),
            sample_rate: 48_000.0,
            topology_l: TopologyEngine::new(),
            topology_r: TopologyEngine::new(),
            main_l: ChannelTaps::new(dummy_max),
            main_r: ChannelTaps::new(dummy_max),
            ping_odd: DelayLine::new(dummy_max),
            ping_even: DelayLine::new(dummy_max),
            ping_on: false,
            ping_leave: None,
            ping_share: 0.0,
            ping_valid: usize::MAX,
            main_quiet: 0,
            bounce_quiet: usize::MAX,
            diff_odd: Diffusion::new(0),
            diff_even: Diffusion::new(1),
            tape_bounce: TapeDegrade::new(48_000.0),
            sat_in: Box::new(Saturate::new(48_000.0)),
            sat_out: Box::new(Saturate::new(48_000.0)),
            sat_main: Box::new(Saturate::new(48_000.0)),
            sat_bounce: Box::new(Saturate::new(48_000.0)),
            dub_main: [DubLoop::new(48_000.0), DubLoop::new(48_000.0)],
            dub_bounce: [DubLoop::new(48_000.0), DubLoop::new(48_000.0)],
            tone_main: LoopTone::default(),
            tone_bounce: LoopTone::default(),
            tone_t: 0.0,
            freeze_w: 0.0,
            freeze_len: 0.0,
            duck_env: 0.0,
            duck_hold: 0,
            duck_depth: 0.0,
            duck_dip: 0.0,
            modu: ModRt::new(48_000.0, &params.motion),
            fb_now: 0.0,
            cross_now: 0.0,
            mix_now: 0.0,
            dry_l: DelayLine::new(dummy_max),
            dry_r: DelayLine::new(dummy_max),
            rep_gain: [0.0; 16],
            rep_w: 0.0,
            rep_n: 0,
            dc: [DcBlock::default(); 4],
            guard_pow: 0.0,
            guard_g: 1.0,
            clear_left: None,
            ping_age: usize::MAX,
            ping_live: 0,
            ping_since_off: usize::MAX,
            ping_fresh: usize::MAX,
            det: (0..NUM_TAPS).map(|_| [Wsola::new(48_000.0), Wsola::new(48_000.0)]).collect(),
            det_ratio: [1.0; NUM_TAPS],
            det_w: [0.0; NUM_TAPS],
            pv: Vec::new(),
            det_zero: [0; NUM_TAPS],
            clean_w: [0.0; NUM_TAPS],
            light_live: [false; NUM_TAPS],
            clean_live: [false; NUM_TAPS],
            ping_gain: [[0.0; 4]; NUM_TAPS],
            ping_gain_mid: [[0.0; 4]; NUM_TAPS],
            xf_mix: 0.0,
            xf_diffuse: 0.0,
            wet_diff_on: false,
            x_feed: 0.0,
            x_warm: 0,
            tap_chain_l: [TapChain::default(); NUM_TAPS],
            tap_chain_r: [TapChain::default(); NUM_TAPS],
            tap_gain_mid: [0.0; NUM_TAPS],
            tap_gain_now: [0.0; NUM_TAPS],
            tap_pan_mid: [(1.0, 1.0); NUM_TAPS],
            tap_pan_now: [(1.0, 1.0); NUM_TAPS],
            tap_cut: [LogGlide::at(20_000.0); NUM_TAPS],
            tap_q: [LogGlide::at(0.707); NUM_TAPS],
            tap_type_now: [crate::params::FilterType::LowPass; NUM_TAPS],
            tap_type_old: [crate::params::FilterType::LowPass; NUM_TAPS],
            tap_layout_now: [TapLayout::of(crate::params::FilterSlope::Db12); NUM_TAPS],
            tap_layout_old: [TapLayout::of(crate::params::FilterSlope::Db12); NUM_TAPS],
            tap_type_xf: [1.0; NUM_TAPS],
            tap_chain_old_l: [TapChain::default(); NUM_TAPS],
            tap_chain_old_r: [TapChain::default(); NUM_TAPS],
            diff_l: Diffusion::new(0),
            diff_r: Diffusion::new(1),
            diff_wet_l: Diffusion::new(0),
            diff_wet_r: Diffusion::new(1),
            tape: TapeDegrade::new(48_000.0),
            fb_env: 0.0,
            fb_time_slew: SlewedTime::new(),
            needs_initial_snap: true,
            prev_n_active: 0,
            prev_tap_active: [false; NUM_TAPS],
            lim_l: PeakLimiter::new(48_000.0),
            lim_r: PeakLimiter::new(48_000.0),
        }
    }
}

impl Afterecho {
    /// Tap `i`'s filter coefficients at its gliding cutoff and Resonance, on both channels and
    /// the old copies (a crossfade's old filter follows the knobs too).
    fn set_tap_coefs(&mut self, i: usize, sr: f32) {
        let mut c = Svf::default();
        c.set(self.tap_cut[i].now, self.tap_q[i].now, sr);
        let (g, k) = (c.g(), c.k());
        for ch in [&mut self.tap_chain_l[i], &mut self.tap_chain_r[i], &mut self.tap_chain_old_l[i], &mut self.tap_chain_old_r[i]] {
            ch.set_gk(g, k);
        }
    }

    /// Allocate everything for `sample_rate` (initialize's work; tests call it
    /// directly).
    fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.modu = ModRt::new(sample_rate, &self.params.motion);
        let max_samples = (MAX_DELAY_SECONDS * self.sample_rate) as usize;
        self.main_l = ChannelTaps::new(max_samples);
        self.main_r = ChannelTaps::new(max_samples);
        self.ping_odd = DelayLine::new(max_samples);
        self.ping_even = DelayLine::new(max_samples);
        self.dry_l = DelayLine::new(max_samples);
        self.dry_r = DelayLine::new(max_samples);
        self.det = (0..NUM_TAPS).map(|_| [Wsola::new(sample_rate), Wsola::new(sample_rate)]).collect();
        self.pv = (0..NUM_TAPS).map(|_| [Pv::new(sample_rate), Pv::new(sample_rate)]).collect();
        self.lim_l = PeakLimiter::new(self.sample_rate);
        self.lim_r = PeakLimiter::new(self.sample_rate);
        self.diff_l.prepare(self.sample_rate);
        self.diff_r.prepare(self.sample_rate);
        self.diff_wet_l.prepare(self.sample_rate);
        self.diff_wet_r.prepare(self.sample_rate);
        self.diff_odd.prepare(self.sample_rate);
        self.diff_even.prepare(self.sample_rate);
        self.diff_l.reset();
        self.diff_r.reset();
        self.diff_wet_l.reset();
        self.diff_wet_r.reset();
        // Rebuild at the real sample rate — runs the ~10k-iteration MagParams
        // calibration, which is why it happens here and not per block.
        self.tape = TapeDegrade::new(self.sample_rate);
        self.tape_bounce = TapeDegrade::new(self.sample_rate);
        let sr = self.sample_rate;
        self.dub_main = [DubLoop::new(sr), DubLoop::new(sr)];
        for s in [&mut self.sat_in, &mut self.sat_out, &mut self.sat_main, &mut self.sat_bounce] {
            **s = Saturate::new(sr);
        }
        self.dub_bounce = [DubLoop::new(sr), DubLoop::new(sr)];
    }

    /// What goes round, read `at` samples back: [main L, main R, bounce odd,
    /// bounce even]. Two loops share it by the bounce's share `b`. The main
    /// loop takes the main lines, plus the bounce's last pass (centred) once
    /// Ping hands it back. The bounce takes the main lines' mono sum (the dry
    /// input) as a new odd repeat, an even repeat back as the next odd one, an
    /// odd one as the next even.
    #[inline]
    fn loop_inputs(&self, at: f32, b: f32, ch_mode: ChannelMode, ping_read: bool) -> [f32; 4] {
        let raw_l = self.main_l.delay.read_before_write(at);
        let raw_r = self.main_r.delay.read_before_write(at);
        // Ping's lines coming back (see bounce.rs): the odd repeats (bound
        // left) and the even ones (bound right).
        let (odd, even) = if ping_read && at <= self.ping_valid as f32 {
            (self.ping_odd.read_before_write(at), self.ping_even.read_before_write(at))
        } else {
            (0.0, 0.0)
        };
        let (tl, tr) = bounce::mono_to(ch_mode, odd + even);
        [
            (1.0 - b) * (raw_l + tl),
            (1.0 - b) * (raw_r + tr),
            b * (bounce::mono_of(ch_mode, raw_l, raw_r) + even),
            b * odd,
        ]
    }
}

/// `x_warm` once warm (anything past the 80 ms it waits).
const X_WARM_DONE: usize = usize::MAX / 2;

/// A tap's lines, as it pans and (in Ping) places them: read through its
/// heads or a detuned tap's grains, L and R (`same`: one reader for both).
struct TapLines<'a> {
    main: [&'a DelayLine; 2],
    ping: [&'a DelayLine; 2],
    pan: (f32, f32),
    ping_g: Option<[f32; 4]>,
}

impl TapLines<'_> {
    /// Side `ch`'s input as the tap hears it, `out[k]` being `back − k`
    /// samples back (for Clean's frames; `aux` is scratch).
    fn fill(&self, ch: usize, back: usize, out: &mut [f32], aux: &mut [f32]) {
        let pan = if ch == 0 { self.pan.0 } else { self.pan.1 };
        self.main[ch].copy_back(back, out);
        for x in out.iter_mut() {
            *x *= pan;
        }
        if let Some(g) = self.ping_g {
            for (line, gain) in [(self.ping[0], g[ch]), (self.ping[1], g[2 + ch])] {
                line.copy_back(back, aux);
                for (x, a) in out.iter_mut().zip(aux.iter()) {
                    *x += a * gain;
                }
            }
        }
    }

    #[inline]
    fn read(&self, rl: &impl ReadLine, rr: &impl ReadLine, same: bool) -> (f32, f32) {
        let mut l = rl.read_line(self.main[0]) * self.pan.0;
        let mut r = rr.read_line(self.main[1]) * self.pan.1;
        if let Some(g) = self.ping_g {
            let (odd_l, even_l) = (rl.read_line(self.ping[0]), rl.read_line(self.ping[1]));
            let (odd_r, even_r) = if same { (odd_l, even_l) } else { (rr.read_line(self.ping[0]), rr.read_line(self.ping[1])) };
            l += odd_l * g[0] + even_l * g[2];
            r += odd_r * g[1] + even_r * g[3];
        }
        (l, r)
    }
}

impl Plugin for Afterecho {
    // The ids are Soft Systems' (v2.55, SOF-92): sessions saved with the old
    // `com.bussien.mooltitwap` ids no longer recall it (no one uses it yet).
    const NAME: &'static str = "Afterecho";
    const VENDOR: &'static str = "Soft Systems";
    const URL: &'static str = "https://softsystems.app";
    const EMAIL: &'static str = "hello@softsystems.app";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create_editor(
            self.params.clone(),
            self.params.editor_state.clone(),
            self.tempo_bpm_shared.clone(),
            self.licence.clone(),
        )
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        self.prepare(buffer_config.sample_rate);
        self.gate.set_sample_rate(buffer_config.sample_rate);
        // the host restores the state before this
        self.licence.check_state_load();
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        self.ping_odd.reset();
        self.ping_even.reset();
        self.dry_l.reset();
        self.dry_r.reset();
        self.rep_gain = [0.0; 16];
        self.det_w = [0.0; NUM_TAPS];
        self.dc = [DcBlock::default(); 4];
        (self.guard_pow, self.guard_g) = (0.0, 1.0);
        self.lim_l = PeakLimiter::new(self.sample_rate);
        self.lim_r = PeakLimiter::new(self.sample_rate);
        self.ping_on = false;
        self.ping_leave = None;
        self.ping_share = 0.0;
        self.ping_valid = usize::MAX;
        self.main_quiet = 0;
        self.bounce_quiet = usize::MAX;
        self.diff_odd.reset();
        self.diff_even.reset();
        self.tape_bounce.reset();
        self.dub_main.iter_mut().chain(self.dub_bounce.iter_mut()).for_each(DubLoop::reset);
        for s in [&mut self.sat_in, &mut self.sat_out, &mut self.sat_main, &mut self.sat_bounce] {
            s.reset();
        }
        self.tone_main.reset();
        self.tone_bounce.reset();
        self.freeze_w = 0.0;
        self.duck_env = 0.0;
        self.duck_hold = 0;
        self.duck_dip = 0.0;
        self.modu.reset();
        self.ping_live = 0;
        self.ping_since_off = usize::MAX;
        self.ping_age = usize::MAX;
        self.ping_fresh = usize::MAX;
        self.xf_mix = 0.0;
        self.xf_diffuse = 0.0;
        (self.x_feed, self.x_warm) = (0.0, 0);
        self.tap_chain_l.iter_mut().chain(self.tap_chain_r.iter_mut()).for_each(TapChain::reset);
        self.tap_type_xf = [1.0; NUM_TAPS];
        self.diff_l.reset();
        self.diff_r.reset();
        self.diff_wet_l.reset();
        self.diff_wet_r.reset();
        self.diff_odd.reset();
        self.diff_even.reset();
        self.wet_diff_on = false;
        self.tape.reset();
        self.fb_env = 0.0;
        self.fb_time_slew.snap_to(0.0);
        // Tell process() to snap (not slew) the tap delays on the next frame.
        // Otherwise the slow tape-bend slew would take minutes to reach long
        // nominal delays (e.g. 2/1 sync at slow tempo), producing audible
        // pitch drift and a near-zero-delay tap stack in the meantime.
        self.needs_initial_snap = true;
        self.prev_n_active = 0;
        self.prev_tap_active = [false; NUM_TAPS];
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sr = self.sample_rate;
        let p = self.params.clone();

        let tempo_bpm = ctx.transport().tempo.unwrap_or(120.0) as f32;
        // Publish tempo to GUI thread for accurate grid/sync display
        self.tempo_bpm_shared.store(tempo_bpm.to_bits(), Ordering::Relaxed);
        let time_l_norm = p.base_time_l.value();
        let time_r_norm = if p.time_linked.value() { time_l_norm } else { p.base_time_r.value() };
        let base_sec_l = p.base_time_seconds(time_l_norm, tempo_bpm);
        let base_sec_r = AfterechoParams::base_time_seconds_mode(p.sync_mode_r_eff(), time_r_norm, tempo_bpm);

        // the taps heard (Euclid places one per step at most)
        let n_active = p.taps_heard();

        // Topology — spacing per channel (each side's own Shape pad when unlinked)
        self.topology_l.compute(p.spacing(false), n_active);
        self.topology_r.compute(p.spacing(true), n_active);

        let pos_l = *self.topology_l.positions();
        let pos_r = *self.topology_r.positions();

        // Tape-style delay-time slew rate: 1 sample of delay-time change per ~50
        // samples of audio. Fast enough to track wobble/offset moves, slow enough
        // to hear the Doppler bend during transitions.
        let bend_slew_rate: f32 = 1.0 / 50.0;
        // Repitch glide bound: big delay jumps (sync-division change, tempo
        // jump, fast knob throws) must finish in ~this long, not minutes at
        // the fixed tape rate. Per block the rate is raised to
        // max(bend_slew_rate, remaining_distance / glide_samples) — small
        // moves keep the classic constant-rate tape bend, large ones become
        // a fast bounded Doppler sweep (Ableton-Repitch feel).
        let glide_samples = (0.10 * sr).max(1.0);
        // Out-of-range threshold: anything beyond this gets silenced, not
        // clamped, so over-budget taps don't stack at the buffer edge.
        // We use 95% of the configured max so we never even approach the
        // wrap boundary of the ring buffer.
        let max_delay_samples = MAX_DELAY_SECONDS * sr * 0.95;
        let delay_mode = p.delay_mode.value();
        let span_secs_l = NUM_TAPS as f32 * base_sec_l;
        let span_secs_r = NUM_TAPS as f32 * base_sec_r;
        let mut nom_l = [0.0f32; NUM_TAPS];
        let mut nom_r = [0.0f32; NUM_TAPS];
        let mut t_secs_l = [0.0f32; NUM_TAPS];
        let mut t_secs_r = [0.0f32; NUM_TAPS];
        let mut tap_in_range = [false; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;

            // Seconds-domain tap time from the shared topology helper — the
            // GUI visualizer calls the exact same function, so the display
            // and the audio cannot drift apart.
            let off_norm = p.taps[i].time_offset.value();
            t_secs_l[i] = topology::tap_delay_secs(pos_l[i], off_norm, span_secs_l);
            t_secs_r[i] = topology::tap_delay_secs(pos_r[i], off_norm, span_secs_r);

            nom_l[i] = (t_secs_l[i] * sr).max(2.0);
            nom_r[i] = (t_secs_r[i] * sr).max(2.0);
            // Update the per-tap transition mode (Repitch / Fade / Jump)
            // BEFORE the rate calc — set_mode may re-seed the reader state.
            // set_mode re-aligns internal state to the current target so the
            // switch is silent.
            self.main_l.readers[i].set_mode(delay_mode);
            self.main_r.readers[i].set_mode(delay_mode);
            // Bounded Repitch glide (see glide_samples above).
            let rate_l = bend_slew_rate
                .max((nom_l[i] - self.main_l.readers[i].current_pos()).abs() / glide_samples);
            let rate_r = bend_slew_rate
                .max((nom_r[i] - self.main_r.readers[i].current_pos()).abs() / glide_samples);
            self.main_l.readers[i].set_nominal_slew_rate(rate_l);
            self.main_r.readers[i].set_nominal_slew_rate(rate_r);
            // Nominal delay is constant across the block (wobble, the only
            // per-sample modulator, was removed in v2.17.5) — set it here.
            self.main_l.readers[i].set_nominal(nom_l[i].max(2.0));
            self.main_r.readers[i].set_nominal(nom_r[i].max(2.0));

            // Mark in-range: only taps whose delay fits in the buffer get
            // to play. Out-of-range taps would otherwise be clamped by
            // DelayLine::read to the buffer boundary, causing them all to
            // read the same sample → loud saturated stack of overlapping
            // taps at the boundary.
            tap_in_range[i] = active
                && topology::tap_fits(t_secs_l[i], t_secs_r[i], MAX_DELAY_SECONDS);
        }

        // ── Snap guards ──────────────────────────────────────────────────
        // Delay-TIME changes (base-time knob, sync division, DAW tempo) are
        // deliberately NOT snapped here: they go through TapReader's mode
        // machinery via set_nominal above — Repitch glides (bounded by
        // glide_samples), Fade crossfades, Jump snaps. Force-snapping them
        // (pre-v2.18.1 "Guard 1") made every knob drag hard-snap all readers
        // every block, collapsing all three modes into the same "digital
        // mash" artifact.
        //
        // Only genuinely discrete re-anchoring events snap:
        //
        // Guard 1 — tap-count change: topology anchoring re-distributes ALL
        // intermediate taps (potentially by hundreds of samples). Gliding
        // that sweep is the "mangled / repitched" artifact. Snap.
        //
        // Guard 2 — newly-activated tap: when a tap goes from inactive to
        // active its reader hasn't been tracking the real nominal and starts
        // from wherever it last was (usually 1.0 after reset). Snap it.
        //
        // Guard 3 — initial snap after reset (same as before).
        let n_active_changed = self.prev_n_active != n_active;
        let topology_jump = n_active_changed;
        let initial_snap = self.needs_initial_snap;

        for i in 0..NUM_TAPS {
            let active_now = tap_in_range[i] && i < n_active;
            let newly_active = active_now && !self.prev_tap_active[i];
            if initial_snap || topology_jump || newly_active {
                self.main_l.readers[i].snap_to(nom_l[i].max(2.0));
                self.main_r.readers[i].snap_to(nom_r[i].max(2.0));
            }
            self.prev_tap_active[i] = active_now;
        }

        self.prev_n_active = n_active;
        self.needs_initial_snap = false;
        if initial_snap {
            for d in [&mut self.diff_wet_l, &mut self.diff_wet_r, &mut self.diff_odd, &mut self.diff_even] {
                d.set_size(p.diffuse_size.value());
                d.snap_size();
            }
        }

        // Per-tap filter configuration. Each tap's filter is a cascade of
        // 1-4 SVF stages (1=12 dB/oct, 2=24, 4=48). All stages get the same
        // cutoff/Q. Cascaded biquads do shift the -3dB point slightly with
        // each stage, but for a music plugin the perceptual difference is
        // exactly "steeper roll-off", which is what the slope control means.
        // The filters run at the gliding cutoff and Q (see `LogGlide`); a
        // new type starts a crossfade from a copy of the filter as it was.
        let mut tap_cut = [0.0f32; NUM_TAPS];
        let mut tap_q = [0.0f32; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let tap = &p.taps[i];
            (tap_cut[i], tap_q[i]) = (tap.filter_cutoff.value(), tap.filter_q.value());
            let ty = tap.filter_type.value();
            let lay = TapLayout::of(tap.filter_slope.value());
            if initial_snap {
                (self.tap_cut[i], self.tap_q[i]) = (LogGlide::at(tap_cut[i]), LogGlide::at(tap_q[i]));
                self.tap_type_now[i] = ty;
                self.tap_layout_now[i] = lay;
                self.tap_type_xf[i] = 1.0;
            } else if ty != self.tap_type_now[i] || lay != self.tap_layout_now[i] {
                self.tap_chain_old_l[i] = self.tap_chain_l[i];
                self.tap_chain_old_r[i] = self.tap_chain_r[i];
                self.tap_type_old[i] = self.tap_type_now[i];
                self.tap_layout_old[i] = self.tap_layout_now[i];
                self.tap_chain_l[i].wake(self.tap_layout_now[i], lay);
                self.tap_chain_r[i].wake(self.tap_layout_now[i], lay);
                self.tap_type_now[i] = ty;
                self.tap_layout_now[i] = lay;
                self.tap_type_xf[i] = 0.0;
            }
            self.set_tap_coefs(i, sr);
        }

        // the shared level knob: silent at its bottom, not −60 dB
        use softsystems_ui::knob::level;
        let input_gain = level::gain(level::norm(p.input_gain.value()));
        let output_gain = level::gain(level::norm(p.output_gain.value()));
        let dry_wet = p.dry_wet.value();
        let feedback = p.feedback.value();
        // Colour's amounts as set; Motion's Colour target scales them (below).
        let diffuse_base = p.diffuse_amount.value();
        let mut diffuse_amount = diffuse_base;
        let diffuse_routing = p.diffuse_routing.value();
        let diffuse_size = p.diffuse_size.value();
        // Size glides in the chains (allocation-free). Ping's bounce loop
        // gets the same, so both loops take back the same delay.
        for d in [&mut self.diff_wet_l, &mut self.diff_wet_r, &mut self.diff_odd, &mut self.diff_even] {
            d.set_size(diffuse_size);
        }

        // DEGRADE block — configured once per block (filter design happens in
        // set_params, control rate), inserted per sample at the selected
        // routing point. Gate on `amount` so the bypass path costs nothing.
        let degrade_amount = p.degrade_amount.value();
        let degrade_routing = p.degrade_routing.value();
        let degrade_on = degrade_amount > 1e-4;
        let (tape_drive, tape_wear) = (p.degrade_drive.value(), p.degrade_wear.value());
        let tape_kind = p.degrade_type.value();
        let tape_cfg = |mix: f32| TapeDegradeParams {
            kind: match tape_kind {
                TapeType::I => TapeKind::I,
                TapeType::II => TapeKind::II,
                TapeType::IV => TapeKind::IV,
            },
            drive_db: tape_drive,
            wear: tape_wear,
            mix,
        };
        if degrade_on {
            self.tape.set_params(&tape_cfg(degrade_amount));
            self.tape_bounce.set_params(&tape_cfg(degrade_amount));
        }
        // SATURATE block — playable tanh drive stage (the loop's fixed safety
        // tanh is separate and always on). Same routing grammar as DIFFUSE.
        let sat_base = p.sat_amount.value();
        let mut sat_amount = sat_base;
        let sat_routing = p.sat_routing.value();
        let sat_gain = nice_plug::util::db_to_gain(p.sat_drive.value());
        let sat_on = sat_base > 1e-4;
        // the crate's curve, if one is picked (beta pass 6): its make-up holds the level, its
        // Mix is Amount; Drive is the same knob
        let sat_engine = match p.sat_curve.value() {
            SatCurve::Classic => None,
            SatCurve::Tube => Some(SatEngine::Tube),
            SatCurve::Smooth => Some(SatEngine::Tape),
            SatCurve::Grit => Some(SatEngine::Grit),
            SatCurve::Amp => Some(SatEngine::Amp),
            SatCurve::Transfo => Some(SatEngine::Transformer),
        };
        let sat_drive_db = p.sat_drive.value();
        let sat_settings = move |mix: f32| -> Option<crate_sat::Settings> {
            sat_engine.map(|engine| crate_sat::Settings {
                engine,
                drive_db: sat_drive_db,
                mix,
                ..crate_sat::map::settings(&crate_sat::map::DEFAULT_CELLS)
            })
        };
        if let (true, Some(s)) = (sat_on, sat_settings(sat_base)) {
            match sat_routing {
                DiffuseRouting::Input => self.sat_in.set(&s),
                DiffuseRouting::Output => self.sat_out.set(&s),
                DiffuseRouting::Feedback => {
                    self.sat_main.set(&s);
                    self.sat_bounce.set(&s);
                }
            }
        }
        // (the stage itself, `sat`, is made per sample: Motion can move its amount)
        // DUB block (beta pass 12): always in the loop; on while Intensity is up
        let dub_base = p.dub_intensity.value();
        let dub_on = dub_base > 1e-4;
        let dub_params = |intensity: f32| DubParams {
            hp_hz: p.dub_low.value(),
            lp_hz: p.dub_high.value(),
            intensity,
            ap_hz: p.dub_swirl.value(),
            ap_stages: p.dub_swirls.value().clamp(0, 3) as usize,
            color: DUB_COLOR,
        };
        if dub_on {
            let dp = dub_params(dub_base);
            self.dub_main.iter_mut().chain(self.dub_bounce.iter_mut()).for_each(|d| d.set(&dp));
        }

        // FB CURVE — decay shape of the repeats. One-pole envelope follower
        // coefficients (5 ms attack / 150 ms release).
        let fb_curve = p.fb_curve.value();
        let env_att = 1.0 - (-1.0 / (0.005 * sr)).exp();
        let env_rel = 1.0 - (-1.0 / (0.150 * sr)).exp();

        let ch_mode = p.channel_mode.value();

        // Feedback delay time.
        // When LOCKED: FB TIME = the cluster span (delay of the last active tap),
        //   so the full cluster echoes as a unit after it finishes playing.
        // When UNLOCKED: FB TIME = the independent feedback_time param.
        // Spiral protection: clamp minimum to 10 ms regardless of mode so a very
        //   short FB TIME can't cause exponential runaway by cycling faster than
        //   the cluster itself decays (sounds like a detuned squeal).
        let min_fb_samples = topology::MIN_FB_SECS * sr; // 10 ms hard floor
        // Cluster span = max delay across all active in-range taps (includes time_offset).
        // Used both as the LOCKED target AND as a floor for the UNLOCKED target — so
        // feedback NEVER lands before any of the direct taps (which would create the
        // ugly "feedback racing the taps" wash the user described).
        // Shared with gui.rs::compute_viz_data so echo bars land where the audio does.
        let cluster_span_samples =
            topology::cluster_span_secs(&t_secs_l, &t_secs_r, &tap_in_range, n_active, p.pattern_secs(base_sec_l, base_sec_r)) * sr;
        let fb_time_target = if p.feedback_time_locked.value() {
            cluster_span_samples
        } else {
            // UNLOCKED is fully free above the 10 ms squeal floor (v2.18.2).
            // The old cluster-span floor ("echo can only land LATER than the
            // last tap") existed because short cycles blew up violently — but
            // that was really the unbounded loop's fault (no saturation, and
            // feedback goes to 1.05 → exponential runaway that doubles faster
            // the shorter the cycle). The loop is now soft-saturated inside
            // (see fb_sat below), so short FB times are a bounded, usable
            // dub-wash / comb zone instead of a detonation.
            (p.feedback_time_seconds(tempo_bpm) * sr).clamp(min_fb_samples, max_delay_samples)
        };
        let fb_time_target = fb_time_target.max(min_fb_samples);
        // The feedback read head follows the same transition rules as the
        // taps: snap on discrete re-anchoring (tap-count change, reset) and
        // in Jump mode; otherwise a BOUNDED glide. The old fixed 0.02
        // samples/sample rate made large FB-TIME moves take minutes to
        // arrive (1/16 → 1/1 at 120 BPM ≈ 90k samples ≈ 94 s of glide) —
        // the knob appeared to do nothing.
        if topology_jump || initial_snap || delay_mode == DelayMode::Jump {
            self.fb_time_slew.snap_to(fb_time_target);
        } else {
            let rate = bend_slew_rate
                .max((fb_time_target - self.fb_time_slew.current()).abs() / glide_samples);
            self.fb_time_slew.set_rate(rate);
            self.fb_time_slew.set_target(fb_time_target);
        }
        // Freeze reads the loop at a length latched as the hold starts from
        // nothing, in whole samples so the held loop repeats bit for bit.
        // While it fully holds, the feedback head waits there, so letting go
        // moves the cycle on the usual way (Repitch glides it, Jump snaps it).
        // The page is told the cycle as it was, not its whole-sample rounding,
        // so its readout doesn't change as the hold begins.
        let freeze_target = if p.freeze.value() { 1.0 } else { 0.0 };
        if freeze_target > 0.0 && self.freeze_w == 0.0 {
            let at = self.fb_time_slew.current().max(2.0);
            self.freeze_len = at.round();
            self.params.held_cycle_secs.store((at / sr).to_bits(), Ordering::Relaxed);
        }
        if initial_snap {
            self.freeze_w = freeze_target;
        }
        if self.freeze_w >= 1.0 {
            self.fb_time_slew.snap_to(self.freeze_len);
        }
        if freeze_target == 0.0 && self.freeze_w == 0.0 {
            self.params.held_cycle_secs.store(0, Ordering::Relaxed);
        }
        let crossfeed = p.crossfeed.value();
        let crossfeed_mode = p.crossfeed_mode.value();

        // Clear: the echoes fade out over 10 ms, then the delay empties.
        let clear_len = ((0.01 * sr) as usize).max(1);
        if self.params.clear_request.swap(false, Ordering::Relaxed) && self.clear_left.is_none() {
            self.clear_left = Some(clear_len);
        }
        let mut cleared = false;
        // Safety: the loop's DC blockers (5 Hz); Ear guard keeps echoes that
        // stay over −8 dBFS RMS (a 300 ms average) for a couple of seconds
        // down to it — 1.5 s to ease in, 0.5 s to let go.
        let dc_r = 1.0 - core::f32::consts::TAU * 5.0 / sr;
        let guard_on = p.ear_guard.value();
        const GUARD_CEIL: f32 = 0.398; // −8 dBFS
        let guard_avg = 1.0 - (-1.0 / (0.3 * sr)).exp();
        let guard_att = 1.0 - (-1.0 / (1.5 * sr)).exp();
        let guard_rel = 1.0 - (-1.0 / (0.5 * sr)).exp();
        let mut guard_least = 1.0f32;
        let mut fault = false;

        // Repeats: a count, or ∞ (the loop).
        let n_rep = p.repeats.value();
        let finite = n_rep < REPEATS_LOOP;
        let n_rep = n_rep as usize;
        let rep_target = if finite { 1.0 } else { 0.0 };
        if finite {
            self.rep_n = n_rep;
        }
        // the copies play while a count is set or the loop is still taking over
        let n_copies = if finite || self.rep_w > 0.0 { self.rep_n } else { 0 };
        if initial_snap {
            self.rep_w = rep_target;
        }

        let mut tap_gain = [0.0f32; NUM_TAPS];
        let mut tap_pan_l = [1.0f32; NUM_TAPS];
        let mut tap_pan_r = [1.0f32; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;
            tap_gain[i] = if active {
                nice_plug::util::db_to_gain(p.taps[i].gain_db.value())
            } else { 0.0 };
            (tap_pan_l[i], tap_pan_r[i]) = bounce::pan_gains(p.taps[i].pan.value());
            // A tap that isn't playing waits where it's going: one that starts
            // (unmuted, or added) comes in over the glide from silence.
            if initial_snap || !(i < n_active && tap_in_range[i]) {
                (self.tap_gain_mid[i], self.tap_gain_now[i]) = (tap_gain[i], tap_gain[i]);
                self.tap_pan_mid[i] = (tap_pan_l[i], tap_pan_r[i]);
                self.tap_pan_now[i] = self.tap_pan_mid[i];
            }
        }
        let tap_glide = 1.0 - (-1.0 / (0.005 * sr)).exp();
        // cents: a tap's Detune is a % of the range (v2.46)
        let det_range = p.detune_range.value() as f32;
        let tap_det: [f32; NUM_TAPS] = core::array::from_fn(|i| p.taps[i].detune.value() * det_range);
        let det_hold = (0.3 * sr) as usize;
        let clean_on = p.detune_quality.value() == crate::params::DetuneQuality::Clean && !self.pv.is_empty();

        // Motion (beta pass 8, the shared crate): its slots move the taps'
        // pan, gain, cutoff and Q around their set values (each tap a
        // read-head, Scatter apart) and the whole echo's knobs, by a share of
        // each knob's travel; with no routes it leaves them as set. It
        // re-aims every 32 samples and the glides smooth the steps.
        self.modu.begin_block(&p.motion, ctx.transport(), n_active);
        let motion = self.modu.routed();
        let tap_on = self.modu.moves_taps();
        let routed_to: [bool; N_TGT] = core::array::from_fn(|t| self.modu.reaches(t));
        // the whole-echo targets Motion aims at (their knobs while nothing moves them)
        let (mut fb_target, mut cross_target, mut wet_target) = (feedback, crossfeed, dry_wet);
        if initial_snap {
            (self.fb_now, self.cross_now, self.mix_now) = (feedback, crossfeed, dry_wet);
        }
        let set_db: [f32; NUM_TAPS] = core::array::from_fn(|i| p.taps[i].gain_db.value());
        let set_pan: [f32; NUM_TAPS] = core::array::from_fn(|i| p.taps[i].pan.value());
        let (set_cut, set_q) = (tap_cut, tap_q);
        // the tap knobs' positions, which the routes move from
        let tp = &p.taps;
        let n_pan: [f32; NUM_TAPS] = core::array::from_fn(|i| tp[i].pan.preview_normalized(set_pan[i]));
        let n_db: [f32; NUM_TAPS] = core::array::from_fn(|i| tp[i].gain_db.preview_normalized(set_db[i]));
        let n_cut: [f32; NUM_TAPS] = core::array::from_fn(|i| tp[i].filter_cutoff.preview_normalized(set_cut[i]));
        let n_q: [f32; NUM_TAPS] = core::array::from_fn(|i| tp[i].filter_q.preview_normalized(set_q[i]));
        let type_step = 1.0 / (0.01 * sr);

        // ── Ping (see bounce.rs) ─────────────────────────────────────────
        // The loop becomes the bounce once Ping is picked (after Diffuse has
        // faded out of the loop, if it was on). Handing it back waits 30 ms
        // while every tap glides its repeats to centre, so the main lines
        // pick them up where they already sat.
        let ping_req = crossfeed_mode == CrossfeedMode::PingPong;
        let ping_was_live = self.ping_on || self.ping_share > 0.0 || self.ping_live > 0;
        if ping_req {
            if !self.ping_on && self.xf_diffuse == 0.0 {
                self.ping_on = true;
                self.ping_age = 0;
                self.ping_live = 0;
            }
            self.ping_leave = None;
        } else if self.ping_on && self.ping_leave.is_none() {
            self.ping_leave = Some(((0.03 * sr) as usize).max(1));
        }
        let width = bounce::width(ch_mode, self.cross_now);
        let swinging = self.ping_on && self.ping_leave.is_none();
        let mut ping_target = [[0.0f32; 4]; NUM_TAPS];
        let mut ping_tap_fresh = [false; NUM_TAPS];
        let mut ping_play_out = self.fb_time_slew.current().max(fb_time_target) as usize;
        for i in 0..NUM_TAPS {
            let reach = self.main_l.readers[i].reach().max(self.main_r.readers[i].reach()) as usize + 4;
            // a tap swings once the first bounced repeat has reached it
            let w = if swinging && self.ping_age >= reach { width } else { 0.0 };
            // (Motion's pan as of the last block: the placement glides anyway)
            let pan = if routed_to[T_PAN] {
                p.taps[i].pan.preview_plain(self.modu.apply_tap(T_PAN, i, n_pan[i]))
            } else {
                p.taps[i].pan.value()
            };
            ping_target[i] = bounce::placement(ch_mode, pan, w);
            if !ping_was_live {
                self.ping_gain[i] = ping_target[i];
                self.ping_gain_mid[i] = ping_target[i];
            }
            ping_tap_fresh[i] = reach <= self.ping_fresh;
            if i < n_active && tap_in_range[i] {
                ping_play_out = ping_play_out.max(reach);
            }
        }
        ping_play_out += 64;
        let ping_glide = 1.0 - (-1.0 / (0.003 * sr)).exp();

        // Mix's output blend and Diffuse's loop allpass fade with the mode.
        // Diffuse only runs in the main loop: it waits for Ping to hand the
        // loop back, and Ping waits for it to fade out.
        let xf_step = 1.0 / (0.02 * sr);
        let mix_target = if crossfeed_mode == CrossfeedMode::Mix { 1.0 } else { 0.0 };
        let diffuse_target = if crossfeed_mode == CrossfeedMode::Diffuse && !self.ping_on && self.ping_share == 0.0 {
            1.0
        } else {
            0.0
        };
        if diffuse_target > 0.0 && self.x_feed == 0.0 {
            // fading in from nothing: no stale tail from the last time
            self.diff_l.reset();
            self.diff_r.reset();
        }
        if initial_snap {
            self.xf_mix = mix_target;
            self.xf_diffuse = diffuse_target;
            self.x_feed = diffuse_target;
            self.x_warm = X_WARM_DONE;
        }
        // Cross → Diffuse's chain is fed first (its input fading in) and
        // heard once it's warm: its 31 ms delay and its tail are longer than
        // the 20 ms crossfade, and would otherwise start abruptly (v2.47).
        let x_warm_len = (0.08 * sr) as usize;

        // The loop tone glides to the knob, a unit per 20 ms.
        let tone_base = p.loop_tone.value();
        let mut tone_target = tone_base;
        if initial_snap {
            self.tone_t = tone_target;
        }
        let tone_step = 1.0 / (0.02 * sr);

        // What the feedback path does to each pass, in both loops. Diffuse,
        // Tape and the tone have memory: a loop that stops getting input
        // keeps running this long so they ring out (the tone's longest tail
        // is its high-pass's, 8 ms at 20 Hz).
        let mut colour = LoopColour {
            diffuse: (diffuse_routing == DiffuseRouting::Feedback).then_some(diffuse_amount),
            tape: degrade_on && degrade_routing == DiffuseRouting::Feedback,
            sat: sat_on && sat_routing == DiffuseRouting::Feedback,
            sat_crate: sat_engine.is_some(),
            dub: if dub_on { dub_wet(dub_base) } else { 0.0 },
            tone: ToneCoefs::new(self.tone_t, sr),
        };
        let ring_len = if colour.diffuse.is_some() || colour.tape || dub_on {
            (3.0 * sr) as usize
        } else if tone_target != 0.0 || self.tone_t != 0.0 || routed_to[T_TONE] {
            (0.1 * sr) as usize
        } else {
            1
        };
        // Ping's lines are read (by the loop and the taps) while they may hold anything.
        let ping_read = self.ping_on || self.ping_share > 0.0 || self.ping_live > 0;

        // Ducking: the depth glides to the Duck knob; the follower's attack and
        // hold are fixed, its release is the knob's.
        let duck_base = p.duck.value();
        let mut duck_target = duck_base;
        if initial_snap {
            self.duck_depth = duck_target;
        }
        let duck_step = 24.0 / (0.02 * sr);
        let duck_att = 1.0 - (-1.0 / (0.005 * sr)).exp();
        let duck_rel = 1.0 - (-1.0 / (p.duck_release.value() * 0.001 * sr)).exp();
        let duck_hold_len = (0.02 * sr) as usize;
        let duck_smooth = 1.0 - (-1.0 / (0.002 * sr)).exp();
        let mut duck_most = 0.0f32;

        let chans = buffer.as_slice();
        let n_samples = chans[0].len();
        let (l_chan, rest) = chans.split_at_mut(1);
        let l_chan = &mut l_chan[0];
        let r_chan = &mut rest[0];

        for s in 0..n_samples {
            let mut in_l = l_chan[s];
            let mut in_r = r_chan[s];

            // Ducking's key: the dry input as it arrives, both sides together
            // (before In and the channel mode), so the image never shifts.
            let key = in_l.abs().max(in_r.abs());
            if key >= self.duck_env {
                self.duck_env += (key - self.duck_env) * duck_att;
                self.duck_hold = duck_hold_len;
            } else if self.duck_hold > 0 {
                self.duck_hold -= 1;
            } else {
                self.duck_env += (key - self.duck_env) * duck_rel;
            }
            // Follow hears the dry input's level
            self.modu.advance(key);

            // Motion: re-aim the live taps and the whole echo (the glides
            // smooth the steps). A target no route reaches stays on its knob.
            if motion && s % 32 == 0 {
                self.modu.update();
                let m = &self.modu;
                if routed_to[T_FDBK] {
                    fb_target = p.feedback.preview_plain(m.apply(T_FDBK, p.feedback.preview_normalized(feedback)));
                }
                if routed_to[T_CROSS] {
                    cross_target = p.crossfeed.preview_plain(m.apply(T_CROSS, p.crossfeed.preview_normalized(crossfeed)));
                }
                if routed_to[T_MIX] {
                    wet_target = p.dry_wet.preview_plain(m.apply(T_MIX, p.dry_wet.preview_normalized(dry_wet)));
                }
                if routed_to[T_TONE] {
                    tone_target = p.loop_tone.preview_plain(m.apply(T_TONE, p.loop_tone.preview_normalized(tone_base)));
                }
                if routed_to[T_DUCK] {
                    duck_target = p.duck.preview_plain(m.apply(T_DUCK, p.duck.preview_normalized(duck_base)));
                }
                if routed_to[T_COLOUR] {
                    let c = m.apply(T_COLOUR, modulation::COLOUR_BASE);
                    diffuse_amount = modulation::colour(diffuse_base, c);
                    colour.diffuse = (diffuse_routing == DiffuseRouting::Feedback).then_some(diffuse_amount);
                    sat_amount = modulation::colour(sat_base, c);
                    if let (true, Some(s)) = (sat_on, sat_settings(sat_amount)) {
                        for st in [&mut self.sat_in, &mut self.sat_out, &mut self.sat_main, &mut self.sat_bounce] {
                            st.set(&s);
                        }
                    }
                    if dub_on {
                        let i = modulation::colour(dub_base, c);
                        colour.dub = dub_wet(i);
                        let dp = dub_params(i);
                        self.dub_main.iter_mut().chain(self.dub_bounce.iter_mut()).for_each(|d| d.set(&dp));
                    }
                    if degrade_on {
                        let cfg = tape_cfg(modulation::colour(degrade_amount, c));
                        self.tape.set_params(&cfg);
                        self.tape_bounce.set_params(&cfg);
                    }
                }
                if tap_on {
                    for i in 0..n_active {
                        if routed_to[T_PAN] {
                            (tap_pan_l[i], tap_pan_r[i]) = bounce::pan_gains(tp[i].pan.preview_plain(m.apply_tap(T_PAN, i, n_pan[i])));
                        }
                        if routed_to[T_GAIN] && tap_gain[i] > 0.0 {
                            tap_gain[i] = nice_plug::util::db_to_gain(tp[i].gain_db.preview_plain(m.apply_tap(T_GAIN, i, n_db[i])));
                        }
                        if routed_to[T_CUTOFF] {
                            tap_cut[i] = tp[i].filter_cutoff.preview_plain(m.apply_tap(T_CUTOFF, i, n_cut[i]));
                        }
                        if routed_to[T_RESO] {
                            tap_q[i] = tp[i].filter_q.preview_plain(m.apply_tap(T_RESO, i, n_q[i]));
                        }
                    }
                }
            }
            self.fb_now += (fb_target - self.fb_now) * tap_glide;
            self.cross_now += (cross_target - self.cross_now) * tap_glide;
            self.mix_now += (wet_target - self.mix_now) * tap_glide;
            let (fb, dry_wet) = (self.fb_now, self.mix_now);
            // Saturate: out = dry·(1−mix) + tanh(dry·gain)·mix — no gain
            // compensation (project rule: never gain-compensate the loop).
            let sat = |x: f32| -> f32 { x * (1.0 - sat_amount) + soft_clip(x * sat_gain) * sat_amount };

            // Channel mode encode
            match ch_mode {
                ChannelMode::MidSide => {
                    let m = (in_l + in_r) * 0.5;
                    let sd = (in_l - in_r) * 0.5;
                    in_l = m;
                    in_r = sd;
                }
                ChannelMode::Mono => {
                    let m = (in_l + in_r) * 0.5;
                    in_l = m;
                    in_r = m;
                }
                ChannelMode::Stereo => {}
            }

            let mut x_l = in_l * input_gain;
            let mut x_r = in_r * input_gain;

            // DIFFUSE routing = Input: smear the dry input BEFORE it hits the delay.
            // Every tap then reads pre-smeared material, giving a layered feel.
            // (In and Out start the chain afresh when Amount leaves 0.)
            let wet_route = matches!(diffuse_routing, DiffuseRouting::Input | DiffuseRouting::Output);
            if !(wet_route && diffuse_amount > 1e-4) {
                self.wet_diff_on = false;
            } else if !self.wet_diff_on {
                self.diff_wet_l.reset();
                self.diff_wet_r.reset();
                self.wet_diff_on = true;
            }
            if self.wet_diff_on && diffuse_routing == DiffuseRouting::Input {
                x_l = self.diff_wet_l.blend(x_l, diffuse_amount);
                x_r = self.diff_wet_r.blend(x_r, diffuse_amount);
            }

            // DEGRADE routing = Input: crush the dry input before the buffer —
            // every tap reads pre-degraded material (single lo-fi generation).
            if degrade_on && degrade_routing == DiffuseRouting::Input {
                self.tape.process(&mut x_l, &mut x_r);
            }

            // SATURATE routing = Input: drive the dry input into the buffer.
            if sat_on && sat_routing == DiffuseRouting::Input {
                if colour.sat_crate {
                    // 20 samples (0.4 ms) later into the line: the echoes, not the dry
                    self.sat_in.process_stereo(&mut x_l, &mut x_r);
                } else {
                    x_l = sat(x_l);
                    x_r = sat(x_r);
                }
            }

            // ─── Cluster-Delay–style feedback (v2.11.0 architecture) ─────
            // Read the main buffer at cluster_span samples back and feed THAT
            // (not the tap-output sum) back into the write input. The buffer
            // already contains the cluster shape; re-injecting it produces a
            // perfect, time-aligned repeat of the whole pattern — every cycle
            // identical, just attenuated by `feedback`.
            //
            // Previous (v2.7–v2.10) implementation summed N tap outputs into a
            // feedback bus. Each tap reads at a different delay, so summing
            // collapses N points in time into one moment — which then got
            // re-tapped at N different delays again the next cycle. Result:
            // exponential smearing instead of clean repeats, AND a 1/N
            // normalisation was needed to keep it from running away, which
            // crushed the feedback level.
            //
            // The new path also makes feedback INDEPENDENT of tap layout: moving
            // the XY pad or modulating with Wobble no longer perturbs what gets
            // recirculated, eliminating the "ringing/saturation" the user heard.
            let cf = self.cross_now.min(1.0);
            let fb_samps = self.fb_time_slew.tick();
            // Read BEFORE write so we sample the buffer state from cluster_span
            // ago, not the value we're about to write.
            let fb_at = fb_samps.max(2.0);
            let b = s_curve(self.ping_share);
            // Diffuse in the loops delays every pass by its chain's length, so
            // they read that much earlier and the cycle stays as set (v2.47).
            // Cross → Diffuse adds its own chain to the main loop: while it
            // fades in or out the loop is read at both times, `plain` without
            // it and `crossed` with it.
            let comp = if colour.diffuse.is_some() { self.diff_wet_l.bulk() } else { 0.0 }
                // the crate's Saturate in the loop delays each pass by its oversampling (pass 6)
                + if colour.sat && colour.sat_crate { crate_sat::LATENCY as f32 } else { 0.0 };
            let x_w = s_curve(self.xf_diffuse);
            let at_plain = (fb_at - comp).max(2.0);
            let at_crossed = (at_plain - self.diff_l.bulk()).max(2.0);
            let plain = if x_w < 1.0 { self.loop_inputs(at_plain, b, ch_mode, ping_read) } else { [0.0; 4] };
            let x_fed = self.x_feed > 0.0;
            let crossed = if x_fed { self.loop_inputs(at_crossed, b, ch_mode, ping_read) } else { [0.0; 4] };
            let [main_l, main_r, odd_in, even_in] = if x_w < 1.0 { plain } else { crossed };
            // Freeze holds the same, read at its latched length, and writes it
            // back untouched; while it fades in or out both paths run.
            let hold = s_curve(self.freeze_w);
            let held = if hold == 0.0 {
                [0.0; 4]
            } else if x_w < 1.0 && at_plain == self.freeze_len {
                plain
            } else {
                self.loop_inputs(self.freeze_len, b, ch_mode, ping_read)
            };

            // FB CURVE — bend the effective loop gain by level so the decay
            // envelope isn't limited to the natural exponential. The envelope
            // tracks the loop signal (pre-gain), linked across channels.
            let fb_gain = match fb_curve {
                FbCurve::Natural => fb,
                _ => {
                    let mag = main_l.abs().max(main_r.abs()).max(odd_in.abs()).max(even_in.abs());
                    let coeff = if mag > self.fb_env { env_att } else { env_rel };
                    self.fb_env += (mag - self.fb_env) * coeff;
                    match fb_curve {
                        // Compressor-in-the-loop: gain bends toward a plateau
                        // at P — loud repeats tamed, quiet ones lifted (capped
                        // at 1.15× so the tail still dies with feedback < 1).
                        FbCurve::Bloom => {
                            const P: f32 = 0.35;
                            let bend = (P / self.fb_env.max(1e-4)).powf(0.4);
                            fb * bend.clamp(0.5, 1.15)
                        }
                        // Expander-in-the-loop: full feedback while the tail
                        // is above the gate threshold, accelerating cut below
                        // it. FEEDBACK sets how long the hold lasts.
                        FbCurve::Gate => {
                            const T: f32 = 0.1;
                            if self.fb_env >= T { fb }
                            else {
                                let k = self.fb_env / T;
                                fb * k * k
                            }
                        }
                        FbCurve::Natural => unreachable!(),
                    }
                }
            };

            // Repeats: a count plays that many copies of the pattern, each `fb`
            // times the last, read from the dry input's own line, and then
            // stops (in Ping the odd copies bounce one way, the even the
            // other); ∞ is the loop. While one hands over to the other both
            // run, crossfading. The loop's colour below shapes the copies too,
            // once (a count has no passes for it to build up over).
            // The copies pass through the loop's colour once, so they're read
            // early by the same amounts.
            let w_rep = s_curve(self.rep_w);
            let fir_on = finite || self.rep_w > 0.0 || self.rep_gain.iter().any(|&g| g > 0.0);
            if fir_on {
                let mut gk = 1.0f32;
                for k in 0..16 {
                    gk *= fb;
                    let at = (k + 1) as f32 * fb_at;
                    let to = if k < n_copies && at < max_delay_samples { gk } else { 0.0 };
                    let g = &mut self.rep_gain[k];
                    *g += (to - *g) * tap_glide;
                    if to == 0.0 && g.abs() < 1e-6 {
                        *g = 0.0;
                    }
                }
            }
            let fir_at = |early: f32| {
                let mut fir = [0.0f32; 4];
                if !fir_on {
                    return fir;
                }
                for (k, &g) in self.rep_gain.iter().enumerate() {
                    if g == 0.0 {
                        continue;
                    }
                    let at = ((k + 1) as f32 * fb_at - early).max(2.0);
                    let (dl, dr) = (self.dry_l.read_before_write(at) * g, self.dry_r.read_before_write(at) * g);
                    fir[0] += dl;
                    fir[1] += dr;
                    if b > 0.0 {
                        // copy 1 is the first bounce (odd), copy 2 the second …
                        fir[2 + k % 2] += bounce::mono_of(ch_mode, dl, dr);
                    }
                }
                fir
            };
            let lw = 1.0 - w_rep;
            let feed = |i: [f32; 4], fir: [f32; 4]| {
                [
                    i[0] * fb_gain * lw + fir[0] * (1.0 - b) * w_rep,
                    i[1] * fb_gain * lw + fir[1] * (1.0 - b) * w_rep,
                    i[2] * fb_gain * lw + fir[2] * b * w_rep,
                    i[3] * fb_gain * lw + fir[3] * b * w_rep,
                ]
            };
            let fb_main = if x_w < 1.0 { feed(plain, fir_at(fb_at - at_plain)) } else { [0.0; 4] };
            let fb_crossed = if x_fed { feed(crossed, fir_at(fb_at - at_crossed)) } else { [0.0; 4] };

            if self.tone_t != tone_target {
                self.tone_t += (tone_target - self.tone_t).clamp(-tone_step, tone_step);
                colour.tone = ToneCoefs::new(self.tone_t, sr);
            }

            // The main loop, while it has input or its colour rings out.
            let (mut fb_sat_l, mut fb_sat_r) = (0.0, 0.0);
            if b < 1.0 || self.main_quiet < ring_len {
                if self.main_quiet >= ring_len {
                    // starting again: nothing left over from the last time
                    // (only what's in the loop; Diffuse and Tape may be routed elsewhere)
                    if colour.diffuse.is_some() {
                        self.diff_wet_l.reset();
                        self.diff_wet_r.reset();
                    }
                    if colour.tape {
                        self.tape.reset();
                    }
                    self.dub_main.iter_mut().for_each(DubLoop::reset);
                    if colour.sat && colour.sat_crate {
                        self.sat_main.reset();
                        if let Some(s) = sat_settings(sat_amount) {
                            self.sat_main.set(&s);
                        }
                    }
                    self.tone_main.reset();
                }
                let mut fb_in_l = fb_main[0];
                let mut fb_in_r = fb_main[1];

                // Cross → Diffuse: its own chain in the main loop, Cross its
                // density (v2.47; 0 is just the delay the read takes back).
                // It crossfades with the plain loop as the mode changes. Mix
                // acts on the output (after the tap loop); Ping is the bounce
                // loop.
                if x_fed {
                    let dl = self.diff_l.process(fb_crossed[0] * self.x_feed, cf);
                    let dr = self.diff_r.process(fb_crossed[1] * self.x_feed, cf);
                    fb_in_l = fb_in_l * (1.0 - x_w) + dl * x_w;
                    fb_in_r = fb_in_r * (1.0 - x_w) + dr * x_w;
                }
                (fb_sat_l, fb_sat_r) = colour.apply(
                    fb_in_l,
                    fb_in_r,
                    &mut self.diff_wet_l,
                    &mut self.diff_wet_r,
                    &mut self.tape,
                    &mut self.tone_main,
                    &sat,
                    &mut self.dub_main,
                    &mut self.sat_main,
                );
            }

            // The bounce loop, the same way, with its own Diffuse, Tape and tone.
            let (mut odd_out, mut even_out) = (0.0, 0.0);
            if b > 0.0 || self.bounce_quiet < ring_len {
                if self.bounce_quiet >= ring_len {
                    self.diff_odd.reset();
                    self.diff_even.reset();
                    self.tape_bounce.reset();
                    self.dub_bounce.iter_mut().for_each(DubLoop::reset);
                    if colour.sat && colour.sat_crate {
                        self.sat_bounce.reset();
                        if let Some(s) = sat_settings(sat_amount) {
                            self.sat_bounce.set(&s);
                        }
                    }
                    self.tone_bounce.reset();
                }
                (odd_out, even_out) = colour.apply(
                    fb_main[2],
                    fb_main[3],
                    &mut self.diff_odd,
                    &mut self.diff_even,
                    &mut self.tape_bounce,
                    &mut self.tone_bounce,
                    &sat,
                    &mut self.dub_bounce,
                    &mut self.sat_bounce,
                );
            }

            // no offset builds up round the loops (the hold below stays bit for bit)
            fb_sat_l = self.dc[0].process(fb_sat_l, dc_r);
            fb_sat_r = self.dc[1].process(fb_sat_r, dc_r);
            odd_out = self.dc[2].process(odd_out, dc_r);
            even_out = self.dc[3].process(even_out, dc_r);

            // Freeze: the hold takes over what's written back, at exactly
            // 100 % (no Feedback, Curve or colour), and new input fades out.
            if hold > 0.0 {
                fb_sat_l = fb_sat_l * (1.0 - hold) + held[0] * hold;
                fb_sat_r = fb_sat_r * (1.0 - hold) + held[1] * hold;
                odd_out = odd_out * (1.0 - hold) + held[2] * hold;
                even_out = even_out * (1.0 - hold) + held[3] * hold;
            }
            let keep = 1.0 - hold;

            // Write the dry input + processed feedback into the main delay.
            // The taps below read from this buffer; each cluster_span the same
            // cluster pattern appears in the buffer, attenuated by feedback^k.
            // In Ping the repeats go to the bounce lines instead.
            self.main_l.delay.write(x_l * keep + fb_sat_l);
            self.main_r.delay.write(x_r * keep + fb_sat_r);
            self.dry_l.write(x_l * keep);
            self.dry_r.write(x_r * keep);
            self.ping_odd.write(odd_out);
            self.ping_even.write(even_out);
            self.main_quiet = if b < 1.0 { 0 } else { self.main_quiet.saturating_add(1) };
            self.bounce_quiet = if b > 0.0 { 0 } else { self.bounce_quiet.saturating_add(1) };

            let mut tap_out_l = 0.0;
            let mut tap_out_r = 0.0;
            for i in 0..n_active {
                if !tap_in_range[i] { continue; }
                let hl = self.main_l.readers[i].heads();
                let hr = self.main_r.readers[i].heads();
                for (mid, now, to) in [
                    (&mut self.tap_gain_mid[i], &mut self.tap_gain_now[i], tap_gain[i]),
                    (&mut self.tap_pan_mid[i].0, &mut self.tap_pan_now[i].0, tap_pan_l[i]),
                    (&mut self.tap_pan_mid[i].1, &mut self.tap_pan_now[i].1, tap_pan_r[i]),
                ] {
                    *mid += (to - *mid) * tap_glide;
                    *now += (*mid - *now) * tap_glide;
                    // close enough: land, rather than creep in denormals
                    if (to - *now).abs() < 1e-9 && (to - *mid).abs() < 1e-9 {
                        (*mid, *now) = (to, to);
                    }
                }
                let ping_g = if ping_read && ping_tap_fresh[i] {
                    let g = &mut self.ping_gain[i];
                    for ((g, m), t) in g.iter_mut().zip(&mut self.ping_gain_mid[i]).zip(ping_target[i]) {
                        *m += (t - *m) * ping_glide;
                        *g += (*m - *g) * ping_glide;
                    }
                    Some(*g)
                } else {
                    None
                };
                let lines = TapLines {
                    main: [&self.main_l.delay, &self.main_r.delay],
                    ping: [&self.ping_odd, &self.ping_even],
                    pan: self.tap_pan_now[i],
                    ping_g,
                };
                let linked = hr == hl;
                // Detune: the tap read through its shifter's grains,
                // crossfading with the plain read as it starts and stops.
                if tap_det[i] != 0.0 || self.det_w[i] == 0.0 {
                    self.det_zero[i] = 0;
                } else {
                    self.det_zero[i] += 1;
                }
                let det_to = if tap_det[i] != 0.0 || (self.det_w[i] > 0.0 && self.det_zero[i] < det_hold) { 1.0 } else { 0.0 };
                let shifted = if det_to > 0.0 || self.det_w[i] > 0.0 {
                    let starting = self.det_w[i] == 0.0;
                    if starting {
                        self.det_ratio[i] = detune::ratio(tap_det[i]);
                        (self.light_live[i], self.clean_live[i]) = (false, false);
                    }
                    self.det_ratio[i] += (detune::ratio(tap_det[i]) - self.det_ratio[i]) * tap_glide;
                    let r = self.det_ratio[i];
                    let (nl, nr) = (hl.centre(), hr.centre());
                    // Clean while the tap is long enough for it, else Light
                    let clean_to = if clean_on && nl.min(nr) >= self.pv[i][0].min_delay(r) { 1.0 } else { 0.0 };
                    if starting {
                        self.clean_w[i] = clean_to;
                    }
                    let cw = self.clean_w[i];
                    // taps start their grain cycles and frame grids apart
                    let phase = |ch: usize| (i as f32 + 0.5 * ch as f32) / NUM_TAPS as f32;
                    let light = if cw < 1.0 {
                        if !self.light_live[i] {
                            self.det[i][0].restart(phase(0));
                            self.det[i][1].restart(phase(1));
                            self.light_live[i] = true;
                        }
                        let (ml, mr) = (&self.main_l.delay, &self.main_r.delay);
                        // the splices match on the left line (Mid in M/S)
                        let gl = self.det[i][0].step(nl, r, &|at, out: &mut [f32]| ml.copy_back(at, out));
                        let gr = if linked { gl } else { self.det[i][1].step(nr, r, &|at, out: &mut [f32]| mr.copy_back(at, out)) };
                        Some(lines.read(&gl, &gr, linked))
                    } else {
                        self.light_live[i] = false;
                        None
                    };
                    let clean = if cw > 0.0 {
                        if !self.clean_live[i] {
                            self.pv[i][0].restart(phase(0));
                            self.pv[i][1].restart(phase(1));
                            self.clean_live[i] = true;
                        }
                        let yl = self.pv[i][0].step(nl, r, &|back, out: &mut [f32], aux: &mut [f32]| lines.fill(0, back, out, aux));
                        let yr = self.pv[i][1].step(nr, r, &|back, out: &mut [f32], aux: &mut [f32]| lines.fill(1, back, out, aux));
                        Some((yl, yr))
                    } else {
                        self.clean_live[i] = false;
                        None
                    };
                    self.clean_w[i] = (cw + (clean_to - cw).clamp(-xf_step, xf_step)).clamp(0.0, 1.0);
                    let out = match (light, clean) {
                        (Some(a), Some(b)) => {
                            let w = s_curve(cw);
                            (a.0 + (b.0 - a.0) * w, a.1 + (b.1 - a.1) * w)
                        }
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => (0.0, 0.0),
                    };
                    self.det_w[i] = (self.det_w[i] + (det_to - self.det_w[i]).clamp(-xf_step, xf_step)).clamp(0.0, 1.0);
                    Some((out, s_curve(self.det_w[i])))
                } else {
                    None
                };
                // Panned ahead of the filter (the same sound: the filter is
                // linear), so Ping's repeats can join each side before it.
                let (mut sl, mut sr_s) = match shifted {
                    Some((d, 1.0)) => d,
                    Some((d, w)) => {
                        let (pl, pr) = lines.read(&hl, &hr, linked);
                        (pl + (d.0 - pl) * w, pr + (d.1 - pr) * w)
                    }
                    None => lines.read(&hl, &hr, linked),
                };

                let moved = self.tap_cut[i].step(tap_cut[i], tap_glide) | self.tap_q[i].step(tap_q[i], tap_glide);
                if moved {
                    self.set_tap_coefs(i, sr);
                }
                let (ty, lay) = (self.tap_type_now[i], self.tap_layout_now[i]);
                let (dry_l, dry_r) = (sl, sr_s);
                sl = self.tap_chain_l[i].tick(sl, ty, lay);
                sr_s = self.tap_chain_r[i].tick(sr_s, ty, lay);
                // a new filter type or Slope: the old one (a copy, still running) fades out
                if self.tap_type_xf[i] < 1.0 {
                    let (old, old_lay) = (self.tap_type_old[i], self.tap_layout_old[i]);
                    let ol = self.tap_chain_old_l[i].tick(dry_l, old, old_lay);
                    let or = self.tap_chain_old_r[i].tick(dry_r, old, old_lay);
                    let w = s_curve(self.tap_type_xf[i]);
                    sl = ol + (sl - ol) * w;
                    sr_s = or + (sr_s - or) * w;
                    self.tap_type_xf[i] = (self.tap_type_xf[i] + type_step).min(1.0);
                }

                tap_out_l += sl * self.tap_gain_now[i];
                tap_out_r += sr_s * self.tap_gain_now[i];
            }

            // Crossfeed Mix — post-tap stereo blend on the OUTPUT (not
            // feedback), faded in and out with the mode.
            if cf > 1e-4 && self.xf_mix > 0.0 {
                let sent = cf * 0.5 * s_curve(self.xf_mix);
                let kept = 1.0 - sent;
                let new_l = tap_out_l * kept + tap_out_r * sent;
                let new_r = tap_out_r * kept + tap_out_l * sent;
                tap_out_l = new_l;
                tap_out_r = new_r;
            }

            // DIFFUSE routing = Output: smear the tap-mix on its way to dry/wet.
            // This is the cleanest option — diffusion doesn't interact with
            // feedback / cluster dynamics, just produces a reverb-tail bloom.
            let out_diff = self.wet_diff_on && diffuse_routing == DiffuseRouting::Output;
            let mut wet_l = if out_diff { self.diff_wet_l.blend(tap_out_l, diffuse_amount) } else { tap_out_l };
            let mut wet_r = if out_diff { self.diff_wet_r.blend(tap_out_r, diffuse_amount) } else { tap_out_r };

            // DEGRADE routing = Output: crush the whole wet mix once (after
            // diffusion so the bloom degrades too), before dry/wet.
            if degrade_on && degrade_routing == DiffuseRouting::Output {
                self.tape.process(&mut wet_l, &mut wet_r);
            }

            // SATURATE routing = Output: drive the wet mix once, before dry/wet.
            if sat_on && sat_routing == DiffuseRouting::Output {
                if colour.sat_crate {
                    self.sat_out.process_stereo(&mut wet_l, &mut wet_r);
                } else {
                    wet_l = sat(wet_l);
                    wet_r = sat(wet_r);
                }
            }

            // Ducking: only the echoes dip, after the output colour and before
            // Mix; the loop keeps going round untouched. The dip follows how
            // loud the input is (Tide's envelope curve): about 70 % of the
            // depth at −12 dBFS, next to nothing for quiet notes.
            let dip_to = self.duck_depth * (1.0 - (-5.0 * self.duck_env).exp());
            self.duck_dip += (dip_to - self.duck_dip) * duck_smooth;
            if self.duck_depth == 0.0 && self.duck_dip < 1e-4 {
                // off: exactly nothing from here, not a tail of tiny dips
                self.duck_dip = 0.0;
            }
            if self.duck_dip > 0.0 {
                duck_most = duck_most.max(self.duck_dip);
                let g = nice_plug::util::db_to_gain(-self.duck_dip);
                wet_l *= g;
                wet_r *= g;
            }

            // Ear guard: only the echoes, after the loop.
            let to = if guard_on {
                let pw = (wet_l * wet_l + wet_r * wet_r) * 0.5;
                self.guard_pow += (pw - self.guard_pow) * guard_avg;
                let rms = self.guard_pow.sqrt();
                if rms > GUARD_CEIL { GUARD_CEIL / rms } else { 1.0 }
            } else {
                1.0
            };
            self.guard_g += (to - self.guard_g) * if to < self.guard_g { guard_att } else { guard_rel };
            if to >= 1.0 && self.guard_g > 0.99999 {
                self.guard_g = 1.0;
            }
            if self.guard_g < 1.0 {
                guard_least = guard_least.min(self.guard_g);
                wet_l *= self.guard_g;
                wet_r *= self.guard_g;
            }
            // Clear: fade the echoes out, then empty the delay (after the block)
            if let Some(left) = self.clear_left {
                let g = left as f32 / clear_len as f32;
                wet_l *= g;
                wet_r *= g;
                if left == 0 {
                    cleared = true;
                } else {
                    self.clear_left = Some(left - 1);
                }
            }

            let mut out_l = in_l * (1.0 - dry_wet) + wet_l * dry_wet;
            let mut out_r = in_r * (1.0 - dry_wet) + wet_r * dry_wet;

            match ch_mode {
                ChannelMode::MidSide => {
                    let l_dec = out_l + out_r;
                    let r_dec = out_l - out_r;
                    out_l = l_dec;
                    out_r = r_dec;
                }
                ChannelMode::Mono => {
                    let m = (out_l + out_r) * 0.5;
                    out_l = m;
                    out_r = m;
                }
                ChannelMode::Stereo => {}
            }

            out_l = self.lim_l.process(out_l * output_gain);
            out_r = self.lim_r.process(out_r * output_gain);

            fault |= !(out_l.is_finite() && out_r.is_finite());
            l_chan[s] = out_l;
            r_chan[s] = out_r;

            self.xf_mix += (mix_target - self.xf_mix).clamp(-xf_step, xf_step);
            if diffuse_target > 0.0 {
                self.x_feed = (self.x_feed + xf_step).min(1.0);
                self.x_warm = self.x_warm.saturating_add(1);
                if self.x_warm >= x_warm_len {
                    self.xf_diffuse = (self.xf_diffuse + xf_step).min(1.0);
                }
            } else {
                self.xf_diffuse = (self.xf_diffuse - xf_step).max(0.0);
                if self.xf_diffuse == 0.0 {
                    (self.x_feed, self.x_warm) = (0.0, 0);
                }
            }
            self.freeze_w += (freeze_target - self.freeze_w).clamp(-xf_step, xf_step);
            self.rep_w += (rep_target - self.rep_w).clamp(-xf_step, xf_step);
            self.duck_depth += (duck_target - self.duck_depth).clamp(-duck_step, duck_step);
            self.ping_share += (if self.ping_on { 1.0 } else { 0.0 } - self.ping_share).clamp(-xf_step, xf_step);
            self.ping_fresh = self.ping_fresh.saturating_add(1);
            self.ping_valid = self.ping_valid.saturating_add(1);
            self.ping_age = self.ping_age.saturating_add(1);
            if self.ping_on {
                match self.ping_leave {
                    // hand the loop back: the main loop takes the bounce's
                    // last pass on (older repeats have gone round already)
                    Some(left) if left <= 1 => {
                        self.ping_on = false;
                        self.ping_leave = None;
                        self.ping_since_off = 0;
                        self.ping_valid = fb_samps as usize;
                        self.ping_live = ping_play_out + (0.02 * sr) as usize + ring_len;
                    }
                    Some(left) => self.ping_leave = Some(left - 1),
                    None => {}
                }
            } else {
                self.ping_since_off = self.ping_since_off.saturating_add(1);
                if self.ping_live > 0 {
                    self.ping_live -= 1;
                    if self.ping_live == 0 {
                        // played out: what's older is stale from here on
                        self.ping_fresh = self.ping_since_off;
                    }
                }
            }
        }

        // Something broke (a NaN or an infinity got in): silence this block
        // and start clean rather than stick loud. Clear empties the same way.
        if fault {
            for ch in buffer.as_slice() {
                ch.fill(0.0);
            }
        }
        if fault || cleared {
            self.reset();
            self.clear_left = None;
        }
        self.params.guard_db.store((-20.0 * guard_least.max(1e-6).log10()).to_bits(), Ordering::Relaxed);
        // the Duck knob shows the most the echoes dipped in this block
        self.params.duck_dip_db.store(duck_most.to_bits(), Ordering::Relaxed);
        // and the editor Motion's values, playheads and each tap's movement
        self.modu.publish(&p.motion.shared);
        // the demo's silences, last, on the output (nothing while licensed)
        self.gate.process(buffer.as_slice());

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Afterecho {
    const CLAP_ID: &'static str = "com.softsystems.afterecho";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Afterecho — delay garden");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Delay];
}

impl Vst3Plugin for Afterecho {
    const VST3_CLASS_ID: [u8; 16] = *b"SoftSystemsAftr1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay, Vst3SubCategory::Stereo];
}

// AUv3 export is iOS-only (only the iOS target enables nice-plug's `auv3`
// feature — see Cargo.toml's target-cfg deps). Gating on target_os keeps the
// desktop clap/vst3/standalone build, which has no `auv3`, compiling.
#[cfg(target_os = "ios")]
impl AuPlugin for Afterecho {
    // Hosts key saved sessions on (type, subtype, manufacturer). Pick once,
    // never change. AU_TYPE auto-derives to `aufx` (audio effect).
    // MUST match bundler.toml's au_subtype / au_manufacturer.
    const AU_SUBTYPE: [u8; 4] = *b"Aftr"; // Afterecho
    const AU_MANUFACTURER: [u8; 4] = *b"SfSy"; // every Soft Systems plug-in (not all-lowercase: Apple reserves those)
}

nice_export_clap!(Afterecho);
nice_export_vst3!(Afterecho);
#[cfg(target_os = "ios")]
nice_export_auv3!(Afterecho);

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;
    use softsystems_mod::{Modulator, Rate};

    const SR: f32 = 48_000.0;

    /// A host that only tells the time.
    struct Host(Transport);

    impl ProcessContext<Afterecho> for Host {
        fn plugin_api(&self) -> PluginApi { PluginApi::Clap }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport { &self.0 }
        fn next_event(&mut self) -> Option<PluginNoteEvent<Afterecho>> { None }
        fn send_event(&mut self, _event: PluginNoteEvent<Afterecho>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn set(plug: &Afterecho, id: &str, plain: f32) {
        let (_, ptr, _) = plug.params.param_map().into_iter().find(|(i, _, _)| i == id)
            .unwrap_or_else(|| panic!("no param {id}"));
        // SAFETY: ptr belongs to plug's params, which outlive the call.
        unsafe { ptr._internal_set_normalized_value(ptr.preview_normalized(plain)) };
    }

    /// Motion: a route from `slot` to `target` at `amount` (a share of the knob's travel),
    /// through the patch, as the editor makes one.
    fn route(plug: &Afterecho, slot: u16, target: &str, amount: f32) {
        let m = &plug.params.motion;
        m.patch.lock().unwrap().routes.set(slot, target, amount, crate::modulation::ROUTE_CAP);
        m.shared.changed();
    }

    /// Motion: slot `slot` at `rate` (its unit in the patch, its value the host's param, as
    /// `ModParams` keeps it), with `edit` applied to the rest.
    fn slot(plug: &Afterecho, n: usize, rate: softsystems_mod::Rate, edit: impl FnOnce(&mut softsystems_mod::Modulator)) {
        let m = &plug.params.motion;
        {
            let mut g = m.patch.lock().unwrap();
            let md = &mut g.modulators[n];
            if let Some(c) = md.clock_mut() {
                c.rate = rate;
                c.trigger = if rate.unit.is_tempo() { softsystems_mod::Trigger::Sync } else { softsystems_mod::Trigger::Free };
            }
            edit(md);
        }
        set(plug, &format!("mod{}_rate", n + 1), rate.position());
        m.shared.changed();
    }

    /// Wet only, one tap per pan, 20 ms apart; the cycle follows the cluster.
    fn plugin(pans: &[f32]) -> Afterecho {
        let mut plug = Afterecho::default();
        // a beta's demo is silent from the first block on a machine without a beta
        // licence: the tests run the gate that never silences (the crate's `dev`)
        plug.gate = softsystems_licence::DemoGate::off();
        plug.prepare(SR);
        plug.reset();
        set(&plug, "dry_wet", 1.0);
        // the loop (these tests were written for it); counts have their own
        set(&plug, "repeats", REPEATS_LOOP as f32);
        set(&plug, "num_taps_active", pans.len() as f32);
        set(&plug, "base_time_l", 20f32.ln() / 2000f32.ln()); // Free: 2000^v ms
        for (i, &pan) in pans.iter().enumerate() {
            set(&plug, &format!("pan_{}", i + 1), pan);
        }
        plug
    }

    const PING: f32 = 1.0;

    /// Run `input` through in 256-sample blocks; `at` runs before each block
    /// with its first sample's index.
    fn render(plug: &mut Afterecho, input: &[(f32, f32)], mut at: impl FnMut(usize, &Afterecho)) -> Vec<(f32, f32)> {
        let mut host = Host(Transport::new(SR));
        let mut out = Vec::with_capacity(input.len());
        for (b, chunk) in input.chunks(256).enumerate() {
            at(b * 256, plug);
            let mut l: Vec<f32> = chunk.iter().map(|s| s.0).collect();
            let mut r: Vec<f32> = chunk.iter().map(|s| s.1).collect();
            {
                let mut buffer = Buffer::default();
                // SAFETY: l and r outlive the buffer, dropped before they're read.
                unsafe { buffer.set_slices(chunk.len(), |s| *s = vec![&mut l[..], &mut r[..]]) };
                plug.process(&mut buffer, &mut AuxiliaryBuffers { inputs: &mut [], outputs: &mut [] }, &mut host);
            }
            out.extend(l.into_iter().zip(r));
        }
        out
    }

    fn click(secs: f32) -> Vec<(f32, f32)> {
        let mut v = vec![(0.0, 0.0); (secs * SR) as usize];
        v[0] = (0.25, 0.25);
        v
    }

    /// Energy on each side around `ms` (−1 … +3 ms).
    fn energy(out: &[(f32, f32)], ms: f32) -> (f32, f32) {
        let c = (ms * 0.001 * SR) as usize;
        out[c - 48..c + 144].iter().fold((0.0, 0.0), |(l, r), s| (l + s.0 * s.0, r + s.1 * s.1))
    }

    #[test]
    fn ping_bounces_a_mono_click() {
        let mut plug = plugin(&[0.0]);
        set(&plug, "feedback", 0.7);
        set(&plug, "crossfeed_mode", PING);
        set(&plug, "crossfeed", 1.0);
        let out = render(&mut plug, &click(0.2), |_, _| {});
        // one tap at 20 ms and a 20 ms cycle: the taps' pass, then the repeats
        let (l0, r0) = energy(&out, 20.0);
        assert!(l0 > 0.0 && (l0 - r0).abs() < 1e-3 * l0, "the taps' pass stays centred: L {l0} R {r0}");
        for k in 1..=4 {
            let (l, r) = energy(&out, 20.0 + 20.0 * k as f32);
            let (near, far) = if k % 2 == 1 { (l, r) } else { (r, l) };
            assert!(near > 0.05 * l0 && far < 1e-3 * near, "repeat {k}: L {l} R {r}");
        }
    }

    #[test]
    fn panned_taps_keep_playing_as_the_pattern_swings() {
        // hard left at 20 ms, hard right at 40 ms; the cycle is 40 ms
        let mut plug = plugin(&[-1.0, 1.0]);
        set(&plug, "feedback", 0.7);
        set(&plug, "crossfeed_mode", PING);
        set(&plug, "crossfeed", 1.0);
        let out = render(&mut plug, &click(0.25), |_, _| {});
        let (a0, _) = energy(&out, 20.0);
        let (_, b0) = energy(&out, 40.0);
        assert!(a0 > 0.0 && b0 > 0.0);
        // repeat 1 puts both taps on the left, repeat 2 both on the right
        for (ms, odd) in [(60.0, true), (80.0, true), (100.0, false), (120.0, false)] {
            let (l, r) = energy(&out, ms);
            let (near, far) = if odd { (l, r) } else { (r, l) };
            assert!(near > 0.1 * a0 && far < 1e-3 * near, "{ms} ms: L {l} R {r}");
        }
    }

    #[test]
    fn ping_at_zero_width_sounds_like_plain_feedback() {
        // with the loop tone off, dark and thin: the bounce gets the same tone
        for tone in [0.0, -0.5, 0.5] {
            let run = |mode: f32| {
                let mut plug = plugin(&[-0.6, 0.3, 1.0]);
                set(&plug, "feedback", 0.8);
                set(&plug, "crossfeed_mode", mode);
                set(&plug, "crossfeed", 0.0);
                set(&plug, "tap_filter_cutoff_2", 2000.0);
                set(&plug, "loop_tone", tone);
                render(&mut plug, &click(0.4), |_, _| {})
            };
            let (mix, ping) = (run(0.0), run(PING));
            let worst = mix.iter().zip(&ping).fold(0.0f32, |m, (a, b)| m.max((a.0 - b.0).abs()).max((a.1 - b.1).abs()));
            let peak = mix.iter().fold(0.0f32, |m, s| m.max(s.0.abs()).max(s.1.abs()));
            assert!(peak > 0.01 && worst < 1e-5 * peak / 0.01, "tone {tone}: worst {worst:e}, peak {peak}");
        }
    }

    #[test]
    fn ping_in_mid_side_still_bounces_left_and_right() {
        let mut plug = plugin(&[0.0]);
        set(&plug, "channel_mode", 1.0); // Mid/Side
        set(&plug, "feedback", 0.7);
        set(&plug, "crossfeed_mode", PING);
        set(&plug, "crossfeed", 1.0);
        let out = render(&mut plug, &click(0.2), |_, _| {});
        let (l0, r0) = energy(&out, 20.0);
        assert!(l0 > 0.0 && (l0 - r0).abs() < 1e-3 * l0, "L {l0} R {r0}");
        for k in 1..=3 {
            let (l, r) = energy(&out, 20.0 + 20.0 * k as f32);
            let (near, far) = if k % 2 == 1 { (l, r) } else { (r, l) };
            assert!(near > 0.05 * l0 && far < 1e-3 * near, "repeat {k}: L {l} R {r}");
        }
    }

    /// 50 ms of white noise, then silence, `secs` long.
    fn noise_burst(secs: f32) -> Vec<(f32, f32)> {
        let mut seed = 1u32;
        (0..(SR * secs) as usize)
            .map(|n| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = if n < 2400 { (seed >> 8) as f32 / 16_777_216.0 - 0.5 } else { 0.0 };
                (v, v)
            })
            .collect()
    }

    #[test]
    fn the_bounce_self_oscillates_and_stays_bounded() {
        let mut plug = plugin(&[0.0, 0.4]);
        set(&plug, "feedback", 1.05);
        set(&plug, "crossfeed_mode", PING);
        set(&plug, "crossfeed", 1.0);
        let out = render(&mut plug, &noise_burst(3.0), |_, _| {});
        // still going after 3 s (no gain compensation), and the loop itself
        // never passes the soft clip's ceiling
        let last = &out[out.len() - (SR / 2.0) as usize..];
        let rms = (last.iter().map(|s| s.0 * s.0 + s.1 * s.1).sum::<f32>() / last.len() as f32).sqrt();
        let lines = plug.ping_odd.buffer_peak().max(plug.ping_even.buffer_peak());
        assert!(rms > 0.01 && lines <= 2.0, "rms {rms}, loop peak {lines}");
    }

    /// A held low tone (110 Hz, a little spread), five seconds.
    fn held_tone() -> Vec<(f32, f32)> {
        (0..(SR * 5.0) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let v = 0.2 * (t / 0.05).min(1.0) * (2.0 * PI * 110.0 * t).sin();
                (v, 0.6 * v)
            })
            .collect()
    }

    /// The click detector: the sharpest bend in the output after its first
    /// 0.3 s (the second difference) and where it is. A step shows up
    /// hundreds of times above a low tone's own.
    fn sharpest_bend(out: &[(f32, f32)]) -> (f32, usize) {
        let d2 = |a: f32, b: f32, c: f32| (a - 2.0 * b + c).abs();
        out[(0.3 * SR) as usize..].windows(3).enumerate().fold((0.0f32, 0), |(m, at), (i, w)| {
            let v = d2(w[0].0, w[1].0, w[2].0).max(d2(w[0].1, w[1].1, w[2].1));
            if v > m { (v, i) } else { (m, at) }
        })
    }

    #[test]
    fn switching_modes_mid_tail_does_not_click() {
        // The held tone through three spread taps, X-feed at 100 %:
        // Mix → Ping → Diffuse → Ping → Mix, a second each, against the same
        // tone held in one mode.
        let input = held_tone();
        let n = input.len();
        // plain, then Diffuse, Tape and the loop tone in the loop (all have memory)
        for colour in [
            &[][..],
            &[("diffuse_routing", 1.0), ("diffuse_amount", 0.5)],
            &[("degrade_routing", 1.0), ("degrade_amount", 0.6)],
            &[("loop_tone", -0.6)],
            &[("loop_tone", 0.6)],
        ] {
            let sharpest = |schedule: [f32; 5]| {
                let mut plug = plugin(&[-0.5, 0.7, 0.0]);
                set(&plug, "feedback", 0.6);
                set(&plug, "crossfeed", 1.0);
                for &(id, v) in colour {
                    set(&plug, id, v);
                }
                sharpest_bend(&render(&mut plug, &input, |s, plug| set(plug, "crossfeed_mode", schedule[s * 5 / n])))
            };
            let held = [[0.0; 5], [PING; 5], [2.0; 5]].map(|s| sharpest(s).0).into_iter().fold(0.0, f32::max);
            let (switched, at) = sharpest([0.0, PING, 2.0, PING, 0.0]);
            assert!(
                switched < 1.5 * held,
                "{colour:?}: {:.1}× the held tone's sharpest bend, at {:.3} s",
                switched / held,
                at as f32 / SR + 0.3
            );
        }
    }

    /// How the repeat around `ms` leans (−1 … +15 ms, left side): the energy
    /// of its first difference (its top) over that of its 16-sample average
    /// (its bottom). Lower is darker.
    fn lean(out: &[(f32, f32)], ms: f32) -> f32 {
        let c = (ms * 0.001 * SR) as usize;
        let w: Vec<f32> = out[c - 48..c + 720].iter().map(|s| s.0).collect();
        let top: f32 = w.windows(2).map(|p| (p[1] - p[0]).powi(2)).sum();
        let bottom: f32 = w.windows(16).map(|p| (p.iter().sum::<f32>() / 16.0).powi(2)).sum();
        top / bottom
    }

    #[test]
    fn the_loop_tone_darkens_or_thins_every_repeat() {
        // One tap at 20 ms and a 20 ms cycle: the taps' pass, then repeats
        // that have been round the loop once, twice… Off, every repeat leans
        // like the first pass; dark, each leans darker than the one before;
        // thin, each brighter.
        let run = |tone: f32| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.8);
            set(&plug, "loop_tone", tone);
            let out = render(&mut plug, &click(0.2), |_, _| {});
            (0..5).map(|k| lean(&out, 20.0 + 20.0 * k as f32)).collect::<Vec<_>>()
        };
        // (each pass adds one pole, so each step is smaller than the last)
        let (off, dark, thin) = (run(0.0), run(-0.6), run(0.6));
        for k in 1..5 {
            assert!((off[k] / off[0] - 1.0).abs() < 0.05, "off: {off:?}");
            assert!(dark[k] < 0.8 * dark[k - 1], "dark: {dark:?}");
            assert!(thin[k] > 1.05 * thin[k - 1], "thin: {thin:?}");
        }
    }

    #[test]
    fn the_loop_tone_leaves_the_taps_own_pass_alone() {
        // No feedback, so nothing goes round: every tone sounds the same.
        let run = |tone: f32| {
            let mut plug = plugin(&[-0.3, 0.5]);
            set(&plug, "feedback", 0.0);
            set(&plug, "loop_tone", tone);
            render(&mut plug, &noise_burst(0.3), |_, _| {})
        };
        let off = run(0.0);
        assert!(off.iter().any(|s| s.0.abs() > 0.01));
        for tone in [-1.0, 1.0] {
            assert!(run(tone) == off, "tone {tone} changed the taps' own pass");
        }
    }

    #[test]
    fn the_loop_tone_still_lets_the_loop_self_oscillate() {
        // Feedback past 1 with the tone at either end, in Mix and in Ping: the
        // passband still gains, so the loop keeps going (no gain
        // compensation) and stays under the soft clip's ceiling.
        for mode in [0.0, PING] {
            for tone in [-1.0, 1.0] {
                let mut plug = plugin(&[0.0, 0.4]);
                set(&plug, "feedback", 1.05);
                set(&plug, "crossfeed_mode", mode);
                set(&plug, "crossfeed", 1.0);
                set(&plug, "loop_tone", tone);
                let out = render(&mut plug, &noise_burst(3.0), |_, _| {});
                let half = (SR / 2.0) as usize;
                let last = &out[out.len() - half..];
                let rms = (last.iter().map(|s| s.0 * s.0 + s.1 * s.1).sum::<f32>() / half as f32).sqrt();
                // what went round over the last half second
                let lines = if mode == PING {
                    plug.ping_odd.buffer_peak().max(plug.ping_even.buffer_peak())
                } else {
                    (1..half).fold(0.0f32, |m, d| {
                        m.max(plug.main_l.delay.read(d as f32).abs()).max(plug.main_r.delay.read(d as f32).abs())
                    })
                };
                assert!(rms > 0.01 && lines <= 2.0, "mode {mode}, tone {tone}: rms {rms}, loop peak {lines}");
            }
        }
    }

    #[test]
    fn dub_drops_each_return_to_its_band() {
        // beta pass 12: the taps' pass is left alone and every return leans darker than the
        // plain loop's (the band at 150 Hz–2.5 kHz, the spike's medium)
        let run = |intensity: f32| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.8);
            set(&plug, "dub_intensity", intensity);
            let out = render(&mut plug, &click(0.2), |_, _| {});
            (0..4).map(|k| lean(&out, 20.0 + 20.0 * k as f32)).collect::<Vec<_>>()
        };
        let (off, dub) = (run(0.0), run(0.6));
        assert!((dub[0] / off[0] - 1.0).abs() < 0.05, "the taps' pass: off {off:?}, dub {dub:?}");
        for k in 1..4 {
            assert!(dub[k] < 0.5 * off[k], "return {k}: off {off:?}, dub {dub:?}");
        }
    }

    #[test]
    fn dub_at_zero_changes_nothing() {
        let run = |dub: bool| {
            let mut plug = plugin(&[-0.3, 0.5]);
            set(&plug, "feedback", 0.7);
            if dub {
                // its band and swirl set, but Intensity at 0
                set(&plug, "dub_low", 800.0);
                set(&plug, "dub_swirls", 3.0);
            }
            render(&mut plug, &noise_burst(0.5), |_, _| {})
        };
        assert!(run(true) == run(false));
    }

    #[test]
    fn dub_heavy_rings_and_stays_bounded() {
        // the owner's rule: no ÷Q, so Intensity's low-pass peak adds loop gain and a hot loop
        // self-oscillates, held by the loop's soft clip; in Mix and in Ping
        for mode in [0.0, PING] {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 0.95);
            set(&plug, "crossfeed_mode", mode);
            set(&plug, "crossfeed", 1.0);
            set(&plug, "dub_intensity", 1.0);
            set(&plug, "dub_low", 200.0);
            set(&plug, "dub_high", 1500.0);
            set(&plug, "dub_swirls", 3.0);
            let out = render(&mut plug, &noise_burst(3.0), |_, _| {});
            let half = (SR / 2.0) as usize;
            let last = &out[out.len() - half..];
            assert!(last.iter().all(|s| s.0.is_finite() && s.1.is_finite()));
            let rms = (last.iter().map(|s| s.0 * s.0 + s.1 * s.1).sum::<f32>() / half as f32).sqrt();
            let lines = if mode == PING {
                plug.ping_odd.buffer_peak().max(plug.ping_even.buffer_peak())
            } else {
                (1..half).fold(0.0f32, |m, d| m.max(plug.main_l.delay.read(d as f32).abs()).max(plug.main_r.delay.read(d as f32).abs()))
            };
            println!("Dub heavy at 95 %, mode {mode}: rms {rms:.3}, loop peak {lines:.3}");
            assert!(rms > 0.005 && lines <= 2.0, "mode {mode}: rms {rms}, loop peak {lines}");
        }
    }

    #[test]
    fn turning_the_tone_does_not_click() {
        // The held tone in the loop, the knob jumping 0 → −1 → +1 → −1 → 0 a
        // second each (as stepped automation can), against it held at each.
        let input = held_tone();
        let n = input.len();
        let sharpest = |schedule: [f32; 5]| {
            let mut plug = plugin(&[-0.5, 0.7, 0.0]);
            set(&plug, "feedback", 0.6);
            sharpest_bend(&render(&mut plug, &input, |s, plug| set(plug, "loop_tone", schedule[s * 5 / n])))
        };
        let held = [[0.0; 5], [-1.0; 5], [1.0; 5]].map(|s| sharpest(s).0).into_iter().fold(0.0, f32::max);
        let (switched, at) = sharpest([0.0, -1.0, 1.0, -1.0, 0.0]);
        assert!(
            switched < 1.5 * held,
            "{:.1}× the held tone's sharpest bend, at {:.3} s",
            switched / held,
            at as f32 / SR + 0.3
        );
    }

    /// Freeze from `secs` on, switched at a block's start as a host would.
    fn freeze_from(secs: f32) -> impl FnMut(usize, &Afterecho) {
        move |s, plug| set(plug, "freeze", if s as f32 >= secs * SR { 1.0 } else { 0.0 })
    }

    fn rms(out: &[(f32, f32)], from: f32, to: f32) -> f32 {
        let w = &out[(from * SR) as usize..(to * SR) as usize];
        (w.iter().map(|s| s.0 * s.0 + s.1 * s.1).sum::<f32>() / w.len() as f32).sqrt()
    }

    #[test]
    fn freeze_holds_the_loop_sample_for_sample() {
        // A noise burst going round two taps (a 40 ms cycle), frozen at 0.1 s,
        // plain and with every colour, Bloom and Tone in the loop. Once held,
        // the main lines repeat exactly every latched cycle: nothing decays,
        // darkens or smears, and what you hear holds its level.
        for colour in [
            &[][..],
            &[
                ("loop_tone", -1.0),
                ("diffuse_routing", 1.0),
                ("diffuse_amount", 0.5),
                ("degrade_routing", 1.0),
                ("degrade_amount", 0.6),
                ("sat_routing", 1.0),
                ("sat_amount", 0.7),
                ("fb_curve", 1.0),
            ],
        ] {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 0.5);
            for &(id, v) in colour {
                set(&plug, id, v);
            }
            let out = render(&mut plug, &noise_burst(1.5), freeze_from(0.1));
            let c = plug.freeze_len;
            assert!((c - 0.04 * SR).abs() <= 1.0, "latched {c} samples");
            // A cycle of c samples comes round every c samples, held or not
            // (v2.44; before, the loop read a sample late: every c + 1).
            for d in 1..(3.0 * c) as usize {
                let (a, b) = (d as f32, d as f32 + c);
                assert_eq!(plug.main_l.delay.read(a), plug.main_l.delay.read(b), "{colour:?}: L at {d}");
                assert_eq!(plug.main_r.delay.read(a), plug.main_r.delay.read(b), "{colour:?}: R at {d}");
            }
            let (early, late) = (rms(&out, 0.5, 0.9), rms(&out, 1.1, 1.5));
            assert!(early > 0.01 && (late / early - 1.0).abs() < 1e-3, "{colour:?}: rms {early} then {late}");
        }
    }

    #[test]
    fn freeze_keeps_new_input_out() {
        // Held from 0.1 s: a loud click at 0.6 s changes nothing (wet only).
        let run = |late: bool| {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 0.5);
            let mut input = noise_burst(1.0);
            if late {
                input[(0.6 * SR) as usize] = (0.9, -0.9);
            }
            render(&mut plug, &input, freeze_from(0.1))
        };
        let (quiet, clicked) = (run(false), run(true));
        assert!(quiet.iter().any(|s| s.0.abs() > 0.01));
        assert!(quiet == clicked, "the click got into the held loop");
    }

    #[test]
    fn letting_go_lets_the_loop_decay_at_feedback() {
        // One tap at 20 ms and a 20 ms cycle: a click going round, held from
        // 50 ms to 0.5 s, then repeating at Feedback (half the level a cycle).
        let mut plug = plugin(&[0.0]);
        set(&plug, "feedback", 0.5);
        let out = render(&mut plug, &click(0.7), |s, plug| {
            let t = s as f32 / SR;
            set(plug, "freeze", if (0.05..0.5).contains(&t) { 1.0 } else { 0.0 })
        });
        let e = |ms: f32| {
            let (l, r) = energy(&out, ms);
            l + r
        };
        assert!(e(300.0) > 0.0 && (e(400.0) / e(300.0) - 1.0).abs() < 1e-3, "held: {} then {}", e(300.0), e(400.0));
        for ms in [580.0, 600.0, 620.0] {
            let ratio = e(ms) / e(ms - 20.0);
            assert!((ratio - 0.25).abs() < 0.03, "{ms} ms: {ratio} of the energy before");
        }
    }

    #[test]
    fn a_frozen_bounce_keeps_bouncing() {
        // Ping, one tap and a 20 ms cycle, frozen after two repeats: long after,
        // neighbouring repeats still sit on opposite sides, each side as loud
        // as two cycles before.
        let mut plug = plugin(&[0.0]);
        set(&plug, "feedback", 0.7);
        set(&plug, "crossfeed_mode", PING);
        set(&plug, "crossfeed", 1.0);
        let out = render(&mut plug, &click(1.0), freeze_from(0.05));
        for ms in [800.0, 820.0, 840.0] {
            let ((l, r), (l2, r2)) = (energy(&out, ms), energy(&out, ms + 40.0));
            let near = l.max(r);
            assert!(near > 0.0 && l.min(r) < 1e-3 * near, "{ms} ms: L {l} R {r}");
            assert!((l2 - l).abs() <= 1e-3 * near && (r2 - r).abs() <= 1e-3 * near, "{ms} ms: L {l} R {r}, then L {l2} R {r2}");
        }
        let (a, b) = (energy(&out, 800.0), energy(&out, 820.0));
        assert!((a.0 > a.1) != (b.0 > b.1), "neighbouring repeats on the same side");
    }

    /// A 220 Hz tone playing from `on` to `off` seconds (5 ms ramps), `l` and
    /// `r` its level on each side.
    fn played(secs: f32, on: f32, off: f32, l: f32, r: f32) -> Vec<(f32, f32)> {
        (0..(secs * SR) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let gate = ((t - on) / 0.005).min((off - t) / 0.005).clamp(0.0, 1.0);
                let v = gate * (2.0 * PI * 220.0 * t).sin();
                (l * v, r * v)
            })
            .collect()
    }

    /// How far `ducked` sits below `plain` from `a` to `b` seconds, in dB,
    /// on each side.
    fn dip_db(ducked: &[(f32, f32)], plain: &[(f32, f32)], a: f32, b: f32) -> (f32, f32) {
        let w = (a * SR) as usize..(b * SR) as usize;
        let e = |o: &[(f32, f32)], left: bool| o[w.clone()].iter().map(|s| if left { s.0 * s.0 } else { s.1 * s.1 }).sum::<f32>();
        (10.0 * (e(ducked, true) / e(plain, true)).log10(), 10.0 * (e(ducked, false) / e(plain, false)).log10())
    }

    #[test]
    fn the_echoes_dip_while_you_play_and_come_back() {
        // A tone (−6 dBFS left, quieter right) for half a second into one tap
        // with a long tail, wet only, turned down so the output limiter never
        // acts. Duck at 12 dB, Release 300 ms, against no ducking: the echoes
        // dip by about 11 dB (the curve gives 92 % of the depth at −6 dBFS),
        // both sides alike, stay down through the 20 ms hold, and are back
        // within 2 dB three release times after the input stops.
        let run = |duck: f32| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.9);
            set(&plug, "output_gain", -24.0);
            set(&plug, "duck", duck);
            set(&plug, "duck_release", 300.0);
            render(&mut plug, &played(1.6, 0.0, 0.5, 0.5, 0.3), |_, _| {})
        };
        let (ducked, plain) = (run(12.0), run(0.0));
        let (l, r) = dip_db(&ducked, &plain, 0.2, 0.45);
        assert!((l + 11.0).abs() < 1.0 && (l - r).abs() < 0.01, "while playing: L {l:.2} dB, R {r:.2} dB");
        let (l, _) = dip_db(&ducked, &plain, 0.53, 0.56);
        assert!(l < -9.0, "just after: {l:.2} dB");
        let (l, _) = dip_db(&ducked, &plain, 1.42, 1.5);
        assert!(l > -2.0 && l < 0.0, "three release times on: {l:.2} dB");
    }

    #[test]
    fn ducking_does_not_click() {
        // The held tone playing half of every second (5 ms ramps) through
        // three taps, Duck at full depth and the shortest Release, against
        // no ducking: the dip comes and goes without a click.
        let input: Vec<(f32, f32)> = held_tone()
            .into_iter()
            .enumerate()
            .map(|(i, (l, r))| {
                let ph = (i as f32 / SR) % 1.0;
                let gate = (ph / 0.005).min((0.5 - ph) / 0.005).clamp(0.0, 1.0);
                (l * gate, r * gate)
            })
            .collect();
        let sharpest = |duck: f32| {
            let mut plug = plugin(&[-0.5, 0.7, 0.0]);
            set(&plug, "feedback", 0.6);
            set(&plug, "duck", duck);
            set(&plug, "duck_release", 50.0);
            sharpest_bend(&render(&mut plug, &input, |_, _| {}))
        };
        let (plain, (ducked, at)) = (sharpest(0.0).0, sharpest(24.0));
        assert!(ducked < 1.5 * plain, "{:.1}× the unducked sharpest bend, at {:.3} s", ducked / plain, at as f32 / SR + 0.3);
    }

    #[test]
    fn a_sudden_note_ducks_without_a_click() {
        // Freeze holds a clean sine (225 Hz fits the 40 ms cycle nine times,
        // so the held loop has no join to speak of). Once the ducking has let
        // go of the tone that filled it, a note that starts with no ramp at
        // all (a pluck, a hit) only keys the ducking, since the hold keeps it
        // out. At full depth the dip must start without a corner. Measured
        // around the hit.
        let input: Vec<(f32, f32)> = (0..(2.6 * SR) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let gate = (t / 0.005).min((0.35 - t) / 0.005).clamp(0.0, 1.0);
                let held = 0.25 * gate * (2.0 * PI * 225.0 * t).sin();
                let hit = if (2.0..2.2).contains(&t) { 0.8 * (2.0 * PI * 330.0 * (t - 2.0)).sin() } else { 0.0 };
                (held + hit, held + hit)
            })
            .collect();
        let sharpest = |duck: f32| {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 0.5);
            set(&plug, "output_gain", -24.0);
            set(&plug, "duck", duck);
            let out = render(&mut plug, &input, freeze_from(0.3));
            // sharpest_bend skips its first 0.3 s: this looks at 1.9–2.3 s
            let (bend, at) = sharpest_bend(&out[(1.6 * SR) as usize..(2.3 * SR) as usize]);
            (bend, at + (1.6 * SR) as usize)
        };
        let (plain, (ducked, at)) = (sharpest(0.0).0, sharpest(24.0));
        assert!(ducked < 1.5 * plain, "{:.1}× the unducked sharpest bend, at {:.3} s", ducked / plain, at as f32 / SR + 0.3);
    }

    #[test]
    fn a_held_loop_ducks_under_your_playing() {
        // Freeze holds a noise burst going round; a tone played over it from
        // 0.6 s (kept out of the loop) dips the held loop, which comes back
        // after it stops.
        let run = |duck: f32| {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 0.5);
            set(&plug, "output_gain", -24.0);
            set(&plug, "duck", duck);
            let mut input = noise_burst(2.5);
            for (s, t) in input.iter_mut().zip(played(2.5, 0.6, 1.0, 0.5, 0.5)) {
                s.0 += t.0;
                s.1 += t.1;
            }
            render(&mut plug, &input, freeze_from(0.1))
        };
        let (ducked, plain) = (run(12.0), run(0.0));
        let (during, _) = dip_db(&ducked, &plain, 0.7, 0.95);
        let (after, _) = dip_db(&ducked, &plain, 2.3, 2.4);
        assert!(during < -9.0 && after > -1.0, "during {during:.2} dB, after {after:.2} dB");
    }

    #[test]
    fn a_pattern_moving_every_tap_does_not_click() {
        // The held tone through eight taps; four times every tap's gain, pan,
        // cutoff, Q and filter type jump at once, swapping between two
        // alternating patterns (as the pattern tools do), against the tone
        // held under each: the per-tap glides and the type crossfade keep it
        // from clicking.
        let input = held_tone();
        let n = input.len();
        let pattern = |plug: &Afterecho, flip: bool| {
            for i in 0..8 {
                let odd = (i % 2 == 1) != flip;
                let k = i + 1;
                set(plug, &format!("gain_db_{k}"), if odd { -18.0 } else { 0.0 });
                set(plug, &format!("pan_{k}"), if odd { 0.8 } else { -0.8 });
                set(plug, &format!("tap_filter_cutoff_{k}"), if odd { 600.0 } else { 9000.0 });
                set(plug, &format!("tap_filter_q_{k}"), if odd { 4.0 } else { 0.7 });
                set(plug, &format!("tap_filter_type_{k}"), if odd { 2.0 } else { 0.0 });
            }
        };
        let sharpest = |schedule: [bool; 4]| {
            let mut plug = plugin(&[0.0; 8]);
            set(&plug, "feedback", 0.4);
            sharpest_bend(&render(&mut plug, &input, |s, plug| pattern(plug, schedule[s * 4 / n])))
        };
        let held = [[false; 4], [true; 4]].map(|s| sharpest(s).0).into_iter().fold(0.0, f32::max);
        let (switched, at) = sharpest([false, true, false, true]);
        assert!(
            switched < 1.5 * held,
            "{:.1}× the held patterns' sharpest bend, at {:.3} s",
            switched / held,
            at as f32 / SR + 0.3
        );
    }

    #[test]
    fn changing_slope_does_not_click() {
        // The held tone through eight resonant low-passed taps; Slope jumps between 6 and
        // 48 dB/oct (stages switching in and out) four times, against the tone held at each:
        // the crossfade from the old stages keeps it from clicking.
        let input = held_tone();
        let n = input.len();
        let slopes = |plug: &Afterecho, steep: bool| {
            for k in 1..=8 {
                set(plug, &format!("tap_filter_cutoff_{k}"), 700.0);
                set(plug, &format!("tap_filter_q_{k}"), 2.0);
                set(plug, &format!("tap_filter_slope_{k}"), if steep { 7.0 } else { 0.0 });
            }
        };
        let sharpest = |schedule: [bool; 4]| {
            let mut plug = plugin(&[0.0; 8]);
            set(&plug, "feedback", 0.4);
            sharpest_bend(&render(&mut plug, &input, |s, plug| slopes(plug, schedule[s * 4 / n])))
        };
        let held = [[false; 4], [true; 4]].map(|s| sharpest(s).0).into_iter().fold(0.0, f32::max);
        let (switched, at) = sharpest([false, true, false, true]);
        assert!(
            switched < 1.5 * held,
            "{:.1}× the held slopes' sharpest bend, at {:.3} s",
            switched / held,
            at as f32 / SR + 0.3
        );
    }

    #[test]
    fn freezing_and_letting_go_do_not_click() {
        // The held tone through three spread taps, frozen and let go a second
        // at a time, against it never frozen: in Mix and in Ping, plain and
        // with Tone and Tape in the loop.
        let input = held_tone();
        let n = input.len();
        for mode in [0.0, PING] {
            for colour in [&[][..], &[("loop_tone", -0.6), ("degrade_routing", 1.0), ("degrade_amount", 0.6)]] {
                let sharpest = |schedule: [f32; 5]| {
                    let mut plug = plugin(&[-0.5, 0.7, 0.0]);
                    set(&plug, "feedback", 0.6);
                    set(&plug, "crossfeed", 1.0);
                    set(&plug, "crossfeed_mode", mode);
                    for &(id, v) in colour {
                        set(&plug, id, v);
                    }
                    sharpest_bend(&render(&mut plug, &input, |s, plug| set(plug, "freeze", schedule[s * 5 / n])))
                };
                let held = sharpest([0.0; 5]).0;
                let (switched, at) = sharpest([0.0, 1.0, 0.0, 1.0, 0.0]);
                assert!(
                    switched < 1.5 * held,
                    "mode {mode}, {colour:?}: {:.1}× the held tone's sharpest bend, at {:.3} s",
                    switched / held,
                    at as f32 / SR + 0.3
                );
            }
        }
    }

    #[test]
    fn swing_moves_the_odd_taps_late_in_the_audio() {
        // A click through 4 taps on 20 ms steps in Lin, then 3 taps in 8 Euclid steps: Swing
        // 75 % moves the 1st and 3rd echoes halfway to the next, in what plays (in Euclid the
        // last one's next is the first, a loop later).
        let arrivals = |mode: f32, swing: f32| {
            let mut plug = plugin(&[0.0; 4]);
            set(&plug, "feedback", 0.0);
            set(&plug, "spacing_mode", mode);
            if mode == 3.0 {
                set(&plug, "num_taps_active", 3.0);
                set(&plug, "euclid_steps", 8.0);
            }
            set(&plug, "swing", swing);
            let out = render(&mut plug, &click(0.25), |_, _| {});
            // each echo: the loudest sample of each burst, in ms, over the first 230 ms
            let env: Vec<f32> = out.iter().take((0.23 * SR) as usize).map(|s| s.0.abs()).collect();
            let top = env.iter().fold(0.0f32, |m, &v| m.max(v));
            let mut at = vec![];
            for i in 24..env.len() - 24 {
                let v = env[i];
                if v > 0.3 * top && env[i - 24..i + 24].iter().all(|&w| w <= v) && at.last().is_none_or(|&a: &f32| i as f32 / SR * 1000.0 - a > 1.0) {
                    at.push((i as f32 / SR * 1000.0 * 10.0).round() / 10.0);
                }
            }
            at
        };
        assert_eq!(arrivals(0.0, 0.5), [20.0, 40.0, 60.0, 80.0]);
        assert_eq!(arrivals(0.0, 0.75), [30.0, 40.0, 70.0, 80.0]);
        assert_eq!(arrivals(3.0, 0.5), [60.0, 120.0, 160.0]);
        assert_eq!(arrivals(3.0, 0.75), [90.0, 120.0, 190.0]);
    }

    #[test]
    fn euclid_loops_on_the_whole_pattern() {
        // 3 taps over 8 steps of 20 ms, turned a step: taps on steps 1, 4 and
        // 7 (20, 80, 140 ms). Auto loops on all 8 steps (160 ms), not on the
        // last tap; the page draws the same cycle. Taps past the steps are
        // silent.
        let mut plug = plugin(&[0.0, 0.0, 0.0]);
        set(&plug, "spacing_mode", 3.0);
        set(&plug, "euclid_steps", 8.0);
        set(&plug, "euclid_rotate", 1.0);
        render(&mut plug, &click(0.05), |_, _| {});
        let cycle = plug.fb_time_slew.current() / SR;
        assert!((cycle - 0.16).abs() < 1e-4, "cycle {cycle} s");
        let viz = crate::gui::compute_viz_data_ex(&plug.params, 120.0, 3);
        assert!((viz.fb_cycle_norm * viz.grid_range_secs - 0.16).abs() < 1e-4);
        set(&plug, "num_taps_active", 12.0);
        assert_eq!(plug.params.taps_heard(), 8);
    }

    /// The stereo balance of 20 ms windows: −1 all left … 1 all right.
    fn balances(out: &[(f32, f32)]) -> Vec<f32> {
        out.chunks((0.02 * SR) as usize)
            .map(|w| {
                let (l, r) = w.iter().fold((0.0, 0.0), |(l, r), s| (l + s.0 * s.0, r + s.1 * s.1));
                (r - l) / (r + l).max(1e-12)
            })
            .collect()
    }

    #[test]
    fn a_count_repeats_the_pattern_then_stops() {
        // one tap at 20 ms (a 20 ms cycle), three repeats at 100 %: the tap
        // and three copies, all one level, then nothing
        let mut plug = plugin(&[0.0]);
        set(&plug, "repeats", 3.0);
        set(&plug, "feedback", 1.0);
        let mut input = vec![(0.0, 0.0); (0.3 * SR) as usize];
        input[(0.05 * SR) as usize] = (0.1, 0.1);
        let out = render(&mut plug, &input, |_, _| {});
        let peak = |ms: f32| {
            let at = ((50.0 + ms) * 0.001 * SR) as usize;
            out[at - 48..at + 48].iter().fold(0.0f32, |m, s| m.max(s.0.abs()))
        };
        let tap = peak(20.0);
        assert!(tap > 0.02, "tap {tap}");
        for ms in [40.0, 60.0, 80.0] {
            assert!((peak(ms) / tap - 1.0).abs() < 0.05, "copy at {ms} ms: {:.3} of the tap", peak(ms) / tap);
        }
        let after = out[((50.0 + 92.0) * 0.001 * SR) as usize..].iter().fold(0.0f32, |m, s| m.max(s.0.abs()));
        assert!(after < 1e-4 * tap.max(1.0), "after the last copy: {after}");
        // at 50 % each copy is half the last
        let mut plug = plugin(&[0.0]);
        set(&plug, "repeats", 2.0);
        set(&plug, "feedback", 0.5);
        let out2 = render(&mut plug, &input, |_, _| {});
        let p2 = |ms: f32| {
            let at = ((50.0 + ms) * 0.001 * SR) as usize;
            out2[at - 48..at + 48].iter().fold(0.0f32, |m, s| m.max(s.0.abs()))
        };
        assert!((p2(40.0) / p2(20.0) - 0.5).abs() < 0.03 && (p2(60.0) / p2(20.0) - 0.25).abs() < 0.03);
    }

    #[test]
    fn switching_between_a_count_and_the_loop_does_not_click() {
        // the held tone through three taps; Repeats flips between 4 and ∞
        // every half second, against it left on ∞
        let input = held_tone();
        let sharpest = |flip: bool| {
            let mut plug = plugin(&[-0.5, 0.7, 0.0]);
            set(&plug, "feedback", 0.7);
            sharpest_bend(&render(&mut plug, &input, |at, plug| {
                if flip && at % (SR as usize / 2) < 256 {
                    let count = (at / (SR as usize / 2)) % 2 == 1;
                    set(plug, "repeats", if count { 4.0 } else { REPEATS_LOOP as f32 });
                }
            }))
        };
        let (still, (flipping, at)) = (sharpest(false).0, sharpest(true));
        assert!(flipping < 1.5 * still, "{:.1}× the steady tone's sharpest bend, at {:.3} s", flipping / still, at as f32 / SR + 0.3);
    }

    #[test]
    fn motion_moves_the_whole_echo() {
        // LFO 1 at 2 Hz on Mix, half the knob's travel either way, from 50 %:
        // the mix swings all the way, and Feedback stays on its knob
        let mut plug = plugin(&[0.0]);
        set(&plug, "dry_wet", 0.5);
        set(&plug, "feedback", 0.4);
        slot(&plug, 0, Rate::hz(2.0), |_| {});
        route(&plug, 0, "mix", 0.5);
        let (mut lo, mut hi, mut fb) = (1.0f32, 0.0f32, (1.0f32, 0.0f32));
        render(&mut plug, &held_tone()[..SR as usize], |at, plug| {
            if at == 0 {
                return; // (nothing has run yet)
            }
            (lo, hi) = (lo.min(plug.mix_now), hi.max(plug.mix_now));
            fb = (fb.0.min(plug.fb_now), fb.1.max(plug.fb_now));
        });
        assert!(lo < 0.05 && hi > 0.95, "mix {lo:.2} … {hi:.2}");
        assert!((fb.0 - 0.4).abs() < 1e-3 && (fb.1 - 0.4).abs() < 1e-3, "feedback {fb:?}");
    }

    #[test]
    fn a_cycle_comes_round_exactly_on_time() {
        // one tap at 20 ms (960 samples, a 960-sample cycle): an impulse
        // comes back at 960, 1920 and 2880 samples, on the loop and on a
        // count alike (before v2.44 the loop's came a sample later each pass)
        for repeats in [REPEATS_LOOP as f32, 3.0] {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 1.0);
            set(&plug, "repeats", repeats);
            let mut input = vec![(0.0, 0.0); 4000];
            input[0] = (0.1, 0.1);
            let out = render(&mut plug, &input, |_, _| {});
            for k in 1..=3 {
                let w = &out[k * 960 - 8..k * 960 + 8];
                let at = w.iter().enumerate().max_by(|a, b| a.1 .0.abs().total_cmp(&b.1 .0.abs())).unwrap().0;
                assert_eq!(at, 8, "repeats {repeats}: pass {k} peaks {} samples off", at as i32 - 8);
            }
        }
    }

/// A stereo 32-bit float WAV at SR (for renders to listen to).
    fn write_wav(path: &std::path::Path, out: &[(f32, f32)]) {
        let mut b = Vec::new();
        let data = (out.len() * 8) as u32;
        b.extend(b"RIFF");
        b.extend((36 + data).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(3u16.to_le_bytes()); // float
        b.extend(2u16.to_le_bytes());
        b.extend((SR as u32).to_le_bytes());
        b.extend((SR as u32 * 8).to_le_bytes());
        b.extend(8u16.to_le_bytes());
        b.extend(32u16.to_le_bytes());
        b.extend(b"data");
        b.extend(data.to_le_bytes());
        for (l, r) in out {
            b.extend(l.to_le_bytes());
            b.extend(r.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// Detune through the plug-in, to hear (not a check):
    /// `DETUNE_WAV=<folder> cargo test … detune_renders -- --ignored`.
    #[test]
    #[ignore]
    fn detune_renders() {
        let Some(dir) = std::env::var_os("DETUNE_WAV") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        // a plucked phrase, then a held chord
        let notes = [220.0f32, 277.2, 329.6, 440.0, 329.6, 277.2, 246.9, 220.0];
        let pluck: Vec<(f32, f32)> = (0..(SR * 6.0) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let k = (t / 0.5) as usize;
                let ph = t - k as f32 * 0.5;
                let v = if k < notes.len() {
                    let f = notes[k];
                    0.3 * (-ph * 6.0).exp() * ((2.0 * PI * f * t).sin() + 0.4 * (4.0 * PI * f * t).sin() + 0.2 * (6.0 * PI * f * t).sin())
                } else {
                    0.0
                };
                (v, v)
            })
            .collect();
        let chord: Vec<(f32, f32)> = (0..(SR * 5.0) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let v = 0.1 * (t / 0.05).min(1.0) * [130.8f32, 196.0, 329.6, 493.9, 587.3].iter().map(|f| (2.0 * PI * f * t).sin()).sum::<f32>();
                (v, v)
            })
            .collect();
        for (q, name) in [(0.0, "light"), (1.0, "clean")] {
            for (input, what) in [(&pluck, "pluck"), (&chord, "chord")] {
                // six taps at −25 … +100 ct, alternating sides, 40 % feedback
                let mut plug = plugin(&[-0.7, 0.7, -0.5, 0.5, -0.3, 0.3]);
                set(&plug, "base_time_l", 110f32.ln() / 2000f32.ln());
                set(&plug, "feedback", 0.4);
                set(&plug, "dry_wet", 0.5);
                set(&plug, "detune_quality", q);
                for (i, c) in [-25.0, 25.0, -50.0, 50.0, -100.0, 100.0].into_iter().enumerate() {
                    set(&plug, &format!("tap_detune_{}", i + 1), c);
                }
                write_wav(&dir.join(format!("{what}-{name}.wav")), &render(&mut plug, input, |_, _| {}));
            }
        }
        for (input, what) in [(&pluck, "pluck"), (&chord, "chord")] {
            let mut plug = plugin(&[-0.7, 0.7, -0.5, 0.5, -0.3, 0.3]);
            set(&plug, "base_time_l", 110f32.ln() / 2000f32.ln());
            set(&plug, "feedback", 0.4);
            set(&plug, "dry_wet", 0.5);
            write_wav(&dir.join(format!("{what}-0-nodetune.wav")), &render(&mut plug, input, |_, _| {}));
        }
        // v2.46, Range 12: six taps at −5, +5, −7, +7, −12, +12 st on a
        // 250 ms base (long enough for Clean an octave down)
        for (q, name) in [(0.0, "light"), (1.0, "clean")] {
            for (input, what) in [(&pluck, "pluck"), (&chord, "chord")] {
                let mut plug = plugin(&[-0.7, 0.7, -0.5, 0.5, -0.3, 0.3]);
                set(&plug, "base_time_l", 250f32.ln() / 2000f32.ln());
                set(&plug, "feedback", 0.4);
                set(&plug, "dry_wet", 0.5);
                set(&plug, "detune_quality", q);
                set(&plug, "detune_range", 12.0);
                for (i, st) in [-5.0f32, 5.0, -7.0, 7.0, -12.0, 12.0].into_iter().enumerate() {
                    set(&plug, &format!("tap_detune_{}", i + 1), st / 12.0 * 100.0);
                }
                write_wav(&dir.join(format!("{what}-range12-{name}.wav")), &render(&mut plug, input, |_, _| {}));
            }
        }
    }

    /// Before/after renders for the SOF-60 fix, to hear (not a check):
    /// `SOF60_WAV=<folder> cargo test … sof60_renders -- --ignored --test-threads=1`.
    #[test]
    #[ignore]
    fn sof60_renders() {
        let Some(dir) = std::env::var_os("SOF60_WAV") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = |name: &str, out: &[(f32, f32)]| write_wav(&dir.join(name), out);
        let ms = |v: f32| v.ln() / 2000f32.ln();
        // 1. a held chord through four taps while Time glides 100 → 160 ms
        //    and back (Repitch): the glide itself, no repeats
        let chord: Vec<(f32, f32)> = (0..(SR * 8.0) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let v = 0.15 * ((2.0 * PI * 220.0 * t).sin() + (2.0 * PI * 277.2 * t).sin() + (2.0 * PI * 329.6 * t).sin());
                (v, v)
            })
            .collect();
        // 2. plucks every second into a loop at 90 % on a 33.3 ms Time
        //    (a fractional cycle), so the repeats' timing and tone show
        let plucks: Vec<(f32, f32)> = (0..(SR * 8.0) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let ph = t.fract();
                let v = if t < 6.0 { 0.4 * (-ph * 9.0).exp() * ((2.0 * PI * 440.0 * t).sin() + 0.5 * (2.0 * PI * 1320.0 * t).sin()) } else { 0.0 };
                (v, v)
            })
            .collect();
        for (legacy, tag) in [(true, "1-before"), (false, "2-after")] {
            crate::delay::LEGACY_READ.store(legacy, Ordering::Relaxed);
            let mut plug = plugin(&[-0.6, -0.2, 0.2, 0.6]);
            set(&plug, "feedback", 0.0);
            set(&plug, "dry_wet", 0.5);
            let n = chord.len();
            let out = render(&mut plug, &chord, |at, plug| {
                let x = at as f32 / n as f32;
                let tri = if x < 0.5 { x * 2.0 } else { 2.0 - x * 2.0 };
                set(plug, "base_time_l", ms(100.0 + 60.0 * tri));
            });
            wav(&format!("glide-{tag}.wav"), &out);
            let mut plug = plugin(&[-0.4, 0.4]);
            set(&plug, "base_time_l", ms(33.3));
            set(&plug, "feedback", 0.9);
            set(&plug, "dry_wet", 0.5);
            wav(&format!("repeats-{tag}.wav"), &render(&mut plug, &plucks, |_, _| {}));
        }
        crate::delay::LEGACY_READ.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_fractional_delay_reads_that_far_back() {
        // a ramp in a line: a read lands exactly that many samples back and
        // moves smoothly through each whole sample (no 2-sample jump)
        let mut line = DelayLine::new(64);
        for k in 0..40 {
            line.write(k as f32);
        }
        let newest = 39.0;
        for d in [1.0, 2.5, 10.25, 10.75, 11.0] {
            assert!((line.read(d) - (newest - d)).abs() < 1e-4, "read({d}) = {}", line.read(d));
        }
        let step = (line.read(11.001) - line.read(10.999)).abs();
        assert!(step < 0.01, "across a whole sample it moves {step}");
        // before the write, the newest is a sample old already
        assert!((line.read_before_write(10.25) - (newest + 1.0 - 10.25)).abs() < 1e-4);
        // and a whole-sample read is the same, without the interpolation
        // (from 2: `read` won't go under 1, so `read_before_write(1)` reads 2 back)
        assert_eq!(line.back(1), newest);
        for k in 2..30 {
            assert_eq!(line.back(k), line.read_before_write(k as f32));
        }
    }

    /// The pitch of `out` (left) between `from` and `to` seconds, from its
    /// upward zero crossings (interpolated).
    fn pitch(out: &[(f32, f32)], from: f32, to: f32) -> f32 {
        let w = &out[(from * SR) as usize..(to * SR) as usize];
        let ups: Vec<f32> = w
            .windows(2)
            .enumerate()
            .filter(|(_, p)| p[0].0 <= 0.0 && p[1].0 > 0.0)
            .map(|(i, p)| i as f32 + p[0].0 / (p[0].0 - p[1].0))
            .collect();
        (ups.len() - 1) as f32 * SR / (ups[ups.len() - 1] - ups[0])
    }

    fn sine(hz: f32, secs: f32) -> Vec<(f32, f32)> {
        (0..(secs * SR) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let v = 0.3 * (t / 0.02).min(1.0) * (2.0 * PI * hz * t).sin();
                (v, v)
            })
            .collect()
    }

    #[test]
    fn detune_moves_a_tap_by_its_cents() {
        for cents in [100.0f32, -100.0, 25.0, -50.0] {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.0);
            set(&plug, "base_time_l", 150f32.ln() / 2000f32.ln());
            set(&plug, "tap_detune_1", cents);
            let out = render(&mut plug, &sine(440.0, 3.0), |_, _| {});
            let want = 440.0 * 2f32.powf(cents / 1200.0);
            let got = pitch(&out, 1.0, 2.8);
            let off = 1200.0 * (got / want).log2();
            assert!(off.abs() < 3.0, "{cents} ct: {got:.2} Hz, {off:.1} cents from {want:.2}");
        }
    }

    #[test]
    fn clean_moves_a_tap_by_its_cents_and_short_taps_use_light() {
        // Clean on a 150 ms tap; a 40 ms tap (too short for Clean) on Light
        for (ms, cents) in [(150.0f32, 100.0f32), (150.0, -50.0), (40.0, 100.0)] {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.0);
            set(&plug, "detune_quality", 1.0);
            set(&plug, "base_time_l", ms.ln() / 2000f32.ln());
            set(&plug, "tap_detune_1", cents);
            let out = render(&mut plug, &sine(440.0, 3.0), |_, _| {});
            let want = 440.0 * 2f32.powf(cents / 1200.0);
            let got = pitch(&out, 1.0, 2.8);
            let off = 1200.0 * (got / want).log2();
            assert!(off.abs() < 3.0, "{ms} ms, {cents} ct: {got:.2} Hz, {off:.1} cents off");
            assert_eq!(plug.clean_w[0] > 0.5, ms > 100.0, "{ms} ms on the wrong quality");
        }
    }

    #[test]
    fn detune_range_reaches_an_octave() {
        // Range 12: a tap at ±100 % is an octave, at +58.3 % a fifth; Light
        // and Clean on a 300 ms tap, and Light on a 10 ms tap (too short for
        // a grain an octave up: it sounds a little late, never reads ahead)
        for (quality, ms, range) in [(0.0f32, 300.0f32, 12.0f32), (1.0, 300.0, 12.0), (0.0, 10.0, 12.0), (0.0, 10.0, 1.0)] {
            for (pct, hz) in [(100.0f32, 220.0f32), (-100.0, 440.0), (700.0 / 12.0, 330.0)] {
                let mut plug = plugin(&[0.0]);
                set(&plug, "feedback", 0.0);
                set(&plug, "detune_quality", quality);
                set(&plug, "detune_range", range);
                set(&plug, "base_time_l", ms.ln() / 2000f32.ln());
                set(&plug, "tap_detune_1", pct);
                let out = render(&mut plug, &sine(hz, 3.0), |_, _| {});
                let cents = pct * range;
                let want = hz * 2f32.powf(cents / 1200.0);
                let got = pitch(&out, 1.0, 2.8);
                let off = 1200.0 * (got / want).log2();
                assert!(off.abs() < 3.0, "q{quality} {ms} ms, {cents:.0} ct: {got:.2} Hz, {off:.1} cents off");
                assert!(out.iter().all(|s| s.0.is_finite() && s.0.abs() < 1.0));
            }
        }
    }

    /// A 5 ms noise burst at 0.1 s, then silence.
    fn burst(secs: f32) -> Vec<(f32, f32)> {
        let mut seed = 1u32;
        (0..(secs * SR) as usize)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let v = if (4800..5040).contains(&i) { 0.5 * (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) } else { 0.0 };
                (v, v)
            })
            .collect()
    }

    /// How much later (ms) each of `b`'s repeats sits than `a`'s: the
    /// energy centres within ±`half_s` of each of `a`'s peaks after `from`
    /// seconds, compared.
    fn repeat_offsets_ms(a: &[(f32, f32)], b: &[(f32, f32)], from: f32, half_s: f32) -> Vec<f32> {
        let e = |x: &[(f32, f32)], i: usize| x[i].0 * x[i].0 + x[i].1 * x[i].1;
        let half = (half_s * SR) as usize;
        let top = (0..a.len()).map(|i| e(a, i)).fold(0.0, f32::max);
        let mut out = Vec::new();
        let mut i = (from * SR) as usize;
        while i + half < a.len() {
            let w = i.saturating_sub(half)..i + half;
            if e(a, i) > 1e-4 * top && w.clone().all(|j| e(a, j) <= e(a, i)) {
                let centre = |x: &[(f32, f32)]| {
                    let (num, den) = w.clone().fold((0.0f64, 0.0f64), |(n, d), j| (n + j as f64 * e(x, j) as f64, d + e(x, j) as f64));
                    num / den.max(1e-30)
                };
                out.push(((centre(b) - centre(a)) / SR as f64 * 1000.0) as f32);
                i += half;
            }
            i += 1;
        }
        out
    }

    #[test]
    fn diffuse_in_the_loop_keeps_the_cycle() {
        // A burst round a 400 ms loop at 70 %: with Diffuse in it (Colour on
        // FB, or Cross → Diffuse) the first three passes land where they do
        // without (v2.46's chain put 179 ms on each pass at 60 %), within
        // 1 ms a pass (the drift moves each pass's centre a little).
        let run = |cfg: &[(&str, f32)]| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "base_time_l", 400f32.ln() / 2000f32.ln());
            set(&plug, "feedback", 0.7);
            for &(id, v) in cfg {
                set(&plug, id, v);
            }
            render(&mut plug, &burst(2.6), |_, _| {})
        };
        let plain = run(&[]);
        for cfg in [
            &[("diffuse_routing", 1.0), ("diffuse_amount", 0.6)][..],
            &[("diffuse_routing", 1.0), ("diffuse_amount", 1.0)],
            &[("diffuse_routing", 1.0), ("diffuse_amount", 0.6), ("diffuse_size", 2.0)],
            &[("crossfeed_mode", 2.0), ("crossfeed", 1.0)],
        ] {
            // the first offset is the tap itself, before any pass
            let offs = repeat_offsets_ms(&plain, &run(cfg), 0.4, 0.19);
            assert!(offs.len() >= 4, "{offs:?}");
            let ok = offs[..4].iter().enumerate().all(|(k, o)| o.abs() <= 0.5 + 1.0 * k as f32);
            assert!(ok, "{cfg:?}: repeats off by {offs:?} ms");
        }
    }

    #[test]
    fn the_crates_saturate_in_the_loop_keeps_the_cycle() {
        // pass 6: each crate curve delays a pass by its oversampling (20 samples, 0.42 ms);
        // the loop reads that much earlier, so passes land where Classic's do
        let run = |curve: f32| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "base_time_l", 400f32.ln() / 2000f32.ln());
            set(&plug, "feedback", 0.7);
            set(&plug, "sat_routing", 1.0);
            set(&plug, "sat_amount", 0.7);
            set(&plug, "sat_curve", curve);
            render(&mut plug, &burst(2.6), |_, _| {})
        };
        let classic = run(0.0);
        for curve in 1..=5 {
            let offs = repeat_offsets_ms(&classic, &run(curve as f32), 0.4, 0.19);
            assert!(offs.len() >= 4, "{offs:?}");
            // without the take-back they'd drift 0.42 ms a pass
            let ok = offs[..4].iter().all(|o| o.abs() <= 0.25);
            assert!(ok, "curve {curve}: repeats off by {offs:?} ms");
        }
    }

    #[test]
    fn the_crates_saturate_lets_the_loop_self_oscillate() {
        // past 100 % with each crate curve on FB: still going (no gain compensation), bounded
        for curve in 1..=5 {
            let mut plug = plugin(&[0.0, 0.4]);
            set(&plug, "feedback", 1.05);
            set(&plug, "sat_routing", 1.0);
            set(&plug, "sat_amount", 1.0);
            set(&plug, "sat_drive", 18.0);
            set(&plug, "sat_curve", curve as f32);
            let out = render(&mut plug, &noise_burst(3.0), |_, _| {});
            let half = (SR / 2.0) as usize;
            let last = &out[out.len() - half..];
            assert!(last.iter().all(|s| s.0.is_finite() && s.1.is_finite()));
            let rms = (last.iter().map(|s| s.0 * s.0 + s.1 * s.1).sum::<f32>() / half as f32).sqrt();
            let lines = (1..half).fold(0.0f32, |m, d| m.max(plug.main_l.delay.read(d as f32).abs()).max(plug.main_r.delay.read(d as f32).abs()));
            assert!(rms > 0.005 && lines <= 2.0, "curve {curve}: rms {rms}, loop peak {lines}");
        }
    }

    #[test]
    fn diffuse_in_the_loop_at_0_is_the_plain_loop() {
        // Colour on FB at Amount 0 is the chain's delay, taken back by the read
        let run = |routing: f32| {
            let mut plug = plugin(&[-0.5, 0.5]);
            set(&plug, "feedback", 0.8);
            set(&plug, "diffuse_routing", routing);
            render(&mut plug, &held_tone(), |_, _| {})
        };
        let (off, fb) = (run(2.0), run(1.0));
        let worst = off.iter().zip(&fb).map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs())).fold(0.0, f32::max);
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn turning_diffuse_size_does_not_click() {
        let input = held_tone();
        let n = input.len();
        let sharpest = |sweep: bool| {
            let mut plug = plugin(&[-0.5, 0.7, 0.0]);
            set(&plug, "feedback", 0.6);
            set(&plug, "diffuse_routing", 1.0);
            set(&plug, "diffuse_amount", 0.6);
            sharpest_bend(&render(&mut plug, &input, |at, plug| {
                if sweep {
                    set(plug, "diffuse_size", 1.0 + 0.6 * (at as f32 / n as f32 * 9.0).sin());
                }
            }))
            .0
        };
        let (still, moving) = (sharpest(false), sharpest(true));
        assert!(moving < 1.5 * still, "{:.2}×", moving / still);
    }

    #[test]
    fn diffuse_widens_a_mono_source() {
        // a centred tap, Diffuse on Out at 100 %: the sides come apart
        let corr = |amount: f32| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "feedback", 0.0);
            set(&plug, "diffuse_routing", 2.0);
            set(&plug, "diffuse_amount", amount);
            let out = render(&mut plug, &burst(1.0), |_, _| {});
            let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
            for s in &out {
                lr += (s.0 * s.1) as f64;
                ll += (s.0 * s.0) as f64;
                rr += (s.1 * s.1) as f64;
            }
            lr / (ll * rr).sqrt()
        };
        assert!(corr(0.0) > 0.99);
        assert!(corr(1.0) < 0.5, "{}", corr(1.0));
    }

    #[test]
    fn in_and_out_are_the_shared_level_knob() {
        use softsystems_ui::knob::level;
        let plug = Afterecho::default();
        for g in [&plug.params.input_gain, &plug.params.output_gain] {
            for db in [-60.0, -24.0, 0.0, 6.0, 12.0] {
                assert!((g.preview_normalized(db) - level::norm(db)).abs() < 1e-5, "{db} dB at {}", g.preview_normalized(db));
            }
            assert!((g.default_normalized_value() - level::ZERO).abs() < 1e-5);
            assert_eq!(g.normalized_value_to_string(0.0, true), "\u{2212}\u{221e} dB");
            assert_eq!(g.string_to_normalized_value("-inf"), Some(0.0));
            assert_eq!(g.string_to_normalized_value("+12 dB"), Some(1.0));
        }
        assert_eq!(level::gain(level::norm(-60.0)), 0.0, "the bottom is silent");
    }

    #[test]
    fn changing_detune_while_it_plays_does_not_click() {
        // Light and Clean: the knob swept across its range over the held
        // tone, against the tone at a steady detune
        let input = held_tone();
        for quality in [0.0, 1.0] {
            let sharpest = |sweep: bool| {
                let mut plug = plugin(&[0.0]);
                set(&plug, "detune_quality", quality);
                set(&plug, "base_time_l", 150f32.ln() / 2000f32.ln());
                set(&plug, "tap_detune_1", 50.0);
                let n = input.len();
                sharpest_bend(&render(&mut plug, &input, |at, plug| {
                    if sweep {
                        set(plug, "tap_detune_1", (100.0 * (at as f32 / n as f32 * 6.0).sin()).round());
                    }
                }))
            };
            let (still, (moving, at)) = (sharpest(false).0, sharpest(true));
            assert!(moving < 1.5 * still, "quality {quality}: {:.2}× at {:.3} s", moving / still, at as f32 / SR + 0.3);
        }
    }

    /// CPU of 16 detuned taps (not a check): `cargo test … detune_cpu -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn detune_cpu() {
        let input: Vec<(f32, f32)> = held_tone()[..(4.0 * SR) as usize].to_vec();
        for (name, quality, cents) in [("plain", 0.0, 0.0), ("light", 0.0, 1.0), ("clean", 1.0, 1.0)] {
            let mut plug = plugin(&[0.0; 16]);
            set(&plug, "base_time_l", 200f32.ln() / 2000f32.ln());
            set(&plug, "feedback", 0.5);
            set(&plug, "detune_quality", quality);
            for i in 0..16 {
                set(&plug, &format!("tap_detune_{}", i + 1), cents * (i as f32 * 13.0 - 100.0).clamp(-100.0, 100.0));
            }
            let t = std::time::Instant::now();
            render(&mut plug, &input, |_, _| {});
            let secs = t.elapsed().as_secs_f64();
            eprintln!("CPU {name}: {:.1} % of one core ({:.0} ns/sample)", 100.0 * secs / 4.0, secs * 1e9 / input.len() as f64);
        }
    }

    #[test]
    fn a_tap_at_zero_cents_is_untouched() {
        // detune set and back to 0 well before the sound: exactly the plain tap
        let run = |touch: bool| {
            let mut plug = plugin(&[-0.3, 0.4]);
            set(&plug, "feedback", 0.6);
            if touch {
                set(&plug, "tap_detune_2", 40.0);
                render(&mut plug, &vec![(0.0, 0.0); 4800], |_, _| {});
                set(&plug, "tap_detune_2", 0.0);
                render(&mut plug, &vec![(0.0, 0.0); 24000], |_, _| {});
            } else {
                render(&mut plug, &vec![(0.0, 0.0); 28800], |_, _| {});
            }
            render(&mut plug, &held_tone()[..SR as usize], |_, _| {})
        };
        assert!(run(false) == run(true));
    }

    #[test]
    fn the_repeats_keep_the_taps_detune_without_stacking() {
        // a 440 Hz burst, one tap at +100 ct, the loop at 80 %: the second
        // and third passes sound at +100 ct too, not +200 and +300
        let mut plug = plugin(&[0.0]);
        set(&plug, "base_time_l", 300f32.ln() / 2000f32.ln());
        set(&plug, "feedback", 0.8);
        set(&plug, "tap_detune_1", 100.0);
        let mut input = sine(440.0, 0.25);
        input.resize((1.5 * SR) as usize, (0.0, 0.0));
        let out = render(&mut plug, &input, |_, _| {});
        let want = 440.0 * 2f32.powf(100.0 / 1200.0);
        for (k, at) in [(1, 0.3), (2, 0.6), (3, 0.9)] {
            let got = pitch(&out, at + 0.08, at + 0.2);
            assert!((1200.0 * (got / want).log2()).abs() < 5.0, "pass {k}: {got:.1} Hz");
        }
    }

    #[test]
    fn a_detuned_taps_sides_splice_together() {
        // the held tone's right side is 0.6 of its left; through a centred,
        // detuned tap it stays exactly that (one splice for both sides)
        let mut plug = plugin(&[0.0]);
        set(&plug, "base_time_l", 150f32.ln() / 2000f32.ln());
        set(&plug, "tap_detune_1", -60.0);
        let out = render(&mut plug, &held_tone()[..(2.0 * SR) as usize], |_, _| {});
        let worst = out.iter().map(|s| (s.0 * 0.6 - s.1).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "the sides part by {worst}");
    }

    #[test]
    fn ear_guard_eases_a_runaway_loop_and_leaves_the_loop_alone() {
        // a loud tone into a loop at 105 %, then silence: with the guard the
        // echoes settle near −8 dBFS RMS; without it they stay louder. The
        // loop itself keeps going either way (it isn't touched).
        let run = |guard: bool| {
            let mut plug = plugin(&[-0.3, 0.3]);
            set(&plug, "feedback", 1.05);
            set(&plug, "ear_guard", if guard { 1.0 } else { 0.0 });
            let mut input = held_tone();
            input.resize((8.0 * SR) as usize, (0.0, 0.0));
            for s in input.iter_mut().skip(SR as usize) {
                *s = (0.0, 0.0);
            }
            let out = render(&mut plug, &input, |_, _| {});
            (rms(&out, 7.0, 8.0), plug.main_l.delay.buffer_peak())
        };
        let ((on, loop_on), (off, loop_off)) = (run(true), run(false));
        let on_db = 20.0 * (on / 2f32.sqrt()).log10();
        assert!(on_db < -7.0, "guarded echoes at {on_db:.1} dBFS RMS");
        assert!(off > on * 1.3, "unguarded {off:.3} vs guarded {on:.3}");
        assert!((loop_on - loop_off).abs() < 1e-3 * loop_off.max(1e-3), "the loop differs: {loop_on} vs {loop_off}");
    }

    #[test]
    fn clear_empties_the_delay() {
        let mut plug = plugin(&[0.0]);
        set(&plug, "feedback", 0.9);
        render(&mut plug, &held_tone()[..SR as usize], |_, _| {});
        assert!(plug.main_l.delay.buffer_peak() > 0.01);
        plug.params.clear_request.store(true, Ordering::Relaxed);
        let out = render(&mut plug, &vec![(0.0, 0.0); (0.2 * SR) as usize], |_, _| {});
        assert_eq!(plug.main_l.delay.buffer_peak(), 0.0);
        // the echoes faded out first: no step bigger than the tone's own
        let step = out.windows(2).fold(0.0f32, |m, w| m.max((w[1].0 - w[0].0).abs()));
        assert!(step < 0.05, "a step of {step}");
        assert!(out[(0.05 * SR) as usize..].iter().all(|s| s.0 == 0.0 && s.1 == 0.0));
    }

    #[test]
    fn a_broken_signal_is_silenced_and_cleared() {
        let mut plug = plugin(&[0.0]);
        set(&plug, "feedback", 0.9);
        let mut input = vec![(0.1, 0.1); 2048];
        input[300] = (f32::NAN, f32::NAN);
        render(&mut plug, &input, |_, _| {});
        let after = render(&mut plug, &held_tone()[..SR as usize], |_, _| {});
        assert!(after.iter().all(|s| s.0.is_finite() && s.1.is_finite()));
        assert!(rms(&after, 0.5, 1.0) > 0.01, "it plays again");
    }

    #[test]
    fn step_on_cutoff_is_no_sharper_than_a_smooth_lfo() {
        // (at Afterecho's default Slew; with Slew off see the eprintln)
        // Step 1 jumping a tap's cutoff over half its knob's travel at 8 steps
        // a second, Slew off, against a sine LFO at the same depth and rate:
        // the jumps are rounded off, so they bend the tone hardly more than
        // the smooth sweep does (the sweep itself is what moves it)
        let input = held_tone();
        let sharpest = |step: bool, slew: bool| {
            let mut plug = plugin(&[0.0]);
            set(&plug, "tap_filter_cutoff_1", 1500.0);
            if step {
                slot(&plug, 4, Rate::hz(8.0), |m| {
                    if let Modulator::Steps(st) = m {
                        st.levels[..4].copy_from_slice(&[1.0, 0.0, 0.75, 0.25]);
                        st.count = 4;
                        st.common.bipolar = true;
                        st.common.slew = slew;
                    }
                });
                route(&plug, 4, "cutoff", 0.25);
            } else {
                slot(&plug, 0, Rate::hz(8.0), |_| {});
                route(&plug, 0, "cutoff", 0.25);
            }
            sharpest_bend(&render(&mut plug, &input, |_, _| {}))
        };
        let (smooth, (stepped, at)) = (sharpest(false, true).0, sharpest(true, true));
        eprintln!("Slew off: {:.2}× the sine's", sharpest(true, false).0 / smooth);
        assert!(stepped < 1.2 * smooth, "{:.2}× the sine's sharpest bend, at {:.3} s", stepped / smooth, at as f32 / SR + 0.3);
    }

    #[test]
    fn motion_swings_the_taps_from_side_to_side() {
        // noise through one centred tap; Motion's sine at 2 Hz on Pan, half
        // the knob's travel either way: hard left to hard right
        let mut plug = plugin(&[0.0]);
        slot(&plug, 0, Rate::hz(2.0), |_| {});
        route(&plug, 0, "pan", 0.5);
        let input: Vec<(f32, f32)> = noise_burst(1.0).iter().enumerate().map(|(i, _)| {
            let v = ((i as u32).wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0 - 0.5;
            (0.3 * v, 0.3 * v)
        }).collect();
        let b = balances(&render(&mut plug, &input, |_, _| {}));
        let (lo, hi) = b[3..].iter().fold((1.0f32, -1.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(hi > 0.8 && lo < -0.8, "balance {lo:.2} … {hi:.2}");
    }

    #[test]
    fn motion_on_gain_stays_within_the_knob() {
        // the same tone with and without Gain motion (the crate's rule: a share
        // of the knob's travel, clamped at its ends): from 0 dB it rises at most
        // to the knob's +6 dB, and dips far more than that
        let run = |depth: f32| {
            let mut plug = plugin(&[0.0, 0.0, 0.0]);
            slot(&plug, 0, Rate::hz(3.0), |_| {});
            if depth != 0.0 {
                route(&plug, 0, "gain", depth);
            }
            render(&mut plug, &held_tone(), |_, _| {})
        };
        let (still, moving) = (run(0.0), run(0.5));
        let peak = |o: &[(f32, f32)], a: f32, b: f32| o[(a * SR) as usize..(b * SR) as usize].iter().fold(0.0f32, |m, s| m.max(s.0.abs()));
        assert!(peak(&moving, 0.5, 4.5) <= peak(&still, 0.5, 4.5) * 2.0 * 1.01);
        let quietest = (0..80).map(|k| rms(&moving, 0.5 + k as f32 * 0.05, 0.55 + k as f32 * 0.05)).fold(1.0f32, f32::min);
        assert!(quietest < 0.3 * rms(&still, 0.5, 4.5), "{quietest} against {}", rms(&still, 0.5, 4.5));
    }

    #[test]
    fn env_motion_opens_the_filter_while_you_play() {
        // Follow on Cutoff, the knob's whole travel, a tap set to 1 kHz: up
        // while the tone plays, back on its knob in the silence after it
        let mut plug = plugin(&[0.0]);
        route(&plug, 3, "cutoff", 1.0);
        set(&plug, "tap_filter_cutoff_1", 1000.0);
        render(&mut plug, &played(0.5, 0.0, 0.5, 0.5, 0.5), |_, _| {});
        let playing = plug.tap_cut[0].now;
        render(&mut plug, &vec![(0.0, 0.0); (1.5 * SR) as usize], |_, _| {});
        let quiet = plug.tap_cut[0].now;
        assert!(playing > 4000.0 && quiet < 1100.0, "playing {playing:.0} Hz, quiet {quiet:.0} Hz");
    }

    #[test]
    fn synced_motion_sits_where_the_song_is() {
        // Sync, a cycle of one bar, the host playing at beat 1 (a quarter of
        // the bar in): the sine is at its top
        let mut plug = plugin(&[0.0]);
        slot(&plug, 0, Rate::bars(1.0), |_| {});
        route(&plug, 0, "pan", 0.5);
        let mut t = Transport::new(SR);
        (t.playing, t.tempo, t.pos_beats) = (true, Some(120.0), Some(1.0));
        let mut host = Host(t);
        // (a few ticks: the crate's output ramps to each tick's value)
        let (mut l, mut r) = (vec![0.0f32; 96], vec![0.0f32; 96]);
        let mut buffer = Buffer::default();
        // SAFETY: l and r outlive the buffer.
        unsafe { buffer.set_slices(96, |b| *b = vec![&mut l[..], &mut r[..]]) };
        plug.process(&mut buffer, &mut AuxiliaryBuffers { inputs: &mut [], outputs: &mut [] }, &mut host);
        let pan = plug.params.motion.shared.matrix.voice_offset(crate::modulation::T_PAN, 0);
        assert!(pan > 0.499, "{pan}");
    }

    #[test]
    fn scatter_spreads_the_taps_round_the_cycle() {
        // two taps reading LFO 1 on Pan: Scatter 100 % puts them half a cycle
        // apart (opposite sides), 0 % together; the whole echo reads tap 1
        let offsets = |scatter: f32| {
            let mut plug = plugin(&[0.0, 0.0]);
            set(&plug, "mx_scatter", scatter);
            slot(&plug, 0, Rate::hz(1.3), |_| {});
            route(&plug, 0, "pan", 0.5);
            render(&mut plug, &vec![(0.0, 0.0); (0.4 * SR) as usize], |_, _| {});
            let mx = &plug.params.motion.shared.matrix;
            (mx.voice_offset(crate::modulation::T_PAN, 0), mx.voice_offset(crate::modulation::T_PAN, 1))
        };
        let (a, b) = offsets(1.0);
        assert!(a.abs() > 0.05 && (a + b).abs() < 0.02, "{a} {b}");
        let (a, b) = offsets(0.0);
        assert!(a.abs() > 0.05 && (a - b).abs() < 1e-4, "{a} {b}");
    }

    #[test]
    fn the_patch_rides_in_presets_and_reaches_the_engine() {
        // a route made in the editor is in the preset's blob, and a preset's
        // blob is what the engine plays after loading it
        use crate::presets::Store;
        use softsystems_presets::Plugin;
        let plug = plugin(&[0.0]);
        route(&plug, 2, "fdbk", -0.25);
        let blobs = Store::capture_blobs(&plug.params);
        let r = blobs.mods.as_ref().unwrap().routes.iter().find(|r| r.target == "fdbk").unwrap();
        assert_eq!((r.source.slot, r.amount), (2, -0.25));
        let mut other = plugin(&[0.0]);
        Store::apply_blobs(&other.params, &blobs);
        set(&other, "feedback", 0.5);
        render(&mut other, &vec![(0.0, 0.0); 4096], |_, _| {});
        assert!(other.modu.reaches(crate::modulation::T_FDBK) && !other.modu.reaches(crate::modulation::T_PAN));
    }

    #[test]
    fn a_slot_can_move_another_slots_rate() {
        // pass 11: LFO 2, held at one end (a square at a slow rate), on LFO 1's Rate runs LFO 1
        // at another speed than its 1 Hz
        let travelled = |routed: bool| {
            let mut plug = plugin(&[0.0]);
            route(&plug, 0, "pan", 0.3);
            if routed {
                slot(&plug, 1, Rate::hz(0.05), |m| {
                    if let Modulator::Curve(c) = m {
                        c.shape = softsystems_mod::Shape::Square;
                    }
                });
                route(&plug, 1, &softsystems_mod::Param::Rate.id(0), 0.5);
            }
            let mut pos = Vec::new();
            render(&mut plug, &vec![(0.0, 0.0); 4800], |_, p| pos.push(p.params.motion.shared.monitor.position(0)));
            // how far LFO 1 went round in 0.1 s, its wraps counted
            let mut turns = 0.0f32;
            for w in pos.windows(2) {
                let d = w[1] - w[0];
                turns += if d < -0.5 { d + 1.0 } else { d };
            }
            turns
        };
        let (plain, moved) = (travelled(false), travelled(true));
        assert!(plain > 0.0, "{plain}");
        // the square sits at one end for the whole render: half the range is the crate's 4 octaves, 16× (or ÷16)
        let ratio = (moved / plain).max(plain / moved);
        println!("LFO 1 in 0.1 s: plain {plain:.3} turns, routed {moved:.3}");
        assert!(ratio > 1.5, "plain {plain}, routed {moved}");
    }

    #[test]
    fn square_motion_does_not_click() {
        // a Square at full depth on Pan, Gain and Cutoff, over the held tone
        // through three taps, against it still: the glides round every edge
        let input = held_tone();
        let sharpest = |depth: f32| {
            let mut plug = plugin(&[-0.5, 0.7, 0.0]);
            set(&plug, "feedback", 0.6);
            slot(&plug, 0, Rate::hz(2.0), |m| {
                if let Modulator::Curve(c) = m {
                    c.shape = softsystems_mod::Shape::Square;
                }
            });
            // v2.42's full depth in knob travel: Pan's whole width, Gain's
            // 24 dB of its 66 (from −12 dB, so it swings −24…0 dB as v2.42's
            // dip did, its top the still tone's level), Cutoff's 4 octaves
            if depth != 0.0 {
                for k in 1..=3 {
                    set(&plug, &format!("gain_db_{k}"), -12.0);
                }
                for (id, a) in [("pan", 0.5), ("gain", 0.18), ("cutoff", 0.3)] {
                    route(&plug, 0, id, a * depth);
                }
            }
            set(&plug, "tap_filter_cutoff_1", 2000.0);
            sharpest_bend(&render(&mut plug, &input, |_, _| {}))
        };
        let (still, (moving, at)) = (sharpest(0.0).0, sharpest(1.0));
        assert!(moving < 1.5 * still, "{:.1}× the still tone's sharpest bend, at {:.3} s", moving / still, at as f32 / SR + 0.3);
    }
}

