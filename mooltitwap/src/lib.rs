mod bounce;
mod delay;
mod diffusion;
mod filter;
mod gui;
mod gui_scope;
mod gui_v3;
mod palette_scope;
mod palette_v3;
mod params;
mod presets;
mod tape;
mod tokens;
mod topology;
mod ui;

use nice_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::delay::{ChannelTaps, DelayLine, SlewedTime};
use crate::diffusion::Diffusion;
use crate::filter::{soft_clip, LoopTone, PeakLimiter, Svf, ToneCoefs};
use crate::params::{
    ChannelMode, CrossfeedMode, DelayMode, DiffuseRouting, FbCurve, PetalParams, TapeType, NUM_TAPS,
};
use crate::tape::{TapeDegrade, TapeDegradeParams, TapeKind};
use crate::topology::TopologyEngine;

/// Smoothstep: the shape of every mode fade (no corner where it starts or
/// ends, so a fade can't tick).
#[inline]
fn s_curve(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// What the feedback path does to each pass: Diffuse, Tape and Saturate
/// when they're routed to it (set per block), and the loop tone (which
/// glides per sample).
#[derive(Clone, Copy)]
struct LoopColour {
    /// Diffuse depth when routed to the feedback path, else 0.
    diffuse: f32,
    tape: bool,
    sat: bool,
    tone: ToneCoefs,
}

impl LoopColour {
    /// One pass through the loop's colour, then the soft clip that bounds it.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn apply(
        self, mut l: f32, mut r: f32, diff_l: &mut Diffusion, diff_r: &mut Diffusion, tape: &mut TapeDegrade,
        tone: &mut LoopTone, sat: &impl Fn(f32) -> f32,
    ) -> (f32, f32) {
        // DIFFUSE routing = Feedback: smear the recirculating signal so
        // each cluster repeat picks up more allpass density.
        if self.diffuse > 1e-4 {
            l = diff_l.process(l, self.diffuse);
            r = diff_r.process(r, self.diffuse);
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
        if self.sat {
            l = sat(l);
            r = sat(r);
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
pub struct Mooltitwap {
    params: Arc<PetalParams>,
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
    /// The loop tone's filters, one set per loop (main L/R, bounce odd/even),
    /// and the knob as it glides to the param (a unit per 20 ms).
    tone_main: LoopTone,
    tone_bounce: LoopTone,
    tone_t: f32,
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
    /// Per tap, the gains placing the repeats ([odd→L, odd→R, even→L,
    /// even→R]), gliding toward the targets set each block through two
    /// one-poles, so a glide starts and ends without a corner.
    ping_gain: [[f32; 4]; NUM_TAPS],
    ping_gain_mid: [[f32; 4]; NUM_TAPS],
    /// Mix's output blend and Diffuse's loop allpass, faded in and out (0..1)
    /// over 20 ms so changing the mode doesn't click.
    xf_mix: f32,
    xf_diffuse: f32,

    // Per-tap filters. Each tap has up to 4 cascaded SVFs for slope control
    // (1 stage = 12 dB/oct, 2 = 24, 4 = 48). The "filter slope" enum picks
    // how many stages to actually run per sample.
    tap_svf_l: [[Svf; 4]; NUM_TAPS],
    tap_svf_r: [[Svf; 4]; NUM_TAPS],

    // Crossfeed diffusion (ChowMatrix-style allpass cascade), one per channel.
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

impl Default for Mooltitwap {
    fn default() -> Self {
        let dummy_max = 192_000;
        Self {
            params: Arc::new(PetalParams::default()),
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
            diff_odd: Diffusion::new(),
            diff_even: Diffusion::new(),
            tape_bounce: TapeDegrade::new(48_000.0),
            tone_main: LoopTone::default(),
            tone_bounce: LoopTone::default(),
            tone_t: 0.0,
            ping_age: usize::MAX,
            ping_live: 0,
            ping_since_off: usize::MAX,
            ping_fresh: usize::MAX,
            ping_gain: [[0.0; 4]; NUM_TAPS],
            ping_gain_mid: [[0.0; 4]; NUM_TAPS],
            xf_mix: 0.0,
            xf_diffuse: 0.0,
            tap_svf_l: core::array::from_fn(|_| core::array::from_fn(|_| Svf::new())),
            tap_svf_r: core::array::from_fn(|_| core::array::from_fn(|_| Svf::new())),
            diff_l: Diffusion::new(),
            diff_r: Diffusion::new(),
            diff_wet_l: Diffusion::new(),
            diff_wet_r: Diffusion::new(),
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

impl Mooltitwap {
    /// Allocate everything for `sample_rate` (initialize's work; tests call it
    /// directly).
    fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let max_samples = (MAX_DELAY_SECONDS * self.sample_rate) as usize;
        self.main_l = ChannelTaps::new(max_samples);
        self.main_r = ChannelTaps::new(max_samples);
        self.ping_odd = DelayLine::new(max_samples);
        self.ping_even = DelayLine::new(max_samples);
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
    }
}

impl Plugin for Mooltitwap {
    // Display name (DAW plugin browser). Renamed to Afterecho (2026-07-11) —
    // "Mooltitwap"/"multitap" branding is retired. The struct/crate/folder are
    // still `mooltitwap` internally (a bigger rename, deferred); VST3_CLASS_ID
    // and CLAP_ID are UNCHANGED so existing sessions still recall the plugin.
    const NAME: &'static str = "Afterecho";
    const VENDOR: &'static str = "Emmanuel Bussien";
    const URL: &'static str = "https://example.com";
    const EMAIL: &'static str = "noreply@example.com";
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
        )
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        self.prepare(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        self.ping_odd.reset();
        self.ping_even.reset();
        self.ping_on = false;
        self.ping_leave = None;
        self.ping_share = 0.0;
        self.ping_valid = usize::MAX;
        self.main_quiet = 0;
        self.bounce_quiet = usize::MAX;
        self.diff_odd.reset();
        self.diff_even.reset();
        self.tape_bounce.reset();
        self.tone_main.reset();
        self.tone_bounce.reset();
        self.ping_live = 0;
        self.ping_since_off = usize::MAX;
        self.ping_age = usize::MAX;
        self.ping_fresh = usize::MAX;
        self.xf_mix = 0.0;
        self.xf_diffuse = 0.0;
        for st in self.tap_svf_l.iter_mut() { for s in st.iter_mut() { s.reset(); } }
        for st in self.tap_svf_r.iter_mut() { for s in st.iter_mut() { s.reset(); } }
        self.diff_l.reset();
        self.diff_r.reset();
        self.diff_wet_l.reset();
        self.diff_wet_r.reset();
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
        let base_sec_r = PetalParams::base_time_seconds_mode(p.sync_mode_r_eff(), time_r_norm, tempo_bpm);

        let n_active = (p.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

        // Topology — XY pad shape + spacing per channel
        let mode = p.spacing_mode.value();
        let shape_linked = p.shape_linked.value();
        self.topology_l.compute(mode, p.shape_x_l.value(), p.shape_y_l.value(), n_active);
        if shape_linked {
            self.topology_r.compute(mode, p.shape_x_l.value(), p.shape_y_l.value(), n_active);
        } else {
            self.topology_r.compute(mode, p.shape_x_r.value(), p.shape_y_r.value(), n_active);
        }

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

        // Per-tap filter configuration. Each tap's filter is a cascade of
        // 1-4 SVF stages (1=12 dB/oct, 2=24, 4=48). All stages get the same
        // cutoff/Q. Cascaded biquads do shift the -3dB point slightly with
        // each stage, but for a music plugin the perceptual difference is
        // exactly "steeper roll-off", which is what the slope control means.
        let mut tap_filter_types = [crate::params::FilterType::LowPass; NUM_TAPS];
        let mut tap_filter_stages = [1usize; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let tap = &p.taps[i];
            let cutoff = tap.filter_cutoff.value();
            let q = tap.filter_q.value();
            for st in 0..4 {
                self.tap_svf_l[i][st].set(cutoff, q, sr);
                self.tap_svf_r[i][st].set(cutoff, q, sr);
            }
            tap_filter_types[i] = tap.filter_type.value();
            tap_filter_stages[i] = match tap.filter_slope.value() {
                crate::params::FilterSlope::Db12 => 1,
                crate::params::FilterSlope::Db24 => 2,
                crate::params::FilterSlope::Db48 => 4,
            };
        }

        let input_gain = nice_plug::util::db_to_gain(p.input_gain.value());
        let output_gain = nice_plug::util::db_to_gain(p.output_gain.value());
        let dry_wet = p.dry_wet.value();
        let feedback = p.feedback.value();
        // (soft_clip param deprecated v2.21.0 — SATURATE routed to IN covers it)
        let diffuse_amount = p.diffuse_amount.value();
        let diffuse_routing = p.diffuse_routing.value();
        let diffuse_size = p.diffuse_size.value();
        // Push SIZE to the diffusion stages per block (allocation-free).
        self.diff_wet_l.set_size(diffuse_size);
        self.diff_wet_r.set_size(diffuse_size);

        // DEGRADE block — configured once per block (filter design happens in
        // set_params, control rate), inserted per sample at the selected
        // routing point. Gate on `amount` so the bypass path costs nothing.
        let degrade_amount = p.degrade_amount.value();
        let degrade_routing = p.degrade_routing.value();
        let degrade_on = degrade_amount > 1e-4;
        if degrade_on {
            let tape = TapeDegradeParams {
                kind: match p.degrade_type.value() {
                    TapeType::I => TapeKind::I,
                    TapeType::II => TapeKind::II,
                    TapeType::IV => TapeKind::IV,
                },
                drive_db: p.degrade_drive.value(),
                wear: p.degrade_wear.value(),
                mix: degrade_amount,
            };
            self.tape.set_params(&tape);
            self.tape_bounce.set_params(&tape);
        }
        // SATURATE block — playable tanh drive stage (the loop's fixed safety
        // tanh is separate and always on). Same routing grammar as DIFFUSE.
        let sat_amount = p.sat_amount.value();
        let sat_routing = p.sat_routing.value();
        let sat_gain = nice_plug::util::db_to_gain(p.sat_drive.value());
        let sat_on = sat_amount > 1e-4;
        // out = dry·(1−mix) + tanh(dry·gain)·mix — no gain compensation
        // (project rule: never gain-compensate the loop).
        let sat = move |x: f32| -> f32 {
            x * (1.0 - sat_amount) + soft_clip(x * sat_gain) * sat_amount
        };

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
            topology::cluster_span_secs(&t_secs_l, &t_secs_r, &tap_in_range, n_active) * sr;
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
        let crossfeed = p.crossfeed.value();
        let crossfeed_mode = p.crossfeed_mode.value();

        let mut tap_gain = [0.0f32; NUM_TAPS];
        let mut tap_pan_l = [1.0f32; NUM_TAPS];
        let mut tap_pan_r = [1.0f32; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;
            tap_gain[i] = if active {
                nice_plug::util::db_to_gain(p.taps[i].gain_db.value())
            } else { 0.0 };
            (tap_pan_l[i], tap_pan_r[i]) = bounce::pan_gains(p.taps[i].pan.value());
        }

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
        let width = bounce::width(ch_mode, crossfeed);
        let swinging = self.ping_on && self.ping_leave.is_none();
        let mut ping_target = [[0.0f32; 4]; NUM_TAPS];
        let mut ping_tap_fresh = [false; NUM_TAPS];
        let mut ping_play_out = self.fb_time_slew.current().max(fb_time_target) as usize;
        for i in 0..NUM_TAPS {
            let reach = self.main_l.readers[i].reach().max(self.main_r.readers[i].reach()) as usize + 4;
            // a tap swings once the first bounced repeat has reached it
            let w = if swinging && self.ping_age >= reach { width } else { 0.0 };
            ping_target[i] = bounce::placement(ch_mode, p.taps[i].pan.value(), w);
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
        if diffuse_target > 0.0 && self.xf_diffuse == 0.0 {
            // fading in from nothing: no stale tail from the last time
            self.diff_l.reset();
            self.diff_r.reset();
        }
        if initial_snap {
            self.xf_mix = mix_target;
            self.xf_diffuse = diffuse_target;
        }

        // The loop tone glides to the knob, a unit per 20 ms.
        let tone_target = p.loop_tone.value();
        if initial_snap {
            self.tone_t = tone_target;
        }
        let tone_step = 1.0 / (0.02 * sr);

        // What the feedback path does to each pass, in both loops. Diffuse,
        // Tape and the tone have memory: a loop that stops getting input
        // keeps running this long so they ring out (the tone's longest tail
        // is its high-pass's, 8 ms at 20 Hz).
        let mut colour = LoopColour {
            diffuse: if diffuse_routing == DiffuseRouting::Feedback { diffuse_amount } else { 0.0 },
            tape: degrade_on && degrade_routing == DiffuseRouting::Feedback,
            sat: sat_on && sat_routing == DiffuseRouting::Feedback,
            tone: ToneCoefs::new(self.tone_t, sr),
        };
        let ring_len = if colour.diffuse > 1e-4 || colour.tape {
            (3.0 * sr) as usize
        } else if tone_target != 0.0 || self.tone_t != 0.0 {
            (0.1 * sr) as usize
        } else {
            1
        };
        // Ping's lines are read (by the loop and the taps) while they may hold anything.
        let ping_read = self.ping_on || self.ping_share > 0.0 || self.ping_live > 0;

        let chans = buffer.as_slice();
        let n_samples = chans[0].len();
        let (l_chan, rest) = chans.split_at_mut(1);
        let l_chan = &mut l_chan[0];
        let r_chan = &mut rest[0];

        for s in 0..n_samples {
            let mut in_l = l_chan[s];
            let mut in_r = r_chan[s];

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
            if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Input {
                x_l = self.diff_wet_l.process(x_l, diffuse_amount);
                x_r = self.diff_wet_r.process(x_r, diffuse_amount);
            }

            // DEGRADE routing = Input: crush the dry input before the buffer —
            // every tap reads pre-degraded material (single lo-fi generation).
            if degrade_on && degrade_routing == DiffuseRouting::Input {
                self.tape.process(&mut x_l, &mut x_r);
            }

            // SATURATE routing = Input: drive the dry input into the buffer.
            if sat_on && sat_routing == DiffuseRouting::Input {
                x_l = sat(x_l);
                x_r = sat(x_r);
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
            let cf = crossfeed.min(1.0);
            let fb_samps = self.fb_time_slew.tick();
            // Read BEFORE write so we sample the buffer state from cluster_span
            // ago, not the value we're about to write.
            let fb_at = fb_samps.max(2.0);
            let raw_fb_l = self.main_l.delay.read(fb_at);
            let raw_fb_r = self.main_r.delay.read(fb_at);
            // Ping's lines coming back (see bounce.rs): the odd repeats (bound
            // left) and the even ones (bound right).
            let (odd, even) = if ping_read && fb_at <= self.ping_valid as f32 {
                (self.ping_odd.read(fb_at), self.ping_even.read(fb_at))
            } else {
                (0.0, 0.0)
            };
            // Two loops share what goes round, by the bounce's share `b`. The
            // main loop takes the main lines, plus the bounce's last pass
            // (centred) once Ping hands it back. The bounce takes the main
            // lines' mono sum (the dry input) as a new odd repeat, an even
            // repeat back as the next odd one, an odd one as the next even.
            let b = s_curve(self.ping_share);
            let (tl, tr) = bounce::mono_to(ch_mode, odd + even);
            let main_l = (1.0 - b) * (raw_fb_l + tl);
            let main_r = (1.0 - b) * (raw_fb_r + tr);
            let odd_in = b * (bounce::mono_of(ch_mode, raw_fb_l, raw_fb_r) + even);
            let even_in = b * odd;

            // FB CURVE — bend the effective loop gain by level so the decay
            // envelope isn't limited to the natural exponential. The envelope
            // tracks the loop signal (pre-gain), linked across channels.
            let fb_gain = match fb_curve {
                FbCurve::Natural => feedback,
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
                            feedback * bend.clamp(0.5, 1.15)
                        }
                        // Expander-in-the-loop: full feedback while the tail
                        // is above the gate threshold, accelerating cut below
                        // it. FEEDBACK sets how long the hold lasts.
                        FbCurve::Gate => {
                            const T: f32 = 0.1;
                            if self.fb_env >= T { feedback }
                            else {
                                let k = self.fb_env / T;
                                feedback * k * k
                            }
                        }
                        FbCurve::Natural => unreachable!(),
                    }
                }
            };

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
                    if colour.diffuse > 1e-4 {
                        self.diff_wet_l.reset();
                        self.diff_wet_r.reset();
                    }
                    if colour.tape {
                        self.tape.reset();
                    }
                    self.tone_main.reset();
                }
                let mut fb_in_l = main_l * fb_gain;
                let mut fb_in_r = main_r * fb_gain;

                // Crossfeed Diffuse acts on the FEEDBACK path. It fades in and
                // out with the mode, at its input too, so the allpass tail starts
                // and stops smoothly. Mix acts on the output (after the tap
                // loop); Ping is the bounce loop.
                let w = s_curve(self.xf_diffuse);
                if cf > 1e-4 && w > 0.0 {
                    let dl = self.diff_l.process(fb_in_l * w, 1.0);
                    let dr = self.diff_r.process(fb_in_r * w, 1.0);
                    fb_in_l = fb_in_l * (1.0 - cf * w) + dl * cf * w;
                    fb_in_r = fb_in_r * (1.0 - cf * w) + dr * cf * w;
                }
                (fb_sat_l, fb_sat_r) = colour.apply(
                    fb_in_l,
                    fb_in_r,
                    &mut self.diff_wet_l,
                    &mut self.diff_wet_r,
                    &mut self.tape,
                    &mut self.tone_main,
                    &sat,
                );
            }

            // The bounce loop, the same way, with its own Diffuse, Tape and tone.
            let (mut odd_out, mut even_out) = (0.0, 0.0);
            if b > 0.0 || self.bounce_quiet < ring_len {
                if self.bounce_quiet >= ring_len {
                    self.diff_odd.reset();
                    self.diff_even.reset();
                    self.tape_bounce.reset();
                    self.tone_bounce.reset();
                }
                (odd_out, even_out) = colour.apply(
                    odd_in * fb_gain,
                    even_in * fb_gain,
                    &mut self.diff_odd,
                    &mut self.diff_even,
                    &mut self.tape_bounce,
                    &mut self.tone_bounce,
                    &sat,
                );
            }

            // Write the dry input + processed feedback into the main delay.
            // The taps below read from this buffer; each cluster_span the same
            // cluster pattern appears in the buffer, attenuated by feedback^k.
            // In Ping the repeats go to the bounce lines instead.
            self.main_l.delay.write(x_l + fb_sat_l);
            self.main_r.delay.write(x_r + fb_sat_r);
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
                // Panned ahead of the filter (the same sound: the filter is
                // linear), so Ping's repeats can join each side before it.
                let mut sl = hl.read(&self.main_l.delay) * tap_pan_l[i];
                let mut sr_s = hr.read(&self.main_r.delay) * tap_pan_r[i];
                if ping_read && ping_tap_fresh[i] {
                    let g = &mut self.ping_gain[i];
                    for ((g, m), t) in g.iter_mut().zip(&mut self.ping_gain_mid[i]).zip(ping_target[i]) {
                        *m += (t - *m) * ping_glide;
                        *g += (*m - *g) * ping_glide;
                    }
                    let (odd_l, even_l) = (hl.read(&self.ping_odd), hl.read(&self.ping_even));
                    let (odd_r, even_r) = if hr == hl {
                        (odd_l, even_l)
                    } else {
                        (hr.read(&self.ping_odd), hr.read(&self.ping_even))
                    };
                    sl += odd_l * g[0] + even_l * g[2];
                    sr_s += odd_r * g[1] + even_r * g[3];
                }

                let stages = tap_filter_stages[i];
                for st in 0..stages {
                    sl = self.tap_svf_l[i][st].process(sl, tap_filter_types[i]);
                    sr_s = self.tap_svf_r[i][st].process(sr_s, tap_filter_types[i]);
                }

                tap_out_l += sl * tap_gain[i];
                tap_out_r += sr_s * tap_gain[i];
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
            let mut wet_l = if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Output {
                self.diff_wet_l.process(tap_out_l, diffuse_amount)
            } else { tap_out_l };
            let mut wet_r = if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Output {
                self.diff_wet_r.process(tap_out_r, diffuse_amount)
            } else { tap_out_r };

            // DEGRADE routing = Output: crush the whole wet mix once (after
            // diffusion so the bloom degrades too), before dry/wet.
            if degrade_on && degrade_routing == DiffuseRouting::Output {
                self.tape.process(&mut wet_l, &mut wet_r);
            }

            // SATURATE routing = Output: drive the wet mix once, before dry/wet.
            if sat_on && sat_routing == DiffuseRouting::Output {
                wet_l = sat(wet_l);
                wet_r = sat(wet_r);
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

            l_chan[s] = out_l;
            r_chan[s] = out_r;

            self.xf_mix += (mix_target - self.xf_mix).clamp(-xf_step, xf_step);
            self.xf_diffuse += (diffuse_target - self.xf_diffuse).clamp(-xf_step, xf_step);
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

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Mooltitwap {
    const CLAP_ID: &'static str = "com.bussien.mooltitwap";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Afterecho — delay garden");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Delay];
}

impl Vst3Plugin for Mooltitwap {
    const VST3_CLASS_ID: [u8; 16] = *b"MooltitwapMultiD";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay, Vst3SubCategory::Stereo];
}

// AUv3 export is iOS-only (only the iOS target enables nice-plug's `auv3`
// feature — see Cargo.toml's target-cfg deps). Gating on target_os keeps the
// desktop clap/vst3/standalone build, which has no `auv3`, compiling.
#[cfg(target_os = "ios")]
impl AuPlugin for Mooltitwap {
    // Hosts key saved sessions on (type, subtype, manufacturer). Pick once,
    // never change. AU_TYPE auto-derives to `aufx` (audio effect).
    // MUST match bundler.toml's au_subtype / au_manufacturer.
    const AU_SUBTYPE: [u8; 4] = *b"Aftr"; // Afterecho
    const AU_MANUFACTURER: [u8; 4] = *b"Busn"; // Bussien (not all-lowercase — Apple reserves those)
}

nice_export_clap!(Mooltitwap);
nice_export_vst3!(Mooltitwap);
#[cfg(target_os = "ios")]
nice_export_auv3!(Mooltitwap);

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    const SR: f32 = 48_000.0;

    /// A host that only tells the time.
    struct Host(Transport);

    impl ProcessContext<Mooltitwap> for Host {
        fn plugin_api(&self) -> PluginApi { PluginApi::Clap }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport { &self.0 }
        fn next_event(&mut self) -> Option<PluginNoteEvent<Mooltitwap>> { None }
        fn send_event(&mut self, _event: PluginNoteEvent<Mooltitwap>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn set(plug: &Mooltitwap, id: &str, plain: f32) {
        let (_, ptr, _) = plug.params.param_map().into_iter().find(|(i, _, _)| i == id)
            .unwrap_or_else(|| panic!("no param {id}"));
        // SAFETY: ptr belongs to plug's params, which outlive the call.
        unsafe { ptr._internal_set_normalized_value(ptr.preview_normalized(plain)) };
    }

    /// Wet only, one tap per pan, 20 ms apart; the cycle follows the cluster.
    fn plugin(pans: &[f32]) -> Mooltitwap {
        let mut plug = Mooltitwap::default();
        plug.prepare(SR);
        plug.reset();
        set(&plug, "dry_wet", 1.0);
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
    fn render(plug: &mut Mooltitwap, input: &[(f32, f32)], mut at: impl FnMut(usize, &Mooltitwap)) -> Vec<(f32, f32)> {
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
}
