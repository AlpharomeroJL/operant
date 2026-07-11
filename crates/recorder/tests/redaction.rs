//! Anchor redaction (C20 Guardian set / FR-S7) end-to-end tests.
//!
//! Exercises `operant_recorder::redact` purely through its public API, the way
//! a capture pipeline sitting in front of the blob store would: build a
//! snapshot modeled on `contracts/fixtures/credential_form/index.html` (the
//! fixture whose own prose requires its password field "MUST be blacked out
//! by the redaction pass before any captured pixels reach disk"), a raw RGBA8
//! frame, call `redact`, and check pixels directly. A second group proves the
//! fail-closed contract: when redaction cannot be safely applied, `redact`
//! returns a typed error and there is no image for a caller to write.

use operant_ir::{Bounds, Element, Role, Snapshot, SnapshotSource, WindowInfo};
use operant_recorder::redact::{redact, RawImage, RedactError};

const CREDENTIAL_FORM: &str = include_str!("../../../contracts/fixtures/credential_form/index.html");

/// Pull the `aria-label` of the `type="password"` input straight from the
/// fixture, so this test stays bound to the actual fixture file (mirrors the
/// helper in `crates/safety/tests/safety_contract.rs`).
fn password_field_label(html: &str) -> Option<String> {
    for line in html.lines() {
        let l = line.trim();
        if l.starts_with("<input") && l.contains(r#"type="password""#) {
            let marker = r#"aria-label=""#;
            if let Some(start) = l.find(marker) {
                let rest = &l[start + marker.len()..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

fn edit_element(idx: u32, name: &str, is_password: bool, bounds: Option<Bounds>) -> Element {
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

fn browser_snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
    Snapshot {
        v: 1,
        source: SnapshotSource::Browser,
        window: WindowInfo {
            hwnd: None,
            process: "msedge.exe".to_string(),
            title: title.to_string(),
            monitor: None,
            dpi_scale: 1.0,
        },
        digest: "d0".to_string(),
        truncated: false,
        captured_ms: None,
        elements,
    }
}

fn row_major_offset(x: u32, y: u32, width: u32) -> usize {
    (y as usize * width as usize + x as usize) * 4
}

// ---- pixel test: the credential_form fixture's password field -------------

#[test]
fn credential_form_password_field_is_blacked_out_pixel_for_pixel() {
    assert!(
        CREDENTIAL_FORM.contains(r#"type="password""#),
        "credential fixture must define a password input"
    );
    let label = password_field_label(CREDENTIAL_FORM).expect("password field has an aria-label");
    assert_eq!(label, "Password");

    // A compliant perceiver flags the fixture's password input is_password,
    // and reports a bounding box for both fields. Coordinates are chosen so
    // the two boxes and the frame margin are all distinguishable in the test.
    let username_box = Bounds { x: 16.0, y: 40.0, w: 200.0, h: 24.0, monitor: None };
    let password_box = Bounds { x: 16.0, y: 84.0, w: 200.0, h: 24.0, monitor: None };
    let username = edit_element(1, "Username", false, Some(username_box.clone()));
    let password = edit_element(2, &label, true, Some(password_box.clone()));
    let snapshot = browser_snapshot("Operant Fixture Sign In", vec![username, password]);

    let width = 320u32;
    let height = 200u32;
    // Fill with a fixed, distinctly non-zero byte so "zeroed" vs "untouched"
    // is unambiguous everywhere in the frame.
    let original = vec![0x64u8; width as usize * height as usize * 4];
    let image = RawImage::new(width, height, original.clone()).expect("well-formed raw image");

    let redacted = redact(&snapshot, &image).expect("fixture snapshot must redact cleanly");

    // Every pixel inside the password field's box is zeroed.
    let px0 = password_box.x as u32;
    let py0 = password_box.y as u32;
    let px1 = px0 + password_box.w as u32;
    let py1 = py0 + password_box.h as u32;
    for y in py0..py1 {
        for x in px0..px1 {
            let off = row_major_offset(x, y, width);
            assert_eq!(
                &redacted.pixels()[off..off + 4],
                [0, 0, 0, 0],
                "password pixel ({x},{y}) must be zeroed"
            );
        }
    }

    // The username field's box (a different, non-sensitive element) is
    // completely untouched.
    let ux0 = username_box.x as u32;
    let uy0 = username_box.y as u32;
    let ux1 = ux0 + username_box.w as u32;
    let uy1 = uy0 + username_box.h as u32;
    for y in uy0..uy1 {
        for x in ux0..ux1 {
            let off = row_major_offset(x, y, width);
            assert_eq!(&redacted.pixels()[off..off + 4], [0x64, 0x64, 0x64, 0x64]);
        }
    }

    // And every pixel in the frame is either inside the (only) redacted box,
    // or exactly matches the original, untouched, byte for byte.
    for y in 0..height {
        for x in 0..width {
            let off = row_major_offset(x, y, width);
            let in_password_box = (px0..px1).contains(&x) && (py0..py1).contains(&y);
            let got = &redacted.pixels()[off..off + 4];
            if in_password_box {
                assert_eq!(got, [0, 0, 0, 0]);
            } else {
                assert_eq!(got, &original[off..off + 4]);
            }
        }
    }
}

// ---- fail closed: refuse the write, never fall through --------------------

#[test]
fn malformed_raw_image_is_refused_before_it_can_be_built() {
    // The public constructor is the only way to get a RawImage at all, and it
    // fails closed on a buffer that does not match width*height*4.
    let err = RawImage::new(8, 8, vec![0u8; 5]).unwrap_err();
    assert_eq!(err, RedactError::BufferSizeMismatch { expected: 256, actual: 5 });
}

#[test]
fn credential_form_password_field_without_bounds_refuses_the_write() {
    // Same fixture-modeled snapshot as the pixel test, but this time the
    // perceiver failed to report a bounding box for the password field, e.g.
    // a partial UIA read. Redaction cannot guarantee that field's pixels are
    // covered, so it must refuse rather than silently ship an unredacted
    // frame.
    let label = password_field_label(CREDENTIAL_FORM).expect("password field has an aria-label");
    let username = edit_element(1, "Username", false, Some(Bounds { x: 0.0, y: 0.0, w: 10.0, h: 10.0, monitor: None }));
    let password = edit_element(2, &label, true, None);
    let snapshot = browser_snapshot("Operant Fixture Sign In", vec![username, password]);

    let width = 64u32;
    let height = 64u32;
    let image = RawImage::new(width, height, vec![0x9Au8; width as usize * height as usize * 4]).unwrap();

    // Model the call site contract: only a successful `redact` produces bytes
    // a caller may pass on to `Recorder::put_artifact`. On error, that never
    // happens.
    let mut blob_store: Vec<Vec<u8>> = Vec::new();
    match redact(&snapshot, &image) {
        Ok(redacted) => blob_store.push(redacted.into_pixels()),
        Err(e) => {
            assert_eq!(e, RedactError::MissingBounds { idx: 2, name: "Password".to_string() });
        }
    }
    assert!(blob_store.is_empty(), "a redaction failure must refuse the write, not fall through");
}
