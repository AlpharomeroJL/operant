//! `operant install <registry-dir> <name>[@version]`: fetch a manifest and
//! its hash-pinned DSL file from a local registry index checkout (a clone
//! of `../operant-registry`, R1A's layout: `index.json`,
//! `manifests/<name>/<version>.json`, `keys/<publisher>.pub`,
//! `<dsl.url>`), run it through `operant-registry`'s staged install
//! pipeline (R1B), and print the plain-language rendering -- step summary,
//! grants, trust note -- an approval surface must show before anything
//! installs, per `docs/specs/registry.md`.
//!
//! Headless and deterministic like every other verb (`cli/src/main.rs`):
//! no interactive prompt. Without `--approve` this only prints the
//! rendering (a preview: nothing is stored). With `--approve` it stores the
//! workflow via `operant-registry::FsStore` and persists the publisher pin.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use operant_registry::{
    parse_publisher_key_hex, Approval, FetchedManifest, FsStore, PinStore, Trust,
};

const DEFAULT_STORE: &str = ".operant/installed";
const DEFAULT_PINS: &str = ".operant/pins.json";

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut positional = Vec::new();
    let mut approve = false;
    let mut store_dir = DEFAULT_STORE.to_string();
    let mut pins_path = DEFAULT_PINS.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--approve" => approve = true,
            "--store" => {
                i += 1;
                store_dir = args.get(i).context("--store needs a value")?.clone();
            }
            "--pins" => {
                i += 1;
                pins_path = args.get(i).context("--pins needs a value")?.clone();
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let registry_dir = positional
        .first()
        .context("usage: operant install <registry-dir> <name>[@version] [--approve]")?;
    let want = positional
        .get(1)
        .context("usage: operant install <registry-dir> <name>[@version] [--approve]")?;
    let (name, version) = match want.split_once('@') {
        Some((n, v)) => (n, Some(v)),
        None => (want.as_str(), None),
    };

    let registry_dir = Path::new(registry_dir);
    let manifest_rel = resolve_manifest_path(registry_dir, name, version)?;
    let manifest_path = registry_dir.join(&manifest_rel);
    let manifest_bytes = fs::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let fetched = FetchedManifest::parse(&manifest_bytes)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    let publisher_key = load_publisher_key(registry_dir, &fetched.manifest.publisher)?;

    let mut pins = load_pins(&pins_path)?;
    let signed = fetched
        .verify_signature(publisher_key.as_ref().map(|k| k.as_slice()), &mut pins)
        .context("signature verification failed")?;
    save_pins(&pins_path, &pins)?;

    let dsl_path = registry_dir.join(&signed.manifest.dsl.url);
    let dsl_bytes =
        fs::read(&dsl_path).with_context(|| format!("reading {}", dsl_path.display()))?;
    let verified = signed
        .verify_dsl(dsl_bytes)
        .context("the fetched workflow file does not match the manifest")?;

    let rendered = verified.render();
    print_rendering(&rendered.rendering, rendered.trust);

    if !approve {
        println!();
        println!("Preview only: nothing was installed. Re-run with --approve to install.");
        return Ok(());
    }

    let installed = rendered
        .decide(Approval::Approved)?
        .store(&mut FsStore::new(&store_dir))?;

    println!();
    if installed.dry_run {
        println!(
            "Installed {}@{} (preview mode: it will not do anything until you promote it).",
            installed.name, installed.version
        );
    } else {
        println!("Installed {}@{}. Ready to run.", installed.name, installed.version);
    }
    Ok(())
}

fn resolve_manifest_path(registry_dir: &Path, name: &str, version: Option<&str>) -> Result<String> {
    let index_path = registry_dir.join("index.json");
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(&index_path).with_context(|| format!("reading {}", index_path.display()))?,
    )
    .with_context(|| format!("parsing {}", index_path.display()))?;

    let entry = index
        .get("workflows")
        .and_then(|w| w.get(name))
        .with_context(|| format!("no workflow named `{name}` in this registry"))?;

    let version = match version {
        Some(v) => v.to_string(),
        None => entry
            .get("latest")
            .and_then(|v| v.as_str())
            .with_context(|| format!("workflow `{name}` has no `latest` version listed"))?
            .to_string(),
    };

    let manifest_rel = entry
        .get("versions")
        .and_then(|v| v.get(&version))
        .and_then(|v| v.get("manifest"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("no version `{version}` of `{name}` in this registry"))?;
    Ok(manifest_rel.to_string())
}

fn load_publisher_key(registry_dir: &Path, publisher: &str) -> Result<Option<[u8; 32]>> {
    let key_path = registry_dir.join("keys").join(format!("{publisher}.pub"));
    if !key_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&key_path)
        .with_context(|| format!("reading {}", key_path.display()))?;
    let key = parse_publisher_key_hex(&raw)
        .with_context(|| format!("{} is not a valid publisher key", key_path.display()))?;
    Ok(Some(key))
}

fn load_pins(path: &str) -> Result<PinStore> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Ok(PinStore::new());
    }
    let raw = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let map: HashMap<String, String> =
        serde_json::from_slice(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(PinStore::from_pins(map))
}

fn save_pins(path: &str, pins: &PinStore) -> Result<()> {
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&path, serde_json::to_vec_pretty(pins.pins())?)
        .with_context(|| format!("writing {}", path.display()))
}

