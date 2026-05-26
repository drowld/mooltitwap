mod delay;
mod filter;
mod gui;
mod params;
mod topology;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::delay::{ChannelTaps, DelayLine};
use crate::filter::{soft_clip, PeakLimiter, Svf};
use crate::params::{ChannelMode, PetalParams, NUM_TAPS};
use crate::topology::TopologyEngine;

const MAX_DELAY_SECONDS: f32 = 16.0;

/// Simple autocorrelation pitch tracker. Maintains a ring buffer of recent
/// input samples and re-estimates the dominant pitch period every
/// `ESTIMATE_INTERVAL` samples. Used to feed the PSOLA shifter.
struct PitchTracker {
    buf: Vec<f32>,
    write_idx: usize,
    counter: usize,
    period: f32, // samples; 0 = unpitched
    sample_rate: f32,
}

const PITCH_BUF_LEN: usize = 4096;
const PITCH_ESTIMATE_INTERVAL: usize = 1024;

impl PitchTracker {
    fn new(sample_rate: f32) -> Self {
        Self {
            buf: vec![0.0; PITCH_BUF_LEN],
            write_idx: 0,
            counter: 0,
            period: 0.0,
            sample_rate,
        }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.buf[self.write_idx] = x;
        self.write_idx = (self.write_idx + 1) % PITCH_BUF_LEN;
        self.counter += 1;
        if self.counter >= PITCH_ESTIMATE_INTERVAL {
            self.counter = 0;
            self.estimate();
        }
    }

    fn estimate(&mut self) {
        let n = self.buf.len();
        // Linearize ring buffer into a temporary
        let mut lin = [0.0_f32; PITCH_BUF_LEN];
        for i in 0..n { lin[i] = self.buf[(self.write_idx + i) % n]; }

        // Skip silent buffers — keep previous estimate
        let mut e0 = 0.0_f32;
        for s in lin.iter() { e0 += s * s; }
        if e0 < 1e-3 {
            // very quiet — reset to "unknown" so PSOLA falls back to Hann
            self.period = 0.0;
            return;
        }

        // Search range: 60 Hz .. 1500 Hz (covers most musical content)
        let min_lag = ((self.sample_rate / 1500.0) as usize).max(16);
        let max_lag = ((self.sample_rate / 60.0) as usize).min(n / 2);

        let mut best_lag = 0usize;
        let mut best_norm = 0.0_f32;
        for lag in min_lag..=max_lag {
            let m = n - lag;
            let mut corr = 0.0_f32;
            let mut e_lag = 0.0_f32;
            for i in 0..m {
                let a = lin[i];
                let b = lin[i + lag];
                corr += a * b;
                e_lag += b * b;
            }
            let denom = (e0 * e_lag).sqrt().max(1e-9);
            let norm = corr / denom;
            if norm > best_norm {
                best_norm = norm;
                best_lag = lag;
            }
        }

        if best_norm > 0.35 && best_lag > 0 {
            // Smooth toward new estimate
            let new_p = best_lag as f32;
            if self.period < 1.0 { self.period = new_p; }
            else { self.period = self.period * 0.5 + new_p * 0.5; }
        } else {
            self.period = 0.0;
        }
    }

    #[inline]
    fn period(&self) -> f32 { self.period }
}

pub struct Mooltitwap {
    params: Arc<PetalParams>,
    editor_state: Arc<EguiState>,
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

    // Feedback delay (separate, as Petal describes)
    fb_l: DelayLine,
    fb_r: DelayLine,
    fb_state_l: f32,
    fb_state_r: f32,

    // Per-tap filters (one SVF per tap, per channel)
    tap_svf_l: [Svf; NUM_TAPS],
    tap_svf_r: [Svf; NUM_TAPS],

