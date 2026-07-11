//! Shared Operant IR types. The Rust embodiment of `contracts/`.
//!
//! These types are the frozen cross-lane surface. They are append-only during the
//! campaign: add optional fields (`#[serde(default)]`), never rename or remove.
//! Every type here round-trips its JSON fixture in a test (see `tests/`).

pub mod action;
pub mod snapshot;
pub mod gate;
pub mod manifest;
pub mod bus;

pub use action::*;
pub use snapshot::*;
pub use gate::*;
pub use manifest::*;

/// Risk class of an action. Ordered: read < write < destructive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskClass {
    Read,
    Write,
    Destructive,
}

impl RiskClass {
    /// True when `self` exceeds the `ceiling` (used by grant checks).
    pub fn exceeds(&self, ceiling: RiskClass) -> bool {
        *self > ceiling
    }
}

/// Grounding strategy actually used for a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grounding {
    Uia,
    Vision,
    Adapter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_ordering_holds() {
        assert!(RiskClass::Destructive > RiskClass::Write);
        assert!(RiskClass::Write > RiskClass::Read);
        assert!(RiskClass::Destructive.exceeds(RiskClass::Write));
        assert!(!RiskClass::Read.exceeds(RiskClass::Write));
    }
}
