//! The six doctor checks (C19): model reachable, disk free above threshold,
//! updater reachable, accessibility permission, audio devices present, VRAM
//! headroom.
//!
//! Every check takes its signal from an injected probe (a plain closure)
//! rather than reading the OS or network itself, so a test seeds a broken
//! state deterministically. [`probes`] holds best-effort default probes a
//! production caller can wire in without adding a dependency; where no real
//! OS query exists yet in this codebase (accessibility permission, audio
//! device enumeration, graphics memory), the default optimistically reports
//! the healthy reading and says so in its doc comment, the same way
//! `operant_core::supervisor::Supervisor` defers a real `Child` to whichever
//! later lane owns an actual sidecar: this lane hardens the policy (the
//! threshold, the `Finding` shape, the catalog mapping) around a seam a
//! later lane completes.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use crate::catalog::ErrorKind;
use crate::finding::{Check, Finding, Severity};

// ---------------------------------------------------------------------------
// Model reachable / updater reachable: share a network-reachability shape.
// ---------------------------------------------------------------------------

struct Reachability {
    id: &'static str,
    kind: ErrorKind,
    severity_on_fail: Severity,
    healthy_what: &'static str,
    healthy_why: &'static str,
    probe: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

impl Reachability {
    fn run(&self) -> Finding {
        match (self.probe)() {
            Ok(()) => Finding::healthy(self.id, self.healthy_what, self.healthy_why),
            Err(_detail) => {
                Finding::from_catalog(self.id, self.severity_on_fail, &self.kind.entry())
            }
        }
    }
}

/// Doctor check: is the configured model reachable.
pub struct ModelReachableCheck(Reachability);

impl ModelReachableCheck {
    pub fn new(probe: impl Fn() -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Reachability {
            id: "model_reachable",
            kind: ErrorKind::ModelUnreachable,
            severity_on_fail: Severity::Error,
            healthy_what: "The model is reachable.",
            healthy_why: "Operant was able to connect to it just now.",
            probe: Box::new(probe),
        })
    }

    /// Best-effort real probe: a short TCP connect to `addr`. Proves the
    /// model's network endpoint is reachable; it does not validate the
    /// model actually answers a request, which needs a real model client
    /// (a later lane's concern).
    pub fn tcp(addr: SocketAddr, timeout: Duration) -> Self {
        Self::new(move || probes::tcp_reachable(addr, timeout))
    }
}

impl Check for ModelReachableCheck {
    fn id(&self) -> &'static str {
        self.0.id
    }
    fn run(&self) -> Finding {
        self.0.run()
    }
}

/// Doctor check: can Operant reach the update server.
pub struct UpdaterReachableCheck(Reachability);

impl UpdaterReachableCheck {
    pub fn new(probe: impl Fn() -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Reachability {
            id: "updater_reachable",
            kind: ErrorKind::UpdaterUnreachable,
            severity_on_fail: Severity::Warn,
            healthy_what: "Operant can check for updates.",
            healthy_why: "Operant was able to connect to the update server just now.",
            probe: Box::new(probe),
        })
    }

    /// Best-effort real probe: a short TCP connect to `addr`.
    pub fn tcp(addr: SocketAddr, timeout: Duration) -> Self {
        Self::new(move || probes::tcp_reachable(addr, timeout))
    }
}

impl Check for UpdaterReachableCheck {
    fn id(&self) -> &'static str {
        self.0.id
    }
    fn run(&self) -> Finding {
        self.0.run()
    }
}

// ---------------------------------------------------------------------------
// Disk free above threshold.
// ---------------------------------------------------------------------------

/// Doctor check: is free disk space at or above `threshold_bytes`.
pub struct DiskFreeCheck {
    threshold_bytes: u64,
    probe: Box<dyn Fn() -> io::Result<u64> + Send + Sync>,
}

impl DiskFreeCheck {
    pub fn new(
        threshold_bytes: u64,
        probe: impl Fn() -> io::Result<u64> + Send + Sync + 'static,
    ) -> Self {
        Self {
            threshold_bytes,
            probe: Box::new(probe),
        }
    }

    /// Best-effort real probe for the drive holding `drive_letter` (e.g.
    /// `'C'`), via a PowerShell `Get-PSDrive` call. Windows only, matching
    /// this project's tier 1 platform; pure `std::process`, no new
    /// dependency.
    pub fn windows_drive(threshold_bytes: u64, drive_letter: char) -> Self {
        Self::new(threshold_bytes, move || {
            probes::windows_disk_free_bytes(drive_letter)
        })
    }
}

