//! Normalization and redaction: turning a raw [`ManualEvent`] into the
//! [`StoredEvent`] that is safe to keep in the local buffer.
//!
//! Two independent privacy properties come together here:
//!
//! * **The pattern token never carries content.** [`normalize`] reduces an
//!   action to its *shape* -- kind plus a stable target descriptor -- and
//!   deliberately drops all free-typed text (a `type` action's `text`, an
//!   element's `value`). Repetition is about *what* the user did, not *what*
//!   they typed, so secrets never enter an n-gram or a pattern digest in the
//!   first place.
//! * **Stored actions are redacted before storage.** [`redact_for_storage`]
//!   reuses X4's credential classifier
//!   ([`operant_recorder::redact::is_credential_dialog_match`], plus the
//!   perceiver's own `is_password` flag) to decide whether the targeted
//!   element was a credential field, and scrubs the action's content params
//!   when it was. The classifier and its term list are X4's, not
//!   reimplemented here.

use operant_ir::{Action, ActionKind, Element, Selector};
use operant_recorder::redact::is_credential_dialog_match;

use super::event::{ManualEvent, StoredEvent};

/// Placeholder substituted for any content param scrubbed by redaction.
const REDACTED: &str = "[redacted]";

/// Content-bearing params dropped from the stored action when the target is a
/// credential field. These are the free-text carriers; structural params
/// (e.g. a `combo` like `ctrl+s`) are keystroke names, not secrets, and are
/// kept so an accepted suggestion can still be replayed faithfully.
const CONTENT_PARAMS: &[&str] = &["text", "value"];

/// Reduce an action to a normalized, content-free token for n-gram matching.
///
/// The token captures the action's *kind* and a stable *target descriptor*
/// (automation id > name/role leaf > window process > `unknown`), plus the
/// key combo for `key` actions (keystroke names are structure, not content).
/// Typed text is never included, so two runs of the same task normalize to the
/// same token regardless of what was typed, and no secret can leak into a
/// pattern.
pub fn normalize(action: &Action) -> String {
    let kind = kind_str(action.kind);
    let target = target_descriptor(action);
    match action.kind {
        // A key action's identity includes which key: ctrl+s and ctrl+c are
        // different steps. The combo is a keystroke name, never free text.
        ActionKind::Key => {
            let combo = action
                .params
                .get("combo")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if combo.is_empty() {
                format!("{kind}:{target}")
            } else {
                format!("{kind}:{target}:{combo}")
            }
        }
        _ => format!("{kind}:{target}"),
    }
}

/// Build the redacted [`StoredEvent`] for a manual event: its normalized token
/// plus a redacted copy of the action. When the targeted element is a
/// credential field, content params are scrubbed before the action is stored,
/// so nothing sensitive ever reaches the buffer.
pub fn redact_for_storage(event: &ManualEvent) -> StoredEvent {
    let token = normalize(&event.action);
    let mut action = event.action.clone();
    if is_sensitive(event.target.as_ref()) {
        scrub_content_params(&mut action);
    }
    StoredEvent { token, action }
}

/// True when the targeted element is a credential field, by the perceiver's
/// own `is_password` flag or X4's credential-dialog classifier. A missing
/// target is treated as not sensitive (there is no content-bearing element to
/// scrub); the token is content-free regardless.
fn is_sensitive(target: Option<&Element>) -> bool {
    target.is_some_and(|el| el.is_password || is_credential_dialog_match(el))
}

/// Replace every content-bearing param with [`REDACTED`], in place.
fn scrub_content_params(action: &mut Action) {
    for key in CONTENT_PARAMS {
        if let Some(slot) = action.params.get_mut(*key) {
            *slot = serde_json::Value::String(REDACTED.to_string());
        }
    }
}

fn kind_str(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Click => "click",
        ActionKind::Type => "type",
        ActionKind::Key => "key",
        ActionKind::Scroll => "scroll",
        ActionKind::Drag => "drag",
        ActionKind::Wait => "wait",
        ActionKind::Assert => "assert",
        ActionKind::AdapterCall => "adapter",
    }
}

