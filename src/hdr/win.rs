//! Windows: an FP16 scRGB swap chain on a child window laid over the client
//! area, which OpenGL draws into through `WGL_NV_DX_interop2`, the same
//! bridge mpv's own OpenGL output uses for D3D11 hardware decoding. The child
//! takes no input: clicks and drops go through to the window under it.

use std::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::glow::{self, HasContext};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_MWA_NO_ALT_ENTER, DXGI_MWA_NO_WINDOW_CHANGES, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter1, IDXGIFactory2, IDXGIOutput6,
    IDXGISwapChain1, IDXGISwapChain3,
};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HTTRANSPARENT, RegisterClassExW, SW_HIDE, SW_SHOWNA,
    SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, ShowWindow, WM_ERASEBKGND, WM_NCHITTEST, WNDCLASSEXW, WS_CHILD,
    WS_CLIPSIBLINGS, WS_DISABLED, WS_EX_NOPARENTNOTIFY,
};
use windows::core::{BOOL, Interface, PCWSTR, w};

use super::Display;

/// The window's screen, when it's in HDR mode. Read at most once a second,
/// and again as soon as the window moves to another screen.
pub fn display() -> Option<Display> {
    static CACHE: Mutex<Option<(Instant, isize, Option<Display>)>> = Mutex::new(None);
    let hwnd = crate::platform::hwnd()?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, m, d)) = *cache
        && m == monitor.0 as isize
        && at.elapsed() < Duration::from_secs(1)
    {
        return d;
    }
    let d = query(monitor);
    *cache = Some((Instant::now(), monitor.0 as isize, d));
    d
}

fn query(monitor: HMONITOR) -> Option<Display> {
    unsafe {
        // A new factory each time: an old one doesn't see HDR being turned on.
        let factory: IDXGIFactory2 = CreateDXGIFactory1().ok()?;
        for a in 0u32.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else { break };
            for o in 0u32.. {
                let Ok(output) = adapter.EnumOutputs(o) else { break };
                let Some(desc) = output.cast::<IDXGIOutput6>().ok().and_then(|o| o.GetDesc1().ok()) else { continue };
                if desc.Monitor != monitor {
                    continue;
                }
                if desc.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
                    return None;
                }
                // Screens that don't say get the HDR10 reference peak.
                let peak_nits = if desc.MaxLuminance > 0.0 { desc.MaxLuminance } else { 1000.0 };
                let sdr_white_nits = sdr_white_nits(&desc.DeviceName).unwrap_or(203.0);
                return Some(Display { peak_nits, sdr_white_nits });
            }
        }
        None
    }
}

/// Windows' "SDR content brightness" for the screen GDI calls `device`.
fn sdr_white_nits(device: &[u16; 32]) -> Option<f32> {
    let name = |s: &[u16]| s.iter().take_while(|&&c| c != 0).copied().collect::<Vec<u16>>();
    unsafe {
        let (mut paths, mut modes) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) != ERROR_SUCCESS {
            return None;
        }
        let mut path_info = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
        let mut mode_info = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        let found = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut paths,
            path_info.as_mut_ptr(),
            &mut modes,
            mode_info.as_mut_ptr(),
            None,
        );
        if found != ERROR_SUCCESS {
            return None;
        }
        for path in &path_info[..paths as usize] {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 || name(&source.viewGdiDeviceName) != name(device) {
                continue;
            }
            let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    size: size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                SDRWhiteLevel: 0,
            };
            if DisplayConfigGetDeviceInfo(&mut white.header) != 0 || white.SDRWhiteLevel == 0 {
                return None;
            }
            // In thousandths of 80 nits.
            return Some(white.SDRWhiteLevel as f32 / 1000.0 * 80.0);
        }
        None
    }
}

const WGL_ACCESS_WRITE_DISCARD_NV: u32 = 0x0002;

type OpenDevice = unsafe extern "system" fn(*mut c_void) -> HANDLE;
type CloseDevice = unsafe extern "system" fn(HANDLE) -> BOOL;
type RegisterObject = unsafe extern "system" fn(HANDLE, *mut c_void, u32, u32, u32) -> HANDLE;
type UnregisterObject = unsafe extern "system" fn(HANDLE, HANDLE) -> BOOL;
type LockObjects = unsafe extern "system" fn(HANDLE, i32, *mut HANDLE) -> BOOL;

/// `WGL_NV_DX_interop2`.
struct Interop {
    device: HANDLE,
    close_device: CloseDevice,
    register_object: RegisterObject,
    unregister_object: UnregisterObject,
    lock_objects: LockObjects,
    unlock_objects: LockObjects,
}

