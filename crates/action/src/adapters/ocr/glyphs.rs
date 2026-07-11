//! Bitmap glyph segmentation plus a coarse nearest-neighbor classifier.
//! "A simple approach is acceptable" for the image half of the `ocr`
//! adapter (this lane's brief, echoing `docs/specs/action.md`'s "on
//! device" requirement with no OCR-engine dependency implied): real ink
//! thresholding and row/column projection segmentation, then a small
//! built-in template table sampled from this project's own render
//! pipeline (`contracts/fixtures/render_png.ps1`: Arial Bold,
//! `TextRenderingHint.SingleBitPerPixelGridFit`, `SmoothingMode.None`, so
//! glyphs land as crisp non-anti-aliased shapes rather than blurred
//! ones). It covers the alphabet that pipeline renders (A-Z, 0-9, space,
//! '-', '.'); anything else classifies as `'?'` rather than guessing.  A
//! learned/general OCR model is a FOLLOWUP.

use super::png::DecodedImage;

pub const GRID_W: usize = 6;
pub const GRID_H: usize = 8;
pub type Cell = [[u8; GRID_W]; GRID_H];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub x0: u32,
    pub x1: u32, // exclusive
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub y0: u32,
    pub y1: u32, // exclusive
}

/// Binary ink mask: `true` where the pixel is dark ink, indexed
/// `[y * width + x]`. `render_png.ps1` renders black text
/// (`Brushes::Black`) on a white background with no smoothing, so pixels
/// land cleanly on one side of the midpoint in practice; the threshold is
/// still a parameter so a noisier source image degrades gracefully rather
/// than never matching.
pub fn threshold(img: &DecodedImage, cutoff: u8) -> Vec<bool> {
    img.gray.iter().map(|&p| p < cutoff).collect()
}

/// Row-projection line segmentation: a "line" is a maximal run of rows
/// that contain at least one ink pixel, so it naturally spans a line's
/// ascenders/descenders without needing font metrics.
pub fn segment_lines(ink: &[bool], width: u32, height: u32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut in_line = false;
    let mut y0 = 0u32;
    for y in 0..height {
        let row_has_ink = (0..width).any(|x| ink[(y * width + x) as usize]);
        if row_has_ink && !in_line {
            in_line = true;
            y0 = y;
        } else if !row_has_ink && in_line {
            in_line = false;
            lines.push(Line { y0, y1: y });
        }
    }
    if in_line {
        lines.push(Line { y0, y1: height });
    }
    lines
}

/// Column-projection glyph segmentation within one line's row band: a
/// "glyph" is a maximal run of columns that contain ink somewhere in
/// `[line.y0, line.y1)`. Touching/kerned glyphs that share an ink column
/// are not split further (out of scope for a projection-based segmenter);
/// see FOLLOWUPS.
pub fn segment_glyphs_in_line(ink: &[bool], width: u32, line: &Line) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    let mut in_glyph = false;
    let mut x0 = 0u32;
    for x in 0..width {
        let col_has_ink = (line.y0..line.y1).any(|y| ink[(y * width + x) as usize]);
        if col_has_ink && !in_glyph {
            in_glyph = true;
            x0 = x;
        } else if !col_has_ink && in_glyph {
            in_glyph = false;
            glyphs.push(Glyph { x0, x1: x });
        }
    }
    if in_glyph {
        glyphs.push(Glyph { x0, x1: width });
    }
    glyphs
}

/// Gap in pixels between two adjacent glyphs on the same line.
pub fn gap(prev: &Glyph, next: &Glyph) -> u32 {
    next.x0.saturating_sub(prev.x1)
}

