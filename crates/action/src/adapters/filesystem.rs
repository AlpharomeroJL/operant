//! `fs` namespace adapter: read, write, copy, move, delete
//! (`docs/specs/action.md`: "Filesystem adapter verbs: read, write, copy,
//! move, delete (delete is destructive risk class and routes through
//! recycle bin so the undo journal holds)").

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use operant_ir::RiskClass;
use serde_json::json;
use thiserror::Error;

use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

const NAMESPACE: &str = "fs";

#[derive(Debug, Error)]
pub enum FsError {
    #[error("io error on `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("`{0}` is not valid UTF-8; read it with encoding \"base64\" instead")]
    NotUtf8(String),
    #[error("invalid base64 content: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("unknown encoding `{0}`, expected \"utf8\" or \"base64\"")]
    UnknownEncoding(String),
    #[error("missing required argument `{0}`")]
    MissingArg(&'static str),
}

/// Where a trashed file goes. `delete` never unlinks outright: it always
/// goes through this seam so the file stays recoverable. The real OS
/// Recycle Bin binding plus the undo journal itself use this trait:
/// recycle-bin semantics with an undo_journal table for recovery.
/// [`SoftTrash`] is the default, dependency-free backend.
pub trait TrashStrategy: Send + Sync {
    /// Move `path` out of place. Returns where it went, when the backend
    /// can report a location (a real OS recycle bin often cannot).
    fn trash(&self, path: &Path) -> io::Result<Option<PathBuf>>;
}

/// Default [`TrashStrategy`]: moves the file into a `.operant-trash`
/// directory next to it. A numeric prefix is added on a name collision so
/// repeated deletes of same-named files never overwrite each other in the
/// trash.
#[derive(Default)]
pub struct SoftTrash;

impl TrashStrategy for SoftTrash {
    fn trash(&self, path: &Path) -> io::Result<Option<PathBuf>> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let trash_dir = match parent {
            Some(p) => p.join(".operant-trash"),
            None => PathBuf::from(".operant-trash"),
        };
        fs::create_dir_all(&trash_dir)?;
        let file_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let mut dest = trash_dir.join(file_name);
        let mut n = 0u32;
        while dest.exists() {
            n += 1;
            dest = trash_dir.join(format!("{n}.{}", file_name.to_string_lossy()));
        }
        fs::rename(path, &dest)?;
        Ok(Some(dest))
    }
}

/// `fs` namespace adapter. Stateless aside from its [`TrashStrategy`]:
/// every verb resolves paths fresh on each call, nothing is cached.
pub struct FsAdapter {
    verbs: Vec<VerbSpec>,
    trash: Box<dyn TrashStrategy>,
}

impl Default for FsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FsAdapter {
    pub fn new() -> Self {
        Self::with_trash(Box::new(SoftTrash))
    }

    /// Inject a [`TrashStrategy`], e.g. a real recycle-bin backend.
    pub fn with_trash(trash: Box<dyn TrashStrategy>) -> Self {
        Self {
            verbs: build_verbs(),
            trash,
        }
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, FsError> {
        match verb {
            "read" => read(args),
            "write" => write(args),
            "copy" => copy(args),
            "move" => move_verb(args),
            "delete" => delete(args, self.trash.as_ref()),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs FsAdapter::verbs() declared, got `{other}`"
            ),
        }
    }
}

impl Adapter for FsAdapter {
    fn namespace(&self) -> &str {
        NAMESPACE
    }

    fn verbs(&self) -> &[VerbSpec] {
        &self.verbs
    }

