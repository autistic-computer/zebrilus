use std::cell::{Cell, RefCell};

use super::test_support::*;
use super::*;
use ala::{Modifiers, TestAppContext};
use zz_client::CoreEvent;
use zz_protocol::CommandPromptKind;

fn menu_state_for_test() -> MenuState {
    MenuState {
        left: 29,
        top: 6,
        width: 20,
        height: 4,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 8,
        cell_height_px: 18,
        title: "menu".to_owned(),
        style: "bg=themedarkgrey,fg=themewhite".to_owned(),
        selected_style: "bg=themeyellow,fg=themeblack".to_owned(),
        border_style: "bg=themedarkgrey,fg=themelightgrey".to_owned(),
        border_lines: PopupBorderLines::Single,
        items: vec![Some(zz_protocol::MenuItem {
            name: "Quit item".to_owned(),
            key: Some("q".to_owned()),
            annotation: Some("q".to_owned()),
            enabled: true,
        })],
        selected: Some(0),
        stay_open: false,
        mouse_keys: false,
    }
}

fn overlay_selector(event: &CoreEvent) -> Option<&'static str> {
    match event {
        CoreEvent::CommandPromptChanged => Some("command-palette-overlay"),
        CoreEvent::ChooseTreeChanged => Some("choose-tree-overlay"),
        CoreEvent::ChooseBufferChanged => Some("choose-buffer-overlay"),
        CoreEvent::DisplayPanesChanged => Some("display-panes-input"),
        CoreEvent::MenuChanged => Some("display-menu"),
        CoreEvent::ConfirmChanged => Some("confirm-before"),
        CoreEvent::HelloReceived
        | CoreEvent::Attached { .. }
        | CoreEvent::SnapshotChanged
        | CoreEvent::ViewportChanged { .. }
        | CoreEvent::AppearanceChanged
        | CoreEvent::MuxOptionsChanged
        | CoreEvent::KeyTablesChanged
        | CoreEvent::StatusChanged
        | CoreEvent::PrefixArmed { .. }
        | CoreEvent::PrefixCancelled { .. }
        | CoreEvent::KeyTableChanged
        | CoreEvent::CommandOutputChanged
        | CoreEvent::PaneRemoved { .. }
        | CoreEvent::Bell { .. }
        | CoreEvent::FocusSidebar
        | CoreEvent::OpenPathPicker { .. }
        | CoreEvent::Detached { .. }
        | CoreEvent::ServerStopping
        | CoreEvent::CommandResponse(_)
        | CoreEvent::ClientMessage { .. }
        | CoreEvent::ClientMessageCleared { .. }
        | CoreEvent::Clipboard { .. }
        | CoreEvent::OpenUri { .. }
        | CoreEvent::BrowserCommand { .. }
        | CoreEvent::TerminalUiCommand { .. }
        | CoreEvent::HistoryChunk { .. }
        | CoreEvent::KittyImageBegin { .. }
        | CoreEvent::KittyImageChunk { .. }
        | CoreEvent::KittyImagesRemoved { .. }
        | CoreEvent::AgentUpdates { .. }
        | CoreEvent::AgentStateChanged { .. }
        | CoreEvent::AgentLagged { .. }
        | CoreEvent::AgentReplayed { .. }
        | CoreEvent::AgentSessions { .. }
        | CoreEvent::ProvidersChanged
        | CoreEvent::AgentAccountsChanged
        | CoreEvent::AgentLoginChanged { .. }
        | CoreEvent::SimulatorChanged { .. }
        | CoreEvent::SimulatorCommand { .. }
        | CoreEvent::Message(_) => None,
    }
}

fn command_prompt_state_for_test() -> zz_protocol::CommandPromptState {
    zz_protocol::CommandPromptState {
        prompt: ":".to_owned(),
        input: String::new(),
        cursor: 0,
        kind: CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    }
}

