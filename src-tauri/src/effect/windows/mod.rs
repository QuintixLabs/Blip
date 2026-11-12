// ============================================================
// effect/windows/mod.rs
// Blips on Windows.
// ============================================================

mod clicks;
mod overlay;

use std::{
    sync::{
        mpsc::{self, Sender},
        Mutex, OnceLock,
    },
    thread,
};

use windows::Win32::Foundation::POINT;

use crate::effect::Button;

static CLICKS: OnceLock<Mutex<Sender<(POINT, Button)>>> = OnceLock::new();

pub fn start() {
    let (tx, rx) = mpsc::channel();
    if CLICKS.set(Mutex::new(tx)).is_err() {
        return; // already running
    }
    thread::Builder::new().name("blips".into()).spawn(move || overlay::Blips::new().run(rx)).ok();
    thread::Builder::new().name("clicks".into()).spawn(clicks::watch_clicks).ok();
}

pub fn unsupported_reason() -> Option<String> {
    None
}

pub fn features() -> super::Features {
    super::Features { text: true, links: true, taskbar: true, games: true, apps: true }
}

pub fn needs_access() -> bool {
    false
}

pub fn access_prompt() -> super::AccessPrompt {
    super::AccessPrompt::NONE
}

pub fn allow_access() -> Result<(), String> {
    Ok(())
}
