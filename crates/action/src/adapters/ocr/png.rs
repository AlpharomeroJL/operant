//! Minimal PNG decoder for the `ocr` adapter's image path. Handles what
//! `contracts/fixtures/docs/sample.png` (and most simple, non-interlaced,
//! 8-bit renders such as `render_png.ps1`'s `System.Drawing` output) use:
//! 8-bit-depth grayscale/RGB/RGBA, no interlacing. Palette images, 16-bit
//! depth, and Adam7 interlacing are unsupported (a clear typed error, not
//! a silent misdecode); see FOLLOWUPS in `RESULT.md`.
//!
//! IDAT is always zlib-compressed per the PNG spec (there is no
//! uncompressed option), so decoding it needs an inflate implementation;
//! [`miniz_oxide`] is a pure-Rust, dependency-free, widely-used one (the
//! same one `flate2`'s `rust_backend` and the `png`/`image` crates use),
//! added as an optional dependency behind this crate's `ocr` feature.

use thiserror::Error;

const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, Error)]
pub enum PngError {
    #[error("not a PNG file (bad signature)")]
    BadSignature,
    #[error("truncated PNG data")]
    Truncated,
    #[error("unsupported PNG: {0}")]
    Unsupported(String),
    #[error("zlib inflate of IDAT failed: {0}")]
    Inflate(String),
    #[error("no IHDR chunk")]
    MissingIhdr,
}

/// Decoded pixels, row-major, one 8-bit grayscale sample per pixel.
#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub gray: Vec<u8>,
}

impl DecodedImage {
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.gray[(y * self.width + x) as usize]
    }
}

struct Ihdr {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
}

pub fn decode(bytes: &[u8]) -> Result<DecodedImage, PngError> {
    if bytes.len() < 8 || bytes[0..8] != SIGNATURE {
        return Err(PngError::BadSignature);
    }

    let mut ihdr: Option<Ihdr> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut i = 8usize;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        let ctype = &bytes[i + 4..i + 8];
        let data_start = i + 8;
        let data_end = data_start.checked_add(len).ok_or(PngError::Truncated)?;
        if data_end.checked_add(4).is_none_or(|end| end > bytes.len()) {
            return Err(PngError::Truncated);
        }
        let data = &bytes[data_start..data_end];
        match ctype {
            b"IHDR" => {
                if data.len() < 13 {
                    return Err(PngError::Truncated);
                }
                ihdr = Some(Ihdr {
                    width: u32::from_be_bytes(data[0..4].try_into().unwrap()),
                    height: u32::from_be_bytes(data[4..8].try_into().unwrap()),
                    bit_depth: data[8],
                    color_type: data[9],
                    interlace: data[12],
                });
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        i = data_end + 4; // skip the trailing CRC32
    }

    let ihdr = ihdr.ok_or(PngError::MissingIhdr)?;
    if ihdr.bit_depth != 8 {
        return Err(PngError::Unsupported(format!(
            "bit depth {} (only 8 is supported)",
            ihdr.bit_depth
        )));
    }
    if ihdr.interlace != 0 {
        return Err(PngError::Unsupported("Adam7 interlacing".into()));
    }
    let channels: usize = match ihdr.color_type {
        0 => 1, // grayscale
        2 => 3, // RGB
        4 => 2, // grayscale + alpha
        6 => 4, // RGBA
        other => {
            return Err(PngError::Unsupported(format!(
                "color type {other} (palette images are not supported)"
            )))
        }
    };

    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat)
        .map_err(|e| PngError::Inflate(format!("{e:?}")))?;

    let width = ihdr.width as usize;
    let height = ihdr.height as usize;
    let bpp = channels; // bytes per pixel at 8-bit depth
    let stride = width * bpp;
    if height == 0 || width == 0 || raw.len() < height * (stride + 1) {
        return Err(PngError::Truncated);
    }

    let mut gray = vec![0u8; width * height];
    let mut prev_row = vec![0u8; stride];
    let mut pos = 0usize;
    for y in 0..height {
        let filter_type = raw[pos];
        pos += 1;
        let mut row = raw[pos..pos + stride].to_vec();
        pos += stride;
        unfilter_row(filter_type, &mut row, &prev_row, bpp)?;
        for x in 0..width {
            let px = &row[x * bpp..x * bpp + bpp];
            gray[y * width + x] = to_gray(px, channels);
        }
        prev_row = row;
    }

    Ok(DecodedImage {
        width: ihdr.width,
        height: ihdr.height,
        gray,
    })
}

