//! Media regression tests: a matrix of clips covering the codecs, containers,
//! HDR formats, subtitle types and odd corners (rotation, anamorphic pixels,
//! interlacing, legacy code pages) that VLC plays. Each clip goes through the
//! app's own code: its mpv options and `Player` for loading and tracks, a full
//! decode with no errors logged, the thumbnail renderer, and, on Linux, the
//! OpenGL render path the window uses, with colours checked against the
//! pattern the clip was made from.
//!
//! Clips are made with ffmpeg on first use (see `fixtures`). Without libmpv or
//! ffmpeg the tests pass with a note, unless OPEN_PREVIEW_MEDIA_TESTS=require,
//! as in CI; scripts/test-media.sh runs them against both the system libmpv
//! and, under Wine, the libmpv-2.dll the installer ships.

mod cases;
mod fixtures;
#[cfg(target_os = "linux")]
mod gl;

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::mpv::{Event, Mpv};
use crate::player::{Player, TrackKind};

pub type Rgb = [u8; 3];

/// The test pattern's quadrants, top left to bottom right: saturated
/// primaries show matrix and gamut mistakes, the light grey range mistakes.
pub const PATTERN: [Rgb; 4] = [[200, 40, 40], [40, 180, 60], [40, 60, 200], [230, 230, 230]];

const TIMEOUT: Duration = Duration::from_secs(30);

fn required() -> bool {
    std::env::var("OPEN_PREVIEW_MEDIA_TESTS").is_ok_and(|v| v == "require")
}

/// Skips (or, when required, fails) a test that can't run here.
fn unavailable(why: &str) {
    assert!(!required(), "{why}");
    eprintln!("skipped: {why}");
}

fn libmpv_loads() -> Result<(), String> {
    static LOADED: OnceLock<Result<(), String>> = OnceLock::new();
    LOADED.get_or_init(|| Mpv::new(&[]).map(drop)).clone()
}

/// The app's mpv options with test overrides after them (later ones win).
pub fn options(overrides: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    let mut opts = crate::mpv_options("");
    opts.extend(
        [
            ("config", "no"),
            ("load-scripts", "no"),
            ("resume-playback", "no"),
            ("save-position-on-quit", "no"),
            ("ytdl", "no"),
            ("terminal", "no"),
            ("ao", "null"),
            ("audio-fallback-to-null", "yes"),
        ]
        .into_iter()
        .chain(overrides.iter().copied())
        .map(|(k, v)| (k, v.to_string())),
    );
    opts
}

