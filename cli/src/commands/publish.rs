//! `operant publish <draft-manifest.json> <dsl-file> --registry <dir> --key
//! <publisher-key.pem>`: sign a draft workflow manifest with the local
//! publisher key, write it (plus the workflow file and an updated
//! `index.json`) into a checkout of the registry index repo
//! (`../operant-registry`, R1A's layout), and commit it to a new branch
//! with a PR-ready title and body, per `docs/specs/registry.md`: "`operant
//! publish` signs with the local publisher key, emits a branch plus PR body
//! against the index repo."
//!
//! The draft manifest needs no `signature` and no `dsl.hash` (both are
//! filled in here); it does need `dsl.url`, the path the workflow file will
//! live at inside the registry repo (for example
//! `workflows/<name>/<version>.ts`).
//!
//! Shells out to `git` for the branch/commit (this crate graph has no git
//! library dependency, and every other repo-shaped operation in this
//! codebase already assumes `git` on PATH -- `scripts/check_emdash.mjs`
//! does the same). Every commit is scoped with `-c user.name=`/`-c
//! user.email=` on the command itself, never `git config`, so publishing
//! never touches global git configuration.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use operant_registry::{dsl_hash, fingerprint, sign_manifest, FetchedManifest, PinStore};

const PUBLISHER_COMMIT_NAME: &str = "Operant Publisher";
const PUBLISHER_COMMIT_EMAIL: &str = "publisher@operant.invalid";

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut positional = Vec::new();
    let mut registry_dir = None;
    let mut key_path = None;
    let mut branch = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--registry" => {
                i += 1;
                registry_dir = Some(args.get(i).context("--registry needs a value")?.clone());
            }
            "--key" => {
                i += 1;
                key_path = Some(args.get(i).context("--key needs a value")?.clone());
            }
            "--branch" => {
                i += 1;
                branch = Some(args.get(i).context("--branch needs a value")?.clone());
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let usage = "usage: operant publish <draft-manifest.json> <dsl-file> --registry <dir> --key <publisher-key.pem> [--branch <name>]";
    let draft_path = positional.first().context(usage)?;
    let dsl_path = positional.get(1).context(usage)?;
    let registry_dir = PathBuf::from(registry_dir.context(usage)?);
    let key_path = key_path.context(usage)?;

    let draft = fs::read_to_string(draft_path)
        .with_context(|| format!("reading {draft_path}"))?;
    let dsl_bytes = fs::read(dsl_path).with_context(|| format!("reading {dsl_path}"))?;
    let key_pem = fs::read_to_string(&key_path)
        .with_context(|| format!("reading {key_path}"))?;

    let outcome = publish(&draft, &dsl_bytes, &key_pem, &registry_dir, branch.as_deref())?;

    println!("Signed {} and committed to branch `{}`.", outcome.manifest_path.display(), outcome.branch);
    println!("Commit: {}", outcome.commit);
    println!();
    println!("--- PR title ---");
    println!("{}", outcome.pr_title);
    println!();
    println!("--- PR body ---");
    println!("{}", outcome.pr_body);
    Ok(())
}

/// A completed publish: everything a caller needs to open the PR by hand
/// (or hand to `gh pr create`) once the branch has been pushed.
pub struct PublishOutcome {
    pub branch: String,
    pub commit: String,
    pub manifest_path: PathBuf,
    pub pr_title: String,
    pub pr_body: String,
}

