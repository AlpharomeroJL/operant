//! Contract tests for the model backend layer (C6): each required
//! quirk-table dialect driven end to end through `HttpBackend` over
//! `MockTransport` (no real sockets, ever), probe capability
//! classification, and the fixture-mode deterministic grounder. Covers at
//! least Anthropic, OpenAI, Gemini, and generic, per
//! `contracts/model_backend.md`'s quirk table.
//!
//! Per-dialect request/response wire-shape detail already has focused unit
//! tests next to each dialect module; what this file proves is that the
//! *whole pipeline* (quirk table -> request -> mock transport -> streaming
//! parse -> `BackendEvent`s) works end to end for each required provider,
//! which is what "contract test" means here.

use std::sync::Arc;

use futures::StreamExt;
use operant_ir::Snapshot;
use operant_orchestrator::backends::{
    BackendConfig, BackendEvent, CompletionRequest, FixtureGrounderBackend, HttpBackend,
    HttpTransport, MockTransport, ModelBackend, RequestRole, Usage,
};

fn backend(provider: &str, model: &str, mock: &Arc<MockTransport>) -> HttpBackend {
    let mut config = BackendConfig::new(provider, model).with_api_key("seeded-fake-key-not-real");
    if provider == "generic" {
        config = config
            .with_base_url("http://localhost:9009/v1")
            .with_label("homelab");
    }
    let transport: Arc<dyn HttpTransport> = mock.clone();
    HttpBackend::new(config, transport).expect("known provider id constructs a backend")
}

fn notepad_snapshot() -> Snapshot {
    let raw = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
    serde_json::from_str(raw).expect("shared notepad fixture parses as operant_ir::Snapshot")
}

// ---- one end-to-end round trip per required quirk-table entry -------------

#[tokio::test]
async fn openai_dialect_round_trips_through_complete() {
    let mock = Arc::new(MockTransport::new());
    mock.push_body(
        concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}],",
            "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n",
        )
        .as_bytes()
        .to_vec(),
    );

    let backend = backend("openai", "gpt-4o-mini", &mock);
    let events: Vec<BackendEvent> = backend
        .complete(CompletionRequest::text(RequestRole::Planner, "hi", 8))
        .collect()
        .await;

    assert_eq!(
        events,
        vec![
            BackendEvent::TextDelta {
                text: "hi".to_string()
            },
            BackendEvent::Done {
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1
                }
            },
        ]
    );
    let sent = mock.last_request().unwrap();
    assert!(sent.url.ends_with("/chat/completions"), "url: {}", sent.url);
    assert_eq!(
        sent.header_value("authorization"),
        Some("Bearer seeded-fake-key-not-real")
    );
}

#[tokio::test]
async fn anthropic_dialect_round_trips_through_complete() {
    let mock = Arc::new(MockTransport::new());
    mock.push_body(
        concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        )
        .as_bytes()
        .to_vec(),
    );

    let backend = backend("anthropic", "claude-sonnet-4-20250514", &mock);
    let events: Vec<BackendEvent> = backend
        .complete(CompletionRequest::text(RequestRole::Planner, "hi", 8))
        .collect()
        .await;

    assert_eq!(
        events,
        vec![
            BackendEvent::TextDelta {
                text: "hi".to_string()
            },
            BackendEvent::Done {
                usage: Usage {
                    input_tokens: 0,
                    output_tokens: 1
                }
            },
        ]
    );
    let sent = mock.last_request().unwrap();
    assert!(sent.url.ends_with("/messages"), "url: {}", sent.url);
    assert_eq!(
        sent.header_value("x-api-key"),
        Some("seeded-fake-key-not-real")
    );
    assert_eq!(
        sent.header_value("authorization"),
        None,
        "anthropic never sends a bearer token"
    );
}

#[tokio::test]
async fn gemini_dialect_round_trips_through_complete() {
    let mock = Arc::new(MockTransport::new());
    mock.push_body(
        concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]},\"finishReason\":\"STOP\"}],",
            "\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1}}\n\n",
        )
        .as_bytes()
        .to_vec(),
    );

    let backend = backend("gemini", "gemini-1.5-pro", &mock);
    let events: Vec<BackendEvent> = backend
        .complete(CompletionRequest::text(RequestRole::Planner, "hi", 8))
        .collect()
        .await;

    assert_eq!(
        events,
        vec![
            BackendEvent::TextDelta {
                text: "hi".to_string()
            },
            BackendEvent::Done {
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1
                }
            },
        ]
    );
    let sent = mock.last_request().unwrap();
    assert!(
        sent.url.contains("key=seeded-fake-key-not-real"),
        "gemini auth is a query param: {}",
        sent.url
    );
    assert_eq!(sent.header_value("authorization"), None);
}

