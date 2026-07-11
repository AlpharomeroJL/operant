//! Anchor redaction (C20 Guardian set / FR-S7).
//!
//! `redact(snapshot, image)` runs between capture and the blob store: every
//! element the snapshot flags sensitive (`Element.is_password`, or a name/role
//! match against a small credential-dialog heuristic) has its bounding-box
//! region zeroed in the captured pixels before any byte reaches
//! [`crate::blobs`]. See `docs/specs/guardian.md`, Redaction section.
//!
//! This pass works on a raw RGBA8 pixel buffer ([`RawImage`]: width, height,
//! and tightly packed bytes) rather than a decoded PNG, so it carries no
//! image-codec dependency. PNG (or other format) encode/decode is a seam at
//! the call site: decode to a `RawImage`, call `redact`, then encode the
//! result, only ever writing the encoded *output* of `redact` to the blob
//! store.
//!
//! Fail closed, per spec: this pass never silently skips a sensitive region.
//! `redact` returns `Result<RawImage, RedactError>`; on `Err`, there is no
//! redacted image to write, so a caller wired correctly (decode -> redact `?`
//! -> encode -> `put_blob`) structurally cannot write unredacted pixels, a
//! malformed buffer, or a half-redacted frame. The one case spec calls out as
//! *not* an error is a bounding box that falls partly or fully outside the
//! frame: that is clamped to the visible region rather than refused.

use operant_ir::{Bounds, Element, Role, Snapshot};

/// Result alias for this module: every fallible function here fails closed
/// with a [`RedactError`], never a panic and never a silent pass-through.
pub type Result<T> = std::result::Result<T, RedactError>;

/// A raw, tightly packed RGBA8 pixel buffer: 4 bytes per pixel, row-major,
/// top-left origin. Fields are private; the only way to build one is
/// [`RawImage::new`], which checks the buffer length against
/// `width * height * 4` so a malformed image can never leave this module's
/// public API. (Internal code, e.g. this module's own tests, can still
/// construct one directly to exercise `redact`'s defensive re-check; that is
/// deliberate defense in depth, not a loophole a caller outside the crate can
/// reach.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RawImage {
    /// Build a raw image. Fails closed with a typed error rather than
    /// constructing something `redact` would have to refuse later: a zero
    /// buffer-length mismatch is [`RedactError::BufferSizeMismatch`], and a
    /// zero width or height is [`RedactError::EmptyImage`] (there is no frame
    /// to have redacted anything in, and a capture pipeline that produced one
    /// has a bug worth surfacing rather than papering over).
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        let expected = pixel_buffer_len(width, height)?;
        if pixels.len() != expected {
            return Err(RedactError::BufferSizeMismatch { expected, actual: pixels.len() });
        }
        if width == 0 || height == 0 {
            return Err(RedactError::EmptyImage { width, height });
        }
        Ok(RawImage { width, height, pixels })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Borrow the raw RGBA8 bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Consume the image, taking ownership of the raw RGBA8 bytes (for a
    /// caller about to hand them, or their PNG encoding, to the blob store).
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

/// `width * height * 4`, computed with checked arithmetic so a huge or
/// corrupt dimension pair fails closed with [`RedactError::DimensionOverflow`]
/// instead of panicking on overflow.
fn pixel_buffer_len(width: u32, height: u32) -> Result<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(RedactError::DimensionOverflow { width, height })
}

/// Typed, exhaustive set of reasons the redaction pass refuses to produce an
/// image. Every variant means the same thing to a caller: there is no
/// redacted image, so there is nothing to write to the blob store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RedactError {
    #[error("raw image buffer length {actual} does not match width*height*4 ({expected})")]
    BufferSizeMismatch { expected: usize, actual: usize },
    #[error("image has zero width or height ({width}x{height})")]
    EmptyImage { width: u32, height: u32 },
    #[error("image dimensions overflow computing buffer size ({width}x{height})")]
    DimensionOverflow { width: u32, height: u32 },
    #[error("element {idx} (\"{name}\") is flagged sensitive but has no bounds to redact by")]
    MissingBounds { idx: u32, name: String },
    #[error("element {idx} (\"{name}\") is flagged sensitive but its bounds are not finite")]
    NonFiniteBounds { idx: u32, name: String },
}

