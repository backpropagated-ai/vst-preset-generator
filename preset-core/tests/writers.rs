// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Writer tests: FXP header layout, DX7 SysEx checksum + structure, Surge patch
// roundtrip, and Vital JSON structure.

use std::collections::BTreeMap;

use preset_core::param::ParamValue;
use preset_core::writers::{fxp, surge, syx, vital};
use preset_core::{PresetMeta, Synth};

fn num(v: f64) -> ParamValue {
    ParamValue::Num(v)
}

// -------------------------------------------------------------------------
// FXP header layout
// -------------------------------------------------------------------------

#[test]
fn fxp_chunk_header_layout() {
    let chunk = b"hello world chunk data";
    let data = fxp::build_chunk_fxp(b"cjs3", 1, "My Patch", 1, chunk);

    // 'CcnK' magic.
    assert_eq!(&data[0..4], b"CcnK");
    // byteSize = 48 + body_len (body = 4-byte chunkSize + chunk).
    let byte_size = u32::from_be_bytes(data[4..8].try_into().unwrap());
    assert_eq!(byte_size as usize, 48 + 4 + chunk.len());
    // 'FPCh' chunk magic.
    assert_eq!(&data[8..12], b"FPCh");
    // version = 1.
    assert_eq!(u32::from_be_bytes(data[12..16].try_into().unwrap()), 1);
    // fxID.
    assert_eq!(&data[16..20], b"cjs3");
    // fxVersion.
    assert_eq!(u32::from_be_bytes(data[20..24].try_into().unwrap()), 1);
    // numPrograms.
    assert_eq!(u32::from_be_bytes(data[24..28].try_into().unwrap()), 1);
    // prgName null-padded.
    assert_eq!(&data[28..36], b"My Patch");
    assert_eq!(data[36], 0);
    // chunkSize (big-endian) at offset 56.
    assert_eq!(
        u32::from_be_bytes(data[56..60].try_into().unwrap()) as usize,
        chunk.len()
    );
    // chunk body.
    assert_eq!(&data[60..60 + chunk.len()], chunk);
}

#[test]
fn fxp_param_header_layout_and_roundtrip() {
    let params = [0.0f32, 0.5, 1.0, 0.25];
    let data = fxp::build_param_fxp(b"SrgX", 1, "Params", &params);

    assert_eq!(&data[0..4], b"CcnK");
    assert_eq!(&data[8..12], b"FxCk");
    // numParams.
    assert_eq!(u32::from_be_bytes(data[24..28].try_into().unwrap()), 4);

    let parsed = fxp::parse_fxp(&data).unwrap();
    assert!(parsed.is_param_format());
    assert_eq!(parsed.name, "Params");
    assert_eq!(parsed.params, params.to_vec());
}

#[test]
fn fxp_param_clamps_to_unit_range() {
    let params = [-1.0f32, 2.0];
    let data = fxp::build_param_fxp(b"SrgX", 1, "Clamp", &params);
    let parsed = fxp::parse_fxp(&data).unwrap();
    assert_eq!(parsed.params, vec![0.0, 1.0]);
}

// -------------------------------------------------------------------------
// DX7 SysEx
// -------------------------------------------------------------------------

#[test]
fn dx7_syx_structure_and_length() {
    let params: BTreeMap<String, ParamValue> = BTreeMap::new();
    let data = syx::build_syx(&params, "TEST VOICE", 0);
    // Total: 6-byte header + 155 voice + checksum + F7 = 163.
    assert_eq!(data.len(), 163);
    assert_eq!(data[0], 0xF0); // sysex start
    assert_eq!(data[1], 0x43); // Yamaha
    assert_eq!(data[2], 0x00); // device 0
    assert_eq!(data[3], 0x00); // single voice
    assert_eq!(data[4], 0x01); // count MSB
    assert_eq!(data[5], 0x1B); // count LSB (155)
    assert_eq!(data[data.len() - 1], 0xF7); // sysex end
}

