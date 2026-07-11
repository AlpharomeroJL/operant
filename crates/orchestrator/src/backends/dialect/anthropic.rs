//! Anthropic Messages API dialect: `system` as a top-level field (not a
//! message), `x-api-key` auth, and named SSE events
//! (`content_block_delta`'s `text_delta` / `input_json_delta`,
//! `message_delta` for usage, `message_stop` to end the turn).

use serde_json::{json, Value};

use crate::backends::client::BackendConfig;
use crate::backends::error::BackendError;
use crate::backends::quirks::ProviderQuirks;
use crate::backends::transport::HttpRequest;
use crate::backends::types::{BackendEvent, CompletionRequest, ContentPart, MessageRole};

use super::{finish_tool_call, IncrementState};

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub fn build_request(
    quirks: &ProviderQuirks,
    config: &BackendConfig,
    base_url: &str,
    request: &CompletionRequest,
) -> Result<HttpRequest, BackendError> {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for m in &request.messages {
        match m.role {
            MessageRole::System => {
                for c in &m.content {
                    if let ContentPart::Text { text } = c {
                        system_parts.push(text.clone());
                    }
                }
            }
            MessageRole::User | MessageRole::Assistant => {
                let role = if m.role == MessageRole::User {
                    "user"
                } else {
                    "assistant"
                };
                let content: Vec<Value> = m.content.iter().map(content_part_to_json).collect();
                messages.push(json!({ "role": role, "content": content }));
            }
        }
    }

    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "stream": true,
        "temperature": request.temperature,
    });
    if !system_parts.is_empty() {
        body["system"] = json!(system_parts.join("\n\n"));
    }
    body[quirks.max_tokens_field.as_str()] = json!(request.max_tokens);

    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.input_schema }))
            .collect();
        body["tools"] = json!(tools);
    }

    let key = config
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            BackendError::config(format!("provider `{}` requires an api_key", quirks.id))
        })?;

    let url = format!("{}/messages", base_url.trim_end_matches('/'));
    let payload = serde_json::to_vec(&body).map_err(|e| BackendError::parse(e.to_string()))?;
    let http = HttpRequest::post(url, payload)
        .header("content-type", "application/json")
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("x-api-key", key);
    Ok(http)
}

fn content_part_to_json(part: &ContentPart) -> Value {
    match part {
        ContentPart::Text { text } => json!({ "type": "text", "text": text }),
        ContentPart::ImagePngB64 { data } => json!({
            "type": "image",
            "source": { "type": "base64", "media_type": "image/png", "data": data }
        }),
    }
}

