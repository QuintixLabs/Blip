// ============================================================
// windows/overlay.rs
// The see-through windows the blips play in, one frame per screen refresh.
// ============================================================

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use tiny_skia::Pixmap;
use windows::{
    core::w,
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
        Graphics::{
            Dwm::DwmFlush,
            Gdi::{
                CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER,
                BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, RegisterClassW, SetWindowPos, ShowWindow, TranslateMessage,
            UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
            ULW_ALPHA, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
            WS_POPUP,
        },
    },
};

use crate::blip::{shared::draw, shared::style::Style, Button};

use super::clicks::scale_at;

/// Most blips at once; a new click past that reuses the oldest window.
const MAX_BLIPS: usize = 16;

// ------------------------------------------------------------
// Showing blips
// ------------------------------------------------------------

struct Slot {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    /// The bitmap's pixels (BGRA, premultiplied), side × side.
    bits: *mut u8,
    side: u32,
    pixmap: Pixmap,
    playing: Option<Playing>,
}

struct Playing {
    style: Style,
    started: Instant,
    at: POINT,
    scale: f32,
}

pub(super) struct Blips {
    slots: Vec<Slot>,
    screen: HDC,
}

impl Blips {
    pub(super) fn new() -> Self {
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

    pub(super) fn run(mut self, clicks: Receiver<(POINT, Button)>) {
        loop {
            self.pump_messages();
            let playing = self.slots.iter().any(|s| s.playing.is_some());
            if playing {
                while let Ok((at, button)) = clicks.try_recv() {
                    self.play(at, button);
                }
                self.draw_frame();
                // Wait for the next screen refresh, so blips move smoothly.
                if unsafe { DwmFlush() }.is_err() {
                    thread::sleep(Duration::from_millis(8));
                }
            } else {
                match clicks.recv_timeout(Duration::from_millis(250)) {
                    Ok((at, button)) => self.play(at, button),
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

    fn play(&mut self, click: POINT, button: Button) {
        let Some(style) = crate::blip::style_for(button) else { return };
        let scale = scale_at(click) as f32;
        // Room for the biggest the blip gets, plus a pixel of soft edge all round.
        let side = ((style.largest() * scale).ceil() as u32 + 4).max(8);
        let Some(index) = self.free_slot(side) else { return };
        let half = (side / 2) as i32;
        self.slots[index].playing =
            Some(Playing { style, started: Instant::now(), at: POINT { x: click.x - half, y: click.y - half }, scale });
        self.draw_slot(index, true);
    }

    fn free_slot(&mut self, side: u32) -> Option<usize> {
        let index = match self.slots.iter().position(|s| s.playing.is_none()) {
            Some(i) => i,
            None if self.slots.len() < MAX_BLIPS => {
                let slot = Slot::new(self.screen, side)?;
                self.slots.push(slot);
                self.slots.len() - 1
            }
            None => self.slots.iter().enumerate().min_by_key(|(_, s)| s.playing.as_ref().map(|p| p.started)).map(|(i, _)| i)?,
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
            let blend =
                BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
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
