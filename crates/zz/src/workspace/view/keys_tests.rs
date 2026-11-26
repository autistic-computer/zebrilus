use std::cell::{Cell, RefCell};

use super::test_support::*;
use super::tests::*;
use super::*;
use ala::{Modifiers, TestAppContext};

#[cfg(unix)]
#[ala::test]
fn ui_chord_bound_to_the_prefix_opens_palette_without_arming(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    cx.update(|_, cx| {
        crate::keymap::install(
            &[crate::keymap::ChromeOverride::Bind {
                table: zz_client::UI_TABLE,
                key: "C-a".to_owned(),
                action: ChromeAction::OpenCommandPalette.name().to_owned(),
            }],
            config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY,
            cx,
        );
    });
    let stroke = Keystroke::parse("ctrl-a").unwrap();
    assert!(mux.read_with(cx, |mux, _| {
        mux.claims_prefix_input(&terminal_key_input(&stroke, TerminalKeyAction::Press))
    }));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_event(KeyUpEvent { keystroke: stroke });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_some()));
    assert!(!mux.read_with(cx, |mux, _| mux.prefix_armed()));
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { .. }))
    );
}

#[cfg(unix)]
#[ala::test]
fn claimed_press_and_release_reach_mux_without_terminal_key_down(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let (mux, terminal) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.mux.clone(),
            workspace.terminals[&PaneId(0)].clone(),
        )
    });
    assert!(cx.update(|window, cx| terminal.read(cx).focus().is_focused(window)));
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    let stroke = Keystroke::parse("ctrl-a").unwrap();
    cx.simulate_event(ala::KeyDownEvent {
        keystroke: stroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent {
        keystroke: stroke.clone(),
    });
    assert_eq!(
        input.borrow().as_slice(),
        &[
            InputMessage::Key {
                pane: PaneId(0),
                input: terminal_key_input(&stroke, TerminalKeyAction::Press),
                text_follows: false
            },
            InputMessage::Key {
                pane: PaneId(0),
                input: terminal_key_input(&stroke, TerminalKeyAction::Release),
                text_follows: false
            },
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.input_router.owner()),
        zz_client::InputOwner::None
    );
}

#[cfg(unix)]
#[ala::test]
fn standalone_modifier_tap_leaves_the_armed_prefix_waiting(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    cx.simulate_modifiers_change(Modifiers::shift());
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { .. }))
    );
    assert!(mux.read_with(cx, |mux, _| mux.prefix_armed()));
}

#[cfg(unix)]
fn publish_key_table(
    mux: &Entity<MuxClient>,
    table: Option<&str>,
    repeat: bool,
    cx: &mut ala::VisualTestContext,
) {
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 0,
                payload: zz_protocol::EventPayload::KeyTableActive {
                    table: table.map(str::to_owned),
                    repeat,
                },
            }),
            cx,
        );
    });
    cx.run_until_parked();
}

#[cfg(unix)]
fn which_key_visible(workspace: &Entity<AppView>, cx: &mut ala::VisualTestContext) -> bool {
    workspace.read_with(cx, |workspace, _| workspace.which_key.is_some())
}

#[cfg(unix)]
fn wait_for_which_key(cx: &mut ala::VisualTestContext, milliseconds: u64) {
    cx.executor()
        .advance_clock(Duration::from_millis(milliseconds));
    cx.run_until_parked();
}

