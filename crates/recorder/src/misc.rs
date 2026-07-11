//! CRUD-lite accessors for the tables whose real logic lives in other lanes:
//! `workflows`, `workflow_versions`, `gates`, `audit`, `undo_journal`, `metrics`.
//! Enough to create rows, read them back, and (for `audit`) verify the hash chain;
//! the compiler, gate engine, and safety lanes own the rest of the behavior.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ids::now_ms;
use crate::store::Recorder;

// ---------------------------------------------------------------- workflows

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dsl_path: Option<String>,
    pub manifest: Option<serde_json::Value>,
    pub signature: Option<String>,
    pub source_run_id: Option<String>,
}

impl Recorder {
    pub fn create_workflow(&self, w: &WorkflowRecord) -> Result<()> {
        let manifest_json = match &w.manifest {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO workflows (id, name, version, dsl_path, manifest_json, signature, source_run_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![w.id, w.name, w.version, w.dsl_path, manifest_json, w.signature, w.source_run_id],
        )?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<WorkflowRecord>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, name, version, dsl_path, manifest_json, signature, source_run_id
                 FROM workflows WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, name, version, dsl_path, manifest_json, signature, source_run_id)) = row else {
            return Ok(None);
        };
        let manifest = match manifest_json {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        };
        Ok(Some(WorkflowRecord { id, name, version, dsl_path, manifest, signature, source_run_id }))
    }
}

// -------------------------------------------------------- workflow_versions

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowVersionRecord {
    pub workflow_id: String,
    pub version: String,
    pub diff_path: Option<String>,
    pub approved_by: Option<String>,
    pub ts: i64,
}

impl Recorder {
    pub fn add_workflow_version(
        &self,
        workflow_id: &str,
        version: &str,
        diff_path: Option<&str>,
        approved_by: Option<&str>,
    ) -> Result<()> {
        let ts = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO workflow_versions (workflow_id, version, diff_path, approved_by, ts)
             VALUES (?1,?2,?3,?4,?5)",
            params![workflow_id, version, diff_path, approved_by, ts],
        )?;
        Ok(())
    }

    pub fn list_workflow_versions(&self, workflow_id: &str) -> Result<Vec<WorkflowVersionRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT workflow_id, version, diff_path, approved_by, ts
             FROM workflow_versions WHERE workflow_id = ?1 ORDER BY ts ASC",
        )?;
        let rows = stmt
            .query_map(params![workflow_id], |r| {
                Ok(WorkflowVersionRecord {
                    workflow_id: r.get(0)?,
                    version: r.get(1)?,
                    diff_path: r.get(2)?,
                    approved_by: r.get(3)?,
                    ts: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

// ------------------------------------------------------------------- gates

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateRecord {
    pub id: String,
    pub workflow_id: Option<String>,
    pub step_ref: Option<String>,
    pub kind: String,
    pub expr: serde_json::Value,
    pub on_fail: Option<String>,
}

impl Recorder {
    pub fn create_gate(&self, g: &GateRecord) -> Result<()> {
        let expr_json = serde_json::to_string(&g.expr)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO gates (id, workflow_id, step_ref, kind, expr_json, on_fail)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![g.id, g.workflow_id, g.step_ref, g.kind, expr_json, g.on_fail],
        )?;
        Ok(())
    }

    pub fn get_gate(&self, id: &str) -> Result<Option<GateRecord>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, workflow_id, step_ref, kind, expr_json, on_fail FROM gates WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, workflow_id, step_ref, kind, expr_json, on_fail)) = row else {
            return Ok(None);
        };
        Ok(Some(GateRecord { id, workflow_id, step_ref, kind, expr: serde_json::from_str(&expr_json)?, on_fail }))
    }
}

// ------------------------------------------------------------------- audit

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub seq: i64,
    pub ts: i64,
    pub actor: String,
    pub event: serde_json::Value,
    pub prev_hash: Option<String>,
    pub hash: String,
}

