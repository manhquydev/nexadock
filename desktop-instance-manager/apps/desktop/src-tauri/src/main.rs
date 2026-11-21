#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod preferences;
mod state;
mod tray;
mod vault;
mod web_workspace;

use preferences::DesktopPrefsState;
use state::AppState;
use tauri::{Manager, WindowEvent};

fn run_startup_migrations() {
    if std::env::var_os(dim_core::DIM_DATA_DIR_ENV)
        .filter(|v| !v.to_string_lossy().trim().is_empty())
        .is_none()
    {
        if let Ok(target_dir) = dim_core::resolve_data_dir(None) {
            if let Some(parent) = target_dir.parent() {
                let legacy_dir = parent.join(dim_core::LEGACY_APP_DIR_NAME);
                if let Err(err) =
                    dim_profile_store::migrate_legacy_data_dir(&legacy_dir, &target_dir)
                {
                    eprintln!("NexaDock legacy data migration skipped safely: {err}");
                }
            }
        }
    }

    // `WebContext` stores `EBWebView` under `<data_dir>/WebSessions/_shared_env` (already
    // migrated with `<data_dir>` above), whereas the main Tauri window uses
    // `%LOCALAPPDATA%\<identifier>\EBWebView`.
    #[cfg(target_os = "windows")]
    if let Some(base) = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.to_string_lossy().trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .filter(|v| !v.to_string_lossy().trim().is_empty())
                .map(|home| std::path::PathBuf::from(home).join("AppData").join("Local"))
        })
    {
        let legacy_eb = base
            .join(dim_profile_store::LEGACY_BUNDLE_ID)
            .join("EBWebView");
        let target_eb = base
            .join(dim_profile_store::CANONICAL_BUNDLE_ID)
            .join("EBWebView");
        if let Err(err) = dim_profile_store::migrate_legacy_ebwebview_dir(&legacy_eb, &target_eb) {
            eprintln!("NexaDock legacy EBWebView migration skipped safely: {err}");
        }
    }
}

fn main() {
    run_startup_migrations();
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
            st.vault.resume_all();
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
            commands::vault_get_status,
            commands::vault_pair,
            commands::vault_login_phone,
            commands::vault_login_code,
            commands::vault_unpair,
            commands::vault_set_enabled,
            commands::vault_sync_now,
            commands::vault_get_log,
        ])
        .build(tauri::generate_context!())
        .expect("error while building NexaDock Tauri application")
        .run(|app_handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main_window(app_handle);
            }
            if let tauri::RunEvent::Exit = event {
                app_handle.state::<AppState>().vault.shutdown();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app_handle, event);
        });
}