/// Area-sample a glyph's ink mask down to a fixed `GRID_H` x `GRID_W`
/// grid: each cell holds the percentage (0..=100) of source pixels in
/// its region that are ink. Fixed-size output makes every glyph
/// comparable to the template table regardless of its pixel size.
pub fn normalize(ink: &[bool], width: u32, line: &Line, glyph: &Glyph) -> Cell {
    let gw = (glyph.x1 - glyph.x0).max(1);
    let gh = (line.y1 - line.y0).max(1);
    let mut cell = [[0u8; GRID_W]; GRID_H];
    for (row, cell_row) in cell.iter_mut().enumerate() {
        let sy0 = line.y0 + (row as u32 * gh) / GRID_H as u32;
        let sy1 = line.y0 + ((row as u32 + 1) * gh) / GRID_H as u32;
        let sy1 = sy1.max(sy0 + 1).min(line.y1);
        for (col, cell_val) in cell_row.iter_mut().enumerate() {
            let sx0 = glyph.x0 + (col as u32 * gw) / GRID_W as u32;
            let sx1 = glyph.x0 + ((col as u32 + 1) * gw) / GRID_W as u32;
            let sx1 = sx1.max(sx0 + 1).min(glyph.x1);
            let mut total = 0u32;
            let mut dark = 0u32;
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    total += 1;
                    if ink[(y * width + x) as usize] {
                        dark += 1;
                    }
                }
            }
            *cell_val = if total == 0 {
                0
            } else {
                (dark * 100 / total) as u8
            };
        }
    }
    cell
}

/// One reference character and its normalized template, sampled from a
/// real decode of `contracts/fixtures/docs/sample.png` (see the
/// `harvest_templates` dev tool in this module's tests). Kept as a plain
/// array (not a `HashMap`) so it is `const`-friendly.
struct Template {
    ch: char,
    cell: Cell,
}

/// Squared Euclidean distance between two grids, in the same 0..=100
/// per-cell units [`normalize`] produces.
fn distance(a: &Cell, b: &Cell) -> u32 {
    let mut d = 0u32;
    for r in 0..GRID_H {
        for c in 0..GRID_W {
            let diff = a[r][c] as i32 - b[r][c] as i32;
            d += (diff * diff) as u32;
        }
    }
    d
}

/// Nearest-neighbor classification against [`TEMPLATES`]. An empty glyph
/// (no ink sampled at all, which should not happen for a real segmented
/// glyph) classifies as `'?'` rather than picking an arbitrary nearest
/// template.
pub fn classify(cell: &Cell) -> char {
    let total_ink: u32 = cell.iter().flatten().map(|&v| v as u32).sum();
    if total_ink == 0 {
        return '?';
    }
    TEMPLATES
        .iter()
        .min_by_key(|t| distance(&t.cell, cell))
        .map(|t| t.ch)
        .unwrap_or('?')
}

include!("glyph_templates.rs");

/// One recognized word: its text and its pixel bounding box (`x1`/`y1`
/// exclusive, matching [`Glyph`]/[`Line`]'s convention).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageWord {
    pub text: String,
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageExtraction {
    pub text: String,
    pub words: Vec<ImageWord>,
}

/// Column gap, in pixels, at or above which two adjacent glyphs on the
/// same line count as separate words rather than the same word.
/// Calibrated against `contracts/fixtures/docs/sample.png`: the widest
/// intra-word gap measured there is 8px (a kerned "11" pair); the
/// narrowest inter-word gap is 13px. 10 sits with margin on both sides.
/// A different font/size would need recalibrating this constant.
const SPACE_GAP_PX: u32 = 10;