/// The texture GL draws into and Direct3D copies to the swap chain, one per
/// window size.
struct Shared {
    texture: ID3D11Texture2D,
    object: HANDLE,
    gl_texture: glow::Texture,
    fbo: glow::Framebuffer,
}

/// Puts frames on screen as HDR, over the window's own OpenGL surface.
pub struct Presenter {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain1,
    child: HWND,
    interop: Interop,
    shared: Option<Shared>,
    size: [u32; 2],
    shown: bool,
}

// Made, used and dropped on the UI thread, which owns the GL context; the
// app's paint callbacks just have to be `Send` to be callbacks at all.
unsafe impl Send for Presenter {}

impl Presenter {
    pub fn new(gl: &glow::Context, get_proc: &crate::mpv::ProcAddress) -> Result<Self, String> {
        let parent = crate::platform::hwnd().ok_or("no window")?;
        let open_device = get_proc(c"wglDXOpenDeviceNV");
        let symbols = [
            get_proc(c"wglDXCloseDeviceNV"),
            get_proc(c"wglDXRegisterObjectNV"),
            get_proc(c"wglDXUnregisterObjectNV"),
            get_proc(c"wglDXLockObjectsNV"),
            get_proc(c"wglDXUnlockObjectsNV"),
        ];
        if open_device.is_null() || symbols.iter().any(|p| p.is_null()) {
            return Err("the OpenGL driver has no WGL_NV_DX_interop2".into());
        }
        unsafe {
            let factory: IDXGIFactory2 = CreateDXGIFactory1().map_err(|e| format!("DXGI: {e}"))?;
            // Interop needs Direct3D on the GPU OpenGL runs on.
            let vendor = gl.get_parameter_string(glow::VENDOR).to_ascii_lowercase();
            let adapter = gl_adapter(&factory, &vendor).ok_or("no Direct3D adapter")?;
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| format!("Direct3D 11: {e}"))?;
            let (device, context) = (device.ok_or("no device")?, context.ok_or("no device context")?);

            let open_device = std::mem::transmute::<*const c_void, OpenDevice>(open_device);
            let interop_device = open_device(device.as_raw());
            if interop_device.is_invalid() {
                return Err("wglDXOpenDeviceNV failed".into());
            }
            let interop = Interop {
                device: interop_device,
                close_device: std::mem::transmute::<*const c_void, CloseDevice>(symbols[0]),
                register_object: std::mem::transmute::<*const c_void, RegisterObject>(symbols[1]),
                unregister_object: std::mem::transmute::<*const c_void, UnregisterObject>(symbols[2]),
                lock_objects: std::mem::transmute::<*const c_void, LockObjects>(symbols[3]),
                unlock_objects: std::mem::transmute::<*const c_void, LockObjects>(symbols[4]),
            };

            let child = match child_window(parent) {
                Ok(c) => c,
                Err(e) => {
                    let _ = (interop.close_device)(interop.device);
                    return Err(e);
                }
            };
            let fail = |e: String| {
                let _ = DestroyWindow(child);
                let _ = (interop.close_device)(interop.device);
                e
            };
            let mut client = RECT::default();
            let _ = GetClientRect(parent, &mut client);
            let size = [(client.right - client.left).max(1) as u32, (client.bottom - client.top).max(1) as u32];
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: size[0],
                Height: size[1],
                Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            let swapchain = factory
                .CreateSwapChainForHwnd(&device, child, &desc, None, None)
                .map_err(|e| fail(format!("swap chain: {e}")))?;
            let _ = factory.MakeWindowAssociation(child, DXGI_MWA_NO_ALT_ENTER | DXGI_MWA_NO_WINDOW_CHANGES);
            let colour = swapchain.cast::<IDXGISwapChain3>().map_err(|e| fail(format!("DXGI 1.4: {e}")))?;
            let support = colour.CheckColorSpaceSupport(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709).unwrap_or(0);
            if support & DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT.0 as u32 == 0 {
                return Err(fail("the swap chain can't present scRGB".into()));
            }
            colour.SetColorSpace1(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709).map_err(|e| fail(format!("scRGB: {e}")))?;

            Ok(Presenter { device, context, swapchain, child, interop, shared: None, size, shown: false })
        }
    }

    /// Starts a frame of `size` pixels; returns the framebuffer to draw it into.
    pub fn begin(&mut self, gl: &glow::Context, size: [u32; 2]) -> Result<glow::Framebuffer, String> {
        unsafe {
            if self.shared.is_none() || size != self.size {
                self.release(gl);
                let flags = DXGI_SWAP_CHAIN_FLAG(0);
                self.swapchain
                    .ResizeBuffers(0, size[0], size[1], DXGI_FORMAT_UNKNOWN, flags)
                    .map_err(|e| format!("resizing the swap chain: {e}"))?;
                let (w, h) = (size[0] as i32, size[1] as i32);
                let _ = SetWindowPos(self.child, None, 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                self.size = size;
                self.shared = Some(self.share(gl)?);
            }
            let shared = self.shared.as_mut().unwrap();
            if !(self.interop.lock_objects)(self.interop.device, 1, &mut shared.object).as_bool() {
                return Err("wglDXLockObjectsNV failed".into());
            }
            Ok(shared.fbo)
        }
    }

    /// Shows the frame drawn since `begin`.
    pub fn present(&mut self, _gl: &glow::Context) -> Result<(), String> {
        let shared = self.shared.as_mut().ok_or("present without begin")?;
        unsafe {
            // Unlocking finishes GL's drawing before Direct3D reads it.
            let _ = (self.interop.unlock_objects)(self.interop.device, 1, &mut shared.object);
            let back: ID3D11Texture2D = self.swapchain.GetBuffer(0).map_err(|e| format!("back buffer: {e}"))?;
            self.context.CopyResource(&back, &shared.texture);
            drop(back);
            // No wait: the window's own buffer swap paces the frames.
            self.swapchain.Present(0, DXGI_PRESENT(0)).ok().map_err(|e| format!("present: {e}"))?;
            if !self.shown {
                let _ = ShowWindow(self.child, SW_SHOWNA);
                self.shown = true;
            }
        }
        Ok(())
    }

    /// Back to the window's own (SDR) surface.
    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.child, SW_HIDE);
            }
            self.shown = false;
        }
    }

    pub fn destroy(mut self, gl: &glow::Context) {
        unsafe {
            self.release(gl);
            let _ = (self.interop.close_device)(self.interop.device);
            let _ = DestroyWindow(self.child);
        }
    }

    unsafe fn share(&self, gl: &glow::Context) -> Result<Shared, String> {
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: self.size[0],
                Height: self.size[1],
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut texture = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("shared texture: {e}"))?;
            let texture = texture.ok_or("no shared texture")?;
            let gl_texture = gl.create_texture()?;
            let mut object = (self.interop.register_object)(
                self.interop.device,
                texture.as_raw(),
                gl_texture.0.get(),
                glow::TEXTURE_2D,
                WGL_ACCESS_WRITE_DISCARD_NV,
            );
            if object.is_invalid() {
                gl.delete_texture(gl_texture);
                return Err("wglDXRegisterObjectNV failed".into());
            }
            let fbo = gl.create_framebuffer()?;
            let _ = (self.interop.lock_objects)(self.interop.device, 1, &mut object);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(gl_texture), 0);
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let _ = (self.interop.unlock_objects)(self.interop.device, 1, &mut object);
            let shared = Shared { texture, object, gl_texture, fbo };
            if status != glow::FRAMEBUFFER_COMPLETE {
                self.unshare(gl, shared);
                return Err(format!("shared framebuffer incomplete ({status:#x})"));
            }
            Ok(shared)
        }
    }

    unsafe fn release(&mut self, gl: &glow::Context) {
        if let Some(shared) = self.shared.take() {
            unsafe { self.unshare(gl, shared) };
        }
    }

    unsafe fn unshare(&self, gl: &glow::Context, shared: Shared) {
        unsafe {
            gl.delete_framebuffer(shared.fbo);
            let _ = (self.interop.unregister_object)(self.interop.device, shared.object);
            gl.delete_texture(shared.gl_texture);
        }
    }
}

