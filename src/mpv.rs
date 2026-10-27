//! Minimal libmpv bindings, loaded at runtime.
//!
//! Loading the library dynamically means no import library or pkg-config is
//! needed at build time: on Windows you just drop `libmpv-2.dll` next to the
//! executable. libmpv itself is built on FFmpeg (decoding) and libass
//! (subtitles) — the same foundations VLC uses.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;

use libloading::Library;

// ---------------------------------------------------------------------------
// C ABI types (client.h, render.h, render_gl.h)
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct MpvRenderContext {
    _private: [u8; 0],
}

pub type Format = c_int;
pub const FORMAT_NONE: Format = 0;
pub const FORMAT_STRING: Format = 1;
pub const FORMAT_FLAG: Format = 3;
pub const FORMAT_INT64: Format = 4;
pub const FORMAT_DOUBLE: Format = 5;

pub const EVENT_NONE: c_int = 0;
pub const EVENT_SHUTDOWN: c_int = 1;
pub const EVENT_START_FILE: c_int = 6;
pub const EVENT_END_FILE: c_int = 7;
pub const EVENT_FILE_LOADED: c_int = 8;
pub const EVENT_VIDEO_RECONFIG: c_int = 17;
pub const EVENT_PROPERTY_CHANGE: c_int = 22;

pub const END_FILE_REASON_ERROR: c_int = 4;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEventProperty {
    name: *const c_char,
    format: Format,
    data: *mut c_void,
}

/// Only the leading fields we read; mpv owns the allocation.
#[repr(C)]
struct RawEventEndFile {
    reason: c_int,
    error: c_int,
}

const RENDER_PARAM_INVALID: c_int = 0;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_PARAM_OPENGL_FBO: c_int = 3;
const RENDER_PARAM_FLIP_Y: c_int = 4;
const RENDER_PARAM_ADVANCED_CONTROL: c_int = 10;
const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;

const RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

type GetProcAddressFn = unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void;

#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: GetProcAddressFn,
    get_proc_address_ctx: *mut c_void,
    // Removed in client API 2.0 but harmless to pass; older builds read it.
    extra_exts: *const c_char,
}

#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

type Callback = unsafe extern "C" fn(*mut c_void);

struct Api {
    _lib: Library,
    create: unsafe extern "C" fn() -> *mut MpvHandle,
    initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    set_option_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int,
    set_property: unsafe extern "C" fn(*mut MpvHandle, *const c_char, Format, *mut c_void) -> c_int,
    get_property: unsafe extern "C" fn(*mut MpvHandle, *const c_char, Format, *mut c_void) -> c_int,
    observe_property: unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, Format) -> c_int,
    wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut RawEvent,
    set_wakeup_callback: unsafe extern "C" fn(*mut MpvHandle, Option<Callback>, *mut c_void),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    free: unsafe extern "C" fn(*mut c_void),
    render_create: unsafe extern "C" fn(*mut *mut MpvRenderContext, *mut MpvHandle, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut MpvRenderContext, Option<Callback>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut MpvRenderContext) -> u64,
    render_render: unsafe extern "C" fn(*mut MpvRenderContext, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut MpvRenderContext),
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

#[cfg(windows)]
const LIB_NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll", "libmpv.dll", "mpv-1.dll"];
#[cfg(target_os = "macos")]
const LIB_NAMES: &[&str] = &["libmpv.2.dylib", "libmpv.dylib"];
#[cfg(all(unix, not(target_os = "macos")))]
const LIB_NAMES: &[&str] = &["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

pub fn library_hint() -> &'static str {
    LIB_NAMES[0]
}

fn candidate_paths() -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from));
    let mut out = Vec::new();
    for name in LIB_NAMES {
        if let Some(dir) = &exe_dir {
            out.push(dir.join(name));
        }
        out.push(PathBuf::from(name));
    }
    out
}

