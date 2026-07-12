//! Undo journal (C20) end-to-end tests.
//!
//! The headline test runs a fixture "session" that creates, moves, overwrites, and
//! deletes files in a private temp directory, then does "undo last run" and asserts
//! the directory is restored to a BYTE-IDENTICAL prior state (the tree is hashed
//! before the run and again after the undo). A second test proves an email-send
//! (irreversible) step renders the "cannot be undone" label instead of an inverse,
//! and a third proves an irreversible step coexists with real file inverses without
//! blocking them.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use operant_core::Bus;
use operant_recorder::undo::{hash_tree, PendingWrite};
use operant_recorder::{Recorder, RunMode};

/// A self-cleaning temp directory (no external crate needed). Removed on drop, so a
/// panicking assertion still tears the directory down.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique_temp_dir(tag: &str) -> TempDir {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "operant-undo-{tag}-{}-{nanos:x}-{n:x}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    TempDir(dir)
}

#[test]
fn undo_last_run_restores_temp_dir_byte_identical() {
    let tmp = unique_temp_dir("bytes");
    let dir = &tmp.0;

    // Prior state: four files. `keep.txt` is a control that must never be touched.
    fs::write(dir.join("keep.txt"), b"i am untouched throughout the run").unwrap();
    fs::write(dir.join("move_me.txt"), b"these bytes are going to move").unwrap();
    fs::write(dir.join("overwrite_me.txt"), b"ORIGINAL contents, a specific length").unwrap();
    fs::write(dir.join("delete_me.txt"), b"soon to be deleted, restore me exactly").unwrap();
    let before = hash_tree(dir).unwrap();

    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run("mutate a temp directory", RunMode::Explore, None).unwrap();

    // Each mutation: journal-ahead the inverse FIRST, then perform the real action.

    // create
    let created = dir.join("created.txt");
    rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: created.clone() }).unwrap();
    fs::write(&created, b"freshly created by the run").unwrap();

    // move / rename
    let moved = dir.join("moved.txt");
    rec.journal_ahead(
        &run_id,
        &PendingWrite::MoveFile { from: dir.join("move_me.txt"), to: moved.clone() },
    )
    .unwrap();
    fs::rename(dir.join("move_me.txt"), &moved).unwrap();

    // overwrite (pre-image captured inside journal_ahead, before the clobber)
    let overwrite = dir.join("overwrite_me.txt");
    rec.journal_ahead(&run_id, &PendingWrite::OverwriteFile { path: overwrite.clone() }).unwrap();
    fs::write(&overwrite, b"CHANGED to a totally different string of another length").unwrap();

    // delete (pre-image captured inside journal_ahead, before the removal)
    let deleted = dir.join("delete_me.txt");
    rec.journal_ahead(&run_id, &PendingWrite::DeleteFile { path: deleted.clone() }).unwrap();
    fs::remove_file(&deleted).unwrap();

    let after_mutations = hash_tree(dir).unwrap();
    assert_ne!(before, after_mutations, "the run must have changed the directory");

    // Dry-run preview: plain English, newest-first, and it must not touch the disk.
    let preview = rec.preview_undo_last_run().unwrap();
    assert_eq!(preview.len(), 4, "one preview line per journaled action");
    assert!(preview.iter().all(|line| !line.trim().is_empty()));
    assert!(
        preview[0].contains("delete_me.txt"),
        "newest-first: the delete (journaled last) previews first, got {:?}",
        preview[0]
    );
    assert_eq!(hash_tree(dir).unwrap(), after_mutations, "preview must not change the disk");

    // Execute the undo and narrate each restoration.
    let narration = rec.undo_last_run().unwrap();
    assert_eq!(narration.len(), 4, "one narration line per restoration");

    // The whole tree is byte-for-byte back to its pre-run state.
    let restored = hash_tree(dir).unwrap();
    assert_eq!(restored, before, "undo must restore the directory byte-identically");

    // Spot-check the individual outcomes too.
    assert!(!created.exists(), "created file is gone");
    assert!(dir.join("move_me.txt").exists(), "moved file is back at its origin");
    assert!(!moved.exists(), "move target no longer exists");
    assert_eq!(fs::read(&overwrite).unwrap(), b"ORIGINAL contents, a specific length");
    assert_eq!(fs::read(&deleted).unwrap(), b"soon to be deleted, restore me exactly");
    assert_eq!(fs::read(dir.join("keep.txt")).unwrap(), b"i am untouched throughout the run");

    // Undo is idempotent: a second pass restores nothing further and changes nothing.
    let second = rec.undo_last_run().unwrap();
    assert!(second.is_empty(), "re-running undo does nothing, got {second:?}");
    assert_eq!(hash_tree(dir).unwrap(), before);
}

