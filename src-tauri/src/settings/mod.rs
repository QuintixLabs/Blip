// ============================================================
// settings/mod.rs
// The settings file, blip.json
// ============================================================

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex, time::SystemTime};
use tauri::{AppHandle, Emitter, Manager};

use crate::tray;

const FILE: &str = "blip.json";

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
    pub left_click: bool,
    pub color: String,
    pub right_click: bool,
    pub right_color: String,
    pub middle_click: bool,
    pub middle_color: String,
    pub start_size: u32,
    pub end_size: u32,
    /// 1-100 %.
    pub opacity: u32,
    /// 1 (slow) to 10 (fast).
    pub speed: u32,
    /// 0-90 %.
    pub fade_start: u32,

    // Where blips don't show
    pub skip_text_cursor: bool,
    pub skip_link_cursor: bool,
    pub skip_taskbar: bool,
    pub skip_games: bool,
    /// Matched by file name only, so the list works on any PC.
    pub skip_apps: Vec<SkipApp>,

    // General
    /// "system", "light" or "dark".
    pub theme: String,
    pub notifications: bool,
    pub start_with_computer: bool,
    pub close_to_tray: bool,
    pub show_in_tray: bool,
    pub auto_update: bool,
    pub show_welcome: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shape: "circle".into(),
            left_click: true,
            color: "#000000".into(),
            right_click: false,
            right_color: "#FFB020".into(),
            middle_click: false,
            middle_color: "#A78BFA".into(),
            start_size: 15,
            end_size: 40,
            opacity: 25,
            speed: 6,
            fade_start: 50,
            skip_text_cursor: true,
            skip_link_cursor: false,
            skip_taskbar: true,
            skip_games: true,
            skip_apps: Vec::new(),
            theme: "system".into(),
            notifications: true,
            start_with_computer: true,
            close_to_tray: true,
            show_in_tray: true,
            auto_update: true,
            show_welcome: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkipApp {
    pub name: String,
    pub exe: String,
    /// Only for apps picked with Browse: where to read the icon from next time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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
        if parse_hex(&self.right_color).is_none() {
            self.right_color = "#FFB020".into();
        }
        if parse_hex(&self.middle_color).is_none() {
            self.middle_color = "#A78BFA".into();
        }
        self.start_size = self.start_size.clamp(2, 400);
        self.end_size = self.end_size.clamp(2, 400);
        self.opacity = self.opacity.clamp(1, 100);
        self.speed = self.speed.clamp(1, 10);
        self.fade_start = self.fade_start.min(90);
        for app in &mut self.skip_apps {
            app.exe = app.exe.trim().to_lowercase();
        }
        self.skip_apps.retain(|app| !app.exe.is_empty());
        let mut seen = Vec::new();
        self.skip_apps.retain(|app| if seen.contains(&app.exe) { false } else { seen.push(app.exe.clone()); true });
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
            Err(_) => Data::default(),
        };
        data.settings = data.settings.sanitized();
        Self { path, data: Mutex::new(data) }
    }

    pub fn get(&self) -> Data {
        self.data.lock().unwrap().clone()
    }

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
