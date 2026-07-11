//! Kill switch (C20 / FR-S5, `docs/specs/guardian.md`): the process-wide
//! freeze [`crate::executor::Executor`] checks before every dispatch
//! attempt, below the planner, so nothing upstream of this crate has to
//! know the switch exists for it to work.
//!
//! [`is_engaged`] is the fast path: one lock-free atomic load, safe to
//! call before every synthesizer batch, with no lock, no bus, and no
//! allocation on the read side. [`engage`] is the "stop right now" path,
//! called exactly once per freeze by a human or by the panic-chord hook
//! below: it flips the atomic first, so the freeze is already in effect
//! the instant `engage` returns, then publishes `killswitch.engaged` on
//! the bus for everything downstream (tray, run recorder, orchestrator)
//! that wants to react. [`reset`] is the explicit human resume; nothing
//! in this crate ever calls it automatically.
//!
//! The atomic, [`engage`], [`reset`], and the executor's check are always
//! built and exercised by `cargo test` with no OS dependency
//! (`crates/action/tests/killswitch.rs`). The WH_KEYBOARD_LL panic-chord
//! watcher that calls `engage` from a real key combo is Windows-only and
//! lives in [`hook`], behind this crate's existing `real-input` feature.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use operant_core::bus::events::KillswitchEngaged;
use operant_core::Bus;

/// Process-wide freeze flag. `SeqCst` throughout: this interlock gates
/// real input synthesis, so the extra fence over `Acquire`/`Release` is
/// cheap insurance for the simplest possible cross-thread reasoning. It
/// is checked once per dispatch attempt, never in a hot loop, so there is
/// no performance case for relaxing it.
static KILL: AtomicBool = AtomicBool::new(false);

/// True while the kill switch is engaged. This is the executor's freeze
/// check: call it before every synthesizer batch. Lock-free and
/// allocation-free, independent of the bus, so it costs nothing to check
/// on every dispatch attempt and can never block on anything the
/// panic-chord hook thread might be holding.
pub fn is_engaged() -> bool {
    KILL.load(Ordering::SeqCst)
}

/// Engage the kill switch. Freezes dispatch immediately: the atomic store
/// is the first thing this function does, before the bus is touched at
/// all, so a caller measuring engage-to-frozen latency is timing the
/// flip, not the notification. After the flip, publishes
/// `killswitch.engaged{at_ms}` on `bus` so the rest of the system (tray,
/// run recorder, orchestrator) can react. Returns the `at_ms` stamped on
/// the event.
///
/// The bus publish is best-effort. `KillswitchEngaged` is a two-field
/// struct of plain integers, so serialization failure is not a realistic
/// outcome, but even if it were: the freeze has already taken effect and
/// stays in effect regardless of whether the notification succeeds. A
/// failed notification must never be allowed to look like a failure to
/// engage, so its error is dropped rather than surfaced.
pub fn engage(bus: &Bus) -> u64 {
    KILL.store(true, Ordering::SeqCst);
    let at_ms = now_ms();
    let _ = bus.publish_event(&KillswitchEngaged { at_ms });
    at_ms
}

/// The explicit human resume (`docs/specs/guardian.md`: "Resume is
/// per-run and explicit"). Clears the freeze so the executor dispatches
/// again. Idempotent: resetting an already-clear switch is a no-op.
pub fn reset() {
    KILL.store(false, Ordering::SeqCst);
}

/// Wall-clock milliseconds since the Unix epoch, for the `at_ms` stamped
/// on `killswitch.engaged`. Falls back to 0 only if the system clock is
/// somehow set before 1970, which must never turn into a panic on the
/// kill switch's own hot path.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The WH_KEYBOARD_LL panic-chord watcher described in
/// `docs/specs/guardian.md`. Windows only; behind the `real-input`
/// feature so the default build, and every headless test, never links
/// user32's hook APIs.
#[cfg(feature = "real-input")]
pub mod hook {
    use std::sync::{Arc, OnceLock};

