// ============================================================
// macos.rs
// macOS: watches the mouse buttons everywhere (an event tap),
// checks the rules, and shows each blip in a small see-through
// window above everything that clicks pass through.
// ============================================================

// Two parts:
//   clicks  a thread with the event tap. macOS may want your OK first
//           (Privacy & Security → Input Monitoring); until then it keeps
//           trying every couple of seconds, so blips start once you allow it.
//   blips   the windows live on the main thread, as macOS requires. While
//           any blip plays, a small ticker asks the main thread for the
//           next frame every few milliseconds.

use std::{
    cell::RefCell,
    ffi::{c_void, CStr},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use dispatch2::DispatchQueue;
use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSColor, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CATransaction;
use tiny_skia::Pixmap;

use super::{draw, style::Style, Button, Features};

/// Moving further than this (in points) while a button is down is a drag, not a click.
const DRAG_PT: f64 = 40.0;
/// Above the menu bar, the Dock and full-screen apps.
const LEVEL: isize = 1000; // NSScreenSaverWindowLevel

/// The event tap is running: blips work.
static TAP_OK: AtomicBool = AtomicBool::new(false);

pub fn start() {
    thread::Builder::new().name("clicks".into()).spawn(watch_clicks).ok();
}

pub fn unsupported_reason() -> Option<String> {
    None
}

pub fn features() -> Features {
    // The window under the cursor gives the app, and the menu bar and Dock.
    Features { text: false, links: false, taskbar: true, games: false, apps: true }
}

pub fn needs_access() -> bool {
    // Give the tap a moment on a fresh start before asking.
    for _ in 0..10 {
        if TAP_OK.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !TAP_OK.load(Ordering::Relaxed)
}

pub fn access_prompt() -> super::AccessPrompt {
    super::AccessPrompt {
        text: "Blip needs permission to see your clicks. Turn on Blip in System Settings, under Privacy & Security → Input Monitoring.",
        button: "Open System Settings",
        waits: true,
    }
}

/// Asks macOS (it shows its own prompt the first time), then opens the right
/// page of System Settings. Blips start by themselves once it's switched on.
pub fn allow_access() -> Result<(), String> {
    unsafe {
        CGRequestListenEventAccess();
    }
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        .status()
        .map_err(|e| format!("Couldn't open System Settings: {e}"))?;
    Ok(())
}

// ------------------------------------------------------------
// The parts of macOS used here, by their C names
// ------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGPoint,
}

type TapCallback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;
type Release = extern "C" fn(*mut c_void, *const c_void, usize);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, events: u64, callback: TapCallback, info: *mut c_void) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGRequestListenEventAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
    fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut CGRect) -> bool;
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGDataProviderCreateWithData(info: *mut c_void, data: *const c_void, size: usize, release: Option<Release>) -> *mut c_void;
    fn CGDataProviderRelease(provider: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        interpolate: bool,
        intent: i32,
    ) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
    static kCGWindowOwnerPID: *const c_void;
    static kCGWindowOwnerName: *const c_void;
    static kCGWindowBounds: *const c_void;
    static kCGWindowLayer: *const c_void;
    static kCGWindowAlpha: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: *mut c_void, order: isize) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
    static kCFRunLoopCommonModes: *const c_void;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: *const c_void, kind: isize, value: *mut c_void) -> bool;
    fn CFStringGetCString(string: *const c_void, buffer: *mut i8, size: isize, encoding: u32) -> bool;
    fn CFRelease(object: *const c_void);
}

// Event types and other numbers from CoreGraphics.
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER: u32 = 0xFFFF_FFFF;
const BUTTON_NUMBER_FIELD: u32 = 3;
const SESSION_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const LISTEN_ONLY: u32 = 1;

// ------------------------------------------------------------
// Watching clicks (left, right and middle)
// ------------------------------------------------------------

struct Press {
    at: CGPoint,
    skip: bool,
}

thread_local! {
    static PRESS: RefCell<[Option<Press>; 3]> = const { RefCell::new([None, None, None]) };
    static TAP: RefCell<*mut c_void> = const { RefCell::new(std::ptr::null_mut()) };
}

