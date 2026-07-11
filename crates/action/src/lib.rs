//! Action layer (C4): Action IR executor, input synthesis, clipboard,
//! window management, and the adapter registration framework. Resolution
//! order adapter > UIA > vision is enforced here. L3A implements
//! SendInput; adapters land in L2B.
//!
//! - [`synth`]: the [`Synthesizer`] trait every input backend implements,
//!   [`MockSynthesizer`] (every test in this crate runs against it), and
//!   the panic-safe [`ModifierReleaseGuard`] the kill switch depends on.
//! - [`real_win`]: the Windows SendInput backend, behind the `real-input`
//!   cargo feature so the default build never has to compile the
//!   `windows` crate.
//! - [`executor`]: [`Executor`], the Action IR dispatcher, plus the
//!   [`Approval`] safety seam that refuses destructive-risk actions.
//! - [`adapter`]: the [`Adapter`] registration framework and JSON Schema
//!   param validation for `adapter_call`, plus [`resolve_strategy`].

pub mod adapter;
pub mod executor;
pub mod synth;

#[cfg(feature = "real-input")]
pub mod real_win;

pub use adapter::{
    resolve_strategy, Adapter, AdapterError, AdapterRegistry, Idempotency, VerbSpec,
};
pub use executor::{
    ActionError, ActionOutcome, Approval, Executor, NoopSleeper, RealSleeper, ResolvedPoint,
    Sleeper,
};
pub use synth::{
    MockSynthesizer, ModifierReleaseGuard, ScrollDirection, SynthCall, Synthesizer,
    SynthesizerError,
};

#[cfg(feature = "real-input")]
pub use real_win::WindowsSynthesizer;

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-action";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-action");
        let _ = "action";
    }
}