/// Sign `draft_json` with `key_pem`, write it plus the workflow file and an
/// updated index into `registry_dir`, and commit both to a new branch.
/// `registry_dir` must already be a git repository (a checkout of the index
/// repo) with a clean working tree; this function never touches any branch
/// but the one it creates.
pub fn publish(
    draft_json: &str,
    dsl_bytes: &[u8],
    key_pem: &str,
    registry_dir: &Path,
    branch: Option<&str>,
) -> Result<PublishOutcome> {
    let mut manifest: serde_json::Value =
        serde_json::from_str(draft_json).context("draft manifest is not valid JSON")?;
    let obj = manifest
        .as_object_mut()
        .context("draft manifest must be a JSON object")?;
    obj.entry("v").or_insert(serde_json::json!(1));
    obj.remove("signature");

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .context("draft manifest needs a `name`")?
        .to_string();
    let version = obj
        .get("version")
        .and_then(|v| v.as_str())
        .context("draft manifest needs a `version`")?
        .to_string();
    let publisher = obj
        .get("publisher")
        .and_then(|v| v.as_str())
        .context("draft manifest needs a `publisher`")?
        .to_string();
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let dsl_url = obj
        .get("dsl")
        .and_then(|d| d.get("url"))
        .and_then(|v| v.as_str())
        .with_context(|| {
            format!("draft manifest for `{name}` needs dsl.url (where it will live in the registry repo)")
        })?
        .to_string();
    let hash = dsl_hash(dsl_bytes);
    obj.insert(
        "dsl".to_string(),
        serde_json::json!({ "url": dsl_url, "hash": hash }),
    );

    let signing_key = operant_registry::parse_publisher_key_pem(key_pem)
        .context("publisher key is not a valid Ed25519 PKCS8 PEM key")?;
    let verifying_key = signing_key.verifying_key();
    obj.insert(
        "pubkey_fingerprint".to_string(),
        serde_json::json!(fingerprint(&verifying_key.to_bytes())),
    );

    let signature = sign_manifest(&manifest, &signing_key);
    manifest["signature"] = serde_json::json!({ "sig": signature.sig });

    // Self-check: the manifest this just produced must actually verify, the
    // same check any real installer will run. A publish that could not
    // survive its own install is a bug here, not a signature the world
    // should trust.
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let self_check = FetchedManifest::parse(&manifest_bytes)?
        .verify_signature(Some(&verifying_key.to_bytes()), &mut PinStore::new())
        .context("the manifest this just signed does not verify; refusing to publish it")?;
    let rendering = self_check
        .verify_dsl(dsl_bytes.to_vec())
        .context("dsl.hash does not match the workflow file's contents")?
        .render()
        .rendering;

    if !registry_dir.is_dir() {
        bail!("{} is not a directory", registry_dir.display());
    }

    let manifest_rel = PathBuf::from("manifests").join(&name).join(format!("{version}.json"));
    let manifest_abs = registry_dir.join(&manifest_rel);
    if let Some(parent) = manifest_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&manifest_abs, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("writing {}", manifest_abs.display()))?;

    let dsl_abs = registry_dir.join(&dsl_url);
    if let Some(parent) = dsl_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dsl_abs, dsl_bytes).with_context(|| format!("writing {}", dsl_abs.display()))?;

    update_index(registry_dir, &name, &version, &publisher, &description, &manifest_rel)?;

    let branch_name = branch
        .map(str::to_string)
        .unwrap_or_else(|| format!("publish/{name}-{version}"));
    let commit = commit_to_branch(
        registry_dir,
        &branch_name,
        &[&manifest_rel, &PathBuf::from(&dsl_url), &PathBuf::from("index.json")],
        &format!("publish: {name}@{version}"),
    )?;

    let pr_title = format!("Add {name} {version} to the registry");
    let pr_body = render_pr_body(&name, &version, &publisher, &description, &rendering);

    Ok(PublishOutcome {
        branch: branch_name,
        commit,
        manifest_path: manifest_rel,
        pr_title,
        pr_body,
    })
}

fn update_index(
    registry_dir: &Path,
    name: &str,
    version: &str,
    publisher: &str,
    description: &str,
    manifest_rel: &Path,
) -> Result<()> {
    let index_path = registry_dir.join("index.json");
    let mut index: serde_json::Value = if index_path.exists() {
        serde_json::from_slice(&fs::read(&index_path)?)
            .with_context(|| format!("parsing {}", index_path.display()))?
    } else {
        serde_json::json!({ "v": 1, "updated": "", "workflows": {} })
    };

    let manifest_rel_str = manifest_rel.to_string_lossy().replace('\\', "/");
    let entry = index["workflows"]
        .as_object_mut()
        .context("index.json's `workflows` must be an object")?
        .entry(name.to_string())
        .or_insert_with(|| serde_json::json!({ "latest": version, "versions": {} }));
    entry["latest"] = serde_json::json!(version);
    entry["versions"][version] = serde_json::json!({
        "manifest": manifest_rel_str,
        "publisher": publisher,
        "description": description,
    });

    fs::write(&index_path, serde_json::to_vec_pretty(&index)?)
        .with_context(|| format!("writing {}", index_path.display()))
}

