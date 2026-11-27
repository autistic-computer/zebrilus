use super::*;
use zz_protocol::{MenuItem, PopupBorderLines};

fn menu_state(selected: Option<u32>, stay_open: bool) -> MenuState {
    MenuState {
        left: 0,
        top: 0,
        width: 20,
        height: 6,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 8,
        cell_height_px: 16,
        title: "Menu".to_owned(),
        style: "default".to_owned(),
        selected_style: "reverse".to_owned(),
        border_style: "default".to_owned(),
        border_lines: PopupBorderLines::Single,
        items: vec![
            Some(MenuItem {
                name: "Quit item".to_owned(),
                key: Some("q".to_owned()),
                annotation: Some("q".to_owned()),
                enabled: true,
            }),
            None,
            Some(MenuItem {
                name: "Disabled".to_owned(),
                key: None,
                annotation: None,
                enabled: false,
            }),
            Some(MenuItem {
                name: "Last".to_owned(),
                key: None,
                annotation: None,
                enabled: true,
            }),
        ],
        selected,
        stay_open,
        mouse_keys: true,
    }
}

fn test_menu_box(state: &MenuState) -> zz_client::MenuBox {
    zz_client::MenuBox {
        left: 5,
        top: 5,
        width: state.width,
        items: state.items.len(),
    }
}

const TEST_POINTER_CELLS: PointerCells = PointerCells {
    pixels: false,
    cell_width_px: 8,
    cell_height_px: 16,
};

fn menu_route(state: &MenuState, selection: Option<usize>, event: TerminalEvent) -> MenuInputRoute {
    menu_input_route(
        Some(state),
        selection,
        false,
        Some(test_menu_box(state)),
        TEST_POINTER_CELLS,
        &mut None,
        event,
    )
}

#[test]
fn menu_shortcuts_win_and_navigation_changes_only_local_selection() {
    let state = menu_state(Some(0), false);
    assert_eq!(
        menu_route(
            &state,
            Some(0),
            TerminalEvent::Key(KeyEvent::new(
                TerminalKeyCode::Char('q'),
                KeyModifiers::NONE,
            )),
        ),
        MenuInputRoute::Action {
            action: MenuAction::Choose(0),
            swallowed_key: Some(KeyCode::Character('q')),
        }
    );
    assert_eq!(
        menu_route(
            &state,
            Some(0),
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::Down, KeyModifiers::NONE,)),
        ),
        MenuInputRoute::Select(Some(3))
    );
    assert_eq!(state.selected, Some(0));
}

#[test]
fn menu_cancel_enter_and_disabled_stay_open_use_the_shared_resolver() {
    let state = menu_state(Some(3), false);
    assert!(matches!(
        menu_route(
            &state,
            Some(3),
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::Esc, KeyModifiers::NONE))
        ),
        MenuInputRoute::Action {
            action: MenuAction::Cancel,
            ..
        }
    ));
    assert!(matches!(
        menu_route(
            &state,
            Some(3),
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE))
        ),
        MenuInputRoute::Action {
            action: MenuAction::Choose(3),
            ..
        }
    ));
    assert!(matches!(
        menu_route(
            &state,
            Some(2),
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE))
        ),
        MenuInputRoute::Action {
            action: MenuAction::Cancel,
            ..
        }
    ));
    let stay_open = menu_state(Some(2), true);
    assert_eq!(
        menu_route(
            &stay_open,
            Some(2),
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE,)),
        ),
        MenuInputRoute::Consume
    );
}

#[test]
fn menu_backtab_uses_real_parser_bytes() {
    let mut parser = crate::terminal_event::EventParser::default();
    let mut events = Vec::new();
    parser.push(b"\x1b[Z", &mut events);
    assert_eq!(events.len(), 1);
    let TerminalEvent::Key(event) = &events[0] else {
        panic!("backtab did not parse as a key");
    };
    let input = key_input(*event);
    assert_eq!(input.key, KeyCode::Tab);
    assert!(input.modifiers.shift());
    assert_eq!(
        menu_route(&menu_state(Some(3), false), Some(3), events.remove(0)),
        MenuInputRoute::Select(Some(0))
    );
}

