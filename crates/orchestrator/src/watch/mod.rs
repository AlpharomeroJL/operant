//! Watch-and-suggest (opt-in, OFF by default): a local repetition detector.
//!
//! Watches manual (non-Operant) user actions, and when the same short sequence
//! repeats often enough offers to learn it -- "You have done this 4 times.
//! Want me to learn it?" Accepting seeds a supervised EXPLORE run through
//! L7A's [`crate::explore::ExploreLoop`] (this module never restructures that
//! loop; it only hands it a goal and the redacted exemplar steps).
//!
//! # Trust invariants
//! This is the trust-sensitive packet of its batch, so its guarantees are
//! stated as hard invariants, not best effort:
//!
//! * **Off means off, provably.** The feature is OFF by default
//!   ([`WatchConfig::default`] sets `enabled = false`). While off,
//!   [`Watcher::observe`] returns before it reaches the buffer's single write
//!   path ([`buffer::EventSink::write`]), so the buffer is never written to --
//!   not merely left empty. The `disabled_watcher_never_writes` test proves
//!   this by asserting the write path is entered zero times via a spy sink,
//!   including its monotonic `total_writes` counter that a purge cannot reset.
//! * **Local-only and redacted before storage.** Every event is passed through
//!   [`normalize::redact_for_storage`] -- which reuses X4's credential
//!   classifier and drops all free-typed text -- before it reaches the buffer.
//!   Nothing leaves the process; the buffer is an in-memory ring.
//! * **Capped and purgeable.** The buffer holds at most `capacity` events, and
//!   [`Watcher::purge`] wipes stored events and all in-flight detector state in
//!   one call.
//! * **Exactly one offer per pattern.** The detector latches each pattern after
//!   its first offer, so a pattern that keeps repeating past the threshold
//!   never produces a second offer.

pub mod buffer;
pub mod detect;
pub mod event;
pub mod normalize;
pub mod suggest;

use std::collections::HashMap;

use operant_core::Bus;
use operant_ir::Action;

pub use buffer::{CappedBuffer, EventSink, SpySink};
pub use detect::{DetectedPattern, NgramDetector};
pub use event::{ManualEvent, StoredEvent};
pub use suggest::{
    offer_sentence, SuggestionAccepted, SuggestionDismissed, SuggestionOffered,
    SUGGESTION_ACCEPTED, SUGGESTION_DISMISSED, SUGGESTION_OFFERED,
};

/// Default sliding-window length for pattern matching.
pub const DEFAULT_NGRAM_SIZE: usize = 3;
/// Default number of occurrences before a pattern is offered.
pub const DEFAULT_THRESHOLD: usize = 4;
/// Default maximum number of events held in the local buffer.
pub const DEFAULT_CAPACITY: usize = 512;

/// Configuration for the detector. `enabled` defaults to `false`: the whole
/// feature is opt-in, and every other field is inert until it is turned on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchConfig {
    /// Master switch. OFF by default. While false, nothing is observed,
    /// stored, or matched.
    pub enabled: bool,
    /// Sliding-window length for n-gram matching.
    pub ngram_size: usize,
    /// Occurrences before a repeated pattern is offered.
    pub threshold: usize,
    /// Maximum events retained in the local buffer.
    pub capacity: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        WatchConfig {
            enabled: false,
            ngram_size: DEFAULT_NGRAM_SIZE,
            threshold: DEFAULT_THRESHOLD,
            capacity: DEFAULT_CAPACITY,
        }
    }
}

/// Status of an offer the detector has made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferStatus {
    Offered,
    Accepted,
    Dismissed,
}

/// What a caller feeds into [`crate::explore::ExploreLoop`] after the user
/// accepts a suggestion: a plain-language goal plus the redacted exemplar
/// steps of the repeated task. This module deliberately stops here -- it hands
/// off a seed rather than owning perception, planning, or execution -- so the
/// EXPLORE loop stays the single owner of supervised runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ExploreSeed {
    /// Plain-language goal for the supervised run.
    pub goal: String,
    /// The pattern this seed came from.
    pub pattern_digest: String,
    /// Redacted actions of the repeated task, in order.
    pub steps: Vec<Action>,
}

