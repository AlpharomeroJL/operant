//! `runs`: one row per explore/replay/dry-run session.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{RecorderError, Result};
use crate::ids::{new_id, now_ms};
use crate::store::Recorder;

/// Mirrors `runs.mode` in `docs/ARCHITECTURE.md` section 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Explore,
    Replay,
    Dry,
}

impl RunMode {
    fn as_str(self) -> &'static str {
        match self {
            RunMode::Explore => "explore",
            RunMode::Replay => "replay",
            RunMode::Dry => "dry",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "explore" => Ok(RunMode::Explore),
            "replay" => Ok(RunMode::Replay),
            "dry" => Ok(RunMode::Dry),
            other => Err(RecorderError::InvalidInput(format!("unknown run mode: {other}"))),
        }
    }
}

/// Mirrors `runs.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Aborted,
}

impl RunStatus {
    fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Aborted => "aborted",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "running" => Ok(RunStatus::Running),
            "completed" => Ok(RunStatus::Completed),
            "failed" => Ok(RunStatus::Failed),
            "aborted" => Ok(RunStatus::Aborted),
            other => Err(RecorderError::InvalidInput(format!("unknown run status: {other}"))),
        }
    }
}

/// A `runs` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub goal: String,
    pub mode: RunMode,
    pub started: i64,
    pub ended: Option<i64>,
    pub status: RunStatus,
    pub model_config: Option<serde_json::Value>,
}

impl Recorder {
    /// Start a new run: inserts a `runs` row with `status = running` and returns its id.
    pub fn start_run(
        &self,
        goal: &str,
        mode: RunMode,
        model_config: Option<serde_json::Value>,
    ) -> Result<String> {
        let id = new_id("run");
        let started = now_ms();
        let model_config_json = match &model_config {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO runs (id, goal, mode, started, ended, status, model_config_json)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)",
            params![id, goal, mode.as_str(), started, RunStatus::Running.as_str(), model_config_json],
        )?;
        Ok(id)
    }

    /// End a run: sets `status` and `ended`. Errors with [`RecorderError::RunNotFound`]
    /// if the run does not exist.
    pub fn end_run(&self, run_id: &str, status: RunStatus) -> Result<()> {
        let ended = now_ms();
        let conn = self.lock()?;
        let changed = conn.execute(
            "UPDATE runs SET status = ?1, ended = ?2 WHERE id = ?3",
            params![status.as_str(), ended, run_id],
        )?;
        if changed == 0 {
            return Err(RecorderError::RunNotFound(run_id.to_string()));
        }
        Ok(())
    }

    /// Fetch one run by id.
    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, goal, mode, started, ended, status, model_config_json
                 FROM runs WHERE id = ?1",
                params![run_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, goal, mode, started, ended, status, model_config_json)) = row else {
            return Ok(None);
        };
        let model_config = match model_config_json {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        };
        Ok(Some(RunRecord {
            id,
            goal,
            mode: RunMode::parse(&mode)?,
            started,
            ended,
            status: RunStatus::parse(&status)?,
            model_config,
        }))
    }

    /// List all run ids, most recently started first. CRUD-lite helper for tests and
    /// callers that want to enumerate runs without a dedicated query surface.
    pub fn list_runs(&self) -> Result<Vec<String>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT id FROM runs ORDER BY started DESC")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_end_and_get_run() {
        let rec = Recorder::open_in_memory().unwrap();
        let id = rec.start_run("write a note", RunMode::Explore, None).unwrap();
        let run = rec.get_run(&id).unwrap().expect("run present");
        assert_eq!(run.goal, "write a note");
        assert_eq!(run.mode, RunMode::Explore);
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.ended.is_none());

        rec.end_run(&id, RunStatus::Completed).unwrap();
        let run = rec.get_run(&id).unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert!(run.ended.is_some());
        assert!(run.ended.unwrap() >= run.started);
    }

    #[test]
    fn end_run_missing_errors() {
        let rec = Recorder::open_in_memory().unwrap();
        let err = rec.end_run("nope", RunStatus::Completed).unwrap_err();
        assert!(matches!(err, RecorderError::RunNotFound(_)));
    }

    #[test]
    fn get_run_missing_is_none() {
        let rec = Recorder::open_in_memory().unwrap();
        assert!(rec.get_run("nope").unwrap().is_none());
    }

    #[test]
    fn model_config_round_trips() {
        let rec = Recorder::open_in_memory().unwrap();
        let cfg = serde_json::json!({"planner": "mock_planner", "grounder": "mock_grounder"});
        let id = rec.start_run("goal", RunMode::Explore, Some(cfg.clone())).unwrap();
        let run = rec.get_run(&id).unwrap().unwrap();
        assert_eq!(run.model_config, Some(cfg));
    }

    #[test]
    fn list_runs_returns_started_ids() {
        let rec = Recorder::open_in_memory().unwrap();
        let a = rec.start_run("a", RunMode::Explore, None).unwrap();
        let b = rec.start_run("b", RunMode::Replay, None).unwrap();
        let mut ids = rec.list_runs().unwrap();
        ids.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(ids, expected);
    }
}
