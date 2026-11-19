#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod preferences;
mod state;
mod tray;

use preferences::DesktopPrefsState;
use state::AppState;
use tauri::{Manager, WindowEvent};

fn main() {
    let app_state = AppState::initialize().expect("failed to initialize KHOANG state");
    let prefs_state = DesktopPrefsState::initialize(app_state.store.data_dir());

    tauri::Builder::default()
        .manage(app_state)
        .manage(prefs_state)
        .setup(|app| {
            let prefs = app.state::<DesktopPrefsState>().snapshot();
            tray::init_tray(app.handle(), tray::TrayLocale::parse(&prefs.locale))?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main"
                    && window
                        .app_handle()
                        .state::<DesktopPrefsState>()
                        .keep_in_tray()
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::create_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::launch_profile,
            commands::focus_profile,
            commands::stop_profile,
            commands::stop_all_profiles,
            commands::backup_profile,
            commands::list_backups,
            commands::restore_backup,
            commands::get_diagnostics,
            commands::verify_isolation,
            commands::get_desktop_preferences,
            commands::set_keep_in_tray,
            commands::set_tray_locale,
            commands::get_autostart_enabled,
            commands::set_autostart_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running KHOANG Tauri application");
}
