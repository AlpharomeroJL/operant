//! References the three cookbook workflows `docs/specs/bench.md` names as
//! part of the suite ("plus three cookbook workflows"): `cookbook/bench-workflows.json`,
//! generated for this lane specifically so it can consume the list "without
//! needing to run Node or parse comments" (that file's own `description`
//! field).
//!
//! These three are TypeScript workflows over `@operant/sdk`
//! (`cookbook/*/workflow.ts`), not compiled fixtures: unlike notepad there is
//! no `CompiledWorkflow` for them anywhere in `contracts/fixtures`, and
//! producing one would mean running the TypeScript compiler pipeline, well
//! outside a Rust crate's owned surface. Faking bench numbers for a workflow
//! this suite never actually replayed would contradict the honesty section
//! `render_benchmarks_md` prints for every other row, so this module only
//! confirms the reference is well-formed and lists it in BENCHMARKS.md; see
//! FOLLOWUPS in this lane's reply for wiring them into real execution.

use serde::Deserialize;

const BENCH_WORKFLOWS_JSON: &str = include_str!("../../../cookbook/bench-workflows.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CookbookWorkflowRef {
    pub slug: String,
    pub workflow: String,
    pub prose: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BenchWorkflowsFile {
    description: String,
    count: usize,
    workflows: Vec<CookbookWorkflowRef>,
}

/// Parse `cookbook/bench-workflows.json`. Panics on malformed JSON: this is
/// a committed fixture-adjacent file this lane reads, not user input.
fn bench_workflows_file() -> BenchWorkflowsFile {
    serde_json::from_str(BENCH_WORKFLOWS_JSON).expect("cookbook/bench-workflows.json parses")
}

/// The three cookbook workflows `docs/specs/bench.md` names as part of the
/// suite's scope, referenced (not executed) by this suite run.
pub fn referenced_cookbook_workflows() -> Vec<CookbookWorkflowRef> {
    let file = bench_workflows_file();
    debug_assert_eq!(file.count, file.workflows.len());
    file.workflows
}

/// The source file's own `description` field, carried into BENCHMARKS.md so
/// the reader sees why these three are listed but not measured.
pub fn reference_note() -> String {
    bench_workflows_file().description
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exactly_three_workflows() {
        let workflows = referenced_cookbook_workflows();
        assert_eq!(workflows.len(), 3);
    }

    #[test]
    fn every_referenced_slug_is_non_empty_and_paths_look_like_cookbook_paths() {
        for w in referenced_cookbook_workflows() {
            assert!(!w.slug.is_empty());
            assert!(w.workflow.starts_with("cookbook/"), "{}", w.workflow);
            assert!(w.workflow.ends_with("workflow.ts"), "{}", w.workflow);
            assert!(w.prose.starts_with("cookbook/"), "{}", w.prose);
            assert!(w.prose.ends_with(".md"), "{}", w.prose);
        }
    }

    #[test]
    fn reference_note_mentions_bench() {
        assert!(reference_note().to_lowercase().contains("bench"));
    }
}
