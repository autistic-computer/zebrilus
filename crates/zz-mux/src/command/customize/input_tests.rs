use super::test_support::{current_row, engine_with_session, press};
use super::*;

#[test]
fn customize_only_the_mode_tree_exit_keys_close_the_mode() {
    let (mut engine, mut context, pane) = engine_with_session();
    for key in ["C-c", "C-d", "C-z", "C-j", "Space", "M-<", "M->", "x"] {
        let mut mode = CustomizeMode::default();
        let original = mode.clone();
        assert!(
            !press(&mut engine, &mut context, pane, &mut mode, key),
            "{key}"
        );
        assert_eq!(mode, original, "{key}");
    }
    for key in ["q", "Escape", "C-[", "C-g", "\u{1b}"] {
        assert!(
            press(
                &mut engine,
                &mut context,
                pane,
                &mut CustomizeMode::default(),
                key
            ),
            "{key}"
        );
    }
}

#[test]
fn customize_preview_flags_follow_mode_tree_start() {
    let (mut engine, mut context, _) = engine_with_session();
    for (flags, expected) in [
        (&[][..], Preview::Normal),
        (&["-N"][..], Preview::Off),
        (&["-NN"][..], Preview::Big),
        (&["-N", "-N"][..], Preview::Big),
    ] {
        let effects = engine
            .execute(
                &mut context,
                &CommandInvocation::new("customize-mode", flags.iter().copied()),
            )
            .unwrap()
            .effects;
        let [
            MuxEffect::PaneModeChanged {
                mode: Some(PaneModeRequest::Customize(mode)),
                ..
            },
        ] = effects.as_slice()
        else {
            panic!("customize-mode {flags:?} opened no mode: {effects:?}");
        };
        assert_eq!(mode.preview, expected, "{flags:?}");
    }
}

#[test]
fn customize_tree_keys_follow_mode_tree_key() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    press(&mut engine, &mut context, pane, &mut mode, "Right");
    assert!(mode.expanded.contains("options:0"));
    assert_eq!(mode.current, 0);
    press(&mut engine, &mut context, pane, &mut mode, "Right");
    assert_eq!(mode.current, 1);
    press(&mut engine, &mut context, pane, &mut mode, "Left");
    assert_eq!(mode.current, 0);
    assert!(!mode.expanded.contains("options:0"));
    press(&mut engine, &mut context, pane, &mut mode, "Down");
    press(&mut engine, &mut context, pane, &mut mode, "Left");
    assert_eq!(mode.current, 0);
    press(&mut engine, &mut context, pane, &mut mode, "M-+");
    press(&mut engine, &mut context, pane, &mut mode, "Down");
    press(&mut engine, &mut context, pane, &mut mode, "Down");
    press(&mut engine, &mut context, pane, &mut mode, "M--");
    assert_eq!(mode.current, 2);
    press(&mut engine, &mut context, pane, &mut mode, "t");
    assert_eq!(mode.current, 2);
    assert!(mode.tagged.is_empty());
    for expected in [Preview::Off, Preview::Big, Preview::Normal] {
        press(&mut engine, &mut context, pane, &mut mode, "v");
        assert_eq!(mode.preview, expected);
    }
}

#[test]
fn customize_search_wraps_backward_into_key_tables() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    for key in ["/", "m", "o", "u", "s", "e", "Enter", "n", "N", "N", "N"] {
        press(&mut engine, &mut context, pane, &mut mode, key);
        let mut expand = customize_expand;
        let _ = engine.customize_presentation(pane, &mode, &mut expand);
    }
    let row = current_row(&engine, pane, &mode);
    assert_eq!(
        (row.id.as_str(), row.name.as_str()),
        ("keys:root\0C-MouseDrag1Pane", "C-MouseDrag1Pane")
    );
}

