use std::fs;
#[cfg(not(target_os = "linux"))]
use zz_terminal::TerminalAppearance;

use super::test_support::*;
use super::*;

#[test]
fn widget_corner_radius_defaults_until_overridden() {
    let parsed = parse_config("");
    assert_f32_eq(
        parsed.config.widget_corner_radius.value,
        DEFAULT_WIDGET_CORNER_RADIUS,
    );
    assert_eq!(
        parsed.config.widget_corner_radius.provenance,
        ConfigProvenance::Default
    );

    let parsed = parse_config("widget-corner-radius = 12\n");
    assert!(parsed.diagnostics.is_empty());
    assert_f32_eq(parsed.config.widget_corner_radius.value, 12.0);
    assert_eq!(
        parsed.config.widget_corner_radius.provenance,
        ConfigProvenance::Override
    );

    let parsed = parse_config("widget-corner-radius = 900\n");
    assert_eq!(parsed.diagnostics.len(), 1);
}

#[test]
fn ui_font_family_round_trips_names_and_preserves_other_settings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    let source = "# keep header\nfont-family = Terminal Font\nui-font-family = Earlier\nui-font-family = Initial # keep comment\n";
    fs::write(&path, source).expect("write configuration");

    for family in [
        "Segoe UI",
        "Noto Sans CJK JP",
        "日本語",
        r#"A "Quoted" #Font\Family"#,
    ] {
        assert!(write_ui_font_family_at(&path, Some(family)).expect("write UI font"));
        let written = fs::read_to_string(&path).expect("read UI font");
        assert!(
            written.starts_with(
                "# keep header\nfont-family = Terminal Font\nui-font-family = Earlier\n"
            )
        );
        assert!(written.ends_with(" # keep comment\n"));
        let parsed = parse_config(&written);
        assert!(parsed.diagnostics.is_empty(), "{written}");
        assert_eq!(parsed.ui_font.family.value.as_deref(), Some(family));
        assert_eq!(parsed.ui_font.family.provenance, ConfigProvenance::Override);
        assert_eq!(
            parsed.daemon_entries,
            [("font-family".to_owned(), "Terminal Font".to_owned())]
        );
    }

    let before = fs::read_to_string(&path).expect("read before invalid writes");
    for family in ["", "  ", "Bad\nFont", "Bad\rFont", "Bad\0Font"] {
        assert!(write_ui_font_family_at(&path, Some(family)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }
    let invalid = parse_config("ui-font-family = Valid\nui-font-family = \"\"\n");
    assert_eq!(invalid.ui_font.family.value.as_deref(), Some("Valid"));
    assert_eq!(invalid.diagnostics.len(), 1);

    write_ui_font_family_at(&path, None).expect("select system default over earlier values");
    assert_eq!(load_config(&path).unwrap().ui_font.family.value, None);
    assert!(!write_ui_font_family_at(&path, None).expect("unchanged default"));
}

#[ala::test]
fn ui_font_family_reloads_with_the_look_and_resets(cx: &mut ala::TestAppContext) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    cx.update(zz_ui::init);
    let mono = cx.update(|cx| penna::Theme::global(cx).mono_font_family.clone());

    for family in [Some("Noto Sans"), Some("Arial"), None] {
        write_ui_font_family_at(&path, family).expect("write UI font");
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            crate::theme::refresh_current_theme(cx);
            let expected =
                family.map_or_else(|| ala::Font::default().family, ala::SharedString::from);
            assert_eq!(penna::Theme::global(cx).font_family, expected);
            assert_eq!(penna::Theme::global(cx).mono_font_family, mono);
            assert_eq!(ui_font_family(cx).value.as_deref(), family);
            crate::theme::sync_system_appearance(None, cx);
            assert_eq!(penna::Theme::global(cx).font_family, expected);
        });
    }

    for family in [Some("Arial"), None] {
        zz_config::write_ui_mono_font_family_at(&path, family).expect("write mono font");
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            crate::theme::refresh_current_theme(cx);
            let expected = family.map_or_else(|| mono.clone(), ala::SharedString::from);
            assert_eq!(penna::Theme::global(cx).mono_font_family, expected);
            assert_eq!(appearance(cx).mono_font(), family.unwrap_or_default());
        });
    }

    for family in [Some("Georgia"), None] {
        zz_config::write_ui_chat_font_family_at(&path, family).expect("write chat font");
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            crate::theme::refresh_current_theme(cx);
            assert_eq!(
                penna::Theme::global(cx).chat_font(),
                family.unwrap_or(zz_ui::fonts::CHAT_FONT)
            );
            assert_eq!(appearance(cx).chat_font(), family.unwrap_or_default());
        });
    }

    write_ui_font_family_at(&path, Some("Arial")).expect("write another override");
    write_config_edit_at(&path, ConfigKey::UiFontFamily.as_str(), None).expect("reset override");
    cx.update(|cx| {
        install_config(Some(&path), Some(load_config(&path)), cx);
        crate::theme::refresh_current_theme(cx);
        assert_eq!(ui_font_family(cx).provenance, ConfigProvenance::Default);
        assert_eq!(
            penna::Theme::global(cx).font_family,
            zz_config::DEFAULT_UI_FONT_FAMILY
        );
        let parsed = parse_config("ui-font-family = Arial\n");
        install_config(Some(&path), Some(Ok(parsed)), cx);
        install_config(None, None, cx);
        crate::theme::refresh_current_theme(cx);
        assert_eq!(ui_font_family(cx), UiFontConfig::default().family);
        assert_eq!(
            penna::Theme::global(cx).font_family,
            zz_config::DEFAULT_UI_FONT_FAMILY
        );
    });
}

