//! Wire-level types for the Model Backend contract (`contracts/model_backend.md`).
//! Field names and casing mirror the contract's JSON shapes exactly, since
//! planners and the explore loop on either side of `ModelBackend` are built
//! by other lanes against this exact shape.

use serde::{Deserialize, Serialize};

/// Routing metadata carried on every request. The backend may ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestRole {
    Planner,
    Grounder,
}

/// One chat-style turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// One part of a message's content array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImagePngB64 { data: String },
}

/// A tool the model may call. `input_schema` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// One request into `ModelBackend::complete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub role: RequestRole,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSchema>,
    pub max_tokens: u32,
    #[serde(default)]
    pub temperature: f32,
}

impl CompletionRequest {
    /// Convenience builder for the common single-turn, text-only case.
    pub fn text(role: RequestRole, text: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            role,
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![ContentPart::Text { text: text.into() }],
            }],
            tools: Vec::new(),
            max_tokens,
            temperature: 0.0,
        }
    }

    /// Every text part across every message, joined with a space. Used by
    /// the fixture grounder, which treats the request's text as the target
    /// hint (there is no separate "target" field on the wire contract).
    pub fn concat_text(&self) -> String {
        self.messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                ContentPart::ImagePngB64 { .. } => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Token accounting reported on the terminal `done` event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// One item in the `complete()` stream. `done` and `error` are terminal:
/// nothing else follows them in a well-formed stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BackendEvent {
    TextDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    Done {
        usage: Usage,
    },
    Error {
        error_id: String,
        message: String,
        retryable: bool,
    },
}

impl BackendEvent {
    /// True for `done` and `error`, the two terminal events. Every stream
    /// `HttpBackend::complete` produces ends in exactly one of these, so
    /// callers can stop watching for a terminal event to reliably close out
    /// the turn.
    pub fn is_terminal(&self) -> bool {
        matches!(self, BackendEvent::Done { .. } | BackendEvent::Error { .. })
    }
}

/// Cached result of `ModelBackend::probe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendProfile {
    pub backend_id: String,
    pub vision: bool,
    pub tool_use: bool,
    pub context_length: u32,
    pub streaming: bool,
    pub probed_at: String,
}

impl BackendProfile {
    /// Plain-language explanation of why `role` cannot be assigned to a
    /// backend with this profile, or `None` when the assignment is valid.
    /// Mirrors `docs/specs/backends.md`'s role-assignment validation, which
    /// asks for mismatches explained "in plain language."
    pub fn explain_role_mismatch(&self, role: RequestRole) -> Option<String> {
        if role == RequestRole::Grounder && !self.vision {
            Some(
                "This model cannot see images, so it cannot find things on screen. \
                 Use it for planning, or pick a vision model."
                    .to_string(),
            )
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_request_matches_contract_shape() {
        // The exact JSON from contracts/model_backend.md's `## CompletionRequest`.
        let json = r#"{
            "role": "planner",
            "messages": [
                { "role": "system", "content": [{ "kind": "text", "text": "sys" }] },
                { "role": "user", "content": [
                    { "kind": "text", "text": "hi" },
                    { "kind": "image_png_b64", "data": "AAAA" }
                ] }
            ],
            "tools": [ { "name": "click", "description": "click a point", "input_schema": { "type": "object" } } ],
            "max_tokens": 1024,
            "temperature": 0.0
        }"#;
        let req: CompletionRequest = serde_json::from_str(json).expect("contract shape parses");
        assert_eq!(req.role, RequestRole::Planner);
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, MessageRole::System);
        assert_eq!(req.tools[0].name, "click");
        assert_eq!(req.max_tokens, 1024);

        // Round-trip.
        let back = serde_json::to_string(&req).unwrap();
        let reparsed: CompletionRequest = serde_json::from_str(&back).unwrap();
        assert_eq!(req, reparsed);
    }

    #[test]
    fn concat_text_joins_text_parts_and_skips_images() {
        let req = CompletionRequest {
            role: RequestRole::Grounder,
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentPart::Text {
                        text: "find the".to_string(),
                    },
                    ContentPart::ImagePngB64 {
                        data: "ignored".to_string(),
                    },
                    ContentPart::Text {
                        text: "save button".to_string(),
                    },
                ],
            }],
            tools: Vec::new(),
            max_tokens: 8,
            temperature: 0.0,
        };
        assert_eq!(req.concat_text(), "find the save button");
    }

    #[test]
    fn backend_event_matches_contract_table() {
        let events = vec![
            BackendEvent::TextDelta {
                text: "hi".to_string(),
            },
            BackendEvent::ToolCall {
                id: "call_1".to_string(),
                name: "click".to_string(),
                arguments: serde_json::json!({ "x": 1.0 }),
            },
            BackendEvent::Done {
                usage: Usage {
                    input_tokens: 3,
                    output_tokens: 5,
                },
            },
            BackendEvent::Error {
                error_id: "timeout".to_string(),
                message: "backend timed out".to_string(),
                retryable: true,
            },
        ];
        for e in &events {
            let json = serde_json::to_value(e).unwrap();
            assert!(
                json.get("event").is_some(),
                "every event carries its discriminant"
            );
        }
        assert!(!events[0].is_terminal());
        assert!(!events[1].is_terminal());
        assert!(events[2].is_terminal());
        assert!(events[3].is_terminal());
    }

    #[test]
    fn backend_profile_matches_contract_shape() {
        let json = r#"{
            "backend_id": "ollama",
            "vision": true,
            "tool_use": true,
            "context_length": 32768,
            "streaming": true,
            "probed_at": "2026-07-11T00:00:00Z"
        }"#;
        let profile: BackendProfile = serde_json::from_str(json).expect("contract shape parses");
        assert_eq!(profile.backend_id, "ollama");
        assert!(profile
            .explain_role_mismatch(RequestRole::Grounder)
            .is_none());
    }

    #[test]
    fn role_mismatch_explains_in_plain_language() {
        let profile = BackendProfile {
            backend_id: "text-only-model".to_string(),
            vision: false,
            tool_use: true,
            context_length: 8192,
            streaming: true,
            probed_at: "2026-07-11T00:00:00Z".to_string(),
        };
        let reason = profile
            .explain_role_mismatch(RequestRole::Grounder)
            .expect("mismatch");
        assert!(reason.contains("cannot see images"));
        assert!(profile
            .explain_role_mismatch(RequestRole::Planner)
            .is_none());
    }
}
