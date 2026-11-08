// ============================================================
// linux/wayland/mice.rs
// Wayland: reads the mouse buttons straight from the mice and
// touchpads (/dev/input), since Wayland doesn't tell apps about
// clicks outside their own windows. Mice plugged in later are
// picked up within a couple of seconds.
// ============================================================

use std::{
    collections::HashSet,
    io::ErrorKind,
    path::PathBuf,
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::Duration,
};

use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode};

use crate::blip::Button;

/// What happened on a mouse.
pub enum Input {
    Press(Button),
    Release(Button),
    /// How far it moved (the mouse's own units), to tell a click from a drag.
    Moved(i32, i32),
}

fn event_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/input") else { return Vec::new() };
    entries.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event"))).collect()
}

/// Has a left button: a mouse, touchpad or trackball.
fn is_pointer(device: &Device) -> bool {
    device.supported_keys().is_some_and(|keys| keys.contains(KeyCode::BTN_LEFT))
}

/// (mice Blip can read, input devices it isn't allowed to open).
pub fn count() -> (usize, usize) {
    let (mut readable, mut blocked) = (0, 0);
    for path in event_files() {
        match Device::open(&path) {
            Ok(device) if is_pointer(&device) => readable += 1,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => blocked += 1,
            _ => {}
        }
    }
    (readable, blocked)
}

/// Starts reading every mouse, now and later. Each one gets its own thread.
pub fn watch(events: Sender<Input>) {
    let open: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    thread::Builder::new()
        .name("mice".into())
        .spawn(move || loop {
            for path in event_files() {
                if open.lock().unwrap().contains(&path) {
                    continue;
                }
                let Ok(device) = Device::open(&path) else { continue };
                if !is_pointer(&device) {
                    continue;
                }
                open.lock().unwrap().insert(path.clone());
                eprintln!("Blip: reading clicks from {}", device.name().unwrap_or("a mouse"));
                let (open, events) = (open.clone(), events.clone());
                let _ = thread::Builder::new().name("mouse".into()).spawn(move || {
                    read(device, &events);
                    // Unplugged (or gone for another reason): may be opened again later.
                    open.lock().unwrap().remove(&path);
                });
            }
            thread::sleep(Duration::from_secs(2));
        })
        .ok();
}

fn read(mut device: Device, events: &Sender<Input>) {
    loop {
        let Ok(batch) = device.fetch_events() else { return };
        for event in batch {
            let input = match event.destructure() {
                EventSummary::Key(_, code, value) => {
                    let button = match code {
                        KeyCode::BTN_LEFT => Button::Left,
                        KeyCode::BTN_RIGHT => Button::Right,
                        KeyCode::BTN_MIDDLE => Button::Middle,
                        _ => continue,
                    };
                    match value {
                        1 => Input::Press(button),
                        0 => Input::Release(button),
                        _ => continue, // 2 = held down, repeating
                    }
                }
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, value) => Input::Moved(value, 0),
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_Y, value) => Input::Moved(0, value),
                _ => continue,
            };
            if events.send(input).is_err() {
                return;
            }
        }
    }
}
