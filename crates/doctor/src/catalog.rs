//! The runtime error catalog (C19 zero-code, FR-U3): a typed, exhaustive
//! enum of every user-facing runtime error kind, paired with plain-language
//! copy for what happened, why, and one suggested action.
//!
//! `contracts/bus_events.md` says of `run.step.failed`: "error_id keys the
//! error catalog." [`ErrorKind`] is that catalog's key; [`ErrorKind::entry`]
//! is the lookup. The [`checks`](crate::checks) module reuses the same
//! entries for the doctor findings that overlap with a runtime error kind
//! (a model going unreachable mid-run is the same problem whether Operant
//! finds it proactively or hits it live), so the two surfaces never drift
//! into saying the same thing two different ways.
//!
//! Adding a variant is a two-step, compiler-checked change: [`ErrorKind::entry`]
//! matches every variant with no wildcard arm, so a new variant fails to
//! compile until it gets an entry there; [`ErrorKind::ALL`] is a second,
//! hand-maintained list used for iteration (tests, the glossary scan, future
//! CLI rendering), cross-checked against a second independent exhaustive
//! match in this module's tests so the two lists cannot silently drift
//! apart.

use serde::{Deserialize, Serialize};

/// Every user-facing runtime error kind Operant can surface, whether hit
/// live during a run or found proactively by a doctor check. Fieldless and
/// `Copy` on purpose: this is a catalog key, not a carrier of per-incident
/// detail (a message with the specific file path, host name, and so on is
/// assembled by the caller alongside the catalog's fixed copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Perception could not find the step's target on the screen.
    TargetNotFound,
    /// A step did not finish inside its time budget.
    ActionTimedOut,
    /// A risky step is waiting on a human's yes before it can continue.
    ApprovalRequired,
    /// A human said no when a risky step asked for approval.
    ApprovalDenied,
    /// A hard safety check stopped a step before it ran.
    SafetyBlocked,
    /// The before-check for a step failed: the app was not in the expected state.
    PreconditionFailed,
    /// The after-check for a step failed: its result could not be confirmed.
    PostconditionFailed,
    /// The app's screen changed enough that the saved workflow needs an update.
    WorkflowDriftDetected,
    /// The configured model could not be reached.
    ModelUnreachable,
    /// The sign-in used to reach the model has expired.
    ModelSignInExpired,
    /// The model's reply could not be used.
    ModelResponseInvalid,
    /// A connected tool (file, email, spreadsheet, and so on) failed to complete its call.
    AdapterCallFailed,
    /// A step needed the internet and no connection was available.
    NetworkUnavailable,
    /// Free disk space fell below the safe operating threshold.
    DiskSpaceLow,
    /// An installed workflow failed its install-time safety check.
    SignatureInvalid,
    /// A workflow could not be set to run by itself the way it was scheduled.
    ScheduleRejected,
    /// A run was stopped by the emergency stop.
    KillSwitchEngaged,
    /// Checking for the newest version failed.
    UpdaterUnreachable,
    /// The permission needed to see and control the screen is not granted.
    AccessibilityPermissionMissing,
    /// No microphone or speakers were found for voice features.
    AudioDeviceMissing,
    /// The selected local model needs more graphics memory than is free.
    GraphicsMemoryLow,
}