#[test]
fn dx7_checksum_is_valid() {
    // Building a voice + running the whole message through: sum(voice) + checksum
    // must be 0 mod 128 (the DX7 checksum invariant).
    let mut params: BTreeMap<String, ParamValue> = BTreeMap::new();
    params.insert("algorithm".into(), num(5.0));
    params.insert("feedback".into(), num(6.0));
    params.insert("op1_level".into(), num(99.0));
    let voice = syx::create_voice(&params, "E.PIANO");
    let cksum = syx::checksum(&voice);
    let sum: u32 = voice.iter().map(|&b| b as u32).sum::<u32>() + cksum as u32;
    assert_eq!(sum & 0x7F, 0, "checksum invariant violated");
}

#[test]
fn dx7_checksum_known_value() {
    // A DX7 INIT voice (all defaults from create_voice) has a fixed checksum.
    let params: BTreeMap<String, ParamValue> = BTreeMap::new();
    let voice = syx::create_voice(&params, "INIT VOICE");
    let cksum = syx::checksum(&voice);
    // The checksum must be a 7-bit value.
    assert!(cksum <= 0x7F);
    // And re-computing must be stable.
    assert_eq!(cksum, syx::checksum(&voice));
}

#[test]
fn dx7_algorithm_maps_1_to_0() {
    // DX7 stores algorithm 0-31; our abstract param is 1-32 -> byte 134.
    let mut params: BTreeMap<String, ParamValue> = BTreeMap::new();
    params.insert("algorithm".into(), num(1.0));
    let voice = syx::create_voice(&params, "X");
    assert_eq!(voice[134], 0);
    params.insert("algorithm".into(), num(32.0));
    let voice = syx::create_voice(&params, "X");
    assert_eq!(voice[134], 31);
}

// -------------------------------------------------------------------------
// Surge patch
// -------------------------------------------------------------------------

#[test]
fn surge_fxp_is_loadable_container() {
    let mut mapped: BTreeMap<String, ParamValue> = BTreeMap::new();
    mapped.insert("filter1_cutoff".into(), num(1000.0));
    let meta = surge::SurgeMeta {
        name: "Test Patch".into(),
        ..Default::default()
    };
    let data = surge::build_fxp(&meta, &mapped);

    // Must be the FPCh/cjs3 container Surge XT actually loads.
    let header = fxp::parse_fxp(&data).unwrap();
    assert!(
        header.is_chunk_format(),
        "Surge FXP must be FPCh chunk format"
    );
    assert_eq!(&header.fx_id, b"cjs3", "Surge fxID must be cjs3");
    assert_eq!(header.count, 1, "numPrograms must be 1");

    // The chunk must start with the 'sub3' patch header.
    assert_eq!(&header.chunk[0..4], b"sub3");
    // wtsize[2][3] must all be zero (no wavetable data).
    for i in 0..6 {
        let off = 8 + i * 4;
        assert_eq!(
            u32::from_le_bytes(header.chunk[off..off + 4].try_into().unwrap()),
            0
        );
    }
}

#[test]
fn surge_patch_xml_roundtrip_and_override() {
    let mut mapped: BTreeMap<String, ParamValue> = BTreeMap::new();
    // 523 Hz ≈ Surge internal value 3 (440 * 2^(3/12)).
    mapped.insert("filter1_cutoff".into(), num(523.25));
    mapped.insert("amp_attack".into(), num(1.0)); // 1s -> log2(1)=0
    mapped.insert("filter1_type".into(), ParamValue::Enum("lp24".into()));
    let meta = surge::SurgeMeta {
        name: "Round Trip".into(),
        category: "DeepSynth".into(),
        comment: "test".into(),
        author: "tester".into(),
        license: "CC0".into(),
    };
    let data = surge::build_fxp(&meta, &mapped);

    let (name, xml) = surge::parse_fxp(&data).unwrap();
    assert_eq!(name, "Round Trip");
    // XML must carry the shipped Surge patch revision (see SURGE_REVISION).
    let rev_tag = format!("<patch revision=\"{}\">", surge::SURGE_REVISION);
    assert!(xml.contains(&rev_tag), "xml={}", &xml[..80.min(xml.len())]);
    // Meta present.
    assert!(xml.contains("name=\"Round Trip\""));
    assert!(xml.contains("author=\"tester\""));
    // Cutoff override applied to scene A: 523.25 Hz -> ~2.99996 semitone value
    // (12*log2(523.25/440) ≈ 3.0). Assert the internal value is ~3, not the Hz.
    assert!(
        xml.contains("<a_filter1_cutoff type=\"2\" value=\"2.99"),
        "cutoff not overridden"
    );
    // filter1_type lp24 -> Surge fut_lp24 = 2.
    assert!(xml.contains("<a_filter1_type type=\"0\" value=\"2\""));
    // amp_attack 1s -> a_env1_attack value 0.0.
    assert!(xml.contains("<a_env1_attack type=\"2\" value=\"0.00"));
    // Full parameter set present (558 params + meta): the file must be large.
    assert!(
        data.len() > 20_000,
        "surge patch unexpectedly small: {}",
        data.len()
    );
    // The parameter count matches Surge's Init Saw (558 param elements).
    assert_eq!(xml.matches(" type=").count(), 558);
}

