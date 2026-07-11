//! The bar for C17 / FR-D3: running the real suite must produce
//! `BENCHMARKS.md` at the repo root and pass the CI regression threshold on
//! the unchanged fixtures; a deliberately-broken replay must fail that same
//! threshold. Two halves, so the positive test alone could not hide a
//! threshold check that always returns true.

use std::path::PathBuf;

use operant_bench::suite::{notepad_task_with_broken_postcondition, run_suite, run_task};
use operant_bench::threshold::check_regression;
use operant_bench::{cookbook, render_benchmarks_md_with_cookbook, BenchMode};

/// `BENCHMARKS.md` lives at the repo root, generated; this lane owns it
/// alongside `crates/bench`. `CARGO_MANIFEST_DIR` is `crates/bench` at
/// compile time regardless of the test runner's working directory, so this
/// resolves to the same path from `cargo test` or `just test` alike.
fn benchmarks_md_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("BENCHMARKS.md")
}

#[test]
fn suite_run_produces_benchmarks_md_and_passes_the_regression_threshold() {
    let results = run_suite();
    assert_eq!(
        results.len(),
        6,
        "3 fixture tasks x 2 modes (replay, reinfer_mock)"
    );

    let cookbook_refs = cookbook::referenced_cookbook_workflows();
    assert_eq!(cookbook_refs.len(), 3);

    let markdown = render_benchmarks_md_with_cookbook(&results, &cookbook_refs);

    // The headline table and honesty section B1A's renderer guarantees.
    assert!(markdown.contains("# BENCHMARKS"));
    assert!(markdown.contains("## Methods"));
    assert!(markdown.contains("reinfer_mock uses recorded latencies"));
    assert!(markdown.contains("notepad"));
    assert!(markdown.contains("web"));
    assert!(markdown.contains("drift_repaired"));
    // This lane's extension.
    assert!(markdown.contains("## Cookbook workflows referenced"));
    assert!(markdown.contains("copy-invoice-rows-web-to-spreadsheet"));
    assert!(markdown.contains("rename-file-pdfs-by-date"));
    assert!(markdown.contains("extract-text-from-images"));

    std::fs::write(benchmarks_md_path(), &markdown).expect("write BENCHMARKS.md at the repo root");

    // The CI regression threshold: docs/specs/bench.md, "replay success 5/5
    // on unchanged fixtures and p50 step under 150 ms, else fail". Checked
    // only against this run's own replay rows, over the real, unmodified
    // fixtures the suite just measured.
    let replay_rows: Vec<_> = results
        .iter()
        .filter(|r| r.mode == BenchMode::Replay)
        .cloned()
        .collect();
    assert_eq!(replay_rows.len(), 3);
    let check = check_regression(&replay_rows);
    assert!(
        check.passed,
        "regression threshold failed on unchanged fixtures: {:?}",
        check.failures
    );

    // reinfer_mock rows are real rows too, just not gated by the threshold:
    // sanity-check they carry the cost signal they claim to.
    for row in results.iter().filter(|r| r.mode == BenchMode::ReinferMock) {
        assert!(
            row.model_calls > 0,
            "{}: reinfer_mock re-plans every step",
            row.task
        );
        assert!(row.tokens > 0, "{}", row.task);
    }
}

#[test]
fn a_deliberately_broken_replay_fails_the_regression_threshold() {
    // Same notepad fixture, empty post-condition snapshot: the compiled
    // workflow's postcondition assert fails on every repetition (mirrors
    // crates/replay/tests/replay_notepad.rs's own
    // postcondition_fails_when_the_note_was_not_written). This is the "wrong
    // expected outcome" case the bar calls for: not a crash, a wrong result.
    let broken = notepad_task_with_broken_postcondition();
    let run = run_task("fixture-broken", &broken);

    assert_eq!(
        run.replay.successes, 0,
        "every repetition must fail the postcondition"
    );
    assert_eq!(run.replay.repetitions, 5);

    let check = check_regression(&[run.replay]);
    assert!(
        !check.passed,
        "a 0/5 replay row must fail the regression threshold"
    );
    assert!(!check.failures.is_empty());
    assert!(
        check.failures.iter().any(|f| f.contains("0/5")),
        "failure reasons: {:?}",
        check.failures
    );
}