/// F1b headline test: the recorder->bus wire this packet builds, exercised
/// against a real temp directory end to end. A run creates, moves, and
/// overwrites real files; the recorder publishes the REAL `undo.previewed`
/// event (crates/core/src/bus/events.rs's `UndoPreviewed`, with its F1b
/// `items` field) onto a REAL `operant_core::Bus`, and the published item
/// content is checked against the run's actual paths, not a fixture. Undo is
/// then executed from that same real journal, and the directory is hashed
/// before the run and again after undo to prove a BYTE-IDENTICAL restore
/// (`ui/src/undo/mockJournal.ts`'s fixture is exactly this journal shape, and
/// `ui/src/undo/realJournal.ts` decodes exactly this wire shape; this test is
/// the recorder-side half of that same contract, deterministic throughout: a
/// database read, struct construction, and an in-process bus publish, never
/// a model or network call).
#[test]
fn undo_previewed_bus_event_carries_real_items_and_undo_still_restores_byte_identical() {
    let tmp = unique_temp_dir("bus-wire");
    let dir = &tmp.0;

    fs::write(dir.join("keep.txt"), b"never touched by the run").unwrap();
    fs::write(dir.join("move_me.txt"), b"these exact bytes are about to move").unwrap();
    fs::write(dir.join("overwrite_me.txt"), b"ORIGINAL bytes before the overwrite").unwrap();
    let before = hash_tree(dir).unwrap();

    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run("mutate a temp directory via the real bus wire", RunMode::Explore, None).unwrap();

    // create
    let created = dir.join("created.txt");
    rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: created.clone() }).unwrap();
    fs::write(&created, b"made by the run").unwrap();

    // move / rename
    let moved = dir.join("moved.txt");
    rec.journal_ahead(&run_id, &PendingWrite::MoveFile { from: dir.join("move_me.txt"), to: moved.clone() })
        .unwrap();
    fs::rename(dir.join("move_me.txt"), &moved).unwrap();

    // overwrite (pre-image captured inside journal_ahead, before the clobber)
    let overwrite = dir.join("overwrite_me.txt");
    rec.journal_ahead(&run_id, &PendingWrite::OverwriteFile { path: overwrite.clone() }).unwrap();
    fs::write(&overwrite, b"CHANGED bytes, a different length than the original").unwrap();

    let after_mutations = hash_tree(dir).unwrap();
    assert_ne!(before, after_mutations, "the run must have changed the directory");

    // The recorder itself publishes the real restoration list onto a real
    // bus: the "recorder -> bus" half of this packet's wire.
    let bus = Bus::new();
    let sub = bus.subscribe("undo.*");
    rec.publish_undo_preview(&bus, &run_id).unwrap();

    let env = sub.rx.try_recv().expect("undo.previewed must be published on the real bus");
    assert_eq!(env.topic, "undo.previewed");
    assert_eq!(env.payload["run_id"], serde_json::json!(run_id));
    assert_eq!(env.payload["entries"], serde_json::json!(3), "one entry per journaled action");
    assert_eq!(env.payload["irreversible"], serde_json::json!(0));

    let items = env.payload["items"].as_array().expect("F1b's items field must be present");
    assert_eq!(items.len(), 3, "one wire item per journaled action, real content, not a fixture");
    // Newest-first: the overwrite (journaled last) previews first, and its
    // path is this run's own real path, not canned fixture text.
    assert_eq!(items[0]["op"], serde_json::json!("restore_overwritten"));
    assert_eq!(items[0]["path"], serde_json::json!(overwrite.display().to_string()));
    assert_eq!(items[1]["op"], serde_json::json!("reverse_move"));
    assert_eq!(items[1]["moved_to"], serde_json::json!(moved.display().to_string()));
    assert_eq!(items[1]["original"], serde_json::json!(dir.join("move_me.txt").display().to_string()));
    assert_eq!(items[2]["op"], serde_json::json!("delete_created"));
    assert_eq!(items[2]["path"], serde_json::json!(created.display().to_string()));

    // Publishing the preview is read-only, same guarantee as preview_undo.
    assert_eq!(hash_tree(dir).unwrap(), after_mutations, "publishing the preview must not touch the disk");

    // Execute the undo from the very same real journal the bus event above
    // was built from.
    let narration = rec.undo_last_run().unwrap();
    assert_eq!(narration.len(), 3, "one narration line per restoration");

    let restored = hash_tree(dir).unwrap();
    assert_eq!(restored, before, "undo via the real journal must restore the directory byte-identically");
    assert_eq!(fs::read(dir.join("move_me.txt")).unwrap(), b"these exact bytes are about to move");
    assert_eq!(fs::read(&overwrite).unwrap(), b"ORIGINAL bytes before the overwrite");
    assert!(!created.exists());
    assert!(!moved.exists());
}

