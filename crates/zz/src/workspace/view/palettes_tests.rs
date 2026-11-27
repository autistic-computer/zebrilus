use std::cell::{Cell, RefCell};

use super::test_support::*;
use super::*;
use ala::TestAppContext;

#[ala::test]
fn local_palette_survives_mux_updates_and_reopens_with_both_shortcuts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        zz_ui::init(cx);
        crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
    });
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "palette lifecycle".to_owned(),
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
    let shortcuts = if cfg!(target_os = "macos") {
        ["cmd-k", "cmd-p"]
    } else {
        ["ctrl-shift-k", "ctrl-shift-p"]
    };
    for shortcut in shortcuts {
        cx.simulate_keystrokes(shortcut);
        cx.run_until_parked();
        let palette = workspace.read_with(cx, |workspace, cx| {
            let palette = workspace.command_palette.clone().expect("palette opened");
            assert!(palette.read(cx).is_local());
            assert!(workspace.mux.read(cx).command_prompt().is_none());
            palette
        });
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(2), cx);
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.command_palette.as_ref(), Some(&palette));
        });
        cx.simulate_keystrokes(shortcut);
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert_ne!(workspace.command_palette.as_ref(), Some(&palette));
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert!(workspace.command_palette.is_none());
        });
    }
    let sidebar = workspace.read_with(cx, |workspace, _| workspace.sidebar.clone());
    sidebar.update_in(cx, WorkspaceSidebar::open_settings);
    cx.run_until_parked();
    let settings_focus = sidebar.read_with(cx, |sidebar, cx| {
        sidebar.settings_view().unwrap().read(cx).focus()
    });
    for shortcut in shortcuts {
        cx.simulate_keystrokes(shortcut);
        cx.simulate_keystrokes(shortcut);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.update(|window, cx| settings_focus.contains_focused(window, cx)));
    }
}

#[ala::test]
fn daemon_window_chooser_uses_palette_and_preserves_selection_actions(cx: &mut TestAppContext) {
    cx.update(|cx| {
        zz_ui::init(cx);
        crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
    });
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "window palette".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    let input = mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        mux.record_input_for_test()
    });
    let sequence = Cell::new(0);
    let publish = |state, cx: &mut ala::VisualTestContext| {
        sequence.set(sequence.get() + 1);
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: sequence.get(),
                    payload: zz_protocol::EventPayload::ChooseTree { state },
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };
    let mut state = choose_tree_state_for_test();
    state.kind = zz_protocol::ChooseTreeKind::Windows;
    state.items[0].flags =
        zz_protocol::ChooseTreeItem::HAS_CHILDREN | zz_protocol::ChooseTreeItem::EXPANDED;
    state.items[1].label = "1:build".to_owned();
    state.items[1].depth = 1;
    let mut logs = state.items[1].clone();
    logs.label = "2:logs".to_owned();
    logs.target = zz_protocol::ChooseTreeTarget::Window(WindowId(1));
    state.items.push(logs);
    state.selected = 2;
    publish(Some(state.clone()), cx);
    assert!(!input.borrow().iter().any(|message| matches!(
        message,
        InputMessage::ChooseTree {
            action: zz_protocol::ChooseTreeAction::ExpandAll
        }
    )));
    assert!(cx.debug_bounds("choose-tree-overlay").is_none());
    assert!(cx.debug_bounds("command-palette-overlay").is_some());
    let palette = workspace.read_with(cx, |workspace, cx| {
        assert!(workspace.choose_tree.is_none());
        let palette = workspace.command_palette.clone().expect("window palette");
        assert!(palette.read(cx).is_window_chooser());
        palette
    });
    assert!(cx.update(|window, cx| palette.read(cx).focus(cx).is_focused(window)));
    input.borrow_mut().clear();
    cx.simulate_input("logs");
    cx.run_until_parked();
    assert!(input.borrow().is_empty(), "search stays in the palette");
    state.items.insert(0, state.items[0].clone());
    publish(Some(state.clone()), cx);
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
        Some(palette)
    );
    cx.simulate_keystrokes("enter");
    assert!(input.borrow().iter().any(|message| matches!(
        message,
        InputMessage::ChooseTree {
            action: zz_protocol::ChooseTreeAction::ActivateIndex(3)
        }
    )));
    publish(None, cx);
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
    publish(Some(state.clone()), cx);
    input.borrow_mut().clear();
    cx.simulate_keystrokes("escape");
    assert!(input.borrow().iter().any(|message| matches!(
        message,
        InputMessage::ChooseTree {
            action: zz_protocol::ChooseTreeAction::Close
        }
    )));
    publish(None, cx);
    publish(Some(state.clone()), cx);
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-k"
    } else {
        "ctrl-shift-k"
    });
    publish(None, cx);
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .command_palette
            .as_ref()
            .is_some_and(|palette| palette.read(cx).is_local())
    }));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    for dismiss_immediately in [false, true] {
        publish(Some(state.clone()), cx);
        let original = workspace.read_with(cx, |workspace, _| {
            workspace.command_palette.clone().unwrap()
        });
        input.borrow_mut().clear();
        cx.simulate_input("x");
        cx.run_until_parked();
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();
        assert!(original.read_with(cx, |palette, _| palette.is_window_chooser()));
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();
        assert!(original.read_with(cx, |palette, _| palette.is_local()
            && !palette.is_window_chooser()));
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
            Some(original.clone())
        );
        assert!(cx.update(|window, cx| original.read(cx).focus(cx).is_focused(window)));
        assert_eq!(
            input
                .borrow()
                .iter()
                .filter(|message| matches!(
                    message,
                    InputMessage::ChooseTree {
                        action: zz_protocol::ChooseTreeAction::Close
                    }
                ))
                .count(),
            1
        );
        if dismiss_immediately {
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
        }
        publish(Some(state.clone()), cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
            (!dismiss_immediately).then_some(original.clone())
        );
        if dismiss_immediately {
            mux.update(cx, |mux, cx| {
                for state in [None, Some(state.clone())] {
                    sequence.set(sequence.get() + 1);
                    mux.handle_message_for_test(
                        zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                            sequence: sequence.get(),
                            payload: zz_protocol::EventPayload::ChooseTree { state },
                        }),
                        cx,
                    );
                }
            });
            cx.run_until_parked();
        } else {
            publish(None, cx);
            assert_eq!(
                workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
                Some(original.clone())
            );
            publish(Some(state.clone()), cx);
        }
        workspace.read_with(cx, |workspace, cx| {
            let current = workspace.command_palette.as_ref().unwrap();
            assert_ne!(current, &original);
            assert!(current.read(cx).is_window_chooser());
        });
        cx.simulate_keystrokes("escape");
        publish(None, cx);
    }
}

