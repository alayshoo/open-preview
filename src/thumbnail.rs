//! `open-preview --thumbnail <file> [size]`: renders one frame of a video and
//! writes it to stdout, for the Explorer thumbnail handler (`thumbnailer/`).
//! No window and no GPU: libmpv's software renderer draws straight to memory,
//! so anything the player can play gets a thumbnail.
//!
//! Output: `OPTH`, width and height as little-endian u32, then width × height
//! BGRA pixels, top row first.

use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use crate::mpv::{Event, Mpv, RenderContext};

/// How far in to take the frame, past most fade-ins and title cards.
const POSITION: &str = "15%";
const TIMEOUT: Duration = Duration::from_secs(8);

pub fn run(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("usage: open-preview --thumbnail <file> [size]");
        return 2;
    };
    let size = args.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(256).clamp(16, 2048);
    match render(path, size) {
        Ok((w, h, pixels)) => {
            let mut out = std::io::stdout().lock();
            let header = [b"OPTH".as_slice(), &w.to_le_bytes(), &h.to_le_bytes()].concat();
            if out.write_all(&header).and_then(|_| out.write_all(&pixels)).and_then(|_| out.flush()).is_err() {
                return 1;
            }
            0
        }
        Err(e) => {
            eprintln!("thumbnail: {e}");
            1
        }
    }
}

fn render(path: &str, size: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let mpv = Mpv::new(&[
        ("vo", "libmpv"),
        ("ao", "null"),
        ("aid", "no"),
        ("sid", "no"),
        ("hwdec", "no"),
        ("pause", "yes"),
        ("start", POSITION),
        ("hr-seek", "yes"),
        ("resume-playback", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("osd-level", "0"),
        ("terminal", "no"),
        ("idle", "yes"),
        ("keep-open", "always"),
    ])?;
    let renderer = RenderContext::new_sw(&mpv)?;
    mpv.command(&["loadfile", path])?;

    let deadline = Instant::now() + TIMEOUT;
    let mut restarted = false;
    loop {
        while let Some(event) = mpv.poll_event() {
            match event {
                Event::PlaybackRestart => restarted = true,
                Event::EndFile { error } => return Err(error.unwrap_or_else(|| "no video".into())),
                Event::Shutdown => return Err("mpv shut down".into()),
                _ => {}
            }
        }
        if restarted && renderer.wants_frame() {
            break;
        }
        if Instant::now() > deadline {
            return Err("timed out waiting for a frame".into());
        }
        thread::sleep(Duration::from_millis(5));
    }

    let (dw, dh) = match (mpv.get_i64("dwidth"), mpv.get_i64("dheight")) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as f64, h as f64),
        _ => return Err("no video track".into()),
    };
    // Fit inside size × size, keeping the display aspect.
    let scale = size as f64 / dw.max(dh);
    let w = ((dw * scale).round() as u32).max(1);
    let h = ((dh * scale).round() as u32).max(1);

    // mpv wants a 64-byte-aligned stride and pointer for its fast paths.
    let stride = (w as usize * 4).div_ceil(64) * 64;
    let mut storage = vec![0u8; stride * h as usize + 64];
    let offset = storage.as_ptr().align_offset(64);
    let frame = &mut storage[offset..offset + stride * h as usize];
    renderer.render_sw(w as i32, h as i32, stride, frame);

    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    for row in frame.chunks_exact(stride) {
        for px in row[..w as usize * 4].chunks_exact(4) {
            // bgr0 → BGRA; the fourth byte is unspecified, so make it opaque.
            pixels.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    drop(renderer);
    Ok((w, h, pixels))
}
