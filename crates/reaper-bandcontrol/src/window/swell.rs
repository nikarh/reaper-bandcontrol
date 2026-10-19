//! The pairing window on Linux and macOS, through SWELL: REAPER's implementation of the Win32 window and drawing API.
//!
//! Linux REAPER hands the SWELL function lookup to [`SWELL_dllMain`] when loading the extension; on macOS it is asked
//! for through the application delegate. Function pointers are resolved by name once, on first use.
#![allow(unsafe_code, non_snake_case)]

use std::{
    cell::Cell,
    ffi::{CString, c_char, c_int, c_uint, c_void},
    ptr::{null, null_mut},
    sync::{
        OnceLock,
        atomic::{AtomicPtr, Ordering},
    },
};

use super::{Op, Rect};

type Hwnd = *mut c_void;
type Hdc = *mut c_void;
type Hgdiobj = *mut c_void;
type GetFunc = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type DlgProc = unsafe extern "C" fn(Hwnd, c_uint, usize, isize) -> isize;

const DLL_PROCESS_ATTACH: c_uint = 1;
/// `SWELL_CreateDialog` resource id for an empty dialog; `| 1` makes it resizable.
const EMPTY_RESIZABLE_DIALOG: usize = 0x40_0001;
/// What the SWELL lookup returns for a null name, to tell it from something else.
const SWELL_API_MAGIC: usize = 0x100;

const WM_DESTROY: c_uint = 0x0002;
const WM_SIZE: c_uint = 0x0005;
const WM_PAINT: c_uint = 0x000F;
const WM_CLOSE: c_uint = 0x0010;
const WM_GETMINMAXINFO: c_uint = 0x0024;
const WM_NCHITTEST: c_uint = 0x0084;
const HTCLIENT: isize = 1;
const WM_INITDIALOG: c_uint = 0x0110;
const WM_LBUTTONUP: c_uint = 0x0202;
const SW_SHOW: c_int = 2;
const SWP_NOMOVE: c_int = 1;
const SWP_NOZORDER: c_int = 4;
const TRANSPARENT: c_int = 0;
const DT_CENTER: c_int = 1;
const DT_VCENTER: c_int = 4;
const DT_SINGLELINE: c_int = 0x20;
const DT_NOPREFIX: c_int = 0x800;
const DT_END_ELLIPSIS: c_int = 0x8000;
const FW_NORMAL: c_int = 400;
const FW_BOLD: c_int = 700;

/// `RECT`; SWELL's `LONG` is 32 bits.
#[repr(C)]
#[derive(Default)]
struct RawRect {
    left: c_int,
    top: c_int,
    right: c_int,
    bottom: c_int,
}

impl From<Rect> for RawRect {
    fn from(r: Rect) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

#[repr(C)]
struct Point {
    x: c_int,
    y: c_int,
}

/// `MINMAXINFO`.
#[repr(C)]
struct MinMaxInfo {
    reserved: Point,
    max_size: Point,
    max_position: Point,
    min_track_size: Point,
    max_track_size: Point,
}

/// Room for SWELL's `PAINTSTRUCT` (a handle, a flag and a rectangle); only `BeginPaint` reads or writes it.
#[repr(C)]
struct PaintStruct([usize; 8]);

static GET_FUNC: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

/// Called by REAPER on Linux before the plugin entry, with SWELL's function lookup.
///
/// # Safety
/// Must only be called by REAPER, `get_func` must be SWELL's `SWELLAPI_GetFunc` or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SWELL_dllMain(_instance: *mut c_void, reason: c_uint, get_func: *mut c_void) -> c_int {
    if reason == DLL_PROCESS_ATTACH && !get_func.is_null() {
        GET_FUNC.store(get_func, Ordering::Release);
    }
    1
}

