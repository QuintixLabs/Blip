// ============================================================
// ui/mod.rs
// Blip's window and its logo.
// ============================================================

pub mod logo;
pub mod main_window;

use std::path::PathBuf;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The window's browser data (sidebar state, theme memory) goes in a "Blip"
/// folder. macOS decides this itself.
fn browser_data(app: &AppHandle) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return None;
    }
    app.path().local_data_dir().ok().map(|dir| dir.join("Blip").join("WebView"))
}

fn builder<'a>(app: &'a AppHandle, label: impl Into<String>, page: &str) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()));
    // On Linux the page can't always tell when the desktop switches between
    // light and dark, so it's told: here at the start, then by "system-theme".
    if cfg!(target_os = "linux") {
        builder = builder.initialization_script(format!("window.__SYSTEM_LIGHT__ = {};", logo::system_is_light()));
    }
    match browser_data(app) {
        Some(dir) => builder.data_directory(dir),
        None => builder,
    }
}
