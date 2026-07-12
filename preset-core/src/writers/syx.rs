// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// DX7 / Dexed SysEx (.syx) writer.
//
// Byte-exact port of the legacy Python `generators/syx_generator.py`. The output
// is a single-voice DX7 SysEx dump:
//
//   F0 43 <dev> 00 01 1B <155 voice bytes> <checksum> F7   (= 163 bytes)
//
// The 155-byte voice is a packed DX7 "VCED" voice: 6 operators × 21 bytes
// (stored in reverse order, OP6..OP1) + 29 global bytes. The checksum is the
// two's-complement of the running 7-bit sum of the 155 voice bytes.
//
// To guarantee byte-for-byte parity with the Python reference, the packing
// order, defaults, and clamping here mirror `syx_generator.py` exactly,
// including its quirks (e.g. `_init_voice_defaults` fills rates/levels and the
// per-operator loop only overrides fields present in the input dict).

use std::collections::BTreeMap;

use crate::param::ParamValue;

/// MIDI SysEx start byte.
pub const SYSEX_START: u8 = 0xF0;
/// MIDI SysEx end byte.
pub const SYSEX_END: u8 = 0xF7;
/// Yamaha manufacturer id.
pub const YAMAHA_ID: u8 = 0x43;
/// DX7 single-voice dump format byte.
pub const DX7_SINGLE_VOICE: u8 = 0x00;

/// An input value for the SysEx writer — a number or an enum/string.
///
/// This is the mapped-parameter view the writer consumes; it mirrors the Python
/// dict where values are ints, floats, bools, or option strings.
pub type Params = BTreeMap<String, ParamValue>;

/// `int(value)` semantics matching Python: bool → 0/1, float → truncate, enum → 0.
fn as_int(v: &ParamValue) -> i64 {
    match v {
        ParamValue::Num(n) => *n as i64, // Python int() truncates toward zero
        ParamValue::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        ParamValue::Enum(_) => 0,
    }
}

/// Clamp to `[min, max]` after truncating to int, matching Python `_clamp`.
fn clamp(v: Option<&ParamValue>, min_val: i64, max_val: i64) -> Option<u8> {
    v.map(|val| {
        let i = match val {
            ParamValue::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            ParamValue::Num(n) => *n as i64,
            ParamValue::Enum(_) => min_val,
        };
        i.clamp(min_val, max_val) as u8
    })
}

/// 7-bit clamp (0..99 for DX7), matching Python `_clamp_7bit`.
fn clamp_7bit(v: Option<&ParamValue>) -> Option<u8> {
    clamp(v, 0, 99)
}

fn curve_to_int(curve: &str) -> u8 {
    match curve {
        "lin_neg" => 0,
        "exp_neg" => 1,
        "exp_pos" => 2,
        "lin_pos" => 3,
        _ => 0,
    }
}

fn lfo_wave_to_int(wave: &str) -> u8 {
    match wave {
        "triangle" => 0,
        "saw_down" => 1,
        "saw_up" => 2,
        "square" => 3,
        "sine" => 4,
        "s&h" => 5,
        _ => 0,
    }
}

/// DX7 detune: input `-7..7` → byte `0..14` (7 = center). Matches Python.
fn detune_to_dx7(detune: f64) -> u8 {
    let d = detune.round() as i64;
    let d = d.clamp(-7, 7);
    (d + 7) as u8
}

/// DX7 transpose: input `-24..24` → byte `0..48` (24 = center). Matches Python.
fn transpose_to_dx7(transpose: f64) -> u8 {
    let t = transpose.round() as i64;
    let t = t.clamp(-24, 24);
    (t + 24) as u8
}

/// DX7 checksum: two's-complement of the running 7-bit sum. Matches Python.
pub fn checksum(data: &[u8]) -> u8 {
    let mut sum: u32 = 0;
    for &b in data {
        sum = (sum + b as u32) & 0x7F;
    }
    ((!sum).wrapping_add(1) & 0x7F) as u8
}

