// ============================================================
// macos/mod.rs
// macOS: watches the mouse buttons everywhere (an event tap),
// checks the rules, and shows each blip in a small see-through
// window above everything that clicks pass through.
// ============================================================

// Two parts:
//   clicks  a thread with the event tap. macOS may want your OK first
//           (Privacy & Security → Input Monitoring); until then it keeps
//           trying every couple of seconds, so blips start once you allow it.
//   blips   the windows live on the main thread, as macOS requires. While
//           any blip plays, a small ticker asks the main thread for the
//           next frame every few milliseconds.

// How it's split up:
//   ffi.rs      the macOS functions Blip calls, by their C names
//   clicks.rs   the event tap and the rules (menu bar, Dock, skipped apps)
//   overlay.rs  the see-through windows

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

/// The event tap is running: blips work.
static TAP_OK: AtomicBool = AtomicBool::new(false);

pub fn start() {
    thread::Builder::new().name("clicks".into()).spawn(clicks::watch_clicks).ok();
}

pub fn unsupported_reason() -> Option<String> {
    None
}

pub fn features() -> Features {
    // The window under the cursor gives the app, and the menu bar and Dock.
    Features { text: false, links: false, taskbar: true, games: false, apps: true }
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
