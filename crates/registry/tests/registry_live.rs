//! C5 registry-live: first contact between the unit tests and a real signed
//! workflow manifest served from a registry repo.
//!
//! The unit tests in `src/` exercise verify/install against the committed
//! fixture manifest in isolation. This suite closes the last gap: it signs a
//! manifest with the crate's own publisher signing path
//! (`parse_publisher_key_pem` + `sign_manifest`), lays it out on disk in the
//! real registry-repo layout (`index.json`, `manifests/<name>/<version>.json`,
//! `keys/<publisher>.pub`, and the hash-pinned DSL file), fetches it back the
//! way `operant install` resolves a local registry checkout, and runs the
//! fetched bytes through the real staged install pipeline. Nothing here mocks
//! verification or install: the only test-owned code is the transport (reading
//! files out of the on-disk registry), which is exactly what the task asks to
//! wire.
//!
//! Two things are proven end to end:
//!   1. A validly signed manifest fetched through a registry index verifies
//!      and installs, dry-run-only on first use and ready to run once the
//!      publisher is pinned.
//!   2. Any tamper (manifest body, signature, served DSL file, or the wrong
//!      publisher key) is refused before anything is stored.
//!
//! A final test installs the ACTUAL manifest published to the registry repo
//! (`github.com/AlpharomeroJL/operant-registry`) from a local checkout when one
//! is present, so the real published bytes are exercised on this box; it skips
//! cleanly elsewhere so `cargo test -p operant-registry` stays portable.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use operant_registry::{
    dsl_hash, fingerprint, install, parse_publisher_key_hex, parse_publisher_key_pem,
    sign_manifest, Approval, FsStore, InstallStore, PinStore, RegistryError, SigningKey,
};

const BASE_MANIFEST: &str = include_str!("../../../contracts/fixtures/registry/manifest.json");
const PUBLISHER_KEY_PEM: &str = include_str!("../../../contracts/fixtures/registry/publisher.key");
const DSL: &[u8] = include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts");

const NAME: &str = "notepad-invoice-note";
const VERSION: &str = "1.0.0";
const PUBLISHER: &str = "operant-fixtures";
// The registry repo stores the compiled DSL under `workflows/<name>/<ver>.ts`
// (github.com/AlpharomeroJL/operant-registry), a different path than the
// contracts fixture, so signing over it produces a genuinely fresh signature.
const DSL_URL: &str = "workflows/notepad-invoice-note/1.0.0.ts";
const FINGERPRINT: &str = "e7f1a7f9ce2a6110cdc750301d5f47c6";

// ---------------------------------------------------------------------------
// Helpers: the publisher signing path, a locally-served registry, and a fetch
// that mirrors how `operant install` resolves a local registry checkout.
// ---------------------------------------------------------------------------

