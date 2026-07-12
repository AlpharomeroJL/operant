//! Strongly-typed constructors for the bus topic payloads documented in
//! `contracts/bus_events.md`. Scope: the runs, gates/approvals/escalations,
//! sidecars/VRAM, and guardian families. Publishers use these instead of
//! hand-building `serde_json::Value` so a typo in a field name or topic string
//! is a compile error, not a silent drift from the contract.
//!
//! Every type here implements [`BusEvent`], which pins the payload to its exact
//! topic string. Call [`crate::bus::Bus::publish_event`] instead of
//! `Bus::publish` to use them.
//!
//! Field shapes mirror the contract exactly. Where the contract points at an
//! existing IR type (`Action IR`, `gate_kind`, `result`, `grounding`) these
//! reuse `operant-ir`'s types rather than redeclaring them, so there is one
//! source of truth. New OPTIONAL fields may be added later per the contract's
//! versioning rules; nothing here is renamed or removed.

use operant_ir::{Action, GateKind, GateResult, Grounding};
use serde::{Deserialize, Serialize};

/// A bus payload bound to the exact topic it publishes under.
pub trait BusEvent: Serialize {
    /// Dot-separated topic string from `contracts/bus_events.md`.
    const TOPIC: &'static str;
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

/// Mode a run executes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Explore,
    Replay,
    Dry,
}

/// `run.started`: run_id, goal, mode, workflow_name?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStarted {
    pub run_id: String,
    pub goal: String,
    pub mode: RunMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
}
impl BusEvent for RunStarted {
    const TOPIC: &'static str = "run.started";
}

/// `run.step.proposed`: run_id, step (Action IR object). Explore only, pre-gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStepProposed {
    pub run_id: String,
    pub step: Action,
}
impl BusEvent for RunStepProposed {
    const TOPIC: &'static str = "run.step.proposed";
}

/// `run.step.gated`: run_id, step_id, gate_kind, result, expr?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStepGated {
    pub run_id: String,
    pub step_id: String,
    pub gate_kind: GateKind,
    pub result: GateResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<serde_json::Value>,
}
impl BusEvent for RunStepGated {
    const TOPIC: &'static str = "run.step.gated";
}

/// Outcome of an executed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepOutcome {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "retried")]
    Retried,
}

/// `run.step.executed`: run_id, step_id, outcome, ms, grounding
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStepExecuted {
    pub run_id: String,
    pub step_id: String,
    pub outcome: StepOutcome,
    pub ms: u64,
    pub grounding: Grounding,
}
impl BusEvent for RunStepExecuted {
    const TOPIC: &'static str = "run.step.executed";
}

/// `run.step.failed`: run_id, step_id, error_id, message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStepFailed {
    pub run_id: String,
    pub step_id: String,
    pub error_id: String,
    pub message: String,
}
impl BusEvent for RunStepFailed {
    const TOPIC: &'static str = "run.step.failed";
}

/// Who paused a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PausedBy {
    Human,
    System,
}

/// `run.paused`: run_id, by
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPaused {
    pub run_id: String,
    pub by: PausedBy,
}
impl BusEvent for RunPaused {
    const TOPIC: &'static str = "run.paused";
}

/// `run.redirected`: run_id, instruction. HITL natural-language redirect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRedirected {
    pub run_id: String,
    pub instruction: String,
}
impl BusEvent for RunRedirected {
    const TOPIC: &'static str = "run.redirected";
}

/// `run.resumed`: run_id
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResumed {
    pub run_id: String,
}
impl BusEvent for RunResumed {
    const TOPIC: &'static str = "run.resumed";
}

/// Why a run halted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HaltReason {
    Gate,
    Killswitch,
    Human,
    Error,
}

/// `run.halted`: run_id, reason, error_id?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunHalted {
    pub run_id: String,
    pub reason: HaltReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_id: Option<String>,
}
impl BusEvent for RunHalted {
    const TOPIC: &'static str = "run.halted";
}

/// Final outcome of a completed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunOutcome {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "failed")]
    Failed,
}

/// `run.completed`: run_id, outcome, steps, wall_ms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunCompleted {
    pub run_id: String,
    pub outcome: RunOutcome,
    pub steps: u32,
    pub wall_ms: u64,
}
impl BusEvent for RunCompleted {
    const TOPIC: &'static str = "run.completed";
}

// ---------------------------------------------------------------------------
// Gates, approvals, escalations
// ---------------------------------------------------------------------------

