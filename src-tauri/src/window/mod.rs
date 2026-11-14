// ============================================================
// window/mod.rs
// The main window: opening, closing and its theme.
// ============================================================

pub mod commands;
pub mod logo;

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};
use tauri::{window::Color, AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::settings::Store;

pub const LABEL: &str = "main";

// ------------------------------------------------------------
// Opening and closing
// ------------------------------------------------------------

pub fn open(app: &AppHandle) -> tauri::Result<()> {
    open_on(app, None)
}

/// For callbacks running on the UI thread (tray, single-instance).
pub fn open_async(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let _ = open(&app);
    });
}

pub fn open_page_async(app: &AppHandle, page: &'static str) {
    let app = app.clone();
    thread::spawn(move || {
        let _ = open_on(&app, Some(page));
    });
}

pub fn toggle_async(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || match window(&app) {
        Some(window) if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) => {
            CLOSED_FROM_TRAY.store(true, Ordering::Relaxed);
            let _ = window.close();
        }
        _ => {
            let _ = open(&app);
        }
    });
}

/// Set when the tray icon closes the window: that hides it, even with Close to tray off.
static CLOSED_FROM_TRAY: AtomicBool = AtomicBool::new(false);

pub fn stays_in_tray(app: &AppHandle) -> bool {
    CLOSED_FROM_TRAY.swap(false, Ordering::Relaxed) || app.state::<Store>().get().settings.close_to_tray
}

pub fn is_visible(app: &AppHandle) -> bool {
    window(app).is_some_and(|w| w.is_visible().unwrap_or(false))
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

fn open_on(app: &AppHandle, page: Option<&str>) -> tauri::Result<()> {
    if let Some(window) = window(app) {
        if let Some(page) = page {
            let _ = window.emit("navigate", page);
        }
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }

    // Told before the page draws anything, so the welcome screen never
    // shows the sidebar for a moment first.
    let settings = app.state::<Store>().get().settings;
    let page = if settings.show_welcome { Some("welcome") } else { page };
    let start = page.map(|p| format!("window.__START_PAGE__ = {p:?};")).unwrap_or_default();
    let theme = window_theme(&settings.theme);
    let background = if theme == Some(Theme::Light) { Color(246, 246, 247, 255) } else { Color(11, 11, 12, 255) };
    let window = builder(app, LABEL, "index.html")
        .initialization_script(&start)
        .title("Blip")
        .inner_size(760.0, 600.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .theme(theme)
        .background_color(background)
        .build()?;

    logo::set_window_icon(&window);
    // Opened from the tray or a second start, Blip isn't the app in front, and
    // Windows would leave a new window behind the others with only a taskbar
    // button. Asking for focus brings it to the front.
    window.set_focus()
}

// ------------------------------------------------------------
// Theme
// ------------------------------------------------------------

/// The title bar follows Blip's theme; None = the system's own setting.
/// On Linux the desktop draws the title bar with its own theme, so it's left alone.
fn window_theme(theme: &str) -> Option<Theme> {
    if cfg!(target_os = "linux") {
        return None;
    }
    match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

pub fn apply_theme(app: &AppHandle, theme: &str) {
    if let Some(window) = window(app) {
        let _ = window.set_theme(window_theme(theme));
    }
}

// ------------------------------------------------------------
// Building it
// ------------------------------------------------------------

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