fn choose_buffer_state_for_test() -> zz_protocol::ChooseBufferState {
    let item = |name: &str| zz_protocol::ChooseBufferItem {
        name: name.to_owned(),
        preview: "preview".to_owned(),
        size_bytes: 7,
        created_unix_seconds: 0,
        key: String::new(),
        text: String::new(),
        tagged: false,
    };
    zz_protocol::ChooseBufferState {
        items: vec![item("buffer0"), item("buffer1")],
        search: None,
        selected: 0,
        filter_no_matches: false,
        help: false,
        prompt: String::new(),
    }
}

#[ala::test]
fn every_daemon_overlay_payload_reaches_a_desktop_consumer(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "overlay matrix".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    let sequence = Cell::new(0_u64);

    let publish = |payload: zz_protocol::EventPayload, cx: &mut ala::VisualTestContext| {
        sequence.set(sequence.get() + 1);
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: sequence.get(),
                    payload,
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };

    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();

    struct OverlayCase {
        event: CoreEvent,
        raise: zz_protocol::EventPayload,
        retire: zz_protocol::EventPayload,
        keystroke: &'static str,
        reaches_daemon: fn(&InputMessage) -> bool,
    }

    let cases = vec![
        OverlayCase {
            event: CoreEvent::CommandPromptChanged,
            raise: zz_protocol::EventPayload::CommandPrompt {
                state: Some(command_prompt_state_for_test()),
            },
            retire: zz_protocol::EventPayload::CommandPrompt { state: None },
            keystroke: "escape",
            reaches_daemon: |message| matches!(message, InputMessage::CommandPrompt { .. }),
        },
        OverlayCase {
            event: CoreEvent::ChooseTreeChanged,
            raise: zz_protocol::EventPayload::ChooseTree {
                state: Some(choose_tree_state_for_test()),
            },
            retire: zz_protocol::EventPayload::ChooseTree { state: None },
            keystroke: "down",
            reaches_daemon: |message| matches!(message, InputMessage::ChooseTree { .. }),
        },
        OverlayCase {
            event: CoreEvent::ChooseBufferChanged,
            raise: zz_protocol::EventPayload::ChooseBuffer {
                state: Some(choose_buffer_state_for_test()),
            },
            retire: zz_protocol::EventPayload::ChooseBuffer { state: None },
            keystroke: "down",
            reaches_daemon: |message| matches!(message, InputMessage::ChooseBuffer { .. }),
        },
        OverlayCase {
            event: CoreEvent::DisplayPanesChanged,
            raise: zz_protocol::EventPayload::DisplayPanes {
                state: Some(display_panes_state_for_test()),
            },
            retire: zz_protocol::EventPayload::DisplayPanes { state: None },
            keystroke: "escape",
            reaches_daemon: |message| matches!(message, InputMessage::DisplayPanes { .. }),
        },
        OverlayCase {
            event: CoreEvent::MenuChanged,
            raise: zz_protocol::EventPayload::Menu {
                state: Some(menu_state_for_test()),
            },
            retire: zz_protocol::EventPayload::Menu { state: None },
            keystroke: "q",
            reaches_daemon: |message| matches!(message, InputMessage::Menu { .. }),
        },
        OverlayCase {
            event: CoreEvent::ConfirmChanged,
            raise: zz_protocol::EventPayload::Confirm {
                state: Some(zz_protocol::ConfirmState {
                    prompt: "Confirm? (y/n) ".to_owned(),
                    confirm_key: b'y',
                    default_yes: false,
                    prompt_cursor: zz_protocol::PromptCursor::default(),
                }),
            },
            retire: zz_protocol::EventPayload::Confirm { state: None },
            keystroke: "y",
            reaches_daemon: |message| matches!(message, InputMessage::Confirm { .. }),
        },
    ];

    for case in cases {
        let selector = overlay_selector(&case.event)
            .unwrap_or_else(|| panic!("{:?} claims no overlay element", case.event));
        publish(case.raise, cx);
        assert!(
            cx.debug_bounds(selector).is_some(),
            "{selector} published its state and drew nothing"
        );
        let focused = workspace.read_with(cx, |workspace, cx| {
            workspace
                .visible_overlay(cx)
                .map(|(_, focus)| focus)
                .expect("a published overlay is the visible overlay")
        });
        assert!(
            cx.update(|window, cx| focused.contains_focused(window, cx)),
            "{selector} drew without taking the keyboard"
        );
        input.borrow_mut().clear();
        cx.simulate_keystrokes(case.keystroke);
        assert!(
            input.borrow().iter().any(case.reaches_daemon),
            "{selector} swallowed {} instead of answering the daemon: {:?}",
            case.keystroke,
            input.borrow()
        );
        publish(case.retire, cx);
        assert!(
            cx.debug_bounds(selector).is_none(),
            "{selector} outlived the state that raised it"
        );
    }

    publish(
        zz_protocol::EventPayload::ChooseTree {
            state: Some(choose_tree_state_for_test()),
        },
        cx,
    );
    let tree_revision = mux.read_with(cx, |mux, _| mux.choose_tree_revision());
    publish(
        zz_protocol::EventPayload::ChooseTreeUpdate {
            search: Some(zz_protocol::ChooseTreeSearchState {
                query: "win".to_owned(),
                reverse: false,
            }),
            selected: 1,
        },
        cx,
    );
    assert!(cx.debug_bounds("choose-tree-overlay").is_some());
    mux.read_with(cx, |mux, _| {
        let state = mux.choose_tree().expect("the update kept the chooser open");
        assert_eq!(state.selected, 1, "ChooseTreeUpdate moved no selection");
        assert_eq!(
            state.search.as_ref().map(|search| search.query.as_str()),
            Some("win"),
            "ChooseTreeUpdate landed no search"
        );
        assert_ne!(
            mux.choose_tree_revision(),
            tree_revision,
            "ChooseTreeUpdate bumped no revision, so no consumer redraws"
        );
    });
    publish(zz_protocol::EventPayload::ChooseTree { state: None }, cx);

    publish(
        zz_protocol::EventPayload::ChooseBuffer {
            state: Some(choose_buffer_state_for_test()),
        },
        cx,
    );
    let buffer_revision = mux.read_with(cx, |mux, _| mux.choose_buffer_revision());
    publish(
        zz_protocol::EventPayload::ChooseBufferUpdate {
            search: Some(zz_protocol::ChooseBufferSearchState {
                query: "buf".to_owned(),
                reverse: false,
            }),
            selected: 1,
        },
        cx,
    );
    assert!(cx.debug_bounds("choose-buffer-overlay").is_some());
    mux.read_with(cx, |mux, _| {
        let state = mux
            .choose_buffer()
            .expect("the update kept the chooser open");
        assert_eq!(state.selected, 1, "ChooseBufferUpdate moved no selection");
        assert_eq!(
            state.search.as_ref().map(|search| search.query.as_str()),
            Some("buf"),
            "ChooseBufferUpdate landed no search"
        );
        assert_ne!(
            mux.choose_buffer_revision(),
            buffer_revision,
            "ChooseBufferUpdate bumped no revision, so no consumer redraws"
        );
    });
    publish(zz_protocol::EventPayload::ChooseBuffer { state: None }, cx);
    assert!(workspace.read_with(cx, |workspace, cx| workspace.visible_overlay(cx).is_none()));
}

