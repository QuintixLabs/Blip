// ============================================================
// effect/macos/ffi.rs
// The macOS functions Blip uses, by their C names.
// ============================================================

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct CGPoint {
    pub(super) x: f64,
    pub(super) y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct CGRect {
    pub(super) origin: CGPoint,
    pub(super) size: CGPoint,
}

pub(super) type TapCallback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;
pub(super) type Release = extern "C" fn(*mut c_void, *const c_void, usize);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub(super) fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events: u64,
        callback: TapCallback,
        info: *mut c_void,
    ) -> *mut c_void;
    pub(super) fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    pub(super) fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    pub(super) fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    pub(super) fn CGRequestListenEventAccess() -> bool;
    pub(super) fn CGPreflightListenEventAccess() -> bool;
    pub(super) fn CGMainDisplayID() -> u32;
    pub(super) fn CGDisplayBounds(display: u32) -> CGRect;
    pub(super) fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
    pub(super) fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut CGRect) -> bool;
    pub(super) fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    pub(super) fn CGColorSpaceRelease(space: *mut c_void);
    pub(super) fn CGDataProviderCreateWithData(
        info: *mut c_void,
        data: *const c_void,
        size: usize,
        release: Option<Release>,
    ) -> *mut c_void;
    pub(super) fn CGDataProviderRelease(provider: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    pub(super) fn CGImageCreate(
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
    pub(super) fn CGImageRelease(image: *mut c_void);
    pub(super) static kCGWindowOwnerPID: *const c_void;
    pub(super) static kCGWindowOwnerName: *const c_void;
    pub(super) static kCGWindowBounds: *const c_void;
    pub(super) static kCGWindowLayer: *const c_void;
    pub(super) static kCGWindowAlpha: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub(super) fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: *mut c_void, order: isize) -> *mut c_void;
    pub(super) fn CFRunLoopGetCurrent() -> *mut c_void;
    pub(super) fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    pub(super) fn CFRunLoopRun();
    pub(super) static kCFRunLoopCommonModes: *const c_void;
    pub(super) fn CFArrayGetCount(array: *const c_void) -> isize;
    pub(super) fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    pub(super) fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    pub(super) fn CFNumberGetValue(number: *const c_void, kind: isize, value: *mut c_void) -> bool;
    pub(super) fn CFStringGetCString(string: *const c_void, buffer: *mut i8, size: isize, encoding: u32) -> bool;
    pub(super) fn CFRelease(object: *const c_void);
}

// Event types and other numbers from CoreGraphics.
pub(super) const LEFT_DOWN: u32 = 1;
pub(super) const LEFT_UP: u32 = 2;
pub(super) const RIGHT_DOWN: u32 = 3;
pub(super) const RIGHT_UP: u32 = 4;
pub(super) const OTHER_DOWN: u32 = 25;
pub(super) const OTHER_UP: u32 = 26;
pub(super) const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub(super) const TAP_DISABLED_BY_USER: u32 = 0xFFFF_FFFF;
pub(super) const BUTTON_NUMBER_FIELD: u32 = 3;
pub(super) const SESSION_TAP: u32 = 1;
pub(super) const HEAD_INSERT: u32 = 0;
pub(super) const LISTEN_ONLY: u32 = 1;
