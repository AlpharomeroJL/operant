//! Minimal PDF text extractor: "minimal" per the brief means this walks
//! the raw content stream operators (`BT`/`ET`, `Tf`, `Tm`/`Td`/`TD`,
//! `Tj`/`TJ`) directly rather than building a general PDF object model.
//! It finds every `stream`...`endstream` block in the file byte-for-byte
//! (skipping ones whose preceding dict declares a `/Filter`, since
//! decoding FlateDecode etc. is out of scope here), so it does not need
//! to resolve the page tree (`Catalog` -> `Pages` -> `Kids` -> `Contents`)
//! to find them. Good enough for the fixture (`contracts/fixtures/docs/
//! sample.pdf`, generated uncompressed by `contracts/fixtures/
//! generate.mjs`) and for any similarly simple, uncompressed PDF;
//! compressed/encrypted/object-stream PDFs are a FOLLOWUP.

/// One word's approximate placement. `x`/`y` come straight from the
/// content stream's text matrix (PDF user space: origin bottom-left, y
/// grows up); `width`/`height` are estimated from character count and
/// font size, not real font metrics, so callers should treat them as
/// approximate, not typographically exact.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfWord {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PdfExtraction {
    pub text: String,
    pub words: Vec<PdfWord>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Str(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    Name(String),
    Op(String),
}

pub fn extract(bytes: &[u8]) -> PdfExtraction {
    let mut text_parts: Vec<String> = Vec::new();
    let mut words: Vec<PdfWord> = Vec::new();
    let mut font_size = 12.0_f64;
    let mut origin = (0.0_f64, 0.0_f64);

    for stream in find_content_streams(bytes) {
        let tokens = tokenize(&stream);
        let mut operands: Vec<Token> = Vec::new();
        for tok in tokens {
            let Token::Op(op) = tok else {
                operands.push(tok);
                continue;
            };
            match op.as_str() {
                "Tf" => {
                    if let Some(Token::Number(size)) = operands.last() {
                        font_size = *size;
                    }
                }
                "Tm" => {
                    if operands.len() >= 6 {
                        if let (Token::Number(e), Token::Number(f)) =
                            (&operands[operands.len() - 2], &operands[operands.len() - 1])
                        {
                            origin = (*e, *f);
                        }
                    }
                }
                "Td" | "TD" => {
                    if operands.len() >= 2 {
                        if let (Token::Number(tx), Token::Number(ty)) =
                            (&operands[operands.len() - 2], &operands[operands.len() - 1])
                        {
                            origin = (origin.0 + tx, origin.1 + ty);
                        }
                    }
                }
                "Tj" => {
                    if let Some(Token::Str(raw)) = operands.last() {
                        let s = pdf_string_to_text(raw);
                        if !s.is_empty() {
                            place_words(&s, origin, font_size, &mut words);
                            text_parts.push(s);
                        }
                    }
                }
                "TJ" => {
                    let mut combined = String::new();
                    for o in &operands {
                        if let Token::Str(raw) = o {
                            combined.push_str(&pdf_string_to_text(raw));
                        }
                        // Numeric kerning adjustments shift the next glyph
                        // by a fraction of an em; a minimal, non-font-
                        // metric extractor ignores them for placement.
                    }
                    if !combined.is_empty() {
                        place_words(&combined, origin, font_size, &mut words);
                        text_parts.push(combined);
                    }
                }
                _ => {}
            }
            operands.clear();
        }
    }

    PdfExtraction {
        text: text_parts.join(" "),
        words,
    }
}

/// Average-advance heuristic (~0.5em per character): a minimal extractor
/// has no font metrics table, so this is deliberately approximate.
fn estimate_width(char_count: usize, font_size: f64) -> f64 {
    char_count as f64 * font_size * 0.5
}

fn place_words(text: &str, origin: (f64, f64), font_size: f64, words: &mut Vec<PdfWord>) {
    let (mut x, y) = origin;
    let space_width = estimate_width(1, font_size);
    for word in text.split_whitespace() {
        let w = estimate_width(word.chars().count(), font_size);
        words.push(PdfWord {
            text: word.to_string(),
            x,
            y,
            width: w,
            height: font_size,
        });
        x += w + space_width;
    }
}