pub fn new_mpv(overrides: &[(&'static str, &str)]) -> Mpv {
    let opts = options(overrides);
    let refs: Vec<(&str, &str)> = opts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Mpv::new(&refs).expect("mpv")
}

fn check(case: &cases::Case) {
    if let Err(e) = libmpv_loads() {
        return unavailable(&format!("libmpv: {e}"));
    }
    let path = match fixtures::get(case) {
        Ok(p) => p,
        Err(e) => return unavailable(&format!("{}: {e}", case.file)),
    };
    let path = path.to_string_lossy();
    let mut failures = Vec::new();
    probe(case, &path, &mut failures);
    decode_through(case, &path, &mut failures);
    thumbnail(case, &path, &mut failures);
    #[cfg(target_os = "linux")]
    if case.source.is_some() || case.cover {
        gl::check(case, &path, &mut failures);
    }
    assert!(failures.is_empty(), "{}:\n  {}", case.file, failures.join("\n  "));
}

/// Opens the clip the way the app does and checks what `Player` finds.
fn probe(case: &cases::Case, path: &str, failures: &mut Vec<String>) {
    let mpv = Arc::new(new_mpv(&[("vo", "null")]));
    let mut player = Player::new(mpv.clone());
    player.open_paths(&[path.to_string()]);
    let deadline = Instant::now() + TIMEOUT;
    let mut sub_text = String::new();
    let want_video = !case.video.is_empty();
    loop {
        player.process_events();
        if let Some(e) = &player.last_error {
            failures.push(format!("failed to open: {e}"));
            return;
        }
        if let Some(t) = mpv.get_str("sub-text").filter(|t| !t.is_empty()) {
            sub_text = t;
        }
        let loaded = !player.loading && !player.idle && !player.tracks.is_empty();
        let ready = loaded
            && (!want_video || player.video_size.is_some())
            && (case.sub_text.is_none() || !sub_text.is_empty())
            && (case.channels.is_none() || mpv.get_i64("audio-params/channel-count").is_some())
            && player.duration > 0.0;
        if ready {
            break;
        }
        if Instant::now() > deadline {
            failures.push(format!(
                "timed out opening (loading {}, idle {}, {} tracks, video {:?}, duration {}, sub text {sub_text:?})",
                player.loading,
                player.idle,
                player.tracks.len(),
                player.video_size,
                player.duration
            ));
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let codecs = |kind: TrackKind| -> Vec<String> {
        let all: Vec<String> = (0..mpv.get_i64("track-list/count").unwrap_or(0))
            .filter(|n| {
                let ty = mpv.get_str(&format!("track-list/{n}/type")).unwrap_or_default();
                // Cover art shows as a video track, which is what we want to see.
                matches!((kind, ty.as_str()), (TrackKind::Video, "video") | (TrackKind::Audio, "audio") | (TrackKind::Sub, "sub"))
            })
            .map(|n| mpv.get_str(&format!("track-list/{n}/codec")).unwrap_or_default())
            .collect();
        assert_eq!(all.len(), player.tracks_of(kind).count(), "Player lists every {kind:?} track");
        all
    };
    for (kind, want) in [(TrackKind::Video, case.video), (TrackKind::Audio, case.audio), (TrackKind::Sub, case.subtitles)] {
        let got = codecs(kind);
        // Some mpv versions name the container flavour too: "webvtt-webm".
        let same = got.len() == want.len() && got.iter().zip(want).all(|(g, w)| g == w || g.starts_with(&format!("{w}-")));
        if !same {
            failures.push(format!("{kind:?} tracks: got {got:?}, want {want:?}"));
        }
        if !want.is_empty() && kind != TrackKind::Sub && !player.tracks_of(kind).any(|t| t.selected) {
            failures.push(format!("no {kind:?} track selected"));
        }
    }
    if let Some(want) = case.sub_text
        && sub_text != want
    {
        failures.push(format!("subtitle text: got {sub_text:?}, want {want:?}"));
    }
    if let Some(want) = case.duration
        && (player.duration - want).abs() > 0.25
    {
        failures.push(format!("duration {:.3}, want {want}", player.duration));
    }
    if let Some(want) = case.channels {
        let got = mpv.get_i64("audio-params/channel-count");
        if got != Some(want) {
            failures.push(format!("audio channels {got:?}, want {want}"));
        }
    }
    if want_video {
        let (w, h) = case.size;
        let want = case.display.unwrap_or((w as i64, h as i64));
        let got = player.video_size.unwrap_or_default();
        if (got.0 - want.0).abs() > 1 || (got.1 - want.1).abs() > 1 {
            failures.push(format!("display size {got:?}, want {want:?}"));
        }
        let gamma = mpv.get_str("video-params/gamma").unwrap_or_default();
        if let Some(want) = case.gamma
            && gamma != want
        {
            failures.push(format!("transfer {gamma:?}, want {want:?}"));
        }
        if case.gamma.is_none() && matches!(gamma.as_str(), "pq" | "hlg") {
            failures.push(format!("unexpected HDR transfer {gamma}"));
        }
        // Interlaced video is deinterlaced, as in VLC; progressive isn't.
        if let Some(active) = mpv.get_bool("deinterlace-active")
            && active != case.interlaced
        {
            failures.push(format!("deinterlacing active: {active}, want {}", case.interlaced));
        }
    }
    player.mpv.cmd(&["stop"]);
}

/// Set to a clip's path, makes `decode_child` decode it.
const DECODE_ONLY: &str = "OPEN_PREVIEW_DECODE_ONLY";

/// Decodes every frame and sample as fast as possible; no errors allowed.
/// Runs in a child process: FFmpeg's log goes to the first mpv instance in
/// a process, so with tests in parallel, errors would land on the wrong one.
fn decode_through(case: &cases::Case, path: &str, failures: &mut Vec<String>) {
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["--exact", "tests::decode_child", "--nocapture", "--test-threads=1"])
        .env(DECODE_ONLY, path)
        .output()
        .expect("run the decode child");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut reached = None;
    let mut failed = false;
    let mut errors = Vec::new();
    for line in stdout.lines() {
        match line.split_once('\t') {
            Some(("reached", t)) => reached = t.parse::<f64>().ok(),
            Some(("failed", e)) => {
                failed = true;
                failures.push(format!("playback failed: {e}"));
            }
            Some(("error", e)) if !case.allow_errors.iter().any(|a| e.contains(a)) => errors.push(e.to_string()),
            _ => {}
        }
    }
    match reached {
        None if !failed => {
            failures.push(format!("decode child failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
        }
        // The last frame starts one frame before the end.
        Some(t) if t < case.duration.unwrap_or(case.secs) - 0.25 => {
            failures.push(format!("playback stopped at {t:.2}s of {}", case.secs))
        }
        _ => {}
    }
    if !errors.is_empty() {
        failures.push(format!("mpv logged errors:\n    {}", errors.join("\n    ")));
    }
}

/// Not a test of its own: the child process `decode_through` starts.
#[test]
fn decode_child() {
    let Some(path) = std::env::var_os(DECODE_ONLY) else { return };
    // Off the line libtest started with this test's name.
    println!();
    let mpv = new_mpv(&[
        ("vo", "null"),
        ("untimed", "yes"),
        ("ao-null-untimed", "yes"),
        // Software decoding: probing for GPUs that aren't there logs errors.
        ("hwdec", "no"),
        ("keep-open", "yes"),
    ]);
    mpv.request_log_messages("error");
    mpv.observe(1, "eof-reached", crate::mpv::FORMAT_FLAG);
    mpv.cmd(&["loadfile", &path.to_string_lossy()]);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match mpv.poll_event() {
            Some(Event::Log { prefix, text }) => println!("error\t[{prefix}] {text}"),
            Some(Event::Property { id: 1, value: crate::mpv::PropValue::Flag(true) }) => {
                println!("reached\t{}", mpv.get_f64("time-pos").unwrap_or(0.0));
                break;
            }
            Some(Event::EndFile { error: Some(e) }) => {
                println!("failed\t{e}");
                break;
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(2)),
        }
        if Instant::now() > deadline {
            println!("failed\tdecoding didn't finish in {TIMEOUT:?}");
            break;
        }
    }
}

/// The Explorer thumbnail: a frame through the software renderer.
fn thumbnail(case: &cases::Case, path: &str, failures: &mut Vec<String>) {
    let result = crate::thumbnail::render(path, 160);
    if case.quads.is_none() {
        if let Ok((w, h, _)) = result {
            failures.push(format!("thumbnail of a clip without video: {w}×{h}"));
        }
        return;
    }
    match result {
        Err(e) => failures.push(format!("thumbnail failed: {e}")),
        Ok((w, h, bgra)) => {
            let (dw, dh) = case.display.unwrap_or((case.size.0 as i64, case.size.1 as i64));
            if (w as f64 / h as f64 - dw as f64 / dh as f64).abs() > 0.03 {
                failures.push(format!("thumbnail is {w}×{h}, want the shape of {dw}×{dh}"));
            }
            let rgba: Vec<u8> = bgra.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            dump(&format!("{}.thumbnail", case.file), &rgba, w as usize, h as usize);
            compare("thumbnail", &rgba, w as usize, h as usize, case, case.thumb_tol.unwrap_or(case.tol), failures);
        }
    }
}

/// With OPEN_PREVIEW_TEST_DUMP set to a folder, saves a rendered frame
/// there as a PPM image, for looking at a failure.
pub fn dump(name: &str, rgba: &[u8], w: usize, h: usize) {
    let Some(dir) = std::env::var_os("OPEN_PREVIEW_TEST_DUMP") else { return };
    let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
    ppm.extend(rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]));
    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), ppm);
}

