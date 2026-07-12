// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Golden-parity tests against the legacy Python generators.
//
// For fixed parameter dicts we run the legacy Python `syx_generator.py` and
// `fxp_generator.py` (generic parameter path) via a `python3` subprocess to
// produce reference bytes, then assert the Rust output is byte-identical.
//
// These tests SKIP GRACEFULLY (pass with a note) if:
//   - `python3` is not available, or
//   - `DEEPSYNTH_PRESETS_PY_REF` is not set to the legacy generator directory.
//
// Point `DEEPSYNTH_PRESETS_PY_REF` at a directory containing the original
// `syx_generator.py` / `fxp_generator.py` this project was ported from to run
// the byte-parity assertions; without it the tests skip, which is the intended
// behavior for standalone checkouts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use preset_core::param::ParamValue;
use preset_core::writers::{fxp, syx};

/// Locate the legacy Python generator dir from `DEEPSYNTH_PRESETS_PY_REF`.
/// Returns `None` (skip) when unset or when the reference files are absent.
fn legacy_generators_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("DEEPSYNTH_PRESETS_PY_REF")?);
    if dir.join("syx_generator.py").is_file() {
        return Some(dir);
    }
    None
}

/// Is a `python3` interpreter available?
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn num(v: f64) -> ParamValue {
    ParamValue::Num(v)
}

// -------------------------------------------------------------------------
// DX7 SysEx byte-exact parity
// -------------------------------------------------------------------------

#[test]
fn dx7_syx_matches_python_bytes() {
    let Some(gen_dir) = legacy_generators_dir() else {
        eprintln!("SKIP: legacy Python generators not found (crate split out) — parity skipped.");
        return;
    };
    if !have_python3() {
        eprintln!("SKIP: python3 not available — DX7 parity skipped.");
        return;
    }

    // A fixed parameter dict exercised on both sides. The Python voice name comes
    // from `_preset_name`; the Rust side takes the preset name explicitly.
    let py_script = format!(
        r#"
import sys
sys.path.insert(0, {gen_dir:?})
from syx_generator import SYXGenerator
params = {{
    "_preset_name": "E.PIANO 1",
    "algorithm": 5,
    "feedback": 6,
    "op1_level": 99, "op1_coarse": 1, "op1_fine": 0, "op1_detune": 0,
    "op2_level": 95, "op2_coarse": 14, "op2_detune": 3,
    "op3_level": 92, "op3_mode": "ratio",
    "op4_level": 87, "op4_mode": "fixed",
    "op1_eg_r1": 80, "op1_eg_l1": 90,
    "lfo_speed": 35, "lfo_wave": "sine", "lfo_sync": True,
    "pitch_eg_r1": 70, "pitch_eg_l1": 50,
    "transpose": 12,
    "osc_sync": True,
    "op1_lev_scale_left_curve": "exp_pos", "op1_lev_scale_right_curve": "lin_pos",
}}
data = SYXGenerator().generate(params, "dexed")
sys.stdout.buffer.write(data)
"#,
        gen_dir = gen_dir.to_string_lossy(),
    );

    let py_bytes = run_python(&py_script);
    let Some(py_bytes) = py_bytes else {
        eprintln!("SKIP: python subprocess failed — DX7 parity skipped.");
        return;
    };

    // Build the same voice in Rust.
    let mut params: BTreeMap<String, ParamValue> = BTreeMap::new();
    params.insert("algorithm".into(), num(5.0));
    params.insert("feedback".into(), num(6.0));
    params.insert("op1_level".into(), num(99.0));
    params.insert("op1_coarse".into(), num(1.0));
    params.insert("op1_fine".into(), num(0.0));
    params.insert("op1_detune".into(), num(0.0));
    params.insert("op2_level".into(), num(95.0));
    params.insert("op2_coarse".into(), num(14.0));
    params.insert("op2_detune".into(), num(3.0));
    params.insert("op3_level".into(), num(92.0));
    params.insert("op3_mode".into(), ParamValue::Enum("ratio".into()));
    params.insert("op4_level".into(), num(87.0));
    params.insert("op4_mode".into(), ParamValue::Enum("fixed".into()));
    params.insert("op1_eg_r1".into(), num(80.0));
    params.insert("op1_eg_l1".into(), num(90.0));
    params.insert("lfo_speed".into(), num(35.0));
    params.insert("lfo_wave".into(), ParamValue::Enum("sine".into()));
    params.insert("lfo_sync".into(), ParamValue::Bool(true));
    params.insert("pitch_eg_r1".into(), num(70.0));
    params.insert("pitch_eg_l1".into(), num(50.0));
    params.insert("transpose".into(), num(12.0));
    params.insert("osc_sync".into(), ParamValue::Bool(true));
    params.insert(
        "op1_lev_scale_left_curve".into(),
        ParamValue::Enum("exp_pos".into()),
    );
    params.insert(
        "op1_lev_scale_right_curve".into(),
        ParamValue::Enum("lin_pos".into()),
    );

    let rust_bytes = syx::build_syx(&params, "E.PIANO 1", 0);

    assert_eq!(
        rust_bytes.len(),
        py_bytes.len(),
        "length mismatch: rust={} python={}",
        rust_bytes.len(),
        py_bytes.len()
    );
    assert_eq!(
        rust_bytes, py_bytes,
        "DX7 SysEx bytes differ from the Python reference"
    );
}

