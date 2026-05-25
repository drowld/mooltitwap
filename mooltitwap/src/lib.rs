mod delay;
mod filter;
mod gui;
mod params;
mod topology;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

use crate::delay::{ChannelTaps, DelayLine};
use crate::filter::{soft_clip, PeakLimiter, Svf};
use crate::params::{ChannelMode, PetalParams, NUM_TAPS};
use crate::topology::TopologyEngine;

const MAX_DELAY_SECONDS: f32 = 4.0;

pub struct Mooltitwap {
    params: Arc<PetalParams>,
    editor_state: Arc<EguiState>,
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

    // Filter
    svf_l: Svf,
    svf_r: Svf,

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
            sample_rate: 48_000.0,
            topology_l: TopologyEngine::new(),
            topology_r: TopologyEngine::new(),
            main_l: ChannelTaps::new(dummy_max),
            main_r: ChannelTaps::new(dummy_max),
            fb_l: DelayLine::new(dummy_max),
            fb_r: DelayLine::new(dummy_max),
            fb_state_l: 0.0,
            fb_state_r: 0.0,
            svf_l: Svf::new(),
            svf_r: Svf::new(),
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
        gui::create_editor(self.params.clone(), self.editor_state.clone())
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
        true
    }

    fn reset(&mut self) {
        self.main_l.reset();
        self.main_r.reset();
        self.fb_l.reset();
        self.fb_r.reset();
        self.fb_state_l = 0.0;
        self.fb_state_r = 0.0;
        self.svf_l.reset();
        self.svf_r.reset();
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

        let mut nom_l = [0.0f32; NUM_TAPS];
        let mut nom_r = [0.0f32; NUM_TAPS];
        for i in 0..NUM_TAPS {
            let active = p.taps[i].active.value() && i < n_active;
            let semis = if active { p.taps[i].pitch_semitones.value() as f32 } else { 0.0 };
            // Per-tap time offset (fraction of full delay span)
            let span_l = NUM_TAPS as f32 * base_samples_l;
            let span_r = NUM_TAPS as f32 * base_samples_r;
            let off = p.taps[i].time_offset.value() * span_l;
            let off_r = p.taps[i].time_offset.value() * span_r;
            nom_l[i] = (pos_l[i] * span_l + off).max(2.0);
            nom_r[i] = (pos_r[i] * span_r + off_r).max(2.0);
            self.main_l.readers[i].set_pitch_semitones(semis);
            self.main_r.readers[i].set_pitch_semitones(semis);
            self.main_l.readers[i].set_window_ms(100.0, sr);
            self.main_r.readers[i].set_window_ms(100.0, sr);
            self.main_l.readers[i].set_nominal_slew_rate(1e6);
            self.main_r.readers[i].set_nominal_slew_rate(1e6);
        }

        self.svf_l.set(p.filter_cutoff.value(), p.filter_q.value(), sr);
        self.svf_r.set(p.filter_cutoff.value(), p.filter_q.value(), sr);
        let f_type = p.filter_type.value();

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

            let mut tap_out_l = 0.0;
            let mut tap_out_r = 0.0;
            let mut fb_tap_l = 0.0;
            let mut fb_tap_r = 0.0;
            for i in 0..n_active {
                self.main_l.readers[i].set_nominal((nom_l[i] * mod_scale).max(2.0));
                self.main_r.readers[i].set_nominal((nom_r[i] * mod_scale).max(2.0));
                let sl = self.main_l.readers[i].read(&self.main_l.delay);
                let sr_s = self.main_r.readers[i].read(&self.main_r.delay);
                let gl = sl * tap_gain[i];
                let gr = sr_s * tap_gain[i];
                tap_out_l += gl * tap_pan_l[i];
                tap_out_r += gr * tap_pan_r[i];
                if i == fb_tap_idx {
                    fb_tap_l = sl;
                    fb_tap_r = sr_s;
                }
            }

            let fb_in_l = fb_tap_l * feedback;
            let fb_in_r = fb_tap_r * feedback;
            self.fb_l.write(fb_in_l);
            self.fb_r.write(fb_in_r);
            let fb_read_l = self.fb_l.read(2.0);
            let fb_read_r = self.fb_r.read(2.0);

            let wet_l = self.svf_l.process(tap_out_l, f_type);
            let wet_r = self.svf_r.process(tap_out_r, f_type);

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
