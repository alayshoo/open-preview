//! Explorer thumbnail handler for Open Preview's video file type.
//!
//! Registered on the `OpenPreview.Video` ProgID, so Explorer only asks it for
//! thumbnails while Open Preview is the default player. It decodes nothing
//! itself: it runs `open-preview.exe --thumbnail` (see `src/thumbnail.rs`),
//! which renders a frame with libmpv, and turns the pixels into a bitmap. The
//! work happens in that short-lived process, so a file that crashes or hangs
//! the decoder takes down a child process, never Explorer.
#![cfg(windows)]

use std::ffi::c_void;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, E_UNEXPECTED, HMODULE, S_FALSE};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HBITMAP};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithFile, IInitializeWithFile_Impl};
use windows::Win32::UI::Shell::{IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_RGB};
use windows_core::{BOOL, Error, GUID, HRESULT, IUnknown, Interface, PCWSTR, Ref, Result, implement};

/// {9153F65E-0762-4F48-87A5-E0200386CDB0}; the installer registers this.
pub const CLSID_THUMBNAIL_PROVIDER: GUID = GUID::from_u128(0x9153F65E_0762_4F48_87A5_E0200386CDB0);

const EXE_NAME: &str = "open-preview.exe";
/// Explorer gives up on slow handlers anyway; don't leave a decoder running.
const TIMEOUT: Duration = Duration::from_secs(10);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[implement(IThumbnailProvider, IInitializeWithFile)]
struct ThumbnailProvider {
    path: Mutex<Option<String>>,
}

impl IInitializeWithFile_Impl for ThumbnailProvider_Impl {
    fn Initialize(&self, pszfilepath: &PCWSTR, _grfmode: u32) -> Result<()> {
        let path = unsafe { pszfilepath.to_string() }.map_err(|_| Error::from(E_FAIL))?;
        *self.path.lock().unwrap() = Some(path);
        Ok(())
    }
}

impl IThumbnailProvider_Impl for ThumbnailProvider_Impl {
    fn GetThumbnail(&self, cx: u32, phbmp: *mut HBITMAP, pdwalpha: *mut WTS_ALPHATYPE) -> Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(E_POINTER.into());
        }
        let path = self.path.lock().unwrap().clone().ok_or_else(|| Error::from(E_UNEXPECTED))?;
        let (width, height, pixels) = render(&path, cx).ok_or_else(|| Error::from(E_FAIL))?;
        let bitmap = to_hbitmap(width, height, &pixels)?;
        unsafe {
            *phbmp = bitmap;
            *pdwalpha = WTSAT_RGB;
        }
        Ok(())
    }
}

/// Runs the player in thumbnail mode and reads back `OPTH` + w + h + BGRA.
fn render(path: &str, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let exe = module_dir()?.join(EXE_NAME);
    let mut child = Command::new(exe)
        .arg("--thumbnail")
        .arg(path)
        .arg(size.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let data = reader.join().ok()?.ok()?;
    if !status.success() || data.len() < 12 || &data[..4] != b"OPTH" {
        return None;
    }
    let width = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(data[8..12].try_into().ok()?);
    let len = width as usize * height as usize * 4;
    let pixels = data.get(12..12 + len)?.to_vec();
    Some((width, height, pixels))
}

fn to_hbitmap(width: u32, height: u32, pixels: &[u8]) -> Result<HBITMAP> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative: rows run top to bottom, as the pixels do.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
    if bits.is_null() {
        return Err(E_FAIL.into());
    }
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len()) };
    Ok(bitmap)
}

/// The folder this DLL was loaded from, which is where the player lives.
fn module_dir() -> Option<PathBuf> {
    let mut module = HMODULE::default();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(module_dir as *const u16),
            &mut module,
        )
        .ok()?;
    }
    let mut buf = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    PathBuf::from(String::from_utf16_lossy(&buf[..len])).parent().map(PathBuf::from)
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<IUnknown>, riid: *const GUID, ppvobject: *mut *mut c_void) -> Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IUnknown = ThumbnailProvider { path: Mutex::new(None) }.into();
        unsafe { provider.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// # Safety
/// Called by COM with valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    unsafe { *ppv = std::ptr::null_mut() };
    if rclsid.is_null() || unsafe { *rclsid } != CLSID_THUMBNAIL_PROVIDER {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    unsafe { factory.query(riid, ppv) }
}

/// Stays loaded for the life of the host; the thumbnail cache reuses it.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}