#[cfg(unix)]
#[ala::test]
fn which_key_shows_after_the_prefix_pause_and_hides_on_the_next_key(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    let stroke = Keystroke::parse("ctrl-a").unwrap();
    cx.simulate_event(ala::KeyDownEvent {
        keystroke: stroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    publish_key_table(&mux, Some("prefix"), false, cx);
    cx.simulate_event(KeyUpEvent {
        keystroke: stroke.clone(),
    });
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[
            InputMessage::Key {
                pane: PaneId(0),
                input: terminal_key_input(&stroke, TerminalKeyAction::Press),
                text_follows: false
            },
            InputMessage::Key {
                pane: PaneId(0),
                input: terminal_key_input(&stroke, TerminalKeyAction::Release),
                text_follows: false
            },
        ]
    );
    wait_for_which_key(cx, 399);
    assert!(!which_key_visible(&workspace, cx));
    wait_for_which_key(cx, 2);
    assert!(which_key_visible(&workspace, cx));
    workspace.read_with(cx, |workspace, _| {
        let sheet = workspace.which_key.as_ref().unwrap();
        assert_eq!(sheet.header().table.as_ref(), "prefix");
        assert_eq!(sheet.header().prefix_raw.as_ref(), "C-a");
        assert!(sheet.header().prefix.is_some());
        let split = sheet
            .rows()
            .iter()
            .find(|row| row.id.as_ref() == "%")
            .unwrap();
        assert_eq!(split.group.as_deref(), Some("Panes"));
        assert!(!split.caps[0].keys.is_empty());
        assert!(!split.caps[0].yours);
        assert!(sheet.rows().iter().all(|row| row.id.as_ref() != "M-1"));
        assert_eq!(
            sheet.header().more.as_ref().map(|more| more.raw.as_ref()),
            Some("?")
        );
    });
    assert!(workspace.read_with(cx, |workspace, _| workspace.focused_overlay.is_none()));
    let sheet = cx.debug_bounds("which-key").unwrap();
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!(sheet.bottom() <= viewport.height);
    assert!(sheet.bottom() > viewport.height - px(40.0));
    assert!(sheet.top() >= px(0.));
    assert!(sheet.left() >= px(16.0));
    let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
    assert!(sheet.top() >= canvas.top(), "{sheet:?} {canvas:?}");
    let terminal = workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
    assert!(cx.update(|window, cx| terminal.read(cx).focus().is_focused(window)));
    cx.simulate_keystrokes("c");
    assert!(!which_key_visible(&workspace, cx));
    assert!(
        input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("c")))
    );
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::CancelPrefix { .. }))
    );
}

#[cfg(unix)]
#[ala::test]
fn a_held_prefix_key_keeps_the_which_key_timer(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
    publish_key_table(&mux, Some("prefix"), false, cx);
    wait_for_which_key(cx, 200);
    cx.simulate_event(ala::KeyDownEvent {
        keystroke: Keystroke::parse("ctrl-a").unwrap(),
        is_held: true,
        prefer_character_input: false,
    });
    cx.run_until_parked();
    wait_for_which_key(cx, 201);
    assert!(which_key_visible(&workspace, cx));
}

#[cfg(unix)]
#[ala::test]
fn which_key_hides_for_a_repeat_window_and_a_cleared_table(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    publish_key_table(&mux, Some("prefix"), false, cx);
    wait_for_which_key(cx, 401);
    assert!(which_key_visible(&workspace, cx));
    publish_key_table(&mux, Some("prefix"), true, cx);
    assert!(!which_key_visible(&workspace, cx));
    wait_for_which_key(cx, 2_000);
    assert!(!which_key_visible(&workspace, cx));
    publish_key_table(&mux, Some("prefix"), false, cx);
    wait_for_which_key(cx, 200);
    publish_key_table(&mux, None, false, cx);
    wait_for_which_key(cx, 2_000);
    assert!(!which_key_visible(&workspace, cx));
    assert!(workspace.read_with(cx, |workspace, _| workspace.which_key_timer.is_none()));
    publish_key_table(&mux, Some("prefix"), false, cx);
    wait_for_which_key(cx, 401);
    assert!(which_key_visible(&workspace, cx));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_command_palette(None, window, cx);
    });
    cx.run_until_parked();
    assert!(!which_key_visible(&workspace, cx));
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.focused_overlay == Some(OverlayKind::CommandPalette)
    }));
    publish_key_table(&mux, Some("prefix"), false, cx);
    wait_for_which_key(cx, 401);
    assert!(!which_key_visible(&workspace, cx));
}

