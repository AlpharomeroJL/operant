//! The doctor finding this broker returns on refresh failure
//! (`docs/specs/backends.md`: "refresh failure emits a doctor finding with
//! a one-click re-auth card"). This module only builds the typed
//! [`operant_doctor::Finding`] value -- registering it as a
//! [`operant_doctor::Check`], publishing it on the `doctor.finding` bus
//! topic (`contracts/bus_events.md`), and rendering the one-click re-auth
//! card are doctor/UI wiring that lives outside this lane's owned paths
//! (see FOLLOWUPS: U2B).

use operant_doctor::{Finding, Severity};

use super::provider::ProviderId;

/// Stable per-provider finding id, so a UI can key a card on it and a
/// later `doctor.fixed{finding_id}` event can reference the same id --
/// the same convention every other `operant-doctor` check's `finding_id`
/// follows.
pub fn refresh_failure_finding_id(provider: ProviderId) -> String {
    format!("oauth_refresh_failed_{}", provider.as_str())
}

/// Build the finding a UI shows when silent refresh fails: what happened,
/// why (in plain language, `detail` folded in but never a raw token --
/// callers pass an already-[`super::redact::SecretGuard::redact`]-ed
/// string), and a one-click fix. `fix_command` is a stable verb string;
/// resolving it to actually re-running [`super::flow::Broker::begin`] for
/// this provider is the doctor/UI's job, not this module's.
pub fn refresh_failure_finding(provider: ProviderId, detail: &str) -> Finding {
    Finding {
        finding_id: refresh_failure_finding_id(provider),
        severity: Severity::Error,
        what: format!("{} could not renew its sign-in.", provider.display_name()),
        why: format!(
            "The silent token refresh for {} failed: {detail}",
            provider.display_name()
        ),
        action: format!("Sign in again with {}.", provider.display_name()),
        fix_command: Some(format!("oauth:reauth:{}", provider.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_failure_finding_is_an_error_with_a_one_click_fix() {
        let finding = refresh_failure_finding(ProviderId::ChatgptPlan, "http 400 invalid_grant");
        assert_eq!(finding.finding_id, "oauth_refresh_failed_chatgpt_plan");
        assert_eq!(finding.severity, Severity::Error);
        assert!(finding.what.contains("Sign in with ChatGPT"));
        assert!(finding.why.contains("invalid_grant"));
        assert_eq!(
            finding.fix_command.as_deref(),
            Some("oauth:reauth:chatgpt_plan")
        );
    }

    #[test]
    fn finding_id_is_stable_and_distinct_per_provider() {
        assert_eq!(
            refresh_failure_finding_id(ProviderId::ChatgptPlan),
            "oauth_refresh_failed_chatgpt_plan"
        );
        assert_eq!(
            refresh_failure_finding_id(ProviderId::ClaudePlan),
            "oauth_refresh_failed_claude_plan"
        );
    }

    #[test]
    fn finding_serializes_the_same_shape_every_other_doctor_finding_uses() {
        let finding = refresh_failure_finding(ProviderId::ClaudePlan, "connection refused");
        let v = serde_json::to_value(&finding).unwrap();
        assert_eq!(v["severity"], "error");
        assert!(v.get("finding_id").is_some());
        assert!(v.get("fix_command").is_some());
    }
}
