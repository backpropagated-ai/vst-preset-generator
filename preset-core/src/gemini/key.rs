// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Google Gemini API-key resolution, in priority order:
//   1. an explicit key (UI field / CLI flag),
//   2. the `GEMINI_API_KEY` environment variable (or `GOOGLE_API_KEY` as a
//      fallback — the two are interchangeable for the Generative Language API),
//   3. the OS keychain (via the `keyring` crate, service `deepsynth-preset`,
//      user `gemini`).
//
// The key is never logged. `store_key`/`clear_key` let the app persist a key
// the user typed into its settings panel.
//
// This mirrors `crate::claude::key` but keys off the Gemini environment
// variables and its own keychain entry, so the two providers' stored keys never
// collide.

use crate::claude::key::KeySource;

/// The keychain service name (shared with the Anthropic key).
pub const KEYRING_SERVICE: &str = "deepsynth-preset";
/// The keychain account/user name under the service for the Gemini key.
pub const KEYRING_USER: &str = "gemini";
/// The primary environment variable checked before the keychain.
pub const ENV_VAR: &str = "GEMINI_API_KEY";
/// An accepted alternative environment variable (Google's SDK convention).
pub const ENV_VAR_ALT: &str = "GOOGLE_API_KEY";

/// Resolve a Gemini API key using the priority chain. `explicit` is a key
/// passed on the CLI or entered in the UI (highest priority). Returns the key
/// and where it came from, or `None` if no key is available anywhere.
pub fn resolve_key(explicit: Option<&str>) -> Option<(String, KeySource)> {
    if let Some(k) = explicit {
        let k = k.trim();
        if !k.is_empty() {
            return Some((k.to_string(), KeySource::Explicit));
        }
    }
    for var in [ENV_VAR, ENV_VAR_ALT] {
        if let Ok(k) = std::env::var(var) {
            let k = k.trim().to_string();
            if !k.is_empty() {
                return Some((k, KeySource::Env));
            }
        }
    }
    if let Some(k) = keychain_get() {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some((k, KeySource::Keychain));
        }
    }
    None
}

/// Read the Gemini key from the OS keychain, or `None` if absent/unavailable.
pub fn keychain_get() -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    entry.get_password().ok()
}

/// Persist a Gemini key into the OS keychain (used by the app's settings panel).
pub fn store_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("could not store key: {e}"))
}

/// Remove the stored Gemini key from the OS keychain.
pub fn clear_key() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not clear key: {e}")),
    }
}
