//! `mock_grounder`: fixture-mode vision grounding. Deterministic
//! coordinates plus a "cropped anchor capture", with no GPU and no real
//! image pipeline, so replay and tests never depend on either. Contract
//! hard rule #4: "Mock backends for CI live behind the same trait: ...
//! `mock_grounder` (fixture-deterministic coordinates)."
//!
//! This mirrors the fixture-mode contract implemented standalone (and
//! process-isolated) in `sidecars/vision`. The two are intentionally
//! separate code, not a shared dependency: `sidecars/vision` is a
//! detached, minimal-dependency crate any future sidecar-supervisor
//! integration can spawn as its own process (see
//! `operant_core::supervisor::Child`, "implementations wrap a real OS
//! process in later lanes"), while this module is the in-process,
//! `ModelBackend`-shaped fallback the explore loop and tests use today.
//! Both implement the same deterministic contract: same snapshot plus same
//! target hint always yields the same coordinates and anchor hash.

use blake3::Hasher;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::FutureExt;
use operant_ir::{Anchor, Bounds, Coords, Element, Snapshot};

use super::probe::now_rfc3339;
use super::{BackendError, BackendEvent, BackendProfile, CompletionRequest, ModelBackend, Usage};

/// Deterministic tolerance every fixture-mode anchor is stamped with. Real
/// (non-fixture) grounding would derive this from the vision model's own
/// reported confidence; fixture mode has no model, so it is a constant.
const FIXTURE_TOLERANCE: f64 = 0.15;

/// Half-width/height cap (px) of the deterministic crop around a matched
/// element's center, so the "capture" stays small even for large elements.
const MAX_CROP_HALF_EXTENT: f64 = 32.0;

/// A deterministic, fixed-size crop rectangle used as the anchor capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One grounded result: where to click, and a content-addressed anchor a
/// future replay can use to re-verify the same visual target.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundResult {
    pub coords: Coords,
    pub anchor: Anchor,
    pub crop: CropRegion,
    pub confidence: f32,
    pub matched_element_idx: u32,
}

/// Find `target_hint` in `snapshot` and return a deterministic ground
/// result. Matching is a case-insensitive substring match against each
/// element's `name`, preferring an exact (case-insensitive) match over a
/// partial one, and the lowest element index on ties, so the same snapshot
/// and hint always resolve to the same element no matter how many times
/// (or in what order) this runs: no randomness, no wall-clock, no model.
pub fn ground_fixture(
    snapshot: &Snapshot,
    target_hint: &str,
) -> Result<GroundResult, BackendError> {
    let hint = target_hint.trim().to_ascii_lowercase();
    if hint.is_empty() {
        return Err(BackendError::new(
            "grounder_empty_hint",
            "target hint is empty",
            false,
        ));
    }

    let best = best_match(snapshot, &hint).ok_or_else(|| {
        BackendError::new(
            "grounder_no_match",
            format!("no element in the snapshot matches `{target_hint}`"),
            false,
        )
    })?;
    let bounds = best.bounds.as_ref().ok_or_else(|| {
        BackendError::new(
            "grounder_no_bounds",
            format!("element `{}` has no bounds to click", best.name),
            false,
        )
    })?;

    let crop = crop_region(bounds);
    let img_hash = anchor_hash(&snapshot.digest, best.idx, &crop);
    let confidence = if best.name.to_ascii_lowercase() == hint {
        1.0
    } else {
        0.75
    };

    Ok(GroundResult {
        coords: Coords {
            x: bounds.x + bounds.w / 2.0,
            y: bounds.y + bounds.h / 2.0,
            monitor: bounds.monitor.clone(),
            dpi_scale: Some(snapshot.window.dpi_scale),
        },
        anchor: Anchor {
            img_hash,
            tolerance: FIXTURE_TOLERANCE,
        },
        crop,
        confidence,
        matched_element_idx: best.idx,
    })
}

fn best_match<'a>(snapshot: &'a Snapshot, hint: &str) -> Option<&'a Element> {
    snapshot
        .elements
        .iter()
        .filter(|e| e.bounds.is_some() && e.name.to_ascii_lowercase().contains(hint))
        .min_by_key(|e| (e.name.to_ascii_lowercase() != hint, e.idx))
}

/// A deterministic, fixed-size crop rectangle around an element's center,
/// capped at [`MAX_CROP_HALF_EXTENT`] so even a full-window match yields a
/// small capture, and never wider than the element itself.
fn crop_region(bounds: &Bounds) -> CropRegion {
    let half_w = (bounds.w / 2.0).min(MAX_CROP_HALF_EXTENT);
    let half_h = (bounds.h / 2.0).min(MAX_CROP_HALF_EXTENT);
    let cx = bounds.x + bounds.w / 2.0;
    let cy = bounds.y + bounds.h / 2.0;
    CropRegion {
        x: cx - half_w,
        y: cy - half_h,
        w: half_w * 2.0,
        h: half_h * 2.0,
    }
}

