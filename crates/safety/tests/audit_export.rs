//! Audit export contract tests (L6B).
//!
//! Two concerns:
//!
//! 1. **PDF export** -- a fixture chain renders to a byte buffer that is a valid
//!    PDF (`%PDF` header, `xref` table, `trailer`/`startxref`, `%%EOF`), prints
//!    the chain head hash on every page, and reproduces the event text.
//!
//! 2. **Air-gap** -- the default safety/audit + export path performs no network
//!    I/O. Rust cannot cheaply hook sockets from a unit test, so this is asserted
//!    on two independent axes:
//!
//!    * *Structural / compile-time*: the resolved dependency closure of
//!      `operant-safety` (walked from the workspace `Cargo.lock`) contains no
//!      HTTP/socket/TLS crate. The same lock DOES resolve `tokio`, `reqwest`,
//!      `hyper`, `mio`, and `socket2` for other workspace crates, so the check
//!      is not vacuous -- those crates exist but are unreachable from safety.
//!    * *Runtime guard*: the export's only output channel is the
//!      caller-supplied sink. `write_pdf` emits exactly the bytes `export_pdf`
//!      returns, to an in-memory buffer or a caller-opened file, and the render
//!      is a deterministic pure function of the in-memory log.
//!
//! ## What this proves, and what it does not
//!
//! Proven: no HTTP/socket/TLS crate is linked into `operant-safety`'s
//! compile-time graph, and the PDF export writes only to the sink the caller
//! provides (nothing is returned or emitted anywhere else). Because there is no
//! transport dependency to construct a connection with, the export cannot open
//! a socket through the normal crate ecosystem.
//!
//! NOT proven: this is not a kernel-level syscall sandbox. It does not intercept
//! `connect(2)`/`send(2)`, so it cannot prove at the OS level that zero packets
//! left the machine, nor exclude a dependency reaching for a raw socket via
//! `libc`/`std::net` directly. No crate in the closure does so, and asserting
//! the absence of the networking crate layer is the strongest guarantee
//! obtainable without an OS sandbox or seccomp/ETW-style syscall filter.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use operant_safety::AuditLog;
use serde_json::json;

// ---------------------------------------------------------------------------
// Fixtures and byte helpers
// ---------------------------------------------------------------------------

fn fixture_chain() -> AuditLog {
    let mut log = AuditLog::new();
    log.append(
        "2026-07-11T10:00:00Z",
        json!({ "event": "workflow.install", "name": "notepad-invoice-note" }),
    );
    log.append(
        "2026-07-11T10:00:01Z",
        json!({ "event": "action.executed", "id": "s1", "outcome": "ok" }),
    );
    log.append(
        "2026-07-11T10:00:02Z",
        json!({ "event": "gate.result", "gate": 2, "result": "pass" }),
    );
    log.append(
        "2026-07-11T10:00:03Z",
        json!({ "event": "approval", "reason": "credential_field", "proposed": { "kind": "type" } }),
    );
    log.append(
        "2026-07-11T10:00:04Z",
        json!({ "event": "invoice.paid", "amount": "142.50 (USD)" }),
    );
    log
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle).is_some()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut n = 0;
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            n += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    n
}

// ---------------------------------------------------------------------------
// 1. PDF export
// ---------------------------------------------------------------------------

#[test]
fn export_pdf_is_valid_and_carries_head_and_event_text() {
    let log = fixture_chain();
    let head = log.head();
    let pdf = log.export_pdf();

    // A valid minimal PDF shell.
    assert!(pdf.starts_with(b"%PDF-1.4"), "missing %PDF header");
    assert!(contains(&pdf, b"xref"), "missing xref table");
    assert!(contains(&pdf, b"trailer"), "missing trailer");
    assert!(contains(&pdf, b"startxref"), "missing startxref");
    assert!(pdf.ends_with(b"%%EOF\n"), "missing %%EOF");

    // One page per event.
    let pages = count(&pdf, b"/Type /Page ");
    assert_eq!(pages, log.events().len(), "expected one page per audit event");

    // The chain head hash must appear on every page (headers) at least.
    assert!(contains(&pdf, head.as_bytes()), "chain head hash absent from PDF");
    assert!(
        count(&pdf, head.as_bytes()) >= pages,
        "chain head hash must be printed on every page"
    );

    // Event text is reproduced.
    assert!(contains(&pdf, b"workflow.install"), "event text absent");
    assert!(contains(&pdf, b"gate.result"), "event text absent");
    assert!(contains(&pdf, b"invoice.paid"), "event text absent");

    // Parens inside a payload value are PDF-escaped, so the raw "(USD)" literal
    // never appears unescaped in a content stream.
    assert!(contains(&pdf, b"142.50 \\(USD\\)"), "payload parens must be escaped");
}

