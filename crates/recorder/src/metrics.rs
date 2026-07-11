//! Time-saved ledger: per-workflow metrics rolled into tray copy and weekly digest.
//!
//! The estimator computes `minutes_saved = (explore_ms - replay_p50_ms) * run_count / 60000`.
//! Weekly roll-ups aggregate per-workflow and system-wide time saved.
//! Plain-language copy helpers render user-facing strings without jargon.

use rusqlite::params;

use crate::error::Result;
use crate::store::Recorder;

/// Estimate minutes saved by comparing explore and replay timings.
///
/// Assumes: user spends `explore_ms` to teach (explore), and each replay takes `replay_p50_ms`.
/// Time saved per run: `explore_ms - replay_p50_ms`. Total over `run_count` runs.
///
/// Returns the time saved in minutes, or 0.0 if explore is faster than or equal to replay
/// (no time saved in that case).
pub fn estimate_minutes_saved(explore_ms: i64, replay_p50_ms: i64, run_count: i64) -> f64 {
    if explore_ms <= replay_p50_ms || run_count == 0 {
        return 0.0;
    }
    let time_saved_per_run_ms = explore_ms - replay_p50_ms;
    (time_saved_per_run_ms as f64 * run_count as f64) / 60_000.0
}

/// Aggregated metrics for a single workflow in a week.
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklyWorkflowMetrics {
    pub workflow_id: String,
    pub week: String,
    pub runs: i64,
    pub explore_ms: i64,
    pub replay_p50_ms: i64,
    pub minutes_saved: f64,
}

/// Aggregated metrics for the entire system in a week.
#[derive(Debug, Clone)]
pub struct WeeklySystemMetrics {
    pub week: String,
    pub total_runs: i64,
    pub total_minutes_saved: f64,
    pub workflows: Vec<WeeklyWorkflowMetrics>,
}

impl Recorder {
    /// Fetch or calculate aggregated metrics for a single workflow in a given week.
    ///
    /// Requires that `explore_ms` and `replay_p50_ms` have been set via prior
    /// [`Recorder::upsert_metrics`] calls; if either is missing, returns the
    /// record as stored (with `minutes_saved` possibly calculated from the
    /// stored value, or 0.0 if stored as None).
    pub fn get_weekly_workflow_metrics(
        &self,
        workflow_id: &str,
        week: &str,
    ) -> Result<Option<WeeklyWorkflowMetrics>> {
        let metric = self.get_metrics(workflow_id, week)?;
        Ok(metric.map(|m| {
            let minutes_saved = m.minutes_saved_est.unwrap_or_else(|| {
                estimate_minutes_saved(
                    m.explore_ms.unwrap_or(0),
                    m.replay_p50_ms.unwrap_or(0),
                    m.runs,
                )
            });
            WeeklyWorkflowMetrics {
                workflow_id: m.workflow_id,
                week: m.week,
                runs: m.runs,
                explore_ms: m.explore_ms.unwrap_or(0),
                replay_p50_ms: m.replay_p50_ms.unwrap_or(0),
                minutes_saved,
            }
        }))
    }

    /// Fetch aggregated metrics for all workflows in a given week.
    pub fn get_weekly_system_metrics(&self, week: &str) -> Result<WeeklySystemMetrics> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT workflow_id, week, runs, explore_ms, replay_p50_ms, minutes_saved_est
             FROM metrics WHERE week = ?1 ORDER BY workflow_id ASC",
        )?;
        let rows = stmt
            .query_map(params![week], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<f64>>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut workflows = Vec::new();
        let mut total_runs = 0i64;
        let mut total_minutes_saved = 0.0;

        for (workflow_id, week_str, runs, explore_ms, replay_p50_ms, stored_minutes_saved) in rows {
            let minutes_saved = stored_minutes_saved.unwrap_or_else(|| {
                estimate_minutes_saved(explore_ms.unwrap_or(0), replay_p50_ms.unwrap_or(0), runs)
            });
            workflows.push(WeeklyWorkflowMetrics {
                workflow_id,
                week: week_str,
                runs,
                explore_ms: explore_ms.unwrap_or(0),
                replay_p50_ms: replay_p50_ms.unwrap_or(0),
                minutes_saved,
            });
            total_runs += runs;
            total_minutes_saved += minutes_saved;
        }

        Ok(WeeklySystemMetrics { week: week.to_string(), total_runs, total_minutes_saved, workflows })
    }
}

