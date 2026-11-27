use super::*;

#[test]
fn light_defaults_put_dark_text_on_a_light_ground() {
    let light = AppearanceLoad::defaults_for(TerminalColorScheme::Light).appearance;
    let dark = AppearanceLoad::defaults_for(TerminalColorScheme::Dark).appearance;
    let level = |color: Color| u32::from(color.r) + u32::from(color.g) + u32::from(color.b);
    assert_eq!(light.color_scheme, TerminalColorScheme::Light);
    assert!(level(light.background) > level(light.foreground));
    assert!(level(dark.background) < level(dark.foreground));
    assert_eq!(dark, TerminalAppearance::default());
    assert_eq!(light.palette, dark.palette);
}

#[test]
fn local_colors_replace_the_defaults_and_base_palette_only() {
    let appearance = TerminalAppearance::default();
    let palette = appearance.palette.as_array();
    let mut local = [Color::rgb(1, 2, 3); 18];
    local[0] = Color::rgb(200, 200, 200);
    local[1] = Color::rgb(10, 10, 10);
    local[3] = Color::rgb(250, 0, 0);
    let colors = LocalColors::new(
        appearance.foreground,
        appearance.background,
        &appearance.palette,
        local,
    );
    let style = |foreground, background| {
        PackedStyle::new(foreground, background, None, 0, crate::UnderlineStyle::None)
    };

    let red_on_default = style(palette[1], appearance.background)
        .with_classes(ColourClass::Palette(1), ColourClass::Default);
    let recolored = colors.recolor(red_on_default);
    assert_eq!(recolored.foreground(), local[3]);
    assert_eq!(recolored.background(), local[1]);
    assert_eq!(recolored.foreground_class(), ColourClass::Palette(1));

    let picked = style(Color::rgb(255, 0, 128), palette[208])
        .with_classes(ColourClass::Rgb, ColourClass::Palette(208));
    assert_eq!(colors.recolor(picked), picked);

    let inverted = colors.recolor(style(appearance.background, appearance.foreground));
    assert_eq!(inverted.foreground(), local[1]);
    assert_eq!(inverted.background(), local[0]);
}

#[test]
fn defaults_are_wire_safe_and_hash_deterministically() {
    let appearance = TerminalAppearance::default();
    appearance.validate().expect("valid defaults");
    assert!((appearance.font_size_points - 13.0).abs() < f32::EPSILON);
    assert_eq!(
        appearance.cell_height_adjustment,
        CellHeightAdjustment::None
    );
    assert_eq!(
        appearance.font_synthetic_style,
        FontSyntheticStyle::default()
    );
    assert!(!appearance.font_thicken);
    assert_eq!(appearance.font_thicken_strength, u8::MAX);
    assert_eq!(appearance.cursor_style, CursorStyle::Block);
    assert_eq!(appearance.stable_hash(), appearance.clone().stable_hash());
    assert_ne!(appearance.palette[16], appearance.palette[17]);
    assert_eq!(appearance.palette[255], Color::rgb(238, 238, 238));
}

#[test]
fn invalid_wire_values_are_rejected() {
    let appearance = TerminalAppearance {
        font_size_points: f32::NAN,
        ..TerminalAppearance::default()
    };
    assert_eq!(
        appearance.validate(),
        Err(AppearanceValidationError::FontSize)
    );
    let appearance = TerminalAppearance {
        font_families: vec!["x".repeat(MAX_FONT_FAMILY_BYTES + 1)],
        ..TerminalAppearance::default()
    };
    assert_eq!(
        appearance.validate(),
        Err(AppearanceValidationError::FontFamily)
    );
    let appearance = TerminalAppearance {
        cursor_blink_interval_ms: 0,
        ..TerminalAppearance::default()
    };
    assert_eq!(
        appearance.validate(),
        Err(AppearanceValidationError::BlinkInterval)
    );
}
