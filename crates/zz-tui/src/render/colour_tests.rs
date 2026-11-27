use super::*;

#[test]
fn a_terminal_under_its_colours_takes_the_pin_nearest_one() {
    assert_eq!(downgrade_palette(42, 16_777_216), 42);
    assert_eq!(downgrade_palette(42, 256), 42);
    assert_eq!(downgrade_palette(42, 16), 10);
    assert_eq!(downgrade_palette(42, 8), 2);
    assert_eq!(downgrade_palette(9, 16), 9);
    assert_eq!(downgrade_palette(9, 8), 1);
    assert_eq!(downgrade_palette(12, 8), 4);
    assert_eq!(downgrade_palette(1, 8), 1);
    let dark = Color::rgb(10, 20, 30);
    assert_eq!(colour_find_rgb(dark), 233);
    assert_eq!(downgrade_palette(colour_find_rgb(dark), 256), 233);
    assert_eq!(downgrade_palette(colour_find_rgb(dark), 8), 0);
    assert_eq!(colour_find_rgb(Color::rgb(0x5f, 0x87, 0xaf)), 67);
    assert_eq!(colour_find_rgb(Color::rgb(255, 255, 255)), 231);
    assert_eq!(colour_find_rgb(Color::rgb(0, 0, 0)), 16);
}

#[test]
fn a_terminal_that_answered_nothing_leaves_every_colour_alone() {
    assert_eq!(crate::tty::terminal_colours(), None);
    let mut output = Vec::new();
    write_rgb_ground(&mut output, Color::rgb(10, 20, 30), Ground::Foreground);
    write_palette_ground(&mut output, 42, Ground::Foreground);
    assert_eq!(output, b"\x1b[38;2;10;20;30m\x1b[38;5;42m");
}

#[test]
fn the_published_theme_defaults_are_the_table_the_client_used_to_hard_code() {
    let published = zz_protocol::ThemeColours::default();
    for (index, name) in DARK_THEME_COLOURS.iter().enumerate() {
        let packed = zz_terminal::parse_x11_color(name)
            .unwrap_or_else(|| panic!("{name} is an X11 colour"))
            .packed();
        assert_eq!(
            published.slot(u8::try_from(index).expect("ten slots")),
            Some(TmuxColour::Rgb(packed)),
            "slot {index} ({name}) drifted from the wire default"
        );
    }
}