#[test]
fn email_send_step_renders_cannot_be_undone_label() {
    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run("send the invoice email", RunMode::Explore, None).unwrap();
    rec.journal_ahead(
        &run_id,
        &PendingWrite::Irreversible { description: "email send to boss@example.com".into() },
    )
    .unwrap();

    let preview = rec.preview_undo_last_run().unwrap();
    assert_eq!(preview.len(), 1);
    assert!(
        preview[0].to_lowercase().contains("cannot be undone"),
        "irreversible step must preview as cannot be undone, got {:?}",
        preview[0]
    );

    let narration = rec.undo_last_run().unwrap();
    assert_eq!(narration.len(), 1);
    assert!(narration[0].to_lowercase().contains("cannot be undone"));
    assert!(narration[0].contains("email send to boss@example.com"));

    // It is listed, not reversed: the journal row stays unapplied and no inverse ran.
    let entries = rec.list_undo(&run_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].applied, "an irreversible step is never marked undone");
}

#[test]
fn irreversible_step_coexists_with_real_inverses() {
    let tmp = unique_temp_dir("mixed");
    let dir = &tmp.0;
    fs::write(dir.join("doc.txt"), b"original document body").unwrap();
    let before = hash_tree(dir).unwrap();

    let rec = Recorder::open_in_memory().unwrap();
    let run_id = rec.start_run("edit a doc then email it", RunMode::Explore, None).unwrap();

    // Overwrite a file, then send an email (irreversible), then create a file.
    let doc = dir.join("doc.txt");
    rec.journal_ahead(&run_id, &PendingWrite::OverwriteFile { path: doc.clone() }).unwrap();
    fs::write(&doc, b"heavily edited document body of a different size").unwrap();

    rec.journal_ahead(
        &run_id,
        &PendingWrite::Irreversible { description: "email send with the doc attached".into() },
    )
    .unwrap();

    let receipt = dir.join("receipt.txt");
    rec.journal_ahead(&run_id, &PendingWrite::CreateFile { path: receipt.clone() }).unwrap();
    fs::write(&receipt, b"a local receipt written after sending").unwrap();

    let narration = rec.undo_last_run().unwrap();
    assert_eq!(narration.len(), 3, "three journal rows, three narration lines");

    // Exactly one line is the irreversible label; the other two are real restorations.
    let cannot: Vec<&String> =
        narration.iter().filter(|l| l.to_lowercase().contains("cannot be undone")).collect();
    assert_eq!(cannot.len(), 1);
    assert!(cannot[0].contains("email send with the doc attached"));

    // The file effects were fully reversed despite the irreversible step in between.
    assert!(!receipt.exists());
    assert_eq!(fs::read(&doc).unwrap(), b"original document body");
    assert_eq!(hash_tree(dir).unwrap(), before, "file tree restored byte-identically");
}