/// `gate.escalation`: run_id, step_id?, sentence, requires_approval. Sentence
/// is plain language, per the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateEscalation {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    pub sentence: String,
    pub requires_approval: bool,
}
impl BusEvent for GateEscalation {
    const TOPIC: &'static str = "gate.escalation";
}

/// `approval.requested`: approval_id, run_id, step_id?, proposed_action, sentence
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequested {
    pub approval_id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    pub proposed_action: Action,
    pub sentence: String,
}
impl BusEvent for ApprovalRequested {
    const TOPIC: &'static str = "approval.requested";
}

/// `approval.granted`: approval_id, approver. Recorded in the audit chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalGranted {
    pub approval_id: String,
    pub approver: String,
}
impl BusEvent for ApprovalGranted {
    const TOPIC: &'static str = "approval.granted";
}

/// `approval.denied`: approval_id, approver
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalDenied {
    pub approval_id: String,
    pub approver: String,
}
impl BusEvent for ApprovalDenied {
    const TOPIC: &'static str = "approval.denied";
}

// ---------------------------------------------------------------------------
// Sidecars and VRAM
// ---------------------------------------------------------------------------

/// `sidecar.started`: name, pid
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarStarted {
    pub name: String,
    pub pid: u32,
}
impl BusEvent for SidecarStarted {
    const TOPIC: &'static str = "sidecar.started";
}

/// `sidecar.health`: name, ok, rss_mb?, vram_mb?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarHealth {
    pub name: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
}
impl BusEvent for SidecarHealth {
    const TOPIC: &'static str = "sidecar.health";
}

/// `sidecar.crashed`: name, exit_code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarCrashed {
    pub name: String,
    pub exit_code: i32,
}
impl BusEvent for SidecarCrashed {
    const TOPIC: &'static str = "sidecar.crashed";
}

/// `sidecar.restarted`: name, attempt. Watchdog-driven.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarRestarted {
    pub name: String,
    pub attempt: u32,
}
impl BusEvent for SidecarRestarted {
    const TOPIC: &'static str = "sidecar.restarted";
}

/// `vram.request`: requester, mb. Broker arbitration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VramRequest {
    pub requester: String,
    pub mb: u64,
}
impl BusEvent for VramRequest {
    const TOPIC: &'static str = "vram.request";
}

/// `vram.grant`: requester, mb
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VramGrant {
    pub requester: String,
    pub mb: u64,
}
impl BusEvent for VramGrant {
    const TOPIC: &'static str = "vram.grant";
}

/// `vram.yield`: yielder, mb. E.g. voice yields to the vision grounder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VramYield {
    pub yielder: String,
    pub mb: u64,
}
impl BusEvent for VramYield {
    const TOPIC: &'static str = "vram.yield";
}

// ---------------------------------------------------------------------------
// Guardian
// ---------------------------------------------------------------------------

/// `killswitch.engaged`: at_ms. Tray red; all input synthesis frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KillswitchEngaged {
    pub at_ms: u64,
}
impl BusEvent for KillswitchEngaged {
    const TOPIC: &'static str = "killswitch.engaged";
}

/// `killswitch.released`: run_id?. Explicit human resume, per run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KillswitchReleased {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}
impl BusEvent for KillswitchReleased {
    const TOPIC: &'static str = "killswitch.released";
}

/// One journal item as carried on the wire (`undo.previewed`'s optional
/// `items` field, F1b): mirrors `operant_recorder::undo`'s internal
/// `Inverse` enum closely enough for a subscriber to reproduce that module's
/// own `preview_line`/`applied_line` wording, while deliberately dropping
/// what a subscriber must never see: blob hashes (an internal storage
/// detail) and actual clipboard contents. `RestoreClipboard` carries only
/// `had_prior`, whether a prior value existed, never what it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UndoInverseWire {
    DeleteCreated { path: String },
    RecreateDeleted { path: String },
    ReverseMove { moved_to: String, original: String },
    RestoreOverwritten { path: String },
    RestoreClipboard { had_prior: bool },
    Irreversible { description: String },
}

/// One `undo_journal` row as carried on the wire: the journal sequence
/// number plus its inverse. `#[serde(flatten)]` places `op` and the
/// inverse's own fields alongside `seq` in one JSON object, e.g.
/// `{"seq": 6, "op": "restore_clipboard", "had_prior": true}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoJournalItemWire {
    pub seq: u32,
    #[serde(flatten)]
    pub inverse: UndoInverseWire,
}

