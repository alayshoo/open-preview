//! File types Open Preview opens, in one place: the open dialogs, drag and
//! drop, and the installer's registrations (kept in step by a test) all use
//! these lists. Everything here plays with the bundled libmpv; the list
//! follows what VLC registers, minus formats FFmpeg can't decode (MIDI, SID).

/// Registered as `OpenPreview.Video`.
pub const VIDEO: &[&str] = &[
    "3g2", "3gp", "3gp2", "3gpp", "amv", "asf", "avi", "bik", "divx", "dv", "dvr-ms", "evo", "f4v", "flv", "gxf",
    "m1v", "m2t", "m2ts", "m2v", "m4v", "mk3d", "mkv", "mov", "mp4", "mp4v", "mpe", "mpeg", "mpg", "mpv", "mts", "mxf",
    "nsv", "nut", "nuv", "ogm", "ogv", "ogx", "qt", "rm", "rmvb", "thp", "tod", "trp", "ts", "tts", "vob", "vro", "webm",
    "wm", "wmv", "wtv", "y4m",
];

/// Registered as `OpenPreview.Audio`. The last five are tracker music (libopenmpt).
pub const AUDIO: &[&str] = &[
    "3ga", "aac", "ac3", "adts", "aif", "aifc", "aiff", "amr", "ape", "au", "awb", "caf", "dts", "eac3", "ec3", "flac",
    "gsm", "m4a", "m4b", "mka", "mlp", "mp1", "mp2", "mp3", "mpa", "mpc", "mpga", "oga", "ogg", "oma", "opus", "qcp",
    "ra", "spx", "tak", "thd", "tta", "voc", "w64", "wav", "weba", "wma", "wv", "it", "mod", "mptm", "s3m", "xm",
];

/// Opened from the dialog or by dropping, but not registered with Windows:
/// playlists, and disc images, which Explorer mounts by default. mpv also
/// takes DVD and Blu-ray folders (VIDEO_TS, BDMV).
pub const OTHER: &[&str] = &["m3u", "m3u8", "pls", "cue", "iso"];

/// Attached to the playing video rather than opened on their own.
pub const SUBTITLE: &[&str] =
    &["srt", "ass", "ssa", "vtt", "sub", "sup", "idx", "smi", "sami", "lrc", "mpl", "jss", "rt", "pjs", "aqt"];

/// Every type the open dialog's "Media" filter offers.
pub fn media() -> Vec<&'static str> {
    [VIDEO, AUDIO, OTHER].concat()
}

pub fn is_subtitle(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUBTITLE.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_type_is_listed_twice() {
        let mut all = [VIDEO, AUDIO, OTHER, SUBTITLE].concat();
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "an extension appears in more than one list");
    }

    #[test]
    fn subtitles_are_told_apart() {
        assert!(is_subtitle("C:\\Films\\Movie.EN.SRT"));
        assert!(is_subtitle("movie.idx"));
        assert!(!is_subtitle("movie.mkv"));
        assert!(!is_subtitle("srt"));
    }

    /// The installer registers exactly VIDEO and AUDIO, each in all three
    /// places Windows looks: "Open with", the ProgID, and Default apps.
    #[test]
    fn installer_registers_every_type() {
        let wxs = include_str!("../installer/open-preview.wxs");
        let mut missing = Vec::new();
        for (list, progid) in [(VIDEO, "OpenPreview.Video"), (AUDIO, "OpenPreview.Audio")] {
            for ext in list {
                for (place, needle) in [
                    ("Open with", format!(r#"SupportedTypes" Name=".{ext}" "#)),
                    ("ProgID", format!(r#"\.{ext}\OpenWithProgids" Name="{progid}" "#)),
                    ("Default apps", format!(r#"FileAssociations" Name=".{ext}" Type="string" Value="{progid}" "#)),
                ] {
                    if !wxs.contains(&needle) {
                        missing.push(format!(".{ext} ({place})"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "not registered by the installer: {}", missing.join(", "));

        // And nothing registered that the app doesn't list.
        let listed = [VIDEO, AUDIO].concat();
        let extra: Vec<&str> = wxs
            .split(r#"SupportedTypes" Name="."#)
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .filter(|ext| !listed.contains(ext))
            .collect();
        assert!(extra.is_empty(), "registered but not in formats.rs: {extra:?}");
        let count = |needle: &str| wxs.matches(needle).count();
        assert_eq!(count(r#"SupportedTypes" Name="."#), listed.len());
        assert_eq!(count(r#"\OpenWithProgids" Name="OpenPreview."#), listed.len());
        assert_eq!(count(r#"FileAssociations" Name="."#), listed.len());
    }
}
