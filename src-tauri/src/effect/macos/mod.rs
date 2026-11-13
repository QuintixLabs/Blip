// ============================================================
// effect/macos/mod.rs
// Blips on macOS.
// ============================================================

mod clicks;
mod ffi;
mod overlay;

use std::{
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use super::Features;
use ffi::CGRequestListenEventAccess;

static TAP_OK: AtomicBool = AtomicBool::new(false);

pub fn start() {
    thread::Builder::new().name("clicks".into()).spawn(clicks::watch_clicks).ok();
}

pub fn unsupported_reason() -> Option<String> {
    None
}

pub fn features() -> Features {
    Features { text: true, links: true, taskbar: true, games: false, apps: true }
}

pub fn needs_access() -> bool {
    // Give the tap a moment on a fresh start before asking.
    for _ in 0..10 {
        if TAP_OK.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !TAP_OK.load(Ordering::Relaxed)
}

pub fn access_prompt() -> super::AccessPrompt {
    super::AccessPrompt {
        text: "Blip needs permission to see your clicks. Turn on Blip in System Settings, under Privacy & Security → Input Monitoring.",
        button: "Open System Settings",
        waits: true,
    }
}

/// Asks macOS (it shows its own prompt the first time), then opens the right
/// page of System Settings. Blips start by themselves once it's switched on.
pub fn allow_access() -> Result<(), String> {
    unsafe {
        CGRequestListenEventAccess();
    }
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        .status()
        .map_err(|e| format!("Couldn't open System Settings: {e}"))?;
    Ok(())
}
