// ============================================================
// wayland.rs
// Wayland (KDE Plasma, Hyprland): turns the mouse buttons from
// mice.rs into blips. On release it asks the desktop where the
// cursor is (cursor.rs), checks the rules, and shows the blip as a
// small surface above everything that clicks pass through.
// ============================================================

// Two threads:
//   clicks  reads the mice, tells clicks from drags, asks where the cursor is
//   blips   talks to the compositor: one small overlay surface per blip,
//           redrawn every few milliseconds until it's done

use std::{
    os::fd::AsRawFd,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use tiny_skia::Pixmap;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
    Connection, QueueHandle,
};

use super::{
    cursor::{Finder, Source},
    mice::{self, Input},
};
use crate::blip::{draw, style::Style, Button};

/// Moving further than this (in the mouse's own units) while a button is down
/// is a drag, not a click: no blip.
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
            if run_overlay(blips_rx).is_none() {
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
    // How far each button has moved since it went down; None while it's up.
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
                if dx * dx + dy * dy > DRAG * DRAG || !crate::blip::active() || crate::blip::style_for(button).is_none() {
                    continue;
                }
                let Some(pointer) = finder.find() else { continue };
                if debug() {
                    eprintln!("Blip: click at {}, {} (pid {}, app {:?}, panel {})", pointer.x, pointer.y, pointer.pid, pointer.app, pointer.panel);
                }
                if crate::blip::rules().taskbar && pointer.panel {
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
// Drawing
// ------------------------------------------------------------

struct Overlay {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    blips: Vec<Blip>,
}

struct Blip {
    layer: LayerSurface,
    style: Style,
    started: Instant,
    /// The screen's scaling (whole steps; the compositor smooths the rest).
    scale: i32,
    /// The compositor has said where it goes; only then can it be drawn.
    configured: bool,
    pixmap: Pixmap,
}

fn run_overlay(clicks: Receiver<(f64, f64, Button)>) -> Option<()> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Overlay>(&conn).ok()?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).ok()?;
    let layer_shell = LayerShell::bind(&globals, &qh).ok()?;
    let shm = Shm::bind(&globals, &qh).ok()?;
    let pool = SlotPool::new(256 * 256 * 4, &shm).ok()?;
    let mut overlay = Overlay {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        pool,
        blips: Vec::new(),
    };
    // Learn the screens before the first click.
    queue.roundtrip(&mut overlay).ok()?;

    loop {
        loop {
            match clicks.try_recv() {
                Ok((x, y, button)) => overlay.play(x, y, button, &qh),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Some(()),
            }
        }
        overlay.draw();
        queue.flush().ok()?;

        // Wait for the compositor, or until the next frame (or next look for clicks).
        let wait_ms = if overlay.blips.is_empty() { 25 } else { 8 };
        if let Some(guard) = queue.prepare_read() {
            let mut fd = libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut fd, 1, wait_ms) } > 0 {
                let _ = guard.read();
            }
        }
        queue.dispatch_pending(&mut overlay).ok()?;
    }
}

impl Overlay {
    /// Starts a blip centered on (x, y), the desktop's screen position.
    fn play(&mut self, x: f64, y: f64, button: Button, qh: &QueueHandle<Self>) {
        let Some(style) = crate::blip::style_for(button) else { return };
        let screen = self.outputs.outputs().find_map(|output| {
            let info = self.outputs.info(&output)?;
            let (ox, oy) = info.logical_position?;
            let (w, h) = info.logical_size?;
            let inside = x >= ox as f64 && x < (ox + w) as f64 && y >= oy as f64 && y < (oy + h) as f64;
            inside.then_some((output, ox, oy, info.scale_factor.max(1)))
        });
        let Some((output, ox, oy, scale)) = screen else {
            eprintln!("Blip: no screen at {x}, {y}");
            return;
        };

        // Room for the biggest the blip gets, plus a pixel of soft edge all round.
        let side = (style.largest().ceil() as u32 + 4).max(8);
        let half = (side / 2) as i32;
        let Some(pixmap) = Pixmap::new(side * scale as u32, side * scale as u32) else { return };

        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(qh, surface, Layer::Overlay, Some("blip"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin((y - oy as f64).round() as i32 - half, 0, 0, (x - ox as f64).round() as i32 - half);
        layer.set_size(side, side);
        // Placed from the screen's edge, ignoring panels.
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        // No input area: clicks go straight through to what's underneath.
        if let Ok(region) = Region::new(&self.compositor) {
            layer.wl_surface().set_input_region(Some(region.wl_region()));
        }
        layer.wl_surface().set_buffer_scale(scale);
        layer.commit();

        self.blips.push(Blip { layer, style, started: Instant::now(), scale, configured: false, pixmap });
    }

    /// The next frame of every blip; finished ones are removed.
    fn draw(&mut self) {
        let pool = &mut self.pool;
        self.blips.retain_mut(|blip| {
            if !blip.configured {
                return true;
            }
            let Some((size, opacity)) = blip.style.at(blip.started.elapsed()) else { return false };
            draw::frame(&mut blip.pixmap, &blip.style, size * blip.scale as f32, opacity);
            let (w, h) = (blip.pixmap.width() as i32, blip.pixmap.height() as i32);
            let Ok((buffer, canvas)) = pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888) else { return true };
            // tiny-skia gives premultiplied RGBA; ARGB8888 is premultiplied BGRA in memory.
            for (d, s) in canvas.chunks_exact_mut(4).zip(blip.pixmap.data().chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
            let surface = blip.layer.wl_surface();
            if buffer.attach_to(surface).is_ok() {
                surface.damage_buffer(0, 0, w, h);
                blip.layer.commit();
            }
            true
        });
    }
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Overlay {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.blips.retain(|b| b.layer.wl_surface() != layer.wl_surface());
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        if let Some(blip) = self.blips.iter_mut().find(|b| b.layer.wl_surface() == layer.wl_surface()) {
            blip.configured = true;
        }
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_compositor!(Overlay);
delegate_output!(Overlay);
delegate_shm!(Overlay);
delegate_layer!(Overlay);
delegate_registry!(Overlay);