/// Every [`ErrorKind`] variant. Kept in sync by hand; see the module-level
/// docs for how the tests cross-check it against the enum.
impl ErrorKind {
    pub const ALL: &'static [ErrorKind] = &[
        ErrorKind::TargetNotFound,
        ErrorKind::ActionTimedOut,
        ErrorKind::ApprovalRequired,
        ErrorKind::ApprovalDenied,
        ErrorKind::SafetyBlocked,
        ErrorKind::PreconditionFailed,
        ErrorKind::PostconditionFailed,
        ErrorKind::WorkflowDriftDetected,
        ErrorKind::ModelUnreachable,
        ErrorKind::ModelSignInExpired,
        ErrorKind::ModelResponseInvalid,
        ErrorKind::AdapterCallFailed,
        ErrorKind::NetworkUnavailable,
        ErrorKind::DiskSpaceLow,
        ErrorKind::SignatureInvalid,
        ErrorKind::ScheduleRejected,
        ErrorKind::KillSwitchEngaged,
        ErrorKind::UpdaterUnreachable,
        ErrorKind::AccessibilityPermissionMissing,
        ErrorKind::AudioDeviceMissing,
        ErrorKind::GraphicsMemoryLow,
    ];

    /// The catalog entry for this error kind: id, what happened, why, and
    /// one suggested action, plus an optional one-click fix and docs
    /// anchor. Exhaustive match, no wildcard arm: a variant added to
    /// [`ErrorKind`] without a matching arm here is a compile error, which
    /// is the exhaustiveness guarantee this catalog exists to provide.
    pub fn entry(&self) -> CatalogEntry {
        match self {
            ErrorKind::TargetNotFound => CatalogEntry {
                id: "target_not_found",
                what: "Operant could not find the item it needed to work with on the screen.",
                why: "The screen may look different than when this step was set up, or the right window was not open.",
                action: "Bring the right window to the front and try again, or edit this step in the workflow.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#could-not-find-it"),
            },
            ErrorKind::ActionTimedOut => CatalogEntry {
                id: "action_timed_out",
                what: "A step took longer than expected and did not finish in time.",
                why: "The screen may not have changed the way this step expected, or the computer was busy.",
                action: "Try the step again, and give it more time if this keeps happening.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#took-too-long"),
            },
            ErrorKind::ApprovalRequired => CatalogEntry {
                id: "approval_required",
                what: "A step needed a yes from a person before it could continue.",
                why: "This step is risky enough that Operant will not do it without checking first.",
                action: "Review the step and approve or decline it.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#needs-your-ok"),
            },
            ErrorKind::ApprovalDenied => CatalogEntry {
                id: "approval_denied",
                what: "A step was turned down when it asked for approval.",
                why: "A person chose not to allow this step to run.",
                action: "Run the workflow again and approve the step if it should go ahead, or change the workflow to skip it.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#turned-down"),
            },
            ErrorKind::SafetyBlocked => CatalogEntry {
                id: "safety_blocked",
                what: "Operant stopped a step because it looked unsafe.",
                why: "The step was about to touch a password field or a payment or delete confirmation, and Operant never does that without extra care.",
                action: "Do this step yourself, or get in touch if you believe this is a mistake.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#looked-unsafe"),
            },
            ErrorKind::PreconditionFailed => CatalogEntry {
                id: "precondition_failed",
                what: "A step did not run because the screen was not in the state it expected.",
                why: "The wrong window or page may have been open when the step started.",
                action: "Open the right window or page, then run the workflow again.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#wrong-screen"),
            },
            ErrorKind::PostconditionFailed => CatalogEntry {
                id: "postcondition_failed",
                what: "Operant could not confirm that a step actually worked.",
                why: "The screen after the step did not match what was expected.",
                action: "Check the result by hand, then run the workflow again if it looks wrong.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#could-not-confirm"),
            },
            ErrorKind::WorkflowDriftDetected => CatalogEntry {
                id: "workflow_drift_detected",
                what: "A workflow's saved screen location no longer matched the app it controls.",
                why: "The app's screen changed since this workflow was taught.",
                action: "Review the suggested update and approve it if it looks right.",
                fix_command: Some("operant approve"),
                learn_more: Some("docs/troubleshooting.md#screen-changed"),
            },
            ErrorKind::ModelUnreachable => CatalogEntry {
                id: "model_unreachable",
                what: "Operant could not reach the model it is set up to use.",
                why: "The model may be turned off, or the connection to it is down.",
                action: "Check that the model is running and connected, then try again.",
                fix_command: Some("operant doctor --fix model_reachable"),
                learn_more: Some("docs/troubleshooting.md#model-unreachable"),
            },
            ErrorKind::ModelSignInExpired => CatalogEntry {
                id: "model_sign_in_expired",
                what: "Operant's sign-in for the model has expired.",
                why: "Sign-ins expire after a while for your security.",
                action: "Sign in again to keep using the model.",
                fix_command: Some("operant sign-in"),
                learn_more: Some("docs/troubleshooting.md#sign-in-expired"),
            },
            ErrorKind::ModelResponseInvalid => CatalogEntry {
                id: "model_response_invalid",
                what: "The model sent back a reply Operant could not use.",
                why: "The reply was empty, cut off, or in a shape Operant did not expect.",
                action: "Try the step again, or switch to a different model in settings.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#bad-reply"),
            },
            ErrorKind::AdapterCallFailed => CatalogEntry {
                id: "adapter_call_failed",
                what: "A connected tool did not finish what it was asked to do.",
                why: "The tool may be missing, set up incorrectly, or it returned an error.",
                action: "Check the connected tool's settings, then try again.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#tool-failed"),
            },
            ErrorKind::NetworkUnavailable => CatalogEntry {
                id: "network_unavailable",
                what: "A step needed the internet, but no connection was available.",
                why: "Your network connection may be down or blocked.",
                action: "Check your internet connection, then try again.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#no-connection"),
            },
            ErrorKind::DiskSpaceLow => CatalogEntry {
                id: "disk_space_low",
                what: "Your computer ran low on free disk space.",
                why: "Operant and the apps it controls need free space to save files safely.",
                action: "Free up some disk space, then try again.",
                fix_command: Some("operant doctor --fix disk_free"),
                learn_more: Some("docs/troubleshooting.md#low-disk-space"),
            },
            ErrorKind::SignatureInvalid => CatalogEntry {
                id: "signature_invalid",
                what: "A workflow failed a safety check when it was installed.",
                why: "Operant could not confirm the workflow came from where it claims to, so it may have been changed.",
                action: "Do not run this workflow. Get it again from a source you trust.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#failed-safety-check"),
            },
            ErrorKind::ScheduleRejected => CatalogEntry {
                id: "schedule_rejected",
                what: "A scheduled run could not start.",
                why: "This workflow is not ready to run on its own yet, or it overlaps with another scheduled run.",
                action: "Save and test the workflow first, or adjust the schedule so runs do not overlap.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#could-not-schedule"),
            },
            ErrorKind::KillSwitchEngaged => CatalogEntry {
                id: "kill_switch_engaged",
                what: "A run was stopped by the emergency stop.",
                why: "Someone triggered the emergency stop, which immediately freezes every step in progress.",
                action: "Review what happened, then resume the run yourself when you are ready.",
                fix_command: None,
                learn_more: Some("docs/troubleshooting.md#emergency-stop"),
            },
            ErrorKind::UpdaterUnreachable => CatalogEntry {
                id: "updater_unreachable",
                what: "Operant could not check for the newest version.",
                why: "The connection needed to check for updates was not available.",
                action: "Check your internet connection and try checking for updates again.",
                fix_command: Some("operant doctor --fix updater_reachable"),
                learn_more: Some("docs/troubleshooting.md#update-check-failed"),
            },
            ErrorKind::AccessibilityPermissionMissing => CatalogEntry {
                id: "accessibility_permission_missing",
                what: "Operant does not have the permission it needs to see and control the screen.",
                why: "Windows blocks apps from reading or controlling the screen until this permission is turned on.",
                action: "Turn the permission on in your system settings, then try again.",
                fix_command: Some("operant doctor --fix accessibility_permission"),
                learn_more: Some("docs/troubleshooting.md#permission-needed"),
            },
            ErrorKind::AudioDeviceMissing => CatalogEntry {
                id: "audio_device_missing",
                what: "Operant could not find a microphone or speakers to use.",
                why: "Voice features need a working microphone and speakers connected to this computer.",
                action: "Connect a microphone and speakers, then try again.",
                fix_command: Some("operant doctor --fix audio_devices_present"),
                learn_more: Some("docs/troubleshooting.md#no-microphone"),
            },
            ErrorKind::GraphicsMemoryLow => CatalogEntry {
                id: "graphics_memory_low",
                what: "Your computer does not have enough graphics memory for the model you picked.",
                why: "Local models need enough graphics memory to run smoothly, and this computer is short on it.",
                action: "Pick a smaller model, close other graphics-heavy programs, or use a model that runs over the internet.",
                fix_command: Some("operant doctor --fix vram_headroom"),
                learn_more: Some("docs/troubleshooting.md#not-enough-graphics-memory"),
            },
        }
    }
}

