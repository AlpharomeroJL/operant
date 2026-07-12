//! Model Backend (C6): the single `ModelBackend` trait every provider
//! implements, one OpenAI-compatible HTTP client parameterized by a
//! per-provider quirk table, and the mock backends CI runs against.
//! Mirrors `contracts/model_backend.md`: the trait's method names and
//! signatures are the binding cross-lane surface; the file layout under
//! this module is not.
//!
//! - [`quirks`]: the provider quirk table (`ProviderQuirks`, `provider_quirks()`).
//! - [`transport`]: the injectable [`transport::HttpTransport`] seam and
//!   [`transport::MockTransport`]; nothing in this crate's default test
//!   suite opens a socket.
//! - [`dialect`]: per-dialect (openai/anthropic/gemini) request building
//!   and streaming-response parsing.
//! - [`client`]: [`client::HttpBackend`], the one client every quirk-table
//!   entry rides.
//! - [`probe`]: capability-probe support (context-length lookup, the
//!   `probed_at` timestamp formatter).
//! - [`grounder`]: [`grounder::FixtureGrounderBackend`], the `mock_grounder`
//!   fixture-mode vision grounder (deterministic coordinates plus a cropped
//!   anchor capture, no GPU).
//! - [`live_config`]: environment-driven [`live_config::LiveBackendConfig`]
//!   resolution for the flagged real-endpoint tests in `live_endpoint_tests`
//!   (test-only; opt-in via `OPERANT_LIVE_BACKEND`, see `docs/models.md`)
//!   and any future caller that wants a live backend without a config file.
//! - [`mock_backends`]: [`mock_backends::MockPlannerBackend`], the
//!   `mock_planner` scripted backend.
//! - [`redact`]: secrets redaction for logs and errors (hard rule #2).
//! - [`sse`]: line/frame buffering shared by the streaming dialect parsers.
//! - [`types`]: the wire types (`CompletionRequest`, `BackendEvent`, ...).
//! - [`error`]: [`error::BackendError`] and [`error::TransportError`].

mod client;
mod dialect;
mod error;
mod grounder;
mod live_config;
mod mock_backends;
mod probe;
mod quirks;
mod redact;
mod sse;
mod transport;
mod types;

#[cfg(feature = "real-transport")]
mod transport_reqwest;

// The P0b proof-harness planner: an operator answers each turn through a
// directory of request/response JSON files. Opt-in only; never in a default or
// release build (see the module docs and `Cargo.toml`'s `dev-agent-bridge`).
#[cfg(feature = "dev-agent-bridge")]
mod agent_bridge;

// Flagged real-endpoint tests (FR-M1/M2): test-only, and only meaningfully
// exercised with `--features real-transport` plus `OPERANT_LIVE_BACKEND`
// set; see the module doc for how it skips cleanly without either.
#[cfg(test)]
mod live_endpoint_tests;

use futures::future::BoxFuture;
use futures::stream::BoxStream;

pub use client::{BackendConfig, HttpBackend};
pub use error::{BackendError, TransportError};
pub use grounder::{ground_fixture, CropRegion, FixtureGrounderBackend, GroundResult};
pub use live_config::{
    LiveBackendConfig, LiveConfigError, API_KEY_ENV, BASE_URL_ENV, LIVE_BACKEND_ENV, MODEL_ENV,
    PROVIDER_ENV,
};
pub use mock_backends::MockPlannerBackend;
pub use probe::{context_length_for, now_rfc3339, DEFAULT_CONTEXT_LENGTH};
pub use quirks::{
    provider_quirks, AuthShape, Dialect, ProviderQuirks, StreamingFormat, VisionEncoding,
};
pub use redact::redact;
pub use transport::{HttpMethod, HttpRequest, HttpResponse, HttpTransport, MockTransport};
pub use types::{
    BackendEvent, BackendProfile, CompletionRequest, ContentPart, Message, MessageRole,
    RequestRole, ToolSchema, Usage,
};

#[cfg(feature = "real-transport")]
pub use transport_reqwest::ReqwestTransport;

#[cfg(feature = "dev-agent-bridge")]
pub use agent_bridge::{AgentBridgeBackend, AGENT_BRIDGE_DIR_ENV};

/// The single trait every model provider implements (C6).
pub trait ModelBackend: Send + Sync {
    /// Streamed completion. The ONLY entry point.
    fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent>;
    /// Cheap capability probe; called on configure, result cached as a BackendProfile.
    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>>;
    /// Stable identifier, e.g. "ollama", "anthropic", "openai_compat:custom".
    fn id(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    /// A trivial `ModelBackend` exercising nothing but the trait's own
    /// shape, to pin down that `ModelBackend` is dyn-compatible (object
    /// safe) and `Send + Sync`, which every caller (the explore loop,
    /// doctor, role-assignment UI) depends on being able to assume.
    struct Echo;
    impl ModelBackend for Echo {
        fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent> {
            let text = request.concat_text();
            Box::pin(futures::stream::iter(vec![
                BackendEvent::TextDelta { text },
                BackendEvent::Done {
                    usage: Usage::default(),
                },
            ]))
        }
        fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>> {
            Box::pin(async {
                Ok(BackendProfile {
                    backend_id: "echo".to_string(),
                    vision: false,
                    tool_use: false,
                    context_length: DEFAULT_CONTEXT_LENGTH,
                    streaming: true,
                    probed_at: now_rfc3339(),
                })
            })
        }
        fn id(&self) -> &str {
            "echo"
        }
    }

    #[tokio::test]
    async fn model_backend_is_object_safe_and_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Box<dyn ModelBackend>>();

        let backend: Box<dyn ModelBackend> = Box::new(Echo);
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "hi", 8))
            .collect()
            .await;
        assert_eq!(
            events[0],
            BackendEvent::TextDelta {
                text: "hi".to_string()
            }
        );
        assert_eq!(backend.id(), "echo");
        assert_eq!(backend.probe().await.unwrap().backend_id, "echo");
    }
}
