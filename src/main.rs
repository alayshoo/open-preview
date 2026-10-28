// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icon;
mod mpv;
mod platform;
mod player;
mod thumbnail;
mod ui;

use std::sync::Arc;

fn mpv_options(watch_later: &str) -> Vec<(&'static str, String)> {
    let mut opts: Vec<(&str, String)> = vec![
        ("vo", "libmpv".into()),
        ("hwdec", "auto-safe".into()),
        ("idle", "yes".into()),
        ("keep-open", "yes".into()),
        ("input-default-bindings", "no".into()),
        ("input-vo-keyboard", "no".into()),
        ("osc", "no".into()),
        ("osd-level", "0".into()),
        ("volume-max", "150".into()),
        ("audio-client-name", "Open Preview".into()),
        ("sub-auto", "fuzzy".into()),
        ("sub-font-size", "46".into()),
        ("sub-border-size", "2.6".into()),
        ("sub-shadow-offset", "1".into()),
        ("save-position-on-quit", "yes".into()),
        ("watch-later-options", "start,speed,aid,sid,audio-delay,sub-delay".into()),
        ("cache", "auto".into()),
        ("demuxer-max-bytes", "150MiB".into()),
        ("ytdl", "yes".into()),
        ("terminal", if cfg!(debug_assertions) || std::env::var("OPEN_PREVIEW_MPV_LOG").is_ok() { "yes" } else { "no" }.into()),
        ("msg-level", std::env::var("OPEN_PREVIEW_MPV_LOG").unwrap_or_else(|_| "all=warn".into())),
    ];
    if !watch_later.is_empty() {
        // Renamed in newer mpv; whichever the loaded build doesn't know is ignored.
        opts.push(("watch-later-dir", watch_later.into()));
        opts.push(("watch-later-directory", watch_later.into()));
    }
    if cfg!(windows) {
        opts.push(("sub-font", "Segoe UI Semibold".into()));
    }
    opts
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--thumbnail") {
        std::process::exit(thumbnail::run(&args[1..]));
    }
    let files: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();

    let data_dir = dirs::data_dir().map(|d| d.join("Open Preview"));
    let watch_later = data_dir
        .as_ref()
        .map(|d| d.join("watch_later"))
        .filter(|d| std::fs::create_dir_all(d).is_ok())
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();

    let opts = mpv_options(&watch_later);
    let opt_refs: Vec<(&str, &str)> = opts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mpv = mpv::Mpv::new(&opt_refs);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Open Preview")
            .with_app_id("open-preview")
            .with_inner_size([960.0, 540.0])
            .with_min_inner_size([240.0, 135.0])
            .with_decorations(false)
            .with_resizable(true)
            .with_drag_and_drop(true)
            .with_icon(Arc::new(icon::app_icon())),
        centered: true,
        persist_window: false,
        persistence_path: data_dir.map(|d| d.join("settings.ron")),
        ..Default::default()
    };

    eframe::run_native(
        "Open Preview",
        options,
        Box::new(move |cc| match mpv {
            Ok(mpv) => Ok(Box::new(app::OpenPreviewApp::new(cc, mpv, files))),
            Err(message) => Ok(Box::new(app::FatalApp { message })),
        }),
    )
}
