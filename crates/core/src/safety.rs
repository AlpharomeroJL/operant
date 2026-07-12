//! Process-global input freeze (SAFETY, never-cut; `docs/specs/ipc-bridge.md`
//! section 4).
//!
//! This is the last line of defense that makes the panic button real: a
//! single process-wide flag that any real input backend checks *before every*
//! keystroke, click, cursor move, or clipboard write, and that any caller
//! (the tray panic button, the teach loop between actions, the `stop_run` /
//! `engage_killswitch` commands) can set the instant a human says "stop."
//!
//! Deliberately in `operant-core`, not `operant-action`: core sits below every
//! runtime crate, so the orchestrator's teach loop, the shell, and the action
//! layer can all read and write the same flag WITHOUT any of them depending on
//! each other. It is UNCONDITIONAL - always compiled, gated behind no cargo
//! feature - so a build that forgot to enable `real-input` cannot also forget
//! the freeze. The real Windows synthesizer
//! (`operant_action::WindowsSynthesizer`, behind `real-input`) checks
//! [`is_frozen`] at the top of every real input call and refuses immediately
//! when it is set.
//!
//! `SeqCst` throughout: this interlock gates real input synthesis, so the
//! extra fence over `Acquire`/`Release` is cheap insurance for the simplest
//! possible cross-thread reasoning. It is read once per input call, never in a
//! hot loop, so there is no performance case for relaxing it.

use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide freeze flag. Starts clear (not frozen).
static FROZEN: AtomicBool = AtomicBool::new(false);

/// Engage (`true`) or release (`false`) the process-global input freeze.
///
/// Setting `true` takes effect immediately: the store is a single atomic
/// write, so the freeze is in force the instant this returns and every
/// subsequent [`is_frozen`] read on any thread observes it. Setting `false`
/// is the explicit human resume; nothing in this crate ever clears the freeze
/// on its own. Idempotent in both directions.
pub fn set_frozen(frozen: bool) {
    FROZEN.store(frozen, Ordering::SeqCst);
}

/// True while the process-global input freeze is engaged.
///
/// Lock-free and allocation-free: safe to call before every real input
/// synthesis call with no measurable cost and no chance of blocking on
/// whatever thread engaged the freeze.
pub fn is_frozen() -> bool {
    FROZEN.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test only, doing the whole set/clear cycle in sequence: `set_frozen`
    // flips a process-wide static, and `cargo test` runs a crate's unit tests
    // as multiple threads in one process. Splitting this across several
    // #[test] functions could let one test observe another's transient
    // `true`, so the full round-trip lives in a single function and always
    // restores the flag to its `false` default before returning.
    #[test]
    fn freeze_flag_round_trips() {
        assert!(!is_frozen(), "the freeze must start clear");
        set_frozen(true);
        assert!(is_frozen(), "set_frozen(true) must engage the freeze");
        set_frozen(true);
        assert!(is_frozen(), "engaging an already-engaged freeze is a no-op");
        set_frozen(false);
        assert!(!is_frozen(), "set_frozen(false) must release the freeze");
    }
}
