use super::test_support::{block_model, menu_state};
use super::*;

#[test]
fn floating_border_tables_cover_every_published_line_style() {
    assert_eq!(floating_border(PopupBorderLines::Single).top_left, "┌");
    assert_eq!(
        floating_border(PopupBorderLines::Double).separator_left,
        "╠"
    );
    assert_eq!(floating_border(PopupBorderLines::Heavy).bottom_right, "┛");
    assert_eq!(floating_border(PopupBorderLines::Simple).vertical, "|");
    assert_eq!(floating_border(PopupBorderLines::Rounded).top_right, "╮");
    assert_eq!(floating_border(PopupBorderLines::Padded).horizontal, " ");
    assert_eq!(floating_border(PopupBorderLines::None).top_left, " ");
}

#[test]
fn menu_paints_title_rows_shortcuts_selection_and_disabled_style() {
    let mut model = block_model(40, 12);
    model.menu = Some(menu_state());
    model.menu_selection = Some(0);
    let mut renderer = Renderer::new();
    renderer.paint_menu(&model);
    let output = String::from_utf8(renderer.output).unwrap();

    assert_eq!(
        output,
        "\u{1b}[3;6H\u{1b}[0m\u{1b}[31m\u{1b}[49m┌──────────────────┐\u{1b}[0m\u{1b}[3;8H\u{1b}[0m\u{1b}[31m\u{1b}[49mActions─────────\u{1b}[0m\u{1b}[4;6H\u{1b}[0m\u{1b}[31m\u{1b}[49m│\u{1b}[0m\u{1b}[1m\u{1b}[30m\u{1b}[43m 界 item      (q) \u{1b}[0m\u{1b}[31m\u{1b}[49m│\u{1b}[0m\u{1b}[5;6H\u{1b}[0m\u{1b}[31m\u{1b}[49m├──────────────────┤\u{1b}[0m\u{1b}[6;6H\u{1b}[0m\u{1b}[31m\u{1b}[49m│\u{1b}[0m\u{1b}[37m\u{1b}[44m \u{1b}[0m\u{1b}[2m\u{1b}[37m\u{1b}[44mDisabled\u{1b}[0m\u{1b}[37m\u{1b}[44m         \u{1b}[0m\u{1b}[31m\u{1b}[49m│\u{1b}[0m\u{1b}[7;6H\u{1b}[0m\u{1b}[31m\u{1b}[49m└──────────────────┘\u{1b}[0m"
    );
}
