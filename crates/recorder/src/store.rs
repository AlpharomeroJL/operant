//! The `Recorder` handle: one SQLite (WAL) connection guarded by a mutex.
//!
//! `rusqlite::Connection` is `Send` but not `Sync`, and step writes must be single
//! committed transactions, so a plain mutex around one connection is the simplest
//! correct concurrency model for a single-machine recorder (see `docs/ARCHITECTURE.md`
//! section 5: "serialized run queue"). Multiple readers/writers across threads are
//! serialized here; SQLite's own WAL concurrency is not relied on beyond a single
//! writer connection.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::{RecorderError, Result};
use crate::schema;

/// A trajectory recorder backed by a SQLite (WAL) database.
pub struct Recorder {
    conn: Mutex<Connection>,
}

impl Recorder {
    /// Open (or create) a recorder database at `path`. Pass `":memory:"` for a
    /// private in-memory database, which is the standard SQLite convention and is
    /// useful for tests.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        Self::from_connection(conn)
    }

    /// Open a private, anonymous in-memory recorder. Equivalent to
    /// `Recorder::open(":memory:")`.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        schema::init(&conn)?;
        Ok(Recorder { conn: Mutex::new(conn) })
    }

    /// Lock the underlying connection. Every call site holds this only for the
    /// duration of one statement or one transaction, never across an `.await` or a
    /// callback into caller code, so contention stays short-lived.
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| RecorderError::Poisoned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_memory() {
        let rec = Recorder::open_in_memory().expect("open in-memory recorder");
        // Schema must be present: a bogus table should not exist, a real one should.
        let conn = rec.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='runs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn open_via_memory_string_matches_open_in_memory() {
        let rec = Recorder::open(":memory:").expect("open(':memory:') recorder");
        let conn = rec.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='steps'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
