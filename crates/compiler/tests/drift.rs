//! Drift repair (C8, FR-T4) end to end over the `drift_renamed_button`
//! fixture: the "Save invoice" button (id `save-btn`) is renamed to "Store
//! invoice" (id `store-btn`), so every stored selector for the old button
//! misses while the element keeps its role and tree slot.
//!
//! `before.json` is the snapshot the workflow was compiled against;
//! `after.json` is what replay meets. The tests walk the full loop: detect a
//! drift-eligible failure (not a wrong-state halt), re-ground to the new
//! selectors, build the patch diff, approve with a version bump and changelog
//! row, confirm the repaired step re-runs cleanly, and confirm a destructive
//! step is refused for auto-repair.

use operant_compiler::drift::{
    approve, attempt_repair, diagnose, make_patch, reground, reject, Diagnosis, PatchStatus,
    PendingPatches, Precondition, RepairOutcome, RepairSession,
};
use operant_compiler::CompiledWorkflow;
use operant_ir::snapshot::{Role, Snapshot};
use operant_ir::{Action, Manifest, Selector};
use serde_json::json;

const BEFORE: &str = include_str!("../../../contracts/fixtures/drift_renamed_button/before.json");
const AFTER: &str = include_str!("../../../contracts/fixtures/drift_renamed_button/after.json");

const WORKFLOW: &str = "invoice-saver";
/// Anchor similarity below the step anchor's tolerance: the recorded image no
/// longer matches, corroborating a real drift.
const ANCHOR_MISS: f64 = 0.2;
/// Anchor similarity above tolerance: the screen still matches the recorded
/// anchor.
const ANCHOR_HIT: f64 = 0.95;

fn before() -> Snapshot {
    serde_json::from_str(BEFORE).expect("before fixture parses")
}

fn after() -> Snapshot {
    serde_json::from_str(AFTER).expect("after fixture parses")
}

/// The precondition that still holds in `after.json`: the page title is
/// unchanged, which is what makes the failure a drift rather than a wrong
/// state.
fn precondition() -> Precondition {
    Precondition::WindowTitleEquals("Operant Fixture Invoices".to_string())
}

/// A click step targeting the old "Save invoice" button. It carries the
/// button's stable-identity selectors (automation id, css, role plus name
/// path); all three miss against `after.json`. It deliberately does not carry
/// the brittle ordinal-path fallback, so re-grounding, not the stored chain,
/// is what relocates the moved control.
fn save_step(risk_class: &str) -> Action {
    serde_json::from_value(json!({
        "v": 1,
        "id": "save-click",
        "kind": "click",
        "intent": "Save the invoice",
        "target": {
            "window": { "process": "fixture-webapp", "title_pattern": "Operant Fixture Invoices" },
            "selectors": [
                { "kind": "automation_id", "value": "save-btn" },
                { "kind": "css", "value": "#save-btn" },
                { "kind": "name_role_path", "path": [
                    { "role": "document", "name": "Operant Fixture Invoices" },
                    { "role": "button", "name": "Save invoice" }
                ]}
            ],
            "anchor": { "img_hash": "aaaaaaaaaaaaaaaa", "tolerance": 0.85 }
        },
        "risk_class": risk_class,
        "grounding": "uia"
    }))
    .expect("save step builds")
}

fn workflow(step: Action) -> CompiledWorkflow {
    let manifest: Manifest = serde_json::from_value(json!({
        "name": WORKFLOW,
        "version": "1.0.0",
        "description": "Saves an invoice.",
        "step_summary": ["Save the invoice"],
        "inputs_schema": { "type": "object", "properties": {}, "additionalProperties": false },
        "capabilities": {
            "apps": ["fixture-webapp"], "paths": [], "network": false, "risk_ceiling": "write"
        },
        "dsl": { "path": "workflow.ts", "hash": "deadbeef" }
    }))
    .expect("manifest builds");
    CompiledWorkflow {
        manifest,
        actions: vec![step],
    }
}

fn automation_id(selectors: &[Selector], value: &str) -> bool {
    selectors
        .iter()
        .any(|s| matches!(s, Selector::AutomationId { value: v } if v == value))
}

