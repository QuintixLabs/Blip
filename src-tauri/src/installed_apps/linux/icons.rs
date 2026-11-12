// ============================================================
// installed_apps/linux/icons.rs
// App icons from the icon themes.
// ============================================================

use std::path::{Path, PathBuf};

use super::data_dirs;
use crate::installed_apps::base64;

const THEMES: [&str; 5] = ["hicolor", "breeze", "breeze-dark", "Papirus", "Adwaita"];
const SIZES: [&str; 7] = ["48x48", "64x64", "128x128", "256x256", "scalable", "32x32", "512x512"];

/// The icon named in a .desktop file, as a data URL.
pub(super) fn data(name: &str) -> Option<String> {
    let file = find(name)?;
    let kind = match file.extension()?.to_str()? {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    let bytes = std::fs::read(&file).ok().filter(|b| b.len() < 512 * 1024)?;
    Some(format!("data:{kind};base64,{}", base64(&bytes)))
}

fn find(name: &str) -> Option<PathBuf> {
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
