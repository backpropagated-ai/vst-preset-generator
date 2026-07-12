// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Provider abstraction — which LLM backend turns a text prompt into named
// parameters. Every provider shares the same `ParamSpec`-derived schema and the
// same downstream mapping/writing; they differ only in the HTTP client, the
// default model, and where the API key comes from.
//
// Providers:
//   * `Claude`  — Anthropic Messages API.
//   * `Gemini`  — Google Generative Language API.
//   * `OpenAi` / `OpenRouter` / `Ollama` / `LmStudio` — the OpenAI-compatible
//     Chat Completions API (see `openai/`). The last two are LOCAL servers and
//     need no API key.

use crate::claude::key::KeySource;
use crate::openai::Flavor;

/// A supported text-to-preset LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Gemini,
    /// An OpenAI-compatible backend (OpenAI, OpenRouter, Ollama, LM Studio).
    OpenAi(Flavor),
}

impl Provider {
    /// Convenience constructors for the OpenAI-compatible flavors.
    pub const OPENAI: Provider = Provider::OpenAi(Flavor::OpenAi);
    pub const OPENROUTER: Provider = Provider::OpenAi(Flavor::OpenRouter);
    pub const OLLAMA: Provider = Provider::OpenAi(Flavor::Ollama);
    pub const LMSTUDIO: Provider = Provider::OpenAi(Flavor::LmStudio);

    /// Parse a provider id string (canonical ids plus common aliases).
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "claude" | "anthropic" => Some(Provider::Claude),
            "gemini" | "google" => Some(Provider::Gemini),
            "openai" | "chatgpt" | "gpt" => Some(Provider::OPENAI),
            "openrouter" => Some(Provider::OPENROUTER),
            "ollama" => Some(Provider::OLLAMA),
            "lmstudio" | "lm-studio" | "lm_studio" => Some(Provider::LMSTUDIO),
            _ => None,
        }
    }

    /// The canonical id string.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Gemini => "gemini",
            Provider::OpenAi(f) => f.id(),
        }
    }

    /// A human-readable label for UI.
    pub fn label(self) -> &'static str {
        match self {
            Provider::Claude => "Claude (Anthropic)",
            Provider::Gemini => "Gemini (Google)",
            Provider::OpenAi(f) => f.label(),
        }
    }

    /// The provider's default model id.
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Claude => crate::claude::DEFAULT_MODEL,
            Provider::Gemini => crate::gemini::DEFAULT_MODEL,
            Provider::OpenAi(f) => f.default_model(),
        }
    }

    /// Whether this provider needs an API key (local backends do not).
    pub fn requires_key(self) -> bool {
        match self {
            Provider::Claude | Provider::Gemini => true,
            Provider::OpenAi(f) => f.requires_key(),
        }
    }

    /// The environment variable a UI should mention for this provider's key.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Claude => crate::claude::key::ENV_VAR,
            Provider::Gemini => crate::gemini::key::ENV_VAR,
            Provider::OpenAi(f) => f.env_var(),
        }
    }

    /// Resolve this provider's API key via its own priority chain
    /// (explicit > provider env var(s) > provider keychain entry). Returns
    /// `None` when no key is found — acceptable for local backends.
    pub fn resolve_key(self, explicit: Option<&str>) -> Option<(String, KeySource)> {
        match self {
            Provider::Claude => crate::claude::key::resolve_key(explicit),
            Provider::Gemini => crate::gemini::key::resolve_key(explicit),
            Provider::OpenAi(f) => crate::openai::key::resolve_key(f, explicit),
        }
    }

    /// Store an API key for this provider in the OS keychain.
    pub fn store_key(self, key: &str) -> Result<(), String> {
        match self {
            Provider::Claude => crate::claude::key::store_key(key),
            Provider::Gemini => crate::gemini::key::store_key(key),
            Provider::OpenAi(f) => crate::openai::key::store_key(f, key),
        }
    }

    /// Clear this provider's stored key from the OS keychain.
    pub fn clear_key(self) -> Result<(), String> {
        match self {
            Provider::Claude => crate::claude::key::clear_key(),
            Provider::Gemini => crate::gemini::key::clear_key(),
            Provider::OpenAi(f) => crate::openai::key::clear_key(f),
        }
    }

    /// All providers, in menu order.
    pub const ALL: &'static [Provider] = &[
        Provider::Claude,
        Provider::Gemini,
        Provider::OPENAI,
        Provider::OPENROUTER,
        Provider::OLLAMA,
        Provider::LMSTUDIO,
    ];
}
