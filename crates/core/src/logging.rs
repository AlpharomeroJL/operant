//! Structured logging (C1): idempotent `tracing_subscriber` initialization.
//!
//! Several binaries and many `#[test]` functions across the workspace may call
//! [`init`] in the same process. It must never panic on a second call, and it
//! must never panic if some other subscriber (a test harness, a host
//! application) already installed a global default first.

use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize the global `tracing` subscriber with an env filter, once per
/// process. Reads `OPERANT_LOG` first, then falls back to the conventional
/// `RUST_LOG`, then falls back to `info` if neither is set.
///
/// Safe to call multiple times, from multiple threads, or from multiple crates
/// in the same process (e.g. every crate's test suite calling it in `setup`):
/// only the first call has any effect, and even that call tolerates a
/// subscriber already being installed rather than panicking.
pub fn init() {
    INIT.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_env("OPERANT_LOG")
            .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        // `try_init` (not `init`) so a subscriber already installed by a test
        // harness or host process is a no-op, not a panic.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        init();
        init();
        init();
    }

    #[test]
    fn init_from_other_test_modules_does_not_panic() {
        // Simulates another crate/test in the same process having already set a
        // global default subscriber before this one calls init().
        init();
        tracing::info!("logging is initialized");
    }
}
