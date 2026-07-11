//! OpenAI Chat Completions dialect: request shape, SSE `delta` streaming.
//! Every provider in the quirk table except Anthropic and Gemini rides this
//! (Ollama, llama.cpp, LM Studio, vLLM, generic, OpenAI itself, DeepSeek,
//! MiniMax, Kimi, Qwen, Groq, Mistral, xAI, OpenRouter).

use serde_json::{json, Value};

use crate::backends::client::BackendConfig;
use crate::backends::error::BackendError;
use crate::backends::quirks::{AuthShape, ProviderQuirks};
use crate::backends::transport::HttpRequest;
use crate::backends::types::{
    BackendEvent, CompletionRequest, ContentPart, Message, MessageRole, Usage,
};

use super::{finish_tool_call, IncrementState};

pub fn build_request(
    quirks: &ProviderQuirks,
    config: &BackendConfig,
    base_url: &str,
    request: &CompletionRequest,
) -> Result<HttpRequest, BackendError> {
    let messages: Vec<Value> = request.messages.iter().map(message_to_json).collect();

    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "stream": true,
        "temperature": request.temperature,
    });
    body[quirks.max_tokens_field.as_str()] = json!(request.max_tokens);

    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();
        body["tools"] = json!(tools);
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let payload = serde_json::to_vec(&body).map_err(|e| BackendError::parse(e.to_string()))?;
    let request = HttpRequest::post(url, payload).header("content-type", "application/json");
    apply_auth(request, quirks, config)
}

fn message_to_json(m: &Message) -> Value {
    let role = match m.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    };
    let content: Vec<Value> = m.content.iter().map(content_part_to_json).collect();
    json!({ "role": role, "content": content })
}

fn content_part_to_json(part: &ContentPart) -> Value {
    match part {
        ContentPart::Text { text } => json!({ "type": "text", "text": text }),
        ContentPart::ImagePngB64 { data } => {
            // quirks.vision_encoding == OpenaiImageUrl for every provider
            // that reaches this function; encoded as a data: URI, which
            // every OpenAI-compatible server accepts in place of a hosted
            // image URL.
            json!({
                "type": "image_url",
                "image_url": { "url": format!("data:image/png;base64,{data}") }
            })
        }
    }
}

fn apply_auth(
    request: HttpRequest,
    quirks: &ProviderQuirks,
    config: &BackendConfig,
) -> Result<HttpRequest, BackendError> {
    match quirks.auth {
        AuthShape::None => Ok(request),
        AuthShape::Bearer => {
            let key = require_api_key(quirks, config)?;
            Ok(request.header("authorization", format!("Bearer {key}")))
        }
        // Not used by any openai-dialect provider in today's table, but
        // handled rather than left to panic in case a future provider entry
        // pairs the openai dialect with a different auth shape.
        AuthShape::XApiKey => {
            let key = require_api_key(quirks, config)?;
            Ok(request.header("x-api-key", key))
        }
        AuthShape::QueryParam => {
            let key = require_api_key(quirks, config)?;
            let sep = if request.url.contains('?') { '&' } else { '?' };
            let url = format!("{}{sep}key={key}", request.url);
            Ok(HttpRequest { url, ..request })
        }
    }
}

fn require_api_key<'a>(
    quirks: &ProviderQuirks,
    config: &'a BackendConfig,
) -> Result<&'a str, BackendError> {
    config
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            BackendError::config(format!("provider `{}` requires an api_key", quirks.id))
        })
}

/// Parse one OpenAI SSE `data:` payload (already stripped of the `data:`
/// prefix). The literal `[DONE]` terminator is handled by `client.rs`
/// before this is called.
pub fn parse_increment(
    payload: &str,
    state: &mut IncrementState,
) -> Result<Vec<BackendEvent>, BackendError> {
    let v: Value =
        serde_json::from_str(payload).map_err(|e| BackendError::parse(format!("openai: {e}")))?;
    let mut events = Vec::new();

    if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
        state.usage = Usage {
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(state.usage.input_tokens),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(state.usage.output_tokens),
        };
    }

    for choice in v
        .get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(delta) = choice.get("delta") {
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    events.push(BackendEvent::TextDelta {
                        text: text.to_string(),
                    });
                }
            }
            for tc in delta
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let idx = tc.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                let entry = state.pending_tools.entry(idx).or_default();
                if let Some(id) = tc.get("id").and_then(Value::as_str) {
                    entry.id = id.to_string();
                }
                if let Some(func) = tc.get("function") {
                    if let Some(name) = func.get("name").and_then(Value::as_str) {
                        entry.name.push_str(name);
                    }
                    if let Some(args) = func.get("arguments").and_then(Value::as_str) {
                        entry.arguments_json.push_str(args);
                    }
                }
            }
        }

        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            if !reason.is_empty() {
                for (_, tool) in std::mem::take(&mut state.pending_tools) {
                    events.push(finish_tool_call("openai", tool)?);
                }
                events.push(BackendEvent::Done { usage: state.usage });
            }
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::RequestRole;

    fn cfg() -> BackendConfig {
        BackendConfig::new("openai", "gpt-4o-mini").with_api_key("sk-test-fake-key")
    }

    #[test]
    fn build_request_uses_configured_max_tokens_field_and_bearer_auth() {
        let quirks = crate::backends::quirks::find("openai").unwrap();
        let req = CompletionRequest::text(RequestRole::Planner, "hi", 42);
        let http = build_request(quirks, &cfg(), "https://api.openai.com/v1", &req).unwrap();

        assert_eq!(http.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            http.header_value("authorization"),
            Some("Bearer sk-test-fake-key")
        );

        let body: Value = serde_json::from_slice(&http.body).unwrap();
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["max_completion_tokens"], 42);
        assert!(
            body.get("max_tokens").is_none(),
            "openai's quirk field is max_completion_tokens"
        );
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn build_request_without_api_key_is_a_config_error() {
        let quirks = crate::backends::quirks::find("openai").unwrap();
        let req = CompletionRequest::text(RequestRole::Planner, "hi", 8);
        let err = build_request(
            quirks,
            &BackendConfig::new("openai", "gpt-4o-mini"),
            "https://api.openai.com/v1",
            &req,
        )
        .unwrap_err();
        assert_eq!(err.error_id, "config_error");
    }

    #[test]
    fn parse_increment_emits_text_deltas() {
        let mut state = IncrementState::default();
        let events = parse_increment(
            r#"{"choices":[{"index":0,"delta":{"content":"Hel"},"finish_reason":null}]}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            events,
            vec![BackendEvent::TextDelta {
                text: "Hel".to_string()
            }]
        );
    }

    #[test]
    fn parse_increment_accumulates_fragmented_tool_call_arguments() {
        let mut state = IncrementState::default();
        let e1 = parse_increment(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"click","arguments":"{\"x\":"}}]},"finish_reason":null}]}"#,
            &mut state,
        )
        .unwrap();
        assert!(e1.is_empty(), "no event until the tool call closes");

        let e2 = parse_increment(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"12}"}}]},"finish_reason":null}]}"#,
            &mut state,
        )
        .unwrap();
        assert!(e2.is_empty());

        let e3 = parse_increment(
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":4}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            e3,
            vec![
                BackendEvent::ToolCall {
                    id: "call_1".to_string(),
                    name: "click".to_string(),
                    arguments: json!({ "x": 12 }),
                },
                BackendEvent::Done {
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 4
                    }
                },
            ]
        );
    }
}
