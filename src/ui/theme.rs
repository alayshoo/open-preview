use egui::{Color32, FontData, FontDefinitions, FontFamily};

// Vault's palette (itself Open Darkroom's): neutral ramps and nothing else.
pub const BG1: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const BG2: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
pub const BG3: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const BG4: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
pub const BG5: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
pub const COLOR1: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xe6);
pub const COLOR2: Color32 = Color32::from_rgb(0xb3, 0xb3, 0xb3);
pub const COLOR3: Color32 = Color32::from_rgb(0x63, 0x63, 0x63);
pub const COLOR4: Color32 = Color32::from_rgb(0x4c, 0x4c, 0x4c);
pub const COLOR5: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);

pub const TEXT: Color32 = COLOR1;
pub const TEXT_DIM: Color32 = COLOR2;
pub const BG: Color32 = BG1;
pub const PANEL: Color32 = BG2;

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
        style.visuals.selection.bg_fill = COLOR4;
        // Tooltips and popups: Vault's neutral panel.
        style.visuals.window_stroke = egui::Stroke::new(1.0, BG5);
        style.visuals.window_corner_radius = egui::CornerRadius::same(7);
        style.visuals.popup_shadow = egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(128) };
        style.interaction.tooltip_delay = 0.4;
    });
}
