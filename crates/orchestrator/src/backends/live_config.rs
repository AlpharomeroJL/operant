//! Environment-driven configuration for a real, non-mock [`super::ModelBackend`].
//!
//! Every test in this crate's default suite talks to [`super::MockTransport`]
//! (see `client.rs`, `transport.rs`); nothing opens a socket. This module is
//! the seam that lets one opted-in caller, a human running the flagged tests
//! in `live_endpoint_tests` (test-only; see that module) against a real
//! Ollama or OpenAI-compatible endpoint, resolve a [`super::client::BackendConfig`]
//! from the process environment instead of a hand-written literal. It is
//! ordinary production code, not test scaffolding: any future caller that
//! wants "configure a live backend from the environment" (a CLI verb, an
//! `operant doctor` reachability check) can use it the same way.
//!
//! Reading these variables never happens unless a caller explicitly asks
//! (calls [`LiveBackendConfig::from_env`] or [`LiveBackendConfig::from_lookup`]),
//! and even then [`LIVE_BACKEND_ENV`] absent is a clean, non-panicking
//! [`LiveConfigError::NotEnabled`]. That is what keeps this consistent with
//! `contracts/model_backend.md` hard rule #3, "zero network calls to any
//! vendor without explicit opt-in configuration": building or testing this
//! crate normally never constructs a live config, let alone sends a request.

use std::env;

use thiserror::Error;

use super::client::BackendConfig;
use super::quirks::{self, ProviderQuirks};

/// Opt-in gate. Its mere presence (any value, including an empty string)
/// enables live-backend resolution; [`LiveBackendConfig::from_env`] and
/// [`LiveBackendConfig::from_lookup`] still validate the resolved provider
/// before returning `Ok`, so "enabled" and "well-configured" are checked
/// separately.
pub const LIVE_BACKEND_ENV: &str = "OPERANT_LIVE_BACKEND";
/// Provider id from the quirk table (e.g. `"ollama"`, `"openai"`). Defaults
/// to `"ollama"`: the cheapest real endpoint to stand up locally, and the
/// only quirk-table entry with both `auth: none` and a working default
/// `base_url`, so a flagged test run needs nothing but Ollama installed and
/// this one variable set.
pub const PROVIDER_ENV: &str = "OPERANT_LIVE_PROVIDER";
/// Model name to request. Defaulted for providers with an obvious local
/// default; required (via [`LiveConfigError::MissingModel`]) for every
/// hosted provider, since guessing a hosted model name tends to silently
/// pin a stale or wrong one rather than fail loudly.
pub const MODEL_ENV: &str = "OPERANT_LIVE_MODEL";
/// Overrides the resolved provider's default `base_url`. Required for
/// providers with no default (`generic`).
pub const BASE_URL_ENV: &str = "OPERANT_LIVE_BASE_URL";
/// API key, forwarded to [`BackendConfig::with_api_key`]. Optional: local
/// providers (Ollama, llama.cpp server, LM Studio, vLLM) need none.
pub const API_KEY_ENV: &str = "OPERANT_LIVE_API_KEY";

const DEFAULT_PROVIDER: &str = "ollama";

/// Why [`LiveBackendConfig::from_env`] / [`LiveBackendConfig::from_lookup`]
/// declined to build a config. Always a reason a flagged test can print and
/// skip on (or, once opted in, a reason a misconfiguration should fail
/// loudly on) rather than a panic.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LiveConfigError {
    /// [`LIVE_BACKEND_ENV`] was not set: live backend tests are opt-in and
    /// this run did not opt in. The one variant every flagged test treats
    /// as "skip cleanly", never as a failure.
    #[error("live backend tests are opt-in; set {LIVE_BACKEND_ENV} to enable")]
    NotEnabled,
    /// [`PROVIDER_ENV`] named a provider id absent from the quirk table.
    #[error(
        "unknown provider id `{0}` in {PROVIDER_ENV}; see backends::provider_quirks() for valid ids"
    )]
    UnknownProvider(String),
    /// The resolved provider has no default `base_url` and
    /// [`BASE_URL_ENV`] was not set either.
    #[error("provider `{0}` has no default base_url; set {BASE_URL_ENV}")]
    MissingBaseUrl(String),
    /// The resolved provider has no obvious default model and
    /// [`MODEL_ENV`] was not set either.
    #[error("provider `{0}` has no default model; set {MODEL_ENV}")]
    MissingModel(String),
}