fn watch_clicks() {
    let mask = [LEFT_DOWN, LEFT_UP, RIGHT_DOWN, RIGHT_UP, OTHER_DOWN, OTHER_UP].iter().fold(0u64, |m, t| m | (1 << t));
    loop {
        let tap = unsafe { CGEventTapCreate(SESSION_TAP, HEAD_INSERT, LISTEN_ONLY, mask, on_event, std::ptr::null_mut()) };
        if tap.is_null() {
            // Not allowed (yet): try again soon, so blips start once you allow it.
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        TAP.with(|t| *t.borrow_mut() = tap);
        TAP_OK.store(true, Ordering::Relaxed);
        unsafe {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            CFRunLoopRun();
        }
        // The run loop only ends if the tap goes away: start over.
        TAP_OK.store(false, Ordering::Relaxed);
    }
}

extern "C" fn on_event(_proxy: *mut c_void, kind: u32, event: *mut c_void, _info: *mut c_void) -> *mut c_void {
    // macOS switches a slow tap off; switch it straight back on.
    if kind == TAP_DISABLED_BY_TIMEOUT || kind == TAP_DISABLED_BY_USER {
        TAP.with(|t| unsafe { CGEventTapEnable(*t.borrow(), true) });
        return event;
    }
    if !super::active() {
        return event;
    }
    let (button, down) = match kind {
        LEFT_DOWN => (Button::Left, true),
        LEFT_UP => (Button::Left, false),
        RIGHT_DOWN => (Button::Right, true),
        RIGHT_UP => (Button::Right, false),
        OTHER_DOWN | OTHER_UP => {
            // Only the wheel button (2); side buttons are 3 and up.
            if unsafe { CGEventGetIntegerValueField(event, BUTTON_NUMBER_FIELD) } != 2 {
                return event;
            }
            (Button::Middle, kind == OTHER_DOWN)
        }
        _ => return event,
    };
    let at = unsafe { CGEventGetLocation(event) };
    if down {
        PRESS.with(|p| p.borrow_mut()[button as usize] = Some(Press { at, skip: false }));
    } else if let Some(press) = PRESS.with(|p| p.borrow_mut()[button as usize].take()) {
        let (dx, dy) = (at.x - press.at.x, at.y - press.at.y);
        if !press.skip && (dx * dx + dy * dy).sqrt() <= DRAG_PT && !skipped_at(at) {
            DispatchQueue::main().exec_async(move || play(at, button));
        }
    }
    event
}

// ------------------------------------------------------------
// The rules: the menu bar and Dock, and skipped apps
// ------------------------------------------------------------

/// System parts that count as "the taskbar" here.
const PANELS: [&str; 8] = [
    "Dock",
    "Window Server",
    "SystemUIServer",
    "Control Center",
    "ControlCenter",
    "Notification Center",
    "NotificationCenter",
    "Spotlight",
];

/// What's on screen under `at`: (owner's pid, owner's name, window layer).
fn window_at(at: CGPoint) -> Option<(i64, String, i64)> {
    unsafe {
        // On-screen windows, front to back, without the desktop.
        let list = CGWindowListCopyWindowInfo(1 | 16, 0);
        if list.is_null() {
            return None;
        }
        let own = std::process::id() as i64;
        let mut found = None;
        for i in 0..CFArrayGetCount(list) {
            let window = CFArrayGetValueAtIndex(list, i);
            let pid = number(CFDictionaryGetValue(window, kCGWindowOwnerPID)).unwrap_or(0);
            let alpha = float(CFDictionaryGetValue(window, kCGWindowAlpha)).unwrap_or(1.0);
            if pid == own || alpha <= 0.0 {
                continue;
            }
            let mut rect = CGRect::default();
            let bounds = CFDictionaryGetValue(window, kCGWindowBounds);
            if bounds.is_null() || !CGRectMakeWithDictionaryRepresentation(bounds, &mut rect) {
                continue;
            }
            let inside = at.x >= rect.origin.x
                && at.y >= rect.origin.y
                && at.x < rect.origin.x + rect.size.x
                && at.y < rect.origin.y + rect.size.y;
            if inside {
                let name = text(CFDictionaryGetValue(window, kCGWindowOwnerName)).unwrap_or_default();
                let layer = number(CFDictionaryGetValue(window, kCGWindowLayer)).unwrap_or(0);
                found = Some((pid, name, layer));
                break;
            }
        }
        CFRelease(list);
        found
    }
}

fn skipped_at(at: CGPoint) -> bool {
    let rules = super::rules();
    let skipped = super::skipped_apps();
    if !rules.taskbar && skipped.is_empty() {
        return false;
    }
    let Some((pid, name, layer)) = window_at(at) else { return false };
    // 24 and 25: the menu bar and its status items.
    if rules.taskbar && (PANELS.contains(&name.as_str()) || layer == 24 || layer == 25) {
        return true;
    }
    !skipped.is_empty() && program_of(pid).is_some_and(|p| skipped.contains(&p))
}

/// The program file's name for a pid, lowercase ("safari").
fn program_of(pid: i64) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    let len = unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
    if len <= 0 {
        return None;
    }
    let path = String::from_utf8_lossy(&buf[..len as usize]).into_owned();
    path.rsplit('/').next().map(|name| name.to_lowercase())
}