// -------------------------------------------------------------------------
// Generic FXP (FxCk) parameter-format header parity
// -------------------------------------------------------------------------

#[test]
fn generic_fxp_header_matches_python() {
    let Some(gen_dir) = legacy_generators_dir() else {
        eprintln!("SKIP: legacy Python generators not found — FXP header parity skipped.");
        return;
    };
    if !have_python3() {
        eprintln!("SKIP: python3 not available — FXP header parity skipped.");
        return;
    }

    // The Python generic path builds a FxCk file with a big-endian float per
    // param. We replicate it exactly: same param order (dict insertion order),
    // same plugin id resolution, same name.
    let py_script = format!(
        r#"
import sys
sys.path.insert(0, {gen_dir:?})
from fxp_generator import FXPGenerator
params = {{
    "_preset_name": "Generic Test",
    "p0": 0.0,
    "p1": 0.5,
    "p2": 1.0,
    "p3": 0.25,
}}
# Force the generic chunk path with an unknown vst_type.
data = FXPGenerator().generate(params, "unknown_synth")
sys.stdout.buffer.write(data)
"#,
        gen_dir = gen_dir.to_string_lossy(),
    );

    let Some(py_bytes) = run_python(&py_script) else {
        eprintln!("SKIP: python subprocess failed — FXP header parity skipped.");
        return;
    };

    // The Python generic path: default plugin id 'VST\0' (0x56535400), fxVersion
    // = preset_version = 1, name "Generic Test". Its generic chunk is
    // `[u32 count][count * be f32]`; the FXP header's numParams counts non-`_`
    // keys (4 here). Rebuild the exact bytes with the FXP writer + generic chunk.
    //
    // NOTE: the legacy `_create_generic_chunk` writes a leading count u32 then
    // the floats, so the FxCk "params" body here is that chunk. We reproduce the
    // whole file with the low-level header writer to match byte-for-byte.
    let plugin_id = 0x56535400u32.to_be_bytes(); // 'VST\0'
    let params = [0.0f32, 0.5, 1.0, 0.25];

    // Generic chunk = count + floats (this is the Python body).
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(params.len() as u32).to_be_bytes());
    for p in params {
        chunk.extend_from_slice(&p.clamp(0.0, 1.0).to_be_bytes());
    }

    // The Python FxCk header sets numParams = number of non-`_` keys = 4, and the
    // body is the generic chunk. Reproduce via the low-level header writer.
    let mut rust_bytes = Vec::new();
    fxp::write_header(
        &mut rust_bytes,
        fxp::FXP_PARAM_MAGIC,
        &plugin_id,
        1,
        4,
        "Generic Test",
        chunk.len(),
    );
    rust_bytes.extend_from_slice(&chunk);

    assert_eq!(
        rust_bytes.len(),
        py_bytes.len(),
        "FXP length mismatch: rust={} python={}",
        rust_bytes.len(),
        py_bytes.len()
    );
    assert_eq!(
        rust_bytes, py_bytes,
        "generic FXP bytes differ from the Python reference"
    );
}

/// Run a python3 script, returning its stdout bytes on success.
fn run_python(script: &str) -> Option<Vec<u8>> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "python stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    Some(output.stdout)
}
