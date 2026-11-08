// ============================================================
// apps.rs
// The programs shown in "Skip these apps": the ones in the Start menu
// and any open right now, each with its real name and icon, or one
// chosen with Browse.
// ============================================================

use serde::Serialize;

/// One program, as the window shows it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Its own name ("Visual Studio Code"), or the file name without ".exe".
    pub name: String,
    /// Lowercase file name ("code.exe"): what's saved and matched.
    pub exe: String,
    /// A PNG data URL, when the icon could be read.
    pub icon: Option<String>,
}

pub use platform::{all, info_for};

/// File name of a path, lowercase ("C:\\…\\Code.exe" → "code.exe", "/usr/bin/firefox" → "firefox").
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

#[cfg(windows)]
mod platform {
    use std::{
        collections::HashMap,
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        sync::{Mutex, OnceLock},
    };

    use tiny_skia::{Pixmap, PremultipliedColorU8};
    use windows::{
        core::{Interface, BOOL, PCWSTR, PWSTR},
        Win32::{
            Foundation::{CloseHandle, HWND, LPARAM},
            Graphics::{
                Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
                Gdi::{DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ},
            },
            Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
            System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ},
            System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ},
            System::Threading::{
                GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::{
                Shell::{IShellLinkW, SHDefExtractIconW, ShellLink},
                WindowsAndMessaging::{
                    DestroyIcon, EnumWindows, GetIconInfo, GetWindow, GetWindowLongW, GetWindowTextLengthW,
                    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, HICON, ICONINFO, WS_EX_TOOLWINDOW,
                },
            },
        },
    };

    use super::{base64, exe_name, plain_name, AppInfo};

    /// Size the icons are read at: sharp at up to 200 % scaling in the list.
    const ICON_PX: u32 = 48;

    /// Windows parts and hosts that aren't apps anyone would pick.
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

    /// Programs that aren't apps anyone would pick: uninstallers, updaters, helpers.
    fn not_an_app(name: &str, exe: &str) -> bool {
        let name = name.to_lowercase();
        NOT_APPS.contains(&exe)
            || exe.starts_with("unins")
            || ["uninstall", "updater", "crashreport", "errorreport", "bugreport"].iter().any(|w| exe.contains(w))
            || exe.ends_with("_proxy.exe")
            || matches!(exe, "msiexec.exe" | "rundll32.exe" | "hh.exe")
            || HELPER_WORDS.iter().any(|w| name.contains(w))
    }

    /// Programs that only open something else when a shortcut gives them more to
    /// do ("notepad.exe readme.txt"): those shortcuts aren't apps of their own.
    const HOSTS: [&str; 12] = [
        "explorer.exe", "notepad.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "python.exe", "pythonw.exe",
        "wsl.exe", "wslg.exe", "control.exe", "mmc.exe", "wscript.exe",
    ];

    /// Every app: the ones in the Start menu, plus any open right now that
    /// aren't in it. Sorted by name.
    pub fn all() -> Vec<AppInfo> {
        // Plain shortcuts first (they name the app itself), then shortcuts that
        // pass something along, then open windows: the first for each program wins.
        let mut found = start_menu();
        found.sort_by_key(|s| (!s.plain, s.name.len()));
        found.extend(steam_games());
        found.extend(epic_games());
        found.extend(open_windows().into_iter().map(|path| Shortcut {
            name: description(&path).map(|d| plain_name(&d)).unwrap_or_else(|| plain_name(&path)),
            exe: exe_name(&path),
            icon: path,
            plain: true,
        }));

        // (name, exe, icon file)
        let mut entries: Vec<(String, String, String)> = Vec::new();
        for s in found {
            if not_an_app(&s.name, &s.exe) || entries.iter().any(|(_, e, _)| *e == s.exe) {
                continue;
            }
            entries.push((s.name, s.exe, s.icon));
        }

        // Icons take the longest, so a few are read at once (and kept for next time).
        let mut apps: Vec<AppInfo> = Vec::with_capacity(entries.len());
        let chunk = entries.len().div_ceil(4).max(1);
        std::thread::scope(|scope| {
            let jobs: Vec<_> = entries
                .chunks(chunk)
                .map(|part| scope.spawn(move || part.iter().map(|(_, _, path)| icon(path)).collect::<Vec<_>>()))
                .collect();
            let icons = jobs.into_iter().flat_map(|job| job.join().unwrap_or_default());
            for ((name, exe, _), icon) in entries.iter().zip(icons) {
                apps.push(AppInfo { name: name.clone(), exe: exe.clone(), icon });
            }
        });
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    /// Name and icon of the program at `path`.
    pub fn info_for(path: &str) -> AppInfo {
        AppInfo {
            // Some programs give their file name as their name ("WindowsTerminal.exe").
            name: description(path).map(|d| plain_name(&d)).unwrap_or_else(|| plain_name(path)),
            exe: exe_name(path),
            icon: icon(path),
        }
    }

    // ------------------------------------------------------------
    // Where apps are found
    // ------------------------------------------------------------

    /// An app found in the Start menu (or open on screen).
    struct Shortcut {
        name: String,
        /// Lowercase file name of the program it starts.
        exe: String,
        /// Where its icon is read from.
        icon: String,
        /// Starts the program with nothing extra passed along.
        plain: bool,
    }

    /// The shortcuts in the Start menu and on the desktop, for everyone and for this user.
    fn start_menu() -> Vec<Shortcut> {
        let mut links = Vec::new();
        let env = |var: &str| std::env::var_os(var).map(std::path::PathBuf::from);
        for var in ["ProgramData", "APPDATA"] {
            if let Some(base) = env(var) {
                find_links(&base.join(r"Microsoft\Windows\Start Menu\Programs"), 0, &mut links);
            }
        }
        let desktops = [env("USERPROFILE").map(|p| p.join("Desktop")), env("USERPROFILE").map(|p| p.join(r"OneDrive\Desktop")), env("PUBLIC").map(|p| p.join("Desktop"))];
        for dir in desktops.into_iter().flatten() {
            find_links(&dir, 4, &mut links);
        }

        let mut apps: Vec<Shortcut> = links.iter().filter_map(|path| web_shortcut(path)).collect();
        unsafe {
            // Already set up on this thread is fine too.
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
                if !target.to_lowercase().ends_with(".exe") || !std::path::Path::new(&target).is_file() {
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
                // The shortcut's own icon when it has one (often nicer than the program's).
                let mut icon_buf = [0u16; 1024];
                let mut index = 0;
                let icon = match link.GetIconLocation(&mut icon_buf, &mut index) {
                    Ok(()) if index == 0 && std::path::Path::new(&text(&icon_buf)).is_file() => text(&icon_buf),
                    _ => target.clone(),
                };
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                apps.push(Shortcut { name, exe, icon, plain: args.trim().is_empty() });
            }
        }
        apps
    }

    /// A Windows text buffer up to its end.
    fn text(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    fn find_links(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
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

    fn is_ext(path: &std::path::Path, ext: &str) -> bool {
        path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    // ------------------------------------------------------------
    // Games
    // ------------------------------------------------------------

    /// Web-link shortcuts (".url"), which game launchers make. Epic's point their
    /// icon at the game itself, which names the program; Steam's are covered by
    /// steam_games().
    fn web_shortcut(path: &std::path::Path) -> Option<Shortcut> {
        if !is_ext(path, "url") {
            return None;
        }
        let ini = std::fs::read_to_string(path).ok()?;
        let icon = ini.lines().find_map(|l| l.trim().strip_prefix("IconFile="))?.trim().to_owned();
        if !icon.to_lowercase().ends_with(".exe") || !std::path::Path::new(&icon).is_file() {
            return None;
        }
        let name = path.file_stem()?.to_string_lossy().into_owned();
        Some(Shortcut { name, exe: exe_name(&icon), icon, plain: true })
    }

    /// `"key"   "value"` in Steam's files.
    fn vdf_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        let quoted = format!("\"{key}\"");
        text.lines().find_map(|line| {
            let rest = line.trim().strip_prefix(quoted.as_str())?;
            rest.trim().strip_prefix('"')?.strip_suffix('"')
        })
    }

    /// Every installed Steam game, from Steam's own lists.
    fn steam_games() -> Vec<Shortcut> {
        let mut games = Vec::new();
        let Some(steam) = steam_folder() else { return games };
        let mut libraries = vec![steam.clone()];
        if let Ok(text) = std::fs::read_to_string(steam.join(r"steamapps\libraryfolders.vdf")) {
            for line in text.lines() {
                if let Some(path) = vdf_value(line, "path") {
                    let path = std::path::PathBuf::from(path.replace("\\\\", "\\"));
                    // The same folder may be spelled differently (case, slashes).
                    let same = |p: &std::path::PathBuf| p.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
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
                    games.push(Shortcut { name: name.to_owned(), exe: exe_name(&exe), icon: exe, plain: true });
                }
            }
        }
        games
    }

    /// Where Steam is installed, from the registry.
    fn steam_folder() -> Option<std::path::PathBuf> {
        unsafe {
            let mut buf = [0u16; 512];
            let mut size = (buf.len() * 2) as u32;
            let key = wide(r"Software\Valve\Steam");
            let value = wide("SteamPath");
            RegGetValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut size))
                .ok()
                .ok()?;
            Some(std::path::PathBuf::from(text(&buf).replace('/', "\\")))
        }
    }

    /// Helpers that ship next to a game but aren't it.
    const GAME_HELPERS: [&str; 14] = [
        "unins", "crash", "redist", "vcredist", "dxsetup", "setup", "install", "report", "unitycrashhandler",
        "easyanticheat", "beservice", "battleye", "dotnet", "cefprocess",
    ];

    /// A game's own program: the biggest one in its folder (a few levels deep),
    /// leaving out installers and crash reporters. Unreal games keep a small
    /// starter next to the real game, and the biggest is the real one.
    fn main_program(dir: &std::path::Path) -> Option<std::path::PathBuf> {
        fn walk(dir: &std::path::Path, depth: u32, best: &mut Option<(u64, std::path::PathBuf)>) {
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

    /// Every installed Epic Games game, from the launcher's own lists.
    fn epic_games() -> Vec<Shortcut> {
        let mut games = Vec::new();
        let Some(data) = std::env::var_os("ProgramData") else { return games };
        let dir = std::path::PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests");
        let Ok(entries) = std::fs::read_dir(dir) else { return games };
        for entry in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Ok(item) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let field = |key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
            let (name, folder, launch) = (field("DisplayName"), field("InstallLocation"), field("LaunchExecutable"));
            if name.is_empty() || launch.is_empty() {
                continue;
            }
            let exe = std::path::Path::new(&folder).join(&launch);
            // The launch program may be a small starter; the game itself is the biggest one.
            let exe = main_program(std::path::Path::new(&folder)).unwrap_or(exe);
            let exe = exe.to_string_lossy().into_owned();
            games.push(Shortcut { name, exe: exe_name(&exe), icon: exe, plain: true });
        }
        games
    }

    /// Programs with a window open right now.
    fn open_windows() -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut paths as *mut Vec<String> as isize));
        }
        paths
    }

    /// Visible, titled, top-level app windows, the kind on the taskbar.
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

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text).encode_wide().chain(Some(0)).collect()
    }

    /// The program's own name ("FileDescription" in its version info).
    fn description(path: &str) -> Option<String> {
        unsafe {
            let file = wide(path);
            let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;

            // The languages it has text for; the first one is used.
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            let query = wide("\\VarFileInfo\\Translation");
            let mut lang = (0x0409u16, 0x04B0u16);
            if VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() && len >= 4 {
                let pair = ptr as *const u16;
                lang = (*pair, *pair.add(1));
            }
            for (l, c) in [lang, (0x0409, 0x04B0), (0x0409, 0x04E4), (0x0000, 0x04B0)] {
                let query = wide(&format!("\\StringFileInfo\\{l:04x}{c:04x}\\FileDescription"));
                if VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() && len > 1 {
                    let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                    let text = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_owned();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
    }

    /// The program's icon as a PNG data URL. Kept once read, so the list opens
    /// quickly the next time.
    fn icon(path: &str) -> Option<String> {
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(hit) = cache.lock().unwrap().get(path) {
            return hit.clone();
        }
        let icon = read_icon(path);
        cache.lock().unwrap().insert(path.to_owned(), icon.clone());
        icon
    }

    fn read_icon(path: &str) -> Option<String> {
        unsafe {
            let file = wide(path);
            let mut large = HICON::default();
            SHDefExtractIconW(PCWSTR(file.as_ptr()), 0, 0, Some(&mut large), None, ICON_PX).ok().ok()?;
            let png = icon_png(large);
            let _ = DestroyIcon(large);
            png.map(|bytes| format!("data:image/png;base64,{}", base64(&bytes)))
        }
    }

    unsafe fn icon_png(icon: HICON) -> Option<Vec<u8>> {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let side = ICON_PX as i32;
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (side * side * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, info.hbmColor, 0, side as u32, Some(bgra.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        if rows == 0 {
            return None;
        }

        // Old icons without transparency info: treat them as fully opaque.
        let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
        let mut pixmap = Pixmap::new(ICON_PX, ICON_PX)?;
        for (dst, p) in pixmap.pixels_mut().iter_mut().zip(bgra.chunks_exact(4)) {
            let a = if has_alpha { p[3] } else { 255 };
            let pre = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
            *dst = PremultipliedColorU8::from_rgba(pre(p[2]), pre(p[1]), pre(p[0]), a)?;
        }
        pixmap.encode_png().ok()
    }
}

/// Linux: the apps in the app menu, from their .desktop files (system, your
/// own, Flatpak and Snap), with their icons from the icon themes.
#[cfg(target_os = "linux")]
mod platform {
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
    };

    use super::{base64, exe_name, plain_name, AppInfo};

    pub fn all() -> Vec<AppInfo> {
        let mut apps: Vec<AppInfo> = Vec::new();
        // The same file name in an earlier folder wins (yours over the system's).
        let mut seen_files = HashSet::new();
        for dir in data_dirs().iter().map(|d| d.join("applications")) {
            let mut files = Vec::new();
            find_desktop_files(&dir, 0, &mut files);
            for file in files {
                let id = file.strip_prefix(&dir).unwrap_or(&file).to_string_lossy().replace('/', "-");
                if !seen_files.insert(id) {
                    continue;
                }
                if let Some(app) = read_entry(&file) {
                    if !apps.iter().any(|a| a.exe == app.exe) {
                        apps.push(app);
                    }
                }
            }
        }
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    /// Browse: a .desktop file, or a program.
    pub fn info_for(path: &str) -> AppInfo {
        if path.ends_with(".desktop") {
            if let Some(app) = read_entry(Path::new(path)) {
                return app;
            }
        }
        AppInfo { name: plain_name(path), exe: exe_name(path), icon: None }
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

    fn find_desktop_files(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 2 {
                find_desktop_files(&path, depth + 1, out);
            } else if path.extension().is_some_and(|e| e == "desktop") {
                out.push(path);
            }
        }
    }

    /// One app from its .desktop file; None for hidden entries and ones that
    /// aren't apps.
    fn read_entry(file: &Path) -> Option<AppInfo> {
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
        // Steam games start through Steam, so their own program isn't known here.
        if exec.contains("steam://") {
            return None;
        }
        let exe = match flatpak {
            // KDE knows Flatpak apps by their id, which is what's matched.
            Some(id) => id,
            None => program(&exec)?,
        };
        Some(AppInfo { name, exe, icon: icon.and_then(|i| icon_data(&i)) })
    }

    /// The program an Exec line starts: "env FOO=1 /usr/bin/firefox %u" → "firefox".
    fn program(exec: &str) -> Option<String> {
        let words: Vec<&str> = exec.split_whitespace().map(|w| w.trim_matches('"')).collect();
        let mut words = words.into_iter().skip_while(|w| *w == "env" || w.contains('=') && !w.starts_with('/'));
        let first = words.next()?;
        Some(exe_name(first))
    }

    // ------------------------------------------------------------
    // Icons
    // ------------------------------------------------------------

    const THEMES: [&str; 5] = ["hicolor", "breeze", "breeze-dark", "Papirus", "Adwaita"];
    const SIZES: [&str; 7] = ["48x48", "64x64", "128x128", "256x256", "scalable", "32x32", "512x512"];

    fn icon_file(name: &str) -> Option<PathBuf> {
        if name.starts_with('/') {
            return Path::new(name).is_file().then(|| PathBuf::from(name));
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let mut bases = vec![home.join(".icons")];
        bases.extend(data_dirs().into_iter().map(|d| d.join("icons")));
        for base in &bases {
            for theme in THEMES {
                for size in SIZES {
                    for ext in ["png", "svg"] {
                        let file = base.join(theme).join(size).join("apps").join(format!("{name}.{ext}"));
                        if file.is_file() {
                            return Some(file);
                        }
                    }
                }
                // Breeze keeps them as apps/48/name.svg.
                let file = base.join(theme).join("apps/48").join(format!("{name}.svg"));
                if file.is_file() {
                    return Some(file);
                }
            }
        }
        ["png", "svg"].iter().map(|ext| PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"))).find(|f| f.is_file())
    }

    fn icon_data(name: &str) -> Option<String> {
        let file = icon_file(name)?;
        let kind = match file.extension()?.to_str()? {
            "png" => "image/png",
            "svg" => "image/svg+xml",
            _ => return None,
        };
        let bytes = std::fs::read(&file).ok().filter(|b| b.len() < 512 * 1024)?;
        Some(format!("data:{kind};base64,{}", base64(&bytes)))
    }
}

/// macOS: blips don't show there yet, so there's nothing to list.
#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::{exe_name, plain_name, AppInfo};

    pub fn all() -> Vec<AppInfo> {
        Vec::new()
    }

    pub fn info_for(path: &str) -> AppInfo {
        AppInfo { name: plain_name(path), exe: exe_name(path), icon: None }
    }
}

/// Bytes as base64, for icons sent to the window as data URLs.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
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