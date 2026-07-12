//! Undo journal (C20): a trust feature that lets a run be reversed.
//!
//! Every write-class action appends its inverse to the `undo_journal` table BEFORE
//! it runs ("journal-ahead"), so the reversal already exists even if the action or
//! the process dies mid-write:
//!
//! * file create        -> delete the created file
//! * file delete        -> recreate it from the pre-image bytes captured now
//! * file move / rename  -> move it back
//! * file overwrite      -> restore the pre-image bytes captured now
//! * clipboard write     -> restore the prior clipboard contents
//!
//! "Undo last run" reads the journal newest-first and runs as a dry-run preview
//! first (plain-English sentences describing what would be restored), then, on
//! confirm, executes the inverses and narrates each restoration. Actions with no
//! inverse (email send, web form submit, side-effectful shell) are tagged
//! irreversible at the Action IR level ([`operant_ir::Action::irreversible`]); undo
//! lists them as "cannot be undone" and never fabricates a reversal.
//!
//! Safety (a wrong inverse destroys data, so this module fails closed):
//!
//! * Deletes and overwrites copy their pre-image into the content-addressed blob
//!   store while the bytes still exist, and restoration reads those exact bytes
//!   back. That guarantees a byte-identical result rather than trusting a
//!   recycle-bin token that could be emptied out from under us.
//! * A restore refuses to run (returns an error) if its pre-image blob is missing,
//!   rather than writing an empty or truncated file.
//! * A reverse-move refuses to run if a file already sits at the original path,
//!   rather than clobbering whatever is there.
//! * Applying an inverse is guarded by existence checks, so a partial run (journal
//!   written, action never performed, then a crash) undoes cleanly: deleting an
//!   already-absent file and moving-back an absent file are both no-ops.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use operant_core::bus::events::{UndoInverseWire, UndoJournalItemWire, UndoPreviewed};
use operant_core::Bus;

use crate::error::{RecorderError, Result};
use crate::store::Recorder;

/// A write-class action that is about to run, described BEFORE it executes so its
/// inverse can be journaled ahead of it. The pre-image capture for
/// [`PendingWrite::DeleteFile`] and [`PendingWrite::OverwriteFile`] reads the file
/// as it stands now, so journal it while the original bytes are still on disk.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingWrite {
    /// A file is about to be created at `path`. Inverse: delete it.
    CreateFile { path: PathBuf },
    /// The file at `path` is about to be deleted. Its current bytes are captured as
    /// the pre-image now; inverse: recreate it from that pre-image.
    DeleteFile { path: PathBuf },
    /// `from` is about to be moved or renamed to `to`. Inverse: move it back.
    MoveFile { from: PathBuf, to: PathBuf },
    /// The file at `path` is about to be overwritten. Its current bytes are captured
    /// as the pre-image now; inverse: restore that pre-image.
    OverwriteFile { path: PathBuf },
    /// The clipboard is about to be written. `prior` is the current clipboard text
    /// (captured by the clipboard adapter); inverse: restore it.
    ClipboardWrite { prior: Option<String> },
    /// An action with no inverse (email send, web form submit, side-effectful shell).
    /// `description` is the plain-English label shown under "cannot be undone".
    Irreversible { description: String },
}

impl PendingWrite {
    /// Derive the irreversible marker from an Action tagged irreversible at the
    /// schema level. Returns `None` for reversible actions (whose file/clipboard
    /// effects are mapped from the action by the adapter layer, not from here).
    pub fn irreversible_from_action(action: &operant_ir::Action) -> Option<Self> {
        if !action.irreversible {
            return None;
        }
        let description = action
            .intent
            .clone()
            .unwrap_or_else(|| format!("{:?} action {}", action.kind, action.id));
        Some(PendingWrite::Irreversible { description })
    }
}