/// `undo.previewed`: run_id, entries (count), irreversible (count), items?
/// (F1b: the real per-item restoration list, newest-first, in the same
/// dry-run scope as `operant_recorder::Recorder::preview_undo`). `items` is
/// an optional field added per the contract's append-only rule: omitted
/// rather than serialized as an empty array when a publisher has none to
/// report, so a consumer reading only the counts is unaffected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoPreviewed {
    pub run_id: String,
    pub entries: u32,
    pub irreversible: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<UndoJournalItemWire>,
}
impl BusEvent for UndoPreviewed {
    const TOPIC: &'static str = "undo.previewed";
}

/// `undo.applied`: run_id, restored (count), narration (array of sentences)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoApplied {
    pub run_id: String,
    pub restored: u32,
    pub narration: Vec<String>,
}
impl BusEvent for UndoApplied {
    const TOPIC: &'static str = "undo.applied";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_match_contract() {
        assert_eq!(RunStarted::TOPIC, "run.started");
        assert_eq!(RunStepProposed::TOPIC, "run.step.proposed");
        assert_eq!(RunStepGated::TOPIC, "run.step.gated");
        assert_eq!(RunStepExecuted::TOPIC, "run.step.executed");
        assert_eq!(RunStepFailed::TOPIC, "run.step.failed");
        assert_eq!(RunPaused::TOPIC, "run.paused");
        assert_eq!(RunRedirected::TOPIC, "run.redirected");
        assert_eq!(RunResumed::TOPIC, "run.resumed");
        assert_eq!(RunHalted::TOPIC, "run.halted");
        assert_eq!(RunCompleted::TOPIC, "run.completed");

        assert_eq!(GateEscalation::TOPIC, "gate.escalation");
        assert_eq!(ApprovalRequested::TOPIC, "approval.requested");
        assert_eq!(ApprovalGranted::TOPIC, "approval.granted");
        assert_eq!(ApprovalDenied::TOPIC, "approval.denied");

        assert_eq!(SidecarStarted::TOPIC, "sidecar.started");
        assert_eq!(SidecarHealth::TOPIC, "sidecar.health");
        assert_eq!(SidecarCrashed::TOPIC, "sidecar.crashed");
        assert_eq!(SidecarRestarted::TOPIC, "sidecar.restarted");
        assert_eq!(VramRequest::TOPIC, "vram.request");
        assert_eq!(VramGrant::TOPIC, "vram.grant");
        assert_eq!(VramYield::TOPIC, "vram.yield");

