//! Input synthesizer: the trait every input backend implements, a
//! deterministic mock every test in this crate runs against, and the
//! panic-safe modifier release-all sweep the kill switch depends on.
//!
//! See `docs/specs/action.md`: modifier keys are always paired press and
//! release, with a panic-time release-all sweep as the backstop. That sweep
//! has to be exercisable without a real keyboard, so it is a first-class
//! [`Synthesizer`] method rather than something buried inside the real
//! Windows backend.

use std::collections::VecDeque;
use std::fmt;

use operant_ir::{Coords, WindowMatch};
use parking_lot::Mutex;
use thiserror::Error;

/// Direction for a `scroll` dispatch. Mirrors the `direction` string enum
/// carried in `contracts/action_ir.schema.json`'s free-form `params` object
/// (not a first-class IR type there, since `params` is kind-specific JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDirection {
    /// Parse the `direction` string from an Action IR `scroll` step's
    /// params. Returns `None` for anything outside the schema's enum.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

impl fmt::Display for ScrollDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        };
        f.write_str(s)
    }
}

/// Errors a [`Synthesizer`] backend can report. Kept small and coarse on
/// purpose: retry policy is decided by [`crate::executor::ActionError`],
/// not by matching on these variants at the call site.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SynthesizerError {
    #[error("failed to focus window: {0}")]
    Focus(String),
    #[error("input synthesis failed: {0}")]
    Input(String),
    #[error("clipboard operation failed: {0}")]
    Clipboard(String),
    #[error("operation not available on this backend: {0}")]
    Unavailable(String),
    #[error("input synthesis refused: the process-global freeze (kill switch) is engaged")]
    Frozen,
}

/// Everything the Action executor needs to turn an Action IR step into
/// real (or, for [`MockSynthesizer`], recorded) input.
///
/// One trait, two backends: [`MockSynthesizer`] backs every test in the
/// workspace; the Windows SendInput backend (`real_win`, behind the
/// `real-input` feature) backs production use. Object-safe by design so an
/// `Executor` can be built generically or, later, against `dyn Synthesizer`
/// if a caller needs to pick a backend at runtime.
pub trait Synthesizer: Send + Sync {
    /// Bring the target window to the foreground. Called before any
    /// keystroke or click that targets a specific window.
    fn focus_window(&self, window: &WindowMatch) -> Result<(), SynthesizerError>;

    /// Send a key combo, e.g. `"ctrl+s"`. Implementations press modifiers
    /// in left-to-right order and release them in reverse order, and must
    /// guarantee release even when the combo fails partway through (hence
    /// [`Synthesizer::release_all_modifiers`] as a separate, unconditional
    /// primitive callers can always fall back to).
    fn key(&self, combo: &str) -> Result<(), SynthesizerError>;

    /// Type literal text. Real backends use `KEYEVENTF_UNICODE` so the
    /// active keyboard layout can never corrupt the characters sent.
    fn type_text(&self, text: &str) -> Result<(), SynthesizerError>;

    /// Click the primary mouse button at an already-resolved screen point.
    fn click_point(&self, point: Coords) -> Result<(), SynthesizerError>;

    /// Scroll the element under the cursor / current focus. Prefers the
    /// UIA Scroll pattern with a wheel-event fallback (`docs/specs/action.md`);
    /// pattern-aware scrolling needs perception and lands in a later lane,
    /// so every backend here is the wheel fallback.
    fn scroll(&self, direction: ScrollDirection, amount: f64) -> Result<(), SynthesizerError>;

    /// Read the current clipboard text contents.
    fn clipboard_get(&self) -> Result<String, SynthesizerError>;

    /// Overwrite the clipboard text contents.
    fn clipboard_set(&self, text: &str) -> Result<(), SynthesizerError>;

    /// Unconditionally release every modifier key (ctrl/alt/shift/win).
    /// Idempotent: safe to call even when nothing is held down. This is
    /// the primitive both [`ModifierReleaseGuard`] and a future kill
    /// switch call to guarantee no modifier is ever left stuck down.
    fn release_all_modifiers(&self) -> Result<(), SynthesizerError>;
}