/// Average colour of a small square around a point, from top-down RGBA.
fn sample(rgba: &[u8], w: usize, h: usize, fx: f64, fy: f64) -> Rgb {
    let (cx, cy) = ((fx * w as f64) as usize, (fy * h as f64) as usize);
    let r = (w.min(h) / 16).max(1);
    let mut sum = [0u64; 3];
    let mut n = 0;
    for y in cy.saturating_sub(r)..(cy + r).min(h) {
        for x in cx.saturating_sub(r)..(cx + r).min(w) {
            for c in 0..3 {
                sum[c] += rgba[(y * w + x) * 4 + c] as u64;
            }
            n += 1;
        }
    }
    sum.map(|s| (s / n) as u8)
}

pub fn compare(what: &str, rgba: &[u8], w: usize, h: usize, case: &cases::Case, tol: u8, failures: &mut Vec<String>) {
    let Some(mut want) = case.quads else { return };
    let spots = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)];
    let got: Vec<Rgb> = spots.iter().map(|&(x, y)| sample(rgba, w, h, x, y)).collect();
    if case.gamma.is_some() {
        // Tone mapping may dim HDR by an amount that depends on the curve and
        // the metadata. Hue, saturation and contrast must survive it, and the
        // picture must stay in sensible bounds: not washed out, not dark.
        let mean = |c: Rgb| c.iter().map(|&v| v as f64).sum::<f64>() / 3.0;
        let k = mean(got[3]) / mean(want[3]);
        if !(0.55..=1.1).contains(&k) {
            failures.push(format!("{what}: HDR shown at {:.0}% of SDR brightness ({got:?})", k * 100.0));
            return;
        }
        want = want.map(|c| c.map(|v| (v as f64 * k).round().min(255.0) as u8));
    }
    let off = got.iter().zip(&want).any(|(g, w)| g.iter().zip(w).any(|(a, b)| a.abs_diff(*b) > tol));
    if off {
        failures.push(format!("{what} colours {got:?}, want {want:?} ±{tol}"));
    }
}

