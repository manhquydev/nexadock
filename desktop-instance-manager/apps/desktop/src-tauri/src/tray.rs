//! Native system tray integration for NexaDock.
//!
//! - Uses a fixed tray ID (`"nexadock-tray"`) to prevent duplicate tray icons.
//! - Supports localized Vietnamese (`vi-VN`) and English (`en-US`) menu/tooltip labels.
//! - Left-click / double-click restores and focuses the `"main"` window.
//! - "Thoát NexaDock" / "Quit NexaDock" exits the NexaDock shell cleanly without terminating
//!   managed Zalo Desktop processes.

use dim_core::{AccountKind, ProfileStore};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::preferences::DesktopPrefsState;
use crate::state::AppState;

pub const TRAY_ID: &str = "nexadock-tray";
const MENU_OPEN_ID: &str = "open";
const MENU_ACCOUNTS_ID: &str = "accounts";
const MENU_SETTINGS_ID: &str = "settings";
const MENU_QUIT_ID: &str = "quit";
const ACCOUNT_MENU_PREFIX: &str = "acct:";

#[derive(Debug, Clone, Serialize)]
pub struct TrayActivatePayload {
    pub id: String,
    pub kind: AccountKind,
}

#[derive(Debug, Clone)]
pub struct TrayAccountEntry {
    pub id: String,
    pub kind: AccountKind,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayLocale {
    Vi,
    En,
}

impl TrayLocale {
    pub fn parse(raw: &str) -> Self {
        if raw.trim().to_ascii_lowercase().starts_with("en") {
            Self::En
        } else {
            Self::Vi
        }
    }