    use operant_core::Bus;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    /// The bus the running hook publishes `killswitch.engaged` to.
    /// `SetWindowsHookExW`'s callback is a bare `extern "system" fn` with
    /// no user-data slot, so a static is the only way to hand it
    /// anything; [`spawn`] fills this in before the hook can possibly
    /// fire, and it is never written again after that.
    static HOOK_BUS: OnceLock<Arc<Bus>> = OnceLock::new();

    /// High bit of a `GetAsyncKeyState` result: "currently down" per
    /// `WinUser.h`. Written as the plain `i16` `GetAsyncKeyState` actually
    /// returns rather than the `0x8000u16` most docs quote, since the
    /// high bit of a negative `i16` is exactly this constant.
    const KEY_DOWN_BIT: i16 = -32768;

    fn is_down(vk: VIRTUAL_KEY) -> bool {
        unsafe { GetAsyncKeyState(vk.0 as i32) & KEY_DOWN_BIT != 0 }
    }

    /// True while every modifier in the default panic chord
    /// (Ctrl+Alt+Shift+Space) is held. Only the three modifiers are
    /// polled here; the Space half of the chord is the key event that
    /// triggers [`hook_proc`] in the first place. A configurable chord is
    /// a FOLLOWUP (see the lane result); this default matches
    /// `docs/specs/guardian.md`.
    fn modifiers_held() -> bool {
        is_down(VK_CONTROL) && is_down(VK_MENU) && is_down(VK_SHIFT)
    }

    /// The low-level keyboard hook procedure. Runs on the thread that
    /// called [`spawn`]'s `SetWindowsHookExW`, per WH_KEYBOARD_LL's
    /// documented behavior. Must call `CallNextHookEx` unconditionally so
    /// this watcher never swallows a keystroke meant for anything else.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let msg = wparam.0 as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                if info.vkCode == VK_SPACE.0 as u32 && modifiers_held() {
                    if let Some(bus) = HOOK_BUS.get() {
                        super::engage(bus);
                    }
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Install the panic-chord hook on a dedicated thread and pump its
    /// message loop for the life of the process. `docs/specs/guardian.md`:
    /// "a low-level keyboard hook... registered by a tiny dedicated
    /// thread." WH_KEYBOARD_LL is thread-affine (the hook only fires on
    /// the thread that installed it), so installation and the message
    /// loop that keeps it alive both have to run on the same thread this
    /// function spawns.
    pub fn spawn(bus: Arc<Bus>) -> std::thread::JoinHandle<()> {
        let _ = HOOK_BUS.set(bus);
        std::thread::spawn(|| unsafe {
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)
                .expect("SetWindowsHookExW(WH_KEYBOARD_LL) failed");
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(hook);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only non-global-mutating checks belong here. `engage`/`reset` flip a
    // process-wide static that `crate::executor`'s own unit tests rely on
    // reading as "not engaged": this file's unit tests share one test
    // binary with those (cargo runs `--lib` tests on multiple threads in
    // one process), so a test here that engaged the switch could freeze
    // an unrelated executor test running concurrently. Every test that
    // actually calls `engage`/`reset` lives in
    // `crates/action/tests/killswitch.rs` instead, which cargo builds as
    // its own process and so cannot race this crate's other unit tests.

    #[test]
    fn starts_out_not_engaged() {
        assert!(!is_engaged());
    }

    #[test]
    fn now_ms_is_a_plausible_unix_timestamp() {
        // Bounds: after 2020-01-01 and before 2100-01-01, generously wide
        // so this is not a flaky "exact clock" assertion, just a sanity
        // check that `now_ms` did not fall through to the 0 fallback.
        let ms = now_ms();
        assert!(ms > 1_577_836_800_000, "expected a post-2020 timestamp");
        assert!(ms < 4_102_444_800_000, "expected a pre-2100 timestamp");
    }
}