/// One offer the detector has surfaced, retained so it can be accepted or
/// dismissed by id later.
#[derive(Debug, Clone)]
struct Offer {
    pattern_digest: String,
    occurrences: u32,
    exemplar: Vec<Action>,
    status: OfferStatus,
}

/// The watch-and-suggest detector. Generic over its [`EventSink`] so a test can
/// inject a spy and prove the write path is never entered while the feature is
/// off.
#[derive(Debug)]
pub struct Watcher<S: EventSink> {
    config: WatchConfig,
    sink: S,
    detector: NgramDetector,
    offers: HashMap<String, Offer>,
}

impl Watcher<CappedBuffer> {
    /// Build a watcher backed by the default in-memory capped ring buffer,
    /// sized from `config.capacity`.
    pub fn capped(config: WatchConfig) -> Self {
        let sink = CappedBuffer::new(config.capacity);
        Watcher::new(config, sink)
    }
}

impl<S: EventSink> Watcher<S> {
    /// Build a watcher with a caller-supplied sink.
    pub fn new(config: WatchConfig, sink: S) -> Self {
        let detector = NgramDetector::new(config.ngram_size, config.threshold);
        Watcher {
            config,
            sink,
            detector,
            offers: HashMap::new(),
        }
    }

    /// Whether the feature is currently on.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Turn the feature on or off at runtime. Turning it off takes effect
    /// immediately: the very next [`Watcher::observe`] returns before the write
    /// path.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// The current configuration.
    pub fn config(&self) -> &WatchConfig {
        &self.config
    }

    /// Read-only view of the sink, e.g. to inspect buffered events or the
    /// lifetime write count.
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// The events currently held in the local buffer (redacted).
    pub fn buffered_events(&self) -> Vec<StoredEvent> {
        self.sink.events()
    }

    /// Observe one manual user action.
    ///
    /// **Off means off:** while the feature is disabled this returns
    /// immediately, before redaction and before the buffer's write path, so a
    /// disabled detector provably never writes. While enabled, the event is
    /// redacted, appended to the capped buffer, and fed to the n-gram detector;
    /// if that recognizes a newly-repeated pattern, a `suggestion.offered`
    /// event is published on `bus` and returned.
    pub fn observe(&mut self, bus: &Bus, event: &ManualEvent) -> Option<SuggestionOffered> {
        // HARD INVARIANT: the disabled feature must never touch the write path.
        // This early return is the single gate that makes that provable.
        if !self.config.enabled {
            return None;
        }

        let stored = normalize::redact_for_storage(event);
        // The one and only write path. Nothing in this module stores an event
        // except through the sink, and this line is only reachable when enabled.
        self.sink.write(stored.clone());

        let pattern = self.detector.observe(&stored)?;
        Some(self.publish_offer(bus, pattern))
    }

    /// Register a detected pattern as an open offer and publish
    /// `suggestion.offered`.
    fn publish_offer(&mut self, bus: &Bus, pattern: DetectedPattern) -> SuggestionOffered {
        let suggestion_id = suggestion_id_for(&pattern.pattern_digest);
        let occurrences = pattern.occurrences as u32;
        self.offers.insert(
            suggestion_id.clone(),
            Offer {
                pattern_digest: pattern.pattern_digest.clone(),
                occurrences,
                exemplar: pattern.exemplar,
                status: OfferStatus::Offered,
            },
        );
        let offered = SuggestionOffered {
            suggestion_id,
            pattern_digest: pattern.pattern_digest,
            occurrences,
        };
        // suggestion.* has no typed constructor in operant-core; publish the
        // raw payload under the exact contract topic.
        if let Ok(payload) = serde_json::to_value(&offered) {
            let _ = bus.publish(SUGGESTION_OFFERED, payload);
        }
        offered
    }

    /// The plain-language sentence for an open offer, for a caller rendering it
    /// to the user. `None` if the id is unknown.
    pub fn offer_text(&self, suggestion_id: &str) -> Option<String> {
        self.offers
            .get(suggestion_id)
            .map(|o| offer_sentence(o.occurrences))
    }