/// RAII guard that sweeps all modifier keys when dropped while still
/// armed, including when the drop happens during a panic unwind. Armed by
/// default; the executor disarms it immediately after a dispatch attempt
/// returns normally (`Ok` or a typed `Err` both count: the retry loop
/// already handles ordinary failures) so only an actual panic mid-dispatch
/// leaves it armed at Drop time. That is deliberate: `docs/specs/action.md`
/// calls this out as a *panic-time* release-all sweep, the kill switch's
/// backstop, not a routine post-action cleanup step.
pub struct ModifierReleaseGuard<'a, S: Synthesizer + ?Sized> {
    synth: &'a S,
    armed: bool,
}

impl<'a, S: Synthesizer + ?Sized> ModifierReleaseGuard<'a, S> {
    pub fn new(synth: &'a S) -> Self {
        Self { synth, armed: true }
    }

    /// Suppress the sweep this guard would otherwise run on drop. Callers
    /// disarm once they know control is leaving normally rather than by
    /// unwinding through a panic.
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<'a, S: Synthesizer + ?Sized> Drop for ModifierReleaseGuard<'a, S> {
    fn drop(&mut self) {
        if self.armed {
            // A Drop impl must never panic (doing so during an active
            // unwind aborts the process), so this is best-effort.
            let _ = self.synth.release_all_modifiers();
        }
    }
}

/// One recorded call to a [`MockSynthesizer`], in call order. Every field
/// is owned so a test can snapshot [`MockSynthesizer::calls`] and compare
/// it against a hand-written expectation.
#[derive(Debug, Clone, PartialEq)]
pub enum SynthCall {
    FocusWindow(WindowMatch),
    Key(String),
    TypeText(String),
    ClickPoint(Coords),
    Scroll {
        direction: ScrollDirection,
        amount: f64,
    },
    ClipboardGet,
    ClipboardSet(String),
    ReleaseAllModifiers,
}

/// Deterministic, in-memory [`Synthesizer`] used by every test in the
/// workspace. Records every call in order; can be told to fail or panic
/// the *next* call so tests can exercise the executor's retry loop and
/// panic-time modifier sweep without a real display or keyboard.
#[derive(Default)]
pub struct MockSynthesizer {
    calls: Mutex<Vec<SynthCall>>,
    clipboard: Mutex<String>,
    fail_queue: Mutex<VecDeque<SynthesizerError>>,
    panic_next: Mutex<bool>,
}

impl MockSynthesizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// All calls recorded so far, in order.
    pub fn calls(&self) -> Vec<SynthCall> {
        self.calls.lock().clone()
    }

    /// Number of calls recorded so far.
    pub fn call_count(&self) -> usize {
        self.calls.lock().len()
    }

    /// Queue one more failure: the next call into this synthesizer (any
    /// method) returns `err` instead of succeeding, then subsequent calls
    /// go back to succeeding (or return the next queued failure, if any
    /// more were queued). Calling this N times before N expected attempts
    /// is how a test drives the executor's retry loop to exhaustion.
    pub fn fail_next_call(&self, err: SynthesizerError) {
        self.fail_queue.lock().push_back(err);
    }

    /// The next call into this synthesizer (any method) panics instead of
    /// returning. Used to test the panic-time modifier release sweep.
    pub fn panic_next_call(&self) {
        *self.panic_next.lock() = true;
    }

