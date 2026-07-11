//! Capability-probe support: the context-length lookup table and an
//! RFC3339 `probed_at` timestamp formatter. Factored out of `client.rs`
//! because both `HttpBackend` and the mock backends need them.

use std::time::{SystemTime, UNIX_EPOCH};

/// Reported-model-name substrings mapped to a context window size, checked
/// in order (first match wins). Per `contracts/model_backend.md`'s probe
/// protocol: "context length from a lookup table keyed by reported model
/// name, conservative default 8192."
const CONTEXT_LENGTHS: &[(&str, u32)] = &[
    ("gpt-4o", 128_000),
    ("gpt-4.1", 1_047_576),
    ("o1", 200_000),
    ("o3", 200_000),
    ("claude-opus-4", 200_000),
    ("claude-sonnet-4", 200_000),
    ("claude-3-5", 200_000),
    ("claude-3", 200_000),
    ("gemini-1.5-pro", 2_000_000),
    ("gemini-1.5-flash", 1_000_000),
    ("gemini-2", 1_000_000),
    ("llama3.1", 128_000),
    ("llama3.2", 128_000),
    ("llama3", 8_192),
    ("mixtral", 32_768),
    ("mistral-large", 128_000),
    ("qwen2.5", 128_000),
    ("deepseek", 64_000),
];

/// Conservative default when the model name matches nothing in the table.
pub const DEFAULT_CONTEXT_LENGTH: u32 = 8192;

/// Context length for a reported model name: case-insensitive substring
/// match, first hit wins, [`DEFAULT_CONTEXT_LENGTH`] otherwise.
pub fn context_length_for(model: &str) -> u32 {
    let lower = model.to_ascii_lowercase();
    CONTEXT_LENGTHS
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, len)| *len)
        .unwrap_or(DEFAULT_CONTEXT_LENGTH)
}

/// Current time as RFC3339 UTC (`probed_at`'s wire format).
///
/// Dependency-free on purpose: `probed_at` is the one timestamp field in
/// this whole crate, so pulling in a datetime crate for it is not worth the
/// extra build weight the "keep `cargo build --workspace` lean" instruction
/// asks for.
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_rfc3339(secs)
}

/// Format a Unix-epoch second count as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_rfc3339(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's `civil_from_days`: converts a day count since the Unix
/// epoch into a proleptic-Gregorian (year, month, day). Public-domain
/// algorithm (see howardhinnant.github.io/date_algorithms.html), reproduced
/// here rather than pulled in as a dependency.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_length_matches_known_model_substrings() {
        assert_eq!(context_length_for("claude-sonnet-4-20250514"), 200_000);
        assert_eq!(context_length_for("gpt-4o-mini"), 128_000);
        assert_eq!(context_length_for("gemini-1.5-pro-latest"), 2_000_000);
        assert_eq!(context_length_for("llama3.1:8b"), 128_000);
    }

    #[test]
    fn context_length_falls_back_to_conservative_default() {
        assert_eq!(
            context_length_for("some-brand-new-unlisted-model"),
            DEFAULT_CONTEXT_LENGTH
        );
        assert_eq!(DEFAULT_CONTEXT_LENGTH, 8192);
    }

    // The reference Unix timestamps below were computed independently via
    // .NET's DateTimeOffset (not derived from this file's own algorithm),
    // so a bug shared between the implementation and the test cannot hide.
    #[test]
    fn epoch_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn y2k_leap_day_boundary_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(951_868_800), "2000-03-01T00:00:00Z");
    }

    #[test]
    fn leap_day_with_a_time_of_day_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(1_709_210_096), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn year_end_boundary_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(1_767_225_599), "2025-12-31T23:59:59Z");
    }

    #[test]
    fn contract_example_timestamp_formats_as_rfc3339() {
        // contracts/model_backend.md's own BackendProfile example carries
        // "probed_at": "2026-07-11T00:00:00Z".
        assert_eq!(format_rfc3339(1_783_728_000), "2026-07-11T00:00:00Z");
    }

    #[test]
    fn now_rfc3339_looks_like_rfc3339() {
        let s = now_rfc3339();
        assert_eq!(s.len(), 20, "YYYY-MM-DDTHH:MM:SSZ is 20 chars: {s}");
        assert!(s.starts_with("20"), "sanity: we are not in the 1900s: {s}");
        assert!(s.ends_with('Z'));
    }
}
