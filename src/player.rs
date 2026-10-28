//! Playback state mirrored from mpv's observed properties, plus the commands
//! the UI issues.

use std::path::Path;
use std::sync::Arc;

use crate::mpv::{self, Event, Mpv, PropValue};

const SUBTITLE_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "sup", "idx", "smi", "lrc"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackKind {
    Video,
    Audio,
    Sub,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    pub start: f64,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub kind: TrackKind,
    pub label: String,
    pub detail: String,
    pub selected: bool,
}

// Observed property ids.
const P_TIME: u64 = 1;
const P_DURATION: u64 = 2;
const P_PAUSE: u64 = 3;
const P_VOLUME: u64 = 4;
const P_MUTE: u64 = 5;
const P_TRACKS: u64 = 6;
const P_TITLE: u64 = 7;
const P_SPEED: u64 = 8;
const P_EOF: u64 = 9;
const P_IDLE: u64 = 10;
const P_DWIDTH: u64 = 11;
const P_DHEIGHT: u64 = 12;
const P_PL_COUNT: u64 = 13;
const P_PL_POS: u64 = 14;
const P_BUFFERING: u64 = 16;
const P_CHAPTERS: u64 = 17;
const P_SUB_SCALE: u64 = 18;
const P_SUB_DELAY: u64 = 19;
const P_ROTATE: u64 = 20;

pub struct Player {
    pub mpv: Arc<Mpv>,
    pub time_pos: f64,
    pub duration: f64,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub title: String,
    pub speed: f64,
    pub eof: bool,
    pub idle: bool,
    pub loading: bool,
    pub buffering: bool,
    /// Displayed video size, rotation applied.
    pub video_size: Option<(i64, i64)>,
    /// Raw `dwidth`/`dheight` (0 while unknown) and the rotation mpv applies on top.
    dsize: (i64, i64),
    rotate: i64,
    pub playlist_count: i64,
    pub playlist_pos: i64,
    pub tracks: Vec<Track>,
    pub chapters: Vec<Chapter>,
    pub sub_scale: f64,
    pub sub_delay: f64,
    /// Set when the video's display size changes, consumed by the window logic.
    pub video_resized: bool,
    pub last_error: Option<String>,
}

impl Player {
    pub fn new(mpv: Arc<Mpv>) -> Self {
        use mpv::{FORMAT_DOUBLE as D, FORMAT_FLAG as F, FORMAT_INT64 as I, FORMAT_NONE as N, FORMAT_STRING as S};
        for (id, name, fmt) in [
            (P_TIME, "time-pos", D),
            (P_DURATION, "duration", D),
            (P_PAUSE, "pause", F),
            (P_VOLUME, "volume", D),
            (P_MUTE, "mute", F),
            (P_TRACKS, "track-list", N),
            (P_TITLE, "media-title", S),
            (P_SPEED, "speed", D),
            (P_EOF, "eof-reached", F),
            (P_IDLE, "idle-active", F),
            (P_DWIDTH, "dwidth", I),
            (P_DHEIGHT, "dheight", I),
            (P_PL_COUNT, "playlist-count", I),
            (P_PL_POS, "playlist-pos", I),
            (P_BUFFERING, "paused-for-cache", F),
            (P_CHAPTERS, "chapter-list", N),
            (P_SUB_SCALE, "sub-scale", D),
            (P_SUB_DELAY, "sub-delay", D),
            (P_ROTATE, "video-out-params/rotate", I),
        ] {
            mpv.observe(id, name, fmt);
        }
        Player {
            mpv,
            time_pos: 0.0,
            duration: 0.0,
            paused: false,
            volume: 100.0,
            muted: false,
            title: String::new(),
            speed: 1.0,
            eof: false,
            idle: true,
            loading: false,
            buffering: false,
            video_size: None,
            dsize: (0, 0),
            rotate: 0,
            playlist_count: 0,
            playlist_pos: -1,
            tracks: Vec::new(),
            chapters: Vec::new(),
            sub_scale: 1.0,
            sub_delay: 0.0,
            video_resized: false,
            last_error: None,
        }
    }

    pub fn has_media(&self) -> bool {
        !self.idle || self.loading
    }

    pub fn has_video(&self) -> bool {
        self.video_size.is_some()
    }

