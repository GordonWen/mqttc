//! OS integration with no UI dependency: dark-mode detection and default fonts.
//!
//! Windows reads the registry directly (cheap, refreshed every second). macOS and
//! Linux shell out to `defaults` / `gsettings`, so their result is cached 30s to
//! keep idle CPU at zero on the System skin.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct Cache {
    at: Instant,
    dark: bool,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache {
            at: Instant::now() - Duration::from_secs(3600),
            dark: false,
        })
    })
}

/// True when the OS is in a dark appearance. Cached per platform policy.
pub fn system_prefers_dark() -> bool {
    #[cfg(windows)]
    const TTL: Duration = Duration::from_secs(1);
    #[cfg(not(windows))]
    const TTL: Duration = Duration::from_secs(30);

    let mut guard = cache().lock().expect("dark cache");
    if guard.at.elapsed() >= TTL {
        guard.dark = detect_dark();
        guard.at = Instant::now();
    }
    guard.dark
}

#[cfg(windows)]
fn detect_dark() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut _,
            &mut size,
        )
    };
    // AppsUseLightTheme = 0 means dark.
    status == ERROR_SUCCESS && data == 0
}

#[cfg(target_os = "macos")]
fn detect_dark() -> bool {
    use std::process::Command;
    Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).contains("Dark"))
        .unwrap_or(false)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect_dark() -> bool {
    use std::process::Command;
    let color_scheme = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_lowercase())
        .unwrap_or_default();
    if color_scheme.contains("dark") {
        return true;
    }
    let gtk_theme = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_lowercase())
        .unwrap_or_default();
    gtk_theme.contains("dark")
}

#[cfg(not(any(windows, unix)))]
fn detect_dark() -> bool {
    false
}

/// `(ui, mono)` families. Every shell relies on system font fallback for CJK.
pub fn platform_fonts() -> (&'static str, &'static str) {
    #[cfg(target_os = "macos")]
    {
        ("PingFang SC", "Menlo")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ("Noto Sans CJK SC", "DejaVu Sans Mono")
    }
    #[cfg(windows)]
    {
        ("Microsoft YaHei UI", "Cascadia Mono")
    }
    #[cfg(not(any(windows, unix)))]
    {
        ("sans-serif", "monospace")
    }
}

/// Combo-box friendly labels for the available skins.
pub fn skin_options() -> Vec<(i32, String)> {
    crate::model::Skin::ALL
        .iter()
        .map(|skin| (skin.to_index(), skin.display()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_cached_and_returns() {
        let a = system_prefers_dark();
        let b = system_prefers_dark();
        assert_eq!(a, b);
        let (ui, mono) = platform_fonts();
        assert!(!ui.is_empty());
        assert!(!mono.is_empty());
    }

    #[test]
    fn skin_options_cover_all_six() {
        assert_eq!(skin_options().len(), 6);
    }
}