/// SWELL's function lookup on macOS: `[[NSApp delegate] swellGetAPPAPIFunc]`.
#[cfg(target_os = "macos")]
fn app_get_func() -> *mut c_void {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
        sel,
    };
    let Some(class) = AnyClass::get(c"NSApplication") else {
        return null_mut();
    };
    // SAFETY: plain Objective-C messages on the main thread: NSApplication's shared instance and its delegate (both
    // may be nil), and REAPER's delegate method returning SWELL's lookup function, only sent when it responds to it.
    unsafe {
        let app: *mut AnyObject = msg_send![class, sharedApplication];
        let Some(app) = app.as_ref() else { return null_mut() };
        let delegate: *mut AnyObject = msg_send![app, delegate];
        let Some(delegate) = delegate.as_ref() else {
            return null_mut();
        };
        let responds: bool = msg_send![delegate, respondsToSelector: sel!(swellGetAPPAPIFunc)];
        if !responds {
            return null_mut();
        }
        let get_func: *mut c_void = msg_send![delegate, swellGetAPPAPIFunc];
        get_func
    }
}

#[cfg(not(target_os = "macos"))]
fn app_get_func() -> *mut c_void {
    null_mut()
}

macro_rules! swell {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        struct Swell {
            $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Swell {
            /// # Safety
            /// `get_func` must be SWELL's lookup, and the signatures must match `swell-functions.h`.
            unsafe fn load(get_func: GetFunc) -> Option<Self> {
                Some(Self {
                    $($name: {
                        // SAFETY: the lookup takes a NUL terminated name.
                        let p = unsafe { get_func(concat!(stringify!($name), "\0").as_ptr().cast()) };
                        if p.is_null() {
                            tracing::warn!(function = stringify!($name), "SWELL function missing");
                            return None;
                        }
                        // SAFETY: SWELL returns the function with the signature declared here.
                        unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p) }
                    },)*
                })
            }
        }
    };
}

swell! {
    SWELL_CreateDialog: fn(*mut c_void, *const c_char, Hwnd, DlgProc, isize) -> Hwnd;
    ShowWindow: fn(Hwnd, c_int);
    DestroyWindow: fn(Hwnd);
    SetDlgItemText: fn(Hwnd, c_int, *const c_char) -> i8;
    SetForegroundWindow: fn(Hwnd);
    SetWindowPos: fn(Hwnd, Hwnd, c_int, c_int, c_int, c_int, c_int);
    GetClientRect: fn(Hwnd, *mut RawRect);
    GetWindowRect: fn(Hwnd, *mut RawRect) -> bool;
    InvalidateRect: fn(Hwnd, *const RawRect, c_int) -> i8;
    BeginPaint: fn(Hwnd, *mut PaintStruct) -> Hdc;
    EndPaint: fn(Hwnd, *mut PaintStruct) -> i8;
    SWELL_FillRect: fn(Hdc, *const RawRect, Hgdiobj);
    CreateSolidBrush: fn(c_int) -> Hgdiobj;
    CreateFont: fn(
        c_int, c_int, c_int, c_int, c_int, c_char, c_char, c_char, c_char, c_char, c_char, c_char, c_char,
        *const c_char
    ) -> Hgdiobj;
    SelectObject: fn(Hdc, Hgdiobj) -> Hgdiobj;
    DeleteObject: fn(Hgdiobj);
    SetTextColor: fn(Hdc, c_int);
    SetBkMode: fn(Hdc, c_int);
    SWELL_DrawText: fn(Hdc, *const c_char, c_int, *mut RawRect, c_int) -> c_int;
    SWELL_GetScaling256: fn() -> c_int;
}

fn swell() -> Option<&'static Swell> {
    static SWELL: OnceLock<Option<Swell>> = OnceLock::new();
    SWELL
        .get_or_init(|| {
            let mut p = GET_FUNC.load(Ordering::Acquire);
            if p.is_null() {
                p = app_get_func();
            }
            if p.is_null() {
                tracing::warn!("REAPER provided no SWELL, the pairing window is not available");
                return None;
            }
            // SAFETY: `p` is SWELL's lookup function, from REAPER.
            let get_func = unsafe { std::mem::transmute::<*mut c_void, GetFunc>(p) };
            // SAFETY: a null name asks for the API magic number.
            if unsafe { get_func(null()) } as usize != SWELL_API_MAGIC {
                tracing::warn!("unexpected SWELL API");
                return None;
            }
            // SAFETY: `get_func` is SWELL's lookup and the signatures match swell-functions.h.
            unsafe { Swell::load(get_func) }
        })
        .as_ref()
}

