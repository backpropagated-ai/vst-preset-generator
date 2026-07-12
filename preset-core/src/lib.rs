// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// preset-core — AI text-to-preset generation for VST synths.
//
// This library has two independent halves:
//
//   1. **Parameter mapping + preset writers** (prompt-independent): for each
//      supported synth, a `ParamSpec` table (`mappers/`) maps normalized 0..1 /
//      enum values onto real parameter values, and a writer (`writers/`) turns
//      those into a real preset file: Surge XT `.fxp`, Dexed/DX7 `.syx`, Vital
//      `.vital`.
//
//   2. **The Claude client** (`claude/`): a raw-HTTPS Anthropic client that
//      turns a text prompt into named-parameter values via structured outputs
//      whose JSON schema is generated from the synth's `ParamSpec` table.
//
// The two are glued by the top-level `generate_preset` (prompt → bytes, needs an
// API key) and `write_preset` (already-mapped params → bytes, fully offline).

// The online halves (Claude/Gemini HTTP clients, OS-keychain key resolution, and
// the provider abstraction that ties them together) live behind the default-on
// `llm` feature. `--no-default-features` compiles a pure, wasm-compatible
// mapping + writers core with no reqwest / keyring dependency.
#[cfg(feature = "llm")]
pub mod claude;
#[cfg(feature = "llm")]
pub mod gemini;
pub mod mappers;
pub mod param;
#[cfg(feature = "llm")]
pub mod provider;
pub mod schema;
pub mod writers;

use std::collections::BTreeMap;

use crate::mappers::{mapper_for, Mapper, NormInput};
use crate::param::ParamValue;
#[cfg(feature = "llm")]
pub use crate::provider::Provider;

/// A supported synth target and its canonical file extension / metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synth {
    Surge,
    Dexed,
    Vital,
}

impl Synth {
    /// Parse a synth id string (`"surge"` / `"dexed"` / `"vital"`).
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "surge" => Some(Synth::Surge),
            "dexed" | "dx7" => Some(Synth::Dexed),
            "vital" => Some(Synth::Vital),
            _ => None,
        }
    }

    /// The canonical id string.
    pub fn id(self) -> &'static str {
        match self {
            Synth::Surge => "surge",
            Synth::Dexed => "dexed",
            Synth::Vital => "vital",
        }
    }

    /// The default output file extension (no dot).
    pub fn extension(self) -> &'static str {
        match self {
            Synth::Surge => "fxp",
            Synth::Dexed => "syx",
            Synth::Vital => "vital",
        }
    }

    /// A boxed mapper for this synth.
    pub fn mapper(self) -> Box<dyn Mapper> {
        mapper_for(self.id()).expect("mapper exists for every Synth variant")
    }
}

/// Metadata common to all writers.
#[derive(Debug, Clone)]
pub struct PresetMeta {
    pub name: String,
    pub author: String,
    pub comment: String,
}

impl Default for PresetMeta {
    fn default() -> Self {
        Self {
            name: "DeepSynth Patch".into(),
            author: "DeepSynth".into(),
            comment: String::new(),
        }
    }
}

/// Errors from the high-level API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown synth id: {0} (expected one of: surge, dexed, vital)")]
    UnknownSynth(String),
    #[cfg(feature = "llm")]
    #[error(transparent)]
    Claude(#[from] claude::ClaudeError),
    #[cfg(feature = "llm")]
    #[error(transparent)]
    Gemini(#[from] gemini::GeminiError),
    #[error("could not serialize preset: {0}")]
    Serialize(String),
}

/// The result of an online generation: the preset file bytes plus the mapped
/// parameter values (so a UI can display them). Shared across providers.
#[derive(Debug, Clone)]
pub struct GeneratedPreset {
    pub bytes: Vec<u8>,
    pub mapped: BTreeMap<String, ParamValue>,
}

/// Write a preset file from *already-mapped* parameter values (fully offline).
///
/// `mapped` is a map of `param_name -> ParamValue` as produced by
/// `Mapper::map_inputs` (or `Mapper::defaults`). Unmapped parameters fall back to
/// the synth's defaults inside the writer.
pub fn write_preset(
    synth: Synth,
    meta: &PresetMeta,
    mapped: &BTreeMap<String, ParamValue>,
) -> Result<Vec<u8>, Error> {
    Ok(match synth {
        Synth::Surge => {
            let sm = writers::surge::SurgeMeta {
                name: meta.name.clone(),
                category: "DeepSynth".into(),
                comment: meta.comment.clone(),
                author: meta.author.clone(),
                license: "CC0".into(),
            };
            writers::surge::build_fxp(&sm, mapped)
        }
        Synth::Dexed => writers::syx::build_syx(mapped, &meta.name, 0),
        Synth::Vital => {
            let vm = writers::vital::VitalMeta {
                name: meta.name.clone(),
                author: meta.author.clone(),
                comments: meta.comment.clone(),
            };
            writers::vital::build_vital(&vm, mapped)
        }
    })
}

/// Map normalized inputs (e.g. from a params-JSON file) into `ParamValue`s using
/// the synth's mapper.
pub fn map_inputs(
    synth: Synth,
    inputs: &BTreeMap<String, NormInput>,
) -> BTreeMap<String, ParamValue> {
    synth.mapper().map_inputs(inputs)
}

/// The full online pipeline: prompt → Claude → mapped params → preset bytes.
///
/// Requires an API key. The mapped parameter values are returned alongside the
/// bytes so a UI can display them.
#[cfg(feature = "llm")]
pub fn generate_preset(
    synth: Synth,
    api_key: &str,
    prompt: &str,
    meta: &PresetMeta,
) -> Result<(Vec<u8>, BTreeMap<String, ParamValue>), Error> {
    let mapper = synth.mapper();
    let client = claude::ClaudeClient::new(api_key)?;
    let inputs = client.generate(mapper.as_ref(), prompt)?;
    let mapped = mapper.map_inputs(&inputs);
    let bytes = write_preset(synth, meta, &mapped)?;
    Ok((bytes, mapped))
}

/// The full online pipeline for a chosen provider: prompt → LLM → mapped params
/// → preset bytes. `model` overrides the provider's default model when `Some`.
///
/// Requires an API key. Returns a [`GeneratedPreset`] (bytes + mapped params).
#[cfg(feature = "llm")]
pub fn generate_preset_with(
    provider: Provider,
    synth: Synth,
    api_key: &str,
    model: Option<&str>,
    prompt: &str,
    meta: &PresetMeta,
) -> Result<GeneratedPreset, Error> {
    let mapper = synth.mapper();
    let inputs = match provider {
        Provider::Claude => {
            let mut client = claude::ClaudeClient::new(api_key)?;
            if let Some(m) = model {
                client = client.with_model(m);
            }
            client.generate(mapper.as_ref(), prompt)?
        }
        Provider::Gemini => {
            let mut client = gemini::GeminiClient::new(api_key)?;
            if let Some(m) = model {
                client = client.with_model(m);
            }
            client.generate(mapper.as_ref(), prompt)?
        }
    };
    let mapped = mapper.map_inputs(&inputs);
    let bytes = write_preset(synth, meta, &mapped)?;
    Ok(GeneratedPreset { bytes, mapped })
}