/// Redact every sensitive element's bounding-box region out of `image`, per
/// the snapshot taken at the same moment as the capture. Returns a new,
/// redacted [`RawImage`]; `image` itself is never mutated.
///
/// An element is sensitive when `is_password` is set, or when
/// [`is_credential_dialog_match`] flags it by name. A sensitive element that
/// is `offscreen` is skipped (it contributes no pixels to `image`, so there is
/// nothing to redact and no bounds are required). A sensitive, on-screen
/// element with no bounds, or with non-finite bounds, cannot be safely
/// redacted, so this fails closed with a typed error instead of writing an
/// image that might still expose it: see `MissingBounds` / `NonFiniteBounds`
/// above. A bounding box that only partially overlaps the frame (or does not
/// overlap it at all) is not an error; it is clamped to the visible region.
pub fn redact(snapshot: &Snapshot, image: &RawImage) -> Result<RawImage> {
    // Defense in depth: re-derive and re-check the buffer invariant here too,
    // rather than trusting that `image` was built through `RawImage::new`.
    // `RawImage`'s fields are private to this module, so today the only
    // constructor is `new`, but this function must keep failing closed even
    // if that ever changes (e.g. a future in-module fast path), so it does
    // not lean on that as its only line of defense.
    let expected = pixel_buffer_len(image.width, image.height)?;
    if image.pixels.len() != expected {
        return Err(RedactError::BufferSizeMismatch { expected, actual: image.pixels.len() });
    }
    if image.width == 0 || image.height == 0 {
        return Err(RedactError::EmptyImage { width: image.width, height: image.height });
    }

    let mut pixels = image.pixels.clone();

    for el in &snapshot.elements {
        if el.offscreen || !is_sensitive(el) {
            continue;
        }
        let region = match &el.bounds {
            Some(b) => clamp_region(b, image.width, image.height, el)?,
            None => return Err(RedactError::MissingBounds { idx: el.idx, name: el.name.clone() }),
        };
        if let Some(region) = region {
            zero_region(&mut pixels, image.width, region);
        }
    }

    Ok(RawImage { width: image.width, height: image.height, pixels })
}

/// True for an element the redaction pass must treat as sensitive: the
/// perceiver's own `is_password` flag, or [`is_credential_dialog_match`].
fn is_sensitive(el: &Element) -> bool {
    el.is_password || is_credential_dialog_match(el)
}

/// Terms that, matched (case-insensitively, as a substring) against a
/// text-entry element's accessible name, mark it as a credential field even
/// when the perceiver did not set `is_password`. A small, local heuristic
/// mirroring the intent of the safety crate's credential lexicon
/// (`crates/safety/src/invariants.rs`, driven by
/// `crates/safety/src/data/dialog_lexicon.json`) without taking a cross-crate
/// dependency for it; see FOLLOWUPS for unifying the two.
const CREDENTIAL_TERMS: &[&str] = &[
    "password",
    "passcode",
    "passphrase",
    "pin code",
    "security code",
    "cvv",
    "cvc",
    "card number",
    "credential",
    "secret key",
    "api key",
    "access token",
    "one-time code",
    "otp",
    "2fa",
];

/// The credential-dialog classifier: a text-entry-shaped element whose name
/// matches [`CREDENTIAL_TERMS`].
pub fn is_credential_dialog_match(el: &Element) -> bool {
    let entry_role = matches!(el.role, Role::Edit | Role::Text | Role::Combobox);
    if !entry_role {
        return false;
    }
    let name = el.name.to_lowercase();
    CREDENTIAL_TERMS.iter().any(|term| name.contains(term))
}

/// A clamped pixel-space rectangle: always `x0 < x1 <= width` and
/// `y0 < y1 <= height` (never empty; [`clamp_region`] returns `None` instead
/// of an empty region).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRegion {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

/// Convert an element's bounds (float, device-space, possibly negative-sized,
/// possibly partly or fully off-frame) into a pixel-space rectangle clamped to
/// `width x height`. Returns `Ok(None)` when the box does not overlap the
/// frame at all: a legitimate, non-error outcome (nothing in this image needs
/// redacting for that element). Errors only when the bounds are not usable at
/// all (non-finite), since spec calls out "out of bounds" as a clamp case,
/// not a refusal case, and a box that simply misses the frame is a special
/// case of that, not a different one.
fn clamp_region(b: &Bounds, width: u32, height: u32, el: &Element) -> Result<Option<PixelRegion>> {
    if !b.x.is_finite() || !b.y.is_finite() || !b.w.is_finite() || !b.h.is_finite() {
        return Err(RedactError::NonFiniteBounds { idx: el.idx, name: el.name.clone() });
    }

    // Do not assume w/h are non-negative: take the rectangle's true extent.
    let (left, right) = order(b.x, b.x + b.w);
    let (top, bottom) = order(b.y, b.y + b.h);

    // Round outward (floor the low edge, ceil the high edge) so a fractional
    // box is fully covered rather than sliced by truncation: over-redacting a
    // pixel is harmless, under-redacting one is exactly what this pass exists
    // to prevent. `.max(0.0).min(dimension as f64)` clamps to the frame,
    // including the pathological case where `right`/`bottom` overflowed to
    // infinity: `min` still brings it back to a valid in-frame value.
    let x0 = left.floor().max(0.0).min(width as f64) as u32;
    let x1 = right.ceil().max(0.0).min(width as f64) as u32;
    let y0 = top.floor().max(0.0).min(height as f64) as u32;
    let y1 = bottom.ceil().max(0.0).min(height as f64) as u32;

    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    Ok(Some(PixelRegion { x0, y0, x1, y1 }))
}

