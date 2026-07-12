// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Mapper spot-checks: known normalized inputs -> known real values, verified
// against the Python `vst_mappers/*.py` scaling tables.

use std::collections::BTreeMap;

use preset_core::mappers::{mapper_for, NormInput};
use preset_core::param::ParamValue;

fn inputs(pairs: &[(&str, NormInput)]) -> BTreeMap<String, NormInput> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn num(mapped: &BTreeMap<String, ParamValue>, k: &str) -> f64 {
    match mapped.get(k) {
        Some(ParamValue::Num(n)) => *n,
        other => panic!("expected Num for {k}, got {other:?}"),
    }
}

fn enum_(mapped: &BTreeMap<String, ParamValue>, k: &str) -> String {
    match mapped.get(k) {
        Some(ParamValue::Enum(s)) => s.clone(),
        other => panic!("expected Enum for {k}, got {other:?}"),
    }
}

#[test]
fn surge_spec_count_and_names() {
    let m = mapper_for("surge").unwrap();
    // The Python table has a fixed set; spot-check a few known parameters exist.
    assert!(m.spec("filter1_cutoff").is_some());
    assert!(m.spec("amp_attack").is_some());
    assert!(m.spec("osc1_type").is_some());
    assert!(m.spec("master_volume").is_some());
    // Count is stable (66 params: the ported table minus the parameters Surge
    // XT cannot express, which were removed so the model is not misled).
    assert_eq!(m.specs().len(), 66);
    // The removed dead params must be gone (they mapped to nothing in Surge).
    for gone in [
        "oversampling",
        "quality",
        "voice_priority",
        "scene_morph",
        "filter1_drive",
        "filter2_drive",
        "portamento_mode",
        "osc1_detune",
        "osc2_detune",
        "osc3_detune",
    ] {
        assert!(m.spec(gone).is_none(), "{gone} should have been removed");
    }
    // `character` now has the real 3 Surge values (was [warm,neutral,bright,extreme]).
    let ch = m.spec("character").unwrap();
    assert_eq!(ch.options, &["warm", "neutral", "bright"]);
}

#[test]
fn surge_filter_cutoff_exponential_endpoints() {
    let m = mapper_for("surge").unwrap();
    // filter1_cutoff: exponential 20..20000, curve 3.
    let lo = m.map_inputs(&inputs(&[("filter1_cutoff", NormInput::Num(0.0))]));
    assert!((num(&lo, "filter1_cutoff") - 20.0).abs() < 1e-3);
    let hi = m.map_inputs(&inputs(&[("filter1_cutoff", NormInput::Num(1.0))]));
    assert!((num(&hi, "filter1_cutoff") - 20000.0).abs() < 1e-2);
}

#[test]
fn surge_pitch_snaps_to_semitone() {
    let m = mapper_for("surge").unwrap();
    // osc1_pitch: linear -48..48 snapped to 1.0. v=0.5 -> 0 semitones.
    let mid = m.map_inputs(&inputs(&[("osc1_pitch", NormInput::Num(0.5))]));
    assert_eq!(num(&mid, "osc1_pitch"), 0.0);
    // v=1.0 -> 48.
    let top = m.map_inputs(&inputs(&[("osc1_pitch", NormInput::Num(1.0))]));
    assert_eq!(num(&top, "osc1_pitch"), 48.0);
}

#[test]
fn surge_enum_by_name_and_default() {
    let m = mapper_for("surge").unwrap();
    // Explicit enum name is honored.
    let by_name = m.map_inputs(&inputs(&[("filter1_type", NormInput::Str("lp24".into()))]));
    assert_eq!(enum_(&by_name, "filter1_type"), "lp24");
    // Unknown enum name falls back to first option.
    let bad = m.map_inputs(&inputs(&[(
        "filter1_type",
        NormInput::Str("nonsense".into()),
    )]));
    assert_eq!(enum_(&bad, "filter1_type"), "lp12");
    // Missing -> default (first option).
    let missing = m.map_inputs(&inputs(&[]));
    assert_eq!(enum_(&missing, "filter1_type"), "lp12");
}

#[test]
fn dexed_operator_count_and_discrete() {
    let m = mapper_for("dexed").unwrap();
    // 6 operators * 22 params + globals; spot-check per-operator names exist.
    assert!(m.spec("op1_level").is_some());
    assert!(m.spec("op6_coarse").is_some());
    assert!(m.spec("algorithm").is_some());
    assert!(m.spec("feedback").is_some());
    // op1_level: discrete 0..99, v=1.0 -> 99.
    let hi = m.map_inputs(&inputs(&[("op1_level", NormInput::Num(1.0))]));
    assert_eq!(num(&hi, "op1_level"), 99.0);
    // algorithm: discrete 1..32, v=0 -> 1.
    let lo = m.map_inputs(&inputs(&[("algorithm", NormInput::Num(0.0))]));
    assert_eq!(num(&lo, "algorithm"), 1.0);
}

#[test]
fn vital_filter_cutoff_and_enum() {
    let m = mapper_for("vital").unwrap();
    // filter_1_cutoff: exponential 8..136 (MIDI notes).
    let lo = m.map_inputs(&inputs(&[("filter_1_cutoff", NormInput::Num(0.0))]));
    assert!((num(&lo, "filter_1_cutoff") - 8.0).abs() < 1e-3);
    let hi = m.map_inputs(&inputs(&[("filter_1_cutoff", NormInput::Num(1.0))]));
    assert!((num(&hi, "filter_1_cutoff") - 136.0).abs() < 1e-2);
    // Enum type.
    let t = m.map_inputs(&inputs(&[(
        "filter_1_type",
        NormInput::Str("ladder".into()),
    )]));
    assert_eq!(enum_(&t, "filter_1_type"), "ladder");
}

#[test]
fn defaults_fill_all_params() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let d = m.defaults();
        assert_eq!(d.len(), m.specs().len(), "defaults count mismatch for {id}");
        // map_inputs with no inputs must equal defaults.
        let mapped = m.map_inputs(&BTreeMap::new());
        assert_eq!(mapped.len(), m.specs().len());
    }
}
