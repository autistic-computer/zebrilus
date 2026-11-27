use std::fs;
use zz_terminal::{
    AppearanceColor, AppearanceConfigKey, CellHeightAdjustment, Color, CursorBlinkPolicy,
    CursorStyle, TerminalAppearance,
};

use super::*;
use zz_terminal::{
    AppearanceLoad, AppearanceSource, FontFeature, FontSyntheticStyle, TerminalColorScheme,
    apply_appearance_overrides, load_ghostty_appearance_from_for,
};

fn distinctive_appearance() -> TerminalAppearance {
    let mut appearance = TerminalAppearance {
        color_scheme: TerminalColorScheme::Light,
        font_families: vec!["Berkeley Mono".to_owned(), "Symbols # Fallback".to_owned()],
        font_families_bold: vec!["Berkeley Mono Bold".to_owned()],
        font_families_italic: vec!["Berkeley Mono Italic".to_owned()],
        font_families_bold_italic: vec!["Berkeley Mono Bold Italic".to_owned()],
        font_size_points: 15.25,
        font_weight: 575,
        font_features: vec![FontFeature::new(*b"liga", 0), FontFeature::new(*b"ss03", 1)],
        font_synthetic_style: FontSyntheticStyle {
            bold: false,
            italic: true,
            bold_italic: false,
        },
        font_thicken: true,
        font_thicken_strength: 173,
        cell_height_adjustment: CellHeightAdjustment::Percent(12.5),
        padding_left: 3.0,
        padding_right: 7.5,
        padding_top: 11.0,
        padding_bottom: 13.25,
        foreground: Color::rgb(0x12, 0x34, 0x56),
        background: Color::rgb(0x65, 0x43, 0x21),
        cursor_color: Color::rgb(0xab, 0xcd, 0xef),
        cursor_style: CursorStyle::Underline,
        selection_foreground: Color::rgb(0x0a, 0x0b, 0x0c),
        selection_background: AppearanceColor::rgba(0x10, 0x20, 0x30, 0x40),
        search_match_color: AppearanceColor::rgba(0x50, 0x60, 0x70, 0x80),
        search_current_color: AppearanceColor::rgba(0x90, 0xa0, 0xb0, 0xc0),
        link_color: Color::rgb(0xde, 0xad, 0x01),
        copy_cursor_color: AppearanceColor::rgba(0xca, 0xfe, 0xba, 0xbe),
        minimum_contrast: 4.5,
        cursor_blink_policy: CursorBlinkPolicy::On,
        cursor_blink_interval_ms: 725,
        rounded_selection: false,
        background_opacity: 0.875,
        ..TerminalAppearance::default()
    };
    appearance.palette[1] = Color::rgb(0x21, 0x43, 0x65);
    appearance.palette[42] = Color::rgb(0xfe, 0xdc, 0xba);
    appearance
}

fn parsed_appearance_entries(parsed: &ParsedConfig) -> Vec<ConfigOverrideEntry> {
    parsed
        .daemon_entries
        .iter()
        .filter(|(key, _)| AppearanceConfigKey::from_config_key(key).is_some())
        .cloned()
        .collect()
}

fn importable_values(appearance: &TerminalAppearance) -> Vec<(AppearanceConfigKey, Vec<String>)> {
    AppearanceConfigKey::ALL
        .into_iter()
        .filter_map(|key| {
            let group = appearance_config_values(appearance, key)
                .unwrap_or_else(|error| panic!("serialize {key:?}: {error}"));
            (!group.is_empty()).then_some((key, group))
        })
        .collect()
}

#[test]
fn import_round_trips_every_appearance_shape() {
    let appearance = distinctive_appearance();
    let values = importable_values(&appearance);
    let source = "# user header stays byte-identical\r\n\
                  unknown-key = untouched\r\n";

    let imported = apply_import_edits(source, &values).expect("import donor values");

    assert!(imported.starts_with(source));
    assert!(imported.contains("font-family = \"Berkeley Mono\"\r\n"));
    assert!(imported.contains("font-family = \"Symbols # Fallback\"\r\n"));
    assert!(imported.contains("font-family-bold = \"Berkeley Mono Bold\"\r\n"));
    assert!(imported.contains("font-family-italic = \"Berkeley Mono Italic\"\r\n"));
    assert!(imported.contains("font-family-bold-italic = \"Berkeley Mono Bold Italic\"\r\n"));
    assert!(imported.contains("font-synthetic-style = no-bold,italic,no-bold-italic\r\n"));
    assert!(imported.contains("font-thicken = true\r\n"));
    assert!(imported.contains("font-thicken-strength = 173\r\n"));
    assert!(imported.contains("cursor-style = underline\r\n"));
    let palette_lines = imported
        .lines()
        .filter(|line| line.starts_with("palette ="))
        .collect::<Vec<_>>();
    assert_eq!(
        palette_lines,
        ["palette = 1=#214365", "palette = 42=#FEDCBA"]
    );

    let parsed = parse_config(&imported);
    let resolved = apply_appearance_overrides(
        AppearanceLoad::defaults_for(appearance.color_scheme),
        &parsed_appearance_entries(&parsed),
    );
    assert_eq!(resolved.appearance, appearance);
    for (key, _) in values {
        assert_eq!(
            resolved.provenance.source(key),
            AppearanceSource::Override,
            "{key:?}"
        );
    }
}

