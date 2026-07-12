//! Windows SendInput backend, behind the `real-input` cargo feature so the
//! default build never links the `windows` crate.
//!
//! Minimal by design (`docs/specs/action.md`): SendInput with
//! `KEYEVENTF_UNICODE` for text, explicit modifier press/release pairing
//! for combos, and an unconditional release-all sweep every backend must
//! expose so [`crate::synth::ModifierReleaseGuard`] and the kill switch can
//! rely on it.
//!
//! Two engine fixes live here (`docs/specs/ipc-bridge.md` section 6):
//! - E1 focus: [`WindowsSynthesizer::focus_window`] enumerates the live
//!   top-level windows and REGEX-matches the IR `title_pattern` (also honoring
//!   `process`), instead of handing the pattern literally to `FindWindowW`,
//!   which only ever did exact-title matching and so never resolved a real
//!   window. The matching logic itself is OS-free and unit tested in
//!   [`crate::focus`].
//! - E2 focus-then-verify: after focusing, [`WindowsSynthesizer::focus_window`]
//!   re-reads the target's UI thread to confirm a control actually holds
//!   keyboard focus before it returns, so the executor's subsequent type/key
//!   is not fire-and-hope.
//!
//! SAFETY: every real input call first checks the process-global freeze
//! (`operant_core::safety::is_frozen`) and refuses with
//! [`SynthesizerError::Frozen`] the instant it is engaged, so the panic button
//! stops a live loop before any keystroke, click, cursor move, or clipboard
//! write reaches the OS. The release-all-modifiers sweep is deliberately NOT
//! frozen: un-sticking held modifiers is the safe response to a freeze.

use operant_ir::{Coords, WindowMatch};
use operant_core::safety;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, SetFocus, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetGUIThreadInfo, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, GUITHREADINFO,
};

use crate::focus::{pick_window, WindowCandidate};
use crate::synth::{ScrollDirection, Synthesizer, SynthesizerError};

/// `CF_UNICODETEXT`: stable Win32 ABI constant (`WinUser.h`), not currently
/// exported by the `windows` crate's `System::DataExchange` bindings under
/// that name, so it is inlined here rather than chased through modules.
const CF_UNICODETEXT: u32 = 13;

/// Modifier virtual-key codes swept by [`Synthesizer::release_all_modifiers`],
/// in release order. Left/right variants collapse to these generic codes,
/// which Windows treats as aliases of whichever side is actually down.
const MODIFIER_VKS: [VIRTUAL_KEY; 4] = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN];

/// SendInput-backed [`Synthesizer`]. Stateless: every call resolves fresh
/// state (foreground window, cursor position) rather than caching
/// anything, matching `docs/specs/action.md`'s "never cached coordinates"
/// rule.
#[derive(Default)]
pub struct WindowsSynthesizer;

impl WindowsSynthesizer {
    pub fn new() -> Self {
        Self
    }
}

/// The process-global freeze gate (SAFETY). Checked at the top of every real
/// input call so a frozen synthesizer refuses BEFORE touching the OS. Returns
/// [`SynthesizerError::Frozen`] when engaged. The release-all-modifiers sweep
/// intentionally does not call this: releasing stuck keys is the safe response
/// to a freeze, not a continuation of the action.
fn frozen_guard() -> Result<(), SynthesizerError> {
    if safety::is_frozen() {
        Err(SynthesizerError::Frozen)
    } else {
        Ok(())
    }
}

