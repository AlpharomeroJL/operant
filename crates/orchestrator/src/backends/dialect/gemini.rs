//! Google Gemini `generateContent` dialect: `contents`/`parts`, a
//! `systemInstruction` field instead of a system message, the API key as a
//! `?key=` query parameter, and SSE streaming via `alt=sse` (each `data:`
//! payload is one `GenerateContentResponse`).

use serde_json::{json, Value};

use crate::backends::client::BackendConfig;
use crate::backends::error::BackendError;
use crate::backends::quirks::ProviderQuirks;
use crate::backends::transport::HttpRequest;
use crate::backends::types::{BackendEvent, CompletionRequest, ContentPart, MessageRole, Usage};

use super::IncrementState;

pub fn build_request(
    quirks: &ProviderQuirks,
    config: &BackendConfig,
    base_url: &str,
    request: &CompletionRequest,
) -> Result<HttpRequest, BackendError> {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();
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
                // Gemini calls the assistant turn "model".
                let role = if m.role == MessageRole::User {
                    "user"
                } else {
                    "model"
                };
                let parts: Vec<Value> = m.content.iter().map(content_part_to_json).collect();
                contents.push(json!({ "role": role, "parts": parts }));
            }
        }
    }

    let mut generation_config = json!({ "temperature": request.temperature });
    generation_config[quirks.max_tokens_field.as_str()] = json!(request.max_tokens);

    let mut body = json!({ "contents": contents, "generationConfig": generation_config });
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system_parts.join("\n\n") }] });
    }
    if !request.tools.is_empty() {
        let declarations: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "parameters": t.input_schema }))
            .collect();
        body["tools"] = json!([{ "functionDeclarations": declarations }]);
    }

    let key = config
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            BackendError::config(format!("provider `{}` requires an api_key", quirks.id))
        })?;

    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={key}",
        base_url.trim_end_matches('/'),
        config.model
    );
    let payload = serde_json::to_vec(&body).map_err(|e| BackendError::parse(e.to_string()))?;
    let http = HttpRequest::post(url, payload).header("content-type", "application/json");
    Ok(http)
}

fn content_part_to_json(part: &ContentPart) -> Value {
    match part {
        ContentPart::Text { text } => json!({ "text": text }),
        ContentPart::ImagePngB64 { data } => {
            json!({ "inline_data": { "mime_type": "image/png", "data": data } })
        }
    }
}

/// Parse one Gemini SSE `data:` payload (one `GenerateContentResponse`).
/// Gemini has no `[DONE]` sentinel; a candidate's non-empty `finishReason`
/// is the turn's own end-of-stream signal.
pub fn parse_increment(
    payload: &str,
    state: &mut IncrementState,
) -> Result<Vec<BackendEvent>, BackendError> {
    let v: Value =
        serde_json::from_str(payload).map_err(|e| BackendError::parse(format!("gemini: {e}")))?;
    let mut events = Vec::new();

    if let Some(err) = v.get("error") {
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("gemini error")
            .to_string();
        let error_id = err
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("gemini_error")
            .to_string();
        events.push(BackendEvent::Error {
            error_id,
            message,
            retryable: false,
        });
        return Ok(events);
    }

    if let Some(usage) = v.get("usageMetadata") {
        state.usage = Usage {
            input_tokens: usage
                .get("promptTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(state.usage.input_tokens),
            output_tokens: usage
                .get("candidatesTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(state.usage.output_tokens),
        };
    }

    let mut finished = false;
    for cand in v
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for (i, part) in cand
            .pointer("/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    events.push(BackendEvent::TextDelta {
                        text: text.to_string(),
                    });
                }
            }
            if let Some(call) = part.get("functionCall") {
                let name = call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let arguments = call.get("args").cloned().unwrap_or_else(|| json!({}));
                // Gemini does not assign call ids; synthesize a stable one
                // from the candidate's part position.
                events.push(BackendEvent::ToolCall {
                    id: format!("gemini-call-{i}"),
                    name,
                    arguments,
                });
            }
        }
        if cand
            .get("finishReason")
            .and_then(Value::as_str)
            .is_some_and(|r| !r.is_empty())
        {
            finished = true;
        }
    }

    if finished {
        events.push(BackendEvent::Done { usage: state.usage });
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::RequestRole;

    fn cfg() -> BackendConfig {
        BackendConfig::new("gemini", "gemini-1.5-pro").with_api_key("fake-gemini-key")
    }

    #[test]
    fn build_request_puts_key_in_query_and_max_tokens_field_in_generation_config() {
        let quirks = crate::backends::quirks::find("gemini").unwrap();
        let req = CompletionRequest::text(RequestRole::Planner, "hi", 64);
        let http = build_request(
            quirks,
            &cfg(),
            "https://generativelanguage.googleapis.com/v1beta",
            &req,
        )
        .unwrap();

        assert!(
            http.url.contains("key=fake-gemini-key"),
            "gemini auth is a query param: {}",
            http.url
        );
        assert!(http.url.contains("gemini-1.5-pro:streamGenerateContent"));
        assert_eq!(http.header_value("authorization"), None);

        let body: Value = serde_json::from_slice(&http.body).unwrap();
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
        assert!(body["generationConfig"].get("max_tokens").is_none());
    }

    #[test]
    fn parse_increment_emits_text_then_done_on_finish_reason() {
        let mut state = IncrementState::default();
        let e1 = parse_increment(
            r#"{"candidates":[{"content":{"parts":[{"text":"Hi"}],"role":"model"}}]}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            e1,
            vec![BackendEvent::TextDelta {
                text: "Hi".to_string()
            }]
        );

        let e2 = parse_increment(
            r#"{"candidates":[{"content":{"parts":[{"text":"!"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":2}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            e2,
            vec![
                BackendEvent::TextDelta {
                    text: "!".to_string()
                },
                BackendEvent::Done {
                    usage: Usage {
                        input_tokens: 5,
                        output_tokens: 2
                    }
                },
            ]
        );
    }

    #[test]
    fn parse_increment_emits_tool_call_from_function_call_part() {
        let mut state = IncrementState::default();
        let events = parse_increment(
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"click","args":{"x":1}}}]},"finishReason":"STOP"}]}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            events,
            vec![
                BackendEvent::ToolCall {
                    id: "gemini-call-0".to_string(),
                    name: "click".to_string(),
                    arguments: json!({"x": 1})
                },
                BackendEvent::Done {
                    usage: Usage::default()
                },
            ]
        );
    }
}
