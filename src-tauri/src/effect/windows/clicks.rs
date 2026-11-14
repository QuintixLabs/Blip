// ============================================================
// effect/windows/clicks.rs
// The mouse hook, and the rules for each click.
// ============================================================

use std::cell::RefCell;

use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST},
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN},
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetAncestor, GetClassNameW, GetClipCursor, GetCursorInfo, GetMessageW, GetSystemMetrics,
                GetWindowThreadProcessId, LoadCursorW, SetWindowsHookExW, TranslateMessage, WindowFromPoint, CURSORINFO, CURSOR_SHOWING,
                GA_ROOT, HCURSOR, IDC_HAND, IDC_IBEAM, MSG, MSLLHOOKSTRUCT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, WH_MOUSE_LL,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
            },
        },
    },
};

use crate::effect::Button;

/// Moving further than this (at 100 %) while held is a drag.
const DRAG_PX: f64 = 40.0;

// ------------------------------------------------------------
// Watching clicks (left, right and middle)
// ------------------------------------------------------------

struct Press {
    at: POINT,
    skip: bool,
}

thread_local! {
    static PRESS: RefCell<[Option<Press>; 3]> = const { RefCell::new([None, None, None]) };
}

pub(super) fn watch_clicks() {
    unsafe {
        let module = GetModuleHandleW(None).unwrap_or_default();
        if SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(HINSTANCE(module.0)), 0).is_err() {
            eprintln!("Blip: couldn't watch clicks");
            return;
        }
        // The hook runs from this loop, so it has to keep going.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && crate::effect::active() {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let (button, down) = match wparam.0 as u32 {
            WM_LBUTTONDOWN => (Some(Button::Left), true),
            WM_LBUTTONUP => (Some(Button::Left), false),
            WM_RBUTTONDOWN => (Some(Button::Right), true),
            WM_RBUTTONUP => (Some(Button::Right), false),
            WM_MBUTTONDOWN => (Some(Button::Middle), true),
            WM_MBUTTONUP => (Some(Button::Middle), false),
            _ => (None, false),
        };
        if let Some(button) = button {
            if down {
                let skip = skipped_on_press(info.pt);
                PRESS.with(|p| p.borrow_mut()[button as usize] = Some(Press { at: info.pt, skip }));
            } else if let Some(press) = PRESS.with(|p| p.borrow_mut()[button as usize].take()) {
                if !press.skip && !dragged(press.at, info.pt) && !skipped_on_release(info.pt) {
                    if let Some(tx) = super::CLICKS.get() {
                        let _ = tx.lock().unwrap().send((info.pt, button));
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn dragged(from: POINT, to: POINT) -> bool {
    let (dx, dy) = ((to.x - from.x) as f64, (to.y - from.y) as f64);
    (dx * dx + dy * dy).sqrt() > DRAG_PX * scale_at(to)
}

// ------------------------------------------------------------
// The rules
// ------------------------------------------------------------

fn skipped_on_press(at: POINT) -> bool {
    if in_skipped_app(at) {
        return true;
    }
    let rules = crate::effect::rules();
    let cursor = current_cursor();
    if rules.text_cursor && cursor.is_some_and(|c| Some(c) == system_cursor(IDC_IBEAM)) {
        return true;
    }
    if rules.link_cursor && cursor.is_some_and(|c| Some(c) == system_cursor(IDC_HAND)) {
        return true;
    }
    rules.games && in_game(cursor.is_none())
}

fn skipped_on_release(at: POINT) -> bool {
    crate::effect::rules().taskbar && on_system_ui(at)
}

fn current_cursor() -> Option<HCURSOR> {
    let mut info = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
    unsafe { GetCursorInfo(&mut info).ok()? };
    (info.flags.0 & CURSOR_SHOWING.0 != 0).then_some(info.hCursor)
}

fn system_cursor(id: PCWSTR) -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, id).ok() }
}

/// Games hide the cursor, keep it inside their window, or run in exclusive
/// full screen. Any of those: no blips.
fn in_game(cursor_hidden: bool) -> bool {
    if cursor_hidden {
        return true;
    }
    let mut clip = RECT::default();
    if unsafe { GetClipCursor(&mut clip) }.is_ok() {
        let (w, h) = (clip.right - clip.left, clip.bottom - clip.top);
        let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN)) };
        if w > 0 && h > 0 && (w < sw || h < sh) {
            return true;
        }
    }
    matches!(unsafe { SHQueryUserNotificationState() }, Ok(state) if state == QUNS_RUNNING_D3D_FULL_SCREEN)
}

/// Windows keeps these in a layer above every app, so a blip there would be
/// hidden underneath: the taskbar (any monitor), Start, search, the
/// notification and quick settings panels, the tray overflow.
const SYSTEM_UI_CLASSES: [&str; 6] = [
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Xaml_WindowedPopupClass",
    "Windows.UI.Core.CoreWindow",
];
const SYSTEM_UI_PROGRAMS: [&str; 5] =
    ["startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe", "shellexperiencehost.exe", "shellhost.exe"];

fn on_system_ui(at: POINT) -> bool {
    unsafe {
        let hwnd = WindowFromPoint(at);
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let top = if root.0.is_null() { hwnd } else { root };
        let mut name = [0u16; 128];
        let len = GetClassNameW(top, &mut name);
        let class = String::from_utf16_lossy(&name[..len.max(0) as usize]);
        SYSTEM_UI_CLASSES.contains(&class.as_str()) || program_of(top).is_some_and(|p| SYSTEM_UI_PROGRAMS.contains(&p.as_str()))
    }
}

fn in_skipped_app(at: POINT) -> bool {
    let skipped = crate::effect::skipped_apps();
    if skipped.is_empty() {
        return false;
    }
    unsafe {
        let hwnd = WindowFromPoint(at);
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let top = if root.0.is_null() { hwnd } else { root };
        program_of(top).is_some_and(|p| skipped.contains(&p))
    }
}

fn program_of(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(|name| name.to_lowercase())
    }
}

/// The screen's scaling where `at` is: 1.0 at 100 %, 1.5 at 150 %.
pub(super) fn scale_at(at: POINT) -> f64 {
    unsafe {
        let monitor = MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        x.max(48) as f64 / 96.0
    }
}
