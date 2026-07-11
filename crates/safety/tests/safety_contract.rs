//! Contract tests for the safety crate (C10):
//! - property-style grant soundness (no over-grant action is ever Allowed);
//! - the credential-form fixture triggers the FR-S4 approval requirement;
//! - a dry-run leaves a temp directory byte-identical (zero filesystem diff).

use std::path::PathBuf;

use operant_safety::{
    check, dry_run, fs_fingerprint, CheckOutcome, Disposition, Grants, ProposedAction, RunGuard,
    SafetyReason,
};
use operant_ir::{Element, RiskClass, Snapshot};
use serde_json::json;

// ---- a tiny dependency-free deterministic RNG ------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn boolean(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn risk(&mut self) -> RiskClass {
        match self.below(3) {
            0 => RiskClass::Read,
            1 => RiskClass::Write,
            _ => RiskClass::Destructive,
        }
    }
}

// ---- self-cleaning temp dir (no external crate) ----------------------------

struct TmpDir(PathBuf);
impl TmpDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("operant-safety-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        TmpDir(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn property_no_action_that_exceeds_grants_is_ever_allowed() {
    let root = PathBuf::from("C:/work/sandbox");
    let mut rng = Rng(0x1234_5678_9abc_def1);

    let mut allowed = 0usize;
    let mut refused = 0usize;

    for _ in 0..20_000 {
        // Randomize the grant set.
        let grants = Grants {
            apps: vec!["notepad.exe".into(), "calc.exe".into()],
            subtrees: vec![root.clone()],
            network: rng.boolean(),
            risk_ceiling: rng.risk(),
        };

        // Build each action dimension as either compliant or violating, and track
        // the ground truth independently of `check`'s own logic.
        let app_bad = rng.boolean();
        let app = if app_bad { "evil.exe" } else if rng.boolean() { "notepad.exe" } else { "calc.exe" };

        // Path is present ~half the time; when present, compliant or violating.
        let (path, path_bad) = if rng.boolean() {
            if rng.boolean() {
                (Some(root.join("a/b/out.txt")), false)
            } else {
                (Some(PathBuf::from("C:/elsewhere/secret.txt")), true)
            }
        } else {
            (None, false)
        };

        let action_risk = rng.risk();
        let risk_bad = action_risk.exceeds(grants.risk_ceiling);

        let network = rng.boolean();
        let net_bad = network && !grants.network;

        let action = ProposedAction {
            app: Some(app.to_string()),
            path,
            network,
            risk: action_risk,
        };

        let should_refuse = app_bad || path_bad || risk_bad || net_bad;
        let outcome = check(&action, &grants);

        // THE property: anything that exceeds grants is never Allowed.
        if should_refuse {
            assert!(
                !outcome.is_allowed(),
                "over-grant action was ALLOWED: {action:?} vs {grants:?}"
            );
            refused += 1;
        } else {
            // Soundness the other way: a fully compliant action is Allowed.
            assert_eq!(
                outcome,
                CheckOutcome::Allowed,
                "compliant action was refused: {action:?} vs {grants:?}"
            );
            allowed += 1;
        }
    }

    // The generator must actually exercise both branches.
    assert!(allowed > 0 && refused > 0, "degenerate run: {allowed} allowed / {refused} refused");
}

// ---- FR-S4 credential-form fixture -----------------------------------------

const CREDENTIAL_FORM: &str =
    include_str!("../../../contracts/fixtures/credential_form/index.html");

