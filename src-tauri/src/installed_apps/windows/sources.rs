// ============================================================
// installed_apps/windows/sources.rs
// Where apps are found: Start menu, desktop, Steam, Epic and open windows.
// ============================================================

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
};

use windows::{
    core::{Interface, BOOL, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ},
            Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ},
            Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Shell::{IShellLinkW, ShellLink},
            WindowsAndMessaging::{
                EnumWindows, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE,
                GW_OWNER, WS_EX_TOOLWINDOW,
            },
        },
    },
};

use super::{is_ext, text, wide, Shortcut};
use crate::installed_apps::exe_name;

/// Programs that a shortcut with arguments only uses to open something else
/// ("notepad.exe readme.txt"): those shortcuts aren't apps.
const HOSTS: [&str; 12] = [
    "explorer.exe",
    "notepad.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "python.exe",
    "pythonw.exe",
    "wsl.exe",
    "wslg.exe",
    "control.exe",
    "mmc.exe",
    "wscript.exe",
];

/// Files next to a game that aren't the game.
const GAME_HELPERS: [&str; 14] = [
    "unins",
    "crash",
    "redist",
    "vcredist",
    "dxsetup",
    "setup",
    "install",
    "report",
    "unitycrashhandler",
    "easyanticheat",
    "beservice",
    "battleye",
    "dotnet",
    "cefprocess",
];

// ------------------------------------------------------------
// Shortcuts
// ------------------------------------------------------------

/// The Start menu and desktop, for everyone and for this user.
pub(super) fn start_menu() -> Vec<Shortcut> {
    let mut links = Vec::new();
    let env = |var: &str| std::env::var_os(var).map(PathBuf::from);
    for var in ["ProgramData", "APPDATA"] {
        if let Some(base) = env(var) {
            find_links(&base.join(r"Microsoft\Windows\Start Menu\Programs"), 0, &mut links);
        }
    }
    let desktops =
        [env("USERPROFILE").map(|p| p.join("Desktop")), env("USERPROFILE").map(|p| p.join(r"OneDrive\Desktop")), env("PUBLIC").map(|p| p.join("Desktop"))];
    for dir in desktops.into_iter().flatten() {
        find_links(&dir, 4, &mut links);
    }

    let mut apps: Vec<Shortcut> = links.iter().filter_map(|path| web_shortcut(path)).collect();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return apps };
        let Ok(file) = link.cast::<IPersistFile>() else { return apps };
        for path in links.iter().filter(|p| is_ext(p, "lnk")) {
            let lnk = wide(&path.to_string_lossy());
            if file.Load(PCWSTR(lnk.as_ptr()), STGM_READ).is_err() {
                continue;
            }
            let mut buf = [0u16; 1024];
            if link.GetPath(&mut buf, std::ptr::null_mut(), 0).is_err() {
                continue;
            }
            let target = text(&buf);
            if !target.to_lowercase().ends_with(".exe") || !Path::new(&target).is_file() {
                continue;
            }
            let mut args = [0u16; 1024];
            let args = if link.GetArguments(&mut args).is_ok() { text(&args) } else { String::new() };
            let mut exe = exe_name(&target);
            // Discord, Slack and others start through "Update.exe --processStart Discord.exe".
            if exe == "update.exe" {
                match args.split("--processStart").nth(1).and_then(|rest| rest.split('"').find(|p| !p.trim().is_empty())) {
                    Some(app) => exe = app.trim().to_lowercase(),
                    None => continue,
                }
            } else if !args.trim().is_empty() && HOSTS.contains(&exe.as_str()) {
                continue;
            }
            // The shortcut's own icon, when it has one, is often nicer than the program's.
            let mut icon_buf = [0u16; 1024];
            let mut index = 0;
            let icon = match link.GetIconLocation(&mut icon_buf, &mut index) {
                Ok(()) if index == 0 && Path::new(&text(&icon_buf)).is_file() => text(&icon_buf),
                _ => target.clone(),
            };
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            apps.push(Shortcut { name, exe, icon, path: target, plain: args.trim().is_empty() });
        }
    }
    apps
}

fn find_links(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth < 4 {
            find_links(&path, depth + 1, out);
        } else if is_ext(&path, "lnk") || is_ext(&path, "url") {
            out.push(path);
        }
    }
}