/// Fixtures and the common case are plain ASCII/Latin text under
/// WinAnsiEncoding, which agrees with UTF-8 in that range; a full
/// PDFDocEncoding/embedded-CMap decode is out of scope for "minimal".
fn pdf_string_to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Every `stream`...`endstream` body in the file, skipping ones whose
/// nearby preceding dict text mentions `/Filter` (compressed; out of
/// scope). Deliberately does not parse the object/xref graph: scanning
/// raw bytes finds every page's content stream regardless of how the
/// page tree is structured.
fn find_content_streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    const FILTER_LOOKBACK: usize = 300;
    let mut out = Vec::new();
    let mut search_from = 0usize;

    while let Some(rel) = find_subslice(&bytes[search_from..], b"stream") {
        let stream_kw = search_from + rel;
        // Whole-word match only (so the tail of a future "xstream"-like
        // token, or `endstream` itself on a re-scan, is never mistaken
        // for the `stream` keyword).
        if stream_kw > 0 && bytes[stream_kw - 1].is_ascii_alphanumeric() {
            search_from = stream_kw + 6;
            continue;
        }

        let mut body_start = stream_kw + 6;
        if bytes.get(body_start) == Some(&b'\r') {
            body_start += 1;
        }
        if bytes.get(body_start) == Some(&b'\n') {
            body_start += 1;
        }

        let window_start = stream_kw.saturating_sub(FILTER_LOOKBACK);
        let filtered = find_subslice(&bytes[window_start..stream_kw], b"/Filter").is_some();

        let Some(end_rel) = find_subslice(&bytes[body_start..], b"endstream") else {
            break;
        };
        let mut body_end = body_start + end_rel;
        if body_end > body_start && bytes[body_end - 1] == b'\n' {
            body_end -= 1;
        }
        if body_end > body_start && bytes[body_end - 1] == b'\r' {
            body_end -= 1;
        }

        if !filtered {
            out.push(bytes[body_start..body_end].to_vec());
        }
        search_from = body_start + end_rel + "endstream".len();
    }
    out
}