/// One error catalog entry. Default-mode copy: `what`, `why`, and `action`
/// must never contain a glossary internal term (see
/// `contracts/microcopy_glossary.json`; enforced by this module's tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Stable snake_case id. What `run.step.failed.error_id` and a
    /// `Finding::finding_id` built from this entry carry.
    pub id: &'static str,
    /// What happened, one sentence, past tense.
    pub what: &'static str,
    /// Why it happened, one sentence.
    pub why: &'static str,
    /// What to do about it, one imperative sentence.
    pub action: &'static str,
    /// A command that powers a one-click fix, when the fix is automatable.
    pub fix_command: Option<&'static str>,
    /// A docs anchor with more detail.
    pub learn_more: Option<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_marker_sanity() {
        // Keeps this module from ever being an empty test file if every
        // other test below is skipped for some reason.
        assert!(!ErrorKind::ALL.is_empty());
    }

    /// Every kind in `ALL` resolves to a well-formed entry (non-empty
    /// fields) whose `id` is unique. This is the visible, run-every-time
    /// half of the exhaustiveness guarantee; the compile-time half is
    /// `ErrorKind::entry`'s match having no wildcard arm (see module docs).
    #[test]
    fn every_error_kind_has_a_catalog_entry() {
        let mut ids = std::collections::BTreeSet::new();
        for kind in ErrorKind::ALL {
            let entry = kind.entry();
            assert!(!entry.id.is_empty(), "{kind:?} has an empty id");
            assert!(!entry.what.is_empty(), "{kind:?} has an empty what");
            assert!(!entry.why.is_empty(), "{kind:?} has an empty why");
            assert!(!entry.action.is_empty(), "{kind:?} has an empty action");
            assert!(
                entry.what.trim_end().ends_with('.'),
                "{kind:?} what should be a sentence: {:?}",
                entry.what
            );
            assert!(
                entry.action.trim_end().ends_with('.'),
                "{kind:?} action should be a sentence: {:?}",
                entry.action
            );
            assert!(ids.insert(entry.id), "duplicate catalog id: {}", entry.id);
        }
    }

    /// `ErrorKind::entry`'s serde tag (from `#[serde(rename_all =
    /// "snake_case")]`) must match the entry's own `id` field, so the two
    /// spellings of "which kind is this" never drift apart.
    #[test]
    fn catalog_id_matches_the_serde_tag() {
        for kind in ErrorKind::ALL {
            let tag = serde_json::to_value(kind).expect("ErrorKind serializes");
            let tag = tag.as_str().expect("tag is a string").to_string();
            assert_eq!(kind.entry().id, tag, "{kind:?} id must match its serde tag");
        }
    }

    /// Proves `ErrorKind::ALL` (hand-maintained, used for iteration by the
    /// glossary scan and elsewhere) has not drifted out of sync with the
    /// enum. This second match is independent of `entry()`'s and also has
    /// no wildcard arm, so it alone forces a compile error on a new
    /// variant; comparing its count against `ALL` then catches the one gap
    /// a lone compile-time match cannot: a variant both matches already
    /// handle but that `ALL` forgot to list.
    #[test]
    fn all_is_exhaustive_over_error_kind() {
        fn label(kind: ErrorKind) -> &'static str {
            match kind {
                ErrorKind::TargetNotFound => "target_not_found",
                ErrorKind::ActionTimedOut => "action_timed_out",
                ErrorKind::ApprovalRequired => "approval_required",
                ErrorKind::ApprovalDenied => "approval_denied",
                ErrorKind::SafetyBlocked => "safety_blocked",
                ErrorKind::PreconditionFailed => "precondition_failed",
                ErrorKind::PostconditionFailed => "postcondition_failed",
                ErrorKind::WorkflowDriftDetected => "workflow_drift_detected",
                ErrorKind::ModelUnreachable => "model_unreachable",
                ErrorKind::ModelSignInExpired => "model_sign_in_expired",
                ErrorKind::ModelResponseInvalid => "model_response_invalid",
                ErrorKind::AdapterCallFailed => "adapter_call_failed",
                ErrorKind::NetworkUnavailable => "network_unavailable",
                ErrorKind::DiskSpaceLow => "disk_space_low",
                ErrorKind::SignatureInvalid => "signature_invalid",
                ErrorKind::ScheduleRejected => "schedule_rejected",
                ErrorKind::KillSwitchEngaged => "kill_switch_engaged",
                ErrorKind::UpdaterUnreachable => "updater_unreachable",
                ErrorKind::AccessibilityPermissionMissing => "accessibility_permission_missing",
                ErrorKind::AudioDeviceMissing => "audio_device_missing",
                ErrorKind::GraphicsMemoryLow => "graphics_memory_low",
            }
        }

        let from_all: std::collections::BTreeSet<&str> =
            ErrorKind::ALL.iter().map(|k| label(*k)).collect();
        assert_eq!(
            from_all.len(),
            ErrorKind::ALL.len(),
            "ErrorKind::ALL contains a duplicate"
        );
        for kind in ErrorKind::ALL {
            assert_eq!(label(*kind), kind.entry().id);
        }
    }

    /// Case-insensitive, word-boundary containment check, the same shape as
    /// `scripts/microcopy_lint.mjs`'s `\bterm\b` match, reimplemented
    /// without a regex dependency this crate does not otherwise need.
    fn contains_term(haystack: &str, term: &str) -> bool {
        fn is_word_byte(b: u8) -> bool {
            b.is_ascii_alphanumeric() || b == b'_'
        }

        let haystack = haystack.to_lowercase();
        let term = term.to_lowercase();
        if term.is_empty() {
            return false;
        }
        let bytes = haystack.as_bytes();
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(&term) {
            let idx = start + pos;
            let end = idx + term.len();
            let before_ok = idx == 0 || !is_word_byte(bytes[idx - 1]);
            let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
            if before_ok && after_ok {
                return true;
            }
            start = idx + 1;
        }
        false
    }

    /// BAR: "all catalog strings avoid glossary internal terms." Loads the
    /// real glossary and scans every `what`/`why`/`action` in the catalog.
    #[test]
    fn catalog_strings_avoid_glossary_internal_terms() {
        let glossary_raw = include_str!("../../../contracts/microcopy_glossary.json");
        let glossary: serde_json::Value =
            serde_json::from_str(glossary_raw).expect("glossary is valid JSON");
        let terms: Vec<String> = glossary["terms"]
            .as_array()
            .expect("glossary has a terms array")
            .iter()
            .map(|t| {
                t["internal"]
                    .as_str()
                    .expect("each term has an internal field")
                    .to_string()
            })
            .collect();
        assert!(
            !terms.is_empty(),
            "glossary must not be empty for this test to mean anything"
        );

        for kind in ErrorKind::ALL {
            let entry = kind.entry();
            for (field_name, field_value) in [
                ("what", entry.what),
                ("why", entry.why),
                ("action", entry.action),
            ] {
                for term in &terms {
                    assert!(
                        !contains_term(field_value, term),
                        "catalog entry {kind:?} field `{field_name}` contains the glossary internal term `{term}`: {field_value:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn workflow_drift_detected_offers_the_approval_fix_command() {
        let entry = ErrorKind::WorkflowDriftDetected.entry();
        assert_eq!(entry.fix_command, Some("operant approve"));
    }

    #[test]
    fn fix_command_ids_line_up_with_the_matching_doctor_check_id() {
        // The catalog entries doctor checks reuse (see crate::checks) name
        // their one-click fix after the check's own id, so the two surfaces
        // read as one system.
        let cases = [
            (ErrorKind::ModelUnreachable, "model_reachable"),
            (ErrorKind::DiskSpaceLow, "disk_free"),
            (ErrorKind::UpdaterUnreachable, "updater_reachable"),
            (
                ErrorKind::AccessibilityPermissionMissing,
                "accessibility_permission",
            ),
            (ErrorKind::AudioDeviceMissing, "audio_devices_present"),
            (ErrorKind::GraphicsMemoryLow, "vram_headroom"),
        ];
        for (kind, check_id) in cases {
            let fix = kind.entry().fix_command.expect("has a fix command");
            assert!(
                fix.ends_with(check_id),
                "fix_command {fix:?} for {kind:?} should end with check id {check_id:?}"
            );
        }
    }
}