unsafe fn number(value: *const c_void) -> Option<i64> {
    let mut out: i64 = 0;
    // 4: kCFNumberSInt64Type
    (!value.is_null() && CFNumberGetValue(value, 4, &mut out as *mut i64 as *mut c_void)).then_some(out)
}

unsafe fn float(value: *const c_void) -> Option<f64> {
    let mut out: f64 = 0.0;
    // 6: kCFNumberFloat64Type
    (!value.is_null() && CFNumberGetValue(value, 6, &mut out as *mut f64 as *mut c_void)).then_some(out)
}

unsafe fn text(value: *const c_void) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut buf = [0i8; 256];
    // 0x0800_0100: kCFStringEncodingUTF8
    CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100)
        .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

// ------------------------------------------------------------
// Showing blips (main thread)
// ------------------------------------------------------------

struct Blip {
    window: Retained<NSWindow>,
    style: Style,
    started: Instant,
    pixmap: Pixmap,
    /// Real pixels per point on that screen (2 on Retina).
    scale: f32,
}

thread_local! {
    static BLIPS: RefCell<Vec<Blip>> = const { RefCell::new(Vec::new()) };
}

/// A ticker is asking the main thread for frames.
static TICKING: AtomicBool = AtomicBool::new(false);

/// Starts a blip centered on `at` (screen points, from the top left of the main screen).
fn play(at: CGPoint, button: Button) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(style) = super::style_for(button) else { return };
    // Windows are placed from the bottom left of the main screen.
    let main_height = unsafe { CGDisplayBounds(CGMainDisplayID()) }.size.y;
    let side = (style.largest().ceil() + 4.0).max(8.0) as f64;
    let frame = NSRect::new(NSPoint::new(at.x - side / 2.0, main_height - at.y - side / 2.0), NSSize::new(side, side));

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    // Clicks go straight through to what's underneath.
    window.setIgnoresMouseEvents(true);
    window.setLevel(LEVEL);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    if let Some(view) = window.contentView() {
        view.setWantsLayer(true);
    }
    let scale = window.backingScaleFactor() as f32;
    let px = (side as f32 * scale).ceil() as u32;
    let Some(pixmap) = Pixmap::new(px, px) else { return };
    window.orderFrontRegardless();

    BLIPS.with(|b| b.borrow_mut().push(Blip { window, style, started: Instant::now(), pixmap, scale }));
    draw_frame();
    start_ticking();
}

/// While blips play, asks the main thread for a frame about every 8 ms.
fn start_ticking() {
    if TICKING.swap(true, Ordering::AcqRel) {
        return;
    }
    thread::Builder::new()
        .name("blips".into())
        .spawn(|| {
            while TICKING.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(8));
                DispatchQueue::main().exec_async(draw_frame);
            }
        })
        .ok();
}

/// The next frame of every blip; finished ones close.
fn draw_frame() {
    BLIPS.with(|blips| {
        let mut blips = blips.borrow_mut();
        // No fade between frames: each picture replaces the last one at once.
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        blips.retain_mut(|blip| {
            let Some((size, opacity)) = blip.style.at(blip.started.elapsed()) else {
                blip.window.orderOut(None);
                blip.window.close();
                return false;
            };
            draw::frame(&mut blip.pixmap, &blip.style, size * blip.scale, opacity);
            if let Some(layer) = blip.window.contentView().and_then(|v| v.layer()) {
                show(&layer, &blip.pixmap);
            }
            true
        });
        CATransaction::commit();
        if blips.is_empty() {
            TICKING.store(false, Ordering::Release);
        }
    });
}

extern "C" fn free_pixels(info: *mut c_void, _data: *const c_void, _size: usize) {
    drop(unsafe { Box::from_raw(info as *mut Vec<u8>) });
}

/// Puts the picture on the window's layer.
fn show(layer: &objc2_quartz_core::CALayer, pixmap: &Pixmap) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    // Premultiplied RGBA, as tiny-skia gives it. The image owns its copy.
    let pixels = Box::new(pixmap.data().to_vec());
    let (ptr, len) = (pixels.as_ptr() as *const c_void, pixels.len());
    unsafe {
        let provider = CGDataProviderCreateWithData(Box::into_raw(pixels) as *mut c_void, ptr, len, Some(free_pixels));
        let space = CGColorSpaceCreateDeviceRGB();
        // 1: kCGImageAlphaPremultipliedLast (RGBA), default byte order.
        let image = CGImageCreate(w, h, 8, 32, w * 4, space, 1, provider, std::ptr::null(), false, 0);
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(space);
        if image.is_null() {
            return;
        }
        // A CGImage can be handed over as an object (toll-free bridged).
        layer.setContents(Some(&*(image as *const AnyObject)));
        CGImageRelease(image);
    }
}
