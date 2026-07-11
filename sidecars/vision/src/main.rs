//! Sidecar entry point: reads one [`GroundRequest`] as JSON from stdin,
//! writes one [`GroundResponse`] (or `{"error": "..."}`) as JSON to stdout,
//! and exits. FIXTURE MODE only, always: this binary never touches a GPU
//! or a real vision model. A supervisor-spawned real (non-fixture) mode is
//! a follow-up once `operant_core::supervisor::Child` grows a real process
//! wrapper for this crate (see `lib.rs`'s module doc).

use std::io::{self, Read, Write};

use operant_vision_grounder::{ground, GroundRequest};

fn main() {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("operant-vision-grounder: failed to read stdin: {e}");
        std::process::exit(2);
    }

    let request: GroundRequest = match serde_json::from_str(&input) {
        Ok(r) => r,
        Err(e) => {
            emit_error(&format!("invalid GroundRequest JSON: {e}"));
            std::process::exit(1);
        }
    };

    match ground(&request) {
        Ok(response) => {
            let out = serde_json::to_string(&response).expect("GroundResponse always serializes");
            println!("{out}");
        }
        Err(e) => {
            emit_error(&e.to_string());
            std::process::exit(1);
        }
    }
}

fn emit_error(message: &str) {
    let payload = serde_json::json!({ "error": message });
    let mut stdout = io::stdout();
    // Best-effort: if stdout itself is broken there is nowhere left to
    // report to, and main() is about to exit(1) regardless.
    let _ = writeln!(stdout, "{payload}");
}
