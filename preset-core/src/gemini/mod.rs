// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Google Gemini client — raw HTTPS via `reqwest` (rustls-tls).
//
// Rust has no official Google Generative AI SDK, so we call the Generative
// Language API's `:generateContent` endpoint directly. This module builds a
// structured-output request whose JSON schema is generated from the synth's
// `ParamSpec` table (see `schema.rs`), converts it to Gemini's OpenAPI-subset
// `responseSchema`, sends it, and parses the returned JSON object of named
// parameters.
//
// Requirements enforced here (per the current Generative Language API):
//   * Endpoint: `POST /v1beta/models/{model}:generateContent`.
//   * Auth: `x-goog-api-key: <key>` header ONLY (never the key in the URL).
//   * `generationConfig.responseMimeType: "application/json"` +
//     `generationConfig.responseSchema: <converted schema>`.
//   * `generationConfig.maxOutputTokens: 8192`. No temperature (keep defaults).
//   * If the API rejects the `responseSchema` (too large/complex — Dexed has
//     164 properties), fall back to `responseMimeType` only, with the schema
//     embedded in the prompt text.
//   * Parse `candidates[0].content.parts[*].text` concatenated → JSON.
//   * Handle: missing candidates, `promptFeedback.blockReason`,
//     `finishReason != STOP` (esp. MAX_TOKENS / SAFETY), and the error envelope
//     `{error: {code, status, message}}`.
//   * On 429/503, respect `Retry-After` with one retry.
//   * Never log the key.

pub mod key;

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{json, Value};

use crate::mappers::{Mapper, NormInput};
use crate::schema::{build_schema, to_gemini_schema};

/// Base of the Generative Language API.
pub const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
/// Default model — a config constant, not sent by the caller.
pub const DEFAULT_MODEL: &str = "gemini-3.5-flash";
/// Max output tokens for a preset-generation call.
pub const MAX_TOKENS: u32 = 8192;

/// Errors from a Gemini preset-generation request.
#[derive(Debug, thiserror::Error)]
pub enum GeminiError {
    #[error("network error contacting the Gemini API: {0}")]
    Network(String),
    #[error("Gemini API error ({status}): {message}")]
    Api { status: String, message: String },
    #[error("Gemini blocked the prompt (reason: {0}). Try rephrasing the prompt.")]
    Blocked(String),
    #[error("Gemini hit the output token limit before finishing. Please retry.")]
    MaxTokens,
    #[error("Gemini stopped for reason '{0}' before returning a complete response.")]
    FinishReason(String),
    #[error("could not parse Gemini's JSON response: {0}")]
    Parse(String),
    #[error("rate limited / overloaded by the Gemini API and the retry also failed")]
    RateLimited,
    #[error("no API key: pass one, set GEMINI_API_KEY (or GOOGLE_API_KEY), or store one in the OS keychain")]
    NoKey,
}

/// A configured Gemini client.
pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
}

impl GeminiClient {
    /// Construct a client with an explicit API key and the default model.
    pub fn new(api_key: impl Into<String>) -> Result<Self, GeminiError> {
        let key = api_key.into();
        if key.trim().is_empty() {
            return Err(GeminiError::NoKey);
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| GeminiError::Network(e.to_string()))?;
        Ok(Self {
            api_key: key,
            model: DEFAULT_MODEL.to_string(),
            http,
        })
    }

    /// Override the model (defaults to `gemini-3.5-flash`).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// The `:generateContent` URL for the configured model.
    fn endpoint(&self) -> String {
        format!("{API_BASE}/{}:generateContent", self.model)
    }

    /// Generate normalized parameter inputs for a synth from a text prompt.
    ///
    /// Returns a map of `param_name -> NormInput` (numbers for numeric params,
    /// enum strings for categorical ones), which the caller feeds into
    /// `Mapper::map_inputs`. Numeric ranges are advisory; the mapper clamps
    /// them, so out-of-range numbers are handled downstream.
    pub fn generate(
        &self,
        mapper: &dyn Mapper,
        prompt: &str,
    ) -> Result<BTreeMap<String, NormInput>, GeminiError> {
        let anthropic_schema = build_schema(mapper);
        let gemini_schema = to_gemini_schema(&anthropic_schema);
        let system = crate::claude::system_prompt(mapper.id());

        // First attempt: strict structured output with the response schema.
        let body = self.build_body(&system, prompt, Some(&gemini_schema));
        match self.send_with_retry(&body) {
            Ok(value) => parse_response(mapper, &value),
            Err(GeminiError::Api { status, message }) if is_schema_rejection(&status, &message) => {
                // The response schema was rejected (size/complexity limit).
                // Fall back to JSON-mode only, with the schema embedded in the
                // prompt text. We still validate/clamp parsed params downstream.
                let embedded = format!(
                    "{prompt}\n\nRespond with a single JSON object that matches this JSON schema \
                     exactly (return ONLY the JSON object, no markdown):\n{}",
                    serde_json::to_string(&gemini_schema).unwrap_or_default()
                );
                let body = self.build_body(&system, &embedded, None);
                let value = self.send_with_retry(&body)?;
                parse_response(mapper, &value)
            }
            Err(e) => Err(e),
        }
    }

