//! The pairing window on Windows: a plain Win32 window, drawn with GDI into an off-screen bitmap.
#![allow(unsafe_code)]

use std::{ffi::c_void, sync::OnceLock};

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC,
            CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DRAW_TEXT_FORMAT, DT_CENTER,
            DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint,
            FW_BOLD, FW_NORMAL, FillRect, HDC, HGDIOBJ, InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY,
            SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
                DestroyWindow, GetClientRect, GetWindowRect, HMENU, IDC_ARROW, LoadCursorW, MINMAXINFO, RegisterClassW,
                SW_SHOW, SWP_NOMOVE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow, WINDOW_EX_STYLE,
                WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_LBUTTONUP, WM_PAINT, WM_SIZE, WNDCLASSW,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, w},
};

use super::{MIN_SIZE, Op, Rgb};

const CLASS: PCWSTR = w!("BandControlPairing");

fn colorref(c: Rgb) -> COLORREF {
    COLORREF(u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[allow(clippy::cast_precision_loss)]
fn scale(hwnd: HWND) -> f32 {
    // SAFETY: `hwnd` is a live window; 0 (failure) falls back to 96 dpi.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

#[allow(clippy::cast_possible_truncation)]
fn scaled((w, h): (f32, f32), scale: f32) -> (i32, i32) {
    ((w * scale).round() as i32, (h * scale).round() as i32)
}

fn instance() -> HINSTANCE {
    // SAFETY: the module handle of the REAPER executable.
    unsafe { GetModuleHandleW(PCWSTR::null()) }
        .map(Into::into)
        .unwrap_or_default()
}

fn register_class() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance(),
            // SAFETY: a system cursor.
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        // SAFETY: `class` is fully initialized and its strings are static.
        unsafe { RegisterClassW(&raw const class) != 0 }
    })
}

/// The open window; only used on the main thread.
#[derive(Debug)]
pub struct Window(HWND);

impl Window {
    pub fn create(title: &str, size: (f32, f32), parent: *mut c_void) -> Option<Self> {
        if !register_class() {
            return None;
        }
        let title = wide(title);
        // SAFETY: the class is registered; strings are NUL terminated and outlive the call. The window is owned by
        // REAPER's main window, so it stays above it.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(HWND(parent)),
                None::<HMENU>,
                Some(instance()),
                None,
            )
        }
        .ok()?;
        let (w, h) = outer(scaled(size, scale(hwnd)));
        let mut owner = RECT::default();
        // SAFETY: `parent` is REAPER's main window, `owner` is writable.
        let centered = !parent.is_null() && unsafe { GetWindowRect(HWND(parent), &raw mut owner) }.is_ok();
        let (x, y) = (
            owner.left.midpoint(owner.right) - w / 2,
            owner.top.midpoint(owner.bottom) - h / 2,
        );
        let flags = if centered {
            SWP_NOZORDER
        } else {
            SWP_NOMOVE | SWP_NOZORDER
        };
        // SAFETY: `hwnd` was just created.
        unsafe {
            let _ = SetWindowPos(hwnd, None, x, y, w, h, flags);
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        Some(Self(hwnd))
    }

    pub fn invalidate(&self) {
        // SAFETY: the window is alive while `Window` exists (it is dropped from the state on WM_DESTROY).
        let _ = unsafe { InvalidateRect(Some(self.0), None, false) };
    }

    pub fn raise(&self) {
        // SAFETY: see `invalidate`.
        unsafe {
            let _ = ShowWindow(self.0, SW_SHOW);
            let _ = SetForegroundWindow(self.0);
        }
    }

    pub fn destroy(self) {
        // SAFETY: see `invalidate`.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

/// The window size for a client area of `size`.
fn outer((w, h): (i32, i32)) -> (i32, i32) {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    // SAFETY: `r` is writable; the style is the one the window was created with.
    let _ = unsafe { AdjustWindowRectEx(&raw mut r, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE::default()) };
    (r.right - r.left, r.bottom - r.top)
}

fn client(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: `hwnd` is the window the message is for.
    let _ = unsafe { GetClientRect(hwnd, &raw mut r) };
    r
}

fn draw(dc: HDC, ops: Vec<Op>) {
    // SAFETY: `dc` is a valid device context; every GDI object created here is deselected and deleted again, strings
    // are NUL terminated and rectangles writable.
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        for op in ops {
            match op {
                Op::Fill(rect, c) => {
                    let brush = CreateSolidBrush(colorref(c));
                    let r = RECT {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    };
                    FillRect(dc, &raw const r, brush);
                    let _ = DeleteObject(HGDIOBJ::from(brush));
                }
                Op::Text {
                    rect,
                    text,
                    font,
                    color,
                    center,
                } => {
                    let face = if font.mono { w!("Consolas") } else { w!("Segoe UI") };
                    let weight = if font.bold { FW_BOLD } else { FW_NORMAL };
                    let handle = CreateFontW(
                        -font.size,
                        0,
                        0,
                        0,
                        i32::try_from(weight.0).unwrap_or(400),
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        CLEARTYPE_QUALITY,
                        u32::from(DEFAULT_PITCH.0),
                        face,
                    );
                    let old = SelectObject(dc, HGDIOBJ::from(handle));
                    SetTextColor(dc, colorref(color));
                    let mut r = RECT {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    };
                    let mut text: Vec<u16> = text.encode_utf16().collect();
                    let align = if center { DT_CENTER } else { DRAW_TEXT_FORMAT::default() };
                    DrawTextW(
                        dc,
                        &mut text,
                        &raw mut r,
                        align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
                    );
                    SelectObject(dc, old);
                    let _ = DeleteObject(HGDIOBJ::from(handle));
                }
            }
        }
    }
}

