//! Operant core runtime (C1), hardened by L1A.
//!
//! Surfaces other crates depend on:
//! - [`bus`]: a typed, versioned in-process pub/sub event bus, plus
//!   [`bus::events`] strongly-typed constructors for the run/gate/sidecar/
//!   guardian topic families in `contracts/bus_events.md`, so publishers do
//!   not hand-build JSON.
//! - [`perceive`]: the OS-agnostic [`perceive::Perceiver`] trait (C2).
//! - [`config`]: a config store with JSON file persistence and a
//!   `config.changed` bus event on every `set`.
//! - [`logging`]: idempotent `tracing_subscriber` initialization.
//! - [`safety`]: the process-global input freeze (kill switch, SAFETY) that
//!   real input backends check before every keystroke/click/clipboard write.
//! - [`supervisor`]: a sidecar supervisor generic over the
//!   [`supervisor::Child`] trait (spawn, health, watchdog restart within a 2s
//!   budget) and a VRAM arbitration broker that serializes contending
//!   sidecars one at a time.
//!
//! L2A adds the UIA `Perceiver` implementation in its own crate against the
//! trait defined here.

pub mod bus;
pub mod config;
pub mod logging;
pub mod perceive;
pub mod safety;
pub mod supervisor;

pub use bus::Bus;
pub use perceive::{Perceiver, PerceptionError};
pub use supervisor::Supervisor;
