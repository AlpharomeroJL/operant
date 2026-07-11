//! Human-readable PDF export of the hash-chained audit log (L6B).
//!
//! The PDF is hand-rolled as a minimal but standards-valid document: a
//! `%PDF-1.4` header, one `/Page` per audit event, a cross-reference (`xref`)
//! table with byte-accurate offsets, and a `trailer` / `startxref` / `%%EOF`
//! footer. Every page prints the chain head hash so a single detached page is
//! self-verifying against the log it came from.
//!
//! No PDF crate is pulled in and no I/O crate is added: [`AuditLog::export_pdf`]
//! renders the whole document into an in-memory `Vec<u8>`, and
//! [`AuditLog::write_pdf`] copies those bytes to a caller-supplied sink. The
//! export path therefore reaches for neither the network nor the filesystem of
//! its own accord (the air-gap tests in `crates/safety/tests/audit_export.rs`
//! assert this structurally and at runtime).

use std::fmt::Write as _;
use std::io;

use super::AuditLog;

/// US Letter media box, in PostScript points.
const PAGE_WIDTH: i64 = 612;
const PAGE_HEIGHT: i64 = 792;
/// Text layout, in points.
const LEFT_MARGIN: i64 = 50;
const TOP_START: i64 = 750;
const LINE_HEIGHT: i64 = 12;
const FONT_SIZE: i64 = 10;
/// Hard character wrap so 64-hex hashes and JSON stay on the page.
const WRAP_COLS: usize = 92;
/// Cap the lines drawn on one page so a pathological payload cannot spill off
/// the bottom of the media box. Normal events render far below this.
const MAX_LINES_PER_PAGE: usize = 60;

impl AuditLog {
    /// Render the chain to a complete, valid PDF document (bytes).
    ///
    /// The layout is one page per event, each page carrying the chain head hash
    /// in its header (so any single page can be checked against the log), the
    /// event's sequence number, timestamp, `prev_hash`, own `hash`, and a
    /// pretty-printed payload. An empty log still yields one valid page.
    ///
    /// The document is built entirely in memory; this method performs no I/O.
    pub fn export_pdf(&self) -> Vec<u8> {
        let head = self.head();
        let pages = render_pages(self, &head);
        assemble_pdf(&pages)
    }

    /// Write the rendered PDF to a caller-supplied sink.
    ///
    /// The sink `w` is the *only* output channel; the method opens no file and
    /// no socket. Callers choose the destination (an in-memory buffer, a file
    /// they already opened, etc.), which keeps the export air-gap friendly.
    pub fn write_pdf<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.export_pdf())
    }
}

/// Build the per-page list of text lines. Each inner `Vec` is one page.
fn render_pages(log: &AuditLog, head: &str) -> Vec<Vec<String>> {
    let events = log.events();
    if events.is_empty() {
        let mut lines = header_lines(head, 0, 1);
        lines.push(String::new());
        lines.push("No events recorded.".to_string());
        return vec![cap_lines(lines)];
    }

    let total = events.len();
    let mut pages = Vec::with_capacity(total);
    for (i, e) in events.iter().enumerate() {
        let mut lines = header_lines(head, i, total);
        lines.push(String::new());
        lines.push(format!("Event seq {}", e.seq));
        lines.push(format!("Timestamp:  {}", e.ts));
        lines.push(format!("Prev hash:  {}", e.prev_hash));
        lines.push(format!("This hash:  {}", e.hash));
        lines.push(String::new());
        lines.push("Payload:".to_string());
        let payload =
            serde_json::to_string_pretty(&e.payload).unwrap_or_else(|_| e.payload.to_string());
        for raw in payload.lines() {
            for wrapped in wrap(raw, WRAP_COLS) {
                lines.push(format!("  {wrapped}"));
            }
        }
        pages.push(cap_lines(lines));
    }
    pages
}

/// The header block printed at the top of every page. The chain head hash is
/// line two so it is present on each page regardless of pagination.
fn header_lines(head: &str, page_idx: usize, total: usize) -> Vec<String> {
    vec![
        "Operant Audit Report".to_string(),
        format!("Chain head: {head}"),
        format!("Page {} of {}", page_idx + 1, total),
        "--------------------------------------------------------".to_string(),
    ]
}

/// Bound a page to [`MAX_LINES_PER_PAGE`] lines, leaving a visible marker if the
/// content was clipped.
fn cap_lines(mut lines: Vec<String>) -> Vec<String> {
    if lines.len() > MAX_LINES_PER_PAGE {
        lines.truncate(MAX_LINES_PER_PAGE - 1);
        lines.push("  [... payload truncated in PDF; see JSONL export ...]".to_string());
    }
    lines
}

