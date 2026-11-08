// ============================================================
// x11.rs
// X11: watches the mouse buttons everywhere, checks the rules, and
// shows each blip in a small see-through window that clicks pass
// through. Needs no special permission, only a compositor (most
// desktops have one) so the windows can be see-through.
// ============================================================

// Two threads, each with its own connection to X:
//   clicks  hears every button press and release, and checks the rules
//   blips   owns the blip windows and redraws them until they're done

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use tiny_skia::Pixmap;
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{self, ConnectionExt as _},
        xfixes::ConnectionExt as _,
        xinput::{self, ConnectionExt as _},
        xproto::{
            AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux,
            ImageFormat, StackMode, VisualClass, Window, WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
};

use crate::blip::{draw, style::Style, Button};

/// Moving further than this (at 100 % scaling) while a button is down is a
/// drag, not a click: no blip.
const DRAG_PX: f64 = 40.0;

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
            if run_overlay(rx).is_none() {
                eprintln!("Blip: couldn't draw over the screen (X11)");
            }
        })
        .ok();
    thread::Builder::new()
        .name("clicks".into())
        .spawn(move || {
            if watch_clicks(tx).is_none() {
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

// ------------------------------------------------------------
// Clicks and the rules
// ------------------------------------------------------------

struct Press {
    at: (i16, i16),
    /// A rule said no when the button went down.
    skip: bool,
}

fn watch_clicks(blips: Sender<(f64, f64, Button)>) -> Option<()> {
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

/// Text cursor, link cursor, or a hidden cursor (a game).
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

/// On a panel (the taskbar), or in a skipped app.
fn skipped_on_release(conn: &RustConnection, atoms: &Atoms, top: Window) -> bool {
    if top == 0 {
        return false;
    }
    let window = client_of(conn, atoms, top, 0).unwrap_or(top);
    if crate::blip::rules().taskbar {
        let types = conn
            .get_property(false, window, atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM, 0, 32)
            .ok()
            .and_then(|c| c.reply().ok());
        if types.is_some_and(|t| t.value32().is_some_and(|mut v| v.any(|a| a == atoms._NET_WM_WINDOW_TYPE_DOCK))) {
            return true;
        }
    }
    let pid = conn
        .get_property(false, window, atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().and_then(|mut v| v.next()));
    pid.is_some_and(|pid| super::skipped_app(pid, ""))
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

// ------------------------------------------------------------
// Drawing
// ------------------------------------------------------------

struct Blip {
    window: Window,
    gc: u32,
    style: Style,
    started: Instant,
    pixmap: Pixmap,
}

fn run_overlay(clicks: Receiver<(f64, f64, Button)>) -> Option<()> {
    let (conn, screen) = RustConnection::connect(None).ok()?;
    let screen = &conn.setup().roots[screen];
    let root = screen.root;
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    // A 32-bit visual: pixels with their own transparency.
    let visual = screen
        .allowed_depths
        .iter()
        .find(|d| d.depth == 32)?
        .visuals
        .iter()
        .find(|v| v.class == VisualClass::TRUE_COLOR)?
        .visual_id;
    let colormap = conn.generate_id().ok()?;
    conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual).ok()?;

    let mut blips: Vec<Blip> = Vec::new();
    loop {
        // Idle: wait for a click. Playing: take any new ones and draw the next frame.
        let next = if blips.is_empty() {
            match clicks.recv_timeout(Duration::from_millis(500)) {
                Ok(click) => Some(click),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Some(()),
            }
        } else {
            match clicks.try_recv() {
                Ok(click) => Some(click),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return Some(()),
            }
        };
        if let Some((x, y, button)) = next {
            let scale = scale(&conn, root, &atoms);
            if let Some(blip) = new_blip(&conn, root, visual, colormap, x, y, button, scale) {
                blips.push(blip);
            }
        }

        blips.retain_mut(|blip| {
            let Some((size, opacity)) = blip.style.at(blip.started.elapsed()) else {
                let _ = conn.destroy_window(blip.window);
                let _ = conn.free_gc(blip.gc);
                return false;
            };
            let scale = blip.pixmap.width() as f32 / (blip.style.largest().ceil() + 4.0).max(8.0);
            draw::frame(&mut blip.pixmap, &blip.style, size * scale, opacity);
            let data: Vec<u8> = blip.pixmap.data().chunks_exact(4).flat_map(|s| [s[2], s[1], s[0], s[3]]).collect();
            let side = blip.pixmap.width() as u16;
            let _ = conn.put_image(ImageFormat::Z_PIXMAP, blip.window, blip.gc, side, side, 0, 0, 0, 32, &data);
            true
        });
        conn.flush().ok()?;
        // Nothing is asked of X here, but errors would pile up unread.
        while let Ok(Some(_)) = conn.poll_for_event() {}
        if !blips.is_empty() {
            thread::sleep(Duration::from_millis(8));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn new_blip(conn: &RustConnection, root: Window, visual: u32, colormap: u32, x: f64, y: f64, button: Button, scale: f64) -> Option<Blip> {
    let style = crate::blip::style_for(button)?;
    let side = ((style.largest().ceil() + 4.0).max(8.0) as f64 * scale).ceil() as u16;
    let half = (side / 2) as i16;
    let window = conn.generate_id().ok()?;
    let aux = CreateWindowAux::new().background_pixel(0).border_pixel(0).colormap(colormap).override_redirect(1);
    conn.create_window(32, window, root, x as i16 - half, y as i16 - half, side, side, 0, WindowClass::INPUT_OUTPUT, visual, &aux)
        .ok()?;
    // No input area: clicks go straight through to what's underneath.
    conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &[]).ok()?;
    conn.map_window(window).ok()?;
    conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE)).ok()?;
    let gc = conn.generate_id().ok()?;
    conn.create_gc(gc, window, &CreateGCAux::new()).ok()?;
    let pixmap = Pixmap::new(side as u32, side as u32)?;
    Some(Blip { window, gc, style, started: Instant::now(), pixmap })
}