/// Pull the `aria-label` of the `type="password"` input straight from the
/// fixture, so this test stays bound to the actual fixture file.
fn password_field_label(html: &str) -> Option<String> {
    for line in html.lines() {
        let l = line.trim();
        if l.starts_with("<input") && l.contains(r#"type="password""#) {
            let marker = r#"aria-label=""#;
            if let Some(start) = l.find(marker) {
                let rest = &l[start + marker.len()..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

fn browser_snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
    serde_json::from_value(json!({
        "v": 1,
        "source": "browser",
        "window": { "process": "msedge.exe", "title": title },
        "digest": "d0",
        "elements": elements.iter().map(|e| serde_json::to_value(e).unwrap()).collect::<Vec<_>>(),
    }))
    .unwrap()
}

fn edit_element(idx: u32, name: &str, is_password: bool) -> Element {
    serde_json::from_value(json!({
        "idx": idx, "parent": 0, "role": "edit", "name": name, "is_password": is_password
    }))
    .unwrap()
}

#[test]
fn credential_form_fixture_triggers_fr_s4_approval() {
    // The fixture's own prose states the password field MUST be flagged and MUST
    // trigger FR-S4. Verify the field is really there, then feed the runtime the
    // snapshot a compliant perceiver would produce for it.
    assert!(
        CREDENTIAL_FORM.contains(r#"type="password""#),
        "credential fixture must define a password input"
    );
    let label = password_field_label(CREDENTIAL_FORM).expect("password field has an aria-label");
    assert_eq!(label, "Password");

    let username = edit_element(1, "Username", false);
    // A compliant perceiver flags type=password inputs as is_password.
    let password = edit_element(2, &label, true);
    let snapshot = browser_snapshot("Operant Fixture Sign In", vec![username.clone(), password.clone()]);

    let guard = RunGuard::new(["msedge.exe"]);
    let proposed = json!({ "id": "type-password", "kind": "type", "params": { "text": "hunter2" } });

    // Typing into the password field must require explicit human approval.
    let verdict = guard.evaluate(Some(&password), &snapshot, &proposed);
    let escalation = verdict.escalation().expect("password field must be blocked");
    assert_eq!(escalation.reason, SafetyReason::CredentialField);
    assert_eq!(escalation.disposition, Disposition::RequireApproval);
    assert!(escalation.sentence.to_lowercase().contains("password"));
    // The proposed action rides along for the audit chain.
    assert_eq!(escalation.proposed, proposed);

    // Typing into the ordinary username field is clear.
    assert!(!guard.evaluate(Some(&username), &snapshot, &proposed).is_blocked());
}

// ---- dry-run zero side effects ---------------------------------------------

#[test]
fn dry_run_leaves_temp_dir_byte_identical() {
    let tmp = TmpDir::new("dryrun");
    std::fs::write(tmp.path().join("seed1.txt"), b"alpha").unwrap();
    std::fs::create_dir_all(tmp.path().join("nested")).unwrap();
    std::fs::write(tmp.path().join("nested/seed2.txt"), b"bravo bravo").unwrap();

    let before = fs_fingerprint(tmp.path());
    assert_eq!(before.len(), 2, "seed fingerprint should list two files");

    let snapshot: Snapshot = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/snapshot_notepad.json"
    ))
    .unwrap();

    let out_path = tmp.path().join("would_be_written.txt");
    let plan: Vec<operant_ir::Action> = vec![
        serde_json::from_value(json!({
            "v": 1, "id": "s1", "kind": "type", "intent": "Type the note",
            "target": { "selectors": [{ "kind": "automation_id", "value": "RichEditD2DPT" }] },
            "params": { "text": "Invoice 2026-07-11 total $142.50" },
            "risk_class": "write", "grounding": "uia"
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "v": 1, "id": "s2", "kind": "adapter_call", "intent": "Save the file",
            "params": { "path": out_path.to_string_lossy() },
            "risk_class": "write", "grounding": "adapter"
        }))
        .unwrap(),
    ];

    let report = dry_run(&plan, &snapshot);

    // It rendered every step and noted the path it *would* write...
    assert_eq!(report.lines.len(), 2);
    assert!(report.would_touch.iter().any(|p| p.ends_with("would_be_written.txt")));
    // ...but did not actually create it.
    assert!(!out_path.exists(), "dry-run must not create files");

    let after = fs_fingerprint(tmp.path());
    assert_eq!(before, after, "dry-run changed the filesystem");
}
