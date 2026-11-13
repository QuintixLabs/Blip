// ============================================================
// effect/linux/wayland/hyprland.rs
// Hyprland: the clicks, where the cursor is and what's under it, all
// from Hyprland's own sockets. No permission needed.
// ============================================================

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{mice::Input, Pointer};
use crate::effect::Button;

pub(super) fn socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    [PathBuf::from(&runtime).join("hypr"), PathBuf::from("/tmp/hypr")]
        .into_iter()
        .map(|dir| dir.join(&signature).join(".socket.sock"))
        .find(|p| p.exists())
}

fn ask(socket: &Path, request: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(300))).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// "cursorpos" answers "1234, 567".
fn cursor_at(socket: &Path) -> Option<(f64, f64)> {
    let reply = ask(socket, "cursorpos")?;
    let (x, y) = reply.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Where the cursor is, and what's under it when a rule needs to know.
pub(super) fn cursor(socket: &Path) -> Option<Pointer> {
    let (x, y) = cursor_at(socket)?;
    let mut pointer = Pointer { x, y, pid: 0, app: String::new(), panel: false };
    let wants_panel = crate::effect::rules().taskbar;
    if !wants_panel && crate::effect::skipped_apps().is_empty() {
        return Some(pointer);
    }
    let monitors = json(socket, "monitors").unwrap_or_default();
    if wants_panel && on_panel(socket, &monitors, x, y) {
        pointer.panel = true;
        return Some(pointer);
    }
    if let Some(window) = window_at(socket, &monitors, x, y) {
        pointer.pid = window["pid"].as_u64().unwrap_or(0) as u32;
        pointer.app = window["class"].as_str().unwrap_or_default().to_owned();
    }
    Some(pointer)
}

// ------------------------------------------------------------
// What's under the cursor
// ------------------------------------------------------------

fn json(socket: &Path, what: &str) -> Option<Vec<Value>> {
    serde_json::from_str(&ask(socket, &format!("j/{what}"))?).ok()
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn inside(x: f64, y: f64, left: f64, top: f64, width: f64, height: f64) -> bool {
    x >= left && y >= top && x < left + width && y < top + height
}

/// A monitor's place on the desktop, in the cursor's units.
fn monitor_rect(monitor: &Value) -> (f64, f64, f64, f64) {
    let scale = monitor["scale"].as_f64().filter(|s| *s > 0.0).unwrap_or(1.0);
    let (mut width, mut height) = (num(&monitor["width"]) / scale, num(&monitor["height"]) / scale);
    if monitor["transform"].as_i64().unwrap_or(0) % 2 == 1 {
        std::mem::swap(&mut width, &mut height);
    }
    (num(&monitor["x"]), num(&monitor["y"]), width, height)
}

/// Bars ask Hyprland to keep an edge of the screen free for them. A click is on
/// a bar when it's in that kept space, on something drawn over the windows.
fn on_panel(socket: &Path, monitors: &[Value], x: f64, y: f64) -> bool {
    let Some(monitor) = monitors.iter().find(|m| {
        let (left, top, width, height) = monitor_rect(m);
        inside(x, y, left, top, width, height)
    }) else {
        return false;
    };
    let (left, top, width, height) = monitor_rect(monitor);
    let kept: Vec<f64> = monitor["reserved"].as_array().map(|r| r.iter().map(num).collect()).unwrap_or_default();
    let [kept_left, kept_top, kept_right, kept_bottom] = kept[..] else { return false };
    let in_kept = x < left + kept_left || y < top + kept_top || x >= left + width - kept_right || y >= top + height - kept_bottom;
    if !in_kept {
        return false;
    }
    let Some(layers) = ask(socket, "j/layers").and_then(|r| serde_json::from_str::<Value>(&r).ok()) else { return false };
    let Some(levels) = layers[monitor["name"].as_str().unwrap_or_default()]["levels"].as_object() else { return false };
    let own = std::process::id() as u64;
    // 1 and up: drawn over the desktop's background.
    levels.iter().filter(|(level, _)| level.as_str() != "0").filter_map(|(_, list)| list.as_array()).flatten().any(|layer| {
        layer["pid"].as_u64() != Some(own)
            && inside(x, y, num(&layer["x"]), num(&layer["y"]), num(&layer["w"]), num(&layer["h"]))
    })
}

/// The window under the cursor, among those on the workspaces showing now.
fn window_at(socket: &Path, monitors: &[Value], x: f64, y: f64) -> Option<Value> {
    let showing: Vec<i64> = monitors
        .iter()
        .flat_map(|m| [m["activeWorkspace"]["id"].as_i64(), m["specialWorkspace"]["id"].as_i64().filter(|id| *id != 0)])
        .flatten()
        .collect();
    let special: Vec<i64> = showing.iter().copied().filter(|id| *id < 0).collect();
    let mut under: Vec<Value> = json(socket, "clients")?
        .into_iter()
        .filter(|w| w["mapped"].as_bool() != Some(false) && w["hidden"].as_bool() != Some(true))
        .filter(|w| w["pinned"].as_bool() == Some(true) || w["workspace"]["id"].as_i64().is_some_and(|id| showing.contains(&id)))
        .filter(|w| {
            let (at, size) = (&w["at"], &w["size"]);
            inside(x, y, num(&at[0]), num(&at[1]), num(&size[0]), num(&size[1]))
        })
        .collect();
    // Front to back: special workspaces, fullscreen, floating, then the most recently used.
    let rank = |w: &Value| {
        let on_special = w["workspace"]["id"].as_i64().is_some_and(|id| special.contains(&id));
        let fullscreen = w["fullscreen"].as_bool().unwrap_or_else(|| w["fullscreen"].as_i64().unwrap_or(0) != 0);
        let floating = w["floating"].as_bool().unwrap_or(false) || w["pinned"].as_bool().unwrap_or(false);
        (!on_special, !fullscreen, !floating, w["focusHistoryID"].as_i64().unwrap_or(i64::MAX))
    };
    under.sort_by_key(rank);
    under.into_iter().next()
}

// ------------------------------------------------------------
// Clicks
// ------------------------------------------------------------

// Blip adds bindings for the three buttons that only send Blip an event: the
// click still goes to the app as usual (non-consuming), and other bindings
// still work (transparent). They aren't written to your config, so a config
// reload drops them; Blip adds them again when that happens.

const BUTTONS: [(Button, &str, u32); 3] = [(Button::Left, "left", 272), (Button::Right, "right", 273), (Button::Middle, "middle", 274)];

static WORKING: AtomicBool = AtomicBool::new(false);

/// Blips get their clicks from Hyprland, so reading the mice isn't needed.
pub(super) fn gives_clicks() -> bool {
    WORKING.load(Ordering::Relaxed)
}

/// Up to Hyprland 0.54 binds are hyprlang keywords; from 0.55 the config is Lua.
fn bind(socket: &Path, code: u32, event: &str, release: bool) -> bool {
    let ok = |reply: Option<String>| reply.is_some_and(|r| r.trim() == "ok");
    let flags = if release { "bindnrt" } else { "bindnt" };
    if ok(ask(socket, &format!("keyword {flags} ,mouse:{code},event,{event}"))) {
        return true;
    }
    let options = if release { "non_consuming = true, transparent = true, release = true" } else { "non_consuming = true, transparent = true" };
    ok(ask(socket, &format!(r#"eval hl.bind("mouse:{code}", hl.dsp.event("{event}"), {{ {options} }})"#)))
}

fn add_binds(socket: &Path) -> bool {
    let mut all = true;
    for (_, name, code) in BUTTONS {
        all &= bind(socket, code, &format!("blip-down-{name}"), false);
        all &= bind(socket, code, &format!("blip-up-{name}"), true);
    }
    all
}

/// Starts listening for clicks. False when Hyprland wouldn't take the bindings.
pub(super) fn watch(socket: &Path, input: Sender<Input>) -> bool {
    let events = socket.with_file_name(".socket2.sock");
    if !events.exists() || !add_binds(socket) {
        eprintln!("Blip: Hyprland didn't take the click bindings; reading the mice instead");
        return false;
    }
    WORKING.store(true, Ordering::Relaxed);
    let socket = socket.to_path_buf();
    thread::Builder::new()
        .name("hyprland".into())
        .spawn(move || loop {
            if let Ok(stream) = UnixStream::connect(&events) {
                read_events(&socket, stream, &input);
            }
            // Hyprland restarted or the socket went away for a moment.
            thread::sleep(Duration::from_secs(1));
            add_binds(&socket);
        })
        .is_ok()
}

fn read_events(socket: &Path, stream: UnixStream, input: &Sender<Input>) {
    // Running Blip again adds the bindings again, so one click can arrive twice.
    let mut last: Option<(String, Instant)> = None;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        if line.starts_with("configreloaded>>") {
            add_binds(socket);
            continue;
        }
        let Some(event) = line.strip_prefix("custom>>").and_then(|e| e.strip_prefix("blip-")) else { continue };
        if last.as_ref().is_some_and(|(e, at)| e == event && at.elapsed() < Duration::from_millis(30)) {
            continue;
        }
        last = Some((event.to_owned(), Instant::now()));
        let Some((kind, name)) = event.split_once('-') else { continue };
        let Some((button, _, _)) = BUTTONS.iter().find(|(_, n, _)| *n == name) else { continue };
        let Some((x, y)) = cursor_at(socket) else { continue };
        let sent = match kind {
            "down" => input.send(Input::PressAt(*button, x, y)),
            "up" => input.send(Input::ReleaseAt(*button, x, y)),
            _ => continue,
        };
        if sent.is_err() {
            return;
        }
    }
}
