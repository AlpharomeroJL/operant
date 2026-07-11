//! The one OpenAI-compatible-ish HTTP client, parameterized by
//! [`super::quirks::ProviderQuirks`]. Every provider in the quirk table
//! rides this same struct; adding a provider is a data edit in
//! `quirks.rs`, never a new client implementation.

use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::FutureExt;

use super::dialect;
use super::error::{BackendError, TransportError};
use super::probe::{context_length_for, now_rfc3339};
use super::quirks::{self, ProviderQuirks, StreamingFormat};
use super::redact::redact;
use super::sse::{LineBuffer, SseAssembler};
use super::transport::HttpTransport;
use super::types::{
    BackendEvent, BackendProfile, CompletionRequest, ContentPart, Message, MessageRole,
    RequestRole, ToolSchema,
};
use super::ModelBackend;

/// Per-instance configuration: which provider, which model, and how to
/// authenticate. Everything provider-shaped lives in [`ProviderQuirks`];
/// this is everything instance-shaped.
#[derive(Clone)]
pub struct BackendConfig {
    pub provider_id: String,
    pub model: String,
    pub base_url_override: Option<String>,
    pub api_key: Option<String>,
    /// Distinguishes multiple `generic` backends in `id()` (e.g.
    /// `openai_compat:custom`). Ignored for known providers.
    pub label: Option<String>,
}

/// Manual, not derived: a derived `Debug` would print `api_key` in full,
/// which is exactly what the secrets redaction hard rule
/// (`contracts/model_backend.md`) forbids from ever landing in a log line.
impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendConfig")
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("base_url_override", &self.base_url_override)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("label", &self.label)
            .finish()
    }
}

impl BackendConfig {
    pub fn new(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: model.into(),
            base_url_override: None,
            api_key: None,
            label: None,
        }
    }

    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url_override = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A well-formed, fully-transparent 1x1 PNG. Used only by the capability
/// probe's vision check; never a real screenshot, just enough bytes to see
/// whether a provider accepts an image content part at all.
const ONE_PX_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

/// The one client every quirk-table entry rides.
pub struct HttpBackend {
    id: String,
    config: BackendConfig,
    quirks: ProviderQuirks,
    transport: Arc<dyn HttpTransport>,
}

/// Manual: `transport` is a `dyn HttpTransport`, which does not (and need
/// not) implement `Debug`. `config`'s own `Debug` already redacts the API
/// key.
impl std::fmt::Debug for HttpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpBackend")
            .field("id", &self.id)
            .field("config", &self.config)
            .field("quirks", &self.quirks)
            .finish_non_exhaustive()
    }
}

