//! Drift detection: classify a replay step that did not execute cleanly.
//!
//! A step is drift-eligible only when every stored selector misses AND the
//! recorded visual anchor no longer matches (below its tolerance) AND the
//! step's precondition still holds. The precondition is what separates a
//! genuine drift (the step still applies, its target just moved) from a
//! wrong-state failure (the app is not where the step expects), which is NOT
//! drift and must halt normally. A destructive-risk step that would otherwise
//! be drift-eligible is refused for auto-repair and always halts to a human.

use operant_ir::snapshot::{Role, Snapshot};
use operant_ir::{Action, RiskClass};

use crate::drift::resolve::resolve;

/// A step precondition gate, evaluated against the live snapshot. Kept as a
/// small data enum rather than the full `operant_ir::Gate` predicate language
/// so the drift module stays self-contained (the gate evaluator lives in a
/// separate crate this one does not depend on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// The live window title equals this string.
    WindowTitleEquals(String),
    /// An element with this role and name exists in the live snapshot.
    ElementExists { role: Role, name: String },
    /// No precondition; always holds.
    Always,
}

impl Precondition {
    /// Whether this precondition holds against the current snapshot.
    pub fn holds(&self, current: &Snapshot) -> bool {
        match self {
            Precondition::WindowTitleEquals(title) => current.window.title == *title,
            Precondition::ElementExists { role, name } => current.find(*role, name).is_some(),
            Precondition::Always => true,
        }
    }
}

/// The classification of a replay step, and the decision that follows from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnosis {
    /// At least one selector still resolves; the step is fine, proceed.
    Resolved,
    /// The step cannot be confidently repaired: either its precondition does
    /// not hold (a wrong-state failure) or the drift signals are inconsistent
    /// (selectors miss but the anchor still matches). Halts normally; never
    /// repaired.
    WrongState,
    /// Every drift signal fires and the precondition holds, but the step's
    /// risk class is destructive. Refused for auto-repair; always halts to a
    /// human.
    RefusedDestructive,
    /// Selectors all miss, the anchor is below tolerance, the precondition
    /// holds, and the step is safe to repair. Eligible for single-step
    /// re-grounding.
    DriftEligible,
}

impl Diagnosis {
    pub fn is_drift_eligible(&self) -> bool {
        matches!(self, Diagnosis::DriftEligible)
    }
}

/// Classify `step` against the live snapshot.
///
/// `anchor_similarity` is the measured match of the step's recorded visual
/// anchor against the current screen, from 0.0 (no match) to 1.0 (identical);
/// it is "below tolerance" when it is under the step anchor's `tolerance`. A
/// step with no anchor treats the visual signal as uninformative and below
/// tolerance, so detection rests on the selector and precondition checks.
pub fn diagnose(
    step: &Action,
    current: &Snapshot,
    anchor_similarity: f64,
    precondition: &Precondition,
) -> Diagnosis {
    let selectors = step
        .target
        .as_ref()
        .map(|t| t.selectors.as_slice())
        .unwrap_or(&[]);

    // Signal 1: a step whose target still resolves is not drifting at all.
    if resolve(current, selectors).is_some() {
        return Diagnosis::Resolved;
    }

    // Signal 2: the recorded visual anchor no longer matches the screen. A
    // step with no anchor has nothing to disprove, so the signal is treated
    // as below tolerance.
    let anchor_below_tolerance = match step.target.as_ref().and_then(|t| t.anchor.as_ref()) {
        Some(anchor) => anchor_similarity < anchor.tolerance,
        None => true,
    };
    if !anchor_below_tolerance {
        // Selectors miss but the anchor still matches: the drift rule's AND
        // is not satisfied, so this is not a confident drift. Halt normally
        // rather than re-ground a target that may still be present.
        return Diagnosis::WrongState;
    }

    // The WHILE clause: a step whose precondition no longer holds is a
    // wrong-state failure, not drift. This is the line that keeps drift
    // repair from papering over an app that navigated somewhere unexpected.
    if !precondition.holds(current) {
        return Diagnosis::WrongState;
    }

    // Every drift signal fires, but a destructive step is never auto-repaired.
    if step.risk_class == RiskClass::Destructive || step.irreversible {
        return Diagnosis::RefusedDestructive;
    }

    Diagnosis::DriftEligible
}
