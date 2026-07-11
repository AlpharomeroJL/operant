//! Fixture-mode vision grounder.
//!
//! Given a flat list of perceived elements (a caller-supplied subset of a
//! perception snapshot) and a target hint, returns a deterministic click
//! point plus a "cropped anchor capture": a small, fixed-size rectangle
//! around that point, content-addressed by a BLAKE3 hash, so a later
//! replay can re-verify it found the same visual target. No GPU, no real
//! image pipeline, no model call: FIXTURE MODE is the only mode this crate
//! implements.
//!
//! ## Why this is not `crates/orchestrator/src/backends/grounder.rs`
//!
//! `operant-orchestrator` has its own `FixtureGrounderBackend` solving the
//! same problem, on purpose, as separate code rather than a shared
//! dependency:
//!
//! - This crate is a **sidecar**: a separate process a supervisor spawns
//!   (`operant_core::supervisor::Child`, "implementations wrap a real OS
//!   process in later lanes"), talking a small stdio JSON protocol
//!   (`src/main.rs`). Real (non-fixture) vision grounding would run here,
//!   isolated from the main process, with its own resource/GPU lifecycle.
//! - It is detached from the main Cargo workspace (see `Cargo.toml`) and
//!   deliberately depends on nothing from `operant-ir`, so it can be
//!   rebuilt, redeployed, or even reimplemented in a different language
//!   without touching the orchestrator crate at all.
//! - `operant-orchestrator`'s `FixtureGrounderBackend` is the in-process
//!   `ModelBackend`-shaped fallback the explore loop and tests use today,
//!   over `operant_ir::Snapshot` directly (no process, no serialization).
//!
//! Both implement the same deterministic contract: the same snapshot
//! digest, element list, and target hint always resolve to the same point
//! and the same anchor hash.

use serde::{Deserialize, Serialize};

/// Deterministic tolerance every fixture-mode anchor is stamped with. Real
/// (non-fixture) grounding would derive this from the vision model's own
/// reported confidence; fixture mode has no model, so it is a constant.
const FIXTURE_TOLERANCE: f64 = 0.15;

/// Half-width/height cap (px) of the deterministic crop around a matched
/// element's center, so the capture stays small even for a large element.
const MAX_CROP_HALF_EXTENT: f64 = 32.0;

/// One candidate element a caller wants matched against, e.g. one row from
/// a UIA perception snapshot. Deliberately smaller than the full
/// `contracts/perception_snapshot.schema.json` element shape: this crate
/// only needs enough to pick a point and a crop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementCandidate {
    pub idx: u32,
    pub name: String,
    #[serde(default)]
    pub bounds: Option<BoundsIn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundsIn {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One grounding request: what to find, and where it might be. A caller
/// serializes this to send over the sidecar's stdin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundRequest {
    /// Identifies the snapshot this element list came from; folded into
    /// the anchor hash so two snapshots never collide onto one anchor.
    pub snapshot_digest: String,
    pub target_hint: String,
    pub elements: Vec<ElementCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CropOut {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One grounding result. The sidecar serializes this to stdout; a caller
/// deserializes it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundResponse {
    pub x: f64,
    pub y: f64,
    pub confidence: f32,
    pub anchor_hash: String,
    pub tolerance: f64,
    pub crop: CropOut,
    pub matched_idx: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundError {
    EmptyHint,
    NoMatch(String),
    NoBounds(String),
}

impl std::fmt::Display for GroundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroundError::EmptyHint => write!(f, "target hint is empty"),
            GroundError::NoMatch(hint) => write!(f, "no element matches `{hint}`"),
            GroundError::NoBounds(name) => write!(f, "element `{name}` has no bounds to click"),
        }
    }
}

impl std::error::Error for GroundError {}

/// Find `request.target_hint` among `request.elements` and return a
/// deterministic ground result. Matching is a case-insensitive substring
/// match against each element's `name`, preferring an exact
/// (case-insensitive) match over a partial one, and the lowest `idx` on
/// ties: the same request, run any number of times, in any process,
/// resolves to the same element, the same point, and the same anchor hash.
pub fn ground(request: &GroundRequest) -> Result<GroundResponse, GroundError> {
    let hint = request.target_hint.trim().to_ascii_lowercase();
    if hint.is_empty() {
        return Err(GroundError::EmptyHint);
    }

    let best = request
        .elements
        .iter()
        .filter(|e| e.bounds.is_some() && e.name.to_ascii_lowercase().contains(&hint))
        .min_by_key(|e| (e.name.to_ascii_lowercase() != hint, e.idx))
        .ok_or_else(|| GroundError::NoMatch(request.target_hint.clone()))?;

    let bounds = best
        .bounds
        .ok_or_else(|| GroundError::NoBounds(best.name.clone()))?;
    let crop = crop_region(bounds);
    let anchor_hash = anchor_hash(&request.snapshot_digest, best.idx, &crop);
    let confidence = if best.name.to_ascii_lowercase() == hint {
        1.0
    } else {
        0.75
    };

    Ok(GroundResponse {
        x: bounds.x + bounds.w / 2.0,
        y: bounds.y + bounds.h / 2.0,
        confidence,
        anchor_hash,
        tolerance: FIXTURE_TOLERANCE,
        crop,
        matched_idx: best.idx,
    })
}

