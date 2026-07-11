//! Sidecar supervisor and VRAM arbitration broker (C1).
//!
//! [`Supervisor`] is generic over the [`Child`] trait so tests exercise the
//! restart policy against [`MockChild`], never a real spawned process. A real
//! `Child` wrapping `std::process::Child` belongs to whichever later lane owns
//! an actual sidecar (vision, voice); this lane hardens the policy and the bus
//! contract around it.
//!
//! Timing is measured through the [`Clock`] trait, never `std::thread::sleep`.
//! Tests use [`TestClock`], which only advances when told to, so restart-budget
//! assertions are deterministic and fast. Production callers use
//! [`SystemClock`].
//!
//! [`VramBroker`] serializes VRAM access across contending sidecars: exactly
//! one requester holds the grant at a time; a second requester queues FIFO
//! until the holder yields. It is poll/call driven (no background thread), so
//! its tests are also deterministic and fast.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::bus::events::{
    SidecarCrashed, SidecarHealth, SidecarRestarted, SidecarStarted, VramGrant, VramRequest,
    VramYield,
};
use crate::bus::Bus;

/// Default restart budget: a crashed child should be back up within 2 seconds.
pub const DEFAULT_RESTART_BUDGET_MS: u64 = 2000;

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// A source of monotonic milliseconds. Abstracts `Supervisor` away from wall
/// time so restart-budget tests are deterministic.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Wall-clock `Clock` for production use, monotonic from construction.
pub struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// Deterministic `Clock` for tests. Starts at 0 and only moves when
/// [`TestClock::advance`] is called; never sleeps.
#[derive(Default)]
pub struct TestClock {
    now_ms: AtomicU64,
}

impl TestClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the clock by `ms` milliseconds.
    pub fn advance(&self, ms: u64) {
        self.now_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Child
// ---------------------------------------------------------------------------

/// Health of a supervised child as of the last probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildHealth {
    Running,
    Crashed { exit_code: i32 },
}

/// Errors a [`Child`] implementation can report while starting.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ChildError {
    #[error("child failed to start: {0}")]
    StartFailed(String),
}

/// A supervisable sidecar. Implementations wrap a real OS process in later
/// lanes; [`MockChild`] here spawns nothing, so tests are fast and
/// deterministic.
pub trait Child: Send {
    /// Start (or restart) the child. Returns its pid on success.
    fn start(&mut self) -> Result<u32, ChildError>;

    /// Non-blocking health probe.
    fn health(&mut self) -> ChildHealth;
}

/// Shared, clonable handle onto a [`MockChild`]'s state, so a test can kill it
/// (simulate a crash) from outside the `Supervisor`, exactly like an OS process
/// dying asynchronously underneath a real watchdog.
#[derive(Clone)]
pub struct MockChildHandle {
    state: Arc<Mutex<MockState>>,
}

struct MockState {
    alive: bool,
    exit_code: i32,
    next_pid: u32,
    start_count: u32,
    fail_next_start: bool,
}

impl MockChildHandle {
    /// Simulate the OS killing/crashing the process.
    pub fn kill(&self, exit_code: i32) {
        let mut s = self.state.lock();
        s.alive = false;
        s.exit_code = exit_code;
    }

    /// Make the next `start()` call fail, e.g. to test exhausted-budget paths.
    pub fn fail_next_start(&self) {
        self.state.lock().fail_next_start = true;
    }

    /// How many times `start()` has succeeded so far.
    pub fn start_count(&self) -> u32 {
        self.state.lock().start_count
    }

    pub fn is_alive(&self) -> bool {
        self.state.lock().alive
    }
}

/// A `Child` that spawns no real process. Pair with the [`MockChildHandle`]
/// returned by [`MockChild::new`] to crash it from a test.
pub struct MockChild {
    handle: MockChildHandle,
}

impl MockChild {
    pub fn new() -> (Self, MockChildHandle) {
        let handle = MockChildHandle {
            state: Arc::new(Mutex::new(MockState {
                alive: false,
                exit_code: 0,
                next_pid: 0,
                start_count: 0,
                fail_next_start: false,
            })),
        };
        (
            MockChild {
                handle: handle.clone(),
            },
            handle,
        )
    }
}

impl Child for MockChild {
    fn start(&mut self) -> Result<u32, ChildError> {
        let mut s = self.handle.state.lock();
        if s.fail_next_start {
            s.fail_next_start = false;
            return Err(ChildError::StartFailed(
                "mock configured to fail this start".into(),
            ));
        }
        s.alive = true;
        s.next_pid += 1;
        s.start_count += 1;
        Ok(s.next_pid)
    }