/// Parse one Anthropic SSE `data:` payload. Anthropic repeats its event
/// kind inside the JSON body's `type` field, so the `event:` line itself
/// (already discarded by [`crate::backends::sse::SseAssembler`]) is
/// redundant for parsing purposes.
pub fn parse_increment(
    payload: &str,
    state: &mut IncrementState,
) -> Result<Vec<BackendEvent>, BackendError> {
    let v: Value = serde_json::from_str(payload)
        .map_err(|e| BackendError::parse(format!("anthropic: {e}")))?;
    let mut events = Vec::new();

    match v.get("type").and_then(Value::as_str).unwrap_or_default() {
        "message_start" => {
            if let Some(input_tokens) = v
                .pointer("/message/usage/input_tokens")
                .and_then(Value::as_u64)
            {
                state.usage.input_tokens = input_tokens;
            }
        }
        "content_block_start" => {
            if let (Some(idx), Some(block)) = (
                v.get("index").and_then(Value::as_u64),
                v.get("content_block"),
            ) {
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let entry = state.pending_tools.entry(idx as u32).or_default();
                    entry.id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    entry.name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
            }
        }
        "content_block_delta" => {
            if let Some(delta) = v.get("delta") {
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                events.push(BackendEvent::TextDelta {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(idx) = v.get("index").and_then(Value::as_u64) {
                            if let Some(partial) = delta.get("partial_json").and_then(Value::as_str)
                            {
                                state
                                    .pending_tools
                                    .entry(idx as u32)
                                    .or_default()
                                    .arguments_json
                                    .push_str(partial);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "content_block_stop" => {
            if let Some(idx) = v.get("index").and_then(Value::as_u64) {
                if let Some(tool) = state.pending_tools.remove(&(idx as u32)) {
                    if !tool.name.is_empty() {
                        events.push(finish_tool_call("anthropic", tool)?);
                    }
                }
            }
        }
        "message_delta" => {
            if let Some(output_tokens) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                state.usage.output_tokens = output_tokens;
            }
        }
        "message_stop" => {
            events.push(BackendEvent::Done { usage: state.usage });
        }
        "error" => {
            let message = v
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("anthropic error")
                .to_string();
            let error_id = v
                .pointer("/error/type")
                .and_then(Value::as_str)
                .unwrap_or("anthropic_error")
                .to_string();
            events.push(BackendEvent::Error {
                error_id,
                message,
                retryable: false,
            });
        }
        _ => {}
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{Message, RequestRole, Usage};

    fn cfg() -> BackendConfig {
        BackendConfig::new("anthropic", "claude-sonnet-4-20250514").with_api_key("sk-ant-fake-key")
    }

    #[test]
    fn build_request_moves_system_out_of_messages_and_sets_x_api_key() {
        let quirks = crate::backends::quirks::find("anthropic").unwrap();
        let req = CompletionRequest {
            role: RequestRole::Planner,
            messages: vec![
                Message {
                    role: MessageRole::System,
                    content: vec![ContentPart::Text {
                        text: "be terse".to_string(),
                    }],
                },
                Message {
                    role: MessageRole::User,
                    content: vec![ContentPart::Text {
                        text: "hi".to_string(),
                    }],
                },
            ],
            tools: Vec::new(),
            max_tokens: 16,
            temperature: 0.0,
        };
        let http = build_request(quirks, &cfg(), "https://api.anthropic.com/v1", &req).unwrap();

        assert_eq!(http.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(http.header_value("x-api-key"), Some("sk-ant-fake-key"));
        assert_eq!(
            http.header_value("authorization"),
            None,
            "anthropic never uses bearer auth"
        );

        let body: Value = serde_json::from_slice(&http.body).unwrap();
        assert_eq!(body["system"], "be terse");
        assert_eq!(
            body["messages"].as_array().unwrap().len(),
            1,
            "system message is not duplicated in messages"
        );
        assert_eq!(body["max_tokens"], 16);
    }

    #[test]
    fn parse_increment_handles_text_then_stop() {
        let mut state = IncrementState::default();
        let mut events = Vec::new();
        events.extend(
            parse_increment(
                r#"{"type":"message_start","message":{"usage":{"input_tokens":12}}}"#,
                &mut state,
            )
            .unwrap(),
        );
        events.extend(
            parse_increment(
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#,
                &mut state,
            )
            .unwrap(),
        );
        events.extend(
            parse_increment(r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#, &mut state)
                .unwrap(),
        );
        events.extend(parse_increment(r#"{"type":"message_stop"}"#, &mut state).unwrap());

        assert_eq!(
            events,
            vec![
                BackendEvent::TextDelta {
                    text: "Hi".to_string()
                },
                BackendEvent::Done {
                    usage: Usage {
                        input_tokens: 12,
                        output_tokens: 3
                    }
                },
            ]
        );
    }

    #[test]
    fn parse_increment_accumulates_tool_use_input_json_delta() {
        let mut state = IncrementState::default();
        parse_increment(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"click","input":{}}}"#,
            &mut state,
        )
        .unwrap();
        parse_increment(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"x\":"}}"#,
            &mut state,
        )
        .unwrap();
        parse_increment(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"7}"}}"#,
            &mut state,
        )
        .unwrap();
        let events =
            parse_increment(r#"{"type":"content_block_stop","index":1}"#, &mut state).unwrap();

        assert_eq!(
            events,
            vec![BackendEvent::ToolCall {
                id: "toolu_1".to_string(),
                name: "click".to_string(),
                arguments: json!({ "x": 7 })
            }]
        );
    }
}