/// Render plain-language human-readable copy for time saved.
///
/// Examples:
/// - `digest_copy(3.5)` -> "Operant saved you 3 hours and 30 minutes this week"
/// - `digest_copy(0.25)` -> "Operant saved you 15 minutes this week"
/// - `digest_copy(45.0)` -> "Operant saved you 45 hours this week"
/// - `digest_copy(0.0)` -> "No time saved this week yet"
pub fn digest_copy(total_minutes: f64) -> String {
    if total_minutes <= 0.0 {
        return "No time saved this week yet".to_string();
    }

    let hours = (total_minutes / 60.0).floor() as i64;
    let minutes = (total_minutes % 60.0).round() as i64;

    if hours == 0 {
        if minutes == 1 {
            "Operant saved you 1 minute this week".to_string()
        } else {
            format!("Operant saved you {} minutes this week", minutes)
        }
    } else if minutes == 0 {
        if hours == 1 {
            "Operant saved you 1 hour this week".to_string()
        } else {
            format!("Operant saved you {} hours this week", hours)
        }
    } else if minutes == 1 {
        if hours == 1 {
            "Operant saved you 1 hour and 1 minute this week".to_string()
        } else {
            format!("Operant saved you {} hours and 1 minute this week", hours)
        }
    } else if hours == 1 {
        format!("Operant saved you 1 hour and {} minutes this week", minutes)
    } else {
        format!("Operant saved you {} hours and {} minutes this week", hours, minutes)
    }
}

