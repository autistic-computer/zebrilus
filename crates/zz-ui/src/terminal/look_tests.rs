use super::*;

#[test]
fn contrast_correction_reaches_the_floor_for_every_color_but_decorative_glyphs() {
    let mut appearance = TerminalAppearance {
        minimum_contrast: 4.5,
        ..TerminalAppearance::default()
    };
    let background = Color::rgb(245, 245, 245);
    let foreground = Color::rgb(238, 238, 238);
    for attributes in [0, zz_terminal::ATTR_EXPLICIT_RGB] {
        let style = PackedStyle::new(
            foreground,
            background,
            None,
            attributes,
            TerminalUnderlineStyle::None,
        );
        let corrected = resolved_foreground(style, &appearance, false);
        assert!(contrast_ratio(corrected, background) >= 4.5);
        assert_eq!(resolved_foreground(style, &appearance, true), foreground);
    }

    let style = PackedStyle::new(
        foreground,
        background,
        None,
        0,
        TerminalUnderlineStyle::None,
    );
    appearance.minimum_contrast = 1.0;
    assert_eq!(resolved_foreground(style, &appearance, false), foreground);
}

#[test]
fn contrast_correction_keeps_normal_and_faint_input_text_readable() {
    let appearance = TerminalAppearance::default();
    assert_eq!(appearance.minimum_contrast, 4.5);
    for (foreground, background) in [
        (Color::rgb(0, 0, 0), Color::rgb(0x38, 0x38, 0x38)),
        (Color::rgb(0, 0, 0), Color::rgb(0x2f, 0x2f, 0x2f)),
        (Color::rgb(0x80, 0x80, 0x80), Color::rgb(0xff, 0xff, 0xff)),
    ] {
        for attributes in [
            0,
            zz_terminal::ATTR_FAINT,
            zz_terminal::ATTR_EXPLICIT_RGB,
            zz_terminal::ATTR_EXPLICIT_RGB | zz_terminal::ATTR_FAINT,
        ] {
            let style = PackedStyle::new(
                foreground,
                background,
                None,
                attributes,
                TerminalUnderlineStyle::None,
            );
            let corrected = resolved_foreground(style, &appearance, false);
            let run = text_run(
                style,
                1,
                &ala::font("monospace"),
                corrected,
                &appearance,
                1.0,
                1.0,
            );
            assert_eq!(run.color.a, 1.0);
            assert!(
                contrast_ratio(terminal_color(run.color), background)
                    >= appearance.minimum_contrast,
                "{foreground:?} on {background:?}, attributes={attributes}: {run:?}"
            );
        }
    }
}

#[test]
fn contrast_exemptions_preserve_faint_colors() {
    let style = PackedStyle::new(
        Color::rgb(0x80, 0x80, 0x80),
        Color::rgb(0xf8, 0xf8, 0xf8),
        None,
        zz_terminal::ATTR_FAINT,
        TerminalUnderlineStyle::None,
    );
    let dimmed = Color::rgb(0xb2, 0xb2, 0xb2);
    assert_eq!(
        resolved_foreground(style, &TerminalAppearance::default(), true),
        dimmed
    );
    let disabled = TerminalAppearance {
        minimum_contrast: 1.0,
        ..TerminalAppearance::default()
    };
    assert_eq!(resolved_foreground(style, &disabled, false), dimmed);
}

#[test]
fn the_default_floor_rescues_unreadable_text_and_leaves_the_rest() {
    let floor = TerminalAppearance::default().minimum_contrast;
    let dark = Color::rgb(0x10, 0x13, 0x18);
    let white = Color::rgb(u8::MAX, u8::MAX, u8::MAX);
    for readable in [
        Color::rgb(0xd8, 0xde, 0xe9),
        Color::rgb(0x00, 0xcd, 0x00),
        Color::rgb(0x7f, 0x7f, 0x7f),
    ] {
        assert_eq!(contrasted_foreground(readable, dark, floor), readable);
    }
    for (unreadable, background) in [
        (Color::rgb(0x00, 0x00, 0x00), dark),
        (Color::rgb(0xff, 0xff, 0x00), white),
        (Color::rgb(0xf8, 0xf8, 0xf2), white),
        (Color::rgb(0x50, 0xfa, 0x7b), white),
    ] {
        let corrected = contrasted_foreground(unreadable, background, floor);
        assert_ne!(corrected, unreadable);
        assert!(contrast_ratio(corrected, background) >= floor);
    }
}

