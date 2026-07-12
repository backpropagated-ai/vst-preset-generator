// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// API-key resolution, in priority order:
//   1. an explicit key (UI field / CLI flag),
//   2. the `ANTHROPIC_API_KEY` environment variable,
//   3. the OS keychain (via the `keyring` crate, service `deepsynth-preset`).
//
// The key is never logged. `store_key`/`clear_key` let the app persist a key the
// user typed into its settings panel.

/// The keychain service name used for the stored Anthropic key.
pub const KEYRING_SERVICE: &str = "deepsynth-preset";
/// The keychain account/user name under the service.
pub const KEYRING_USER: &str = "anthropic-api-key";
/// The environment variable checked before the keychain.
pub const ENV_VAR: &str = "ANTHROPIC_API_KEY";

/// Where a resolved key came from (for status display; never the key itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Explicit,
    Env,
    Keychain,
}

/// Resolve an API key using the priority chain. `explicit` is a key passed on
/// the CLI or entered in the UI (highest priority). Returns the key and where it
/// came from, or `None` if no key is available anywhere.
pub fn resolve_key(explicit: Option<&str>) -> Option<(String, KeySource)> {
    if let Some(k) = explicit {
        let k = k.trim();
        if !k.is_empty() {
            return Some((k.to_string(), KeySource::Explicit));
        }
    }
    if let Ok(k) = std::env::var(ENV_VAR) {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some((k, KeySource::Env));
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

/// Read the key from the OS keychain, or `None` if absent/unavailable.
pub fn keychain_get() -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    entry.get_password().ok()
}

/// Persist a key into the OS keychain (used by the app's settings panel).
pub fn store_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("could not store key: {e}"))
}

/// Remove the stored key from the OS keychain.
pub fn clear_key() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not clear key: {e}")),
    }
}
