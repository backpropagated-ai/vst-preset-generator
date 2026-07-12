// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// `deepsynth-preset` — CLI for text-to-preset generation.
//
// Usage:
//   deepsynth-preset "warm analog pad" --synth surge -o warm_pad.fxp
//     Generate a preset from a text prompt via Claude (needs an API key).
//
//   deepsynth-preset "warm pad" --synth surge --provider gemini -o pad.fxp
//     Same, via Google Gemini (needs a Gemini API key).
//
//   deepsynth-preset --params-json params.json --synth surge -o out.fxp
//     OFFLINE mode: skip the API. `params.json` is a flat object of normalized
//     inputs (`{"param": 0.7, "filter1_type": "lp24", ...}`); the mapper +
//     writer turn it into a preset file. Missing params use synth defaults.
//
//   deepsynth-preset --schema --synth surge
//     Print the generated JSON schema for a synth's parameters.
//
//   deepsynth-preset --defaults --synth vital -o default.vital
//     Write a default preset (no prompt, no API).
//
// Provider + API key resolution. `--provider claude|gemini` (default claude)
// picks the LLM backend. For each provider the key resolves highest-priority
// first: --api-key > its env var (ANTHROPIC_API_KEY / GEMINI_API_KEY, also
// GOOGLE_API_KEY) > OS keychain (service `deepsynth-preset`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use preset_core::claude::key::KeySource;
use preset_core::mappers::NormInput;
use preset_core::schema::build_schema;
use preset_core::{PresetMeta, Provider, Synth};

#[derive(Parser, Debug)]
#[command(
    name = "deepsynth-preset",
    about = "AI text-to-preset generation for Surge XT / Dexed / Vital (bring your own Anthropic or Gemini API key).",
    version
)]
struct Cli {
    /// The text prompt describing the sound (omit for --params-json/--defaults/--schema).
    prompt: Option<String>,

    /// Target synth: surge | dexed | vital.
    #[arg(long, short = 's', default_value = "surge")]
    synth: String,

    /// Output file path (defaults to `<name>.<ext>` in the current directory).
    #[arg(long, short = 'o')]
    out: Option<PathBuf>,

    /// Preset name (also the FXP/voice name).
    #[arg(long, default_value = "DeepSynth Patch")]
    name: String,

    /// Offline mode: read normalized inputs from this JSON file instead of calling the API.
    #[arg(long)]
    params_json: Option<PathBuf>,

    /// Write a default preset (no prompt, no API call).
    #[arg(long)]
    defaults: bool,

    /// Print the generated JSON schema for the synth's parameters and exit.
    #[arg(long)]
    schema: bool,

    /// LLM provider: claude | gemini.
    #[arg(long, default_value = "claude")]
    provider: String,

    /// Override the provider's default model (claude-opus-4-8 / gemini-3.5-flash).
    #[arg(long)]
    model: Option<String>,

    /// API key (highest priority; else the provider's env var, else OS keychain).
    /// Prefer the env var/keychain: a key on the command line lands in shell
    /// history and is visible in the process list.
    #[arg(long)]
    api_key: Option<String>,

    /// Also write the mapped parameter values to this JSON file (for inspection).
    #[arg(long)]
    dump_params: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let synth = Synth::from_id(&cli.synth).ok_or_else(|| {
        anyhow!(
            "unknown synth '{}' (expected: surge, dexed, vital)",
            cli.synth
        )
    })?;

    // --schema: print and exit.
    if cli.schema {
        let mapper = synth.mapper();
        let schema = build_schema(mapper.as_ref());
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    let meta = PresetMeta {
        name: cli.name.clone(),
        ..Default::default()
    };

    // Determine the mapped params by the chosen path.
    let (bytes, mapped) = if cli.defaults {
        let mapper = synth.mapper();
        let mapped = mapper.defaults();
        let bytes = preset_core::write_preset(synth, &meta, &mapped)?;
        (bytes, mapped)
    } else if let Some(path) = &cli.params_json {
        // Offline: normalized inputs from JSON -> mapper -> writer.
        let inputs = read_params_json(path)?;
        let mapped = preset_core::map_inputs(synth, &inputs);
        let bytes = preset_core::write_preset(synth, &meta, &mapped)?;
        (bytes, mapped)
    } else {
        // Online: prompt -> provider (Claude/Gemini) -> writer.
        let provider = Provider::from_id(&cli.provider).ok_or_else(|| {
            anyhow!(
                "unknown provider '{}' (expected: claude, gemini)",
                cli.provider
            )
        })?;
        let prompt = cli.prompt.as_deref().ok_or_else(|| {
            anyhow!("a prompt is required (or use --params-json / --defaults / --schema)")
        })?;
        let (key, source) = provider
            .resolve_key(cli.api_key.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "no API key: pass --api-key, set {}, or store one in the OS keychain",
                    provider.env_var()
                )
            })?;
        let model = cli.model.as_deref().unwrap_or(provider.default_model());
        eprintln!("Using API key from {}.", describe_source(provider, source));
        eprintln!(
            "Generating {} preset via {} ({}) for: {:?}",
            synth.id(),
            provider.id(),
            model,
            prompt
        );
        let gen = preset_core::generate_preset_with(
            provider,
            synth,
            &key,
            cli.model.as_deref(),
            prompt,
            &meta,
        )
        .with_context(|| format!("preset generation via {} failed", provider.label()))?;
        (gen.bytes, gen.mapped)
    };

    // Resolve output path.
    let out = cli
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{}.{}", sanitize(&cli.name), synth.extension())));
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    eprintln!("Wrote {} ({} bytes).", out.display(), bytes.len());

    if let Some(dump) = &cli.dump_params {
        let json = mapped_to_json(&mapped);
        std::fs::write(dump, serde_json::to_string_pretty(&json)?)
            .with_context(|| format!("writing {}", dump.display()))?;
        eprintln!("Wrote mapped params to {}.", dump.display());
    }

    Ok(())
}

/// Read a flat JSON object of normalized inputs into a `NormInput` map.
fn read_params_json(path: &PathBuf) -> Result<BTreeMap<String, NormInput>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading params JSON {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing params JSON {}", path.display()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("params JSON must be an object"))?;

    let mut out = BTreeMap::new();
    for (k, v) in obj {
        let input = match v {
            serde_json::Value::Number(n) => NormInput::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => NormInput::Str(s.clone()),
            serde_json::Value::Bool(b) => NormInput::Num(if *b { 1.0 } else { 0.0 }),
            _ => bail!("params JSON value for '{k}' must be a number, string, or bool"),
        };
        out.insert(k.clone(), input);
    }
    Ok(out)
}

/// Serialize mapped `ParamValue`s to a JSON object.
fn mapped_to_json(mapped: &BTreeMap<String, preset_core::param::ParamValue>) -> serde_json::Value {
    use preset_core::param::ParamValue;
    let mut map = serde_json::Map::new();
    for (k, v) in mapped {
        let jv = match v {
            ParamValue::Num(n) => serde_json::json!(n),
            ParamValue::Enum(s) => serde_json::json!(s),
            ParamValue::Bool(b) => serde_json::json!(b),
        };
        map.insert(k.clone(), jv);
    }
    serde_json::Value::Object(map)
}

fn describe_source(provider: Provider, source: KeySource) -> String {
    match source {
        KeySource::Explicit => "the --api-key flag".to_string(),
        KeySource::Env => format!("the {} environment variable", provider.env_var()),
        KeySource::Keychain => "the OS keychain".to_string(),
    }
}

/// Sanitize a preset name into a safe filename stem.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "preset".to_string()
    } else {
        trimmed.to_string()
    }
}
