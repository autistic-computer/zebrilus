use super::*;

#[test]
fn command_output_routes_press_and_repeat_keys_but_not_releases() {
    let pane = zz_protocol::PaneId(9);
    for (event, expected_action, expected_key) in [
        (
            KeyEvent::new(TerminalKeyCode::Char('q'), KeyModifiers::NONE),
            KeyAction::Press,
            KeyCode::Character('q'),
        ),
        (
            KeyEvent {
                code: TerminalKeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Repeat,
            },
            KeyAction::Repeat,
            KeyCode::Escape,
        ),
    ] {
        let Some(InputMessage::Key {
            pane: target,
            input,
            text_follows,
        }) = command_output_key_input(pane, event)
        else {
            panic!("command output key was not routed through the daemon");
        };
        assert_eq!(target, pane);
        assert_eq!(input.action, expected_action);
        assert_eq!(input.key, expected_key);
        assert!(!text_follows);
    }
    assert!(
        command_output_key_input(
            pane,
            KeyEvent {
                code: TerminalKeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Release,
            },
        )
        .is_none()
    );
}

fn assert_search_exit_swallow(terminal_code: TerminalKeyCode, key: KeyCode, closes_search: bool) {
    let pane = zz_protocol::PaneId(9);
    let mut search = Some(SearchQuery::default());
    let mut swallowed = None;

    let press = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent::new(terminal_code, KeyModifiers::NONE),
    );
    let CommandOutputKeyRoute::Search(edit) = press else {
        panic!("search exit press escaped the local prompt");
    };
    assert_eq!(
        edit.action,
        closes_search.then_some(TerminalViewAction::SearchClose)
    );
    assert!(edit.repaint);
    assert!(search.is_none());
    assert_eq!(swallowed, Some(key));

    let repeat = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent {
            code: terminal_code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
        },
    );
    assert!(matches!(repeat, CommandOutputKeyRoute::Swallowed));
    assert_eq!(swallowed, Some(key));

    let release = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent {
            code: terminal_code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
        },
    );
    assert!(matches!(release, CommandOutputKeyRoute::Swallowed));
    assert_eq!(swallowed, None);

    let fresh = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent::new(terminal_code, KeyModifiers::NONE),
    );
    assert!(matches!(
        fresh,
        CommandOutputKeyRoute::Forward(Some(InputMessage::Key {
            pane: target,
            input,
            text_follows: false,
        })) if target == pane && input.key == key && input.action == KeyAction::Press
    ));
}

#[test]
fn command_output_search_enter_swallows_its_repeat_and_release() {
    assert_search_exit_swallow(TerminalKeyCode::Enter, KeyCode::Enter, false);
}

#[test]
fn command_output_search_escape_swallows_its_repeat_and_release() {
    assert_search_exit_swallow(TerminalKeyCode::Esc, KeyCode::Escape, true);
}

#[test]
fn command_output_search_exit_does_not_swallow_unrelated_keys() {
    let pane = zz_protocol::PaneId(9);
    let mut search = Some(SearchQuery::default());
    let mut swallowed = None;
    let _ = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
    );

    let unrelated = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent::new(TerminalKeyCode::Char('q'), KeyModifiers::NONE),
    );
    assert!(matches!(
        unrelated,
        CommandOutputKeyRoute::Forward(Some(InputMessage::Key { input, .. }))
            if input.key == KeyCode::Character('q')
    ));
    assert_eq!(swallowed, Some(KeyCode::Enter));

    let fresh_same_key = command_output_key_route(
        &mut search,
        &mut swallowed,
        pane,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(matches!(
        fresh_same_key,
        CommandOutputKeyRoute::Forward(Some(InputMessage::Key { input, .. }))
            if input.key == KeyCode::Enter && input.action == KeyAction::Press
    ));
    assert_eq!(swallowed, None);
}

#[test]
fn command_output_search_edits_accepts_and_closes_locally() {
    let mut search = Some(SearchQuery::default());
    let appended = command_output_search_key(
        &mut search,
        KeyEvent::new(TerminalKeyCode::Char('é'), KeyModifiers::NONE),
    );
    assert_eq!(search.as_ref().unwrap().text, "é");
    assert!(matches!(
        appended.action,
        Some(TerminalViewAction::SearchUpdate(ref query)) if query.text == "é"
    ));

    let erased = command_output_search_key(
        &mut search,
        KeyEvent::new(TerminalKeyCode::Backspace, KeyModifiers::NONE),
    );
    assert_eq!(search.as_ref().unwrap().text, "");
    assert!(matches!(
        erased.action,
        Some(TerminalViewAction::SearchUpdate(ref query)) if query.text.is_empty()
    ));

    let accepted = command_output_search_key(
        &mut search,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
    );
    assert_eq!(accepted.action, None);
    assert!(accepted.repaint);
    assert!(search.is_none());

    search = Some(SearchQuery::default());
    let closed = command_output_search_key(
        &mut search,
        KeyEvent::new(TerminalKeyCode::Esc, KeyModifiers::NONE),
    );
    assert_eq!(closed.action, Some(TerminalViewAction::SearchClose));
    assert!(search.is_none());
}

