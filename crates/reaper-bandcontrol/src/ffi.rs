//! Minimal REAPER API bindings. All `unsafe` of the extension lives here.
//!
//! Function pointers are resolved by name through `GetFunc` once at load time. Every wrapper must only be called on
//! REAPER's main thread (plugin entry, timer and action callbacks), which is also the only place where the raw track
//! pointers handed out by [`Track`] are valid; they are never kept across callbacks.
#![allow(unsafe_code, non_snake_case)]

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::{NonNull, null, null_mut},
    sync::OnceLock,
};

pub type RegisterFn = unsafe extern "C" fn(name: *const c_char, info: *mut c_void) -> c_int;
pub type GetFuncFn = unsafe extern "C" fn(name: *const c_char) -> *mut c_void;

pub const REAPER_PLUGIN_VERSION: c_int = 0x20E;

/// `reaper_plugin_info_t`.
#[repr(C)]
#[derive(Debug)]
pub struct PluginInfo {
    pub caller_version: c_int,
    pub hwnd_main: *mut c_void,
    pub register: Option<RegisterFn>,
    pub get_func: Option<GetFuncFn>,
}

/// `custom_action_register_t`.
#[repr(C)]
struct CustomAction {
    section: c_int,
    id: *const c_char,
    name: *const c_char,
    extra: *mut c_void,
}

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        struct Functions {
            $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Functions {
            /// # Safety
            /// `get_func` must be REAPER's `GetFunc`, and the declared signatures must match `reaper_plugin_functions.h`.
            unsafe fn load(get_func: GetFuncFn) -> Result<Self, &'static str> {
                Ok(Self {
                    $($name: {
                        // SAFETY: `get_func` is REAPER's lookup function, it takes a NUL terminated name.
                        let p = unsafe { get_func(concat!(stringify!($name), "\0").as_ptr().cast()) };
                        if p.is_null() {
                            return Err(stringify!($name));
                        }
                        // SAFETY: REAPER returns a function pointer with the documented signature, which is the one
                        // declared here.
                        unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p) }
                    },)*
                })
            }
        }
    };
}

api! {
    CountTracks: fn(*mut c_void) -> c_int;
    GetTrack: fn(*mut c_void, c_int) -> *mut c_void;
    GetTrackGUID: fn(*mut c_void) -> *const [u8; 16];
    GetTrackName: fn(*mut c_void, *mut c_char, c_int) -> bool;
    GetParentTrack: fn(*mut c_void) -> *mut c_void;
    Track_GetPeakInfo: fn(*mut c_void, c_int) -> f64;
    GetMediaTrackInfo_Value: fn(*mut c_void, *const c_char) -> f64;
    SetMediaTrackInfo_Value: fn(*mut c_void, *const c_char, f64) -> bool;
    GetTrackNumSends: fn(*mut c_void, c_int) -> c_int;
    GetTrackSendInfo_Value: fn(*mut c_void, c_int, c_int, *const c_char) -> f64;
    SetTrackSendInfo_Value: fn(*mut c_void, c_int, c_int, *const c_char, f64) -> bool;
    GetSetTrackSendInfo: fn(*mut c_void, c_int, c_int, *const c_char, *mut c_void) -> *mut c_void;
    GetPlayState: fn() -> c_int;
    GetPlayPosition: fn() -> f64;
    GetCursorPosition: fn() -> f64;
    GetSetRepeat: fn(c_int) -> c_int;
    Main_OnCommand: fn(c_int, c_int);
    CountProjectMarkers: fn(*mut c_void, *mut c_int, *mut c_int) -> c_int;
    EnumProjectMarkers3: fn(*mut c_void, c_int, *mut bool, *mut f64, *mut f64, *mut *const c_char, *mut c_int, *mut c_int) -> c_int;
    GetSet_LoopTimeRange2: fn(*mut c_void, bool, bool, *mut f64, *mut f64, bool);
    SetEditCurPos2: fn(*mut c_void, f64, bool, bool);
    GetProjExtState: fn(*mut c_void, *const c_char, *const c_char, *mut c_char, c_int) -> c_int;
    SetProjExtState: fn(*mut c_void, *const c_char, *const c_char, *const c_char) -> c_int;
    GetExtState: fn(*const c_char, *const c_char) -> *const c_char;
    SetExtState: fn(*const c_char, *const c_char, *const c_char, bool);
    GetProjectStateChangeCount: fn(*mut c_void) -> c_int;
    EnumProjects: fn(c_int, *mut c_char, c_int) -> *mut c_void;
    ShowConsoleMsg: fn(*const c_char);
    ShowMessageBox: fn(*const c_char, *const c_char, c_int) -> c_int;
    GetUserInputs: fn(*const c_char, c_int, *const c_char, *mut c_char, c_int) -> bool;
    GetResourcePath: fn() -> *const c_char;
}

