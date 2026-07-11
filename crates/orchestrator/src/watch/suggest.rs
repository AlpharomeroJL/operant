//! The `suggestion.*` bus payloads and the plain-language offer copy.
//!
//! These topics (`suggestion.offered` / `.accepted` / `.dismissed`) are the
//! watch-and-suggest family from `contracts/bus_events.md`. That family has no
//! typed constructor in `operant-core` (its typed set covers runs, gates,
//! sidecars, and guardian only), so the payloads are declared here and
//! published with [`operant_core::Bus::publish`] under their exact contract
//! topics. Field names mirror the contract verbatim.

use serde::{Deserialize, Serialize};

/// `suggestion.offered` topic string.
pub const SUGGESTION_OFFERED: &str = "suggestion.offered";
/// `suggestion.accepted` topic string.
pub const SUGGESTION_ACCEPTED: &str = "suggestion.accepted";
/// `suggestion.dismissed` topic string.
pub const SUGGESTION_DISMISSED: &str = "suggestion.dismissed";

/// `suggestion.offered`: suggestion_id, pattern_digest, occurrences. Published
/// only when the feature is on (opt-in); the contract notes it as
/// "watch-and-suggest, opt-in only".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestionOffered {
    pub suggestion_id: String,
    pub pattern_digest: String,
    pub occurrences: u32,
}

/// `suggestion.accepted`: suggestion_id. Acceptance seeds a supervised run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestionAccepted {
    pub suggestion_id: String,
}

/// `suggestion.dismissed`: suggestion_id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestionDismissed {
    pub suggestion_id: String,
}

/// The plain-language offer shown to the user when a pattern repeats. Jargon
/// free: it says what happened ("done this N times") and asks permission to
/// learn it, matching the brief's wording.
pub fn offer_sentence(occurrences: u32) -> String {
    format!("You have done this {occurrences} times. Want me to learn it?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offered_payload_matches_contract_fields() {
        let v = serde_json::to_value(SuggestionOffered {
            suggestion_id: "sug-1".into(),
            pattern_digest: "abc".into(),
            occurrences: 4,
        })
        .unwrap();
        assert_eq!(v["suggestion_id"], "sug-1");
        assert_eq!(v["pattern_digest"], "abc");
        assert_eq!(v["occurrences"], 4);
    }

    #[test]
    fn offer_sentence_is_plain_language() {
        let s = offer_sentence(4);
        assert_eq!(s, "You have done this 4 times. Want me to learn it?");
        // No internal jargon leaks into user-facing copy.
        for jargon in ["n-gram", "digest", "pattern_digest", "explore", "trajectory"] {
            assert!(!s.contains(jargon), "offer copy must stay jargon-free: {jargon}");
        }
    }
}