#[allow(clippy::cast_precision_loss)]
fn scale(s: &Swell) -> f32 {
    // SAFETY: no arguments.
    unsafe { (s.SWELL_GetScaling256)() as f32 / 256.0 }
}

#[allow(clippy::cast_possible_truncation)]
fn scaled((w, h): (f32, f32), scale: f32) -> (c_int, c_int) {
    ((w * scale).round() as c_int, (h * scale).round() as c_int)
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// SWELL's `RGB()`: red in the high byte, unlike Win32.
fn color(c: super::Rgb) -> c_int {
    (c_int::from(c.0) << 16) | (c_int::from(c.1) << 8) | c_int::from(c.2)
}

thread_local! {
    /// The minimum size before system scaling, for WM_GETMINMAXINFO.
    static MIN: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

/// The open window; only used on the main thread.
#[derive(Debug)]
pub struct Window(Hwnd);

impl Window {
    pub fn create(title: &str, size: (f32, f32), min_size: (f32, f32), parent: *mut c_void) -> Option<Self> {
        MIN.with(|m| m.set(min_size));
        let api = swell()?;
        // SAFETY: an empty resizable dialog without resources, owned by REAPER's main window; `dialog_proc` matches
        // `DLGPROC`.
        let hwnd = unsafe {
            (api.SWELL_CreateDialog)(
                null_mut(),
                EMPTY_RESIZABLE_DIALOG as *const c_char,
                parent,
                dialog_proc,
                0,
            )
        };
        if hwnd.is_null() {
            tracing::warn!("SWELL could not create the pairing window");
            return None;
        }
        let (w, h) = scaled(size, scale(api));
        let mut owner = RawRect::default();
        // SAFETY: `parent` is REAPER's main window (or null, then nothing is read), `owner` is writable.
        let centered = !parent.is_null() && unsafe { (api.GetWindowRect)(parent, &raw mut owner) };
        // Centered on REAPER's window. On macOS y grows upwards, which the same arithmetic handles.
        let (x, y) = (
            owner.left.midpoint(owner.right) - w / 2,
            owner.top.midpoint(owner.bottom) - h / 2,
        );
        let flags = if centered {
            SWP_NOZORDER
        } else {
            SWP_NOMOVE | SWP_NOZORDER
        };
        let title = cstring(title);
        // SAFETY: `hwnd` was just created; item 0 is the window itself.
        unsafe {
            (api.SetDlgItemText)(hwnd, 0, title.as_ptr());
            (api.SetWindowPos)(hwnd, null_mut(), x, y, w, h, flags);
            (api.ShowWindow)(hwnd, SW_SHOW);
        }
        Some(Self(hwnd))
    }

    pub fn invalidate(&self) {
        if let Some(s) = swell() {
            // SAFETY: the window is alive while `Window` exists (it is dropped from the state on WM_DESTROY).
            unsafe { (s.InvalidateRect)(self.0, null(), 0) };
        }
    }

    pub fn raise(&self) {
        if let Some(s) = swell() {
            // SAFETY: see `invalidate`.
            unsafe {
                (s.ShowWindow)(self.0, SW_SHOW);
                (s.SetForegroundWindow)(self.0);
            }
        }
    }

    pub fn destroy(self) {
        if let Some(s) = swell() {
            // SAFETY: see `invalidate`.
            unsafe { (s.DestroyWindow)(self.0) };
        }
    }
}

fn client_size(s: &Swell, hwnd: Hwnd) -> (c_int, c_int) {
    let mut r = RawRect::default();
    // SAFETY: `hwnd` is the window the message is for, `r` is writable.
    unsafe { (s.GetClientRect)(hwnd, &raw mut r) };
    (r.right - r.left, r.bottom - r.top)
}

fn paint(s: &Swell, hwnd: Hwnd) {
    let (width, height) = client_size(s, hwnd);
    let ops = super::paint(width, height, scale(s));
    let mut ps = PaintStruct([0; 8]);
    // SAFETY: called for WM_PAINT of `hwnd`; every GDI object created here is deselected and deleted before
    // EndPaint, strings are NUL terminated and rectangles writable.
    unsafe {
        let dc = (s.BeginPaint)(hwnd, &raw mut ps);
        if dc.is_null() {
            return;
        }
        (s.SetBkMode)(dc, TRANSPARENT);
        for op in ops {
            match op {
                Op::Fill(rect, c) => {
                    let brush = (s.CreateSolidBrush)(color(c));
                    (s.SWELL_FillRect)(dc, &RawRect::from(rect), brush);
                    (s.DeleteObject)(brush);
                }
                Op::Text {
                    rect,
                    text,
                    font,
                    color: c,
                    center,
                } => {
                    let face = if font.mono { c"Courier New" } else { c"Arial" };
                    let weight = if font.bold { FW_BOLD } else { FW_NORMAL };
                    let handle = (s.CreateFont)(-font.size, 0, 0, 0, weight, 0, 0, 0, 0, 0, 0, 0, 0, face.as_ptr());
                    let old = (s.SelectObject)(dc, handle);
                    (s.SetTextColor)(dc, color(c));
                    let text = cstring(&text);
                    let mut r = RawRect::from(rect);
                    let align = if center { DT_CENTER } else { 0 };
                    let flags = align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS;
                    (s.SWELL_DrawText)(dc, text.as_ptr(), -1, &raw mut r, flags);
                    (s.SelectObject)(dc, old);
                    (s.DeleteObject)(handle);
                }
            }
        }
        (s.EndPaint)(hwnd, &raw mut ps);
    }
}

#[allow(clippy::cast_possible_truncation)]
unsafe extern "C" fn dialog_proc(hwnd: Hwnd, msg: c_uint, _wparam: usize, lparam: isize) -> isize {
    let Some(s) = swell() else { return 0 };
    // SWELL's default for an empty dialog is no client area at all: clicks would arrive as WM_NCLBUTTON*.
    if msg == WM_NCHITTEST {
        return HTCLIENT;
    }
    let handled = std::panic::catch_unwind(|| match msg {
        WM_INITDIALOG => true,
        WM_PAINT => {
            paint(s, hwnd);
            true
        }
        WM_SIZE => {
            // SAFETY: `hwnd` is the window the message is for.
            unsafe { (s.InvalidateRect)(hwnd, null(), 0) };
            false
        }
        WM_GETMINMAXINFO => {
            // SAFETY: for WM_GETMINMAXINFO, `lparam` points to the window's MINMAXINFO.
            if let Some(info) = unsafe { (lparam as *mut MinMaxInfo).as_mut() } {
                let (w, h) = scaled(MIN.with(Cell::get), scale(s));
                info.min_track_size = Point { x: w, y: h };
            }
            true
        }
        WM_LBUTTONUP => {
            let (width, height) = client_size(s, hwnd);
            // Client coordinates, packed as two signed 16 bit values.
            let (x, y) = (c_int::from(lparam as i16), c_int::from((lparam >> 16) as i16));
            super::clicked(width, height, scale(s), x, y);
            true
        }
        WM_CLOSE => {
            // SAFETY: closing destroys the window, WM_DESTROY clears the state.
            unsafe { (s.DestroyWindow)(hwnd) };
            true
        }
        WM_DESTROY => {
            super::destroyed();
            false
        }
        _ => false,
    })
    .unwrap_or(false);
    isize::from(handled)
}
