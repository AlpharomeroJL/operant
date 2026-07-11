//! Trajectory compiler (C8): normalize, parameterize, selectorize, insert waits
//! and asserts, emit TypeScript DSL plus manifest. L8A implements the five
//! passes; L8B adds drift repair.
//!
//! The compiler turns one recorded run (a [`Trajectory`], the shape of
//! `contracts/fixtures/trajectory_notepad.json`) into a [`CompiledWorkflow`]:
//! an [`operant_ir::Manifest`] plus the ordered [`operant_ir::Action`]s the
//! deterministic replay executor consumes, alongside a readable TypeScript file
//! over `@operant/sdk`.
//!
//! ```
//! use operant_compiler::{compile, Trajectory};
//!
//! let raw = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
//! let traj: Trajectory = serde_json::from_str(raw).unwrap();
//! let out = compile(&traj).unwrap();
//! assert_eq!(out.workflow.manifest.name, "notepad-invoice-note");
//! assert_eq!(out.workflow.manifest.step_summary.len(), 6);
//! ```

pub mod drift;
mod emit;
mod pipeline;
mod trajectory;

use operant_ir::{Action, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub use pipeline::{InputDef, InputKind};
pub use trajectory::{HumanCorrection, RunMeta, Trajectory, TrajectoryStep};

use pipeline::{normalize, parameterize, selectorize, waits_and_asserts, NormRow, WorkStep};

/// The structured compiler output the replay executor consumes: the manifest
/// plus the ordered actions. Serializes as `{ "manifest": ..., "actions": [...] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledWorkflow {
    pub manifest: Manifest,
    pub actions: Vec<Action>,
}

/// Everything one `compile` call produces: the [`CompiledWorkflow`] and the
/// TypeScript DSL source the manifest's `dsl.hash` is taken over.
#[derive(Debug, Clone)]
pub struct Compilation {
    pub workflow: CompiledWorkflow,
    /// The `workflow.ts` source. `workflow.manifest.dsl.hash` is its BLAKE3.
    pub dsl_source: String,
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("trajectory has no steps to compile")]
    EmptyTrajectory,
}

/// Compile a recorded [`Trajectory`] into a [`Compilation`].
pub fn compile(traj: &Trajectory) -> Result<Compilation, CompileError> {
    let rows: Vec<NormRow> = traj
        .steps
        .iter()
        .map(|s| NormRow {
            seq: s.seq,
            outcome: s.outcome.clone(),
            supersedes_seq: s.human_correction.as_ref().map(|hc| hc.supersedes_seq),
            step: WorkStep::new(
                s.action.clone(),
                s.snapshot_digest_before.clone(),
                s.snapshot_digest_after.clone(),
                s.outcome_bearing,
            ),
        })
        .collect();

    lower(&traj.run.goal, &traj.run.id, rows)
}

/// Compile straight from recorder rows (`operant_recorder::StepRecord`), the
/// real pipeline path: recorder -> compiler. Same five passes as [`compile`].
pub fn compile_records(
    goal: &str,
    run_id: &str,
    steps: &[operant_recorder::StepRecord],
) -> Result<Compilation, CompileError> {
    let rows: Vec<NormRow> = steps
        .iter()
        .map(|s| NormRow {
            seq: s.seq,
            outcome: Some(s.outcome.clone()),
            supersedes_seq: s
                .human_correction
                .as_ref()
                .and_then(|hc| hc.get("supersedes_seq"))
                .and_then(Value::as_u64)
                .map(|n| n as u32),
            step: WorkStep::new(
                s.action.clone(),
                s.snapshot_digest_before.clone(),
                s.snapshot_digest_after.clone(),
                s.outcome_bearing,
            ),
        })
        .collect();

    lower(goal, run_id, rows)
}

