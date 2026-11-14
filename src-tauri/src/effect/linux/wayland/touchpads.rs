// ============================================================
// effect/linux/wayland/touchpads.rs
// Touchpads: taps and clicks, through libinput.
// ============================================================

use std::{
    collections::HashSet,
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    thread,
};

use super::mice::Input;
use crate::effect::Button;

// ------------------------------------------------------------
// libinput's functions, looked up when Blip starts
// ------------------------------------------------------------

type Ptr = *mut c_void;

#[repr(C)]
struct Interface {
    open_restricted: extern "C" fn(*const c_char, c_int, Ptr) -> c_int,
    close_restricted: extern "C" fn(c_int, Ptr),
}

struct Lib {
    path_create_context: unsafe extern "C" fn(*const Interface, Ptr) -> Ptr,
    path_add_device: unsafe extern "C" fn(Ptr, *const c_char) -> Ptr,
    get_fd: unsafe extern "C" fn(Ptr) -> c_int,
    dispatch: unsafe extern "C" fn(Ptr) -> c_int,
    get_event: unsafe extern "C" fn(Ptr) -> Ptr,
    event_get_type: unsafe extern "C" fn(Ptr) -> c_int,
    event_get_device: unsafe extern "C" fn(Ptr) -> Ptr,
    event_get_pointer_event: unsafe extern "C" fn(Ptr) -> Ptr,
    event_destroy: unsafe extern "C" fn(Ptr),
    pointer_get_button: unsafe extern "C" fn(Ptr) -> u32,
    pointer_get_button_state: unsafe extern "C" fn(Ptr) -> c_int,
    pointer_get_dx_unaccelerated: unsafe extern "C" fn(Ptr) -> f64,
    pointer_get_dy_unaccelerated: unsafe extern "C" fn(Ptr) -> f64,
    device_get_sysname: unsafe extern "C" fn(Ptr) -> *const c_char,
    tap_get_finger_count: unsafe extern "C" fn(Ptr) -> c_int,
    tap_set_enabled: unsafe extern "C" fn(Ptr, c_int) -> c_int,
    tap_set_button_map: unsafe extern "C" fn(Ptr, c_int) -> c_int,
    tap_set_drag_enabled: unsafe extern "C" fn(Ptr, c_int) -> c_int,
    tap_set_drag_lock_enabled: unsafe extern "C" fn(Ptr, c_int) -> c_int,
    click_set_method: unsafe extern "C" fn(Ptr, c_int) -> c_int,
    left_handed_set: unsafe extern "C" fn(Ptr, c_int) -> c_int,
}

fn load() -> Option<Lib> {
    let lib = ["libinput.so.10", "libinput.so"].iter().find_map(|name| {
        let name = CString::new(*name).ok()?;
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        (!handle.is_null()).then_some(handle)
    })?;
    macro_rules! find {
        ($name:literal) => {{
            let symbol = unsafe { libc::dlsym(lib, concat!($name, "\0").as_ptr() as *const c_char) };
            if symbol.is_null() {
                return None;
            }
            unsafe { std::mem::transmute::<*mut c_void, _>(symbol) }
        }};
    }
    Some(Lib {
        path_create_context: find!("libinput_path_create_context"),
        path_add_device: find!("libinput_path_add_device"),
        get_fd: find!("libinput_get_fd"),
        dispatch: find!("libinput_dispatch"),
        get_event: find!("libinput_get_event"),
        event_get_type: find!("libinput_event_get_type"),
        event_get_device: find!("libinput_event_get_device"),
        event_get_pointer_event: find!("libinput_event_get_pointer_event"),
        event_destroy: find!("libinput_event_destroy"),
        pointer_get_button: find!("libinput_event_pointer_get_button"),
        pointer_get_button_state: find!("libinput_event_pointer_get_button_state"),
        pointer_get_dx_unaccelerated: find!("libinput_event_pointer_get_dx_unaccelerated"),
        pointer_get_dy_unaccelerated: find!("libinput_event_pointer_get_dy_unaccelerated"),
        device_get_sysname: find!("libinput_device_get_sysname"),
        tap_get_finger_count: find!("libinput_device_config_tap_get_finger_count"),
        tap_set_enabled: find!("libinput_device_config_tap_set_enabled"),
        tap_set_button_map: find!("libinput_device_config_tap_set_button_map"),
        tap_set_drag_enabled: find!("libinput_device_config_tap_set_drag_enabled"),
        tap_set_drag_lock_enabled: find!("libinput_device_config_tap_set_drag_lock_enabled"),
        click_set_method: find!("libinput_device_config_click_set_method"),
        left_handed_set: find!("libinput_device_config_left_handed_set"),
    })
}

// From libinput.h.
const EVENT_DEVICE_REMOVED: c_int = 2;
const EVENT_POINTER_MOTION: c_int = 400;
const EVENT_POINTER_BUTTON: c_int = 402;
const BUTTON_PRESSED: c_int = 1;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const CLICK_BUTTON_AREAS: c_int = 1 << 0;
const CLICK_CLICKFINGER: c_int = 1 << 1;

extern "C" fn open_restricted(path: *const c_char, flags: c_int, _: Ptr) -> c_int {
    let fd = unsafe { libc::open(path, flags) };
    if fd < 0 {
        -std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
    } else {
        fd
    }
}

extern "C" fn close_restricted(fd: c_int, _: Ptr) {
    unsafe { libc::close(fd) };
}

static INTERFACE: Interface = Interface { open_restricted, close_restricted };

// ------------------------------------------------------------
// Your touchpad settings, from KWin
// ------------------------------------------------------------

