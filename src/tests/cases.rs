//! The clip matrix. Each entry is made by ffmpeg (see `fixtures`) and becomes
//! one test. Codec names are mpv's (`track-list/N/codec`, FFmpeg's ids).

use super::{PATTERN, Rgb};

/// How the RGB test pattern is turned into the encoder's input.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    /// Left for the encoder to convert; for RGB and palette codecs.
    Rgb,
    /// BT.601 limited range, tagged as such.
    Yuv601,
    /// BT.601 without tags: players have to guess, and SD guesses 601.
    Untagged601,
    Yuv709,
    /// BT.709 without tags, at HD size, which players guess as 709.
    Untagged709,
    /// BT.601 full range (JPEG levels), tagged.
    Full601,
    /// HDR10: BT.2020, PQ, 10-bit; the pattern's white at 203 nits (BT.2408).
    Pq,
    /// HLG, BT.2020, 10-bit.
    Hlg,
}

pub struct Sidecar {
    pub name: &'static str,
    pub cp1252: bool,
}

pub struct Case {
    pub file: &'static str,
    // --- how ffmpeg makes it
    pub source: Option<Source>,
    /// ffmpeg options for the pattern input, like rotation flags.
    pub input_args: &'static str,
    pub size: (u32, u32),
    pub fps: &'static str,
    pub secs: f64,
    pub audio_channels: Option<u32>,
    pub sample_rate: u32,
    /// ffmpeg encoders for embedded subtitle tracks, all saying `SUB_TEXT`.
    pub subs: &'static [&'static str],
    pub sidecars: &'static [Sidecar],
    /// An attached picture (cover art) after the other streams.
    pub cover: bool,
    /// Codecs, container options and anything else, split on whitespace.
    pub args: &'static str,
    // --- what Open Preview should find
    pub video: &'static [&'static str],
    pub audio: &'static [&'static str],
    pub subtitles: &'static [&'static str],
    pub channels: Option<i64>,
    /// What mpv should report; None where the container doesn't say.
    pub duration: Option<f64>,
    /// Display size after aspect and rotation; defaults to `size`.
    pub display: Option<(i64, i64)>,
    /// Expected colours of the quadrants, top left to bottom right.
    pub quads: Option<[Rgb; 4]>,
    /// Allowed difference per channel, out of 255.
    pub tol: u8,
    /// The same for the thumbnail, where it differs.
    pub thumb_tol: Option<u8>,
    /// `video-params/gamma`, for HDR.
    pub gamma: Option<&'static str>,
    pub interlaced: bool,
    /// Subtitle text expected on screen at the start.
    pub sub_text: Option<&'static str>,
    /// Parts of error messages that are expected for this clip.
    pub allow_errors: &'static [&'static str],
}

const BASE: Case = Case {
    file: "",
    source: None,
    input_args: "",
    size: (320, 180),
    fps: "25",
    secs: 2.0,
    audio_channels: None,
    sample_rate: 48000,
    subs: &[],
    sidecars: &[],
    cover: false,
    args: "",
    video: &[],
    audio: &[],
    subtitles: &[],
    channels: None,
    duration: Some(2.0),
    display: None,
    quads: None,
    tol: 10,
    thumb_tol: None,
    gamma: None,
    interlaced: false,
    sub_text: None,
    allow_errors: &[],
};

