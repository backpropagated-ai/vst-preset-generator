// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// API-key resolution for the OpenAI-compatible backends. Each [`Flavor`] keys
// off its own environment variable and its own OS-keychain entry, so keys for
// different backends never collide (all under the shared `deepsynth-preset`
// service). Local backends (Ollama, LM Studio) have no meaningful key; their
// resolution simply reports "no key needed".
//
// Priority: explicit key > env var(s) > OS keychain (same chain as the other
// providers). Also supports a per-flavor `*_BASE_URL` env var so a UI/CLI can
// point at a non-default host without a code change.

use crate::claude::key::KeySource;
use crate::openai::Flavor;

/// The shared keychain service name.
pub const KEYRING_SERVICE: &str = "deepsynth-preset";

impl Flavor {
    /// The keychain account/user name under the service for this backend.
    pub fn keychain_user(self) -> &'static str {
        match self {
            Flavor::OpenAi => "openai",
            Flavor::OpenRouter => "openrouter",
            Flavor::Ollama => "ollama",
            Flavor::LmStudio => "lmstudio",
        }
    }

    /// The environment variable(s) checked for this backend's key, in order.
    pub fn env_vars(self) -> &'static [&'static str] {
        match self {
            Flavor::OpenAi => &["OPENAI_API_KEY"],
            Flavor::OpenRouter => &["OPENROUTER_API_KEY"],
            // Local servers usually need no key, but respect one if set (some
            // users put an auth proxy in front).
            Flavor::Ollama => &["OLLAMA_API_KEY"],
            Flavor::LmStudio => &["LMSTUDIO_API_KEY"],
        }
    }

    /// The primary environment variable a UI should mention.
    pub fn env_var(self) -> &'static str {
        self.env_vars().first().copied().unwrap_or("")
    }

    /// The environment variable that overrides this backend's base URL.
    pub fn base_url_env_var(self) -> &'static str {
        match self {
            Flavor::OpenAi => "OPENAI_BASE_URL",
            Flavor::OpenRouter => "OPENROUTER_BASE_URL",
            Flavor::Ollama => "OLLAMA_BASE_URL",
            Flavor::LmStudio => "LMSTUDIO_BASE_URL",
        }
    }

    /// Resolve the base URL: the `*_BASE_URL` env var if set, else the default.
    pub fn resolve_base_url(self) -> String {
        std::env::var(self.base_url_env_var())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.default_base_url().to_string())
    }
}

/// Resolve an API key for `flavor` using the priority chain. Returns `None`
/// when no key is found (which is fine for local backends — see
/// [`Flavor::requires_key`]).
pub fn resolve_key(flavor: Flavor, explicit: Option<&str>) -> Option<(String, KeySource)> {
    if let Some(k) = explicit {
        let k = k.trim();
        if !k.is_empty() {
            return Some((k.to_string(), KeySource::Explicit));
        }
    }
    for var in flavor.env_vars() {
        if let Ok(k) = std::env::var(var) {
            let k = k.trim().to_string();
            if !k.is_empty() {
                return Some((k, KeySource::Env));
            }
        }
    }
    if let Some(k) = keychain_get(flavor) {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some((k, KeySource::Keychain));
        }
    }
    None
}

/// Read the backend's key from the OS keychain, or `None` if absent/unavailable.
pub fn keychain_get(flavor: Flavor) -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, flavor.keychain_user()).ok()?;
    entry.get_password().ok()
}

/// Persist a key for `flavor` into the OS keychain.
pub fn store_key(flavor: Flavor, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, flavor.keychain_user())
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("could not store key: {e}"))
}

/// Remove the backend's stored key from the OS keychain.
pub fn clear_key(flavor: Flavor) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, flavor.keychain_user())
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not clear key: {e}")),
    }
}