#[ala::test]
fn a_nomouse_menu_leaves_on_any_button_but_the_first(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "menu mouse".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    let sequence = Cell::new(0_u64);
    let raise = |mouse_keys: bool, cx: &mut ala::VisualTestContext| {
        let mut state = menu_state_for_test();
        state.mouse_keys = mouse_keys;
        sequence.set(sequence.get() + 1);
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: sequence.get(),
                    payload: zz_protocol::EventPayload::Menu { state: Some(state) },
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };

    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();

    raise(false, cx);
    let row = cx
        .debug_bounds("display-menu-row-0")
        .expect("the menu drew its first row");
    let inside = row.center();

    input.borrow_mut().clear();
    cx.simulate_mouse_down(inside, MouseButton::Right, Modifiers::default());
    assert!(
        input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Cancel
            }
        )),
        "a NOMOUSE menu must leave on button 3: {:?}",
        input.borrow()
    );

    input.borrow_mut().clear();
    cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
    assert!(
        input.borrow().is_empty(),
        "a NOMOUSE menu swallows the button 1 press: {:?}",
        input.borrow()
    );
    cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
    assert!(
        input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Cancel
            }
        )),
        "a NOMOUSE menu leaves on the button 1 release, which reports 3: {:?}",
        input.borrow()
    );
    assert!(
        !input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Choose(_)
            }
        )),
        "a NOMOUSE menu chooses nothing under button 1: {:?}",
        input.borrow()
    );

    raise(true, cx);
    input.borrow_mut().clear();
    cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
    assert!(
        input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Choose(0)
            }
        )),
        "a menu that took the mouse still chooses the row under button 1: {:?}",
        input.borrow()
    );
}

