//! OS integration. Windows gets rounded corners, a dark frame, aspect-ratio
//! locked resizing and work-area queries; other platforms use no-ops.

#[cfg(not(windows))]
use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub fn init(_window: &impl HasWindowHandle) {}

/// Locks interactive resizing to `aspect` (width / height). `None` unlocks.
#[cfg(not(windows))]
pub fn set_aspect_lock(_aspect: Option<f32>) {}

/// Work area (desktop minus taskbar) of the window's monitor, in physical pixels.
#[cfg(not(windows))]
pub fn work_area() -> Option<[i32; 4]> {
    None
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::WM_SIZING;

    static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
    static ASPECT_BITS: AtomicU32 = AtomicU32::new(0);

    fn hwnd() -> Option<HWND> {
        let raw = HWND_RAW.load(Ordering::Relaxed);
        (raw != 0).then_some(HWND(raw as *mut c_void))
    }

    pub fn init(window: &impl HasWindowHandle) {
        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
        HWND_RAW.store(h.hwnd.get(), Ordering::Relaxed);
        let hwnd = HWND(h.hwnd.get() as *mut c_void);
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

    pub fn set_aspect_lock(aspect: Option<f32>) {
        ASPECT_BITS.store(aspect.filter(|a| a.is_finite() && *a > 0.0).map_or(0, f32::to_bits), Ordering::Relaxed);
    }

    pub fn work_area() -> Option<[i32; 4]> {
        let hwnd = hwnd()?;
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return None;
            }
            let r = info.rcWork;
            Some([r.left, r.top, r.right, r.bottom])
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_SIZING {
            let aspect = f32::from_bits(ASPECT_BITS.load(Ordering::Relaxed));
            if aspect > 0.0 {
                let r = unsafe { &mut *(lparam.0 as *mut RECT) };
                constrain(r, wparam.0 as u32, aspect);
                return LRESULT(1);
            }
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
