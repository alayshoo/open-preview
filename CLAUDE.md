Open Preview: a minimal Windows video player. Rust, egui/eframe (glow) UI drawn over libmpv's OpenGL render API; libmpv-2.dll is loaded at runtime.

Design follows Vault / Open Darkroom (github.com/alayshoo): neutral greys only, no accent colour, no ALL CAPS, Figtree font. Timeline/volume slider copy Vault's; the title bar (title left, Open Darkroom's window buttons right, 8px from the top and side edges) sits on a black-to-clear gradient. Controls auto-hide; the window itself is the picture-in-picture (no PiP mode), with a pin button for always-on-top. There's no fullscreen: F, double-click and the bottom-row button maximize.

Windows: `scripts/build-msi.sh` cross-builds and packages the MSI (wixl); CI runs it on every push, and a push to main with a new Cargo.toml version tags and publishes a Release (pushing a `v*` tag by hand does too). The installer registers with Default apps (apps can't set themselves default) and a thumbnail handler (`thumbnailer/`, COM DLL calling `open-preview.exe --thumbnail`).

License 0BSD. The bundled libmpv is GPL.
