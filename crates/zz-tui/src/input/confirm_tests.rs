use super::*;

fn confirm_state(confirm_key: u8, default_yes: bool) -> ConfirmState {
    ConfirmState {
        prompt: "Confirm? ".to_owned(),
        confirm_key,
        default_yes,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    }
}

fn confirm_route(state: &ConfirmState, event: TerminalEvent) -> ConfirmInputRoute {
    confirm_input_route(Some(state), false, &mut None, event)
}

fn assert_confirm_reply(state: &ConfirmState, event: KeyEvent, accepted: bool) {
    assert!(matches!(
        confirm_route(state, TerminalEvent::Key(event)),
        ConfirmInputRoute::Reply {
            accepted: actual,
            swallowed_key: Some(_),
        } if actual == accepted
    ));
}

#[test]
fn confirm_keys_follow_the_published_case_and_enter_rules() {
    let lower = confirm_state(b'y', false);
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::NONE),
        true,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('n'), KeyModifiers::NONE),
        false,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('Y'), KeyModifiers::SHIFT),
        false,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::CONTROL),
        false,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::ALT),
        true,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::SUPER),
        true,
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
        false,
    );

    let upper = confirm_state(b'Y', false);
    assert_confirm_reply(
        &upper,
        KeyEvent::new(TerminalKeyCode::Char('Y'), KeyModifiers::SHIFT),
        true,
    );
    assert_confirm_reply(
        &upper,
        KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::NONE),
        false,
    );

    let enter = confirm_state(b'y', true);
    assert_confirm_reply(
        &enter,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
        true,
    );
    assert_confirm_reply(
        &enter,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::SHIFT),
        true,
    );
    assert_confirm_reply(
        &enter,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::ALT),
        true,
    );
    assert_confirm_reply(
        &enter,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::CONTROL),
        true,
    );
    assert_confirm_reply(
        &enter,
        KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::SUPER),
        true,
    );
    assert_eq!(
        confirm_route(
            &lower,
            TerminalEvent::Key(KeyEvent::new(TerminalKeyCode::F(2), KeyModifiers::NONE))
        ),
        ConfirmInputRoute::Consume
    );
    assert_confirm_reply(
        &lower,
        KeyEvent::new(TerminalKeyCode::Esc, KeyModifiers::NONE),
        false,
    );
}

#[test]
fn confirm_routes_extended_modifier_bit_eight_like_tmux_meta() {
    let mut parser = crate::terminal_event::EventParser::default();
    let mut events = Vec::new();
    parser.push(b"\x1b[121;9u\x1b[13;9u", &mut events);
    assert_eq!(events.len(), 2);

    let lower = confirm_state(b'y', false);
    assert!(matches!(
        confirm_route(&lower, events[0].clone()),
        ConfirmInputRoute::Reply { accepted: true, .. }
    ));

    let enter = confirm_state(b'y', true);
    assert!(matches!(
        confirm_route(&enter, events[1].clone()),
        ConfirmInputRoute::Reply { accepted: true, .. }
    ));
}

#[test]
fn confirm_consumes_paste_pointer_and_the_reply_key_lifecycle() {
    let lower = confirm_state(b'y', false);
    assert_eq!(
        confirm_route(&lower, TerminalEvent::Paste("y trailing".to_owned())),
        ConfirmInputRoute::Consume
    );
    assert_eq!(
        confirm_route(
            &lower,
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 4,
                row: 3,
                modifiers: KeyModifiers::NONE,
            })
        ),
        ConfirmInputRoute::Consume
    );
    let press = KeyEvent::new(TerminalKeyCode::Char('y'), KeyModifiers::NONE);
    let ConfirmInputRoute::Reply {
        swallowed_key: Some(key),
        ..
    } = confirm_route(&lower, TerminalEvent::Key(press))
    else {
        panic!("confirm key was not consumed");
    };
    let mut swallowed = Some(key);
    assert_eq!(
        confirm_input_route(
            None,
            false,
            &mut swallowed,
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Repeat,
                ..press
            })
        ),
        ConfirmInputRoute::Consume
    );
    assert_eq!(
        confirm_input_route(
            None,
            false,
            &mut swallowed,
            TerminalEvent::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..press
            })
        ),
        ConfirmInputRoute::Consume
    );
    assert_eq!(swallowed, None);
    assert_eq!(
        confirm_input_route(None, false, &mut swallowed, TerminalEvent::Key(press)),
        ConfirmInputRoute::Forward(TerminalEvent::Key(press))
    );
    assert_eq!(
        confirm_input_route(
            Some(&lower),
            true,
            &mut None,
            TerminalEvent::Key(KeyEvent::new(
                TerminalKeyCode::Char('n'),
                KeyModifiers::NONE,
            ))
        ),
        ConfirmInputRoute::Consume
    );
    assert_eq!(
        confirm_route(
            &lower,
            TerminalEvent::CellSize {
                width_px: 8,
                height_px: 16,
            }
        ),
        ConfirmInputRoute::Forward(TerminalEvent::CellSize {
            width_px: 8,
            height_px: 16,
        })
    );
}
