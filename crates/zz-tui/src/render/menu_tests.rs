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

    assert!(output.contains("\x1b[3;6H"), "{output:?}");
    assert!(output.contains("Actions"));
    assert!(output.contains("界 item"));
    assert!(output.contains("(q)"));
    assert!(output.contains('┌'));
    assert!(output.contains('├'));
    assert!(output.contains('┘'));
    assert!(output.contains("\x1b[1m"));
    assert!(output.contains("\x1b[2m"));
}
