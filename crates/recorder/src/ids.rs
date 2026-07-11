//! Id generation and timestamps.
//!
//! Ids are monotonic-ish, human-scannable strings (`prefix_<time-hex>_<counter-hex>`),
//! not a formal ULID: uniqueness comes from a process-wide atomic counter combined with
//! nanosecond time, which is sufficient for a single-machine recorder and avoids pulling
//! in an extra dependency. Timestamps are stored as epoch milliseconds (INTEGER columns)
//! rather than formatted strings, so no calendar/timezone logic lives in this crate.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh, unique id with the given prefix (e.g. `"run"`, `"step"`).
pub(crate) fn new_id(prefix: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}_{now:x}_{n:x}")
}

/// Current wall-clock time as epoch milliseconds.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = new_id("run");
        let b = new_id("run");
        assert_ne!(a, b);
        assert!(a.starts_with("run_"));
        assert!(b.starts_with("run_"));
    }

    #[test]
    fn now_ms_is_plausible() {
        // Sanity bound: after 2020-01-01 and before year 2100, as a guard against
        // an obviously broken clock computation.
        let ms = now_ms();
        assert!(ms > 1_577_836_800_000);
        assert!(ms < 4_102_444_800_000);
    }
}
