// The shell side of the shell-to-core bridge (contracts/ipc.md,
// docs/adr/0002-core-sidecar-ipc.md).
//
// This module is the Tauri boundary. It owns nothing of the protocol or the
// supervision state machine (those live in the submodules and are Tauri-free so
// they test in isolation); it wires them to the app:
//
//   * `init` spawns and supervises the core child, and installs the sinks that
//     forward the core's event stream to the webview and show a restart toast.
//   * The `#[tauri::command]`s expose the surface B3 drives: `core_call` proxies
//     one req/res, `core_capabilities` / `core_ready` / `core_status` expose the
//     handshake result and the blocking-screen gate, `core_kill` is kill-switch
//     path 2, and `core_restart` recovers a killed core.
//
// Events reach the webview on three channels:
//   * `operant://bus`         one bus envelope per core `evt`, unchanged, so
//                             ui/src/bus consumes it with no translation.
//   * `operant://thumb`       the optional flight-recorder screenshot that rides
//                             a run-step evt (contracts/ipc.md section 7), kept
//                             off the bus channel so that stream stays byte
//                             identical to contracts/bus_events.md.
//   * `operant://core-status` shell supervision status (connected, core_ready,
//                             capabilities, restarts). The core's own bus is
//                             silent while the core is dead, so supervision
//                             status is a shell-owned channel, not a bus topic.

mod protocol;
mod supervisor;
mod transport;

pub use protocol::{Capabilities, CoreError};
pub use supervisor::{CoreStatus, KillReport};

use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use supervisor::{RealCoreSpawner, Supervisor, SupervisorConfig};

/// Webview event channel: one bus envelope per core `evt`, forwarded unchanged.
pub const EVENT_BUS: &str = "operant://bus";
/// Webview event channel: the optional flight-recorder thumbnail on a run-step
/// evt (contracts/ipc.md section 7).
pub const EVENT_THUMB: &str = "operant://thumb";
/// Webview event channel: shell supervision status.
pub const EVENT_STATUS: &str = "operant://core-status";

/// Environment override for the core binary path (dev convenience: point it at
/// the freshly built `operant.exe`). Falls back to the bundled sidecar beside
/// the shell executable.
pub const CORE_BIN_ENV: &str = "OPERANT_CORE_BIN";

/// Tauri-managed handle onto the supervised core.
pub struct CoreBridge {
    pub supervisor: std::sync::Arc<Supervisor>,
}

/// Spawn and supervise the core sidecar, and manage the bridge state. Called
/// once from `main.rs`'s `.setup`.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Sink 1: forward each core evt to the webview. The bus envelope goes on
    // operant://bus unchanged; a present thumbnail goes on operant://thumb.
    let evt_app = app.clone();
    let on_evt: supervisor::EvtSink = std::sync::Arc::new(move |evt: protocol::EvtFrame| {
        let _ = evt_app.emit(EVENT_BUS, &evt.env);
        if let Some(thumb) = &evt.thumb {
            let _ = evt_app.emit(EVENT_THUMB, thumb);
        }
    });

    // Sink 2: publish supervision status changes to the webview.
    let status_app = app.clone();
    let on_status: supervisor::StatusSink =
        std::sync::Arc::new(move |status: &CoreStatus| {
            let _ = status_app.emit(EVENT_STATUS, status);
        });

    // Sink 3: a user-visible toast when the core is restarted after a crash.
    let toast_app = app.clone();
    let on_toast: supervisor::ToastSink =
        std::sync::Arc::new(move |message: &str| notify(&toast_app, message));

    let bin = resolve_core_bin(app);
    let spawner = RealCoreSpawner::new(bin);
    let supervisor = std::sync::Arc::new(Supervisor::new(
        Box::new(spawner),
        SupervisorConfig::default(),
        on_evt,
        on_status,
        on_toast,
    ));
    supervisor.start();

    app.manage(CoreBridge { supervisor });
    Ok(())
}