/// Content-address the crop: a BLAKE3 hash over the snapshot's own digest,
/// the matched element index, and the crop rectangle. Deterministic given
/// the same inputs; changes if the snapshot, the matched element, or the
/// crop geometry changes, which is exactly what an anchor hash is for.
fn anchor_hash(snapshot_digest: &str, element_idx: u32, crop: &CropRegion) -> String {
    let mut hasher = Hasher::new();
    hasher.update(snapshot_digest.as_bytes());
    hasher.update(&element_idx.to_le_bytes());
    for v in [crop.x, crop.y, crop.w, crop.h] {
        hasher.update(&v.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// [`ModelBackend`] wrapper around [`ground_fixture`], so fixture-mode
/// grounding is reachable through the same trait as every real vision
/// backend. `complete` treats the request's concatenated text content as
/// the target hint (the wire contract has no separate "target" field), and
/// reports the ground result as a single `ground` tool call.
pub struct FixtureGrounderBackend {
    id: String,
    snapshot: Snapshot,
}

impl FixtureGrounderBackend {
    pub fn new(id: impl Into<String>, snapshot: Snapshot) -> Self {
        Self {
            id: id.into(),
            snapshot,
        }
    }
}

impl ModelBackend for FixtureGrounderBackend {
    fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent> {
        let hint = request.concat_text();
        let events = match ground_fixture(&self.snapshot, &hint) {
            Ok(g) => vec![
                BackendEvent::ToolCall {
                    id: "ground_1".to_string(),
                    name: "ground".to_string(),
                    arguments: serde_json::json!({
                        "x": g.coords.x,
                        "y": g.coords.y,
                        "anchor_hash": g.anchor.img_hash,
                        "tolerance": g.anchor.tolerance,
                        "confidence": g.confidence,
                    }),
                },
                BackendEvent::Done {
                    usage: Usage::default(),
                },
            ],
            Err(e) => vec![BackendEvent::Error {
                error_id: e.error_id,
                message: e.message,
                retryable: e.retryable,
            }],
        };
        stream::iter(events).boxed()
    }

    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>> {
        let id = self.id.clone();
        async move {
            Ok(BackendProfile {
                backend_id: id,
                vision: true,
                tool_use: true,
                context_length: super::probe::DEFAULT_CONTEXT_LENGTH,
                streaming: true,
                probed_at: now_rfc3339(),
            })
        }
        .boxed()
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::backends::types::RequestRole;

    fn notepad_snapshot() -> Snapshot {
        let raw = include_str!("../../../../contracts/fixtures/snapshot_notepad.json");
        serde_json::from_str(raw).expect("shared notepad fixture parses as operant_ir::Snapshot")
    }

    #[test]
    fn ground_fixture_is_deterministic_across_repeated_calls() {
        let snap = notepad_snapshot();
        let a = ground_fixture(&snap, "Text editor").unwrap();
        let b = ground_fixture(&snap, "Text editor").unwrap();
        assert_eq!(
            a, b,
            "same snapshot and hint must yield byte-identical results every time"
        );
    }

    #[test]
    fn ground_fixture_finds_the_text_editor_at_its_bounds_center() {
        let snap = notepad_snapshot();
        let g = ground_fixture(&snap, "Text editor").unwrap();
        // From contracts/fixtures/snapshot_notepad.json: bounds x=100 y=156 w=1200 h=716.
        assert_eq!(g.coords.x, 100.0 + 1200.0 / 2.0);
        assert_eq!(g.coords.y, 156.0 + 716.0 / 2.0);
        assert_eq!(g.confidence, 1.0, "exact name match");
        assert_eq!(g.matched_element_idx, 5);
    }

    #[test]
    fn ground_fixture_matches_case_insensitively_and_by_substring() {
        let snap = notepad_snapshot();
        let exact = ground_fixture(&snap, "text editor").unwrap();
        let partial = ground_fixture(&snap, "Editor").unwrap();
        assert_eq!(exact.matched_element_idx, partial.matched_element_idx);
        assert!(
            partial.confidence < exact.confidence,
            "substring match is less confident than exact match"
        );
    }

    #[test]
    fn ground_fixture_crop_stays_within_max_extent_and_anchors_differ_by_element() {
        let snap = notepad_snapshot();
        let editor = ground_fixture(&snap, "Text editor").unwrap();
        let statusbar = ground_fixture(&snap, "Status bar").unwrap();

        assert!(editor.crop.w <= MAX_CROP_HALF_EXTENT * 2.0);
        assert!(editor.crop.h <= MAX_CROP_HALF_EXTENT * 2.0);
        assert_ne!(
            editor.anchor.img_hash, statusbar.anchor.img_hash,
            "different elements must not collide onto the same anchor hash"
        );
    }

    #[test]
    fn ground_fixture_errors_on_empty_hint_and_on_no_match() {
        let snap = notepad_snapshot();
        assert_eq!(
            ground_fixture(&snap, "").unwrap_err().error_id,
            "grounder_empty_hint"
        );
        assert_eq!(
            ground_fixture(&snap, "a button that does not exist")
                .unwrap_err()
                .error_id,
            "grounder_no_match"
        );
    }

    #[tokio::test]
    async fn fixture_grounder_backend_reports_a_ground_tool_call() {
        let backend = FixtureGrounderBackend::new("mock_grounder", notepad_snapshot());
        let request = CompletionRequest::text(RequestRole::Grounder, "Text editor", 8);
        let events: Vec<BackendEvent> = backend.complete(request).collect().await;

        match &events[0] {
            BackendEvent::ToolCall {
                name, arguments, ..
            } => {
                assert_eq!(name, "ground");
                assert!(arguments.get("x").is_some());
                assert!(arguments.get("anchor_hash").is_some());
            }
            other => panic!("expected a ToolCall event, got {other:?}"),
        }
        assert!(events.last().unwrap().is_terminal());

        let profile = backend.probe().await.unwrap();
        assert!(profile.vision);
        assert_eq!(backend.id(), "mock_grounder");
    }

    #[tokio::test]
    async fn fixture_grounder_backend_surfaces_a_no_match_as_an_error_event() {
        let backend = FixtureGrounderBackend::new("mock_grounder", notepad_snapshot());
        let request =
            CompletionRequest::text(RequestRole::Grounder, "a button that does not exist", 8);
        let events: Vec<BackendEvent> = backend.complete(request).collect().await;
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], BackendEvent::Error { error_id, .. } if error_id == "grounder_no_match")
        );
    }
}
