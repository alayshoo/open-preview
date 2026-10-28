//! Dev tool: gets the thumbnail for a file the way Explorer does: resolves the
//! handler through the file's association, creates it through COM from the
//! installer's registration, and saves what it returns as a .bmp.
//!
//!     probe.exe <video> <out.bmp> [size]
//!
//! Run on Windows (or under Wine) after installing the MSI.

#[cfg(not(windows))]
fn main() {
    eprintln!("probe only runs on Windows");
}

#[cfg(windows)]
fn main() {
    if let Err(e) = probe() {
        eprintln!("probe failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn probe() -> windows_core::Result<()> {
    use windows::Win32::Graphics::Gdi::{DIBSECTION, GetObjectW, HGDIOBJ};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, STGM_READ};
    use windows::Win32::UI::Shell::PropertiesSystem::IInitializeWithFile;
    use windows::Win32::UI::Shell::{ASSOCF_NONE, ASSOCSTR_SHELLEXTENSION, AssocQueryStringW, IThumbnailProvider, WTS_ALPHATYPE};
    use windows_core::{GUID, HSTRING, Interface, PWSTR, w};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("usage: probe <video> <out.bmp> [size]");
        std::process::exit(2);
    };
    let size: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(256);

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };

    // Which handler the association names for this extension's thumbnails.
    let ext = std::path::Path::new(input).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut buf = [0u16; 128];
    let mut len = buf.len() as u32;
    let hr = unsafe {
        AssocQueryStringW(ASSOCF_NONE, ASSOCSTR_SHELLEXTENSION, &HSTRING::from(ext.as_str()), w!("{e357fccd-a995-4576-b01f-234630154e96}"), Some(PWSTR(buf.as_mut_ptr())), &mut len)
    };
    let handler = if hr.is_ok() { String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]) } else { format!("none ({hr:?})") };
    println!("{ext} thumbnail handler: {handler}");
    let clsid = GUID::try_from(handler.trim_matches(|c| c == '{' || c == '}')).unwrap_or(GUID::from_u128(0x9153F65E_0762_4F48_87A5_E0200386CDB0));

    let provider: IThumbnailProvider = unsafe { CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER)? };
    let init: IInitializeWithFile = provider.cast()?;
    unsafe { init.Initialize(&HSTRING::from(input.as_str()), STGM_READ.0)? };

    let mut bitmap = Default::default();
    let mut alpha = WTS_ALPHATYPE::default();
    let started = std::time::Instant::now();
    unsafe { provider.GetThumbnail(size, &mut bitmap, &mut alpha)? };
    println!("GetThumbnail took {:?}", started.elapsed());

    let mut dib = DIBSECTION::default();
    let got = unsafe { GetObjectW(HGDIOBJ(bitmap.0), size_of::<DIBSECTION>() as i32, Some(&mut dib as *mut _ as *mut _)) };
    assert!(got > 0 && !dib.dsBm.bmBits.is_null(), "not a DIB section");
    let (w, h) = (dib.dsBm.bmWidth as u32, dib.dsBm.bmHeight.unsigned_abs());
    let pixels = unsafe { std::slice::from_raw_parts(dib.dsBm.bmBits as *const u8, (w * h * 4) as usize) };

    // A 32-bit top-down BMP.
    let mut file = Vec::new();
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&(w as i32).to_le_bytes());
    file.extend_from_slice(&(-(h as i32)).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&[0; 24]);
    file.extend_from_slice(pixels);
    std::fs::write(output, file).expect("write bmp");
    println!("{w}x{h} thumbnail -> {output}");
    Ok(())
}
