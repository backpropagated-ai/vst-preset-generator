// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Vital `.vital` preset writer.
//
// `.vital` files are UTF-8 JSON (Vital also reads a zlib-wrapped form, but the
// plain JSON form loads fine and is what the exporter emits). Format learned
// from real `.vital` samples (read-only; no third-party content copied):
//
//   {
//     "name": "...", "author": "...", "comments": "...",
//     "macro1".."macro4": 0.0,
//     "synth_version": "1.0.7",
//     "settings": { "<param>": <float>, ..., "filter_1_type": <int>, ... }
//   }
//
// `settings` is a flat map of Vital parameter name -> value. We map our abstract
// parameters onto Vital's real setting keys with sensible ranges. Enum params
// (filter type, distortion type) become their integer index in Vital's own
// enum ordering.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::param::ParamValue;

/// The `synth_version` string written into the preset.
pub const VITAL_SYNTH_VERSION: &str = "1.0.7";

/// Metadata for a Vital preset.
#[derive(Debug, Clone)]
pub struct VitalMeta {
    pub name: String,
    pub author: String,
    pub comments: String,
}

impl Default for VitalMeta {
    fn default() -> Self {
        Self {
            name: "DeepSynth Patch".into(),
            author: "DeepSynth".into(),
            comments: String::new(),
        }
    }
}

fn num(mapped: &BTreeMap<String, ParamValue>, key: &str) -> Option<f64> {
    mapped.get(key).map(|v| match v {
        ParamValue::Num(n) => *n,
        ParamValue::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        ParamValue::Enum(_) => 0.0,
    })
}

fn enum_str<'a>(mapped: &'a BTreeMap<String, ParamValue>, key: &str) -> Option<&'a str> {
    mapped.get(key).and_then(|v| match v {
        ParamValue::Enum(s) => Some(s.as_str()),
        _ => None,
    })
}

/// Vital filter type enum index (from Vital's filter model ordering).
fn filter_type_idx(name: &str) -> i64 {
    match name {
        "analog" => 0,
        "dirty" => 1,
        "ladder" => 2,
        "digital" => 3,
        "diode" => 4,
        "formant" => 5,
        "comb" => 6,
        "phaser" => 7,
        _ => 0,
    }
}

/// Vital distortion type enum index.
fn distortion_type_idx(name: &str) -> i64 {
    match name {
        "soft_clip" => 0,
        "hard_clip" => 1,
        "linear_fold" => 2,
        "sine_fold" => 3,
        "bit_crush" => 4,
        "down_sample" => 5,
        _ => 0,
    }
}

/// Vital LFO sync mode index (trigger/sync/envelope/random ordering).
fn lfo_mode_idx(name: &str) -> i64 {
    match name {
        "trigger" => 0,
        "sync" => 1,
        "envelope" => 2,
        "random" => 3,
        _ => 0,
    }
}

/// Insert a float setting.
fn set(s: &mut Map<String, Value>, k: &str, v: f64) {
    s.insert(k.to_string(), json!(v));
}
/// Insert an integer setting.
fn set_i(s: &mut Map<String, Value>, k: &str, v: i64) {
    s.insert(k.to_string(), json!(v));
}

