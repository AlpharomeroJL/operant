// Automatic update checking, staged download, Ed25519 signature verification,
// and restart-triggered install (docs/KNOWN_ISSUES.md: "Automatic updates are
// not active yet. The updater is configured but not wired into this build.").
// This module is that wiring; `tauri_plugin_updater` itself is registered in
// main.rs, using the pubkey and endpoints already committed in
// tauri.conf.json (see release/KEYS.md for how that key landed there).
//
// Flow: `spawn_update_checker` runs once on start and then every
// `CHECK_INTERVAL`, gated by `should_check_for_updates` (air-gap override,
// then the persisted on/off setting, default on). A successful check that
// finds a newer version downloads it and lets `tauri_plugin_updater` verify
// its Ed25519 signature against the embedded pubkey before the bytes are
// trusted at all (`Update::download`, not this module, does that check).
// Verified bytes are held in `PendingUpdateState` (in memory only) and a
// "restart to update" OS notification is shown. The actual swap happens in
// `install_pending_on_exit`, called from main.rs's `RunEvent::ExitRequested`
// handler: closing or restarting Operant after an update has been staged
// installs it instead of just quitting.
//
// Testing note, stated plainly: `tests/updater_signature.rs` exercises the real
// `tauri_plugin_updater` crate (nothing reimplemented or mocked at the
// signature-verification layer) against a local fixture HTTP server for the
// full check -> download -> verify round trip, and separately proves a
// tampered manifest is rejected. It also calls the real `Update::install`
// against the fixture bytes; since those bytes are deliberately not a real
// Windows executable, `install` fails fast at its own "is this a real
// installer" sniff (`Error::InvalidUpdaterFormat`) before it would ever shell
// out to a real installer, so the test proves the wiring reaches the swap
// step without launching one. What is not, and cannot safely be, covered by
// an automated test here: a real NSIS installer actually replacing files on
// disk and the process relaunching, and a real Windows toast notification
// rendering on screen. Both require a live update server or an interactive
// desktop session; see this lane's return notes for exactly what was and
// was not verified.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Set to any value other than "0"/"false"/"" (case-insensitive) to force
/// zero updater network activity regardless of the settings toggle. This is
/// the hard air-gap override docs/KNOWN_ISSUES.md and release/KEYS.md call
/// for: unset by default, so a fresh install is not air-gapped unless this is
/// explicitly requested (by an operator, a packaging profile, or a test).
pub const AIRGAP_ENV_VAR: &str = "OPERANT_AIRGAPPED";

/// How often to re-check after the on-start check.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

const SETTINGS_FILE: &str = "updater-settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct UpdaterSettings {
    #[serde(default = "default_true")]
    auto_update_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for UpdaterSettings {
    fn default() -> Self {
        Self {
            auto_update_enabled: true,
        }
    }
}

/// Pure predicate so the on/off logic is unit-testable without a filesystem
/// or a running app: air-gap wins first, then the persisted toggle.
fn should_check_given(airgapped: bool, setting_enabled: bool) -> bool {
    !airgapped && setting_enabled
}

fn is_airgapped_value(raw: Option<&str>) -> bool {
    match raw {
        None => false,
        Some(v) => !(v.is_empty() || v.eq_ignore_ascii_case("0") || v.eq_ignore_ascii_case("false")),
    }
}

/// Whether `OPERANT_AIRGAPPED` is currently set to a truthy value.
pub fn is_airgapped() -> bool {
    is_airgapped_value(env::var(AIRGAP_ENV_VAR).ok().as_deref())
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(SETTINGS_FILE))
}

fn read_settings<R: Runtime>(app: &AppHandle<R>) -> UpdaterSettings {
    let Some(path) = settings_path(app) else {
        return UpdaterSettings::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => UpdaterSettings::default(),
    }
}

fn write_settings<R: Runtime>(app: &AppHandle<R>, settings: UpdaterSettings) -> std::io::Result<()> {
    let Some(path) = settings_path(app) else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(&settings)?)
}

/// Persists the auto-update on/off setting this app process owns. Not yet
/// reachable from the frontend: ui/src/settings has its own toggle of the
/// same name and same default, but this shell has no settings IPC bridge yet
/// (see docs/KNOWN_ISSUES.md and ui/src/settings/mockStore.ts's own note
/// about `watchAndSuggestEnabled` having the identical gap). This function
/// exists so that gap has a real, tested seam to close later, rather than
/// nothing at all.
#[allow(dead_code)]
pub fn set_auto_update_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> std::io::Result<()> {
    write_settings(app, UpdaterSettings { auto_update_enabled: enabled })
}

/// Whether the background checker should attempt a network request right
/// now: false whenever air-gapped, otherwise the persisted toggle (default
/// on).
pub fn should_check_for_updates<R: Runtime>(app: &AppHandle<R>) -> bool {
    should_check_given(is_airgapped(), read_settings(app).auto_update_enabled)
}

