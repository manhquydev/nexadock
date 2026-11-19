#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod preferences;
mod state;
mod tray;
mod web_workspace;

use preferences::DesktopPrefsState;
use state::AppState;
use tauri::{Manager, WindowEvent};

fn main() {
    let app_state = AppState::initialize().expect("failed to initialize NexaDock state");
    let prefs_state = DesktopPrefsState::initialize(app_state.store.data_dir());

    tauri::Builder::default()
        .manage(app_state)
        .manage(prefs_state)
        .setup(|app| {
            web_workspace::register_main_thread();
            let st = app.state::<AppState>();
            let main_win = app.get_webview_window("main");
            let _ = web_workspace::sweep_pending_profile_deletes(
                main_win.as_ref(),
                st.store.data_dir(),
                &st.web_sessions,
            );
            let prefs = app.state::<DesktopPrefsState>().snapshot();
            tray::init_tray(app.handle(), tray::TrayLocale::parse(&prefs.locale))?;
            tray::refresh_tray_accounts(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. } => {
                    web_workspace::notify_parent_moved_on_current_thread();
                }
                WindowEvent::CloseRequested { api, .. } => {
                    if window
                        .app_handle()
                        .state::<DesktopPrefsState>()
                        .keep_in_tray()
                    {
                        api.prevent_close();
                        web_workspace::set_main_window_visible_on_current_thread(false);
                        let _ = window.hide();
                    } else {
                        web_workspace::clear_all_on_current_thread();
                    }
                }
                _ => {}
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
            commands::get_workspace_snapshot,
            commands::activate_account,
            commands::set_account_pinned,
            commands::set_companion_snap,
            commands::web_session_create,
            commands::web_session_show,
            commands::web_session_set_bounds,
            commands::web_session_hide,
            commands::web_session_hide_all,
            commands::web_session_reload,
            commands::web_session_reset,
            commands::web_session_delete,
            commands::web_session_rename,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NexaDock Tauri application");
}
