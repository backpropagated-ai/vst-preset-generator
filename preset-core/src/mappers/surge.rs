// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Surge / Surge XT parameter mapper.
//
// Ported from the legacy Python `vst_mappers/surge.py` parameter table. These
// are the abstract, prompt-independent parameters we expose to the model; the
// Surge `.fxp` writer (see `writers/surge.rs`) translates the mapped subset into
// Surge XT's real internal patch parameters.
//
// The table was trimmed from the original Python port: parameters that Surge XT
// cannot express (per its GPL-3.0 source) were removed so the model is not
// misled into designing controls that would be silently discarded. Removed:
// `oversampling`, `quality`, `voice_priority`, `scene_morph`,
// `filter1_drive`/`filter2_drive` (Surge has no per-filter drive),
// `portamento_mode`, and `osc1_detune`/`osc2_detune`/`osc3_detune` (Surge's
// classic oscillator only detunes via unison spread, exposed here as
// `unison_detune`). Everything remaining is wired into the patch by the writer
// except `unison_spread` (Surge's classic oscillator has no separate spread
// control) — see the README coverage table.

use crate::mappers::Mapper;
use crate::param::ParamSpec;

/// The Surge parameter mapper.
pub struct SurgeMapper;

impl Mapper for SurgeMapper {
    fn id(&self) -> &'static str {
        "surge"
    }
    fn specs(&self) -> &'static [ParamSpec] {
        SURGE_SPECS
    }
}

const OSC_TYPES: &[&str] = &[
    "classic",
    "modern",
    "wavetable",
    "window",
    "sine",
    "fm2",
    "fm3",
];
const FILTER_TYPES: &[&str] = &[
    "lp12",
    "lp24",
    "lp_ladder",
    "hp12",
    "hp24",
    "bp12",
    "notch",
    "comb",
    "sample_hold",
];
const LFO_SHAPES: &[&str] = &["sine", "triangle", "square", "saw", "noise", "s&h"];

