// ============================================================
// installed_apps/mod.rs
// The apps listed in "Skip these apps", with their names and icons.
// ============================================================

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{all, info_for, is_app};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{all, info_for, is_app};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{all, info_for, is_app};

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    /// Lowercase file name ("code.exe"): what's saved and matched.
    pub exe: String,
    /// A PNG or SVG data URL.
    pub icon: Option<String>,
    /// For "show in folder". Never saved.
    pub path: Option<String>,
}

/// Lowercase file name: "C:\…\Code.exe" → "code.exe".
fn exe_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase()
}

/// "code.exe" → "Code", for programs that don't give a name.
fn plain_name(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
    let mut chars = stem.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A file you can run: it's marked runnable and starts like a program
/// (`magic`) or a script. Pictures and documents don't.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn is_runnable(path: &std::path::Path, magic: &[&[u8]]) -> bool {
    use std::{io::Read, os::unix::fs::PermissionsExt};
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return false;
    }
    let mut head = [0u8; 4];
    let read = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    let head = &head[..read];
    head.starts_with(b"#!") || magic.iter().any(|m| head.starts_with(m))
}

#[cfg_attr(not(any(windows, target_os = "linux", target_os = "macos")), allow(dead_code))]
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// Systems without blips have nothing to list.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn all() -> Vec<AppInfo> {
    Vec::new()
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn info_for(path: &str) -> AppInfo {
    AppInfo { name: plain_name(path), exe: exe_name(path), icon: None, path: Some(path.to_owned()) }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn is_app(path: &str) -> bool {
    std::path::Path::new(path).is_file()
}
