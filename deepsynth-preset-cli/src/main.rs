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

    /// LLM provider: claude | gemini | openai | openrouter | ollama | lmstudio.
    /// The last two are LOCAL servers (no API key needed); point them at a
    /// non-default host with the matching *_BASE_URL env var.
    #[arg(long, default_value = "claude")]
    provider: String,

    /// Override the provider's default model. Required in practice for local
    /// backends (e.g. --provider ollama --model llama3.1, or an LM Studio model id).
    #[arg(long)]
    model: Option<String>,

    /// API key (highest priority; else the provider's env var, else OS keychain).
    #[arg(long)]
    api_key: Option<String>,

    /// Also write the mapped parameter values to this JSON file (for inspection).
    #[arg(long)]
    dump_params: Option<PathBuf>,

    /// Suppress the ASCII banner and result panel (decoration) on stderr.
    #[arg(long)]
    quiet: bool,
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

    // --schema: pure JSON on stdout, no decoration, and exit.
    if cli.schema {
        let mapper = synth.mapper();
        let schema = build_schema(mapper.as_ref());
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    // Decoration (banner + result panel) is stderr-only and only when stderr is
    // an interactive terminal, NO_COLOR is unset, and --quiet was not passed.
    let decorate = decorate_stderr(cli.quiet);
    if decorate {
        print_banner();
    }

    let meta = PresetMeta {
        name: cli.name.clone(),
        ..Default::default()
    };

    // Provider · model for the result panel (only for the online path).
    let mut provider_model: Option<String> = None;

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
        // Online: prompt -> provider -> writer.
        let provider = Provider::from_id(&cli.provider).ok_or_else(|| {
            anyhow!(
                "unknown provider '{}' (expected: claude, gemini, openai, openrouter, \
                 ollama, lmstudio)",
                cli.provider
            )
        })?;
        let prompt = cli.prompt.as_deref().ok_or_else(|| {
            anyhow!("a prompt is required (or use --params-json / --defaults / --schema)")
        })?;
        // Local backends (Ollama, LM Studio) need no key; hosted ones do.
        let resolved = provider.resolve_key(cli.api_key.as_deref());
        let key = match resolved {
            Some((k, source)) => {
                eprintln!("Using API key from {}.", describe_source(provider, source));
                k
            }
            None if !provider.requires_key() => String::new(),
            None => bail!(
                "no API key: pass --api-key, set {}, or store one in the OS keychain",
                provider.env_var()
            ),
        };
        let model = cli.model.as_deref().unwrap_or(provider.default_model());
        provider_model = Some(format!("{} · {}", provider.id(), model));
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
    // Machine-readable result line on stdout (scripts can capture this); the
    // pretty box panel is decoration on stderr.
    if decorate {
        print_result_panel(&out.display().to_string(), bytes.len(), mapped.len(), provider_model);
    }
    println!("Wrote {} ({} bytes).", out.display(), bytes.len());

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

/// Whether stderr should carry the ASCII banner and result panel: only when it
/// is an interactive terminal, NO_COLOR is unset, and --quiet was not passed.
fn decorate_stderr(quiet: bool) -> bool {
    use std::io::IsTerminal;
    !quiet && std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

/// Linear-interpolate the brand gradient from pink (224,80,122) at t=0 to
/// teal (86,216,201) at t=1.
fn brand_gradient(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| (a + (b - a) * t).round() as u8;
    (lerp(224.0, 86.0), lerp(80.0, 216.0), lerp(122.0, 201.0))
}

/// Print the hand-designed "DEEPSYNTH" wordmark on stderr, colored with a
/// left-to-right pink→teal truecolor gradient, followed by the tagline.
fn print_banner() {
    // ANSI Shadow style block letters: D E E P S Y N T H.
    const ROWS: [&str; 6] = [
        "██████╗ ███████╗███████╗██████╗ ███████╗██╗   ██╗███╗   ██╗████████╗██╗  ██╗",
        "██╔══██╗██╔════╝██╔════╝██╔══██╗██╔════╝╚██╗ ██╔╝████╗  ██║╚══██╔══╝██║  ██║",
        "██║  ██║█████╗  █████╗  ██████╔╝███████╗ ╚████╔╝ ██╔██╗ ██║   ██║   ███████║",
        "██║  ██║██╔══╝  ██╔══╝  ██╔═══╝ ╚════██║  ╚██╔╝  ██║╚██╗██║   ██║   ██╔══██║",
        "██████╔╝███████╗███████╗██║     ███████║   ██║   ██║ ╚████║   ██║   ██║  ██║",
        "╚═════╝ ╚══════╝╚══════╝╚═╝     ╚══════╝   ╚═╝   ╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═╝",
    ];
    let width = ROWS.iter().map(|r| r.chars().count()).max().unwrap_or(1).max(2);
    let mut out = String::new();
    for row in ROWS {
        for (i, ch) in row.chars().enumerate() {
            let (r, g, b) = brand_gradient(i as f32 / (width - 1) as f32);
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{ch}"));
        }
        out.push_str("\x1b[0m\n");
    }
    let (r, g, b) = brand_gradient(0.5);
    out.push_str(&format!(
        "\x1b[38;2;{r};{g};{b}m  text \u{2192} preset  \u{00b7}  Surge XT / Dexed / Vital\x1b[0m\n\n"
    ));
    eprint!("{out}");
}

/// Print a slim box-drawing result panel on stderr summarizing the write.
fn print_result_panel(file: &str, bytes: usize, params: usize, provider_model: Option<String>) {
    let mut rows = vec![
        format!("file      {file}"),
        format!("size      {bytes} bytes"),
        format!("params    {params}"),
    ];
    if let Some(pm) = provider_model {
        rows.push(format!("provider  {pm}"));
    }
    let title = "─ deepsynth ";
    let content_w = rows
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count());
    let (r, g, b) = brand_gradient(1.0); // teal borders
    let c = |s: String| format!("\x1b[38;2;{r};{g};{b}m{s}\x1b[0m");

    let mut out = String::new();
    let top_fill = "─".repeat(content_w + 2 - title.chars().count());
    out.push_str(&c(format!("┌{title}{top_fill}┐")));
    out.push('\n');
    for row in &rows {
        let pad = " ".repeat(content_w - row.chars().count());
        out.push_str(&format!("{} {row}{pad} {}\n", c("│".into()), c("│".into())));
    }
    out.push_str(&c(format!("└{}┘", "─".repeat(content_w + 2))));
    out.push('\n');
    eprint!("{out}");
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
