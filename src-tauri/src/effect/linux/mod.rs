// ============================================================
// effect/linux/mod.rs
// Blips on Linux: X11, and Wayland on KDE Plasma, Hyprland and GNOME.
// ============================================================

mod gnome;
mod wayland;
mod x11;

use super::Features;

#[derive(Clone, Copy, PartialEq)]
enum Session {
    X11,
    Kde,
    Hyprland,
    Gnome,
    /// Sway, river, COSMIC and others: no way to ask where the cursor is.
    OtherWayland,
    None,
}

fn session() -> Session {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let wayland = !var("WAYLAND_DISPLAY").is_empty() || var("XDG_SESSION_TYPE") == "wayland";
    let desktop = var("XDG_CURRENT_DESKTOP").to_lowercase();
    if !wayland {
        return if var("DISPLAY").is_empty() { Session::None } else { Session::X11 };
    }
    if !var("HYPRLAND_INSTANCE_SIGNATURE").is_empty() {
        Session::Hyprland
    } else if desktop.contains("kde") {
        Session::Kde
    } else if desktop.contains("gnome") {
        Session::Gnome
    } else {
        Session::OtherWayland
    }
}

pub fn start() {
    match session() {
        Session::X11 => x11::start(),
        Session::Kde => wayland::start(wayland::Source::Kde),
        Session::Hyprland => wayland::start(wayland::Source::Hyprland),
        Session::Gnome => gnome::start(),
        _ => {}
    }
}

pub fn unsupported_reason() -> Option<String> {
    match session() {
        Session::X11 => x11::unsupported_reason(),
        Session::Kde | Session::Hyprland | Session::Gnome => None,
        Session::OtherWayland => Some(
            "This desktop doesn't tell apps where the cursor is, so blips can't show here yet. \
             They work on KDE Plasma, Hyprland, GNOME and any X11 desktop."
                .into(),
        ),
        Session::None => Some("Blips need a desktop to show on (X11 or Wayland).".into()),
    }
}

pub fn features() -> Features {
    match session() {
        // The cursor's name tells text and links apart; the window under it gives
        // the app and the panels. A hidden cursor means a game.
        Session::X11 => Features { text: true, links: true, taskbar: true, games: true, apps: true },
        // KDE says which window is under the cursor, so the app and the panels.
        Session::Kde => Features { text: false, links: false, taskbar: true, games: false, apps: true },
        // The extension knows the window under the cursor, and GNOME's top bar and overview.
        Session::Gnome => Features { text: false, links: false, taskbar: true, games: false, apps: true },
        // Hyprland lists the windows and the bars, with where they are.
        Session::Hyprland => Features { text: false, links: false, taskbar: true, games: false, apps: true },
        _ => Features { text: false, links: false, taskbar: false, games: false, apps: false },
    }
}

pub fn needs_access() -> bool {
    match session() {
        Session::Kde => wayland::access::needed(),
        Session::Gnome => !gnome::running(),
        Session::Hyprland => {
            // Hyprland gives the clicks itself; on a fresh start, give that a moment.
            for _ in 0..10 {
                if wayland::Finder::gives_clicks(wayland::Source::Hyprland) {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            wayland::access::needed()
        }
        _ => false,
    }
}

pub fn access_prompt() -> super::AccessPrompt {
    if session() == Session::Gnome {
        return super::AccessPrompt {
            text: "Blip needs a quick one-time setup on GNOME. Click Set up, then log out and back in.",
            button: "Set up",
            waits: false,
        };
    }
    super::AccessPrompt {
        text: "Blip needs your OK to see mouse clicks on this system. Your password is asked once.",
        button: "Allow",
        waits: false,
    }
}

pub fn allow_access() -> Result<(), String> {
    match session() {
        Session::Gnome => gnome::set_up(),
        _ => wayland::access::allow(),
    }
}
fn program_of(pid: u32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    Some(exe.file_name()?.to_string_lossy().to_lowercase())
}

/// `extra`: another name the desktop knows the app by (KDE's desktop file name).
fn skipped_app(pid: u32, extra: &str) -> bool {
    let skipped = super::skipped_apps();
    if skipped.is_empty() {
        return false;
    }
    let extra = extra.to_lowercase();
    program_of(pid).is_some_and(|p| skipped.contains(&p)) || (!extra.is_empty() && skipped.contains(&extra))
}