#[test]
fn menu_pending_action_owns_input_until_close() {
    let state = menu_state(Some(0), false);
    let press = KeyEvent::new(TerminalKeyCode::Char('q'), KeyModifiers::NONE);
    assert!(matches!(
        menu_route(&state, Some(0), TerminalEvent::Key(press)),
        MenuInputRoute::Action {
            action: MenuAction::Choose(0),
            ..
        }
    ));
    assert_eq!(
        menu_input_route(
            Some(&state),
            Some(0),
            true,
            Some(test_menu_box(&state)),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Repeat,
                ..press
            }),
        ),
        MenuInputRoute::Consume
    );
    assert_eq!(
        menu_input_route(
            Some(&state),
            Some(0),
            true,
            Some(test_menu_box(&state)),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::Paste("q".to_owned()),
        ),
        MenuInputRoute::Consume
    );
    assert_eq!(
        menu_input_route(
            Some(&state),
            Some(0),
            true,
            Some(test_menu_box(&state)),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 1,
                modifiers: KeyModifiers::NONE,
            }),
        ),
        MenuInputRoute::Consume
    );
    assert_eq!(
        menu_input_route(
            Some(&state),
            Some(0),
            true,
            Some(test_menu_box(&state)),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::CellSize {
                width_px: 8,
                height_px: 16,
            },
        ),
        MenuInputRoute::Forward(TerminalEvent::CellSize {
            width_px: 8,
            height_px: 16,
        })
    );
}

#[test]
fn menu_paste_answers_every_character_and_hands_the_tail_back() {
    let state = menu_state(Some(0), false);
    assert_eq!(
        menu_route(&state, Some(0), TerminalEvent::Paste("Zq XY".to_owned())),
        MenuInputRoute::Paste {
            selection: Some(0),
            action: Some(MenuAction::Choose(0)),
            remainder: " XY".to_owned(),
        }
    );
    assert_eq!(
        menu_route(&state, Some(0), TerminalEvent::Paste("ZY".to_owned())),
        MenuInputRoute::Paste {
            selection: Some(0),
            action: None,
            remainder: String::new(),
        },
        "a run with no row key and no navigation key leaves the menu up"
    );
    assert_eq!(
        menu_route(&state, None, TerminalEvent::Paste("jZ".to_owned())),
        MenuInputRoute::Paste {
            selection: Some(0),
            action: None,
            remainder: String::new(),
        },
        "a navigation character moves the highlight the way a typed one does"
    );
}

