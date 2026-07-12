// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Anthropic (Claude) client — raw HTTPS via `reqwest` (rustls-tls).
//
// Rust has no official Anthropic SDK, so we call `POST /v1/messages` directly.
// This module builds a structured-outputs request whose JSON schema is generated
// from the synth's `ParamSpec` table (see `schema.rs`), sends it, and parses the
// returned JSON object of named parameters.
//
// Requirements enforced here (per the current Anthropic API for `claude-opus-4-8`):
//   * Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type`.
//   * Model: `claude-opus-4-8` (a config constant).
//   * DO NOT send `temperature` / `top_p` / `top_k` (rejected on this model).
//   * Send `thinking: {"type": "adaptive"}`.
//   * `max_tokens: 8000`.
//   * `output_config: {"format": {"type": "json_schema", "schema": <schema>}}`.
//   * Handle `stop_reason`: `refusal` (clear error), `max_tokens` (retry ask).
//   * On 429, respect `retry-after` with one retry.
//   * Never log the key.

pub mod key;

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{json, Value};

use crate::mappers::{Mapper, NormInput};
use crate::schema::build_schema;

/// Anthropic API endpoint.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// Anthropic API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Default model — a config constant, not sent by the caller.
pub const DEFAULT_MODEL: &str = "claude-opus-4-8";
/// Max output tokens for a preset-generation call.
pub const MAX_TOKENS: u32 = 8000;

/// Errors from a Claude preset-generation request.
#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("network error contacting the Anthropic API: {0}")]
    Network(String),
    #[error("Anthropic API error ({type_}): {message}")]
    Api { type_: String, message: String },
    #[error("Claude refused the request (reason: {0}). Try rephrasing the prompt.")]
    Refusal(String),
    #[error("Claude hit the output token limit before finishing. Please retry.")]
    MaxTokens,
    #[error("could not parse Claude's JSON response: {0}")]
    Parse(String),
    #[error("rate limited by the Anthropic API and the retry also failed")]
    RateLimited,
    #[error("no API key: pass one, set ANTHROPIC_API_KEY, or store one in the OS keychain")]
    NoKey,
}

/// A configured Claude client.
pub struct ClaudeClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
}

impl ClaudeClient {
    /// Construct a client with an explicit API key and the default model.
    pub fn new(api_key: impl Into<String>) -> Result<Self, ClaudeError> {
        let key = api_key.into();
        if key.trim().is_empty() {
            return Err(ClaudeError::NoKey);
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| ClaudeError::Network(e.to_string()))?;
        Ok(Self {
            api_key: key,
            model: DEFAULT_MODEL.to_string(),
            http,
        })
    }

    /// Override the model (defaults to `claude-opus-4-8`).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Generate normalized parameter inputs for a synth from a text prompt.
    ///
    /// Returns a map of `param_name -> NormInput` (numbers for numeric params,
    /// enum strings for categorical ones), which the caller feeds into
    /// `Mapper::map_inputs`. Any numeric ranges are advisory in the schema; the
    /// mapper clamps them, so out-of-range numbers are handled downstream.
    pub fn generate(
        &self,
        mapper: &dyn Mapper,
        prompt: &str,
    ) -> Result<BTreeMap<String, NormInput>, ClaudeError> {
        let schema = build_schema(mapper);
        let system = system_prompt(mapper.id());
        let body = self.build_body(&schema, &system, prompt);

        let value = self.send_with_retry(&body)?;
        parse_response(mapper, &value)
    }

    /// Build the JSON request body.
    fn build_body(&self, schema: &Value, system: &str, prompt: &str) -> Value {
        json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            // Adaptive thinking (required on this model family). No temperature/top_p/top_k.
            "thinking": { "type": "adaptive" },
            "system": system,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "output_config": {
                "format": {
                    "type": "json_schema",
                    "schema": schema
                }
            }
        })
    }

    /// Send the request; retry once on 429 honoring `retry-after`.
    fn send_with_retry(&self, body: &Value) -> Result<Value, ClaudeError> {
        match self.send_once(body) {
            Err(RawError::RateLimited { retry_after }) => {
                // Respect retry-after (capped) and retry exactly once.
                let wait = retry_after.unwrap_or(5).min(60);
                std::thread::sleep(Duration::from_secs(wait));
                match self.send_once(body) {
                    Ok(v) => Ok(v),
                    Err(RawError::RateLimited { .. }) => Err(ClaudeError::RateLimited),
                    Err(e) => Err(e.into()),
                }
            }
            Ok(v) => Ok(v),
            Err(e) => Err(e.into()),
        }
    }

    /// One HTTP round-trip.
    fn send_once(&self, body: &Value) -> Result<Value, RawError> {
        let resp = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(body)
            .send()
            .map_err(|e| RawError::Network(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok());
            return Err(RawError::RateLimited { retry_after });
        }

        let text = resp.text().map_err(|e| RawError::Network(e.to_string()))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|e| RawError::Parse(e.to_string()))?;

        // API-level error object: {"type":"error","error":{"type":..,"message":..}}
        if value.get("type").and_then(Value::as_str) == Some("error") {
            let err = value.get("error");
            let type_ = err
                .and_then(|e| e.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let message = err
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("(no message)")
                .to_string();
            return Err(RawError::Api { type_, message });
        }

        if !status.is_success() {
            return Err(RawError::Api {
                type_: format!("http_{}", status.as_u16()),
                message: "non-success HTTP status with no error body".to_string(),
            });
        }

        Ok(value)
    }
}