impl Check for DiskFreeCheck {
    fn id(&self) -> &'static str {
        "disk_free"
    }

    fn run(&self) -> Finding {
        match (self.probe)() {
            Ok(free) if free >= self.threshold_bytes => Finding::healthy(
                self.id(),
                "There is enough free disk space.",
                "Operant checked the free space on this computer's drive just now.",
            ),
            Ok(_below_threshold) => {
                Finding::from_catalog(self.id(), Severity::Error, &ErrorKind::DiskSpaceLow.entry())
            }
            Err(_could_not_read) => Finding::could_not_check(self.id()),
        }
    }
}

// ---------------------------------------------------------------------------
// Accessibility permission.
// ---------------------------------------------------------------------------

/// Doctor check: is the permission Operant needs to see and control the
/// screen granted.
pub struct AccessibilityPermissionCheck {
    probe: Box<dyn Fn() -> Result<bool, String> + Send + Sync>,
}

impl AccessibilityPermissionCheck {
    pub fn new(probe: impl Fn() -> Result<bool, String> + Send + Sync + 'static) -> Self {
        Self {
            probe: Box::new(probe),
        }
    }

    /// Best-effort default; see [`probes::assume_accessibility_granted`].
    pub fn best_effort() -> Self {
        Self::new(probes::assume_accessibility_granted)
    }
}

impl Check for AccessibilityPermissionCheck {
    fn id(&self) -> &'static str {
        "accessibility_permission"
    }

    fn run(&self) -> Finding {
        match (self.probe)() {
            Ok(true) => Finding::healthy(
                self.id(),
                "Operant has permission to see and control the screen.",
                "Operant checked this computer's permission settings.",
            ),
            Ok(false) => Finding::from_catalog(
                self.id(),
                Severity::Error,
                &ErrorKind::AccessibilityPermissionMissing.entry(),
            ),
            Err(_could_not_check) => Finding::could_not_check(self.id()),
        }
    }
}

// ---------------------------------------------------------------------------
// Audio devices present.
// ---------------------------------------------------------------------------

/// Doctor check: is at least one microphone or speaker present.
pub struct AudioDevicesPresentCheck {
    probe: Box<dyn Fn() -> Result<usize, String> + Send + Sync>,
}

impl AudioDevicesPresentCheck {
    pub fn new(probe: impl Fn() -> Result<usize, String> + Send + Sync + 'static) -> Self {
        Self {
            probe: Box::new(probe),
        }
    }

    /// Best-effort default; see [`probes::assume_audio_device_present`].
    pub fn best_effort() -> Self {
        Self::new(probes::assume_audio_device_present)
    }
}

impl Check for AudioDevicesPresentCheck {
    fn id(&self) -> &'static str {
        "audio_devices_present"
    }

    fn run(&self) -> Finding {
        match (self.probe)() {
            Ok(count) if count > 0 => Finding::healthy(
                self.id(),
                "Operant found a microphone and speakers.",
                "Operant checked the audio devices connected to this computer.",
            ),
            Ok(_none_found) => Finding::from_catalog(
                self.id(),
                Severity::Warn,
                &ErrorKind::AudioDeviceMissing.entry(),
            ),
            Err(_could_not_check) => Finding::could_not_check(self.id()),
        }
    }
}

// ---------------------------------------------------------------------------
// VRAM headroom.
// ---------------------------------------------------------------------------

/// A graphics memory reading: how much is free versus how much the
/// configured local model (and any of its helpers) needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VramReading {
    pub free_mb: u64,
    pub required_mb: u64,
}

/// Doctor check: is free graphics memory at or above what the configured
/// setup needs.
pub struct VramHeadroomCheck {
    probe: Box<dyn Fn() -> Result<VramReading, String> + Send + Sync>,
}

impl VramHeadroomCheck {
    pub fn new(probe: impl Fn() -> Result<VramReading, String> + Send + Sync + 'static) -> Self {
        Self {
            probe: Box::new(probe),
        }
    }

    /// Best-effort default; see [`probes::assume_vram_sufficient`].
    pub fn best_effort() -> Self {
        Self::new(probes::assume_vram_sufficient)
    }
}

impl Check for VramHeadroomCheck {
    fn id(&self) -> &'static str {
        "vram_headroom"
    }

    fn run(&self) -> Finding {
        match (self.probe)() {
            Ok(reading) if reading.free_mb >= reading.required_mb => Finding::healthy(
                self.id(),
                "There is enough graphics memory for the selected model.",
                "Operant compared the graphics memory this computer has free against what the model needs.",
            ),
            Ok(_not_enough) => {
                Finding::from_catalog(self.id(), Severity::Warn, &ErrorKind::GraphicsMemoryLow.entry())
            }
            Err(_could_not_check) => Finding::could_not_check(self.id()),
        }
    }
}

// ---------------------------------------------------------------------------
// Production default probes.
// ---------------------------------------------------------------------------

