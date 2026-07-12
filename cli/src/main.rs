//! Operant CLI (C14, FR-O4): run | explore | compile | import | dry-run |
//! list | install | publish | bench | doctor | explain.
//!
//! L13A wired `run`, `compile`, `dry-run`, `list`, `doctor`, and `explain`
//! against the already-merged crates (`operant-compiler`, `operant-replay`,
//! `operant-doctor`). L7B wires `install` and `publish` against
//! `operant-registry` (R1B/R1A). `bench` (L9B) is a later lane's verb and
//! stays unimplemented here.
//!
//! X9 adds `import`: `operant import playwright <spec.ts>` parses a basic
//! subset of a Playwright spec, maps it onto the `browser` namespace's
//! Action IR (`operant-action`, L9A), and compiles it through the same
//! `operant-compiler` (L8A) pipeline `compile` uses.
//!
//! Every verb is headless and deterministic: no verb prompts, polls, or
//! blocks on anything but the filesystem, `git` (for `publish`'s branch and
//! commit), and (for `explain` only) a local `node` subprocess running the
//! existing `@operant/sdk/render` renderer.

mod commands;
mod snapshot;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("operant 1.0.0");
            ExitCode::SUCCESS
        }
        Some("run") => finish(commands::run::run(&args[1..])),
        Some("explore") => finish(commands::explore::run(&args[1..])),
        Some("compile") => finish(commands::compile::run(&args[1..])),
        Some("import") => finish(commands::import::run(&args[1..])),
        Some("dry-run") => finish(commands::dry_run::run(&args[1..])),
        Some("list") => finish(commands::list::run(&args[1..])),
        Some("doctor") => finish(commands::doctor::run(&args[1..])),
        Some("explain") => finish(commands::explain::run(&args[1..])),
        Some("install") => finish(commands::install::run(&args[1..])),
        Some("publish") => finish(commands::publish::run(&args[1..])),
        #[cfg(feature = "dev-ipc-record")]
        Some("record-ipc") => finish(commands::record_ipc::run(&args[1..])),
        Some(verb) => {
            eprintln!("operant: verb '{verb}' not yet implemented in this build");
            ExitCode::FAILURE
        }
        None => {
            println!("operant 1.0.0");
            println!(
                "usage: operant <run|explore|compile|import|dry-run|list|install|publish|bench|doctor|explain> [args]"
            );
            ExitCode::SUCCESS
        }
    }
}

fn finish(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("operant: {e:#}");
            ExitCode::FAILURE
        }
    }
}