#[test]
fn menu_pointer_reports_follow_the_pin_hit_box() {
    let state = menu_state(Some(0), false);
    let frame = test_menu_box(&state);
    let report = |kind, column, row| {
        menu_route(
            &state,
            Some(0),
            TerminalEvent::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
        )
    };
    let inside_row = frame.top + 1;
    assert_eq!(
        report(
            MouseEventKind::Down(MouseButton::Left),
            frame.left + 2,
            inside_row + 3
        ),
        MenuInputRoute::Select(Some(3)),
        "a press inside the box moves the highlight without choosing"
    );
    assert_eq!(
        report(MouseEventKind::Moved, frame.left + 2, inside_row + 3),
        MenuInputRoute::Select(Some(3)),
        "a motion with no button held is highlight-only"
    );
    assert_eq!(
        report(
            MouseEventKind::Up(MouseButton::Left),
            frame.left + 2,
            inside_row + 3
        ),
        MenuInputRoute::Action {
            action: MenuAction::Choose(0),
            swallowed_key: None,
        },
        "a release chooses the row the highlight already sits on"
    );
    assert_eq!(
        report(MouseEventKind::Up(MouseButton::Left), frame.left, frame.top),
        MenuInputRoute::Action {
            action: MenuAction::Cancel,
            swallowed_key: None,
        },
        "the title row sits outside the box, so a release there closes"
    );
    assert_eq!(
        report(MouseEventKind::Moved, 0, 0),
        MenuInputRoute::Select(None),
        "a motion outside the box only clears the highlight"
    );
    assert_eq!(
        report(MouseEventKind::Down(MouseButton::Left), 0, 0),
        MenuInputRoute::Select(None),
        "a press outside the box is not a release, so it does not close"
    );
    assert_eq!(
        report(
            MouseEventKind::Up(MouseButton::Left),
            frame.left + frame.width,
            inside_row,
        ),
        MenuInputRoute::Action {
            action: MenuAction::Choose(0),
            swallowed_key: None,
        },
        "px + 4 + width is the pin's inclusive right limit"
    );

    let stay_open = menu_state(Some(0), true);
    let stay_frame = test_menu_box(&stay_open);
    let stay_report = |kind, column, row| {
        menu_input_route(
            Some(&stay_open),
            Some(0),
            false,
            Some(stay_frame),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
        )
    };
    assert_eq!(
        stay_report(MouseEventKind::Up(MouseButton::Left), 0, 0),
        MenuInputRoute::Select(None),
        "a stay-open menu ignores a release outside the box"
    );
    assert_eq!(
        stay_report(MouseEventKind::Down(MouseButton::Left), 0, 0),
        MenuInputRoute::Action {
            action: MenuAction::Cancel,
            swallowed_key: None,
        },
        "a stay-open menu closes on a report that is neither release, wheel nor drag"
    );
}

#[test]
fn a_nomouse_menu_answers_only_button_one() {
    let mut state = menu_state(Some(0), false);
    state.mouse_keys = false;
    let frame = test_menu_box(&state);
    let report = |kind, column, row| {
        menu_input_route(
            Some(&state),
            Some(0),
            false,
            Some(frame),
            TEST_POINTER_CELLS,
            &mut None,
            TerminalEvent::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
        )
    };
    let inside_row = frame.top + 1;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
    ] {
        assert_eq!(
            report(kind, frame.left + 2, inside_row),
            MenuInputRoute::Consume,
            "button 1 is ignored whole"
        );
    }
    for kind in [
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::ScrollUp,
    ] {
        assert_eq!(
            report(kind, frame.left + 2, inside_row),
            MenuInputRoute::Action {
                action: MenuAction::Cancel,
                swallowed_key: None,
            },
            "any other button leaves the menu"
        );
    }
}

#[test]
fn menu_release_and_action_key_lifecycle() {
    let state = menu_state(Some(0), false);
    let press = KeyEvent::new(TerminalKeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(
        menu_route(
            &state,
            Some(0),
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..press
            }),
        ),
        MenuInputRoute::Consume
    );
    let MenuInputRoute::Action {
        swallowed_key: Some(key),
        ..
    } = menu_route(&state, Some(0), TerminalEvent::Key(press))
    else {
        panic!("menu shortcut was not consumed");
    };
    let mut swallowed = Some(key);
    assert_eq!(
        menu_input_route(
            None,
            Some(0),
            false,
            None,
            TEST_POINTER_CELLS,
            &mut swallowed,
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Repeat,
                ..press
            }),
        ),
        MenuInputRoute::Consume
    );
    assert_eq!(
        menu_input_route(
            None,
            Some(0),
            false,
            None,
            TEST_POINTER_CELLS,
            &mut swallowed,
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..press
            }),
        ),
        MenuInputRoute::Consume
    );
    assert_eq!(swallowed, None);
    assert_eq!(
        menu_input_route(
            None,
            Some(0),
            false,
            None,
            TEST_POINTER_CELLS,
            &mut swallowed,
            TerminalEvent::Key(press),
        ),
        MenuInputRoute::Forward(TerminalEvent::Key(press))
    );
}