pub struct Reaper {
    f: Functions,
    register: RegisterFn,
}

impl std::fmt::Debug for Reaper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reaper")
    }
}

static REAPER: OnceLock<Reaper> = OnceLock::new();

/// Opaque `MediaTrack*`, only valid during the current main thread callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Track(NonNull<c_void>);

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerInfo {
    pub is_region: bool,
    pub pos: f64,
    pub end: f64,
    pub name: String,
    pub number: i32,
    pub color: i32,
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// Initializes the bindings from the plugin entry arguments.
///
/// # Safety
/// `info` must be the pointer REAPER passed to `ReaperPluginEntry`.
pub unsafe fn init(info: *const PluginInfo) -> Result<&'static Reaper, String> {
    // SAFETY: guaranteed by the caller.
    let info = unsafe { info.as_ref() }.ok_or("no plugin info")?;
    if info.caller_version != REAPER_PLUGIN_VERSION {
        return Err(format!("unsupported plugin API version {:#x}", info.caller_version));
    }
    let (Some(get_func), Some(register)) = (info.get_func, info.register) else {
        return Err("REAPER did not provide GetFunc/Register".into());
    };
    // SAFETY: `get_func` comes from REAPER and the declared signatures match the SDK headers.
    let f = unsafe { Functions::load(get_func) }.map_err(|name| format!("missing REAPER API function {name}"))?;
    Ok(REAPER.get_or_init(|| Reaper { f, register }))
}

