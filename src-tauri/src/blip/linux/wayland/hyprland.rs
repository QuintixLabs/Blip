// ============================================================
// linux/wayland/hyprland.rs
// Where the cursor is, from Hyprland's control socket.
// ============================================================

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use super::Pointer;

pub(super) fn socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    [PathBuf::from(&runtime).join("hypr"), PathBuf::from("/tmp/hypr")]
        .into_iter()
        .map(|dir| dir.join(&signature).join(".socket.sock"))
        .find(|p| p.exists())
}

/// "cursorpos" answers "1234, 567".
pub(super) fn cursor(socket: &PathBuf) -> Option<Pointer> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(300))).ok()?;
    stream.write_all(b"cursorpos").ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    let (x, y) = reply.trim().split_once(',')?;
    Some(Pointer { x: x.trim().parse().ok()?, y: y.trim().parse().ok()?, pid: 0, app: String::new(), panel: false })
}