#[test]
fn customize_menu_feed_answers_each_row() {
    assert_eq!(customize_menu_feed(false, 0), Some("Enter"));
    assert_eq!(customize_menu_feed(false, 1), Some("e"));
    assert_eq!(customize_menu_feed(false, 2), Some("Right"));
    assert_eq!(customize_menu_feed(false, 3), None);
    assert_eq!(customize_menu_feed(false, 4), Some("t"));
    assert_eq!(customize_menu_feed(false, 5), Some("\x14"));
    assert_eq!(customize_menu_feed(false, 6), Some("T"));
    assert_eq!(customize_menu_feed(false, 7), None);
    assert_eq!(customize_menu_feed(false, 8), Some("C"));
    assert_eq!(customize_menu_feed(false, 9), None);
    assert_eq!(customize_menu_feed(false, 10), Some("q"));
    assert_eq!(customize_menu_feed(false, 11), None);
    assert_eq!(customize_menu_feed(true, 0), Some("<"));
    assert_eq!(customize_menu_feed(true, 1), Some(">"));
    assert_eq!(customize_menu_feed(true, 2), None);
    assert_eq!(customize_menu_feed(true, 3), Some("q"));
    assert_eq!(customize_menu_feed(true, 4), None);
    let tag_all = CUSTOMIZE_MENU_ITEMS
        .iter()
        .flatten()
        .find(|item| item.name == "Tag All")
        .unwrap();
    assert_eq!(tag_all.key, "[DC4]");
    assert_eq!(tag_all.annotation, "[DC4]");
    assert_eq!(tag_all.feed, "\x14");
}

#[test]
fn customize_mouse_down3_selects_and_requests_the_line_menu() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    let result = engine.customize_mouse(pane, &mut mode, "MouseDown1Pane", 3, 2, &mut expand);
    assert!(!result.close);
    assert!(result.menu.is_none());
    let result = engine.customize_mouse(pane, &mut mode, "MouseDown3Pane", 3, 2, &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, 2);
    let menu = result.menu.expect("a menu request");
    assert!(!menu.outside);
    assert_eq!(menu.line, 2);
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let lines = customize_lines(&rows, &mode);
    assert_eq!(menu.name, rows[lines[2]].name);
    let result = engine.customize_mouse(pane, &mut mode, "WheelUpPane", 3, 2, &mut expand);
    assert!(!result.close);
    assert!(result.menu.is_none());
}

#[test]
fn customize_mouse_down3_outside_requests_the_outside_menu() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let size = customize_lines(&rows, &mode).len();
    let below = size.max(mode.height) + 5;
    let result = engine.customize_mouse(pane, &mut mode, "MouseDown3Pane", 3, below, &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, 0);
    let menu = result.menu.expect("a menu request");
    assert!(menu.outside);
    assert_eq!(menu.line, 0);
    let columns = engine.customize_screen_columns(pane);
    let result = engine.customize_mouse(
        pane,
        &mut mode,
        "MouseDown3Pane",
        columns + 1,
        1,
        &mut expand,
    );
    assert!(result.menu.is_some_and(|menu| menu.outside));
    let result = engine.customize_mouse(pane, &mut mode, "MouseDown1Pane", 3, below, &mut expand);
    assert!(result.menu.is_none());
}

#[test]
fn customize_mouse_down3_below_the_lines_keeps_the_current_line() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let size = customize_lines(&rows, &mode).len();
    mode.height = size + 4;
    mode.current = 1;
    let result = engine.customize_mouse(pane, &mut mode, "MouseDown3Pane", 3, size, &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, 1);
    let menu = result.menu.expect("a menu request");
    assert!(!menu.outside);
    assert_eq!(menu.line, 1);
}

#[test]
fn customize_menu_choice_runs_the_key_on_the_menu_line() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    mode.expanded.insert("options:0".to_owned());
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let lines = customize_lines(&rows, &mode);
    let option = 1;
    assert_eq!(rows[lines[option]].depth, 1);
    mode.current = 0;
    let result = engine.customize_menu_choice(pane, &mut mode, option, "t", &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, option);
    assert!(mode.tagged.contains(&rows[lines[option]].id));
    let tagged = mode.tagged.clone();
    let result = engine.customize_menu_choice(pane, &mut mode, option, "\x14", &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, option);
    assert_eq!(mode.tagged, tagged);
    let result = engine.customize_menu_choice(pane, &mut mode, option, "T", &mut expand);
    assert!(!result.close);
    assert!(mode.tagged.is_empty());
    let result = engine.customize_menu_choice(pane, &mut mode, lines.len() + 4, "t", &mut expand);
    assert!(!result.close);
    assert_eq!(mode.current, option);
    assert!(mode.tagged.is_empty());
    let closed = engine.customize_menu_choice(pane, &mut mode, option, "q", &mut expand);
    assert!(closed.close);
}
