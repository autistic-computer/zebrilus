use super::*;

#[test]
fn named_theme_is_loaded_before_explicit_user_overrides() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    let theme = themes.join("My Theme");
    fs::write(
        &theme,
        "foreground = #e0e8f0\nbackground = #445566\nfont-family = Theme Mono\npalette = 4=#abcdef\n",
    )
    .expect("write theme");
    fs::write(
        &root,
        "foreground = #fedcba\ntheme = My Theme\nfont-family = User Mono\n",
    )
    .expect("write config");

    let load = load_ghostty_appearance_from_for(&root, TerminalColorScheme::Dark);
    assert!(!load.fatal, "{:#?}", load.diagnostics);
    assert_eq!(load.theme_path, Some(theme));
    assert_eq!(load.appearance.foreground, Color::rgb(0xfe, 0xdc, 0xba));
    assert_eq!(load.appearance.background, Color::rgb(0x44, 0x55, 0x66));
    assert_eq!(load.appearance.font_families, ["User Mono"]);
    assert_eq!(load.appearance.palette[4], Color::rgb(0xab, 0xcd, 0xef));
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.key == "theme" && diagnostic.disposition == AppearanceConfigDisposition::Applied
    }));
}

#[test]
fn a_listed_light_theme_fills_missing_keys_from_the_light_defaults() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    fs::write(
        themes.join("zz-listed-paper"),
        "background = #f0f1f2\nforeground = #202122\n",
    )
    .expect("write light theme");
    fs::write(&root, "theme = zz-listed-paper\n").expect("write config");

    let listed = enumerate_ghostty_themes(Some(&root))
        .into_iter()
        .find(|theme| theme.name == "zz-listed-paper")
        .expect("listed theme")
        .appearance;
    let host = load_ghostty_appearance_from_for(&root, TerminalColorScheme::Light).appearance;
    assert_eq!(listed.cursor_color, Color::rgb(0x2f, 0x5f, 0xc4));
    assert_eq!(
        (
            listed.cursor_color,
            listed.selection_foreground,
            listed.selection_background
        ),
        (
            host.cursor_color,
            host.selection_foreground,
            host.selection_background
        )
    );
}

#[test]
fn adaptive_theme_uses_the_requested_system_scheme() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    fs::write(themes.join("day"), "background = #f0f1f2\n").expect("write light theme");
    fs::write(themes.join("night"), "background = #101112\n").expect("write dark theme");
    fs::write(&root, "theme = dark:night,light:day\n").expect("write config");

    let light = load_ghostty_appearance_from_for(&root, TerminalColorScheme::Light);
    let dark = load_ghostty_appearance_from_for(&root, TerminalColorScheme::Dark);
    assert_eq!(light.appearance.color_scheme, TerminalColorScheme::Light);
    assert_eq!(dark.appearance.color_scheme, TerminalColorScheme::Dark);
    assert_eq!(light.appearance.background, Color::rgb(0xf0, 0xf1, 0xf2));
    assert_eq!(dark.appearance.background, Color::rgb(0x10, 0x11, 0x12));
}

#[test]
fn a_lone_theme_paints_only_its_own_mode_and_a_pair_may_leave_one_built_in() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    fs::write(themes.join("night"), "background = #101112\n").expect("write dark theme");
    let built_in = TerminalAppearance::default_for(TerminalColorScheme::Light).background;
    for (theme, light, dark) in [
        ("night", built_in, Color::rgb(0x10, 0x11, 0x12)),
        ("dark:night", built_in, Color::rgb(0x10, 0x11, 0x12)),
        (
            "light:night",
            Color::rgb(0x10, 0x11, 0x12),
            TerminalAppearance::default().background,
        ),
    ] {
        fs::write(&root, format!("theme = {theme}\n")).expect("write config");
        let shown = |scheme| load_ghostty_appearance_from_for(&root, scheme);
        assert_eq!(
            [
                shown(TerminalColorScheme::Light).appearance.background,
                shown(TerminalColorScheme::Dark).appearance.background,
            ],
            [light, dark],
            "{theme}"
        );
        assert!(!shown(TerminalColorScheme::Light).fatal, "{theme}");
    }
    assert_eq!(
        ThemeChoice::parse("night").map(|choice| choice.slots(&root)),
        Ok((None, Some("night".to_owned())))
    );
    assert_eq!(
        ThemeChoice::spelled(Some("day"), Some("night")).as_deref(),
        Some("light:day,dark:night")
    );
    assert_eq!(
        ThemeChoice::spelled(None, Some("night")).as_deref(),
        Some("dark:night")
    );
}

#[test]
fn missing_or_invalid_theme_is_fatal_but_bounded() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    fs::write(&root, "theme = missing-theme\nforeground = #abcdef\n").expect("write config");

    let load = load_ghostty_appearance_from(&root);
    assert!(load.fatal);
    assert_eq!(load.invalid, 1);
    assert_eq!(load.appearance.foreground, Color::rgb(0xab, 0xcd, 0xef));
    assert!(load.diagnostics[0].message.contains("cannot resolve"));
}