    // Pitch trackers for the PSOLA shifter (one per channel)
    pitch_l: PitchTracker,
    pitch_r: PitchTracker,

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
            editor_state: gui::default_state(),
            tempo_bpm_shared: Arc::new(AtomicU32::new(120.0_f32.to_bits())),
            sample_rate: 48_000.0,
            topology_l: TopologyEngine::new(),
            topology_r: TopologyEngine::new(),
            main_l: ChannelTaps::new(dummy_max),
            main_r: ChannelTaps::new(dummy_max),
            fb_l: DelayLine::new(dummy_max),
            fb_r: DelayLine::new(dummy_max),
            fb_state_l: 0.0,
            fb_state_r: 0.0,
            tap_svf_l: core::array::from_fn(|_| Svf::new()),
            tap_svf_r: core::array::from_fn(|_| Svf::new()),
            pitch_l: PitchTracker::new(48_000.0),
            pitch_r: PitchTracker::new(48_000.0),
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
            self.editor_state.clone(),
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
        self.fb_l = DelayLine::new(max_samples);
        self.fb_r = DelayLine::new(max_samples);
        self.lim_l = PeakLimiter::new(self.sample_rate);
        self.lim_r = PeakLimiter::new(self.sample_rate);
        self.pitch_l = PitchTracker::new(self.sample_rate);
        self.pitch_r = PitchTracker::new(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        self.fb_l.reset();
        self.fb_r.reset();
        self.fb_state_l = 0.0;
        self.fb_state_r = 0.0;
        for s in self.tap_svf_l.iter_mut() { s.reset(); }
        for s in self.tap_svf_r.iter_mut() { s.reset(); }
        self.lfo_phase = 0.0;
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

        // Topology — XY pad shape + spacing per channel
        let mode = p.spacing_mode.value();
        let shape_linked = p.shape_linked.value();
        self.topology_l.compute(mode, p.shape_x_l.value(), p.shape_y_l.value());
        if shape_linked {
            self.topology_r.compute(mode, p.shape_x_l.value(), p.shape_y_l.value());
        } else {
            self.topology_r.compute(mode, p.shape_x_r.value(), p.shape_y_r.value());
        }

        let pos_l = *self.topology_l.positions();
        let pos_r = *self.topology_r.positions();
        let n_active = (p.num_taps_active.value() as usize).clamp(1, NUM_TAPS);

        let grain_ms = p.grain_size_ms.value();
        // Tape-bend slew: 1 sample of delay-time change per ~50 samples of audio.
        // Fast enough to track changes, slow enough to hear the Doppler.
        let bend_slew_rate = 1.0 / 50.0;
        let mut nom_l = [0.0f32; NUM_TAPS];
        let mut nom_r = [0.0f32; NUM_TAPS];
        let mut continuous_mode = [false; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;
            let semis = if active { p.taps[i].pitch_semitones.value() as f32 } else { 0.0 };
            let cont = p.taps[i].pitch_continuous.value();
            continuous_mode[i] = cont;

            let span_l = NUM_TAPS as f32 * base_samples_l;
            let span_r = NUM_TAPS as f32 * base_samples_r;
            let off = p.taps[i].time_offset.value() * span_l;
            let off_r = p.taps[i].time_offset.value() * span_r;

            if cont {
                // CONTINUOUS pitch: nominal stays at the topology position;
                // pitch is applied by the dual-head grain shifter.
                nom_l[i] = (pos_l[i] * span_l + off).max(2.0);
                nom_r[i] = (pos_r[i] * span_r + off_r).max(2.0);
                self.main_l.readers[i].set_pitch_semitones(semis);
                self.main_r.readers[i].set_pitch_semitones(semis);
                self.main_l.readers[i].set_nominal_slew_rate(1e6);
                self.main_r.readers[i].set_nominal_slew_rate(1e6);
            } else {
                // TAPE BEND: pitch modifies the effective delay time.
                // Higher pitch = shorter delay (tape sped up). The slewed
                // delay-time transition produces the Doppler bend.
                let mult = 2.0_f32.powf(-semis / 12.0);
                nom_l[i] = ((pos_l[i] * span_l + off) * mult).max(2.0);
                nom_r[i] = ((pos_r[i] * span_r + off_r) * mult).max(2.0);
                self.main_l.readers[i].set_pitch_semitones(0.0);
                self.main_r.readers[i].set_pitch_semitones(0.0);
                self.main_l.readers[i].set_nominal_slew_rate(bend_slew_rate);
                self.main_r.readers[i].set_nominal_slew_rate(bend_slew_rate);
            }
            self.main_l.readers[i].set_window_ms(grain_ms, sr);
            self.main_r.readers[i].set_window_ms(grain_ms, sr);
        }

        // Per-tap filter configuration
        let mut tap_filter_types = [crate::params::FilterType::LowPass; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let tap = &p.taps[i];
            let cutoff = tap.filter_cutoff.value();
            let q = tap.filter_q.value();
            self.tap_svf_l[i].set(cutoff, q, sr);
            self.tap_svf_r[i].set(cutoff, q, sr);
            tap_filter_types[i] = tap.filter_type.value();
        }

        let lfo_rate = p.wobble_rate.value();
        let wobble_amt = p.wobble_amount.value();
        let lfo_inc = lfo_rate / sr;

        let input_gain = nih_plug::util::db_to_gain(p.input_gain.value());
        let output_gain = nih_plug::util::db_to_gain(p.output_gain.value());
        let dry_wet = p.dry_wet.value();
        let feedback = p.feedback.value();
        let soft = p.soft_clip.value();
        let fb_tap_idx = ((p.feedback_tap.value() as usize).min(n_active).saturating_sub(1)).min(NUM_TAPS - 1);
        let ch_mode = p.channel_mode.value();
        let crossfeed = p.crossfeed.value();

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

            // Wobble — gentler: ±5% at max amount, scaled by amount squared for taper.
            let lfo = (self.lfo_phase * core::f32::consts::TAU).sin();
            self.lfo_phase += lfo_inc;
            if self.lfo_phase >= 1.0 { self.lfo_phase -= 1.0; }
            let mod_scale = 1.0 + lfo * wobble_amt * wobble_amt * 0.05;

            self.main_l.delay.write(x_l + self.fb_state_l);
            self.main_r.delay.write(x_r + self.fb_state_r);

            // Feed input into the pitch trackers for PSOLA-mode shifting
            self.pitch_l.push(x_l);
            self.pitch_r.push(x_r);
            let period_l = self.pitch_l.period();
            let period_r = self.pitch_r.period();

            let mut tap_out_l = 0.0;
            let mut tap_out_r = 0.0;
            let mut fb_tap_l = 0.0;
            let mut fb_tap_r = 0.0;
            for i in 0..n_active {
                self.main_l.readers[i].set_nominal((nom_l[i] * mod_scale).max(2.0));
                self.main_r.readers[i].set_nominal((nom_r[i] * mod_scale).max(2.0));
                let (mut sl, mut sr_s) = if continuous_mode[i] {
                    // PSOLA — period-synchronous granular shifter
                    (
                        self.main_l.readers[i].read_psola(&self.main_l.delay, period_l),
                        self.main_r.readers[i].read_psola(&self.main_r.delay, period_r),
                    )
                } else {
                    (
                        self.main_l.readers[i].read_tape_bend(&self.main_l.delay),
                        self.main_r.readers[i].read_tape_bend(&self.main_r.delay),
                    )
                };

                // Per-tap filter
                sl = self.tap_svf_l[i].process(sl, tap_filter_types[i]);
                sr_s = self.tap_svf_r[i].process(sr_s, tap_filter_types[i]);

                let gl = sl * tap_gain[i];
                let gr = sr_s * tap_gain[i];
                tap_out_l += gl * tap_pan_l[i];
                tap_out_r += gr * tap_pan_r[i];
                if i == fb_tap_idx {
                    fb_tap_l = sl;
                    fb_tap_r = sr_s;
                }
            }

            // Crossfeed: mix opposite channel into each (equal-power blend)
            if crossfeed > 1e-4 {
                let cf = crossfeed.min(1.0);
                let kept = 1.0 - cf * 0.5;
                let sent = cf * 0.5;
                let new_l = tap_out_l * kept + tap_out_r * sent;
                let new_r = tap_out_r * kept + tap_out_l * sent;
                tap_out_l = new_l;
                tap_out_r = new_r;
            }

            let fb_in_l = fb_tap_l * feedback;
            let fb_in_r = fb_tap_r * feedback;
            self.fb_l.write(fb_in_l);
            self.fb_r.write(fb_in_r);
            let fb_read_l = self.fb_l.read(2.0);
            let fb_read_r = self.fb_r.read(2.0);

            // Per-tap filters were applied above; wet bus is the tap sum
            let wet_l = tap_out_l;
            let wet_r = tap_out_r;

            self.fb_state_l = fb_read_l;
            self.fb_state_r = fb_read_r;

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
