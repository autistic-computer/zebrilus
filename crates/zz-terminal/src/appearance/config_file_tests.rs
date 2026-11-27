use super::*;

#[test]
fn parses_every_ghostty_cursor_style_and_rejects_invalid_values() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    for (value, expected) in [
        ("bar", CursorStyle::Bar),
        ("block", CursorStyle::Block),
        ("underline", CursorStyle::Underline),
        ("block_hollow", CursorStyle::BlockHollow),
    ] {
        fs::write(&path, format!("cursor-style = {value}\n")).expect("write config");
        let load = load_ghostty_appearance_from(&path);
        assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
        assert_eq!(load.appearance.cursor_style, expected);
        assert_eq!(
            load.provenance.source(AppearanceConfigKey::CursorStyle),
            AppearanceSource::Ghostty
        );
    }

    fs::write(&path, "cursor-style = beam\n").expect("write invalid config");
    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 1, "{:#?}", load.diagnostics);
    assert_eq!(load.appearance.cursor_style, CursorStyle::Block);
}

#[test]
fn discovery_candidates_follow_ghostty_precedence() {
    assert_eq!(
        ghostty_config_candidates(Some(OsStr::new("/xdg")), Some(OsStr::new("/home/me"))),
        [
            PathBuf::from("/xdg/ghostty/config.ghostty"),
            PathBuf::from("/xdg/ghostty/config"),
            PathBuf::from("/home/me/.config/ghostty/config.ghostty"),
            PathBuf::from("/home/me/.config/ghostty/config"),
        ]
    );
}

#[test]
fn parses_supported_values_including_named_and_alpha_colors() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "\
font-family = Example Mono\n\
font-family = Emoji Font\n\
font-family-bold = Example Mono Bold\n\
font-family-bold = Bold Symbols\n\
font-family-italic = Example Mono Italic\n\
font-family-bold-italic = Example Mono Bold Italic\n\
font-size = 12.5\n\
font-feature = -liga\n\
font-feature = ss01=2\n\
font-synthetic-style = no-bold,italic,no-bold-italic\n\
font-thicken = true\n\
font-thicken-strength = 173\n\
adjust-cell-height = 12%\n\
window-padding-x = 7\n\
foreground = rebeccapurple\n\
selection-background = #11223380\n\
palette = 42=#abcdef\n\
cursor-style-blink = terminal\n\
zz-font-weight = 550\n\
zz-rounded-selection = false\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(
        load.appearance.font_families,
        ["Example Mono", "Emoji Font"]
    );
    assert_eq!(
        load.appearance.font_families_bold,
        ["Example Mono Bold", "Bold Symbols"]
    );
    assert_eq!(
        load.appearance.font_families_italic,
        ["Example Mono Italic"]
    );
    assert_eq!(
        load.appearance.font_families_bold_italic,
        ["Example Mono Bold Italic"]
    );
    assert_eq!(load.appearance.font_features.len(), 2);
    assert_eq!(
        load.appearance.font_features[0],
        FontFeature::new(*b"liga", 0)
    );
    assert_eq!(load.appearance.foreground, Color::rgb(102, 51, 153));
    assert_eq!(
        load.appearance.selection_background,
        AppearanceColor::rgba(0x11, 0x22, 0x33, 0x80)
    );
    assert_eq!(load.appearance.palette[42], Color::rgb(0xab, 0xcd, 0xef));
    assert_eq!(load.appearance.font_weight, 550);
    assert_eq!(
        load.appearance.font_synthetic_style,
        FontSyntheticStyle {
            bold: false,
            italic: true,
            bold_italic: false,
        }
    );
    assert!(load.appearance.font_thicken);
    assert_eq!(load.appearance.font_thicken_strength, 173);
    assert!(!load.appearance.rounded_selection);
}