/// A stable, content-free descriptor of an action's target. Prefers the most
/// stable identifier available so the same on-screen control normalizes
/// identically across repetitions.
fn target_descriptor(action: &Action) -> String {
    let Some(target) = action.target.as_ref() else {
        return "unknown".to_string();
    };
    for selector in &target.selectors {
        match selector {
            Selector::AutomationId { value } if !value.is_empty() => {
                return format!("automation_id={value}");
            }
            Selector::Css { value } if !value.is_empty() => {
                return format!("css={value}");
            }
            Selector::NameRolePath { path } => {
                if let Some(seg) = path.last() {
                    return format!("{}={}", seg.role, seg.name);
                }
            }
            Selector::OrdinalPath { path } => {
                if let Some(seg) = path.last() {
                    return format!("{}#{}", seg.role, seg.ordinal);
                }
            }
            _ => {}
        }
    }
    if let Some(process) = target.window.as_ref().and_then(|w| w.process.as_ref()) {
        return format!("window={process}");
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{Grounding, NameRoleSeg, RiskClass, Role, Target};

    fn base(kind: ActionKind) -> Action {
        Action {
            v: 1,
            id: "x".to_string(),
            kind,
            intent: None,
            target: None,
            params: serde_json::Map::new(),
            pace: Default::default(),
            risk_class: RiskClass::Write,
            irreversible: false,
            grounding: Grounding::Uia,
            timeout_ms: 5000,
            retry: Default::default(),
        }
    }

    fn credential_edit() -> Element {
        Element {
            idx: 1,
            parent: None,
            role: Role::Edit,
            name: "Password".to_string(),
            value: None,
            automation_id: None,
            bounds: None,
            enabled: true,
            offscreen: false,
            is_password: true,
            patterns: vec![],
            selectors: vec![],
        }
    }

    fn plain_edit() -> Element {
        Element {
            idx: 2,
            parent: None,
            role: Role::Edit,
            name: "Subject".to_string(),
            value: None,
            automation_id: None,
            bounds: None,
            enabled: true,
            offscreen: false,
            is_password: false,
            patterns: vec![],
            selectors: vec![],
        }
    }

    #[test]
    fn normalize_prefers_automation_id() {
        let mut a = base(ActionKind::Click);
        a.target = Some(Target {
            selectors: vec![Selector::AutomationId { value: "SaveButton".into() }],
            ..Default::default()
        });
        assert_eq!(normalize(&a), "click:automation_id=SaveButton");
    }

    #[test]
    fn normalize_key_includes_the_combo() {
        let mut a = base(ActionKind::Key);
        a.params.insert("combo".into(), serde_json::json!("ctrl+s"));
        assert_eq!(normalize(&a), "key:unknown:ctrl+s");
    }

    #[test]
    fn normalize_type_never_includes_typed_text() {
        let mut a = base(ActionKind::Type);
        a.target = Some(Target {
            selectors: vec![Selector::NameRolePath {
                path: vec![NameRoleSeg { role: "edit".into(), name: "Subject".into() }],
            }],
            ..Default::default()
        });
        a.params.insert("text".into(), serde_json::json!("my secret invoice text"));
        let token = normalize(&a);
        assert_eq!(token, "type:edit=Subject");
        assert!(!token.contains("secret"), "typed text must never enter the token");
    }

    #[test]
    fn redact_scrubs_content_when_target_is_a_credential_field() {
        let mut a = base(ActionKind::Type);
        a.params.insert("text".into(), serde_json::json!("hunter2"));
        let event = ManualEvent::with_target(a, credential_edit());
        let stored = redact_for_storage(&event);
        assert_eq!(
            stored.action.params.get("text").and_then(|v| v.as_str()),
            Some(REDACTED)
        );
        assert!(!format!("{stored:?}").contains("hunter2"));
    }

    #[test]
    fn redact_matches_credential_field_by_name_without_is_password() {
        // Reuses X4's classifier: a "Card CVV" edit is sensitive even though
        // is_password was never set.
        let mut cvv = plain_edit();
        cvv.name = "Card CVV".to_string();
        assert!(!cvv.is_password);
        let mut a = base(ActionKind::Type);
        a.params.insert("value".into(), serde_json::json!("123"));
        let stored = redact_for_storage(&ManualEvent::with_target(a, cvv));
        assert_eq!(
            stored.action.params.get("value").and_then(|v| v.as_str()),
            Some(REDACTED)
        );
    }

    #[test]
    fn redact_keeps_content_for_non_credential_fields() {
        let mut a = base(ActionKind::Type);
        a.params.insert("text".into(), serde_json::json!("just a subject line"));
        let stored = redact_for_storage(&ManualEvent::with_target(a, plain_edit()));
        assert_eq!(
            stored.action.params.get("text").and_then(|v| v.as_str()),
            Some("just a subject line")
        );
    }

    #[test]
    fn redact_keeps_key_combo_untouched() {
        let mut a = base(ActionKind::Key);
        a.params.insert("combo".into(), serde_json::json!("ctrl+s"));
        // Even with a credential target, a keystroke name is not content.
        let stored = redact_for_storage(&ManualEvent::with_target(a, credential_edit()));
        assert_eq!(
            stored.action.params.get("combo").and_then(|v| v.as_str()),
            Some("ctrl+s")
        );
    }
}
