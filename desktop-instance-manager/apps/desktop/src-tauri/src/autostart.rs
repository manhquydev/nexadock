//! Transparent, opt-in OS autostart management for KHOANG (off by default).
//!
//! - Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `"KHOANG"` via `winreg`.
//! - macOS: `~/Library/LaunchAgents/com.multizalodesktop.manager.plist`.
//! - Linux: `$XDG_CONFIG_HOME/autostart/khoang.desktop` (fallback `~/.config/autostart/khoang.desktop`).

use std::path::Path;

use dim_core::DimError;

#[allow(dead_code)]
pub const AUTOSTART_ENTRY_NAME: &str = "NexaDock";
#[cfg(target_os = "macos")]
const MACOS_LAUNCH_AGENT_LABEL: &str = "com.multizalodesktop.manager";

/// Returns whether KHOANG is currently registered to launch on OS startup.
pub fn is_autostart_enabled() -> Result<bool, DimError> {
    is_autostart_enabled_impl()
}

/// Enables or disables KHOANG OS startup registration, returning the resulting state.
pub fn set_autostart_enabled(enabled: bool) -> Result<bool, DimError> {
    set_autostart_enabled_impl(enabled)?;
    Ok(enabled)
}

#[cfg(target_os = "windows")]
fn is_autostart_enabled_impl() -> Result<bool, DimError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run_key) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") else {
        return Ok(false);
    };
    match run_key.get_value::<String, _>(AUTOSTART_ENTRY_NAME) {
        Ok(val) => Ok(!val.trim().is_empty()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(DimError::Io(err)),
    }
}

#[cfg(target_os = "windows")]
fn set_autostart_enabled_impl(enabled: bool) -> Result<(), DimError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let exe = std::env::current_exe()?;
        let (run_key, _) =
            hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
        let command = format_windows_run_value(&exe);
        run_key.set_value(AUTOSTART_ENTRY_NAME, &command)?;
    } else if let Ok(run_key) = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        winreg::enums::KEY_SET_VALUE,
    ) {
        match run_key.delete_value(AUTOSTART_ENTRY_NAME) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(DimError::Io(err)),
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn format_windows_run_value(exe_path: &Path) -> String {
    format!("\"{}\"", exe_path.display())
}

#[allow(dead_code)]
fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[allow(dead_code)]
fn format_macos_launch_agent_plist(label: &str, exe_path: &Path) -> String {
    let escaped_label = xml_escape(label);
    let escaped_exe = xml_escape(&exe_path.display().to_string());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>{escaped_label}</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{escaped_exe}</string>\n\
  </array>\n\
  <key>RunAtLoad</key>\n\
  <true/>\n\
</dict>\n\
</plist>\n"
    )
}

#[allow(dead_code)]
fn format_linux_desktop_entry(exe_path: &Path) -> String {
    let escaped_exe = exe_path.display().to_string().replace('"', "\\\"");
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name=NexaDock\n\
Comment=Independent Account Workspace\n\
Exec=\"{escaped_exe}\"\n\
Terminal=false\n\
X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(target_os = "macos")]
fn macos_plist_path() -> Result<std::path::PathBuf, DimError> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            DimError::Io(std::io::Error::other(
                "HOME environment variable is not set",
            ))
        })?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{MACOS_LAUNCH_AGENT_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn is_autostart_enabled_impl() -> Result<bool, DimError> {
    Ok(macos_plist_path()?.exists())
}

#[cfg(target_os = "macos")]
fn set_autostart_enabled_impl(enabled: bool) -> Result<(), DimError> {
    let plist_path = macos_plist_path()?;
    if enabled {
        let exe = std::env::current_exe()?;
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = format_macos_launch_agent_plist(MACOS_LAUNCH_AGENT_LABEL, &exe);
        std::fs::write(&plist_path, body)?;
    } else if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
    }
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn linux_autostart_desktop_path() -> Result<std::path::PathBuf, DimError> {
    let config_dir = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(raw) if !raw.is_empty() => std::path::PathBuf::from(raw),
        _ => {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .ok_or_else(|| {
                    DimError::Io(std::io::Error::other(
                        "HOME environment variable is not set",
                    ))
                })?;
            home.join(".config")
        }
    };
    Ok(config_dir.join("autostart").join("khoang.desktop"))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn is_autostart_enabled_impl() -> Result<bool, DimError> {
    Ok(linux_autostart_desktop_path()?.exists())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn set_autostart_enabled_impl(enabled: bool) -> Result<(), DimError> {
    let desktop_path = linux_autostart_desktop_path()?;
    if enabled {
        let exe = std::env::current_exe()?;
        if let Some(parent) = desktop_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = format_linux_desktop_entry(&exe);
        std::fs::write(&desktop_path, body)?;
    } else if desktop_path.exists() {
        std::fs::remove_file(&desktop_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_autostart_entries_with_nexadock_branding() {
        let win = format_windows_run_value(Path::new("C:\\Program Files\\NexaDock\\nexadock.exe"));
        assert_eq!(win, "\"C:\\Program Files\\NexaDock\\nexadock.exe\"");

        let plist = format_macos_launch_agent_plist(
            "com.multizalodesktop.manager",
            Path::new("/Applications/NexaDock.app/Contents/MacOS/NexaDock"),
        );
        assert!(plist.contains("<string>com.multizalodesktop.manager</string>"));
        assert!(
            plist.contains("<string>/Applications/NexaDock.app/Contents/MacOS/NexaDock</string>")
        );
        assert!(plist.contains("<true/>"));

        let desktop = format_linux_desktop_entry(Path::new("/usr/bin/nexadock"));
        assert!(desktop.contains("Name=NexaDock"));
        assert!(desktop.contains("Exec=\"/usr/bin/nexadock\""));
    }
}
