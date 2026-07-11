//! Operant core runtime.
//!
//! Scaffold surfaces that other crates depend on:
//! - [`bus`]: a typed, versioned in-process pub/sub event bus (C1).
//! - [`perceive`]: the OS-agnostic [`perceive::Perceiver`] trait (C2).
//! - [`config`]: a minimal config store.
//!
//! L1A (core-bus) hardens the bus, config, logging, and sidecar supervisor with
//! watchdog and VRAM arbitration. L2A adds the UIA `Perceiver` implementation in
//! its own crate against the trait defined here.

pub mod bus;
pub mod config;
pub mod perceive;

pub use bus::Bus;
pub use perceive::{PerceptionError, Perceiver};
