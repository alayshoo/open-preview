//! Makes the test clips with ffmpeg, once, into target/media-fixtures (or
//! $OPEN_PREVIEW_FIXTURES), one folder per clip so side-by-side subtitle
//! files can't be picked up by the wrong one. A clip is rebuilt when the
//! ffmpeg command that made it changes.

use std::path::PathBuf;
use std::process::Command;

use super::cases::{Case, Source};
use super::{PATTERN, Rgb};

pub const SUB_TEXT: &str = "Open Preview";
/// Western European text in a legacy code page, for charset detection.
pub const SUB_TEXT_CP1252: &str = "Été à Lisboa, café é ótimo";

/// The picture embedded as cover art. Kept out of the clip's folder, where
/// mpv would also find it as a separate cover, as VLC does.
fn art(folder: &std::path::Path) -> PathBuf {
    folder.with_extension("art.png")
}

pub fn dir() -> PathBuf {
    std::env::var_os("OPEN_PREVIEW_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/media-fixtures"))
}

fn hex(c: Rgb) -> String {
    format!("0x{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// Four flat quadrants (see `PATTERN`) with a grey bar moving across the
/// middle, so inter-frame coding has something to do. RGB, `w`×`h`.
fn pattern(w: u32, h: u32, fps: &str, secs: f64) -> String {
    let quad = |c: Rgb, label: &str| format!("color=c={}:s=160x90:r={fps}:d={secs}[{label}]", hex(c));
    [
        quad(PATTERN[0], "a"),
        quad(PATTERN[1], "b"),
        quad(PATTERN[2], "c"),
        quad(PATTERN[3], "d"),
        format!("color=c=0x808080:s=16x8:r={fps}:d={secs}[bar]"),
        "[a][b]hstack[t]".into(),
        "[c][d]hstack[u]".into(),
        "[t][u]vstack,format=rgb24[q]".into(),
        format!("[q][bar]overlay=x='mod(t*160,W)':y=(H-h)/2,scale={w}:{h}:flags=bicubic,setsar=1,format=rgb24[out0]"),
    ]
    .join(";")
}

fn srt(text: &str) -> String {
    format!("1\r\n00:00:00,000 --> 00:00:01,900\r\n{text}\r\n\r\n")
}

fn ffmpeg_args(case: &Case, file: &str, folder: &std::path::Path) -> Vec<String> {
    let mut a: Vec<String> = ["-nostdin", "-y", "-v", "error"].map(String::from).to_vec();
    let mut maps = Vec::new();
    // Output options, which ffmpeg wants after every input.
    let mut out = Vec::new();
    let mut input = 0;
    let (w, h) = case.size;
    if let Some(src) = case.source {
        a.extend(case.input_args.split_whitespace().map(String::from));
        a.extend(["-f".into(), "lavfi".into(), "-i".into(), pattern(w, h, case.fps, case.secs)]);
        maps.push(format!("{input}:v"));
        input += 1;
        let tags: &[&str] = match src {
            Source::Rgb => &[],
            Source::Yuv601 | Source::Untagged601 => &["scale=out_color_matrix=bt601:out_range=tv"],
            Source::Yuv709 | Source::Untagged709 => &["scale=out_color_matrix=bt709:out_range=tv"],
            Source::Full601 => &["scale=out_color_matrix=bt601:out_range=pc"],
            Source::Pq => {
                &["zscale=tin=iec61966-2-1:pin=bt709:min=gbr:rin=full:t=smpte2084:p=bt2020:m=bt2020nc:r=limited:npl=203"]
            }
            Source::Hlg => {
                &["zscale=tin=iec61966-2-1:pin=bt709:min=gbr:rin=full:t=arib-std-b67:p=bt2020:m=bt2020nc:r=limited:npl=203"]
            }
        };
        if !tags.is_empty() {
            out.extend(["-vf".into(), tags.join(",")]);
        }
        let meta: &[&str] = match src {
            // BT.709 primaries: mpv's software renderer converts SMPTE-C ones to
            // the display's, its OpenGL one doesn't, and either is fine.
            Source::Yuv601 => &["-colorspace", "smpte170m", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"],
            Source::Yuv709 => &["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"],
            Source::Full601 => &["-colorspace", "smpte170m", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "pc"],
            Source::Pq => &["-colorspace", "bt2020nc", "-color_primaries", "bt2020", "-color_trc", "smpte2084", "-color_range", "tv"],
            Source::Hlg => &["-colorspace", "bt2020nc", "-color_primaries", "bt2020", "-color_trc", "arib-std-b67", "-color_range", "tv"],
            Source::Rgb | Source::Untagged601 | Source::Untagged709 => &[],
        };
        out.extend(meta.iter().map(|s| s.to_string()));
    }
    if let Some(ch) = case.audio_channels {
        let layout = match ch {
            1 => "mono",
            2 => "stereo",
            6 => "5.1",
            8 => "7.1",
            _ => panic!("no layout for {ch} channels"),
        };
        // A tone on the first channel, a different one on the rest.
        let exprs = (0..ch).map(|c| format!("0.4*sin({}*2*PI*t)", 440 + 110 * c)).collect::<Vec<_>>().join("|");
        let src = format!("aevalsrc={exprs}:s={}:d={}:c={layout}", case.sample_rate, case.secs);
        a.extend(["-f".into(), "lavfi".into(), "-i".into(), src]);
        maps.push(format!("{input}:a"));
        input += 1;
    }
    for _ in case.subs {
        a.extend(["-i".into(), folder.join("embedded.srt").to_string_lossy().into_owned()]);
        maps.push(format!("{input}:s"));
        input += 1;
    }
    if case.cover {
        a.extend(["-i".into(), art(folder).to_string_lossy().into_owned()]);
        maps.push(format!("{input}:v"));
    }
    for m in maps {
        a.extend(["-map".into(), m]);
    }
    a.extend(out);
    for (n, codec) in case.subs.iter().enumerate() {
        a.extend([format!("-c:s:{n}"), codec.to_string(), format!("-metadata:s:s:{n}"), "language=eng".into()]);
    }
    // Flagged default, as players (VLC too) only show those, or forced ones, unasked.
    if !case.subs.is_empty() {
        a.extend(["-disposition:s:0".into(), "default".into()]);
    }
    if case.cover {
        a.extend(["-disposition:v:0".into(), "attached_pic".into()]);
    }
    a.extend(case.args.split_whitespace().map(String::from));
    if !case.args.contains("-frames:v") {
        a.extend(["-t".into(), case.secs.to_string()]);
    }
    a.push(file.into());
    a
}

/// The clip's path, made first if needed. Err says why it couldn't be.
pub fn get(case: &Case) -> Result<PathBuf, String> {
    let folder = dir().join(case.file.replace('.', "_"));
    let path = folder.join(case.file);
    let stamp = folder.join("ffmpeg-args.txt");
    let part = format!("part.{}", case.file);
    let args = ffmpeg_args(case, &folder.join(&part).to_string_lossy(), &folder);
    // The stamp records the command without the folder, which differs between
    // hosts, as do path separators (Windows tests use clips Linux made).
    let recorded = args.join(" ").replace(&*folder.to_string_lossy(), "").replace('\\', "/");
    if path.exists() && std::fs::read_to_string(&stamp).ok().as_deref() == Some(recorded.as_str()) {
        return Ok(path);
    }
    if std::env::var_os("OPEN_PREVIEW_NO_FFMPEG").is_some() {
        return Err(format!("{} is missing or stale and OPEN_PREVIEW_NO_FFMPEG is set", path.display()));
    }
    // From scratch: leftovers would be found as side files.
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("embedded.srt"), srt(SUB_TEXT)).map_err(|e| e.to_string())?;
    if case.cover {
        let cover = ["-nostdin", "-y", "-v", "error", "-f", "lavfi", "-i", &pattern(320, 180, "1", 1.0), "-frames:v", "1"];
        run(&cover.iter().map(|s| s.to_string()).chain([art(&folder).to_string_lossy().into()]).collect::<Vec<_>>())?;
    }
    for side in case.sidecars {
        let bytes = match side.cp1252 {
            false => srt(SUB_TEXT).into_bytes(),
            true => srt(SUB_TEXT_CP1252).chars().map(|c| u8::try_from(c as u32).expect("Latin-1 only")).collect(),
        };
        std::fs::write(folder.join(side.name), bytes).map_err(|e| e.to_string())?;
    }
    run(&args)?;
    std::fs::rename(folder.join(&part), &path).map_err(|e| e.to_string())?;
    std::fs::write(&stamp, recorded).map_err(|e| e.to_string())?;
    Ok(path)
}

fn run(args: &[String]) -> Result<(), String> {
    let out = Command::new("ffmpeg").args(args).output().map_err(|e| format!("can't run ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffmpeg {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}
