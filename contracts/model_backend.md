# Contract: Model Backend

The single trait every model provider implements (C6), and the quirk-table format that parameterizes the one OpenAI-compatible client. Append-only in released versions.

## Trait

```rust
// crates/orchestrator/src/backends/mod.rs (conceptual; signatures binding, names binding)
pub trait ModelBackend: Send + Sync {
    /// Streamed completion. The ONLY entry point.
    fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent>;
    /// Cheap capability probe; called on configure, result cached as a BackendProfile.
    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>>;
    /// Stable identifier, e.g. "ollama", "anthropic", "openai_compat:custom".
    fn id(&self) -> &str;
}
```

## CompletionRequest

```json
{
  "role": "planner",
  "messages": [
    { "role": "system", "content": [{ "kind": "text", "text": "..." }] },
    { "role": "user", "content": [{ "kind": "text", "text": "..." }, { "kind": "image_png_b64", "data": "..." }] }
  ],
  "tools": [ { "name": "...", "description": "...", "input_schema": { } } ],
  "max_tokens": 1024,
  "temperature": 0.0
}
```

- `role`: `planner` or `grounder`. Routing metadata; the backend may ignore it.
- `messages[].content` is an array of parts: `text` or `image_png_b64`.
- `tools`: optional tool schemas (JSON Schema per tool input).

## BackendEvent (stream items)

| Event | Fields | Notes |
|---|---|---|
| text_delta | text | incremental text |
| tool_call | name, arguments (object), id | complete tool call |
| done | usage { input_tokens, output_tokens } | terminal |
| error | error_id, message, retryable (bool) | terminal |

## BackendProfile (probe result)

```json
{
  "backend_id": "ollama",
  "vision": true,
  "tool_use": true,
  "context_length": 32768,
  "streaming": true,
  "probed_at": "2026-07-11T00:00:00Z"
}
```

Probe protocol: one cheap request per capability (text round-trip, vision with a 1x1 PNG, tool call with a trivial schema); context length from a lookup table keyed by reported model name, conservative default 8192. Role assignment validates against the profile and explains mismatches in plain language.

## Quirk table

One OpenAI-compatible HTTP client, parameterized per provider. The table is data (a Rust const table plus a JSON copy for tests); provider-side changes are a data edit, not a code change.

| Field | Type | Values |
|---|---|---|
| id | string | ollama, llamacpp, lmstudio, vllm, generic, openai, anthropic, gemini, deepseek, minimax, kimi, qwen, groq, mistral, xai, openrouter |
| base_url | string | default endpoint; user-overridable for local/generic |
| auth | enum | bearer, x_api_key, query_param, none |
| dialect | enum | openai, anthropic, gemini |
| streaming | enum | sse, chunked |
| vision_encoding | enum | openai_image_url, anthropic_source, gemini_inline |
| max_tokens_field | string | max_tokens, max_completion_tokens, maxOutputTokens |

Anthropic and Gemini get native dialect implementations; everything else rides the OpenAI dialect. OAuth-brokered identities (ChatGPT plan, Claude plan) map to their API dialect and endpoints through this same table.

## Hard rules

1. The replay executor links against a backend-free crate. This contract exists only in explore, probe, and drift-repair paths. Enforced by the crate graph.
2. Secrets redaction middleware strips keys and tokens from every log line and error (grep-tested against seeded fakes).
3. Zero network calls to any vendor without explicit opt-in configuration.
4. Mock backends for CI live behind the same trait: `mock_planner` (scripted plans), `mock_grounder` (fixture-deterministic coordinates), and per-dialect mock servers for quirk-table contract tests.