/// Render a short digest line for a single workflow's time savings.
///
/// Examples:
/// - `workflow_digest("invoice-writer", 2.5)` -> "invoice-writer: 2 hours and 30 minutes"
/// - `workflow_digest("browser-auto", 0.75)` -> "browser-auto: 45 minutes"
pub fn workflow_digest(workflow_name: &str, minutes: f64) -> String {
    if minutes <= 0.0 {
        format!("{}: no time saved", workflow_name)
    } else {
        let hours = (minutes / 60.0).floor() as i64;
        let mins = (minutes % 60.0).round() as i64;

        if hours == 0 {
            if mins == 1 {
                format!("{}: 1 minute", workflow_name)
            } else {
                format!("{}: {} minutes", workflow_name, mins)
            }
        } else if mins == 0 {
            if hours == 1 {
                format!("{}: 1 hour", workflow_name)
            } else {
                format!("{}: {} hours", workflow_name, hours)
            }
        } else if mins == 1 {
            if hours == 1 {
                format!("{}: 1 hour and 1 minute", workflow_name)
            } else {
                format!("{}: {} hours and 1 minute", workflow_name, hours)
            }
        } else if hours == 1 {
            format!("{}: 1 hour and {} minutes", workflow_name, mins)
        } else {
            format!("{}: {} hours and {} minutes", workflow_name, hours, mins)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_minutes_saved_basic() {
        // 5s explore, 0.5s replay, 100 runs => 4.5s * 100 = 450s = 7.5 min
        assert_eq!(estimate_minutes_saved(5000, 500, 100), 7.5);
    }

    #[test]
    fn estimate_minutes_saved_no_savings_when_replay_faster() {
        // If replay is faster than explore (no time saved)
        assert_eq!(estimate_minutes_saved(100, 500, 10), 0.0);
    }

    #[test]
    fn estimate_minutes_saved_zero_runs() {
        assert_eq!(estimate_minutes_saved(5000, 500, 0), 0.0);
    }

    #[test]
    fn estimate_minutes_saved_equal_times() {
        // If explore and replay times are equal
        assert_eq!(estimate_minutes_saved(500, 500, 10), 0.0);
    }

    #[test]
    fn estimate_minutes_saved_fixture_workflow() {
        // Fixture: a workflow taking 30 seconds to explore, 2 seconds p50 replay, run 50 times
        // 28s * 50 = 1400s = 23.33 minutes
        let minutes = estimate_minutes_saved(30_000, 2_000, 50);
        assert!((minutes - 23.33).abs() < 0.01);
    }

    #[test]
    fn digest_copy_zero_minutes() {
        assert_eq!(digest_copy(0.0), "No time saved this week yet");
    }

    #[test]
    fn digest_copy_minutes_only() {
        assert_eq!(digest_copy(15.0), "Operant saved you 15 minutes this week");
        assert_eq!(digest_copy(1.0), "Operant saved you 1 minute this week");
    }

    #[test]
    fn digest_copy_hours_only() {
        assert_eq!(digest_copy(60.0), "Operant saved you 1 hour this week");
        assert_eq!(digest_copy(120.0), "Operant saved you 2 hours this week");
    }

    #[test]
    fn digest_copy_hours_and_minutes() {
        assert_eq!(digest_copy(90.0), "Operant saved you 1 hour and 30 minutes this week");
        assert_eq!(
            digest_copy(125.0),
            "Operant saved you 2 hours and 5 minutes this week"
        );
    }

    #[test]
    fn digest_copy_many_hours() {
        assert_eq!(digest_copy(300.0), "Operant saved you 5 hours this week");
        assert_eq!(
            digest_copy(330.5),
            "Operant saved you 5 hours and 31 minutes this week"
        );
    }

    #[test]
    fn workflow_digest_minutes() {
        assert_eq!(workflow_digest("invoice-writer", 30.0), "invoice-writer: 30 minutes");
        assert_eq!(workflow_digest("browser-auto", 1.0), "browser-auto: 1 minute");
    }

    #[test]
    fn workflow_digest_hours() {
        assert_eq!(workflow_digest("invoice-writer", 60.0), "invoice-writer: 1 hour");
        assert_eq!(workflow_digest("browser-auto", 120.0), "browser-auto: 2 hours");
    }

    #[test]
    fn workflow_digest_mixed() {
        assert_eq!(
            workflow_digest("invoice-writer", 90.0),
            "invoice-writer: 1 hour and 30 minutes"
        );
        assert_eq!(
            workflow_digest("browser-auto", 125.0),
            "browser-auto: 2 hours and 5 minutes"
        );
    }

    #[test]
    fn workflow_digest_zero() {
        assert_eq!(workflow_digest("invoice-writer", 0.0), "invoice-writer: no time saved");
    }

    #[test]
    fn get_weekly_workflow_metrics_integration() {
        let rec = Recorder::open_in_memory().unwrap();
        rec.upsert_metrics("wf1", "2026-W28", 50, Some(30_000), Some(2_000), None).unwrap();

        let metrics = rec.get_weekly_workflow_metrics("wf1", "2026-W28").unwrap();
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m.workflow_id, "wf1");
        assert_eq!(m.week, "2026-W28");
        assert_eq!(m.runs, 50);
        assert_eq!(m.explore_ms, 30_000);
        assert_eq!(m.replay_p50_ms, 2_000);
        assert!((m.minutes_saved - 23.33).abs() < 0.01);
    }

    #[test]
    fn get_weekly_system_metrics_aggregates() {
        let rec = Recorder::open_in_memory().unwrap();
        // First workflow: 30s explore, 2s replay, 50 runs = 23.33 minutes
        rec.upsert_metrics("wf1", "2026-W28", 50, Some(30_000), Some(2_000), None).unwrap();
        // Second workflow: 60s explore, 5s replay, 20 runs = 18.33 minutes
        rec.upsert_metrics("wf2", "2026-W28", 20, Some(60_000), Some(5_000), None).unwrap();

        let system = rec.get_weekly_system_metrics("2026-W28").unwrap();
        assert_eq!(system.week, "2026-W28");
        assert_eq!(system.total_runs, 70);
        assert!((system.total_minutes_saved - 41.67).abs() < 0.01);
        assert_eq!(system.workflows.len(), 2);
    }

    #[test]
    fn get_weekly_system_metrics_empty_week() {
        let rec = Recorder::open_in_memory().unwrap();
        let system = rec.get_weekly_system_metrics("2099-W01").unwrap();
        assert_eq!(system.week, "2099-W01");
        assert_eq!(system.total_runs, 0);
        assert_eq!(system.total_minutes_saved, 0.0);
        assert_eq!(system.workflows.len(), 0);
    }

    #[test]
    fn digest_copy_plain_language() {
        // Test that copy is jargon-free and human-readable
        let copies = vec![
            digest_copy(0.0),
            digest_copy(15.0),
            digest_copy(60.0),
            digest_copy(125.0),
        ];
        for copy in copies {
            // Should never contain words like "ms", "milliseconds", "explore", "replay", "estimate"
            assert!(!copy.contains("ms"));
            assert!(!copy.contains("milliseconds"));
            assert!(!copy.contains("explore"));
            assert!(!copy.contains("replay"));
            assert!(!copy.contains("estimate"));
        }
    }
}
