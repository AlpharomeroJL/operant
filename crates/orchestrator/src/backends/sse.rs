//! Line and frame buffering shared by every streaming dialect parser.
//!
//! An [`super::HttpTransport`] response arrives as a sequence of raw byte
//! chunks with no guarantee that a chunk boundary lines up with a line
//! boundary (real sockets never promise that, and this is exactly the case
//! [`super::MockTransport`] is built to exercise on purpose). [`LineBuffer`]
//! absorbs that; [`SseAssembler`] groups the resulting lines into
//! `text/event-stream` events per the SSE framing every `streaming: sse`
//! provider in the quirk table uses.

/// Accumulates raw byte chunks and yields complete, newline-terminated
/// lines. Any trailing partial line is held until the next `push`.
#[derive(Debug, Default)]
pub struct LineBuffer {
    buf: String,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    /// Feed one chunk, return every newly-completed line (without the
    /// trailing `\n` or `\r\n`).
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        let mut out = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let raw: String = self.buf.drain(..=pos).collect();
            let trimmed = raw.trim_end_matches(['\n', '\r']);
            out.push(trimmed.to_string());
        }
        out
    }
}

/// Groups SSE `data:` lines into one payload string per event. A blank line
/// ends an event (joining multi-line `data:` bodies with `\n`, per the SSE
/// spec); `event:`, `id:`, `retry:`, and `:`-comment lines are ignored,
/// which is enough for every dialect in the quirk table, since each of them
/// carries everything this client needs inside the JSON `data:` payload
/// itself.
#[derive(Debug, Default)]
pub struct SseAssembler {
    data: Vec<String>,
}

impl SseAssembler {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Feed one already-dechunked line (see [`LineBuffer`]). Returns
    /// `Some(payload)` when the line completes an event.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        if line.is_empty() {
            if self.data.is_empty() {
                return None;
            }
            let payload = self.data.join("\n");
            self.data.clear();
            return Some(payload);
        }
        if let Some(rest) = line.strip_prefix("data:") {
            self.data
                .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
        // event: / id: / retry: / ":"-comment lines carry nothing this
        // client needs; every provider in the table repeats its event kind
        // inside the JSON `data:` payload.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_buffer_yields_nothing_until_a_newline_arrives() {
        let mut lb = LineBuffer::new();
        assert!(lb.push(b"data: {\"a\":1}").is_empty());
        let lines = lb.push(b"\ndata: {\"b\":2}\n");
        assert_eq!(
            lines,
            vec!["data: {\"a\":1}".to_string(), "data: {\"b\":2}".to_string()]
        );
    }

    #[test]
    fn line_buffer_handles_a_line_split_mid_token_across_chunks() {
        // The exact scenario that makes MockTransport's multi-chunk mode
        // worth having: a single "data:" line arrives in three pieces.
        let mut lb = LineBuffer::new();
        assert!(lb.push(b"data: {\"tex").is_empty());
        assert!(lb.push(b"t\":\"hel").is_empty());
        let lines = lb.push(b"lo\"}\n");
        assert_eq!(lines, vec!["data: {\"text\":\"hello\"}".to_string()]);
    }

    #[test]
    fn line_buffer_strips_crlf() {
        let mut lb = LineBuffer::new();
        let lines = lb.push(b"one\r\ntwo\r\n");
        assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn sse_assembler_emits_on_blank_line_and_ignores_event_lines() {
        let mut sse = SseAssembler::new();
        assert_eq!(sse.push_line("event: message_delta"), None);
        assert_eq!(sse.push_line("data: {\"a\":1}"), None);
        assert_eq!(sse.push_line(""), Some("{\"a\":1}".to_string()));
    }

    #[test]
    fn sse_assembler_joins_multiline_data() {
        let mut sse = SseAssembler::new();
        assert_eq!(sse.push_line("data: line one"), None);
        assert_eq!(sse.push_line("data: line two"), None);
        assert_eq!(sse.push_line(""), Some("line one\nline two".to_string()));
    }

    #[test]
    fn sse_assembler_ignores_repeated_blank_lines() {
        let mut sse = SseAssembler::new();
        assert_eq!(sse.push_line(""), None);
        assert_eq!(sse.push_line("data: x"), None);
        assert_eq!(sse.push_line(""), Some("x".to_string()));
        assert_eq!(sse.push_line(""), None);
    }
}
