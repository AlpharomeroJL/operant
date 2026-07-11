//! [`Finding`], the [`Check`] trait, and [`run_doctor`] (C19 doctor).
//!
//! A [`Finding`] is the same plain-language triple as an
//! [`crate::catalog::ErrorKind`] catalog entry (what, why, action), plus a
//! [`Severity`] and the same optional one-click fix, matching the
//! `doctor.finding` bus payload in `contracts/bus_events.md` exactly:
//! `finding_id, severity (info/warn/error), what, why, action, fix_command?`.
//!
//! A [`Check`] is deliberately just "gather a signal, reduce it to one
//! `Finding`." Every concrete check in [`crate::checks`] takes its signal
//! from an injected probe (a closure, or [`crate::checks::probes`]'s
//! best-effort default), so a test seeds a broken state without touching
//! real hardware or a network.

use serde::{Deserialize, Serialize};

use crate::catalog::CatalogEntry;

/// Severity of a doctor finding. Mirrors `doctor.finding`'s
/// `severity (info/warn/error)` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// One diagnostic result. `finding_id` is stable per check (the check's own
/// [`Check::id`]), so a UI can key a card on it and a later
/// `doctor.fixed{finding_id}` event (a future lane's concern; see
/// FOLLOWUPS) can reference the same id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub finding_id: String,
    pub severity: Severity,
    pub what: String,
    pub why: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_command: Option<String>,
}

impl Finding {
    /// A healthy result: `Severity::Info`, no fix needed.
    pub fn healthy(
        finding_id: impl Into<String>,
        what: impl Into<String>,
        why: impl Into<String>,
    ) -> Self {
        Finding {
            finding_id: finding_id.into(),
            severity: Severity::Info,
            what: what.into(),
            why: why.into(),
            action: "No action needed.".to_string(),
            fix_command: None,
        }
    }

    /// A problem result built directly from an [`crate::catalog::ErrorKind`]
    /// catalog entry, so a doctor finding and the matching runtime error
    /// kind always say the same thing.
    pub fn from_catalog(
        finding_id: impl Into<String>,
        severity: Severity,
        entry: &CatalogEntry,
    ) -> Self {
        Finding {
            finding_id: finding_id.into(),
            severity,
            what: entry.what.to_string(),
            why: entry.why.to_string(),
            action: entry.action.to_string(),
            fix_command: entry.fix_command.map(str::to_string),
        }
    }

    /// The check itself could not run to completion (its probe returned an
    /// error rather than a reading). Distinct from a confirmed problem: this
    /// is Operant being unsure, not Operant having found something wrong.
    pub fn could_not_check(finding_id: impl Into<String>) -> Self {
        Finding {
            finding_id: finding_id.into(),
            severity: Severity::Warn,
            what: "Operant could not finish this check.".to_string(),
            why: "Something on this computer stopped the check from completing.".to_string(),
            action: "Run the check again in a moment.".to_string(),
            fix_command: None,
        }
    }
}

/// A single doctor check: gather a signal (via an injected probe) and
/// reduce it to one [`Finding`]. Implementations live in [`crate::checks`].
pub trait Check: Send + Sync {
    /// Stable identifier, e.g. `"model_reachable"`. Becomes the returned
    /// finding's `finding_id`.
    fn id(&self) -> &'static str;

    /// Run the check now and return its one finding.
    fn run(&self) -> Finding;
}

/// Run every check and collect the findings, in order. This is C19's
/// `run_doctor()`: the full "Check my setup" / `operant doctor` result set,
/// healthy checks included, not just problems.
pub fn run_doctor(checks: &[Box<dyn Check>]) -> Vec<Finding> {
    checks.iter().map(|check| check.run()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ErrorKind;

    struct AlwaysHealthy;
    impl Check for AlwaysHealthy {
        fn id(&self) -> &'static str {
            "always_healthy"
        }
        fn run(&self) -> Finding {
            Finding::healthy(self.id(), "All good.", "Nothing to report.")
        }
    }

    struct AlwaysBroken;
    impl Check for AlwaysBroken {
        fn id(&self) -> &'static str {
            "always_broken"
        }
        fn run(&self) -> Finding {
            Finding::from_catalog(
                self.id(),
                Severity::Error,
                &ErrorKind::ModelUnreachable.entry(),
            )
        }
    }

    #[test]
    fn run_doctor_collects_one_finding_per_check_in_order() {
        let checks: Vec<Box<dyn Check>> = vec![Box::new(AlwaysHealthy), Box::new(AlwaysBroken)];
        let findings = run_doctor(&checks);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].finding_id, "always_healthy");
        assert_eq!(findings[0].severity, Severity::Info);
        assert_eq!(findings[1].finding_id, "always_broken");
        assert_eq!(findings[1].severity, Severity::Error);
    }

    #[test]
    fn run_doctor_on_an_empty_check_list_is_an_empty_result() {
        let checks: Vec<Box<dyn Check>> = vec![];
        assert!(run_doctor(&checks).is_empty());
    }

    #[test]
    fn from_catalog_copies_the_entry_verbatim() {
        let entry = ErrorKind::DiskSpaceLow.entry();
        let finding = Finding::from_catalog("disk_free", Severity::Error, &entry);
        assert_eq!(finding.what, entry.what);
        assert_eq!(finding.why, entry.why);
        assert_eq!(finding.action, entry.action);
        assert_eq!(finding.fix_command.as_deref(), entry.fix_command);
    }

    #[test]
    fn finding_serializes_without_a_fix_command_field_when_none() {
        let finding = Finding::healthy("x", "ok", "checked");
        let v = serde_json::to_value(&finding).unwrap();
        assert!(v.get("fix_command").is_none());
    }

    #[test]
    fn finding_roundtrips_through_json() {
        let finding = Finding::from_catalog(
            "vram_headroom",
            Severity::Warn,
            &ErrorKind::GraphicsMemoryLow.entry(),
        );
        let v = serde_json::to_value(&finding).unwrap();
        let back: Finding = serde_json::from_value(v).unwrap();
        assert_eq!(back, finding);
    }
}
