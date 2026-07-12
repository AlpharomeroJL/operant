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

mod updater;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(updater::PendingUpdateState::default())
        .setup(|app| {
            updater::spawn_update_checker(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the Operant shell")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
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
