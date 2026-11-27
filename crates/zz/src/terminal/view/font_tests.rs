use super::*;
use ala::{FontStyle, FontWeight};

#[test]
fn daemon_default_font_is_replaced_with_the_rendering_hosts_default() {
    let mut appearance = TerminalAppearance {
        font_families: vec!["Other OS Mono".to_owned()],
        ..TerminalAppearance::default()
    };

    localize_terminal_font_families_with_default(
        &mut appearance,
        &AppearanceProvenance::default(),
        &["Client Mono".to_owned()],
        "Client Mono",
    );

    assert_eq!(appearance.font_families, ["Client Mono"]);
}

#[test]
fn installed_configured_fallback_is_promoted_over_a_missing_primary() {
    let mut appearance = TerminalAppearance {
        font_families: vec!["Missing Mono".to_owned(), "Available Mono".to_owned()],
        ..TerminalAppearance::default()
    };
    let mut provenance = AppearanceProvenance::default();
    provenance.set_source(AppearanceConfigKey::FontFamily, AppearanceSource::Override);

    localize_terminal_font_families_with_default(
        &mut appearance,
        &provenance,
        &["available mono".to_owned()],
        "Client Mono",
    );

    assert_eq!(appearance.font_families, ["Available Mono"]);
}

#[test]
fn entirely_missing_stacks_fall_back_to_local_regular_and_inherit_for_styles() {
    let mut appearance = TerminalAppearance {
        font_families: vec!["Missing Regular".to_owned()],
        font_families_bold: vec!["Missing Bold".to_owned()],
        font_families_italic: vec!["Missing Italic".to_owned()],
        ..TerminalAppearance::default()
    };
    let mut provenance = AppearanceProvenance::default();
    for key in [
        AppearanceConfigKey::FontFamily,
        AppearanceConfigKey::FontFamilyBold,
        AppearanceConfigKey::FontFamilyItalic,
    ] {
        provenance.set_source(key, AppearanceSource::Override);
    }

    localize_terminal_font_families_with_default(&mut appearance, &provenance, &[], "Client Mono");

    assert_eq!(appearance.font_families, ["Client Mono"]);
    assert!(appearance.font_families_bold.is_empty());
    assert!(appearance.font_families_italic.is_empty());
    assert_eq!(
        terminal_font_for_style(&appearance, true, false).family,
        "Client Mono",
    );
}

#[test]
fn appearance_maps_primary_fallback_features_weight_and_metrics() {
    let appearance = TerminalAppearance {
        font_families: vec!["Primary Mono".to_owned(), "Emoji Fallback".to_owned()],
        font_features: vec![zz_terminal::FontFeature::new(*b"ss01", 2)],
        font_weight: 550,
        font_size_points: 12.0,
        cell_height_adjustment: zz_terminal::CellHeightAdjustment::Percent(25.0),
        ..TerminalAppearance::default()
    };

    let font = terminal_font(&appearance);
    assert_eq!(&*font.family, "Primary Mono");
    assert_eq!(font.weight, FontWeight(550.0));
    assert_eq!(
        font.features.tag_value_list(),
        [("liga".to_owned(), 1), ("ss01".to_owned(), 2)]
    );
    assert_eq!(
        font.fallbacks.expect("fallbacks").fallback_list(),
        ["Emoji Fallback"]
    );
    let (font_size, expanded_line_height, compact_line_height) = if cfg!(target_os = "macos") {
        (px(12.0), px(15.0), px(9.0))
    } else {
        (px(16.0), px(20.0), px(12.0))
    };
    assert_eq!(terminal_font_size(&appearance), font_size);
    assert_eq!(terminal_line_height(&appearance), expanded_line_height);

    let compact = TerminalAppearance {
        cell_height_adjustment: zz_terminal::CellHeightAdjustment::Percent(-25.0),
        ..appearance
    };
    assert_eq!(terminal_line_height(&compact), compact_line_height);
}

#[cfg(target_os = "macos")]
#[test]
fn ghostty_font_points_reach_coretext_without_dpi_conversion() {
    let appearance = TerminalAppearance {
        font_size_points: 13.0,
        ..TerminalAppearance::default()
    };

    assert_eq!(terminal_font_size(&appearance), px(13.0));
}

#[test]
fn default_terminal_font_enables_ghostty_ligatures_without_empty_fallbacks() {
    let appearance = TerminalAppearance::default();
    let font = terminal_font(&appearance);
    assert_eq!(font.features.tag_value_list(), [("liga".to_owned(), 1)]);
    assert!(font.fallbacks.is_none());
}

#[test]
fn configured_ligature_override_follows_the_ghostty_default() {
    let appearance = TerminalAppearance {
        font_features: vec![zz_terminal::FontFeature::new(*b"liga", 0)],
        ..TerminalAppearance::default()
    };

    assert_eq!(
        terminal_font(&appearance).features.tag_value_list(),
        [("liga".to_owned(), 1), ("liga".to_owned(), 0)]
    );
}

#[test]
fn styled_terminal_fonts_use_ghostty_face_stacks_and_regular_fallbacks() {
    let appearance = TerminalAppearance {
        font_families: vec!["Regular Mono".to_owned(), "Regular Symbols".to_owned()],
        font_families_bold: vec!["Bold Mono".to_owned(), "Bold Symbols".to_owned()],
        font_families_italic: vec!["Italic Mono".to_owned()],
        font_weight: 425,
        ..TerminalAppearance::default()
    };

    let bold = terminal_font_for_style(&appearance, true, false);
    assert_eq!(&*bold.family, "Bold Mono");
    assert_eq!(
        bold.fallbacks.expect("bold fallback").fallback_list(),
        ["Bold Symbols"]
    );
    assert_eq!(bold.weight, FontWeight(725.0));

    let italic = terminal_font_for_style(&appearance, false, true);
    assert_eq!(&*italic.family, "Italic Mono");
    assert_eq!(italic.style, FontStyle::Italic);

    let bold_italic = terminal_font_for_style(&appearance, true, true);
    assert_eq!(&*bold_italic.family, "Regular Mono");
    assert_eq!(
        bold_italic
            .fallbacks
            .expect("regular fallback")
            .fallback_list(),
        ["Regular Symbols"]
    );
    assert_eq!(bold_italic.weight, FontWeight(725.0));
    assert_eq!(bold_italic.style, FontStyle::Italic);
}
