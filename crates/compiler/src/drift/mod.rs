//! Drift repair (C8, FR-T4): the full detect, re-ground, patch, approve loop.
//!
//! When a compiled workflow is replayed and a step's target has moved under
//! it, drift repair turns a hard failure into a reviewable offer. The loop,
//! per `docs/specs/drift.md`:
//!
//! 1. DETECT ([`detect::diagnose`]): during a replay step, classify the
//!    failure. A step is drift-eligible only when every stored selector misses
//!    AND the recorded anchor is below tolerance, WHILE the step's
//!    precondition still holds. A precondition that no longer holds is a
//!    wrong-state failure and halts normally, not a drift.
//! 2. RE-GROUND ([`reground::reground`]): single-step re-ground the target
//!    against the current snapshot, matching by role plus stable position when
//!    id and name changed, producing a candidate selector chain plus anchor.
//! 3. PATCH ([`make_patch`]): write the old-to-new diff, with before and after
//!    screenshot references, into the workflow's pending-patches store. The
//!    run halts as "needs your OK", not "failed".
//! 4. APPROVE ([`patch::approve`]): bump the workflow version, append a
//!    `workflow_versions` changelog row with the diff path, and merge the
//!    re-grounded target. [`patch::reject`] archives the patch instead.
//!
//! Never: auto-merge, multi-step repair in one patch, or repair of a
//! destructive-risk step (those always halt to a human).

mod detect;
mod patch;
mod reground;
mod resolve;

pub use detect::{diagnose, Diagnosis, Precondition};
pub use patch::{
    approve, reject, DriftError, Patch, PatchStatus, PendingPatches, RepairSession, SelectorDiff,
    WorkflowVersion,
};
pub use reground::{reground, Candidate};

use operant_ir::{Action, Selector};
use operant_ir::snapshot::Snapshot;

/// The outcome of one drift-repair attempt on a failed replay step.
///
/// The `NeedsApproval` variant carries a full [`Patch`] and so dwarfs the
/// others, but this is a transient, one-at-a-time control-flow result rather
/// than something held in bulk, so the size spread does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum RepairOutcome {
    /// The step's target still resolves; there is nothing to repair.
    NoDrift,
    /// The step cannot be auto-repaired and the run halts. Carries the reason:
    /// a wrong-state failure, a destructive step refused for auto-repair, or a
    /// drift whose target could not be re-grounded to a unique slot.
    Halt(String),
    /// The step drifted and was re-grounded into a pending patch. The run
    /// halts as "needs your OK"; the patch is staged for human approval.
    NeedsApproval(Patch),
}

/// Run the drift loop for one replay step: diagnose, and on a clean drift,
/// re-ground and stage a pending patch. Never merges anything; on approval the
/// caller invokes [`patch::approve`] with the returned patch's id.
///
/// `before` is the snapshot the workflow was compiled against and `current` is
/// what replay meets. `anchor_similarity` is the measured match of the step's
/// recorded anchor against the current screen. A destructive step that is
/// otherwise drift-eligible returns [`RepairOutcome::Halt`] with no patch:
/// those always halt to a human.
#[allow(clippy::too_many_arguments)]
pub fn attempt_repair(
    workflow_name: &str,
    step: &Action,
    before: &Snapshot,
    current: &Snapshot,
    anchor_similarity: f64,
    precondition: &Precondition,
    pending: &mut PendingPatches,
) -> RepairOutcome {
    match diagnose(step, current, anchor_similarity, precondition) {
        Diagnosis::Resolved => RepairOutcome::NoDrift,
        Diagnosis::WrongState => RepairOutcome::Halt(
            "precondition does not hold; wrong-state failure halts normally".to_string(),
        ),
        Diagnosis::RefusedDestructive => RepairOutcome::Halt(
            "destructive-risk step refused for auto-repair; halting to a human".to_string(),
        ),
        Diagnosis::DriftEligible => match reground(step, before, current) {
            Some(candidate) => {
                let patch = make_patch(workflow_name, step, &candidate);
                pending.push(patch.clone());
                RepairOutcome::NeedsApproval(patch)
            }
            None => RepairOutcome::Halt(
                "drift detected but re-grounding found no stable match; halting to a human"
                    .to_string(),
            ),
        },
    }
}

/// Build a pending [`Patch`] from a re-ground [`Candidate`]: the old-to-new
/// selector and anchor diff, screenshot references, a stable id, and the
/// plain-language offer. Building a patch never changes the workflow.
pub fn make_patch(workflow_name: &str, step: &Action, candidate: &Candidate) -> Patch {
    let old_target = step.target.as_ref();
    let old_selectors = old_target.map(|t| t.selectors.clone()).unwrap_or_default();
    let old_anchor = old_target.and_then(|t| t.anchor.clone());
    let id = patch_id(workflow_name, step, candidate);

    let diff = SelectorDiff {
        step_id: step.id.clone(),
        old_selectors,
        new_selectors: candidate.selectors.clone(),
        old_anchor,
        new_anchor: candidate.anchor.clone(),
        before_screenshot: format!("pending-patches/{workflow_name}/{id}/before.png"),
        after_screenshot: format!("pending-patches/{workflow_name}/{id}/after.png"),
    };

    Patch {
        id,
        workflow: workflow_name.to_string(),
        offer: offer_text(step, candidate),
        diff,
        status: PatchStatus::Pending,
    }
}

/// A stable patch id, unique per workflow, step, and re-grounded identity, so
/// re-detecting the same drift yields the same id rather than piling up
/// duplicates in the pending store.
fn patch_id(workflow_name: &str, step: &Action, candidate: &Candidate) -> String {
    let seed = format!("{}|{}|{}", workflow_name, step.id, candidate.name);
    let short = &blake3::hash(seed.as_bytes()).to_hex()[..8];
    format!("patch-{}-{}", step.id, short)
}

/// The plain-language offer the approval card shows, naming the old control and
/// where it moved: `The "Save invoice" button moved (now "Store invoice"). Update the workflow?`.
fn offer_text(step: &Action, candidate: &Candidate) -> String {
    match old_label(step) {
        Some((role, name)) => format!(
            "The \"{name}\" {role} moved (now \"{}\"). Update the workflow?",
            candidate.name
        ),
        None => format!(
            "A step target moved (now \"{}\"). Update the workflow?",
            candidate.name
        ),
    }
}

/// Recover a human label (role, name) for the old target from the step's
/// name-role-path selector leaf, used only for the offer wording.
fn old_label(step: &Action) -> Option<(String, String)> {
    step.target.as_ref()?.selectors.iter().find_map(|s| match s {
        Selector::NameRolePath { path } => {
            path.last().map(|seg| (seg.role.clone(), seg.name.clone()))
        }
        _ => None,
    })
}
