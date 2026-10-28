fn main() {
    // Embed the icon and version info so Explorer, the taskbar and the
    // installer show Open Preview's icon for the .exe itself.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/open-preview.ico")
            .set("ProductName", "Open Preview")
            .set("FileDescription", "Open Preview media player")
            .set("LegalCopyright", "Open Preview contributors");
        res.compile().expect("failed to embed Windows resources");
    }
    println!("cargo:rerun-if-changed=assets/open-preview.ico");
}
