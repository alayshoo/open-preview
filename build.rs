fn main() {
    // Embed the icon and version info so Explorer, the taskbar and the
    // installer show Lumen's icon for the .exe itself.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/lumen.ico")
            .set("ProductName", "Lumen")
            .set("FileDescription", "Lumen media player")
            .set("LegalCopyright", "Lumen contributors");
        res.compile().expect("failed to embed Windows resources");
    }
    println!("cargo:rerun-if-changed=assets/lumen.ico");
}
