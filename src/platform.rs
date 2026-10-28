//! OS integration. Windows gets rounded corners, a dark frame, aspect-ratio
//! locked resizing and a fix for dragging between monitors of different
//! scale; other platforms use no-ops.

#[cfg(not(windows))]
use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub fn init(_window: &impl HasWindowHandle) {}

/// Locks interactive resizing to `aspect` (width / height). `None` unlocks.
#[cfg(not(windows))]
pub fn set_aspect_lock(_aspect: Option<f32>) {}

/// The usable area (minus the taskbar) of the monitor the window is on, in
/// physical pixels. `None` where it isn't known.
#[cfg(not(windows))]
pub fn work_area() -> Option<egui::Rect> {
    None
}

/// Whether this executable is what opens `.mp4` files. `None` where the
/// question doesn't apply (not Windows).
#[cfg(not(windows))]
pub fn is_default_video_player() -> Option<bool> {
    None
}

/// Opens the system's default-apps page for this app.
#[cfg(not(windows))]
pub fn open_default_apps_settings() {}

#[cfg(windows)]
mod win {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromWindow};
    use windows::Win32::UI::Shell::{
        ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, AssocQueryStringW, DefSubclassProc, SetWindowSubclass, ShellExecuteW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SW_SHOWNORMAL, SWP_NOMOVE, WINDOWPOS, WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_SIZING,
        WM_WINDOWPOSCHANGING,
    };
    use windows::core::{PWSTR, w};

    static ASPECT_BITS: AtomicU32 = AtomicU32::new(0);
    static HWND_PTR: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
    /// Between `WM_ENTERSIZEMOVE` and `WM_EXITSIZEMOVE`: the user is dragging
    /// or resizing the window.
    static IN_SIZE_MOVE: AtomicBool = AtomicBool::new(false);

    thread_local! {
        /// Where Windows wants the window during a scale change mid-drag; see
        /// `subclass_proc`.
        static DPI_RECT: Cell<Option<RECT>> = const { Cell::new(None) };
    }

    pub fn init(window: &impl HasWindowHandle) {
        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
        let hwnd = HWND(h.hwnd.get() as *mut c_void);
        HWND_PTR.store(hwnd.0, Ordering::Relaxed);
        unsafe {
            // Windows 11: rounded corners and a dark frame for the borderless window.
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const c_void,
                size_of_val(&corner) as u32,
            );
            let dark: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as *const c_void,
                size_of_val(&dark) as u32,
            );
            let _ = SetWindowSubclass(hwnd, Some(subclass_proc), 0x4C55, 0);
        }
    }

    /// Asks the shell which program opens `.mp4` and compares it with us.
    /// Windows doesn't let an app make itself the default; it can only ask.
    pub fn is_default_video_player() -> Option<bool> {
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let hr = unsafe {
            AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, w!(".mp4"), w!("open"), Some(PWSTR(buf.as_mut_ptr())), &mut len)
        };
        if hr.is_err() {
            return Some(false);
        }
        let handler = String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]);
        let me = std::env::current_exe().ok()?;
        Some(handler.eq_ignore_ascii_case(&me.to_string_lossy()))
    }

    /// Windows' own Default apps page for Open Preview, where one "Set
    /// default" covers every video type the installer registered. Falls back
    /// to the main Default apps page on older builds.
    pub fn open_default_apps_settings() {
        unsafe {
            ShellExecuteW(None, w!("open"), w!("ms-settings:defaultapps?registeredAppMachine=Open%20Preview"), None, None, SW_SHOWNORMAL);
        }
    }

    pub fn set_aspect_lock(aspect: Option<f32>) {
        ASPECT_BITS.store(aspect.filter(|a| a.is_finite() && *a > 0.0).map_or(0, f32::to_bits), Ordering::Relaxed);
    }

    pub fn work_area() -> Option<egui::Rect> {
        let hwnd = HWND(HWND_PTR.load(Ordering::Relaxed));
        if hwnd.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let ok = unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
            !monitor.is_invalid() && GetMonitorInfoW(monitor, &mut info).as_bool()
        };
        let r = info.rcWork;
        ok.then(|| egui::Rect::from_min_max(egui::pos2(r.left as f32, r.top as f32), egui::pos2(r.right as f32, r.bottom as f32)))
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match msg {
            WM_SIZING => {
                let aspect = f32::from_bits(ASPECT_BITS.load(Ordering::Relaxed));
                if aspect > 0.0 {
                    let r = unsafe { &mut *(lparam.0 as *mut RECT) };
                    constrain(r, wparam.0 as u32, aspect);
                    return LRESULT(1);
                }
            }
            WM_ENTERSIZEMOVE => IN_SIZE_MOVE.store(true, Ordering::Relaxed),
            WM_EXITSIZEMOVE => IN_SIZE_MOVE.store(false, Ordering::Relaxed),
            // Dragged onto a monitor with a different scale. winit 0.30 checks
            // the new rect against `MonitorFromWindow`, which mid-drag still
            // names the monitor being left, so it shoves the window back onto
            // it. That changes the scale back, which rescales and shoves it
            // again: the window balloons and never crosses over
            // (rust-windowing/winit#4041). Let winit handle the message, since
            // it updates the scale factor egui draws with, but put its
            // `SetWindowPos` where Windows suggests, as Microsoft documents.
            WM_DPICHANGED if IN_SIZE_MOVE.load(Ordering::Relaxed) => {
                let suggested = unsafe { *(lparam.0 as *const RECT) };
                DPI_RECT.set(Some(suggested));
                let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
                DPI_RECT.set(None);
                return result;
            }
            WM_WINDOWPOSCHANGING => {
                if let Some(r) = DPI_RECT.get() {
                    let pos = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
                    if !pos.flags.contains(SWP_NOMOVE) {
                        // Keep winit's size: it holds the window's size in
                        // points steady across the scale change.
                        pos.x = r.left;
                        pos.y = r.top;
                    }
                }
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Adjusts the rect being dragged so it keeps the video's aspect ratio.
    fn constrain(r: &mut RECT, edge: u32, aspect: f32) {
        const LEFT: u32 = 1;
        const RIGHT: u32 = 2;
        const TOP: u32 = 3;
        const TOPLEFT: u32 = 4;
        const TOPRIGHT: u32 = 5;
        const BOTTOM: u32 = 6;
        let w = (r.right - r.left) as f32;
        let h = (r.bottom - r.top) as f32;
        match edge {
            LEFT | RIGHT => r.bottom = r.top + (w / aspect).round() as i32,
            TOP | BOTTOM => r.right = r.left + (h * aspect).round() as i32,
            TOPLEFT | TOPRIGHT => r.top = r.bottom - (w / aspect).round() as i32,
            _ => r.bottom = r.top + (w / aspect).round() as i32,
        }
    }
}