#[test]
fn follows_relative_includes_once_and_preserves_valid_values_after_errors() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("root");
    let child = directory.path().join("child");
    fs::write(
        &root,
        "font-size = 11\nconfig-file = child\nconfig-file = child\nfont-size = nope\n",
    )
    .expect("write root");
    fs::write(&child, "window-padding-y = 9\nconfig-file = root\n").expect("write child");

    let load = load_ghostty_appearance_from(&root);
    assert!((load.appearance.font_size_points - 11.0).abs() < f32::EPSILON);
    assert!((load.appearance.padding_top - 9.0).abs() < f32::EPSILON);
    assert!((load.appearance.padding_bottom - 9.0).abs() < f32::EPSILON);
    assert_eq!(load.invalid, 2);
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.key == "config-file" && diagnostic.message.contains("cycle")
    }));
    assert_eq!(
        load.bytes_read,
        fs::read(&root).unwrap().len() + fs::read(&child).unwrap().len()
    );
}

#[test]
fn ghostty_background_blur_is_ignored_but_background_opacity_is_applied() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(&path, "background-opacity = 0.72\nbackground-blur = 24\n").expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(load.unsupported, 1);
    assert!((load.appearance.background_opacity - 0.72).abs() < f32::EPSILON);
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.key == "background-blur"
            && diagnostic.disposition == AppearanceConfigDisposition::Unsupported
    }));
}

#[test]
fn parses_every_renderer_color_and_behavior_setting() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "\
background = #010203\n\
cursor-color = #040506\n\
selection-foreground = #070809\n\
zz-search-match-color = #10111280\n\
zz-search-current-color = #13141590\n\
zz-link-color = #161718\n\
zz-copy-cursor-color = #191a1ba0\n\
minimum-contrast = 4.5\n\
background-opacity = 0.75\n\
cursor-style-blink = off\n\
zz-cursor-blink-interval-ms = 725\n\
font-thicken = true\n\
font-thicken-strength = 191\n\
unsupported-key = ignored\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(load.unsupported, 1);
    assert_eq!(load.appearance.background, Color::rgb(1, 2, 3));
    assert_eq!(load.appearance.cursor_color, Color::rgb(4, 5, 6));
    assert_eq!(load.appearance.selection_foreground, Color::rgb(7, 8, 9));
    assert_eq!(
        load.appearance.search_match_color,
        AppearanceColor::rgba(0x10, 0x11, 0x12, 0x80)
    );
    assert_eq!(load.appearance.link_color, Color::rgb(0x16, 0x17, 0x18));
    assert!((load.appearance.minimum_contrast - 4.5).abs() < f32::EPSILON);
    assert!((load.appearance.background_opacity - 0.75).abs() < f32::EPSILON);
    assert_eq!(load.appearance.cursor_blink_policy, CursorBlinkPolicy::Off);
    assert_eq!(load.appearance.cursor_blink_interval_ms, 725);
    assert!(load.appearance.font_thicken);
    assert_eq!(load.appearance.font_thicken_strength, 191);
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.key == "font-thicken"
            && diagnostic.disposition == AppearanceConfigDisposition::Applied
    }));
}

#[test]
fn invalid_synthetic_style_and_thickening_strength_preserve_defaults() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "font-synthetic-style = no-bold,unknown\nfont-thicken-strength = 256\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    let defaults = TerminalAppearance::default();
    assert_eq!(load.invalid, 2, "{:#?}", load.diagnostics);
    assert_eq!(
        load.appearance.font_synthetic_style,
        defaults.font_synthetic_style
    );
    assert_eq!(
        load.appearance.font_thicken_strength,
        defaults.font_thicken_strength
    );
}

#[test]
fn matches_ghostty_quotes_features_x11_colors_and_asymmetric_padding() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "\
font-family = \"JetBrains Mono\"\n\
font-feature = -calt, -liga, 'kern' off, cv01 2\n\
window-padding-x = 2,4\n\
window-padding-y = 3,5\n\
foreground = gray42\n\
background = medium spring green\n\
palette = 0x0f=#102030\n\
palette = 0o20=#405060\n\
palette = 0b10001=#708090\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(load.appearance.font_families, ["JetBrains Mono"]);
    assert_eq!(
        load.appearance.font_features,
        [
            FontFeature::new(*b"calt", 0),
            FontFeature::new(*b"liga", 0),
            FontFeature::new(*b"kern", 0),
            FontFeature::new(*b"cv01", 2),
        ]
    );
    assert!((load.appearance.padding_left - 2.0).abs() < f32::EPSILON);
    assert!((load.appearance.padding_right - 4.0).abs() < f32::EPSILON);
    assert!((load.appearance.padding_top - 3.0).abs() < f32::EPSILON);
    assert!((load.appearance.padding_bottom - 5.0).abs() < f32::EPSILON);
    assert_eq!(load.appearance.foreground, Color::rgb(107, 107, 107));
    assert_eq!(load.appearance.background, Color::rgb(0, 250, 154));
    assert_eq!(load.appearance.palette[15], Color::rgb(0x10, 0x20, 0x30));
    assert_eq!(load.appearance.palette[16], Color::rgb(0x40, 0x50, 0x60));
    assert_eq!(load.appearance.palette[17], Color::rgb(0x70, 0x80, 0x90));
}

