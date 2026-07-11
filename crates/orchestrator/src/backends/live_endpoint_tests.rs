//! Flagged real-endpoint integration tests (FR-M1/M2).
//!
//! Every other test in this crate talks to [`super::MockTransport`]; these
//! two are the exception on purpose, proving the client actually round-trips
//! with a real Ollama or OpenAI-compatible server, not just a canned mock
//! response shaped like one. Per `contracts/model_backend.md` hard rule #3
//! ("zero network calls to any vendor without explicit opt-in
//! configuration"), reaching a socket here requires BOTH of:
//!
//! 1. Building with the off-by-default `real-transport` feature (the only
//!    way [`super::ReqwestTransport`] exists at all; see `transport_reqwest.rs`).
//! 2. [`super::live_config::LIVE_BACKEND_ENV`] set at runtime.
//!
//! Absent either, both tests below still compile and still run (they are
//! ordinary `#[test]`/`#[tokio::test]` functions, never `#[ignore]`), they
//! just print why there is nothing to do and return, so `cargo test -p
//! operant-orchestrator` (default features, no live backend) stays green
//! with zero network I/O. This is also why the module is split in two by
//! `cfg`: under default features `ReqwestTransport` does not exist, so the
//! "real" bodies below are not just runtime-skipped, they are not even
//! compiled.
//!
//! ## Running for real, against a local Ollama
//!
//! ```text
//! ollama pull llama3.2
//! $env:OPERANT_LIVE_BACKEND = "1"
//! cargo test -p operant-orchestrator --features real-transport live_endpoint -- --nocapture
//! ```
//!
//! ## Running for real, against a hosted OpenAI-compatible provider
//!
//! ```text
//! $env:OPERANT_LIVE_BACKEND = "1"
//! $env:OPERANT_LIVE_PROVIDER = "openai"
//! $env:OPERANT_LIVE_MODEL = "gpt-4o-mini"
//! $env:OPERANT_LIVE_API_KEY = "sk-..."
//! cargo test -p operant-orchestrator --features real-transport live_endpoint -- --nocapture
//! ```

#[cfg(feature = "real-transport")]
mod live {
    use std::sync::Arc;

    use futures::StreamExt;

    use crate::backends::{
        BackendEvent, CompletionRequest, HttpBackend, HttpTransport, LiveBackendConfig,
        LiveConfigError, ModelBackend, RequestRole, ReqwestTransport,
    };

    /// Resolve the live config, or print the skip reason and hand back
    /// `None`. Shared by both tests below so the skip message and the
    /// "opted in but misconfigured" behavior (a hard `panic!`, since that
    /// case means a human asked for a live run and got their setup wrong,
    /// which should be loud) stay identical between them.
    fn resolve_or_skip(test_name: &str) -> Option<LiveBackendConfig> {
        match LiveBackendConfig::from_env() {
            Ok(live) => Some(live),
            Err(LiveConfigError::NotEnabled) => {
                println!(
                    "skipped {test_name}: set OPERANT_LIVE_BACKEND (and, for a hosted \
                     provider, OPERANT_LIVE_PROVIDER/OPERANT_LIVE_MODEL/OPERANT_LIVE_API_KEY) \
                     to run this against a real endpoint"
                );
                None
            }
            Err(e) => panic!("{test_name}: live backend opted in but misconfigured: {e}"),
        }
    }

    fn real_backend(live: LiveBackendConfig) -> HttpBackend {
        let transport: Arc<dyn HttpTransport> = Arc::new(ReqwestTransport::new());
        HttpBackend::new(live.backend_config, transport)
            .expect("a LiveBackendConfig always resolves to a valid HttpBackend")
    }

    #[tokio::test]
    async fn live_endpoint_completes_a_real_prompt() {
        let Some(live) = resolve_or_skip("live_endpoint_completes_a_real_prompt") else {
            return;
        };
        let backend = real_backend(live);

        let request = CompletionRequest::text(
            RequestRole::Planner,
            "Reply with exactly one word: pong",
            16,
        );
        let events: Vec<BackendEvent> = backend.complete(request).collect().await;

        assert!(
            !events.is_empty(),
            "live endpoint returned no events at all"
        );
        let last = events.last().expect("checked non-empty above");
        assert!(
            last.is_terminal(),
            "live endpoint stream must end in done or error, got: {events:?}"
        );
        if let BackendEvent::Error { message, .. } = last {
            panic!(
                "live endpoint ({}) returned an error event: {message}",
                backend.id()
            );
        }
        println!("live endpoint ({}) events: {events:?}", backend.id());
    }

    #[tokio::test]
    async fn live_endpoint_probe_reports_a_profile() {
        let Some(live) = resolve_or_skip("live_endpoint_probe_reports_a_profile") else {
            return;
        };
        let backend = real_backend(live);

        let profile = backend
            .probe()
            .await
            .unwrap_or_else(|e| panic!("probe against a real endpoint failed: {e}"));

        assert_eq!(profile.backend_id, backend.id());
        assert!(profile.context_length > 0);
        println!("live probe ({}) profile: {profile:?}", backend.id());
    }
}

#[cfg(not(feature = "real-transport"))]
mod live {
    #[test]
    fn live_endpoint_completes_a_real_prompt() {
        println!(
            "skipped live_endpoint_completes_a_real_prompt: built without the real-transport \
             feature; rerun with `--features real-transport` and OPERANT_LIVE_BACKEND set"
        );
    }

    #[test]
    fn live_endpoint_probe_reports_a_profile() {
        println!(
            "skipped live_endpoint_probe_reports_a_profile: built without the real-transport \
             feature; rerun with `--features real-transport` and OPERANT_LIVE_BACKEND set"
        );
    }
}
