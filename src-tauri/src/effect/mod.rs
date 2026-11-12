// ============================================================
// effect/mod.rs
// The blip itself: a growing, fading shape wherever you click,
// unless a rule says not to.
// ============================================================

// How it's split up:
//   shape/                  how a blip moves, and drawing it
//   windows/ linux/ macos/  each system's click watcher, rules and overlay

mod shape;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

/// Systems without blips: the window says why.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod unsupported {
    use super::{AccessPrompt, Features};

    pub fn start() {}

    pub fn unsupported_reason() -> Option<String> {
        Some("Blips don't work on this system yet. They're coming in a later version.".into())
    }

    pub fn features() -> Features {
        Features { text: false, links: false, taskbar: false, games: false, apps: false }
    }

    pub fn needs_access() -> bool {
        false
    }

    pub fn access_prompt() -> AccessPrompt {
        AccessPrompt::NONE
    }

    pub fn allow_access() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use unsupported as platform;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock, RwLock,
};

use serde::Serialize;

use crate::settings::{parse_hex, Settings};
use shape::style::Style;

/// Which "Show on" rules this system can check. The window hides the others.
#[derive(Clone, Copy, Serialize)]
pub struct Features {
    pub text: bool,
    pub links: bool,
    pub taskbar: bool,
    pub games: bool,
    pub apps: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub reason: Option<String>,
    /// Blips need the user's OK first; the window shows `access_text` and `access_button`.
    pub needs_access: bool,
    pub access_text: &'static str,
    pub access_button: &'static str,
    /// The OK is given in another app (macOS System Settings), so the window
    /// keeps checking after the button.
    pub access_waits: bool,
    pub features: Features,
}

pub struct AccessPrompt {
    pub text: &'static str,
    pub button: &'static str,
    pub waits: bool,
}

impl AccessPrompt {
    #[allow(dead_code)] // for systems that never need it
    pub const NONE: Self = Self { text: "", button: "", waits: false };
}

/// When a click shouldn't get a blip. Not every system can check every rule.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Rules {
    pub text_cursor: bool,
    pub link_cursor: bool,
    pub taskbar: bool,
    pub games: bool,
}

fn skipped_apps() -> std::sync::RwLockReadGuard<'static, Vec<String>> {
    live().skip_apps.read().unwrap()
}

#[derive(Clone, Copy)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Read by the click watcher on every click, so it's kept ready here.
struct Live {
    style: RwLock<Style>,
    /// Each button's color, or None when that button gets no blips.
    colors: RwLock<[Option<[u8; 3]>; 3]>,
    rules: RwLock<Rules>,
    skip_apps: RwLock<Vec<String>>,
    /// Off until the welcome screen is done: blips start with "Start using Blip".
    active: AtomicBool,
}

static LIVE: OnceLock<Live> = OnceLock::new();

fn live() -> &'static Live {
    LIVE.get_or_init(|| {
        let s = Settings::default();
        Live {
            style: RwLock::new(Style::from_settings(&s)),
            colors: RwLock::new(colors_of(&s)),
            rules: RwLock::new(rules_of(&s)),
            skip_apps: RwLock::new(Vec::new()),
            active: AtomicBool::new(false),
        }
    })
}

fn colors_of(s: &Settings) -> [Option<[u8; 3]>; 3] {
    let pick = |on: bool, hex: &str| if on { parse_hex(hex) } else { None };
    [pick(s.left_click, &s.color), pick(s.right_click, &s.right_color), pick(s.middle_click, &s.middle_color)]
}

fn rules_of(s: &Settings) -> Rules {
    Rules { text_cursor: s.skip_text_cursor, link_cursor: s.skip_link_cursor, taskbar: s.skip_taskbar, games: s.skip_games }
}

pub fn start(settings: &Settings) {
    apply(settings);
    platform::start();
}

/// New settings take effect from the next click.
pub fn apply(settings: &Settings) {
    let live = live();
    *live.style.write().unwrap() = Style::from_settings(settings);
    *live.colors.write().unwrap() = colors_of(settings);
    *live.rules.write().unwrap() = rules_of(settings);
    *live.skip_apps.write().unwrap() = settings.skip_apps.iter().map(|a| a.exe.clone()).collect();
    live.active.store(!settings.show_welcome, Ordering::Relaxed);
}

pub fn info() -> Info {
    let prompt = platform::access_prompt();
    Info {
        reason: platform::unsupported_reason(),
        needs_access: platform::needs_access(),
        access_text: prompt.text,
        access_button: prompt.button,
        access_waits: prompt.waits,
        features: platform::features(),
    }
}

pub fn allow_access() -> Result<(), String> {
    platform::allow_access()
}

fn style_for(button: Button) -> Option<Style> {
    let color = live().colors.read().unwrap()[button as usize]?;
    Some(Style { color, ..*live().style.read().unwrap() })
}

fn rules() -> Rules {
    *live().rules.read().unwrap()
}

fn active() -> bool {
    live().active.load(Ordering::Relaxed)
}
