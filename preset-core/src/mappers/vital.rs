// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Vital parameter mapper.
//
// Faithful port of the legacy Python `vst_mappers/vital.py` (simplified subset).
// The Vital `.vital` writer (see `writers/vital.rs`) maps these onto Vital's
// real flat `settings` JSON keys.

use crate::mappers::Mapper;
use crate::param::ParamSpec;

/// The Vital parameter mapper.
pub struct VitalMapper;

impl Mapper for VitalMapper {
    fn id(&self) -> &'static str {
        "vital"
    }
    fn specs(&self) -> &'static [ParamSpec] {
        VITAL_SPECS
    }
}

const FILTER_TYPES: &[&str] = &[
    "analog", "dirty", "ladder", "digital", "diode", "formant", "comb", "phaser",
];
const LFO_MODES: &[&str] = &["trigger", "sync", "envelope", "sustain_envelope"];
const DIST_TYPES: &[&str] = &[
    "soft_clip",
    "hard_clip",
    "linear_fold",
    "sine_fold",
    "bit_crush",
    "down_sample",
];
const MOD_SOURCES: &[&str] = &[
    "lfo_1",
    "lfo_2",
    "env_1",
    "env_2",
    "velocity",
    "aftertouch",
    "mod_wheel",
];
const MOD_DESTS: &[&str] = &[
    "osc_1_level",
    "osc_2_level",
    "filter_1_cutoff",
    "filter_1_resonance",
];