/// A video clip of the pattern, BT.601 by default.
const fn video(file: &'static str, codec: &'static [&'static str], args: &'static str) -> Case {
    Case { file, source: Some(Source::Yuv601), video: codec, quads: Some(PATTERN), args, ..BASE }
}

/// Two seconds of tones.
const fn audio(file: &'static str, codec: &'static [&'static str], channels: u32, args: &'static str) -> Case {
    Case { file, audio_channels: Some(channels), audio: codec, channels: Some(channels as i64), args, ..BASE }
}

impl Case {
    const fn sound(self, codec: &'static [&'static str], channels: u32) -> Case {
        Case { audio_channels: Some(channels), audio: codec, channels: Some(channels as i64), ..self }
    }
    const fn src(self, source: Source) -> Case {
        Case { source: Some(source), ..self }
    }
    const fn size(self, w: u32, h: u32) -> Case {
        Case { size: (w, h), ..self }
    }
    const fn display(self, w: i64, h: i64) -> Case {
        Case { display: Some((w, h)), ..self }
    }
    const fn rate(self, sample_rate: u32) -> Case {
        Case { sample_rate, ..self }
    }
    const fn fps(self, fps: &'static str) -> Case {
        Case { fps, ..self }
    }
    const fn tol(self, tol: u8) -> Case {
        Case { tol, ..self }
    }
    const fn thumb_tol(self, tol: u8) -> Case {
        Case { thumb_tol: Some(tol), ..self }
    }
    const fn quads(self, quads: [Rgb; 4]) -> Case {
        Case { quads: Some(quads), ..self }
    }
    const fn hdr(self, gamma: &'static str, source: Source) -> Case {
        Case { gamma: Some(gamma), source: Some(source), ..self }
    }
    const fn interlaced(self) -> Case {
        Case { interlaced: true, ..self }
    }
    const fn subs(self, encoders: &'static [&'static str], codecs: &'static [&'static str]) -> Case {
        Case { subs: encoders, subtitles: codecs, sub_text: Some(super::fixtures::SUB_TEXT), ..self }
    }
    const fn sidecars(self, sidecars: &'static [Sidecar], codecs: &'static [&'static str], text: &'static str) -> Case {
        Case { sidecars, subtitles: codecs, sub_text: Some(text), ..self }
    }
    const fn cover(self, codec: &'static [&'static str]) -> Case {
        Case { cover: true, video: codec, quads: Some(PATTERN), ..self }
    }
    const fn rotate(self, input_args: &'static str) -> Case {
        Case { input_args, ..self }
    }
    const fn duration(self, duration: Option<f64>) -> Case {
        Case { duration, ..self }
    }
    const fn allow(self, errors: &'static [&'static str]) -> Case {
        Case { allow_errors: errors, ..self }
    }
}

const HDR10: &str = "-c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -x265-params log-level=error:hdr10=1:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400";

/// Transport streams have no index, so seeking needs keyframes nearby, as
/// broadcasts and discs have them: every half second here.
const HDR10_TS: &str = "-c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -g 12 -x265-params log-level=error:open-gop=0:hdr10=1:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400 -mpegts_m2ts_mode 1";

/// The pattern turned 90° anticlockwise, as a rotation flag asks.
const ROTATED: [Rgb; 4] = [PATTERN[1], PATTERN[3], PATTERN[0], PATTERN[2]];

macro_rules! cases {
    ($($name:ident = $case:expr;)*) => {
        pub const ALL: &[(&str, Case)] = &[$((stringify!($name), $case)),*];
        $(
            #[test]
            fn $name() {
                super::check(&ALL.iter().find(|(n, _)| *n == stringify!($name)).unwrap().1);
            }
        )*
    };
}

pub fn find(file: &str) -> Option<&'static Case> {
    ALL.iter().map(|(_, c)| c).find(|c| c.file == file)
}