fn order(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Zero every RGBA byte inside `region` (all four channels, every pixel).
/// "Filled solid (zeroed)" per spec: this is deliberately the simplest
/// possible fill, so the pixel test is exact.
fn zero_region(pixels: &mut [u8], width: u32, region: PixelRegion) {
    let stride = width as usize * 4;
    for y in region.y0..region.y1 {
        let row_start = y as usize * stride;
        let from = row_start + region.x0 as usize * 4;
        let to = row_start + region.x1 as usize * 4;
        pixels[from..to].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(idx: u32, name: &str, is_password: bool, bounds: Option<Bounds>) -> Element {
        Element {
            idx,
            parent: None,
            role: Role::Edit,
            name: name.to_string(),
            value: None,
            automation_id: None,
            bounds,
            enabled: true,
            offscreen: false,
            is_password,
            patterns: vec![],
            selectors: vec![],
        }
    }

    fn bounds(x: f64, y: f64, w: f64, h: f64) -> Bounds {
        Bounds { x, y, w, h, monitor: None }
    }

    fn snapshot(elements: Vec<Element>) -> Snapshot {
        Snapshot {
            v: 1,
            source: operant_ir::SnapshotSource::Browser,
            window: operant_ir::WindowInfo {
                hwnd: None,
                process: "msedge.exe".to_string(),
                title: "Operant Fixture Sign In".to_string(),
                monitor: None,
                dpi_scale: 1.0,
            },
            digest: "d0".to_string(),
            truncated: false,
            captured_ms: None,
            elements,
        }
    }

    fn solid_image(width: u32, height: u32, fill: u8) -> RawImage {
        RawImage::new(width, height, vec![fill; width as usize * height as usize * 4]).unwrap()
    }

    // ---- pixel test: the fixture credential_form's password field --------

    #[test]
    fn password_region_is_zeroed_and_the_rest_is_untouched() {
        let width = 40;
        let height = 30;
        let image = solid_image(width, height, 0xAB);

        // Modeled on contracts/fixtures/credential_form/index.html: a
        // Username field (not sensitive) and a Password field (is_password),
        // each given a known bounding box as a compliant perceiver would.
        let username = edit(1, "Username", false, Some(bounds(2.0, 2.0, 20.0, 6.0)));
        let password = edit(2, "Password", true, Some(bounds(2.0, 12.0, 20.0, 6.0)));
        let snap = snapshot(vec![username, password]);

        let out = redact(&snap, &image).expect("well-formed input redacts cleanly");
        assert_eq!(out.width(), width);
        assert_eq!(out.height(), height);

        let stride = width as usize * 4;
        for y in 0..height {
            for x in 0..width {
                let off = y as usize * stride + x as usize * 4;
                let px = &out.pixels()[off..off + 4];
                let in_password_box = (2..22).contains(&x) && (12..18).contains(&y);
                if in_password_box {
                    assert_eq!(px, [0, 0, 0, 0], "password pixel ({x},{y}) must be zeroed");
                } else {
                    assert_eq!(
                        px,
                        [0xAB, 0xAB, 0xAB, 0xAB],
                        "non-password pixel ({x},{y}) must be untouched, including the Username field"
                    );
                }
            }
        }
    }

    #[test]
    fn credential_dialog_classifier_matches_by_name_without_is_password_flag() {
        let width = 20;
        let height = 20;
        let image = solid_image(width, height, 0x40);

        // is_password is false, but the name matches the credential lexicon,
        // e.g. a card CVV field the perceiver did not flag.
        let cvv = edit(1, "Card CVV", false, Some(bounds(0.0, 0.0, 10.0, 10.0)));
        let snap = snapshot(vec![cvv]);

        let out = redact(&snap, &image).unwrap();
        let stride = width as usize * 4;
        for y in 0..10u32 {
            for x in 0..10u32 {
                let off = y as usize * stride + x as usize * 4;
                assert_eq!(&out.pixels()[off..off + 4], [0, 0, 0, 0]);
            }
        }
        // Untouched just past the matched box, same row.
        let off = 10 * 4;
        assert_eq!(&out.pixels()[off..off + 4], [0x40, 0x40, 0x40, 0x40]);
    }

    #[test]
    fn region_extending_past_the_frame_edge_is_clamped_not_refused() {
        let width = 10;
        let height = 10;
        let image = solid_image(width, height, 0x77);

        // Box spans (-5,-5) to (15,15): runs off the bottom-right corner and
        // starts before the top-left corner of the 10x10 frame. Both edges
        // must clamp to the frame, not error.
        let password = edit(1, "Password", true, Some(bounds(-5.0, -5.0, 20.0, 20.0)));
        let snap = snapshot(vec![password]);

        let out = redact(&snap, &image).expect("out-of-bounds regions clamp, they do not refuse");
        // The whole 10x10 frame is inside the clamped box: everything zeroed.
        assert!(out.pixels().iter().all(|&b| b == 0));
    }

    #[test]
    fn region_entirely_outside_the_frame_redacts_nothing_and_is_not_an_error() {
        let width = 10;
        let height = 10;
        let image = solid_image(width, height, 0x99);

        let password = edit(1, "Password", true, Some(bounds(1000.0, 1000.0, 20.0, 20.0)));
        let snap = snapshot(vec![password]);

        let out = redact(&snap, &image).expect("a box with no overlap is not an error");
        assert!(out.pixels().iter().all(|&b| b == 0x99), "nothing overlapped, nothing changes");
    }

    #[test]
    fn offscreen_sensitive_element_is_skipped_without_bounds_or_error() {
        let width = 10;
        let height = 10;
        let image = solid_image(width, height, 0x55);

        let mut password = edit(1, "Password", true, None);
        password.offscreen = true;
        let snap = snapshot(vec![password]);

        let out = redact(&snap, &image).expect("an offscreen element needs no bounds");
        assert!(out.pixels().iter().all(|&b| b == 0x55));
    }

    // ---- fail closed: refuse rather than fall through ---------------------

    #[test]
    fn malformed_buffer_is_refused_by_the_constructor() {
        let err = RawImage::new(4, 4, vec![0u8; 10]).unwrap_err();
        assert_eq!(err, RedactError::BufferSizeMismatch { expected: 64, actual: 10 });
    }

    #[test]
    fn zero_dimension_image_is_refused_by_the_constructor() {
        let err = RawImage::new(0, 5, vec![]).unwrap_err();
        assert_eq!(err, RedactError::EmptyImage { width: 0, height: 5 });
    }

    #[test]
    fn malformed_buffer_reaching_redact_directly_is_refused_and_nothing_is_written() {
        // Bypass `RawImage::new` (same-module access to private fields) to
        // prove `redact` itself fails closed on a bad buffer, not just the
        // constructor: defense in depth, not "trust the caller validated it."
        let bad = RawImage { width: 4, height: 4, pixels: vec![0u8; 10] };
        let snap = snapshot(vec![]);

        // Model the call site: only a successful redact() yields bytes a
        // caller may hand to the blob store.
        let mut wrote: Option<Vec<u8>> = None;
        match redact(&snap, &bad) {
            Ok(image) => wrote = Some(image.into_pixels()),
            Err(e) => assert_eq!(e, RedactError::BufferSizeMismatch { expected: 64, actual: 10 }),
        }
        assert!(wrote.is_none(), "a redaction error must refuse the write");
    }

    #[test]
    fn sensitive_element_with_no_bounds_refuses_the_write() {
        let image = solid_image(10, 10, 0x22);
        let password = edit(1, "Password", true, None);
        let snap = snapshot(vec![password]);

        let mut wrote: Option<Vec<u8>> = None;
        match redact(&snap, &image) {
            Ok(out) => wrote = Some(out.into_pixels()),
            Err(e) => {
                assert_eq!(e, RedactError::MissingBounds { idx: 1, name: "Password".to_string() })
            }
        }
        assert!(wrote.is_none(), "a sensitive element with no bounds must refuse the write, never fall through");
    }

    #[test]
    fn sensitive_element_with_non_finite_bounds_refuses_the_write() {
        let image = solid_image(10, 10, 0x22);
        let password = edit(1, "Password", true, Some(bounds(f64::NAN, 0.0, 5.0, 5.0)));
        let snap = snapshot(vec![password]);

        let err = redact(&snap, &image).unwrap_err();
        assert_eq!(err, RedactError::NonFiniteBounds { idx: 1, name: "Password".to_string() });
    }

    #[test]
    fn non_sensitive_elements_never_change_the_image() {
        let width = 6;
        let height = 6;
        let image = solid_image(width, height, 0x11);
        let username = edit(1, "Username", false, Some(bounds(0.0, 0.0, 6.0, 6.0)));
        let snap = snapshot(vec![username]);

        let out = redact(&snap, &image).unwrap();
        assert_eq!(out.pixels(), image.pixels());
    }
}