/// The Vital parameter specifications, in the exact order of the Python table.
pub const VITAL_SPECS: &[ParamSpec] = &[
    // Oscillator 1
    ParamSpec::linear("osc_1_wave", 0.0, 1.0, 0.5)
        .with_note("Wavetable scan position 0..1 (0 = first frame, often sine-ish; higher = brighter/more complex)."),
    ParamSpec::linear("osc_1_level", 0.0, 1.0, 0.7)
        .with_note("Amplitude 0..1; > 0 auto-enables the oscillator."),
    ParamSpec::linear("osc_1_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_1_tune", -48.0, 48.0, 0.0, 1.0),
    ParamSpec::discrete("osc_1_unison_voices", 1.0, 16.0, 1.0, 1.0),
    ParamSpec::linear("osc_1_unison_detune", 0.0, 1.0, 0.0)
        .with_note("0 = none, 0.3..0.5 = classic supersaw spread, 1 = extreme."),
    // Oscillator 2
    ParamSpec::linear("osc_2_wave", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_2_level", 0.0, 1.0, 0.0)
        .with_note("Amplitude 0..1; > 0 auto-enables the oscillator."),
    ParamSpec::linear("osc_2_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_2_tune", -48.0, 48.0, 0.0, 1.0),
    // Oscillator 3
    ParamSpec::linear("osc_3_wave", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_3_level", 0.0, 1.0, 0.0)
        .with_note("Amplitude 0..1; > 0 auto-enables the oscillator."),
    ParamSpec::linear("osc_3_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_3_tune", -48.0, 48.0, 0.0, 1.0),
    // Sample
    ParamSpec::linear("sample_level", 0.0, 1.0, 0.0),
    ParamSpec::linear("sample_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("sample_tune", -48.0, 48.0, 0.0, 1.0),
    // Filter 1 (cutoff in MIDI notes)
    ParamSpec::enum_("filter_1_type", FILTER_TYPES),
    ParamSpec::linear("filter_1_cutoff", 8.0, 136.0, 80.0)
        .with_note("MIDI note; Hz = 440*2^((n-69)/12). Note 48≈130Hz, 69=440Hz, 96≈2kHz."),
    ParamSpec::linear("filter_1_resonance", 0.0, 1.0, 0.5),
    ParamSpec::linear("filter_1_drive", 0.0, 20.0, 0.0),
    ParamSpec::linear("filter_1_mix", 0.0, 1.0, 1.0)
        .with_note("Dry/wet; a value > 0 auto-enables filter 1."),
    // Filter 2
    ParamSpec::enum_("filter_2_type", FILTER_TYPES),
    ParamSpec::linear("filter_2_cutoff", 8.0, 136.0, 80.0)
        .with_note("MIDI note; Hz = 440*2^((n-69)/12)."),
    ParamSpec::linear("filter_2_resonance", 0.0, 1.0, 0.5),
    ParamSpec::linear("filter_2_drive", 0.0, 20.0, 0.0),
    ParamSpec::linear("filter_2_mix", 0.0, 1.0, 0.0)
        .with_note("Dry/wet; a value > 0 auto-enables filter 2."),
    // Envelope 1
    ParamSpec::logarithmic("env_1_attack", 0.001, 8.0, 0.01).with_note("Seconds. env_1 is the AMP envelope (hardwired)."),
    ParamSpec::logarithmic("env_1_decay", 0.001, 8.0, 1.0).with_note("Seconds."),
    ParamSpec::linear("env_1_sustain", 0.0, 1.0, 1.0),
    ParamSpec::logarithmic("env_1_release", 0.001, 8.0, 0.3).with_note("Seconds."),
    // Envelope 2
    ParamSpec::logarithmic("env_2_attack", 0.001, 8.0, 0.01).with_note("Seconds. env_2 is free; route it via the mod matrix."),
    ParamSpec::logarithmic("env_2_decay", 0.001, 8.0, 1.0).with_note("Seconds."),
    ParamSpec::linear("env_2_sustain", 0.0, 1.0, 0.5),
    ParamSpec::logarithmic("env_2_release", 0.001, 8.0, 0.3).with_note("Seconds."),
    // LFO 1
    ParamSpec::logarithmic("lfo_1_frequency", 0.01, 20.0, 2.0).with_note("Hz."),
    ParamSpec::linear("lfo_1_phase", 0.0, 1.0, 0.0),
    ParamSpec::enum_("lfo_1_mode", LFO_MODES)
        .with_note("trigger = restart per note; envelope = one-shot."),
    // LFO 2
    ParamSpec::logarithmic("lfo_2_frequency", 0.01, 20.0, 2.0).with_note("Hz."),
    ParamSpec::linear("lfo_2_phase", 0.0, 1.0, 0.0),
    // Reverb
    ParamSpec::linear("reverb_size", 0.0, 1.0, 0.5),
    ParamSpec::logarithmic("reverb_decay_time", 0.1, 30.0, 2.0).with_note("Seconds."),
    ParamSpec::linear("reverb_mix", 0.0, 1.0, 0.0)
        .with_note("Dry/wet; > 0 auto-enables reverb (0.15..0.35 typical)."),
    // Delay
    ParamSpec::logarithmic("delay_frequency", 0.25, 16.0, 2.0)
        .with_note("Echo rate in Hz (delay time = 1/Hz: 2 Hz = 0.5 s)."),
    ParamSpec::linear("delay_feedback", 0.0, 0.95, 0.4),
    ParamSpec::linear("delay_mix", 0.0, 1.0, 0.0).with_note("Dry/wet; > 0 auto-enables delay."),
    // Chorus
    ParamSpec::logarithmic("chorus_frequency", 0.02, 8.0, 0.125).with_note("Hz."),
    ParamSpec::linear("chorus_depth", 0.0, 1.0, 0.5),
    ParamSpec::linear("chorus_mix", 0.0, 1.0, 0.0)
        .with_note("Dry/wet; > 0 auto-enables chorus (great for pads/EPs)."),
    // Distortion
    ParamSpec::enum_("distortion_type", DIST_TYPES),
    ParamSpec::linear("distortion_drive", 0.0, 20.0, 0.0).with_note("Drive in dB."),
    ParamSpec::linear("distortion_mix", 0.0, 1.0, 0.0)
        .with_note("Dry/wet; > 0 auto-enables distortion."),
    // Modulation matrix (simplified)
    ParamSpec::enum_("mod_1_source", MOD_SOURCES),
    ParamSpec::enum_("mod_1_destination", MOD_DESTS),
    ParamSpec::linear("mod_1_amount", -1.0, 1.0, 0.0)
        .with_note("Non-zero wires mod_1_source -> mod_1_destination in the mod matrix."),
    // Master
    ParamSpec::linear("master_volume", 0.0, 1.0, 0.8)
        .with_note("1.0 = 0 dB full scale; 0.8 ≈ -2 dB; 0.5 = -6 dB."),
];
