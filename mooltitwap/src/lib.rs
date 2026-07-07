mod delay;
mod diffusion;
mod filter;
mod gui;
mod params;
mod topology;

use nih_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::delay::{ChannelTaps, SlewedTime};
use crate::diffusion::Diffusion;
use crate::filter::{soft_clip, PeakLimiter, Svf};
use crate::params::{ChannelMode, DelayMode, DiffuseRouting, PetalParams, NUM_TAPS};
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

    /// Nominal base delay (samples) from the previous process block.
    /// Used to detect sync-division / tempo changes between blocks so we can
    /// snap immediately instead of letting SlewedTime drift at 0.02 samp/samp.
    prev_base_samples_l: f32,
    prev_base_samples_r: f32,
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

    // LFO
    lfo_phase: f32,
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
            fb_time_slew: SlewedTime::new(),
            needs_initial_snap: true,
            prev_base_samples_l: 0.0,
            prev_base_samples_r: 0.0,
            prev_n_active: 0,
            prev_tap_active: [false; NUM_TAPS],
            lim_l: PeakLimiter::new(48_000.0),
            lim_r: PeakLimiter::new(48_000.0),
            lfo_phase: 0.0,
        }
    }
}

impl Plugin for Mooltitwap {
    const NAME: &'static str = "Mooltitwap";
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
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        for st in self.tap_svf_l.iter_mut() { for s in st.iter_mut() { s.reset(); } }
        for st in self.tap_svf_r.iter_mut() { for s in st.iter_mut() { s.reset(); } }
        self.diff_wet_l.reset();
        self.diff_wet_r.reset();
        self.fb_time_slew.snap_to(0.0);
        self.lfo_phase = 0.0;
        // Tell process() to snap (not slew) the tap delays on the next frame.
        // Otherwise the slow tape-bend slew would take minutes to reach long
        // nominal delays (e.g. 2/1 sync at slow tempo), producing audible
        // pitch drift and a near-zero-delay tap stack in the meantime.
        self.needs_initial_snap = true;
        // Force the sync-change detector to fire on the next block too.
        self.prev_base_samples_l = 0.0;
        self.prev_base_samples_r = 0.0;
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
        let base_sec_r = p.base_time_seconds(time_r_norm, tempo_bpm);
        let base_samples_l = base_sec_l * sr;
        let base_samples_r = base_sec_r * sr;

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
        let bend_slew_rate = 1.0 / 50.0;
        // Out-of-range threshold: anything beyond this gets silenced, not
        // clamped, so over-budget taps don't stack at the buffer edge.
        // We use 95% of the configured max so we never even approach the
        // wrap boundary of the ring buffer.
        let max_delay_samples = MAX_DELAY_SECONDS * sr * 0.95;
        let delay_mode = p.delay_mode.value();
        let mut nom_l = [0.0f32; NUM_TAPS];
        let mut nom_r = [0.0f32; NUM_TAPS];
        let mut tap_in_range = [false; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;

            let span_l = NUM_TAPS as f32 * base_samples_l;
            let span_r = NUM_TAPS as f32 * base_samples_r;
            let off = p.taps[i].time_offset.value() * span_l;
            let off_r = p.taps[i].time_offset.value() * span_r;

            nom_l[i] = (pos_l[i] * span_l + off).max(2.0);
            nom_r[i] = (pos_r[i] * span_r + off_r).max(2.0);
            self.main_l.readers[i].set_nominal_slew_rate(bend_slew_rate);
            self.main_r.readers[i].set_nominal_slew_rate(bend_slew_rate);
            // Update the per-tap transition mode (Repitch / Fade / Jump).
            // set_mode re-aligns internal state to the current target so the
            // switch is silent.
            self.main_l.readers[i].set_mode(delay_mode);
            self.main_r.readers[i].set_mode(delay_mode);

            // Mark in-range: only taps whose delay fits in the buffer get
            // to play. Out-of-range taps would otherwise be clamped by
            // DelayLine::read to the buffer boundary, causing them all to
            // read the same sample → loud saturated stack of overlapping
            // taps at the boundary.
            tap_in_range[i] = active
                && nom_l[i] <= max_delay_samples
                && nom_r[i] <= max_delay_samples;
        }

        // ── Snap guards ──────────────────────────────────────────────────
        // The tape-bend slew (0.02 samp/samp) is intentional for pitch-bend
        // transitions, but catastrophic when the whole sync grid shifts (e.g.
        // 1/8 → 1/16): taps have to travel thousands of samples at that rate,
        // producing loud Doppler ringing for up to 12+ seconds.
        //
        // Guard 1 — sync/tempo change: if base_samples changed since the last
        // block, snap all taps immediately. base_samples encodes tempo × sync-
        // division but not per-tap pitch, so a change here can only mean the
        // user changed the sync division or the DAW tempo jumped.
        //
        // Guard 2 — newly-activated tap: when a tap goes from inactive to
        // active its reader hasn't been tracking the real nominal and starts
        // from wherever it last was (usually 1.0 after reset). Snap it.
        //
        // Guard 3 — initial snap after reset (same as before).
        let base_changed = (base_samples_l - self.prev_base_samples_l).abs() > 0.5
            || (base_samples_r - self.prev_base_samples_r).abs() > 0.5;
        // n_active change re-anchors topology: every intermediate tap's
        // nominal delay shifts (potentially by hundreds of samples). If we let
        // SlewedTime glide that, the reads sweep across the buffer and we
        // hear a pitch sweep — the "mangled / repitched" artifact the user
        // reported. Detect and snap.
        let n_active_changed = self.prev_n_active != n_active;
        let topology_jump = base_changed || n_active_changed;