/// Look up a numeric input as f64 (for detune/transpose).
fn num_or(params: &Params, key: &str, default: f64) -> f64 {
    match params.get(key) {
        Some(ParamValue::Num(n)) => *n,
        Some(ParamValue::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => default,
    }
}

/// Look up an enum/string input, with a default.
fn str_or<'a>(params: &'a Params, key: &str, default: &'a str) -> &'a str {
    match params.get(key) {
        Some(ParamValue::Enum(s)) => s.as_str(),
        _ => default,
    }
}

/// Build the 155-byte DX7 voice from mapped parameters (port of
/// `_create_dx7_voice`, including its default-fill and overwrite ordering).
pub fn create_voice(params: &Params, preset_name: &str) -> [u8; 155] {
    let mut voice = [0u8; 155];

    // _init_voice_defaults: for each of the 6 operator blocks (0,21,42,...105)
    for op_offset in (0..126).step_by(21) {
        voice[op_offset..op_offset + 4].copy_from_slice(&[99, 99, 99, 99]); // rates
        voice[op_offset + 4..op_offset + 8].copy_from_slice(&[99, 99, 99, 0]); // levels
        voice[op_offset + 15] = 99; // operator level
        voice[op_offset + 16] = 1; // coarse (ratio 1)
    }
    voice[134] = 0; // default algorithm 1

    // Per-operator overrides. DX7 stores operators reversed: op1 -> offset 105.
    for op_num in 1..=6 {
        let op_offset = (6 - op_num) * 21;
        let pre = format!("op{op_num}_");

        // Envelope rates R1-R4 -> offset+0..3
        for (i, suffix) in ["r1", "r2", "r3", "r4"].iter().enumerate() {
            let k = format!("{pre}eg_{suffix}");
            if let Some(b) = clamp_7bit(params.get(&k)) {
                voice[op_offset + i] = b;
            }
        }
        // Envelope levels L1-L4 -> offset+4..7
        for (i, suffix) in ["l1", "l2", "l3", "l4"].iter().enumerate() {
            let k = format!("{pre}eg_{suffix}");
            if let Some(b) = clamp_7bit(params.get(&k)) {
                voice[op_offset + 4 + i] = b;
            }
        }
        // Level scaling break/depths
        if let Some(b) = clamp_7bit(params.get(&format!("{pre}lev_scale_break"))) {
            voice[op_offset + 8] = b;
        }
        if let Some(b) = clamp_7bit(params.get(&format!("{pre}lev_scale_left_depth"))) {
            voice[op_offset + 9] = b;
        }
        if let Some(b) = clamp_7bit(params.get(&format!("{pre}lev_scale_right_depth"))) {
            voice[op_offset + 10] = b;
        }
        // Scaling curves packed: (left << 2) | right
        let left_curve = curve_to_int(str_or(
            params,
            &format!("{pre}lev_scale_left_curve"),
            "lin_neg",
        ));
        let right_curve = curve_to_int(str_or(
            params,
            &format!("{pre}lev_scale_right_curve"),
            "lin_neg",
        ));
        voice[op_offset + 11] = (left_curve << 2) | right_curve;

        // Rate scaling / sensitivities
        if let Some(b) = clamp_7bit(params.get(&format!("{pre}rate_scale"))) {
            voice[op_offset + 12] = b;
        }
        if let Some(b) = clamp(params.get(&format!("{pre}amp_mod_sens")), 0, 3) {
            voice[op_offset + 13] = b;
        }
        if let Some(b) = clamp(params.get(&format!("{pre}vel_sens")), 0, 7) {
            voice[op_offset + 14] = b;
        }
        if let Some(b) = clamp_7bit(params.get(&format!("{pre}level"))) {
            voice[op_offset + 15] = b;
        }

        // Frequency: mode (bit 5) | coarse (0..31)
        let mode = if str_or(params, &format!("{pre}mode"), "ratio") == "ratio" {
            0u8
        } else {
            1u8
        };
        let coarse = match params.get(&format!("{pre}coarse")) {
            Some(v) => (as_int(v)).clamp(0, 31) as u8,
            None => 1u8, // Python .get(..., 1)
        };
        voice[op_offset + 16] = (mode << 5) | coarse;

        if let Some(b) = clamp_7bit(params.get(&format!("{pre}fine"))) {
            voice[op_offset + 17] = b;
        }

        // Detune -> offset+18
        let detune = num_or(params, &format!("{pre}detune"), 0.0);
        voice[op_offset + 18] = detune_to_dx7(detune);
    }

    // Global: pitch envelope 126..133
    for (i, suffix) in ["r1", "r2", "r3", "r4"].iter().enumerate() {
        if let Some(b) = clamp_7bit(params.get(&format!("pitch_eg_{suffix}"))) {
            voice[126 + i] = b;
        }
    }
    for (i, suffix) in ["l1", "l2", "l3", "l4"].iter().enumerate() {
        if let Some(b) = clamp_7bit(params.get(&format!("pitch_eg_{suffix}"))) {
            voice[130 + i] = b;
        }
    }

    // Algorithm (1..32 -> byte 0..31)
    if let Some(v) = params.get("algorithm") {
        voice[134] = (as_int(v) - 1).clamp(0, 31) as u8;
    }
    // Feedback
    if let Some(b) = clamp(params.get("feedback"), 0, 7) {
        voice[135] = b;
    }
    // Osc sync
    if let Some(v) = params.get("osc_sync") {
        voice[136] = if matches!(v, ParamValue::Bool(true)) || as_int(v) != 0 {
            1
        } else {
            0
        };
    }

    // LFO 137..140
    if let Some(b) = clamp_7bit(params.get("lfo_speed")) {
        voice[137] = b;
    }
    if let Some(b) = clamp_7bit(params.get("lfo_delay")) {
        voice[138] = b;
    }
    if let Some(b) = clamp_7bit(params.get("lfo_pmd")) {
        voice[139] = b;
    }
    if let Some(b) = clamp_7bit(params.get("lfo_amd")) {
        voice[140] = b;
    }
    // LFO sync + wave packed: (sync << 3) | wave
    let lfo_sync = match params.get("lfo_sync") {
        Some(ParamValue::Bool(true)) => 1u8,
        Some(v) if as_int(v) != 0 => 1u8,
        _ => 0u8,
    };
    let lfo_wave = lfo_wave_to_int(str_or(params, "lfo_wave", "triangle"));
    voice[141] = (lfo_sync << 3) | lfo_wave;
    // LFO pitch mod sensitivity
    if let Some(b) = clamp(params.get("lfo_pitch_mod_sens"), 0, 7) {
        voice[142] = b;
    }

    // Transpose
    let transpose = num_or(params, "transpose", 0.0);
    voice[143] = transpose_to_dx7(transpose);

    // Voice name 144..153 (10 chars, uppercased ASCII, space-padded)
    let name = preset_name;
    let upper = name.to_uppercase();
    let name_bytes: Vec<u8> = upper.bytes().filter(|b| b.is_ascii()).take(10).collect();
    for (i, &b) in name_bytes.iter().enumerate() {
        voice[144 + i] = b;
    }
    for i in name_bytes.len()..10 {
        voice[144 + i] = 0x20; // space pad
    }

    // Operator on/off flags -> byte 154, all on
    voice[154] = 0x3F;

    voice
}

/// Build the full SysEx (.syx) dump: `F0 43 dev 00 01 1B <voice> <cksum> F7`.
///
/// `device_id` defaults to 0.
pub fn build_syx(params: &Params, preset_name: &str, device_id: u8) -> Vec<u8> {
    let voice = create_voice(params, preset_name);
    let cksum = checksum(&voice);

    let mut out = Vec::with_capacity(6 + 155 + 2);
    out.push(SYSEX_START);
    out.push(YAMAHA_ID);
    out.push(device_id);
    out.push(DX7_SINGLE_VOICE);
    out.push(0x01); // format / byte count MSB
    out.push(0x1B); // byte count LSB (155)
    out.extend_from_slice(&voice);
    out.push(cksum);
    out.push(SYSEX_END);
    out
}
