//! The provider quirk table (`contracts/model_backend.md`'s `## Quirk
//! table`, enumerated fully in `docs/specs/backends.md`).
//!
//! One entry per provider; `client.rs` and `dialect/` are entirely generic
//! over this data, so adding or fixing a provider is a data edit here,
//! never a new code path. Fields are owned (`String`, not `&'static str`)
//! specifically so the table round-trips through JSON: "the table is data
//! (a Rust const table plus a JSON copy for tests)" is only true if the
//! Rust type can actually deserialize that JSON copy.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    Openai,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthShape {
    Bearer,
    XApiKey,
    QueryParam,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingFormat {
    Sse,
    Chunked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionEncoding {
    OpenaiImageUrl,
    AnthropicSource,
    GeminiInline,
}

/// One provider's row in the quirk table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderQuirks {
    pub id: String,
    /// Default endpoint; empty for `generic`, which requires a caller
    /// override (`BackendConfig::with_base_url`).
    pub base_url: String,
    pub auth: AuthShape,
    pub dialect: Dialect,
    pub streaming: StreamingFormat,
    pub vision_encoding: VisionEncoding,
    pub max_tokens_field: String,
}

fn row(
    id: &str,
    base_url: &str,
    auth: AuthShape,
    dialect: Dialect,
    streaming: StreamingFormat,
    vision_encoding: VisionEncoding,
    max_tokens_field: &str,
) -> ProviderQuirks {
    ProviderQuirks {
        id: id.to_string(),
        base_url: base_url.to_string(),
        auth,
        dialect,
        streaming,
        vision_encoding,
        max_tokens_field: max_tokens_field.to_string(),
    }
}

fn build_table() -> Vec<ProviderQuirks> {
    use AuthShape::*;
    use Dialect::*;
    use StreamingFormat::*;
    use VisionEncoding::*;

    vec![
        // Local / self-hosted OpenAI-compatible servers: no auth by
        // default, riding the OpenAI dialect over their own `/v1` shim.
        row(
            "ollama",
            "http://localhost:11434/v1",
            None,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "llamacpp",
            "http://localhost:8080/v1",
            None,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "lmstudio",
            "http://localhost:1234/v1",
            None,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "vllm",
            "http://localhost:8000/v1",
            None,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        // Generic: a user-supplied base URL with an optional bearer key.
        // No default SSE framing guarantee for an arbitrary server, so this
        // entry rides plain newline-delimited JSON chunking instead, which
        // also keeps the Chunked framing path under real test coverage.
        row(
            "generic",
            "",
            Bearer,
            Openai,
            Chunked,
            OpenaiImageUrl,
            "max_tokens",
        ),
        // Hosted OpenAI-dialect providers.
        row(
            "openai",
            "https://api.openai.com/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_completion_tokens",
        ),
        row(
            "deepseek",
            "https://api.deepseek.com/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "minimax",
            "https://api.minimax.chat/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "kimi",
            "https://api.moonshot.cn/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "groq",
            "https://api.groq.com/openai/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "mistral",
            "https://api.mistral.ai/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "xai",
            "https://api.x.ai/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        row(
            "openrouter",
            "https://openrouter.ai/api/v1",
            Bearer,
            Openai,
            Sse,
            OpenaiImageUrl,
            "max_tokens",
        ),
        // Native dialects.
        row(
            "anthropic",
            "https://api.anthropic.com/v1",
            XApiKey,
            Anthropic,
            Sse,
            AnthropicSource,
            "max_tokens",
        ),
        row(
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta",
            QueryParam,
            Gemini,
            Sse,
            GeminiInline,
            "maxOutputTokens",
        ),
    ]
}

/// The provider quirk table. Built once and cached; see the module doc for
/// why entries are owned data rather than a `'static` const array.
pub fn provider_quirks() -> &'static [ProviderQuirks] {
    static TABLE: OnceLock<Vec<ProviderQuirks>> = OnceLock::new();
    TABLE.get_or_init(build_table)
}

