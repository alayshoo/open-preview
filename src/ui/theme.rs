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

/// Figtree, the typeface Vault and Open Darkroom use, bundled so it looks the
/// same on every machine. egui's own fonts stay behind it as fallbacks.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let regular = include_bytes!("../../assets/fonts/Figtree-Regular.ttf");
    let semibold_face = include_bytes!("../../assets/fonts/Figtree-SemiBold.ttf");
    fonts.font_data.insert("figtree".into(), FontData::from_static(regular).into());
    fonts.font_data.insert("figtree-semibold".into(), FontData::from_static(semibold_face).into());

    let fallbacks = fonts.families.entry(FontFamily::Proportional).or_default().clone();
    let mut proportional = vec!["figtree".to_string()];
    proportional.extend(fallbacks.iter().cloned());
    let mut semibold_family = vec!["figtree-semibold".to_string()];
    semibold_family.extend(fallbacks);
    fonts.families.insert(FontFamily::Proportional, proportional);
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

        // Scrollbars as Vault draws them: beside the content rather than over
        // it, a 6px rounded thumb in COLOR5 (COLOR4 when hovered), no track.
        let mut scroll = egui::style::ScrollStyle::solid();
        scroll.bar_width = 6.0;
        scroll.bar_inner_margin = 4.0;
        scroll.bar_outer_margin = 2.0;
        scroll.dormant_background_opacity = 0.0;
        scroll.active_background_opacity = 0.0;
        scroll.interact_background_opacity = 0.0;
        scroll.dormant_handle_opacity = 1.0;
        scroll.active_handle_opacity = 1.0;
        scroll.interact_handle_opacity = 1.0;
        style.spacing.scroll = scroll;
        style.visuals.widgets.inactive.bg_fill = COLOR5;
        style.visuals.widgets.hovered.bg_fill = COLOR4;
        style.visuals.widgets.active.bg_fill = COLOR4;
        style.interaction.tooltip_delay = 0.4;
    });
}