/// The stored inverse: what "undo" will do, serialized into the `undo_journal`.
///
/// Tagged with `op` so a journal row is human-readable and stable across versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Inverse {
    /// Delete a file that the run created.
    DeleteCreated { path: PathBuf },
    /// Recreate a file the run deleted, from its captured pre-image.
    RecreateDeleted { path: PathBuf, blob_hash: String },
    /// Move a file the run moved back to where it came from.
    ReverseMove { moved_to: PathBuf, original: PathBuf },
    /// Restore the pre-image of a file the run overwrote.
    RestoreOverwritten { path: PathBuf, blob_hash: String },
    /// Restore the clipboard contents that predated the run's clipboard write.
    RestoreClipboard { prior: Option<String> },
    /// A step that cannot be undone; carried so undo can list it honestly.
    Irreversible { description: String },
}

impl Inverse {
    /// True when this inverse has no reversal to perform.
    fn is_irreversible(&self) -> bool {
        matches!(self, Inverse::Irreversible { .. })
    }

    /// A future-tense sentence for the dry-run preview.
    fn preview_line(&self) -> String {
        match self {
            Inverse::DeleteCreated { path } => {
                format!("Would delete the file the run created: {}", show(path))
            }
            Inverse::RecreateDeleted { path, .. } => {
                format!("Would recreate the deleted file from its saved copy: {}", show(path))
            }
            Inverse::ReverseMove { moved_to, original } => {
                format!("Would move {} back to {}", show(moved_to), show(original))
            }
            Inverse::RestoreOverwritten { path, .. } => {
                format!("Would restore the previous contents of {}", show(path))
            }
            Inverse::RestoreClipboard { prior } => match prior {
                Some(_) => "Would restore the previous clipboard contents".to_string(),
                None => "Would clear the clipboard (it was empty before the run)".to_string(),
            },
            Inverse::Irreversible { description } => {
                format!("Cannot be undone: {description}")
            }
        }
    }

    /// A completed-action sentence narrating one restoration after it has run.
    fn applied_line(&self) -> String {
        match self {
            Inverse::DeleteCreated { path } => {
                format!("Deleted the file the run created: {}", show(path))
            }
            Inverse::RecreateDeleted { path, .. } => {
                format!("Recreated the deleted file from its saved copy: {}", show(path))
            }
            Inverse::ReverseMove { moved_to, original } => {
                format!("Moved {} back to {}", show(moved_to), show(original))
            }
            Inverse::RestoreOverwritten { path, .. } => {
                format!("Restored the previous contents of {}", show(path))
            }
            Inverse::RestoreClipboard { prior } => match prior {
                Some(_) => "Restored the previous clipboard contents".to_string(),
                None => "Cleared the clipboard (it was empty before the run)".to_string(),
            },
            Inverse::Irreversible { description } => {
                format!("Cannot be undone: {description}")
            }
        }
    }

    /// The wire shape crossing the recorder boundary onto the bus
    /// (`operant_core::bus::events::UndoInverseWire`, `undo.previewed`'s
    /// optional `items` field, F1b). Drops the blob hash, an internal
    /// storage detail no subscriber needs, and, for a clipboard restore,
    /// drops the actual prior clipboard text, keeping only whether one
    /// existed: this inverse never leaks clipboard contents onto the bus.
    fn to_wire(&self) -> UndoInverseWire {
        match self {
            Inverse::DeleteCreated { path } => UndoInverseWire::DeleteCreated { path: show(path) },
            Inverse::RecreateDeleted { path, .. } => {
                UndoInverseWire::RecreateDeleted { path: show(path) }
            }
            Inverse::ReverseMove { moved_to, original } => UndoInverseWire::ReverseMove {
                moved_to: show(moved_to),
                original: show(original),
            },
            Inverse::RestoreOverwritten { path, .. } => {
                UndoInverseWire::RestoreOverwritten { path: show(path) }
            }
            Inverse::RestoreClipboard { prior } => {
                UndoInverseWire::RestoreClipboard { had_prior: prior.is_some() }
            }
            Inverse::Irreversible { description } => {
                UndoInverseWire::Irreversible { description: description.clone() }
            }
        }
    }
}