#[cfg(unix)]
#[ala::test]
fn which_key_lists_a_custom_table_flat_and_hides_on_its_keys(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    let mut tables = zz_protocol::KeyTables::default();
    for (key, direction) in [("h", "-L"), ("l", "-R")] {
        tables.bind(
            "resize",
            key,
            zz_protocol::Binding {
                commands: vec![CommandInvocation::new("resize-pane", [direction])],
                repeat: true,
                note: None,
            },
        );
    }
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 0,
                payload: zz_protocol::EventPayload::KeyTablesChanged {
                    tables: tables.snapshot(),
                },
            }),
            cx,
        );
    });
    publish_key_table(&mux, Some("resize"), false, cx);
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    wait_for_which_key(cx, 401);
    workspace.read_with(cx, |workspace, _| {
        let sheet = workspace.which_key.as_ref().unwrap();
        assert_eq!(sheet.header().table.as_ref(), "resize");
        assert!(sheet.header().prefix.is_none());
        assert_eq!(
            sheet
                .rows()
                .iter()
                .map(|row| row.caps[0].raw.as_ref())
                .collect::<Vec<_>>(),
            ["h l"]
        );
        assert!(sheet.header().more.is_none());
        assert!(
            sheet
                .rows()
                .iter()
                .all(|row| row.group.is_none() && row.repeat)
        );
    });
    cx.simulate_keystrokes("h");
    assert!(!which_key_visible(&workspace, cx));
    assert!(
        input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("h")))
    );
    publish_key_table(&mux, Some("resize"), false, cx);
    wait_for_which_key(cx, 401);
    assert!(which_key_visible(&workspace, cx));
}

#[cfg(unix)]
#[ala::test]
fn question_mark_opens_every_key_and_leaves_the_prefix_armed(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
    publish_key_table(&mux, Some("prefix"), false, cx);
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    cx.simulate_keystrokes("?");
    workspace.read_with(cx, |workspace, _| {
        let sheet = workspace.which_key.as_ref().unwrap();
        assert!(sheet.header().more.is_none());
        assert!(sheet.rows().iter().any(|row| row.id.as_ref() == "M-1"));
    });
    assert!(input.borrow().is_empty(), "{:?}", input.borrow());
    cx.simulate_keystrokes("c");
    assert!(!which_key_visible(&workspace, cx));
    assert!(
        input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("c")))
    );
}

#[cfg(unix)]
#[ala::test]
fn which_key_delay_zero_never_shows(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    cx.update(|_, cx| {
        let mut config = config::resolved_config(cx);
        config.which_key_delay = zz_config::ConfigValue::from_default(0.0);
        cx.set_global(config);
    });
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    publish_key_table(&mux, Some("prefix"), false, cx);
    assert!(workspace.read_with(cx, |workspace, _| workspace.which_key_timer.is_none()));
    wait_for_which_key(cx, 5_000);
    assert!(!which_key_visible(&workspace, cx));
}

