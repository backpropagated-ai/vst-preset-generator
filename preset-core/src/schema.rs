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
use crate::param::ParamType;

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
            ParamType::Discrete => json!({
                "type": "number",
                "description": format!(
                    "Normalized 0.0..1.0 for `{}` (maps to integer range {}..{}). \
                     Clamped client-side; ranges are advisory (the API rejects numeric bounds).{}",
                    spec.name, spec.min_val as i64, spec.max_val as i64, note
                ),
            }),
            _ => json!({
                "type": "number",
                "description": format!(
                    "Normalized 0.0..1.0 for `{}` (real range {} .. {}). \
                     Clamped client-side; ranges are advisory (the API rejects numeric bounds).{}",
                    spec.name, spec.min_val, spec.max_val, note
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