fn name_role_leaf(selectors: &[Selector], role: &str, name: &str) -> bool {
    selectors.iter().any(|s| match s {
        Selector::NameRolePath { path } => {
            path.last().map(|seg| (seg.role.as_str(), seg.name.as_str())) == Some((role, name))
        }
        _ => false,
    })
}

// ---- DETECT -----------------------------------------------------------------

#[test]
fn same_step_resolves_against_the_compile_time_snapshot() {
    // Sanity: the step is well formed. Against `before.json` its selectors
    // resolve, so it is not drifting there.
    let step = save_step("write");
    assert_eq!(
        diagnose(&step, &before(), 1.0, &precondition()),
        Diagnosis::Resolved
    );
}

#[test]
fn renamed_button_is_drift_eligible_not_wrong_state() {
    let step = save_step("write");
    // Every selector misses, the anchor is below tolerance, and the page-title
    // precondition still holds: a clean drift.
    let diag = diagnose(&step, &after(), ANCHOR_MISS, &precondition());
    assert_eq!(diag, Diagnosis::DriftEligible);
    assert!(diag.is_drift_eligible());

    // An element-existence precondition that holds in `after.json` reads the
    // same way.
    let holds = Precondition::ElementExists {
        role: Role::List,
        name: "Saved invoices".to_string(),
    };
    assert_eq!(
        diagnose(&step, &after(), ANCHOR_MISS, &holds),
        Diagnosis::DriftEligible
    );
}

#[test]
fn broken_precondition_is_a_wrong_state_halt_not_drift() {
    let step = save_step("write");
    // Same missing selectors and below-tolerance anchor, but the app is on the
    // wrong page: this must NOT be treated as drift.
    let wrong_page = Precondition::WindowTitleEquals("Some Other App".to_string());
    let diag = diagnose(&step, &after(), ANCHOR_MISS, &wrong_page);
    assert_eq!(diag, Diagnosis::WrongState);
    assert!(!diag.is_drift_eligible());
}

#[test]
fn a_matching_anchor_blocks_drift() {
    let step = save_step("write");
    // Selectors miss and the precondition holds, but the recorded anchor still
    // matches the screen. Detection requires both signals, so this is not a
    // confident drift.
    assert_eq!(
        diagnose(&step, &after(), ANCHOR_HIT, &precondition()),
        Diagnosis::WrongState
    );
}

// ---- RE-GROUND --------------------------------------------------------------

#[test]
fn reground_finds_the_renamed_button() {
    let step = save_step("write");
    let candidate = reground(&step, &before(), &after()).expect("re-grounds the moved button");

    assert_eq!(candidate.name, "Store invoice");
    // The re-grounded chain carries the new identity.
    assert!(automation_id(&candidate.selectors, "store-btn"));
    assert!(name_role_leaf(&candidate.selectors, "button", "Store invoice"));
    // The strongest selector leads (automation id, score 100).
    assert!(
        matches!(candidate.selectors.first(), Some(Selector::AutomationId { value }) if value == "store-btn"),
        "expected the automation-id selector to lead, got {:?}",
        candidate.selectors.first()
    );
    // A fresh anchor was captured, keeping the step's tolerance.
    let anchor = candidate.anchor.expect("a re-captured anchor");
    assert_eq!(anchor.tolerance, 0.85);
    assert_eq!(anchor.img_hash.len(), 64);
}

// ---- PATCH ------------------------------------------------------------------

#[test]
fn patch_diff_maps_old_selectors_to_new() {
    let step = save_step("write");
    let candidate = reground(&step, &before(), &after()).unwrap();
    let patch = make_patch(WORKFLOW, &step, &candidate);

    assert_eq!(patch.status, PatchStatus::Pending);
    assert_eq!(patch.workflow, WORKFLOW);
    assert_eq!(patch.diff.step_id, "save-click");

    // Old side: the missing save-btn identity.
    assert!(automation_id(&patch.diff.old_selectors, "save-btn"));
    assert!(name_role_leaf(&patch.diff.old_selectors, "button", "Save invoice"));
    // New side: the re-grounded store-btn identity.
    assert!(automation_id(&patch.diff.new_selectors, "store-btn"));
    assert!(name_role_leaf(&patch.diff.new_selectors, "button", "Store invoice"));

    // Both screenshot references are present and distinct.
    assert!(!patch.diff.before_screenshot.is_empty());
    assert!(!patch.diff.after_screenshot.is_empty());
    assert_ne!(patch.diff.before_screenshot, patch.diff.after_screenshot);

    // The anchor changed with the re-grounding.
    assert!(patch.diff.old_anchor.is_some());
    assert!(patch.diff.new_anchor.is_some());
    assert_ne!(patch.diff.old_anchor, patch.diff.new_anchor);

    // The plain-language offer names both the old and new control.
    assert!(patch.offer.contains("Save invoice"), "offer: {}", patch.offer);
    assert!(patch.offer.contains("Store invoice"), "offer: {}", patch.offer);
}

