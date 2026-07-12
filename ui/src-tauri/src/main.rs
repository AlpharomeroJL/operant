// Operant C13 shell UI: Tauri v2 host process.
//
// This binary owns window chrome and IPC plumbing only; product logic
// (orchestrator, safety gates, recorder) lives in the core crates and will
// talk to this shell over the typed event bus (contracts/bus_events.md)
// once C1 is wired in here. Until then the frontend runs against a mocked
// bus client (ui/src/bus/mockClient.ts) so the shell is reviewable end to
// end on its own, with no backend process required.
//
// F2 (updater-wiring): the updater plugin is registered below using the
// Ed25519 pubkey and endpoints already committed in tauri.conf.json
// (release/KEYS.md). `updater` (this crate's own module, src/updater.rs)
// drives it: check on start and every 24h, staged verified download, a
// restart-to-update notification, and the swap on the next exit/restart. See
// updater.rs's module doc for exactly what is and is not covered by
// automated tests, and docs/KNOWN_ISSUES.md for the current honest status.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod updater;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(updater::PendingUpdateState::default())
        .invoke_handler(tauri::generate_handler![
            bridge::core_call,
            bridge::core_capabilities,
            bridge::core_ready,
            bridge::core_status,
            bridge::core_kill,
            bridge::core_restart,
        ])
        .setup(|app| {
            updater::spawn_update_checker(app.handle().clone());
            // Spawn and supervise the core sidecar, and wire its event stream
            // onto the webview (contracts/ipc.md, docs/adr/0002). The shell
            // still builds and runs if the core binary is absent: the bridge
            // reports a disconnected status rather than failing startup.
            if let Err(err) = bridge::init(app.handle()) {
                eprintln!("operant-shell: core bridge failed to start: {err}");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the Operant shell")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Best-effort graceful stop of the core child before we exit,
                // so a running core is never orphaned (contracts/ipc.md
                // section 8c: closing the child's stdin is its "shell gone,
                // exit" signal).
                bridge::shutdown(app_handle);
                // If an update finished downloading and verifying in the
                // background, swap it in now instead of just exiting: this is
                // the "restart to update" the staged-download notification
                // promises. On Windows, a successful install exits the
                // process itself; on any failure (or nothing staged) normal
                // exit proceeds right after.
                updater::install_pending_on_exit(app_handle);
            }
        });
}
