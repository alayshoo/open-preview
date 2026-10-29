Open Preview: a minimal Windows video player. Rust, egui/eframe (glow) UI drawn over libmpv's OpenGL render API; libmpv-2.dll is loaded at runtime.

Design follows Vault / Open Darkroom (github.com/alayshoo): neutral greys only, no accent colour, no ALL CAPS, Figtree font. Timeline/volume slider copy Vault's; the title bar (title left, Open Darkroom's window buttons right, 8px from the top and side edges) sits on a black-to-clear gradient. Controls auto-hide; the window itself is the picture-in-picture (no PiP mode), with a pin button for always-on-top. There's no fullscreen: F, double-click and the bottom-row button maximize.

Windows: `scripts/build-msi.sh` cross-builds and packages the MSI (wixl); CI runs it on every push, and a push to main with a new Cargo.toml version tags and publishes a Release (pushing a `v*` tag by hand does too). The installer registers with Default apps (apps can't set themselves default) and a thumbnail handler (`thumbnailer/`, COM DLL calling `open-preview.exe --thumbnail`). Keep `REINSTALLMODE=amus` in the .wxs: wixl writes no file versions, and without it a major upgrade skips the versioned exe and libmpv, then deletes them with the old version.

HDR video on an HDR screen (Windows HDR on) goes out as HDR: mpv renders PQ into a float texture, egui into its own, `src/hdr.rs` composites them as scRGB and `src/hdr/win.rs` presents through a D3D11 FP16 swap chain on an input-transparent child window via WGL_NV_DX_interop2; anything missing falls back to the SDR path.

Media regression tests (`src/tests/`, run by `scripts/test-media.sh` and CI) play a matrix of ffmpeg-made clips through the app's own code against the system libmpv and, under Wine, the shipped DLL; add a case there when touching playback, formats or colour. File types live in `src/formats.rs`; a test keeps the installer's registrations in step.

License 0BSD. The bundled libmpv is GPL.
