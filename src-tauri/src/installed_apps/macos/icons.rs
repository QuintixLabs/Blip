// ============================================================
// installed_apps/macos/icons.rs
// App icons as Finder shows them.
// ============================================================

use std::path::Path;

use objc2::{rc::autoreleasepool, AllocAnyThread};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

use crate::installed_apps::base64;

/// A small PNG data URL.
pub(super) fn icon(bundle: &Path) -> Option<String> {
    autoreleasepool(|_| {
        let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&bundle.to_string_lossy()));
        image.setSize(NSSize::new(48.0, 48.0));
        // The picture that best fits 48 points: 96 pixels on Retina.
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(96.0, 96.0));
        let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
        Some(format!("data:image/png;base64,{}", base64(&png.to_vec())))
    })
}
