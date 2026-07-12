// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Schema-generation tests: the JSON schema built from a synth's ParamSpec table
// must be a valid Anthropic structured-outputs schema — an object with
// `additionalProperties: false`, every parameter in `required`, enums for
// categorical params, and no numeric range constraints (ranges live in
// descriptions).

use preset_core::mappers::mapper_for;
use preset_core::schema::{build_schema, to_gemini_schema};

#[test]
fn schema_is_object_with_additional_properties_false() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let schema = build_schema(m.as_ref());
        assert_eq!(schema["type"], "object", "{id} schema type");
        assert_eq!(
            schema["additionalProperties"], false,
            "{id} must set additionalProperties=false"
        );
    }
}

#[test]
fn schema_required_lists_every_param() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let schema = build_schema(m.as_ref());
        let required = schema["required"].as_array().unwrap();
        let props = schema["properties"].as_object().unwrap();

        assert_eq!(required.len(), m.specs().len(), "{id} required count");
        assert_eq!(props.len(), m.specs().len(), "{id} properties count");

        for spec in m.specs() {
            assert!(
                props.contains_key(spec.name),
                "{id} missing property {}",
                spec.name
            );
            assert!(
                required.iter().any(|r| r == spec.name),
                "{id} {} not in required",
                spec.name
            );
        }
    }
}

#[test]
fn enum_params_carry_enum_list() {
    let m = mapper_for("surge").unwrap();
    let schema = build_schema(m.as_ref());
    let props = &schema["properties"];
    // filter1_type is an enum with 9 options.
    let ft = &props["filter1_type"];
    assert_eq!(ft["type"], "string");
    let opts = ft["enum"].as_array().unwrap();
    assert_eq!(opts.len(), 9);
    assert!(opts.iter().any(|o| o == "lp24"));
}

#[test]
fn numeric_params_have_no_range_constraints() {
    // The Anthropic API rejects numeric min/max in structured-output schemas,
    // so no numeric property may carry `minimum`/`maximum`.
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let schema = build_schema(m.as_ref());
        let props = schema["properties"].as_object().unwrap();
        for (name, prop) in props {
            assert!(
                prop.get("minimum").is_none() && prop.get("maximum").is_none(),
                "{id}.{name} must not carry numeric range constraints"
            );
        }
    }
}

#[test]
fn schema_serializes_to_valid_json() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let schema = build_schema(m.as_ref());
        let s = serde_json::to_string(&schema).unwrap();
        // Round-trips.
        let back: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back, schema);
    }
}

// --- Gemini schema conversion --------------------------------------------
//
// Gemini's `responseSchema` is an OpenAPI-3.0 subset that rejects
// `additionalProperties` but keeps type/properties/required/enum/description.

#[test]
fn gemini_schema_strips_additional_properties_everywhere() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let anthropic = build_schema(m.as_ref());
        let gemini = to_gemini_schema(&anthropic);

        // Top-level `additionalProperties` is gone.
        assert!(
            gemini.get("additionalProperties").is_none(),
            "{id}: top-level additionalProperties must be stripped"
        );
        // And it appears nowhere in the (recursively walked) schema.
        assert!(
            !contains_key(&gemini, "additionalProperties"),
            "{id}: additionalProperties must be stripped recursively"
        );
    }
}

#[test]
fn gemini_schema_preserves_type_properties_required() {
    for id in ["surge", "dexed", "vital"] {
        let m = mapper_for(id).unwrap();
        let anthropic = build_schema(m.as_ref());
        let gemini = to_gemini_schema(&anthropic);

        assert_eq!(gemini["type"], "object", "{id}: type preserved");

        // `required` is kept verbatim (same list, same order).
        assert_eq!(
            gemini["required"], anthropic["required"],
            "{id}: required list preserved"
        );

        // Every parameter property survives with its type/description.
        let props = gemini["properties"].as_object().unwrap();
        assert_eq!(props.len(), m.specs().len(), "{id}: all properties kept");
        for spec in m.specs() {
            let p = props
                .get(spec.name)
                .unwrap_or_else(|| panic!("{id}: property {} dropped", spec.name));
            assert!(p.get("type").is_some(), "{id}.{} keeps type", spec.name);
        }
    }
}

#[test]
fn gemini_schema_preserves_enum_options() {
    let m = mapper_for("surge").unwrap();
    let anthropic = build_schema(m.as_ref());
    let gemini = to_gemini_schema(&anthropic);

    let ft = &gemini["properties"]["filter1_type"];
    assert_eq!(ft["type"], "string");
    let opts = ft["enum"].as_array().unwrap();
    assert_eq!(opts.len(), 9, "enum options preserved");
    assert!(opts.iter().any(|o| o == "lp24"));
}

/// Recursively check whether any object anywhere in `v` contains `key`.
fn contains_key(v: &serde_json::Value, key: &str) -> bool {
    match v {
        serde_json::Value::Object(map) => {
            map.contains_key(key) || map.values().any(|x| contains_key(x, key))
        }
        serde_json::Value::Array(items) => items.iter().any(|x| contains_key(x, key)),
        _ => false,
    }
}