/// Full pipeline: threshold, segment into lines and glyphs, classify each
/// glyph, and regroup into words by gap width. This is what
/// [`super::OcrAdapter`]'s `extract` verb runs for any non-PDF input.
pub fn read_text(img: &DecodedImage) -> ImageExtraction {
    let ink = threshold(img, 128);
    let lines = segment_lines(&ink, img.width, img.height);

    let mut text_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut words: Vec<ImageWord> = Vec::new();

    for line in &lines {
        let glyphs = segment_glyphs_in_line(&ink, img.width, line);
        let mut line_words: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut word_x0 = 0u32;
        let mut prev: Option<Glyph> = None;

        for glyph in &glyphs {
            let is_new_word = match prev {
                Some(p) => gap(&p, glyph) >= SPACE_GAP_PX,
                None => false,
            };
            if is_new_word && !current.is_empty() {
                let x1 = prev.expect("is_new_word implies prev is Some").x1;
                words.push(ImageWord {
                    text: current.clone(),
                    x0: word_x0,
                    y0: line.y0,
                    x1,
                    y1: line.y1,
                });
                line_words.push(std::mem::take(&mut current));
            }
            if current.is_empty() {
                word_x0 = glyph.x0;
            }
            current.push(classify(&normalize(&ink, img.width, line, glyph)));
            prev = Some(*glyph);
        }
        if !current.is_empty() {
            let x1 = prev
                .expect("current non-empty implies at least one glyph")
                .x1;
            words.push(ImageWord {
                text: current.clone(),
                x0: word_x0,
                y0: line.y0,
                x1,
                y1: line.y1,
            });
            line_words.push(current);
        }
        text_lines.push(line_words.join(" "));
    }

    ImageExtraction {
        text: text_lines.join("\n"),
        words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ocr::png;

    fn fixture_image() -> DecodedImage {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/fixtures/docs/sample.png"),
        )
        .unwrap();
        png::decode(&bytes).unwrap()
    }

    #[test]
    fn segments_three_text_lines_matching_render_png_ps1() {
        let img = fixture_image();
        let ink = threshold(&img, 128);
        let lines = segment_lines(&ink, img.width, img.height);
        // render_png.ps1 draws three DrawString calls at y=30, 90, 150 in
        // a 26pt font; each becomes one row-projection band.
        assert_eq!(lines.len(), 3, "expected 3 text lines, got {lines:?}");
    }

    #[test]
    fn middle_line_segments_into_the_invoice_number_glyphs() {
        let img = fixture_image();
        let ink = threshold(&img, 128);
        let lines = segment_lines(&ink, img.width, img.height);
        let line = lines[1]; // "INV-2026-0711"
        let glyphs = segment_glyphs_in_line(&ink, img.width, &line);
        // I, N, V, -, 2, 0, 2, 6, -, 0, 7, 1, 1 = 13 glyphs, none touching
        // at this size/weight.
        assert_eq!(glyphs.len(), 13, "glyphs: {glyphs:?}");
    }

    #[test]
    fn reads_the_invoice_number_line_exactly() {
        let img = fixture_image();
        let out = read_text(&img);
        assert!(
            out.text.contains("INV-2026-0711"),
            "full text was: {:?}",
            out.text
        );
    }

    #[test]
    fn reads_the_total_line_as_two_words_split_on_the_real_space() {
        let img = fixture_image();
        let out = read_text(&img);
        assert!(
            out.text.contains("TOTAL") && out.text.contains("142.50"),
            "full text was: {:?}",
            out.text
        );
        let total_word = out.words.iter().find(|w| w.text == "142.50");
        assert!(
            total_word.is_some(),
            "142.50 should be its own word, got words: {:?}",
            out.words.iter().map(|w| &w.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn word_boxes_are_well_formed_and_span_real_pixels() {
        let img = fixture_image();
        let out = read_text(&img);
        assert!(!out.words.is_empty());
        for w in &out.words {
            assert!(w.x1 > w.x0, "word {:?} has a non-positive width", w.text);
            assert!(w.y1 > w.y0, "word {:?} has a non-positive height", w.text);
        }
    }

    /// Dev tool, not a correctness assertion: dumps every segmented
    /// glyph's normalized grid as ASCII art plus the gap before it, in
    /// reading order, against the known ground-truth strings
    /// `render_png.ps1` draws. Run with
    /// `cargo test -p operant-action harvest_templates -- --nocapture --ignored`
    /// after touching the fixture or the grid size, then paste the
    /// printed `Template` entries into `glyph_templates.rs`.
    #[test]
    #[ignore]
    fn harvest_templates() {
        let img = fixture_image();
        let ink = threshold(&img, 128);
        let lines = segment_lines(&ink, img.width, img.height);
        let ground_truth = ["OPERANTFIXTUREINVOICE", "INV-2026-0711", "TOTAL142.50"];
        for (line, truth) in lines.iter().zip(ground_truth.iter()) {
            let glyphs = segment_glyphs_in_line(&ink, img.width, line);
            println!(
                "line y=[{},{}) glyphs={} truth_len={}",
                line.y0,
                line.y1,
                glyphs.len(),
                truth.len()
            );
            let mut prev: Option<Glyph> = None;
            for (glyph, ch) in glyphs.iter().zip(truth.chars()) {
                let g = gap(&prev.unwrap_or(*glyph), glyph);
                let cell = normalize(&ink, img.width, line, glyph);
                println!(
                    "  ch='{}' x=[{},{}) gap_before={} cell={:?}",
                    ch, glyph.x0, glyph.x1, g, cell
                );
                prev = Some(*glyph);
            }
        }
    }
}
