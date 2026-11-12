// ============================================================
// apps/macos/mod.rs
// Puts the list together from the Applications folders, one entry
// per program.
// ============================================================

mod icons;
mod sources;

use std::path::{Path, PathBuf};

use super::{exe_name, plain_name, AppInfo};

pub fn all() -> Vec<AppInfo> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let dirs = [
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        home.join("Applications"),
    ];
    let mut apps: Vec<AppInfo> = Vec::new();
    for dir in dirs {
        for bundle in sources::bundles(&dir) {
            if let Some(app) = sources::read_bundle(&bundle) {
                if !apps.iter().any(|a| a.exe == app.exe) {
                    apps.push(app);
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// An .app, or a program inside one, picked with Browse.
pub fn info_for(path: &str) -> AppInfo {
    let bundle = Path::new(path).ancestors().find(|p| p.extension().is_some_and(|e| e == "app"));
    bundle
        .and_then(sources::read_bundle)
        .unwrap_or_else(|| AppInfo { name: plain_name(path), exe: exe_name(path), icon: None, path: Some(path.to_owned()) })
}