#[test]
fn import_replaces_existing_keys_in_place_and_preserves_comments() {
    let source = "# user header\n\
                  background = #123456 # keep comment\n\
                  unknown-key = untouched\n";

    let imported = apply_import_edits(
        source,
        &[
            (AppearanceConfigKey::Background, vec!["#654321".to_owned()]),
            (AppearanceConfigKey::Foreground, vec!["#ABCDEF".to_owned()]),
        ],
    )
    .expect("import replaces in place");

    assert_eq!(
        imported,
        "# user header\n\
         background = #654321 # keep comment\n\
         unknown-key = untouched\n\
         foreground = #ABCDEF\n"
    );
}

#[test]
fn reimport_syncs_changed_donor_values_without_accumulation() {
    let first = apply_import_edits(
        "",
        &[
            (AppearanceConfigKey::FontSize, vec!["14".to_owned()]),
            (AppearanceConfigKey::Palette, vec!["1=#111111".to_owned()]),
        ],
    )
    .expect("first import");
    let second = apply_import_edits(
        &first,
        &[
            (AppearanceConfigKey::FontSize, vec!["16".to_owned()]),
            (AppearanceConfigKey::Palette, vec!["2=#222222".to_owned()]),
        ],
    )
    .expect("second import");

    assert_eq!(second, "font-size = 16\npalette = 2=#222222\n");
}

#[test]
fn import_replaces_cumulative_groups_wholesale_and_keeps_neighbors() {
    let source = "palette = 1=#111111\n\
                  # palette comment stays\n\
                  palette = 2=#222222\n\
                  font-size = 14\n";

    let imported = apply_import_edits(
        source,
        &[(
            AppearanceConfigKey::Palette,
            vec!["1=#214365".to_owned(), "42=#FEDCBA".to_owned()],
        )],
    )
    .expect("import replaces the group");

    assert_eq!(
        imported,
        "# palette comment stays\n\
         font-size = 14\n\
         palette = 1=#214365\n\
         palette = 42=#FEDCBA\n"
    );
}

#[test]
fn import_represents_natural_cell_height_with_a_reset() {
    let imported = apply_import_edits(
        "# keep\n",
        &[(AppearanceConfigKey::AdjustCellHeight, vec![String::new()])],
    )
    .expect("import natural cell height");
    assert_eq!(imported, "# keep\nadjust-cell-height = \n");

    let parsed = parse_config(&imported);
    let resolved = apply_appearance_overrides(
        AppearanceLoad::defaults_for(TerminalColorScheme::Dark),
        &parsed_appearance_entries(&parsed),
    );
    assert_eq!(
        resolved.appearance.cell_height_adjustment,
        CellHeightAdjustment::None
    );
}

#[test]
fn import_without_values_is_byte_identical() {
    let source = "# comments\r\nunknown = bytes # exactly\r\n";
    assert_eq!(
        apply_import_edits(source, &[]).expect("no-op import"),
        source
    );
}

#[test]
fn import_rejects_growth_past_the_bound_without_touching_the_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    let source = " ".repeat(MAX_CONFIG_BYTES);
    fs::write(&path, &source).expect("write bounded configuration");

    let error = import_appearance_values_at(
        &path,
        &[(AppearanceConfigKey::Background, vec!["#654321".to_owned()])],
    )
    .expect_err("import past the bound must fail");

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(error.to_string().contains("exceeding the 65536-byte limit"));
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged configuration"),
        source
    );
}

#[test]
fn deleting_ghostty_donors_after_import_changes_nothing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let ghostty_root = directory.path().join("xdg/ghostty/config");
    let theme = directory.path().join("xdg/ghostty/themes/M3 Distinctive");
    let zz_config = directory.path().join("xdg/zz/config");
    fs::create_dir_all(theme.parent().expect("theme parent")).expect("create theme directory");
    fs::create_dir_all(zz_config.parent().expect("zz config parent")).expect("create zz directory");
    fs::write(
        &theme,
        "background = #102938\n\
         foreground = #F1E2D3\n\
         selection-background = #22446688\n\
         palette = 1=#A1B2C3\n\
         palette = 42=#0F1E2D\n\
         zz-link-color = #55AAEE\n",
    )
    .expect("write theme");
    fs::write(
        &ghostty_root,
        "theme = M3 Distinctive\n\
         cursor-color = #ABCDEF\n\
         font-family = M3 Mono Family\n\
         window-padding-x = 4,9\n\
         background-opacity = 0.83\n",
    )
    .expect("write Ghostty root");
    fs::write(
        &zz_config,
        "# existing local settings and unknown lines survive\n\
         pane-margin = 12\n\
         future-key = byte-identical\n",
    )
    .expect("write zz config");

    let donor_appearance =
        load_ghostty_appearance_from_for(&ghostty_root, TerminalColorScheme::Dark);
    let values = crate::config::import::ghostty_import_values(&donor_appearance)
        .expect("serialize donor values");
    crate::config::import::copy_theme_files(&donor_appearance, &ghostty_root, &zz_config)
        .expect("copy the theme next to zz config");
    import_appearance_values_at(&zz_config, &values).expect("write imported zz config");

    fs::remove_file(&ghostty_root).expect("delete Ghostty donor");
    fs::remove_file(&theme).expect("delete Ghostty theme donor");
    assert!(!ghostty_root.exists());
    assert!(!theme.exists());

    let imported = fs::read_to_string(&zz_config).expect("read imported zz config");
    assert!(imported.starts_with(
        "# existing local settings and unknown lines survive\n\
         pane-margin = 12\n\
         future-key = byte-identical\n"
    ));
    let parsed = parse_config(&imported);
    let mut load = AppearanceLoad::defaults_for(TerminalColorScheme::Dark);
    load.root = Some(zz_config.clone());
    let resolved = apply_appearance_overrides(load, &parsed_appearance_entries(&parsed));
    assert_eq!(resolved.appearance, donor_appearance.appearance);
}