impl Reaper {
    /// The instance initialized by [`init`].
    pub fn get() -> Option<&'static Self> {
        REAPER.get()
    }

    /// `plugin_register` with a function pointer as info struct (`timer`, `hookcommand2`, `-timer`, ...).
    pub fn register_fn(&self, name: &str, callback: *const c_void) -> i32 {
        let name = cstring(name);
        // SAFETY: REAPER copies the name; the callback is a `'static` extern "C" function with the signature REAPER
        // expects for `name`, guaranteed by the call sites.
        unsafe { (self.register)(name.as_ptr(), callback.cast_mut()) }
    }

    /// Registers a custom action in the main section, returns its command id (0 on failure).
    pub fn register_action(&self, id: &str, name: &str) -> i32 {
        // REAPER keeps referring to the strings, so they live for the rest of the process.
        let id: &'static CStr = Box::leak(cstring(id).into_boxed_c_str());
        let name: &'static CStr = Box::leak(cstring(name).into_boxed_c_str());
        let action: &'static mut CustomAction = Box::leak(Box::new(CustomAction {
            section: 0,
            id: id.as_ptr(),
            name: name.as_ptr(),
            extra: null_mut(),
        }));
        let kind = cstring("custom_action");
        // SAFETY: `action` matches `custom_action_register_t`, it and its strings are `'static`.
        unsafe { (self.register)(kind.as_ptr(), std::ptr::from_mut(action).cast()) }
    }

    pub fn tracks(&self) -> impl Iterator<Item = Track> + '_ {
        // SAFETY: a null project means the current project.
        let count = unsafe { (self.f.CountTracks)(null_mut()) };
        // SAFETY: index is in range, null project means the current project.
        (0..count).filter_map(move |i| NonNull::new(unsafe { (self.f.GetTrack)(null_mut(), i) }).map(Track))
    }

    pub fn track_guid(&self, track: Track) -> [u8; 16] {
        // SAFETY: `track` is a live track pointer (module invariant); REAPER returns a pointer to its GUID.
        let guid = unsafe { (self.f.GetTrackGUID)(track.0.as_ptr()) };
        // SAFETY: non-null pointers returned by GetTrackGUID point to 16 readable bytes.
        unsafe { guid.as_ref() }.copied().unwrap_or_default()
    }

    pub fn track_name(&self, track: Track) -> String {
        let mut buf = [0 as c_char; 512];
        // SAFETY: `track` is live, the buffer size is passed along.
        let ok = unsafe { (self.f.GetTrackName)(track.0.as_ptr(), buf.as_mut_ptr(), 512) };
        if !ok {
            return String::new();
        }
        // SAFETY: REAPER NUL terminates within the buffer.
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    }

    /// The folder track containing `track`.
    pub fn parent_track(&self, track: Track) -> Option<Track> {
        // SAFETY: `track` is live.
        NonNull::new(unsafe { (self.f.GetParentTrack)(track.0.as_ptr()) }).map(Track)
    }

    /// Current meter peak of `channel` as linear amplitude.
    pub fn track_peak(&self, track: Track, channel: i32) -> f64 {
        // SAFETY: `track` is live, out of range channels read as 0.
        unsafe { (self.f.Track_GetPeakInfo)(track.0.as_ptr(), channel) }
    }

    pub fn track_info(&self, track: Track, param: &CStr) -> f64 {
        // SAFETY: `track` is live, `param` is NUL terminated.
        unsafe { (self.f.GetMediaTrackInfo_Value)(track.0.as_ptr(), param.as_ptr()) }
    }

    pub fn set_track_info(&self, track: Track, param: &CStr, value: f64) {
        // SAFETY: `track` is live, `param` is NUL terminated.
        unsafe { (self.f.SetMediaTrackInfo_Value)(track.0.as_ptr(), param.as_ptr(), value) };
    }

    /// Number of sends (`category` 0), receives (-1) or hardware outputs (1).
    pub fn send_count(&self, track: Track, category: i32) -> i32 {
        // SAFETY: `track` is live.
        unsafe { (self.f.GetTrackNumSends)(track.0.as_ptr(), category) }
    }

    pub fn send_info(&self, track: Track, category: i32, index: i32, param: &CStr) -> f64 {
        // SAFETY: `track` is live, out of range indices return 0.
        unsafe { (self.f.GetTrackSendInfo_Value)(track.0.as_ptr(), category, index, param.as_ptr()) }
    }

    pub fn set_send_info(&self, track: Track, category: i32, index: i32, param: &CStr, value: f64) {
        // SAFETY: `track` is live, out of range indices are ignored by REAPER.
        unsafe { (self.f.SetTrackSendInfo_Value)(track.0.as_ptr(), category, index, param.as_ptr(), value) };
    }

    /// Source track of receive `index`.
    pub fn receive_source(&self, track: Track, index: i32) -> Option<Track> {
        // SAFETY: `track` is live; querying P_SRCTRACK with a null new value only reads the pointer.
        let p =
            unsafe { (self.f.GetSetTrackSendInfo)(track.0.as_ptr(), -1, index, c"P_SRCTRACK".as_ptr(), null_mut()) };
        NonNull::new(p).map(Track)
    }

    pub fn play_state(&self) -> i32 {
        // SAFETY: no arguments.
        unsafe { (self.f.GetPlayState)() }
    }

    pub fn play_position(&self) -> f64 {
        // SAFETY: no arguments.
        unsafe { (self.f.GetPlayPosition)() }
    }

    pub fn cursor_position(&self) -> f64 {
        // SAFETY: no arguments.
        unsafe { (self.f.GetCursorPosition)() }
    }

    pub fn repeat(&self) -> bool {
        // SAFETY: -1 queries without changing.
        unsafe { (self.f.GetSetRepeat)(-1) != 0 }
    }

    pub fn command(&self, id: i32) {
        // SAFETY: unknown command ids are ignored by REAPER.
        unsafe { (self.f.Main_OnCommand)(id, 0) };
    }

    pub fn markers(&self) -> Vec<MarkerInfo> {
        // SAFETY: null project means current; null out pointers are allowed.
        let total = unsafe { (self.f.CountProjectMarkers)(null_mut(), null_mut(), null_mut()) };
        (0..total)
            .filter_map(|i| {
                let mut m = MarkerInfo {
                    is_region: false,
                    pos: 0.0,
                    end: 0.0,
                    name: String::new(),
                    number: 0,
                    color: 0,
                };
                let mut name: *const c_char = null();
                // SAFETY: all out pointers are valid for writes; `i` is in range.
                let next = unsafe {
                    (self.f.EnumProjectMarkers3)(
                        null_mut(),
                        i,
                        &raw mut m.is_region,
                        &raw mut m.pos,
                        &raw mut m.end,
                        &raw mut name,
                        &raw mut m.number,
                        &raw mut m.color,
                    )
                };
                if next == 0 {
                    return None;
                }
                if !name.is_null() {
                    // SAFETY: REAPER returns a NUL terminated string valid until the project changes.
                    m.name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
                }
                Some(m)
            })
            .collect()
    }

    pub fn set_time_selection(&self, start: f64, end: f64) {
        let (mut s, mut e) = (start, end);
        // SAFETY: valid pointers to the new range, current project.
        unsafe { (self.f.GetSet_LoopTimeRange2)(null_mut(), true, false, &raw mut s, &raw mut e, false) };
    }

    pub fn set_cursor(&self, pos: f64) {
        // SAFETY: current project.
        unsafe { (self.f.SetEditCurPos2)(null_mut(), pos, true, true) };
    }

    pub fn proj_ext_state(&self, section: &CStr, key: &CStr) -> String {
        let mut buf = vec![0 as c_char; 16 * 1024];
        let len = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: buffer size passed along, strings NUL terminated.
        unsafe { (self.f.GetProjExtState)(null_mut(), section.as_ptr(), key.as_ptr(), buf.as_mut_ptr(), len) };
        // SAFETY: REAPER NUL terminates within the buffer (it is zero initialized as well).
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    }

    pub fn set_proj_ext_state(&self, section: &CStr, key: &CStr, value: &str) {
        let value = cstring(value);
        // SAFETY: NUL terminated strings, current project.
        unsafe { (self.f.SetProjExtState)(null_mut(), section.as_ptr(), key.as_ptr(), value.as_ptr()) };
    }

    pub fn ext_state(&self, section: &CStr, key: &CStr) -> Option<String> {
        // SAFETY: NUL terminated strings.
        let p = unsafe { (self.f.GetExtState)(section.as_ptr(), key.as_ptr()) };
        if p.is_null() {
            return None;
        }
        // SAFETY: REAPER returns a NUL terminated string.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        (!s.is_empty()).then_some(s)
    }

    pub fn set_ext_state(&self, section: &CStr, key: &CStr, value: &str) {
        let value = cstring(value);
        // SAFETY: NUL terminated strings; `true` persists to reaper-extstate.ini.
        unsafe { (self.f.SetExtState)(section.as_ptr(), key.as_ptr(), value.as_ptr(), true) };
    }

    /// Changes whenever the current project changes (undo state count + project identity).
    pub fn change_token(&self) -> u64 {
        // SAFETY: -1 is the current project, null buffer is allowed.
        let project = unsafe { (self.f.EnumProjects)(-1, null_mut(), 0) };
        // SAFETY: null project means current.
        let count = unsafe { (self.f.GetProjectStateChangeCount)(null_mut()) };
        (project as u64).rotate_left(32) ^ u64::from(count.cast_unsigned())
    }

    pub fn console(&self, msg: &str) {
        let msg = cstring(msg);
        // SAFETY: NUL terminated string.
        unsafe { (self.f.ShowConsoleMsg)(msg.as_ptr()) };
    }

    /// Message box; `kind` 0 = OK, 4 = Yes/No (returns 6 for yes).
    pub fn message_box(&self, msg: &str, title: &str, kind: i32) -> i32 {
        let (msg, title) = (cstring(msg), cstring(title));
        // SAFETY: NUL terminated strings.
        unsafe { (self.f.ShowMessageBox)(msg.as_ptr(), title.as_ptr(), kind) }
    }

    /// Asks for one line of text; `None` when cancelled. Commas are not allowed, they separate fields.
    pub fn user_input(&self, title: &str, caption: &str, initial: &str) -> Option<String> {
        const SIZE: usize = 512;
        let (title, caption) = (cstring(title), cstring(caption));
        let mut buf = vec![0 as c_char; SIZE];
        for (dst, src) in buf.iter_mut().zip(initial.replace(',', " ").bytes().take(SIZE - 1)) {
            *dst = c_char::from_ne_bytes([src]);
        }
        // SAFETY: NUL terminated strings, the buffer size is passed along and REAPER NUL terminates within it.
        let ok = unsafe {
            (self.f.GetUserInputs)(
                title.as_ptr(),
                1,
                caption.as_ptr(),
                buf.as_mut_ptr(),
                c_int::try_from(SIZE).ok()?,
            )
        };
        // SAFETY: NUL terminated within the buffer, see above.
        ok.then(|| unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }

    pub fn resource_path(&self) -> String {
        // SAFETY: REAPER returns a static NUL terminated string.
        let p = unsafe { (self.f.GetResourcePath)() };
        if p.is_null() {
            return String::new();
        }
        // SAFETY: non-null, NUL terminated.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}
