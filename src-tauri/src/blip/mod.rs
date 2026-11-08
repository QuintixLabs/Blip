// ============================================================
// blip/mod.rs
// The blip itself: watches your clicks everywhere and shows a
// growing, fading shape where you clicked, unless a rule says not to.
// ============================================================

// How it's split up:
//   style.rs     what a blip looks like and how it moves
//   draw.rs      draws one frame of a blip (the same on every system)
//   windows.rs   Windows: watching clicks, the rules, putting blips on screen
//   other.rs     systems Blip can't draw on yet

mod draw;
mod style;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
use other as platform;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock, RwLock,
};

use crate::app::settings::Settings;
use style::Style;

/// When a click shouldn't get a blip.
#[derive(Clone, Copy)]
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

/// Read by the click watcher on every click, so it's kept ready here.
struct Live {
    style: RwLock<Style>,
    rules: RwLock<Rules>,
    /// Off until the welcome screen is done: blips start with "Start using Blip".
    active: AtomicBool,
}

static LIVE: OnceLock<Live> = OnceLock::new();

fn live() -> &'static Live {
    LIVE.get_or_init(|| {
        let s = Settings::default();
        Live { style: RwLock::new(Style::from_settings(&s)), rules: RwLock::new(rules_of(&s)), active: AtomicBool::new(false) }
    })
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
    *live.rules.write().unwrap() = rules_of(settings);
    live.active.store(!settings.show_welcome, Ordering::Relaxed);
}

/// None when blips work here; otherwise why not, in plain words (shown in the window).
pub fn unsupported_reason() -> Option<String> {
    platform::unsupported_reason()
}

fn style() -> Style {
    *live().style.read().unwrap()
}

fn rules() -> Rules {
    *live().rules.read().unwrap()
}

fn active() -> bool {
    live().active.load(Ordering::Relaxed)
}
