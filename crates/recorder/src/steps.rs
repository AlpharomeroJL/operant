//! `steps`: one row per recorded Action IR step within a run.
//!
//! Shape follows `contracts/fixtures/trajectory_notepad.json` (the frozen compiler
//! input fixture) rather than the abbreviated single-`snapshot_digest` sketch in
//! `docs/ARCHITECTURE.md` section 3: per `operant-contracts`, the fixture wins on
//! disagreement. That fixture carries a digest *before* and *after* each step (the
//! compiler's pass 4 needs both to decide where waits are required), plus an optional
//! human correction, free-text note, and an `outcome_bearing` flag on the final
//! postcondition-checking step.

use operant_ir::{Action, Grounding};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::{RecorderError, Result};
use crate::ids::{new_id, now_ms};
use crate::store::Recorder;

fn grounding_str(g: Grounding) -> &'static str {
    match g {
        Grounding::Uia => "uia",
        Grounding::Vision => "vision",
        Grounding::Adapter => "adapter",
    }
}

fn grounding_parse(s: &str) -> Result<Grounding> {
    match s {
        "uia" => Ok(Grounding::Uia),
        "vision" => Ok(Grounding::Vision),
        "adapter" => Ok(Grounding::Adapter),
        other => Err(RecorderError::InvalidInput(format!("unknown grounding: {other}"))),
    }
}

/// Input to [`Recorder::record_step`].
#[derive(Debug, Clone)]
pub struct NewStep {
    pub seq: u32,
    pub action: Action,
    pub grounding: Grounding,
    pub snapshot_digest_before: Option<String>,
    pub snapshot_digest_after: Option<String>,
    pub outcome: String,
    pub ms: u64,
    pub note: Option<String>,
    pub human_correction: Option<serde_json::Value>,
    pub outcome_bearing: bool,
}

impl NewStep {
    /// Convenience constructor for the common case: no note, no correction, not
    /// outcome-bearing.
    pub fn new(seq: u32, action: Action, grounding: Grounding, outcome: impl Into<String>, ms: u64) -> Self {
        NewStep {
            seq,
            action,
            grounding,
            snapshot_digest_before: None,
            snapshot_digest_after: None,
            outcome: outcome.into(),
            ms,
            note: None,
            human_correction: None,
            outcome_bearing: false,
        }
    }

    pub fn with_digests(mut self, before: Option<String>, after: Option<String>) -> Self {
        self.snapshot_digest_before = before;
        self.snapshot_digest_after = after;
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn with_human_correction(mut self, correction: serde_json::Value) -> Self {
        self.human_correction = Some(correction);
        self
    }

    pub fn outcome_bearing(mut self, yes: bool) -> Self {
        self.outcome_bearing = yes;
        self
    }
}

/// A `steps` row, decoded back into typed IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    pub id: String,
    pub run_id: String,
    pub seq: u32,
    pub action: Action,
    pub grounding: Grounding,
    pub snapshot_digest_before: Option<String>,
    pub snapshot_digest_after: Option<String>,
    pub outcome: String,
    pub ms: u64,
    pub note: Option<String>,
    pub human_correction: Option<serde_json::Value>,
    pub outcome_bearing: bool,
    pub created_at: i64,
}

type RawStepRow = (
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

const STEP_COLUMNS: &str = "id, run_id, seq, action_ir_json, grounding, snapshot_digest_before, \
    snapshot_digest_after, outcome, ms, note, human_correction_json, outcome_bearing, created_at";

fn row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawStepRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
    ))
}

fn raw_to_record(raw: RawStepRow) -> Result<StepRecord> {
    let (id, run_id, seq, action_json, grounding, before, after, outcome, ms, note, hc, outcome_bearing, created_at) = raw;
    Ok(StepRecord {
        id,
        run_id,
        seq: seq as u32,
        action: serde_json::from_str(&action_json)?,
        grounding: grounding_parse(&grounding)?,
        snapshot_digest_before: before,
        snapshot_digest_after: after,
        outcome,
        ms: ms as u64,
        note,
        human_correction: hc.map(|s| serde_json::from_str(&s)).transpose()?,
        outcome_bearing: outcome_bearing != 0,
        created_at,
    })
}