impl Recorder {
    /// Append one hash-chained audit entry: `hash = blake3(prev_hash || ts || actor
    /// || event_json)`, chained from the previous row so any edit or reordering of
    /// history breaks the chain and is detectable by [`Recorder::verify_audit_chain`].
    pub fn append_audit(&self, actor: &str, event: &serde_json::Value) -> Result<i64> {
        let event_json = serde_json::to_string(event)?;
        let ts = now_ms();
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let prev_hash: Option<String> = tx
            .query_row("SELECT hash FROM audit ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
            .optional()?;
        let hash = chain_hash(prev_hash.as_deref(), ts, actor, &event_json);
        tx.execute(
            "INSERT INTO audit (ts, actor, event_json, prev_hash, hash) VALUES (?1,?2,?3,?4,?5)",
            params![ts, actor, event_json, prev_hash, hash],
        )?;
        let seq = tx.last_insert_rowid();
        tx.commit()?;
        Ok(seq)
    }

    pub fn list_audit(&self) -> Result<Vec<AuditRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT seq, ts, actor, event_json, prev_hash, hash FROM audit ORDER BY seq ASC",
        )?;
        let raw = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|(seq, ts, actor, event_json, prev_hash, hash)| {
                Ok(AuditRecord { seq, ts, actor, event: serde_json::from_str(&event_json)?, prev_hash, hash })
            })
            .collect()
    }

    /// Recompute every row's hash from its neighbor and content, returning `Ok(())`
    /// if the chain is intact or the seq of the first row that fails to verify.
    pub fn verify_audit_chain(&self) -> Result<std::result::Result<(), i64>> {
        let rows = self.list_audit()?;
        let mut expected_prev: Option<String> = None;
        for row in &rows {
            if row.prev_hash != expected_prev {
                return Ok(Err(row.seq));
            }
            let event_json = serde_json::to_string(&row.event)?;
            let recomputed = chain_hash(row.prev_hash.as_deref(), row.ts, &row.actor, &event_json);
            if recomputed != row.hash {
                return Ok(Err(row.seq));
            }
            expected_prev = Some(row.hash.clone());
        }
        Ok(Ok(()))
    }
}

fn chain_hash(prev_hash: Option<&str>, ts: i64, actor: &str, event_json: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(prev_hash.unwrap_or("").as_bytes());
    hasher.update(ts.to_le_bytes().as_slice());
    hasher.update(actor.as_bytes());
    hasher.update(event_json.as_bytes());
    hasher.finalize().to_hex().to_string()
}

// ----------------------------------------------------------- undo_journal

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UndoEntry {
    pub run_id: String,
    pub seq: u32,
    pub inverse_action: Option<serde_json::Value>,
    pub applied: bool,
    pub ts: i64,
}

impl Recorder {
    pub fn append_undo(&self, run_id: &str, seq: u32, inverse_action: Option<&serde_json::Value>) -> Result<()> {
        let inverse_json = match inverse_action {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let ts = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO undo_journal (run_id, seq, inverse_action_ir_json, applied, ts)
             VALUES (?1,?2,?3,0,?4)",
            params![run_id, seq, inverse_json, ts],
        )?;
        Ok(())
    }

    pub fn mark_undo_applied(&self, run_id: &str, seq: u32) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE undo_journal SET applied = 1 WHERE run_id = ?1 AND seq = ?2",
            params![run_id, seq],
        )?;
        Ok(())
    }

    pub fn list_undo(&self, run_id: &str) -> Result<Vec<UndoEntry>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT run_id, seq, inverse_action_ir_json, applied, ts
             FROM undo_journal WHERE run_id = ?1 ORDER BY seq ASC",
        )?;
        let raw = stmt
            .query_map(params![run_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|(run_id, seq, inverse_json, applied, ts)| {
                Ok(UndoEntry {
                    run_id,
                    seq: seq as u32,
                    inverse_action: inverse_json.map(|s| serde_json::from_str(&s)).transpose()?,
                    applied: applied != 0,
                    ts,
                })
            })
            .collect()
    }
}

// --------------------------------------------------------------- metrics

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsRecord {
    pub workflow_id: String,
    pub week: String,
    pub runs: i64,
    pub explore_ms: Option<i64>,
    pub replay_p50_ms: Option<i64>,
    pub minutes_saved_est: Option<f64>,
}

