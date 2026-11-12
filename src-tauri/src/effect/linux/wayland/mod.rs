// ============================================================
// effect/linux/wayland/mod.rs
// Turns mouse buttons into blips: asks the desktop where the cursor
// is, checks the rules and shows the blip.
// ============================================================

pub(super) mod access;
mod hyprland;
mod kde;
mod mice;
mod overlay;

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};

use crate::effect::Button;
use mice::Input;

/// Moving further than this while held is a drag (in the mouse's own units).
const DRAG: i64 = 60;

/// BLIP_DEBUG=1 prints every click, for testing.
fn debug() -> bool {
    std::env::var_os("BLIP_DEBUG").is_some()
}

pub fn start(source: Source) {
    let (blips_tx, blips_rx) = mpsc::channel();
    thread::Builder::new()
        .name("blips".into())
        .spawn(move || {
            if overlay::run_overlay(blips_rx).is_none() {
                eprintln!("Blip: couldn't draw over the screen (no layer shell)");
            }
        })
        .ok();
    thread::Builder::new()
        .name("clicks".into())
        .spawn(move || {
            let Some(finder) = Finder::new(source) else {
                eprintln!("Blip: couldn't ask the desktop where the cursor is");
                return;
            };
            let (input_tx, input_rx) = mpsc::channel();
            mice::watch(input_tx);
            watch_clicks(&finder, input_rx, |x, y, button| {
                let _ = blips_tx.send((x, y, button));
            });
        })
        .ok();
}

// ------------------------------------------------------------
// Clicks
// ------------------------------------------------------------

fn watch_clicks(finder: &Finder, input: Receiver<Input>, play: impl Fn(f64, f64, Button)) {
    let mut held: [Option<(i64, i64)>; 3] = [None; 3];
    for event in input {
        match event {
            Input::Press(button) => held[button as usize] = Some((0, 0)),
            Input::Moved(dx, dy) => {
                for moved in held.iter_mut().flatten() {
                    moved.0 += dx as i64;
                    moved.1 += dy as i64;
                }
            }
            Input::Release(button) => {
                let Some((dx, dy)) = held[button as usize].take() else { continue };
                if dx * dx + dy * dy > DRAG * DRAG || !crate::effect::active() || crate::effect::style_for(button).is_none() {
                    continue;
                }
                let Some(pointer) = finder.find() else { continue };
                if debug() {
                    eprintln!(
                        "Blip: click at {}, {} (pid {}, app {:?}, panel {})",
                        pointer.x, pointer.y, pointer.pid, pointer.app, pointer.panel
                    );
                }
                if crate::effect::rules().taskbar && pointer.panel {
                    continue;
                }
                if pointer.pid != 0 && super::skipped_app(pointer.pid, &pointer.app) {
                    continue;
                }
                play(pointer.x, pointer.y, button);
            }
        }
    }
}

// ------------------------------------------------------------
// Asking the desktop where the cursor is
// ------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Source {
    Kde,
    Hyprland,
}

pub struct Pointer {
    pub x: f64,
    pub y: f64,
    /// The program under the cursor; 0 when unknown.
    pub pid: u32,
    /// Another name the desktop knows that program by (KDE: its desktop file).
    pub app: String,
    pub panel: bool,
}

pub enum Finder {
    Kde(kde::Kde),
    Hyprland(PathBuf),
}

impl Finder {
    pub fn new(source: Source) -> Option<Self> {
        match source {
            Source::Kde => kde::Kde::new().map(Finder::Kde),
            Source::Hyprland => hyprland::socket().map(Finder::Hyprland),
        }
    }

    pub fn find(&self) -> Option<Pointer> {
        match self {
            Finder::Kde(kde) => kde.find(),
            Finder::Hyprland(socket) => hyprland::cursor(socket),
        }
    }
}
