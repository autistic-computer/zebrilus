use super::*;

#[test]
fn a_shift_click_past_the_line_number_gutter_lands_on_the_first_column() {
    use crate::terminal_event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use zz_protocol::{ModePresentation, PaneId};
    use zz_terminal::{SessionStatus, TerminalMode, TerminalViewAction, TerminalViewport};

    let core = zz_client::ClientCore::new();
    let endpoint =
        zz_daemon_client::Endpoint::parse("unix:///tmp/zz-input-test.sock").expect("test endpoint");
    let mut model = crate::state::Model::new(
        &core,
        crate::tty::TerminalSize {
            columns: 80,
            rows: 24,
            cell_width_px: 8,
            cell_height_px: 16,
        },
        "host".to_owned(),
        "host".to_owned(),
        endpoint.clone(),
        endpoint,
        Vec::new(),
    );
    let mut viewport = TerminalViewport::blank(80, 24, SessionStatus::Running);
    viewport.mode = TerminalMode::Copy {
        position: 0,
        total: 0,
        hide_position: false,
    };
    model.viewports.insert(PaneId(1), viewport);
    std::sync::Arc::make_mut(&mut model.status)
        .modes
        .push(ModePresentation {
            pane: PaneId(1),
            view: false,
            position: String::new(),
            position_style: String::new(),
            selection_style: String::new(),
            vi_keys: false,
            match_style: String::new(),
            current_match_style: String::new(),
            line_numbers: 1,
            line_number_style: String::new(),
            current_line_number_style: String::new(),
            current_line_style: String::new(),
        });
    let content = crate::layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 0,
        modifiers: KeyModifiers::SHIFT,
    };
    let Some(TerminalViewAction::Mouse(input)) =
        super::native_pane_mouse_action(&model, PaneId(1), content, event, (4, 0, 32, 0))
    else {
        panic!("a click in a copy-mode pane is a view action");
    };
    assert_eq!(input.cell.column, 0);
}

#[test]
fn browser_mouse_uses_content_relative_pixels_and_wheel_direction() {
    let input = browser_pointer_input(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::CONTROL | KeyModifiers::ALT,
        },
        Rect {
            x: 10,
            y: 4,
            width: 20,
            height: 10,
        },
        93,
        71,
        8,
        16,
    );

    assert_eq!(input.x, 13);
    assert_eq!(input.y, 7);
    assert_eq!(input.phase, ProviderPointerPhase::Wheel);
    assert_eq!((input.wheel_delta_x, input.wheel_delta_y), (0, -120));
    assert!(input.modifiers.control());
    assert!(input.modifiers.alt());
    assert!(!input.modifiers.shift());
    assert_eq!(input.button, None);
}

#[test]
fn a_pointer_event_names_the_pins_mouse_key_for_where_it_landed() {
    let event = |kind, modifiers| crate::terminal_event::MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers,
    };
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
            "Pane",
            false
        )
        .as_deref(),
        Some("MouseDown1Pane")
    );
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::Drag(MouseButton::Left), KeyModifiers::ALT),
            "Border",
            false
        )
        .as_deref(),
        Some("M-MouseDrag1Border")
    );
    assert_eq!(
        mouse_key_name(
            event(
                MouseEventKind::Up(MouseButton::Right),
                KeyModifiers::CONTROL
            ),
            "StatusLeft",
            false
        )
        .as_deref(),
        Some("C-MouseUp3StatusLeft")
    );
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::Up(MouseButton::Left), KeyModifiers::NONE),
            "Pane",
            true
        )
        .as_deref(),
        Some("MouseDragEnd1Pane")
    );
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::ScrollUp, KeyModifiers::NONE),
            "Status",
            false
        )
        .as_deref(),
        Some("WheelUpStatus")
    );
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
            "Pane",
            false
        )
        .as_deref(),
        Some("S-WheelDownPane")
    );
    assert_eq!(
        mouse_key_name(
            event(MouseEventKind::Moved, KeyModifiers::NONE),
            "Pane",
            false
        ),
        None
    );
}
