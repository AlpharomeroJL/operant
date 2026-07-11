//! Runtime-owned hard safety invariants (FR-S4).
//!
//! These are a deny-list evaluated on EVERY proposed action, in explore and in
//! replay. No workflow file can declare, weaken, or disable them (see
//! [`crate::load_manifest`]). A hit does not stop the machine; it freezes the
//! run, emits a plain-language [`Escalation`], and (for approval-class hits)
//! requires an explicit human approval event to proceed.
//!
//! Three invariants:
//! 1. The target element is a credential/password field (snapshot `is_password`,
//!    or a role+name match against the credential lexicon) -> require approval.
//! 2. The foreground dialog classifies as payment or deletion (title/button
//!    lexicon in [`data/dialog_lexicon.json`]) -> require approval.
//! 3. An unexpected window class appears mid-run -> escalate.

use std::collections::BTreeSet;

use operant_ir::{Element, Role, Snapshot};
use serde::Deserialize;

/// The classifier vocabulary, loaded from the embedded data file.
static LEXICON_JSON: &str = include_str!("data/dialog_lexicon.json");

#[derive(Debug, Clone, Deserialize)]
struct Lexicon {
    credential_terms: Vec<String>,
    payment: DialogTerms,
    deletion: DialogTerms,
}

#[derive(Debug, Clone, Deserialize)]
struct DialogTerms {
    title_terms: Vec<String>,
    button_terms: Vec<String>,
}

/// Which hard invariant fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyReason {
    /// The target element is a credential / password field.
    CredentialField,
    /// The foreground dialog is a payment confirmation.
    PaymentDialog,
    /// The foreground dialog is a deletion confirmation.
    DeletionDialog,
    /// An unexpected window class appeared mid-run.
    UnexpectedWindowClass,
}

/// How the runtime must react to a fired invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Freeze the run and require an explicit human approval event to proceed.
    RequireApproval,
    /// Freeze the run and escalate (a surprise the plan did not anticipate).
    Escalate,
}

/// A frozen-run event: the reason, the required disposition, and a plain-language
/// sentence suitable for a human approver. The proposed action is attached for
/// the audit chain.
#[derive(Debug, Clone, PartialEq)]
pub struct Escalation {
    /// Which invariant fired.
    pub reason: SafetyReason,
    /// What the runtime must do.
    pub disposition: Disposition,
    /// A one-sentence, plain-language explanation.
    pub sentence: String,
    /// The proposed action, verbatim, for the audit record.
    pub proposed: serde_json::Value,
}

/// The verdict of the hard-invariant deny-list for one proposed action.
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyVerdict {
    /// No invariant fired; the action may proceed (subject to grants).
    Clear,
    /// An invariant fired; the run must freeze.
    Blocked(Escalation),
}

impl SafetyVerdict {
    /// True when an invariant fired.
    pub fn is_blocked(&self) -> bool {
        matches!(self, SafetyVerdict::Blocked(_))
    }

    /// The escalation, if any.
    pub fn escalation(&self) -> Option<&Escalation> {
        match self {
            SafetyVerdict::Blocked(e) => Some(e),
            SafetyVerdict::Clear => None,
        }
    }
}

/// The runtime guard that enforces FR-S4 across a run.
///
/// Holds the set of window classes (by process name) the run expects to see, so
/// invariant 3 can flag a surprise. The classifier lexicon is embedded.
#[derive(Debug, Clone)]
pub struct RunGuard {
    expected_windows: BTreeSet<String>,
    lexicon: Lexicon,
}

impl RunGuard {
    /// Build a guard for a run that expects the given window classes (process
    /// names). Pass an empty set to disable the unexpected-window invariant
    /// (e.g. before the first window is known).
    pub fn new<I, S>(expected_windows: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let lexicon: Lexicon =
            serde_json::from_str(LEXICON_JSON).expect("embedded dialog lexicon is valid JSON");
        RunGuard {
            expected_windows: expected_windows
                .into_iter()
                .map(|s| s.into().to_lowercase())
                .collect(),
            lexicon,
        }
    }

    /// Evaluate the deny-list for one proposed action.
    ///
    /// `target` is the element the action is about to interact with (already
    /// resolved from the snapshot), or `None` for actions with no element target.
    /// `snapshot` is the current perception snapshot (its `window` is the
    /// foreground). `proposed` is the action IR, attached to any escalation.
    pub fn evaluate(
        &self,
        target: Option<&Element>,
        snapshot: &Snapshot,
        proposed: &serde_json::Value,
    ) -> SafetyVerdict {
        // Invariant 1: credential / password field.
        if let Some(el) = target {
            if self.is_credential_field(el) {
                return blocked(
                    SafetyReason::CredentialField,
                    Disposition::RequireApproval,
                    format!(
                        "This step would enter text into the \"{}\" field, which looks like a \
                         password or credential field. Approve before continuing.",
                        el.name
                    ),
                    proposed,
                );
            }
        }

        // Invariant 2: payment / deletion confirmation dialog.
        if let Some(reason) = self.classify_dialog(snapshot) {
            let what = match reason {
                SafetyReason::PaymentDialog => "a payment confirmation",
                SafetyReason::DeletionDialog => "a deletion confirmation",
                _ => "a sensitive confirmation",
            };
            return blocked(
                reason,
                Disposition::RequireApproval,
                format!(
                    "The foreground window \"{}\" looks like {}. Approve before continuing.",
                    snapshot.window.title, what
                ),
                proposed,
            );
        }

        // Invariant 3: unexpected window class mid-run.
        if !self.expected_windows.is_empty() {
            let current = snapshot.window.process.to_lowercase();
            if !self.expected_windows.contains(&current) {
                return blocked(
                    SafetyReason::UnexpectedWindowClass,
                    Disposition::Escalate,
                    format!(
                        "An unexpected window appeared: \"{}\" ({}). This run did not expect it, \
                         so it has been paused.",
                        snapshot.window.title, snapshot.window.process
                    ),
                    proposed,
                );
            }
        }

        SafetyVerdict::Clear
    }

