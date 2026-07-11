//! macOS AX and Linux AT-SPI Perceiver stubs behind the trait [stub at launch]. Compile-only; every method returns PerceptionError::Denied with a typed 'not implemented on this platform' message.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-perception-stubs";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-perception-stubs");
        let _ = "perception_stubs";
    }
}