fn commit_to_branch(registry_dir: &Path, branch: &str, paths: &[&Path], message: &str) -> Result<String> {
    let git = |args: &[&str]| -> Result<std::process::Output> {
        let out = Command::new("git")
            .arg("-C")
            .arg(registry_dir)
            .args(args)
            .output()
            .with_context(|| format!("running `git {}`", args.join(" ")))?;
        Ok(out)
    };

    // Create the branch if it does not exist yet, otherwise switch to it:
    // publishing a second version of the same workflow in one session reuses
    // its branch rather than failing on "branch already exists".
    let checkout_new = git(&["checkout", "-b", branch])?;
    if !checkout_new.status.success() {
        let checkout_existing = git(&["checkout", branch])?;
        if !checkout_existing.status.success() {
            bail!(
                "could not create or switch to branch `{branch}`: {}",
                String::from_utf8_lossy(&checkout_existing.stderr)
            );
        }
    }

    let mut add_args = vec!["add"];
    let path_strings: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    add_args.extend(path_strings.iter().map(String::as_str));
    let add = git(&add_args)?;
    if !add.status.success() {
        bail!("git add failed: {}", String::from_utf8_lossy(&add.stderr));
    }

    let commit = git(&[
        "-c",
        &format!("user.name={PUBLISHER_COMMIT_NAME}"),
        "-c",
        &format!("user.email={PUBLISHER_COMMIT_EMAIL}"),
        "commit",
        "-m",
        message,
    ])?;
    if !commit.status.success() {
        bail!(
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
    }

    let rev = git(&["rev-parse", "HEAD"])?;
    if !rev.status.success() {
        bail!("git rev-parse HEAD failed: {}", String::from_utf8_lossy(&rev.stderr));
    }
    Ok(String::from_utf8_lossy(&rev.stdout).trim().to_string())
}

fn render_pr_body(
    name: &str,
    version: &str,
    publisher: &str,
    description: &str,
    rendering: &operant_registry::GrantRendering,
) -> String {
    let mut body = String::new();
    body.push_str(&format!("Publishes `{name}` v{version} from publisher `{publisher}`.\n\n"));
    if !description.is_empty() {
        body.push_str(&description);
        body.push_str("\n\n");
    }
    if !rendering.step_summary.is_empty() {
        body.push_str("Steps:\n");
        for (i, step) in rendering.step_summary.iter().enumerate() {
            body.push_str(&format!("{}. {}\n", i + 1, step));
        }
        body.push('\n');
    }
    body.push_str("Permissions:\n");
    for grant in &rendering.grants {
        body.push_str(&format!("- {grant}\n"));
    }
    body
}

fn print_help() {
    println!("operant publish <draft-manifest.json> <dsl-file> --registry <dir> --key <publisher-key.pem> [--branch <name>]");
    println!();
    println!("Sign a draft workflow manifest with the given publisher key, write it and");
    println!("the workflow file into the registry checkout at <dir>, update its index, and");
    println!("commit both to a new branch with a PR-ready title and body.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_registry_repo(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .status()
                .expect("git available on PATH");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q"]);
        fs::write(
            dir.join("index.json"),
            serde_json::to_vec_pretty(&serde_json::json!({ "v": 1, "updated": "2026-07-11", "workflows": {} }))
                .unwrap(),
        )
        .unwrap();
        fs::create_dir_all(dir.join("keys")).unwrap();
        fs::write(
            dir.join("keys/operant-fixtures.pub"),
            include_str!("../../../contracts/fixtures/registry/publisher.pub"),
        )
        .unwrap();
        git(&["add", "-A"]);
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                "seed",
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("operant-cli-publish-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn draft_manifest() -> String {
        serde_json::json!({
            "v": 1,
            "name": "notepad-invoice-note",
            "version": "1.0.0",
            "publisher": "operant-fixtures",
            "description": "Writes a dated invoice note into Notepad and saves it.",
            "step_summary": [
                "Click the text editor",
                "Type the invoice note",
                "Wait for the screen to update",
                "Save the file",
                "Wait for the screen to update",
                "Check that the note was written"
            ],
            "inputs_schema": {
                "type": "object",
                "properties": {
                    "invoice_date": { "type": "string", "format": "date", "default": "2026-07-11", "title": "Invoice date" },
                    "amount": { "type": "string", "pattern": "^\\d+\\.\\d{2}$", "default": "142.50", "title": "Amount" }
                },
                "additionalProperties": false
            },
            "capabilities": { "apps": ["notepad.exe"], "paths": [], "network": false, "risk_ceiling": "write" },
            "min_operant_version": "1.0.0",
            "dsl": { "url": "workflows/notepad-invoice-note/1.0.0.ts" }
        })
        .to_string()
    }

    // BAR: publish signs a manifest that verifies against the publisher key,
    // and produces a branch/commit in the registry checkout with the new
    // manifest, workflow file, and index entry.
    #[test]
    fn publish_signs_and_commits_a_branch_against_the_fixture_index() {
        let dir = temp_dir("publish-ok");
        init_registry_repo(&dir);

        let dsl_bytes = include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts");
        let key_pem = include_str!("../../../contracts/fixtures/registry/publisher.key");

        let outcome = publish(&draft_manifest(), dsl_bytes, key_pem, &dir, None)
            .expect("publish succeeds against a clean fixture index");

        assert_eq!(outcome.branch, "publish/notepad-invoice-note-1.0.0");
        assert!(!outcome.commit.is_empty());
        assert!(outcome.pr_title.contains("notepad-invoice-note"));
        assert!(outcome.pr_body.contains("Click the text editor"));
        assert!(outcome.pr_body.contains("notepad.exe"));

        // The manifest actually on disk verifies against the publisher key,
        // the same check a real install would run.
        let manifest_abs = dir.join(&outcome.manifest_path);
        let manifest_bytes = fs::read(&manifest_abs).unwrap();
        let fetched = FetchedManifest::parse(&manifest_bytes).unwrap();
        assert_eq!(fetched.manifest.dsl.hash.len(), 64);
        let key = operant_registry::parse_publisher_key_hex(include_str!(
            "../../../contracts/fixtures/registry/publisher.pub"
        ))
        .unwrap();
        let mut pins = PinStore::new();
        fetched
            .verify_signature(Some(&key), &mut pins)
            .expect("the manifest publish wrote verifies against the publisher key");

        // index.json now lists the published version.
        let index: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("index.json")).unwrap()).unwrap();
        assert_eq!(
            index["workflows"]["notepad-invoice-note"]["latest"],
            serde_json::json!("1.0.0")
        );

        // The branch exists and HEAD there is the commit publish() returned.
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), outcome.commit);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_key_produces_a_manifest_that_would_not_be_trusted() {
        let dir = temp_dir("publish-wrong-key");
        init_registry_repo(&dir);
        let dsl_bytes = include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts");

        // A key whose fingerprint does not match a publisher this registry
        // would trust for `operant-fixtures`: publish still succeeds (this
        // publisher is signing their own new key), but the resulting
        // manifest does not verify against the *fixture* publisher key.
        let other_key_pem = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIA/hGApfRBqQgtTZJlaCTp+ujg92l83L4rWno9zoBb+G\n-----END PRIVATE KEY-----\n";
        let outcome = publish(&draft_manifest(), dsl_bytes, other_key_pem, &dir, None)
            .expect("publish succeeds with any valid key; trust is the installer's decision");

        let manifest_abs = dir.join(&outcome.manifest_path);
        let manifest_bytes = fs::read(&manifest_abs).unwrap();
        let fetched = FetchedManifest::parse(&manifest_bytes).unwrap();
        let fixture_key = operant_registry::parse_publisher_key_hex(include_str!(
            "../../../contracts/fixtures/registry/publisher.pub"
        ))
        .unwrap();
        let mut pins = PinStore::new();
        let err = fetched
            .verify_signature(Some(&fixture_key), &mut pins)
            .expect_err("a manifest signed by a different key must not verify against the fixture key");
        assert!(matches!(
            err,
            operant_registry::RegistryError::PublisherKeyMismatch { .. }
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}