    fn health(&mut self) -> ChildHealth {
        let s = self.handle.state.lock();
        if s.alive {
            ChildHealth::Running
        } else {
            ChildHealth::Crashed {
                exit_code: s.exit_code,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

/// Outcome of one [`Supervisor::tick`].
#[derive(Debug, Clone, PartialEq)]
pub enum TickOutcome {
    /// The child was healthy; `sidecar.health{ok:true}` was emitted.
    Healthy,
    /// The child had crashed and was restarted; `elapsed_ms` is measured from
    /// the tick that first observed the crash, via the supervisor's `Clock`.
    Restarted {
        attempt: u32,
        elapsed_ms: u64,
        within_budget: bool,
    },
    /// The child had crashed and a restart attempt was made but failed. The
    /// crash remains pending; the next `tick()` will retry.
    RestartFailed { error: String },
}

struct Inner<C> {
    child: C,
    attempt: u32,
    /// Clock time (per this supervisor's `Clock`) at which a crash was first
    /// observed and not yet resolved by a successful restart.
    crashed_at_ms: Option<u64>,
}

/// Spawns, monitors, and restarts a sidecar [`Child`], emitting
/// `sidecar.started` / `sidecar.health` / `sidecar.crashed` / `sidecar.restarted`
/// per `contracts/bus_events.md`.
///
/// Poll-driven: call [`Supervisor::tick`] from whatever loop owns real time (a
/// background thread with a real interval in production, or direct calls with
/// a [`TestClock`] in tests). This keeps the policy itself free of threads and
/// sleeps, which is what makes it possible to test deterministically.
pub struct Supervisor<C: Child> {
    name: String,
    bus: Arc<Bus>,
    clock: Arc<dyn Clock>,
    restart_budget_ms: u64,
    inner: Mutex<Inner<C>>,
}

impl<C: Child> Supervisor<C> {
    /// New supervisor with the default 2s restart budget.
    pub fn new(name: impl Into<String>, child: C, bus: Arc<Bus>, clock: Arc<dyn Clock>) -> Self {
        Self::with_budget_ms(name, child, bus, clock, DEFAULT_RESTART_BUDGET_MS)
    }

    pub fn with_budget_ms(
        name: impl Into<String>,
        child: C,
        bus: Arc<Bus>,
        clock: Arc<dyn Clock>,
        restart_budget_ms: u64,
    ) -> Self {
        Supervisor {
            name: name.into(),
            bus,
            clock,
            restart_budget_ms,
            inner: Mutex::new(Inner {
                child,
                attempt: 0,
                crashed_at_ms: None,
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Start the child for the first time. Emits `sidecar.started`.
    pub fn start(&self) -> Result<u32, ChildError> {
        let mut inner = self.inner.lock();
        let pid = inner.child.start()?;
        inner.crashed_at_ms = None;
        self.bus
            .publish_event(&SidecarStarted {
                name: self.name.clone(),
                pid,
            })
            .expect("SidecarStarted always serializes");
        Ok(pid)
    }

    /// Poll health once. On a healthy child, emits `sidecar.health{ok:true}`.
    /// On the *first* poll that observes a crash, stamps the detection time
    /// (via `Clock`) as the start of the restart-budget window and emits
    /// `sidecar.crashed`; a still-crashed child on a later poll (restart not
    /// yet attempted or still failing) does not re-stamp the time or re-emit
    /// the event. Pairs with [`Supervisor::restart_if_needed`], split out so a
    /// caller (or a test, via `TestClock::advance` in between) can let real
    /// time pass between detecting a crash and attempting the restart.
    pub fn check_health(&self) -> ChildHealth {
        let mut inner = self.inner.lock();
        let health = inner.child.health();
        match health {
            ChildHealth::Running => {
                self.bus
                    .publish_event(&SidecarHealth {
                        name: self.name.clone(),
                        ok: true,
                        rss_mb: None,
                        vram_mb: None,
                    })
                    .expect("SidecarHealth always serializes");
            }
            ChildHealth::Crashed { exit_code } => {
                let first_observation = inner.crashed_at_ms.is_none();
                inner
                    .crashed_at_ms
                    .get_or_insert_with(|| self.clock.now_ms());
                if first_observation {
                    self.bus
                        .publish_event(&SidecarCrashed {
                            name: self.name.clone(),
                            exit_code,
                        })
                        .expect("SidecarCrashed always serializes");
                }
            }
        }
        health
    }

    /// If a crash is pending from [`Supervisor::check_health`], attempt one
    /// restart now. `elapsed_ms` in the returned outcome is measured (via
    /// `Clock`) from the poll that first detected the crash to now, which is
    /// what makes `within_budget` a real assertion rather than a decorative
    /// one. Returns `None` if there is no pending crash.
    pub fn restart_if_needed(&self) -> Option<TickOutcome> {
        let mut inner = self.inner.lock();
        let crash_ms = inner.crashed_at_ms?;
        Some(match inner.child.start() {
            Ok(pid) => {
                let elapsed_ms = self.clock.now_ms().saturating_sub(crash_ms);
                inner.attempt += 1;
                let attempt = inner.attempt;
                inner.crashed_at_ms = None;
                self.bus
                    .publish_event(&SidecarStarted {
                        name: self.name.clone(),
                        pid,
                    })
                    .expect("SidecarStarted always serializes");
                self.bus
                    .publish_event(&SidecarRestarted {
                        name: self.name.clone(),
                        attempt,
                    })
                    .expect("SidecarRestarted always serializes");
                TickOutcome::Restarted {
                    attempt,
                    elapsed_ms,
                    within_budget: elapsed_ms <= self.restart_budget_ms,
                }
            }
            Err(e) => TickOutcome::RestartFailed {
                error: e.to_string(),
            },
        })
    }

    /// Convenience for a real watchdog loop that calls this repeatedly on a
    /// timer: check health, and if crashed, immediately attempt one restart in
    /// the same call. Callers that want fine control over the gap between
    /// detection and restart (as `TestClock`-driven tests do) should call
    /// [`Supervisor::check_health`] and [`Supervisor::restart_if_needed`]
    /// directly instead.
    pub fn tick(&self) -> TickOutcome {
        match self.check_health() {
            ChildHealth::Running => TickOutcome::Healthy,
            ChildHealth::Crashed { .. } => self
                .restart_if_needed()
                .expect("check_health just recorded a pending crash"),
        }
    }
}

// ---------------------------------------------------------------------------
// VRAM arbitration broker
// ---------------------------------------------------------------------------

/// Result of a [`VramBroker::request`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VramLease {
    /// Granted immediately; the caller now holds the broker.
    Granted,
    /// Another requester currently holds the broker; queued FIFO.
    Queued,
}

#[derive(Clone)]
struct PendingRequest {
    requester: String,
    mb: u64,
}

#[derive(Default)]
struct BrokerState {
    holder: Option<PendingRequest>,
    queue: VecDeque<PendingRequest>,
}

/// Serializes VRAM access across contending sidecars so at most one holds the
/// grant at a time. Mirrors `contracts/bus_events.md` "Sidecars and VRAM":
/// `vram.request` on every call, `vram.grant` when a requester is granted
/// (immediately or later, off the queue), `vram.yield` when the holder
/// releases. Poll/call driven, like [`Supervisor`]: no background thread, no
/// blocking, so tests are deterministic.
pub struct VramBroker {
    bus: Arc<Bus>,
    state: Mutex<BrokerState>,
}

impl VramBroker {
    pub fn new(bus: Arc<Bus>) -> Self {
        VramBroker {
            bus,
            state: Mutex::new(BrokerState::default()),
        }
    }

    /// Request `mb` of VRAM as `requester`. Always emits `vram.request`. Grants
    /// immediately (emits `vram.grant`) if nobody currently holds the broker;
    /// otherwise queues FIFO behind the current holder and any earlier queued
    /// requesters, returning [`VramLease::Queued`].
    pub fn request(&self, requester: &str, mb: u64) -> VramLease {
        let mut state = self.state.lock();
        self.bus
            .publish_event(&VramRequest {
                requester: requester.to_string(),
                mb,
            })
            .expect("VramRequest always serializes");

        if let Some(holder) = &state.holder {
            if holder.requester == requester {
                // Already holds the grant: idempotent, no duplicate grant event.
                return VramLease::Granted;
            }
            state.queue.push_back(PendingRequest {
                requester: requester.to_string(),
                mb,
            });
            return VramLease::Queued;
        }

        state.holder = Some(PendingRequest {
            requester: requester.to_string(),
            mb,
        });
        self.bus
            .publish_event(&VramGrant {
                requester: requester.to_string(),
                mb,
            })
            .expect("VramGrant always serializes");
        VramLease::Granted
    }

    /// Release the broker as `requester`. Emits `vram.yield`. If a request is
    /// queued, promotes the next one FIFO and emits its `vram.grant`, returning
    /// the newly granted requester's name. A no-op (returns `None`, no events)
    /// if `requester` does not currently hold the broker.
    pub fn yield_now(&self, requester: &str) -> Option<String> {
        let mut state = self.state.lock();
        let holds = matches!(&state.holder, Some(h) if h.requester == requester);
        if !holds {
            return None;
        }
        let held = state.holder.take().expect("checked Some above");
        self.bus
            .publish_event(&VramYield {
                yielder: held.requester.clone(),
                mb: held.mb,
            })
            .expect("VramYield always serializes");

        if let Some(next) = state.queue.pop_front() {
            let granted_name = next.requester.clone();
            self.bus
                .publish_event(&VramGrant {
                    requester: next.requester.clone(),
                    mb: next.mb,
                })
                .expect("VramGrant always serializes");
            state.holder = Some(next);
            Some(granted_name)
        } else {
            None
        }
    }

    /// The requester currently holding the broker, if any.
    pub fn current_holder(&self) -> Option<String> {
        self.state
            .lock()
            .holder
            .as_ref()
            .map(|h| h.requester.clone())
    }

    /// Requesters currently queued, FIFO order.
    pub fn queue_len(&self) -> usize {
        self.state.lock().queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topics(bus: &Bus, pattern: &str) -> crate::bus::Subscription {
        bus.subscribe(pattern)
    }

    #[test]
    fn start_emits_sidecar_started_with_pid() {
        let bus = Arc::new(Bus::new());
        let clock: Arc<dyn Clock> = Arc::new(TestClock::new());
        let (child, handle) = MockChild::new();
        let sub = topics(&bus, "sidecar.started");

        let sup = Supervisor::new("vision", child, bus.clone(), clock);
        let pid = sup.start().expect("start succeeds");
        assert_eq!(pid, 1);
        assert_eq!(handle.start_count(), 1);

        let env = sub.rx.try_recv().expect("sidecar.started published");
        let payload: SidecarStarted = serde_json::from_value(env.payload).unwrap();
        assert_eq!(
            payload,
            SidecarStarted {
                name: "vision".into(),
                pid: 1
            }
        );
    }

    #[test]
    fn tick_on_healthy_child_emits_health_ok() {
        let bus = Arc::new(Bus::new());
        let clock: Arc<dyn Clock> = Arc::new(TestClock::new());
        let (child, _handle) = MockChild::new();
        let sup = Supervisor::new("vision", child, bus.clone(), clock);
        sup.start().unwrap();

        let sub = topics(&bus, "sidecar.health");
        let outcome = sup.tick();
        assert_eq!(outcome, TickOutcome::Healthy);
        let env = sub.rx.try_recv().expect("sidecar.health published");
        let payload: SidecarHealth = serde_json::from_value(env.payload).unwrap();
        assert!(payload.ok);
    }

    /// Test (b): supervisor restarts a killed MockChild and the restart is
    /// observed on the bus within the budget, measured on a deterministic
    /// clock (advanced explicitly, never a wall-sleep).
    #[test]
    fn restarts_crashed_child_within_budget() {
        let bus = Arc::new(Bus::new());
        let clock = Arc::new(TestClock::new());
        let clock_dyn: Arc<dyn Clock> = clock.clone();
        let (child, handle) = MockChild::new();
        let sup = Supervisor::new("vision", child, bus.clone(), clock_dyn);

        let sidecar_events = topics(&bus, "sidecar.*");

        sup.start().unwrap();
        assert_eq!(handle.start_count(), 1);

        handle.kill(1);
        assert!(!handle.is_alive());

        // Detect the crash now (clock at 0ms); this stamps the start of the
        // restart-budget window.
        sup.check_health();

        // Simulate 1.5s passing before the watchdog gets around to actually
        // restarting, well inside the 2s budget.
        clock.advance(1500);

        let outcome = sup.restart_if_needed().expect("crash was pending");
        match outcome {
            TickOutcome::Restarted {
                attempt,
                elapsed_ms,
                within_budget,
            } => {
                assert_eq!(attempt, 1);
                assert_eq!(elapsed_ms, 1500);
                assert!(
                    within_budget,
                    "1500ms must be within the {DEFAULT_RESTART_BUDGET_MS}ms budget"
                );
            }
            other => panic!("expected Restarted, got {other:?}"),
        }
        assert_eq!(handle.start_count(), 2);
        assert!(handle.is_alive());

        let topics_seen: Vec<String> = sidecar_events.rx.try_iter().map(|e| e.topic).collect();
        assert_eq!(
            topics_seen,
            vec![
                "sidecar.started",
                "sidecar.crashed",
                "sidecar.started",
                "sidecar.restarted"
            ]
        );
    }

    /// Bonus hardening: the budget check is real, not decorative. When the
    /// simulated gap between crash and restart exceeds 2s, `within_budget` is
    /// false even though the restart still happened.
    #[test]
    fn restart_reports_budget_exceeded_when_slow() {
        let bus = Arc::new(Bus::new());
        let clock = Arc::new(TestClock::new());
        let clock_dyn: Arc<dyn Clock> = clock.clone();
        let (child, handle) = MockChild::new();
        let sup = Supervisor::new("voice", child, bus, clock_dyn);

        sup.start().unwrap();
        handle.kill(9);
        sup.check_health();
        clock.advance(2500);

        match sup.restart_if_needed().expect("crash was pending") {
            TickOutcome::Restarted {
                elapsed_ms,
                within_budget,
                ..
            } => {
                assert_eq!(elapsed_ms, 2500);
                assert!(!within_budget);
            }
            other => panic!("expected Restarted, got {other:?}"),
        }
    }

    #[test]
    fn failed_restart_is_retried_on_next_tick() {
        let bus = Arc::new(Bus::new());
        let clock = Arc::new(TestClock::new());
        let clock_dyn: Arc<dyn Clock> = clock.clone();
        let (child, handle) = MockChild::new();
        let sup = Supervisor::new("vision", child, bus, clock_dyn);

        sup.start().unwrap();
        handle.kill(1);
        handle.fail_next_start();

        match sup.tick() {
            TickOutcome::RestartFailed { .. } => {}
            other => panic!("expected RestartFailed, got {other:?}"),
        }
        assert!(
            !handle.is_alive(),
            "still down after a failed restart attempt"
        );

        clock.advance(100);
        match sup.tick() {
            TickOutcome::Restarted { elapsed_ms, .. } => assert_eq!(elapsed_ms, 100),
            other => panic!("expected Restarted on retry, got {other:?}"),
        }
    }

    // -- VRAM broker ---------------------------------------------------------

    /// Test (c): broker serializes two concurrent requests; the second waits
    /// for the first to yield.
    #[test]
    fn broker_serializes_two_contending_requesters() {
        let bus = Arc::new(Bus::new());
        let broker = VramBroker::new(bus.clone());
        let vram_events = topics(&bus, "vram.*");

        let a = broker.request("vision", 4000);
        assert_eq!(a, VramLease::Granted);
        assert_eq!(broker.current_holder(), Some("vision".to_string()));

        // Second, contending requester: must wait, not be granted.
        let b = broker.request("voice", 1500);
        assert_eq!(b, VramLease::Queued);
        assert_eq!(
            broker.current_holder(),
            Some("vision".to_string()),
            "vision still holds"
        );
        assert_eq!(broker.queue_len(), 1);

        // First yields; the queued second is promoted automatically.
        let promoted = broker.yield_now("vision");
        assert_eq!(promoted, Some("voice".to_string()));
        assert_eq!(broker.current_holder(), Some("voice".to_string()));
        assert_eq!(broker.queue_len(), 0);

        let topics_seen: Vec<String> = vram_events.rx.try_iter().map(|e| e.topic).collect();
        assert_eq!(
            topics_seen,
            vec![
                "vram.request",
                "vram.grant",
                "vram.request",
                "vram.yield",
                "vram.grant"
            ]
        );
    }

    #[test]
    fn broker_yield_by_non_holder_is_a_noop() {
        let bus = Arc::new(Bus::new());
        let broker = VramBroker::new(bus.clone());
        broker.request("vision", 4000);

        let sub = topics(&bus, "vram.yield");
        assert_eq!(
            broker.yield_now("voice"),
            None,
            "voice never held the broker"
        );
        assert!(sub.rx.try_recv().is_err(), "no vram.yield for a non-holder");
        assert_eq!(broker.current_holder(), Some("vision".to_string()));
    }

    #[test]
    fn broker_queues_fifo_across_three_requesters() {
        let bus = Arc::new(Bus::new());
        let broker = VramBroker::new(bus);

        assert_eq!(broker.request("a", 1000), VramLease::Granted);
        assert_eq!(broker.request("b", 1000), VramLease::Queued);
        assert_eq!(broker.request("c", 1000), VramLease::Queued);
        assert_eq!(broker.queue_len(), 2);

        assert_eq!(broker.yield_now("a"), Some("b".to_string()));
        assert_eq!(broker.yield_now("b"), Some("c".to_string()));
        assert_eq!(broker.yield_now("c"), None, "queue exhausted");
        assert_eq!(broker.current_holder(), None);
    }
}
