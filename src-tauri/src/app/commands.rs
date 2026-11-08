// ============================================================
// commands.rs
// Everything the window can ask the app to do: save settings,
// check for updates, open a link, and so on.
// ============================================================

// Anything that touches windows is async: plain commands run on the UI
// thread, and doing that work there can freeze the app on Windows.

use tauri::{AppHandle, State};

use crate::{
    app::{
        settings::{self, Data, Settings, Store},
        updates,
    },
    blip,
    system::{apps, startup},
    ui::main_window,
};

// ------------------------------------------------------------
// Settings
// ------------------------------------------------------------

#[tauri::command]
pub fn state(store: State<Store>) -> Data {
    store.get()
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, store: State<'_, Store>, settings: Settings) -> Result<Data, String> {
    let previous = store.get().settings;
    if settings.start_with_computer != previous.start_with_computer {
        startup::set(&app, settings.start_with_computer)?;
    }
    if settings.theme != previous.theme {
        main_window::apply_theme(&app, &settings.theme);
    }
    let data = store.update(&app, |data| data.settings = settings);
    blip::apply(&data.settings);
    Ok(data)
}

/// "Reset all settings": back to the defaults, keeping the welcome screen done.
#[tauri::command]
pub async fn reset_settings(app: AppHandle, store: State<'_, Store>) -> Result<Data, String> {
    let settings = Settings { show_welcome: false, ..Settings::default() };
    save_settings(app, store, settings).await
}

/// The Blip page's "Reset to defaults": everything on that page goes back to
/// how Blip comes. The Settings page's choices stay.
#[tauri::command]
pub async fn reset_look(app: AppHandle, store: State<'_, Store>) -> Result<Data, String> {
    let d = Settings::default();
    let current = store.get().settings;
    let settings = Settings {
        shape: d.shape,
        left_click: d.left_click,
        color: d.color,
        right_click: d.right_click,
        right_color: d.right_color,
        middle_click: d.middle_click,
        middle_color: d.middle_color,
        start_size: d.start_size,
        end_size: d.end_size,
        opacity: d.opacity,
        speed: d.speed,
        fade_start: d.fade_start,
        skip_text_cursor: d.skip_text_cursor,
        skip_link_cursor: d.skip_link_cursor,
        skip_taskbar: d.skip_taskbar,
        skip_games: d.skip_games,
        skip_apps: d.skip_apps,
        ..current
    };
    save_settings(app, store, settings).await
}

/// "Skip these apps": installed and open programs, with names and icons.
#[tauri::command]
pub async fn list_apps() -> Vec<apps::AppInfo> {
    tauri::async_runtime::spawn_blocking(apps::all).await.unwrap_or_default()
}

/// "Browse...": a program picked from disk; None when the picker is cancelled.
#[tauri::command]
pub async fn browse_app(app: AppHandle) -> Option<apps::AppInfo> {
    use tauri_plugin_dialog::DialogExt;
    let mut picker = app.dialog().file().set_title("Choose an app");
    if cfg!(windows) {
        picker = picker.add_filter("Apps", &["exe"]);
    }
    let path = picker.blocking_pick_file()?.into_path().ok()?;
    let path = path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || apps::info_for(&path)).await.ok()
}

/// None when blips work on this system; otherwise why not, for the window to show.
#[tauri::command]
pub fn blip_status() -> Option<String> {
    blip::unsupported_reason()
}

// ------------------------------------------------------------
// Updates
// ------------------------------------------------------------

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    updates::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    updates::install(&app).await
}

/// Version of a downloaded update waiting to install, if any.
#[tauri::command]
pub async fn pending_update(app: AppHandle) -> Option<String> {
    updates::ready_version(&app)
}

// ------------------------------------------------------------
// Links and folders
// ------------------------------------------------------------

/// Footer links. Only these addresses can be opened from the window.
const LINKS: [(&str, &str); 3] = [
    ("github", "https://github.com/QuintixLabs/Blip"),
    ("website", "https://itsblip.netlify.app"),
    ("kofi", "https://ko-fi.com/fr0stiwnl"),
];

#[tauri::command]
pub async fn open_link(app: AppHandle, name: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let (_, url) = LINKS.iter().find(|(key, _)| *key == name).ok_or("unknown link")?;
    app.opener().open_url(*url, None::<&str>).map_err(|e| e.to_string())
}

/// "Open settings folder": shows blip.json in the file manager.
#[tauri::command]
pub async fn open_settings_folder(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = settings::settings_dir(&app);
    let file = dir.join("blip.json");
    if file.exists() {
        app.opener().reveal_item_in_dir(&file).map_err(|e| e.to_string())
    } else {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    }
}
