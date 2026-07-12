// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// preset-wasm — a thin wasm-bindgen wrapper over the *prompt-independent* half
// of `preset-core` (parameter mapping + preset writers), for the browser-only
// static preset generator hosted on GitHub Pages.
//
// It deliberately depends on `preset-core` with `default-features = false`, so
// none of the online machinery (Claude/Gemini HTTP clients, OS keychain) is
// linked — those are not wasm-compatible and are not needed in the browser. The
// browser page does the "prompt -> normalized params" step itself (a small
// in-browser embedding model + a hand-authored anchor bank); this crate only
// runs the exact same `Mapper::map_inputs` + writer path the native app uses,
// so a preset generated in the browser is byte-identical to one the CLI would
// produce from the same normalized inputs.
//
// Four exports, matching the native offline pipeline:
//   * `synths()               -> JSON array of {id, name, extension}`
//   * `schema_json(synth)      -> the synth's generated JSON schema (string)`
//   * `build_preset(synth, normalized_params_json, preset_name) -> Vec<u8>`
//   * `preset_file_extension(synth) -> "fxp" | "syx" | "vital"`

use std::collections::BTreeMap;

use preset_core::mappers::NormInput;
use preset_core::schema::build_schema;
use preset_core::{PresetMeta, Synth};
use serde_json::Value;
use wasm_bindgen::prelude::*;

/// Parse a synth id, returning a JS-facing error for an unknown id.
fn synth_from_id(id: &str) -> Result<Synth, JsValue> {
    Synth::from_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("unknown synth id: {id} (expected surge, dexed, vital)")))
}

/// The list of supported synths, as a JSON array of `{id, name, extension}`.
///
/// The browser uses this to populate the synth selector.
#[wasm_bindgen]
pub fn synths() -> String {
    let list: Vec<Value> = [Synth::Surge, Synth::Dexed, Synth::Vital]
        .into_iter()
        .map(|s| {
            let name = match s {
                Synth::Surge => "Surge XT",
                Synth::Dexed => "Dexed / DX7",
                Synth::Vital => "Vital",
            };
            serde_json::json!({
                "id": s.id(),
                "name": name,
                "extension": s.extension(),
            })
        })
        .collect();
    // Serializing a Vec<Value> cannot fail.
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
}

/// The generated JSON schema (as a pretty string) for a synth's parameters.
///
/// Identical to `deepsynth-preset --schema --synth <synth>`. The browser does
/// not strictly need this to build a preset, but it lets the page introspect
/// which parameters exist / their ranges.
#[wasm_bindgen]
pub fn schema_json(synth: &str) -> Result<String, JsValue> {
    let synth = synth_from_id(synth)?;
    let mapper = synth.mapper();
    let schema = build_schema(mapper.as_ref());
    serde_json::to_string_pretty(&schema).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// The canonical output file extension for a synth (no dot).
#[wasm_bindgen]
pub fn preset_file_extension(synth: &str) -> Result<String, JsValue> {
    let synth = synth_from_id(synth)?;
    Ok(synth.extension().to_string())
}

/// Build a real preset file from *normalized* inputs — the browser equivalent of
/// the CLI's `--params-json` offline path.
///
/// `normalized_params_json` is a flat JSON object of `{ "param_name": value }`
/// where a value is either a number in `0..1` (clamped + scaled by the mapper)
/// or a string (an enum option name, or a numeric string). This is passed
/// straight through `Mapper::map_inputs` — the *exact same* mapping the native
/// app runs — and then the synth's writer, so the returned bytes are a fully
/// loadable `.fxp` / `.syx` / `.vital`. Missing parameters fall back to the
/// synth's defaults, and unknown parameter names are ignored (identical native
/// semantics).
#[wasm_bindgen]
pub fn build_preset(
    synth: &str,
    normalized_params_json: &str,
    preset_name: &str,
) -> Result<Vec<u8>, JsValue> {
    let synth = synth_from_id(synth)?;
    let inputs = parse_normalized(normalized_params_json)?;
    let mapped = preset_core::map_inputs(synth, &inputs);
    let meta = PresetMeta {
        name: preset_name.to_string(),
        ..Default::default()
    };
    preset_core::write_preset(synth, &meta, &mapped).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Parse a flat JSON object of normalized inputs into a `NormInput` map, using
/// exactly the same number/string/bool coercions as the CLI's params-JSON path.
fn parse_normalized(json: &str) -> Result<BTreeMap<String, NormInput>, JsValue> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&format!("invalid params JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| JsValue::from_str("params JSON must be an object"))?;

    let mut out = BTreeMap::new();
    for (k, v) in obj {
        let input = match v {
            Value::Number(n) => NormInput::Num(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => NormInput::Str(s.clone()),
            Value::Bool(b) => NormInput::Num(if *b { 1.0 } else { 0.0 }),
            _ => {
                return Err(JsValue::from_str(&format!(
                    "params JSON value for '{k}' must be a number, string, or bool"
                )))
            }
        };
        out.insert(k.clone(), input);
    }
    Ok(out)
}
