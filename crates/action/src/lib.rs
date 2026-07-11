//! Action layer (C4): Action IR executor, input synthesis, clipboard, window management, and the adapter registration framework. Resolution order adapter > UIA > vision is enforced here. L3A implements SendInput; adapters land in L2B.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

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
