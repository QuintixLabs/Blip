// ============================================================
// cursor.rs
// Wayland: where the cursor is when you click. Wayland never tells
// apps that, so Blip asks the desktop:
//   KDE Plasma  runs a tiny KWin script that reports the cursor, and
//               the window under it, back to Blip
//   Hyprland    asks Hyprland's control socket
// ============================================================

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Sender},
        Mutex,
    },
    time::Duration,
};

/// Which desktop to ask.
#[derive(Clone, Copy)]
pub enum Source {
    Kde,
    Hyprland,
}

/// Where the cursor is (screen position, in the desktop's own units) and what's under it.
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    /// The program under the cursor; 0 when unknown.
    pub pid: u32,
    /// Another name the desktop knows that program by (KDE: its desktop file).
    pub app: String,
    /// The window under the cursor is a panel (the taskbar).
    pub panel: bool,
}

pub enum Finder {
    Kde(Kde),
    Hyprland(PathBuf),
}

impl Finder {
    pub fn new(source: Source) -> Option<Self> {
        match source {
            Source::Kde => Kde::new().map(Finder::Kde),
            Source::Hyprland => hyprland_socket().map(Finder::Hyprland),
        }
    }

    pub fn find(&self) -> Option<Pointer> {
        match self {
            Finder::Kde(kde) => kde.find(),
            Finder::Hyprland(socket) => hyprland_cursor(socket),
        }
    }
}

// ------------------------------------------------------------
// KDE Plasma
// ------------------------------------------------------------

const SERVICE: &str = "com.quintixlabs.blip.Cursor";

/// Who's waiting for the script's answer.
static WAITING: Mutex<Option<Sender<Pointer>>> = Mutex::new(None);

/// What the KWin script calls with its answer.
struct Reporter;

#[zbus::interface(name = "com.quintixlabs.blip.Cursor")]
impl Reporter {
    fn report(&self, x: f64, y: f64, pid: f64, panel: bool, app: String) {
        if let Some(waiting) = WAITING.lock().unwrap().take() {
            let _ = waiting.send(Pointer { x, y, pid: pid.max(0.0) as u32, app, panel });
        }
    }
}

pub struct Kde {
    bus: zbus::blocking::Connection,
    script: PathBuf,
    count: AtomicU32,
}

/// Runs inside KWin. Finds the cursor and the top window under it (leaving out
/// Blip's own blips), then calls Blip back. Works on Plasma 5 and 6.
const SCRIPT: &str = r#"
(function () {
  var p = workspace.cursorPos;
  var found = null;
  try {
    var list = workspace.windowAt(p, 5);
    for (var i = 0; i < list.length; i++) {
      if (list[i].pid != __BLIP_PID__) { found = list[i]; break; }
    }
  } catch (e) {
    var order = workspace.stackingOrder || [];
    for (var j = order.length - 1; j >= 0; j--) {
      var w = order[j];
      var g = w.frameGeometry;
      if (w.pid == __BLIP_PID__ || w.minimized || !g) continue;
      if (p.x >= g.x && p.x < g.x + g.width && p.y >= g.y && p.y < g.y + g.height) { found = w; break; }
    }
  }
  var pid = found ? (found.pid || 0) : 0;
  var panel = found ? !!found.dock : false;
  var app = found ? String(found.desktopFileName || found.resourceClass || "") : "";
  callDBus("__SERVICE__", "/Cursor", "__SERVICE__", "Report", p.x, p.y, pid, panel, app);
})();
"#;

impl Kde {
    fn new() -> Option<Self> {
        let bus = zbus::blocking::connection::Builder::session()
            .ok()?
            .name(SERVICE)
            .ok()?
            .serve_at("/Cursor", Reporter)
            .ok()?
            .build()
            .ok()?;
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let script = dir.join("blip-cursor.js");
        let text = SCRIPT.replace("__BLIP_PID__", &std::process::id().to_string()).replace("__SERVICE__", SERVICE);
        std::fs::write(&script, text).ok()?;
        Some(Self { bus, script, count: AtomicU32::new(0) })
    }

    fn call(&self, path: &str, interface: &str, method: &str, args: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> Option<zbus::Message> {
        self.bus.call_method(Some("org.kde.KWin"), path, Some(interface), method, args).ok()
    }

    fn find(&self) -> Option<Pointer> {
        let (tx, rx) = mpsc::channel();
        *WAITING.lock().unwrap() = Some(tx);

        // Each run is its own small script: load, run, then unload it again.
        let name = format!("blip-cursor-{}", self.count.fetch_add(1, Ordering::Relaxed));
        let path = self.script.to_string_lossy().into_owned();
        let id: i32 = self.call("/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path, name.clone()))?.body().deserialize().ok()?;
        // Plasma 6 keeps scripts at /Scripting/Script<id>, Plasma 5 at /<id>.
        if self.call(&format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run", &()).is_none() {
            self.call(&format!("/{id}"), "org.kde.kwin.Script", "run", &());
        }
        let answer = rx.recv_timeout(Duration::from_millis(300)).ok();
        self.call("/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(name,));
        answer
    }
}

// ------------------------------------------------------------
// Hyprland
// ------------------------------------------------------------

fn hyprland_socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    [PathBuf::from(&runtime).join("hypr"), PathBuf::from("/tmp/hypr")]
        .into_iter()
        .map(|dir| dir.join(&signature).join(".socket.sock"))
        .find(|p| p.exists())
}

/// "cursorpos" answers "1234, 567".
fn hyprland_cursor(socket: &PathBuf) -> Option<Pointer> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(300))).ok()?;
    stream.write_all(b"cursorpos").ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    let (x, y) = reply.trim().split_once(',')?;
    Some(Pointer { x: x.trim().parse().ok()?, y: y.trim().parse().ok()?, pid: 0, app: String::new(), panel: false })
}
