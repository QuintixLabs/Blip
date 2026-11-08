// ============================================================
// windows.rs
// Windows: watches left clicks everywhere, checks the rules, and
// shows each blip in a small see-through window that clicks pass
// through.
// ============================================================

// Two threads:
//   clicks  holds the mouse hook. Windows waits for it on every mouse event,
//           so it only notes the click and returns straight away.
//   blips   owns the blip windows and animates them, one frame per screen
//           refresh, then sleeps until the next click.

use std::{
    cell::RefCell,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use tiny_skia::Pixmap;
use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::{
            Dwm::DwmFlush,
            Gdi::{
                CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, MonitorFromPoint, ReleaseDC, SelectObject,
                AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
                HGDIOBJ, MONITOR_DEFAULTTONEAREST,
            },
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN},
            WindowsAndMessaging::{
                CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetAncestor, GetClassNameW, GetClipCursor,
                GetCursorInfo, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId, LoadCursorW, PeekMessageW, RegisterClassW, SetWindowPos,
                SetWindowsHookExW, ShowWindow, TranslateMessage, UpdateLayeredWindow, WindowFromPoint, CURSORINFO,
                CURSOR_SHOWING, GA_ROOT, HCURSOR, HWND_TOPMOST, IDC_HAND, IDC_IBEAM, MSG, MSLLHOOKSTRUCT, PM_REMOVE,
                SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
                ULW_ALPHA, WH_MOUSE_LL, WINDOW_EX_STYLE, WM_LBUTTONDOWN, WM_LBUTTONUP, WNDCLASSW, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
};

use super::{draw, style::Style};

/// Moving further than this (at 100 % scaling) while the button is down is a
/// drag, not a click: no blip.
const DRAG_PX: f64 = 40.0;
/// Most blips at once; a new click past that reuses the oldest window.
const MAX_BLIPS: usize = 16;

static CLICKS: OnceLock<Mutex<Sender<POINT>>> = OnceLock::new();

pub fn start() {
    let (tx, rx) = mpsc::channel();
    if CLICKS.set(Mutex::new(tx)).is_err() {
        return; // already running
    }
    thread::Builder::new().name("blips".into()).spawn(move || Blips::new().run(rx)).ok();
    thread::Builder::new().name("clicks".into()).spawn(watch_clicks).ok();
}

pub fn unsupported_reason() -> Option<String> {
    None
}

// ------------------------------------------------------------
// Watching clicks
// ------------------------------------------------------------

/// The click being held down right now.
struct Press {
    at: POINT,
    /// A rule said no when the button went down.
    skip: bool,
}

thread_local! {
    static PRESS: RefCell<Option<Press>> = const { RefCell::new(None) };
}

fn watch_clicks() {
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
    if code >= 0 && super::active() {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                let skip = skipped_on_press();
                PRESS.with(|p| *p.borrow_mut() = Some(Press { at: info.pt, skip }));
            }
            WM_LBUTTONUP => {
                if let Some(press) = PRESS.with(|p| p.borrow_mut().take()) {
                    if !press.skip && !dragged(press.at, info.pt) && !skipped_on_release(info.pt) {
                        if let Some(tx) = CLICKS.get() {
                            let _ = tx.lock().unwrap().send(info.pt);
                        }
                    }
                }
            }
            _ => {}
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

fn skipped_on_press() -> bool {
    let rules = super::rules();
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
    super::rules().taskbar && on_system_ui(at)
}

/// The cursor showing right now; None when it's hidden.
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

/// The file name of the program that owns `hwnd`, lowercase ("searchhost.exe").
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
fn scale_at(at: POINT) -> f64 {
    unsafe {
        let monitor = MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        x.max(48) as f64 / 96.0
    }
}

// ------------------------------------------------------------
// Showing blips
// ------------------------------------------------------------

/// One see-through window and the picture it shows.
struct Slot {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    /// The bitmap's pixels (BGRA, premultiplied), side × side.
    bits: *mut u8,
    side: u32,
    pixmap: Pixmap,
    /// The blip playing in it, if any.
    playing: Option<Playing>,
}

struct Playing {
    style: Style,
    started: Instant,
    /// Top left of the window on screen.
    at: POINT,
    scale: f32,
}

struct Blips {
    slots: Vec<Slot>,
    screen: HDC,
}

impl Blips {
    fn new() -> Self {
        unsafe {
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
                lpszClassName: w!("BlipOverlay"),
                ..Default::default()
            };
            RegisterClassW(&class);
            Self { slots: Vec::new(), screen: GetDC(None) }
        }
    }

    fn run(mut self, clicks: Receiver<POINT>) {
        loop {
            self.pump_messages();
            let playing = self.slots.iter().any(|s| s.playing.is_some());
            if playing {
                while let Ok(at) = clicks.try_recv() {
                    self.play(at);
                }
                self.draw_frame();
                // Wait for the next screen refresh, so blips move smoothly.
                if unsafe { DwmFlush() }.is_err() {
                    thread::sleep(Duration::from_millis(8));
                }
            } else {
                match clicks.recv_timeout(Duration::from_millis(250)) {
                    Ok(at) => self.play(at),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }

    /// The blip windows never get input, but Windows still expects the thread
    /// that owns them to look at its messages now and then.
    fn pump_messages(&self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Starts a blip centered on `click`.
    fn play(&mut self, click: POINT) {
        let style = super::style();
        let scale = scale_at(click) as f32;
        // Room for the biggest the blip gets, plus a pixel of soft edge all round.
        let side = ((style.largest() * scale).ceil() as u32 + 4).max(8);
        let Some(index) = self.free_slot(side) else { return };
        let half = (side / 2) as i32;
        self.slots[index].playing =
            Some(Playing { style, started: Instant::now(), at: POINT { x: click.x - half, y: click.y - half }, scale });
        self.draw_slot(index, true);
    }

    /// A window with room for `side` px, reusing the oldest blip when all are busy.
    fn free_slot(&mut self, side: u32) -> Option<usize> {
        let index = match self.slots.iter().position(|s| s.playing.is_none()) {
            Some(i) => i,
            None if self.slots.len() < MAX_BLIPS => {
                let slot = Slot::new(self.screen, side)?;
                self.slots.push(slot);
                self.slots.len() - 1
            }
            None => self
                .slots
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.playing.as_ref().map(|p| p.started))
                .map(|(i, _)| i)?,
        };
        if self.slots[index].side != side {
            self.slots[index].resize(self.screen, side)?;
        }
        Some(index)
    }

    fn draw_frame(&mut self) {
        for index in 0..self.slots.len() {
            if self.slots[index].playing.is_some() {
                self.draw_slot(index, false);
            }
        }
    }

    fn draw_slot(&mut self, index: usize, first: bool) {
        let screen = self.screen;
        let slot = &mut self.slots[index];
        let Some(playing) = &slot.playing else { return };
        match playing.style.at(playing.started.elapsed()) {
            Some((size, opacity)) => {
                draw::frame(&mut slot.pixmap, &playing.style, size * playing.scale, opacity);
                let at = playing.at;
                slot.show(screen, at, first);
            }
            None => {
                slot.playing = None;
                unsafe {
                    let _ = ShowWindow(slot.hwnd, SW_HIDE);
                }
            }
        }
    }
}

/// Blip windows never get input; everything goes to Windows' default handling.
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

impl Slot {
    fn new(screen: HDC, side: u32) -> Option<Self> {
        unsafe {
            let style = WINDOW_EX_STYLE(WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0);
            let hwnd = CreateWindowExW(
                style,
                w!("BlipOverlay"),
                w!("Blip"),
                WS_POPUP,
                0,
                0,
                side as i32,
                side as i32,
                None,
                None,
                Some(GetModuleHandleW(None).ok()?.into()),
                None,
            )
            .ok()?;
            let mut slot = Slot {
                hwnd,
                dc: CreateCompatibleDC(Some(screen)),
                bitmap: HBITMAP::default(),
                bits: std::ptr::null_mut(),
                side: 0,
                pixmap: Pixmap::new(side, side)?,
                playing: None,
            };
            slot.resize(screen, side)?;
            Some(slot)
        }
    }

    /// A new picture size: new bitmap and pixmap.
    fn resize(&mut self, screen: HDC, side: u32) -> Option<()> {
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: side as i32,
                    biHeight: -(side as i32), // top-down rows, like the pixmap
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let old = SelectObject(self.dc, HGDIOBJ(bitmap.0));
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            } else {
                let _ = old;
            }
            self.bitmap = bitmap;
            self.bits = bits as *mut u8;
            self.side = side;
            self.pixmap = Pixmap::new(side, side)?;
            Some(())
        }
    }

    /// Puts the pixmap on screen at `at` (the window's top left).
    fn show(&mut self, screen: HDC, at: POINT, first: bool) {
        unsafe {
            // tiny-skia gives premultiplied RGBA; the bitmap wants premultiplied BGRA.
            let src = self.pixmap.data();
            let dst = std::slice::from_raw_parts_mut(self.bits, src.len());
            for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
            let size = SIZE { cx: self.side as i32, cy: self.side as i32 };
            let origin = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&at),
                Some(&size),
                Some(self.dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            if first {
                // Above everything, including other always-on-top windows.
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
    }
}

impl Drop for Blips {
    fn drop(&mut self) {
        unsafe {
            for slot in &self.slots {
                let _ = DeleteDC(slot.dc);
                let _ = DeleteObject(HGDIOBJ(slot.bitmap.0));
            }
            ReleaseDC(None, self.screen);
        }
    }
}
