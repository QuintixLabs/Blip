// ============================================================
// main.rs
// Starts Blip: the tray icon, the click watcher and the first window.
// ============================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod effect;
mod installed_apps;
mod settings;
mod tray;
mod window;

use std::time::Duration;
use tauri::{AppHandle, Manager, RunEvent};

use tray::{notifications, startup, updates};
use window::commands;

fn main() {
    let app = tauri::Builder::default()
        // Opening Blip again while it runs just opens its window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::open_async(app)))
        .plugin(startup::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(updates::Pending::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle();
            let store = settings::Store::load(handle);
            let settings = store.get().settings;
            app.manage(store);

            notifications::register(handle);
            tray::create(handle)?;
            effect::start(&settings);
            // Keep the system's "start with computer" in step with the setting.
            let _ = startup::set(handle, settings.start_with_computer);
            open_on_start(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::state,
            commands::save_settings,
            commands::reset_settings,
            commands::reset_look,
            commands::blip_info,
            commands::blip_allow_access,
            commands::list_apps,
            commands::browse_app,
            commands::app_info,
            commands::reveal_app,
            commands::check_update,
            commands::install_update,
            commands::pending_update,
            commands::open_link,
            commands::open_settings_folder,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start Blip");

    app.run(|app, event| match event {
        // Last window closed: Blip keeps running in the tray, that's its job,
        // unless Close to tray is off (closing from the tray icon still keeps it).
        // Exit from the tray menu (code is set) quits.
        RunEvent::ExitRequested { code, api, .. } => {
            if code.is_none() && window::stays_in_tray(app) {
                api.prevent_exit();
            } else {
                updates::install_on_exit(app);
            }
        }
        // macOS: opening a running app asks it to reopen instead of starting a
        // second copy, so it opens the window like a second start does elsewhere.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => window::open_async(app),
        _ => {}
    });
}

fn open_on_start(app: &AppHandle) {
    let at_login = startup::launched_at_login();
    let show_welcome = app.state::<settings::Store>().get().settings.show_welcome;
    match updates::just_updated(app).map(|notice| notice.mode) {
        // Restarted by a quiet update: back to the tray, nothing pops up.
        Some(mode) if mode == "background" => {}
        Some(_) => window::open_async(app),
        // Login start: quietly in the tray, unless setup isn't done yet.
        _ if at_login && !show_welcome => {}
        _ => window::open_async(app),
    }

    // A login start waits a minute so it doesn't slow down logging in.
    updates::start_auto(app, Duration::from_secs(if at_login { 60 } else { 10 }));
}
