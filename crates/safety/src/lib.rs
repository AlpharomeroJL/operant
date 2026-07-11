//! Safety, permissions, and audit (C10).
//!
//! This crate owns the parts of the runtime that no workflow may weaken:
//!
//! - **Grants** ([`grants`]): capability scopes (app, directory subtree, network,
//!   risk ceiling) and [`check`], the execution-time grant check that returns a
//!   typed [`Refusal`] when an action exceeds its grants.
//! - **Hard invariants (FR-S4)** ([`invariants`]): a deny-list evaluated on every
//!   action -- credential-field targeting, payment/deletion dialogs, unexpected
//!   windows -- that freezes the run and requires human approval. No workflow can
//!   disable it, and a manifest that declares a safety gate [`fails to load`](load_manifest).
//! - **Dry-run** ([`dryrun`]): renders a plan against a snapshot with zero side
//!   effects.
//! - **Audit** ([`audit`]): a BLAKE3 hash-chained, JSONL, verifiable event log.

mod audit;
mod dryrun;
mod error;
mod grants;
mod invariants;
mod manifest_guard;

pub use audit::{AuditEvent, AuditLog, GENESIS};
pub use dryrun::{dry_run, fs_fingerprint, DryRunReport};
pub use error::{AuditError, SafetyError};
pub use grants::{check, path_within, CheckOutcome, Grants, ProposedAction, Refusal};
pub use invariants::{
    Disposition, Escalation, RunGuard, SafetyReason, SafetyVerdict,
};
pub use manifest_guard::{guard_manifest, load_manifest};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-safety";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-safety");
    }
}
