// ============================================================
// linux/x11/clicks.rs
// X11: hears every click and checks the rules.
// ============================================================

use std::sync::mpsc::Sender;

use x11rb::{
    connection::Connection,
    protocol::{
        xfixes::ConnectionExt as _,
        xinput::{self, ConnectionExt as _},
        xproto::{AtomEnum, ConnectionExt as _, Window},
        Event,
    },
    rust_connection::RustConnection,
};

use crate::blip::Button;

use super::{scale, Atoms};

/// Moving further than this (at 100 %) while held is a drag.
const DRAG_PX: f64 = 40.0;

// ------------------------------------------------------------
// Clicks and the rules
// ------------------------------------------------------------

struct Press {
    at: (i16, i16),
    skip: bool,
}

pub(super) fn watch_clicks(blips: Sender<(f64, f64, Button)>) -> Option<()> {
    let (conn, screen) = RustConnection::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    conn.xinput_xi_query_version(2, 2).ok()?.reply().ok()?;
    conn.xfixes_query_version(5, 0).ok()?.reply().ok()?;
    // Raw button events reach Blip whatever window (or game) has the mouse.
    let mask = xinput::EventMask {
        deviceid: 1, // all master devices
        mask: vec![xinput::XIEventMask::RAW_BUTTON_PRESS | xinput::XIEventMask::RAW_BUTTON_RELEASE],
    };
    conn.xinput_xi_select_events(root, &[mask]).ok()?;
    conn.flush().ok()?;

    let mut pressed: [Option<Press>; 3] = [None, None, None];
    loop {
        let (down, detail) = match conn.wait_for_event().ok()? {
            Event::XinputRawButtonPress(e) => (true, e.detail),
            Event::XinputRawButtonRelease(e) => (false, e.detail),
            _ => continue,
        };
        let button = match detail {
            1 => Button::Left,
            2 => Button::Middle,
            3 => Button::Right,
            _ => continue, // the wheel and side buttons
        };
        if !crate::blip::active() {
            continue;
        }
        let Some(pointer) = conn.query_pointer(root).ok().and_then(|c| c.reply().ok()) else { continue };
        let at = (pointer.root_x, pointer.root_y);
        if down {
            pressed[button as usize] = Some(Press { at, skip: skipped_on_press(&conn) });
            continue;
        }
        let Some(press) = pressed[button as usize].take() else { continue };
        let (dx, dy) = ((at.0 - press.at.0) as f64, (at.1 - press.at.1) as f64);
        let dragged = (dx * dx + dy * dy).sqrt() > DRAG_PX * scale(&conn, root, &atoms);
        if press.skip || dragged || skipped_on_release(&conn, &atoms, pointer.child) {
            continue;
        }
        let _ = blips.send((at.0 as f64, at.1 as f64, button));
    }
}

fn skipped_on_press(conn: &RustConnection) -> bool {
    let rules = crate::blip::rules();
    let Some(cursor) = conn.xfixes_get_cursor_image_and_name().ok().and_then(|c| c.reply().ok()) else { return false };
    let name = String::from_utf8_lossy(&cursor.name).to_lowercase();
    if rules.text_cursor && matches!(name.as_str(), "xterm" | "text" | "ibeam") {
        return true;
    }
    if rules.link_cursor && matches!(name.as_str(), "hand1" | "hand2" | "pointer" | "pointing_hand") {
        return true;
    }
    let hidden = cursor.cursor_image.iter().all(|pixel| pixel >> 24 == 0);
    rules.games && hidden
}

fn skipped_on_release(conn: &RustConnection, atoms: &Atoms, top: Window) -> bool {
    if top == 0 {
        return false;
    }
    let window = client_of(conn, atoms, top, 0).unwrap_or(top);
    if crate::blip::rules().taskbar {
        let types = conn.get_property(false, window, atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM, 0, 32).ok().and_then(|c| c.reply().ok());
        if types.is_some_and(|t| t.value32().is_some_and(|mut v| v.any(|a| a == atoms._NET_WM_WINDOW_TYPE_DOCK))) {
            return true;
        }
    }
    let pid = conn
        .get_property(false, window, atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().and_then(|mut v| v.next()));
    pid.is_some_and(|pid| crate::blip::linux::skipped_app(pid, ""))
}

/// Window managers wrap each app window in a frame; the app's own window is
/// the one with WM_STATE, a level or two down.
fn client_of(conn: &RustConnection, atoms: &Atoms, window: Window, depth: u32) -> Option<Window> {
    let state = conn.get_property(false, window, atoms.WM_STATE, AtomEnum::ANY, 0, 0).ok()?.reply().ok()?;
    if state.type_ != 0 {
        return Some(window);
    }
    if depth >= 3 {
        return None;
    }
    let tree = conn.query_tree(window).ok()?.reply().ok()?;
    tree.children.iter().rev().find_map(|&child| client_of(conn, atoms, child, depth + 1))
}
