//! One-click export/import of workflows, grants, and settings (C21/FR-U9).
//!
//! A single self-describing JSON file containing workflows, workflow versions,
//! gates, metrics, and settings. Deterministic serialization allows round-trip
//! comparison by hash.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::misc::{GateRecord, MetricsRecord, WorkflowRecord, WorkflowVersionRecord};
use crate::store::Recorder;

/// Export container: version, workflows, and settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportData {
    pub version: i32,
    pub workflows: Vec<WorkflowRecord>,
    pub workflow_versions: Vec<WorkflowVersionRecord>,
    pub gates: Vec<GateRecord>,
    pub metrics: Vec<MetricsRecord>,
    pub settings: BTreeMap<String, serde_json::Value>,
}

impl Recorder {
    /// List all workflows.
    pub fn list_workflows(&self) -> Result<Vec<WorkflowRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, version, dsl_path, manifest_json, signature, source_run_id
             FROM workflows ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|(id, name, version, dsl_path, manifest_json, signature, source_run_id)| {
                let manifest = match manifest_json {
                    Some(s) => Some(serde_json::from_str(&s)?),
                    None => None,
                };
                Ok(WorkflowRecord { id, name, version, dsl_path, manifest, signature, source_run_id })
            })
            .collect()
    }

    /// List all workflow versions, ordered by workflow_id and timestamp.
    pub fn list_all_workflow_versions(&self) -> Result<Vec<WorkflowVersionRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT workflow_id, version, diff_path, approved_by, ts
             FROM workflow_versions ORDER BY workflow_id ASC, ts ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
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

    /// List all gates.
    pub fn list_gates(&self) -> Result<Vec<GateRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, step_ref, kind, expr_json, on_fail
             FROM gates ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|(id, workflow_id, step_ref, kind, expr_json, on_fail)| {
                Ok(GateRecord {
                    id,
                    workflow_id,
                    step_ref,
                    kind,
                    expr: serde_json::from_str(&expr_json)?,
                    on_fail,
                })
            })
            .collect()
    }

    /// List all metrics records.
    pub fn list_metrics(&self) -> Result<Vec<MetricsRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT workflow_id, week, runs, explore_ms, replay_p50_ms, minutes_saved_est
             FROM metrics ORDER BY workflow_id ASC, week ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(MetricsRecord {
                    workflow_id: r.get(0)?,
                    week: r.get(1)?,
                    runs: r.get(2)?,
                    explore_ms: r.get(3)?,
                    replay_p50_ms: r.get(4)?,
                    minutes_saved_est: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

/// Export all workflows, workflow versions, gates, metrics, and settings to a
/// JSON-encoded byte string. The result is deterministic (workflows, versions,
/// gates, metrics sorted by id/timestamp) for hash-based round-trip comparison.
///
/// Exported workflows have source_run_id set to None, since run history is
/// specific to the source recorder and not portable to a fresh import.
pub fn export(recorder: &Recorder, settings: &BTreeMap<String, serde_json::Value>) -> Result<Vec<u8>> {
    let mut workflows = recorder.list_workflows()?;
    // Clear source_run_id since runs are not exported and would fail foreign
    // key constraints when importing into a fresh database.
    for wf in &mut workflows {
        wf.source_run_id = None;
    }
    let data = ExportData {
        version: 1,
        workflows,
        workflow_versions: recorder.list_all_workflow_versions()?,
        gates: recorder.list_gates()?,
        metrics: recorder.list_metrics()?,
        settings: settings.clone(),
    };
    // Use compact JSON for deterministic output (no extra spaces).
    Ok(serde_json::to_vec(&data)?)
}

/// Import workflows, workflow versions, gates, metrics, and settings from an
/// exported JSON byte string into a fresh Recorder and settings map.
/// Returns the populated ExportData on success.
pub fn import(bytes: &[u8], recorder: &Recorder, settings: &mut BTreeMap<String, serde_json::Value>) -> Result<ExportData> {
    let data: ExportData = serde_json::from_slice(bytes)?;

    // Create all workflows.
    for wf in &data.workflows {
        recorder.create_workflow(wf)?;
    }

    // Create all workflow versions, preserving timestamps.
    for wv in &data.workflow_versions {
        recorder.add_workflow_version_with_ts(&wv.workflow_id, &wv.version, wv.diff_path.as_deref(), wv.approved_by.as_deref(), Some(wv.ts))?;
    }

    // Create all gates.
    for gate in &data.gates {
        recorder.create_gate(gate)?;
    }

    // Upsert all metrics.
    for m in &data.metrics {
        recorder.upsert_metrics(&m.workflow_id, &m.week, m.runs, m.explore_ms, m.replay_p50_ms, m.minutes_saved_est)?;
    }

    // Restore settings.
    *settings = data.settings.clone();

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunMode;

    #[test]
    fn round_trip_workflows_and_settings() {
        // Create source recorder with workflows and settings.
        let source_rec = Recorder::open_in_memory().unwrap();
        let mut source_settings = BTreeMap::new();
        source_settings.insert("model.planner".to_string(), serde_json::json!("mock_planner"));
        source_settings.insert("voice.enabled".to_string(), serde_json::json!(false));

        let run_id = source_rec.start_run("teach workflow", RunMode::Explore, None).unwrap();

        // Create workflows.
        let wf1 = WorkflowRecord {
            id: "wf1".to_string(),
            name: "Write invoice note".to_string(),
            version: "1".to_string(),
            dsl_path: Some("workflows/wf1.ts".to_string()),
            manifest: Some(serde_json::json!({"inputs": []})),
            signature: None,
            source_run_id: Some(run_id.clone()),
        };
        let wf2 = WorkflowRecord {
            id: "wf2".to_string(),
            name: "Send email".to_string(),
            version: "2".to_string(),
            dsl_path: Some("workflows/wf2.ts".to_string()),
            manifest: Some(serde_json::json!({"inputs": [{"name": "recipient"}]})),
            signature: Some("sig2".to_string()),
            source_run_id: None,
        };
        source_rec.create_workflow(&wf1).unwrap();
        source_rec.create_workflow(&wf2).unwrap();

        // Add workflow versions.
        source_rec.add_workflow_version("wf1", "1", None, None).unwrap();
        source_rec.add_workflow_version("wf1", "2", Some("diffs/wf1-v2.patch"), Some("josef")).unwrap();
        source_rec.add_workflow_version("wf2", "1", None, None).unwrap();

        // Create gates.
        let gate1 = GateRecord {
            id: "g1".to_string(),
            workflow_id: Some("wf1".to_string()),
            step_ref: Some("s1".to_string()),
            kind: "post".to_string(),
            expr: serde_json::json!({"op": "matches", "regex": "^ok$"}),
            on_fail: Some("halt".to_string()),
        };
        let gate2 = GateRecord {
            id: "g2".to_string(),
            workflow_id: Some("wf2".to_string()),
            step_ref: None,
            kind: "pre".to_string(),
            expr: serde_json::json!({"op": "equals", "value": true}),
            on_fail: None,
        };
        source_rec.create_gate(&gate1).unwrap();
        source_rec.create_gate(&gate2).unwrap();

        // Add metrics.
        source_rec.upsert_metrics("wf1", "2026-W28", 5, Some(2000), Some(150), Some(10.5)).unwrap();
        source_rec.upsert_metrics("wf2", "2026-W28", 3, Some(1500), None, Some(5.0)).unwrap();

        // Export.
        let export_bytes = export(&source_rec, &source_settings).unwrap();
        let export_hash = blake3::hash(&export_bytes).to_hex().to_string();

        // Import into fresh recorder and settings.
        let target_rec = Recorder::open_in_memory().unwrap();
        let mut target_settings = BTreeMap::new();
        import(&export_bytes, &target_rec, &mut target_settings).unwrap();

        // Verify workflows match (source_run_id is cleared during export).
        let target_wfs = target_rec.list_workflows().unwrap();
        let mut source_wfs = source_rec.list_workflows().unwrap();
        for wf in &mut source_wfs {
            wf.source_run_id = None;
        }
        assert_eq!(target_wfs.len(), source_wfs.len());
        assert_eq!(target_wfs, source_wfs);

        // Verify workflow versions match.
        let target_versions = target_rec.list_all_workflow_versions().unwrap();
        let source_versions = source_rec.list_all_workflow_versions().unwrap();
        assert_eq!(target_versions.len(), source_versions.len());
        assert_eq!(target_versions, source_versions);

        // Verify gates match.
        let target_gates = target_rec.list_gates().unwrap();
        let source_gates = source_rec.list_gates().unwrap();
        assert_eq!(target_gates.len(), source_gates.len());
        assert_eq!(target_gates, source_gates);

        // Verify metrics match.
        let target_metrics = target_rec.list_metrics().unwrap();
        let source_metrics = source_rec.list_metrics().unwrap();
        assert_eq!(target_metrics.len(), source_metrics.len());
        assert_eq!(target_metrics, source_metrics);

        // Verify settings match.
        assert_eq!(target_settings, source_settings);

        // Export again from target and verify hash is identical (deterministic).
        let export_bytes_again = export(&target_rec, &target_settings).unwrap();
        let export_hash_again = blake3::hash(&export_bytes_again).to_hex().to_string();
        assert_eq!(export_hash, export_hash_again, "export must be deterministic");
    }

    #[test]
    fn export_empty_recorder_and_settings() {
        let rec = Recorder::open_in_memory().unwrap();
        let settings = BTreeMap::new();
        let bytes = export(&rec, &settings).unwrap();

        // Should produce valid JSON with empty collections.
        let data: ExportData = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(data.version, 1);
        assert_eq!(data.workflows.len(), 0);
        assert_eq!(data.workflow_versions.len(), 0);
        assert_eq!(data.gates.len(), 0);
        assert_eq!(data.metrics.len(), 0);
        assert_eq!(data.settings.len(), 0);
    }
}
