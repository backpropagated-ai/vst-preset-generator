// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// OpenAI-compatible Chat Completions client — raw HTTPS via `reqwest`
// (rustls-tls). One client covers every backend that speaks the OpenAI
// `/v1/chat/completions` API: OpenAI itself, OpenRouter, and local servers
// (Ollama, LM Studio). They differ only in base URL, auth requirement, and
// default model — captured by [`Flavor`].
//
// The request uses structured outputs via `response_format`:
//   * First attempt: `{"type":"json_schema","json_schema":{...,"strict":true}}`
//     (OpenAI / OpenRouter / recent LM Studio & Ollama support this).
//   * On rejection (older local servers), fall back to
//     `{"type":"json_object"}` with the schema embedded in the prompt text —
//     the same degradation the Gemini client uses. Parsed params are
//     clamped/validated downstream either way.
//
// The JSON schema comes from the synth's `ParamSpec` table (see `schema.rs`).
// The `additionalProperties:false`, `required`-everything object it produces is
// exactly what OpenAI strict structured outputs want, so no conversion is
// needed (unlike Gemini).

pub mod key;

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{json, Value};

use crate::mappers::{Mapper, NormInput};
use crate::schema::build_schema;

/// Max output tokens for a preset-generation call.
pub const MAX_TOKENS: u32 = 8000;

/// A concrete OpenAI-compatible backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// api.openai.com — OpenAI's own hosted models (ChatGPT family).
    OpenAi,
    /// openrouter.ai — a router in front of many hosted models.
    OpenRouter,
    /// A local Ollama server (`http://localhost:11434/v1`).
    Ollama,
    /// A local LM Studio server (`http://localhost:1234/v1`).
    LmStudio,
}

impl Flavor {
    /// The canonical id string.
    pub fn id(self) -> &'static str {
        match self {
            Flavor::OpenAi => "openai",
            Flavor::OpenRouter => "openrouter",
            Flavor::Ollama => "ollama",
            Flavor::LmStudio => "lmstudio",
        }
    }

    /// Human-readable label for menus.
    pub fn label(self) -> &'static str {
        match self {
            Flavor::OpenAi => "ChatGPT (OpenAI)",
            Flavor::OpenRouter => "OpenRouter",
            Flavor::Ollama => "Ollama (local)",
            Flavor::LmStudio => "LM Studio (local)",
        }
    }

    /// The default base URL (up to and including `/v1`). Overridable per run via
    /// the client's `with_base_url` (or the `*_BASE_URL` env vars a UI wires up).
    pub fn default_base_url(self) -> &'static str {
        match self {
            Flavor::OpenAi => "https://api.openai.com/v1",
            Flavor::OpenRouter => "https://openrouter.ai/api/v1",
            Flavor::Ollama => "http://localhost:11434/v1",
            Flavor::LmStudio => "http://localhost:1234/v1",
        }
    }

    /// The default model id for this backend.
    pub fn default_model(self) -> &'static str {
        match self {
            Flavor::OpenAi => "gpt-5.1",
            Flavor::OpenRouter => "openai/gpt-5.1",
            // Local servers vary widely; these are common, sane defaults the
            // user is expected to override with `--model`.
            Flavor::Ollama => "llama3.1",
            Flavor::LmStudio => "local-model",
        }
    }

    /// Whether this backend requires an API key. Local servers do not.
    pub fn requires_key(self) -> bool {
        matches!(self, Flavor::OpenAi | Flavor::OpenRouter)
    }
}

