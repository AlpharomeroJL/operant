//! Scheduler and triggers (C11): cron, file-watch, window-appears, email-arrives. Serialized queue; unattended triggers launch compiled (replay) workflows only, enforced in code. Depends on replay, never on orchestrator. L10A owns it.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-scheduler";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-scheduler");
        let _ = "scheduler";
    }
}