/// Build the `settings` object mapping abstract params onto Vital's real keys.
pub fn build_settings(mapped: &BTreeMap<String, ParamValue>) -> Map<String, Value> {
    let mut s = Map::new();

    // Oscillators (level 0..1, tune in semitones, pan -1..1, wt_pos from wave).
    for (i, pre) in ["osc_1", "osc_2", "osc_3"].iter().enumerate() {
        let abs = format!("osc_{}", i + 1);
        if let Some(v) = num(mapped, &format!("{abs}_level")) {
            set(&mut s, &format!("{pre}_level"), v.clamp(0.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("{abs}_pan")) {
            set(&mut s, &format!("{pre}_pan"), v.clamp(-1.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("{abs}_tune")) {
            set(&mut s, &format!("{pre}_transpose"), v.round());
        }
        if let Some(v) = num(mapped, &format!("{abs}_wave")) {
            // Vital wavetable position is 0..1 across the table.
            set(&mut s, &format!("{pre}_wt_pos"), v.clamp(0.0, 1.0));
        }
    }
    if let Some(v) = num(mapped, "osc_1_unison_voices") {
        set_i(&mut s, "osc_1_unison_voices", v.round() as i64);
    }
    if let Some(v) = num(mapped, "osc_1_unison_detune") {
        set(&mut s, "osc_1_unison_detune", v.clamp(0.0, 1.0));
    }

    // Sample
    if let Some(v) = num(mapped, "sample_level") {
        set(&mut s, "sample_level", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "sample_pan") {
        set(&mut s, "sample_pan", v.clamp(-1.0, 1.0));
    }

    // Filters (cutoff already in MIDI notes from the mapper; type as enum index).
    for (i, pre) in ["filter_1", "filter_2"].iter().enumerate() {
        let n = i + 1;
        if let Some(name) = enum_str(mapped, &format!("filter_{n}_type")) {
            set_i(&mut s, &format!("{pre}_model"), filter_type_idx(name));
        }
        if let Some(v) = num(mapped, &format!("filter_{n}_cutoff")) {
            set(&mut s, &format!("{pre}_cutoff"), v.clamp(8.0, 136.0));
        }
        if let Some(v) = num(mapped, &format!("filter_{n}_resonance")) {
            set(&mut s, &format!("{pre}_resonance"), v.clamp(0.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("filter_{n}_drive")) {
            set(&mut s, &format!("{pre}_drive"), v.max(0.0));
        }
        if let Some(v) = num(mapped, &format!("filter_{n}_mix")) {
            set(&mut s, &format!("{pre}_mix"), v.clamp(0.0, 1.0));
        }
    }

    // Envelopes (attack/decay/release in seconds, sustain 0..1).
    for (i, pre) in ["env_1", "env_2"].iter().enumerate() {
        let n = i + 1;
        if let Some(v) = num(mapped, &format!("env_{n}_attack")) {
            set(&mut s, &format!("{pre}_attack"), v.max(0.0));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_decay")) {
            set(&mut s, &format!("{pre}_decay"), v.max(0.0));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_sustain")) {
            set(&mut s, &format!("{pre}_sustain"), v.clamp(0.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_release")) {
            set(&mut s, &format!("{pre}_release"), v.max(0.0));
        }
    }

    // LFOs (frequency in Hz, phase 0..1, mode as enum index).
    for (i, pre) in ["lfo_1", "lfo_2"].iter().enumerate() {
        let n = i + 1;
        if let Some(v) = num(mapped, &format!("lfo_{n}_frequency")) {
            set(&mut s, &format!("{pre}_frequency"), v.max(0.0));
        }
        if let Some(v) = num(mapped, &format!("lfo_{n}_phase")) {
            set(&mut s, &format!("{pre}_phase"), v.clamp(0.0, 1.0));
        }
    }
    if let Some(name) = enum_str(mapped, "lfo_1_mode") {
        set_i(&mut s, "lfo_1_sync_type", lfo_mode_idx(name));
    }

    // Effects: reverb / delay / chorus / distortion.
    if let Some(v) = num(mapped, "reverb_size") {
        set(&mut s, "reverb_size", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "reverb_decay_time") {
        set(&mut s, "reverb_decay_time", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "reverb_mix") {
        set(&mut s, "reverb_dry_wet", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "delay_frequency") {
        set(&mut s, "delay_frequency", v.max(0.0));
    }
    if let Some(v) = num(mapped, "delay_feedback") {
        set(&mut s, "delay_feedback", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "delay_mix") {
        set(&mut s, "delay_dry_wet", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "chorus_frequency") {
        set(&mut s, "chorus_frequency", v.max(0.0));
    }
    if let Some(v) = num(mapped, "chorus_depth") {
        set(&mut s, "chorus_mod_depth", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "chorus_mix") {
        set(&mut s, "chorus_dry_wet", v.clamp(0.0, 1.0));
    }
    if let Some(name) = enum_str(mapped, "distortion_type") {
        set_i(&mut s, "distortion_type", distortion_type_idx(name));
    }
    if let Some(v) = num(mapped, "distortion_drive") {
        set(&mut s, "distortion_drive", v.max(0.0));
    }
    if let Some(v) = num(mapped, "distortion_mix") {
        set(&mut s, "distortion_mix", v.clamp(0.0, 1.0));
    }

    // Master
    if let Some(v) = num(mapped, "master_volume") {
        set(&mut s, "volume", v.clamp(0.0, 1.0));
    }

    s
}

/// Build the full `.vital` preset JSON `Value`.
pub fn build_preset(meta: &VitalMeta, mapped: &BTreeMap<String, ParamValue>) -> Value {
    let settings = build_settings(mapped);
    json!({
        "name": meta.name,
        "author": meta.author,
        "comments": meta.comments,
        "macro1": 0.0,
        "macro2": 0.0,
        "macro3": 0.0,
        "macro4": 0.0,
        "synth_version": VITAL_SYNTH_VERSION,
        "preset_style": "",
        "settings": Value::Object(settings),
    })
}

/// Serialize a `.vital` preset to pretty JSON bytes (what Vital writes).
pub fn build_vital(meta: &VitalMeta, mapped: &BTreeMap<String, ParamValue>) -> Vec<u8> {
    let v = build_preset(meta, mapped);
    serde_json::to_vec_pretty(&v).unwrap_or_default()
}