/// A [`BackendConfig`] resolved from the environment, paired with the
/// [`ProviderQuirks`] row it resolved against (handed back so a caller, or
/// a test's assertions, can inspect auth shape/dialect without a second
/// `quirks::find` lookup).
#[derive(Debug, Clone)]
pub struct LiveBackendConfig {
    pub backend_config: BackendConfig,
    pub quirks: ProviderQuirks,
}

impl LiveBackendConfig {
    /// Resolve a live backend configuration from the real process
    /// environment. See [`Self::from_lookup`] for the exact resolution
    /// order; this is a thin wrapper around it reading
    /// [`std::env::var`].
    ///
    /// # Examples
    ///
    /// ```
    /// use operant_orchestrator::backends::{LiveBackendConfig, LiveConfigError};
    ///
    /// // Reads OPERANT_LIVE_BACKEND and friends from the process
    /// // environment. None of them are set here, so this always reports
    /// // NotEnabled: the same skip path every flagged real-endpoint test
    /// // in this crate relies on. Never panics regardless of what is (or
    /// // is not) set in the surrounding environment.
    /// match LiveBackendConfig::from_env() {
    ///     Ok(live) => println!("configured: {}", live.backend_config.provider_id),
    ///     Err(LiveConfigError::NotEnabled) => println!("no live backend configured"),
    ///     Err(e) => eprintln!("misconfigured: {e}"),
    /// }
    /// ```
    pub fn from_env() -> Result<Self, LiveConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Resolve a live backend configuration from an arbitrary key lookup
    /// function rather than the real process environment. `from_env` is a
    /// thin wrapper around this; tests call this directly with a `HashMap`
    /// or closure instead of `std::env::set_var`, which is both easier to
    /// make deterministic and immune to the well-known hazard of two
    /// `#[test]` functions racing on process-global environment state when
    /// the default test harness runs them on separate threads.
    ///
    /// Resolution order:
    ///
    /// 1. [`LIVE_BACKEND_ENV`] absent -> [`LiveConfigError::NotEnabled`].
    /// 2. [`PROVIDER_ENV`] (default `"ollama"`) looked up in
    ///    [`quirks::provider_quirks`]; unknown id ->
    ///    [`LiveConfigError::UnknownProvider`].
    /// 3. [`BASE_URL_ENV`] overrides the provider's default `base_url`;
    ///    required when the provider (e.g. `generic`) has none ->
    ///    [`LiveConfigError::MissingBaseUrl`] otherwise.
    /// 4. [`MODEL_ENV`], defaulted only for providers with an obvious local
    ///    default (`ollama`, `llamacpp`, `lmstudio`, `vllm`, `generic`);
    ///    every hosted provider requires it explicitly ->
    ///    [`LiveConfigError::MissingModel`] otherwise.
    /// 5. [`API_KEY_ENV`], optional (local providers need none).
    ///
    /// # Examples
    ///
    /// ```
    /// use operant_orchestrator::backends::{LiveBackendConfig, LiveConfigError};
    ///
    /// // No live backend configured: the exact skip path every flagged
    /// // real-endpoint test in this crate relies on.
    /// let result = LiveBackendConfig::from_lookup(|_key| None);
    /// assert_eq!(result.unwrap_err(), LiveConfigError::NotEnabled);
    /// ```
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, LiveConfigError> {
        if lookup(LIVE_BACKEND_ENV).is_none() {
            return Err(LiveConfigError::NotEnabled);
        }

        let provider_id = lookup(PROVIDER_ENV).unwrap_or_else(|| DEFAULT_PROVIDER.to_string());
        let quirks = quirks::find(&provider_id)
            .cloned()
            .ok_or_else(|| LiveConfigError::UnknownProvider(provider_id.clone()))?;

        let model = match lookup(MODEL_ENV) {
            Some(m) => m,
            None => default_model_for(&provider_id)
                .ok_or_else(|| LiveConfigError::MissingModel(provider_id.clone()))?
                .to_string(),
        };

        let mut backend_config = BackendConfig::new(provider_id.clone(), model);

        match lookup(BASE_URL_ENV) {
            Some(base_url) => backend_config = backend_config.with_base_url(base_url),
            None if quirks.base_url.is_empty() => {
                return Err(LiveConfigError::MissingBaseUrl(provider_id));
            }
            None => {}
        }

        if let Some(key) = lookup(API_KEY_ENV) {
            backend_config = backend_config.with_api_key(key);
        }

        Ok(Self {
            backend_config,
            quirks,
        })
    }
}

