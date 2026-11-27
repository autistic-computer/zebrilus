use super::*;

#[test]
fn appearance_override_beats_the_ghostty_value() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    fs::write(&root, "background = #112233\n").expect("write config");

    let load = load_ghostty_appearance_from_for_with_overrides(
        &root,
        TerminalColorScheme::Dark,
        &[("background".to_owned(), "#abcdef".to_owned())],
    );

    assert_eq!(load.appearance.background, Color::rgb(0xab, 0xcd, 0xef));
    assert_eq!(
        load.provenance.source(AppearanceConfigKey::Background),
        AppearanceSource::Override
    );
}

#[test]
fn override_theme_loads_before_other_override_keys_regardless_of_file_order() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    fs::write(&root, "background = #010203\n").expect("write config");
    fs::write(
        themes.join("Overlay Theme"),
        "background = #112233\nforeground = #445566\npalette = 4=#778899\n",
    )
    .expect("write theme");

    let entries = vec![
        ("background".to_owned(), "#abcdef".to_owned()),
        ("theme".to_owned(), "Overlay Theme".to_owned()),
    ];
    let load =
        load_ghostty_appearance_from_for_with_overrides(&root, TerminalColorScheme::Dark, &entries);

    assert!(!load.fatal, "{:#?}", load.diagnostics);
    assert_eq!(load.appearance.background, Color::rgb(0xab, 0xcd, 0xef));
    assert_eq!(load.appearance.foreground, Color::rgb(0x44, 0x55, 0x66));
    assert_eq!(load.appearance.palette[4], Color::rgb(0x77, 0x88, 0x99));
}

#[test]
fn empty_override_set_restores_the_unmodified_base_load() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    fs::write(&root, "foreground = #123456\n").expect("write config");
    let base = load_ghostty_appearance_from_for(&root, TerminalColorScheme::Dark);
    let overridden = apply_appearance_overrides(
        base.clone(),
        &[("foreground".to_owned(), "#abcdef".to_owned())],
    );
    let restored = apply_appearance_overrides(base.clone(), &[]);

    assert_ne!(overridden.appearance, base.appearance);
    assert_eq!(restored.appearance, base.appearance);
    assert_eq!(restored.provenance, base.provenance);
}

#[test]
fn provenance_distinguishes_default_theme_ghostty_and_override_tiers() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("config");
    let themes = directory.path().join("themes");
    fs::create_dir(&themes).expect("create themes");
    fs::write(themes.join("Tiered"), "palette = 4=#778899\n").expect("write theme");
    fs::write(&root, "theme = Tiered\nforeground = #123456\n").expect("write config");

    let load = load_ghostty_appearance_from_for_with_overrides(
        &root,
        TerminalColorScheme::Dark,
        &[("background".to_owned(), "#abcdef".to_owned())],
    );

    assert_eq!(
        load.provenance.source(AppearanceConfigKey::FontSize),
        AppearanceSource::Default
    );
    assert_eq!(
        load.provenance.source(AppearanceConfigKey::Palette),
        AppearanceSource::ThemeFile
    );
    assert_eq!(
        load.provenance.source(AppearanceConfigKey::Foreground),
        AppearanceSource::Ghostty
    );
    assert_eq!(
        load.provenance.source(AppearanceConfigKey::Background),
        AppearanceSource::Override
    );
    assert_eq!(
        load.provenance.source(AppearanceConfigKey::Theme),
        AppearanceSource::Ghostty
    );
}
