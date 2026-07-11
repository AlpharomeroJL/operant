//! The `operant doctor` / "Check my setup" entry point (C19, FR-U3): run
//! every check, publish each result as `doctor.finding`, and render a
//! CLI-verb-shaped report.
//!
//! `crates/doctor` owns this function body; wiring it to the `doctor`
//! subcommand's argument parsing in `cli/src/main.rs` is the CLI lane's
//! job (this lane's owned path is `crates/doctor` only), so
//! [`run_doctor_verb`] takes a plain check list and an optional bus and
//! returns a plain report, no argv parsing or process exit inside it.

use operant_core::Bus;

use crate::finding::{run_doctor, Check, Finding, Severity};

/// Everything one `operant doctor` invocation produces.
pub struct DoctorReport {
    pub findings: Vec<Finding>,
    /// Human-readable, newline-terminated report suitable for printing
    /// straight to the terminal.
    pub text: String,
    /// `0` when nothing is at `Severity::Error`, `1` otherwise. A Unix-style
    /// exit code the CLI lane can hand to `std::process::exit`.
    pub exit_code: i32,
}

/// Run every check in `checks`, publish each result on `bus` (when given)
/// as a `doctor.finding` event, and render the report. This is the whole
/// `operant doctor` verb and the whole "Check my setup" button: both surface
/// this same function, they just differ in how they display `text` /
/// `findings` and whether they pass a live bus.
pub fn run_doctor_verb(checks: &[Box<dyn Check>], bus: Option<&Bus>) -> DoctorReport {
    let findings = run_doctor(checks);
    if let Some(bus) = bus {
        for finding in &findings {
            bus.publish_event(finding)
                .expect("Finding always serializes");
        }
    }
    let exit_code = if findings.iter().any(|f| f.severity == Severity::Error) {
        1
    } else {
        0
    };
    let text = render_text(&findings);
    DoctorReport {
        findings,
        text,
        exit_code,
    }
}

fn render_text(findings: &[Finding]) -> String {
    let mut out = String::new();
    for finding in findings {
        out.push_str(&format!(
            "[{}] {}\n",
            severity_label(finding.severity),
            finding.what
        ));
        out.push_str(&format!("    {}\n", finding.why));
        out.push_str(&format!("    -> {}\n", finding.action));
        if let Some(fix_command) = &finding.fix_command {
            out.push_str(&format!("    fix: {fix_command}\n"));
        }
    }
    out
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "OK",
        Severity::Warn => "WARN",
        Severity::Error => "ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::DiskFreeCheck;

    const GB: u64 = 1024 * 1024 * 1024;

    fn healthy_disk() -> Box<dyn Check> {
        Box::new(DiskFreeCheck::new(1, || Ok(50 * GB)))
    }

    fn broken_disk() -> Box<dyn Check> {
        Box::new(DiskFreeCheck::new(50 * GB, || Ok(1)))
    }

    #[test]
    fn exit_code_is_zero_when_every_finding_is_healthy() {
        let checks: Vec<Box<dyn Check>> = vec![healthy_disk()];
        let report = run_doctor_verb(&checks, None);
        assert_eq!(report.exit_code, 0);
        assert!(report.text.contains("[OK]"));
        assert!(report.text.contains("There is enough free disk space."));
    }

    #[test]
    fn exit_code_is_nonzero_when_a_finding_is_an_error() {
        let checks: Vec<Box<dyn Check>> = vec![broken_disk()];
        let report = run_doctor_verb(&checks, None);
        assert_eq!(report.exit_code, 1);
        assert!(report.text.contains("[ERROR]"));
        assert!(report.text.contains("fix: operant doctor --fix disk_free"));
    }

    #[test]
    fn a_warn_finding_alone_does_not_flip_the_exit_code() {
        use crate::checks::UpdaterReachableCheck;
        let checks: Vec<Box<dyn Check>> = vec![Box::new(UpdaterReachableCheck::new(|| {
            Err("offline".to_string())
        }))];
        let report = run_doctor_verb(&checks, None);
        assert_eq!(report.exit_code, 0, "Warn alone must not fail the verb");
        assert!(report.text.contains("[WARN]"));
    }

    #[test]
    fn publishes_one_doctor_finding_event_per_check() {
        let bus = Bus::new();
        let sub = bus.subscribe("doctor.finding");
        let checks: Vec<Box<dyn Check>> = vec![healthy_disk(), broken_disk()];

        run_doctor_verb(&checks, Some(&bus));

        let events: Vec<_> = sub.rx.try_iter().collect();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.topic == "doctor.finding"));
    }

    #[test]
    fn no_bus_means_no_publish_and_no_panic() {
        let checks: Vec<Box<dyn Check>> = vec![healthy_disk()];
        let report = run_doctor_verb(&checks, None);
        assert_eq!(report.findings.len(), 1);
    }
}