impl HttpBackend {
    pub fn new(
        config: BackendConfig,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<Self, BackendError> {
        let quirks = quirks::find(&config.provider_id).cloned().ok_or_else(|| {
            BackendError::config(format!("unknown provider id `{}`", config.provider_id))
        })?;

        let effective_base = config
            .base_url_override
            .clone()
            .unwrap_or_else(|| quirks.base_url.clone());
        if effective_base.is_empty() {
            return Err(BackendError::config(format!(
                "provider `{}` has no default base_url; set BackendConfig::with_base_url",
                quirks.id
            )));
        }

        let id = if quirks.id == "generic" {
            format!(
                "openai_compat:{}",
                config.label.clone().unwrap_or_else(|| "custom".to_string())
            )
        } else {
            quirks.id.clone()
        };

        Ok(Self {
            id,
            config,
            quirks,
            transport,
        })
    }

    fn base_url(&self) -> String {
        self.config
            .base_url_override
            .clone()
            .unwrap_or_else(|| self.quirks.base_url.clone())
    }
}

impl ModelBackend for HttpBackend {
    fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent> {
        let transport = self.transport.clone();
        let quirks = self.quirks.clone();
        let config = self.config.clone();
        let base_url = self.base_url();
        let fut = async move {
            match run_completion(transport.as_ref(), &quirks, &config, &base_url, &request).await {
                Ok(events) => events,
                Err(e) => vec![BackendEvent::Error {
                    error_id: e.error_id,
                    message: e.message,
                    retryable: e.retryable,
                }],
            }
        };
        stream::once(fut).flat_map(stream::iter).boxed()
    }

    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>> {
        let transport = self.transport.clone();
        let quirks = self.quirks.clone();
        let config = self.config.clone();
        let base_url = self.base_url();
        let id = self.id.clone();
        async move {
            // 1. Text round-trip: a basic failure here fails the whole probe.
            let text_req = CompletionRequest::text(RequestRole::Planner, "ping", 8);
            let text_events =
                run_completion(transport.as_ref(), &quirks, &config, &base_url, &text_req).await?;
            if let Some(err) = first_error(&text_events) {
                return Err(err);
            }

            // 2. Vision: a 1x1 PNG content part either works or it does not.
            let vision_req = CompletionRequest {
                role: RequestRole::Grounder,
                messages: vec![Message {
                    role: MessageRole::User,
                    content: vec![
                        ContentPart::Text {
                            text: "what is in this image?".to_string(),
                        },
                        ContentPart::ImagePngB64 {
                            data: ONE_PX_PNG_B64.to_string(),
                        },
                    ],
                }],
                tools: Vec::new(),
                max_tokens: 8,
                temperature: 0.0,
            };
            let vision =
                run_completion(transport.as_ref(), &quirks, &config, &base_url, &vision_req)
                    .await
                    .map(|events| first_error(&events).is_none())
                    .unwrap_or(false);

            // 3. Tool use: a trivial no-arg tool schema either gets called or not.
            let tool_req = CompletionRequest {
                role: RequestRole::Planner,
                messages: vec![Message {
                    role: MessageRole::User,
                    content: vec![ContentPart::Text {
                        text: "call the ping tool".to_string(),
                    }],
                }],
                tools: vec![ToolSchema {
                    name: "ping".to_string(),
                    description: "no-op capability probe tool".to_string(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                }],
                max_tokens: 32,
                temperature: 0.0,
            };
            let tool_use =
                run_completion(transport.as_ref(), &quirks, &config, &base_url, &tool_req)
                    .await
                    .map(|events| {
                        events
                            .iter()
                            .any(|e| matches!(e, BackendEvent::ToolCall { .. }))
                    })
                    .unwrap_or(false);

            Ok(BackendProfile {
                backend_id: id,
                vision,
                tool_use,
                context_length: context_length_for(&config.model),
                streaming: true,
                probed_at: now_rfc3339(),
            })
        }
        .boxed()
    }

    fn id(&self) -> &str {
        &self.id
    }
}

fn first_error(events: &[BackendEvent]) -> Option<BackendError> {
    events.iter().find_map(|e| match e {
        BackendEvent::Error {
            error_id,
            message,
            retryable,
        } => Some(BackendError::new(
            error_id.clone(),
            message.clone(),
            *retryable,
        )),
        _ => None,
    })
}

/// Send one [`CompletionRequest`], collect the full (mocked or real)
/// response, and parse it into the sequence of [`BackendEvent`]s it
/// represents. Always returns a stream that ends in exactly one terminal
/// event (`Done` or `Error`), synthesizing one if the transport stream
/// ended without the dialect ever signaling completion.
async fn run_completion(
    transport: &dyn HttpTransport,
    quirks: &ProviderQuirks,
    config: &BackendConfig,
    base_url: &str,
    request: &CompletionRequest,
) -> Result<Vec<BackendEvent>, BackendError> {
    let http_request = dialect::build_request(quirks, config, base_url, request)?;

    let known_key = config.api_key.as_deref().unwrap_or("");
    tracing::debug!(
        provider = %quirks.id,
        url = %redact(&http_request.url, &[known_key]),
        "sending model backend request"
    );

    let response = transport
        .send(http_request)
        .await
        .map_err(|e: TransportError| BackendError::transport(e.to_string()))?;
    let status = response.status;

    if status >= 400 {
        let raw = response
            .collect_body()
            .await
            .map_err(|e| BackendError::transport(e.to_string()))?;
        let body_text = String::from_utf8_lossy(&raw).to_string();
        let redacted = redact(&body_text, &[known_key]);
        return Ok(vec![BackendEvent::Error {
            error_id: format!("http_{status}"),
            message: format!(
                "{} returned HTTP {status}: {}",
                quirks.id,
                truncate(&redacted, 300)
            ),
            retryable: status == 429 || status >= 500,
        }]);
    }

    let mut lines = LineBuffer::new();
    let mut sse = SseAssembler::new();
    let mut state = dialect::IncrementState::default();
    let mut events = Vec::new();
    let mut body = response.body;

    'chunks: while let Some(chunk_result) = body.next().await {
        let chunk = chunk_result.map_err(|e| BackendError::transport(e.to_string()))?;
        for line in lines.push(&chunk) {
            let payload = match quirks.streaming {
                StreamingFormat::Sse => sse.push_line(&line),
                StreamingFormat::Chunked => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
            };
            let Some(payload) = payload else { continue };

            if payload.trim() == "[DONE]" {
                break 'chunks;
            }

            let mut increment = dialect::parse_increment(quirks.dialect, &payload, &mut state)?;
            events.append(&mut increment);
            if events.iter().any(BackendEvent::is_terminal) {
                break 'chunks;
            }
        }
    }

    if !events.iter().any(BackendEvent::is_terminal) {
        events.push(BackendEvent::Done { usage: state.usage });
    }

