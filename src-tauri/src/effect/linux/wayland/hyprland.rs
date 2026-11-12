// ============================================================
// effect/linux/wayland/hyprland.rs
// Hyprland: where the cursor is, and the clicks themselves, both from
// Hyprland's own sockets. No permission needed.
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
pub(super) fn cursor(socket: &Path) -> Option<Pointer> {
    let reply = ask(socket, "cursorpos")?;
    let (x, y) = reply.trim().split_once(',')?;
    Some(Pointer { x: x.trim().parse().ok()?, y: y.trim().parse().ok()?, pid: 0, app: String::new(), panel: false })
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
        let Some(at) = cursor(socket) else { continue };
        let sent = match kind {
            "down" => input.send(Input::PressAt(*button, at.x, at.y)),
            "up" => input.send(Input::ReleaseAt(*button, at.x, at.y)),
            _ => continue,
        };
        if sent.is_err() {
            return;
        }
    }
}