/// The Surge parameter specifications. Order follows the original Python table
/// (dead params removed); notes document what each value maps to in the patch.
pub const SURGE_SPECS: &[ParamSpec] = &[
    // Oscillator 1
    ParamSpec::enum_("osc1_type", OSC_TYPES),
    ParamSpec::linear_snap("osc1_pitch", -48.0, 48.0, 0.0, 1.0)
        .with_note("Semitones; decomposed into osc octave (±3) + a ±7 remainder in Surge."),
    ParamSpec::linear("osc1_width", 0.0, 1.0, 0.5)
        .with_note("Classic-oscillator pulse width (0..1); ignored for non-classic osc types."),
    // Oscillator 2
    ParamSpec::enum_("osc2_type", OSC_TYPES),
    ParamSpec::linear_snap("osc2_pitch", -48.0, 48.0, 0.0, 1.0)
        .with_note("Semitones; decomposed into osc octave (±3) + a ±7 remainder in Surge."),
    ParamSpec::linear("osc2_width", 0.0, 1.0, 0.5)
        .with_note("Classic-oscillator pulse width (0..1); ignored for non-classic osc types."),
    // Oscillator 3
    ParamSpec::enum_("osc3_type", OSC_TYPES),
    ParamSpec::linear_snap("osc3_pitch", -48.0, 48.0, 0.0, 1.0)
        .with_note("Semitones; decomposed into osc octave (±3) + a ±7 remainder in Surge."),
    ParamSpec::linear("osc3_width", 0.0, 1.0, 0.5)
        .with_note("Classic-oscillator pulse width (0..1); ignored for non-classic osc types."),
    // Mixer
    ParamSpec::linear("osc1_level", 0.0, 1.0, 0.33)
        .with_note("Mixer level; a non-zero value auto-unmutes the oscillator."),
    ParamSpec::linear("osc2_level", 0.0, 1.0, 0.33)
        .with_note("Mixer level; a non-zero value auto-unmutes the oscillator."),
    ParamSpec::linear("osc3_level", 0.0, 1.0, 0.33)
        .with_note("Mixer level; a non-zero value auto-unmutes the oscillator."),
    ParamSpec::linear("noise_level", 0.0, 1.0, 0.0)
        .with_note("Noise mixer level; a non-zero value auto-unmutes noise."),
    ParamSpec::linear("ring_12", 0.0, 1.0, 0.0)
        .with_note("Ring-mod 1x2 mixer level; a non-zero value auto-unmutes the ring mod."),
    ParamSpec::linear("ring_23", 0.0, 1.0, 0.0)
        .with_note("Ring-mod 2x3 mixer level; a non-zero value auto-unmutes the ring mod."),
    // Filter 1
    ParamSpec::enum_("filter1_type", FILTER_TYPES),
    ParamSpec::exponential("filter1_cutoff", 20.0, 20000.0, 1000.0, 3.0)
        .with_note("Cutoff in Hz; stored as Surge's 12*log2(hz/440) semitone offset."),
    ParamSpec::linear("filter1_resonance", 0.0, 1.0, 0.2),
    // Filter 2
    ParamSpec::enum_("filter2_type", FILTER_TYPES),
    ParamSpec::exponential("filter2_cutoff", 20.0, 20000.0, 1000.0, 3.0)
        .with_note("Cutoff in Hz; stored as Surge's 12*log2(hz/440) semitone offset."),
    ParamSpec::linear("filter2_resonance", 0.0, 1.0, 0.2),
    // Filter routing
    ParamSpec::enum_("filter_config", &["serial", "parallel", "wide", "dual"]),
    ParamSpec::linear("filter_balance", -1.0, 1.0, 0.0)
        .with_note("Filter 1<->2 balance (-1..1)."),
    ParamSpec::linear("filter_feedback", 0.0, 1.0, 0.0)
        .with_note("Filter feedback amount (0..1, positive half of Surge's bipolar feedback)."),
    // Amp Envelope
    ParamSpec::exponential("amp_attack", 0.001, 10.0, 0.01, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    ParamSpec::exponential("amp_decay", 0.001, 10.0, 0.1, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    ParamSpec::linear("amp_sustain", 0.0, 1.0, 0.7),
    ParamSpec::exponential("amp_release", 0.001, 10.0, 0.3, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    // Filter Envelope
    ParamSpec::exponential("filter_attack", 0.001, 10.0, 0.01, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    ParamSpec::exponential("filter_decay", 0.001, 10.0, 0.1, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    ParamSpec::linear("filter_sustain", 0.0, 1.0, 0.5),
    ParamSpec::exponential("filter_release", 0.001, 10.0, 0.3, 2.0)
        .with_note("Seconds; stored as log2(seconds)."),
    ParamSpec::linear("filter_env_depth", -1.0, 1.0, 0.5)
        .with_note("-1..1, scaled to ±60 semitones of cutoff sweep (filter 1 envmod)."),
    // LFO 1
    ParamSpec::enum_("lfo1_shape", LFO_SHAPES),
    ParamSpec::exponential("lfo1_rate", 0.01, 20.0, 1.0, 2.0)
        .with_note("Rate in Hz; stored as log2(Hz)."),
    ParamSpec::linear("lfo1_phase", 0.0, 360.0, 0.0)
        .with_note("Start phase in degrees; stored as a 0..1 fraction of one cycle."),
    ParamSpec::linear("lfo1_deform", -1.0, 1.0, 0.0),
    // LFO 2
    ParamSpec::enum_("lfo2_shape", LFO_SHAPES),
    ParamSpec::exponential("lfo2_rate", 0.01, 20.0, 1.0, 2.0)
        .with_note("Rate in Hz; stored as log2(Hz)."),
    ParamSpec::linear("lfo2_phase", 0.0, 360.0, 0.0)
        .with_note("Start phase in degrees; stored as a 0..1 fraction of one cycle."),
    ParamSpec::linear("lfo2_deform", -1.0, 1.0, 0.0),
    // LFO 3
    ParamSpec::enum_("lfo3_shape", LFO_SHAPES),
    ParamSpec::exponential("lfo3_rate", 0.01, 20.0, 1.0, 2.0)
        .with_note("Rate in Hz; stored as log2(Hz)."),
    ParamSpec::linear("lfo3_phase", 0.0, 360.0, 0.0)
        .with_note("Start phase in degrees; stored as a 0..1 fraction of one cycle."),
    ParamSpec::linear("lfo3_deform", -1.0, 1.0, 0.0),
    // FX Send
    ParamSpec::linear("fx_send_1", 0.0, 1.0, 0.0).with_note("Scene send to FX bus 1 (0..1)."),
    ParamSpec::linear("fx_send_2", 0.0, 1.0, 0.0).with_note("Scene send to FX bus 2 (0..1)."),
    // Scene
    ParamSpec::enum_(
        "scene_mode",
        &["single", "key_split", "dual", "channel_split"],
    ),
    // Global
    ParamSpec::linear("master_volume", 0.0, 1.0, 0.7).with_note("Scene A output volume (0..1)."),
    ParamSpec::discrete("pitch_bend_range", 0.0, 24.0, 2.0, 1.0)
        .with_note("Semitones; applied to both up and down bend range."),
    ParamSpec::exponential("portamento_time", 0.0, 1.0, 0.0, 2.0)
        .with_note("Glide time in seconds; 0 = off (Surge portamento minimum)."),
    // Character
    ParamSpec::enum_("character", &["warm", "neutral", "bright"]),
    // Voice
    ParamSpec::discrete("polyphony", 1.0, 64.0, 16.0, 1.0).with_note("Voice limit (2..64)."),
    // Modulation depths (routed as <modrouting> in the patch)
    ParamSpec::linear("mod_lfo1_pitch", -1.0, 1.0, 0.0)
        .with_note("LFO1 -> pitch depth; -1..1 scaled to ±12 semitones."),
    ParamSpec::linear("mod_lfo1_cutoff", -1.0, 1.0, 0.0)
        .with_note("LFO1 -> filter 1 cutoff depth; -1..1 scaled to ±60 semitones."),
    ParamSpec::linear("mod_lfo2_pitch", -1.0, 1.0, 0.0)
        .with_note("LFO2 -> pitch depth; -1..1 scaled to ±12 semitones."),
    ParamSpec::linear("mod_lfo2_cutoff", -1.0, 1.0, 0.0)
        .with_note("LFO2 -> filter 1 cutoff depth; -1..1 scaled to ±60 semitones."),
    ParamSpec::linear("mod_env_pitch", -1.0, 1.0, 0.0)
        .with_note("Filter envelope -> pitch depth; -1..1 scaled to ±12 semitones."),
    ParamSpec::linear("mod_velocity_cutoff", -1.0, 1.0, 0.0)
        .with_note("Velocity -> filter 1 cutoff depth; -1..1 scaled to ±60 semitones."),
    ParamSpec::linear("mod_velocity_amp", 0.0, 1.0, 0.5)
        .with_note("Velocity -> amp sensitivity; 0 = none, 1 = full (-48 dB at zero velocity)."),
    // Unison
    ParamSpec::discrete("unison_voices", 1.0, 16.0, 1.0, 1.0)
        .with_note("Classic-oscillator unison voice count (1..16)."),
    ParamSpec::linear("unison_detune", 0.0, 100.0, 0.0)
        .with_note("Classic-oscillator unison detune; applied when unison_voices > 1."),
    ParamSpec::linear("unison_spread", 0.0, 1.0, 0.5),
    // Drift
    ParamSpec::linear("drift", 0.0, 1.0, 0.1).with_note("Oscillator drift amount (0..1)."),
    // Output
    ParamSpec::logarithmic("output_gain", 0.1, 2.0, 1.0)
        .with_note("Linear output gain; stored as global volume in dB (clamped to -48..0)."),
    ParamSpec::linear("output_pan", -1.0, 1.0, 0.0).with_note("Scene pan (-1..1)."),
];