#[ala::test]
fn appearance_keys_redraw_the_look_and_reset_to_zz_look(cx: &mut ala::TestAppContext) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    cx.update(zz_ui::init);

    for (accent, tint, controls, glass) in [
        (Some("green"), Some("1"), Some("pill"), Some("none")),
        (Some("#ff8800"), None, Some("rounded"), Some("liquid")),
        (Some("purple"), None, None, None),
        (None, None, None, None),
    ] {
        for (key, value) in [
            (ConfigKey::ChromeAccent, accent),
            (ConfigKey::ChromeTint, tint),
            (ConfigKey::ControlShape, controls),
            (ConfigKey::InterfaceGlass, glass),
        ] {
            write_config_edit_at(&path, key.as_str(), value).expect("write appearance key");
        }
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            crate::theme::refresh_current_theme(cx);
            let mut expected = interface_theme::platform_theme().clone();
            interface_theme::ThemeOverrides {
                radius: Some(zz_config::DEFAULT_WIDGET_CORNER_RADIUS),
                accent: accent.map(str::to_owned),
                tint: tint.map(|tint| tint.parse().expect("a number")),
                controls: Some(
                    controls.map_or(interface_theme::Controls::Rounded, |controls| {
                        serde_json::from_value(serde_json::Value::String(controls.to_owned()))
                            .expect("a control shape")
                    }),
                ),
                glass: Some(
                    glass
                        .and_then(interface_theme::parse_glass)
                        .unwrap_or(interface_theme::Glass::Liquid),
                ),
                ..interface_theme::ThemeOverrides::default()
            }
            .apply_to(&mut expected);
            assert_eq!(shown_theme(cx), expected);
            let theme = penna::Theme::global(cx);
            assert_eq!(theme.colors, expected.colors(theme.mode));
            assert_eq!(theme.radius, px(expected.radius));
            assert_eq!(
                theme.control_radius,
                expected
                    .control_radius
                    .map(penna::foundation::interface_theme::ControlRadius::pixels)
            );
            assert_eq!(theme.glass.is_some(), glass != Some("none"));
        });
    }
}

#[ala::test]
fn importing_a_theme_file_sets_every_look_key_and_resetting_clears_them(
    cx: &mut ala::TestAppContext,
) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    fs::write(&path, "pane-gaps = true\nchrome-accent = pink\n").expect("write config");
    let file = interface_theme::parse_theme(
        r##"{"name":"Mono","tint":{"light":"#336699","dark":"#336699"},"face":"Lilex","radius":2.5,"control-radius":"full","neutral":{"hue":200,"amount":0.4},"glass":{"blur":8}}"##,
    )
    .expect("a theme file");
    let mut imported = Appearance::default();
    imported.import(&file);
    cx.update(zz_ui::init);

    let edits = look_edits(&imported.overrides, file.face.as_deref());
    write_config_keys_at(&path, &edits).expect("import");
    cx.update(|cx| {
        install_config(Some(&path), Some(load_config(&path)), cx);
        let appearance = appearance(cx);
        assert_eq!(appearance.overrides, imported.overrides);
        assert_eq!(appearance.ui_font(), "Lilex");
        assert_eq!(appearance.shown_theme(), imported.shown_theme());
    });

    let reset = look_edits(&interface_theme::ThemeOverrides::default(), None);
    write_config_keys_at(&path, &reset).expect("reset");
    assert_eq!(
        fs::read_to_string(&path).expect("read config"),
        "pane-gaps = true\n"
    );
}

