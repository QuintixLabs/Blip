// ============================================================
// linux/wayland/kde.rs
// KDE Plasma: where the cursor is, and what's under it. A tiny KWin
// script reports both back to Blip over D-Bus.
// ============================================================

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Sender},
        Mutex,
    },
    time::Duration,
};

use super::Pointer;

const SERVICE: &str = "com.quintixlabs.blip.Cursor";

/// Who's waiting for the script's answer.
static WAITING: Mutex<Option<Sender<Pointer>>> = Mutex::new(None);

/// What the KWin script calls with its answer.
struct Reporter;

// Everything comes as text: KWin sends whole numbers as integers and others as
// decimals, and D-Bus turns away a call whose types don't match exactly.
#[zbus::interface(name = "com.quintixlabs.blip.Cursor")]
impl Reporter {
    fn report(&self, x: String, y: String, pid: String, panel: String, app: String) {
        let number = |text: &str| text.trim().parse::<f64>().ok();
        let (Some(x), Some(y)) = (number(&x), number(&y)) else {
            eprintln!("Blip: KWin sent a cursor position Blip can't read: {x}, {y}");
            return;
        };
        let pointer = Pointer { x, y, pid: number(&pid).unwrap_or(0.0).max(0.0) as u32, app, panel: panel == "true" };
        if let Some(waiting) = WAITING.lock().unwrap().take() {
            let _ = waiting.send(pointer);
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
  callDBus("__SERVICE__", "/Cursor", "__SERVICE__", "Report", String(p.x), String(p.y), String(pid), String(panel), app);
})();
"#;

impl Kde {
    pub(super) fn new() -> Option<Self> {
        let bus = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(SERVICE))
            .and_then(|b| b.serve_at("/Cursor", Reporter))
            .and_then(|b| b.build())
            .map_err(|err| eprintln!("Blip: couldn't set up the KWin answer service: {err}"))
            .ok()?;
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let script = dir.join("blip-cursor.js");
        let text = SCRIPT.replace("__BLIP_PID__", &std::process::id().to_string()).replace("__SERVICE__", SERVICE);
        std::fs::write(&script, text).ok()?;
        Some(Self { bus, script, count: AtomicU32::new(0) })
    }

    fn call(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        args: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> Option<zbus::Message> {
        self.bus.call_method(Some("org.kde.KWin"), path, Some(interface), method, args).ok()
    }

    pub(super) fn find(&self) -> Option<Pointer> {
        let (tx, rx) = mpsc::channel();
        *WAITING.lock().unwrap() = Some(tx);

        // Each run is its own small script: load, run, then unload it again.
        let name = format!("blip-cursor-{}", self.count.fetch_add(1, Ordering::Relaxed));
        let path = self.script.to_string_lossy().into_owned();
        let Some(id) = self
            .call("/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path, name.clone()))
            .and_then(|reply| reply.body().deserialize::<i32>().ok())
        else {
            eprintln!("Blip: KWin didn't load the cursor script");
            return None;
        };
        // Plasma 6 keeps scripts at /Scripting/Script<id>, Plasma 5 at /<id>.
        if self.call(&format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run", &()).is_none()
            && self.call(&format!("/{id}"), "org.kde.kwin.Script", "run", &()).is_none()
        {
            eprintln!("Blip: KWin didn't run the cursor script (id {id})");
        }
        let answer = rx.recv_timeout(Duration::from_millis(300)).ok();
        if answer.is_none() {
            eprintln!("Blip: no answer from the KWin cursor script");
        }
        self.call("/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(name,));
        answer
    }
}