fn load_api() -> Result<Api, String> {
    let mut last_err = String::new();
    for path in candidate_paths() {
        match unsafe { Library::new(&path) } {
            Ok(lib) => return unsafe { bind(lib) },
            Err(e) => last_err = format!("{}: {e}", path.display()),
        }
    }
    Err(format!("Could not load libmpv ({last_err})"))
}

unsafe fn bind(lib: Library) -> Result<Api, String> {
    macro_rules! sym {
        ($name:literal) => {
            *unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                .map_err(|e| format!("libmpv is missing {}: {e}", $name))?
        };
    }
    Ok(Api {
        create: sym!("mpv_create"),
        initialize: sym!("mpv_initialize"),
        terminate_destroy: sym!("mpv_terminate_destroy"),
        set_option_string: sym!("mpv_set_option_string"),
        command: sym!("mpv_command"),
        set_property: sym!("mpv_set_property"),
        get_property: sym!("mpv_get_property"),
        observe_property: sym!("mpv_observe_property"),
        wait_event: sym!("mpv_wait_event"),
        set_wakeup_callback: sym!("mpv_set_wakeup_callback"),
        error_string: sym!("mpv_error_string"),
        free: sym!("mpv_free"),
        render_create: sym!("mpv_render_context_create"),
        render_set_update_callback: sym!("mpv_render_context_set_update_callback"),
        render_update: sym!("mpv_render_context_update"),
        render_render: sym!("mpv_render_context_render"),
        render_free: sym!("mpv_render_context_free"),
        _lib: lib,
    })
}

fn api() -> &'static Api {
    API.get().and_then(|r| r.as_ref().ok()).expect("libmpv not loaded")
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

