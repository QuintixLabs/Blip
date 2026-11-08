// ============================================================
// blip/mod.rs
// The blip itself: watches your clicks everywhere and shows a
// growing, fading shape where you clicked, unless a rule says not to.
// ============================================================

// How it's split up:
//   style.rs     what a blip looks like and how it moves
//   draw.rs      draws one frame of a blip (the same on every system)
//   windows.rs   Windows: watching clicks, the rules, putting blips on screen
//   linux/       Linux: X11, and Wayland on KDE Plasma and Hyprland
//   macos.rs     macOS: watching clicks, the rules, putting blips on screen
//   other.rs     systems Blip can't draw on

mod draw;
mod style;

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

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod other;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use other as platform;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock, RwLock,
};

use serde::Serialize;

use crate::app::settings::{parse_hex, Settings};
use style::Style;

/// Which "Show on" rules this system can check. The window hides the others.
#[derive(Clone, Copy, Serialize)]
pub struct Features {
    pub text: bool,
    pub links: bool,
    pub taskbar: bool,
    pub games: bool,
    pub apps: bool,
}

/// What the window needs to know about blips here.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Why blips can't show on this system, if they can't.
    pub reason: Option<String>,
    /// Blips work here once Blip may see clicks (Linux on Wayland, macOS); the
    /// window then shows ccess_text and a button labelled ccess_button.
    pub needs_access: bool,
    pub access_text: &'static str,
    pub access_button: &'static str,
    /// The OK is given in another app (macOS System Settings), so the window
    /// keeps checking after the button.
    pub access_waits: bool,
    pub features: Features,
}

/// How a system asks for the OK to see clicks.
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
    /// The text cursor (I-beam) is showing: typing, selecting text.
    pub text_cursor: bool,
    /// The link cursor (hand) is showing.
    pub link_cursor: bool,
    /// The taskbar, Start menu and other system panels (Windows draws them above all apps).
    pub taskbar: bool,
    /// A game holds or hides the cursor, or runs full screen.
    pub games: bool,
}

/// Programs on the "skip these apps" list, lowercase file names.
fn skipped_apps() -> std::sync::RwLockReadGuard<'static, Vec<String>> {
    live().skip_apps.read().unwrap()
}

/// The mouse button a click came from.
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

/// Starts watching clicks. Called once at startup.
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

/// For the window: whether blips work here, and which rules can be checked.
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

/// The Allow button: lets Blip see clicks (Linux on Wayland), or opens the
/// place to switch that on (macOS).
pub fn allow_access() -> Result<(), String> {
    platform::allow_access()
}

/// The blip for a click with that button, in that button's color; None when that button is off.
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
