// ============================================================
// window/commands.rs
// What the window can ask the app to do.
// ============================================================

// Anything that touches windows is async: plain commands run on the UI
// thread, and doing that work there can freeze the app on Windows.

use tauri::{AppHandle, State};

use crate::{
    effect, installed_apps as apps,
    settings::{self, Data, Settings, Store},
    tray::{startup, updates},
    window,
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
        window::apply_theme(&app, &settings.theme);
    }
    if previous.auto_update && !settings.auto_update {
        updates::cancel(&app);
    }
    let data = store.update(&app, |data| data.settings = settings);
    effect::apply(&data.settings);
    Ok(data)
}

#[tauri::command]
pub async fn reset_settings(app: AppHandle, store: State<'_, Store>) -> Result<Data, String> {
    let settings = Settings { show_welcome: false, ..Settings::default() };
    save_settings(app, store, settings).await
}

/// The Blip page's "Reset to defaults": only that page's settings.
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

#[tauri::command]
pub async fn list_apps() -> Vec<apps::AppInfo> {
    tauri::async_runtime::spawn_blocking(apps::all).await.unwrap_or_default()
}

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

#[tauri::command]
pub async fn blip_info() -> effect::Info {
    tauri::async_runtime::spawn_blocking(effect::info).await.unwrap_or(effect::Info {
        reason: None,
        needs_access: false,
        access_text: "",
        access_button: "",
        access_waits: false,
        features: effect::Features { text: true, links: true, taskbar: true, games: true, apps: true },
    })
}

#[tauri::command]
pub async fn blip_allow_access() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(effect::allow_access).await.map_err(|e| e.to_string())?
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

#[tauri::command]
pub async fn pending_update(app: AppHandle) -> Option<String> {
    updates::ready_version(&app)
}

// ------------------------------------------------------------
// Links and folders
// ------------------------------------------------------------

/// The only addresses the window can open.
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

#[tauri::command]
pub async fn reveal_app(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !std::path::Path::new(&path).exists() {
        return Err("That app isn't there anymore.".into());
    }
    app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

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
