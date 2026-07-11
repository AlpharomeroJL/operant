//! Time-saved ledger (C21) end-to-end tests.
//!
//! Tests the metrics module: fixture workflows with known explore/replay timings,
//! weekly roll-up aggregation, and plain-language digest copy rendering.

use operant_recorder::metrics::{digest_copy, estimate_minutes_saved, workflow_digest};
use operant_recorder::Recorder;

#[test]
fn estimate_fixture_workflow_time_savings() {
    // Fixture: workflow taking 30 seconds to teach (explore), replaying in 2 seconds p50,
    // run 50 times. Time saved: (30 - 2) * 50 = 1400 seconds = 23.33 minutes.
    let minutes = estimate_minutes_saved(30_000, 2_000, 50);
    assert!((minutes - 23.33).abs() < 0.01, "expected 23.33 minutes, got {}", minutes);
}

#[test]
fn estimate_negative_savings_yields_zero() {
    // If replay is faster than explore, no time is saved.
    let minutes = estimate_minutes_saved(1000, 5000, 100);
    assert_eq!(minutes, 0.0);
}

#[test]
fn estimate_zero_runs_yields_zero() {
    let minutes = estimate_minutes_saved(30_000, 2_000, 0);
    assert_eq!(minutes, 0.0);
}

#[test]
fn weekly_rollup_aggregates_multiple_workflows() {
    let rec = Recorder::open_in_memory().unwrap();

    // Create two workflows in the same week
    // Workflow 1: 30s explore, 2s replay, 50 runs = 23.33 minutes
    rec.upsert_metrics("invoice-writer", "2026-W28", 50, Some(30_000), Some(2_000), None)
        .unwrap();

    // Workflow 2: 60s explore, 5s replay, 20 runs = 18.33 minutes
    rec.upsert_metrics("browser-auto", "2026-W28", 20, Some(60_000), Some(5_000), None)
        .unwrap();

    let system_metrics = rec.get_weekly_system_metrics("2026-W28").unwrap();

    assert_eq!(system_metrics.week, "2026-W28");
    assert_eq!(system_metrics.total_runs, 70, "total runs should aggregate");
    assert!(
        (system_metrics.total_minutes_saved - 41.67).abs() < 0.01,
        "total minutes saved should be 41.67, got {}",
        system_metrics.total_minutes_saved
    );
    assert_eq!(system_metrics.workflows.len(), 2);

    // Spot-check individual workflow metrics
    let wf1 = system_metrics.workflows.iter().find(|w| w.workflow_id == "invoice-writer");
    assert!(wf1.is_some());
    let wf1 = wf1.unwrap();
    assert_eq!(wf1.runs, 50);
    assert!((wf1.minutes_saved - 23.33).abs() < 0.01);

    let wf2 = system_metrics.workflows.iter().find(|w| w.workflow_id == "browser-auto");
    assert!(wf2.is_some());
    let wf2 = wf2.unwrap();
    assert_eq!(wf2.runs, 20);
    assert!((wf2.minutes_saved - 18.33).abs() < 0.01);
}

#[test]
fn weekly_rollup_empty_week_has_zero_savings() {
    let rec = Recorder::open_in_memory().unwrap();
    let system_metrics = rec.get_weekly_system_metrics("2099-W01").unwrap();

    assert_eq!(system_metrics.week, "2099-W01");
    assert_eq!(system_metrics.total_runs, 0);
    assert_eq!(system_metrics.total_minutes_saved, 0.0);
    assert!(system_metrics.workflows.is_empty());
}

#[test]
fn digest_copy_zero_is_plain_language() {
    let copy = digest_copy(0.0);
    assert_eq!(copy, "No time saved this week yet");
    // Plain language: no jargon
    assert!(!copy.contains("ms"));
    assert!(!copy.contains("estimate"));
}

#[test]
fn digest_copy_minutes_is_plain_language() {
    let copy = digest_copy(45.0);
    assert_eq!(copy, "Operant saved you 45 minutes this week");
    // Plain language: no technical jargon
    assert!(!copy.contains("ms"));
    assert!(!copy.contains("explore"));
    assert!(!copy.contains("replay"));
    assert!(copy.contains("45 minutes"));
}

#[test]
fn digest_copy_hours_is_plain_language() {
    let copy = digest_copy(180.0);
    assert_eq!(copy, "Operant saved you 3 hours this week");
    assert!(!copy.contains("ms"));
    assert!(!copy.contains("explore"));
    assert!(copy.contains("3 hours"));
}

#[test]
fn digest_copy_hours_and_minutes_is_plain_language() {
    let copy = digest_copy(125.0);
    assert_eq!(copy, "Operant saved you 2 hours and 5 minutes this week");
    assert!(!copy.contains("ms"));
    assert!(!copy.contains("replay"));
    assert!(copy.contains("2 hours"));
    assert!(copy.contains("5 minutes"));
}

