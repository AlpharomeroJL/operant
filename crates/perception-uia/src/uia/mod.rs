//! [`UiaPerceiver`]: the real Windows UIA backend behind the `real-uia`
//! cargo feature (`docs/specs/perception.md`). Cached one-round-trip
//! subtree walk (`walk`), ControlType -> Role map (`roles`), target window
//! resolution plus the elevated/secure-desktop access checks (`window`).
//! Selector-chain resolve/diff/digest are NOT duplicated here: they are
//! pure data operations over an already-captured `Snapshot`
//! (`crate::resolve`, `crate::diff`, `crate::digest`), so this backend and
//! [`crate::FixturePerceiver`] share that logic exactly, and only differ
//! in how the `Snapshot` gets built in the first place.
//!
//! `wait_until_changed` implements only the spec's documented fallback
//! path (poll-diff at 100ms). Subscribing to native UIA structure/property
//! change events needs a real `IUIAutomationEventHandler` COM sink
//! (`windows_core::implement!`) plus `AddAutomationEventHandler`, and a
//! live desktop to verify event delivery against -- neither of which this
//! headless lane can exercise -- so it is left as a documented FOLLOWUP
//! rather than guessed at.

mod roles;
mod walk;
mod window;

use std::thread;
use std::time::{Duration, Instant};

use operant_core::perceive::{Perceiver, PerceptionError, Resolved};
use operant_ir::snapshot::{Snapshot, SnapshotSource, WindowInfo};
use operant_ir::Selector;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;

use crate::digest::compute_digest;
use crate::resolve::resolve_in_snapshot;
use crate::selectors::attach_selectors;
use window::{deny_if_inaccessible, find_window_by_process};

const POLL_INTERVAL_MS: u64 = 100;

/// Real UIA-backed [`Perceiver`]. Stateless beyond per-thread COM
/// initialization: every call resolves the target window fresh instead of
/// caching a handle, so a closed/reopened window is never a stale
/// reference.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiaPerceiver;

impl UiaPerceiver {
    pub fn new() -> Self {
        Self
    }
}

impl Perceiver for UiaPerceiver {
    fn snapshot(&self, window_process: &str) -> Result<Snapshot, PerceptionError> {
        capture(window_process)
    }

    fn resolve(
        &self,
        snapshot: &Snapshot,
        selectors: &[Selector],
    ) -> Result<Resolved, PerceptionError> {
        resolve_in_snapshot(snapshot, selectors)
    }

    fn wait_until_changed(
        &self,
        window_process: &str,
        prev_digest: &str,
        timeout_ms: u64,
    ) -> Result<Snapshot, PerceptionError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let snap = capture(window_process)?;
            if snap.digest != prev_digest {
                return Ok(snap);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(PerceptionError::Timeout(timeout_ms));
            }
            let remaining = deadline.saturating_duration_since(now);
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS).min(remaining));
        }
    }
}

fn ensure_com_initialized() {
    thread_local! {
        static COM_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    COM_READY.with(|ready| {
        if !ready.get() {
            // S_FALSE / RPC_E_CHANGED_MODE both just mean COM is already
            // initialized in some concurrency model on this thread, which
            // is fine for CoCreateInstance; never paired with
            // CoUninitialize since this runs on worker threads whose
            // lifetime this crate does not own.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            ready.set(true);
        }
    });
}

fn capture(window_process: &str) -> Result<Snapshot, PerceptionError> {
    ensure_com_initialized();
    let hwnd = find_window_by_process(window_process)?;
    deny_if_inaccessible(hwnd)?;

    let started = Instant::now();
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    }
    .map_err(|e| PerceptionError::Backend(format!("CoCreateInstance(CUIAutomation): {e}")))?;

    let monitor_id = monitor_id_for(hwnd);
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi_scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };

    let cache_request = walk::build_cache_request(&automation)
        .map_err(|e| PerceptionError::Backend(format!("CreateCacheRequest: {e}")))?;
    let root = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|e| classify_com_error("ElementFromHandle", &e))?;
    let cached_root = unsafe { root.BuildUpdatedCache(&cache_request) }
        .map_err(|e| classify_com_error("BuildUpdatedCache", &e))?;

    let outcome = walk::walk_subtree(&cached_root, &cache_request, &monitor_id);
    if outcome.elements.is_empty() {
        return Err(PerceptionError::Backend(
            "UIA subtree walk produced no elements".to_string(),
        ));
    }

    let mut elements = outcome.elements;
    attach_selectors(&mut elements);
    let digest = compute_digest(&elements);

    Ok(Snapshot {
        v: 1,
        source: SnapshotSource::Uia,
        window: WindowInfo {
            hwnd: Some(format!("{:#010x}", hwnd.0 as isize)),
            process: window_process.to_string(),
            title: window_title(hwnd),
            monitor: Some(monitor_id),
            dpi_scale,
        },
        digest,
        truncated: outcome.truncated,
        captured_ms: Some(started.elapsed().as_millis() as u64),
        elements,
    })
}

fn monitor_id_for(hwnd: HWND) -> String {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    format!("{:#010x}", monitor.0 as isize)
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn classify_com_error(op: &str, e: &windows::core::Error) -> PerceptionError {
    // The real gate is `window::deny_if_inaccessible`'s pre-check; this is
    // defense in depth for whatever access failure slips past it and
    // surfaces as a plain COM error instead.
    const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
    if e.code().0 == E_ACCESSDENIED {
        PerceptionError::Denied(format!("{op}: access denied ({e})"))
    } else {
        PerceptionError::Backend(format!("{op}: {e}"))
    }
}