/// A verified, downloaded update staged in memory, waiting for the app to
/// exit or restart so it can be swapped in (see `install_pending_on_exit`).
pub struct PendingUpdate {
    pub update: Update,
    pub bytes: Vec<u8>,
}

/// Tauri-managed slot for at most one staged update. Registered in main.rs
/// via `app.manage(PendingUpdateState::default())`.
#[derive(Default)]
pub struct PendingUpdateState(pub Mutex<Option<PendingUpdate>>);

/// Starts the background loop: check immediately, then every
/// [`CHECK_INTERVAL`]. A pure no-op when `OPERANT_AIRGAPPED` is set: nothing
/// is spawned at all, so an air-gapped run makes zero updater network calls
/// by construction, not just by choosing not to call `check()`.
pub fn spawn_update_checker<R: Runtime>(app: AppHandle<R>) {
    if is_airgapped() {
        eprintln!(
            "operant: {AIRGAP_ENV_VAR} is set; automatic update checks are disabled for this run."
        );
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if should_check_for_updates(&app) {
                check_once(&app).await;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

async fn check_once<R: Runtime>(app: &AppHandle<R>) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(err) => {
            eprintln!("operant: updater unavailable: {err}");
            return;
        }
    };

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(err) => {
            eprintln!("operant: update check failed: {err}");
            return;
        }
    };

    let version = update.version.clone();
    // `Update::download` is where `tauri_plugin_updater` verifies the Ed25519
    // signature against the embedded pubkey (release/KEYS.md); bytes that
    // fail verification never reach the `Ok` branch below.
    let bytes = match update.download(|_chunk_len, _total_len| {}, || {}).await {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("operant: update download or signature verification failed: {err}");
            return;
        }
    };

    if let Some(state) = app.try_state::<PendingUpdateState>() {
        *state.0.lock().unwrap() = Some(PendingUpdate { update, bytes });
    } else {
        eprintln!("operant: PendingUpdateState not managed; staged update {version} will not survive to restart");
    }

    notify_update_ready(app, &version);
}

fn notify_update_ready<R: Runtime>(app: &AppHandle<R>, version: &str) {
    // Guard with try_state instead of calling the extension trait directly:
    // NotificationExt::notification() panics if the plugin was never
    // registered, and this module's own tests build a minimal app that
    // registers only the updater plugin.
    if app
        .try_state::<tauri_plugin_notification::Notification<R>>()
        .is_none()
    {
        eprintln!(
            "operant: notification plugin not registered; update {version} is staged but no toast will be shown"
        );
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let result = app
        .notification()
        .builder()
        .title("Operant update ready")
        .body(format!(
            "Operant {version} was downloaded and verified. Restart Operant to finish installing it."
        ))
        .show();
    if let Err(err) = result {
        eprintln!("operant: failed to show the update-ready notification: {err}");
    }
}

/// Called from main.rs's `RunEvent::ExitRequested` handler. If a verified
/// update is staged, installs it now (the actual swap) instead of letting the
/// app exit normally. Returns true if an install was attempted; on Windows,
/// `Update::install` exits the process itself on success, so control may
/// never return to the caller in that case.
pub fn install_pending_on_exit<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(state) = app.try_state::<PendingUpdateState>() else {
        return false;
    };
    let pending = state.0.lock().unwrap().take();
    match pending {
        Some(PendingUpdate { update, bytes }) => {
            if let Err(err) = update.install(bytes) {
                eprintln!("operant: staged update failed to install: {err}");
            }
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airgap_defaults_off_when_unset() {
        assert!(!is_airgapped_value(None));
    }

    #[test]
    fn airgap_engages_on_truthy_values() {
        for v in ["1", "true", "TRUE", "yes", "on"] {
            assert!(is_airgapped_value(Some(v)), "expected {v:?} to engage air-gap mode");
        }
    }

    #[test]
    fn airgap_stays_off_on_falsy_values() {
        for v in ["0", "false", "False", ""] {
            assert!(!is_airgapped_value(Some(v)), "expected {v:?} to NOT engage air-gap mode");
        }
    }

    #[test]
    fn checks_fire_only_when_enabled_and_not_airgapped() {
        assert!(should_check_given(false, true), "online and enabled: should check");
        assert!(!should_check_given(true, true), "air-gapped overrides an enabled toggle");
        assert!(!should_check_given(false, false), "disabled toggle: should not check");
        assert!(!should_check_given(true, false), "air-gapped and disabled: still should not check");
    }

    #[test]
    fn settings_default_to_auto_update_enabled() {
        assert!(UpdaterSettings::default().auto_update_enabled);
    }

    #[test]
    fn missing_or_corrupt_settings_file_defaults_to_enabled() {
        // read_settings falls back to UpdaterSettings::default() on any parse
        // error, mirroring the "default on, fail open to the same default"
        // behavior used everywhere else settings are read in this codebase.
        let parsed: UpdaterSettings = serde_json::from_str("{}").unwrap();
        assert!(parsed.auto_update_enabled);
        assert!(serde_json::from_str::<UpdaterSettings>("not json").is_err());
    }
}
