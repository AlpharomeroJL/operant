//! Benchmark harness (C17): compiled replay vs re-inference-mock, emits BENCHMARKS.md. B1A scaffolds the renderer; L9B builds the real suite and the CI regression threshold.
//!
//! Provides BenchResult type mirroring contracts/bench_result.schema.json and a markdown renderer.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version marker: matches schema "v" field constraint (const 1).
fn default_v() -> i32 {
    1
}

/// One row of benchmark output: one task in one mode. A bench run emits an array of these.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchResult {
    #[serde(default = "default_v")]
    pub v: i32,
    pub suite: String,
    pub task: String,
    pub mode: BenchMode,
    pub repetitions: i32,
    pub successes: i32,
    pub p50_step_ms: f64,
    pub p95_step_ms: f64,
    pub total_wall_ms: f64,
    pub model_calls: i32,
    pub tokens: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
}

/// Benchmark execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchMode {
    Replay,
    ReinferMock,
    ReinferReal,
}

impl std::fmt::Display for BenchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchMode::Replay => write!(f, "replay"),
            BenchMode::ReinferMock => write!(f, "reinfer_mock"),
            BenchMode::ReinferReal => write!(f, "reinfer_real"),
        }
    }
}

/// Render benchmark results to BENCHMARKS.md markdown.
/// Emits a headline table (task x mode) and a methods section with honesty notes.
pub fn render_benchmarks_md(results: &[BenchResult]) -> String {
    if results.is_empty() {
        return String::from("# BENCHMARKS\n\nNo results.\n");
    }

    // Organize results by task and mode for the headline table
    let mut by_task_mode: BTreeMap<String, BTreeMap<BenchMode, &BenchResult>> = BTreeMap::new();

    for result in results {
        by_task_mode
            .entry(result.task.clone())
            .or_insert_with(BTreeMap::new)
            .insert(result.mode, result);
    }

    // Collect unique modes in sorted order
    let mut modes = std::collections::BTreeSet::new();
    for task_map in by_task_mode.values() {
        for mode in task_map.keys() {
            modes.insert(*mode);
        }
    }

    // Sort modes: replay, reinfer_mock, reinfer_real
    let mut mode_list: Vec<_> = modes.into_iter().collect();
    mode_list.sort_by_key(|m| match m {
        BenchMode::Replay => 0,
        BenchMode::ReinferMock => 1,
        BenchMode::ReinferReal => 2,
    });

    let mut output = String::from("# BENCHMARKS\n\n");

    // Build headline table
    output.push_str("| Task");
    for mode in &mode_list {
        output.push_str(" | ");
        output.push_str(&mode.to_string());
    }
    output.push_str(" |\n");

    // Separator row
    output.push_str("|---");
    for _ in &mode_list {
        output.push_str("|---");
    }
    output.push_str("|\n");

    // Data rows
    for (task, mode_results) in &by_task_mode {
        output.push('|');
        output.push_str(task);

        for mode in &mode_list {
            output.push_str(" | ");
            if let Some(result) = mode_results.get(mode) {
                // Format all metrics on one line per cell
                let success_rate = format!("{}/{}", result.successes, result.repetitions);
                let latency = format!(
                    "p50: {:.0}ms, p95: {:.0}ms",
                    result.p50_step_ms, result.p95_step_ms
                );
                let calls_tokens = format!("calls: {}, tokens: {}", result.model_calls, result.tokens);
                output.push_str(&success_rate);
                output.push_str(" | ");
                output.push_str(&latency);
                output.push_str(" | ");
                output.push_str(&calls_tokens);
            } else {
                output.push('-');
            }
        }
        output.push_str(" |\n");
    }

    output.push_str("\n## Methods\n\n");
    output.push_str("Measurements capture per-step latency, total wall time, model calls, and token usage.\n\n");
    output.push_str(
        "**Honesty note:** reinfer_mock uses recorded latencies from the actual replay,\n\
         simulating agent-at-every-step cost without hitting a real backend.\n",
    );

    output
}

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-bench";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-bench");
        let _ = "bench";
    }

    /// Embedded fixture: notepad and web tasks in replay and reinfer_mock modes
    fn fixture_results() -> Vec<BenchResult> {
        vec![
            BenchResult {
                v: 1,
                suite: "fixture".to_string(),
                task: "notepad".to_string(),
                mode: BenchMode::Replay,
                repetitions: 5,
                successes: 5,
                p50_step_ms: 42.5,
                p95_step_ms: 105.3,
                total_wall_ms: 8250.0,
                model_calls: 0,
                tokens: 0,
                notes: Some("baseline compiled".to_string()),
                ts: Some("2025-07-11T14:30:00Z".to_string()),
            },
            BenchResult {
                v: 1,
                suite: "fixture".to_string(),
                task: "notepad".to_string(),
                mode: BenchMode::ReinferMock,
                repetitions: 5,
                successes: 5,
                p50_step_ms: 48.2,
                p95_step_ms: 112.1,
                total_wall_ms: 9150.0,
                model_calls: 142,
                tokens: 28450,
                notes: Some("mock planner re-plans every step".to_string()),
                ts: Some("2025-07-11T14:31:00Z".to_string()),
            },
            BenchResult {
                v: 1,
                suite: "fixture".to_string(),
                task: "web".to_string(),
                mode: BenchMode::Replay,
                repetitions: 5,
                successes: 5,
                p50_step_ms: 67.8,
                p95_step_ms: 201.5,
                total_wall_ms: 12890.0,
                model_calls: 0,
                tokens: 0,
                notes: None,
                ts: Some("2025-07-11T14:32:00Z".to_string()),
            },
            BenchResult {
                v: 1,
                suite: "fixture".to_string(),
                task: "web".to_string(),
                mode: BenchMode::ReinferMock,
                repetitions: 5,
                successes: 4,
                p50_step_ms: 78.5,
                p95_step_ms: 225.3,
                total_wall_ms: 15620.0,
                model_calls: 187,
                tokens: 42120,
                notes: Some("one failure in step 7".to_string()),
                ts: Some("2025-07-11T14:33:00Z".to_string()),
            },
        ]
    }

    #[test]
    fn test_bench_result_serde_json_round_trip() {
        let original = fixture_results();

        for result in original.iter() {
            let json = serde_json::to_string(result).expect("serialize");
            let deserialized: BenchResult = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(result, &deserialized);
        }
    }

    #[test]
    fn test_bench_result_serde_json_array_round_trip() {
        let original = fixture_results();
        let json = serde_json::to_string(&original).expect("serialize array");
        let deserialized: Vec<BenchResult> = serde_json::from_str(&json).expect("deserialize array");
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_render_benchmarks_md_headline_table() {
        let results = fixture_results();
        let markdown = render_benchmarks_md(&results);

        eprintln!("Generated markdown:\n{}", markdown);

        // Check for headline table components
        assert!(markdown.contains("# BENCHMARKS"), "should have heading");
        assert!(
            markdown.contains("| Task |"),
            "should have table header with Task column"
        );
        assert!(
            markdown.contains("| replay |"),
            "should have replay mode column"
        );
        assert!(
            markdown.contains("| reinfer_mock |"),
            "should have reinfer_mock mode column"
        );
        assert!(
            markdown.contains("notepad"),
            "should have notepad task row"
        );
        assert!(markdown.contains("web"), "should have web task row");
    }

    #[test]
    fn test_render_benchmarks_md_includes_metrics() {
        let results = fixture_results();
        let markdown = render_benchmarks_md(&results);

        // Check for specific metrics from fixture
        assert!(markdown.contains("5/5"), "should show success count");
        assert!(markdown.contains("p50:"), "should show p50 latency");
        assert!(markdown.contains("p95:"), "should show p95 latency");
        assert!(markdown.contains("calls:"), "should show model calls");
        assert!(markdown.contains("tokens:"), "should show token count");
    }

    #[test]
    fn test_render_benchmarks_md_methods_section() {
        let results = fixture_results();
        let markdown = render_benchmarks_md(&results);

        // Check for methods section
        assert!(
            markdown.contains("## Methods"),
            "should have Methods section"
        );
        assert!(
            markdown.contains("reinfer_mock uses recorded latencies"),
            "should include honesty note about reinfer_mock"
        );
        assert!(
            markdown.contains("without hitting a real backend"),
            "should explain recorded latencies strategy"
        );
    }

    #[test]
    fn test_render_benchmarks_md_empty_results() {
        let markdown = render_benchmarks_md(&[]);
        assert!(markdown.contains("# BENCHMARKS"));
        assert!(markdown.contains("No results"));
    }

    #[test]
    fn bench_mode_display() {
        assert_eq!(BenchMode::Replay.to_string(), "replay");
        assert_eq!(BenchMode::ReinferMock.to_string(), "reinfer_mock");
        assert_eq!(BenchMode::ReinferReal.to_string(), "reinfer_real");
    }
}