/// Assemble the object graph, xref table, and trailer into PDF bytes.
///
/// Object numbering is fixed:
/// * `1` catalog, `2` page tree, `3` font;
/// * page `k` (0-based) uses object `4 + 2k` for the `/Page` and `5 + 2k` for
///   its content stream.
fn assemble_pdf(pages: &[Vec<String>]) -> Vec<u8> {
    let n_pages = pages.len().max(1);
    let total_objs = 3 + 2 * n_pages;
    // 1-based offset table; index 0 is the free entry.
    let mut offsets = vec![0usize; total_objs + 1];

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    // A comment line of high bytes marks the file as binary for transfer tools.
    buf.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

    // Object 1: document catalog.
    offsets[1] = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // Object 2: page tree.
    offsets[2] = buf.len();
    let mut kids = String::new();
    for k in 0..n_pages {
        let _ = write!(kids, "{} 0 R ", 4 + 2 * k);
    }
    let pages_obj = format!(
        "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
        kids.trim_end(),
        n_pages
    );
    buf.extend_from_slice(pages_obj.as_bytes());

    // Object 3: the one shared font.
    offsets[3] = buf.len();
    buf.extend_from_slice(
        b"3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    // Page + content-stream objects.
    for (k, lines) in pages.iter().enumerate() {
        let page_obj = 4 + 2 * k;
        let contents_obj = 5 + 2 * k;

        offsets[page_obj] = buf.len();
        let page = format!(
            "{page_obj} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {contents_obj} 0 R >>\nendobj\n"
        );
        buf.extend_from_slice(page.as_bytes());

        offsets[contents_obj] = buf.len();
        let stream = page_content_stream(lines);
        let head = format!("{contents_obj} 0 obj\n<< /Length {} >>\nstream\n", stream.len());
        buf.extend_from_slice(head.as_bytes());
        buf.extend_from_slice(stream.as_bytes());
        buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    // Cross-reference table. Each entry is exactly 20 bytes.
    let xref_offset = buf.len();
    let size = total_objs + 1;
    let mut xref = String::new();
    let _ = writeln!(xref, "xref\n0 {size}");
    xref.push_str("0000000000 65535 f \n");
    // Entries for objects 1..=total_objs (index 0 is the free entry above).
    for off in offsets.iter().skip(1) {
        // Each entry is exactly 20 bytes: 10-digit offset, gen, type, EOL.
        let _ = writeln!(xref, "{off:010} 00000 n ");
    }
    buf.extend_from_slice(xref.as_bytes());

    // Trailer + pointer back to the xref table.
    let trailer = format!(
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    );
    buf.extend_from_slice(trailer.as_bytes());

    buf
}

/// Emit the content stream that draws one page's lines with the Helvetica font.
fn page_content_stream(lines: &[String]) -> String {
    let mut c = String::new();
    c.push_str("BT\n");
    let _ = writeln!(c, "/F1 {FONT_SIZE} Tf");
    let _ = writeln!(c, "{LINE_HEIGHT} TL");
    let _ = writeln!(c, "{LEFT_MARGIN} {TOP_START} Td");
    for (i, line) in lines.iter().enumerate() {
        let esc = escape_pdf_text(line);
        if i == 0 {
            // First line drawn at the Td origin.
            let _ = writeln!(c, "({esc}) Tj");
        } else {
            // The quote operator advances one line (by TL) then shows the text.
            let _ = writeln!(c, "({esc}) '");
        }
    }
    c.push_str("ET\n");
    c
}

/// Escape a string for a PDF literal `( ... )` text object. Parentheses and
/// backslashes are escaped; control characters are dropped; non-ASCII is folded
/// to `?` so the WinAnsi-encoded Helvetica stays within range and the stream
/// remains pure ASCII (so its byte length equals its char length).
fn escape_pdf_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c if (c as u32) < 0x20 => {}
            c if (c as u32) < 0x7f => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

/// Hard-wrap a line to at most `cols` characters. Used for long JSON values and
/// 64-hex hashes; it never breaks on word boundaries, which is fine for a
/// fixed-width audit dump.
fn wrap(s: &str, cols: usize) -> Vec<String> {
    let count = s.chars().count();
    if count <= cols {
        return vec![s.to_string()];
    }
    let mut out = Vec::with_capacity(count / cols + 1);
    let mut cur = String::new();
    let mut n = 0usize;
    for ch in s.chars() {
        cur.push(ch);
        n += 1;
        if n == cols {
            out.push(std::mem::take(&mut cur));
            n = 0;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn built_chain() -> AuditLog {
        let mut log = AuditLog::new();
        log.append("t0", json!({ "event": "workflow.install", "name": "notepad-invoice-note" }));
        log.append("t1", json!({ "event": "action.executed", "id": "s1", "outcome": "ok" }));
        log.append("t2", json!({ "event": "approval", "reason": "credential_field" }));
        log
    }

    #[test]
    fn export_is_a_valid_pdf_shell() {
        let pdf = built_chain().export_pdf();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(find(&pdf, b"xref").is_some());
        assert!(find(&pdf, b"startxref").is_some());
        assert!(find(&pdf, b"trailer").is_some());
        assert!(pdf.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn head_hash_and_event_text_are_present() {
        let log = built_chain();
        let pdf = log.export_pdf();
        assert!(find(&pdf, log.head().as_bytes()).is_some(), "head hash must appear");
        assert!(find(&pdf, b"workflow.install").is_some(), "event text must appear");
    }

    #[test]
    fn one_page_per_event() {
        let log = built_chain();
        let pdf = log.export_pdf();
        assert_eq!(count(&pdf, b"/Type /Page "), log.events().len());
    }

    #[test]
    fn empty_log_still_exports_one_valid_page() {
        let pdf = AuditLog::new().export_pdf();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert_eq!(count(&pdf, b"/Type /Page "), 1);
        // Empty chain head is GENESIS; it must still be printed.
        assert!(find(&pdf, super::super::GENESIS.as_bytes()).is_some());
    }

    #[test]
    fn parens_in_payload_are_escaped() {
        let mut log = AuditLog::new();
        log.append("t0", json!({ "note": "value (with) parens \\ slash" }));
        let pdf = log.export_pdf();
        // The raw unescaped sequence "(with)" must not leak into the stream;
        // it is escaped as \(with\).
        assert!(find(&pdf, b"\\(with\\)").is_some());
    }

    // --- tiny byte-substring helpers (no external crate) ---

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || needle.len() > hay.len() {
            return None;
        }
        (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
    }

    fn count(hay: &[u8], needle: &[u8]) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            if &hay[i..i + needle.len()] == needle {
                n += 1;
                i += needle.len();
            } else {
                i += 1;
            }
        }
        n
    }
}
