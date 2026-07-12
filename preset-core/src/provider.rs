// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Provider abstraction — which LLM backend turns a text prompt into named
// parameters. Both providers share the same `ParamSpec`-derived schema and the
// same downstream mapping/writing; they differ only in the HTTP client, the
// default model, and where the API key comes from.

use crate::claude::key::KeySource;

/// A supported text-to-preset LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Gemini,
}

impl Provider {
    /// Parse a provider id string (`"claude"` / `"gemini"`, plus common aliases).
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "claude" | "anthropic" => Some(Provider::Claude),
            "gemini" | "google" => Some(Provider::Gemini),
            _ => None,
        }
    }

    /// The canonical id string.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Gemini => "gemini",
        }
    }

    /// A human-readable label for UI.
    pub fn label(self) -> &'static str {
        match self {
            Provider::Claude => "Claude (Anthropic)",
            Provider::Gemini => "Gemini (Google)",
        }
    }

    /// The provider's default model id.
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Claude => crate::claude::DEFAULT_MODEL,
            Provider::Gemini => crate::gemini::DEFAULT_MODEL,
        }
    }

    /// The environment variable a UI should mention for this provider's key.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Claude => crate::claude::key::ENV_VAR,
            Provider::Gemini => crate::gemini::key::ENV_VAR,
        }
    }

    /// Resolve this provider's API key via its own priority chain
    /// (explicit > provider env var(s) > provider keychain entry).
    pub fn resolve_key(self, explicit: Option<&str>) -> Option<(String, KeySource)> {
        match self {
            Provider::Claude => crate::claude::key::resolve_key(explicit),
            Provider::Gemini => crate::gemini::key::resolve_key(explicit),
        }
    }

    /// Store an API key for this provider in the OS keychain.
    pub fn store_key(self, key: &str) -> Result<(), String> {
        match self {
            Provider::Claude => crate::claude::key::store_key(key),
            Provider::Gemini => crate::gemini::key::store_key(key),
        }
    }

    /// Clear this provider's stored key from the OS keychain.
    pub fn clear_key(self) -> Result<(), String> {
        match self {
            Provider::Claude => crate::claude::key::clear_key(),
            Provider::Gemini => crate::gemini::key::clear_key(),
        }
    }

    /// All providers, in menu order.
    pub const ALL: &'static [Provider] = &[Provider::Claude, Provider::Gemini];
}