fn crop_region(bounds: BoundsIn) -> CropOut {
    let half_w = (bounds.w / 2.0).min(MAX_CROP_HALF_EXTENT);
    let half_h = (bounds.h / 2.0).min(MAX_CROP_HALF_EXTENT);
    let cx = bounds.x + bounds.w / 2.0;
    let cy = bounds.y + bounds.h / 2.0;
    CropOut {
        x: cx - half_w,
        y: cy - half_h,
        w: half_w * 2.0,
        h: half_h * 2.0,
    }
}

fn anchor_hash(snapshot_digest: &str, element_idx: u32, crop: &CropOut) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(snapshot_digest.as_bytes());
    hasher.update(&element_idx.to_le_bytes());
    for v in [crop.x, crop.y, crop.w, crop.h] {
        hasher.update(&v.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(hint: &str) -> GroundRequest {
        GroundRequest {
            snapshot_digest: "d0d0d0".to_string(),
            target_hint: hint.to_string(),
            elements: vec![
                ElementCandidate {
                    idx: 0,
                    name: "Untitled - Notepad".to_string(),
                    bounds: Some(BoundsIn {
                        x: 100.0,
                        y: 100.0,
                        w: 1200.0,
                        h: 800.0,
                    }),
                },
                ElementCandidate {
                    idx: 5,
                    name: "Text editor".to_string(),
                    bounds: Some(BoundsIn {
                        x: 100.0,
                        y: 156.0,
                        w: 1200.0,
                        h: 716.0,
                    }),
                },
                ElementCandidate {
                    idx: 6,
                    name: "Status bar".to_string(),
                    bounds: Some(BoundsIn {
                        x: 100.0,
                        y: 872.0,
                        w: 1200.0,
                        h: 28.0,
                    }),
                },
                ElementCandidate {
                    idx: 7,
                    name: "no bounds here".to_string(),
                    bounds: None,
                },
            ],
        }
    }

    #[test]
    fn ground_is_deterministic_across_repeated_calls() {
        let req = sample_request("Text editor");
        let a = ground(&req).unwrap();
        let b = ground(&req).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ground_picks_the_element_center() {
        let g = ground(&sample_request("Text editor")).unwrap();
        assert_eq!(g.x, 100.0 + 1200.0 / 2.0);
        assert_eq!(g.y, 156.0 + 716.0 / 2.0);
        assert_eq!(g.matched_idx, 5);
        assert_eq!(g.confidence, 1.0);
    }

    #[test]
    fn ground_prefers_exact_match_over_substring_match() {
        let exact = ground(&sample_request("status bar")).unwrap();
        let partial = ground(&sample_request("Status")).unwrap();
        assert_eq!(exact.matched_idx, partial.matched_idx);
        assert!(partial.confidence < exact.confidence);
    }

    #[test]
    fn ground_crop_is_capped_and_anchors_differ_per_element() {
        let editor = ground(&sample_request("Text editor")).unwrap();
        let statusbar = ground(&sample_request("Status bar")).unwrap();
        assert!(editor.crop.w <= MAX_CROP_HALF_EXTENT * 2.0);
        assert!(editor.crop.h <= MAX_CROP_HALF_EXTENT * 2.0);
        assert_ne!(editor.anchor_hash, statusbar.anchor_hash);
    }

    #[test]
    fn ground_rejects_empty_hint() {
        assert_eq!(
            ground(&sample_request("")).unwrap_err(),
            GroundError::EmptyHint
        );
        assert_eq!(
            ground(&sample_request("   ")).unwrap_err(),
            GroundError::EmptyHint
        );
    }

    #[test]
    fn ground_reports_no_match_for_an_absent_target() {
        assert_eq!(
            ground(&sample_request("save dialog")).unwrap_err(),
            GroundError::NoMatch("save dialog".to_string())
        );
    }

    #[test]
    fn ground_reports_no_bounds_rather_than_matching_an_unclickable_element() {
        // The only element containing "no bounds" has bounds: None, so it
        // must be filtered out rather than returned as an un-clickable hit.
        assert_eq!(
            ground(&sample_request("no bounds")).unwrap_err(),
            GroundError::NoMatch("no bounds".to_string())
        );
    }

    #[test]
    fn different_snapshot_digest_changes_the_anchor_hash() {
        let mut req_a = sample_request("Text editor");
        let mut req_b = req_a.clone();
        req_a.snapshot_digest = "aaaa".to_string();
        req_b.snapshot_digest = "bbbb".to_string();
        assert_ne!(
            ground(&req_a).unwrap().anchor_hash,
            ground(&req_b).unwrap().anchor_hash
        );
    }

    #[test]
    fn ground_request_and_response_round_trip_through_json() {
        let req = sample_request("Text editor");
        let json = serde_json::to_string(&req).unwrap();
        let back: GroundRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);

        let response = ground(&req).unwrap();
        let response_json = serde_json::to_string(&response).unwrap();
        assert!(response_json.contains("anchor_hash"));
    }
}