// ---- APPROVE / rerun --------------------------------------------------------

#[test]
fn approval_bumps_version_records_changelog_and_repairs_the_step() {
    let step = save_step("write");
    let mut session = RepairSession::new(workflow(step.clone()));

    // The run halts as "needs your OK": a pending patch, no merge yet.
    let outcome = attempt_repair(
        WORKFLOW,
        &step,
        &before(),
        &after(),
        ANCHOR_MISS,
        &precondition(),
        &mut session.pending,
    );
    let patch = match outcome {
        RepairOutcome::NeedsApproval(p) => p,
        other => panic!("expected NeedsApproval, got {other:?}"),
    };

    // Staging did not merge: version unchanged, step still targets save-btn.
    assert_eq!(session.workflow.manifest.version, "1.0.0");
    assert!(automation_id(
        &session.workflow.actions[0].target.as_ref().unwrap().selectors,
        "save-btn"
    ));
    assert!(session.versions.is_empty());

    // Approve the specific patch: version bumps, changelog gains one row.
    let new_version = approve(&mut session, &patch.id).expect("approves");
    assert_eq!(new_version, "1.0.1");
    assert_eq!(session.workflow.manifest.version, "1.0.1");
    assert_eq!(session.versions.len(), 1);
    assert_eq!(session.versions[0].version, "1.0.1");
    assert!(
        session.versions[0].diff_path.contains(&patch.id),
        "changelog diff path should reference the patch id: {}",
        session.versions[0].diff_path
    );

    // The patch is now merged; the pending store has no live patch left.
    assert_eq!(session.pending.pending().count(), 0);

    // Rerun: the repaired step now resolves against `after.json`.
    let repaired = &session.workflow.actions[0];
    assert!(automation_id(
        &repaired.target.as_ref().unwrap().selectors,
        "store-btn"
    ));
    assert_eq!(
        diagnose(repaired, &after(), ANCHOR_HIT, &precondition()),
        Diagnosis::Resolved,
        "the repaired step should resolve cleanly on rerun"
    );
}

#[test]
fn rejection_archives_the_patch_without_touching_the_version() {
    let step = save_step("write");
    let candidate = reground(&step, &before(), &after()).unwrap();
    let patch = make_patch(WORKFLOW, &step, &candidate);

    let mut session = RepairSession::new(workflow(step));
    session.pending.push(patch.clone());

    reject(&mut session, &patch.id).expect("rejects");

    assert_eq!(session.workflow.manifest.version, "1.0.0");
    assert!(session.versions.is_empty());
    assert_eq!(session.pending.pending().count(), 0);
    assert_eq!(session.pending.patches[0].status, PatchStatus::Rejected);
}

// ---- destructive refusal ----------------------------------------------------

#[test]
fn destructive_step_is_refused_for_auto_repair() {
    let step = save_step("destructive");
    // Every drift signal fires, but a destructive step is never auto-repaired.
    assert_eq!(
        diagnose(&step, &after(), ANCHOR_MISS, &precondition()),
        Diagnosis::RefusedDestructive
    );

    let mut pending = PendingPatches::default();
    let outcome = attempt_repair(
        WORKFLOW,
        &step,
        &before(),
        &after(),
        ANCHOR_MISS,
        &precondition(),
        &mut pending,
    );
    match outcome {
        RepairOutcome::Halt(reason) => assert!(
            reason.contains("destructive"),
            "halt reason should name the destructive refusal: {reason}"
        ),
        other => panic!("expected Halt, got {other:?}"),
    }
    // No patch is ever staged for a destructive step.
    assert!(
        pending.patches.is_empty(),
        "a destructive step must not stage a patch"
    );
}
