// ============================================================
// settings.rs
// The settings file, saved in
//   Windows  %APPDATA%\Blip\blip.json
//   macOS    ~/Library/Application Support/Blip/blip.json
//   Linux    ~/.config/Blip/blip.json
// To see the welcome screen again: exit Blip, set "showWelcome": true.
// ============================================================

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex, time::SystemTime};
use tauri::{AppHandle, Emitter, Manager};

use crate::system::tray;

const FILE: &str = "blip.json";

/// Where settings live: a plain "Blip" folder in the system's config folder.
pub fn settings_dir(app: &AppHandle) -> PathBuf {
    app.path().config_dir().unwrap_or_else(|_| PathBuf::from(".")).join("Blip")
}

// ------------------------------------------------------------
// What's saved
// ------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // The blip
    /// "circle", "ring", "square", "hexagon" or "diamond".
    pub shape: String,
    /// "#RRGGBB".
    pub color: String,
    /// Size when it appears and when it's done growing, in px.
    pub start_size: u32,
    pub end_size: u32,
    /// How see-through it starts, 0–100 %.
    pub opacity: u32,
    /// 1 (slow) to 10 (fast).
    pub speed: u32,
    /// How far through the blip it starts fading, 0–90 %.
    pub fade_start: u32,

    // Where blips don't show
    pub skip_text_cursor: bool,
    pub skip_link_cursor: bool,
    pub skip_taskbar: bool,
    pub skip_games: bool,

    // General
    /// Off: Blip is paused (tray: "Pause blips").
    pub enabled: bool,
    /// "system", "light" or "dark".
    pub theme: String,
    pub start_with_computer: bool,
    pub auto_update: bool,
    /// Show the welcome screen on the next start (true until it's been gone through).
    pub show_welcome: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shape: "circle".into(),
            color: "#000000".into(),
            start_size: 15,
            end_size: 40,
            opacity: 25,
            speed: 6,
            fade_start: 50,
            skip_text_cursor: true,
            skip_link_cursor: false,
            skip_taskbar: true,
            skip_games: true,
            enabled: true,
            theme: "system".into(),
            start_with_computer: true,
            auto_update: true,
            show_welcome: true,
        }
    }
}

impl Settings {
    /// Keeps every value in a range Blip can draw, whatever is in the file.
    pub fn sanitized(mut self) -> Self {
        if !matches!(self.shape.as_str(), "circle" | "ring" | "square" | "hexagon" | "diamond") {
            self.shape = "circle".into();
        }
        if parse_hex(&self.color).is_none() {
            self.color = "#000000".into();
        }
        self.start_size = self.start_size.clamp(2, 400);
        self.end_size = self.end_size.clamp(2, 400);
        self.opacity = self.opacity.clamp(1, 100);
        self.speed = self.speed.clamp(1, 10);
        self.fade_start = self.fade_start.min(90);
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self
    }
}

/// "#1a2B3c" → [26, 43, 60].
pub fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// Left behind right before an update installs, read on the next start.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    pub version: String,
    /// "background" (restart quietly into the tray) or "manual" (reopen the window).
    pub mode: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Data {
    pub settings: Settings,
    /// Unix ms of the last automatic update check.
    pub last_update_check: u64,
    pub update_notice: Option<UpdateNotice>,
}

// ------------------------------------------------------------
// Reading and saving
// ------------------------------------------------------------

pub struct Store {
    path: PathBuf,
    data: Mutex<Data>,
}

impl Store {
    pub fn load(app: &AppHandle) -> Self {
        let path = settings_dir(app).join(FILE);
        let mut data = match fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str::<Data>(&json).unwrap_or_default(),
            // First start: bring over the settings of the old AutoHotkey Blip.
            Err(_) => Data { settings: from_old_blip().unwrap_or_default(), ..Data::default() },
        };
        data.settings = data.settings.sanitized();
        Self { path, data: Mutex::new(data) }
    }

    pub fn get(&self) -> Data {
        self.data.lock().unwrap().clone()
    }

    /// Changes, saves to disk, then tells every window and the tray menu.
    pub fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Data)) -> Data {
        let data = {
            let mut data = self.data.lock().unwrap();
            change(&mut data);
            data.settings = data.settings.clone().sanitized();

            if let Some(dir) = self.path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(&*data) {
                let _ = fs::write(&self.path, json);
            }
            data.clone()
        }; // lock released: the tray reads the settings while rebuilding

        let _ = app.emit("state", &data);
        tray::refresh(app);
        data
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or_default()
}

// ------------------------------------------------------------
// The old AutoHotkey Blip
// ------------------------------------------------------------

/// Blip 1.x (AutoHotkey) kept its settings in %LOCALAPPDATA%\Blip\settings.ini.
/// Its values are turned into the new ones, so nobody has to set Blip up again.
fn from_old_blip() -> Option<Settings> {
    let dir = std::env::var_os("LOCALAPPDATA")?;
    let ini = fs::read_to_string(PathBuf::from(dir).join("Blip").join("settings.ini")).ok()?;
    let value = |key: &str| {
        ini.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(key))
            .map(|(_, v)| v.trim().to_owned())
    };
    let number = |key: &str| value(key).and_then(|v| v.parse::<f64>().ok());
    let flag = |key: &str| value(key).map(|v| v == "1");

    let mut s = Settings::default();
    if let Some(v) = value("ShadowShape") {
        s.shape = v.to_lowercase();
    }
    if let Some(v) = value("ShadowColor") {
        s.color = format!("#{}", v.trim_start_matches('#'));
    }
    if let Some(v) = number("InitialSize") {
        s.start_size = v as u32;
    }
    if let Some(v) = number("MaxSize") {
        s.end_size = v as u32;
    }
    // Opacity was 0–255, speed was "growth per tick" (1–20ish), fade start 0–1.
    if let Some(v) = number("MaxOpacity") {
        s.opacity = (v / 255.0 * 100.0).round() as u32;
    }
    if let Some(v) = number("ExpandSpeed") {
        s.speed = v.round() as u32;
    }
    if let Some(v) = number("FadeStartPercent") {
        s.fade_start = (v * 100.0).round() as u32;
    }
    s.skip_text_cursor = flag("BlockIBeam").unwrap_or(s.skip_text_cursor);
    s.skip_link_cursor = flag("BlockHand").unwrap_or(s.skip_link_cursor);
    s.skip_taskbar = flag("BlockTaskbar").unwrap_or(s.skip_taskbar);
    s.skip_games = flag("BlockGames").unwrap_or(s.skip_games);
    s.start_with_computer = flag("RunOnStartup").unwrap_or(s.start_with_computer);
    Some(s.sanitized())
}
