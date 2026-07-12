// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Synth mappers. Each mapper owns an ordered `ParamSpec` table (ported from the
// legacy Python `vst_mappers/*.py`) and knows how to turn a set of normalized
// 0..1 (or enum-string) inputs into real, validated parameter values.
//
// A `Mapper` is text-prompt-independent: it maps values, it does not generate
// them. The Claude client produces the normalized values; the writers consume
// the mapped ones.

use std::collections::BTreeMap;

use crate::param::{map_normalized, ParamSpec, ParamType, ParamValue};

pub mod dexed;
pub mod surge;
pub mod vital;

/// A synthesizer parameter mapper.
pub trait Mapper {
    /// Stable synth identifier (`"surge"`, `"dexed"`, `"vital"`).
    fn id(&self) -> &'static str;

    /// The ordered parameter specifications.
    fn specs(&self) -> &'static [ParamSpec];

    /// Look up a spec by name.
    fn spec(&self, name: &str) -> Option<&'static ParamSpec> {
        self.specs().iter().find(|s| s.name == name)
    }

    /// Every parameter's default (mapped) value, keyed by name.
    fn defaults(&self) -> BTreeMap<String, ParamValue> {
        self.specs()
            .iter()
            .map(|s| (s.name.to_string(), s.default_value()))
            .collect()
    }

    /// Map a set of *normalized* inputs (from Claude) into real parameter values.
    ///
    /// - Numeric inputs are clamped to `0..1` and run through the spec's scaling.
    /// - Enum params accept either an enum-string (looked up in `options`) or a
    ///   normalized number.
    /// - Unknown input names are ignored.
    /// - Any spec not present in `inputs` falls back to its default.
    fn map_inputs(&self, inputs: &BTreeMap<String, NormInput>) -> BTreeMap<String, ParamValue> {
        let mut out = BTreeMap::new();
        for spec in self.specs() {
            let value = match inputs.get(spec.name) {
                Some(input) => map_one(spec, input),
                None => spec.default_value(),
            };
            out.insert(spec.name.to_string(), value);
        }
        out
    }
}

/// A normalized input value coming from Claude: either a number (`0..1`, or an
/// enum index-as-fraction) or an explicit enum/string choice.
#[derive(Debug, Clone, PartialEq)]
pub enum NormInput {
    Num(f64),
    Str(String),
}

/// Map one input against its spec, honoring enum-by-name and clamped numerics.
fn map_one(spec: &ParamSpec, input: &NormInput) -> ParamValue {
    match spec.ty {
        ParamType::Enum => match input {
            NormInput::Str(s) => {
                // Accept an exact option name; else fall back to the first option.
                if spec.options.contains(&s.as_str()) {
                    ParamValue::Enum(s.clone())
                } else {
                    ParamValue::Enum(spec.options.first().copied().unwrap_or("").to_string())
                }
            }
            NormInput::Num(n) => map_normalized(*n, spec),
        },
        ParamType::Boolean => match input {
            NormInput::Num(n) => ParamValue::Bool(*n > 0.5),
            NormInput::Str(s) => {
                let t = s.trim().to_ascii_lowercase();
                ParamValue::Bool(matches!(t.as_str(), "true" | "1" | "on" | "yes"))
            }
        },
        _ => match input {
            NormInput::Num(n) => map_normalized(*n, spec),
            // A string for a numeric param: try to parse, else default.
            NormInput::Str(s) => match s.trim().parse::<f64>() {
                Ok(n) => map_normalized(n, spec),
                Err(_) => spec.default_value(),
            },
        },
    }
}

/// Return the mapper for a synth id, or `None`.
pub fn mapper_for(id: &str) -> Option<Box<dyn Mapper>> {
    match id {
        "surge" => Some(Box::new(surge::SurgeMapper)),
        "dexed" => Some(Box::new(dexed::DexedMapper)),
        "vital" => Some(Box::new(vital::VitalMapper)),
        _ => None,
    }
}

/// All supported synth ids.
pub const SUPPORTED: &[&str] = &["surge", "dexed", "vital"];