    pub fn labels(self) -> TrayLabels {
        match self {
            Self::Vi => TrayLabels {
                tooltip: "NexaDock — Một máy. Nhiều tài khoản. Không trộn dữ liệu.",
                open: "Mở NexaDock",
                accounts: "Tài khoản",
                settings: "Cài đặt",
                quit: "Thoát NexaDock",
            },
            Self::En => TrayLabels {
                tooltip: "NexaDock — Your accounts. Their own space.",
                open: "Open NexaDock",
                accounts: "Accounts",
                settings: "Settings",
                quit: "Quit NexaDock",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayLabels {
    pub tooltip: &'static str,
    pub open: &'static str,
    pub accounts: &'static str,
    pub settings: &'static str,
    pub quit: &'static str,
}

fn load_platform_tray_icon() -> tauri::Result<Image<'static>> {
    #[cfg(target_os = "windows")]
    let bytes: &[u8] = include_bytes!("../icons/tray/tray-windows.ico");
    #[cfg(target_os = "macos")]
    let bytes: &[u8] = include_bytes!("../icons/tray/tray-macos-template.png");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let bytes: &[u8] = include_bytes!("../icons/tray/tray-linux.png");

    Image::from_bytes(bytes)
}

pub fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    locale: TrayLocale,
) -> tauri::Result<Menu<R>> {
    build_tray_menu_with_accounts(app, locale, &[])
}

pub fn build_tray_menu_with_accounts<R: Runtime>(
    app: &AppHandle<R>,
    locale: TrayLocale,
    accounts: &[TrayAccountEntry],
) -> tauri::Result<Menu<R>> {
    let labels = locale.labels();
    let open_item = MenuItemBuilder::with_id(MENU_OPEN_ID, labels.open).build(app)?;
    let accounts_item = MenuItemBuilder::with_id(MENU_ACCOUNTS_ID, labels.accounts).build(app)?;
    let settings_item = MenuItemBuilder::with_id(MENU_SETTINGS_ID, labels.settings).build(app)?;
    let separator_top = PredefinedMenuItem::separator(app)?;
    let separator_bottom = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItemBuilder::with_id(MENU_QUIT_ID, labels.quit).build(app)?;

    let mut builder = MenuBuilder::new(app).item(&open_item);

    if !accounts.is_empty() {
        let acct_sep = PredefinedMenuItem::separator(app)?;
        builder = builder.item(&acct_sep);
        for entry in accounts {
            let kind_str = match entry.kind {
                AccountKind::DesktopPrimary => "desktop_primary",
                AccountKind::DesktopIsolated => "desktop_isolated",
                AccountKind::WebIsolated => "web_isolated",
            };
            let menu_id = format!("{ACCOUNT_MENU_PREFIX}{kind_str}:{}", entry.id);
            let item = MenuItemBuilder::with_id(menu_id, &entry.label).build(app)?;
            builder = builder.item(&item);
        }
    }

    builder
        .items(&[
            &separator_top,
            &accounts_item,
            &settings_item,
            &separator_bottom,
            &quit_item,
        ])
        .build()
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    crate::web_workspace::set_main_window_visible_on_current_thread(true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn parse_account_menu_id(menu_id: &str) -> Option<TrayActivatePayload> {
    let rest = menu_id.strip_prefix(ACCOUNT_MENU_PREFIX)?;
    let (kind_raw, id) = rest.split_once(':')?;
    let kind = match kind_raw {
        "desktop_primary" => AccountKind::DesktopPrimary,
        "desktop_isolated" => AccountKind::DesktopIsolated,
        "web_isolated" => AccountKind::WebIsolated,
        _ => return None,
    };
    Some(TrayActivatePayload {
        id: id.to_owned(),
        kind,
    })
}

fn collect_tray_accounts<R: Runtime>(
    app: &AppHandle<R>,
    locale: TrayLocale,
) -> Vec<TrayAccountEntry> {
    let mut entries = vec![TrayAccountEntry {
        id: "primary".to_owned(),
        kind: AccountKind::DesktopPrimary,
        label: match locale {
            TrayLocale::Vi => "Zalo gốc (Chính)".to_owned(),
            TrayLocale::En => "Primary Zalo".to_owned(),
        },
    }];

    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(profiles) = state.store.list_profiles() {
            for p in profiles {
                entries.push(TrayAccountEntry {
                    id: p.id,
                    kind: AccountKind::DesktopIsolated,
                    label: format!("{} (Desktop)", p.name),
                });
            }
        }
        for ws in state.web_sessions.list() {
            entries.push(TrayAccountEntry {
                id: ws.id,
                kind: AccountKind::WebIsolated,
                label: format!("{} (Web)", ws.display_name),
            });
        }
    }

    entries
}

/// Initializes the single NexaDock system tray icon if not already created.
pub fn init_tray<R: Runtime>(app: &AppHandle<R>, locale: TrayLocale) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }

    let labels = locale.labels();
    let menu = build_tray_menu(app, locale)?;
    let icon = load_platform_tray_icon()?;

    #[allow(unused_mut)]
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .tooltip(labels.tooltip)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id_str = event.id().as_ref();
            match id_str {
                MENU_OPEN_ID => {
                    show_main_window(app);
                }
                MENU_ACCOUNTS_ID => {
                    let _ = crate::web_workspace::hide_all_sessions(app);
                    show_main_window(app);
                    let _ = app.emit("nexadock://navigate", "profiles");
                }
                MENU_SETTINGS_ID => {
                    let _ = crate::web_workspace::hide_all_sessions(app);
                    show_main_window(app);
                    let _ = app.emit("nexadock://navigate", "settings");
                }
                MENU_QUIT_ID => {
                    // Cleanly drop all child WebViews before exiting NexaDock shell; never stop Zalo.
                    crate::web_workspace::clear_all_on_current_thread();
                    app.exit(0);
                }
                _ => {
                    if let Some(payload) = parse_account_menu_id(id_str) {
                        show_main_window(app);
                        let _ = app.emit("nexadock://activate-request", payload);
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }

    builder.build(app)?;
    Ok(())
}

/// Updates the existing tray menu and tooltip when the UI language changes.
pub fn update_tray_locale<R: Runtime>(app: &AppHandle<R>, locale_raw: &str) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let locale = TrayLocale::parse(locale_raw);
    let labels = locale.labels();
    let accounts = collect_tray_accounts(app, locale);
    let menu = build_tray_menu_with_accounts(app, locale, &accounts)?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(labels.tooltip))?;
    Ok(())
}

/// Refreshes the dynamic per-account tray menu items using the current locale.
pub fn refresh_tray_accounts<R: Runtime>(app: &AppHandle<R>) {
    let locale_raw = app
        .try_state::<DesktopPrefsState>()
        .map(|p| p.snapshot().locale)
        .unwrap_or_else(|| "vi-VN".to_owned());
    let _ = update_tray_locale(app, &locale_raw);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locale_and_provides_localized_labels() {
        assert_eq!(TrayLocale::parse("vi-VN"), TrayLocale::Vi);
        assert_eq!(TrayLocale::parse("en-US"), TrayLocale::En);
        assert_eq!(TrayLocale::Vi.labels().quit, "Thoát NexaDock");
        assert_eq!(TrayLocale::En.labels().quit, "Quit NexaDock");
    }
}