#[test]
fn export_pdf_xref_offsets_point_at_real_objects() {
    // Parse the trailer's startxref, seek to that offset, and confirm the byte
    // there begins the xref table. This exercises the offset bookkeeping that
    // makes the file openable, not merely the presence of the keywords.
    let pdf = fixture_chain().export_pdf();
    let start = find(&pdf, b"startxref\n").expect("startxref present") + b"startxref\n".len();
    let end = start + pdf[start..].iter().position(|&b| b == b'\n').expect("newline after offset");
    let offset: usize = std::str::from_utf8(&pdf[start..end])
        .unwrap()
        .trim()
        .parse()
        .expect("startxref offset parses");
    assert!(offset < pdf.len(), "startxref offset out of range");
    assert_eq!(&pdf[offset..offset + 4], b"xref", "startxref must point at the xref table");
}

// ---------------------------------------------------------------------------
// 2a. Air-gap: structural / compile-time dependency closure
// ---------------------------------------------------------------------------

/// Crates that would indicate an HTTP client/server, a socket/async-net
/// reactor, a websocket/QUIC transport, or a TLS stack. Matched by exact crate
/// name against the resolved dependency graph.
const NETWORK_CRATES: &[&str] = &[
    "tokio",
    "mio",
    "socket2",
    "hyper",
    "hyper-util",
    "reqwest",
    "ureq",
    "isahc",
    "surf",
    "curl",
    "attohttpc",
    "h2",
    "tonic",
    "async-std",
    "smol",
    "tungstenite",
    "tokio-tungstenite",
    "quinn",
    "trust-dns-resolver",
    "trust-dns-proto",
    "hickory-resolver",
    "hickory-proto",
    "native-tls",
    "openssl",
    "openssl-sys",
    "rustls",
    "boring",
    "websocket",
];

#[test]
fn airgap_safety_closure_pulls_in_no_networking_crate() {
    let (lock_path, lock) = workspace_lock();
    let graph = parse_lock_dependencies(&lock);
    assert!(
        graph.contains_key("operant-safety"),
        "Cargo.lock at {lock_path:?} has no operant-safety entry"
    );

    let reachable = transitive_closure(&graph, "operant-safety");
    // Guard against a mis-parse silently emptying the closure.
    assert!(
        reachable.contains("blake3") && reachable.contains("serde_json") && reachable.contains("operant-core"),
        "closure looks mis-parsed (missing known deps): {reachable:?}"
    );

    let leaked: Vec<&str> = NETWORK_CRATES
        .iter()
        .copied()
        .filter(|c| reachable.contains(*c))
        .collect();
    assert!(
        leaked.is_empty(),
        "operant-safety transitively links networking crate(s): {leaked:?}"
    );
}

#[test]
fn airgap_structural_check_is_not_vacuous() {
    // The denylist is only meaningful if those crate names really are resolved
    // somewhere in the workspace. They are (other crates -- e.g. the scheduler --
    // depend on tokio/reqwest), yet none is reachable from operant-safety.
    let (_lock_path, lock) = workspace_lock();
    let graph = parse_lock_dependencies(&lock);

    let present_elsewhere = ["tokio", "reqwest", "socket2", "mio", "hyper"];
    for c in present_elsewhere {
        assert!(
            graph.contains_key(c),
            "expected networking crate {c} to be resolved in the workspace lock; \
             without it the air-gap assertion would be vacuous"
        );
    }

    let reachable = transitive_closure(&graph, "operant-safety");
    for c in present_elsewhere {
        assert!(
            !reachable.contains(c),
            "networking crate {c} is present in the workspace but must not be reachable from operant-safety"
        );
    }
}

