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

/// HDR to SDR for the software renderer, which otherwise maps 100 nits to
/// white and clips the rest. The picture's reference white (203 nits, BT.2408)
/// becomes SDR white and highlights roll off above it, as on an SDR screen.
const TONE_MAP: &str = "lavfi=[zscale=t=linear:npl=203,format=gbrpf32le,zscale=p=bt709,\
                        tonemap=tonemap=mobius:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv444p]";

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

pub(crate) fn render(path: &str, size: u32) -> Result<(u32, u32, Vec<u8>), String> {
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
    // Kept so a failure can say what mpv saw go wrong.
    mpv.request_log_messages("error");
    mpv.command(&["loadfile", path])?;

    let deadline = Instant::now() + TIMEOUT;
    let mut restarted: Option<Instant> = None;
    let mut tone_mapped = false;
    let mut last_error = None;
    let explain = |what: String, log: &Option<String>| match log {
        Some(l) => format!("{what} ({l})"),
        None => what,
    };
    loop {
        while let Some(event) = mpv.poll_event() {
            match event {
                Event::FileLoaded => rotate_in_filter(&mpv),
                Event::PlaybackRestart => restarted = Some(Instant::now()),
                Event::Log { prefix, text } => last_error = Some(format!("{prefix}: {text}")),
                Event::EndFile { error } => {
                    return Err(explain(error.unwrap_or_else(|| "no video".into()), &last_error));
                }
                Event::Shutdown => return Err("mpv shut down".into()),
                _ => {}
            }
        }
        // mpv hands each frame over once and keeps it as the current one if
        // nobody renders it within 200 ms, as can happen to a lone cover art
        // frame. Redrawing that is as good, so don't wait for another.
        let ready = restarted.is_some_and(|t| renderer.wants_frame() || t.elapsed() > Duration::from_millis(100));
        if ready {
            let hdr = matches!(mpv.get_str("video-params/gamma").as_deref(), Some("pq" | "hlg"));
            // Adding the filter redoes the frame, with another restart.
            if hdr && !tone_mapped && mpv.command(&["vf", "append", TONE_MAP]).is_ok() {
                tone_mapped = true;
                restarted = None;
                continue;
            }
            break;
        }
        if Instant::now() > deadline {
            return Err(explain("timed out waiting for a frame".into(), &last_error));
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
        for px in row[..w as usize * 4].as_chunks::<4>().0 {
            // bgr0 → BGRA; the fourth byte is unspecified, so make it opaque.
            pixels.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    drop(renderer);
    Ok((w, h, pixels))
}

/// The libmpv VO claims it can rotate, so mpv leaves rotation metadata (phone
/// videos) to it, but the software renderer can't and draws sideways or
/// aborts. Rotate in the filter chain instead.
fn rotate_in_filter(mpv: &Mpv) {
    let filter = match mpv.get_i64("current-tracks/video/demux-rotation").unwrap_or(0).rem_euclid(360) {
        90 => "transpose=clock",
        180 => "hflip,vflip",
        270 => "transpose=cclock",
        _ => return,
    };
    mpv.set_str("video-rotate", "no");
    mpv.set_str("vf", &format!("lavfi=[{filter}]"));
}