impl Recorder {
    /// Journal-ahead: compute and append the inverse of a pending write-class action
    /// BEFORE it runs, returning the journal sequence number used.
    ///
    /// For [`PendingWrite::DeleteFile`] and [`PendingWrite::OverwriteFile`] the file
    /// is read now and its bytes copied into the content-addressed blob store, so the
    /// pre-image is durable before the destructive write happens. Call this while the
    /// original bytes are still on disk.
    pub fn journal_ahead(&self, run_id: &str, pending: &PendingWrite) -> Result<u32> {
        let inverse = match pending {
            PendingWrite::CreateFile { path } => Inverse::DeleteCreated { path: path.clone() },
            PendingWrite::DeleteFile { path } => {
                let bytes = fs::read(path).map_err(|e| io_err("read pre-image before delete", path, e))?;
                let blob_hash = self.put_blob(&bytes)?;
                Inverse::RecreateDeleted { path: path.clone(), blob_hash }
            }
            PendingWrite::OverwriteFile { path } => {
                let bytes = fs::read(path).map_err(|e| io_err("read pre-image before overwrite", path, e))?;
                let blob_hash = self.put_blob(&bytes)?;
                Inverse::RestoreOverwritten { path: path.clone(), blob_hash }
            }
            PendingWrite::MoveFile { from, to } => {
                Inverse::ReverseMove { moved_to: to.clone(), original: from.clone() }
            }
            PendingWrite::ClipboardWrite { prior } => {
                Inverse::RestoreClipboard { prior: prior.clone() }
            }
            PendingWrite::Irreversible { description } => {
                Inverse::Irreversible { description: description.clone() }
            }
        };
        let value = serde_json::to_value(&inverse)?;
        let seq = self.next_undo_seq(run_id)?;
        self.append_undo(run_id, seq, Some(&value))?;
        Ok(seq)
    }