fn err_str(code: c_int) -> String {
    unsafe { CStr::from_ptr((api().error_string)(code)) }.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Safe wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum PropValue {
    None,
    Flag(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

#[derive(Debug)]
pub enum Event {
    Shutdown,
    StartFile,
    FileLoaded,
    EndFile { error: Option<String> },
    VideoReconfig,
    Property { id: u64, value: PropValue },
}

pub struct Mpv {
    handle: *mut MpvHandle,
    wakeup: *mut Box<dyn Fn() + Send + Sync>,
}

// libmpv's client API is fully thread-safe.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Loads libmpv, applies `options` and initializes the core.
    pub fn new(options: &[(&str, &str)]) -> Result<Self, String> {
        API.get_or_init(load_api).as_ref().map_err(Clone::clone)?;
        let handle = unsafe { (api().create)() };
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let mpv = Mpv { handle, wakeup: ptr::null_mut() };
        for (k, v) in options {
            // Option names drift between mpv versions; an unknown one is not fatal.
            if let Err(e) = mpv.set_option(k, v) {
                eprintln!("mpv: option {k}={v} rejected: {e}");
            }
        }
        let rc = unsafe { (api().initialize)(handle) };
        if rc < 0 {
            return Err(format!("mpv_initialize failed: {}", err_str(rc)));
        }
        Ok(mpv)
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<(), String> {
        let (n, v) = (cstr(name), cstr(value));
        let rc = unsafe { (api().set_option_string)(self.handle, n.as_ptr(), v.as_ptr()) };
        if rc < 0 { Err(err_str(rc)) } else { Ok(()) }
    }

    /// Called from an mpv thread whenever new events are queued.
    pub fn set_wakeup_callback(&mut self, f: impl Fn() + Send + Sync + 'static) {
        unsafe extern "C" fn trampoline(data: *mut c_void) {
            let f = unsafe { &*(data as *const Box<dyn Fn() + Send + Sync>) };
            f();
        }
        let boxed: *mut Box<dyn Fn() + Send + Sync> = Box::into_raw(Box::new(Box::new(f)));
        unsafe { (api().set_wakeup_callback)(self.handle, Some(trampoline), boxed as *mut c_void) };
        if !self.wakeup.is_null() {
            drop(unsafe { Box::from_raw(self.wakeup) });
        }
        self.wakeup = boxed;
    }

    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        let rc = unsafe { (api().command)(self.handle, ptrs.as_mut_ptr()) };
        if rc < 0 { Err(format!("{}: {}", args.join(" "), err_str(rc))) } else { Ok(()) }
    }

    /// Fire-and-forget command; failures are logged.
    pub fn cmd(&self, args: &[&str]) {
        if let Err(e) = self.command(args) {
            eprintln!("mpv: {e}");
        }
    }

    pub fn set_flag(&self, name: &str, v: bool) {
        let mut val: c_int = v as c_int;
        self.set_raw(name, FORMAT_FLAG, &mut val as *mut _ as *mut c_void);
    }

    pub fn set_double(&self, name: &str, v: f64) {
        let mut val = v;
        self.set_raw(name, FORMAT_DOUBLE, &mut val as *mut _ as *mut c_void);
    }

    pub fn set_str(&self, name: &str, v: &str) {
        let s = cstr(v);
        let mut p = s.as_ptr();
        self.set_raw(name, FORMAT_STRING, &mut p as *mut _ as *mut c_void);
    }

    fn set_raw(&self, name: &str, format: Format, data: *mut c_void) {
        let n = cstr(name);
        let rc = unsafe { (api().set_property)(self.handle, n.as_ptr(), format, data) };
        if rc < 0 {
            eprintln!("mpv: set {name} failed: {}", err_str(rc));
        }
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        let n = cstr(name);
        let mut v: i64 = 0;
        let rc = unsafe { (api().get_property)(self.handle, n.as_ptr(), FORMAT_INT64, &mut v as *mut _ as *mut c_void) };
        (rc >= 0).then_some(v)
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let n = cstr(name);
        let mut v: f64 = 0.0;
        let rc = unsafe { (api().get_property)(self.handle, n.as_ptr(), FORMAT_DOUBLE, &mut v as *mut _ as *mut c_void) };
        (rc >= 0).then_some(v)
    }

    pub fn get_bool(&self, name: &str) -> Option<bool> {
        let n = cstr(name);
        let mut v: c_int = 0;
        let rc = unsafe { (api().get_property)(self.handle, n.as_ptr(), FORMAT_FLAG, &mut v as *mut _ as *mut c_void) };
        (rc >= 0).then_some(v != 0)
    }

    pub fn get_str(&self, name: &str) -> Option<String> {
        let n = cstr(name);
        let mut p: *mut c_char = ptr::null_mut();
        let rc = unsafe { (api().get_property)(self.handle, n.as_ptr(), FORMAT_STRING, &mut p as *mut _ as *mut c_void) };
        if rc < 0 || p.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { (api().free)(p as *mut c_void) };
        Some(s)
    }

    pub fn observe(&self, id: u64, name: &str, format: Format) {
        let n = cstr(name);
        unsafe { (api().observe_property)(self.handle, id, n.as_ptr(), format) };
    }

    /// Returns the next queued event without blocking.
    pub fn poll_event(&self) -> Option<Event> {
        loop {
            let ev = unsafe { &*(api().wait_event)(self.handle, 0.0) };
            let out = match ev.event_id {
                EVENT_NONE => return None,
                EVENT_SHUTDOWN => Event::Shutdown,
                EVENT_START_FILE => Event::StartFile,
                EVENT_FILE_LOADED => Event::FileLoaded,
                EVENT_VIDEO_RECONFIG => Event::VideoReconfig,
                EVENT_END_FILE => {
                    let ef = unsafe { &*(ev.data as *const RawEventEndFile) };
                    let error = (ef.reason == END_FILE_REASON_ERROR).then(|| err_str(ef.error));
                    Event::EndFile { error }
                }
                EVENT_PROPERTY_CHANGE => {
                    let p = unsafe { &*(ev.data as *const RawEventProperty) };
                    let value = unsafe {
                        match p.format {
                            FORMAT_FLAG => PropValue::Flag(*(p.data as *const c_int) != 0),
                            FORMAT_INT64 => PropValue::Int(*(p.data as *const i64)),
                            FORMAT_DOUBLE => PropValue::Double(*(p.data as *const f64)),
                            FORMAT_STRING => {
                                let s = *(p.data as *const *const c_char);
                                PropValue::Str(CStr::from_ptr(s).to_string_lossy().into_owned())
                            }
                            _ => PropValue::None,
                        }
                    };
                    Event::Property { id: ev.reply_userdata, value }
                }
                _ => continue,
            };
            return Some(out);
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            (api().set_wakeup_callback)(self.handle, None, ptr::null_mut());
            (api().terminate_destroy)(self.handle);
            if !self.wakeup.is_null() {
                drop(Box::from_raw(self.wakeup));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OpenGL render context
// ---------------------------------------------------------------------------

pub type ProcAddress = dyn Fn(&CStr) -> *const c_void + Send + Sync;

/// Renders mpv's video into the caller's OpenGL framebuffer. Every method
/// must be called on the thread that owns the GL context, with it current.
pub struct RenderContext {
    ctx: *mut MpvRenderContext,
    _proc: *mut Box<ProcAddress>,
    update: *mut Box<dyn Fn() + Send + Sync>,
}

unsafe impl Send for RenderContext {}

impl RenderContext {
    pub fn new(
        mpv: &Mpv,
        get_proc: Box<ProcAddress>,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        unsafe extern "C" fn proc_trampoline(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
            let f = unsafe { &*(ctx as *const Box<ProcAddress>) };
            f(unsafe { CStr::from_ptr(name) }) as *mut c_void
        }
        unsafe extern "C" fn update_trampoline(data: *mut c_void) {
            let f = unsafe { &*(data as *const Box<dyn Fn() + Send + Sync>) };
            f();
        }

        let proc_box: *mut Box<ProcAddress> = Box::into_raw(Box::new(get_proc));
        let mut init = OpenGlInitParams {
            get_proc_address: proc_trampoline,
            get_proc_address_ctx: proc_box as *mut c_void,
            extra_exts: ptr::null(),
        };
        let api_type = cstr("opengl");
        let mut advanced: c_int = 0;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
            RenderParam { kind: RENDER_PARAM_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_ADVANCED_CONTROL, data: &mut advanced as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_INVALID, data: ptr::null_mut() },
        ];
        let mut ctx = ptr::null_mut();
        let rc = unsafe { (api().render_create)(&mut ctx, mpv.handle, params.as_mut_ptr()) };
        if rc < 0 {
            drop(unsafe { Box::from_raw(proc_box) });
            return Err(format!("mpv_render_context_create failed: {}", err_str(rc)));
        }

        let update: *mut Box<dyn Fn() + Send + Sync> = Box::into_raw(Box::new(Box::new(on_update)));
        unsafe { (api().render_set_update_callback)(ctx, Some(update_trampoline), update as *mut c_void) };
        Ok(RenderContext { ctx, _proc: proc_box, update })
    }

    /// True when mpv has a new frame for us.
    pub fn wants_frame(&self) -> bool {
        unsafe { (api().render_update)(self.ctx) & RENDER_UPDATE_FRAME != 0 }
    }

    pub fn render(&self, fbo: i32, width: i32, height: i32, flip_y: bool) {
        let mut fbo = OpenGlFbo { fbo, w: width, h: height, internal_format: 0 };
        let mut flip: c_int = flip_y as c_int;
        // Never stall the UI thread waiting for a frame's display time.
        let mut block: c_int = 0;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_OPENGL_FBO, data: &mut fbo as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME, data: &mut block as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_INVALID, data: ptr::null_mut() },
        ];
        unsafe { (api().render_render)(self.ctx, params.as_mut_ptr()) };
    }

}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            (api().render_set_update_callback)(self.ctx, None, ptr::null_mut());
            (api().render_free)(self.ctx);
            drop(Box::from_raw(self.update));
            drop(Box::from_raw(self._proc));
        }
    }
}
