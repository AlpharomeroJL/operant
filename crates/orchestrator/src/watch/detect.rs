//! N-gram repetition detection over the normalized token stream.
//!
//! The detector keeps a sliding window of the last `ngram_size` normalized
//! tokens. Every time the window is full it counts that exact token sequence.
//! When a sequence reaches `threshold` occurrences it fires -- but at most once
//! per *cyclic task*.
//!
//! The cyclic-task rule matters because a periodic stream `A B C A B C ...` is,
//! read through a sliding window, simultaneously a run of `A B C`, of `B C A`,
//! and of `C A B`. Those are rotations of the same repeated loop, not three
//! separate habits, so the detector latches on a rotation-invariant key
//! (`canonical rotation`) and offers the loop exactly once, no matter which
//! phase the window happened to be in. Occurrence counts stay per-exact-sequence
//! so "you have done this N times" reflects aligned repetitions of the task, not
//! the larger count of overlapping sub-windows.
//!
//! Matching is on the content-free tokens from [`super::normalize`], so a
//! detected pattern is a repeated *shape* of user actions and can never encode
//! typed text.

use std::collections::{HashMap, HashSet, VecDeque};

use operant_ir::Action;

use super::event::StoredEvent;

/// A repeated pattern the detector has just recognized for the first time.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedPattern {
    /// Rotation-invariant digest of the token sequence; the `pattern_digest`
    /// published on `suggestion.offered`. Stable across which phase of the
    /// loop the window was in when it fired.
    pub pattern_digest: String,
    /// How many aligned repetitions of the exact sequence had occurred when it
    /// fired (equals the threshold: the detector fires the instant the count
    /// reaches it).
    pub occurrences: usize,
    /// The normalized tokens of the firing window, in order.
    pub tokens: Vec<String>,
    /// The redacted actions of the firing window, kept so an accepted
    /// suggestion can seed a supervised run.
    pub exemplar: Vec<Action>,
}

/// Sliding-window n-gram counter with a one-offer-per-cyclic-task latch.
#[derive(Debug)]
pub struct NgramDetector {
    ngram_size: usize,
    threshold: usize,
    window: VecDeque<StoredEvent>,
    /// Occurrence count per exact token sequence (keyed by its ordered digest).
    counts: HashMap<String, usize>,
    /// Canonical-rotation digests already offered, so rotations of an
    /// already-offered loop are suppressed.
    offered: HashSet<String>,
}

impl NgramDetector {
    /// Build a detector matching windows of `ngram_size` tokens, firing at
    /// `threshold` occurrences. Both are clamped to at least 1 so a
    /// misconfiguration cannot produce a zero-length window or a pattern that
    /// fires before it has occurred.
    pub fn new(ngram_size: usize, threshold: usize) -> Self {
        NgramDetector {
            ngram_size: ngram_size.max(1),
            threshold: threshold.max(1),
            window: VecDeque::new(),
            counts: HashMap::new(),
            offered: HashSet::new(),
        }
    }

    /// Feed one stored event. Returns `Some(pattern)` at most once per cyclic
    /// task, on the event that pushes an exact sequence's count up to the
    /// threshold (unless a rotation of that loop was already offered).
    pub fn observe(&mut self, event: &StoredEvent) -> Option<DetectedPattern> {
        self.window.push_back(event.clone());
        if self.window.len() > self.ngram_size {
            self.window.pop_front();
        }
        if self.window.len() < self.ngram_size {
            return None;
        }

        let tokens: Vec<String> = self.window.iter().map(|e| e.token.clone()).collect();
        let ordered_digest = digest(&tokens);
        let canonical_digest = digest(&canonical_rotation(&tokens));

        let count = self.counts.entry(ordered_digest).or_insert(0);
        *count += 1;
        let count = *count;

        if count >= self.threshold && !self.offered.contains(&canonical_digest) {
            self.offered.insert(canonical_digest.clone());
            return Some(DetectedPattern {
                pattern_digest: canonical_digest,
                occurrences: count,
                tokens,
                exemplar: self.window.iter().map(|e| e.action.clone()).collect(),
            });
        }
        None
    }

    /// Drop all sliding-window, count, and offer state. Called on purge so a
    /// purge truly forgets everything, including in-flight counts, not just the
    /// stored events.
    pub fn reset(&mut self) {
        self.window.clear();
        self.counts.clear();
        self.offered.clear();
    }
}

/// The lexicographically smallest rotation of a token sequence, so every phase
/// of a cyclic loop maps to one canonical key.
fn canonical_rotation(tokens: &[String]) -> Vec<String> {
    let n = tokens.len();
    if n <= 1 {
        return tokens.to_vec();
    }
    let mut best: Option<Vec<String>> = None;
    for start in 0..n {
        let rotation: Vec<String> =
            (0..n).map(|i| tokens[(start + i) % n].clone()).collect();
        match &best {
            Some(current) if *current <= rotation => {}
            _ => best = Some(rotation),
        }
    }
    best.unwrap_or_else(|| tokens.to_vec())
}