#[tokio::test]
async fn generic_provider_rides_the_openai_dialect_over_chunked_ndjson_framing() {
    let mock = Arc::new(MockTransport::new());
    // Chunked (NDJSON) framing: no "data:" prefix, one JSON object per
    // line, and the line itself split across two transport chunks to prove
    // line reassembly holds for this framing too, not just SSE.
    let line =
        "{\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n";
    let mid = line.len() / 2;
    mock.push_chunks(vec![
        line.as_bytes()[..mid].to_vec(),
        line.as_bytes()[mid..].to_vec(),
    ]);

    let backend = backend("generic", "local-model", &mock);
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
    assert!(events.last().unwrap().is_terminal());
    assert_eq!(
        backend.id(),
        "openai_compat:homelab",
        "generic backends label themselves openai_compat:<label>"
    );
}

// ---- probe capability classification ---------------------------------------

#[tokio::test]
async fn probe_classifies_full_capability_backend_as_vision_and_tool_use() {
    let mock = Arc::new(MockTransport::new());
    mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"pong\"},\"finish_reason\":\"stop\"}]}\n\n".to_vec());
    mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"a red pixel\"},\"finish_reason\":\"stop\"}]}\n\n".to_vec());
    mock.push_body(
        concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",",
            "\"function\":{\"name\":\"ping\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        )
        .as_bytes()
        .to_vec(),
    );

    let backend = backend("openai", "gpt-4o", &mock);
    let profile = backend.probe().await.unwrap();

    assert_eq!(profile.backend_id, "openai");
    assert!(profile.vision);
    assert!(profile.tool_use);
    assert_eq!(
        profile.context_length, 128_000,
        "gpt-4o is in the context-length lookup table"
    );
    assert!(profile.streaming);
    assert!(profile
        .explain_role_mismatch(RequestRole::Grounder)
        .is_none());
}

#[tokio::test]
async fn probe_classifies_text_only_backend_and_explains_the_role_mismatch_in_plain_language() {
    let mock = Arc::new(MockTransport::new());
    mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"pong\"},\"finish_reason\":\"stop\"}]}\n\n".to_vec());
    // Vision probe rejected outright by the provider.
    mock.push_status_chunks(
        400,
        vec![b"{\"error\":\"this endpoint does not accept images\"}".to_vec()],
    );
    // Tool probe: the provider answers in plain text instead of calling the tool.
    mock.push_body(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"no tools here\"},\"finish_reason\":\"stop\"}]}\n\n".to_vec());

    let backend = backend("openai", "some-old-text-model", &mock);
    let profile = backend.probe().await.unwrap();

    assert!(!profile.vision);
    assert!(!profile.tool_use);
    let reason = profile
        .explain_role_mismatch(RequestRole::Grounder)
        .expect("grounder role should be rejected");
    assert!(
        reason.contains("cannot see images"),
        "expected plain-language mismatch, got: {reason}"
    );
}

// ---- deterministic fixture-mode grounder -----------------------------------

#[tokio::test]
async fn fixture_grounder_is_deterministic_and_needs_no_network_or_gpu() {
    let snapshot = notepad_snapshot();
    let backend = FixtureGrounderBackend::new("mock_grounder", snapshot);

    let hint_request = || CompletionRequest::text(RequestRole::Grounder, "Text editor", 8);
    let first: Vec<BackendEvent> = backend.complete(hint_request()).collect().await;
    let second: Vec<BackendEvent> = backend.complete(hint_request()).collect().await;
    assert_eq!(
        first, second,
        "the same snapshot and hint must ground to the same result every time"
    );

    match &first[0] {
        BackendEvent::ToolCall {
            name, arguments, ..
        } => {
            assert_eq!(name, "ground");
            assert!(arguments
                .get("anchor_hash")
                .and_then(|v| v.as_str())
                .is_some_and(|h| !h.is_empty()));
        }
        other => panic!("expected the grounder's first event to be a ToolCall, got {other:?}"),
    }
    assert!(first.last().unwrap().is_terminal());

    let profile = backend.probe().await.unwrap();
    assert!(
        profile.vision,
        "the grounder is, definitionally, a vision-capable backend"
    );
}
