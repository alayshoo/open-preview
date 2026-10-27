use egui::{Color32, FontData, FontDefinitions, FontFamily};

pub const ACCENT: Color32 = Color32::from_rgb(124, 108, 255);
pub const TEXT: Color32 = Color32::from_rgb(245, 245, 247);
pub const TEXT_DIM: Color32 = Color32::from_rgb(170, 170, 178);
pub const BG: Color32 = Color32::from_rgb(10, 10, 12);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(22, 22, 26, 238);
pub const CLOSE_RED: Color32 = Color32::from_rgb(232, 17, 35);

/// Named font family for semi-bold text (falls back to the regular face).
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Uses Segoe UI on Windows so the player feels native; egui's bundled
/// fonts stay as fallbacks for symbols and other scripts.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    #[cfg(windows)]
    let (regular, bold) = {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let read = |f: &str| std::fs::read(format!("{dir}\\Fonts\\{f}")).ok();
        (read("segoeui.ttf"), read("seguisb.ttf"))
    };
    #[cfg(not(windows))]
    let (regular, bold) = {
        let read = |f: &str| std::fs::read(f).ok();
        (
            read("/usr/share/fonts/truetype/inter/Inter-Regular.ttf"),
            read("/usr/share/fonts/truetype/inter/Inter-SemiBold.ttf"),
        )
    };

    let prop = fonts.families.entry(FontFamily::Proportional).or_default().clone();
    let mut semibold_family = prop.clone();
    if let Some(data) = regular {
        fonts.font_data.insert("ui".into(), FontData::from_owned(data).into());
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "ui".into());
        semibold_family.insert(0, "ui".into());
    }
    if let Some(data) = bold {
        fonts.font_data.insert("ui-semibold".into(), FontData::from_owned(data).into());
        semibold_family.insert(0, "ui-semibold".into());
    }
    fonts.families.insert(semibold(), semibold_family);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = ACCENT;
        style.interaction.tooltip_delay = 0.4;
    });
}