/// Obvious local-server default model per provider id, for providers where
/// guessing is reasonable (a fresh local install of any of these commonly
/// ships or recommends this exact tag). `None` for anything else: a hosted
/// provider's model catalog changes on its own schedule, so
/// [`LiveBackendConfig::from_lookup`] requires [`MODEL_ENV`] explicitly
/// rather than silently pinning a guess that may not exist.
fn default_model_for(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "ollama" => Some("llama3.2"),
        "llamacpp" | "lmstudio" | "vllm" | "generic" => Some("local-model"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup_from<'a>(map: &'a HashMap<&'a str, &'a str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| map.get(key).map(|v| v.to_string())
    }

    #[test]
    fn not_enabled_when_the_gate_variable_is_absent() {
        let map = HashMap::new();
        let err = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap_err();
        assert_eq!(err, LiveConfigError::NotEnabled);
    }

    #[test]
    fn enabled_with_no_overrides_defaults_to_ollama_with_its_table_base_url() {
        let map = HashMap::from([(LIVE_BACKEND_ENV, "1")]);
        let live = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap();
        assert_eq!(live.backend_config.provider_id, "ollama");
        assert_eq!(live.backend_config.model, "llama3.2");
        assert_eq!(live.quirks.id, "ollama");
        assert_eq!(live.backend_config.base_url_override, None);
        assert!(
            !live.quirks.base_url.is_empty(),
            "ollama must have a working default base_url with no override"
        );
        assert_eq!(live.backend_config.api_key, None);
    }

    #[test]
    fn unknown_provider_id_is_a_named_error() {
        let map = HashMap::from([(LIVE_BACKEND_ENV, "1"), (PROVIDER_ENV, "not-a-provider")]);
        let err = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap_err();
        assert_eq!(
            err,
            LiveConfigError::UnknownProvider("not-a-provider".to_string())
        );
    }

    #[test]
    fn generic_provider_without_a_base_url_override_is_a_named_error() {
        let map = HashMap::from([(LIVE_BACKEND_ENV, "1"), (PROVIDER_ENV, "generic")]);
        let err = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap_err();
        assert_eq!(err, LiveConfigError::MissingBaseUrl("generic".to_string()));
    }

    #[test]
    fn generic_provider_with_a_base_url_override_resolves() {
        let map = HashMap::from([
            (LIVE_BACKEND_ENV, "1"),
            (PROVIDER_ENV, "generic"),
            (BASE_URL_ENV, "http://localhost:9009/v1"),
        ]);
        let live = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap();
        assert_eq!(
            live.backend_config.base_url_override.as_deref(),
            Some("http://localhost:9009/v1")
        );
        assert_eq!(live.backend_config.model, "local-model");
    }

    #[test]
    fn base_url_override_wins_even_when_the_provider_already_has_a_default() {
        let map = HashMap::from([
            (LIVE_BACKEND_ENV, "1"),
            (PROVIDER_ENV, "ollama"),
            (BASE_URL_ENV, "http://192.168.1.50:11434/v1"),
        ]);
        let live = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap();
        assert_eq!(
            live.backend_config.base_url_override.as_deref(),
            Some("http://192.168.1.50:11434/v1")
        );
    }

    #[test]
    fn hosted_provider_without_a_model_is_a_named_error() {
        let map = HashMap::from([(LIVE_BACKEND_ENV, "1"), (PROVIDER_ENV, "openai")]);
        let err = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap_err();
        assert_eq!(err, LiveConfigError::MissingModel("openai".to_string()));
    }

    #[test]
    fn hosted_provider_with_model_and_key_resolves() {
        let map = HashMap::from([
            (LIVE_BACKEND_ENV, "1"),
            (PROVIDER_ENV, "openai"),
            (MODEL_ENV, "gpt-4o-mini"),
            (API_KEY_ENV, "sk-test-fake"),
        ]);
        let live = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap();
        assert_eq!(live.backend_config.model, "gpt-4o-mini");
        assert_eq!(live.backend_config.api_key.as_deref(), Some("sk-test-fake"));
        assert_eq!(live.backend_config.base_url_override, None);
        assert!(!live.quirks.base_url.is_empty());
    }

    #[test]
    fn the_gate_variable_enables_even_when_its_value_is_an_empty_string() {
        let map = HashMap::from([(LIVE_BACKEND_ENV, "")]);
        let live = LiveBackendConfig::from_lookup(lookup_from(&map)).unwrap();
        assert_eq!(live.backend_config.provider_id, "ollama");
    }
}
