//! HITL control (C6): pause, redirect, and resume as bus events.
//!
//! The loop polls a [`HitlControl`] once at each action boundary.
//! Production wires this to the bus ([`BusControl`]) so a UI, the voice
//! sidecar, or the CLI can steer a running loop purely by publishing
//! `run.control.*` events; tests inject a deterministic script via
//! [`ScriptedControl`]. Either way, [`crate::explore::ExploreLoop::run`]
//! is the only thing that ever publishes the outgoing `run.paused` /
//! `run.redirected` / `run.resumed` events documented in
//! `contracts/bus_events.md`.

use std::collections::VecDeque;

use operant_core::Bus;
use operant_ir::bus::Envelope;

/// One HITL command a human can inject between action-execution steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunControl {
    /// Freeze the run until a [`RunControl::Resume`] or
    /// [`RunControl::Redirect`] arrives.
    Pause,
    /// A natural-language correction. Captured into the trajectory as a
    /// `human_correction` on the next recorded step (so the compiler can
    /// later collapse it), then the run resumes automatically -- an
    /// explicit [`RunControl::Pause`] is not required first.
    Redirect(String),
    /// Explicit resume out of a [`RunControl::Pause`].
    Resume,
}

/// Polled once per action boundary by [`crate::explore::ExploreLoop::run`].
/// Non-blocking by contract: implementations must return `None` promptly
/// when nothing is pending rather than wait.
pub trait HitlControl: Send {
    fn poll(&mut self) -> Option<RunControl>;
}

/// No human ever intervenes. The default for a run with no HITL wiring.
#[derive(Debug, Default)]
pub struct NoControl;

impl HitlControl for NoControl {
    fn poll(&mut self) -> Option<RunControl> {
        None
    }
}

/// Bus-driven HITL: a UI, the voice sidecar, or a CLI can pause, redirect,
/// or resume a running loop purely by publishing on `run.control.*`.
///
/// `run.control.pause` / `run.control.resume` payloads are ignored beyond
/// the topic itself; `run.control.redirect` carries
/// `{ "instruction": "<text>" }`. These topics are additive to
/// `contracts/bus_events.md` (the contract's own versioning rule 3: "New
/// topics may be added freely"); this module is the sole owner of the
/// incoming-control side of the run family, the outgoing side being the
/// existing `run.paused` / `run.redirected` / `run.resumed` events. Scoped
/// to one actively-controlled run at a time, matching
/// `docs/ARCHITECTURE.md` section 5's serialized run queue; a future
/// multi-run scheduler would additionally tag these by `run_id`.
pub struct BusControl {
    rx: crossbeam_channel::Receiver<Envelope>,
}

impl BusControl {
    pub fn subscribe(bus: &Bus) -> Self {
        BusControl {
            rx: bus.subscribe("run.control.*").rx,
        }
    }
}

impl HitlControl for BusControl {
    fn poll(&mut self) -> Option<RunControl> {
        // Drain until a recognized command is found (or the channel runs
        // dry): one bit of noise on this topic family must not mask a real
        // command queued right behind it.
        while let Ok(env) = self.rx.try_recv() {
            let cmd = match env.topic.as_str() {
                "run.control.pause" => Some(RunControl::Pause),
                "run.control.resume" => Some(RunControl::Resume),
                "run.control.redirect" => env
                    .payload
                    .get("instruction")
                    .and_then(|v| v.as_str())
                    .map(|s| RunControl::Redirect(s.to_string())),
                _ => None,
            };
            if cmd.is_some() {
                return cmd;
            }
        }
        None
    }
}

/// A plain public test utility, in the same spirit as this workspace's
/// other mocks (`MockPlannerBackend`, `MockSynthesizer`, `FixturePerceiver`):
/// a fixed, deterministic sequence of control values, one consumed per
/// [`HitlControl::poll`] call. Returns `None` once exhausted.
pub struct ScriptedControl {
    script: VecDeque<Option<RunControl>>,
}

impl ScriptedControl {
    pub fn new(script: impl IntoIterator<Item = Option<RunControl>>) -> Self {
        ScriptedControl {
            script: script.into_iter().collect(),
        }
    }
}

impl HitlControl for ScriptedControl {
    fn poll(&mut self) -> Option<RunControl> {
        self.script.pop_front().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_control_replays_in_order_then_goes_quiet() {
        let mut c = ScriptedControl::new([None, Some(RunControl::Redirect("use ctrl+s".into())), None]);
        assert_eq!(c.poll(), None);
        assert_eq!(c.poll(), Some(RunControl::Redirect("use ctrl+s".into())));
        assert_eq!(c.poll(), None);
        assert_eq!(c.poll(), None, "stays quiet once exhausted");
    }

    #[test]
    fn bus_control_translates_each_recognized_topic() {
        let bus = Bus::default();
        let mut control = BusControl::subscribe(&bus);
        assert_eq!(control.poll(), None, "nothing published yet");

        bus.publish("something.else", serde_json::json!({}));
        bus.publish(
            "run.control.redirect",
            serde_json::json!({ "instruction": "stop clicking the menu" }),
        );
        assert_eq!(
            control.poll(),
            Some(RunControl::Redirect("stop clicking the menu".to_string())),
            "an unrelated topic must not be delivered at all (the bus itself filters it)"
        );

        bus.publish("run.control.pause", serde_json::json!({}));
        assert_eq!(control.poll(), Some(RunControl::Pause));
        bus.publish("run.control.resume", serde_json::json!({}));
        assert_eq!(control.poll(), Some(RunControl::Resume));
    }

    #[test]
    fn bus_control_skips_unrecognized_topics_and_finds_the_real_command_behind_them() {
        let bus = Bus::default();
        let mut control = BusControl::subscribe(&bus);
        bus.publish("run.control.mystery", serde_json::json!({}));
        bus.publish("run.control.resume", serde_json::json!({}));
        assert_eq!(control.poll(), Some(RunControl::Resume));
    }

    #[test]
    fn bus_control_ignores_a_redirect_with_no_instruction_field() {
        let bus = Bus::default();
        let mut control = BusControl::subscribe(&bus);
        bus.publish("run.control.redirect", serde_json::json!({}));
        bus.publish("run.control.resume", serde_json::json!({}));
        assert_eq!(
            control.poll(),
            Some(RunControl::Resume),
            "a malformed redirect payload is skipped, not delivered as garbage"
        );
    }
}