fn is_regular(b: u8) -> bool {
    !matches!(
        b,
        b' ' | b'\t'
            | b'\r'
            | b'\n'
            | 0x0c
            | 0x00
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

fn read_literal_string(bytes: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut i = start + 1; // skip '('
    let mut depth = 1i32;
    let mut out = Vec::new();
    while i < bytes.len() && depth > 0 {
        let c = bytes[i];
        match c {
            b'\\' if i + 1 < bytes.len() => {
                let e = bytes[i + 1];
                match e {
                    b'n' => {
                        out.push(b'\n');
                        i += 2;
                    }
                    b'r' => {
                        out.push(b'\r');
                        i += 2;
                    }
                    b't' => {
                        out.push(b'\t');
                        i += 2;
                    }
                    b'b' => {
                        out.push(0x08);
                        i += 2;
                    }
                    b'f' => {
                        out.push(0x0c);
                        i += 2;
                    }
                    b'(' | b')' | b'\\' => {
                        out.push(e);
                        i += 2;
                    }
                    b'\r' => {
                        i += 2;
                        if bytes.get(i) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {
                        i += 2;
                    }
                    b'0'..=b'7' => {
                        let mut j = i + 1;
                        let mut val: u32 = 0;
                        let mut count = 0;
                        while j < bytes.len() && count < 3 && (b'0'..=b'7').contains(&bytes[j]) {
                            val = val * 8 + (bytes[j] - b'0') as u32;
                            j += 1;
                            count += 1;
                        }
                        out.push(val as u8);
                        i = j;
                    }
                    other => {
                        out.push(other);
                        i += 2;
                    }
                }
            }
            b'(' => {
                depth += 1;
                out.push(c);
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth > 0 {
                    out.push(c);
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i)
}

fn tokenize(bytes: &[u8]) -> Vec<Token> {
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00 => i += 1,
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                let (s, next) = read_literal_string(bytes, i);
                toks.push(Token::Str(s));
                i = next;
            }
            b'[' => {
                toks.push(Token::ArrayStart);
                i += 1;
            }
            b']' => {
                toks.push(Token::ArrayEnd);
                i += 1;
            }
            b'/' => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && is_regular(bytes[j]) {
                    j += 1;
                }
                toks.push(Token::Name(
                    String::from_utf8_lossy(&bytes[start..j]).into_owned(),
                ));
                i = j;
            }
            b'<' => {
                if bytes.get(i + 1) == Some(&b'<') {
                    // Inline dict (e.g. a BDC property list): skip to the
                    // matching `>>`, tracking nesting depth.
                    let mut depth = 1i32;
                    let mut j = i + 2;
                    while j < bytes.len() && depth > 0 {
                        if bytes[j] == b'<' && bytes.get(j + 1) == Some(&b'<') {
                            depth += 1;
                            j += 2;
                        } else if bytes[j] == b'>' && bytes.get(j + 1) == Some(&b'>') {
                            depth -= 1;
                            j += 2;
                        } else {
                            j += 1;
                        }
                    }
                    i = j;
                } else {
                    let start = i + 1;
                    let mut j = start;
                    while j < bytes.len() && bytes[j] != b'>' {
                        j += 1;
                    }
                    toks.push(Token::Str(decode_hex_string(&bytes[start..j])));
                    i = j + 1;
                }
            }
            b'-' | b'+' | b'.' | b'0'..=b'9' => {
                let start = i;
                let mut j = i + 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_digit()
                        || matches!(bytes[j], b'.' | b'-' | b'+' | b'e' | b'E'))
                {
                    j += 1;
                }
                if let Ok(s) = std::str::from_utf8(&bytes[start..j]) {
                    if let Ok(n) = s.parse::<f64>() {
                        toks.push(Token::Number(n));
                    }
                }
                i = j;
            }
            _ => {
                let start = i;
                let mut j = i;
                while j < bytes.len() && is_regular(bytes[j]) {
                    j += 1;
                }
                if j == start {
                    i += 1; // stray delimiter byte (`{`, `}`); skip it
                    continue;
                }
                toks.push(Token::Op(
                    String::from_utf8_lossy(&bytes[start..j]).into_owned(),
                ));
                i = j;
            }
        }
    }
    toks
}

fn decode_hex_string(hex: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = hex
        .iter()
        .copied()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    digits
        .chunks(2)
        .map(|pair| {
            let hi = hex_val(pair[0]);
            let lo = if pair.len() == 2 { hex_val(pair[1]) } else { 0 };
            (hi << 4) | lo
        })
        .collect()
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_fixture_invoice_tokens() {
        let bytes = fixture_bytes();
        let out = extract(&bytes);
        assert!(out.text.contains("INV-2026-0711"), "text was: {}", out.text);
        assert!(out.text.contains("142.50"), "text was: {}", out.text);
        assert!(out.text.contains("Operant fixture invoice"));
    }

    #[test]
    fn produces_word_boxes_for_every_word() {
        let bytes = fixture_bytes();
        let out = extract(&bytes);
        let word = out
            .words
            .iter()
            .find(|w| w.text.contains("INV-2026-0711"))
            .expect("the invoice number is its own word");
        assert!(word.x > 0.0);
        assert!(word.width > 0.0);
        assert!(word.height > 0.0);
    }

    #[test]
    fn escaped_parens_and_backslashes_round_trip() {
        let content = b"BT\n/F1 12 Tf\n1 0 0 1 0 0 Tm\n(a \\(b\\) c \\\\ d) Tj\nET";
        let (s, end) =
            read_literal_string(content, content.iter().position(|&b| b == b'(').unwrap());
        assert_eq!(end, content.len() - "Tj\nET".len() - "\n".len());
        assert_eq!(String::from_utf8(s).unwrap(), "a (b) c \\ d");
    }

    #[test]
    fn filtered_streams_are_skipped_not_garbled_into_the_output() {
        let pdf = b"1 0 obj\n<< /Length 20 /Filter /FlateDecode >>\nstream\nabsolutely not text\nendstream\nendobj\n";
        let out = extract(pdf);
        assert!(out.text.is_empty());
        assert!(out.words.is_empty());
    }

    fn fixture_bytes() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/fixtures/docs/sample.pdf"),
        )
        .expect("contracts/fixtures/docs/sample.pdf exists")
    }
}
