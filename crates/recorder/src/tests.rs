//! Crate-level tests that need more than one module or a real on-disk file.
//!
//! Per-module unit tests live next to the code they cover (`runs.rs`, `steps.rs`,
//! `blobs.rs`, `misc.rs`, `store.rs`). This file holds the required success-bar
//! tests that don't fit that pattern:
//! (a) [`fifty_step_run_records_and_reads_back_in_order`]
//! (b) [`wal_survives_unclean_shutdown_and_discards_uncommitted_write`]
//! (c) and (d) are covered in `blobs.rs` (`gc_removes_unreferenced_and_keeps_referenced`,
//!     `put_then_get_round_trips_bytes_and_hash`).
//! Plus a fixture fidelity round trip against the frozen compiler-input contract.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use operant_ir::{Action, ActionKind, Grounding, RiskClass};

use crate::runs::{RunMode, RunStatus};
use crate::steps::NewStep;
use crate::store::Recorder;

fn sample_action(seq: u32) -> Action {
    Action {
        v: 1,
        id: format!("step-{seq}"),
        kind: ActionKind::Click,
        intent: Some(format!("click element {seq}")),
        target: None,
        params: serde_json::Map::new(),
        pace: Default::default(),
        risk_class: RiskClass::Read,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Default::default(),
    }
}

fn sample_step(seq: u32) -> NewStep {
    NewStep::new(seq, sample_action(seq), Grounding::Uia, "ok", 100 + seq as u64)
        .with_digests(Some(format!("digest-{seq}")), Some(format!("digest-{}", seq + 1)))
}

/// A fresh path under the OS temp dir, unique per call so parallel `cargo test`
/// threads never collide on the same SQLite file.
fn unique_temp_db_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!("operant_recorder_test_{tag}_{}_{nanos}_{n}.sqlite3", std::process::id()));
    path
}

fn remove_db_and_sidecars(path: &std::path::Path) {
    let base = path.to_string_lossy().to_string();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{base}-wal"));
    let _ = std::fs::remove_file(format!("{base}-shm"));
    let _ = std::fs::remove_file(format!("{base}-journal"));
}

/// (a) A 50-step run records and reads back in order.
#[test]
fn fifty_step_run_records_and_reads_back_in_order() {
    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run("fifty step stress run", RunMode::Explore, None).unwrap();

    // Insert in reverse order so the assertion below actually exercises the
    // `ORDER BY seq ASC` in list_steps, rather than passively matching insertion order.
    for seq in (1..=50u32).rev() {
        rec.record_step(&run_id, sample_step(seq)).unwrap();
    }

    let steps = rec.list_steps(&run_id).unwrap();
    assert_eq!(steps.len(), 50);
    for (i, s) in steps.iter().enumerate() {
        let expected_seq = (i + 1) as u32;
        assert_eq!(s.seq, expected_seq, "steps must read back in ascending seq order");
        assert_eq!(s.action.id, format!("step-{expected_seq}"));
        assert_eq!(s.snapshot_digest_before.as_deref(), Some(format!("digest-{expected_seq}")).as_deref());
    }
    assert_eq!(rec.step_count(&run_id).unwrap(), 50);
}

/// (b) Kill-mid-write survivability. Writes are committed via the normal API on a
/// real file, the connection is torn down with no explicit shutdown/checkpoint call,
/// and a second write that never reached `COMMIT` is layered in to prove atomicity.
/// Reopening the same file must show exactly the committed prefix, and the recovered
/// database must still be fully writable.
#[test]
fn wal_survives_unclean_shutdown_and_discards_uncommitted_write() {
    let path = unique_temp_db_path("crash");
    let path_str = path.to_string_lossy().to_string();
    remove_db_and_sidecars(&path);

    let run_id = {
        let rec = Recorder::open(&path_str).unwrap();
        let run_id = rec.start_run("write an invoice note", RunMode::Explore, None).unwrap();
        for seq in 1..=5u32 {
            rec.record_step(&run_id, sample_step(seq)).unwrap();
        }

        // Simulate a write that was in flight when the process died: a second
        // connection to the same file begins a transaction, inserts a row, and is
        // dropped having never called COMMIT. No cleanup routine of ours runs here;
        // whatever happens is whatever a bare `drop` does.
        {
            let raw = rusqlite::Connection::open(&path_str).unwrap();
            raw.execute_batch("BEGIN IMMEDIATE;").unwrap();
            raw.execute(
                "INSERT INTO steps (id, run_id, seq, action_ir_json, grounding, outcome, ms, outcome_bearing, created_at)
                 VALUES ('uncommitted-step', ?1, 999, '{}', 'uia', 'ok', 1, 0, 0)",
                rusqlite::params![run_id],
            )
            .unwrap();
            // `raw` drops here uncommitted.
        }

        // `rec` also just falls out of scope at the end of this block: no explicit
        // checkpoint, no explicit close call.
        run_id
    };

    let reopened = Recorder::open(&path_str).unwrap();
    let steps = reopened.list_steps(&run_id).unwrap();
    assert_eq!(steps.len(), 5, "only the 5 committed steps survive the unclean shutdown");
    for (i, s) in steps.iter().enumerate() {
        assert_eq!(s.seq, (i + 1) as u32);
    }
    assert!(steps.iter().all(|s| s.id != "uncommitted-step"), "the never-committed row must not surface");

    // The recovered database is not just readable but fully healthy: it accepts
    // further committed writes.
    reopened.record_step(&run_id, sample_step(6)).unwrap();
    let steps = reopened.list_steps(&run_id).unwrap();
    assert_eq!(steps.len(), 6);
    assert_eq!(steps[5].seq, 6);

    drop(reopened);
    remove_db_and_sidecars(&path);
}

