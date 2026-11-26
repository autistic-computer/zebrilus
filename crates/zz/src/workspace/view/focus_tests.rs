use std::cell::{Cell, RefCell};

use super::test_support::*;
use super::*;
use ala::TestAppContext;

#[test]
fn empty_workspace_requires_a_real_snapshot_without_an_error() {
    assert!(!empty_workspace_available(0, 0, false));
    assert!(!empty_workspace_available(4, 1, false));
    assert!(!empty_workspace_available(4, 0, true));
    assert!(empty_workspace_available(4, 0, false));
}

#[test]
fn the_empty_workspace_focus_debt_outlives_a_pass_that_never_pays_it() {
    let became_visible = empty_workspace_focus_owed(true, false, false);
    assert!(became_visible);
    assert!(empty_workspace_focus_owed(true, true, became_visible));
    assert!(!empty_workspace_focus_owed(true, true, false));
    assert!(!empty_workspace_focus_owed(false, true, true));
}

#[test]
fn the_revision_carries_the_window_the_focus_passes_resolve() {
    let pane = PaneId(1);
    let window = |id: WindowId| WindowSnapshot {
        id,
        index: u32::try_from(id.0).expect("fixture index"),
        name: id.to_string(),
        automatic_rename: true,
        active_pane: pane,
        zoomed_pane: None,
        layout: LayoutNode::Pane(pane),
        panes: BTreeMap::new(),
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: zz_protocol::PaneBorderStatus::Off,
        pane_border_lines: zz_protocol::PaneBorderLines::Single,
        pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
        pane_order: Vec::new(),
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 0,
    };
    let attached = SessionId(1);
    let session = zz_protocol::SessionSnapshot {
        id: attached,
        name: "zz".to_owned(),
        active_window: WindowId(1),
        windows: vec![window(WindowId(1)), window(WindowId(2))],
        viewers: Vec::new(),
    };
    let snapshot = MuxSnapshot {
        generation: 4,
        sessions: vec![session],
        focused_window: Some(WindowId(2)),
    };
    assert_eq!(
        attached_focused_window(&snapshot, Some(attached)),
        Some(WindowId(2))
    );
    assert_eq!(attached_focused_window(&snapshot, None), None);
    assert_eq!(
        attached_focused_window(
            &MuxSnapshot {
                focused_window: None,
                ..snapshot.clone()
            },
            Some(attached)
        ),
        Some(WindowId(1))
    );

    let revision = AppRevision {
        snapshot_generation: snapshot.generation,
        attached_host: HostId::LOCAL,
        attached: Some(attached),
        focused_window: attached_focused_window(&snapshot, Some(attached)),
        error: None,
        command_output_pane: None,
        menu: 0,
        confirm: 0,
        command_prompt: 0,
        choose_tree: 0,
        choose_buffer: 0,
        display_panes: 0,
        prefix_armed: false,
        prefix_cancelled_request: None,
        sidebar_focus: 0,
        bell: 0,
        pending_commands: 0,
    };
    assert_ne!(
        revision,
        AppRevision {
            focused_window: Some(WindowId(1)),
            ..revision.clone()
        }
    );
}

#[ala::test]
fn window_activation_is_the_only_workspace_client_focus_source(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let initial_active_slot = Rc::new(Cell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let captured_initial_active = Rc::clone(&initial_active_slot);
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
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        captured_initial_active.set(Some(window.is_window_active()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    let initial_active = initial_active_slot.get().expect("initial activation state");
    assert!(!initial_active);
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Attached {
                session: SessionId(0),
                snapshot: two_pane_snapshot_of(PaneId(0), PaneKindSnapshot::Terminal),
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
    });
    cx.run_until_parked();
    assert!(
        input
            .borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::ClientFocus { focused } => Some(*focused),
                _ => None,
            })
            .collect::<Vec<_>>()
            .is_empty()
    );

    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: zz_protocol::EventPayload::Snapshot(two_pane_snapshot_of(
                    PaneId(2),
                    PaneKindSnapshot::Terminal,
                )),
            }),
            cx,
        );
    });
    cx.run_until_parked();
    assert!(
        input
            .borrow()
            .iter()
            .all(|message| !matches!(message, InputMessage::ClientFocus { .. }))
    );

    input.borrow_mut().clear();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert_eq!(
        input
            .borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::ClientFocus { focused } => Some(*focused),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [true]
    );

    input.borrow_mut().clear();
    cx.deactivate_window();
    assert_eq!(
        input
            .borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::ClientFocus { focused } => Some(*focused),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [false]
    );
}

