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

/// Vital LFO sync-type index (`lfo_N_sync_type`; synth_lfo.h:48 kSyncNames:
/// 0 Trigger, 1 Sync, 2 Envelope, 3 Sustain Envelope, 4 Loop Point, 5 Loop Hold).
fn lfo_mode_idx(name: &str) -> i64 {
    match name {
        "trigger" => 0,
        "sync" => 1,
        "envelope" => 2,
        "sustain_envelope" => 3,
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

// ---------------------------------------------------------------------------
// Vital storage-domain conversions.
//
// `.vital` settings store RAW engine-domain values (load_save.cpp:95-98), and
// the engine applies a per-parameter skew (value_bridge.h:134-152). Each
// converter below inverts the physical unit into that stored domain, verified
// against Vital's ValueDetails table (synth_parameters.cpp, lines cited).
// ---------------------------------------------------------------------------

/// Amplitude 0..1 → quadratic-scale stored value (`osc_N_level`, `sample_level`;
/// synth_parameters.cpp:484 kQuadratic): amplitude = stored², so stored = √amp.
fn amp_to_quadratic(amp: f64) -> f64 {
    amp.clamp(0.0, 1.0).sqrt()
}

/// Seconds → quartic-scale envelope time (`env_N_attack/decay/release`;
/// synth_parameters.cpp:354 kQuartic, max 2.37842 → 32 s): stored = s^(1/4).
fn sec_to_quartic(s: f64) -> f64 {
    s.max(0.0).powf(0.25).clamp(0.0, 2.37842)
}

/// Hz → exponential-scale stored value (`lfo_N_frequency` −7..9,
/// `delay_frequency` −2..9, `chorus_frequency` −6..3; kExponential): the engine
/// reads Hz = 2^stored (synth_module.cpp:130-141), so stored = log2(Hz).
fn hz_to_log2(hz: f64, lo: f64, hi: f64) -> f64 {
    hz.max(1e-4).log2().clamp(lo, hi)
}

/// Seconds → reverb decay stored value (`reverb_decay_time` −6..6,
/// kExponential): seconds = 2^stored.
fn sec_to_log2(s: f64) -> f64 {
    s.max(1e-4).log2().clamp(-6.0, 6.0)
}

/// Master volume 0..1 (linear amplitude) → Vital `volume` stored value
/// (synth_parameters.cpp:201, kSquareRoot with −80 dB offset): dB = √stored −
/// 80, 0 dB ⇔ 6400. Stored = (dB + 80)², dB = 20·log10(v), floor −80 dB.
fn volume_to_stored(v: f64) -> f64 {
    if v <= 0.0 {
        return 0.0;
    }
    let db = (20.0 * v.log10()).clamp(-80.0, 6.0);
    (db + 80.0).powi(2)
}

/// Unison detune 0..1 → stored value (`osc_N_unison_detune` 0..10, kQuadratic,
/// percent = stored²; synth_parameters.cpp:474): stored = 10·√v.
fn detune_to_stored(v: f64) -> f64 {
    10.0 * v.clamp(0.0, 1.0).sqrt()
}

/// Build the `settings` object mapping abstract params onto Vital's real keys,
/// converting each into Vital's stored domain (see the converters above).
///
/// Sections whose Vital modules default to OFF (`osc_2/3_on`, `sample_on`,
/// `filter_N_on`, every effect `_on`; synth_parameters.cpp defaults) are
/// auto-enabled when their level/mix is audibly non-zero — otherwise the
/// written parameters would be silently inert.
pub fn build_settings(mapped: &BTreeMap<String, ParamValue>) -> Map<String, Value> {
    let mut s = Map::new();
    let audible = |v: f64| v > 1e-4;

    // Oscillators (level quadratic, tune in semitones, pan -1..1, wave_frame).
    for (i, pre) in ["osc_1", "osc_2", "osc_3"].iter().enumerate() {
        let abs = format!("osc_{}", i + 1);
        if let Some(v) = num(mapped, &format!("{abs}_level")) {
            set(&mut s, &format!("{pre}_level"), amp_to_quadratic(v));
            set_i(&mut s, &format!("{pre}_on"), audible(v) as i64);
        }
        if let Some(v) = num(mapped, &format!("{abs}_pan")) {
            set(&mut s, &format!("{pre}_pan"), v.clamp(-1.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("{abs}_tune")) {
            set(&mut s, &format!("{pre}_transpose"), v.round().clamp(-48.0, 48.0));
        }
        if let Some(v) = num(mapped, &format!("{abs}_wave")) {
            // Wavetable scan position: stored as a frame index 0..256
            // (`osc_N_wave_frame`, synth_parameters.cpp:492).
            set(&mut s, &format!("{pre}_wave_frame"), (v.clamp(0.0, 1.0) * 256.0).round());
        }
    }
    if let Some(v) = num(mapped, "osc_1_unison_voices") {
        set_i(&mut s, "osc_1_unison_voices", (v.round() as i64).clamp(1, 16));
    }
    if let Some(v) = num(mapped, "osc_1_unison_detune") {
        set(&mut s, "osc_1_unison_detune", detune_to_stored(v));
    }

    // Sample
    if let Some(v) = num(mapped, "sample_level") {
        set(&mut s, "sample_level", amp_to_quadratic(v));
        set_i(&mut s, "sample_on", audible(v) as i64);
    }
    if let Some(v) = num(mapped, "sample_pan") {
        set(&mut s, "sample_pan", v.clamp(-1.0, 1.0));
    }

    // Filters (cutoff already in MIDI notes from the mapper; type as enum
    // index). A filter with non-zero mix is switched on; osc_1's default
    // destination is already FILTER 1 and osc_2's FILTER 2
    // (synth_parameters.cpp:586-590), so enabling the filter is sufficient.
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
            set(&mut s, &format!("{pre}_drive"), v.clamp(0.0, 20.0));
        }
        if let Some(v) = num(mapped, &format!("filter_{n}_mix")) {
            set(&mut s, &format!("{pre}_mix"), v.clamp(0.0, 1.0));
            set_i(&mut s, &format!("{pre}_on"), audible(v) as i64);
        }
    }

    // Envelopes: attack/decay/release come from the mapper in seconds and are
    // stored on Vital's quartic scale; sustain is linear 0..1.
    for (i, pre) in ["env_1", "env_2"].iter().enumerate() {
        let n = i + 1;
        if let Some(v) = num(mapped, &format!("env_{n}_attack")) {
            set(&mut s, &format!("{pre}_attack"), sec_to_quartic(v));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_decay")) {
            set(&mut s, &format!("{pre}_decay"), sec_to_quartic(v));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_sustain")) {
            set(&mut s, &format!("{pre}_sustain"), v.clamp(0.0, 1.0));
        }
        if let Some(v) = num(mapped, &format!("env_{n}_release")) {
            set(&mut s, &format!("{pre}_release"), sec_to_quartic(v));
        }
    }

    // LFOs: frequency stored as log2(Hz); `lfo_N_sync` must be forced to 0
    // (seconds mode) or the frequency knob is ignored — Vital defaults the
    // sync selector to Tempo (synth_parameters.cpp, kFrequencySyncNames).
    for (i, pre) in ["lfo_1", "lfo_2"].iter().enumerate() {
        let n = i + 1;
        if let Some(v) = num(mapped, &format!("lfo_{n}_frequency")) {
            set(&mut s, &format!("{pre}_frequency"), hz_to_log2(v, -7.0, 9.0));
            set_i(&mut s, &format!("{pre}_sync"), 0);
        }
        if let Some(v) = num(mapped, &format!("lfo_{n}_phase")) {
            set(&mut s, &format!("{pre}_phase"), v.clamp(0.0, 1.0));
        }
    }
    if let Some(name) = enum_str(mapped, "lfo_1_mode") {
        set_i(&mut s, "lfo_1_sync_type", lfo_mode_idx(name));
    }

    // Effects: reverb / delay / chorus / distortion. Each is enabled when its
    // dry/wet is non-zero; time-domain knobs get the same sync=0 treatment.
    if let Some(v) = num(mapped, "reverb_size") {
        set(&mut s, "reverb_size", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "reverb_decay_time") {
        set(&mut s, "reverb_decay_time", sec_to_log2(v));
    }
    if let Some(v) = num(mapped, "reverb_mix") {
        set(&mut s, "reverb_dry_wet", v.clamp(0.0, 1.0));
        set_i(&mut s, "reverb_on", audible(v) as i64);
    }
    if let Some(v) = num(mapped, "delay_frequency") {
        set(&mut s, "delay_frequency", hz_to_log2(v, -2.0, 9.0));
        set_i(&mut s, "delay_sync", 0);
    }
    if let Some(v) = num(mapped, "delay_feedback") {
        set(&mut s, "delay_feedback", v.clamp(-1.0, 1.0));
    }
    if let Some(v) = num(mapped, "delay_mix") {
        set(&mut s, "delay_dry_wet", v.clamp(0.0, 1.0));
        set_i(&mut s, "delay_on", audible(v) as i64);
    }
    if let Some(v) = num(mapped, "chorus_frequency") {
        set(&mut s, "chorus_frequency", hz_to_log2(v, -6.0, 3.0));
        set_i(&mut s, "chorus_sync", 0);
    }
    if let Some(v) = num(mapped, "chorus_depth") {
        set(&mut s, "chorus_mod_depth", v.clamp(0.0, 1.0));
    }
    if let Some(v) = num(mapped, "chorus_mix") {
        set(&mut s, "chorus_dry_wet", v.clamp(0.0, 1.0));
        set_i(&mut s, "chorus_on", audible(v) as i64);
    }
    if let Some(name) = enum_str(mapped, "distortion_type") {
        set_i(&mut s, "distortion_type", distortion_type_idx(name));
    }
    if let Some(v) = num(mapped, "distortion_drive") {
        set(&mut s, "distortion_drive", v.clamp(-30.0, 30.0));
    }
    if let Some(v) = num(mapped, "distortion_mix") {
        set(&mut s, "distortion_mix", v.clamp(0.0, 1.0));
        set_i(&mut s, "distortion_on", audible(v) as i64);
    }

    // Master volume: linear amplitude → Vital's (dB+80)² domain.
    if let Some(v) = num(mapped, "master_volume") {
        set(&mut s, "volume", volume_to_stored(v.clamp(0.0, 1.0)));
    }

    // Modulation matrix: one optional connection. Vital stores connections in
    // settings["modulations"] as {source, destination} objects with the depth
    // in a `modulation_N_amount` parameter (load_save.cpp:104-119; unknown
    // sources/destinations are skipped at load, :180-183).
    let mod_amount = num(mapped, "mod_1_amount").unwrap_or(0.0);
    if audible(mod_amount.abs()) {
        if let (Some(src), Some(dst)) = (
            enum_str(mapped, "mod_1_source"),
            enum_str(mapped, "mod_1_destination"),
        ) {
            s.insert(
                "modulations".to_string(),
                json!([{ "source": src, "destination": dst }]),
            );
            set(&mut s, "modulation_1_amount", mod_amount.clamp(-1.0, 1.0));
        }
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