#[ala::test]
fn window_shortcuts_intercept_before_pane_input_and_honor_unbinds(cx: &mut TestAppContext) {
    let fallback_count = Rc::new(Cell::new(0));
    let (modifier, key) = if cfg!(target_os = "macos") {
        ("cmd", "D-1")
    } else {
        ("ctrl", "C-1")
    };
    cx.update(|cx| {
        zz_ui::init(cx);
        crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
        cx.bind_keys((1..=9).map(|position| {
            ala::KeyBinding::new(
                &format!("{modifier}-{position}"),
                WindowShortcutFallback,
                None,
            )
        }));
        let fallback_count = fallback_count.clone();
        cx.on_action(move |_: &WindowShortcutFallback, _| {
            fallback_count.set(fallback_count.get() + 1);
        });
    });
    let (workspace, cx) = cx.add_window_view(|window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "window shortcuts".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();
    for position in 1..=9 {
        cx.simulate_keystrokes(&format!("{modifier}-{position}"));
    }
    assert_eq!(fallback_count.get(), 0);
    cx.update(|_, cx| {
        crate::keymap::install(
            &[crate::keymap::ChromeOverride::Unbind {
                table: zz_client::UI_TABLE,
                key: key.to_owned(),
            }],
            config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY,
            cx,
        );
    });
    cx.simulate_keystrokes(&format!("{modifier}-1"));
    assert_eq!(fallback_count.get(), 1);
}

#[ala::test]
fn dialog_prefix_cancel_barrier_handles_acknowledgements_and_connection_reset(
    cx: &mut TestAppContext,
) {
    cx.update(zz_ui::init);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let captured_observed = Rc::clone(&observed);
    cx.update(move |cx| {
        cx.observe_keystrokes(move |event, _, _| {
            captured_observed.borrow_mut().push((
                event.keystroke.modifiers.platform,
                event.keystroke.modifiers.function,
                event.keystroke.key.clone(),
            ));
        })
        .detach();
    });
    let workspace_slot = Rc::new(RefCell::new(None));
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_workspace = Rc::clone(&workspace_slot);
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
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        let workspace =
            cx.new(|cx| AppView::new(controller, agent_controller, mux.clone(), window, cx));
        captured_workspace.replace(Some(workspace.clone()));
        captured_mux.replace(Some(mux));
        captured_input.replace(Some(input));
        penna::Root::new(workspace, window, cx)
    });
    let workspace = workspace_slot.borrow().clone().expect("captured workspace");
    let mux = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    input.borrow_mut().clear();

    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[InputMessage::CancelPrefix { request_id: 1 }]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
        Some(1)
    );

    cx.update(penna::WindowExt::close_dialog);
    cx.simulate_keystrokes("c cmd-dialogtest fn-dialogtest");
    assert_eq!(
        input.borrow().as_slice(),
        &[InputMessage::CancelPrefix { request_id: 1 }]
    );
    assert_eq!(
        observed.borrow().as_slice(),
        &[
            (true, false, "dialogtest".to_owned()),
            (false, true, "dialogtest".to_owned()),
        ]
    );

    mux.update(cx, |mux, cx| {
        mux.acknowledge_prefix_cancel_for_test(1, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
        None
    );
    cx.simulate_keystrokes("c");
    assert_eq!(
        observed.borrow().as_slice(),
        &[
            (true, false, "dialogtest".to_owned()),
            (false, true, "dialogtest".to_owned()),
            (false, false, "c".to_owned()),
        ]
    );

    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[
            InputMessage::CancelPrefix { request_id: 1 },
            InputMessage::CancelPrefix { request_id: 2 },
        ]
    );
    cx.update(penna::WindowExt::close_dialog);
    mux.update(cx, |mux, cx| {
        mux.acknowledge_prefix_cancel_for_test(1, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
        Some(2)
    );
    mux.update(cx, |mux, cx| {
        mux.acknowledge_prefix_cancel_for_test(2, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
        None
    );

    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[
            InputMessage::CancelPrefix { request_id: 1 },
            InputMessage::CancelPrefix { request_id: 2 },
            InputMessage::CancelPrefix { request_id: 3 },
        ]
    );
    cx.update(penna::WindowExt::close_dialog);
    mux.update(cx, |mux, cx| {
        mux.reset_session_state_for_test(cx);
        mux.acknowledge_prefix_cancel_for_test(2, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        mux.read_with(cx, |mux, _| mux.prefix_cancelled_request()),
        Some(3)
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
        None
    );
    cx.simulate_keystrokes("r");
    assert_eq!(
        observed.borrow().as_slice(),
        &[
            (true, false, "dialogtest".to_owned()),
            (false, true, "dialogtest".to_owned()),
            (false, false, "c".to_owned()),
            (false, false, "r".to_owned()),
        ]
    );
}
