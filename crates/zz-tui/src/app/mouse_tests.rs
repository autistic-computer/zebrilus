use super::test_support::paned_model;
use super::*;
use zz_protocol::{MenuItem, MenuState, PopupBorderLines};

fn tracking_viewport(tracking: bool) -> zz_terminal::TerminalViewport {
    let mut viewport =
        zz_terminal::TerminalViewport::blank(79, 22, zz_terminal::SessionStatus::Running);
    viewport.mouse_tracking = tracking;
    viewport
}

fn menu_state(mouse_keys: bool) -> MenuState {
    MenuState {
        left: 4,
        top: 3,
        width: 20,
        height: 3,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 8,
        cell_height_px: 16,
        title: "Menu".to_owned(),
        style: "default".to_owned(),
        selected_style: "reverse".to_owned(),
        border_style: "default".to_owned(),
        border_lines: PopupBorderLines::Single,
        items: vec![Some(MenuItem {
            name: "First".to_owned(),
            key: Some("f".to_owned()),
            annotation: Some("f".to_owned()),
            enabled: true,
        })],
        selected: None,
        stay_open: false,
        mouse_keys,
    }
}

#[test]
fn app_requested_mouse_lights_the_outer_modes_while_the_option_is_off() {
    let (mut model, pane) = paned_model();
    model.mouse_option = false;
    model.mouse_arming = MouseArming::Off;

    assert!(sync_mouse_modes(&mut model, false).is_none());

    model.viewports.insert(pane, tracking_viewport(true));
    assert_eq!(
        sync_mouse_modes(&mut model, false).as_deref(),
        Some(crate::tty::mouse_mode_sequence(MouseArming::Any, false).as_slice())
    );
    assert_eq!(model.mouse_arming, MouseArming::Any);
    assert!(sync_mouse_modes(&mut model, false).is_none());

    model.viewports.insert(pane, tracking_viewport(false));
    assert_eq!(
        sync_mouse_modes(&mut model, false).as_deref(),
        Some(crate::tty::mouse_mode_sequence(MouseArming::Off, false).as_slice())
    );
    assert_eq!(model.mouse_arming, MouseArming::Off);

    model.mouse_option = true;
    assert_eq!(
        sync_mouse_modes(&mut model, true).as_deref(),
        Some(crate::tty::mouse_mode_sequence(MouseArming::Button, true).as_slice())
    );
}

#[test]
fn a_nomouse_menu_leaves_the_arming_to_the_mouse_option() {
    let (mut model, pane) = paned_model();
    model.viewports.insert(pane, tracking_viewport(false));

    model.mouse_option = true;
    model.menu = Some(menu_state(false));
    assert_eq!(desired_mouse_arming(&model), MouseArming::Button);
    model.menu = Some(menu_state(true));
    assert_eq!(desired_mouse_arming(&model), MouseArming::Any);

    model.mouse_option = false;
    model.menu = Some(menu_state(false));
    assert_eq!(desired_mouse_arming(&model), MouseArming::Off);
    model.menu = Some(menu_state(true));
    assert_eq!(desired_mouse_arming(&model), MouseArming::Any);

    model.mouse_option = true;
    model.focus_follows_mouse = true;
    model.menu = Some(menu_state(false));
    assert_eq!(desired_mouse_arming(&model), MouseArming::Any);
}

#[test]
fn mouse_off_forwards_only_to_a_tracking_pane_and_skips_chrome() {
    let (mut model, pane) = paned_model();
    model.mouse_option = false;
    let event = crate::terminal_event::MouseEvent {
        kind: crate::terminal_event::MouseEventKind::Down(crate::terminal_event::MouseButton::Left),
        column: 5,
        row: 2,
        modifiers: crate::terminal_event::KeyModifiers::NONE,
    };

    assert!(
        crate::input::app_mouse_forward_action(&model, event, 5, 2, 40, 32).is_none(),
        "no viewport yet: nothing forwards"
    );

    model.viewports.insert(pane, tracking_viewport(false));
    assert!(
        crate::input::app_mouse_forward_action(&model, event, 5, 2, 40, 32).is_none(),
        "a pane that did not request mouse receives nothing"
    );

    model.viewports.insert(pane, tracking_viewport(true));
    let (target, action) = crate::input::app_mouse_forward_action(&model, event, 5, 2, 40, 32)
        .expect("tracking pane receives the event");
    assert_eq!(target, pane);
    assert!(matches!(
        action,
        zz_terminal::TerminalViewAction::Mouse(input)
            if !input.force_selection()
    ));

    assert!(
        crate::input::app_mouse_forward_action(&model, event, 5, 0, 40, 0).is_some(),
        "pane-border-status off reserves no row, so the pane's first row is content"
    );

    let mut snapshot = (*model.snapshot).clone();
    snapshot.sessions[0].windows[0].pane_border_status = zz_protocol::PaneBorderStatus::Top;
    model.update_snapshot(Arc::new(snapshot));
    assert!(
        crate::input::app_mouse_forward_action(&model, event, 5, 0, 40, 0).is_none(),
        "pane-border-status top spends the pane's first row on the status row"
    );
    assert!(
        crate::input::app_mouse_forward_action(&model, event, 5, 1, 40, 16).is_some(),
        "the row below it is content again"
    );
}

#[test]
fn mouse_gate_follows_the_effective_option_value() {
    let mut options = zz_protocol::MuxOptions::default();
    assert!(mouse_option_enabled(&options));
    options.set(
        zz_protocol::MuxOptionKey::Mouse,
        "off",
        zz_protocol::MuxOptionSource::RuntimeCommand,
    );
    assert!(!mouse_option_enabled(&options));
}

#[test]
fn only_the_root_tables_mouse_names_are_collected() {
    let binding = |key: &str| zz_protocol::KeyBindingSnapshot {
        key: key.to_owned(),
        commands: Vec::new(),
        repeat: false,
        note: None,
    };
    let tables = vec![
        zz_protocol::KeyTableSnapshot {
            name: "root".to_owned(),
            bindings: vec![
                binding("MouseDown1Pane"),
                binding("M-MouseDrag1Border"),
                binding("WheelUpStatus"),
                binding("C-b"),
                binding("F9"),
            ],
        },
        zz_protocol::KeyTableSnapshot {
            name: "copy-mode".to_owned(),
            bindings: vec![binding("MouseDrag1Pane")],
        },
    ];
    let names = mouse_binding_names(&tables);
    assert!(names.contains("MouseDown1Pane"));
    assert!(names.contains("M-MouseDrag1Border"));
    assert!(names.contains("WheelUpStatus"));
    assert!(!names.contains("C-b"));
    assert!(!names.contains("F9"));
    assert!(!names.contains("MouseDrag1Pane"));
}
