//! Windows SendInput backend, behind the `real-input` cargo feature so the
//! default build never links the `windows` crate.
//!
//! Minimal by design (`docs/specs/action.md`): SendInput with
//! `KEYEVENTF_UNICODE` for text, explicit modifier press/release pairing
//! for combos, and an unconditional release-all sweep every backend must
//! expose so [`crate::synth::ModifierReleaseGuard`] and a future kill
//! switch can rely on it. Focus-then-verify (re-reading the focused UIA
//! element before any keystroke) needs a `Perceiver` and stays a
//! FOLLOWUP; this backend does the SetForegroundWindow half only.

use operant_ir::{Coords, WindowMatch};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, SetFocus, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
};

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
        // Minimal: exact-title lookup only. Matching `process` /
        // `title_pattern` (a regex) against the live window list needs a
        // window enumeration + UIA-backed process lookup (perception, C2)
        // and is a FOLLOWUP; see the L3A result.
        let title = window.title_pattern.clone().ok_or_else(|| {
            SynthesizerError::Focus("no title_pattern to resolve a window handle from".into())
        })?;
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) }
            .map_err(|e| SynthesizerError::Focus(format!("FindWindowW({title}): {e}")))?;
        focus_with_attach_workaround(hwnd)
    }

    fn key(&self, combo: &str) -> Result<(), SynthesizerError> {
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
        let mut events = Vec::with_capacity(text.len() * 2);
        for unit in text.encode_utf16() {
            events.push(unicode_event(unit, false));
            events.push(unicode_event(unit, true));
        }
        send(&events)
    }

    fn click_point(&self, point: Coords) -> Result<(), SynthesizerError> {
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
        let events: Vec<INPUT> = MODIFIER_VKS
            .iter()
            .map(|vk| key_event(*vk, KEYEVENTF_KEYUP))
            .collect();
        send(&events)
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

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise pure parsing/mapping logic only; anything that calls
    // into user32/SendInput needs an interactive desktop session and is
    // out of scope for `cargo test` (that is exactly why the crate default
    // build excludes this module).

    #[test]
    fn modifier_vk_recognizes_common_aliases() {
        assert_eq!(modifier_vk("ctrl"), Some(VK_CONTROL));
        assert_eq!(modifier_vk("Control"), Some(VK_CONTROL));
        assert_eq!(modifier_vk("alt"), Some(VK_MENU));
        assert_eq!(modifier_vk("shift"), Some(VK_SHIFT));
        assert_eq!(modifier_vk("win"), Some(VK_LWIN));
        assert_eq!(modifier_vk("s"), None);
    }
}
