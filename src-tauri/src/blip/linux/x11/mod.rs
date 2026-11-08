// ============================================================
// linux/x11/mod.rs
// X11: watches the mouse buttons everywhere, checks the rules, and
// shows each blip in a small see-through window that clicks pass
// through. Needs no special permission, only a compositor (most
// desktops have one) so the windows can be see-through.
// ============================================================

// Two threads, each with its own connection to X:
//   clicks  hears every button press and release, and checks the rules (clicks.rs)
//   blips   owns the blip windows and redraws them until they're done (overlay.rs)

mod clicks;
mod overlay;

use std::{
    sync::mpsc::{self},
    thread,
};

use x11rb::{
    protocol::{
        xinput::ConnectionExt as _,
        xproto::{AtomEnum, ConnectionExt as _, Window},
    },
    rust_connection::RustConnection,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        WM_STATE,
        _NET_WM_PID,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        RESOURCE_MANAGER,
    }
}

pub fn start() {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("blips".into())
        .spawn(move || {
            if overlay::run_overlay(rx).is_none() {
                eprintln!("Blip: couldn't draw over the screen (X11)");
            }
        })
        .ok();
    thread::Builder::new()
        .name("clicks".into())
        .spawn(move || {
            if clicks::watch_clicks(tx).is_none() {
                eprintln!("Blip: couldn't watch clicks (X11)");
            }
        })
        .ok();
}

pub fn unsupported_reason() -> Option<String> {
    let Ok((conn, screen)) = RustConnection::connect(None) else {
        return Some("Blip couldn't connect to the display.".into());
    };
    // See-through windows need a compositor, which owns this selection.
    let owner = conn
        .intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes())
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|atom| conn.get_selection_owner(atom.atom).ok()?.reply().ok())
        .map(|r| r.owner)
        .unwrap_or(0);
    if owner == 0 {
        return Some("Blips need see-through windows. Turn on compositing (desktop effects) in your desktop's settings.".into());
    }
    if conn.xinput_xi_query_version(2, 2).ok().and_then(|c| c.reply().ok()).is_none() {
        return Some("This display can't report clicks to Blip (XInput 2 is missing).".into());
    }
    None
}

/// The screen's scaling: 1.0 at 96 dpi, 1.5 at 144 (from Xft.dpi, which desktops set).
fn scale(conn: &RustConnection, root: Window, atoms: &Atoms) -> f64 {
    let text = conn
        .get_property(false, root, atoms.RESOURCE_MANAGER, AtomEnum::STRING, 0, 100_000)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| String::from_utf8_lossy(&r.value).into_owned())
        .unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("Xft.dpi:"))
        .and_then(|dpi| dpi.trim().parse::<f64>().ok())
        .map(|dpi| (dpi / 96.0).max(0.5))
        .unwrap_or(1.0)
}
