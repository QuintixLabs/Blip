// ============================================================
// installed_apps/windows/mod.rs
// Puts the list together, one entry per program.
// ============================================================

mod icons;
mod sources;

use std::os::windows::ffi::OsStrExt;

use super::{exe_name, AppInfo};

/// Windows parts that aren't apps anyone would pick.
const NOT_APPS: [&str; 9] = [
    "applicationframehost.exe",
    "textinputhost.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "shellexperiencehost.exe",
    "shellhost.exe",
    "systemsettingsbroker.exe",
    "lockapp.exe",
    "blip.exe",
];

/// Words in a shortcut's name that mean it's a helper, not the app.
const HELPER_WORDS: [&str; 10] =
    ["uninstall", "update", "bug report", "error report", "crash report", "reset", "readme", "documentation", "help", "website"];

/// An app found somewhere, before its icon is read.
struct Shortcut {
    name: String,
    exe: String,
    /// The file its icon is read from.
    icon: String,
    /// The program itself.
    path: String,
    /// Starts the program with no arguments, so it names the app itself.
    plain: bool,
}

fn not_an_app(name: &str, exe: &str) -> bool {
    let name = name.to_lowercase();
    NOT_APPS.contains(&exe)
        || exe.starts_with("unins")
        || ["uninstall", "updater", "crashreport", "errorreport", "bugreport"].iter().any(|w| exe.contains(w))
        || exe.ends_with("_proxy.exe")
        || matches!(exe, "msiexec.exe" | "rundll32.exe" | "hh.exe")
        || HELPER_WORDS.iter().any(|w| name.contains(w))
}

pub fn all() -> Vec<AppInfo> {
    // The first entry for each program wins, so plain shortcuts go first.
    let mut found = sources::start_menu();
    found.sort_by_key(|s| (!s.plain, s.name.len()));
    found.extend(sources::steam_games());
    found.extend(sources::epic_games());
    found.extend(sources::open_windows().into_iter().map(|path| Shortcut {
        name: icons::name(&path),
        exe: exe_name(&path),
        icon: path.clone(),
        path,
        plain: true,
    }));

    let mut entries: Vec<Shortcut> = Vec::new();
    for s in found {
        if not_an_app(&s.name, &s.exe) || entries.iter().any(|e| e.exe == s.exe) {
            continue;
        }
        entries.push(s);
    }

    // Reading icons is the slow part, so four threads share it.
    let mut apps: Vec<AppInfo> = Vec::with_capacity(entries.len());
    let chunk = entries.len().div_ceil(4).max(1);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = entries
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().map(|s| icons::icon(&s.icon)).collect::<Vec<_>>()))
            .collect();
        let icons = jobs.into_iter().flat_map(|job| job.join().unwrap_or_default());
        for (s, icon) in entries.iter().zip(icons) {
            let path = std::path::Path::new(&s.path).is_file().then(|| s.path.clone());
            apps.push(AppInfo { name: s.name.clone(), exe: s.exe.clone(), icon, path });
        }
    });
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// A program picked with Browse.
pub fn info_for(path: &str) -> AppInfo {
    AppInfo { name: icons::name(path), exe: exe_name(path), icon: icons::icon(path), path: Some(path.to_owned()) }
}

pub fn is_app(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

/// A Windows text buffer up to its end.
fn text(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn is_ext(path: &std::path::Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}
