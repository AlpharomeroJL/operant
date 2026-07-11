// Operant C13 shell UI: Tauri v2 host process.
//
// This binary owns window chrome and IPC plumbing only; product logic
// (orchestrator, safety gates, recorder) lives in the core crates and will
// talk to this shell over the typed event bus (contracts/bus_events.md)
// once C1 is wired in here. Until then the frontend runs against a mocked
// bus client (ui/src/bus/mockClient.ts) so the shell is reviewable end to
// end on its own, with no backend process required.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running the Operant shell");
}
