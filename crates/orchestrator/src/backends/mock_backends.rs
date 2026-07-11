//! `mock_planner`: a [`ModelBackend`] that replays a scripted sequence of
//! [`BackendEvent`]s, ignoring the request content. Contract hard rule #4:
//! "Mock backends for CI live behind the same trait." See `grounder.rs` for
//! `mock_grounder`.

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::FutureExt;

use super::probe::now_rfc3339;
use super::{BackendError, BackendEvent, BackendProfile, CompletionRequest, ModelBackend, Usage};

/// Replays a fixed, caller-supplied script of [`BackendEvent`]s on every
/// `complete` call, regardless of what the request contains. Used by tests
/// and other lanes that need a scripted plan (or a scripted failure)
/// without a real model or network.
pub struct MockPlannerBackend {
    id: String,
    script: Vec<BackendEvent>,
    profile: BackendProfile,
}

impl MockPlannerBackend {
    /// Build a scripted backend. If `script` does not already end in a
    /// terminal event (`Done`/`Error`), `complete` appends
    /// `Done { usage: 0/0 }` so callers can always rely on the stream
    /// ending in a terminal event, matching every real backend.
    pub fn new(id: impl Into<String>, script: Vec<BackendEvent>) -> Self {
        let id = id.into();
        let tool_use = script
            .iter()
            .any(|e| matches!(e, BackendEvent::ToolCall { .. }));
        Self {
            profile: BackendProfile {
                backend_id: id.clone(),
                vision: false,
                tool_use,
                context_length: super::probe::DEFAULT_CONTEXT_LENGTH,
                streaming: true,
                probed_at: now_rfc3339(),
            },
            id,
            script,
        }
    }
}

impl ModelBackend for MockPlannerBackend {
    fn complete(&self, _request: CompletionRequest) -> BoxStream<'static, BackendEvent> {
        let mut events = self.script.clone();
        if !events.iter().any(BackendEvent::is_terminal) {
            events.push(BackendEvent::Done {
                usage: Usage::default(),
            });
        }
        stream::iter(events).boxed()
    }

    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>> {
        let profile = self.profile.clone();
        async move { Ok(profile) }.boxed()
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::backends::types::RequestRole;

    #[tokio::test]
    async fn replays_the_script_verbatim_when_it_is_already_terminal() {
        let script = vec![
            BackendEvent::TextDelta {
                text: "click ".to_string(),
            },
            BackendEvent::TextDelta {
                text: "save".to_string(),
            },
            BackendEvent::Done {
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 2,
                },
            },
        ];
        let backend = MockPlannerBackend::new("mock_planner", script.clone());
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "anything", 8))
            .collect()
            .await;
        assert_eq!(events, script);
    }

    #[tokio::test]
    async fn appends_a_done_event_when_the_script_forgets_to_terminate() {
        let backend = MockPlannerBackend::new(
            "mock_planner",
            vec![BackendEvent::TextDelta {
                text: "hi".to_string(),
            }],
        );
        let events: Vec<BackendEvent> = backend
            .complete(CompletionRequest::text(RequestRole::Planner, "anything", 8))
            .collect()
            .await;
        assert!(events.last().unwrap().is_terminal());
    }

    #[tokio::test]
    async fn ignores_request_content_and_probe_reports_tool_use_when_scripted() {
        let backend = MockPlannerBackend::new(
            "mock_planner",
            vec![BackendEvent::ToolCall {
                id: "1".to_string(),
                name: "click".to_string(),
                arguments: serde_json::json!({}),
            }],
        );
        let profile = backend.probe().await.unwrap();
        assert_eq!(profile.backend_id, "mock_planner");
        assert!(profile.tool_use);
        assert!(!profile.vision);
        assert_eq!(backend.id(), "mock_planner");
    }
}
