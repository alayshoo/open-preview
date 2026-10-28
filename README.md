# Open Preview

![Open Preview playing a video, with and without its auto-hiding controls](docs/screenshot.png)

A minimal, modern video player for Windows, written in Rust.

Playback runs on **libmpv**, which is built on the same libraries as VLC:
**FFmpeg** for demuxing and decoding, and **libass** for styled ASS/SSA/SRT
subtitles. The UI is drawn with egui into the same OpenGL surface as the video.
That lets the controls sit on the picture instead of in a separate toolbar.

## Design

- **Vault's look.** The timeline and volume slider are copied from Vault (and
  so from Open Darkroom): its neutral grey palette, its Figtree typeface, and
  its slider, which is cut into chapters on the timeline, keeps a gap around
  the head and grows the head as you take hold of it. The title bar puts the
  title on the left and Open Darkroom's window buttons on the right, 8 px from
  the top and side edges, over a black-to-clear gradient. No accent colour
  anywhere.
- **Borderless window that is its own picture-in-picture.** The title bar and
  controls fade out after about 2 s without mouse movement, or as soon as the
  cursor leaves the window, leaving just the video.
- **Pin** (title bar, next to minimise, or `P`): keeps the window above other
  apps even when it isn't focused.
- **Aspect-locked resizing** on Windows, so the window always fits the video
  with no black bars. Drag anywhere to move the window and drag the edges to
  resize it.
- **Responsive controls.** Less important buttons drop away as the window gets
  smaller.
- **Subtitles move up** while the control bar is visible, so they never collide
  with it.
- Windows 11 rounded corners and a dark frame.

## Features

- Opens anything FFmpeg can play, which covers what VLC plays: H.264, HEVC
  (8 to 12-bit, 4:2:2, 4:4:4), AV1, VP8/VP9, MPEG-1/2/4, DivX/Xvid, WMV, Theora,
  RealVideo, ProRes, DNxHR, CineForm, DV, FFV1 and other lossless and legacy
  codecs; AAC, MP3, Opus, Vorbis, FLAC, ALAC, AC-3, E-AC-3, DTS, TrueHD, PCM,
  WMA and more; in MKV, MP4/MOV, AVI, TS/M2TS, MPEG-PS/VOB, WebM, Ogg, FLV,
  ASF, MXF, 3GP, RealMedia and HLS. DVD and Blu-ray folders and ISO images,
  tracker music (MOD, XM, S3M, IT) and M3U/PLS/CUE playlists open too.
  Hardware decoding is on (`hwdec=auto-safe`).