#[ala::test]
fn a_bare_motion_over_a_nomouse_menu_sends_nothing(cx: &mut TestAppContext) {
    let (input, cx) = menu_mouse_fixture(cx, false, false);
    let row = cx
        .debug_bounds("display-menu-row-1")
        .expect("the menu drew its second row");

    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    assert!(
        input.borrow().is_empty(),
        "the pin never hears a bare motion over a NOMOUSE menu: {:?}",
        input.borrow()
    );

    cx.simulate_keystrokes("escape");
    assert!(
        menu_cancelled(&input),
        "the menu is still up for its keys: {:?}",
        input.borrow()
    );
}

fn menu_mouse_fixture(
    cx: &mut TestAppContext,
    mouse_keys: bool,
    stay_open: bool,
) -> (Rc<RefCell<Vec<InputMessage>>>, &mut ala::VisualTestContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "menu mouse".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();

    let mut state = menu_state_for_test();
    state.mouse_keys = mouse_keys;
    state.stay_open = stay_open;
    state.height = 5;
    state.items = (0..3)
        .map(|index| {
            Some(zz_protocol::MenuItem {
                name: format!("Item {index}"),
                key: None,
                annotation: None,
                enabled: true,
            })
        })
        .collect();
    state.selected = None;
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: zz_protocol::EventPayload::Menu { state: Some(state) },
            }),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    input.borrow_mut().clear();
    (input, cx)
}

fn menu_chose(input: &Rc<RefCell<Vec<InputMessage>>>, index: u32) -> bool {
    input.borrow().iter().any(|message| {
        matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Choose(chosen)
            } if *chosen == index
        )
    })
}

fn menu_cancelled(input: &Rc<RefCell<Vec<InputMessage>>>) -> bool {
    input.borrow().iter().any(|message| {
        matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Cancel
            }
        )
    })
}

#[ala::test]
fn a_mouse_menu_moves_its_highlight_under_a_motion(cx: &mut TestAppContext) {
    let (input, cx) = menu_mouse_fixture(cx, true, false);
    let row = cx
        .debug_bounds("display-menu-row-2")
        .expect("the menu drew its third row");

    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    assert!(
        !menu_cancelled(&input) && input.borrow().is_empty(),
        "a motion neither chooses nor closes: {:?}",
        input.borrow()
    );

    cx.simulate_keystrokes("enter");
    assert!(
        menu_chose(&input, 2),
        "Enter chooses the row the motion highlighted: {:?}",
        input.borrow()
    );
}

#[ala::test]
fn a_release_inside_a_mouse_menu_chooses_the_highlighted_row(cx: &mut TestAppContext) {
    let (input, cx) = menu_mouse_fixture(cx, true, false);
    let highlighted = cx
        .debug_bounds("display-menu-row-2")
        .expect("the menu drew its third row");
    let landed_on = cx
        .debug_bounds("display-menu-row-0")
        .expect("the menu drew its first row");

    cx.simulate_mouse_down(landed_on.center(), MouseButton::Left, Modifiers::default());
    assert!(
        input.borrow().is_empty(),
        "the press only moves the highlight: {:?}",
        input.borrow()
    );

    cx.simulate_mouse_move(highlighted.center(), None, Modifiers::default());
    cx.simulate_mouse_up(landed_on.center(), MouseButton::Left, Modifiers::default());
    assert!(
        menu_chose(&input, 2),
        "the release chooses the highlighted row: {:?}",
        input.borrow()
    );
    assert!(
        !menu_chose(&input, 0),
        "and not the row it landed on: {:?}",
        input.borrow()
    );
}