/// Internal raw error before mapping to the public `ClaudeError`.
enum RawError {
    Network(String),
    Parse(String),
    Api { type_: String, message: String },
    RateLimited { retry_after: Option<u64> },
}

impl From<RawError> for ClaudeError {
    fn from(e: RawError) -> Self {
        match e {
            RawError::Network(s) => ClaudeError::Network(s),
            RawError::Parse(s) => ClaudeError::Parse(s),
            RawError::Api { type_, message } => ClaudeError::Api { type_, message },
            RawError::RateLimited { .. } => ClaudeError::RateLimited,
        }
    }
}

/// Parse the Messages API response into normalized inputs.
///
/// Handles `stop_reason` (`refusal`, `max_tokens`) before reading `content`,
/// then extracts the first `text` block (guaranteed valid JSON by
/// `output_config.format`) and turns it into `NormInput`s keyed by param name.
fn parse_response(
    mapper: &dyn Mapper,
    value: &Value,
) -> Result<BTreeMap<String, NormInput>, ClaudeError> {
    match value.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => {
            let detail = value
                .get("stop_details")
                .and_then(|d| d.get("category"))
                .and_then(Value::as_str)
                .unwrap_or("safety")
                .to_string();
            return Err(ClaudeError::Refusal(detail));
        }
        Some("max_tokens") => return Err(ClaudeError::MaxTokens),
        _ => {}
    }

    // Find the first text content block.
    let text = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks.iter().find_map(|b| {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    b.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| ClaudeError::Parse("no text content block in response".into()))?;

    let obj: Value = serde_json::from_str(text)
        .map_err(|e| ClaudeError::Parse(format!("response text was not JSON: {e}")))?;
    let map = obj
        .as_object()
        .ok_or_else(|| ClaudeError::Parse("response JSON was not an object".into()))?;

    // Only keep keys we recognize as parameters for this synth.
    let mut out = BTreeMap::new();
    for spec in mapper.specs() {
        if let Some(v) = map.get(spec.name) {
            let input = match v {
                Value::Number(n) => NormInput::Num(n.as_f64().unwrap_or(0.0)),
                Value::String(s) => NormInput::Str(s.clone()),
                Value::Bool(b) => NormInput::Num(if *b { 1.0 } else { 0.0 }),
                _ => continue,
            };
            out.insert(spec.name.to_string(), input);
        }
    }
    Ok(out)
}

/// Concise, synth-specific sound-design system prompt.
pub fn system_prompt(synth: &str) -> String {
    let arch = match synth {
        "surge" => {
            "Surge XT is a hybrid subtractive/wavetable synth: three oscillators \
             (classic/wavetable/FM/etc.), a mixer, two multimode filters, an amp \
             ADSR and a filter ADSR, and LFOs. Filters shape brightness; the amp \
             envelope shapes the note contour; the filter envelope opens/closes the \
             filter over time."
        }
        "dexed" => {
            "Dexed emulates the Yamaha DX7: 6 FM operators arranged by one of 32 \
             algorithms, each operator with a coarse/fine frequency ratio and a \
             4-stage rate/level envelope, plus operator feedback and an LFO. \
             Brightness and timbre come from operator frequency ratios and how much \
             modulator level is fed into carriers; the envelope rates/levels shape \
             the note."
        }
        "vital" => {
            "Vital is a spectral wavetable synth: three wavetable oscillators, a \
             sample source, two multimode filters, two ADSR envelopes, two LFOs, and \
             reverb/delay/chorus/distortion effects. Wavetable position, filter \
             cutoff/resonance and the envelopes shape the sound."
        }
        _ => "A subtractive/FM software synthesizer.",
    };
    format!(
        "You are an expert sound designer creating a preset for the {synth} synthesizer.\n\n\
         {arch}\n\n\
         You will be given a text description of a desired sound. Respond with a JSON \
         object assigning every parameter in the provided schema.\n\n\
         Rules:\n\
         - Numeric parameters are NORMALIZED to 0.0..1.0 unless a property says otherwise; \
         0.0 is the minimum of the parameter's real range and 1.0 is the maximum. Choose \
         values that realize the requested sound.\n\
         - Enum (string) parameters must be exactly one of the listed options.\n\
         - Boolean parameters are true/false.\n\
         - Think about the whole signal path: pick oscillator/operator settings, filter \
         cutoff/resonance, and envelope times that together produce the described character \
         (e.g. a 'warm pad' wants a slow attack, low-ish cutoff, gentle resonance; a \
         'plucky bass' wants a fast attack, short decay, low sustain).\n\
         - Return ONLY the JSON object required by the schema."
    )
}
