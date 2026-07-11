//! Capability grants and the execution-time grant check.
//!
//! A [`Grants`] set scopes what a run may touch: which applications, which
//! directory subtrees, whether the network is reachable, and a ceiling on
//! action risk. [`check`] is evaluated by the action layer at execution time
//! (not at plan time): any action that steps outside its grants gets a typed
//! [`Refusal`] the planner can react to in explore and that halts replay.

use std::path::{Component, Path, PathBuf};

use operant_ir::{Action, RiskClass};

/// The capability envelope a run executes inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    /// Allowed applications, by executable path or package id.
    pub apps: Vec<String>,
    /// Allowed directory subtrees. A path is in scope if it lies under one.
    pub subtrees: Vec<PathBuf>,
    /// Whether the run may use the network (boolean at v1).
    pub network: bool,
    /// The highest risk class an action may carry.
    pub risk_ceiling: RiskClass,
}

impl Grants {
    /// A minimal read-only grant: one app, one subtree, no network.
    pub fn app_scoped(app: impl Into<String>, subtree: impl Into<PathBuf>) -> Self {
        Grants {
            apps: vec![app.into()],
            subtrees: vec![subtree.into()],
            network: false,
            risk_ceiling: RiskClass::Read,
        }
    }
}

/// A concrete action the action layer is about to perform, reduced to the four
/// dimensions a grant check cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedAction {
    /// The application the action targets (executable path or package id).
    pub app: Option<String>,
    /// The filesystem path the action would touch, if any.
    pub path: Option<PathBuf>,
    /// Whether the action needs the network.
    pub network: bool,
    /// The action's declared risk class.
    pub risk: RiskClass,
}

impl Default for ProposedAction {
    fn default() -> Self {
        ProposedAction { app: None, path: None, network: false, risk: RiskClass::Read }
    }
}

impl ProposedAction {
    /// Derive the checkable surface from an [`Action`] IR node.
    ///
    /// App comes from the target window process; risk from the declared risk
    /// class; a `path` param (string) is treated as the touched path; a `url`
    /// param (or an explicit `network: true` param) marks a network need.
    pub fn from_action(action: &Action) -> Self {
        let app = action
            .target
            .as_ref()
            .and_then(|t| t.window.as_ref())
            .and_then(|w| w.process.clone());
        let path = action
            .params
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let network = action.params.get("url").is_some()
            || action.params.get("network").and_then(|v| v.as_bool()).unwrap_or(false);
        ProposedAction { app, path, network, risk: action.risk_class }
    }
}

/// Why an action was refused. One variant per grant dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The targeted app is not in the grant's app list.
    WrongApp {
        /// The app the action targeted.
        requested: String,
        /// The apps the grant allows.
        granted: Vec<String>,
    },
    /// The touched path is not under any granted subtree.
    OutOfSubtree {
        /// The path the action targeted.
        path: PathBuf,
        /// The subtrees the grant allows.
        subtrees: Vec<PathBuf>,
    },
    /// The action's risk class exceeds the grant ceiling.
    OverRiskCeiling {
        /// The action's risk class.
        requested: RiskClass,
        /// The grant's ceiling.
        ceiling: RiskClass,
    },
    /// The action needs the network but the grant withholds it.
    NetworkWithoutGrant,
}

/// The outcome of a grant check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The action is within grants and may proceed.
    Allowed,
    /// The action exceeds grants; carries the typed reason.
    Refused(Refusal),
}

impl CheckOutcome {
    /// True only for [`CheckOutcome::Allowed`].
    pub fn is_allowed(&self) -> bool {
        matches!(self, CheckOutcome::Allowed)
    }

    /// The refusal, if any.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            CheckOutcome::Refused(r) => Some(r),
            CheckOutcome::Allowed => None,
        }
    }
}

/// Check a proposed action against a grant set.
///
/// Returns [`CheckOutcome::Allowed`] only when the action satisfies *every*
/// dimension. The dimensions are checked in a fixed order (app, path, risk,
/// network) and the first violation is reported; the guarantee callers rely on
/// is the contrapositive: an action that exceeds any dimension is never allowed.
pub fn check(action: &ProposedAction, grants: &Grants) -> CheckOutcome {
    // App scope.
    if let Some(app) = &action.app {
        if !grants.apps.iter().any(|g| app_matches(g, app)) {
            return CheckOutcome::Refused(Refusal::WrongApp {
                requested: app.clone(),
                granted: grants.apps.clone(),
            });
        }
    }

    // Directory subtree scope.
    if let Some(path) = &action.path {
        if !grants.subtrees.iter().any(|root| path_within(root, path)) {
            return CheckOutcome::Refused(Refusal::OutOfSubtree {
                path: path.clone(),
                subtrees: grants.subtrees.clone(),
            });
        }
    }

    // Risk ceiling.
    if action.risk.exceeds(grants.risk_ceiling) {
        return CheckOutcome::Refused(Refusal::OverRiskCeiling {
            requested: action.risk,
            ceiling: grants.risk_ceiling,
        });
    }

    // Network.
    if action.network && !grants.network {
        return CheckOutcome::Refused(Refusal::NetworkWithoutGrant);
    }

    CheckOutcome::Allowed
}

