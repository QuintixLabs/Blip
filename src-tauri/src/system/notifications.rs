// ============================================================
// notifications.rs
// Blip's notifications ("Blip was updated", "Update available"),
// shown as real system notifications.
// ============================================================

use tauri::{AppHandle, Manager};

/// Update news. These stay in the notification center.
pub fn info(app: &AppHandle, title: &str, body: &str) {
    let app_id = app_id(app);
    let (title, body) = (title.to_owned(), body.to_owned());
    // Off the UI thread: talking to the notification service can take a moment.
    std::thread::spawn(move || {
        if let Err(err) = platform::show(&app_id, &title, &body) {
            eprintln!("Blip: couldn't show a notification: {err}");
        }
    });
}

fn app_id(app: &AppHandle) -> String {
    app.config().identifier.clone()
}

/// Windows only shows notifications from apps it knows, and silently drops the
/// rest. The installer's Start menu shortcut introduces Blip, but that's
/// missing in a dev build, a copy run without installing, or after the shortcut
/// is removed. So Blip introduces itself at every start: its name and icon,
/// under the current user. The uninstaller removes it again (see hooks.nsh).
pub fn register(app: &AppHandle) {
    #[cfg(windows)]
    if let Err(err) = platform::register(app, &app_id(app)) {
        eprintln!("Blip: couldn't register for notifications: {err}");
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::{
        core::HSTRING,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
    };

    pub fn show(app_id: &str, title: &str, body: &str) -> windows::core::Result<()> {
        let xml = format!(
            r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual><audio silent="true"/></toast>"#,
            escape(title),
            escape(body),
        );
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    /// The icon notifications show next to "Blip".
    const ICON: &[u8] = include_bytes!("../../icons/app/128x128.png");

    pub fn register(app: &AppHandle, app_id: &str) -> Result<(), String> {
        use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

        let icon = app.path().cache_dir().map_err(|e| e.to_string())?.join("Blip").join("icon.png");
        if !icon.exists() {
            std::fs::create_dir_all(icon.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&icon, ICON).map_err(|e| e.to_string())?;
        }

        let key = HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{app_id}"));
        for (name, value) in [("DisplayName", app.package_info().name.clone()), ("IconUri", icon.to_string_lossy().into_owned())] {
            let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
            let result = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &key,
                    &HSTRING::from(name),
                    REG_SZ.0,
                    Some(data.as_ptr().cast()),
                    (data.len() * 2) as u32,
                )
            };
            result.ok().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn show(app_id: &str, title: &str, body: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let _ = notify_rust::set_application(app_id);
        }
        let _ = app_id;
        notify_rust::Notification::new()
            .appname("Blip")
            .summary(title)
            .body(body)
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