fn paint(hwnd: HWND) {
    let r = client(hwnd);
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    let ops = super::paint(width, height, scale(hwnd));
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: called for WM_PAINT of `hwnd`. The off-screen bitmap and its DC are released before EndPaint.
    unsafe {
        let dc = BeginPaint(hwnd, &raw mut ps);
        let memory = CreateCompatibleDC(Some(dc));
        let bitmap = CreateCompatibleBitmap(dc, width.max(1), height.max(1));
        let old = SelectObject(memory, HGDIOBJ::from(bitmap));
        draw(memory, ops);
        let _ = BitBlt(dc, 0, 0, width, height, Some(memory), 0, 0, SRCCOPY);
        SelectObject(memory, old);
        let _ = DeleteObject(HGDIOBJ::from(bitmap));
        let _ = DeleteDC(memory);
        let _ = EndPaint(hwnd, &raw const ps);
    }
}

#[allow(clippy::cast_possible_truncation)]
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = std::panic::catch_unwind(|| match msg {
        WM_PAINT => {
            paint(hwnd);
            true
        }
        // Everything is painted in WM_PAINT, off-screen.
        WM_ERASEBKGND => true,
        WM_SIZE => {
            // SAFETY: `hwnd` is the window the message is for.
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            false
        }
        WM_GETMINMAXINFO => {
            // SAFETY: for WM_GETMINMAXINFO, `lparam` points to the window's MINMAXINFO.
            if let Some(info) = unsafe { (lparam.0 as *mut MINMAXINFO).as_mut() } {
                let (w, h) = outer(scaled(MIN_SIZE, scale(hwnd)));
                info.ptMinTrackSize.x = w;
                info.ptMinTrackSize.y = h;
            }
            true
        }
        WM_LBUTTONUP => {
            let r = client(hwnd);
            // Client coordinates, packed as two signed 16 bit values.
            let (x, y) = (i32::from(lparam.0 as i16), i32::from((lparam.0 >> 16) as i16));
            super::clicked(r.right - r.left, r.bottom - r.top, scale(hwnd), x, y);
            true
        }
        WM_CLOSE => {
            // SAFETY: closing destroys the window, WM_DESTROY clears the state.
            let _ = unsafe { DestroyWindow(hwnd) };
            true
        }
        WM_DESTROY => {
            super::destroyed();
            false
        }
        _ => false,
    })
    .unwrap_or(false);
    if handled {
        LRESULT(isize::from(msg == WM_ERASEBKGND))
    } else {
        // SAFETY: default handling for everything else.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}