    Ok(events)
}

/// Char-boundary-safe truncation for error message snippets (a byte-index
/// slice can panic on multi-byte UTF-8 input).
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}...")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::backends::transport::MockTransport;
    use crate::backends::types::Usage;

    fn openai_backend(mock: &Arc<MockTransport>) -> HttpBackend {
        HttpBackend::new(
            BackendConfig::new("openai", "gpt-4o-mini").with_api_key("sk-test-fake"),
            mock.clone(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn complete_streams_text_deltas_then_done_for_openai_sse() {
        let mock = Arc::new(MockTransport::new());
        mock.push_body(
            [
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ]
            .concat(),
        );

        let backend = openai_backend(&mock);
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "hi", 16))
            .collect()
            .await;

        assert_eq!(
            events,
            vec![
                BackendEvent::TextDelta {
                    text: "Hel".to_string()
                },
                BackendEvent::TextDelta {
                    text: "lo".to_string()
                },
                BackendEvent::Done {
                    usage: Usage {
                        input_tokens: 3,
                        output_tokens: 2
                    }
                },
            ]
        );

        let sent = mock.last_request().unwrap();
        assert_eq!(sent.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            sent.header_value("authorization"),
            Some("Bearer sk-test-fake")
        );
    }

    #[tokio::test]
    async fn complete_survives_a_frame_split_across_transport_chunks() {
        let mock = Arc::new(MockTransport::new());
        let full = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";
        let mid = full.len() / 2;
        mock.push_chunks(vec![
            full.as_bytes()[..mid].to_vec(),
            full.as_bytes()[mid..].to_vec(),
        ]);

        let backend = openai_backend(&mock);
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "hi", 16))
            .collect()
            .await;

        assert_eq!(
            events[0],
            BackendEvent::TextDelta {
                text: "hi".to_string()
            }
        );
        assert!(events.last().unwrap().is_terminal());
    }

    #[tokio::test]
    async fn complete_maps_http_error_status_to_a_terminal_error_event() {
        let mock = Arc::new(MockTransport::new());
        mock.push_status_chunks(429, vec![b"{\"error\":\"rate limited\"}".to_vec()]);

        let backend = openai_backend(&mock);
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "hi", 16))
            .collect()
            .await;

        assert_eq!(events.len(), 1);
        match &events[0] {
            BackendEvent::Error {
                error_id,
                retryable,
                ..
            } => {
                assert_eq!(error_id, "http_429");
                assert!(retryable);
            }
            other => panic!("expected an Error event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_redacts_the_api_key_from_an_error_body() {
        let mock = Arc::new(MockTransport::new());
        mock.push_status_chunks(401, vec![b"auth failed for key sk-test-fake".to_vec()]);

        let backend = openai_backend(&mock);
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "hi", 16))
            .collect()
            .await;

        match &events[0] {
            BackendEvent::Error { message, .. } => {
                assert!(
                    !message.contains("sk-test-fake"),
                    "api key leaked into error message: {message}"
                );
            }
            other => panic!("expected an Error event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_reports_capabilities_from_three_cheap_requests() {
        let mock = Arc::new(MockTransport::new());
        // 1) text round trip
        mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"pong\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec());
        // 2) vision check succeeds
        mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"a pixel\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec());
        // 3) tool-use check returns a tool call
        mock.push_body(
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"ping\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n".to_vec(),
        );

        let backend = openai_backend(&mock);
        let profile = backend.probe().await.unwrap();

        assert_eq!(profile.backend_id, "openai");
        assert!(profile.vision);
        assert!(profile.tool_use);
        assert_eq!(profile.context_length, 128_000);
        assert!(profile.streaming);
        assert_eq!(mock.requests().len(), 3);
    }

    #[tokio::test]
    async fn probe_fails_fast_when_the_basic_text_round_trip_errors() {
        let mock = Arc::new(MockTransport::new());
        mock.push_status_chunks(500, vec![b"boom".to_vec()]);
        let backend = openai_backend(&mock);
        let err = backend.probe().await.unwrap_err();
        assert_eq!(err.error_id, "http_500");
        // Only the text round trip should have been attempted.
        assert_eq!(mock.requests().len(), 1);
    }

    #[test]
    fn new_rejects_an_unknown_provider_id() {
        let mock: Arc<dyn HttpTransport> = Arc::new(MockTransport::new());
        let err = HttpBackend::new(BackendConfig::new("not-a-provider", "m"), mock).unwrap_err();
        assert_eq!(err.error_id, "config_error");
    }

    #[test]
    fn new_requires_a_base_url_override_for_generic() {
        let mock: Arc<dyn HttpTransport> = Arc::new(MockTransport::new());
        let err = HttpBackend::new(BackendConfig::new("generic", "m"), mock).unwrap_err();
        assert_eq!(err.error_id, "config_error");
    }

    #[test]
    fn id_labels_generic_backends_as_openai_compat() {
        let mock: Arc<dyn HttpTransport> = Arc::new(MockTransport::new());
        let backend = HttpBackend::new(
            BackendConfig::new("generic", "m")
                .with_base_url("http://localhost:9009/v1")
                .with_label("homelab"),
            mock,
        )
        .unwrap();
        assert_eq!(backend.id(), "openai_compat:homelab");
    }

    #[test]
    fn id_is_the_bare_provider_id_for_known_providers() {
        let mock: Arc<dyn HttpTransport> = Arc::new(MockTransport::new());
        let backend = HttpBackend::new(
            BackendConfig::new("anthropic", "claude-sonnet-4").with_api_key("k"),
            mock,
        )
        .unwrap();
        assert_eq!(backend.id(), "anthropic");
    }
}