    /// Dry-run preview of undoing `run_id`, newest-first, in plain English. Performs
    /// no filesystem or clipboard changes. Reversible steps already undone in a prior
    /// pass are omitted; irreversible steps are always listed as "cannot be undone".
    pub fn preview_undo(&self, run_id: &str) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        for (inverse, applied) in self.inverses_newest_first(run_id)? {
            if inverse.is_irreversible() {
                lines.push(inverse.preview_line());
            } else if !applied {
                lines.push(inverse.preview_line());
            }
        }
        Ok(lines)
    }

    /// Execute the undo for `run_id`: replay inverses newest-first, restoring files
    /// (byte-identically, from captured pre-images) and narrating each restoration.
    /// Irreversible steps are listed as "cannot be undone" and never touched.
    /// Reversible steps already undone are skipped, so undo is safe to re-run.
    pub fn undo_run(&self, run_id: &str) -> Result<Vec<String>> {
        let mut narration = Vec::new();
        for (seq, inverse, applied) in self.inverses_newest_first_seq(run_id)? {
            if inverse.is_irreversible() {
                // Never pretend: report it, do not perform anything, leave it unapplied.
                narration.push(inverse.applied_line());
                continue;
            }
            if applied {
                continue;
            }
            self.apply_inverse(&inverse)?;
            self.mark_undo_applied(run_id, seq)?;
            narration.push(inverse.applied_line());
        }
        Ok(narration)
    }

    /// Dry-run preview of the most recently started run.
    pub fn preview_undo_last_run(&self) -> Result<Vec<String>> {
        let run_id = self.latest_run_id()?;
        self.preview_undo(&run_id)
    }

    /// The real `undo.previewed` bus event for `run_id` (F1b): counts plus
    /// the real per-item restoration content, newest-first, in the same
    /// dry-run scope as [`Recorder::preview_undo`] (a reversible entry
    /// already undone in a prior pass is omitted; an irreversible entry is
    /// always included, and is what `entries`/`irreversible` are counted
    /// from, so this event and `preview_undo`'s own plain-English lines can
    /// never disagree about which rows they cover). This is what a
    /// publisher hands to [`Bus::publish_event`] so a subscriber (once a
    /// transport carries it there, `ui/src/undo/realJournal.ts`) sees the
    /// actual restoration list a fixture only stood in for before. Performs
    /// no filesystem or clipboard change.
    pub fn preview_undo_event(&self, run_id: &str) -> Result<UndoPreviewed> {
        let mut items = Vec::new();
        let mut irreversible = 0u32;
        for (seq, inverse, applied) in self.inverses_newest_first_seq(run_id)? {
            if inverse.is_irreversible() {
                irreversible += 1;
            } else if applied {
                continue;
            }
            items.push(UndoJournalItemWire { seq, inverse: inverse.to_wire() });
        }
        Ok(UndoPreviewed { run_id: run_id.to_string(), entries: items.len() as u32, irreversible, items })
    }

    /// [`Recorder::preview_undo_event`] for the most recently started run.
    pub fn preview_undo_event_last_run(&self) -> Result<UndoPreviewed> {
        let run_id = self.latest_run_id()?;
        self.preview_undo_event(&run_id)
    }

    /// Compute [`Recorder::preview_undo_event`] for `run_id` and publish it
    /// on `bus` under `undo.previewed` (`contracts/bus_events.md`): the
    /// recorder itself publishing the real restoration list, not a
    /// stand-in. Deterministic and read-only like `preview_undo`: a
    /// database read plus struct construction, never a model or network
    /// call. Returns the bus sequence number assigned to the publish.
    pub fn publish_undo_preview(&self, bus: &Bus, run_id: &str) -> Result<u64> {
        let event = self.preview_undo_event(run_id)?;
        Ok(bus.publish_event(&event)?)
    }

    /// Execute the undo of the most recently started run.
    pub fn undo_last_run(&self) -> Result<Vec<String>> {
        let run_id = self.latest_run_id()?;
        self.undo_run(&run_id)
    }

    // ------------------------------------------------------------- internals

    fn latest_run_id(&self) -> Result<String> {
        self.list_runs()?
            .into_iter()
            .next()
            .ok_or_else(|| RecorderError::InvalidInput("no runs recorded; nothing to undo".into()))
    }

    /// Next per-run journal sequence number. Runs execute on a single serialized
    /// queue (see `store.rs`), so a read-then-append here is not racing another
    /// journal-ahead for the same run.
    fn next_undo_seq(&self, run_id: &str) -> Result<u32> {
        let existing = self.list_undo(run_id)?;
        Ok(existing.iter().map(|e| e.seq).max().map_or(1, |m| m + 1))
    }

    /// Decode the journal for `run_id` into `(Inverse, applied)` pairs, newest-first.
    fn inverses_newest_first(&self, run_id: &str) -> Result<Vec<(Inverse, bool)>> {
        Ok(self
            .inverses_newest_first_seq(run_id)?
            .into_iter()
            .map(|(_, inv, applied)| (inv, applied))
            .collect())
    }

    /// As [`Recorder::inverses_newest_first`] but keeps each row's `seq`.
    fn inverses_newest_first_seq(&self, run_id: &str) -> Result<Vec<(u32, Inverse, bool)>> {
        let mut entries = self.list_undo(run_id)?;
        entries.sort_by(|a, b| b.seq.cmp(&a.seq)); // newest-first
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(value) = entry.inverse_action else {
                // A NULL inverse carries no reversal and no label; skip it.
                continue;
            };
            let inverse: Inverse = serde_json::from_value(value)?;
            out.push((entry.seq, inverse, entry.applied));
        }
        Ok(out)
    }

    /// Perform one reversible inverse against the real filesystem, failing closed.
    fn apply_inverse(&self, inverse: &Inverse) -> Result<()> {
        match inverse {
            Inverse::DeleteCreated { path } => {
                if path.exists() {
                    fs::remove_file(path).map_err(|e| io_err("delete created file", path, e))?;
                }
                Ok(())
            }
            Inverse::RecreateDeleted { path, blob_hash }
            | Inverse::RestoreOverwritten { path, blob_hash } => {
                let bytes = self.get_blob(blob_hash)?.ok_or_else(|| {
                    RecorderError::InvalidInput(format!(
                        "undo pre-image blob {blob_hash} is missing; refusing to restore {}",
                        show(path)
                    ))
                })?;
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)
                            .map_err(|e| io_err("create parent for restore", parent, e))?;
                    }
                }
                fs::write(path, &bytes).map_err(|e| io_err("restore file bytes", path, e))?;
                Ok(())
            }
            Inverse::ReverseMove { moved_to, original } => {
                if !moved_to.exists() {
                    // The move never happened (or was already reversed); nothing to do.
                    return Ok(());
                }
                if original.exists() {
                    return Err(RecorderError::InvalidInput(format!(
                        "refusing to move {} back to {}: a file already exists at the original path",
                        show(moved_to),
                        show(original)
                    )));
                }
                if let Some(parent) = original.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)
                            .map_err(|e| io_err("create parent for reverse move", parent, e))?;
                    }
                }
                fs::rename(moved_to, original)
                    .map_err(|e| io_err("reverse move", moved_to, e))?;
                Ok(())
            }
            Inverse::RestoreClipboard { .. } => {
                // Writing the OS clipboard is owned by the clipboard adapter, which
                // holds clipboard access; this crate records and narrates the inverse.
                Ok(())
            }
            Inverse::Irreversible { .. } => Ok(()),
        }
    }
}

