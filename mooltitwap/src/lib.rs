mod delay;
mod diffusion;
mod filter;
mod gui;
mod gui_scope;
mod gui_v3;
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

use crate::delay::{ChannelTaps, SlewedTime};
use crate::diffusion::Diffusion;
use crate::filter::{soft_clip, PeakLimiter, Svf};
use crate::params::{ChannelMode, DelayMode, DiffuseRouting, FbCurve, PetalParams, TapeType, NUM_TAPS};
use crate::tape::{TapeDegrade, TapeDegradeParams, TapeKind};
use crate::topology::TopologyEngine;

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
        self.sample_rate = buffer_config.sample_rate;
        let max_samples = (MAX_DELAY_SECONDS * self.sample_rate) as usize;
        self.main_l = ChannelTaps::new(max_samples);
        self.main_r = ChannelTaps::new(max_samples);
        self.lim_l = PeakLimiter::new(self.sample_rate);
        self.lim_r = PeakLimiter::new(self.sample_rate);
        self.diff_l.prepare(self.sample_rate);
        self.diff_r.prepare(self.sample_rate);
        self.diff_wet_l.prepare(self.sample_rate);
        self.diff_wet_r.prepare(self.sample_rate);
        self.diff_l.reset();
        self.diff_r.reset();
        self.diff_wet_l.reset();
        self.diff_wet_r.reset();
        // Rebuild at the real sample rate — runs the ~10k-iteration MagParams
        // calibration, which is why it happens here and not per block.
        self.tape = TapeDegrade::new(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        for st in self.tap_svf_l.iter_mut() { for s in st.iter_mut() { s.reset(); } }
        for st in self.tap_svf_r.iter_mut() { for s in st.iter_mut() { s.reset(); } }
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
            self.tape.set_params(&TapeDegradeParams {
                kind: match p.degrade_type.value() {
                    TapeType::I => TapeKind::I,
                    TapeType::II => TapeKind::II,
                    TapeType::IV => TapeKind::IV,
                },
                drive_db: p.degrade_drive.value(),
                wear: p.degrade_wear.value(),
                mix: degrade_amount,
            });
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
            let pan = p.taps[i].pan.value();
            let angle = (pan + 1.0) * 0.25 * core::f32::consts::PI;
            tap_pan_l[i] = angle.cos();
            tap_pan_r[i] = angle.sin();
        }

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
            let raw_fb_l = self.main_l.delay.read(fb_samps.max(2.0));
            let raw_fb_r = self.main_r.delay.read(fb_samps.max(2.0));

            // FB CURVE — bend the effective loop gain by level so the decay
            // envelope isn't limited to the natural exponential. The envelope
            // tracks the RAW buffer signal (pre-gain), linked across channels.
            let fb_gain = match fb_curve {
                FbCurve::Natural => feedback,
                _ => {
                    let mag = raw_fb_l.abs().max(raw_fb_r.abs());
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
            let mut fb_in_l = raw_fb_l * fb_gain;
            let mut fb_in_r = raw_fb_r * fb_gain;

            // Crossfeed Ping-Pong & Diffuse act on the FEEDBACK path.
            // Mix acts on the output and is handled after the tap loop.
            if cf > 1e-4 {
                use crate::params::CrossfeedMode;
                match crossfeed_mode {
                    CrossfeedMode::Mix => { /* handled post-tap */ }
                    CrossfeedMode::PingPong => {
                        let kept = 1.0 - cf;
                        let new_fb_l = fb_in_l * kept + fb_in_r * cf;
                        let new_fb_r = fb_in_r * kept + fb_in_l * cf;
                        fb_in_l = new_fb_l;
                        fb_in_r = new_fb_r;
                    }
                    CrossfeedMode::Diffuse => {
                        fb_in_l = self.diff_l.process(fb_in_l, cf);
                        fb_in_r = self.diff_r.process(fb_in_r, cf);
                    }
                }
            }

            // DIFFUSE routing = Feedback: smear the recirculating signal so
            // each cluster repeat picks up more allpass density.
            if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Feedback {
                fb_in_l = self.diff_wet_l.process(fb_in_l, diffuse_amount);
                fb_in_r = self.diff_wet_r.process(fb_in_r, diffuse_amount);
            }

            // DEGRADE routing = Feedback: generation loss — every repeat gets
            // re-crushed, like tape/BBD copies of copies. Before the loop
            // saturator so the quantization grit stays bounded.
            if degrade_on && degrade_routing == DiffuseRouting::Feedback {
                self.tape.process(&mut fb_in_l, &mut fb_in_r);
            }

            // SATURATE routing = Feedback: drive the recirculating signal —
            // each repeat picks up another pass of harmonics (tape-loop
            // buildup). Sits before the fixed safety tanh.
            if sat_on && sat_routing == DiffuseRouting::Feedback {
                fb_in_l = sat(fb_in_l);
                fb_in_r = sat(fb_in_r);
            }

            // In-loop soft saturation (v2.18.2) — the ONLY thing bounding the
            // loop; there is deliberately NO gain compensation (project rule:
            // feedback paths must be allowed to self-oscillate). Headroomed
            // tanh: ~transparent at normal levels (−0.2 dB at 0.5 amplitude),
            // ceiling ≈ 2.0. With feedback ≥ 1.0 the loop settles into
            // bounded, tape-style self-oscillation instead of exponential
            // runaway — which is what used to detonate at short FB times.
            let fb_sat_l = 2.0 * soft_clip(fb_in_l * 0.5);
            let fb_sat_r = 2.0 * soft_clip(fb_in_r * 0.5);

            // Write the dry input + processed feedback into the main delay.
            // The taps below read from this buffer; each cluster_span the same
            // cluster pattern appears in the buffer, attenuated by feedback^k.
            self.main_l.delay.write(x_l + fb_sat_l);
            self.main_r.delay.write(x_r + fb_sat_r);

            let mut tap_out_l = 0.0;
            let mut tap_out_r = 0.0;
            for i in 0..n_active {
                if !tap_in_range[i] { continue; }
                let mut sl = self.main_l.readers[i].read(&self.main_l.delay);
                let mut sr_s = self.main_r.readers[i].read(&self.main_r.delay);

                let stages = tap_filter_stages[i];
                for st in 0..stages {
                    sl = self.tap_svf_l[i][st].process(sl, tap_filter_types[i]);
                    sr_s = self.tap_svf_r[i][st].process(sr_s, tap_filter_types[i]);
                }

                let gl = sl * tap_gain[i];
                let gr = sr_s * tap_gain[i];
                tap_out_l += gl * tap_pan_l[i];
                tap_out_r += gr * tap_pan_r[i];
            }

            // Crossfeed Mix — post-tap stereo blend on the OUTPUT (not feedback).
            if cf > 1e-4 && matches!(crossfeed_mode, crate::params::CrossfeedMode::Mix) {
                let kept = 1.0 - cf * 0.5;
                let sent = cf * 0.5;
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