/// What KWin says about a touchpad, by its name in /dev/input ("event5").
/// Unknown settings stay at libinput's own defaults.
fn apply_settings(lib: &Lib, device: Ptr, sysname: &str) {
    let get = |name: &str| -> Option<bool> {
        let bus = zbus::blocking::Connection::session().ok()?;
        let reply = bus
            .call_method(
                Some("org.kde.KWin"),
                format!("/org/kde/KWin/InputDevice/{sysname}").as_str(),
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.kde.KWin.InputDevice", name),
            )
            .ok()?;
        let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
        bool::try_from(value).ok()
    };
    unsafe {
        if let Some(on) = get("tapToClick") {
            (lib.tap_set_enabled)(device, on as c_int);
        }
        // Two fingers: right (lrm, 0) or middle (lmr, 1).
        if let Some(lmr) = get("lmrTapButtonMap") {
            (lib.tap_set_button_map)(device, lmr as c_int);
        }
        if let Some(on) = get("tapAndDrag") {
            (lib.tap_set_drag_enabled)(device, on as c_int);
        }
        if let Some(on) = get("tapDragLock") {
            (lib.tap_set_drag_lock_enabled)(device, on as c_int);
        }
        if let Some(clickfinger) = get("clickMethodClickfinger") {
            (lib.click_set_method)(device, if clickfinger { CLICK_CLICKFINGER } else { CLICK_BUTTON_AREAS });
        }
        if let Some(on) = get("leftHanded") {
            (lib.left_handed_set)(device, on as c_int);
        }
    }
}

// ------------------------------------------------------------
// Reading them
// ------------------------------------------------------------

/// Starts the touchpad reader. Send it each touchpad's path; a path leaves
/// `open` again when that touchpad goes away. None without libinput.
pub(super) fn start(events: Sender<Input>, open: Arc<Mutex<HashSet<PathBuf>>>) -> Option<Sender<PathBuf>> {
    let lib = load()?;
    let (add_tx, add_rx) = mpsc::channel();
    thread::Builder::new()
        .name("touchpads".into())
        .spawn(move || {
            let context = unsafe { (lib.path_create_context)(&INTERFACE, std::ptr::null_mut()) };
            if context.is_null() {
                eprintln!("Blip: libinput didn't start; touchpad taps won't blip");
                return;
            }
            run(&lib, context, &add_rx, &events, &open);
        })
        .ok()?;
    Some(add_tx)
}

fn run(lib: &Lib, context: Ptr, add: &Receiver<PathBuf>, events: &Sender<Input>, open: &Mutex<HashSet<PathBuf>>) {
    let fd = unsafe { (lib.get_fd)(context) };
    // Motion comes in fractions; the rest carries over.
    let (mut rest_x, mut rest_y) = (0.0f64, 0.0f64);
    loop {
        loop {
            match add.try_recv() {
                Ok(path) => {
                    let Ok(c_path) = CString::new(path.to_string_lossy().as_bytes()) else { continue };
                    let device = unsafe { (lib.path_add_device)(context, c_path.as_ptr()) };
                    if device.is_null() {
                        open.lock().unwrap().remove(&path);
                        continue;
                    }
                    let sysname = unsafe { CStr::from_ptr((lib.device_get_sysname)(device)) }.to_string_lossy().into_owned();
                    if unsafe { (lib.tap_get_finger_count)(device) } > 0 {
                        apply_settings(lib, device, &sysname);
                    }
                    eprintln!("Blip: reading clicks and taps from touchpad {sysname}");
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        // Wait for something to read, checking for new touchpads twice a second.
        let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut poll, 1, 500) } < 0 {
            continue;
        }
        unsafe { (lib.dispatch)(context) };
        loop {
            let event = unsafe { (lib.get_event)(context) };
            if event.is_null() {
                break;
            }
            let kind = unsafe { (lib.event_get_type)(event) };
            let input = match kind {
                EVENT_POINTER_BUTTON => {
                    let pointer = unsafe { (lib.event_get_pointer_event)(event) };
                    let button = match unsafe { (lib.pointer_get_button)(pointer) } {
                        BTN_LEFT => Some(Button::Left),
                        BTN_RIGHT => Some(Button::Right),
                        BTN_MIDDLE => Some(Button::Middle),
                        _ => None,
                    };
                    let pressed = unsafe { (lib.pointer_get_button_state)(pointer) } == BUTTON_PRESSED;
                    button.map(|b| if pressed { Input::Press(b) } else { Input::Release(b) })
                }
                EVENT_POINTER_MOTION => {
                    let pointer = unsafe { (lib.event_get_pointer_event)(event) };
                    rest_x += unsafe { (lib.pointer_get_dx_unaccelerated)(pointer) };
                    rest_y += unsafe { (lib.pointer_get_dy_unaccelerated)(pointer) };
                    let (dx, dy) = (rest_x.trunc(), rest_y.trunc());
                    rest_x -= dx;
                    rest_y -= dy;
                    (dx != 0.0 || dy != 0.0).then(|| Input::Moved(dx as i32, dy as i32))
                }
                EVENT_DEVICE_REMOVED => {
                    let device = unsafe { (lib.event_get_device)(event) };
                    let sysname = unsafe { CStr::from_ptr((lib.device_get_sysname)(device)) }.to_string_lossy().into_owned();
                    open.lock().unwrap().remove(&PathBuf::from("/dev/input").join(sysname));
                    None
                }
                _ => None,
            };
            unsafe { (lib.event_destroy)(event) };
            if let Some(input) = input {
                if events.send(input).is_err() {
                    return;
                }
            }
        }
    }
}
