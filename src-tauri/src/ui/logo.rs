// ============================================================
// logo.rs
// Blip's logo for the title bar and the tray, and whether the
// system is light or dark.
// ============================================================

// The logo (a dark circle with a bright ring) shows on light and dark
// backgrounds alike, so one version works for both.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{image::Image, WebviewWindow};

const WINDOW: &[u8] = include_bytes!("../../icons/runtime/logo.png");
/// Windows and macOS show tray icons small: drawn at 32 px straight from the
/// logo, so they stay sharp. Linux panels are often bigger.
#[cfg(not(target_os = "linux"))]
const TRAY: &[u8] = include_bytes!("../../icons/app/32x32.png");
#[cfg(target_os = "linux")]
const TRAY: &[u8] = include_bytes!("../../icons/runtime/tray.png");

/// The last light/dark seen by taskbar_is_light(), kept current by the tray.
static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn remember_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
}

/// Linux: whether the desktop is light, as last checked. The windows can't
/// always see it change themselves there (see ui/mod.rs).
pub fn system_is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

pub fn tray_icon() -> Option<Image<'static>> {
    Image::from_bytes(TRAY).ok()
}

/// On Windows the window already uses the app's icon file, which has a sharp
/// picture for every size (title bar, taskbar, Alt+Tab). Giving it one big
/// picture instead makes Windows shrink it, blurry. Elsewhere it's set here.
pub fn set_window_icon(window: &WebviewWindow) {
    if cfg!(windows) {
        return;
    }
    if let Ok(icon) = Image::from_bytes(WINDOW) {
        let _ = window.set_icon(icon);
    }
}

/// Is the taskbar / menu bar / panel light? On Windows this is its own setting,
/// separate from the apps' light/dark mode.
#[cfg(windows)]
pub fn taskbar_is_light() -> bool {
    use windows::{
        core::w,
        Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    };
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 1
}

#[cfg(not(windows))]
pub fn taskbar_is_light() -> bool {
    matches!(dark_light::detect(), Ok(dark_light::Mode::Light))
}
