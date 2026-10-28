# Open Preview

![Open Preview playing a video, with and without its auto-hiding controls](docs/screenshot.png)

A minimal, modern video player for Windows, written in Rust.

Playback runs on **libmpv**, which is built on the same libraries as VLC:
**FFmpeg** for demuxing and decoding, and **libass** for styled ASS/SSA/SRT
subtitles. The UI is drawn with egui into the same OpenGL surface as the video.
That lets the controls sit on the picture instead of in a separate toolbar.

## Design

- **Vault's look.** The title bar, timeline and volume slider are copied from
  Vault (and so from Open Darkroom): its neutral grey palette, its Figtree
  typeface, a 40 px title bar with the title centred over a black-to-clear
  gradient, and its slider, which is cut into chapters on the timeline, keeps a
  gap around the head and grows the head as you take hold of it. No accent
  colour anywhere.
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

- Opens anything FFmpeg can play. Hardware decoding is on (`hwdec=auto-safe`).
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
| F, double-click | Fullscreen |
| P | Keep on top (pin) |
| PgUp / PgDn | Previous / next in playlist |
| Ctrl+O / Ctrl+V | Open file / paste path or URL |
| F1 | Shortcut overlay |
| Esc | Leave fullscreen |

## Code layout

| File | Purpose |
| --- | --- |
| `src/mpv.rs` | Runtime-loaded libmpv bindings: client API and OpenGL render API |
| `src/player.rs` | Mirrors mpv's properties (time, tracks, chapters…) and wraps its commands |
| `src/app.rs` | Window behavior, title bar, auto-hide logic, pin, menus, input handling |
| `src/ui/` | Vector icons, theme, custom widgets (seek bar, sliders, menus) |
| `src/platform.rs` | Windows: rounded corners, aspect-locked resizing, default-player check |
| `src/thumbnail.rs` | `--thumbnail` mode: one frame to stdout via libmpv's software renderer |
| `thumbnailer/` | Explorer thumbnail handler (COM DLL) that calls `--thumbnail` |
| `installer/` | WiX source for the MSI and the third-party notices it ships |

## Known limitations

- Windows 11 Snap Layouts don't appear when hovering the custom maximize
  button. Win+Z and drag-to-edge snapping still work.
- Video goes through OpenGL. On Windows that means mpv's `dxva2`/`nvdec`
  interop, or `*-copy` hardware decoding. Rendering straight through D3D11 would
  need mpv's (still experimental) D3D11 render API or a wgpu backend.
- There are no file associations or installer yet. You can still drag files onto
  `open-preview.exe` or use "Open with".

## License

Open Preview is released under the [0BSD license](LICENSE): do anything you
like with it, no attribution required.

Two bundled pieces carry their own terms. Figtree (`assets/fonts`) is under the
SIL Open Font License 1.1. The Windows installer ships a prebuilt libmpv, which
is GPL-licensed; that affects anyone redistributing the installer, not the
source in this repository, which only loads libmpv at run time. See
`installer/THIRD-PARTY-NOTICES.txt`.