/// A ".url" shortcut, which game launchers make. Epic's point their icon at the
/// game itself, which tells us the program.
fn web_shortcut(path: &Path) -> Option<Shortcut> {
    if !is_ext(path, "url") {
        return None;
    }
    let ini = std::fs::read_to_string(path).ok()?;
    let icon = ini.lines().find_map(|l| l.trim().strip_prefix("IconFile="))?.trim().to_owned();
    if !icon.to_lowercase().ends_with(".exe") || !Path::new(&icon).is_file() {
        return None;
    }
    let name = path.file_stem()?.to_string_lossy().into_owned();
    Some(Shortcut { name, exe: exe_name(&icon), path: icon.clone(), icon, plain: true })
}

// ------------------------------------------------------------
// Games
// ------------------------------------------------------------

/// Every installed Steam game, from Steam's own lists.
pub(super) fn steam_games() -> Vec<Shortcut> {
    let mut games = Vec::new();
    let Some(steam) = steam_folder() else { return games };
    let mut libraries = vec![steam.clone()];
    if let Ok(text) = std::fs::read_to_string(steam.join(r"steamapps\libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(path) = vdf_value(line, "path") {
                let path = PathBuf::from(path.replace("\\\\", "\\"));
                // The same folder can be written with different case or slashes.
                let same = |p: &PathBuf| p.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
                if !libraries.iter().any(same) {
                    libraries.push(path);
                }
            }
        }
    }
    for library in libraries {
        let apps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else { continue };
        for entry in entries.flatten() {
            let file = entry.path();
            if !is_ext(&file, "acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let (Some(name), Some(dir)) = (vdf_value(&text, "name"), vdf_value(&text, "installdir")) else { continue };
            if ["Redistributable", "Steamworks", "Proton", "Steam Linux Runtime"].iter().any(|w| name.contains(w)) {
                continue;
            }
            if let Some(exe) = main_program(&apps.join("common").join(dir)) {
                let exe = exe.to_string_lossy().into_owned();
                games.push(Shortcut { name: name.to_owned(), exe: exe_name(&exe), icon: exe.clone(), path: exe, plain: true });
            }
        }
    }
    games
}

fn steam_folder() -> Option<PathBuf> {
    unsafe {
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let key = wide(r"Software\Valve\Steam");
        let value = wide("SteamPath");
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        Some(PathBuf::from(text(&buf).replace('/', "\\")))
    }
}

/// `"key"   "value"` in Steam's files.
fn vdf_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let quoted = format!("\"{key}\"");
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(quoted.as_str())?;
        rest.trim().strip_prefix('"')?.strip_suffix('"')
    })
}

/// Every installed Epic Games game, from the launcher's own lists.
pub(super) fn epic_games() -> Vec<Shortcut> {
    let mut games = Vec::new();
    let Some(data) = std::env::var_os("ProgramData") else { return games };
    let dir = PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = std::fs::read_dir(dir) else { return games };
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        let Ok(item) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let field = |key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let (name, folder, launch) = (field("DisplayName"), field("InstallLocation"), field("LaunchExecutable"));
        if name.is_empty() || launch.is_empty() {
            continue;
        }
        let exe = main_program(Path::new(&folder)).unwrap_or_else(|| Path::new(&folder).join(&launch));
        let exe = exe.to_string_lossy().into_owned();
        games.push(Shortcut { name, exe: exe_name(&exe), icon: exe.clone(), path: exe, plain: true });
    }
    games
}

/// A game's real program: the biggest one in its folder. Unreal games keep a
/// small starter next to the real game, which is always bigger.
fn main_program(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: u32, best: &mut Option<(u64, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 3 {
                    walk(&path, depth + 1, best);
                }
            } else if is_ext(&path, "exe") {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                if GAME_HELPERS.iter().any(|h| name.contains(h)) {
                    continue;
                }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if best.as_ref().is_none_or(|(s, _)| size > *s) {
                    *best = Some((size, path));
                }
            }
        }
    }
    let mut best = None;
    walk(dir, 0, &mut best);
    best.map(|(_, path)| path)
}

// ------------------------------------------------------------
// Open windows
// ------------------------------------------------------------

pub(super) fn open_windows() -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut paths as *mut Vec<String> as isize));
    }
    paths
}

/// Visible, titled, top-level windows: the kind on the taskbar.
unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    let paths = &mut *(data.0 as *mut Vec<String>);
    let tool = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
    let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null());
    if IsWindowVisible(hwnd).as_bool() && !tool && !owned && GetWindowTextLengthW(hwnd) > 0 && !cloaked(hwnd) {
        if let Some(path) = program_path(hwnd) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    BOOL(1)
}

/// Hidden by Windows itself (suspended Store apps, other desktops).
unsafe fn cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut value as *mut u32 as *mut c_void, 4).is_ok() && value != 0
}

unsafe fn program_path(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == GetCurrentProcessId() {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(process);
    ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
}