    /// Status of an offer by id, if known.
    pub fn offer_status(&self, suggestion_id: &str) -> Option<OfferStatus> {
        self.offers.get(suggestion_id).map(|o| o.status)
    }

    /// Accept an offer: publish `suggestion.accepted` and return the
    /// [`ExploreSeed`] the caller feeds to [`crate::explore::ExploreLoop`] to
    /// start a supervised run. `None` if the id is unknown. An offer can only
    /// be accepted once; a second accept returns `None`.
    pub fn accept(&mut self, bus: &Bus, suggestion_id: &str) -> Option<ExploreSeed> {
        let offer = self.offers.get_mut(suggestion_id)?;
        if offer.status != OfferStatus::Offered {
            return None;
        }
        offer.status = OfferStatus::Accepted;
        let seed = ExploreSeed {
            goal: seed_goal(offer.occurrences),
            pattern_digest: offer.pattern_digest.clone(),
            steps: offer.exemplar.clone(),
        };
        if let Ok(payload) = serde_json::to_value(SuggestionAccepted {
            suggestion_id: suggestion_id.to_string(),
        }) {
            let _ = bus.publish(SUGGESTION_ACCEPTED, payload);
        }
        Some(seed)
    }

    /// Dismiss an offer: publish `suggestion.dismissed`. Returns `true` if the
    /// id was a still-open offer. Dismissing latches it, so it cannot later be
    /// accepted.
    pub fn dismiss(&mut self, bus: &Bus, suggestion_id: &str) -> bool {
        let Some(offer) = self.offers.get_mut(suggestion_id) else {
            return false;
        };
        if offer.status != OfferStatus::Offered {
            return false;
        }
        offer.status = OfferStatus::Dismissed;
        if let Ok(payload) = serde_json::to_value(SuggestionDismissed {
            suggestion_id: suggestion_id.to_string(),
        }) {
            let _ = bus.publish(SUGGESTION_DISMISSED, payload);
        }
        true
    }

    /// The one-click purge: wipe every stored event and all in-flight detector
    /// and offer state. The sink's lifetime write count is deliberately NOT
    /// reset (see [`buffer::EventSink::total_writes`]).
    pub fn purge(&mut self) {
        self.sink.purge();
        self.detector.reset();
        self.offers.clear();
    }
}

/// Deterministic, human-recognizable suggestion id derived from the pattern
/// digest, so the same pattern always maps to the same id.
fn suggestion_id_for(pattern_digest: &str) -> String {
    let short: String = pattern_digest.chars().take(12).collect();
    format!("sug-{short}")
}