/// App match: case-insensitive equality, or the granted token equals the file
/// name of the requested executable path (so `notepad.exe` matches
/// `C:\Windows\notepad.exe`).
fn app_matches(granted: &str, requested: &str) -> bool {
    if granted.eq_ignore_ascii_case(requested) {
        return true;
    }
    let req_file = Path::new(requested)
        .file_name()
        .map(|f| f.to_string_lossy().to_string());
    let grant_file = Path::new(granted)
        .file_name()
        .map(|f| f.to_string_lossy().to_string());
    match (req_file, grant_file) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// Lexical, filesystem-free subtree containment. Both paths are normalized
/// (`.` dropped, `..` popped, components lowercased for the Windows target) and
/// `root` must be a component-wise prefix of `candidate`. A `..` that escapes
/// the root therefore fails, with no disk access and no symlink surprises.
pub fn path_within(root: &Path, candidate: &Path) -> bool {
    let r = normalize(root);
    let c = normalize(candidate);
    if r.is_empty() {
        // An empty root would match everything; treat it as granting nothing.
        return false;
    }
    r.len() <= c.len() && r.iter().zip(c.iter()).all(|(a, b)| a == b)
}

fn normalize(p: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_string_lossy().to_lowercase()),
            Component::RootDir => out.push("/".to_string()),
            Component::Prefix(pre) => out.push(pre.as_os_str().to_string_lossy().to_lowercase()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_when_every_dimension_satisfied() {
        let grants = Grants {
            apps: vec!["notepad.exe".into()],
            subtrees: vec![PathBuf::from("C:/work")],
            network: false,
            risk_ceiling: RiskClass::Write,
        };
        let action = ProposedAction {
            app: Some("notepad.exe".into()),
            path: Some(PathBuf::from("C:/work/sub/out.txt")),
            network: false,
            risk: RiskClass::Write,
        };
        assert_eq!(check(&action, &grants), CheckOutcome::Allowed);
    }

    #[test]
    fn each_dimension_refuses_with_its_variant() {
        let grants = Grants {
            apps: vec!["notepad.exe".into()],
            subtrees: vec![PathBuf::from("C:/work")],
            network: false,
            risk_ceiling: RiskClass::Write,
        };

        let wrong_app = ProposedAction { app: Some("evil.exe".into()), ..Default::default() };
        assert!(matches!(
            check(&wrong_app, &grants).refusal(),
            Some(Refusal::WrongApp { .. })
        ));

        let out_of_tree = ProposedAction {
            app: Some("notepad.exe".into()),
            path: Some(PathBuf::from("C:/other/secret.txt")),
            ..Default::default()
        };
        assert!(matches!(
            check(&out_of_tree, &grants).refusal(),
            Some(Refusal::OutOfSubtree { .. })
        ));

        let too_risky = ProposedAction {
            app: Some("notepad.exe".into()),
            risk: RiskClass::Destructive,
            ..Default::default()
        };
        assert!(matches!(
            check(&too_risky, &grants).refusal(),
            Some(Refusal::OverRiskCeiling { .. })
        ));

        let needs_net = ProposedAction {
            app: Some("notepad.exe".into()),
            network: true,
            ..Default::default()
        };
        assert!(matches!(
            check(&needs_net, &grants).refusal(),
            Some(Refusal::NetworkWithoutGrant)
        ));
    }

    #[test]
    fn subtree_containment_blocks_dotdot_escape() {
        let root = Path::new("C:/work/sandbox");
        assert!(path_within(root, Path::new("C:/work/sandbox/a/b.txt")));
        assert!(!path_within(root, Path::new("C:/work/other/b.txt")));
        // `..` that climbs out of the sandbox must not be considered inside.
        assert!(!path_within(root, Path::new("C:/work/sandbox/../secret.txt")));
    }

    #[test]
    fn app_matches_by_basename() {
        assert!(app_matches("notepad.exe", "C:/Windows/System32/notepad.exe"));
        assert!(app_matches("notepad.exe", "NOTEPAD.EXE"));
        assert!(!app_matches("notepad.exe", "calc.exe"));
    }
}