#[test]
fn playlist_and_folder() {
    if let Err(e) = libmpv_loads() {
        return unavailable(&format!("libmpv: {e}"));
    }
    let clips: Vec<_> = ["h264.mp4", "vp9.webm"]
        .iter()
        .filter_map(|f| cases::find(f))
        .map(fixtures::get)
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    if clips.len() < 2 {
        return unavailable("playlist clips");
    }
    let dir = fixtures::dir().join("playlist");
    let folder = dir.join("folder");
    std::fs::create_dir_all(&folder).unwrap();
    for clip in &clips {
        std::fs::copy(clip, folder.join(clip.file_name().unwrap())).unwrap();
    }
    let m3u = dir.join("list.m3u");
    let lines: Vec<String> = clips.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    std::fs::write(&m3u, format!("#EXTM3U\n{}\n", lines.join("\n"))).unwrap();

    for (what, target) in [("m3u", &m3u), ("folder", &folder)] {
        let want = 2;
        let mpv = Arc::new(new_mpv(&[("vo", "null"), ("pause", "yes")]));
        let mut player = Player::new(mpv.clone());
        player.open_paths(&[target.to_string_lossy().into_owned()]);
        let deadline = Instant::now() + TIMEOUT;
        while player.playlist_count < want || player.video_size.is_none() {
            player.process_events();
            assert!(player.last_error.is_none(), "{what}: {:?}", player.last_error);
            assert!(Instant::now() < deadline, "{what}: playlist of {} entries", player.playlist_count);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(player.playlist_count, want, "{what}");
    }
}

#[test]
fn bad_file_fails_cleanly() {
    if let Err(e) = libmpv_loads() {
        return unavailable(&format!("libmpv: {e}"));
    }
    let dir = fixtures::dir().join("broken");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not-a-video.mkv");
    std::fs::write(&path, b"this is not a video".repeat(100)).unwrap();
    let path = path.to_string_lossy().into_owned();
    assert!(crate::thumbnail::render(&path, 64).is_err());

    let mpv = Arc::new(new_mpv(&[("vo", "null")]));
    let mut player = Player::new(mpv);
    player.open_paths(std::slice::from_ref(&path));
    let deadline = Instant::now() + TIMEOUT;
    while player.last_error.is_none() {
        player.process_events();
        assert!(Instant::now() < deadline, "no error reported for a broken file");
        std::thread::sleep(Duration::from_millis(5));
    }
}
