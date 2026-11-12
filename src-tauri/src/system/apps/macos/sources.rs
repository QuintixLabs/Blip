// ============================================================
// apps/macos/sources.rs
// Where apps are found: the .app bundles and their Info.plist.
// ============================================================

use std::path::{Path, PathBuf};

use super::icons;
use crate::system::apps::AppInfo;

/// .app bundles, also one folder down ("/Applications/Adobe Photoshop 2025/…").
pub(super) fn bundles(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "app") {
                out.push(path);
            } else if path.is_dir() && depth < 1 {
                walk(&path, depth + 1, out);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, 0, &mut found);
    found
}

pub(super) fn read_bundle(bundle: &Path) -> Option<AppInfo> {
    let info = plist::Value::from_file(bundle.join("Contents/Info.plist")).ok()?;
    let info = info.as_dictionary()?;
    let field = |key: &str| info.get(key).and_then(|v| v.as_string()).filter(|s| !s.is_empty()).map(str::to_owned);
    // Matched against the program a running app shows.
    let exe = field("CFBundleExecutable")?.to_lowercase();
    let name = field("CFBundleDisplayName")
        .or_else(|| field("CFBundleName"))
        .unwrap_or_else(|| bundle.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    Some(AppInfo { name, exe, icon: icons::icon(bundle), path: Some(bundle.to_string_lossy().into_owned()) })
}