#[ala::test]
fn chrome_contrast_validates_reloads_survives_mode_changes_and_resets(
    cx: &mut ala::TestAppContext,
) {
    for value in ["0.99", "2.01", "NaN", "inf", "invalid"] {
        let parsed = parse_config(&format!("chrome-contrast = {value}\n"));
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_f32_eq(parsed.config.chrome_contrast.value, 1.0);
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    cx.update(zz_ui::init);
    for (value, expected) in [(Some("1.5"), 1.5), (None, 1.0)] {
        write_config_edit_at(&path, ConfigKey::ChromeContrast.as_str(), value)
            .expect("write contrast");
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            crate::theme::refresh_current_theme(cx);
            assert_f32_eq(chrome_contrast(cx), expected);
            assert_f32_eq(penna::Theme::global(cx).contrast(), expected);
            assert_f32_eq(
                penna::Colorize::wash(&penna::Theme::global(cx).foreground).a,
                0.28 * expected,
            );
            assert_eq!(
                resolved_config(cx).chrome_contrast.provenance,
                if value.is_some() {
                    ConfigProvenance::Override
                } else {
                    ConfigProvenance::Default
                }
            );
            for mode in [penna::ThemeMode::Dark, penna::ThemeMode::Light] {
                penna::Theme::change(mode, None, cx);
                assert_f32_eq(penna::Theme::global(cx).contrast(), expected);
                assert_f32_eq(
                    penna::Colorize::wash(&penna::Theme::global(cx).foreground).a,
                    0.28 * expected,
                );
            }
        });
    }
    cx.update(|cx| penna::Theme::global_mut(cx).set_contrast(1.0));
}

#[ala::test]
fn chrome_density_validates_and_sets_the_kit_control_height(cx: &mut ala::TestAppContext) {
    let parsed = parse_config("chrome-density = roomy\n");
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.config.chrome_density.value,
        zz_client::appearance::DensitySetting::Regular
    );
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    cx.update(zz_ui::init);
    for (value, expected) in [
        (Some("compact"), 24.0),
        (Some("comfortable"), 32.0),
        (None, 28.0),
    ] {
        write_config_edit_at(&path, ConfigKey::ChromeDensity.as_str(), value)
            .expect("write density");
        cx.update(|cx| {
            install_config(Some(&path), Some(load_config(&path)), cx);
            assert_f32_eq(penna::Size::Medium.control_px(), expected);
        });
    }
}

#[ala::test]
fn ala_global_exposes_configured_pane_geometry(cx: &mut ala::TestAppContext) {
    assert_eq!(cx.update(|cx| resolved_config(cx)), AppConfig::default());

    let config = parse_config("pane-gaps = true\npane-margin = 9\n").config;
    cx.update(|cx| cx.set_global(AppConfig::from(config)));

    let resolved = cx.update(|cx| resolved_config(cx));
    assert!(resolved.pane_gaps.value);
    assert_f32_eq(resolved.pane_margin.value, 9.0);
}

#[ala::test]
fn install_config_publishes_and_clears_browser_config(cx: &mut ala::TestAppContext) {
    let parsed = parse_config("browser-element-selector-hotkey = alt-shift-e\n");
    let expected = parsed.browser.clone();

    cx.update(|cx| {
        install_config(Some(Path::new("/tmp/zz/config")), Some(Ok(parsed)), cx);
    });
    assert_eq!(cx.update(|cx| browser_config(cx)).0, expected);

    cx.update(|cx| install_config(None, None, cx));
    assert_eq!(cx.update(|cx| browser_config(cx)), BrowserConfig::default());
}

#[ala::test]
fn install_config_applies_and_resets_the_motion_setting(cx: &mut ala::TestAppContext) {
    let parsed = parse_config("motion = off\n");

    cx.update(|cx| {
        install_config(Some(Path::new("/tmp/zz/config")), Some(Ok(parsed)), cx);
    });
    assert!(cx.update(|cx| cx.reduce_motion()));

    cx.update(|cx| install_config(None, None, cx));
    assert!(!cx.update(|cx| cx.reduce_motion()));

    let parsed = parse_config("motion = fast\n");
    cx.update(|cx| {
        install_config(Some(Path::new("/tmp/zz/config")), Some(Ok(parsed)), cx);
    });
    assert!(!cx.update(|cx| cx.reduce_motion()));
    assert_eq!(cx.update(|cx| cx.animation_speed()), 2.0);
}

#[ala::test]
fn motion_setting_preserves_platform_reduced_motion(cx: &mut ala::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(PlatformReduceMotion(true));
        install_config(None, None, cx);
    });
    assert!(cx.update(|cx| cx.reduce_motion()));

    let parsed = parse_config("motion = fast\n");
    cx.update(|cx| {
        install_config(Some(Path::new("/tmp/zz/config")), Some(Ok(parsed)), cx);
    });
    assert!(cx.update(|cx| cx.reduce_motion()));
}