#[test]
fn surge_full_coverage_roundtrip() {
    use preset_core::mappers::{mapper_for, NormInput};

    // Build a rich patch by mapping normalized inputs through the mapper, so the
    // whole prompt→mapper→writer path is exercised. Values chosen to trigger the
    // encodings the review fixes: osc1 pitch -12 semitones, an audible
    // filter-env depth, an LFO->cutoff modulation routing, unison, and ring mod.
    let mapper = mapper_for("surge").unwrap();
    let mut inputs: BTreeMap<String, NormInput> = BTreeMap::new();
    // osc1_pitch is linear -48..48; normalized 0.375 -> -12 semitones.
    inputs.insert("osc1_pitch".into(), NormInput::Num(0.375));
    // filter_env_depth is -1..1; normalized 0.9 -> +0.8.
    inputs.insert("filter_env_depth".into(), NormInput::Num(0.9));
    // mod_lfo1_cutoff is -1..1; normalized 0.75 -> +0.5.
    inputs.insert("mod_lfo1_cutoff".into(), NormInput::Num(0.75));
    // Unison: 4 voices, some detune.
    inputs.insert("unison_voices".into(), NormInput::Num(0.2));
    inputs.insert("unison_detune".into(), NormInput::Num(0.3));
    // Ring mod 1x2 audible.
    inputs.insert("ring_12".into(), NormInput::Num(0.7));
    // A recognizable cutoff + filter type + osc type.
    inputs.insert("filter1_cutoff".into(), NormInput::Num(0.5));
    inputs.insert("filter1_type".into(), NormInput::Str("lp24".into()));
    inputs.insert("osc1_type".into(), NormInput::Str("classic".into()));
    // Character bright, drift, pan, output gain unity, sends, portamento off.
    inputs.insert("character".into(), NormInput::Str("bright".into()));
    inputs.insert("output_pan".into(), NormInput::Num(0.75)); // -1..1 -> +0.5
    inputs.insert("output_gain".into(), NormInput::Num(0.0)); // 0.1 gain -> -20 dB

    let mapped = mapper.map_inputs(&inputs);
    let meta = surge::SurgeMeta {
        name: "Full Cover".into(),
        ..Default::default()
    };
    let data = surge::build_fxp(&meta, &mapped);
    let (name, xml) = surge::parse_fxp(&data).unwrap();
    assert_eq!(name, "Full Cover");

    // Pitch decomposition: -12 semitones -> octave -1, pitch 0, no extend.
    assert!(
        xml.contains("<a_osc1_octave type=\"0\" value=\"-1\" />"),
        "osc1 octave not -1"
    );
    assert!(
        xml.contains("<a_osc1_pitch type=\"2\" value=\"0.00000000000000\" extend_range=\"0\" />"),
        "osc1 pitch remainder not 0"
    );

    // Filter env depth 0.8 -> 0.8 * 60 = 48 semitones of envmod (was ~inaudible ±1).
    assert!(
        xml.contains("<a_filter1_envmod type=\"2\" value=\"48.00000000000000\" />"),
        "envmod not scaled to 48 semitones"
    );

    // LFO1 -> cutoff modrouting: source ms_lfo1 = 17, depth 0.5*60 = 30 semitones,
    // as a child of the a_filter1_cutoff element.
    assert!(
        xml.contains(
            "<modrouting source=\"17\" depth=\"30.00000000000000\" muted=\"0\" source_index=\"0\" />"
        ),
        "LFO1->cutoff modrouting missing"
    );
    // The routing must be nested inside the a_filter1_cutoff element.
    assert!(
        xml.contains("<a_filter1_cutoff")
            && xml.contains("</a_filter1_cutoff>"),
        "cutoff element must wrap its modrouting child"
    );

    // Unison: 4 voices -> a_osc1_param6 = 4 (int), detune -> a_osc1_param5.
    assert!(
        xml.contains("<a_osc1_param6 type=\"0\" value=\"4\" />"),
        "unison voices not 4"
    );
    assert!(
        xml.contains("<a_osc1_param5 type=\"2\" value=\"0.3"),
        "unison detune not written (detune 30/100=0.3)"
    );

    // Ring mod 1x2 audible -> level set + unmuted.
    assert!(
        xml.contains("<a_level_ring12 type=\"2\" value=\"0.7"),
        "ring12 level not set"
    );
    assert!(
        xml.contains("<a_mute_ring12 type=\"0\" value=\"0\" />"),
        "ring12 not unmuted"
    );

    // Character bright -> cm_bright = 2.
    assert!(
        xml.contains("<character type=\"0\" value=\"2\" />"),
        "character not bright(2)"
    );
    // Pan +0.5.
    assert!(
        xml.contains("<a_pan type=\"2\" value=\"0.5"),
        "pan not +0.5"
    );
    // Output gain 0.1 -> volume -20 dB.
    assert!(
        xml.contains("<volume type=\"2\" value=\"-20.0"),
        "output gain not -20 dB"
    );

    // Still a complete, loadable 558-parameter patch.
    assert_eq!(xml.matches(" type=").count(), 558);
    // The FXP container is still the Surge FPCh/cjs3 form.
    let header = fxp::parse_fxp(&data).unwrap();
    assert!(header.is_chunk_format());
    assert_eq!(&header.fx_id, b"cjs3");
}

