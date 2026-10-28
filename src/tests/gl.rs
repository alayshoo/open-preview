//! The window's render path without a window: an EGL context with no surface
//! (Mesa's llvmpipe in CI), mpv's OpenGL renderer drawing into a framebuffer
//! as it does into egui's, painted the way the app paints, then read back and
//! checked. This is where HDR tone mapping, colour matrices and subtitles are
//! actually drawn.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::glow::{self, HasContext};
use libloading::Library;

use super::cases::Case;
use crate::mpv::{Event, RenderContext};

type Display = *mut c_void;

const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
const EGL_OPENGL_API: u32 = 0x30A2;
const EGL_NONE: i32 = 0x3038;

struct Egl {
    _lib: Library,
    display: usize,
    create_context: unsafe extern "C" fn(Display, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    /// Per thread, like the current context.
    bind_api: unsafe extern "C" fn(u32) -> u32,
    make_current: unsafe extern "C" fn(Display, *mut c_void, *mut c_void, *mut c_void) -> u32,
    destroy_context: unsafe extern "C" fn(Display, *mut c_void) -> u32,
    get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
}

fn egl() -> Result<&'static Egl, String> {
    static EGL: OnceLock<Result<Egl, String>> = OnceLock::new();
    EGL.get_or_init(|| unsafe {
        let lib = Library::new("libEGL.so.1").map_err(|e| e.to_string())?;
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| e.to_string())?
            };
        }
        let get_platform_display: unsafe extern "C" fn(u32, *mut c_void, *const isize) -> Display =
            sym!("eglGetPlatformDisplay");
        let initialize: unsafe extern "C" fn(Display, *mut i32, *mut i32) -> u32 = sym!("eglInitialize");
        let display = get_platform_display(EGL_PLATFORM_SURFACELESS_MESA, std::ptr::null_mut(), std::ptr::null());
        if display.is_null() || initialize(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            return Err("no surfaceless EGL display (Mesa's libegl-mesa0 is needed)".into());
        }
        Ok(Egl {
            bind_api: sym!("eglBindAPI"),
            display: display as usize,
            create_context: sym!("eglCreateContext"),
            make_current: sym!("eglMakeCurrent"),
            destroy_context: sym!("eglDestroyContext"),
            get_proc_address: sym!("eglGetProcAddress"),
            _lib: lib,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Renders the frame 0.5 s in, with the app's options, into a "window" of
/// `w`×`h`; returns top-down RGBA.
pub fn render(path: &str, (w, h): (usize, usize), extra: &[(&'static str, &str)]) -> Result<Vec<u8>, String> {
    // llvmpipe is threaded already; contexts one at a time keep memory down.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let egl = egl()?;
    let display = egl.display as Display;
    unsafe {
        if (egl.bind_api)(EGL_OPENGL_API) == 0 {
            return Err("EGL has no desktop OpenGL".into());
        }
        // EGL_NO_CONFIG_KHR and no surface: everything goes to our own FBO.
        let attribs = [EGL_NONE];
        let ctx = (egl.create_context)(display, std::ptr::null_mut(), std::ptr::null_mut(), attribs.as_ptr());
        if ctx.is_null() {
            return Err("eglCreateContext failed".into());
        }
        if (egl.make_current)(display, std::ptr::null_mut(), std::ptr::null_mut(), ctx) == 0 {
            return Err("eglMakeCurrent failed".into());
        }
        let result = render_current(egl, path, (w, h), extra);
        (egl.make_current)(display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        (egl.destroy_context)(display, ctx);
        result
    }
}

unsafe fn render_current(
    egl: &'static Egl,
    path: &str,
    (w, h): (usize, usize),
    extra: &[(&'static str, &str)],
) -> Result<Vec<u8>, String> {
    let gpa = egl.get_proc_address;
    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            let name = CString::new(name).unwrap();
            gpa(name.as_ptr()) as *const c_void
        })
    };
    let mut overrides = vec![
        ("start", "0.5"),
        ("hwdec", "no"),
        // One pass straight into the target. llvmpipe often leaves mpv's
        // intermediate framebuffers black; GPUs don't. Colour conversion and
        // tone mapping are the same code either way, only the scalers and HDR
        // peak detection (which need those framebuffers) are left out.
        ("gpu-dumb-mode", "yes"),
    ];
    overrides.extend_from_slice(extra);
    let mpv = super::new_mpv(&overrides);
    let debug = std::env::var_os("OPEN_PREVIEW_TEST_LOG").is_some();
    mpv.request_log_messages(if debug { "v" } else { "error" });
    let get_proc: Box<crate::mpv::ProcAddress> = Box::new(move |name: &CStr| unsafe { gpa(name.as_ptr()) as *const c_void });
    let rc = RenderContext::new(&mpv, get_proc, || {})?;
    mpv.command(&["loadfile", path])?;

    let target = unsafe { Target::new(&gl, w as i32, h as i32)? };
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut shown = None;
    // Only for explaining a failure: FFmpeg's messages reach whichever mpv
    // instance in the process came first, so they may be another test's.
    let mut errors = Vec::new();
    // As the app paints: every frame, whether or not mpv has something new.
    // Looked at 0.4 s into playback: right after a seek, a stream without an
    // index (TS) can show a few grey frames until the decoder has its
    // references, in VLC too.
    while shown.is_none_or(|t: Instant| t.elapsed() < Duration::from_millis(400)) {
        while let Some(ev) = mpv.poll_event() {
            match ev {
                Event::PlaybackRestart => shown = shown.or(Some(Instant::now())),
                Event::Log { prefix, text } => {
                    if debug {
                        eprintln!("[{prefix}] {text}");
                    }
                    errors.push(format!("[{prefix}] {text}"));
                }
                Event::EndFile { error: Some(e) } => return Err(e),
                _ => {}
            }
        }
        rc.wants_frame();
        rc.render(target.fbo.0.get() as i32, w as i32, h as i32, false);
        if Instant::now() > deadline {
            return Err(format!("no frame ({})", errors.join("; ")));
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let mut pixels = vec![0u8; w * h * 4];
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
        gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
        drop(rc);
        target.delete(&gl);
    }
    // Without FLIP_Y, mpv draws the top row first.
    Ok(pixels)
}

/// An RGBA8 framebuffer, as egui's default one is.
struct Target {
    fbo: glow::NativeFramebuffer,
    tex: glow::NativeTexture,
}

impl Target {
    unsafe fn new(gl: &glow::Context, w: i32, h: i32) -> Result<Self, String> {
        unsafe {
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            let empty = glow::PixelUnpackData::Slice(None);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, empty);
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            let complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            // Default state for mpv, as the app leaves it.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            if !complete {
                return Err("framebuffer incomplete".into());
            }
            Ok(Target { fbo, tex })
        }
    }

    unsafe fn delete(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex);
        }
    }
}

pub fn check(case: &Case, path: &str, failures: &mut Vec<String>) {
    if let Err(e) = egl() {
        return super::unavailable(&format!("OpenGL: {e}"));
    }
    if case.quads.is_none() {
        return;
    }
    // A window the video's shape, as the app sizes it.
    let (w, h) = case.display.unwrap_or((case.size.0 as i64, case.size.1 as i64));
    let (w, h) = (w as usize, h as usize);
    let plain = match render(path, (w, h), &[("sid", "no")]) {
        Err(e) => return failures.push(format!("OpenGL render failed: {e}")),
        Ok(rgba) => rgba,
    };
    super::dump(&format!("{}.opengl", case.file), &plain, w, h);
    super::compare("OpenGL", &plain, w, h, case, case.tol, failures);
    if case.sub_text.is_some() {
        match render(path, (w, h), &[]) {
            Err(e) => failures.push(format!("OpenGL render with subtitles failed: {e}")),
            Ok(rgba) => {
                super::dump(&format!("{}.opengl-subs", case.file), &rgba, w, h);
                // Text near the bottom: pixels there that subtitles changed.
                let bottom = w * 4 * (h / 2);
                let changed = rgba[bottom..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(plain[bottom..].as_chunks::<4>().0)
                    .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 60))
                    .count();
                if changed < 50 {
                    failures.push(format!("no subtitles drawn ({changed} pixels changed)"));
                }
            }
        }
    }
}