fn signing_key() -> SigningKey {
    parse_publisher_key_pem(PUBLISHER_KEY_PEM).expect("fixture publisher key parses")
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn pub_hex(key: &SigningKey) -> String {
    format!("{}\n", hex_encode(&key.verifying_key().to_bytes()))
}

/// Take the committed manifest as a template, re-point its DSL at the registry
/// repo layout, re-pin the DSL hash and publisher fingerprint, and sign the
/// canonical bytes with the crate's own signing path. The result is a real,
/// freshly produced signature over different canonical JSON than either
/// committed fixture.
fn build_signed_manifest(key: &SigningKey) -> Vec<u8> {
    let mut value: serde_json::Value = serde_json::from_str(BASE_MANIFEST).unwrap();
    value.as_object_mut().unwrap().remove("signature");
    value["dsl"]["url"] = json!(DSL_URL);
    value["dsl"]["hash"] = json!(dsl_hash(DSL));
    value["pubkey_fingerprint"] = json!(fingerprint(&key.verifying_key().to_bytes()));
    let signature = sign_manifest(&value, key);
    value["signature"] = json!({ "sig": signature.sig });
    serde_json::to_vec_pretty(&value).unwrap()
}

/// Write a registry repo layout to `root`: index, manifest, publisher key, and
/// the hash-pinned DSL file at `dsl_url`.
fn write_registry(root: &Path, manifest_bytes: &[u8], key_pub: &str, dsl_bytes: &[u8], dsl_url: &str) {
    fs::create_dir_all(root.join("manifests").join(NAME)).unwrap();
    fs::create_dir_all(root.join("keys")).unwrap();
    if let Some(parent) = root.join(dsl_url).parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        root.join("manifests").join(NAME).join(format!("{VERSION}.json")),
        manifest_bytes,
    )
    .unwrap();
    fs::write(root.join("keys").join(format!("{PUBLISHER}.pub")), key_pub).unwrap();
    fs::write(root.join(dsl_url), dsl_bytes).unwrap();

    let index = json!({
        "v": 1,
        "updated": "2026-07-15",
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
    fs::write(root.join("index.json"), serde_json::to_vec_pretty(&index).unwrap()).unwrap();
}

/// Resolve and read a manifest, its publisher key, and its hash-pinned DSL out
/// of a registry checkout exactly the way `operant install` does: index.json
/// gives the manifest path, the manifest names the publisher and DSL URL.
fn fetch(root: &Path) -> (Vec<u8>, [u8; 32], Vec<u8>) {
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("index.json")).unwrap()).unwrap();
    let manifest_rel = index["workflows"][NAME]["versions"][VERSION]["manifest"]
        .as_str()
        .expect("index resolves the manifest path");
    let manifest_bytes = fs::read(root.join(manifest_rel)).unwrap();

    let meta: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let publisher = meta["publisher"].as_str().unwrap();
    let dsl_url = meta["dsl"]["url"].as_str().unwrap();

    let key_hex = fs::read_to_string(root.join("keys").join(format!("{publisher}.pub"))).unwrap();
    let key = parse_publisher_key_hex(&key_hex).expect("registry publisher key parses");
    let dsl_bytes = fs::read(root.join(dsl_url)).unwrap();

    (manifest_bytes, key, dsl_bytes)
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "operant-registry-live-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A refused install must never reach the store: no record, no directory.
fn assert_no_install(dir: &Path, store: &FsStore) {
    assert!(
        store.get(NAME, VERSION).is_none(),
        "a refused install must not be stored"
    );
    assert!(
        !dir.join("store").join(NAME).exists(),
        "no store entry may be written for a refused install"
    );
}

/// Corrupt a base64 signature while keeping it valid base64 of the right
/// length, so verification fails on the signature itself rather than on a
/// decode or length error.
fn corrupt_sig(sig: &str) -> String {
    let first = sig.chars().next().expect("signature is non-empty");
    let replacement = if first == 'A' { 'B' } else { 'A' };
    let mut chars: Vec<char> = sig.chars().collect();
    chars[0] = replacement;
    chars.into_iter().collect()
}

// ---------------------------------------------------------------------------
// 1. A real signed manifest, fetched from a registry index, verifies and
//    installs.
// ---------------------------------------------------------------------------

#[test]
fn signed_manifest_fetched_from_a_registry_index_verifies_and_installs() {
    let key = signing_key();
    let manifest = build_signed_manifest(&key);
    let dir = temp_dir("verify-install");
    write_registry(&dir, &manifest, &pub_hex(&key), DSL, DSL_URL);

    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let installed = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect("a validly signed manifest fetched from the registry installs");

    // First observation of this publisher: it installs, but dry-run-only until
    // the user promotes it, and the publisher is now pinned by fingerprint.
    assert!(installed.dry_run, "first use of a publisher is dry-run-only");
    assert_eq!(pins.fingerprint_for(PUBLISHER), Some(FINGERPRINT));

    // It really landed in the store and round-trips with its DSL intact.
    let reloaded = store.get(NAME, VERSION).expect("installed workflow reloads");
    assert_eq!(reloaded.name, NAME);
    assert_eq!(reloaded.dsl_bytes, DSL);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_pinned_publishers_signed_manifest_installs_ready_to_run() {
    let key = signing_key();
    let manifest = build_signed_manifest(&key);
    let dir = temp_dir("ready-to-run");
    write_registry(&dir, &manifest, &pub_hex(&key), DSL, DSL_URL);
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    // Publisher already pinned from a prior trusted install: no dry-run gate.
    let mut pins = PinStore::new();
    pins.observe(PUBLISHER, FINGERPRINT).unwrap();
    let mut store = FsStore::new(dir.join("store"));

    let installed = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect("a signed manifest from a pinned publisher installs");

    assert!(
        !installed.dry_run,
        "a pinned publisher installs ready to run, not dry-run-only"
    );

    // Persisted as live: state.json records dry_run=false, and the workflow is
    // reloadable with its DSL bytes.
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("store").join(NAME).join(VERSION).join("state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["dry_run"], json!(false));
    let reloaded = store.get(NAME, VERSION).expect("installed workflow reloads");
    assert!(!reloaded.dry_run);
    assert_eq!(reloaded.dsl_bytes, DSL);

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 2. Tamper anywhere in the served registry: refused before install.
// ---------------------------------------------------------------------------

#[test]
fn a_tampered_manifest_body_is_refused_and_nothing_is_installed() {
    let key = signing_key();
    let manifest = String::from_utf8(build_signed_manifest(&key)).unwrap();
    // Flip one byte inside a signed field (the human-readable description).
    let tampered = manifest.replacen(
        "Writes a dated invoice note",
        "Xrites a dated invoice note",
        1,
    );
    assert_ne!(tampered, manifest, "sanity: the byte was actually flipped");

    let dir = temp_dir("tamper-body");
    write_registry(&dir, tampered.as_bytes(), &pub_hex(&key), DSL, DSL_URL);
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let err = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect_err("a tampered manifest body must be refused");
    assert!(matches!(err, RegistryError::SignatureInvalid { .. }));
    assert_no_install(&dir, &store);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_tampered_signature_is_refused_and_nothing_is_installed() {
    let key = signing_key();
    let mut value: serde_json::Value =
        serde_json::from_slice(&build_signed_manifest(&key)).unwrap();
    let sig = value["signature"]["sig"].as_str().unwrap().to_string();
    let corrupted = corrupt_sig(&sig);
    assert_ne!(corrupted, sig, "sanity: the signature was actually changed");
    value["signature"]["sig"] = json!(corrupted);
    let manifest = serde_json::to_vec_pretty(&value).unwrap();

    let dir = temp_dir("tamper-sig");
    write_registry(&dir, &manifest, &pub_hex(&key), DSL, DSL_URL);
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let err = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect_err("a corrupted signature must be refused");
    assert!(matches!(err, RegistryError::SignatureInvalid { .. }));
    assert_no_install(&dir, &store);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_tampered_dsl_file_is_refused_and_nothing_is_installed() {
    let key = signing_key();
    let manifest = build_signed_manifest(&key);
    let dir = temp_dir("tamper-dsl");

    // The manifest and its signature are untouched; only the served DSL file is
    // altered, so the hash pin (checked after the signature) catches it.
    let mut tampered_dsl = DSL.to_vec();
    tampered_dsl.extend_from_slice(b"\n// injected line the manifest never hashed\n");
    write_registry(&dir, &manifest, &pub_hex(&key), &tampered_dsl, DSL_URL);
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let err = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect_err("a tampered DSL file must be refused");
    assert!(matches!(err, RegistryError::DslHashMismatch { .. }));
    assert_no_install(&dir, &store);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_manifest_served_with_the_wrong_publisher_key_is_refused() {
    let key = signing_key();
    let manifest = build_signed_manifest(&key);
    let dir = temp_dir("wrong-key");

    // A different but well-formed 32-byte key in keys/: its fingerprint cannot
    // match the one the manifest commits to, so verification refuses it before
    // the signature is even checked.
    let wrong_key = format!("{}\n", "00".repeat(32));
    write_registry(&dir, &manifest, &wrong_key, DSL, DSL_URL);
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&dir);

    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let err = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect_err("the wrong publisher key must be refused");
    assert!(matches!(err, RegistryError::PublisherKeyMismatch { .. }));
    assert_no_install(&dir, &store);

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 3. The actual published registry repo (when a local checkout is present).
// ---------------------------------------------------------------------------

/// Locate a checkout of the registry repo: an explicit override, then the
/// conventional sibling checkout the CLI documents as `../operant-registry`
/// relative to the operant repo root.
fn published_registry_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("OPERANT_REGISTRY_DIR") {
        let p = PathBuf::from(dir);
        if p.join("index.json").is_file() {
            return Some(p);
        }
    }
    let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../operant-registry");
    if sibling.join("index.json").is_file() {
        return Some(sibling);
    }
    None
}

#[test]
fn the_published_registry_repo_manifest_verifies_and_installs() {
    let Some(registry) = published_registry_dir() else {
        eprintln!(
            "skipping the-published-registry-repo test: no registry checkout found. \
             Set OPERANT_REGISTRY_DIR to a clone of github.com/AlpharomeroJL/operant-registry \
             (or place it at ../operant-registry) to exercise the real published manifest."
        );
        return;
    };

    // Fetch the ACTUAL published manifest, key, and DSL from the checkout.
    let (manifest_bytes, pub_key, dsl_bytes) = fetch(&registry);

    // First install: verifies against the pinned publisher key and lands
    // dry-run-only on first use, exactly as the spec requires.
    let dir = temp_dir("published-live");
    let mut pins = PinStore::new();
    let mut store = FsStore::new(dir.join("store"));
    let installed = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes.clone(),
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect("the real published manifest verifies and installs");
    assert!(installed.dry_run, "first use is dry-run-only");
    assert_eq!(pins.fingerprint_for(PUBLISHER), Some(FINGERPRINT));

    // Second install, now that the publisher is pinned: ready to run.
    let live = install(
        &manifest_bytes,
        Some(pub_key.as_slice()),
        dsl_bytes.clone(),
        &mut pins,
        Approval::Approved,
        &mut store,
    )
    .expect("second install of a now-pinned publisher succeeds");
    assert!(!live.dry_run, "a pinned publisher installs ready to run");
    assert_eq!(live.dsl_bytes, dsl_bytes);

    // Tamper the real published bytes: refused, and nothing is stored.
    let tampered = String::from_utf8(manifest_bytes).unwrap().replacen(
        "Writes a dated invoice note",
        "Xrites a dated invoice note",
        1,
    );
    let tdir = temp_dir("published-tamper");
    let mut tpins = PinStore::new();
    let mut tstore = FsStore::new(tdir.join("store"));
    let err = install(
        tampered.as_bytes(),
        Some(pub_key.as_slice()),
        dsl_bytes,
        &mut tpins,
        Approval::Approved,
        &mut tstore,
    )
    .expect_err("a tampered copy of the real manifest is refused");
    assert!(matches!(err, RegistryError::SignatureInvalid { .. }));
    assert_no_install(&tdir, &tstore);

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&tdir);
}
