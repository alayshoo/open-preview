Open Preview: a minimal Windows video player. Rust, egui/eframe (glow) UI drawn over libmpv's OpenGL render API; libmpv-2.dll is loaded at runtime.

Design follows Vault / Open Darkroom (github.com/alayshoo): neutral greys only, no accent colour, no ALL CAPS, Figtree font. Title bar and timeline/volume slider copy Vault's; the title bar sits on a black-to-clear gradient. Controls auto-hide; the window itself is the picture-in-picture (no PiP mode), with a pin button for always-on-top.

Windows: `scripts/build-msi.sh` cross-builds and packages the MSI (wixl); CI runs it on every push, and `v*` tags publish a Release. The installer registers with Default apps (apps can't set themselves default) and a thumbnail handler (`thumbnailer/`, COM DLL calling `open-preview.exe --thumbnail`).

License 0BSD. The bundled libmpv is GPL.