    fn record(&self, call: SynthCall) -> Result<(), SynthesizerError> {
        if std::mem::take(&mut *self.panic_next.lock()) {
            panic!("MockSynthesizer: simulated panic on {call:?}");
        }
        self.calls.lock().push(call);
        match self.fail_queue.lock().pop_front() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

impl Synthesizer for MockSynthesizer {
    fn focus_window(&self, window: &WindowMatch) -> Result<(), SynthesizerError> {
        self.record(SynthCall::FocusWindow(window.clone()))
    }

    fn key(&self, combo: &str) -> Result<(), SynthesizerError> {
        self.record(SynthCall::Key(combo.to_string()))
    }

    fn type_text(&self, text: &str) -> Result<(), SynthesizerError> {
        self.record(SynthCall::TypeText(text.to_string()))
    }

    fn click_point(&self, point: Coords) -> Result<(), SynthesizerError> {
        self.record(SynthCall::ClickPoint(point))
    }

    fn scroll(&self, direction: ScrollDirection, amount: f64) -> Result<(), SynthesizerError> {
        self.record(SynthCall::Scroll { direction, amount })
    }

    fn clipboard_get(&self) -> Result<String, SynthesizerError> {
        self.record(SynthCall::ClipboardGet)?;
        Ok(self.clipboard.lock().clone())
    }

    fn clipboard_set(&self, text: &str) -> Result<(), SynthesizerError> {
        self.record(SynthCall::ClipboardSet(text.to_string()))?;
        *self.clipboard.lock() = text.to_string();
        Ok(())
    }

    fn release_all_modifiers(&self) -> Result<(), SynthesizerError> {
        self.record(SynthCall::ReleaseAllModifiers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{self, AssertUnwindSafe};

    #[test]
    fn mock_records_calls_in_order() {
        let mock = MockSynthesizer::new();
        let window = WindowMatch {
            process: Some("notepad.exe".into()),
            title_pattern: None,
        };
        mock.focus_window(&window).unwrap();
        mock.key("ctrl+s").unwrap();
        mock.type_text("hi").unwrap();
        mock.click_point(Coords {
            x: 1.0,
            y: 2.0,
            monitor: None,
            dpi_scale: None,
        })
        .unwrap();
        mock.scroll(ScrollDirection::Down, 3.0).unwrap();

        assert_eq!(
            mock.calls(),
            vec![
                SynthCall::FocusWindow(window),
                SynthCall::Key("ctrl+s".into()),
                SynthCall::TypeText("hi".into()),
                SynthCall::ClickPoint(Coords {
                    x: 1.0,
                    y: 2.0,
                    monitor: None,
                    dpi_scale: None
                }),
                SynthCall::Scroll {
                    direction: ScrollDirection::Down,
                    amount: 3.0
                },
            ]
        );
    }

    #[test]
    fn mock_clipboard_round_trips() {
        let mock = MockSynthesizer::new();
        mock.clipboard_set("payload").unwrap();
        assert_eq!(mock.clipboard_get().unwrap(), "payload");
        assert_eq!(mock.call_count(), 2);
    }

    #[test]
    fn mock_fail_next_call_fires_once() {
        let mock = MockSynthesizer::new();
        mock.fail_next_call(SynthesizerError::Input("boom".into()));
        let first = mock.key("ctrl+s");
        assert_eq!(first, Err(SynthesizerError::Input("boom".into())));
        // The failure is one-shot; the retried call succeeds.
        assert!(mock.key("ctrl+s").is_ok());
        // Both attempts were still recorded: a failing call is a real
        // attempt, not a no-op.
        assert_eq!(mock.call_count(), 2);
    }

    #[test]
    fn scroll_direction_parses_schema_enum() {
        assert_eq!(ScrollDirection::parse("up"), Some(ScrollDirection::Up));
        assert_eq!(ScrollDirection::parse("down"), Some(ScrollDirection::Down));
        assert_eq!(ScrollDirection::parse("left"), Some(ScrollDirection::Left));
        assert_eq!(
            ScrollDirection::parse("right"),
            Some(ScrollDirection::Right)
        );
        assert_eq!(ScrollDirection::parse("diagonal"), None);
    }

    #[test]
    fn modifier_guard_sweeps_on_normal_drop() {
        let mock = MockSynthesizer::new();
        {
            let _guard = ModifierReleaseGuard::new(&mock);
        }
        assert_eq!(mock.calls(), vec![SynthCall::ReleaseAllModifiers]);
    }

    #[test]
    fn modifier_guard_sweeps_on_panic_unwind() {
        let mock = MockSynthesizer::new();
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {})); // keep the deliberate panic quiet
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = ModifierReleaseGuard::new(&mock);
            panic!("simulated kill switch");
        }));
        panic::set_hook(prev_hook);

        assert!(result.is_err(), "expected the simulated panic to unwind");
        assert_eq!(
            mock.calls(),
            vec![SynthCall::ReleaseAllModifiers],
            "the guard must sweep modifiers even when the scope unwinds via panic"
        );
    }
}
