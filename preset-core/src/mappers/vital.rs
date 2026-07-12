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
const LFO_MODES: &[&str] = &["trigger", "sync", "envelope", "random"];
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
    ParamSpec::linear("osc_1_wave", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_1_level", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_1_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_1_tune", -48.0, 48.0, 0.0, 1.0),
    ParamSpec::discrete("osc_1_unison_voices", 1.0, 16.0, 1.0, 1.0),
    ParamSpec::linear("osc_1_unison_detune", 0.0, 1.0, 0.0),
    // Oscillator 2
    ParamSpec::linear("osc_2_wave", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_2_level", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_2_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_2_tune", -48.0, 48.0, 0.0, 1.0),
    // Oscillator 3
    ParamSpec::linear("osc_3_wave", 0.0, 1.0, 0.5),
    ParamSpec::linear("osc_3_level", 0.0, 1.0, 0.0),
    ParamSpec::linear("osc_3_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("osc_3_tune", -48.0, 48.0, 0.0, 1.0),
    // Sample
    ParamSpec::linear("sample_level", 0.0, 1.0, 0.0),
    ParamSpec::linear("sample_pan", -1.0, 1.0, 0.0),
    ParamSpec::linear_snap("sample_tune", -48.0, 48.0, 0.0, 1.0),
    // Filter 1 (cutoff in MIDI notes)
    ParamSpec::enum_("filter_1_type", FILTER_TYPES),
    ParamSpec::exponential("filter_1_cutoff", 8.0, 136.0, 80.0, 1.0),
    ParamSpec::linear("filter_1_resonance", 0.0, 1.0, 0.5),
    ParamSpec::linear("filter_1_drive", 0.0, 20.0, 0.0),
    ParamSpec::linear("filter_1_mix", 0.0, 1.0, 1.0),
    // Filter 2
    ParamSpec::enum_("filter_2_type", FILTER_TYPES),
    ParamSpec::exponential("filter_2_cutoff", 8.0, 136.0, 80.0, 1.0),
    ParamSpec::linear("filter_2_resonance", 0.0, 1.0, 0.5),
    ParamSpec::linear("filter_2_drive", 0.0, 20.0, 0.0),
    ParamSpec::linear("filter_2_mix", 0.0, 1.0, 0.0),
    // Envelope 1
    ParamSpec::exponential("env_1_attack", 0.0, 4.0, 0.01, 1.0),
    ParamSpec::exponential("env_1_decay", 0.0, 4.0, 0.3, 1.0),
    ParamSpec::linear("env_1_sustain", 0.0, 1.0, 1.0),
    ParamSpec::exponential("env_1_release", 0.0, 4.0, 0.3, 1.0),
    // Envelope 2
    ParamSpec::exponential("env_2_attack", 0.0, 4.0, 0.01, 1.0),
    ParamSpec::exponential("env_2_decay", 0.0, 4.0, 0.3, 1.0),
    ParamSpec::linear("env_2_sustain", 0.0, 1.0, 0.5),
    ParamSpec::exponential("env_2_release", 0.0, 4.0, 0.3, 1.0),
    // LFO 1
    ParamSpec::exponential("lfo_1_frequency", 0.01, 20.0, 2.0, 1.0),
    ParamSpec::linear("lfo_1_phase", 0.0, 1.0, 0.0),
    ParamSpec::enum_("lfo_1_mode", LFO_MODES),
    // LFO 2
    ParamSpec::exponential("lfo_2_frequency", 0.01, 20.0, 2.0, 1.0),
    ParamSpec::linear("lfo_2_phase", 0.0, 1.0, 0.0),
    // Reverb
    ParamSpec::linear("reverb_size", 0.0, 1.0, 0.5),
    ParamSpec::linear("reverb_decay_time", 0.0, 1.0, 0.5),
    ParamSpec::linear("reverb_mix", 0.0, 1.0, 0.0),
    // Delay
    ParamSpec::exponential("delay_frequency", 0.25, 16.0, 2.0, 1.0),
    ParamSpec::linear("delay_feedback", 0.0, 0.95, 0.4),
    ParamSpec::linear("delay_mix", 0.0, 1.0, 0.0),
    // Chorus
    ParamSpec::exponential("chorus_frequency", 0.01, 10.0, 0.5, 1.0),
    ParamSpec::linear("chorus_depth", 0.0, 1.0, 0.5),
    ParamSpec::linear("chorus_mix", 0.0, 1.0, 0.0),
    // Distortion
    ParamSpec::enum_("distortion_type", DIST_TYPES),
    ParamSpec::linear("distortion_drive", 0.0, 20.0, 0.0),
    ParamSpec::linear("distortion_mix", 0.0, 1.0, 0.0),
    // Modulation matrix (simplified)
    ParamSpec::enum_("mod_1_source", MOD_SOURCES),
    ParamSpec::enum_("mod_1_destination", MOD_DESTS),
    ParamSpec::linear("mod_1_amount", -1.0, 1.0, 0.0),
    // Master
    ParamSpec::linear("master_volume", 0.0, 1.0, 0.7),
];
