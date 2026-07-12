// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// JSON-schema generation from a synth's `ParamSpec` table.
//
// This replaces the legacy 127-dim VAE latent vector with *named-parameter*
// generation. We build a JSON schema whose properties are the synth's named
// parameters, and hand it to the Anthropic Messages API via
// `output_config.format` (structured outputs). Claude then returns a JSON object
// with one entry per parameter.
//
// The Anthropic structured-outputs JSON-schema subset does NOT support numeric
// range constraints (`minimum`/`maximum`), so — per the API guidance — numeric
// bounds are documented in each property's `description` string and enforced
// client-side after parsing. Enum params use `enum`. The top-level object sets
// `additionalProperties: false` and lists every parameter in `required`.

use serde_json::{json, Map, Value};

use crate::mappers::Mapper;
use crate::param::{map_normalized, ParamSpec, ParamType, ParamValue};

/// Format a mapped value compactly for anchor tables (3 significant digits).
fn fmt_real(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let a = v.abs();
    if (v.fract()).abs() < 1e-9 && a < 1e7 {
        format!("{}", v as i64)
    } else if a >= 100.0 {
        format!("{:.0}", v)
    } else if a >= 1.0 {
        format!("{:.2}", v)
    } else {
        format!("{:.3}", v)
    }
}

/// Build a "0.0→x, 0.25→y, 0.5→z, 0.75→w, 1.0→v" anchor table by running the
/// spec's actual mapping at five normalized points. This pins down the meaning
/// of a normalized value regardless of the parameter's scaling curve, so the
/// model can interpolate real units (Hz, seconds, semitones) reliably.
fn anchor_table(spec: &ParamSpec) -> String {
    let pts = [0.0, 0.25, 0.5, 0.75, 1.0];
    let cells: Vec<String> = pts
        .iter()
        .map(|&v| {
            let mapped = match map_normalized(v, spec) {
                ParamValue::Num(n) => fmt_real(n),
                ParamValue::Bool(b) => b.to_string(),
                ParamValue::Enum(s) => s,
            };
            format!("{v}→{mapped}")
        })
        .collect();
    cells.join(", ")
}

/// Build the JSON schema object for a synth's parameters.
///
/// - Numeric params are `{"type": "number", "description": "... normalized 0..1
///   ..."}` (or the real range in the description for clarity).
/// - Enum params are `{"type": "string", "enum": [...]}`.
/// - Boolean params are `{"type": "boolean"}`.
/// - `additionalProperties` is `false`; `required` lists every parameter.
pub fn build_schema(mapper: &dyn Mapper) -> Value {
    let mut props = Map::new();
    let mut required: Vec<Value> = Vec::new();

    for spec in mapper.specs() {
        required.push(Value::String(spec.name.to_string()));

        // An optional per-param note (e.g. what the value maps to in the target
        // synth) is appended to the generated description.
        let note = spec.note.map(|n| format!(" {n}")).unwrap_or_default();

        let prop = match spec.ty {
            ParamType::Enum => {
                let opts: Vec<Value> = spec
                    .options
                    .iter()
                    .map(|o| Value::String((*o).to_string()))
                    .collect();
                json!({
                    "type": "string",
                    "enum": opts,
                    "description": format!(
                        "One of the listed options for `{}`.{}", spec.name, note
                    ),
                })
            }
            ParamType::Boolean => json!({
                "type": "boolean",
                "description": format!("Boolean toggle for `{}`.{}", spec.name, note),
            }),
            ParamType::Discrete => {
                // Worked example: the normalized value that selects the integer
                // one step above the minimum — the most common LLM mistake is
                // sending the integer itself, so spell the formula out.
                let (lo, hi) = (spec.min_val, spec.max_val);
                let example_int = (lo + 1.0).min(hi) as i64;
                let example_norm = if hi > lo { (example_int as f64 - lo) / (hi - lo) } else { 0.0 };
                json!({
                    "type": "number",
                    "description": format!(
                        "NORMALIZED 0.0..1.0 for `{}`, mapped to an INTEGER in {}..{} via \
                         round({} + v*{}). Do NOT send the integer itself — send the fraction: \
                         to select {}, send {:.4}. Anchors: {}.{}",
                        spec.name, lo as i64, hi as i64,
                        fmt_real(lo), fmt_real(hi - lo),
                        example_int, example_norm,
                        anchor_table(spec), note
                    ),
                })
            }
            _ => json!({
                "type": "number",
                "description": format!(
                    "NORMALIZED 0.0..1.0 for `{}` (real range {} .. {}). Do NOT send \
                     real-unit values — send the normalized fraction. Anchors \
                     (normalized→real): {}.{}",
                    spec.name, fmt_real(spec.min_val), fmt_real(spec.max_val),
                    anchor_table(spec), note
                ),
            }),
        };
        props.insert(spec.name.to_string(), prop);
    }

    json!({
        "type": "object",
        "additionalProperties": false,
        "required": Value::Array(required),
        "properties": Value::Object(props),
    })
}

/// Convert an Anthropic structured-outputs schema (from [`build_schema`]) into
/// Google Gemini's `responseSchema` shape (an OpenAPI-3.0 subset).
///
/// Gemini's `responseSchema` accepts `type` / `properties` / `required` /
/// `enum` / `description` but rejects `additionalProperties`. This walks the
/// schema recursively and strips every `additionalProperties` key, leaving
/// everything else (types, properties, required, enums, descriptions) intact.
/// Our schemas are shallow (a flat object of scalar params), but the walk is
/// fully recursive so nested objects/arrays would be handled too.
pub fn to_gemini_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if k == "additionalProperties" {
                    continue;
                }
                out.insert(k.clone(), to_gemini_schema(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(to_gemini_schema).collect()),
        other => other.clone(),
    }
}