        for i in 0..NUM_TAPS {
            let active_now = tap_in_range[i] && i < n_active;
            let newly_active = active_now && !self.prev_tap_active[i];
            if self.needs_initial_snap || topology_jump || newly_active {
                self.main_l.readers[i].snap_to(nom_l[i].max(2.0));
                self.main_r.readers[i].snap_to(nom_r[i].max(2.0));
            }
            self.prev_tap_active[i] = active_now;
        }

        if base_changed {
            self.prev_base_samples_l = base_samples_l;
            self.prev_base_samples_r = base_samples_r;
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

        let lfo_rate = p.wobble_rate.value();
        let wobble_amt = p.wobble_amount.value();
        let lfo_inc = lfo_rate / sr;

        let input_gain = nih_plug::util::db_to_gain(p.input_gain.value());
        let output_gain = nih_plug::util::db_to_gain(p.output_gain.value());
        let dry_wet = p.dry_wet.value();
        let feedback = p.feedback.value();
        let soft = p.soft_clip.value();
        let diffuse_amount = p.diffuse_amount.value();
        let diffuse_routing = p.diffuse_routing.value();
        let diffuse_size = p.diffuse_size.value();
        // Push SIZE to the diffusion stages per block (allocation-free).
        self.diff_wet_l.set_size(diffuse_size);
        self.diff_wet_r.set_size(diffuse_size);
        let ch_mode = p.channel_mode.value();

        // Feedback delay time.
        // When LOCKED: FB TIME = the cluster span (delay of the last active tap),
        //   so the full cluster echoes as a unit after it finishes playing.
        // When UNLOCKED: FB TIME = the independent feedback_time param.
        // Spiral protection: clamp minimum to 10 ms regardless of mode so a very
        //   short FB TIME can't cause exponential runaway by cycling faster than
        //   the cluster itself decays (sounds like a detuned squeal).
        let min_fb_samples = 0.010 * sr; // 10 ms hard floor
        // Cluster span = max delay across all active in-range taps (includes time_offset).
        // Used both as the LOCKED target AND as a floor for the UNLOCKED target — so
        // feedback NEVER lands before any of the direct taps (which would create the
        // ugly "feedback racing the taps" wash the user described).
        let cluster_span_samples = (0..n_active)
            .filter(|&i| tap_in_range[i])
            .map(|i| nom_l[i].max(nom_r[i]))
            .fold(min_fb_samples, f32::max);
        let fb_time_target = if p.feedback_time_locked.value() {
            cluster_span_samples
        } else {
            let param_target = (p.feedback_time_seconds(tempo_bpm) * sr)
                .clamp(min_fb_samples, max_delay_samples);
            // Floor at cluster_span: UNLOCKED can only push the echo LATER than
            // the last tap, never earlier.
            param_target.max(cluster_span_samples)
        };
        let fb_time_target = fb_time_target.max(min_fb_samples);
        // Snap the feedback read-position on topology jumps (tap count or
        // base-time change) so the feedback doesn't sweep across the buffer
        // — that sweep is what produced the residual "pitch impact from
        // changing feedback knob" tail the user was hearing (when LOCKED,
        // every knob touch that affected n_active or tempo re-targeted the
        // fb read by hundreds of samples).
        if topology_jump || self.needs_initial_snap {
            self.fb_time_slew.snap_to(fb_time_target);
        } else {
            self.fb_time_slew.set_rate(1.0 / 50.0);
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
                nih_plug::util::db_to_gain(p.taps[i].gain_db.value())
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
            if soft {
                x_l = soft_clip(x_l);
                x_r = soft_clip(x_r);
            }

            // DIFFUSE routing = Input: smear the dry input BEFORE it hits the delay.
            // Every tap then reads pre-smeared material, giving a layered feel.
            if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Input {
                x_l = self.diff_wet_l.process(x_l, diffuse_amount);
                x_r = self.diff_wet_r.process(x_r, diffuse_amount);
            }

            // Wobble — gentler: ±5% at max amount, scaled by amount squared for taper.
            let lfo = (self.lfo_phase * core::f32::consts::TAU).sin();
            self.lfo_phase += lfo_inc;
            if self.lfo_phase >= 1.0 { self.lfo_phase -= 1.0; }
            let mod_scale = 1.0 + lfo * wobble_amt * wobble_amt * 0.05;

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

            let mut fb_in_l = raw_fb_l * feedback;
            let mut fb_in_r = raw_fb_r * feedback;

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

            // Write the dry input + processed feedback into the main delay.
            // The taps below read from this buffer; each cluster_span the same
            // cluster pattern appears in the buffer, attenuated by feedback^k.
            self.main_l.delay.write(x_l + fb_in_l);
            self.main_r.delay.write(x_r + fb_in_r);

            // In Fade mode, wobble is bypassed: its constant per-sample
            // mod_scale change would re-trigger fades every sample, mangling
            // the audio. Repitch & Jump can use it normally.
            let wobble_mod = if matches!(delay_mode, DelayMode::Fade) { 1.0 } else { mod_scale };

            let mut tap_out_l = 0.0;
            let mut tap_out_r = 0.0;
            for i in 0..n_active {
                if !tap_in_range[i] { continue; }
                self.main_l.readers[i].set_nominal((nom_l[i] * wobble_mod).max(2.0));
                self.main_r.readers[i].set_nominal((nom_r[i] * wobble_mod).max(2.0));
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
            let wet_l = if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Output {
                self.diff_wet_l.process(tap_out_l, diffuse_amount)
            } else { tap_out_l };
            let wet_r = if diffuse_amount > 1e-4 && diffuse_routing == DiffuseRouting::Output {
                self.diff_wet_r.process(tap_out_r, diffuse_amount)
            } else { tap_out_r };

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
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Multitap delay garden");
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

nih_export_clap!(Mooltitwap);
nih_export_vst3!(Mooltitwap);
