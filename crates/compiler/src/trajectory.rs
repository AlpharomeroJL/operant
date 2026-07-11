//! The compiler INPUT shape: a recorded trajectory.
//!
//! Mirrors `contracts/fixtures/trajectory_notepad.json`. A trajectory is one
//! run plus its ordered steps; each step wraps an `operant_ir::Action` with the
//! recorder metadata the passes need (snapshot digests before and after, the
//! outcome string, any human correction, and the outcome-bearing flag).
//!
//! This is a read-only view onto the recorder's rows (see
//! [`crate::compile_records`] for compiling straight from
//! `operant_recorder::StepRecord`s); the JSON fixture and the recorder shape
//! agree field for field, so both funnel into the same passes.

use operant_ir::Action;
use serde::Deserialize;

fn default_v() -> u32 {
    1
}

/// A recorded run and its steps: exactly the JSON the recorder writes and the
/// compiler reads back.
#[derive(Debug, Clone, Deserialize)]
pub struct Trajectory {
    #[serde(default = "default_v")]
    pub v: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub run: RunMeta,
    pub steps: Vec<TrajectoryStep>,
}

/// Run-level metadata. Only `id` and `goal` drive compilation; the rest is
/// carried for provenance.
#[derive(Debug, Clone, Deserialize)]
pub struct RunMeta {
    pub id: String,
    pub goal: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// One recorded step: the Action IR plus the recorder metadata the passes read.
#[derive(Debug, Clone, Deserialize)]
pub struct TrajectoryStep {
    pub seq: u32,
    pub action: Action,
    #[serde(default)]
    pub snapshot_digest_before: Option<String>,
    #[serde(default)]
    pub snapshot_digest_after: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub human_correction: Option<HumanCorrection>,
    #[serde(default)]
    pub outcome_bearing: bool,
}

/// A human correction attached to a step: it supersedes the earlier step named
/// by `supersedes_seq` (pass 1 keeps this corrected step and drops that one).
#[derive(Debug, Clone, Deserialize)]
pub struct HumanCorrection {
    pub supersedes_seq: u32,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub ts: Option<String>,
}

/// The outcome string the recorder writes for a step that a later attempt
/// replaced. Pass 1 drops every step carrying it.
pub const OUTCOME_RETRY_SUPERSEDED: &str = "retry_superseded";