    pub fn aspect(&self) -> Option<f32> {
        self.video_size.filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| w as f32 / h as f32)
    }

    /// Recomputes `video_size` from `dsize` and `rotate`. `dwidth`/`dheight`
    /// ignore rotation, so a phone video stored as 1920×1080 with a 90° flag
    /// would otherwise look landscape.
    fn update_video_size(&mut self) {
        let (mut w, mut h) = self.dsize;
        if self.rotate.rem_euclid(180) == 90 {
            std::mem::swap(&mut w, &mut h);
        }
        let new = (w > 0 && h > 0).then_some((w, h));
        if new.is_some() && new != self.video_size {
            self.video_resized = true;
        }
        // Keep the old size while one half of the pair is still unknown.
        if new.is_some() || self.dsize == (0, 0) {
            self.video_size = new;
        }
    }

    /// Drains mpv's event queue. Returns true if anything changed.
    pub fn process_events(&mut self) -> bool {
        let mut changed = false;
        while let Some(ev) = self.mpv.poll_event() {
            changed = true;
            match ev {
                Event::Shutdown => {}
                Event::StartFile => {
                    self.loading = true;
                    self.last_error = None;
                }
                Event::FileLoaded => self.loading = false,
                Event::EndFile { error } => {
                    self.loading = false;
                    if let Some(e) = error {
                        self.last_error = Some(e);
                    }
                }
                Event::VideoReconfig | Event::PlaybackRestart => {}
                Event::Property { id, value } => self.apply(id, value),
            }
        }
        changed
    }

    fn apply(&mut self, id: u64, v: PropValue) {
        let f = |v: &PropValue| match v {
            PropValue::Double(d) => Some(*d),
            PropValue::Int(i) => Some(*i as f64),
            _ => None,
        };
        let b = |v: &PropValue| matches!(v, PropValue::Flag(true));
        let i = |v: &PropValue| match v {
            PropValue::Int(i) => Some(*i),
            _ => None,
        };
        match id {
            P_TIME => self.time_pos = f(&v).unwrap_or(0.0),
            P_DURATION => self.duration = f(&v).unwrap_or(0.0),
            P_PAUSE => self.paused = b(&v),
            P_VOLUME => self.volume = f(&v).unwrap_or(self.volume),
            P_MUTE => self.muted = b(&v),
            P_TRACKS => self.reload_tracks(),
            P_TITLE => {
                self.title = match v {
                    PropValue::Str(s) => s,
                    _ => String::new(),
                }
            }
            P_SPEED => self.speed = f(&v).unwrap_or(1.0),
            P_EOF => self.eof = b(&v),
            P_IDLE => self.idle = b(&v),
            P_DWIDTH => {
                self.dsize.0 = i(&v).unwrap_or(0);
                self.update_video_size();
            }
            P_DHEIGHT => {
                self.dsize.1 = i(&v).unwrap_or(0);
                self.update_video_size();
            }
            P_ROTATE => {
                self.rotate = i(&v).unwrap_or(0);
                self.update_video_size();
            }
            P_PL_COUNT => self.playlist_count = i(&v).unwrap_or(0),
            P_PL_POS => self.playlist_pos = i(&v).unwrap_or(-1),
            P_BUFFERING => self.buffering = b(&v),
            P_CHAPTERS => self.reload_chapters(),
            P_SUB_SCALE => self.sub_scale = f(&v).unwrap_or(1.0),
            P_SUB_DELAY => self.sub_delay = f(&v).unwrap_or(0.0),
            _ => {}
        }
    }

    fn reload_tracks(&mut self) {
        let m = &self.mpv;
        let count = m.get_i64("track-list/count").unwrap_or(0);
        self.tracks = (0..count)
            .filter_map(|n| {
                let p = |k: &str| format!("track-list/{n}/{k}");
                let kind = match m.get_str(&p("type"))?.as_str() {
                    "video" => TrackKind::Video,
                    "audio" => TrackKind::Audio,
                    "sub" => TrackKind::Sub,
                    _ => return None,
                };
                let id = m.get_i64(&p("id"))?;
                let title = m.get_str(&p("title")).filter(|s| !s.is_empty());
                let lang = m.get_str(&p("lang")).filter(|s| !s.is_empty()).map(|l| language_name(&l));
                let codec = m.get_str(&p("codec")).unwrap_or_default();
                let external = m.get_bool(&p("external")).unwrap_or(false);
                let label = match (&title, &lang) {
                    (Some(t), Some(l)) if !t.to_lowercase().contains(&l.to_lowercase()) => format!("{l} · {t}"),
                    (Some(t), _) => t.clone(),
                    (None, Some(l)) => l.clone(),
                    (None, None) => format!("Track {id}"),
                };
                let mut detail = codec_name(&codec);
                if kind == TrackKind::Audio
                    && let Some(ch) = m.get_i64(&p("demux-channel-count")) {
                        detail = format!("{detail} {}", channels_label(ch));
                    }
                if external {
                    detail = format!("{detail} · file");
                }
                Some(Track {
                    id,
                    kind,
                    label,
                    detail: detail.trim().to_string(),
                    selected: m.get_bool(&p("selected")).unwrap_or(false),
                })
            })
            .collect();
    }

    fn reload_chapters(&mut self) {
        let count = self.mpv.get_i64("chapter-list/count").unwrap_or(0);
        self.chapters = (0..count)
            .filter_map(|n| {
                let start = self.mpv.get_f64(&format!("chapter-list/{n}/time"))?;
                let title = self.mpv.get_str(&format!("chapter-list/{n}/title")).unwrap_or_default();
                Some(Chapter { start, title })
            })
            .collect();
    }

    pub fn tracks_of(&self, kind: TrackKind) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(move |t| t.kind == kind)
    }

    // --- commands ---------------------------------------------------------

    /// Opens files. Subtitle files are attached to the current video instead.
    pub fn open_paths(&mut self, paths: &[String]) {
        let (subs, media): (Vec<&String>, Vec<&String>) = paths.iter().partition(|p| is_subtitle(p));
        if !media.is_empty() {
            self.save_position();
            for (n, p) in media.iter().enumerate() {
                let mode = if n == 0 { "replace" } else { "append" };
                self.mpv.cmd(&["loadfile", p, mode]);
            }
            self.mpv.set_flag("pause", false);
        }
        if self.has_media() {
            for s in subs {
                self.mpv.cmd(&["sub-add", s, "select"]);
            }
        }
    }

    pub fn save_position(&self) {
        if self.has_media() {
            let _ = self.mpv.command(&["write-watch-later-config"]);
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.eof {
            self.seek_absolute(0.0, true);
            self.mpv.set_flag("pause", false);
        } else {
            self.mpv.set_flag("pause", !self.paused);
        }
    }

    pub fn seek_relative(&self, secs: f64) {
        self.mpv.cmd(&["seek", &format!("{secs}"), "relative+exact"]);
    }

    pub fn seek_absolute(&self, secs: f64, exact: bool) {
        let flags = if exact { "absolute+exact" } else { "absolute+keyframes" };
        self.mpv.cmd(&["seek", &format!("{:.3}", secs.max(0.0)), flags]);
    }

    pub fn set_volume(&mut self, v: f64) {
        let v = v.clamp(0.0, 150.0);
        self.volume = v;
        self.mpv.set_double("volume", v);
        if self.muted && v > 0.0 {
            self.set_muted(false);
        }
    }

    pub fn set_muted(&mut self, m: bool) {
        self.muted = m;
        self.mpv.set_flag("mute", m);
    }

    pub fn set_speed(&mut self, s: f64) {
        let s = (s * 100.0).round() / 100.0;
        self.speed = s.clamp(0.1, 4.0);
        self.mpv.set_double("speed", self.speed);
    }

    pub fn select_track(&mut self, kind: TrackKind, id: Option<i64>) {
        let prop = match kind {
            TrackKind::Video => "vid",
            TrackKind::Audio => "aid",
            TrackKind::Sub => "sid",
        };
        let v = id.map(|i| i.to_string()).unwrap_or_else(|| "no".into());
        self.mpv.set_str(prop, &v);
    }

    pub fn cycle_frame(&self, forward: bool) {
        self.mpv.cmd(&[if forward { "frame-step" } else { "frame-back-step" }]);
    }

    pub fn set_sub_scale(&self, v: f64) {
        self.mpv.set_double("sub-scale", v.clamp(0.3, 3.0));
    }

    pub fn set_sub_delay(&self, v: f64) {
        self.mpv.set_double("sub-delay", v);
    }
}