    fn call(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, AdapterError> {
        self.call_inner(verb, args)
            .map_err(|e| AdapterError::CallFailed {
                namespace: NAMESPACE.to_string(),
                verb: verb.to_string(),
                message: e.to_string(),
            })
    }
}

fn build_verbs() -> Vec<VerbSpec> {
    let path_prop = json!({ "type": "string", "minLength": 1 });
    let encoding_prop = json!({ "type": "string", "enum": ["utf8", "base64"], "default": "utf8" });
    vec![
        VerbSpec::new(
            "read",
            json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": path_prop,
                    "encoding": encoding_prop
                },
                "additionalProperties": false
            }),
            RiskClass::Read,
            // Reads have no side effect: safe to retry unconditionally.
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "write",
            json!({
                "type": "object",
                "required": ["path", "content"],
                "properties": {
                    "path": { "type": "string", "minLength": 1 },
                    "content": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["utf8", "base64"], "default": "utf8" },
                    "create_dirs": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            // A write is a full overwrite (never append): retrying with the
            // same args reproduces the same end state.
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "copy",
            json!({
                "type": "object",
                "required": ["from", "to"],
                "properties": {
                    "from": { "type": "string", "minLength": 1 },
                    "to": { "type": "string", "minLength": 1 },
                    "overwrite": { "type": "boolean", "default": false },
                    "create_dirs": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "move",
            json!({
                "type": "object",
                "required": ["from", "to"],
                "properties": {
                    "from": { "type": "string", "minLength": 1 },
                    "to": { "type": "string", "minLength": 1 },
                    "overwrite": { "type": "boolean", "default": false },
                    "create_dirs": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            // A retry after a successful move finds `from` already gone;
            // whether that reads as "safe" depends on what else may have
            // since claimed that path. Not established either way.
            Idempotency::Unknown,
        ),
        VerbSpec::new(
            "delete",
            json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Destructive,
            // "Ensure absent" semantics: deleting an already-gone path is a
            // successful no-op (see `delete` below), so retrying is safe.
            Idempotency::Idempotent,
        ),
    ]
}

fn arg_str<'a>(args: &'a serde_json::Value, key: &'static str) -> Result<&'a str, FsError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or(FsError::MissingArg(key))
}

fn arg_bool(args: &serde_json::Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn encoding_of(args: &serde_json::Value) -> &str {
    args.get("encoding")
        .and_then(|v| v.as_str())
        .unwrap_or("utf8")
}

fn read(args: &serde_json::Value) -> Result<serde_json::Value, FsError> {
    let path = arg_str(args, "path")?;
    let encoding = encoding_of(args);
    let bytes = fs::read(path).map_err(|source| FsError::Io {
        path: path.to_string(),
        source,
    })?;
    let content = match encoding {
        "utf8" => {
            String::from_utf8(bytes.clone()).map_err(|_| FsError::NotUtf8(path.to_string()))?
        }
        "base64" => BASE64.encode(&bytes),
        other => return Err(FsError::UnknownEncoding(other.to_string())),
    };
    Ok(json!({
        "path": path,
        "encoding": encoding,
        "content": content,
        "size_bytes": bytes.len()
    }))
}

fn write(args: &serde_json::Value) -> Result<serde_json::Value, FsError> {
    let path = arg_str(args, "path")?;
    let content = arg_str(args, "content")?;
    let encoding = encoding_of(args);
    let create_dirs = arg_bool(args, "create_dirs", false);
    let bytes: Vec<u8> = match encoding {
        "utf8" => content.as_bytes().to_vec(),
        "base64" => BASE64.decode(content)?,
        other => return Err(FsError::UnknownEncoding(other.to_string())),
    };
    let path_ref = Path::new(path);
    if create_dirs {
        create_parent_dirs(path_ref).map_err(|source| FsError::Io {
            path: path.to_string(),
            source,
        })?;
    }
    fs::write(path_ref, &bytes).map_err(|source| FsError::Io {
        path: path.to_string(),
        source,
    })?;
    Ok(json!({ "path": path, "bytes_written": bytes.len() }))
}

fn copy(args: &serde_json::Value) -> Result<serde_json::Value, FsError> {
    let from = arg_str(args, "from")?;
    let to = arg_str(args, "to")?;
    let overwrite = arg_bool(args, "overwrite", false);
    let create_dirs = arg_bool(args, "create_dirs", false);
    prepare_destination(to, overwrite, create_dirs)?;
    let bytes_copied = fs::copy(from, to).map_err(|source| FsError::Io {
        path: from.to_string(),
        source,
    })?;
    Ok(json!({ "from": from, "to": to, "bytes_copied": bytes_copied }))
}

fn move_verb(args: &serde_json::Value) -> Result<serde_json::Value, FsError> {
    let from = arg_str(args, "from")?;
    let to = arg_str(args, "to")?;
    let overwrite = arg_bool(args, "overwrite", false);
    let create_dirs = arg_bool(args, "create_dirs", false);
    prepare_destination(to, overwrite, create_dirs)?;
    if fs::rename(from, to).is_err() {
        // Cross-device (e.g. a different drive letter) rename fails on
        // Windows; fall back to copy-then-remove so `move` still works
        // across volumes.
        fs::copy(from, to).map_err(|source| FsError::Io {
            path: from.to_string(),
            source,
        })?;
        fs::remove_file(from).map_err(|source| FsError::Io {
            path: from.to_string(),
            source,
        })?;
    }
    Ok(json!({ "from": from, "to": to }))
}

fn delete(
    args: &serde_json::Value,
    trash: &dyn TrashStrategy,
) -> Result<serde_json::Value, FsError> {
    let path = arg_str(args, "path")?;
    let path_ref = Path::new(path);
    if !path_ref.exists() {
        // Idempotent delete: "ensure absent" already holds.
        return Ok(json!({ "path": path, "existed": false, "trashed_to": null }));
    }
    let trashed_to = trash.trash(path_ref).map_err(|source| FsError::Io {
        path: path.to_string(),
        source,
    })?;
    Ok(json!({
        "path": path,
        "existed": true,
        "trashed_to": trashed_to.map(|p| p.to_string_lossy().into_owned())
    }))
}

fn create_parent_dirs(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn prepare_destination(to: &str, overwrite: bool, create_dirs: bool) -> Result<(), FsError> {
    let to_path = Path::new(to);
    if create_dirs {
        create_parent_dirs(to_path).map_err(|source| FsError::Io {
            path: to.to_string(),
            source,
        })?;
    }
    if !overwrite && to_path.exists() {
        return Err(FsError::Io {
            path: to.to_string(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination exists and overwrite is false",
            ),
        });
    }
    Ok(())
}

/// Unique-per-call temp directory under the OS temp root, removed on
/// `Drop`. Test-only: keeps every test's filesystem fixture isolated
/// without adding a `tempfile` dependency for one helper.
#[cfg(test)]
struct TempDir(PathBuf);

#[cfg(test)]
impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("operant-fs-test-{tag}-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp test dir");
        Self(dir)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

#[cfg(test)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> FsAdapter {
        FsAdapter::new()
    }

    fn p(path: &Path) -> serde_json::Value {
        json!(path.to_string_lossy())
    }

    #[test]
    fn write_then_read_round_trips_utf8() {
        let dir = TempDir::new("rw");
        let path = dir.join("hello.txt");
        let a = adapter();
        let out = a
            .call("write", &json!({ "path": p(&path), "content": "hi there" }))
            .unwrap();
        assert_eq!(out["bytes_written"], json!(8));
        let out = a.call("read", &json!({ "path": p(&path) })).unwrap();
        assert_eq!(out["content"], json!("hi there"));
        assert_eq!(out["encoding"], json!("utf8"));
    }

    #[test]
    fn write_can_create_missing_parent_dirs() {
        let dir = TempDir::new("mkdirp");
        let path = dir.join("nested").join("deep").join("file.txt");
        let a = adapter();
        a.call(
            "write",
            &json!({ "path": p(&path), "content": "x", "create_dirs": true }),
        )
        .unwrap();
        assert!(path.exists());
    }

    #[test]
    fn read_binary_via_base64_round_trips() {
        let dir = TempDir::new("b64");
        let path = dir.join("bin.dat");
        fs::write(&path, [0u8, 159, 146, 150]).unwrap(); // not valid UTF-8
        let a = adapter();

        let err = a.call("read", &json!({ "path": p(&path) })).unwrap_err();
        assert!(matches!(err, AdapterError::CallFailed { .. }));

        let out = a
            .call("read", &json!({ "path": p(&path), "encoding": "base64" }))
            .unwrap();
        let decoded = BASE64.decode(out["content"].as_str().unwrap()).unwrap();
        assert_eq!(decoded, vec![0u8, 159, 146, 150]);
    }

    #[test]
    fn copy_then_move_then_delete_round_trips() {
        let dir = TempDir::new("cmd");
        let src = dir.join("a.txt");
        fs::write(&src, "payload").unwrap();
        let a = adapter();

        let dst1 = dir.join("b.txt");
        let out = a
            .call("copy", &json!({ "from": p(&src), "to": p(&dst1) }))
            .unwrap();
        assert_eq!(out["bytes_copied"], json!(7));
        assert!(dst1.exists());
        assert!(src.exists(), "copy leaves the source in place");

        let dst2 = dir.join("c.txt");
        a.call("move", &json!({ "from": p(&dst1), "to": p(&dst2) }))
            .unwrap();
        assert!(!dst1.exists());
        assert!(dst2.exists());

        let out = a.call("delete", &json!({ "path": p(&dst2) })).unwrap();
        assert_eq!(out["existed"], json!(true));
        assert!(!dst2.exists(), "delete removes the file from its path");
        let trashed_to = out["trashed_to"].as_str().unwrap();
        assert!(
            Path::new(trashed_to).exists(),
            "delete recycles, it does not unlink"
        );
        assert_eq!(fs::read_to_string(trashed_to).unwrap(), "payload");
    }

    #[test]
    fn delete_of_missing_path_is_idempotent() {
        let dir = TempDir::new("del-missing");
        let a = adapter();
        let ghost = dir.join("never-existed.txt");
        let out = a.call("delete", &json!({ "path": p(&ghost) })).unwrap();
        assert_eq!(out["existed"], json!(false));
        assert_eq!(out["trashed_to"], serde_json::Value::Null);
    }

    #[test]
    fn repeated_deletes_of_same_named_files_do_not_collide_in_trash() {
        let dir = TempDir::new("del-collide");
        let a = adapter();
        let path = dir.join("dup.txt");

        fs::write(&path, "first").unwrap();
        a.call("delete", &json!({ "path": p(&path) })).unwrap();
        fs::write(&path, "second").unwrap();
        let out = a.call("delete", &json!({ "path": p(&path) })).unwrap();

        let trashed_to = out["trashed_to"].as_str().unwrap();
        assert_eq!(fs::read_to_string(trashed_to).unwrap(), "second");
        let trash_dir = dir.join(".operant-trash");
        let entries: Vec<_> = fs::read_dir(&trash_dir).unwrap().collect();
        assert_eq!(entries.len(), 2, "both trashed copies must survive");
    }

    #[test]
    fn copy_refuses_overwrite_by_default() {
        let dir = TempDir::new("no-overwrite");
        let src = dir.join("a.txt");
        let dst = dir.join("b.txt");
        fs::write(&src, "one").unwrap();
        fs::write(&dst, "two").unwrap();
        let a = adapter();

        let err = a
            .call("copy", &json!({ "from": p(&src), "to": p(&dst) }))
            .unwrap_err();
        assert!(matches!(err, AdapterError::CallFailed { .. }));
        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "two",
            "a refused copy must not touch the destination"
        );

        a.call(
            "copy",
            &json!({ "from": p(&src), "to": p(&dst), "overwrite": true }),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&dst).unwrap(), "one");
    }

    #[test]
    fn schema_rejects_an_unknown_encoding_via_the_registry() {
        let mut reg = crate::adapter::AdapterRegistry::new();
        reg.register(Box::new(adapter()));
        let err = reg
            .validate("fs", "read", &json!({ "path": "x", "encoding": "shouty" }))
            .unwrap_err();
        assert!(matches!(err, AdapterError::SchemaValidation { .. }));
    }

    #[test]
    fn delete_is_destructive_and_needs_approval_through_the_executor() {
        use crate::{AdapterRegistry, Approval, Executor, MockSynthesizer, NoopSleeper};
        use operant_ir::{Action, ActionKind, Grounding, Pace, Retry};

        let dir = TempDir::new("exec-delete");
        let victim = dir.join("victim.txt");
        fs::write(&victim, "bye").unwrap();

        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(FsAdapter::new()));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("fs"));
        params.insert("verb".into(), json!("delete"));
        params.insert("args".into(), json!({ "path": p(&victim) }));
        let action = Action {
            v: 1,
            id: "delete-victim".into(),
            kind: ActionKind::AdapterCall,
            intent: None,
            target: None,
            params,
            pace: Pace::Instant,
            risk_class: RiskClass::Destructive,
            irreversible: false,
            grounding: Grounding::Adapter,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 0,
                backoff_ms: 0,
            },
        };

        let refused = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(
            refused,
            crate::ActionError::ApprovalRequired { .. }
        ));
        assert!(victim.exists(), "a refused delete must not touch the file");

        let approval = Approval::for_action("delete-victim", "josef");
        let outcome = exec.execute(&action, None, Some(&approval)).unwrap();
        assert!(!victim.exists());
        assert_eq!(outcome.adapter_result.unwrap()["existed"], json!(true));
    }
}