type PathPickerFixture<'a> = (
    Entity<AppView>,
    Entity<MuxClient>,
    Rc<RefCell<Vec<InputMessage>>>,
    Rc<RefCell<Vec<zz_protocol::ProtocolMessage>>>,
    &'a mut ala::VisualTestContext,
);

fn path_picker_workspace(cx: &mut TestAppContext) -> PathPickerFixture<'_> {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "path picker".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let (input, lists) = mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        (
            mux.record_input_for_test(),
            mux.record_path_lists_for_test(),
        )
    });
    cx.run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.focus_active_pane(window, cx);
    });
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    (workspace, mux, input, lists, cx)
}

fn publish_to_mux(
    mux: &Entity<MuxClient>,
    message: zz_protocol::ProtocolMessage,
    cx: &mut ala::VisualTestContext,
) {
    mux.update(cx, |mux, cx| mux.handle_message_for_test(message, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn open_path_picker_event(sequence: u64) -> zz_protocol::ProtocolMessage {
    open_path_picker_event_for(sequence, PaneId(0))
}

fn open_path_picker_event_for(sequence: u64, pane: PaneId) -> zz_protocol::ProtocolMessage {
    zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
        sequence,
        payload: zz_protocol::EventPayload::OpenPathPicker {
            pane,
            start_dir: None,
        },
    })
}

fn path_list_root_for_test() -> zz_protocol::PathListRoot {
    zz_protocol::PathListRoot {
        root: "/home/me/src".to_owned(),
        display_root: "~/src/".to_owned(),
        cwd: Some("/home/me/src".to_owned()),
        insert: zz_protocol::InsertStyle::Shell(zz_protocol::ShellKind::Posix),
    }
}

fn palette_input(workspace: &Entity<AppView>, cx: &mut ala::VisualTestContext) -> Option<String> {
    cx.update(|window, cx| {
        let palette = workspace.read(cx).command_palette.clone()?;
        let palette = palette.read(cx);
        palette
            .focus(cx)
            .is_focused(window)
            .then(|| palette.query().to_owned())
    })
}

fn terminal_is_focused(workspace: &Entity<AppView>, cx: &mut ala::VisualTestContext) -> bool {
    cx.update(|window, cx| {
        workspace.read(cx).terminals[&PaneId(0)]
            .read(cx)
            .focus()
            .is_focused(window)
    })
}

fn answer_listing(
    mux: &Entity<MuxClient>,
    entries: Vec<zz_protocol::PathEntry>,
    cx: &mut ala::VisualTestContext,
) {
    publish_to_mux(
        mux,
        zz_protocol::ProtocolMessage::PathListBegin {
            request_id: 1,
            result: Ok(path_list_root_for_test()),
        },
        cx,
    );
    publish_to_mux(
        mux,
        zz_protocol::ProtocolMessage::PathListChunk {
            request_id: 1,
            entries,
            done: true,
            truncated: false,
        },
        cx,
    );
}

fn notes() -> Vec<zz_protocol::PathEntry> {
    let mut entries = (0..30)
        .map(|index| zz_protocol::PathEntry {
            rel: format!("file{index:02}.rs"),
            kind: zz_protocol::PathKind::File,
            symlink: false,
        })
        .collect::<Vec<_>>();
    entries.push(zz_protocol::PathEntry {
        rel: "my notes.txt".to_owned(),
        kind: zz_protocol::PathKind::File,
        symlink: false,
    });
    entries
}

#[ala::test]
fn prefix_f_opens_the_palette_on_the_panes_folder_and_pastes_the_pick(cx: &mut TestAppContext) {
    let (workspace, mux, input, lists, cx) = path_picker_workspace(cx);
    assert!(terminal_is_focused(&workspace, cx));
    input.borrow_mut().clear();
    publish_to_mux(&mux, open_path_picker_event(1), cx);
    assert_eq!(palette_input(&workspace, cx).as_deref(), Some("~/"));
    assert!(
        input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::CancelPrefix { .. }))
    );
    assert_eq!(
        lists.borrow().as_slice(),
        &[zz_protocol::ProtocolMessage::PathListRequest {
            request_id: 1,
            pane: Some(PaneId(0)),
            dir: Some("~/".to_owned()),
        }]
    );
    answer_listing(&mux, notes(), cx);
    input.borrow_mut().clear();
    cx.simulate_input("notes");
    cx.run_until_parked();
    assert!(input.borrow().is_empty());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[InputMessage::TerminalView {
            pane: PaneId(0),
            action: zz_terminal::TerminalViewAction::Paste("'my notes.txt' ".to_owned()),
        }]
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
    assert!(terminal_is_focused(&workspace, cx));
}

