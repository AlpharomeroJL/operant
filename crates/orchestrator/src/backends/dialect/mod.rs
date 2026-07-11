//! Per-dialect request building and streaming-response parsing. `client.rs`
//! dispatches on [`super::quirks::Dialect`] into exactly one of these three
//! modules; nothing outside `dialect/` knows what an OpenAI `delta` or an
//! Anthropic `content_block_delta` looks like.

pub mod anthropic;
pub mod gemini;
pub mod openai;

use std::collections::BTreeMap;

use crate::backends::client::BackendConfig;
use crate::backends::error::BackendError;
use crate::backends::quirks::{Dialect, ProviderQuirks};
use crate::backends::transport::HttpRequest;
use crate::backends::types::{BackendEvent, CompletionRequest, Usage};

/// One in-flight tool call being assembled from streamed fragments. Both
/// OpenAI (`tool_calls[].function.arguments` deltas) and Anthropic
/// (`input_json_delta.partial_json`) send a tool call's arguments as a
/// string built up across several increments; this accumulates that string
/// until the block closes.
#[derive(Debug, Default, Clone)]
pub struct PendingTool {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

/// Accumulated state across one `complete()` call's increments: running
/// token usage and any tool calls still being assembled. Each dialect
/// parser owns updating this; `client.rs` just threads it through.
#[derive(Debug, Default, Clone)]
pub struct IncrementState {
    pub usage: Usage,
    pub pending_tools: BTreeMap<u32, PendingTool>,
}

/// Build the dialect-specific HTTP request for one [`CompletionRequest`].
pub fn build_request(
    quirks: &ProviderQuirks,
    config: &BackendConfig,
    base_url: &str,
    request: &CompletionRequest,
) -> Result<HttpRequest, BackendError> {
    match quirks.dialect {
        Dialect::Openai => openai::build_request(quirks, config, base_url, request),
        Dialect::Anthropic => anthropic::build_request(quirks, config, base_url, request),
        Dialect::Gemini => gemini::build_request(quirks, config, base_url, request),
    }
}

/// Parse one already-framed payload (one SSE `data:` event, or one NDJSON
/// line) into zero or more [`BackendEvent`]s, updating `state` in place.
pub fn parse_increment(
    dialect: Dialect,
    payload: &str,
    state: &mut IncrementState,
) -> Result<Vec<BackendEvent>, BackendError> {
    match dialect {
        Dialect::Openai => openai::parse_increment(payload, state),
        Dialect::Anthropic => anthropic::parse_increment(payload, state),
        Dialect::Gemini => gemini::parse_increment(payload, state),
    }
}

/// Finalize a [`PendingTool`] into a [`BackendEvent::ToolCall`], parsing its
/// accumulated arguments string as JSON (an empty accumulator becomes `{}`,
/// since a no-argument tool call is legitimate).
pub(crate) fn finish_tool_call(
    dialect_name: &str,
    tool: PendingTool,
) -> Result<BackendEvent, BackendError> {
    let arguments = if tool.arguments_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&tool.arguments_json).map_err(|e| {
            BackendError::parse(format!(
                "{dialect_name}: tool call `{}` arguments: {e}",
                tool.name
            ))
        })?
    };
    Ok(BackendEvent::ToolCall {
        id: tool.id,
        name: tool.name,
        arguments,
    })
}
