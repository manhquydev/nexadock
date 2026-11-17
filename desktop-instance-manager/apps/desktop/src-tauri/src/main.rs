#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;

fn main() {
    let app_state = AppState::initialize().expect("failed to initialize Multi-Zalo Desktop state");

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::create_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::launch_profile,
            commands::focus_profile,
            commands::stop_profile,
            commands::backup_profile,
            commands::list_backups,
            commands::restore_backup,
            commands::get_diagnostics,
            commands::verify_isolation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Multi-Zalo Desktop Tauri application");
}