#[ala::test]
fn a_release_outside_a_mouse_menu_closes_it_and_a_press_does_not(cx: &mut TestAppContext) {
    let (input, cx) = menu_mouse_fixture(cx, true, false);
    let menu = cx
        .debug_bounds("display-menu")
        .expect("the menu drew its box");
    let outside = ala::point(menu.origin.x - px(24.0), menu.center().y);

    cx.simulate_mouse_down(outside, MouseButton::Left, Modifiers::default());
    assert!(
        !menu_cancelled(&input),
        "a press outside leaves the menu up: {:?}",
        input.borrow()
    );
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    assert!(
        menu_cancelled(&input),
        "a release outside closes the menu: {:?}",
        input.borrow()
    );
    assert!(
        !menu_chose(&input, 0) && !menu_chose(&input, 1) && !menu_chose(&input, 2),
        "closing outside the box chooses nothing: {:?}",
        input.borrow()
    );
}

#[ala::test]
fn a_stay_open_menu_closes_outside_on_a_press_and_not_on_a_wheel(cx: &mut TestAppContext) {
    let (input, cx) = menu_mouse_fixture(cx, true, true);
    let menu = cx
        .debug_bounds("display-menu")
        .expect("the menu drew its box");
    let outside = ala::point(menu.origin.x - px(24.0), menu.center().y);

    cx.simulate_event(ala::ScrollWheelEvent {
        position: outside,
        delta: ala::ScrollDelta::Lines(ala::point(0.0, -1.0)),
        modifiers: Modifiers::default(),
        touch_phase: ala::TouchPhase::Moved,
    });
    assert!(
        !menu_cancelled(&input),
        "a wheel outside a stay-open menu leaves it up: {:?}",
        input.borrow()
    );

    cx.simulate_mouse_down(outside, MouseButton::Left, Modifiers::default());
    assert!(
        menu_cancelled(&input),
        "a press outside closes a stay-open menu: {:?}",
        input.borrow()
    );
}

#[ala::test]
fn menu_and_confirm_surfaces_take_focus_and_bypass_the_prefix_claim(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: zz_protocol::EventPayload::Menu {
                    state: Some(menu_state_for_test()),
                },
            }),
            cx,
        );
        mux.set_prefix_armed_for_test(true, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });

    let menu = workspace.read_with(cx, |workspace, _| {
        workspace.menu.clone().expect("menu entity")
    });
    assert!(cx.debug_bounds("display-menu").is_some());
    assert!(cx.update(|window, cx| menu.read(cx).focus().contains_focused(window, cx)));
    input.borrow_mut().clear();
    cx.simulate_keystrokes("q");
    assert!(input.borrow().iter().any(|message| matches!(
        message,
        InputMessage::Menu {
            action: zz_protocol::MenuAction::Choose(0)
        }
    )));
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { .. }))
    );
    input.borrow_mut().clear();
    cx.simulate_keystrokes("ctrl-a");
    assert!(input.borrow().is_empty());
    assert!(cx.update(|window, cx| menu.read(cx).focus().contains_focused(window, cx)));

    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 2,
                payload: zz_protocol::EventPayload::Menu { state: None },
            }),
            cx,
        );
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: zz_protocol::EventPayload::Confirm {
                    state: Some(zz_protocol::ConfirmState {
                        prompt: "Confirm? (y/n) ".to_owned(),
                        confirm_key: b'y',
                        default_yes: false,
                        prompt_cursor: zz_protocol::PromptCursor::default(),
                    }),
                },
            }),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let confirm = workspace.read_with(cx, |workspace, _| {
        workspace.confirm.clone().expect("confirm entity")
    });
    assert!(cx.debug_bounds("confirm-before").is_some());
    assert!(cx.update(|window, cx| confirm.read(cx).focus().contains_focused(window, cx)));
    input.borrow_mut().clear();
    cx.simulate_keystrokes("y");
    assert!(input.borrow().iter().any(|message| matches!(
        message,
        InputMessage::Confirm {
            action: zz_protocol::ConfirmAction::Reply(true)
        }
    )));
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { .. }))
    );
}
