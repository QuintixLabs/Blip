// ============================================================
// tray.rs
// The tray icon next to the clock and its menu. Click opens Blip
// (click again to close it); right-click shows the menu.
// ============================================================

//   Open Blip
//   Settings
//   ─────────
//   ✓ Pause blips
//   ─────────
//   Exit
//
// The menu is rebuilt whenever the settings change (see Store::update).

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use crate::{
    app::settings::Store,
    blip,
    ui::{logo, main_window},
};

const ID: &str = "blip";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(ID)
        .tooltip(tooltip(app))
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
    tray.build(app)?;
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

/// Rebuilds the menu and tooltip from the current settings.
pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(ID) {
        if let Ok(menu) = menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(tooltip(app)));
    }
}

/// Hover text while something is happening ("Updating to 2.0.1…"); None goes back to normal.
pub fn set_status(app: &AppHandle, status: Option<&str>) {
    if let Some(tray) = app.tray_by_id(ID) {
        let text = status.map(|s| format!("Blip · {s}")).unwrap_or_else(|| tooltip(app));
        let _ = tray.set_tooltip(Some(text));
    }
}

fn tooltip(app: &AppHandle) -> String {
    let enabled = app.try_state::<Store>().map(|s| s.get().settings.enabled).unwrap_or(true);
    if enabled { "Blip".into() } else { "Blip · paused".into() }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let enabled = app.state::<Store>().get().settings.enabled;
    let open = MenuItem::with_id(app, "open", "Open Blip", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause blips", true, !enabled, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&open, &settings, &PredefinedMenuItem::separator(app)?, &pause, &PredefinedMenuItem::separator(app)?, &exit],
    )
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => main_window::open_async(app),
        "settings" => main_window::open_page_async(app, "settings"),
        "pause" => toggle_pause(app),
        "exit" => app.exit(0),
        _ => {}
    }
}

/// Pauses or resumes blips, like the switch in the window.
fn toggle_pause(app: &AppHandle) {
    let store = app.state::<Store>();
    let data = store.update(app, |data| data.settings.enabled = !data.settings.enabled);
    blip::apply(&data.settings);
}
