//! The OAuth identity table (`docs/specs/backends.md` OAuth broker
//! section): "the quirk table maps each OAuth identity to its API dialect
//! and endpoints, and endpoints are config-table driven so provider-side
//! changes are a data edit, not a code change." Mirrors
//! `backends::quirks`'s own per-provider table in spirit, scoped to the two
//! subscription sign-in identities FR-M1 asks for: "Sign in with ChatGPT"
//! (Codex/ChatGPT plan) and "Sign in with Claude" (Claude plan).
//!
//! `base_url` is deliberately not baked in here for either identity: the
//! real endpoints for these subscription sign-in flows are not published
//! where this lane can verify them, and hardcoding a guess would be worse
//! than requiring one. [`OauthProviderConfig::base_url`] must be supplied
//! at [`super::flow::Broker`] construction (production config for real
//! use, `http://127.0.0.1:{port}` for the mock server in tests) -- exactly
//! the "data edit, not a code change" the spec calls for.

/// Stable identifier for an OAuth-backed subscription identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    ChatgptPlan,
    ClaudePlan,
}

impl ProviderId {
    /// The wire/config identifier, matching
    /// `contracts/fixtures/oauth/config.json`'s `providers[].id`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::ChatgptPlan => "chatgpt_plan",
            ProviderId::ClaudePlan => "claude_plan",
        }
    }

    /// The button label a UI shows, matching
    /// `contracts/fixtures/oauth/config.json`'s `providers[].display`.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::ChatgptPlan => "Sign in with ChatGPT",
            ProviderId::ClaudePlan => "Sign in with Claude",
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the OAuth identity table: everything about a provider that
/// is data, not instance state. Endpoint paths are relative; joined onto
/// `base_url` at request-building time.
#[derive(Debug, Clone)]
pub struct OauthProviderConfig {
    pub id: ProviderId,
    pub client_id: String,
    pub scopes: Vec<String>,
    /// The `ModelBackend` dialect the issued token authenticates against
    /// once role-assigned (`backends::Dialect`'s own values -- "openai" or
    /// "anthropic"). Kept as a plain string so this module does not need a
    /// dependency on `backends` just to name it; whichever lane wires an
    /// OAuth-issued token into a `BackendConfig` maps this string onto
    /// `backends::Dialect` there.
    pub dialect: String,
    pub base_url: String,
    pub authorize_path: String,
    pub token_path: String,
    pub revoke_path: String,
}

impl OauthProviderConfig {
    /// The built-in row for a known [`ProviderId`], with `base_url` left
    /// for the caller to fill in (see module doc). Endpoint paths default
    /// to `contracts/fixtures/oauth/config.json`'s shape, which every real
    /// provider table entry is expected to match unless a future data
    /// edit says otherwise.
    pub fn for_provider(id: ProviderId, base_url: impl Into<String>) -> Self {
        let (client_id, scopes, dialect): (&str, &[&str], &str) = match id {
            ProviderId::ChatgptPlan => ("operant-chatgpt-plan", &["model.complete"], "openai"),
            ProviderId::ClaudePlan => ("operant-claude-plan", &["model.complete"], "anthropic"),
        };
        OauthProviderConfig {
            id,
            client_id: client_id.to_string(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            dialect: dialect.to_string(),
            base_url: base_url.into(),
            authorize_path: "/oauth/authorize".to_string(),
            token_path: "/oauth/token".to_string(),
            revoke_path: "/oauth/revoke".to_string(),
        }
    }

    #[must_use]
    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    pub fn authorize_url_base(&self) -> String {
        format!("{}{}", self.base_url, self.authorize_path)
    }

    pub fn token_url(&self) -> String {
        format!("{}{}", self.base_url, self.token_path)
    }

    pub fn revoke_url(&self) -> String {
        format!("{}{}", self.base_url, self.revoke_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatgpt_plan_row_matches_the_mock_contract_fixture() {
        let cfg = OauthProviderConfig::for_provider(ProviderId::ChatgptPlan, "http://127.0.0.1:9");
        assert_eq!(cfg.id.as_str(), "chatgpt_plan");
        assert_eq!(cfg.id.display_name(), "Sign in with ChatGPT");
        assert_eq!(cfg.dialect, "openai");
        assert_eq!(cfg.scopes, vec!["model.complete".to_string()]);
    }

    #[test]
    fn claude_plan_row_matches_the_mock_contract_fixture() {
        let cfg = OauthProviderConfig::for_provider(ProviderId::ClaudePlan, "http://127.0.0.1:9");
        assert_eq!(cfg.id.as_str(), "claude_plan");
        assert_eq!(cfg.id.display_name(), "Sign in with Claude");
        assert_eq!(cfg.dialect, "anthropic");
    }

    #[test]
    fn endpoint_urls_join_base_and_path() {
        let cfg =
            OauthProviderConfig::for_provider(ProviderId::ClaudePlan, "http://127.0.0.1:4321");
        assert_eq!(
            cfg.authorize_url_base(),
            "http://127.0.0.1:4321/oauth/authorize"
        );
        assert_eq!(cfg.token_url(), "http://127.0.0.1:4321/oauth/token");
        assert_eq!(cfg.revoke_url(), "http://127.0.0.1:4321/oauth/revoke");
    }

    #[test]
    fn display_renders_the_wire_id() {
        assert_eq!(ProviderId::ChatgptPlan.to_string(), "chatgpt_plan");
    }
}