#[ala::test]
fn glass_blurs_the_window_until_it_is_off_or_the_system_makes_it_opaque(
    cx: &mut ala::TestAppContext,
) {
    let requested =
        |cx: &mut ala::TestAppContext| cx.update(|cx| requested_window_background_appearance(cx));
    assert_eq!(requested(cx), WindowBackgroundAppearance::Blurred);

    cx.update(|cx| {
        cx.set_global(GlassConfig(parse_config("interface-glass = none\n").glass));
    });
    assert_eq!(requested(cx), UNBLURRED_WINDOW_BACKGROUND);

    cx.update(|cx| {
        cx.set_global(GlassConfig::default());
        cx.set_global(SystemReducesTransparency(true));
    });
    assert_eq!(requested(cx), UNBLURRED_WINDOW_BACKGROUND);

    cx.update(|cx| cx.set_global(SystemReducesTransparency(false)));
    assert_eq!(requested(cx), WindowBackgroundAppearance::Blurred);

    #[cfg(target_os = "macos")]
    assert_eq!(
        cx.update(|cx| window_background_appearance(cx)),
        WindowBackgroundAppearance::Transparent
    );
}

#[cfg(not(target_os = "linux"))]
#[ala::test]
fn translucent_terminal_content_keeps_the_window_opaque(cx: &mut ala::TestAppContext) {
    let appearance = Arc::new(TerminalAppearance {
        background_opacity: 0.8,
        ..TerminalAppearance::default()
    });
    cx.update(|cx| {
        cx.set_global(GlassConfig(parse_config("interface-glass = none\n").glass));
        crate::theme::set_terminal_appearance(appearance, cx);
    });

    assert_eq!(
        cx.update(|cx| window_background_appearance(cx)),
        UNBLURRED_WINDOW_BACKGROUND
    );
}

fn corners(radius: f32) -> Corners<Pixels> {
    Corners::all(px(radius))
}

#[ala::test]
fn pane_content_radius_follows_the_theme_radius(cx: &mut ala::TestAppContext) {
    cx.update(zz_ui::init);
    let config = parse_config("pane-gaps = true\npane-margin = 6\n").config;
    cx.update(|cx| {
        cx.set_global(AppConfig::from(config));
        penna::Theme::global_mut(cx).radius = px(5.5);
    });

    assert_eq!(
        cx.update(|cx| pane_content_radii(cx, WindowCorners::NONE)),
        Corners::all(px(5.5))
    );
}

#[ala::test]
fn pane_gap_effective_values_follow_the_toggle(cx: &mut ala::TestAppContext) {
    cx.update(zz_ui::init);
    cx.update(|cx| penna::Theme::global_mut(cx).radius = px(10.0));
    assert!(cx.update(|cx| pane_gaps(cx)));

    let config = parse_config("pane-gaps = true\n").config;
    cx.update(|cx| cx.set_global(AppConfig::from(config)));
    assert!(cx.update(|cx| pane_gaps(cx)));
    assert_eq!(
        cx.update(|cx| pane_margin(cx)),
        px(zz_config::DEFAULT_PANE_MARGIN)
    );
    assert_eq!(
        cx.update(|cx| pane_content_radii(cx, WindowCorners::NONE)),
        Corners::all(cx.update(|cx| penna::Theme::global(cx).pane_radius()))
    );

    let config = parse_config("pane-gaps = true\npane-margin = 3\n").config;
    cx.update(|cx| {
        cx.set_global(AppConfig::from(config));
        penna::Theme::global_mut(cx).radius = px(0.0);
    });
    assert_eq!(cx.update(|cx| pane_margin(cx)), px(3.0));
    assert_eq!(
        cx.update(|cx| pane_content_radii(cx, WindowCorners::NONE)),
        corners(0.0)
    );

    let config = parse_config("pane-gaps = false\npane-margin = 2\n").config;
    cx.update(|cx| {
        cx.set_global(AppConfig::from(config));
        penna::Theme::global_mut(cx).radius = px(10.0);
    });
    assert_eq!(cx.update(|cx| pane_margin(cx)), px(0.0));
    assert_eq!(
        cx.update(|cx| pane_content_radii(cx, WindowCorners::NONE)),
        corners(0.0)
    );
}

#[ala::test]
fn flush_panes_inherit_the_derived_frame_curve_at_exposed_corners(cx: &mut ala::TestAppContext) {
    cx.update(zz_ui::init);
    let config = parse_config("pane-gaps = true\npane-margin = 0\n").config;
    cx.update(|cx| {
        cx.set_global(AppConfig::from(config));
        let theme = penna::Theme::global_mut(cx);
        theme.radius = px(5.5);
        theme.control_radius = None;
    });

    assert_eq!(
        cx.update(|cx| pane_content_radii(cx, WindowCorners::from_tiling(ala::Tiling::default()),)),
        corners(12.5)
    );
}