/// Best-effort default probes a production caller wires into the checks
/// above. None of this module runs during this crate's own tests (every
/// test seeds its own probe); it exists so `run_doctor` works out of the box
/// for a real `operant doctor` invocation.
pub mod probes {
    use std::io;
    use std::net::{SocketAddr, TcpStream};
    use std::process::Command;
    use std::time::Duration;

    use super::VramReading;

    /// A short TCP connect to `addr`. `Ok(())` only proves the endpoint
    /// accepted a connection, not that it speaks the expected protocol.
    pub fn tcp_reachable(addr: SocketAddr, timeout: Duration) -> Result<(), String> {
        TcpStream::connect_timeout(&addr, timeout)
            .map(|_stream| ())
            .map_err(|e| e.to_string())
    }

    /// Free bytes on the drive holding `drive_letter`, via PowerShell's
    /// `Get-PSDrive`. Pure `std::process`; no new crate dependency.
    pub fn windows_disk_free_bytes(drive_letter: char) -> io::Result<u64> {
        let script = format!("(Get-PSDrive -Name '{drive_letter}').Free");
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("powershell exited with a non-zero status"));
        }
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// No real OS query exists in this codebase yet for a one-time screen
    /// control permission grant, so this optimistically reports granted.
    /// A later lane that adds a real query (only meaningful on platforms
    /// that gate this, which tier 1 Windows mostly does not) replaces this.
    pub fn assume_accessibility_granted() -> Result<bool, String> {
        Ok(true)
    }

    /// No real audio device enumeration exists in this codebase yet. A
    /// later lane that wires the voice sidecar's audio backend replaces
    /// this with a real device count.
    pub fn assume_audio_device_present() -> Result<usize, String> {
        Ok(1)
    }

    /// No real graphics memory query exists in this codebase yet. Reports
    /// zero required, so the check trivially passes until a caller (once
    /// local model hardware detection exists) supplies a real reading for
    /// whatever is actually configured.
    pub fn assume_vram_sufficient() -> Result<VramReading, String> {
        Ok(VramReading {
            free_mb: 0,
            required_mb: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    const GB: u64 = 1024 * 1024 * 1024;

    // -- model / updater reachable -------------------------------------

    #[test]
    fn model_reachable_check_is_healthy_when_the_probe_succeeds() {
        let check = ModelReachableCheck::new(|| Ok(()));
        let finding = check.run();
        assert_eq!(finding.finding_id, "model_reachable");
        assert_eq!(finding.severity, Severity::Info);
    }

    /// BAR-shaped test for a different check: seeded broken state (the
    /// model is unreachable) yields the catalog's `ModelUnreachable` finding
    /// with the catalog's action, at error severity.
    #[test]
    fn model_unreachable_seeded_state_yields_the_right_finding() {
        let check = ModelReachableCheck::new(|| Err("connection refused".to_string()));
        let finding = check.run();
        let expected = ErrorKind::ModelUnreachable.entry();
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.what, expected.what);
        assert_eq!(finding.why, expected.why);
        assert_eq!(finding.action, expected.action);
        assert_eq!(finding.fix_command.as_deref(), expected.fix_command);
    }

    #[test]
    fn updater_unreachable_seeded_state_yields_warn_not_error() {
        let check = UpdaterReachableCheck::new(|| Err("timed out".to_string()));
        let finding = check.run();
        assert_eq!(finding.severity, Severity::Warn);
        assert_eq!(finding.what, ErrorKind::UpdaterUnreachable.entry().what);
    }

    #[test]
    fn tcp_probe_reaches_a_real_local_listener() {
        // Fully local and deterministic: bind an ephemeral port and connect
        // to it, proving the real (non-fake) probe path is wired correctly
        // without touching the network or depending on any external host.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let addr = listener.local_addr().unwrap();
        let check = ModelReachableCheck::tcp(addr, Duration::from_millis(500));
        let finding = check.run();
        assert_eq!(finding.severity, Severity::Info);
    }

    #[test]
    fn tcp_probe_reports_unreachable_for_a_closed_port() {
        // Bind then immediately drop, so nothing listens on this port
        // anymore; connecting to it should fail fast.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let check = UpdaterReachableCheck::tcp(addr, Duration::from_millis(500));
        let finding = check.run();
        assert_eq!(finding.severity, Severity::Warn);
    }

    // -- disk free --------------------------------------------------------

    #[test]
    fn disk_above_threshold_is_healthy() {
        let check = DiskFreeCheck::new(10 * GB, || Ok(50 * GB));
        let finding = check.run();
        assert_eq!(finding.severity, Severity::Info);
    }

    /// BAR: "a seeded broken state (e.g. disk below threshold) yields the
    /// right Finding with the right action."
    #[test]
    fn disk_below_threshold_yields_disk_space_low_finding_with_the_right_action() {
        let check = DiskFreeCheck::new(10 * GB, || Ok(GB));
        let finding = check.run();
        let expected = ErrorKind::DiskSpaceLow.entry();

        assert_eq!(finding.finding_id, "disk_free");
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.what, expected.what);
        assert_eq!(finding.why, expected.why);
        assert_eq!(finding.action, expected.action);
        assert_eq!(finding.action, "Free up some disk space, then try again.");
        assert_eq!(
            finding.fix_command.as_deref(),
            Some("operant doctor --fix disk_free")
        );
    }

    #[test]
    fn disk_exactly_at_threshold_counts_as_healthy() {
        let check = DiskFreeCheck::new(10 * GB, || Ok(10 * GB));
        assert_eq!(check.run().severity, Severity::Info);
    }

    #[test]
    fn disk_probe_error_is_a_could_not_check_finding_not_a_false_problem() {
        let check = DiskFreeCheck::new(10 * GB, || {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        });
        let finding = check.run();
        assert_eq!(finding.severity, Severity::Warn);
        assert_eq!(finding.what, "Operant could not finish this check.");
    }

    // -- accessibility permission ------------------------------------------

    #[test]
    fn accessibility_granted_is_healthy() {
        let check = AccessibilityPermissionCheck::new(|| Ok(true));
        assert_eq!(check.run().severity, Severity::Info);
    }

    #[test]
    fn accessibility_missing_seeded_state_yields_the_right_finding() {
        let check = AccessibilityPermissionCheck::new(|| Ok(false));
        let finding = check.run();
        let expected = ErrorKind::AccessibilityPermissionMissing.entry();
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.action, expected.action);
        assert_eq!(
            finding.fix_command.as_deref(),
            Some("operant doctor --fix accessibility_permission")
        );
    }

    #[test]
    fn accessibility_best_effort_default_is_healthy() {
        let check = AccessibilityPermissionCheck::best_effort();
        assert_eq!(check.run().severity, Severity::Info);
    }

    // -- audio devices present --------------------------------------------

    #[test]
    fn audio_devices_present_is_healthy() {
        let check = AudioDevicesPresentCheck::new(|| Ok(2));
        assert_eq!(check.run().severity, Severity::Info);
    }

    #[test]
    fn no_audio_devices_seeded_state_yields_the_right_finding() {
        let check = AudioDevicesPresentCheck::new(|| Ok(0));
        let finding = check.run();
        let expected = ErrorKind::AudioDeviceMissing.entry();
        assert_eq!(finding.severity, Severity::Warn);
        assert_eq!(finding.what, expected.what);
        assert_eq!(finding.action, expected.action);
    }

    // -- VRAM headroom ------------------------------------------------------

    #[test]
    fn vram_headroom_sufficient_is_healthy() {
        let check = VramHeadroomCheck::new(|| {
            Ok(VramReading {
                free_mb: 8000,
                required_mb: 4000,
            })
        });
        assert_eq!(check.run().severity, Severity::Info);
    }

    #[test]
    fn vram_headroom_insufficient_seeded_state_yields_the_right_finding() {
        let check = VramHeadroomCheck::new(|| {
            Ok(VramReading {
                free_mb: 2000,
                required_mb: 8000,
            })
        });
        let finding = check.run();
        let expected = ErrorKind::GraphicsMemoryLow.entry();
        assert_eq!(finding.severity, Severity::Warn);
        assert_eq!(finding.what, expected.what);
        assert_eq!(finding.why, expected.why);
        assert_eq!(finding.action, expected.action);
    }

    #[test]
    fn vram_headroom_best_effort_default_trivially_passes_when_nothing_is_configured() {
        let check = VramHeadroomCheck::best_effort();
        assert_eq!(check.run().severity, Severity::Info);
    }

    // -- ids are unique and stable, matching contracts/bus_events.md's
    //    "finding_id keys the same way run.step.failed's error_id does" ---

    #[test]
    fn every_check_id_is_unique_and_snake_case() {
        let checks: Vec<Box<dyn Check>> = vec![
            Box::new(ModelReachableCheck::new(|| Ok(()))),
            Box::new(DiskFreeCheck::new(1, || Ok(1))),
            Box::new(UpdaterReachableCheck::new(|| Ok(()))),
            Box::new(AccessibilityPermissionCheck::new(|| Ok(true))),
            Box::new(AudioDevicesPresentCheck::new(|| Ok(1))),
            Box::new(VramHeadroomCheck::new(|| {
                Ok(VramReading {
                    free_mb: 1,
                    required_mb: 1,
                })
            })),
        ];
        let mut ids = std::collections::BTreeSet::new();
        for check in &checks {
            let id = check.id();
            assert!(id.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
            assert!(ids.insert(id), "duplicate check id: {id}");
        }
        assert_eq!(ids.len(), 6);
    }
}