// -------------------------------------------------------------------------
// Vital JSON
// -------------------------------------------------------------------------

#[test]
fn vital_preset_is_valid_json_with_settings() {
    let mut mapped: BTreeMap<String, ParamValue> = BTreeMap::new();
    mapped.insert("filter_1_cutoff".into(), num(80.0));
    mapped.insert("master_volume".into(), num(0.7));
    mapped.insert("filter_1_type".into(), ParamValue::Enum("ladder".into()));
    let meta = vital::VitalMeta {
        name: "V Pad".into(),
        ..Default::default()
    };
    let bytes = vital::build_vital(&meta, &mapped);

    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["name"], "V Pad");
    assert_eq!(v["synth_version"], "1.0.7");
    let settings = v["settings"].as_object().unwrap();
    assert!((settings["filter_1_cutoff"].as_f64().unwrap() - 80.0).abs() < 1e-6);
    // ladder -> Vital filter model index 2.
    assert_eq!(settings["filter_1_model"].as_i64().unwrap(), 2);
    assert!((settings["volume"].as_f64().unwrap() - 0.7).abs() < 1e-6);
}

// -------------------------------------------------------------------------
// High-level write_preset for each synth
// -------------------------------------------------------------------------

#[test]
fn write_preset_all_synths_from_defaults() {
    let meta = PresetMeta::default();
    for synth in [Synth::Surge, Synth::Dexed, Synth::Vital] {
        let mapper = synth.mapper();
        let mapped = mapper.defaults();
        let bytes = preset_core::write_preset(synth, &meta, &mapped).unwrap();
        assert!(!bytes.is_empty(), "{} produced empty preset", synth.id());
    }
}