/// Hash a token sequence into a stable hex digest. A unit separator that
/// cannot appear inside a token keeps `["ab", "c"]` distinct from
/// `["a", "bc"]`.
fn digest(tokens: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for token in tokens {
        hasher.update(token.as_bytes());
        hasher.update(&[0x1f]); // ASCII unit separator, never present in a token
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{ActionKind, Grounding, RiskClass};

    fn stored(token: &str) -> StoredEvent {
        StoredEvent {
            token: token.to_string(),
            action: Action {
                v: 1,
                id: token.to_string(),
                kind: ActionKind::Key,
                intent: None,
                target: None,
                params: serde_json::Map::new(),
                pace: Default::default(),
                risk_class: RiskClass::Read,
                irreversible: false,
                grounding: Grounding::Uia,
                timeout_ms: 5000,
                retry: Default::default(),
            },
        }
    }

    /// Feed a 3-token pattern repeated four times; expect exactly one fire, on
    /// the fourth aligned occurrence.
    #[test]
    fn fires_once_at_threshold_for_a_repeated_ngram() {
        let mut d = NgramDetector::new(3, 4);
        let pattern = ["a", "b", "c"];
        let mut fires = Vec::new();
        for rep in 0..4 {
            for tok in pattern {
                if let Some(p) = d.observe(&stored(tok)) {
                    fires.push((rep, p));
                }
            }
        }
        assert_eq!(fires.len(), 1, "exactly one offer for a 4x pattern");
        assert_eq!(fires[0].1.occurrences, 4);
        assert_eq!(fires[0].1.tokens, vec!["a", "b", "c"]);
    }

    #[test]
    fn does_not_fire_again_past_the_threshold() {
        let mut d = NgramDetector::new(2, 3);
        let mut fire_count = 0;
        // "x y" repeated six times. Read through the window this is also a run
        // of "y x": both are rotations of one loop, so it must offer only once.
        for _ in 0..6 {
            if d.observe(&stored("x")).is_some() {
                fire_count += 1;
            }
            if d.observe(&stored("y")).is_some() {
                fire_count += 1;
            }
        }
        assert_eq!(fire_count, 1, "a cyclic pattern past threshold offers only once");
    }

    #[test]
    fn rotations_of_a_periodic_stream_offer_only_once() {
        // A B C repeated seven times: the window sees A B C, B C A, and C A B
        // all cross the threshold, but they are one loop and must offer once.
        let mut d = NgramDetector::new(3, 4);
        let mut fires = 0;
        for _ in 0..7 {
            for tok in ["a", "b", "c"] {
                if d.observe(&stored(tok)).is_some() {
                    fires += 1;
                }
            }
        }
        assert_eq!(fires, 1);
    }

    #[test]
    fn does_not_fire_below_threshold() {
        let mut d = NgramDetector::new(2, 4);
        // "x y" only three times: below the threshold of 4, never fires.
        for _ in 0..3 {
            assert!(d.observe(&stored("x")).is_none());
            assert!(d.observe(&stored("y")).is_none());
        }
    }

    #[test]
    fn distinct_loops_each_offer_once() {
        // Two genuinely different loops (not rotations of each other) should
        // each get their own offer.
        let mut d = NgramDetector::new(2, 3);
        let mut fires = 0;
        for _ in 0..3 {
            for tok in ["a", "b"] {
                if d.observe(&stored(tok)).is_some() {
                    fires += 1;
                }
            }
        }
        // Reset the window boundary with a filler unrelated token run.
        for _ in 0..3 {
            for tok in ["p", "q"] {
                if d.observe(&stored(tok)).is_some() {
                    fires += 1;
                }
            }
        }
        assert_eq!(fires, 2, "two distinct loops offer twice");
    }

    #[test]
    fn reset_forgets_counts_and_offers() {
        let mut d = NgramDetector::new(2, 2);
        d.observe(&stored("x"));
        d.observe(&stored("y"));
        d.reset();
        // After reset the pattern must climb from zero again.
        assert!(d.observe(&stored("x")).is_none());
        assert!(d.observe(&stored("y")).is_none());
    }

    #[test]
    fn distinct_sequences_have_distinct_digests() {
        assert_ne!(
            digest(&["ab".into(), "c".into()]),
            digest(&["a".into(), "bc".into()])
        );
    }

    #[test]
    fn canonical_rotation_is_phase_invariant() {
        let abc = canonical_rotation(&["a".into(), "b".into(), "c".into()]);
        let bca = canonical_rotation(&["b".into(), "c".into(), "a".into()]);
        let cab = canonical_rotation(&["c".into(), "a".into(), "b".into()]);
        assert_eq!(abc, bca);
        assert_eq!(bca, cab);
        assert_eq!(abc, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