/// Errors from an OpenAI-compatible preset-generation request.
#[derive(Debug, thiserror::Error)]
pub enum OpenAiError {
    #[error("network error contacting {backend} at {base_url}: {source_msg}")]
    Network {
        backend: &'static str,
        base_url: String,
        source_msg: String,
    },
    #[error("{backend} API error ({status}): {message}")]
    Api {
        backend: &'static str,
        status: String,
        message: String,
    },
    #[error("{0} refused the request. Try rephrasing the prompt.")]
    Refusal(&'static str),
    #[error("{0} hit the output token limit before finishing. Please retry.")]
    MaxTokens(&'static str),
    #[error("could not parse {backend}'s JSON response: {detail}")]
    Parse {
        backend: &'static str,
        detail: String,
    },
    #[error("rate limited by {0} and the retry also failed")]
    RateLimited(&'static str),
    #[error("no API key for {0}: pass one, set its env var, or store one in the OS keychain")]
    NoKey(&'static str),
}

/// A configured OpenAI-compatible client.
pub struct OpenAiClient {
    flavor: Flavor,
    api_key: Option<String>,
    base_url: String,
    model: String,
    http: reqwest::blocking::Client,
}

impl OpenAiClient {
    /// Construct a client for `flavor` with an optional API key (local backends
    /// accept `None`/empty). Uses the flavor's default base URL and model.
    pub fn new(flavor: Flavor, api_key: Option<&str>) -> Result<Self, OpenAiError> {
        let api_key = api_key
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string);
        if flavor.requires_key() && api_key.is_none() {
            return Err(OpenAiError::NoKey(flavor.label()));
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| OpenAiError::Network {
                backend: flavor.label(),
                base_url: flavor.default_base_url().to_string(),
                source_msg: e.to_string(),
            })?;
        Ok(Self {
            flavor,
            api_key,
            base_url: flavor.default_base_url().to_string(),
            model: flavor.default_model().to_string(),
            http,
        })
    }

    /// Override the model id.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let m = model.into();
        if !m.trim().is_empty() {
            self.model = m;
        }
        self
    }

    /// Override the base URL (e.g. a non-default local port or a proxy). A
    /// trailing `/` and a trailing `/chat/completions` are trimmed so callers
    /// can pass either the `/v1` root or a full endpoint.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let mut b = base_url.into().trim().trim_end_matches('/').to_string();
        if let Some(stripped) = b.strip_suffix("/chat/completions") {
            b = stripped.trim_end_matches('/').to_string();
        }
        if !b.is_empty() {
            self.base_url = b;
        }
        self
    }

    /// The `/chat/completions` URL for the configured base.
    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// Generate normalized parameter inputs for a synth from a text prompt.
    pub fn generate(
        &self,
        mapper: &dyn Mapper,
        prompt: &str,
    ) -> Result<BTreeMap<String, NormInput>, OpenAiError> {
        let schema = build_schema(mapper);
        let system = crate::claude::system_prompt(mapper.id());

        // First attempt: strict json_schema structured output.
        let body = build_request_body(&self.model, &system, prompt, Some(&schema));
        match self.send_with_retry(&body) {
            Ok(value) => parse_response(self.flavor, mapper, &value),
            Err(OpenAiError::Api { status, message, .. })
                if is_response_format_rejection(&status, &message) =>
            {
                // Fall back to json_object with the schema embedded in the
                // prompt (older Ollama / LM Studio builds).
                let embedded = format!(
                    "{prompt}\n\nRespond with a single JSON object matching this JSON schema \
                     exactly (return ONLY the JSON object, no markdown):\n{}",
                    serde_json::to_string(&schema).unwrap_or_default()
                );
                let body = build_request_body(&self.model, &system, &embedded, None);
                let value = self.send_with_retry(&body)?;
                parse_response(self.flavor, mapper, &value)
            }
            Err(e) => Err(e),
        }
    }

    /// Send the request; retry once on 429 honoring `retry-after`.
    fn send_with_retry(&self, body: &Value) -> Result<Value, OpenAiError> {
        match self.send_once(body) {
            Err(RawError::RateLimited { retry_after }) => {
                let wait = retry_after.unwrap_or(5).min(60);
                std::thread::sleep(Duration::from_secs(wait));
                match self.send_once(body) {
                    Ok(v) => Ok(v),
                    Err(RawError::RateLimited { .. }) => {
                        Err(OpenAiError::RateLimited(self.flavor.label()))
                    }
                    Err(e) => Err(self.raw_into(e)),
                }
            }
            Ok(v) => Ok(v),
            Err(e) => Err(self.raw_into(e)),
        }
    }

    /// One HTTP round-trip.
    fn send_once(&self, body: &Value) -> Result<Value, RawError> {
        let mut req = self
            .http
            .post(self.endpoint())
            .header("content-type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.header("authorization", format!("Bearer {key}"));
        }
        // OpenRouter asks for attribution headers; harmless elsewhere.
        if self.flavor == Flavor::OpenRouter {
            req = req
                .header("http-referer", "https://github.com/backpropagated-ai")
                .header("x-title", "DeepSynth Preset");
        }

        let resp = req
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

        // Error envelope: {"error": {"message": .., "type"/"code": ..}}.
        if let Some(err) = value.get("error") {
            let status_str = err
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    err.get("code").map(|c| match c {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                })
                .unwrap_or_else(|| format!("http_{}", status.as_u16()));
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

    /// Attach the backend label + base URL context to a raw error.
    fn raw_into(&self, e: RawError) -> OpenAiError {
        match e {
            RawError::Network(s) => OpenAiError::Network {
                backend: self.flavor.label(),
                base_url: self.base_url.clone(),
                source_msg: s,
            },
            RawError::Parse(s) => OpenAiError::Parse {
                backend: self.flavor.label(),
                detail: s,
            },
            RawError::Api { status, message } => OpenAiError::Api {
                backend: self.flavor.label(),
                status,
                message,
            },
            RawError::RateLimited { .. } => OpenAiError::RateLimited(self.flavor.label()),
        }
    }
}

/// Internal raw error before backend context is attached.
enum RawError {
    Network(String),
    Parse(String),
    Api { status: String, message: String },
    RateLimited { retry_after: Option<u64> },
}

/// Does this API error look like a rejection of `response_format` /
/// `json_schema` support (older local servers), as opposed to a real error?
fn is_response_format_rejection(status: &str, message: &str) -> bool {
    let s = status.to_ascii_lowercase();
    let m = message.to_ascii_lowercase();
    // A 400/invalid-request that names response_format or json_schema.
    let looks_like_bad_request = s.contains("invalid_request")
        || s.contains("http_400")
        || s.contains("400")
        || s.contains("bad_request");
    let names_feature = m.contains("response_format")
        || m.contains("json_schema")
        || m.contains("json schema")
        || m.contains("structured output")
        || m.contains("not supported")
        || m.contains("unsupported");
    looks_like_bad_request && names_feature
}

/// Build the `/chat/completions` request body. Split out so its shape can be
/// unit-tested. When `schema` is `Some`, requests strict `json_schema`; when
/// `None`, requests `json_object`.
fn build_request_body(model: &str, system: &str, prompt: &str, schema: Option<&Value>) -> Value {
    let response_format = match schema {
        Some(schema) => json!({
            "type": "json_schema",
            "json_schema": {
                "name": "preset_parameters",
                "strict": true,
                "schema": schema,
            }
        }),
        None => json!({ "type": "json_object" }),
    };
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": prompt },
        ],
        "response_format": response_format,
    })
}

