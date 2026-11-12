// ============================================================
// tray/updates.rs
// Checking for, downloading and installing new versions.
// ============================================================

// Manual:     Settings -> Check for updates; installs right away and reopens.
// Automatic:  at every start and every 6 hours; installs once the window is
//             closed (restarting into the tray) or as Blip exits.
// Updates are signed; anything unsigned is refused.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::notifications;
use crate::{
    settings::{self, Store, UpdateNotice},
    tray, window,
};

const CHECK_EVERY_MS: u64 = 6 * 60 * 60 * 1000;
const LOOP_EVERY: Duration = Duration::from_secs(15 * 60);
const IDLE_POLL: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Pending {
    found: Mutex<Option<Update>>,
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    busy: AtomicBool,
}

/// Right after publishing, GitHub can serve the previous latest.json from its
/// cache for a while; ask for a fresh copy every time.
fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .timeout(REQUEST_TIMEOUT)
        .header("Cache-Control", "no-cache")
        .and_then(|b| b.header("Pragma", "no-cache"))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------
// Manual
// ------------------------------------------------------------

pub async fn check(app: &AppHandle) -> Result<Option<String>, String> {
    let update = updater(app)?.check().await.map_err(|e| e.to_string())?;
    let version = update.as_ref().map(|u| u.version.clone());
    *app.state::<Pending>().found.lock().unwrap() = update;
    Ok(version)
}

pub async fn install(app: &AppHandle) -> Result<(), String> {
    let pending = app.state::<Pending>();
    let ready = pending.ready.lock().unwrap().take();
    let (update, bytes) = match ready {
        Some(ready) => ready,
        None => {
            let update = pending.found.lock().unwrap().take().ok_or("no update to install")?;
            let mut received = 0usize;
            let progress = app.clone();
            let bytes = update
                .download(
                    move |chunk, total| {
                        received += chunk;
                        if let Some(total) = total.filter(|t| *t > 0) {
                            let _ = progress.emit("update-progress", (received as u64 * 100 / total).min(100));
                        }
                    },
                    || {},
                )
                .await
                .map_err(|e| e.to_string())?;
            (update, bytes)
        }
    };
    apply(app, update, &bytes, "manual")
}

// ------------------------------------------------------------
// Automatic
// ------------------------------------------------------------

/// `delay` lets a login start settle first.
pub fn start_auto(app: &AppHandle, delay: Duration) {
    if cfg!(debug_assertions) {
        return; // a dev build must never replace itself with a release
    }
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        // Once at every start, then every few hours while Blip keeps running.
        let mut first = true;
        loop {
            let data = app.state::<Store>().get();
            let due = first || settings::now_ms().saturating_sub(data.last_update_check) >= CHECK_EVERY_MS;
            if data.settings.auto_update && due {
                auto_update(&app);
            }
            first = false;
            thread::sleep(LOOP_EVERY);
        }
    });
}

fn auto_update(app: &AppHandle) {
    let pending = app.state::<Pending>();
    if pending.busy.swap(true, Ordering::AcqRel) || pending.ready.lock().unwrap().is_some() {
        pending.busy.store(false, Ordering::Release);
        return;
    }

    let found = tauri::async_runtime::block_on(async { updater(app)?.check().await.map_err(|e| e.to_string()) });
    app.state::<Store>().update(app, |data| data.last_update_check = settings::now_ms());

    if let Ok(Some(update)) = found {
        let version = update.version.clone();
        if !installs_silently() {
            notifications::info(app, "Update available", &format!("Blip {version} is ready. Open Settings → Updates to install it."));
        } else {
            status(app, Some(&format!("Updating to {version}…")));
            let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
            status(app, None);
            match bytes {
                Ok(bytes) => {
                    *pending.ready.lock().unwrap() = Some((update, bytes));
                    let _ = app.emit("update-ready", &version);
                    install_when_idle(app);
                }
                Err(err) => eprintln!("Blip: update download failed: {err}"),
            }
        }
    }
    pending.busy.store(false, Ordering::Release);
}

/// Linux .deb/.rpm installs need your password, so they're never automatic.
fn installs_silently() -> bool {
    !matches!(
        tauri::utils::platform::bundle_type(),
        Some(tauri::utils::config::BundleType::Deb | tauri::utils::config::BundleType::Rpm)
    )
}

fn install_when_idle(app: &AppHandle) {
    loop {
        if app.state::<Pending>().ready.lock().unwrap().is_none() {
            return; // installed another way (Settings button, or on exit)
        }
        if !window::is_visible(app) {
            let ready = app.state::<Pending>().ready.lock().unwrap().take();
            if let Some((update, bytes)) = ready {
                if let Err(err) = apply(app, update, &bytes, "background") {
                    eprintln!("Blip: update install failed: {err}");
                }
            }
            return;
        }
        thread::sleep(IDLE_POLL);
    }
}

pub fn install_on_exit(app: &AppHandle) {
    let ready = app.state::<Pending>().ready.lock().unwrap().take();
    if let Some((update, bytes)) = ready {
        let _ = apply(app, update, &bytes, "exit");
    }
}

// ------------------------------------------------------------
// Installing
// ------------------------------------------------------------

/// Leaves a note for the next start, then installs. On Windows the installer
/// takes over and this process ends here.
fn apply(app: &AppHandle, update: Update, bytes: &[u8], mode: &str) -> Result<(), String> {
    let notice = UpdateNotice { version: update.version.clone(), mode: mode.into() };
    app.state::<Store>().update(app, |data| data.update_notice = Some(notice));

    let reopen = mode != "exit";
    let update = update.restart_after_install(reopen);
    if let Err(err) = update.install(bytes) {
        app.state::<Store>().update(app, |data| data.update_notice = None);
        return Err(err.to_string());
    }
    if reopen {
        app.restart();
    }
    Ok(())
}

fn status(app: &AppHandle, text: Option<&str>) {
    let _ = app.emit("update-status", text);
    tray::set_status(app, text);
}

/// "Update automatically" switched off: a downloaded update waiting to install is
/// dropped, so Blip stays on this version.
pub fn cancel(app: &AppHandle) {
    if app.state::<Pending>().ready.lock().unwrap().take().is_some() {
        let _ = app.emit("update-cancelled", ());
    }
}

pub fn ready_version(app: &AppHandle) -> Option<String> {
    app.state::<Pending>().ready.lock().unwrap().as_ref().map(|(update, _)| update.version.clone())
}

/// Read at startup: was Blip just updated? Tells you once.
pub fn just_updated(app: &AppHandle) -> Option<UpdateNotice> {
    let notice = app.state::<Store>().get().update_notice?;
    app.state::<Store>().update(app, |data| data.update_notice = None);
    if notice.version != app.package_info().version.to_string() {
        return None; // the install didn't happen
    }
    notifications::info(app, &format!("Blip was updated to {}", notice.version), "You're on the latest version.");
    Some(notice)
}
