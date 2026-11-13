// ============================================================
// effect/linux/wayland/mice.rs
// Reads the mouse buttons from /dev/input, picking up new mice as
// they're plugged in.
// ============================================================

use std::{
    collections::HashSet,
    io::ErrorKind,
    path::PathBuf,
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::Duration,
};

use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode, RelativeAxisCode};

use crate::effect::Button;

pub enum Input {
    Press(Button),
    Release(Button),
    /// How far it moved (the mouse's own units), to tell a click from a drag.
    Moved(i32, i32),
    /// From the desktop itself: the button and where the cursor was (px).
    PressAt(Button, f64, f64),
    ReleaseAt(Button, f64, f64),
}

fn event_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/input") else { return Vec::new() };
    entries.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event"))).collect()
}

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

/// Touchpads say where the finger is, and taps on them only become clicks in
/// libinput, so they go to touchpads.rs.
fn is_touchpad(device: &Device) -> bool {
    device.supported_keys().is_some_and(|keys| keys.contains(KeyCode::BTN_TOOL_FINGER))
        && device.supported_absolute_axes().is_some_and(|axes| axes.contains(AbsoluteAxisCode::ABS_X))
}

pub fn watch(events: Sender<Input>) {
    let open: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    let touchpads = super::touchpads::start(events.clone(), open.clone());
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
                if let Some(touchpads) = touchpads.as_ref().filter(|_| is_touchpad(&device)) {
                    drop(device);
                    let _ = touchpads.send(path);
                    continue;
                }
                eprintln!("Blip: reading clicks from {}", device.name().unwrap_or("a mouse"));
                let (open, events) = (open.clone(), events.clone());
                let _ = thread::Builder::new().name("mouse".into()).spawn(move || {
                    read(device, &events);
                    open.lock().unwrap().remove(&path);
                });
            }
            thread::sleep(Duration::from_secs(2));
        })
        .ok();
}

/// Touchpads, tablets and virtual machines' mice say where they are instead of
/// how far they moved. Their whole width counts as this many mouse units.
const ABS_SPAN: i64 = 4000;

fn read(mut device: Device, events: &Sender<Input>) {
    let range = |axis: AbsoluteAxisCode| {
        device.get_absinfo().ok()?.find(|(code, _)| *code == axis).map(|(_, info)| (info.maximum() - info.minimum()).max(1) as i64)
    };
    let ranges = [range(AbsoluteAxisCode::ABS_X), range(AbsoluteAxisCode::ABS_Y)];
    // The last position on each axis; forgotten when the finger lifts.
    let mut last: [Option<i32>; 2] = [None, None];
    loop {
        let Ok(batch) = device.fetch_events() else { return };
        for event in batch {
            let input = match event.destructure() {
                EventSummary::Key(_, KeyCode::BTN_TOUCH, 0) => {
                    last = [None, None];
                    continue;
                }
                EventSummary::AbsoluteAxis(_, axis @ (AbsoluteAxisCode::ABS_X | AbsoluteAxisCode::ABS_Y), value) => {
                    let i = usize::from(axis == AbsoluteAxisCode::ABS_Y);
                    let Some(span) = ranges[i] else { continue };
                    let Some(before) = last[i].replace(value) else { continue };
                    let moved = ((value - before) as i64 * ABS_SPAN / span) as i32;
                    if moved == 0 {
                        continue;
                    }
                    if i == 0 { Input::Moved(moved, 0) } else { Input::Moved(0, moved) }
                }
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
