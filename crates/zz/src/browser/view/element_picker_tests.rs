use super::*;

#[test]
fn element_picker_appearance_uses_resolved_theme_tokens() {
    let mut theme = penna::Theme::default();
    theme.colors.background = penna::parse_hex("#102030").unwrap();
    theme.colors.foreground = penna::parse_hex("#abcdef").unwrap();
    theme.colors.shadow_color = penna::parse_hex("#01020366").unwrap();
    theme.radius = px(17.0);
    theme.mono_font_family = "Iosevka".into();

    let appearance = element_picker_appearance(&theme, 1.25);

    assert_eq!(appearance.highlight_outline, "#abcdef");
    assert_eq!(appearance.highlight_fill, "#abcdef1f");
    assert_eq!(appearance.highlight_contrast, "#102030");
    assert_eq!(
        appearance.preview_background,
        to_hex(theme.surface(Elevation::Panel).fill.opaque())
    );
    assert_eq!(appearance.preview_foreground, "#abcdef");
    assert_eq!(appearance.preview_border, to_hex(theme.border()));
    assert_eq!(appearance.shadow.as_deref(), Some("#01020366"));
    assert_eq!(appearance.radius, 17.0);
    assert_eq!(appearance.font_family, "Iosevka");
    assert_eq!(appearance.page_zoom, 1.25);

    theme.float_shadow.strength = 0.0;
    assert_eq!(element_picker_appearance(&theme, 1.0).shadow, None);
}