fn print_rendering(rendering: &operant_registry::GrantRendering, trust: Trust) {
    println!(
        "{} v{}  (publisher: {})",
        rendering.name, rendering.version, rendering.publisher
    );
    println!();
    if !rendering.step_summary.is_empty() {
        println!("Steps:");
        for (i, step) in rendering.step_summary.iter().enumerate() {
            println!("  {}. {}", i + 1, step);
        }
        println!();
    }
    println!("This workflow needs permission to:");
    for grant in &rendering.grants {
        println!("  - {grant}");
    }
    println!();
    println!("{}", rendering.trust_note);
    if trust == Trust::Trusted {
        println!("This publisher is already trusted, so this installs ready to run.");
    }
}

fn print_help() {
    println!("operant install <registry-dir> <name>[@version] [--approve] [--store <dir>] [--pins <file>]");
    println!();
    println!("Fetch a workflow from a local registry checkout, verify its signature and");
    println!("file hash, and show the plain-language steps and permissions it needs.");
    println!("Without --approve this only previews; nothing is installed. --store defaults");
    println!("to {DEFAULT_STORE}, --pins to {DEFAULT_PINS}.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_registry(dir: &Path) {
        fs::create_dir_all(dir.join("manifests/notepad-invoice-note")).unwrap();
        fs::create_dir_all(dir.join("keys")).unwrap();
        // The committed fixture manifest's dsl.url is "workflow_notepad/workflow.ts"
        // (contracts/fixtures/registry/manifest.json), so the workflow file must
        // land at that exact relative path for the hash-pin check to find it.
        fs::create_dir_all(dir.join("workflow_notepad")).unwrap();

        let manifest = include_str!("../../../contracts/fixtures/registry/manifest.json");
        fs::write(
            dir.join("manifests/notepad-invoice-note/1.0.0.json"),
            manifest,
        )
        .unwrap();
        fs::write(
            dir.join("keys/operant-fixtures.pub"),
            include_str!("../../../contracts/fixtures/registry/publisher.pub"),
        )
        .unwrap();
        fs::write(
            dir.join("workflow_notepad/workflow.ts"),
            include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts"),
        )
        .unwrap();

        let index = serde_json::json!({
            "v": 1,
            "updated": "2026-07-11",
            "workflows": {
                "notepad-invoice-note": {
                    "latest": "1.0.0",
                    "versions": {
                        "1.0.0": {
                            "manifest": "manifests/notepad-invoice-note/1.0.0.json",
                            "publisher": "operant-fixtures",
                            "description": "Writes a dated invoice note into Notepad and saves it."
                        }
                    }
                }
            }
        });
        fs::write(
            dir.join("index.json"),
            serde_json::to_vec_pretty(&index).unwrap(),
        )
        .unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("operant-cli-install-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_manifest_path_uses_latest_when_no_version_given() {
        let dir = temp_dir("resolve");
        fixture_registry(&dir);
        let rel = resolve_manifest_path(&dir, "notepad-invoice-note", None).unwrap();
        assert_eq!(rel, "manifests/notepad-invoice-note/1.0.0.json");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_without_approve_does_not_write_the_store() {
        let dir = temp_dir("preview");
        fixture_registry(&dir);
        let store = dir.join("installed");
        let pins = dir.join("pins.json");

        let args = vec![
            dir.to_string_lossy().to_string(),
            "notepad-invoice-note".to_string(),
            "--store".to_string(),
            store.to_string_lossy().to_string(),
            "--pins".to_string(),
            pins.to_string_lossy().to_string(),
        ];
        run(&args).expect("preview install succeeds");
        assert!(!store.exists(), "a preview must not write the store");
        // Pin-on-first-use bookkeeping still happens during the preview: the
        // trust decision is part of reading the manifest, not of approving it.
        let saved: HashMap<String, String> =
            serde_json::from_slice(&fs::read(&pins).unwrap()).unwrap();
        assert_eq!(
            saved.get("operant-fixtures").map(String::as_str),
            Some("e7f1a7f9ce2a6110cdc750301d5f47c6")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // BAR: approving a first-use publisher installs the fixture workflow but
    // flags it dry-run-only, and a second install of the same publisher
    // (now pinned) installs ready to run.
    #[test]
    fn approve_installs_first_use_dry_run_then_pinned_publisher_installs_live() {
        let dir = temp_dir("approve");
        fixture_registry(&dir);
        let store = dir.join("installed");
        let pins = dir.join("pins.json");

        let args = |extra: bool| {
            let mut a = vec![
                dir.to_string_lossy().to_string(),
                "notepad-invoice-note".to_string(),
                "--store".to_string(),
                store.to_string_lossy().to_string(),
                "--pins".to_string(),
                pins.to_string_lossy().to_string(),
            ];
            if extra {
                a.push("--approve".to_string());
            }
            a
        };

        run(&args(true)).expect("first install succeeds");
        let manifest_out = store.join("notepad-invoice-note/1.0.0/manifest.json");
        assert!(manifest_out.exists());
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(store.join("notepad-invoice-note/1.0.0/state.json")).unwrap())
                .unwrap();
        assert_eq!(state["dry_run"], serde_json::json!(true));

        // Re-run: the publisher is now pinned, so this install lands live.
        run(&args(true)).expect("second install succeeds");
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(store.join("notepad-invoice-note/1.0.0/state.json")).unwrap())
                .unwrap();
        assert_eq!(state["dry_run"], serde_json::json!(false));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_workflow_name_is_a_clear_error() {
        let dir = temp_dir("missing");
        fixture_registry(&dir);
        let args = vec![dir.to_string_lossy().to_string(), "does-not-exist".to_string()];
        let err = run(&args).expect_err("unknown workflow must fail, not silently no-op");
        assert!(err.to_string().contains("does-not-exist"));
        let _ = fs::remove_dir_all(&dir);
    }
}