// ---------------------------------------------------------------------------
// 2b. Air-gap: runtime guard -- output goes only to the caller's sink
// ---------------------------------------------------------------------------

#[test]
fn write_pdf_emits_only_to_the_caller_supplied_sink() {
    let log = fixture_chain();
    let expected = log.export_pdf();

    // Sink 1: an in-memory buffer receives exactly the exported bytes.
    let mut buf: Vec<u8> = Vec::new();
    log.write_pdf(&mut buf).expect("write_pdf to Vec");
    assert_eq!(buf, expected, "write_pdf must emit exactly export_pdf()'s bytes");

    // The export is a deterministic pure function of the in-memory log: a second
    // render is byte-identical, so nothing is drawn from a clock, socket, or RNG.
    assert_eq!(log.export_pdf(), expected, "export_pdf must be deterministic");

    // Sink 2: a caller-opened file receives exactly those bytes and nothing else.
    let dir = unique_tmp("write");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("audit.pdf");
    {
        let mut f = fs::File::create(&path).expect("create pdf file");
        log.write_pdf(&mut f).expect("write_pdf to file");
    }
    let on_disk = fs::read(&path).expect("read pdf back");
    assert_eq!(on_disk, expected, "the file sink must hold exactly the exported bytes");

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Cargo.lock helpers (no TOML crate; the format is line-regular)
// ---------------------------------------------------------------------------

/// Locate and read the workspace `Cargo.lock` by walking up from this crate.
fn workspace_lock() -> (PathBuf, String) {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("Cargo.lock");
        if candidate.is_file() {
            let text = fs::read_to_string(&candidate).expect("read Cargo.lock");
            return (candidate, text);
        }
        if !dir.pop() {
            panic!("Cargo.lock not found at or above CARGO_MANIFEST_DIR");
        }
    }
}

/// Parse `Cargo.lock` into `crate name -> direct dependency names`.
///
/// Each `[[package]]` block has a `name = "..."` and an optional
/// `dependencies = [ "dep", "dep 1.2.3", ... ]` list. Version/source suffixes
/// on a dependency are dropped; only the crate name is kept.
fn parse_lock_dependencies(lock: &str) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut name: Option<String> = None;
    let mut deps: Vec<String> = Vec::new();
    let mut in_deps = false;

    for line in lock.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            if let Some(n) = name.take() {
                graph.insert(n, std::mem::take(&mut deps));
            }
            deps.clear();
            in_deps = false;
        } else if in_deps {
            if t.starts_with(']') {
                in_deps = false;
            } else if let Some(after) = t.strip_prefix('"') {
                if let Some(endq) = after.find('"') {
                    if let Some(crate_name) = after[..endq].split_whitespace().next() {
                        if !crate_name.is_empty() {
                            deps.push(crate_name.to_string());
                        }
                    }
                }
            }
        } else if let Some(rest) = t.strip_prefix("name = \"") {
            if let Some(endq) = rest.find('"') {
                name = Some(rest[..endq].to_string());
            }
        } else if t == "dependencies = [" {
            in_deps = true;
            deps.clear();
        }
    }
    if let Some(n) = name.take() {
        graph.insert(n, deps);
    }
    graph
}

/// All crate names reachable from `root` (inclusive) in the dependency graph.
fn transitive_closure(graph: &HashMap<String, Vec<String>>, root: &str) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(node) = stack.pop() {
        if !seen.insert(node.clone()) {
            continue;
        }
        if let Some(children) = graph.get(&node) {
            for child in children {
                if !seen.contains(child) {
                    stack.push(child.clone());
                }
            }
        }
    }
    seen
}

fn unique_tmp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut p = std::env::temp_dir();
    p.push(format!("operant-audit-pdf-{tag}-{}-{nanos}", std::process::id()));
    p
}
