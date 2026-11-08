// ============================================================
// other.rs
// Systems Blip can't show blips on yet. The app still runs (window,
// settings, updates), and the window says why there are no blips.
// ============================================================

pub fn start() {}

pub fn unsupported_reason() -> Option<String> {
    Some("Blips don't work on this system yet. They're coming in a later version.".into())
}