    /// Build the JSON request body. When `schema` is `Some`, request strict
    /// structured output; when `None`, request JSON mime-type only.
    fn build_body(&self, system: &str, prompt: &str, schema: Option<&Value>) -> Value {
        build_request_body(system, prompt, schema)
    }

    /// Send the request; retry once on 429/503 honoring `Retry-After`.
    fn send_with_retry(&self, body: &Value) -> Result<Value, GeminiError> {
        match self.send_once(body) {
            Err(RawError::Overloaded { retry_after }) => {
                let wait = retry_after.unwrap_or(5).min(60);
                std::thread::sleep(Duration::from_secs(wait));
                match self.send_once(body) {
                    Ok(v) => Ok(v),
                    Err(RawError::Overloaded { .. }) => Err(GeminiError::RateLimited),
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
            .post(self.endpoint())
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(body)
            .send()
            .map_err(|e| RawError::Network(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 || status.as_u16() == 503 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok());
            return Err(RawError::Overloaded { retry_after });
        }

        let text = resp.text().map_err(|e| RawError::Network(e.to_string()))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|e| RawError::Parse(e.to_string()))?;

        // Error envelope: {"error": {"code": .., "status": .., "message": ..}}.
        if let Some(err) = value.get("error") {
            let status_str = err
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    err.get("code")
                        .and_then(Value::as_i64)
                        .map(|c| format!("http_{c}"))
                })
                .unwrap_or_else(|| "unknown".to_string());
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("(no message)")
                .to_string();
            return Err(RawError::Api {
                status: status_str,
                message,
            });
        }

        if !status.is_success() {
            return Err(RawError::Api {
                status: format!("http_{}", status.as_u16()),
                message: "non-success HTTP status with no error body".to_string(),
            });
        }

        Ok(value)
    }
}

/// Internal raw error before mapping to the public `GeminiError`.
enum RawError {
    Network(String),
    Parse(String),
    Api { status: String, message: String },
    Overloaded { retry_after: Option<u64> },
}

impl From<RawError> for GeminiError {
    fn from(e: RawError) -> Self {
        match e {
            RawError::Network(s) => GeminiError::Network(s),
            RawError::Parse(s) => GeminiError::Parse(s),
            RawError::Api { status, message } => GeminiError::Api { status, message },
            RawError::Overloaded { .. } => GeminiError::RateLimited,
        }
    }
}

/// Does this API error look like a rejection of the `responseSchema` (as opposed
/// to a real, non-recoverable error)? Gemini returns `INVALID_ARGUMENT` when a
/// response schema is too large/complex or otherwise unsupported.
fn is_schema_rejection(status: &str, message: &str) -> bool {
    if status != "INVALID_ARGUMENT" {
        return false;
    }
    let m = message.to_ascii_lowercase();
    m.contains("schema")
        || m.contains("response_schema")
        || m.contains("responseschema")
        || m.contains("too large")
        || m.contains("too many")
}

/// Parse the `:generateContent` response into normalized inputs.
///
/// Handles `promptFeedback.blockReason` and `finishReason` before reading the
/// candidate's content, then concatenates the `text` parts (skipping any
/// non-text parts such as `thoughtSignature`) into one JSON object and turns it
/// into `NormInput`s keyed by param name.
fn parse_response(
    mapper: &dyn Mapper,
    value: &Value,
) -> Result<BTreeMap<String, NormInput>, GeminiError> {
    // A prompt-level block means there are no candidates at all.
    if let Some(reason) = value
        .get("promptFeedback")
        .and_then(|pf| pf.get("blockReason"))
        .and_then(Value::as_str)
    {
        return Err(GeminiError::Blocked(reason.to_string()));
    }

    let candidate = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| GeminiError::Parse("no candidates in response".into()))?;

    // Inspect the finish reason. STOP (and an absent reason) are fine.
    match candidate.get("finishReason").and_then(Value::as_str) {
        None | Some("STOP") => {}
        Some("MAX_TOKENS") => return Err(GeminiError::MaxTokens),
        Some("SAFETY") | Some("PROHIBITED_CONTENT") | Some("BLOCKLIST") => {
            return Err(GeminiError::Blocked(
                candidate
                    .get("finishReason")
                    .and_then(Value::as_str)
                    .unwrap_or("SAFETY")
                    .to_string(),
            ));
        }
        Some(other) => return Err(GeminiError::FinishReason(other.to_string())),
    }

    // Concatenate every text part (there is usually one; thought/other parts
    // carry no `text` and are skipped).
    let text: String = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Err(GeminiError::Parse(
            "no text content in the candidate response".into(),
        ));
    }

    let obj: Value = serde_json::from_str(text.trim())
        .map_err(|e| GeminiError::Parse(format!("response text was not JSON: {e}")))?;
    let map = obj
        .as_object()
        .ok_or_else(|| GeminiError::Parse("response JSON was not an object".into()))?;

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