/// Parse a Chat Completions response into normalized inputs.
fn parse_response(
    flavor: Flavor,
    mapper: &dyn Mapper,
    value: &Value,
) -> Result<BTreeMap<String, NormInput>, OpenAiError> {
    let backend = flavor.label();
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| OpenAiError::Parse {
            backend,
            detail: "no choices in response".into(),
        })?;

    match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => return Err(OpenAiError::MaxTokens(backend)),
        Some("content_filter") => return Err(OpenAiError::Refusal(backend)),
        _ => {}
    }

    let message = choice.get("message").ok_or_else(|| OpenAiError::Parse {
        backend,
        detail: "choice had no message".into(),
    })?;

    // An explicit refusal field (OpenAI structured outputs) short-circuits.
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        if !refusal.trim().is_empty() {
            return Err(OpenAiError::Refusal(backend));
        }
    }

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenAiError::Parse {
            backend,
            detail: "message had no text content".into(),
        })?;

    let obj: Value = serde_json::from_str(content.trim()).map_err(|e| OpenAiError::Parse {
        backend,
        detail: format!("response text was not JSON: {e}"),
    })?;
    let map = obj.as_object().ok_or_else(|| OpenAiError::Parse {
        backend,
        detail: "response JSON was not an object".into(),
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mappers::mapper_for;

    #[test]
    fn request_body_uses_json_schema_when_present() {
        let schema = json!({ "type": "object", "properties": {} });
        let body = build_request_body("gpt-5.1", "sys", "warm pad", Some(&schema));
        assert_eq!(body["model"], "gpt-5.1");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "sys");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "warm pad");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
    }

    #[test]
    fn request_body_falls_back_to_json_object() {
        let body = build_request_body("m", "sys", "p", None);
        assert_eq!(body["response_format"]["type"], "json_object");
        assert!(body["response_format"].get("json_schema").is_none());
    }

    #[test]
    fn response_format_rejection_detection() {
        assert!(is_response_format_rejection(
            "invalid_request_error",
            "response_format.type json_schema is not supported"
        ));
        assert!(is_response_format_rejection(
            "http_400",
            "this model does not support structured output"
        ));
        // A genuine auth error is not a response-format rejection.
        assert!(!is_response_format_rejection(
            "invalid_api_key",
            "Incorrect API key provided"
        ));
    }

    #[test]
    fn parse_response_extracts_named_params() {
        let mapper = mapper_for("surge").unwrap();
        let value = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "{\"filter1_cutoff\": 0.6, \"filter1_type\": \"lp24\"}"
                }
            }]
        });
        let out = parse_response(Flavor::OpenAi, mapper.as_ref(), &value).unwrap();
        assert_eq!(out.get("filter1_cutoff"), Some(&NormInput::Num(0.6)));
        assert_eq!(
            out.get("filter1_type"),
            Some(&NormInput::Str("lp24".to_string()))
        );
    }

    #[test]
    fn parse_response_handles_length_and_refusal() {
        let mapper = mapper_for("surge").unwrap();
        let length = json!({
            "choices": [{ "finish_reason": "length", "message": { "content": "" } }]
        });
        assert!(matches!(
            parse_response(Flavor::OpenAi, mapper.as_ref(), &length).unwrap_err(),
            OpenAiError::MaxTokens(_)
        ));
        let refusal = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": { "refusal": "I can't help with that." }
            }]
        });
        assert!(matches!(
            parse_response(Flavor::OpenAi, mapper.as_ref(), &refusal).unwrap_err(),
            OpenAiError::Refusal(_)
        ));
    }

    #[test]
    fn local_flavors_do_not_require_a_key() {
        assert!(!Flavor::Ollama.requires_key());
        assert!(!Flavor::LmStudio.requires_key());
        assert!(Flavor::OpenAi.requires_key());
        // Constructing a local client without a key succeeds.
        assert!(OpenAiClient::new(Flavor::Ollama, None).is_ok());
        // A hosted client without a key fails cleanly.
        assert!(matches!(
            OpenAiClient::new(Flavor::OpenAi, None),
            Err(OpenAiError::NoKey(_))
        ));
    }

    #[test]
    fn base_url_override_trims_endpoint_suffix() {
        let c = OpenAiClient::new(Flavor::LmStudio, None)
            .unwrap()
            .with_base_url("http://localhost:4321/v1/chat/completions/");
        assert_eq!(c.endpoint(), "http://localhost:4321/v1/chat/completions");
    }
}
