//! Self-diagnostics and the runtime error catalog (C19 doctor, FR-U3):
//! model reachability, disk, updater, permissions, audio. Surfaced as
//! `operant doctor` and the "Check my setup" button. U3A owns it.
//!
//! Two related surfaces live here:
//! - [`catalog`]: [`ErrorKind`], the exhaustive, typed enum of every
//!   user-facing runtime error kind, and [`ErrorKind::entry`], its
//!   plain-language catalog entry (what happened, why, and one suggested
//!   action). This is what `run.step.failed`'s `error_id` keys into per
//!   `contracts/bus_events.md`.
//! - [`checks`] / [`finding`]: [`Finding`], the same plain-language triple
//!   (plus a [`Severity`] and an optional fix) a proactive [`Check`]
//!   returns. [`run_doctor`] runs a set of checks; [`cli::run_doctor_verb`]
//!   is the `operant doctor` / "Check my setup" entry point, additionally
//!   publishing each finding as `doctor.finding` on the bus
//!   ([`bus`]).
//!
//! Every check is built from an injected probe, so a test seeds a broken
//! state (a low disk reading, an unreachable model) without touching real
//! hardware or a network; [`checks::probes`] holds the best-effort default
//! probes a production caller wires in.
//!
//! ```
//! use operant_doctor::{Check, DiskFreeCheck, Severity, run_doctor};
//!
//! // Seed a broken state: 1 GB free against a 10 GB threshold.
//! let checks: Vec<Box<dyn Check>> = vec![
//!     Box::new(DiskFreeCheck::new(10_000_000_000, || Ok(1_000_000_000))),
//! ];
//! let findings = run_doctor(&checks);
//! assert_eq!(findings.len(), 1);
//! assert_eq!(findings[0].severity, Severity::Error);
//! assert_eq!(findings[0].action, "Free up some disk space, then try again.");
//! ```

pub mod bus;
pub mod bundle;
pub mod catalog;
pub mod checks;
pub mod cli;
pub mod finding;

pub use bundle::{build_bundle, BundleError, BundleInputs};
pub use catalog::{CatalogEntry, ErrorKind};
pub use checks::{
    probes, AccessibilityPermissionCheck, AudioDevicesPresentCheck, DiskFreeCheck,
    ModelReachableCheck, UpdaterReachableCheck, VramHeadroomCheck, VramReading,
};
pub use cli::{run_doctor_verb, DoctorReport};
pub use finding::{run_doctor, Check, Finding, Severity};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-doctor";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-doctor");
        let _ = "doctor";
    }
}