    /// True when an element is a credential/password field: the perceiver flagged
    /// `is_password`, or it is a text-entry role whose name matches the credential
    /// lexicon.
    fn is_credential_field(&self, el: &Element) -> bool {
        if el.is_password {
            return true;
        }
        let entry_role = matches!(el.role, Role::Edit | Role::Text | Role::Combobox);
        if !entry_role {
            return false;
        }
        let name = el.name.to_lowercase();
        self.lexicon
            .credential_terms
            .iter()
            .any(|term| name.contains(&term.to_lowercase()))
    }

    /// Classify the foreground window as a payment or deletion dialog, using its
    /// title plus the names of its button elements.
    fn classify_dialog(&self, snapshot: &Snapshot) -> Option<SafetyReason> {
        let title = snapshot.window.title.to_lowercase();
        let buttons: Vec<String> = snapshot
            .elements
            .iter()
            .filter(|e| e.role == Role::Button)
            .map(|e| e.name.to_lowercase())
            .collect();

        let title_hits = |terms: &[String]| terms.iter().any(|t| title.contains(&t.to_lowercase()));
        let button_hits = |terms: &[String]| {
            terms
                .iter()
                .any(|t| buttons.iter().any(|b| b.contains(&t.to_lowercase())))
        };

        // Payment first, then deletion.
        if title_hits(&self.lexicon.payment.title_terms)
            || button_hits(&self.lexicon.payment.button_terms)
        {
            return Some(SafetyReason::PaymentDialog);
        }
        if title_hits(&self.lexicon.deletion.title_terms)
            || button_hits(&self.lexicon.deletion.button_terms)
        {
            return Some(SafetyReason::DeletionDialog);
        }
        None
    }
}

fn blocked(
    reason: SafetyReason,
    disposition: Disposition,
    sentence: String,
    proposed: &serde_json::Value,
) -> SafetyVerdict {
    SafetyVerdict::Blocked(Escalation { reason, disposition, sentence, proposed: proposed.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn window_snapshot(process: &str, title: &str, elements: Vec<Element>) -> Snapshot {
        serde_json::from_value(json!({
            "v": 1,
            "source": "fixture",
            "window": { "process": process, "title": title },
            "digest": "d0",
            "elements": elements.iter().map(|e| serde_json::to_value(e).unwrap()).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn element(idx: u32, role: &str, name: &str, is_password: bool) -> Element {
        serde_json::from_value(json!({
            "idx": idx, "parent": null, "role": role, "name": name,
            "is_password": is_password
        }))
        .unwrap()
    }

    #[test]
    fn password_field_requires_approval() {
        let guard = RunGuard::new(["chrome.exe"]);
        let pw = element(1, "edit", "Password", true);
        let snap = window_snapshot("chrome.exe", "Sign in", vec![pw.clone()]);
        let v = guard.evaluate(Some(&pw), &snap, &json!({"id": "s1"}));
        let e = v.escalation().expect("blocked");
        assert_eq!(e.reason, SafetyReason::CredentialField);
        assert_eq!(e.disposition, Disposition::RequireApproval);
    }

    #[test]
    fn credential_by_name_without_is_password_flag() {
        let guard = RunGuard::new(["chrome.exe"]);
        // is_password false, but the name matches the credential lexicon.
        let pin = element(1, "edit", "Card CVV", false);
        let snap = window_snapshot("chrome.exe", "Checkout", vec![pin.clone()]);
        assert!(guard.evaluate(Some(&pin), &snap, &json!({})).is_blocked());
    }

    #[test]
    fn non_credential_edit_is_clear() {
        let guard = RunGuard::new(["notepad.exe"]);
        let doc = element(1, "document", "Text editor", false);
        let snap = window_snapshot("notepad.exe", "Untitled - Notepad", vec![doc.clone()]);
        assert_eq!(guard.evaluate(Some(&doc), &snap, &json!({})), SafetyVerdict::Clear);
    }

    #[test]
    fn deletion_dialog_requires_approval() {
        let guard = RunGuard::new(["explorer.exe"]);
        let btn = element(1, "button", "Delete", false);
        let snap = window_snapshot("explorer.exe", "Confirm Delete", vec![btn]);
        let v = guard.evaluate(None, &snap, &json!({}));
        assert_eq!(v.escalation().unwrap().reason, SafetyReason::DeletionDialog);
    }

    #[test]
    fn payment_dialog_requires_approval() {
        let guard = RunGuard::new(["chrome.exe"]);
        let btn = element(1, "button", "Pay now", false);
        let snap = window_snapshot("chrome.exe", "Complete purchase", vec![btn]);
        assert_eq!(
            guard.evaluate(None, &snap, &json!({})).escalation().unwrap().reason,
            SafetyReason::PaymentDialog
        );
    }

    #[test]
    fn unexpected_window_escalates() {
        let guard = RunGuard::new(["notepad.exe"]);
        let snap = window_snapshot("uac.exe", "User Account Control", vec![]);
        let e = guard.evaluate(None, &snap, &json!({})).escalation().cloned().unwrap();
        assert_eq!(e.reason, SafetyReason::UnexpectedWindowClass);
        assert_eq!(e.disposition, Disposition::Escalate);
    }
}