#[test]
fn invalid_font_feature_lines_do_not_partially_mutate_prior_state() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "font-feature = liga=0\nfont-feature = liga=1, invalid\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 1, "{:#?}", load.diagnostics);
    assert_eq!(
        load.appearance.font_features,
        [FontFeature::new(*b"liga", 0)]
    );
}

#[test]
fn includes_apply_after_the_containing_file_and_optional_missing_files_are_quiet() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("root");
    let child = directory.path().join("child");
    fs::write(
        &root,
        "config-file = child\nconfig-file = ?missing\nfont-size = 12\n",
    )
    .expect("write root");
    fs::write(&child, "font-size = 20\n").expect("write child");

    let load = load_ghostty_appearance_from(&root);
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert!((load.appearance.font_size_points - 20.0).abs() < f32::EPSILON);
}

#[test]
fn quoted_empty_repeated_values_restore_defaults() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "font-family = Custom\nfont-family = \"\"\nfont-family-bold = Custom Bold\nfont-family-bold = \"\"\nfont-feature = liga\nfont-feature = \"\"\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    let defaults = TerminalAppearance::default();
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(load.appearance.font_families, defaults.font_families);
    assert_eq!(
        load.appearance.font_families_bold,
        defaults.font_families_bold
    );
    assert_eq!(load.appearance.font_features, defaults.font_features);
}

#[test]
fn configuration_diagnostics_are_counted_but_memory_bounded() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(&path, "invalid\n".repeat(MAX_CONFIG_DIAGNOSTICS + 137)).expect("write config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid as usize, MAX_CONFIG_DIAGNOSTICS + 137);
    assert_eq!(load.diagnostics.len(), MAX_CONFIG_DIAGNOSTICS);
    assert_eq!(load.diagnostics_dropped, 137);
}

#[test]
fn non_regular_configuration_roots_are_rejected_without_opening_them() {
    let directory = tempfile::tempdir().expect("tempdir");
    let load = load_ghostty_appearance_from(directory.path());
    assert_eq!(load.invalid, 1);
    assert!(load.diagnostics[0].message.contains("not a regular file"));
}

#[test]
fn empty_repeated_values_restore_defaults_before_later_overrides() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    fs::write(
        &path,
        "\
font-family = First\n\
font-family =\n\
font-family = Final\n\
font-feature = liga=1\n\
font-feature =\n\
font-feature = calt=0\n\
palette = 7=#abcdef\n\
palette =\n\
font-size = 20\n\
font-size =\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from(&path);
    let defaults = TerminalAppearance::default();
    assert_eq!(load.invalid, 0, "{:#?}", load.diagnostics);
    assert_eq!(load.appearance.font_families, ["Final"]);
    assert_eq!(
        load.appearance.font_features,
        [FontFeature::new(*b"calt", 0)]
    );
    assert_eq!(load.appearance.palette[7], defaults.palette[7]);
    assert!((load.appearance.font_size_points - defaults.font_size_points).abs() < f32::EPSILON);
}

#[test]
fn oversized_configuration_stops_before_unbounded_parsing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("config");
    let file = fs::File::create(&path).expect("create oversized config");
    file.set_len(u64::try_from(MAX_CONFIG_BYTES).expect("limit fits u64") * 128)
        .expect("extend sparse config");

    let load = load_ghostty_appearance_from(&path);
    assert_eq!(load.invalid, 1);
    assert_eq!(load.bytes_read, 0);
    assert_eq!(load.appearance, TerminalAppearance::default());
    assert!(load.diagnostics[0].message.contains("byte limit"));
}