pub fn is_subtitle(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUBTITLE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// How a codec is usually written, rather than mpv's lowercase id.
fn codec_name(codec: &str) -> String {
    let name = match codec {
        "subrip" => "SubRip",
        "ass" | "ssa" => "ASS",
        "webvtt" => "WebVTT",
        "hdmv_pgs_subtitle" => "PGS",
        "dvd_subtitle" => "VobSub",
        "mov_text" => "Timed text",
        "aac" => "AAC",
        "ac3" => "AC-3",
        "eac3" => "E-AC-3",
        "dts" => "DTS",
        "truehd" => "TrueHD",
        "flac" => "FLAC",
        "opus" => "Opus",
        "vorbis" => "Vorbis",
        "mp3" => "MP3",
        "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" => "PCM",
        "h264" => "H.264",
        "hevc" => "HEVC",
        "av1" => "AV1",
        "vp9" => "VP9",
        other => return other.to_string(),
    };
    name.to_string()
}

fn channels_label(ch: i64) -> String {
    match ch {
        1 => "Mono".into(),
        2 => "Stereo".into(),
        6 => "5.1".into(),
        8 => "7.1".into(),
        n => format!("{n}ch"),
    }
}

fn language_name(code: &str) -> String {
    let name = match code.to_ascii_lowercase().as_str() {
        "en" | "eng" => "English",
        "pt" | "por" => "Portuguese",
        "pt-br" => "Portuguese (Brazil)",
        "es" | "spa" => "Spanish",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "it" | "ita" => "Italian",
        "ja" | "jpn" => "Japanese",
        "ko" | "kor" => "Korean",
        "zh" | "chi" | "zho" => "Chinese",
        "ru" | "rus" => "Russian",
        "ar" | "ara" => "Arabic",
        "nl" | "dut" | "nld" => "Dutch",
        "pl" | "pol" => "Polish",
        "sv" | "swe" => "Swedish",
        "tr" | "tur" => "Turkish",
        "hi" | "hin" => "Hindi",
        "und" => "Unknown",
        _ => return code.to_string(),
    };
    name.to_string()
}

pub fn format_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
