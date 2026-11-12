// ============================================================
// apps/linux/mod.rs
// Puts the list together from the app menu, one entry per program.
// ============================================================

mod icons;
mod sources;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::{exe_name, plain_name, AppInfo};

pub fn all() -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = Vec::new();
    // The same file in an earlier folder wins: yours over the system's.
    let mut seen = HashSet::new();
    for dir in data_dirs().iter().map(|d| d.join("applications")) {
        for file in sources::desktop_files(&dir) {
            let id = file.strip_prefix(&dir).unwrap_or(&file).to_string_lossy().replace('/', "-");
            if !seen.insert(id) {
                continue;
            }
            if let Some(app) = sources::read_entry(&file) {
                if !apps.iter().any(|a| a.exe == app.exe) {
                    apps.push(app);
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// A .desktop file or a program picked with Browse.
pub fn info_for(path: &str) -> AppInfo {
    if path.ends_with(".desktop") {
        if let Some(app) = sources::read_entry(Path::new(path)) {
            return app;
        }
    }
    AppInfo { name: plain_name(path), exe: exe_name(path), icon: None, path: Some(path.to_owned()) }
}

/// Where apps and icons live: yours first, then the system's, Flatpak and Snap.
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"))];
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let system = if system.is_empty() { "/usr/local/share:/usr/share".to_owned() } else { system };
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.push(home.join(".local/share/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop"));
    let mut unique = Vec::new();
    for dir in dirs {
        if !unique.contains(&dir) {
            unique.push(dir);
        }
    }
    unique
}
