use super::test_support::styled_viewport;
use super::*;

#[test]
fn sgr_changes_once_per_style_run() {
    let viewport = styled_viewport();
    let mut renderer = Renderer::new();
    renderer.blit_row(
        &viewport,
        0,
        Rect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        },
    );
    let output = String::from_utf8(renderer.output).unwrap();

    assert_eq!(output.matches("\x1b[38;2;").count(), 2);
    assert!(output.contains("ab"));
    assert!(output.contains('c'));
}

#[test]
fn pane_cells_reach_the_terminal_in_the_class_the_program_wrote() {
    let default_foreground = Color::rgb(216, 222, 233);
    let default_background = Color::rgb(16, 19, 24);
    let sgr = |foreground: ColourClass, background: ColourClass, colour: Color| {
        let style = PackedStyle::new(colour, colour, None, 0, UnderlineStyle::None)
            .with_classes(foreground, background);
        let mut output = Vec::new();
        write_sgr(
            &mut output,
            style,
            false,
            default_foreground,
            default_background,
            &TmuxStyle::default(),
        );
        String::from_utf8(output).unwrap()
    };
    let red = Color::rgb(205, 0, 0);
    assert_eq!(
        sgr(ColourClass::Palette(1), ColourClass::Palette(9), red),
        "\x1b[0m\x1b[31m\x1b[101m"
    );
    assert_eq!(
        sgr(ColourClass::Palette(42), ColourClass::Palette(200), red),
        "\x1b[0m\x1b[38;5;42m\x1b[48;5;200m"
    );
    assert_eq!(
        sgr(ColourClass::Rgb, ColourClass::Default, default_foreground),
        "\x1b[0m\x1b[38;2;216;222;233m\x1b[49m"
    );
    assert_eq!(
        sgr(ColourClass::Default, ColourClass::Rgb, red),
        "\x1b[0m\x1b[39m\x1b[48;2;205;0;0m"
    );
    assert_eq!(
        sgr(
            ColourClass::Resolved,
            ColourClass::Resolved,
            default_foreground
        ),
        "\x1b[0m\x1b[39m\x1b[48;2;216;222;233m"
    );
}

#[test]
fn styled_lines_send_named_and_indexed_colours_the_way_the_pin_does() {
    let appearance = TerminalAppearance::default();
    let line = StyledLine::parsed(
        "#[fg=red,bg=#010203,bold,underscore,reverse]X#[fg=colour42,nobold,nounderscore,noreverse]Y#[fg=brightgreen]Z",
    );
    let mut output = Vec::new();
    write_styled_text(
        &mut output,
        0,
        0,
        &line,
        appearance.background,
        appearance.foreground,
        &appearance,
    );
    let output = String::from_utf8(output).unwrap();

    assert!(output.contains("\x1b[1m"));
    assert!(output.contains("\x1b[4m"));
    assert!(output.contains("\x1b[7m"));
    assert!(output.contains("\x1b[31m"), "{output:?}");
    assert!(output.contains("\x1b[48;2;1;2;3m"), "{output:?}");
    assert!(output.contains("\x1b[38;5;42m"), "{output:?}");
    assert!(output.contains("\x1b[92m"), "{output:?}");
    let red = appearance.palette[1];
    assert!(
        !output.contains(&format!("38;2;{};{};{}", red.r, red.g, red.b)),
        "a named colour must not be resolved against zz's own palette: {output:?}"
    );
    assert!(output.contains('X'));
    assert!(output.contains('Y'));
    assert!(output.contains('Z'));
    assert!(!output.contains("#["));
}

#[test]
fn only_cursor_placement_counts_as_cursor_only() {
    assert!(cursor_only(b"\x1b[1;31H\x1b[?25h"));
    assert!(cursor_only(b"\x1b[?25l"));
    assert!(cursor_only(b""));
    assert!(!cursor_only(b"\x1b[1;31Hx"));
    assert!(!cursor_only(b"\x1b[0m\x1b[1;1H"));
    assert!(!cursor_only(b"\x1b[2J"));
}

#[test]
fn border_runs_place_the_cursor_once_per_run_and_the_rendition_once_per_change() {
    let red = Rc::new(b"\x1b[0;31m".to_vec());
    let green = Rc::new(b"\x1b[0;32m".to_vec());
    let cells = BTreeMap::from([
        ((0, 0), (Rc::clone(&red), "─".to_owned())),
        ((0, 1), (Rc::clone(&red), "─".to_owned())),
        ((0, 2), (Rc::clone(&red), "┬".to_owned())),
        ((1, 2), (Rc::new(b"\x1b[0;31m".to_vec()), "│".to_owned())),
        ((2, 2), (green, "│".to_owned())),
    ]);
    let mut output = Vec::new();
    write_border_runs(&mut output, &cells, 80);
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "\x1b[1;1H\x1b[0;31m──┬\x08\n│\x08\n\x1b[0;32m│\x1b[0m"
    );
    let mut output = Vec::new();
    write_border_runs(&mut output, &cells, 3);
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "\x1b[1;1H\x1b[0;31m──┬\x1b[2;3H│\x1b[3;3H\x1b[0;32m│\x1b[0m",
        "a cell in the last column leaves the cursor waiting to wrap"
    );
}
