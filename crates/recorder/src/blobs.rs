//! Content-addressed blob store plus refcounted GC.
//!
//! Bytes live in `blobs`, keyed by their blake3 hex digest. `artifacts` is the
//! metadata and refcount ledger over `blobs` (kind, logical path, refcount): every
//! [`Recorder::put_blob`] call is a reference (insert-or-bump), [`Recorder::release_blob`]
//! drops one, and [`Recorder::gc`] deletes anything that reaches zero. Storing bytes in
//! a table (rather than a blobs directory) keeps blob writes inside the same WAL
//! transaction machinery as everything else, and works uniformly for `":memory:"`.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{RecorderError, Result};
use crate::ids::now_ms;
use crate::store::Recorder;

/// An `artifacts` row: metadata and refcount for one content-addressed blob.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub hash: String,
    pub kind: String,
    pub path: Option<String>,
    pub refcount: i64,
    pub size: i64,
    pub created_at: i64,
}

impl Recorder {
    /// Store `bytes` content-addressed by their blake3 hex digest and add one
    /// reference. Idempotent: calling this again with the same bytes does not
    /// duplicate storage, it increments the refcount. Returns the hash.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let created_at = now_ms();
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO blobs (hash, data, size, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(hash) DO NOTHING",
            params![hash, bytes, bytes.len() as i64, created_at],
        )?;
        tx.execute(
            "INSERT INTO artifacts (hash, kind, path, refcount, created_at)
             VALUES (?1, 'blob', NULL, 1, ?2)
             ON CONFLICT(hash) DO UPDATE SET refcount = refcount + 1",
            params![hash, created_at],
        )?;
        tx.commit()?;
        Ok(hash)
    }

    /// Like [`Recorder::put_blob`], but tags the artifact with a `kind`
    /// (`anchor` | `screenshot` | `export`, per `docs/ARCHITECTURE.md` section 3) the
    /// first time it is stored. Later calls only bump the refcount; `kind` is not
    /// overwritten once set, since a hash's content, not its callers, determines it.
    pub fn put_artifact(&self, bytes: &[u8], kind: &str) -> Result<String> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let created_at = now_ms();
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO blobs (hash, data, size, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(hash) DO NOTHING",
            params![hash, bytes, bytes.len() as i64, created_at],
        )?;
        tx.execute(
            "INSERT INTO artifacts (hash, kind, path, refcount, created_at)
             VALUES (?1, ?2, NULL, 1, ?3)
             ON CONFLICT(hash) DO UPDATE SET refcount = refcount + 1",
            params![hash, kind, created_at],
        )?;
        tx.commit()?;
        Ok(hash)
    }

    /// Fetch bytes by hash. Does not touch the refcount.
    pub fn get_blob(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.lock()?;
        conn.query_row("SELECT data FROM blobs WHERE hash = ?1", params![hash], |r| r.get(0))
            .optional()
            .map_err(RecorderError::from)
    }

    /// Drop one reference to `hash`. Floors at zero rather than going negative or
    /// erroring, since callers may legitimately race to release the same hash.
    /// Errors with [`RecorderError::BlobNotFound`] if the hash has no artifact row.
    pub fn release_blob(&self, hash: &str) -> Result<()> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "UPDATE artifacts SET refcount = MAX(refcount - 1, 0) WHERE hash = ?1",
            params![hash],
        )?;
        if changed == 0 {
            return Err(RecorderError::BlobNotFound(hash.to_string()));
        }
        Ok(())
    }

    /// Artifact metadata (including refcount) for a hash, if known.
    pub fn get_artifact(&self, hash: &str) -> Result<Option<ArtifactRecord>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT a.hash, a.kind, a.path, a.refcount, b.size, a.created_at
             FROM artifacts a JOIN blobs b ON b.hash = a.hash
             WHERE a.hash = ?1",
            params![hash],
            |r| {
                Ok(ArtifactRecord {
                    hash: r.get(0)?,
                    kind: r.get(1)?,
                    path: r.get(2)?,
                    refcount: r.get(3)?,
                    size: r.get(4)?,
                    created_at: r.get(5)?,
                })
            },
        )
        .optional()
        .map_err(RecorderError::from)
    }

    /// Delete every blob (and its artifact row) whose refcount has reached zero.
    /// Returns the number of blobs removed.
    pub fn gc(&self) -> Result<usize> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let removed = tx.execute(
            "DELETE FROM blobs WHERE hash IN (SELECT hash FROM artifacts WHERE refcount <= 0)",
            [],
        )?;
        tx.execute("DELETE FROM artifacts WHERE refcount <= 0", [])?;
        tx.commit()?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_round_trips_bytes_and_hash() {
        let rec = Recorder::open_in_memory().unwrap();
        let bytes = b"hello trajectory recorder";
        let hash = rec.put_blob(bytes).unwrap();

        // Hash must match an independent blake3 computation of the same bytes.
        assert_eq!(hash, blake3::hash(bytes).to_hex().to_string());

        let back = rec.get_blob(&hash).unwrap().expect("blob present");
        assert_eq!(back, bytes);
    }

    #[test]
    fn get_missing_blob_is_none() {
        let rec = Recorder::open_in_memory().unwrap();
        assert!(rec.get_blob("does-not-exist").unwrap().is_none());
    }

    #[test]
    fn put_blob_is_content_addressed_and_dedupes() {
        let rec = Recorder::open_in_memory().unwrap();
        let h1 = rec.put_blob(b"same content").unwrap();
        let h2 = rec.put_blob(b"same content").unwrap();
        assert_eq!(h1, h2);
        let artifact = rec.get_artifact(&h1).unwrap().unwrap();
        assert_eq!(artifact.refcount, 2, "second put_blob call is a second reference");
    }

    #[test]
    fn release_blob_floors_at_zero_and_missing_errors() {
        let rec = Recorder::open_in_memory().unwrap();
        let hash = rec.put_blob(b"x").unwrap();
        rec.release_blob(&hash).unwrap();
        assert_eq!(rec.get_artifact(&hash).unwrap().unwrap().refcount, 0);
        // Releasing again floors at zero rather than going negative.
        rec.release_blob(&hash).unwrap();
        assert_eq!(rec.get_artifact(&hash).unwrap().unwrap().refcount, 0);

        let err = rec.release_blob("never-existed").unwrap_err();
        assert!(matches!(err, RecorderError::BlobNotFound(_)));
    }

    #[test]
    fn gc_removes_unreferenced_and_keeps_referenced() {
        let rec = Recorder::open_in_memory().unwrap();
        let kept = rec.put_blob(b"kept: still referenced").unwrap();
        let removed = rec.put_blob(b"removed: about to be released").unwrap();
        rec.release_blob(&removed).unwrap();
        assert_eq!(rec.get_artifact(&removed).unwrap().unwrap().refcount, 0);

        let n = rec.gc().unwrap();
        assert_eq!(n, 1, "exactly the zero-refcount blob is collected");

        assert!(rec.get_blob(&removed).unwrap().is_none(), "unreferenced blob is gone");
        assert!(rec.get_artifact(&removed).unwrap().is_none(), "its artifact row is gone too");

        assert_eq!(rec.get_blob(&kept).unwrap().unwrap(), b"kept: still referenced");
        assert_eq!(rec.get_artifact(&kept).unwrap().unwrap().refcount, 1, "referenced blob untouched");
    }

    #[test]
    fn gc_is_idempotent_when_nothing_to_collect() {
        let rec = Recorder::open_in_memory().unwrap();
        rec.put_blob(b"alive").unwrap();
        assert_eq!(rec.gc().unwrap(), 0);
        assert_eq!(rec.gc().unwrap(), 0);
    }

    #[test]
    fn put_artifact_tags_kind() {
        let rec = Recorder::open_in_memory().unwrap();
        let hash = rec.put_artifact(b"png bytes", "screenshot").unwrap();
        assert_eq!(rec.get_artifact(&hash).unwrap().unwrap().kind, "screenshot");
    }
}
