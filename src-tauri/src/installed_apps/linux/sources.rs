// ============================================================
// installed_apps/linux/sources.rs
// Where apps are found: the app menu's .desktop files.
// ============================================================

use std::path::{Path, PathBuf};

use super::icons;
use crate::installed_apps::{exe_name, AppInfo};

pub(super) fn desktop_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 2 {
                walk(&path, depth + 1, out);
            } else if path.extension().is_some_and(|e| e == "desktop") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, 0, &mut files);
    files
}

/// None for hidden entries and ones that aren't apps.
pub(super) fn read_entry(file: &Path) -> Option<AppInfo> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut in_entry = false;
    let (mut name, mut exec, mut icon, mut flatpak) = (None, None, None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        match key.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Exec" => exec = Some(value.trim().to_owned()),
            "Icon" => icon = Some(value.trim().to_owned()),
            "X-Flatpak" => flatpak = Some(value.trim().to_lowercase()),
            "Type" if value.trim() != "Application" => return None,
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }
    let (name, exec) = (name?, exec?);
    // Steam games start through Steam, so their own program isn't known.
    if exec.contains("steam://") {
        return None;
    }
    let (exe, path) = match flatpak {
        // KDE knows Flatpak apps by their id, and their program has no folder to show.
        Some(id) => (id, None),
        None => {
            let program = first_word(&exec)?;
            (exe_name(&program), find_program(&program))
        }
    };
    Some(AppInfo { name, exe, icon: icon.and_then(|i| icons::data(&i)), path })
}

/// "env FOO=1 /usr/bin/firefox %u" → "/usr/bin/firefox".
fn first_word(exec: &str) -> Option<String> {
    let words: Vec<&str> = exec.split_whitespace().map(|w| w.trim_matches('"')).collect();
    let mut words = words.into_iter().skip_while(|w| *w == "env" || w.contains('=') && !w.starts_with('/'));
    words.next().map(str::to_owned)
}

/// A full path as it is, otherwise looked up in PATH.
fn find_program(program: &str) -> Option<String> {
    if program.starts_with('/') {
        return Path::new(program).is_file().then(|| program.to_owned());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}
