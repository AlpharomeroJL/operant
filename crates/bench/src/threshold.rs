//! CI regression threshold (C17, FR-D3): `docs/specs/bench.md` states it
//! plainly, "CI threshold: replay success 5/5 on unchanged fixtures and p50
//! step under 150 ms, else fail." This module is that check, expressed as a
//! pure function over `BenchResult` rows so it is trivial to exercise both
//! ways: pass it the real suite's `replay` rows and it must pass; pass it a
//! row built from a deliberately-broken fixture and it must fail.
//!
//! Only `replay` rows are checked. `reinfer_mock` has no success or latency
//! bar of its own here: it is a cost simulation layered on the same
//! trajectory replay already proved out, not an independent execution the
//! regression gate needs to police.

use crate::{BenchMode, BenchResult};

/// The p50 step latency ceiling, milliseconds. `docs/specs/bench.md`: "p50
/// step under 150 ms".
pub const MAX_P50_STEP_MS: f64 = 150.0;

/// Outcome of checking a set of bench rows against the regression threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionCheck {
    pub passed: bool,
    /// One human-readable reason per violation. Empty when `passed`.
    pub failures: Vec<String>,
}

impl RegressionCheck {
    fn pass() -> Self {
        Self {
            passed: true,
            failures: Vec::new(),
        }
    }
}

/// Check every `replay`-mode row in `results` against the CI regression
/// threshold: `successes == repetitions` (5/5 on unchanged fixtures) and
/// `p50_step_ms < MAX_P50_STEP_MS`. Rows in other modes are ignored. A
/// `results` slice with no `replay` rows passes vacuously; callers that need
/// suite coverage enforced should check that separately (see
/// `suite::run_suite`'s own test asserting all three fixture tasks appear).
pub fn check_regression(results: &[BenchResult]) -> RegressionCheck {
    let mut failures = Vec::new();

    for row in results.iter().filter(|r| r.mode == BenchMode::Replay) {
        if row.successes != row.repetitions {
            failures.push(format!(
                "{}/{}: replay succeeded {}/{} repetitions, need {}/{}",
                row.suite,
                row.task,
                row.successes,
                row.repetitions,
                row.repetitions,
                row.repetitions
            ));
        }
        if row.p50_step_ms >= MAX_P50_STEP_MS {
            failures.push(format!(
                "{}/{}: replay p50 step latency {:.1}ms is not under the {:.1}ms threshold",
                row.suite, row.task, row.p50_step_ms, MAX_P50_STEP_MS
            ));
        }
    }

    if failures.is_empty() {
        RegressionCheck::pass()
    } else {
        RegressionCheck {
            passed: false,
            failures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_row(task: &str) -> BenchResult {
        BenchResult {
            v: 1,
            suite: "fixture".to_string(),
            task: task.to_string(),
            mode: BenchMode::Replay,
            repetitions: 5,
            successes: 5,
            p50_step_ms: 12.0,
            p95_step_ms: 20.0,
            total_wall_ms: 60.0,
            model_calls: 0,
            tokens: 0,
            notes: None,
            ts: None,
        }
    }

    #[test]
    fn passes_when_every_replay_row_hits_five_of_five_under_threshold() {
        let results = vec![passing_row("notepad"), passing_row("web")];
        let check = check_regression(&results);
        assert!(check.passed, "unexpected failures: {:?}", check.failures);
        assert!(check.failures.is_empty());
    }

    #[test]
    fn fails_when_a_replay_row_has_fewer_than_five_of_five_successes() {
        let mut broken = passing_row("notepad");
        broken.successes = 3;
        let check = check_regression(&[broken]);
        assert!(!check.passed);
        assert_eq!(check.failures.len(), 1);
        assert!(check.failures[0].contains("3/5"));
    }

    #[test]
    fn fails_when_p50_step_latency_meets_or_exceeds_the_ceiling() {
        let mut slow = passing_row("web");
        slow.p50_step_ms = MAX_P50_STEP_MS;
        let check = check_regression(&[slow]);
        assert!(!check.passed);
        assert_eq!(check.failures.len(), 1);
        assert!(check.failures[0].contains("150.0ms"));
    }

    #[test]
    fn reports_every_violation_not_just_the_first() {
        let mut broken = passing_row("notepad");
        broken.successes = 0;
        broken.p50_step_ms = 999.0;
        let check = check_regression(&[broken]);
        assert!(!check.passed);
        assert_eq!(check.failures.len(), 2);
    }

    #[test]
    fn ignores_reinfer_mock_rows() {
        let mut mock_row = passing_row("notepad");
        mock_row.mode = BenchMode::ReinferMock;
        mock_row.successes = 0; // would fail if this mode were checked
        mock_row.p50_step_ms = 9999.0;
        let check = check_regression(&[mock_row]);
        assert!(
            check.passed,
            "reinfer_mock rows must not be checked against the replay bar"
        );
    }

    #[test]
    fn empty_input_passes_vacuously() {
        assert!(check_regression(&[]).passed);
    }
}