#[test]
fn contrast_correction_keeps_the_hue() {
    let hue = |color: Color| {
        let (_, green_red, blue_yellow) = oklab(color);
        blue_yellow.atan2(green_red).to_degrees()
    };
    for (foreground, background) in [
        (Color::rgb(0x00, 0x00, 0xee), Color::rgb(0x10, 0x13, 0x18)),
        (Color::rgb(0xcd, 0x00, 0x00), Color::rgb(0x10, 0x13, 0x18)),
        (Color::rgb(0xb5, 0x89, 0x00), Color::rgb(0xfd, 0xf6, 0xe3)),
    ] {
        let corrected = ensure_minimum_contrast(foreground, background, 4.5);
        assert!(contrast_ratio(corrected, background) >= 4.5);
        let drift = (hue(corrected) - hue(foreground)).abs();
        assert!(
            drift.min(360.0 - drift) < 4.0,
            "{foreground:?} -> {corrected:?}"
        );
    }
}

#[test]
fn contrast_correction_is_cached_per_color_pair_and_floor() {
    let foreground = Color::rgb(0x00, 0x00, 0xee);
    let background = Color::rgb(0x10, 0x13, 0x18);
    for floor in [2.0, 4.5, 7.0] {
        let expected = ensure_minimum_contrast(foreground, background, floor);
        assert_eq!(
            contrasted_foreground(foreground, background, floor),
            expected
        );
        assert_eq!(
            contrasted_foreground(foreground, background, floor),
            expected
        );
    }
    assert_ne!(
        contrasted_foreground(foreground, background, 2.0),
        contrasted_foreground(foreground, background, 7.0)
    );
}

#[test]
fn decorative_character_ranges_match_terminal_separators_only() {
    for character in ['─', '█', '◆', '\u{e0b0}', '\u{e0d7}', '\u{1fb00}'] {
        assert!(is_decorative_character(character), "{character:?}");
    }
    for character in ['A', '$', '→', '\u{f00c}', '😀'] {
        assert!(!is_decorative_character(character), "{character:?}");
    }
}

#[test]
fn a_terminal_theme_hands_the_chrome_its_ground_its_signature_and_its_readable_status() {
    let stock = TerminalAppearance::default_for(zz_terminal::TerminalColorScheme::Dark);
    assert_eq!(terminal_look(&stock), None);
    let hex = |value: &str| {
        let digits = u32::from_str_radix(value, 16).expect("hex color");
        let [_, r, g, b] = digits.to_be_bytes();
        Color::rgb(r, g, b)
    };
    let mut classic = TerminalAppearance {
        background: hex("131313"),
        foreground: hex("dddddd"),
        cursor_color: hex("dddddd"),
        ..stock
    };
    for (index, value) in [
        "303030", "c75828", "62ba46", "ded9b7", "c28b12", "c72855", "fdd888", "dddddd", "57564f",
        "e7988a", "b0dca2", "f0edde", "f1c464", "efb0c2", "fef1d5", "dddddd",
    ]
    .into_iter()
    .enumerate()
    {
        classic.palette[index] = hex(value);
    }
    let look = terminal_look(&classic).expect("a themed terminal has a look");
    let spelled = |color: Option<Hsla>| color.map(penna::to_hex);
    assert_eq!(look.mode, ThemeMode::Dark);
    assert_eq!(
        [
            spelled(look.colors.bg),
            spelled(look.colors.fg),
            spelled(look.accent),
            spelled(look.colors.danger),
            spelled(look.colors.success),
            spelled(look.colors.warning),
        ],
        [
            Some("#131313".to_owned()),
            Some("#dddddd".to_owned()),
            Some("#c28b12".to_owned()),
            Some("#e7988a".to_owned()),
            Some("#62ba46".to_owned()),
            None,
        ]
    );
}

#[ala::test]
fn the_theme_palette_draws_text_on_the_theme_and_keeps_dark_and_light_apart(
    cx: &mut ala::TestAppContext,
) {
    cx.update(crate::init);
    for mode in [crate::ThemeMode::Dark, crate::ThemeMode::Light] {
        cx.update(|cx| penna::Theme::change(mode, None, cx));
        let colors = cx.update(|cx| theme_terminal_colors(cx));
        let theme = cx.update(|cx| cx.theme().colors);
        assert_eq!(colors[0], terminal_color(theme.foreground.opaque()));
        assert_eq!(colors[1], terminal_color(theme.background.opaque()));
        assert_eq!(colors[3], terminal_color(theme.danger.opaque()));
        assert_eq!(colors[4], terminal_color(theme.success.opaque()));
        let lightness = |color: Color| crate::oklab_lightness(color_of(color));
        assert!(
            lightness(colors[2]) < lightness(colors[9]),
            "{mode:?}: black under white"
        );
        assert!(
            lightness(colors[10]) < lightness(colors[17]),
            "{mode:?}: bright black under bright white"
        );
    }
}

fn color_of(value: Color) -> Hsla {
    color(value, 1.0)
}