#[ala::test]
fn prefix_f_closes_on_escape_and_cancels_its_listing(cx: &mut TestAppContext) {
    let (workspace, mux, _, lists, cx) = path_picker_workspace(cx);
    publish_to_mux(&mux, open_path_picker_event(1), cx);
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
    assert!(terminal_is_focused(&workspace, cx));
    assert!(lists.borrow().iter().any(|message| matches!(
        message,
        zz_protocol::ProtocolMessage::PathListCancel { request_id: 1 }
    )));
}

#[ala::test]
fn prefix_f_pastes_into_the_pane_it_came_from(cx: &mut TestAppContext) {
    let (workspace, mux, input, _, cx) = path_picker_workspace(cx);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(
            SessionId(0),
            two_pane_snapshot_of(PaneId(0), PaneKindSnapshot::Terminal),
            cx,
        );
    });
    cx.run_until_parked();
    publish_to_mux(&mux, open_path_picker_event_for(1, PaneId(2)), cx);
    answer_listing(&mux, notes(), cx);
    input.borrow_mut().clear();
    cx.simulate_input("notes");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        input.borrow().as_slice(),
        &[InputMessage::TerminalView {
            pane: PaneId(2),
            action: zz_terminal::TerminalViewAction::Paste("'my notes.txt' ".to_owned()),
        }]
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
}

#[test]
fn a_picked_path_follows_the_listing_root() {
    let root = path_list_root_for_test();
    let entry = |rel: &str, kind| zz_protocol::PathEntry {
        rel: rel.to_owned(),
        kind,
        symlink: false,
    };
    let insert = crate::command::palette::path_insert_text;
    assert_eq!(
        insert(&root, &entry("src/main.rs", zz_protocol::PathKind::File)).as_deref(),
        Some("src/main.rs ")
    );
    assert_eq!(
        insert(&root, &entry("docs", zz_protocol::PathKind::Dir)).as_deref(),
        Some("docs/ ")
    );
    let claude = zz_protocol::PathListRoot {
        insert: zz_protocol::InsertStyle::Claude,
        ..root
    };
    assert_eq!(
        insert(&claude, &entry("src/main.rs", zz_protocol::PathKind::File)).as_deref(),
        Some("@src/main.rs ")
    );
}