fn send(inputs: &[INPUT]) -> Result<(), SynthesizerError> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(SynthesizerError::Input(format!(
            "SendInput dispatched {sent} of {} queued events (last_error={:?})",
            inputs.len(),
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

fn key_event(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// One UTF-16 code unit as a Unicode key-down or key-up event. `wVk` stays
/// zero: `KEYEVENTF_UNICODE` tells Windows to synthesize the character
/// straight from `wScan`, bypassing the active keyboard layout entirely
/// (`docs/specs/action.md`: "keyboard layout never corrupts text").
fn unicode_event(unit: u16, key_up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_event(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            // MOUSEINPUT.mouseData is a DWORD, but MOUSEEVENTF_WHEEL wants a
            // signed delta in it; `as u32` is the correct two's-complement
            // reinterpretation Windows itself expects here.
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn modifier_vk(token: &str) -> Option<VIRTUAL_KEY> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(VK_CONTROL),
        "alt" | "menu" => Some(VK_MENU),
        "shift" => Some(VK_SHIFT),
        "win" | "meta" | "super" => Some(VK_LWIN),
        _ => None,
    }
}

/// Resolve the non-modifier tail of a combo (e.g. the `s` in `ctrl+s`) to a
/// virtual-key code via the active layout's `VkKeyScanW`.
fn main_key_vk(token: &str) -> Option<VIRTUAL_KEY> {
    if let Some(vk) = modifier_vk(token) {
        return Some(vk);
    }
    let ch = token.chars().next()?;
    let mut units = [0u16; 2];
    let encoded = ch.encode_utf16(&mut units);
    let packed = unsafe { VkKeyScanW(encoded[0]) };
    if packed == -1 {
        return None;
    }
    Some(VIRTUAL_KEY((packed as u16) & 0x00FF))
}

impl Synthesizer for WindowsSynthesizer {
    fn focus_window(&self, window: &WindowMatch) -> Result<(), SynthesizerError> {
        frozen_guard()?;

        // E1: resolve the target HWND by enumerating live top-level windows and
        // REGEX-matching `title_pattern` (and `process`), instead of the old
        // literal `FindWindowW`, which did exact-title matching and so never
        // resolved a regex like `.* - Notepad`.
        let candidates = enumerate_top_level_windows();
        let hwnd = match pick_window(&candidates, window)? {
            Some(found) => HWND(found.hwnd as *mut _),
            None => {
                return Err(SynthesizerError::Focus(format!(
                    "no live top-level window matched {window:?} \
                     (searched {} enumerated windows)",
                    candidates.len()
                )));
            }
        };

        focus_with_attach_workaround(hwnd)?;

        // E2 focus-then-verify: re-read the target's UI thread and confirm a
        // control actually holds keyboard focus before returning, so the
        // executor's subsequent type/key is not fire-and-hope.
        verify_focus_landed(hwnd)
    }

    fn key(&self, combo: &str) -> Result<(), SynthesizerError> {
        frozen_guard()?;
        let mut tokens: Vec<&str> = combo.split('+').filter(|t| !t.is_empty()).collect();
        let Some(main_token) = tokens.pop() else {
            return Err(SynthesizerError::Input("empty key combo".into()));
        };
        let modifiers: Vec<VIRTUAL_KEY> = tokens.iter().filter_map(|t| modifier_vk(t)).collect();
        let main_vk = main_key_vk(main_token).ok_or_else(|| {
            SynthesizerError::Input(format!(
                "unrecognized key `{main_token}` in combo `{combo}`"
            ))
        })?;

        // Press modifiers in order, tap the main key, release modifiers in
        // reverse order: docs/specs/action.md's "modifier keys always
        // paired press/release."
        let mut events = Vec::with_capacity(modifiers.len() * 2 + 2);
        for vk in &modifiers {
            events.push(key_event(*vk, KEYBD_EVENT_FLAGS(0)));
        }
        events.push(key_event(main_vk, KEYBD_EVENT_FLAGS(0)));
        events.push(key_event(main_vk, KEYEVENTF_KEYUP));
        for vk in modifiers.iter().rev() {
            events.push(key_event(*vk, KEYEVENTF_KEYUP));
        }
        send(&events)
    }

    fn type_text(&self, text: &str) -> Result<(), SynthesizerError> {
        frozen_guard()?;
        let mut events = Vec::with_capacity(text.len() * 2);
        for unit in text.encode_utf16() {
            events.push(unicode_event(unit, false));
            events.push(unicode_event(unit, true));
        }
        send(&events)
    }

    fn click_point(&self, point: Coords) -> Result<(), SynthesizerError> {
        frozen_guard()?;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetCursorPos(point.x as i32, point.y as i32)
                .map_err(|e| SynthesizerError::Input(format!("SetCursorPos: {e}")))?;
        }
        send(&[
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0),
            mouse_event(MOUSEEVENTF_LEFTUP, 0),
        ])
    }

    fn scroll(&self, direction: ScrollDirection, amount: f64) -> Result<(), SynthesizerError> {
        frozen_guard()?;
        const WHEEL_DELTA: f64 = 120.0;
        let magnitude = (amount * WHEEL_DELTA).round() as i32;
        let signed = match direction {
            ScrollDirection::Up => magnitude,
            ScrollDirection::Down => -magnitude,
            ScrollDirection::Left | ScrollDirection::Right => {
                // Horizontal wheel needs MOUSEEVENTF_HWHEEL; no fixture
                // exercises it yet. FOLLOWUP.
                return Err(SynthesizerError::Unavailable(format!(
                    "horizontal scroll ({direction}) not implemented in the minimal real backend"
                )));
            }
        };
        send(&[mouse_event(MOUSEEVENTF_WHEEL, signed)])
    }

    fn clipboard_get(&self) -> Result<String, SynthesizerError> {
        frozen_guard()?;
        unsafe {
            OpenClipboard(None)
                .map_err(|e| SynthesizerError::Clipboard(format!("OpenClipboard: {e}")))?;
            let result = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT)
                    .map_err(|e| SynthesizerError::Clipboard(format!("GetClipboardData: {e}")))?;
                let ptr =
                    GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0 as _)) as *const u16;
                if ptr.is_null() {
                    return Err(SynthesizerError::Clipboard(
                        "GlobalLock returned null".into(),
                    ));
                }
                let mut len = 0usize;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);
                let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0 as _));
                Ok(text)
            })();
            let _ = CloseClipboard();
            result
        }
    }

    fn clipboard_set(&self, text: &str) -> Result<(), SynthesizerError> {
        frozen_guard()?;
        unsafe {
            OpenClipboard(None)
                .map_err(|e| SynthesizerError::Clipboard(format!("OpenClipboard: {e}")))?;
            let result = (|| {
                EmptyClipboard()
                    .map_err(|e| SynthesizerError::Clipboard(format!("EmptyClipboard: {e}")))?;
                let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let byte_len = units.len() * std::mem::size_of::<u16>();
                let handle = GlobalAlloc(GMEM_MOVEABLE, byte_len)
                    .map_err(|e| SynthesizerError::Clipboard(format!("GlobalAlloc: {e}")))?;
                let ptr = GlobalLock(handle) as *mut u16;
                if ptr.is_null() {
                    return Err(SynthesizerError::Clipboard(
                        "GlobalLock returned null".into(),
                    ));
                }
                std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
                let _ = GlobalUnlock(handle);
                // Ownership of `handle` transfers to the clipboard on
                // success; it must not be freed here.
                SetClipboardData(
                    CF_UNICODETEXT,
                    windows::Win32::Foundation::HANDLE(handle.0 as _),
                )
                .map_err(|e| SynthesizerError::Clipboard(format!("SetClipboardData: {e}")))?;
                Ok(())
            })();
            let _ = CloseClipboard();
            result
        }
    }

    fn release_all_modifiers(&self) -> Result<(), SynthesizerError> {
        // Intentionally NOT frozen-guarded: releasing stuck modifiers is the
        // safe response to a freeze (and the executor's own frozen path calls
        // this), so it must still run when the freeze is engaged.
        let events: Vec<INPUT> = MODIFIER_VKS
            .iter()
            .map(|vk| key_event(*vk, KEYEVENTF_KEYUP))
            .collect();
        send(&events)
    }
}