#[test]
fn command_output_search_releases_and_modified_text_are_inert() {
    let mut search = Some(SearchQuery::default());
    let released = command_output_search_key(
        &mut search,
        KeyEvent {
            code: TerminalKeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
        },
    );
    assert_eq!(released, CommandOutputSearchEdit::default());
    assert_eq!(search.as_ref().unwrap().text, "");

    let modified = command_output_search_key(
        &mut search,
        KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::CONTROL),
    );
    assert_eq!(modified, CommandOutputSearchEdit::default());
    assert_eq!(search.as_ref().unwrap().text, "");
}

#[test]
fn command_output_search_paste_updates_the_query() {
    let mut search = Some(SearchQuery::literal("start"));
    let action = command_output_search_paste(&mut search, " + pasted");
    assert_eq!(search.as_ref().unwrap().text, "start + pasted");
    assert!(matches!(
        action,
        Some(TerminalViewAction::SearchUpdate(ref query))
            if query.text == "start + pasted"
    ));

    let mut inactive = None;
    assert_eq!(command_output_search_paste(&mut inactive, "ignored"), None);
}

#[test]
fn command_output_search_drops_control_characters_from_paste() {
    let mut search = Some(SearchQuery::default());
    let action = command_output_search_paste(&mut search, "a\n\t\u{1b}\u{7f}界");
    assert_eq!(search.as_ref().unwrap().text, "a界");
    assert!(matches!(
        action,
        Some(TerminalViewAction::SearchUpdate(ref query)) if query.text == "a界"
    ));

    assert_eq!(command_output_search_paste(&mut search, "\n\t\u{1b}"), None);
    assert_eq!(search.as_ref().unwrap().text, "a界");
}

#[test]
fn command_output_search_respects_the_utf8_byte_limit() {
    let mut paste = Some(SearchQuery::literal("a".repeat(4094)));
    let action = command_output_search_paste(&mut paste, "éx");
    assert_eq!(
        paste.as_ref().unwrap().text.len(),
        MAX_COMMAND_OUTPUT_SEARCH_BYTES
    );
    assert!(paste.as_ref().unwrap().text.ends_with('é'));
    assert!(matches!(
        action,
        Some(TerminalViewAction::SearchUpdate(ref query))
            if query.text.len() == MAX_COMMAND_OUTPUT_SEARCH_BYTES
    ));

    let mut typed = Some(SearchQuery::literal("a".repeat(4095)));
    let rejected = command_output_search_key(
        &mut typed,
        KeyEvent::new(TerminalKeyCode::Char('é'), KeyModifiers::NONE),
    );
    assert_eq!(rejected, CommandOutputSearchEdit::default());
    assert_eq!(typed.as_ref().unwrap().text.len(), 4095);

    let accepted = command_output_search_key(
        &mut typed,
        KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::NONE),
    );
    assert!(accepted.repaint);
    assert_eq!(
        typed.as_ref().unwrap().text.len(),
        MAX_COMMAND_OUTPUT_SEARCH_BYTES
    );

    let full = command_output_search_key(
        &mut typed,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::NONE),
    );
    assert_eq!(full, CommandOutputSearchEdit::default());
    assert_eq!(
        typed.as_ref().unwrap().text.len(),
        MAX_COMMAND_OUTPUT_SEARCH_BYTES
    );
}

#[test]
fn command_output_mouse_owns_every_outer_route() {
    for mouse_option in [false, true] {
        for sidebar_hit in [false, true] {
            assert_eq!(
                mouse_route_owner(true, mouse_option, sidebar_hit),
                MouseRouteOwner::CommandOutput
            );
        }
    }
    assert_eq!(
        mouse_route_owner(false, false, true),
        MouseRouteOwner::Application
    );
    assert_eq!(
        mouse_route_owner(false, true, true),
        MouseRouteOwner::Sidebar
    );
    assert_eq!(
        mouse_route_owner(false, true, false),
        MouseRouteOwner::Workspace
    );
}