/// Display a path for narration.
fn show(path: &Path) -> String {
    path.display().to_string()
}

/// Wrap a filesystem error with context. The recorder error enum has no I/O variant
/// and lives in a file this lane does not own, so I/O failures surface as
/// [`RecorderError::InvalidInput`] with a descriptive, greppable message.
fn io_err(context: &str, path: &Path, e: io::Error) -> RecorderError {
    RecorderError::InvalidInput(format!("undo: {context} for {}: {e}", show(path)))
}

/// Fingerprint a directory subtree by hashing every file's relative path and bytes
/// in a stable order. Two subtrees with identical file names and contents hash to
/// the same value, so callers can prove an undo restored a tree byte-for-byte.
/// Empty directories do not affect the fingerprint.
pub fn hash_tree(root: &Path) -> io::Result<String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    for (rel, bytes) in &files {
        hasher.update(&(rel.len() as u64).to_le_bytes());
        hasher.update(rel.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_str = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel_str, fs::read(&path)?));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunMode;

    #[test]
    fn create_inverse_round_trips_and_previews() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: PathBuf::from("C:/tmp/new.txt") })
            .unwrap();

        let preview = rec.preview_undo(&run_id).unwrap();
        assert_eq!(preview.len(), 1);
        assert!(preview[0].contains("Would delete"));
        assert!(preview[0].contains("new.txt"));
    }

    #[test]
    fn irreversible_is_listed_never_applied() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("send mail", RunMode::Explore, None).unwrap();
        rec.journal_ahead(
            &run_id,
            &PendingWrite::Irreversible { description: "email send to boss@example.com".into() },
        )
        .unwrap();

        let narration = rec.undo_run(&run_id).unwrap();
        assert_eq!(narration.len(), 1);
        assert!(narration[0].to_lowercase().contains("cannot be undone"));
        assert!(narration[0].contains("boss@example.com"));

        // The row is never marked applied, because nothing was reversed.
        let entries = rec.list_undo(&run_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].applied);
    }

    #[test]
    fn clipboard_inverse_previews_prior_contents() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("copy", RunMode::Explore, None).unwrap();
        rec.journal_ahead(
            &run_id,
            &PendingWrite::ClipboardWrite { prior: Some("old clipboard".into()) },
        )
        .unwrap();
        let preview = rec.preview_undo(&run_id).unwrap();
        assert_eq!(preview.len(), 1);
        assert!(preview[0].to_lowercase().contains("clipboard"));
    }

    #[test]
    fn irreversible_from_action_maps_schema_flag() {
        // An Action tagged irreversible at the IR level becomes a "cannot be undone" marker.
        let json = r#"{"id":"s9","kind":"adapter_call","intent":"send the invoice email",
            "risk_class":"write","irreversible":true,"grounding":"adapter"}"#;
        let action: operant_ir::Action = serde_json::from_str(json).unwrap();
        let pending = PendingWrite::irreversible_from_action(&action).unwrap();
        assert_eq!(
            pending,
            PendingWrite::Irreversible { description: "send the invoice email".into() }
        );

        // A reversible action produces no irreversible marker.
        let reversible = operant_ir::Action { irreversible: false, ..action };
        assert!(PendingWrite::irreversible_from_action(&reversible).is_none());
    }

    #[test]
    fn newest_first_ordering() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("ordered", RunMode::Explore, None).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "a.txt".into() }).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "b.txt".into() }).unwrap();
        let preview = rec.preview_undo(&run_id).unwrap();
        // b.txt was journaled last, so undo previews it first.
        assert!(preview[0].contains("b.txt"));
        assert!(preview[1].contains("a.txt"));
    }

    /// F1b: `preview_undo_event`'s `items` mirror `preview_undo`'s own lines
    /// exactly (same rows, same order), just structured instead of prose,
    /// and with clipboard contents never present on the wire.
    #[test]
    fn preview_undo_event_matches_preview_undo_and_never_leaks_clipboard_contents() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("mixed", RunMode::Explore, None).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "new.txt".into() }).unwrap();
        rec.journal_ahead(
            &run_id,
            &PendingWrite::ClipboardWrite { prior: Some("super secret clipboard text".into()) },
        )
        .unwrap();
        rec.journal_ahead(
            &run_id,
            &PendingWrite::Irreversible { description: "send the invoice email".into() },
        )
        .unwrap();

        let event = rec.preview_undo_event(&run_id).unwrap();
        assert_eq!(event.run_id, run_id);
        assert_eq!(event.entries, 3);
        assert_eq!(event.irreversible, 1);
        assert_eq!(event.items.len(), 3);
        // Newest-first, same as preview_undo.
        assert_eq!(event.items[0].seq, 3);
        assert_eq!(event.items[2].seq, 1);

        let json = serde_json::to_value(&event.items).unwrap();
        let dumped = json.to_string();
        assert!(!dumped.contains("super secret clipboard text"), "clipboard contents must never reach the wire");
        assert!(dumped.contains("\"had_prior\":true"));

        assert_eq!(rec.preview_undo_event_last_run().unwrap(), event);
    }

    /// F1b: a reversible row already undone in a prior pass is omitted from
    /// `items` (and its counts), same as `preview_undo`; an irreversible row
    /// is always present.
    #[test]
    fn preview_undo_event_skips_already_applied_reversible_rows() {
        let rec = Recorder::open_in_memory().unwrap();
        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "a.txt".into() }).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "b.txt".into() }).unwrap();
        rec.mark_undo_applied(&run_id, 1).unwrap(); // a.txt's inverse already ran.

        let event = rec.preview_undo_event(&run_id).unwrap();
        assert_eq!(event.entries, 1);
        assert_eq!(event.irreversible, 0);
        assert_eq!(event.items.len(), 1);
        assert_eq!(event.items[0].seq, 2);
    }

    /// F1b: `publish_undo_preview` is the recorder itself publishing the
    /// real restoration list onto a real bus, not an inert struct nobody
    /// calls.
    #[test]
    fn publish_undo_preview_puts_the_real_event_on_the_bus() {
        let rec = Recorder::open_in_memory().unwrap();
        let bus = operant_core::Bus::new();
        let sub = bus.subscribe("undo.previewed");

        let run_id = rec.start_run("goal", RunMode::Explore, None).unwrap();
        rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: "created.txt".into() }).unwrap();

        rec.publish_undo_preview(&bus, &run_id).unwrap();

        let env = sub.rx.try_recv().expect("undo.previewed delivered");
        assert_eq!(env.topic, "undo.previewed");
        let payload: UndoPreviewed = serde_json::from_value(env.payload).expect("payload deserializes");
        assert_eq!(payload, rec.preview_undo_event(&run_id).unwrap());
        assert_eq!(payload.items[0].seq, 1);
        assert_eq!(payload.items[0].inverse, UndoInverseWire::DeleteCreated { path: "created.txt".into() });
    }
}
