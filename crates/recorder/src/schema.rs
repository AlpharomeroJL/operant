//! DDL for the recorder's SQLite core (`docs/ARCHITECTURE.md` section 3).
//!
//! `runs`, `steps`, and the `blobs`/`artifacts` pair get full logic elsewhere in this
//! crate. `workflows`, `workflow_versions`, `gates`, `audit`, `undo_journal`, and
//! `metrics` are created here and get CRUD-lite accessors in `misc.rs`: enough to prove
//! the shape is right and reachable, without building out the compiler/gate engines
//! that own their real logic in other lanes.

pub(crate) const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS runs (
    id                  TEXT PRIMARY KEY,
    goal                TEXT NOT NULL,
    mode                TEXT NOT NULL CHECK (mode IN ('explore','replay','dry')),
    started             INTEGER NOT NULL,
    ended               INTEGER,
    status              TEXT NOT NULL CHECK (status IN ('running','completed','failed','aborted')),
    model_config_json   TEXT
);

CREATE TABLE IF NOT EXISTS steps (
    id                      TEXT PRIMARY KEY,
    run_id                  TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    seq                     INTEGER NOT NULL,
    action_ir_json          TEXT NOT NULL,
    grounding               TEXT NOT NULL CHECK (grounding IN ('uia','vision','adapter')),
    snapshot_digest_before  TEXT,
    snapshot_digest_after   TEXT,
    outcome                 TEXT NOT NULL,
    ms                      INTEGER NOT NULL,
    note                    TEXT,
    human_correction_json   TEXT,
    outcome_bearing         INTEGER NOT NULL DEFAULT 0,
    created_at              INTEGER NOT NULL,
    UNIQUE (run_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_steps_run_seq ON steps(run_id, seq);

-- Content-addressed byte store. Keyed by the blake3 hex digest of the content.
CREATE TABLE IF NOT EXISTS blobs (
    hash        TEXT PRIMARY KEY,
    data        BLOB NOT NULL,
    size        INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

-- Metadata plus refcount ledger over `blobs`. gc() deletes rows (and their blob) once
-- refcount drops to zero.
CREATE TABLE IF NOT EXISTS artifacts (
    hash        TEXT PRIMARY KEY REFERENCES blobs(hash) ON DELETE CASCADE,
    kind        TEXT NOT NULL DEFAULT 'blob',
    path        TEXT,
    refcount    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    version         TEXT NOT NULL,
    dsl_path        TEXT,
    manifest_json   TEXT,
    signature       TEXT,
    source_run_id   TEXT REFERENCES runs(id)
);

CREATE TABLE IF NOT EXISTS workflow_versions (
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    version     TEXT NOT NULL,
    diff_path   TEXT,
    approved_by TEXT,
    ts          INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, version)
);

CREATE TABLE IF NOT EXISTS gates (
    id          TEXT PRIMARY KEY,
    workflow_id TEXT REFERENCES workflows(id) ON DELETE CASCADE,
    step_ref    TEXT,
    kind        TEXT NOT NULL CHECK (kind IN ('pre','post','safety')),
    expr_json   TEXT NOT NULL,
    on_fail     TEXT
);

-- Hash-chained append-only audit log (C10).
CREATE TABLE IF NOT EXISTS audit (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          INTEGER NOT NULL,
    actor       TEXT NOT NULL,
    event_json  TEXT NOT NULL,
    prev_hash   TEXT,
    hash        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS undo_journal (
    run_id                  TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    seq                     INTEGER NOT NULL,
    inverse_action_ir_json  TEXT,
    applied                 INTEGER NOT NULL DEFAULT 0,
    ts                      INTEGER NOT NULL,
    PRIMARY KEY (run_id, seq)
);

CREATE TABLE IF NOT EXISTS metrics (
    workflow_id         TEXT NOT NULL,
    week                TEXT NOT NULL,
    runs                INTEGER NOT NULL DEFAULT 0,
    explore_ms          INTEGER,
    replay_p50_ms       INTEGER,
    minutes_saved_est   REAL,
    PRIMARY KEY (workflow_id, week)
);
"#;

pub(crate) fn init(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // journal_mode=WAL is ignored (falls back to `memory`) for `:memory:` databases;
    // SQLite does this silently, no error. synchronous=FULL keeps commits durable
    // across an unclean shutdown, which is what the crash-safety test relies on.
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA foreign_keys = ON;",
    )?;
    conn.execute_batch(DDL)?;
    Ok(())
}
