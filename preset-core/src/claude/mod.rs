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

/// Synth-specific sound-design system prompt.
///
/// Structured as: common value conventions (normalized values + per-property
/// anchor tables), synth architecture, audibility rules (the checklist that
/// keeps a generated patch from being silent or degenerate), and archetype
/// recipes the model can adapt to the requested sound.
pub fn system_prompt(synth: &str) -> String {
    let specific = match synth {
        "surge" => {
            "ARCHITECTURE (Surge XT, scene A): three oscillators (classic saw/pulse, modern, \
             wavetable, window, sine, fm2, fm3) feed a mixer (per-osc levels, noise, ring-mod \
             1x2 / 2x3), then two multimode filters, a VCA. env1 = AMP ADSR (note contour), \
             env2 = FILTER ADSR (drives filter1 cutoff by filter_env_depth). Three voice LFOs \
             plus explicit mod-depth params (mod_lfo1_pitch = vibrato, mod_lfo1_cutoff = wah/\
             wobble, mod_env_pitch, mod_velocity_*).\n\n\
             AUDIBILITY RULES (a patch violating these is broken):\n\
             - At least one oscillator level >= 0.5. Unused oscillators: level 0.\n\
             - filter1_cutoff below 0.3 (~160 Hz) with an lp type strangles the sound; only \
             go that low for a filter-envelope sweep (then set filter_env_depth 0.5..1.0 so \
             env2 opens the filter) or a deep bass with high resonance.\n\
             - amp_sustain > 0.5 for any held sound (pad/lead/organ/strings); low sustain \
             only for plucks/percussion, and then amp_decay sets the note length.\n\
             - master_volume and output_gain 0.7..1.0. Do not stack low values.\n\
             - filter_env_depth is bipolar around 0.5: 0.5 = none, > 0.5 opens, < 0.5 closes.\n\n\
             RECIPES (adapt, don't copy):\n\
             - Warm pad: classic x2, osc2_pitch +0.5625 (=+6 st) or unison_voices 3..5 with \
             unison_detune 0.15..0.3; cutoff 0.45..0.6, lp24; amp attack 0.65..0.8 (0.5..2 s), \
             release 0.7+; slow lfo1 (rate 0.3..0.45) -> mod_lfo1_cutoff 0.55..0.6 for movement; \
             drift 0.2..0.4; character warm.\n\
             - Pluck/bass: fast attack (0..0.2), decay 0.45..0.6, sustain 0..0.3; cutoff \
             0.35..0.5 + filter_env_depth 0.7..0.9 (env2 snaps the filter); osc pitch -12 st \
             (=0.375) for bass; lp_ladder or lp24.\n\
             - Bright lead: cutoff 0.7..0.9, resonance 0.2..0.4; mod_lfo1_pitch ~0.53 (subtle \
             vibrato) with lfo1 rate ~0.75 (5 Hz); unison 2..3 voices; character bright.\n\
             - Bell/metallic: sine or fm2 osc + ring_12 0.4..0.7; long decay, sustain 0; \
             high cutoff.\n\
             - Strings/brass: saw (classic width ~0.5..1), cutoff 0.55..0.7, filter_env_depth \
             ~0.6, medium attack (0.4..0.6), high sustain; brass wants faster attack + \
             character bright."
        }
        "dexed" => {
            "ARCHITECTURE (Dexed = Yamaha DX7, 6-operator FM): the ALGORITHM (1..32) decides \
             which operators are CARRIERS (audible output) and which are MODULATORS (shape \
             the timbre of the operator below them). Carrier level = loudness; modulator \
             level = brightness/harmonic content — this is THE main timbre control. Each \
             operator has a 4-stage envelope (rates r1..r4, levels l1..l4; l3 = sustain, \
             r4/l4 = release). Feedback on one operator adds saw-like buzz.\n\n\
             CARRIER MAP (by algorithm, op numbers): 1..2 -> {1,3}; 3..4 -> {1,4}; \
             5..6 -> {1,3,5}; 7..9 -> {1,3}; 10..11 -> {1,4}; 12..15 -> {1,3}; 16..18 -> {1}; \
             19 -> {1,4,5}; 20 -> {1,2,4}; 21 -> {1,2,4,5}; 22 -> {1,3,4,5}; 23 -> {1,2,4,5}; \
             24..25 -> {1,2,3,4,5}; 26..27 -> {1,2,4}; 28 -> {1,3,6}; 29 -> {1,2,3,5}; \
             30 -> {1,2,3,6}; 31 -> {1,2,3,4,5}; 32 -> all six.\n\n\
             AUDIBILITY RULES:\n\
             - EVERY carrier of the chosen algorithm: level >= 90 (normalized ~0.9+), eg_l1 \
             ~99, and eg_r4 >= 25 with eg_l4 = 0. A carrier at low level = silence.\n\
             - Unused modulators: level 0. Active modulators: 40..85 (higher = brighter).\n\
             - Keep ALL pitch_eg levels at 50 (≈0.505 normalized) unless a pitch sweep is \
             explicitly wanted — other values detune the whole patch.\n\
             - transpose 0 (=0.5 normalized) unless an octave shift is wanted.\n\n\
             RECIPES:\n\
             - E-piano (DX7 classic): algorithm 5, three carrier/modulator pairs; carriers \
             ratio 1 (coarse 1), modulators coarse 1 and one pair coarse 14 for the tine; \
             modulator levels 55..75; carrier vel_sens 2..3, modulator vel_sens 5..7; fast \
             attack, l3 ~0 with slow r3 for the singing decay.\n\
             - Bell/tine: modulator coarse 3..7 vs carrier 1 (inharmonic: add fine ~41); \
             long decay (r2/r3 low), sustain l3 = 0.\n\
             - FM bass: algorithm 1 or 16; modulator level 70..85 for growl, carrier ratio \
             0.5 (coarse 0); fast envelope, medium l3.\n\
             - Brass: algorithm 18 or 22, feedback 5..7, modulator levels ~70, attack r1 \
             ~55..70 (slight blip), high l3.\n\
             - Organ: algorithm 32, several carriers at ratios 0.5/1/2/3, levels 85..99, \
             instant envelopes, l3 = 99.\n\
             - Pad: slow r1 (30..50) on carriers AND modulators, high l3, slow r4 (20..35); \
             lfo_pmd 10..20 with lfo_speed ~35 for chorus-like movement."
        }
        "vital" => {
            "ARCHITECTURE (Vital, spectral wavetable): three wavetable oscillators (osc_N_wave \
             = scan position, higher = brighter/more complex) + sample source -> filter 1 / \
             filter 2 (osc1 routes to filter 1, osc2 to filter 2 by default) -> effects \
             (reverb, delay, chorus, distortion). env_1 is HARDWIRED to amplitude; env_2 is \
             free — wire it (or an LFO/velocity) to a target via mod_1_source/destination/\
             amount. Filter cutoff is in MIDI-note terms (see anchors), resonance 0..1.\n\n\
             AUDIBILITY RULES:\n\
             - osc_1_level >= 0.6 for the main voice; unused oscillators level 0. (Any \
             non-zero level auto-enables its oscillator; mixes auto-enable filters/effects.)\n\
             - env_1_sustain > 0.5 for held sounds; plucks: sustain 0, decay sets length.\n\
             - filter_1_mix 1.0 when filtering; filter_1_cutoff below ~0.35 (~note 53/175 Hz) \
             muffles everything unless env_2 or an LFO opens it via the mod matrix.\n\
             - master_volume 0.7..0.9.\n\
             - For filter movement you MUST use the mod matrix: e.g. mod_1_source env_2, \
             mod_1_destination filter_1_cutoff, mod_1_amount 0.3..0.8.\n\n\
             RECIPES:\n\
             - Supersaw/trance: osc_1_wave ~0.2..0.4, unison_voices 9..16, unison_detune \
             0.3..0.5, cutoff 0.6..0.8, chorus_mix 0.3, reverb_mix 0.25.\n\
             - Wobble/dubstep bass: osc tune -24 st (0.25), lfo_1 -> filter_1_cutoff via mod \
             matrix amount 0.6..0.9, lfo_1_frequency 0.3..0.6 (1..5 Hz), ladder/dirty filter, \
             distortion_mix 0.2..0.4.\n\
             - Lush pad: two oscs detuned (osc_2_tune ±0.07 st offsets near 0.5), slow attack \
             0.6..0.75 (0.5..2 s), long release, chorus_mix 0.4, reverb_mix 0.35, \
             reverb_decay_time 0.6..0.8; slow lfo -> cutoff, small amount.\n\
             - Pluck: attack 0, decay ~0.5 (0.1..0.3 s), sustain 0; env_2 -> cutoff amount \
             0.5..0.7 with env_2 decay matching; delay_mix 0.2 for space.\n\
             - Keys/EP: formant or analog filter, moderate cutoff, velocity -> osc_1_level \
             via mod matrix (amount 0.4..0.6), chorus_mix 0.3."
        }
        _ => "A subtractive/FM software synthesizer.",
    };
    format!(
        "You are an expert sound designer creating a preset for the {synth} synthesizer.\n\n\
         You will be given a text description of a desired sound. Respond with a JSON object \
         assigning every parameter in the provided schema.\n\n\
         VALUE CONVENTIONS:\n\
         - Every numeric parameter is NORMALIZED 0.0..1.0. Each property description carries \
         an anchor table '0→a, 0.25→b, 0.5→c, 0.75→d, 1→e' showing the REAL value (Hz, \
         seconds, semitones, integer steps) at those normalized points — interpolate between \
         anchors to hit a real target. NEVER output real-unit values (e.g. for a 500 Hz \
         cutoff output the normalized fraction, not 500).\n\
         - Integer-valued parameters (algorithm, voices, ratios) are ALSO normalized — use \
         the formula/example in their description.\n\
         - Enum parameters: exactly one of the listed options. Booleans: true/false.\n\n\
         {specific}\n\n\
         Design the WHOLE signal path coherently for the requested character, respect every \
         audibility rule, and return ONLY the JSON object required by the schema."
    )
}