#[test]
fn digest_copy_singular_forms() {
    assert_eq!(digest_copy(1.0), "Operant saved you 1 minute this week");
    assert_eq!(digest_copy(60.0), "Operant saved you 1 hour this week");
    assert_eq!(digest_copy(61.0), "Operant saved you 1 hour and 1 minute this week");
}

#[test]
fn workflow_digest_minutes() {
    let copy = workflow_digest("invoice-writer", 30.0);
    assert_eq!(copy, "invoice-writer: 30 minutes");
    assert!(!copy.contains("ms"));
}

#[test]
fn workflow_digest_hours() {
    let copy = workflow_digest("invoice-writer", 120.0);
    assert_eq!(copy, "invoice-writer: 2 hours");
    assert!(!copy.contains("ms"));
}

#[test]
fn workflow_digest_mixed() {
    let copy = workflow_digest("browser-auto", 90.0);
    assert_eq!(copy, "browser-auto: 1 hour and 30 minutes");
    assert!(!copy.contains("ms"));
    assert!(!copy.contains("explore"));
}

#[test]
fn workflow_digest_zero() {
    let copy = workflow_digest("browser-auto", 0.0);
    assert_eq!(copy, "browser-auto: no time saved");
}

#[test]
fn get_workflow_metrics_single_workflow() {
    let rec = Recorder::open_in_memory().unwrap();
    rec.upsert_metrics("invoice-writer", "2026-W28", 50, Some(30_000), Some(2_000), None)
        .unwrap();

    let metrics = rec.get_weekly_workflow_metrics("invoice-writer", "2026-W28").unwrap();
    assert!(metrics.is_some());

    let m = metrics.unwrap();
    assert_eq!(m.workflow_id, "invoice-writer");
    assert_eq!(m.week, "2026-W28");
    assert_eq!(m.runs, 50);
    assert_eq!(m.explore_ms, 30_000);
    assert_eq!(m.replay_p50_ms, 2_000);
    assert!((m.minutes_saved - 23.33).abs() < 0.01);
}

#[test]
fn get_workflow_metrics_missing_week_returns_none() {
    let rec = Recorder::open_in_memory().unwrap();
    let metrics = rec.get_weekly_workflow_metrics("invoice-writer", "2099-W01").unwrap();
    assert!(metrics.is_none());
}

#[test]
fn metrics_upsert_with_explicit_saved_estimate() {
    let rec = Recorder::open_in_memory().unwrap();

    // Insert with explicit minutes_saved estimate already computed elsewhere
    rec.upsert_metrics("invoice-writer", "2026-W28", 50, Some(30_000), Some(2_000), Some(25.0))
        .unwrap();

    let metrics = rec.get_weekly_workflow_metrics("invoice-writer", "2026-W28").unwrap();
    assert!(metrics.is_some());

    let m = metrics.unwrap();
    // Should use the stored estimate, not recalculate
    assert_eq!(m.minutes_saved, 25.0);
}

#[test]
fn end_to_end_realistic_workflow_scenario() {
    // Simulate a realistic week: one invoice workflow and one browser workflow
    let rec = Recorder::open_in_memory().unwrap();

    // Monday: invoice workflow first run (5 runs that day)
    rec.upsert_metrics("invoice-writer", "2026-W28", 5, Some(45_000), Some(3_000), None)
        .unwrap();

    // Wednesday: same invoice workflow runs 3 more times this week
    rec.upsert_metrics("invoice-writer", "2026-W28", 3, None, None, None).unwrap();

    // Wednesday: browser automation workflow starts, runs once
    rec.upsert_metrics("browser-auto", "2026-W28", 1, Some(120_000), Some(8_000), None)
        .unwrap();

    // Friday: browser workflow runs 4 more times
    rec.upsert_metrics("browser-auto", "2026-W28", 4, None, None, None).unwrap();

    let system = rec.get_weekly_system_metrics("2026-W28").unwrap();

    // Invoice: (45 - 3) * 8 = 336 seconds = 5.6 minutes
    // Browser: (120 - 8) * 5 = 560 seconds = 9.33 minutes
    // Total: 14.93 minutes, rounds to 15 minutes

    assert_eq!(system.total_runs, 13);
    assert!((system.total_minutes_saved - 14.93).abs() < 0.01);

    let digest = digest_copy(system.total_minutes_saved);
    assert_eq!(digest, "Operant saved you 15 minutes this week");
    assert!(!digest.contains("ms"));
    assert!(!digest.contains("estimate"));

    // Spot-check workflow digests
    let invoice = system.workflows.iter().find(|w| w.workflow_id == "invoice-writer").unwrap();
    let invoice_line = workflow_digest(&invoice.workflow_id, invoice.minutes_saved);
    assert!(invoice_line.contains("invoice-writer"));
    assert!(invoice_line.contains("5") || invoice_line.contains("minutes"));

    let browser = system.workflows.iter().find(|w| w.workflow_id == "browser-auto").unwrap();
    let browser_line = workflow_digest(&browser.workflow_id, browser.minutes_saved);
    assert!(browser_line.contains("browser-auto"));
    assert!(browser_line.contains("9") || browser_line.contains("minutes"));
}