/// Locate the core binary. Order: the `OPERANT_CORE_BIN` override, then the
/// sidecar bundled beside the shell executable (Tauri names an `externalBin`
/// entry with the target triple), then a bare `operant` on `PATH`. A missing
/// binary is not fatal: the supervisor surfaces a disconnected status and keeps
/// retrying, so the shell still runs and B3 can render the disconnected state.
fn resolve_core_bin<R: Runtime>(_app: &AppHandle<R>) -> PathBuf {
    if let Ok(explicit) = std::env::var(CORE_BIN_ENV) {
        if !explicit.trim().is_empty() {
            return PathBuf::from(explicit);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // The Tauri sidecar is named `operant-<target-triple>` beside the
            // shell exe; a plain `operant.exe` is also accepted for dev layouts.
            let triple = env!("OPERANT_TARGET_TRIPLE");
            for name in [
                format!("operant-{triple}.exe"),
                "operant.exe".to_string(),
            ] {
                let candidate = dir.join(&name);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    PathBuf::from("operant")
}

/// Show a native toast, guarding on the notification plugin being registered
/// (mirrors updater.rs's approach so the shell's own tests can build a minimal
/// app without it).
fn notify<R: Runtime>(app: &AppHandle<R>, message: &str) {
    if app
        .try_state::<tauri_plugin_notification::Notification<R>>()
        .is_none()
    {
        eprintln!("operant-shell: notification plugin not registered; toast dropped: {message}");
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app
        .notification()
        .builder()
        .title("Operant")
        .body(message)
        .show()
    {
        eprintln!("operant-shell: failed to show a toast: {e}");
    }
}

// ---------------------------------------------------------------------------
// The Tauri command surface B3 drives.
// ---------------------------------------------------------------------------

/// Proxy one `req`/`res` to the core (contracts/ipc.md section 5). `args`
/// defaults to `{}` when omitted. Runs on a blocking task so the main thread is
/// never blocked waiting on the core.
#[tauri::command]
pub async fn core_call(
    bridge: State<'_, CoreBridge>,
    cmd: String,
    args: Option<Value>,
) -> Result<Value, CoreError> {
    let supervisor = bridge.supervisor.clone();
    let args = args.unwrap_or_else(|| Value::Object(Default::default()));
    tauri::async_runtime::spawn_blocking(move || supervisor.call(&cmd, args))
        .await
        .map_err(|e| CoreError::internal(format!("core_call task failed: {e}")))?
}

/// The capability handshake result (contracts/ipc.md section 3), or `null`
/// before the handshake completes. B3's blocking screen enumerates each false
/// capability from this.
#[tauri::command]
pub fn core_capabilities(bridge: State<'_, CoreBridge>) -> Option<Capabilities> {
    bridge.supervisor.capabilities()
}

/// Whether the core is up, handshaken, and able to automate. B3 gates the
/// blocking screen on this being false.
#[tauri::command]
pub fn core_ready(bridge: State<'_, CoreBridge>) -> bool {
    bridge.supervisor.core_ready()
}

/// The full supervision status (connected, core_ready, desired, capabilities,
/// restarts, last_error). B3 also receives this on `operant://core-status`.
#[tauri::command]
pub fn core_status(bridge: State<'_, CoreBridge>) -> CoreStatus {
    bridge.supervisor.status()
}

/// Kill-switch path 2: hard-terminate the core child immediately and keep it
/// down. Returns how fast it went so the panic path can prove it stopped.
#[tauri::command]
pub fn core_kill(bridge: State<'_, CoreBridge>) -> Result<KillReport, CoreError> {
    bridge.supervisor.kill()
}

/// Bring the core back after an intentional kill (or force a fresh process).
#[tauri::command]
pub fn core_restart(bridge: State<'_, CoreBridge>) -> Result<(), CoreError> {
    bridge.supervisor.request_restart();
    Ok(())
}

/// Best-effort graceful shutdown of the core, for the app-exit handler.
pub fn shutdown<R: Runtime>(app: &AppHandle<R>) {
    if let Some(bridge) = app.try_state::<CoreBridge>() {
        bridge.supervisor.shutdown();
    }
}