cases! {
    // --- H.264 / AVC
    h264_mp4 = video("h264.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac").sound(&["aac"], 2);
    h264_bt709_mkv = video("h264-bt709.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").src(Source::Yuv709);
    h264_untagged_hd = video("h264-untagged-hd.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").src(Source::Untagged709).size(1280, 720);
    h264_untagged_sd = video("h264-untagged-sd.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").src(Source::Untagged601).size(640, 360);
    h264_full_range = video("h264-full-range.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").src(Source::Full601);
    h264_high10 = video("h264-high10.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p10le");
    h264_422_10bit = video("h264-422-10bit.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv422p10le");
    h264_444 = video("h264-444.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv444p");
    h264_rotated = video("h264-rotated.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").rotate("-autorotate 0 -display_rotation 90").display(180, 320).quads(ROTATED);
    h264_mov_alac = video("h264-alac.mov", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a alac").sound(&["alac"], 2);
    h264_flv = video("h264.flv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac").sound(&["aac"], 2);
    h264_interlaced_ts = video("h264-interlaced.ts", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -g 12 -flags +ildct+ilme -x264-params interlaced=1:tff=1 -c:a ac3").size(720, 576).fps("25").sound(&["ac3"], 2).interlaced();
    h264_m2ts = video("h264.m2ts", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -g 12 -c:a ac3 -mpegts_m2ts_mode 1").sound(&["ac3"], 6);

    // --- HEVC / H.265
    hevc_mkv = video("hevc.mkv", &["hevc"], "-c:v libx265 -preset ultrafast -x265-params log-level=error -pix_fmt yuv420p -c:a eac3").sound(&["eac3"], 6);
    hevc_main10_hvc1_mp4 = video("hevc-main10-hvc1.mp4", &["hevc"], "-c:v libx265 -preset ultrafast -x265-params log-level=error -pix_fmt yuv420p10le -tag:v hvc1 -c:a aac").sound(&["aac"], 2);
    hevc_main12 = video("hevc-main12.mkv", &["hevc"], "-c:v libx265 -preset ultrafast -x265-params log-level=error -pix_fmt yuv420p12le");
    hevc_444_10bit = video("hevc-444-10bit.mkv", &["hevc"], "-c:v libx265 -preset ultrafast -x265-params log-level=error -pix_fmt yuv444p10le");

    // --- HDR
    hdr10_hevc_mkv = video("hdr10-hevc.mkv", &["hevc"], HDR10).hdr("pq", Source::Pq).tol(24);
    hdr10_hevc_mp4 = video("hdr10-hevc.mp4", &["hevc"], HDR10).hdr("pq", Source::Pq).tol(24);
    hdr10_hevc_m2ts = video("hdr10-hevc.m2ts", &["hevc"], HDR10_TS).hdr("pq", Source::Pq).tol(24);
    hlg_hevc_mkv = video("hlg-hevc.mkv", &["hevc"], "-c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -x265-params log-level=error:colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc").hdr("hlg", Source::Hlg).tol(30);
    hdr10_av1_mkv = video("hdr10-av1.mkv", &["av1"], "-c:v libsvtav1 -preset 12 -pix_fmt yuv420p10le").hdr("pq", Source::Pq).tol(24);
    hdr10_vp9_webm = video("hdr10-vp9.webm", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -profile:v 2 -pix_fmt yuv420p10le").hdr("pq", Source::Pq).tol(24);

    // --- AV1, VP9, VP8
    av1_mp4 = video("av1.mp4", &["av1"], "-c:v libsvtav1 -preset 12 -pix_fmt yuv420p -c:a libopus").sound(&["opus"], 2);
    av1_10bit_webm = video("av1-10bit.webm", &["av1"], "-c:v libsvtav1 -preset 12 -pix_fmt yuv420p10le -c:a libopus").sound(&["opus"], 2);
    vp9_webm = video("vp9.webm", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -pix_fmt yuv420p -c:a libvorbis").sound(&["vorbis"], 2);
    vp9_10bit = video("vp9-10bit.mkv", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -profile:v 2 -pix_fmt yuv420p10le");
    vp9_444 = video("vp9-444.webm", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -profile:v 1 -pix_fmt yuv444p");
    vp9_alpha = video("vp9-alpha.webm", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -pix_fmt yuva420p");
    vp9_mp4 = video("vp9.mp4", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -pix_fmt yuv420p -c:a libopus").sound(&["opus"], 2);
    vp8_webm = video("vp8.webm", &["vp8"], "-c:v libvpx -deadline realtime -cpu-used 8 -b:v 1M -c:a libvorbis").sound(&["vorbis"], 2);

    // --- MPEG-1/2/4, DVD and broadcast
    mpeg2_mpg = video("mpeg2.mpg", &["mpeg2video"], "-c:v mpeg2video -q:v 2 -c:a mp2").sound(&["mp2"], 2);
    mpeg2_dvd_vob = video("mpeg2-dvd.vob", &["mpeg2video"], "-f dvd -c:v mpeg2video -q:v 2 -aspect 16:9 -c:a ac3").size(720, 480).fps("30000/1001").display(853, 480).sound(&["ac3"], 2);
    mpeg2_interlaced_ts = video("mpeg2-interlaced.ts", &["mpeg2video"], "-c:v mpeg2video -q:v 2 -flags +ildct+ilme -top 1 -c:a mp2").size(720, 576).sound(&["mp2"], 2).interlaced();
    mpeg1_mpg = video("mpeg1.mpg", &["mpeg1video"], "-c:v mpeg1video -q:v 2 -c:a mp2").sound(&["mp2"], 2);
    mpeg4_avi = video("mpeg4.avi", &["mpeg4"], "-c:v mpeg4 -q:v 2 -c:a libmp3lame").sound(&["mp3"], 2);
    xvid_avi = video("xvid.avi", &["mpeg4"], "-c:v libxvid -q:v 2 -c:a libmp3lame").sound(&["mp3"], 2);
    msmpeg4v3_avi = video("msmpeg4v3.avi", &["msmpeg4v3"], "-c:v msmpeg4 -q:v 2 -c:a libmp3lame").sound(&["mp3"], 2);
    msmpeg4v2_avi = video("msmpeg4v2.avi", &["msmpeg4v2"], "-c:v msmpeg4v2 -q:v 2");
    h263_3gp = video("h263.3gp", &["h263"], "-c:v h263 -q:v 2 -c:a aac -ar 16000").size(352, 288).sound(&["aac"], 1).rate(16000);
    h261_avi = video("h261.avi", &["h261"], "-c:v h261 -q:v 2").size(352, 288).tol(14).allow(&["first frame is no keyframe"]);
    mpeg2_mxf = video("mpeg2.mxf", &["mpeg2video"], "-c:v mpeg2video -q:v 2 -pix_fmt yuv422p -c:a pcm_s16le").size(720, 576).sound(&["pcm_s16le"], 1);
    mpeg2_wtv = video("mpeg2.wtv", &["mpeg2video"], "-c:v mpeg2video -q:v 2 -c:a ac3").sound(&["ac3"], 2);

    // --- Windows Media, Flash, Real, QuickTime, Ogg
    wmv2_wmv = video("wmv2.wmv", &["wmv2"], "-c:v wmv2 -q:v 2 -c:a wmav2").sound(&["wmav2"], 2);
    wmv1_asf = video("wmv1.asf", &["wmv1"], "-c:v wmv1 -q:v 2 -c:a wmav1").sound(&["wmav1"], 2);
    flv1_flv = video("flv1.flv", &["flv1"], "-c:v flv -q:v 2 -c:a libmp3lame -ar 44100").sound(&["mp3"], 2).rate(44100);
    rv20_rm = video("rv20.rm", &["rv20"], "-c:v rv20 -q:v 2 -c:a real_144 -ar 8000").sound(&["ra_144"], 1).rate(8000);
    theora_ogv = video("theora.ogv", &["theora"], "-c:v libtheora -q:v 8 -c:a libvorbis").sound(&["vorbis"], 2);
    svq1_mov = video("svq1.mov", &["svq1"], "-c:v svq1").tol(16);
    cinepak_mov = video("cinepak.mov", &["cinepak"], "-c:v cinepak").src(Source::Rgb).tol(24);
    qtrle_mov = video("qtrle.mov", &["qtrle"], "-c:v qtrle").src(Source::Rgb).tol(4);
    msvideo1_avi = video("msvideo1.avi", &["msvideo1"], "-c:v msvideo1").src(Source::Rgb).tol(10);
    msrle_avi = video("msrle.avi", &["msrle"], "-c:v msrle -pix_fmt pal8").src(Source::Rgb).tol(4);
    zmbv_avi = video("zmbv.avi", &["zmbv"], "-c:v zmbv -pix_fmt pal8").src(Source::Rgb).tol(4);

    // --- Intermediate, lossless and image codecs
    mjpeg_avi = video("mjpeg.avi", &["mjpeg"], "-c:v mjpeg -q:v 2 -pix_fmt yuvj420p -c:a pcm_s16le").src(Source::Full601).sound(&["pcm_s16le"], 2);
    prores_mov = video("prores.mov", &["prores"], "-c:v prores_ks -profile:v 3 -pix_fmt yuv422p10le -c:a pcm_s24le").sound(&["pcm_s24le"], 2);
    prores_4444_mov = video("prores-4444.mov", &["prores"], "-c:v prores_ks -profile:v 4 -pix_fmt yuva444p10le");
    dnxhr_mxf = video("dnxhr.mxf", &["dnxhd"], "-c:v dnxhd -profile:v dnxhr_sq -pix_fmt yuv422p -c:a pcm_s16le").size(1280, 720).src(Source::Yuv709).sound(&["pcm_s16le"], 1);
    dv_pal = video("dv-pal.dv", &["dvvideo"], "-c:v dvvideo -pix_fmt yuv420p -c:a pcm_s16le -ar 48000").size(720, 576).display(768, 576).sound(&["pcm_s16le"], 2).interlaced()
        // Untagged 576-line video is guessed as EBU primaries; the software
        // renderer converts those to BT.709, which takes the green's dark red
        // channel to 0. Correct, but off the pattern.
        .thumb_tol(42);
    ffv1_mkv = video("ffv1.mkv", &["ffv1"], "-c:v ffv1 -level 3 -c:a flac").sound(&["flac"], 2).tol(4);
    ffv1_odd_size = video("ffv1-odd-size.mkv", &["ffv1"], "-c:v ffv1 -pix_fmt yuv444p").size(321, 181).tol(4);
    huffyuv_avi = video("huffyuv.avi", &["huffyuv"], "-c:v huffyuv").src(Source::Rgb).tol(4);
    utvideo_avi = video("utvideo.avi", &["utvideo"], "-c:v utvideo -pix_fmt yuv422p").tol(4);
    magicyuv_avi = video("magicyuv.avi", &["magicyuv"], "-c:v magicyuv").src(Source::Rgb).tol(4);
    cineform_mov = video("cineform.mov", &["cfhd"], "-c:v cfhd -pix_fmt yuv422p10le");
    hap_mov = video("hap.mov", &["hap"], "-c:v hap").src(Source::Rgb).tol(16);
    jpeg2000_mov = video("jpeg2000.mov", &["jpeg2000"], "-c:v jpeg2000 -pix_fmt yuv444p");
    png_mov = video("png.mov", &["png"], "-c:v png").src(Source::Rgb).tol(4);
    gif = video("anim.gif", &["gif"], "-c:v gif").src(Source::Rgb).tol(16).fps("10");
    apng = video("anim.apng", &["apng"], "-c:v apng -plays 0").src(Source::Rgb).tol(4).fps("10").duration(None);
    snow_nut = video("snow.nut", &["snow"], "-c:v snow -q:v 2").tol(14);
    y4m = video("raw.y4m", &["rawvideo"], "-pix_fmt yuv420p").src(Source::Untagged601).tol(4);

    // --- Several tracks, subtitles
    multitrack_mkv = video("multitrack.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -map 1:a -c:a:0 aac -c:a:1 ac3").sound(&["aac", "ac3"], 2).subs(&["srt", "ass"], &["subrip", "ass"]);
    srt_mkv = video("srt.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").subs(&["srt"], &["subrip"]);
    ass_mkv = video("ass.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").subs(&["ass"], &["ass"]);
    mov_text_mp4 = video("mov-text.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").subs(&["mov_text"], &["mov_text"]);
    webvtt_webm = video("webvtt.webm", &["vp9"], "-c:v libvpx-vp9 -deadline realtime -cpu-used 8").subs(&["webvtt"], &["webvtt"]);
    sidecar_srt = video("sidecar.mp4", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").sidecars(&[Sidecar { name: "sidecar.en.srt", cp1252: false }], &["subrip"], super::fixtures::SUB_TEXT);
    sidecar_srt_cp1252 = video("cp1252.mkv", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p").sidecars(&[Sidecar { name: "cp1252.srt", cp1252: true }], &["subrip"], super::fixtures::SUB_TEXT_CP1252);
    hls = video("stream.m3u8", &["h264"], "-c:v libx264 -preset veryfast -pix_fmt yuv420p -g 25 -c:a aac -f hls -hls_time 1 -hls_list_size 0 -hls_playlist_type vod").sound(&["aac"], 2);

    // --- Audio
    mp3 = audio("tone.mp3", &["mp3"], 2, "-c:a libmp3lame");
    mp3_cover = audio("cover.mp3", &["mp3"], 2, "-c:a libmp3lame -c:v mjpeg -id3v2_version 3").cover(&["mjpeg"]).tol(12);
    aac_adts = audio("tone.aac", &["aac"], 2, "-c:a aac");
    aac_51_m4a = audio("tone-51.m4a", &["aac"], 6, "-c:a aac");
    m4a_cover = audio("cover.m4a", &["aac"], 2, "-c:a aac -c:v png").cover(&["png"]).tol(4);
    alac_m4a = audio("alac.m4a", &["alac"], 2, "-c:a alac");
    flac = audio("tone.flac", &["flac"], 2, "-c:a flac");
    flac_71_hires = audio("hires-71.flac", &["flac"], 8, "-c:a flac -sample_fmt s32").rate(192000);
    flac_cover = audio("cover.flac", &["flac"], 2, "-c:a flac -c:v png").cover(&["png"]).tol(4);
    wav = audio("tone.wav", &["pcm_s16le"], 2, "-c:a pcm_s16le");
    wav_24bit = audio("tone-24bit.wav", &["pcm_s24le"], 2, "-c:a pcm_s24le");
    wav_float = audio("tone-float.wav", &["pcm_f32le"], 2, "-c:a pcm_f32le");
    wav_adpcm_ima = audio("adpcm-ima.wav", &["adpcm_ima_wav"], 2, "-c:a adpcm_ima_wav");
    wav_adpcm_ms = audio("adpcm-ms.wav", &["adpcm_ms"], 2, "-c:a adpcm_ms");
    wav_alaw = audio("alaw.wav", &["pcm_alaw"], 1, "-c:a pcm_alaw -ar 8000").rate(8000);
    wav_mulaw = audio("mulaw.wav", &["pcm_mulaw"], 1, "-c:a pcm_mulaw -ar 8000").rate(8000);
    w64 = audio("tone.w64", &["pcm_s16le"], 2, "-c:a pcm_s16le");
    aiff = audio("tone.aiff", &["pcm_s16be"], 2, "-c:a pcm_s16be");
    au = audio("tone.au", &["pcm_s16be"], 1, "-c:a pcm_s16be");
    caf = audio("tone.caf", &["alac"], 2, "-c:a alac");
    voc = audio("tone.voc", &["pcm_u8"], 1, "-c:a pcm_u8 -ar 22050").rate(22050);
    ogg_vorbis = audio("tone.ogg", &["vorbis"], 2, "-c:a libvorbis");
    opus = audio("tone.opus", &["opus"], 2, "-c:a libopus");
    opus_51_mka = audio("opus-51.mka", &["opus"], 6, "-c:a libopus");
    flac_oga = audio("flac.oga", &["flac"], 2, "-c:a flac");
    speex_spx = audio("speex.spx", &["speex"], 1, "-c:a libspeex -ar 16000").rate(16000);
    wma = audio("tone.wma", &["wmav2"], 2, "-c:a wmav2");
    ac3 = audio("tone.ac3", &["ac3"], 6, "-c:a ac3");
    eac3 = audio("tone.eac3", &["eac3"], 6, "-c:a eac3");
    dts = audio("tone.dts", &["dts"], 6, "-c:a dca -strict -2");
    truehd_mka = audio("truehd.mka", &["truehd"], 2, "-c:a truehd -strict -2");
    // Raw streams carry no duration; it's estimated.
    mlp = audio("tone.mlp", &["mlp"], 2, "-c:a mlp -strict -2").duration(None);
    mp2 = audio("tone.mp2", &["mp2"], 2, "-c:a mp2");
    wavpack = audio("tone.wv", &["wavpack"], 2, "-c:a wavpack");
    tta = audio("tone.tta", &["tta"], 2, "-c:a tta");
    gsm = audio("tone.gsm", &["gsm"], 1, "-c:a libgsm -ar 8000").rate(8000);
}
