// ============================================================
// macos/clicks.rs
// macOS: the event tap that hears every button, and the rules (the
// menu bar and Dock, skipped apps).
// ============================================================

use std::{
    cell::RefCell,
    ffi::{c_void, CStr},
    sync::atomic::Ordering,
    thread,
    time::Duration,
};

use dispatch2::DispatchQueue;

use super::ffi::*;
use crate::blip::Button;

/// Moving further than this (in points) while held is a drag.
const DRAG_PT: f64 = 40.0;

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

pub(super) fn watch_clicks() {
    let mask = [LEFT_DOWN, LEFT_UP, RIGHT_DOWN, RIGHT_UP, OTHER_DOWN, OTHER_UP].iter().fold(0u64, |m, t| m | (1 << t));
    loop {
        let tap = unsafe { CGEventTapCreate(SESSION_TAP, HEAD_INSERT, LISTEN_ONLY, mask, on_event, std::ptr::null_mut()) };
        if tap.is_null() {
            // Not allowed (yet): try again soon, so blips start once you allow it.
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        TAP.with(|t| *t.borrow_mut() = tap);
        super::TAP_OK.store(true, Ordering::Relaxed);
        unsafe {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            CFRunLoopRun();
        }
        // The run loop only ends if the tap goes away: start over.
        super::TAP_OK.store(false, Ordering::Relaxed);
    }
}

extern "C" fn on_event(_proxy: *mut c_void, kind: u32, event: *mut c_void, _info: *mut c_void) -> *mut c_void {
    // macOS switches a slow tap off; switch it straight back on.
    if kind == TAP_DISABLED_BY_TIMEOUT || kind == TAP_DISABLED_BY_USER {
        TAP.with(|t| unsafe { CGEventTapEnable(*t.borrow(), true) });
        return event;
    }
    if !crate::blip::active() {
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
            DispatchQueue::main().exec_async(move || super::overlay::play(at, button));
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
            let inside =
                at.x >= rect.origin.x && at.y >= rect.origin.y && at.x < rect.origin.x + rect.size.x && at.y < rect.origin.y + rect.size.y;
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
    let rules = crate::blip::rules();
    let skipped = crate::blip::skipped_apps();
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
