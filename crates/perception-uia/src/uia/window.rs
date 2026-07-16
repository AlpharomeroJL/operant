//! Target-window resolution and the two access checks
//! `docs/specs/perception.md` calls out by name: elevated windows and the
//! secure desktop must both come back as a typed `PerceptionError::Denied`,
//! never a quietly empty tree. Both checks run BEFORE any UIA call, as an
//! explicit, self-contained gate rather than relying on however UIA itself
//! happens to fail against an inaccessible window (which is inconsistent
//! in practice and not something a headless environment without an
//! elevated window or a secure-desktop prompt on hand can observe
//! directly; see the crate result notes for this FOLLOWUP).

use operant_core::perceive::PerceptionError;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_CONTROL_FLAGS,
    DESKTOP_READOBJECTS, UOI_NAME,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

/// Find the first (topmost in z-order) visible, titled top-level window
/// belonging to `process_name`, matched case-insensitively against just
/// the executable file name (e.g. `notepad.exe`).
pub fn find_window_by_process(process_name: &str) -> Result<HWND, PerceptionError> {
    let mut candidates: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(std::ptr::addr_of_mut!(candidates) as isize),
        );
    }
    for hwnd in candidates {
        if let Some(name) = process_image_name(hwnd) {
            if name.eq_ignore_ascii_case(process_name) {
                return Ok(hwnd);
            }
        }
    }
    Err(PerceptionError::WindowNotFound(process_name.to_string()))
}

/// One visible top-level window, for the target picker (ADR 0003,
/// `list_windows`). `hwnd` is the raw handle as a signed integer; the caller
/// renders it as a hex id.
#[derive(Debug, Clone)]
pub struct EnumeratedWindow {
    pub process: String,
    pub title: String,
    pub hwnd: isize,
}

/// Enumerate visible, titled top-level windows in z-order (topmost first),
/// each with its owning process basename and title. Backs the `list_windows`
/// command that populates the palette target picker so a teach binds to the
/// app the user means rather than to Operant (the foreground window while the
/// palette is open). A window whose owning process cannot be resolved is
/// skipped, the same rule [`find_window_by_process`] applies.
pub fn enumerate_windows() -> Vec<EnumeratedWindow> {
    let mut candidates: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(std::ptr::addr_of_mut!(candidates) as isize),
        );
    }
    candidates
        .into_iter()
        .filter_map(|hwnd| {
            let process = process_image_name(hwnd)?;
            Some(EnumeratedWindow {
                process,
                title: window_title_text(hwnd),
                hwnd: hwnd.0 as isize,
            })
        })
        .collect()
}

fn window_title_text(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..copied as usize])
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    let titled = unsafe { GetWindowTextLengthW(hwnd) } > 0;
    if visible && titled {
        let candidates = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        candidates.push(hwnd);
    }
    BOOL(1) // continue enumeration
}

fn process_image_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    Some(
        std::path::Path::new(&path)
            .file_name()?
            .to_string_lossy()
            .into_owned(),
    )
}

/// `docs/specs/perception.md`: "elevated (admin) windows are invisible
/// without elevation, return a typed PerceptionDenied error, never an
/// empty tree; secure desktop (UAC) likewise."
pub fn deny_if_inaccessible(hwnd: HWND) -> Result<(), PerceptionError> {
    if is_secure_desktop() {
        return Err(PerceptionError::Denied(
            "active desktop is not the interactive user desktop (secure desktop / UAC prompt)"
                .into(),
        ));
    }
    if is_target_elevated_relative_to_self(hwnd)? {
        return Err(PerceptionError::Denied(
            "target window belongs to an elevated process; relaunch elevated to perceive it".into(),
        ));
    }
    Ok(())
}

fn is_secure_desktop() -> bool {
    let Ok(desk) =
        (unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) })
    else {
        // Cannot even open the input desktop from here: treat as
        // inaccessible rather than silently walking nothing.
        return true;
    };
    let mut name_buf = [0u16; 64];
    let mut needed = 0u32;
    let got = unsafe {
        GetUserObjectInformationW(
            HANDLE(desk.0),
            UOI_NAME,
            Some(name_buf.as_mut_ptr() as *mut core::ffi::c_void),
            (name_buf.len() * 2) as u32,
            Some(&mut needed),
        )
    };
    let _ = unsafe { CloseDesktop(desk) };
    if got.is_err() {
        return true;
    }
    let len = name_buf.iter().position(|&c| c == 0).unwrap_or(0);
    let name = String::from_utf16_lossy(&name_buf[..len]);
    !name.eq_ignore_ascii_case("default")
}

fn is_target_elevated_relative_to_self(hwnd: HWND) -> Result<bool, PerceptionError> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return Ok(false);
    }
    let target = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(|e| PerceptionError::Backend(format!("OpenProcess({pid}): {e}")))?;
    let target_elevated = process_is_elevated(target);
    let _ = unsafe { CloseHandle(target) };
    let self_elevated = process_is_elevated(unsafe { GetCurrentProcess() });
    Ok(target_elevated? && !self_elevated?)
}

fn process_is_elevated(process: HANDLE) -> Result<bool, PerceptionError> {
    let mut token = HANDLE(std::ptr::null_mut());
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }
        .map_err(|e| PerceptionError::Backend(format!("OpenProcessToken: {e}")))?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut ret_len = 0u32;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    let _ = unsafe { CloseHandle(token) };
    result.map_err(|e| PerceptionError::Backend(format!("GetTokenInformation: {e}")))?;
    Ok(elevation.TokenIsElevated != 0)
}