/// `EnumWindows` callback: push each top-level HWND into the `Vec<HWND>` handed
/// through `lparam`. Always returns TRUE so enumeration visits every window.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let handles = &mut *(lparam.0 as *mut Vec<HWND>);
    handles.push(hwnd);
    BOOL(1)
}

/// Enumerate every top-level window and reduce each to the [`WindowCandidate`]
/// the pure matcher ([`crate::focus`]) needs: title, owning-process basename,
/// and visibility. This is the OS half of the E1 fix; the matching itself is
/// OS-free and unit tested.
fn enumerate_top_level_windows() -> Vec<WindowCandidate> {
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: EnumWindows calls `collect_window` synchronously on this thread
    // once per window with the pointer we pass; `handles` outlives the call.
    unsafe {
        let _ = EnumWindows(Some(collect_window), LPARAM(&mut handles as *mut _ as isize));
    }
    handles
        .into_iter()
        .map(|hwnd| WindowCandidate {
            hwnd: hwnd.0 as isize,
            title: window_title(hwnd),
            process: window_process_basename(hwnd),
            visible: unsafe { IsWindowVisible(hwnd).as_bool() },
        })
        .collect()
}

/// The window's title text, or an empty string when it has none.
fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        // +1 for the NUL GetWindowTextW writes; it returns the copied length
        // excluding that terminator.
        let mut buf = vec![0u16; len as usize + 1];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..copied as usize])
    }
}

/// The image basename (e.g. `notepad.exe`) of the process owning `hwnd`, or
/// `None` when it cannot be resolved (a window owned by a more-privileged
/// process the current one cannot open). `None` never matches a `process`
/// constraint, which is the safe default: an unidentifiable window is not
/// treated as the target.
fn window_process_basename(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 260];
        let mut size = buf.len() as u32;
        let query =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
        let _ = CloseHandle(handle);
        query.ok()?;
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        let base = full
            .rsplit(|c| c == '\\' || c == '/')
            .next()
            .unwrap_or(full.as_str());
        Some(base.to_string())
    }
}