        assert_eq!(KillswitchEngaged::TOPIC, "killswitch.engaged");
        assert_eq!(KillswitchReleased::TOPIC, "killswitch.released");
        assert_eq!(UndoPreviewed::TOPIC, "undo.previewed");
        assert_eq!(UndoApplied::TOPIC, "undo.applied");
    }

    fn sample_action() -> Action {
        serde_json::from_value(serde_json::json!({
            "v": 1, "id": "s1", "kind": "key",
            "params": {"combo": "ctrl+s"},
            "risk_class": "write", "grounding": "uia"
        }))
        .expect("sample action parses")
    }

    /// Every field name and enum spelling here is the contract, so a serde
    /// round trip through `serde_json::Value` is the regression guard against
    /// silent drift.
    #[test]
    fn runs_family_roundtrips() {
        let events: Vec<serde_json::Value> = vec![
            serde_json::to_value(RunStarted {
                run_id: "r1".into(),
                goal: "test".into(),
                mode: RunMode::Explore,
                workflow_name: None,
            })
            .unwrap(),
            serde_json::to_value(RunStepProposed {
                run_id: "r1".into(),
                step: sample_action(),
            })
            .unwrap(),
            serde_json::to_value(RunStepGated {
                run_id: "r1".into(),
                step_id: "s1".into(),
                gate_kind: GateKind::Pre,
                result: GateResult::Pass,
                expr: None,
            })
            .unwrap(),
            serde_json::to_value(RunStepExecuted {
                run_id: "r1".into(),
                step_id: "s1".into(),
                outcome: StepOutcome::Ok,
                ms: 42,
                grounding: Grounding::Uia,
            })
            .unwrap(),
            serde_json::to_value(RunStepFailed {
                run_id: "r1".into(),
                step_id: "s1".into(),
                error_id: "E_TIMEOUT".into(),
                message: "timed out".into(),
            })
            .unwrap(),
            serde_json::to_value(RunPaused {
                run_id: "r1".into(),
                by: PausedBy::Human,
            })
            .unwrap(),
            serde_json::to_value(RunRedirected {
                run_id: "r1".into(),
                instruction: "try again".into(),
            })
            .unwrap(),
            serde_json::to_value(RunResumed {
                run_id: "r1".into(),
            })
            .unwrap(),
            serde_json::to_value(RunHalted {
                run_id: "r1".into(),
                reason: HaltReason::Killswitch,
                error_id: None,
            })
            .unwrap(),
            serde_json::to_value(RunCompleted {
                run_id: "r1".into(),
                outcome: RunOutcome::Ok,
                steps: 6,
                wall_ms: 1234,
            })
            .unwrap(),
        ];
        for v in events {
            assert!(
                v.is_object(),
                "payload must serialize to a JSON object: {v}"
            );
        }
        assert_eq!(
            serde_json::to_value(RunMode::Dry).unwrap(),
            serde_json::json!("dry")
        );
        assert_eq!(
            serde_json::to_value(StepOutcome::Retried).unwrap(),
            serde_json::json!("retried")
        );
        assert_eq!(
            serde_json::to_value(HaltReason::Killswitch).unwrap(),
            serde_json::json!("killswitch")
        );
    }

    #[test]
    fn gates_family_roundtrips() {
        let escalation = GateEscalation {
            run_id: "r1".into(),
            step_id: Some("s1".into()),
            sentence: "This looks risky.".into(),
            requires_approval: true,
        };
        let v = serde_json::to_value(&escalation).unwrap();
        let back: GateEscalation = serde_json::from_value(v).unwrap();
        assert_eq!(escalation, back);

        let requested = ApprovalRequested {
            approval_id: "a1".into(),
            run_id: "r1".into(),
            step_id: None,
            proposed_action: sample_action(),
            sentence: "Delete the file?".into(),
        };
        let v = serde_json::to_value(&requested).unwrap();
        let back: ApprovalRequested = serde_json::from_value(v).unwrap();
        assert_eq!(requested, back);
        assert!(!v_step_id_present(&requested));

        let granted = ApprovalGranted {
            approval_id: "a1".into(),
            approver: "josef".into(),
        };
        assert_eq!(
            serde_json::from_value::<ApprovalGranted>(serde_json::to_value(&granted).unwrap())
                .unwrap(),
            granted
        );
        let denied = ApprovalDenied {
            approval_id: "a1".into(),
            approver: "josef".into(),
        };
        assert_eq!(
            serde_json::from_value::<ApprovalDenied>(serde_json::to_value(&denied).unwrap())
                .unwrap(),
            denied
        );
    }

    fn v_step_id_present(r: &ApprovalRequested) -> bool {
        // Helper purely to exercise the Option in a second way than direct eq.
        r.step_id.is_some()
    }

    #[test]
    fn sidecars_and_vram_family_roundtrips() {
        let started = SidecarStarted {
            name: "vision".into(),
            pid: 4242,
        };
        assert_eq!(
            serde_json::from_value::<SidecarStarted>(serde_json::to_value(&started).unwrap())
                .unwrap(),
            started
        );

        let health = SidecarHealth {
            name: "vision".into(),
            ok: true,
            rss_mb: Some(512),
            vram_mb: Some(2048),
        };
        let v = serde_json::to_value(&health).unwrap();
        assert_eq!(v["rss_mb"], serde_json::json!(512));
        assert_eq!(serde_json::from_value::<SidecarHealth>(v).unwrap(), health);

        let health_minimal = SidecarHealth {
            name: "voice".into(),
            ok: false,
            rss_mb: None,
            vram_mb: None,
        };
        let v = serde_json::to_value(&health_minimal).unwrap();
        assert!(
            v.get("rss_mb").is_none(),
            "optional fields must be omitted, not null"
        );

        let crashed = SidecarCrashed {
            name: "vision".into(),
            exit_code: -1,
        };
        assert_eq!(
            serde_json::from_value::<SidecarCrashed>(serde_json::to_value(&crashed).unwrap())
                .unwrap(),
            crashed
        );

        let restarted = SidecarRestarted {
            name: "vision".into(),
            attempt: 1,
        };
        assert_eq!(
            serde_json::from_value::<SidecarRestarted>(serde_json::to_value(&restarted).unwrap())
                .unwrap(),
            restarted
        );

        let request = VramRequest {
            requester: "vision".into(),
            mb: 4000,
        };
        let grant = VramGrant {
            requester: "vision".into(),
            mb: 4000,
        };
        let yield_ = VramYield {
            yielder: "vision".into(),
            mb: 4000,
        };
        assert_eq!(
            serde_json::from_value::<VramRequest>(serde_json::to_value(&request).unwrap()).unwrap(),
            request
        );
        assert_eq!(
            serde_json::from_value::<VramGrant>(serde_json::to_value(&grant).unwrap()).unwrap(),
            grant
        );
        assert_eq!(
            serde_json::from_value::<VramYield>(serde_json::to_value(&yield_).unwrap()).unwrap(),
            yield_
        );
    }

    #[test]
    fn guardian_family_roundtrips() {
        let engaged = KillswitchEngaged { at_ms: 99 };
        assert_eq!(
            serde_json::from_value::<KillswitchEngaged>(serde_json::to_value(engaged).unwrap())
                .unwrap(),
            engaged
        );

        let released = KillswitchReleased {
            run_id: Some("r1".into()),
        };
        assert_eq!(
            serde_json::from_value::<KillswitchReleased>(serde_json::to_value(&released).unwrap())
                .unwrap(),
            released
        );

        let previewed = UndoPreviewed {
            run_id: "r1".into(),
            entries: 3,
            irreversible: 1,
            items: Vec::new(),
        };
        assert_eq!(
            serde_json::from_value::<UndoPreviewed>(serde_json::to_value(&previewed).unwrap())
                .unwrap(),
            previewed
        );

        let applied = UndoApplied {
            run_id: "r1".into(),
            restored: 2,
            narration: vec!["Restored config.json from the recycle bin.".into()],
        };
        assert_eq!(
            serde_json::from_value::<UndoApplied>(serde_json::to_value(&applied).unwrap()).unwrap(),
            applied
        );
    }

    /// F1b: `items` is optional and additive. An empty list is omitted
    /// entirely from the JSON (not serialized as `"items":[]`), a populated
    /// list round-trips every variant with the exact wire field names the
    /// contract documents, and JSON from an older publisher with no `items`
    /// key at all still deserializes (`#[serde(default)]`), so this field
    /// cannot break an existing consumer.
    #[test]
    fn undo_previewed_items_roundtrip_and_omit_when_empty() {
        let empty = UndoPreviewed {
            run_id: "r1".into(),
            entries: 0,
            irreversible: 0,
            items: Vec::new(),
        };
        let v = serde_json::to_value(&empty).unwrap();
        assert!(v.get("items").is_none(), "empty items must be omitted, not serialized as []");

        let old_shape = serde_json::json!({ "run_id": "r1", "entries": 0, "irreversible": 0 });
        assert_eq!(
            serde_json::from_value::<UndoPreviewed>(old_shape).unwrap(),
            empty,
            "JSON from a publisher that predates items must still deserialize"
        );

        let populated = UndoPreviewed {
            run_id: "r2".into(),
            entries: 6,
            irreversible: 1,
            items: vec![
                UndoJournalItemWire { seq: 6, inverse: UndoInverseWire::RestoreClipboard { had_prior: true } },
                UndoJournalItemWire {
                    seq: 5,
                    inverse: UndoInverseWire::Irreversible {
                        description: "sent the invoice email to boss@example.com".into(),
                    },
                },
                UndoJournalItemWire {
                    seq: 4,
                    inverse: UndoInverseWire::DeleteCreated { path: "receipt.txt".into() },
                },
                UndoJournalItemWire {
                    seq: 3,
                    inverse: UndoInverseWire::RestoreOverwritten { path: "invoice.txt".into() },
                },
                UndoJournalItemWire {
                    seq: 2,
                    inverse: UndoInverseWire::ReverseMove {
                        moved_to: "Archive/draft.txt".into(),
                        original: "draft.txt".into(),
                    },
                },
                UndoJournalItemWire {
                    seq: 1,
                    inverse: UndoInverseWire::RecreateDeleted { path: "old_notes.txt".into() },
                },
            ],
        };
        let v = serde_json::to_value(&populated).unwrap();
        assert_eq!(v["items"][0]["seq"], serde_json::json!(6));
        assert_eq!(v["items"][0]["op"], serde_json::json!("restore_clipboard"));
        assert_eq!(v["items"][0]["had_prior"], serde_json::json!(true));
        assert_eq!(v["items"][2]["op"], serde_json::json!("delete_created"));
        assert_eq!(v["items"][2]["path"], serde_json::json!("receipt.txt"));
        assert_eq!(v["items"][4]["op"], serde_json::json!("reverse_move"));
        assert_eq!(v["items"][4]["moved_to"], serde_json::json!("Archive/draft.txt"));
        assert_eq!(
            serde_json::from_value::<UndoPreviewed>(v).unwrap(),
            populated,
            "a populated items list must round-trip exactly"
        );
    }
}
