// ============================================================
// apps/windows/icons.rs
// A program's own name and its icon.
// ============================================================

use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{Mutex, OnceLock},
};

use tiny_skia::{Pixmap, PremultipliedColorU8};
use windows::{
    core::PCWSTR,
    Win32::{
        Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ},
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        UI::{
            Shell::SHDefExtractIconW,
            WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
        },
    },
};

use super::wide;
use crate::system::apps::{base64, plain_name};

/// Sharp up to 200 % scaling in the list.
const ICON_PX: u32 = 48;

/// The program's own name ("FileDescription"), or its file name.
pub(super) fn name(path: &str) -> String {
    // Some programs give their file name as their name ("WindowsTerminal.exe").
    description(path).map(|d| plain_name(&d)).unwrap_or_else(|| plain_name(path))
}

fn description(path: &str) -> Option<String> {
    unsafe {
        let file = wide(path);
        let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;

        // Use the first language it has text for, then common fallbacks.
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let query = wide("\\VarFileInfo\\Translation");
        let mut lang = (0x0409u16, 0x04B0u16);
        if VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() && len >= 4 {
            let pair = ptr as *const u16;
            lang = (*pair, *pair.add(1));
        }
        for (l, c) in [lang, (0x0409, 0x04B0), (0x0409, 0x04E4), (0x0000, 0x04B0)] {
            let query = wide(&format!("\\StringFileInfo\\{l:04x}{c:04x}\\FileDescription"));
            if VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() && len > 1 {
                let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let text = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_owned();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

/// The icon as a PNG data URL, kept once read so the list opens quickly next time.
pub(super) fn icon(path: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(path) {
        return hit.clone();
    }
    let icon = read_icon(path);
    cache.lock().unwrap().insert(path.to_owned(), icon.clone());
    icon
}

fn read_icon(path: &str) -> Option<String> {
    unsafe {
        let file = wide(path);
        let mut large = HICON::default();
        SHDefExtractIconW(PCWSTR(file.as_ptr()), 0, 0, Some(&mut large), None, ICON_PX).ok().ok()?;
        let png = icon_png(large);
        let _ = DestroyIcon(large);
        png.map(|bytes| format!("data:image/png;base64,{}", base64(&bytes)))
    }
}

unsafe fn icon_png(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    GetIconInfo(icon, &mut info).ok()?;
    let side = ICON_PX as i32;
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            biHeight: -side,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; (side * side * 4) as usize];
    let dc = GetDC(None);
    let rows = GetDIBits(dc, info.hbmColor, 0, side as u32, Some(bgra.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
    let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    if rows == 0 {
        return None;
    }

    // Old icons have no transparency at all: treat them as fully opaque.
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    let mut pixmap = Pixmap::new(ICON_PX, ICON_PX)?;
    for (dst, p) in pixmap.pixels_mut().iter_mut().zip(bgra.chunks_exact(4)) {
        let a = if has_alpha { p[3] } else { 255 };
        let pre = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
        *dst = PremultipliedColorU8::from_rgba(pre(p[2]), pre(p[1]), pre(p[0]), a)?;
    }
    pixmap.encode_png().ok()
}