/// Look up one provider's quirks by id (e.g. `"anthropic"`).
pub fn find(id: &str) -> Option<&'static ProviderQuirks> {
    provider_quirks().iter().find(|q| q.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_every_provider_docs_specs_backends_promises() {
        let expected = [
            "ollama",
            "llamacpp",
            "lmstudio",
            "vllm",
            "generic",
            "openai",
            "anthropic",
            "gemini",
            "deepseek",
            "minimax",
            "kimi",
            "qwen",
            "groq",
            "mistral",
            "xai",
            "openrouter",
        ];
        for id in expected {
            assert!(find(id).is_some(), "missing quirk table entry for `{id}`");
        }
        assert_eq!(provider_quirks().len(), expected.len());
    }

    #[test]
    fn anthropic_and_gemini_get_native_dialects_everything_else_rides_openai() {
        assert_eq!(find("anthropic").unwrap().dialect, Dialect::Anthropic);
        assert_eq!(find("gemini").unwrap().dialect, Dialect::Gemini);
        for id in [
            "ollama",
            "llamacpp",
            "lmstudio",
            "vllm",
            "generic",
            "openai",
            "deepseek",
            "openrouter",
        ] {
            assert_eq!(
                find(id).unwrap().dialect,
                Dialect::Openai,
                "{id} should ride the openai dialect"
            );
        }
    }

    #[test]
    fn auth_shapes_match_each_providers_real_api() {
        assert_eq!(find("anthropic").unwrap().auth, AuthShape::XApiKey);
        assert_eq!(find("gemini").unwrap().auth, AuthShape::QueryParam);
        assert_eq!(find("openai").unwrap().auth, AuthShape::Bearer);
        assert_eq!(find("ollama").unwrap().auth, AuthShape::None);
    }

    #[test]
    fn generic_has_no_default_base_url() {
        assert_eq!(find("generic").unwrap().base_url, "");
    }

    #[test]
    fn max_tokens_field_name_is_a_real_per_provider_quirk() {
        assert_eq!(find("gemini").unwrap().max_tokens_field, "maxOutputTokens");
        assert_eq!(find("anthropic").unwrap().max_tokens_field, "max_tokens");
        assert_eq!(
            find("openai").unwrap().max_tokens_field,
            "max_completion_tokens"
        );
        assert_eq!(find("ollama").unwrap().max_tokens_field, "max_tokens");
    }

    #[test]
    fn quirk_table_round_trips_as_json() {
        // "The table is data (a Rust const table plus a JSON copy for
        // tests)": prove the Rust table serializes to JSON and back without
        // loss, so a provider-side change really can be authored as a JSON
        // edit rather than a Rust code change.
        let json = serde_json::to_string_pretty(provider_quirks()).unwrap();
        let back: Vec<ProviderQuirks> =
            serde_json::from_str(&json).expect("quirk table JSON round-trips");
        assert_eq!(back, provider_quirks());
    }

    #[test]
    fn provider_quirks_are_a_data_edit_not_a_code_change() {
        // A hand-authored quirk row, written as JSON the way a provider-side
        // fix would be, loads through the exact same type the const table
        // uses.
        let json = r#"{
            "id": "custom_local_server",
            "base_url": "http://localhost:9009/v1",
            "auth": "bearer",
            "dialect": "openai",
            "streaming": "sse",
            "vision_encoding": "openai_image_url",
            "max_tokens_field": "max_tokens"
        }"#;
        let q: ProviderQuirks =
            serde_json::from_str(json).expect("hand-authored quirk JSON parses");
        assert_eq!(q.dialect, Dialect::Openai);
        assert_eq!(q.auth, AuthShape::Bearer);
    }

    #[test]
    fn unknown_provider_id_is_not_found() {
        assert!(find("not-a-real-provider").is_none());
    }
}