/// The recorder is the compiler's input: ingest the frozen `trajectory_notepad`
/// fixture end to end and confirm every field the compiler passes rely on
/// (retry-superseded outcome, human correction, before/after digests, the
/// outcome-bearing final assert) survives a write/read round trip unchanged.
#[test]
fn ingests_trajectory_notepad_fixture_faithfully() {
    let raw = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("fixture parses as JSON");

    let run = &fixture["run"];
    let goal = run["goal"].as_str().expect("run.goal");
    assert_eq!(run["mode"].as_str(), Some("explore"));

    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run(goal, RunMode::Explore, None).unwrap();

    let fixture_steps = fixture["steps"].as_array().expect("steps array");
    for fs in fixture_steps {
        let action: Action = serde_json::from_value(fs["action"].clone()).expect("action parses into IR");
        let grounding = match fs["grounding"].as_str().expect("step.grounding") {
            "uia" => Grounding::Uia,
            "vision" => Grounding::Vision,
            "adapter" => Grounding::Adapter,
            other => panic!("unexpected grounding {other}"),
        };
        let mut step = NewStep::new(
            fs["seq"].as_u64().expect("seq") as u32,
            action,
            grounding,
            fs["outcome"].as_str().expect("outcome"),
            fs["ms"].as_u64().expect("ms"),
        )
        .with_digests(
            fs["snapshot_digest_before"].as_str().map(String::from),
            fs["snapshot_digest_after"].as_str().map(String::from),
        );
        if let Some(note) = fs["note"].as_str() {
            step = step.with_note(note);
        }
        if let Some(hc) = fs.get("human_correction") {
            step = step.with_human_correction(hc.clone());
        }
        if fs["outcome_bearing"].as_bool().unwrap_or(false) {
            step = step.outcome_bearing(true);
        }
        rec.record_step(&run_id, step).unwrap();
    }

    let recorded = rec.list_steps(&run_id).unwrap();
    assert_eq!(recorded.len(), fixture_steps.len());

    for (got, want) in recorded.iter().zip(fixture_steps.iter()) {
        assert_eq!(got.seq, want["seq"].as_u64().unwrap() as u32);
        assert_eq!(got.action.id, want["action"]["id"].as_str().unwrap());
        assert_eq!(got.outcome, want["outcome"].as_str().unwrap());
        assert_eq!(got.snapshot_digest_before.as_deref(), want["snapshot_digest_before"].as_str());
        assert_eq!(got.snapshot_digest_after.as_deref(), want["snapshot_digest_after"].as_str());
    }

    // Step 3: the retry-superseded vision misground that pass 1 drops.
    let step3 = &recorded[2];
    assert_eq!(step3.outcome, "retry_superseded");
    assert!(step3.note.as_deref().unwrap_or("").contains("superseded"));

    // Step 4: carries the human correction that supersedes step 3.
    let step4 = &recorded[3];
    let hc = step4.human_correction.as_ref().expect("step 4 carries a human correction");
    assert_eq!(hc["supersedes_seq"], 3);
    assert_eq!(hc["instruction"], "Do not use the menu. Press Ctrl+S instead.");

    // Step 5: the outcome-bearing postcondition assert pass 4 needs.
    let step5 = &recorded[4];
    assert!(step5.outcome_bearing);
    assert_eq!(step5.action.kind, ActionKind::Assert);

    rec.end_run(&run_id, RunStatus::Completed).unwrap();
    assert_eq!(rec.get_run(&run_id).unwrap().unwrap().status, RunStatus::Completed);
}
