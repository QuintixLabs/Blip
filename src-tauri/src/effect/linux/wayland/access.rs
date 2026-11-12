// ============================================================
// effect/linux/wayland/access.rs
// The permission to read mice and touchpads: a udev rule installed by the
// .deb/.rpm or by the Allow button. Keyboards are never included.
// ============================================================

use std::{fs, path::Path, process::Command};

const RULE: &str = include_str!("../../../../installer/linux/70-blip.rules");

const RULE_FILE: &str = "/etc/udev/rules.d/70-blip.rules";

pub fn needed() -> bool {
    let (readable, blocked) = super::mice::count();
    readable == 0 && blocked > 0
}

pub fn allow() -> Result<(), String> {
    let temp = std::env::temp_dir().join("blip-70.rules");
    fs::write(&temp, RULE).map_err(|e| format!("Couldn't prepare the rule: {e}"))?;

    let script = format!(
        "install -Dm644 '{}' '{RULE_FILE}' && udevadm control --reload-rules && udevadm trigger --subsystem-match=input --action=change",
        temp.display()
    );
    let status = Command::new("pkexec").args(["/bin/sh", "-c", &script]).status();
    let _ = fs::remove_file(&temp);

    match status {
        Ok(s) if s.success() => {
            // udev applies it in the background; give it a moment.
            for _ in 0..20 {
                if !needed() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if Path::new(RULE_FILE).exists() {
                Err("Allowed. If blips don't show, log out and back in once.".into())
            } else {
                Err("The rule wasn't saved.".into())
            }
        }
        // 126: the password prompt was closed. 127: not allowed.
        Ok(s) if matches!(s.code(), Some(126 | 127)) => Err("Not allowed. Blip still needs your OK to see clicks.".into()),
        Ok(_) => Err("Couldn't install the rule.".into()),
        Err(_) => Err("This system has no password prompt for apps (pkexec). Install polkit, or install Blip's .deb or .rpm.".into()),
    }
}
