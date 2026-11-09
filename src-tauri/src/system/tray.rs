// ============================================================
// tray.rs
// The tray icon next to the clock and its menu. Click opens Blip
// (click again to close it); right-click shows the menu.
// ============================================================

//   Open Blip
//   Settings
//   ─────────
//   Exit
//
// Blip shows blips while it runs; Exit stops it.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use crate::{
    app::settings::Store,
    ui::{logo, main_window},
};

const ID: &str = "blip";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(ID)
        .tooltip("Blip")
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                main_window::toggle_async(tray.app_handle());
            }
        });

    if let Some(icon) = logo::tray_icon().or_else(|| app.default_window_icon().cloned()) {
        tray = tray.icon(icon);
    }
    let tray = tray.build(app)?;
    let _ = tray.set_visible(shown(app));
    watch_theme(app);
    Ok(())
}

/// Linux: the windows don't always hear when the desktop switches between light
/// and dark, so they get told ("system-theme"). See ui/mod.rs.
fn watch_theme(app: &AppHandle) {
    let mut light = logo::taskbar_is_light();
    logo::remember_light(light);
    if !cfg!(target_os = "linux") {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let now = logo::taskbar_is_light();
        if now != light {
            light = now;
            logo::remember_light(light);
            let _ = app.emit("system-theme", light);
        }
    });
}

/// Called after every settings save (see Store::update). The menu never changes;
/// only whether the icon shows.
pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_visible(shown(app));
    }
}

/// "Tray icon" in Settings. Hidden, Blip is opened by starting it again
/// (it's already running, so that just opens the window).
fn shown(app: &AppHandle) -> bool {
    app.state::<Store>().get().settings.show_in_tray
}

/// Hover text while something is happening ("Updating to 2.0.1…"); None goes back to normal.
pub fn set_status(app: &AppHandle, status: Option<&str>) {
    if let Some(tray) = app.tray_by_id(ID) {
        let text = status.map(|s| format!("Blip · {s}")).unwrap_or_else(|| "Blip".into());
        let _ = tray.set_tooltip(Some(text));
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", "Open Blip", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &PredefinedMenuItem::separator(app)?, &exit])
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => main_window::open_async(app),
        "settings" => main_window::open_page_async(app, "settings"),
        "exit" => app.exit(0),
        _ => {}
    }
}
