// ============================================================
// linux/x11/overlay.rs
// X11: the see-through windows the blips play in.
// ============================================================

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use tiny_skia::Pixmap;
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{self, ConnectionExt as _},
        xproto::{
            ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, ImageFormat, StackMode,
            VisualClass, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
};

use crate::blip::{shared::draw, shared::style::Style, Button};

use super::{scale, Atoms};

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

pub(super) fn run_overlay(clicks: Receiver<(f64, f64, Button)>) -> Option<()> {
    let (conn, screen) = RustConnection::connect(None).ok()?;
    let screen = &conn.setup().roots[screen];
    let root = screen.root;
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    // A 32-bit visual: pixels with their own transparency.
    let visual =
        screen.allowed_depths.iter().find(|d| d.depth == 32)?.visuals.iter().find(|v| v.class == VisualClass::TRUE_COLOR)?.visual_id;
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
    conn.create_window(32, window, root, x as i16 - half, y as i16 - half, side, side, 0, WindowClass::INPUT_OUTPUT, visual, &aux).ok()?;
    // No input area: clicks go straight through to what's underneath.
    conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &[]).ok()?;
    conn.map_window(window).ok()?;
    conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE)).ok()?;
    let gc = conn.generate_id().ok()?;
    conn.create_gc(gc, window, &CreateGCAux::new()).ok()?;
    let pixmap = Pixmap::new(side as u32, side as u32)?;
    Some(Blip { window, gc, style, started: Instant::now(), pixmap })
}