impl Recorder {
    /// Add `runs_delta` to the run count for `(workflow_id, week)`, creating the row
    /// if needed, and overwrite the timing/savings fields when provided.
    pub fn upsert_metrics(
        &self,
        workflow_id: &str,
        week: &str,
        runs_delta: i64,
        explore_ms: Option<i64>,
        replay_p50_ms: Option<i64>,
        minutes_saved_est: Option<f64>,
    ) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO metrics (workflow_id, week, runs, explore_ms, replay_p50_ms, minutes_saved_est)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(workflow_id, week) DO UPDATE SET
                runs = runs + excluded.runs,
                explore_ms = COALESCE(excluded.explore_ms, metrics.explore_ms),
                replay_p50_ms = COALESCE(excluded.replay_p50_ms, metrics.replay_p50_ms),
                minutes_saved_est = COALESCE(excluded.minutes_saved_est, metrics.minutes_saved_est)",
            params![workflow_id, week, runs_delta, explore_ms, replay_p50_ms, minutes_saved_est],
        )?;
        Ok(())
    }

    pub fn get_metrics(&self, workflow_id: &str, week: &str) -> Result<Option<MetricsRecord>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT workflow_id, week, runs, explore_ms, replay_p50_ms, minutes_saved_est
             FROM metrics WHERE workflow_id = ?1 AND week = ?2",
            params![workflow_id, week],
            |r| {
                Ok(MetricsRecord {
                    workflow_id: r.get(0)?,
                    week: r.get(1)?,
                    runs: r.get(2)?,
                    explore_ms: r.get(3)?,
                    replay_p50_ms: r.get(4)?,
                    minutes_saved_est: r.get(5)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunMode;

    #[test]
    fn workflow_crud() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        let w = WorkflowRecord {
            id: "wf1".into(),
            name: "Write invoice note".into(),
            version: "1".into(),
            dsl_path: Some("workflows/wf1.ts".into()),
            manifest: Some(serde_json::json!({"inputs": []})),
            signature: None,
            source_run_id: Some(run_id),
        };
        rec.create_workflow(&w).unwrap();
        let fetched = rec.get_workflow("wf1").unwrap().unwrap();
        assert_eq!(fetched, w);
        assert!(rec.get_workflow("missing").unwrap().is_none());
    }

    #[test]
    fn workflow_versions_list_in_order() {
        let rec = Recorder::open_in_memory().unwrap();
        rec.create_workflow(&WorkflowRecord {
            id: "wf1".into(),
            name: "n".into(),
            version: "1".into(),
            dsl_path: None,
            manifest: None,
            signature: None,
            source_run_id: None,
        })
        .unwrap();
        rec.add_workflow_version("wf1", "1", None, None).unwrap();
        rec.add_workflow_version("wf1", "2", Some("diffs/2.patch"), Some("josef")).unwrap();
        let versions = rec.list_workflow_versions("wf1").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[1].version, "2");
        assert_eq!(versions[1].approved_by.as_deref(), Some("josef"));
    }

    #[test]
    fn gate_crud() {
        let rec = Recorder::open_in_memory().unwrap();
        let g = GateRecord {
            id: "g1".into(),
            workflow_id: None,
            step_ref: Some("s5".into()),
            kind: "post".into(),
            expr: serde_json::json!({"op": "matches", "regex": "^ok$"}),
            on_fail: Some("halt".into()),
        };
        rec.create_gate(&g).unwrap();
        assert_eq!(rec.get_gate("g1").unwrap().unwrap(), g);
    }

    #[test]
    fn audit_chain_is_verifiable_and_detects_tampering() {
        let rec = Recorder::open_in_memory().unwrap();
        rec.append_audit("system", &serde_json::json!({"event": "boot"})).unwrap();
        rec.append_audit("josef", &serde_json::json!({"event": "grant", "scope": "fs:Downloads"})).unwrap();
        rec.append_audit("system", &serde_json::json!({"event": "run_started"})).unwrap();

        let rows = rec.list_audit().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].prev_hash, None);
        assert_eq!(rows[1].prev_hash.as_deref(), Some(rows[0].hash.as_str()));
        assert_eq!(rows[2].prev_hash.as_deref(), Some(rows[1].hash.as_str()));
        assert_eq!(rec.verify_audit_chain().unwrap(), Ok(()));

        // Tamper directly with the row storage and confirm the chain now fails.
        {
            let conn = rec.lock().unwrap();
            conn.execute("UPDATE audit SET event_json = '{\"event\":\"tampered\"}' WHERE seq = 2", [])
                .unwrap();
        }
        let result = rec.verify_audit_chain().unwrap();
        assert_eq!(result, Err(2));
    }

    #[test]
    fn undo_journal_crud() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        rec.append_undo(&run_id, 1, Some(&serde_json::json!({"kind": "key", "params": {"combo": "ctrl+z"}})))
            .unwrap();
        rec.append_undo(&run_id, 2, None).unwrap();
        rec.mark_undo_applied(&run_id, 1).unwrap();

        let entries = rec.list_undo(&run_id).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].applied);
        assert!(!entries[1].applied);
    }

    #[test]
    fn metrics_upsert_accumulates_runs_and_overwrites_timings() {
        let rec = Recorder::open_in_memory().unwrap();
        rec.upsert_metrics("wf1", "2026-W28", 1, Some(1200), None, Some(3.5)).unwrap();
        rec.upsert_metrics("wf1", "2026-W28", 1, None, Some(180), Some(4.0)).unwrap();

        let m = rec.get_metrics("wf1", "2026-W28").unwrap().unwrap();
        assert_eq!(m.runs, 2, "run counts accumulate");
        assert_eq!(m.explore_ms, Some(1200), "unset field keeps the earlier value");
        assert_eq!(m.replay_p50_ms, Some(180));
        assert_eq!(m.minutes_saved_est, Some(4.0));

        assert!(rec.get_metrics("wf1", "2099-W01").unwrap().is_none());
    }
}