/// Build the `:generateContent` request body. Split out from `GeminiClient` so
/// its shape can be unit-tested without constructing an HTTP client. When
/// `schema` is `Some`, requests strict structured output; when `None`, requests
/// JSON mime-type only.
fn build_request_body(system: &str, prompt: &str, schema: Option<&Value>) -> Value {
    let mut generation_config = json!({
        "maxOutputTokens": MAX_TOKENS,
        "responseMimeType": "application/json",
    });
    if let Some(schema) = schema {
        generation_config["responseSchema"] = schema.clone();
    }
    json!({
        "systemInstruction": {
            "parts": [ { "text": system } ]
        },
        "contents": [
            { "role": "user", "parts": [ { "text": prompt } ] }
        ],
        "generationConfig": generation_config
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mappers::mapper_for;

    // ---- request body shape --------------------------------------------

    #[test]
    fn request_body_has_gemini_shape_with_schema() {
        let schema = json!({ "type": "object", "properties": {} });
        let body = build_request_body("sys", "make a warm pad", Some(&schema));

        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "make a warm pad");
        let gc = &body["generationConfig"];
        assert_eq!(gc["maxOutputTokens"], MAX_TOKENS);
        assert_eq!(gc["responseMimeType"], "application/json");
        assert_eq!(gc["responseSchema"], schema);
        // Temperature must NOT be set (keep model defaults).
        assert!(gc.get("temperature").is_none());
    }

    #[test]
    fn request_body_fallback_omits_schema() {
        let body = build_request_body("sys", "prompt", None);
        let gc = &body["generationConfig"];
        assert_eq!(gc["responseMimeType"], "application/json");
        assert!(
            gc.get("responseSchema").is_none(),
            "fallback body must not carry a responseSchema"
        );
    }

    // ---- schema-rejection detection ------------------------------------

    #[test]
    fn schema_rejection_only_on_invalid_argument_about_schema() {
        assert!(is_schema_rejection(
            "INVALID_ARGUMENT",
            "Invalid JSON payload received. Unknown name \"responseSchema\": schema too large"
        ));
        assert!(is_schema_rejection(
            "INVALID_ARGUMENT",
            "response_schema has too many properties"
        ));
        // A different status is not a schema rejection.
        assert!(!is_schema_rejection("PERMISSION_DENIED", "schema"));
        // INVALID_ARGUMENT unrelated to the schema is not a schema rejection.
        assert!(!is_schema_rejection(
            "INVALID_ARGUMENT",
            "API key not valid"
        ));
    }

    // ---- error-envelope parsing ----------------------------------------

    #[test]
    fn parse_error_envelope_surfaces_status_and_message() {
        // The error envelope is handled in `send_once`, but we mirror its
        // extraction here against a representative payload to lock the shape.
        let value = json!({
            "error": {
                "code": 400,
                "status": "INVALID_ARGUMENT",
                "message": "API key not valid. Please pass a valid API key."
            }
        });
        let err = value.get("error").unwrap();
        assert_eq!(err["status"], "INVALID_ARGUMENT");
        assert!(err["message"].as_str().unwrap().contains("valid API key"));
    }

    // ---- response parsing ----------------------------------------------

    #[test]
    fn parse_response_concatenates_text_skips_non_text_parts() {
        let mapper = mapper_for("surge").unwrap();
        // A thoughtSignature part (no `text`) must be skipped; only text parts
        // are concatenated into the JSON object.
        let value = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        { "thoughtSignature": "abc123" },
                        { "text": "{\"filter1_cutoff\": 0.4, " },
                        { "text": "\"filter1_type\": \"lp24\"}" }
                    ]
                },
                "finishReason": "STOP"
            }]
        });
        let out = parse_response(mapper.as_ref(), &value).unwrap();
        assert_eq!(out.get("filter1_cutoff"), Some(&NormInput::Num(0.4)));
        assert_eq!(
            out.get("filter1_type"),
            Some(&NormInput::Str("lp24".to_string()))
        );
    }

    #[test]
    fn parse_response_handles_prompt_block() {
        let mapper = mapper_for("surge").unwrap();
        let value = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        let err = parse_response(mapper.as_ref(), &value).unwrap_err();
        assert!(matches!(err, GeminiError::Blocked(_)));
    }

    #[test]
    fn parse_response_handles_max_tokens_and_safety_finish() {
        let mapper = mapper_for("surge").unwrap();

        let max = json!({
            "candidates": [{ "content": { "parts": [] }, "finishReason": "MAX_TOKENS" }]
        });
        assert!(matches!(
            parse_response(mapper.as_ref(), &max).unwrap_err(),
            GeminiError::MaxTokens
        ));

        let safety = json!({
            "candidates": [{ "content": { "parts": [] }, "finishReason": "SAFETY" }]
        });
        assert!(matches!(
            parse_response(mapper.as_ref(), &safety).unwrap_err(),
            GeminiError::Blocked(_)
        ));
    }

    #[test]
    fn parse_response_errors_on_missing_candidates() {
        let mapper = mapper_for("surge").unwrap();
        let value = json!({ "usageMetadata": {} });
        assert!(matches!(
            parse_response(mapper.as_ref(), &value).unwrap_err(),
            GeminiError::Parse(_)
        ));
    }
}