/// The shared pipeline: run passes 1-5 over the normalized rows.
fn lower(goal: &str, run_id: &str, rows: Vec<NormRow>) -> Result<Compilation, CompileError> {
    if rows.is_empty() {
        return Err(CompileError::EmptyTrajectory);
    }

    // Pass 1: normalize.
    let mut steps = normalize(rows);
    if steps.is_empty() {
        return Err(CompileError::EmptyTrajectory);
    }

    // Pass 2: parameterize.
    let inputs = parameterize(&mut steps);

    // Pass 3: selectorize.
    selectorize(&mut steps);

    // Pass 4: waits and asserts.
    let (actions, post_expr) = waits_and_asserts(steps);

    // Pass 5: emit.
    let emitted = emit::emit(goal, run_id, &actions, &inputs, post_expr.as_ref());

    Ok(Compilation {
        workflow: CompiledWorkflow {
            manifest: emitted.manifest,
            actions,
        },
        dsl_source: emitted.dsl_source,
    })
}

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-compiler";

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{ActionKind, Capabilities, RiskClass};

    const TRAJECTORY: &str = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
    const EXPECTED_MANIFEST: &str =
        include_str!("../../../contracts/fixtures/workflow_notepad/manifest.json");
    const SCHEMA: &str = include_str!("../../../contracts/workflow_manifest.schema.json");

    fn compile_fixture() -> Compilation {
        let traj: Trajectory = serde_json::from_str(TRAJECTORY).expect("fixture parses");
        compile(&traj).expect("fixture compiles")
    }

    #[test]
    fn crate_present() {
        assert_eq!(CRATE, "operant-compiler");
    }

    #[test]
    fn name_matches_expected_manifest() {
        let out = compile_fixture();
        let expected: Value = serde_json::from_str(EXPECTED_MANIFEST).unwrap();
        assert_eq!(out.workflow.manifest.name, "notepad-invoice-note");
        assert_eq!(
            Value::String(out.workflow.manifest.name.clone()),
            expected["name"]
        );
    }

    #[test]
    fn step_summary_matches_expected_manifest() {
        let out = compile_fixture();
        let expected: Value = serde_json::from_str(EXPECTED_MANIFEST).unwrap();
        let got: Value = serde_json::to_value(&out.workflow.manifest.step_summary).unwrap();
        assert_eq!(got, expected["step_summary"]);
        assert_eq!(
            out.workflow.manifest.step_summary,
            vec![
                "Click the text editor",
                "Type the invoice note",
                "Wait for the screen to update",
                "Save the file",
                "Wait for the screen to update",
                "Check that the note was written",
            ]
        );
    }

    #[test]
    fn inputs_schema_matches_expected_manifest() {
        let out = compile_fixture();
        let expected: Value = serde_json::from_str(EXPECTED_MANIFEST).unwrap();
        // Value equality is key-order independent, so this is a true structural
        // match of the inferred inputs schema against the frozen fixture.
        assert_eq!(
            out.workflow.manifest.inputs_schema,
            expected["inputs_schema"]
        );
    }

    #[test]
    fn parameterization_inferred_invoice_date_and_amount() {
        let out = compile_fixture();
        let props = &out.workflow.manifest.inputs_schema["properties"];
        assert_eq!(props["invoice_date"]["format"], "date");
        assert_eq!(props["invoice_date"]["default"], "2026-07-11");
        assert_eq!(props["amount"]["pattern"], r"^\d+\.\d{2}$");
        assert_eq!(props["amount"]["default"], "142.50");

        // The type step now carries the template, not the literal.
        let type_step = out
            .workflow
            .actions
            .iter()
            .find(|a| a.kind == ActionKind::Type)
            .expect("a type step survives");
        assert_eq!(
            type_step.params["text"],
            Value::String("Invoice {invoice_date} total ${amount}".to_string())
        );
    }

    #[test]
    fn normalize_dropped_the_superseded_step_and_kept_the_correction() {
        let out = compile_fixture();
        // The retry-superseded "Open the File menu" click is gone.
        assert!(!out
            .workflow
            .actions
            .iter()
            .any(|a| a.intent.as_deref() == Some("Open the File menu")));
        // The corrected save (ctrl+s) survives.
        let save = out
            .workflow
            .actions
            .iter()
            .find(|a| a.kind == ActionKind::Key)
            .expect("save survives");
        assert_eq!(save.params["combo"], "ctrl+s");
    }

    #[test]
    fn steps_are_click_type_wait_key_wait_assert() {
        let out = compile_fixture();
        let kinds: Vec<ActionKind> = out.workflow.actions.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ActionKind::Click,
                ActionKind::Type,
                ActionKind::Wait,
                ActionKind::Key,
                ActionKind::Wait,
                ActionKind::Assert,
            ]
        );
    }

    #[test]
    fn wait_follows_the_type_step_that_changed_the_digest() {
        let out = compile_fixture();
        let actions = &out.workflow.actions;
        // Index 1 is the type step; index 2 must be the wait it triggered.
        assert_eq!(actions[1].kind, ActionKind::Type);
        assert_eq!(actions[2].kind, ActionKind::Wait);
        assert_eq!(
            actions[2].intent.as_deref(),
            Some("Wait for the screen to update")
        );
    }

    #[test]
    fn selectors_are_ordered_by_stability_score() {
        let out = compile_fixture();
        let click = &out.workflow.actions[0];
        let sels = &click.target.as_ref().unwrap().selectors;
        let scores: Vec<i32> = sels.iter().map(|s| s.score()).collect();
        // Automation id (100), then name+role path (50), then ordinal (20).
        assert_eq!(scores, vec![100, 50, 20]);
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(scores, sorted);
    }

    #[test]
    fn capabilities_and_gates_match_expected() {
        let out = compile_fixture();
        let expected: CompiledManifestView =
            serde_json::from_str::<CompiledManifestView>(EXPECTED_MANIFEST).unwrap();

        assert_eq!(
            out.workflow.manifest.capabilities,
            Capabilities {
                apps: vec!["notepad.exe".to_string()],
                paths: vec![],
                network: false,
                risk_ceiling: RiskClass::Write,
            }
        );

        // Gates: a pre (process == notepad.exe) and the post assert, compared as
        // JSON so predicate structure is what is checked.
        let got_gates = serde_json::to_value(&out.workflow.manifest.gates).unwrap();
        assert_eq!(got_gates, expected.gates);
    }

    #[test]
    fn final_assert_becomes_the_postcondition_gate() {
        let out = compile_fixture();
        let post = out
            .workflow
            .manifest
            .gates
            .iter()
            .find(|g| serde_json::to_value(g.kind).unwrap() == "post")
            .expect("a post gate exists");
        assert_eq!(post.expr["op"], "matches");
        assert_eq!(post.expr["query"]["name"], "Text editor");
    }

    #[test]
    fn manifest_validates_against_the_schema() {
        let out = compile_fixture();
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let compiled = jsonschema::JSONSchema::compile(&schema).expect("schema compiles");
        let instance = serde_json::to_value(&out.workflow.manifest).unwrap();
        let errors: Vec<String> = match compiled.validate(&instance) {
            Ok(()) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(
            errors.is_empty(),
            "manifest failed schema validation: {errors:?}"
        );
    }

    #[test]
    fn dsl_hash_is_blake3_of_the_emitted_source() {
        let out = compile_fixture();
        let recomputed = blake3::hash(out.dsl_source.as_bytes()).to_hex().to_string();
        assert_eq!(out.workflow.manifest.dsl.hash, recomputed);
        assert_eq!(out.workflow.manifest.dsl.hash.len(), 64);
        assert_eq!(out.workflow.manifest.dsl.path, "workflow.ts");
    }

    #[test]
    fn emitted_ts_has_one_statement_per_step_over_the_sdk() {
        let out = compile_fixture();
        let ts = &out.dsl_source;
        assert!(ts.contains("import { defineWorkflow, step, input } from \"@operant/sdk\";"));
        assert!(ts.contains("export default defineWorkflow({"));
        assert!(ts.contains("invoice_date: input.date({"));
        assert!(ts.contains("amount: input.currency({"));
        assert!(ts.contains("text: \"Invoice {invoice_date} total ${amount}\""));
        assert!(ts.contains("combo: \"ctrl+s\""));
        // One comment per emitted step (6 steps).
        let step_comments = ts
            .lines()
            .filter(|l| l.trim_start().starts_with("// ") && l.contains('.'))
            .count();
        assert!(step_comments >= 6, "expected a numbered comment per step");
    }

    #[test]
    fn compiled_workflow_round_trips_as_json() {
        let out = compile_fixture();
        let json = serde_json::to_string(&out.workflow).unwrap();
        let back: CompiledWorkflow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, out.workflow);
    }

    /// A narrow view over the expected manifest for gate comparison.
    #[derive(serde::Deserialize)]
    struct CompiledManifestView {
        gates: Value,
    }
}
