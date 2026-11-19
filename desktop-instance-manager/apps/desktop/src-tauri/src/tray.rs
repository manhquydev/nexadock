//! Native system tray integration for KHOANG.
//!
//! - Uses a fixed tray ID (`"khoang-tray"`) to prevent duplicate tray icons.
//! - Supports localized Vietnamese (`vi-VN`) and English (`en-US`) menu/tooltip labels.
//! - Left-click / double-click restores and focuses the `"main"` window.
//! - "Thoát KHOANG" / "Quit KHOANG" exits the KHOANG shell cleanly without terminating
//!   managed Zalo Desktop processes.

use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const TRAY_ID: &str = "khoang-tray";
const MENU_OPEN_ID: &str = "open";
const MENU_ACCOUNTS_ID: &str = "accounts";
const MENU_SETTINGS_ID: &str = "settings";
const MENU_QUIT_ID: &str = "quit";

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
                tooltip: "KHOANG — Một máy. Nhiều tài khoản. Không trộn dữ liệu.",
                open: "Mở KHOANG",
                accounts: "Tài khoản",
                settings: "Cài đặt",
                quit: "Thoát KHOANG",
            },
            Self::En => TrayLabels {
                tooltip: "KHOANG — Your accounts. Their own space.",
                open: "Open KHOANG",
                accounts: "Accounts",
                settings: "Settings",
                quit: "Quit KHOANG",
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
    let labels = locale.labels();
    let open_item = MenuItemBuilder::with_id(MENU_OPEN_ID, labels.open).build(app)?;
    let accounts_item = MenuItemBuilder::with_id(MENU_ACCOUNTS_ID, labels.accounts).build(app)?;
    let settings_item = MenuItemBuilder::with_id(MENU_SETTINGS_ID, labels.settings).build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItemBuilder::with_id(MENU_QUIT_ID, labels.quit).build(app)?;

    MenuBuilder::new(app)
        .items(&[
            &open_item,
            &accounts_item,
            &settings_item,
            &separator,
            &quit_item,
        ])
        .build()
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Initializes the single KHOANG system tray icon if not already created.
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
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN_ID => {
                show_main_window(app);
            }
            MENU_ACCOUNTS_ID => {
                show_main_window(app);
                let _ = app.emit("khoang://navigate", "profiles");
            }
            MENU_SETTINGS_ID => {
                show_main_window(app);
                let _ = app.emit("khoang://navigate", "settings");
            }
            MENU_QUIT_ID => {
                // Exit KHOANG shell cleanly without stopping managed Zalo processes.
                app.exit(0);
            }
            _ => {}
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
    let menu = build_tray_menu(app, locale)?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(labels.tooltip))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locale_and_provides_localized_labels() {
        assert_eq!(TrayLocale::parse("vi-VN"), TrayLocale::Vi);
        assert_eq!(TrayLocale::parse("en-US"), TrayLocale::En);
        assert_eq!(TrayLocale::Vi.labels().quit, "Thoát KHOANG");
        assert_eq!(TrayLocale::En.labels().quit, "Quit KHOANG");
    }
}