impl Recorder {
    /// Record one step of `run_id` as a single committed transaction: either the
    /// whole row lands durably or none of it does. Crash-safety (test b) relies on
    /// this being one transaction per step, on a WAL-mode connection.
    pub fn record_step(&self, run_id: &str, step: NewStep) -> Result<String> {
        let id = new_id("step");
        let action_json = serde_json::to_string(&step.action)?;
        let human_correction_json = match &step.human_correction {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let created_at = now_ms();

        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO steps ({STEP_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"),
            params![
                id,
                run_id,
                step.seq,
                action_json,
                grounding_str(step.grounding),
                step.snapshot_digest_before,
                step.snapshot_digest_after,
                step.outcome,
                step.ms as i64,
                step.note,
                human_correction_json,
                step.outcome_bearing as i64,
                created_at,
            ],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// All steps of a run, ordered by `seq` ascending.
    pub fn list_steps(&self, run_id: &str) -> Result<Vec<StepRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {STEP_COLUMNS} FROM steps WHERE run_id = ?1 ORDER BY seq ASC"
        ))?;
        let raw = stmt
            .query_map(params![run_id], row_to_raw)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter().map(raw_to_record).collect()
    }

    /// One step by its own id.
    pub fn get_step(&self, step_id: &str) -> Result<Option<StepRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(&format!("SELECT {STEP_COLUMNS} FROM steps WHERE id = ?1"))?;
        let mut rows = stmt.query(params![step_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(raw_to_record(row_to_raw(row)?)?)),
            None => Ok(None),
        }
    }

    /// Count of steps recorded for a run. Cheaper than `list_steps(..).len()` for
    /// callers that only need the count (e.g. progress display).
    pub fn step_count(&self, run_id: &str) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn.query_row(
            "SELECT count(*) FROM steps WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunMode;
    use operant_ir::{ActionKind, RiskClass};

    fn sample_action(id: &str) -> Action {
        Action {
            v: 1,
            id: id.to_string(),
            kind: ActionKind::Key,
            intent: Some("save".into()),
            target: None,
            params: {
                let mut m = serde_json::Map::new();
                m.insert("combo".into(), serde_json::json!("ctrl+s"));
                m
            },
            pace: Default::default(),
            risk_class: RiskClass::Write,
            irreversible: false,
            grounding: Grounding::Uia,
            timeout_ms: 5000,
            retry: Default::default(),
        }
    }

    #[test]
    fn record_and_read_back_one_step() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        let step = NewStep::new(1, sample_action("s1"), Grounding::Uia, "ok", 310)
            .with_digests(Some("d0".into()), Some("d1".into()))
            .with_note("saved the file")
            .outcome_bearing(true);
        let step_id = rec.record_step(&run_id, step).unwrap();

        let fetched = rec.get_step(&step_id).unwrap().expect("step present");
        assert_eq!(fetched.run_id, run_id);
        assert_eq!(fetched.seq, 1);
        assert_eq!(fetched.action.id, "s1");
        assert_eq!(fetched.grounding, Grounding::Uia);
        assert_eq!(fetched.snapshot_digest_before.as_deref(), Some("d0"));
        assert_eq!(fetched.snapshot_digest_after.as_deref(), Some("d1"));
        assert_eq!(fetched.outcome, "ok");
        assert_eq!(fetched.ms, 310);
        assert_eq!(fetched.note.as_deref(), Some("saved the file"));
        assert!(fetched.outcome_bearing);
        assert!(fetched.human_correction.is_none());
    }

    #[test]
    fn human_correction_round_trips() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        let correction = serde_json::json!({"supersedes_seq": 3, "instruction": "use ctrl+s"});
        let step = NewStep::new(4, sample_action("s4"), Grounding::Uia, "ok", 300)
            .with_human_correction(correction.clone());
        let step_id = rec.record_step(&run_id, step).unwrap();
        let fetched = rec.get_step(&step_id).unwrap().unwrap();
        assert_eq!(fetched.human_correction, Some(correction));
    }

    #[test]
    fn steps_missing_run_rejected_by_foreign_key() {
        let rec = Recorder::open_in_memory().unwrap();
        let step = NewStep::new(1, sample_action("s1"), Grounding::Uia, "ok", 1);
        let err = rec.record_step("no-such-run", step).unwrap_err();
        assert!(matches!(err, RecorderError::Sqlite(_)));
    }

    #[test]
    fn step_count_matches_list_len() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        for seq in 1..=7u32 {
            rec.record_step(&run_id, NewStep::new(seq, sample_action("s"), Grounding::Uia, "ok", 1))
                .unwrap();
        }
        assert_eq!(rec.step_count(&run_id).unwrap(), 7);
        assert_eq!(rec.list_steps(&run_id).unwrap().len(), 7);
    }
}
