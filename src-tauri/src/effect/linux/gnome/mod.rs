// ============================================================
// effect/linux/gnome/mod.rs
// GNOME on Wayland: a small GNOME Shell extension (in extension/) sees
// the clicks and draws the blips; Blip decides which clicks get one and
// how it looks. No password, but GNOME loads a new extension only after
// logging out and back in once.
// ============================================================

use std::{collections::HashMap, path::PathBuf, process::Command, sync::OnceLock};

use crate::effect::{shape::style::Shape, Button};

const UUID: &str = "blip@quintixlabs.com";
const SERVICE: &str = "com.quintixlabs.blip.Gnome";
const FILES: [(&str, &str); 2] = [("metadata.json", include_str!("extension/metadata.json")), ("extension.js", include_str!("extension/extension.js"))];

static BUS: OnceLock<zbus::blocking::Connection> = OnceLock::new();

struct Clicks;

#[zbus::interface(name = "com.quintixlabs.blip.Gnome")]
impl Clicks {
    /// A click the extension saw. Answers how its blip looks, or "" for none.
    fn click(&self, button: u32, x: f64, y: f64, pid: u32, app: String, shell: bool) -> String {
        let button = match button {
            0 => Button::Left,
            1 => Button::Right,
            2 => Button::Middle,
            _ => return String::new(),
        };
        if !crate::effect::active() {
            return String::new();
        }
        let Some(style) = crate::effect::style_for(button) else { return String::new() };
        if std::env::var_os("BLIP_DEBUG").is_some() {
            eprintln!("Blip: click at {x}, {y} (pid {pid}, app {app:?}, shell {shell})");
        }
        if (crate::effect::rules().taskbar && shell) || (pid != 0 && super::skipped_app(pid, &app)) {
            return String::new();
        }
        let shape = match style.shape {
            Shape::Circle => "circle",
            Shape::Ring => "ring",
            Shape::Square => "square",
            Shape::Hexagon => "hexagon",
            Shape::Diamond => "diamond",
        };
        let [r, g, b] = style.color;
        format!(
            r#"{{"shape":"{shape}","color":[{r},{g},{b}],"start":{},"end":{},"opacity":{},"ms":{},"fade":{}}}"#,
            style.start_size,
            style.end_size,
            style.opacity,
            style.duration.as_millis().max(1),
            style.fade_start.min(0.99),
        )
    }
}

pub fn start() {
    install();
    let bus = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(SERVICE))
        .and_then(|b| b.serve_at("/Gnome", Clicks))
        .and_then(|b| b.build());
    match bus {
        // Kept for as long as Blip runs; the extension starts once it sees the name.
        Ok(bus) => {
            let _ = BUS.set(bus);
        }
        Err(err) => eprintln!("Blip: couldn't set up the GNOME answer service: {err}"),
    }
}

// ------------------------------------------------------------
// The extension
// ------------------------------------------------------------

fn folder() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("gnome-shell/extensions").join(UUID))
}

/// Puts the extension's files in place, or brings them up to date. GNOME reads
/// new files at the next login.
fn install() -> bool {
    let Some(dir) = folder() else { return false };
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    FILES.iter().all(|(name, text)| {
        let path = dir.join(name);
        std::fs::read_to_string(&path).is_ok_and(|now| now == *text) || std::fs::write(&path, text).is_ok()
    })
}

/// GNOME is running the extension right now.
pub fn running() -> bool {
    let Ok(bus) = zbus::blocking::Connection::session() else { return false };
    let Ok(reply) = bus.call_method(
        Some("org.gnome.Shell.Extensions"),
        "/org/gnome/Shell/Extensions",
        Some("org.gnome.Shell.Extensions"),
        "GetExtensionInfo",
        &(UUID,),
    ) else {
        return false;
    };
    let Ok(info) = reply.body().deserialize::<HashMap<String, zbus::zvariant::OwnedValue>>() else { return false };
    // 1: active (called "enabled" before GNOME 46).
    info.get("state").and_then(|state| f64::try_from(state).ok()) == Some(1.0)
}

fn gsettings(args: &[&str]) -> Option<String> {
    let out = Command::new("gsettings").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The Set up button: installs the extension and turns it on.
pub fn set_up() -> Result<(), String> {
    if !install() {
        return Err("Couldn't put the GNOME extension in place.".into());
    }
    if gsettings(&["get", "org.gnome.shell", "disable-user-extensions"]).as_deref() == Some("true") {
        return Err("Extensions are turned off on this computer. Turn them on in the Extensions app, then try again.".into());
    }
    let list = gsettings(&["get", "org.gnome.shell", "enabled-extensions"]).unwrap_or_default();
    if !list.contains(UUID) {
        // "['a@b', 'c@d']", or "@as []" when empty.
        let mut ids: Vec<String> = list.split('\'').skip(1).step_by(2).map(|id| format!("'{id}'")).collect();
        ids.push(format!("'{UUID}'"));
        if gsettings(&["set", "org.gnome.shell", "enabled-extensions", &format!("[{}]", ids.join(", "))]).is_none() {
            return Err("Couldn't turn on the GNOME extension.".into());
        }
    }
    // GNOME may already know it (set up before): then blips start now.
    let _ = Command::new("gnome-extensions").args(["enable", UUID]).status();
    for _ in 0..10 {
        if running() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Err("All set. Log out and back in once, and blips start.".into())
}