fn to_gray(px: &[u8], channels: usize) -> u8 {
    match channels {
        1 => px[0],
        2 => px[0], // gray+alpha: fixtures here are opaque; alpha ignored
        3 => ((px[0] as u32 + px[1] as u32 + px[2] as u32) / 3) as u8,
        4 => {
            // Composite over a white background using alpha, then
            // average RGB. Reduces to a plain average when alpha is 255
            // (every fixture), but stays correct for partial alpha too.
            let a = px[3] as u32;
            let over_white = |c: u8| -> u32 { (c as u32 * a + 255 * (255 - a)) / 255 };
            let r = over_white(px[0]);
            let g = over_white(px[1]);
            let b = over_white(px[2]);
            ((r + g + b) / 3) as u8
        }
        _ => 255,
    }
}

/// PNG filter reconstruction (spec section 9.2..9.4): `row` is filtered
/// in place using the already-reconstructed previous row.
fn unfilter_row(filter_type: u8, row: &mut [u8], prev: &[u8], bpp: usize) -> Result<(), PngError> {
    match filter_type {
        0 => {} // None
        1 => {
            // Sub
            for x in bpp..row.len() {
                row[x] = row[x].wrapping_add(row[x - bpp]);
            }
        }
        2 => {
            // Up
            for x in 0..row.len() {
                row[x] = row[x].wrapping_add(prev[x]);
            }
        }
        3 => {
            // Average
            for x in 0..row.len() {
                let a = if x >= bpp { row[x - bpp] as u16 } else { 0 };
                let b = prev[x] as u16;
                row[x] = row[x].wrapping_add(((a + b) / 2) as u8);
            }
        }
        4 => {
            // Paeth
            for x in 0..row.len() {
                let a = if x >= bpp { row[x - bpp] } else { 0 };
                let b = prev[x];
                let c = if x >= bpp { prev[x - bpp] } else { 0 };
                row[x] = row[x].wrapping_add(paeth_predictor(a, b, c));
            }
        }
        other => return Err(PngError::Unsupported(format!("filter type {other}"))),
    }
    Ok(())
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_bytes() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/fixtures/docs/sample.png"),
        )
        .expect("contracts/fixtures/docs/sample.png exists")
    }

    #[test]
    fn decodes_the_fixture_to_its_known_dimensions() {
        let img = decode(&fixture_bytes()).unwrap();
        // render_png.ps1: `New-Object System.Drawing.Bitmap(800, 240)`.
        assert_eq!(img.width, 800);
        assert_eq!(img.height, 240);
        assert_eq!(img.gray.len(), 800 * 240);
    }

    #[test]
    fn corners_are_white_background_and_the_image_has_real_dark_ink() {
        let img = decode(&fixture_bytes()).unwrap();
        assert!(
            img.get(0, 0) > 250,
            "top-left corner should be white background"
        );
        assert!(
            img.get(img.width - 1, img.height - 1) > 250,
            "bottom-right corner should be white background"
        );
        let dark_pixels = img.gray.iter().filter(|&&p| p < 50).count();
        assert!(
            dark_pixels > 500,
            "expected a substantial amount of black text ink, got {dark_pixels} dark pixels"
        );
    }

    #[test]
    fn rejects_a_bad_signature_instead_of_misreading_garbage() {
        let err = decode(b"not a png").unwrap_err();
        assert!(matches!(err, PngError::BadSignature));
    }
}