- HDR10 and HLG (HEVC, AV1, VP9) play as HDR on an HDR screen (see
  [HDR](#hdr)), and are tone-mapped on SDR screens and in thumbnails.
  Interlaced video (TV, DVD) is deinterlaced automatically, and rotation flags,
  non-square pixels and full-range video are honoured.
- Subtitles: picks up embedded and side-by-side subtitle files automatically.
  You can also drag in or browse for `.srt/.ass/.vtt/...` files, and change the
  subtitle size and delay from the subtitles menu.
- Audio track and subtitle track menus, a playback speed menu, chapter marks on
  the seek bar, and a hover timestamp while scrubbing.
- Playlists: drop several files at once, then use PgUp/PgDn.
- Remembers where you stopped in each file and resumes from there. Volume and
  subtitle size persist between sessions.
- Paste a file path or a URL with Ctrl+V. URLs work for anything
  yt-dlp supports, if `yt-dlp` is on your `PATH`.

## Build and run on Windows

1. Install Rust with the MSVC toolchain: <https://rustup.rs>.
2. Fetch libmpv. This script copies `libmpv-2.dll` into `target\debug` and
   `target\release`:
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\fetch-libmpv.ps1
   ```
   Or do it by hand: download `mpv-dev-x86_64-*.7z` from
   <https://github.com/shinchiro/mpv-winbuild-cmake/releases>, then put
   `libmpv-2.dll` next to `open-preview.exe`.
3. Run it:
   ```powershell
   cargo run --release -- "C:\path\to\movie.mkv"
   ```

libmpv is loaded at runtime, so building needs no import libraries and no
pkg-config. If the DLL is missing, Open Preview shows a screen that names the file it
needs.

Release builds have no console window. To see mpv's log, run a debug build
(`cargo run`) or set `OPEN_PREVIEW_MPV_LOG=all=v`.

## Installer (MSI)

`scripts/build-msi.sh` cross-compiles `open-preview.exe` from Linux and packages
`dist/Open-Preview-<version>-x64.msi` with msitools' `wixl`. You need mingw-w64,
msitools, 7z and the `x86_64-pc-windows-gnu` Rust target. The script fetches
the latest libmpv from the official SourceForge mirror. The installer:

- installs to `C:\Program Files\Open Preview` for all users and adds a Start menu
  shortcut;
- adds Open Preview to Explorer's "Open with" menu for common video and audio
  types, and registers it with Windows' Default apps so it can be chosen as
  the default player;
- installs a thumbnail handler for videos (see below);
- upgrades older versions in place, including Lumen, the earlier name.
  Uninstall it from Settings → Apps.

GitHub Actions (`.github/workflows/ci.yml`) lints and runs the media tests (see
[Tests](#tests)) on every push and pull request, and builds the MSI on every
push; it's attached to the run as an artifact.
To publish a release, bump `version` in `Cargo.toml`, then push a matching tag
(`git tag v0.4.0 && git push origin v0.4.0`); the MSI goes up as a GitHub
Release.

The MSI isn't code-signed, so SmartScreen warns on first run. Choose
**More info → Run anyway**.

## Default player and thumbnails

Windows doesn't let an app make itself the default player, so the installer
registers Open Preview with Default apps and the app asks. While it isn't the
default, the empty window shows **Make Open Preview your default video player**,
which opens Settings at Open Preview's page; one **Set default** there covers
every video type.

Once it's the default, Explorer gets video thumbnails from Open Preview,
including for formats Windows can't thumbnail itself, like MKV and WebM. The
handler (`thumbnailer/`, a small COM DLL) runs `open-preview.exe --thumbnail
<file> <size>`, which renders a frame 15% into the video with libmpv's
software renderer, with no window and no GPU. The decoding happens in that
child process, so a broken file can't take Explorer down. Thumbnails Explorer
cached before the switch may linger; Disk Cleanup → Thumbnails clears them.

`cargo run --example probe -p open-preview-thumbs` (on Windows, after
installing) fetches a thumbnail through COM the way Explorer does, for testing.

## HDR

When Windows' **Use HDR** is on for the screen the window is on, HDR video
plays as HDR. It's tone-mapped only for highlights brighter than the screen
can show (its peak, as Windows reports it). The controls stay at Windows'
**SDR content brightness**, like any other app. SDR video, and anything on an
SDR screen, takes the usual path. Moving the window between screens, or
turning HDR on or off, switches over within a second.

egui draws through OpenGL into an 8-bit window, so HDR frames take another
route (`src/hdr.rs`). mpv renders the video as absolute PQ / BT.2020 into a
float texture, and egui draws the controls into a texture of their own. A
shader then decodes the video to nits and blends the controls over it the way
egui would over SDR video, writing scRGB, the format Windows composes HDR
desktops in. A Direct3D 11 FP16 swap chain on a child window over the client
area shows the result. OpenGL draws into it through `WGL_NV_DX_interop2`, which
current NVIDIA, AMD and Intel drivers have (mpv relies on it for D3D11 hardware
decoding with OpenGL). The child window passes every click and drop through
to the window under it.

If any of that isn't available, the reason is logged and the app stays SDR.
`OPEN_PREVIEW_HDR=off` turns HDR output off, and `OPEN_PREVIEW_HDR=force` uses
it on any screen, for trying it out.

## Linux

Install `libmpv-dev`, or just `libmpv2`, then run `cargo run -- video.mp4`.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| Space / K, click video | Play / pause |
| ← / → | Seek 5 s (Shift: 1 s) |
| J / L | Seek 10 s |
| 0 – 9 | Jump to 0 % – 90 % |
| , / . | Previous / next frame |
| ↑ / ↓, mouse wheel | Volume |
| M | Mute |
| S / A | Cycle subtitles / audio track |
| Z / X | Subtitle delay −/+ 0.1 s |
| [ / ] / Backspace | Speed down / up / reset |
| F, double-click | Maximize / restore |
| P | Keep on top (pin) |
| PgUp / PgDn | Previous / next in playlist |
| Ctrl+O / Ctrl+V | Open file / paste path or URL |
| F1 | Shortcut overlay |

## Tests

`cargo test` runs the unit tests and the media regression tests in
`src/tests/`: over a hundred short clips that ffmpeg makes on first run (into
`target/media-fixtures`), one per codec, container, HDR format, subtitle type
and odd case (rotation, anamorphic DVD, interlacing, odd frame sizes, cover art,
a legacy-code-page `.srt`). Each clip goes through the app's own code and mpv
options. The tests check the tracks and codecs `Player` finds, the duration,
channel count and display size, and that a full decode logs no errors. They
also check the colours of a known test pattern: in the thumbnail, and on Linux
in the OpenGL path the window draws with, where HDR is tone-mapped and
subtitles are drawn. On that path, HDR output is checked too: an HDR10 clip
rendered as PQ and composited with the controls must come out as scRGB at the
clip's real brightness.

Without libmpv or ffmpeg these tests are skipped with a note.
`OPEN_PREVIEW_MEDIA_TESTS=require` turns that into a failure.
`scripts/test-media.sh` runs them twice: against the system libmpv, then,
cross-built and under Wine, against the `libmpv-2.dll` the installer ships, on
the same clips. CI does the same on every push and pull request, and won't
build the installer if they fail. To look at a failure,
`OPEN_PREVIEW_TEST_DUMP=<folder>` saves the rendered frames there, and
`OPEN_PREVIEW_LIBMPV=<path>` tries another libmpv build. It needs ffmpeg,
`libmpv2`, Mesa's EGL (`libegl-mesa0`, `libgl1-mesa-dri`), `wine`, mingw-w64
and 7z.

## Code layout

| File | Purpose |
| --- | --- |
| `src/mpv.rs` | Runtime-loaded libmpv bindings: client API and OpenGL render API |
| `src/player.rs` | Mirrors mpv's properties (time, tracks, chapters…) and wraps its commands |
| `src/app.rs` | Window behavior, title bar, auto-hide logic, pin, menus, input handling |
| `src/ui/` | Vector icons, theme, custom widgets (seek bar, sliders, menus) |
| `src/platform.rs` | Windows: rounded corners, aspect-locked resizing, default-player check |
| `src/thumbnail.rs` | `--thumbnail` mode: one frame to stdout via libmpv's software renderer |
| `src/hdr.rs` | HDR output: the PQ-to-scRGB compositor; `src/hdr/win.rs` presents it through DXGI |
| `src/formats.rs` | File types for the open dialog and drag and drop; the installer registers the same |
| `src/tests/` | Media regression tests: the clip matrix, ffmpeg fixtures, a headless OpenGL harness |
| `thumbnailer/` | Explorer thumbnail handler (COM DLL) that calls `--thumbnail` |
| `installer/` | WiX source for the MSI and the third-party notices it ships |

## Known limitations

- Windows 11 Snap Layouts don't appear when hovering the custom maximize
  button. Win+Z and drag-to-edge snapping still work.
- Video goes through OpenGL. On Windows that means mpv's `dxva2`/`nvdec`
  interop, or `*-copy` hardware decoding. Rendering straight through D3D11 would
  need mpv's (still experimental) D3D11 render API or a wgpu backend.
- HDR output needs an OpenGL driver with `WGL_NV_DX_interop2`. Without it
  (some virtual machines, Remote Desktop), HDR video is tone-mapped to SDR.
- Dolby Vision profile 5 (common in streaming rips) plays with wrong, green
  and purple colours. Reshaping it needs mpv's `gpu-next` renderer, which
  libmpv's render API doesn't use. VLC 3 has the same problem. Profiles 7 and 8
  play their HDR10 base layer and look right.
- MIDI and SID files don't play: FFmpeg has no synthesizer for them.

## License

Open Preview is released under the [0BSD license](LICENSE): do anything you
like with it, no attribution required.

Two bundled pieces carry their own terms. Figtree (`assets/fonts`) is under the
SIL Open Font License 1.1. The Windows installer ships a prebuilt libmpv, which
is GPL-licensed; that affects anyone redistributing the installer, not the
source in this repository, which only loads libmpv at run time. See
`installer/THIRD-PARTY-NOTICES.txt`.
