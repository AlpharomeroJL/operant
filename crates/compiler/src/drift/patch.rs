//! Pending patches, approval, rejection, and the workflow-version changelog.
//!
//! A drift repair never merges itself. Detection and re-grounding stage a
//! [`Patch`] into a [`PendingPatches`] store and the run halts as "needs your
//! OK". A human then calls [`approve`], which applies the new selectors and
//! anchor to the step, bumps the workflow version, and appends a
//! [`WorkflowVersion`] changelog row pointing at the diff. [`reject`] archives
//! the patch and leaves the workflow and its version untouched.

use operant_ir::{Anchor, Selector, Target};

use crate::CompiledWorkflow;

/// The selector and anchor delta a repair proposes for one step, carrying the
/// before and after screenshot references the approval card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorDiff {
    /// The id of the step this diff repairs.
    pub step_id: String,
    /// The step's selectors before repair (the ones that missed).
    pub old_selectors: Vec<Selector>,
    /// The re-grounded selectors that replace them.
    pub new_selectors: Vec<Selector>,
    /// The step's anchor before repair, if any.
    pub old_anchor: Option<Anchor>,
    /// The re-captured anchor, if the step carried one.
    pub new_anchor: Option<Anchor>,
    /// Path reference to the screenshot of the old target.
    pub before_screenshot: String,
    /// Path reference to the screenshot of the re-grounded target.
    pub after_screenshot: String,
}

/// Where a patch sits in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchStatus {
    /// Awaiting a human decision.
    Pending,
    /// Approved and merged into the workflow.
    Approved,
    /// Rejected and archived.
    Rejected,
}

/// A pending drift patch: one step's diff, a plain-language offer, and its
/// status. Held in the workflow's [`PendingPatches`] store until a human
/// decides. Producing one never changes the workflow; only [`approve`] does.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// Stable id, unique per (workflow, step, re-grounded identity).
    pub id: String,
    /// The workflow name this patch belongs to.
    pub workflow: String,
    /// The plain-language offer, for example
    /// `The "Save invoice" button moved (now "Store invoice"). Update the workflow?`.
    pub offer: String,
    /// The selector and anchor delta.
    pub diff: SelectorDiff,
    /// Life-cycle status.
    pub status: PatchStatus,
}

impl Patch {
    /// The on-disk diff path recorded in the changelog on approval.
    pub fn diff_path(&self) -> String {
        format!("pending-patches/{}/{}.diff.json", self.workflow, self.id)
    }
}

/// The workflow's pending-patches store: the patches awaiting a human
/// decision. This is where a drift repair parks itself instead of failing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PendingPatches {
    pub patches: Vec<Patch>,
}

impl PendingPatches {
    /// Stage a patch for human review.
    pub fn push(&mut self, patch: Patch) {
        self.patches.push(patch);
    }

    /// The patches still awaiting a decision.
    pub fn pending(&self) -> impl Iterator<Item = &Patch> {
        self.patches
            .iter()
            .filter(|p| p.status == PatchStatus::Pending)
    }

    fn position(&self, patch_id: &str) -> Option<usize> {
        self.patches.iter().position(|p| p.id == patch_id)
    }
}

/// One row of the append-only workflow_versions changelog: the version cut and
/// the diff that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowVersion {
    pub version: String,
    pub diff_path: String,
}

/// A workflow plus its append-only version history and pending patches: the
/// drift-repair working set the runner threads through detect, reground, and
/// approve. The version history starts empty; each approval appends a row.
#[derive(Debug, Clone)]
pub struct RepairSession {
    pub workflow: CompiledWorkflow,
    pub versions: Vec<WorkflowVersion>,
    pub pending: PendingPatches,
}

impl RepairSession {
    /// Start a session over a freshly compiled workflow.
    pub fn new(workflow: CompiledWorkflow) -> Self {
        RepairSession {
            workflow,
            versions: Vec::new(),
            pending: PendingPatches::default(),
        }
    }
}

/// Failure modes for approval and rejection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriftError {
    #[error("no patch with id {0}")]
    UnknownPatch(String),
    #[error("patch {0} is not pending")]
    PatchNotPending(String),
    #[error("workflow has no step {0}")]
    StepNotFound(String),
}

/// Approve a pending patch: apply its re-grounded selectors and anchor to the
/// matching step, bump the workflow version, and append a changelog row
/// pointing at the diff. Returns the new version string.
///
/// This never runs on its own. It merges only the specific patch a caller
/// names, which is what keeps drift repair from ever auto-merging.
pub fn approve(session: &mut RepairSession, patch_id: &str) -> Result<String, DriftError> {
    let idx = session
        .pending
        .position(patch_id)
        .ok_or_else(|| DriftError::UnknownPatch(patch_id.to_string()))?;
    if session.pending.patches[idx].status != PatchStatus::Pending {
        return Err(DriftError::PatchNotPending(patch_id.to_string()));
    }

    let diff = session.pending.patches[idx].diff.clone();
    let diff_path = session.pending.patches[idx].diff_path();

    // Apply the re-grounded target to the step.
    let step = session
        .workflow
        .actions
        .iter_mut()
        .find(|a| a.id == diff.step_id)
        .ok_or_else(|| DriftError::StepNotFound(diff.step_id.clone()))?;
    let target = step.target.get_or_insert_with(Target::default);
    target.selectors = diff.new_selectors.clone();
    target.anchor = diff.new_anchor.clone();

    // Bump the version and append the changelog row.
    let new_version = bump_patch(&session.workflow.manifest.version);
    session.workflow.manifest.version = new_version.clone();
    session.pending.patches[idx].status = PatchStatus::Approved;
    session.versions.push(WorkflowVersion {
        version: new_version.clone(),
        diff_path,
    });

    Ok(new_version)
}

/// Reject a pending patch: archive it, leaving the workflow and its version
/// untouched.
pub fn reject(session: &mut RepairSession, patch_id: &str) -> Result<(), DriftError> {
    let idx = session
        .pending
        .position(patch_id)
        .ok_or_else(|| DriftError::UnknownPatch(patch_id.to_string()))?;
    if session.pending.patches[idx].status != PatchStatus::Pending {
        return Err(DriftError::PatchNotPending(patch_id.to_string()));
    }
    session.pending.patches[idx].status = PatchStatus::Rejected;
    Ok(())
}

/// Bump the patch component of a dotted version (`1.0.0` becomes `1.0.1`).
/// Falls back to appending `.1` when the last component is not numeric, so a
/// non-semver version string still advances rather than silently staying put.
fn bump_patch(version: &str) -> String {
    let mut parts: Vec<String> = version.split('.').map(String::from).collect();
    if let Some(last) = parts.last_mut() {
        if let Ok(n) = last.parse::<u64>() {
            *last = (n + 1).to_string();
            return parts.join(".");
        }
    }
    format!("{version}.1")
}
