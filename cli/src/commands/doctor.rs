//! `operant doctor`: run `operant-doctor`'s checks and print the report.
//! `crates/doctor/src/cli.rs`'s own module doc: "wiring it to the `doctor`
//! subcommand's argument parsing in `cli/src/main.rs` is the CLI lane's
//! job (this lane's owned path is `crates/doctor` only)" -- this is that
//! wiring.
//!
//! Runs the checks this build has a real or documented best-effort probe
//! for (disk space, accessibility permission, audio devices, graphics
//! memory headroom); `model_reachable`/`updater_reachable` need a
//! configured endpoint no lane has wired into the CLI yet, so they are
//! left out of the default set rather than faked as always-healthy or
//! always-unreachable (see FOLLOWUPS in the lane report).

use anyhow::Result;
use operant_doctor::{
    AccessibilityPermissionCheck, AudioDevicesPresentCheck, Check, DiskFreeCheck, Severity,
    VramHeadroomCheck,
};

const DEFAULT_MIN_DISK_GB: u64 = 1;
const DEFAULT_DRIVE: char = 'C';

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let drive = flag_value(args, "--drive")
        .and_then(|s| s.chars().next())
        .unwrap_or(DEFAULT_DRIVE);
    let min_disk_bytes = flag_value(args, "--min-disk-gb")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MIN_DISK_GB)
        * 1_000_000_000;

    let checks: Vec<Box<dyn Check>> = vec![
        Box::new(DiskFreeCheck::windows_drive(min_disk_bytes, drive)),
        Box::new(AccessibilityPermissionCheck::best_effort()),
        Box::new(AudioDevicesPresentCheck::best_effort()),
        Box::new(VramHeadroomCheck::best_effort()),
    ];

    let report = operant_doctor::run_doctor_verb(&checks, None);
    print!("{}", report.text);

    if report.exit_code != 0 {
        let errors = report
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        anyhow::bail!("doctor found {errors} problem(s) needing attention");
    }
    Ok(())
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn print_help() {
    println!("operant doctor [--drive C] [--min-disk-gb 1]");
    println!();
    println!("Run self-diagnostics: disk space, accessibility permission, audio devices,");
    println!("graphics memory headroom. Exits non-zero if any check is at error severity.");
}