/// SetForegroundWindow refuses to steal focus from a foreground process
/// that did not yield it voluntarily unless the calling thread's input
/// state is attached to the foreground thread's. `docs/specs/action.md`
/// calls this out explicitly as "the attach-thread-input workaround."
fn focus_with_attach_workaround(hwnd: HWND) -> Result<(), SynthesizerError> {
    unsafe {
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let cur_thread = GetCurrentThreadId();
        let attached = fg_thread != 0
            && fg_thread != cur_thread
            && AttachThreadInput(cur_thread, fg_thread, true).as_bool();

        let focused = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);

        if attached {
            let _ = AttachThreadInput(cur_thread, fg_thread, false);
        }
        if !focused.as_bool() {
            return Err(SynthesizerError::Focus(
                "SetForegroundWindow returned false".into(),
            ));
        }
    }
    Ok(())
}

/// E2: re-read the focused element to confirm focus actually landed on the
/// target window's UI thread before any keystroke is sent. Confirms both that
/// the target is now the foreground window and that some control on its thread
/// holds keyboard focus; either failing means a keystroke would go nowhere
/// useful, so it is surfaced as a [`SynthesizerError::Focus`] (retryable by the
/// executor) rather than typed into the void.
fn verify_focus_landed(hwnd: HWND) -> Result<(), SynthesizerError> {
    unsafe {
        if GetForegroundWindow().0 != hwnd.0 {
            return Err(SynthesizerError::Focus(
                "target window did not become the foreground window after focus".into(),
            ));
        }
        let thread = GetWindowThreadProcessId(hwnd, None);
        if thread == 0 {
            return Err(SynthesizerError::Focus(
                "target window has no UI thread to verify focus against".into(),
            ));
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread, &mut info)
            .map_err(|e| SynthesizerError::Focus(format!("GetGUIThreadInfo: {e}")))?;
        if info.hwndFocus.0.is_null() {
            return Err(SynthesizerError::Focus(
                "no control holds keyboard focus on the target window after focus".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise pure parsing/mapping and the freeze gate only. Anything
    // that calls into user32/SendInput (focus enumeration, real input) needs an
    // interactive desktop session and is out of scope for `cargo test`; the
    // OS-free window-matching logic is tested in `crate::focus`, and the freeze
    // cases below short-circuit BEFORE any user32 call, so they are safe to run
    // headlessly.

    #[test]
    fn modifier_vk_recognizes_common_aliases() {
        assert_eq!(modifier_vk("ctrl"), Some(VK_CONTROL));
        assert_eq!(modifier_vk("Control"), Some(VK_CONTROL));
        assert_eq!(modifier_vk("alt"), Some(VK_MENU));
        assert_eq!(modifier_vk("shift"), Some(VK_SHIFT));
        assert_eq!(modifier_vk("win"), Some(VK_LWIN));
        assert_eq!(modifier_vk("s"), None);
    }

    // SAFETY freeze test. The frozen synthesizer must refuse every real input
    // call BEFORE it reaches SendInput/SetCursorPos/clipboard/EnumWindows, so
    // setting the freeze and calling each method injects nothing into the live
    // desktop: the whole test runs headlessly. Kept as one function so the
    // process-global flag it flips can never race another test in this binary,
    // and it restores the flag to `false` before returning.
    #[test]
    fn freeze_refuses_every_real_input_call_then_release_restores() {
        let synth = WindowsSynthesizer::new();
        let point = Coords {
            x: 10.0,
            y: 20.0,
            monitor: None,
            dpi_scale: None,
        };
        let window = WindowMatch {
            process: Some("notepad.exe".into()),
            title_pattern: Some(r".* - Notepad".into()),
        };

        safety::set_frozen(true);
        // Every guarded call short-circuits to Frozen without touching the OS.
        assert_eq!(synth.focus_window(&window), Err(SynthesizerError::Frozen));
        assert_eq!(synth.key("ctrl+s"), Err(SynthesizerError::Frozen));
        assert_eq!(synth.type_text("hi"), Err(SynthesizerError::Frozen));
        assert_eq!(synth.click_point(point), Err(SynthesizerError::Frozen));
        assert_eq!(
            synth.scroll(ScrollDirection::Down, 1.0),
            Err(SynthesizerError::Frozen)
        );
        assert_eq!(synth.clipboard_get(), Err(SynthesizerError::Frozen));
        assert_eq!(synth.clipboard_set("x"), Err(SynthesizerError::Frozen));

        // Releasing the freeze re-opens the gate: the guard no longer refuses.
        // (We assert via the gate itself rather than calling a real method, so
        // the test never actually injects input.)
        safety::set_frozen(false);
        assert!(!safety::is_frozen());
        assert_eq!(frozen_guard(), Ok(()));
    }
}
