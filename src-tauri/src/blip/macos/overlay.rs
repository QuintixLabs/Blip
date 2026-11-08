// ============================================================
// macos/overlay.rs
// macOS: the see-through windows the blips play in. They live on the
// main thread, as macOS requires; a small ticker asks for each frame.
// ============================================================

use std::{
    cell::RefCell,
    ffi::c_void,
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

use super::ffi::*;
use crate::blip::{shared::draw, shared::style::Style, Button};

/// Above the menu bar, the Dock and full-screen apps.
const LEVEL: isize = 1000; // NSScreenSaverWindowLevel

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
pub(super) fn play(at: CGPoint, button: Button) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(style) = crate::blip::style_for(button) else { return };
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