/// Plain-language goal for the supervised run seeded from an accepted offer.
fn seed_goal(occurrences: u32) -> String {
    format!("Repeat the task you just did {occurrences} times")
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{
        ActionKind, Element, Grounding, RiskClass, Role, Selector, Target,
    };

    fn on_config() -> WatchConfig {
        WatchConfig { enabled: true, ngram_size: 3, threshold: 4, capacity: 128 }
    }

    fn action(id: &str, kind: ActionKind, automation_id: &str) -> Action {
        Action {
            v: 1,
            id: id.to_string(),
            kind,
            intent: None,
            target: Some(Target {
                selectors: vec![Selector::AutomationId { value: automation_id.to_string() }],
                ..Default::default()
            }),
            params: serde_json::Map::new(),
            pace: Default::default(),
            risk_class: RiskClass::Write,
            irreversible: false,
            grounding: Grounding::Uia,
            timeout_ms: 5000,
            retry: Default::default(),
        }
    }

    /// The three-step task the fixture user repeats: click a field, type into
    /// it, save. Distinct automation ids keep the tokens stable and unique.
    fn task_step(step: usize) -> ManualEvent {
        match step {
            0 => ManualEvent::new(action("click", ActionKind::Click, "SubjectField")),
            1 => {
                let mut a = action("type", ActionKind::Type, "SubjectField");
                a.params.insert("text".into(), serde_json::json!("weekly status"));
                ManualEvent::new(a)
            }
            _ => {
                let mut a = action("save", ActionKind::Key, "Editor");
                a.params.insert("combo".into(), serde_json::json!("ctrl+s"));
                ManualEvent::new(a)
            }
        }
    }

    /// A spy-backed watcher, so tests can assert on the write path directly.
    fn spy_watcher(config: WatchConfig) -> Watcher<SpySink> {
        Watcher::new(config, SpySink::new())
    }

    // ---- off means off: provably never writes ----------------------------

    #[test]
    fn disabled_watcher_never_writes() {
        let bus = Bus::new();
        // Default config is OFF.
        let mut w = spy_watcher(WatchConfig::default());
        assert!(!w.is_enabled(), "the feature must default to OFF");

        // Feed a long stream that WOULD trigger an offer if it were on.
        for _ in 0..10 {
            for step in 0..3 {
                let offer = w.observe(&bus, &task_step(step));
                assert!(offer.is_none(), "a disabled watcher must never offer");
            }
        }

        // The proof: the write path was entered zero times, not merely that the
        // buffer reads empty.
        assert_eq!(w.sink().total_writes(), 0, "disabled feature must never write");
        assert!(w.sink().is_empty());
        assert!(w.buffered_events().is_empty());
    }

    #[test]
    fn toggling_off_after_on_stops_writes_immediately() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        w.observe(&bus, &task_step(0));
        let after_one = w.sink().total_writes();
        assert_eq!(after_one, 1);

        w.set_enabled(false);
        for step in 0..3 {
            w.observe(&bus, &task_step(step));
        }
        assert_eq!(
            w.sink().total_writes(),
            after_one,
            "no further writes once toggled off"
        );
    }

    // ---- exactly one offer for a planted 4x pattern ----------------------

    #[test]
    fn planted_four_times_pattern_triggers_exactly_one_offer() {
        let bus = Bus::new();
        let sub = bus.subscribe("suggestion.*");
        let mut w = spy_watcher(on_config());

        let mut offers = Vec::new();
        // Repeat the 3-step task four times.
        for _ in 0..4 {
            for step in 0..3 {
                if let Some(o) = w.observe(&bus, &task_step(step)) {
                    offers.push(o);
                }
            }
        }

        assert_eq!(offers.len(), 1, "exactly one offer for a 4x pattern (not zero, not per-repeat)");
        assert_eq!(offers[0].occurrences, 4);

        // Exactly one suggestion.offered on the bus.
        let published: Vec<_> = sub.rx.try_iter().collect();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].topic, SUGGESTION_OFFERED);
    }

    #[test]
    fn pattern_repeating_past_threshold_still_offers_only_once() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        let mut offers = 0;
        for _ in 0..7 {
            for step in 0..3 {
                if w.observe(&bus, &task_step(step)).is_some() {
                    offers += 1;
                }
            }
        }
        assert_eq!(offers, 1, "a pattern seen seven times still offers once");
    }

    #[test]
    fn fewer_than_threshold_never_offers() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        let mut offered = false;
        for _ in 0..3 {
            for step in 0..3 {
                if w.observe(&bus, &task_step(step)).is_some() {
                    offered = true;
                }
            }
        }
        assert!(!offered, "three repetitions is below the threshold of four");
    }

    // ---- purge empties the buffer, subsequent read confirms --------------

    #[test]
    fn purge_empties_the_buffer_and_a_read_confirms() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        for step in 0..3 {
            w.observe(&bus, &task_step(step));
        }
        assert!(!w.buffered_events().is_empty(), "buffer has events before purge");

        w.purge();

        // A subsequent read confirms it is empty.
        assert!(w.buffered_events().is_empty(), "purge empties the buffer");
        assert!(w.sink().is_empty());
        // Purge does not erase the proof that writes happened.
        assert_eq!(w.sink().total_writes(), 3);
    }

    #[test]
    fn purge_resets_detection_so_the_count_restarts() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        // Three of four repetitions, then purge: the count must restart, so a
        // further single repetition does NOT reach the threshold.
        for _ in 0..3 {
            for step in 0..3 {
                w.observe(&bus, &task_step(step));
            }
        }
        w.purge();
        let mut offered = false;
        for step in 0..3 {
            if w.observe(&bus, &task_step(step)).is_some() {
                offered = true;
            }
        }
        assert!(!offered, "purge resets in-flight pattern counts");
    }

    // ---- redaction before storage ----------------------------------------

    #[test]
    fn credential_content_is_redacted_before_it_reaches_the_buffer() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());

        let password_field = Element {
            idx: 1,
            parent: None,
            role: Role::Edit,
            name: "Password".to_string(),
            value: None,
            automation_id: Some("PwdBox".to_string()),
            bounds: None,
            enabled: true,
            offscreen: false,
            is_password: true,
            patterns: vec![],
            selectors: vec![],
        };
        let mut typing = action("type", ActionKind::Type, "PwdBox");
        typing.params.insert("text".into(), serde_json::json!("hunter2"));
        let event = ManualEvent::with_target(typing, password_field);

        w.observe(&bus, &event);

        let stored = w.buffered_events();
        assert_eq!(stored.len(), 1);
        let dump = format!("{:?}", stored[0]);
        assert!(!dump.contains("hunter2"), "credential text must never reach the buffer");
    }

    // ---- accept seeds a supervised run -----------------------------------

    #[test]
    fn accepting_an_offer_yields_a_seed_and_publishes_accepted() {
        let bus = Bus::new();
        let sub = bus.subscribe("suggestion.*");
        let mut w = spy_watcher(on_config());

        let mut suggestion_id = None;
        for _ in 0..4 {
            for step in 0..3 {
                if let Some(o) = w.observe(&bus, &task_step(step)) {
                    suggestion_id = Some(o.suggestion_id);
                }
            }
        }
        let id = suggestion_id.expect("a suggestion was offered");
        assert_eq!(w.offer_status(&id), Some(OfferStatus::Offered));
        assert_eq!(
            w.offer_text(&id).as_deref(),
            Some("You have done this 4 times. Want me to learn it?")
        );

        let seed = w.accept(&bus, &id).expect("offer accepts");
        assert_eq!(seed.steps.len(), 3, "seed carries the three exemplar steps");
        assert!(seed.goal.contains("4 times"));
        assert_eq!(w.offer_status(&id), Some(OfferStatus::Accepted));

        // A second accept is refused.
        assert!(w.accept(&bus, &id).is_none());

        // suggestion.offered then suggestion.accepted were both published.
        let topics: Vec<_> = sub.rx.try_iter().map(|e| e.topic).collect();
        assert!(topics.contains(&SUGGESTION_OFFERED.to_string()));
        assert!(topics.contains(&SUGGESTION_ACCEPTED.to_string()));
    }

    #[test]
    fn dismissing_an_offer_latches_it() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        let mut id = None;
        for _ in 0..4 {
            for step in 0..3 {
                if let Some(o) = w.observe(&bus, &task_step(step)) {
                    id = Some(o.suggestion_id);
                }
            }
        }
        let id = id.unwrap();
        assert!(w.dismiss(&bus, &id));
        assert_eq!(w.offer_status(&id), Some(OfferStatus::Dismissed));
        // Cannot accept a dismissed offer, and cannot dismiss twice.
        assert!(w.accept(&bus, &id).is_none());
        assert!(!w.dismiss(&bus, &id));
    }

    #[test]
    fn capped_watcher_uses_a_bounded_buffer() {
        let bus = Bus::new();
        let mut w = Watcher::capped(WatchConfig { enabled: true, capacity: 4, ..on_config() });
        // A different-shaped stream so no offer fires; just fill past capacity.
        for i in 0..10 {
            let mut a = action(&format!("k{i}"), ActionKind::Key, &format!("Btn{i}"));
            a.params.insert("combo".into(), serde_json::json!(format!("ctrl+{i}")));
            w.observe(&bus, &ManualEvent::new(a));
        }
        assert_eq!(w.buffered_events().len(), 4, "buffer is capped");
        assert_eq!(w.sink().total_writes(), 10, "all writes counted");
    }

    #[test]
    fn unknown_suggestion_id_is_a_no_op() {
        let bus = Bus::new();
        let mut w = spy_watcher(on_config());
        assert!(w.accept(&bus, "sug-nope").is_none());
        assert!(!w.dismiss(&bus, "sug-nope"));
        assert!(w.offer_text("sug-nope").is_none());
        assert!(w.offer_status("sug-nope").is_none());
    }
}