/// The adapter whose vendor matches OpenGL's (on laptops with two GPUs they
/// differ from the default), or the default one.
unsafe fn gl_adapter(factory: &IDXGIFactory2, gl_vendor: &str) -> Option<IDXGIAdapter1> {
    let want = [("nvidia", 0x10de), ("amd", 0x1002), ("ati", 0x1002), ("intel", 0x8086)]
        .into_iter()
        .find(|(name, _)| gl_vendor.contains(name))
        .map(|(_, id)| id);
    unsafe {
        let mut first = None;
        for a in 0u32.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else { break };
            let vendor = adapter.GetDesc1().map(|d| d.VendorId).unwrap_or(0);
            if want == Some(vendor) {
                return Some(adapter);
            }
            first.get_or_insert(adapter);
        }
        first
    }
}

/// A borderless child window over the parent's client area that lets all
/// input through.
unsafe fn child_window(parent: HWND) -> Result<HWND, String> {
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_ERASEBKGND => LRESULT(1),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
    const CLASS: PCWSTR = w!("OpenPreviewHdrSurface");
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        // Fails harmlessly when already registered.
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_NOPARENTNOTIFY,
            CLASS,
            PCWSTR::null(),
            // Disabled as well as transparent to hit tests: never takes input.
            WS_CHILD | WS_DISABLED | WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("HDR surface window: {e}"))
    }
}