#[ala::test]
fn directional_focus_crosses_pane_kinds(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
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
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");

    let publish = |snapshot: MuxSnapshot, cx: &mut ala::VisualTestContext| {
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
    };
    let activate = |pane: PaneId, cx: &mut ala::VisualTestContext| {
        publish(two_pane_snapshot(pane), cx);
    };

    publish(one_pane_snapshot(1), cx);
    publish(two_pane_snapshot(PaneId(2)), cx);
    let picker_after_split = workspace.read_with(cx, |workspace, cx| {
        workspace
            .pickers
            .get(&PaneId(2))
            .expect("picker entity")
            .read(cx)
            .focus()
            .clone()
    });
    assert!(
        cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
        "the picker raised by a split never took focus"
    );
    let terminal_has_the_keyboard = |cx: &mut ala::VisualTestContext| {
        cx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|context| context.contains("Terminal"))
        })
    };

    activate(PaneId(0), cx);
    let terminal_focus = workspace.read_with(cx, |workspace, cx| {
        workspace
            .terminals
            .get(&PaneId(0))
            .expect("terminal entity")
            .read(cx)
            .focus()
    });
    assert!(
        cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
            && terminal_has_the_keyboard(cx),
        "the terminal never took focus"
    );

    activate(PaneId(2), cx);
    assert!(
        cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
        "terminal -> picker left the window unfocused"
    );

    activate(PaneId(0), cx);
    assert!(
        cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
            && terminal_has_the_keyboard(cx),
        "picker -> terminal left the window unfocused"
    );

    activate(PaneId(2), cx);
    mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_keystrokes("h");
    mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(false, cx));
    activate(PaneId(0), cx);
    assert!(
        terminal_has_the_keyboard(cx),
        "the prefix chord left the terminal without the keyboard"
    );

    for round in 0..3 {
        activate(PaneId(2), cx);
        assert!(
            cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
            "round {round}: terminal -> picker left the window unfocused"
        );
        activate(PaneId(0), cx);
        assert!(
            cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
                && terminal_has_the_keyboard(cx),
            "round {round}: picker -> terminal left the window unfocused"
        );
    }

    activate(PaneId(0), cx);
    cx.update(ala::Window::blur);
    let mut same_pane_again = two_pane_snapshot(PaneId(0));
    same_pane_again.generation = 99;
    publish(same_pane_again, cx);
    assert!(
        terminal_has_the_keyboard(cx),
        "a stranded window focus was never handed back to the active pane"
    );

    let typed = mux.update(cx, |mux, _| mux.record_input_for_test());
    activate(PaneId(2), cx);
    activate(PaneId(0), cx);
    cx.simulate_input("x");
    assert!(
        typed.borrow().iter().any(|input| matches!(
            input,
            InputMessage::Text { pane, text } if *pane == PaneId(0) && text == "x"
        )),
        "the terminal took no typed text after picker -> terminal: {:?}",
        typed.borrow()
    );
    typed.borrow_mut().clear();

    activate(PaneId(2), cx);
    publish(
        two_pane_snapshot_of(PaneId(2), PaneKindSnapshot::Terminal),
        cx,
    );
    let materialized = workspace.read_with(cx, |workspace, cx| {
        workspace
            .terminals
            .get(&PaneId(2))
            .expect("the answered picker becomes a terminal")
            .read(cx)
            .focus()
    });
    assert!(
        cx.update(|window, cx| materialized.contains_focused(window, cx))
            && terminal_has_the_keyboard(cx),
        "the pane the picker turned into never took the keyboard"
    );
}

#[ala::test]
fn a_cross_host_attach_hands_the_keyboard_to_the_machine_it_lands_on(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![crate::config::HostEntry {
                name: "remote".to_owned(),
                endpoint: zz_daemon_client::Endpoint::parse("unix:///tmp/zz-cross-host.sock")
                    .expect("test endpoint"),
            }],
            cx,
        );
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
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let publish = |snapshot: MuxSnapshot, cx: &mut ala::VisualTestContext| {
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
    };
    let terminal_has_the_keyboard = |cx: &mut ala::VisualTestContext| {
        cx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|context| context.contains("Terminal"))
        })
    };

    publish(one_pane_snapshot(1), cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .sidebar
            .update(cx, |sidebar, cx| sidebar.focus(window, cx));
    });
    cx.run_until_parked();
    assert!(
        !terminal_has_the_keyboard(cx),
        "the tree never took the keyboard, so the switch under test proves nothing"
    );

    mux.update(cx, |mux, cx| mux.attach_host_for_test("remote", cx));
    cx.run_until_parked();
    assert!(
        workspace.read_with(cx, |workspace, _| workspace.pane_focus_owed),
        "a machine with nothing on screen yet dropped the handover instead of holding it"
    );

    publish(one_pane_snapshot(1), cx);
    assert!(
        terminal_has_the_keyboard(cx),
        "the pane the new machine landed on never took the keyboard"
    );
}

#[test]
fn pane_selection_targets_the_requested_pane() {
    assert_eq!(
        pane_select_command(PaneId(7)),
        CommandInvocation::new("select-pane", ["-t", "%7"])
    );
}
