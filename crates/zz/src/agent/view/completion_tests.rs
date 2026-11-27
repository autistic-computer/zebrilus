#[cfg(not(target_os = "macos"))]
use std::{cell::RefCell, rc::Rc};

#[cfg(not(target_os = "macos"))]
use ala::{TestAppContext, VisualTestContext};
#[cfg(not(target_os = "macos"))]
use penna::Root;
#[cfg(not(target_os = "macos"))]
use zz_daemon_client::DaemonError;

use super::*;

#[cfg(not(target_os = "macos"))]
fn command(name: &str) -> AgentCommand {
    AgentCommand {
        name: name.to_owned(),
        description: format!("Run {name}"),
        ..AgentCommand::default()
    }
}

fn permission(request_id: u64) -> AgentPermissionRequest {
    use crate::agent::controller::AgentPermissionOption;

    AgentPermissionRequest {
        request_id,
        tool_call_id: format!("tool-{request_id}"),
        title: format!("Run command {request_id}"),
        options: vec![
            AgentPermissionOption {
                id: format!("allow-{request_id}"),
                name: "Allow once".to_owned(),
                kind: AgentPermissionKind::AllowOnce,
            },
            AgentPermissionOption {
                id: format!("reject-{request_id}"),
                name: "Reject".to_owned(),
                kind: AgentPermissionKind::RejectOnce,
            },
        ],
        questions: Vec::new(),
    }
}

#[test]
fn a_card_takes_its_own_request_off_the_wizard_whatever_page_is_focused() {
    let requests = [permission(1), permission(2)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);
    assert!(wizard.take(&requests, 2));
    assert_eq!(wizard.current(&requests).map(|r| r.request_id), Some(1));
    assert!(
        !wizard.take(&requests, 2),
        "an answered request is taken once"
    );
    assert!(!wizard.take(&requests, 9));
    wizard.release(&requests, 2);
    assert_eq!(wizard.page_label(&requests).as_deref(), Some("1 of 2"));
}

#[test]
fn composer_action_follows_the_live_turn_and_the_draft() {
    assert_eq!(composer_action(false, false), ComposerAction::Send);
    assert_eq!(composer_action(false, true), ComposerAction::Send);
    assert_eq!(composer_action(true, true), ComposerAction::Queue);
    assert_eq!(composer_action(true, false), ComposerAction::Stop);
}

#[test]
fn permission_wizard_answers_the_focused_page_then_advances() {
    let requests = vec![permission(1), permission(2)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);

    assert_eq!(wizard.page_label(&requests).as_deref(), Some("1 of 2"));
    assert_eq!(
        wizard.answer(&requests, Some(1)),
        PermissionStep::Answer {
            request_id: 1,
            option_id: Some("reject-1".to_owned()),
        }
    );
    assert_eq!(
        wizard.current(&requests).map(|request| request.request_id),
        Some(2)
    );
    assert_eq!(wizard.page_label(&requests), None);
}

#[test]
fn permission_wizard_latches_an_answered_page_until_the_controller_drops_it() {
    let requests = vec![permission(1), permission(2)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);
    wizard.answer(&requests, Some(0));

    assert_eq!(
        wizard.answer(&requests, Some(0)),
        PermissionStep::Answer {
            request_id: 2,
            option_id: Some("allow-2".to_owned()),
        }
    );
    assert_eq!(wizard.current(&requests), None);

    wizard.sync(&[]);

    assert!(wizard.answered.is_empty());
}

#[test]
fn permission_wizard_keeps_its_page_when_another_request_resolves_elsewhere() {
    let requests = vec![permission(1), permission(2), permission(3)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);
    wizard.answer(&requests, Some(0));
    wizard.highlight(1);

    let remaining = vec![permission(2), permission(3)];
    wizard.sync(&remaining);

    assert_eq!(
        wizard.current(&remaining).map(|request| request.request_id),
        Some(2)
    );
    assert_eq!(wizard.highlighted, 1);

    let auto_approved = vec![permission(3)];
    wizard.sync(&auto_approved);

    assert_eq!(
        wizard
            .current(&auto_approved)
            .map(|request| request.request_id),
        Some(3)
    );
    assert_eq!(wizard.highlighted, 0);
}

#[test]
fn permission_digits_defer_to_a_composer_the_user_is_typing_into() {
    let requests = vec![permission(1)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);

    assert_eq!(wizard.press_digit(&requests, 1, true), PermissionStep::Stay);
    assert_eq!(
        wizard.press_digit(&requests, 0, false),
        PermissionStep::Stay
    );
    assert_eq!(
        wizard.press_digit(&requests, 7, false),
        PermissionStep::Stay
    );
    assert_eq!(
        wizard.press_digit(&requests, 2, false),
        PermissionStep::Answer {
            request_id: 1,
            option_id: Some("reject-1".to_owned()),
        }
    );
}

#[test]
fn permission_wizard_confirms_the_highlight_and_cancels_the_page() {
    let requests = vec![permission(1), permission(2)];
    let mut wizard = PermissionWizard::default();
    wizard.sync(&requests);
    wizard.highlight(1);

    assert_eq!(
        wizard.confirm(&requests),
        PermissionStep::Answer {
            request_id: 1,
            option_id: Some("reject-1".to_owned()),
        }
    );
    assert_eq!(
        wizard.cancel(&requests),
        PermissionStep::Answer {
            request_id: 2,
            option_id: None,
        }
    );
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn arrow_actions_navigate_completions_before_the_multiline_input(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&view_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(2, 8)
                .default_value("/")
        });
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let controller = cx.new(|_| AgentController::new());
        let mention = mention_picker(&palette_backend(mux.clone()), input.clone(), window, cx);
        let reply = cx.new(|cx| InputState::new(window, cx));
        let search = cx.new(|cx| InputState::new(window, cx));
        let view = cx.new(|_| AgentView {
            pane: PaneId(7),
            mux,
            controller,
            input,
            reply,
            search,
            visible: false,
            pane_state: disconnected_pane_state(),
            chat_revision: 0,
            completion_scroll: UniformListScrollHandle::new(),
            submission_error: None,
            permission_wizard: PermissionWizard::default(),
            question: None,
            plan_open: false,
            attachments: Vec::new(),
            mention,
            completions: vec![
                CommandCompletion {
                    command: command("first"),
                    replacement: 0..1,
                },
                CommandCompletion {
                    command: command("second"),
                    replacement: 0..1,
                },
            ]
            .into(),
            completion_selected: Some(0),
            completion_dismissed: false,
            last_input: "/".to_owned(),
            last_cursor: 1,
            window_corners: WindowCorners::NONE,
            header: None,
            floats: false,
            active: false,
            chat: zz_ui::agent::chat::ChatPane::default(),
            pane_canvas: Rc::default(),
            resume_for: None,
            folder_seen: None,
            resume_revision: 0,
            account: None,
            accounts: None,
            quotas: Vec::new(),
            login: None,
            code: None,
            open_usage: Cell::new(false),
            resume_scroll: ScrollHandle::new(),
            focus_search: false,
            menu_hidden: None,
            browse: None,
            child_requests: HashMap::new(),
            provider_status: None,
            reopen_after_install: false,
            _subscriptions: Vec::new(),
        });
        view.read(cx).focus(cx).focus(window, cx);
        captured.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let view = view_slot.borrow().clone().expect("captured agent view");

    cx.simulate_keystrokes("down");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).completion_selected),
        Some(1)
    );

    cx.simulate_keystrokes("up");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).completion_selected),
        Some(0)
    );

    cx.simulate_keystrokes("tab");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).input.read(cx).value().to_string()),
        "/first "
    );
    assert!(cx.update(|_, cx| view.read(cx).completions.is_empty()));
}

#[cfg(not(target_os = "macos"))]
type RecordedRequests = Rc<RefCell<Vec<(PaneId, crate::mux::client::AgentRequest)>>>;

#[cfg(not(target_os = "macos"))]
fn wired_view(
    cx: &mut TestAppContext,
    pane: PaneId,
) -> (
    Entity<AgentView>,
    Entity<AgentController>,
    RecordedRequests,
    &mut VisualTestContext,
) {
    cx.update(zz_ui::init);
    let slot = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let sink = mux.update(cx, |mux, _| {
            mux.set_agent_client_instance_id_for_test(zz_protocol::ClientInstanceId(1));
            mux.record_agent_requests_for_test()
        });
        let controller = cx.new(|_| AgentController::new());
        controller.update(cx, |controller, _| controller.attach_mux(mux.clone()));
        let view = cx.new(|cx| {
            let mut view = AgentView::new(
                pane,
                AgentProvider::ClaudeCode,
                controller.clone(),
                mux,
                window,
                cx,
            );
            view.set_visible(true, cx);
            view
        });
        view.read(cx).focus(cx).focus(window, cx);
        captured.replace(Some((view.clone(), controller, sink)));
        Root::new(view, window, cx)
    });
    let (view, controller, sink) = slot.borrow_mut().take().expect("captured agent view");
    (view, controller, sink, cx)
}

#[cfg(not(target_os = "macos"))]
fn ask(
    cx: &mut VisualTestContext,
    controller: &Entity<AgentController>,
    pane: PaneId,
    request_id: u64,
    questions: Vec<zz_protocol::agent_stream::AgentQuestion>,
) {
    cx.update(|_, cx| {
        controller.update(cx, |controller, cx| {
            controller.apply_stream_items(
                pane,
                vec![zz_daemon::AgentStreamItem {
                    seq: request_id,
                    payload: zz_daemon::AgentStreamPayload::PermissionRequested {
                        request_id,
                        tool_call: serde_json::json!({
                            "toolCallId": format!("ask-{request_id}"),
                            "title": "Claude has questions",
                        }),
                        options: serde_json::json!([]),
                        questions,
                    },
                }],
                cx,
            );
        });
    });
    cx.run_until_parked();
}

#[cfg(not(target_os = "macos"))]
fn asked(
    id: &str,
    labels: &[&str],
    multi_select: bool,
) -> zz_protocol::agent_stream::AgentQuestion {
    zz_protocol::agent_stream::AgentQuestion {
        id: id.to_owned(),
        question: format!("{id}?"),
        options: labels
            .iter()
            .map(|label| zz_protocol::agent_stream::AgentQuestionOption {
                label: (*label).to_owned(),
                description: None,
            })
            .collect(),
        multi_select,
        allow_other: true,
        ..Default::default()
    }
}

#[cfg(not(target_os = "macos"))]
fn reply(id: &str, answers: &[&str]) -> zz_protocol::AgentQuestionAnswer {
    zz_protocol::AgentQuestionAnswer {
        id: id.to_owned(),
        answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
    }
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn a_question_card_answers_choices_and_typed_text_from_the_keyboard(cx: &mut TestAppContext) {
    let pane = PaneId(41);
    let (view, controller, sink, cx) = wired_view(cx, pane);
    ask(
        cx,
        &controller,
        pane,
        1,
        vec![
            asked("fruit", &["apple", "pear"], false),
            asked("tools", &["saw", "drill"], true),
        ],
    );
    assert!(cx.update(|_, cx| view.read(cx).question.is_some()));

    cx.simulate_keystrokes("2 1 3");
    assert!(
        cx.update(|_, cx| view.read(cx).input.read(cx).value().is_empty()),
        "the card keeps its digits out of the composer"
    );
    cx.simulate_input("chisel");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        sink.borrow().as_slice(),
        &[(
            pane,
            crate::mux::client::AgentRequest::AnswerQuestion {
                request_id: 1,
                answers: vec![
                    reply("fruit", &["pear"]),
                    reply("tools", &["saw", "chisel"])
                ],
            }
        )]
    );
    assert!(cx.update(|_, cx| view.read(cx).question.is_none()));
    assert!(
        cx.update(|window, cx| view.read(cx).focus(cx).is_focused(window)),
        "the composer takes the keyboard back"
    );
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn one_plain_question_answers_on_its_digit_and_escape_dismisses(cx: &mut TestAppContext) {
    let pane = PaneId(42);
    let (_view, controller, sink, cx) = wired_view(cx, pane);
    ask(
        cx,
        &controller,
        pane,
        1,
        vec![asked("fruit", &["apple", "pear"], false)],
    );
    cx.simulate_keystrokes("1");
    ask(
        cx,
        &controller,
        pane,
        2,
        vec![asked("tools", &["saw"], true)],
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert_eq!(
        sink.borrow().as_slice(),
        &[
            (
                pane,
                crate::mux::client::AgentRequest::AnswerQuestion {
                    request_id: 1,
                    answers: vec![reply("fruit", &["apple"])],
                }
            ),
            (
                pane,
                crate::mux::client::AgentRequest::RespondPermission {
                    request_id: 2,
                    option_id: None,
                }
            ),
        ]
    );
}

#[cfg(not(target_os = "macos"))]
fn stream(
    cx: &mut VisualTestContext,
    controller: &Entity<AgentController>,
    pane: PaneId,
    items: Vec<(u64, zz_daemon::AgentStreamPayload)>,
) {
    cx.update(|_, cx| {
        controller.update(cx, |controller, cx| {
            controller.apply_stream_items(
                pane,
                items
                    .into_iter()
                    .map(|(seq, payload)| zz_daemon::AgentStreamItem { seq, payload })
                    .collect(),
                cx,
            );
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

#[cfg(not(target_os = "macos"))]
fn sent_answers(sink: &RecordedRequests) -> Vec<crate::mux::client::AgentRequest> {
    sink.borrow()
        .iter()
        .map(|(_, request)| request.clone())
        .filter(|request| {
            matches!(
                request,
                crate::mux::client::AgentRequest::AnswerQuestion { .. }
                    | crate::mux::client::AgentRequest::RespondPermission { .. }
                    | crate::mux::client::AgentRequest::Prompt { .. }
            )
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn ready_with_an_image(
    cx: &mut VisualTestContext,
    view: &Entity<AgentView>,
    controller: &Entity<AgentController>,
    pane: PaneId,
) {
    stream(
        cx,
        controller,
        pane,
        vec![
            (
                1,
                zz_daemon::AgentStreamPayload::Ready {
                    agent_name: "Claude Code".to_owned(),
                    agent_key: "claude".to_owned(),
                    auth_methods: Vec::new(),
                    capabilities: zz_protocol::agent_stream::AgentSessionCapabilities {
                        images: true,
                        ..Default::default()
                    },
                },
            ),
            (
                2,
                zz_daemon::AgentStreamPayload::SessionReady {
                    session_id: "s-1".to_owned(),
                    modes: None,
                    config_options: None,
                },
            ),
        ],
    );
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.attachments.push(Arc::new(Image::from_bytes(
                ala::ImageFormat::Png,
                vec![0x89, b'P', b'N', b'G'],
            )));
        });
    });
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn one_enter_answers_the_card_and_nothing_behind_it(cx: &mut TestAppContext) {
    let pane = PaneId(45);
    let (view, controller, sink, cx) = wired_view(cx, pane);
    ready_with_an_image(cx, &view, &controller, pane);
    ask(
        cx,
        &controller,
        pane,
        3,
        vec![
            asked("fruit", &["apple", "pear"], false),
            asked("tools", &["saw", "drill"], true),
        ],
    );
    stream(
        cx,
        &controller,
        pane,
        vec![(
            4,
            zz_daemon::AgentStreamPayload::PermissionRequested {
                request_id: 4,
                tool_call: serde_json::json!({"toolCallId": "bash-1", "title": "Run ls"}),
                options: serde_json::json!([
                    {"optionId": "allow-once", "name": "Allow once", "kind": "allow_once"},
                ]),
                questions: Vec::new(),
            },
        )],
    );

    cx.simulate_keystrokes("2 1 enter");
    cx.run_until_parked();

    assert_eq!(
        sent_answers(&sink),
        [crate::mux::client::AgentRequest::AnswerQuestion {
            request_id: 3,
            answers: vec![reply("fruit", &["pear"]), reply("tools", &["saw"])],
        }],
        "the Enter that answers the card neither approves the next request nor sends the images"
    );
    assert_eq!(
        cx.update(|_, cx| view
            .read(cx)
            .pane_state
            .pending_permissions
            .iter()
            .map(|request| request.request_id)
            .collect::<Vec<_>>()),
        [4]
    );
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn the_enter_that_answers_a_card_keeps_the_attached_images(cx: &mut TestAppContext) {
    let pane = PaneId(48);
    let (view, controller, sink, cx) = wired_view(cx, pane);
    ready_with_an_image(cx, &view, &controller, pane);
    ask(
        cx,
        &controller,
        pane,
        3,
        vec![asked("fruit", &["apple"], true)],
    );

    cx.simulate_keystrokes("1 enter");
    cx.run_until_parked();

    assert_eq!(
        sent_answers(&sink),
        [crate::mux::client::AgentRequest::AnswerQuestion {
            request_id: 3,
            answers: vec![reply("fruit", &["apple"])],
        }]
    );
    assert_eq!(cx.update(|_, cx| view.read(cx).attachments.len()), 1);
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn a_path_in_the_folder_search_lists_that_folder_and_tab_goes_deeper(cx: &mut TestAppContext) {
    use zz_protocol::{InsertStyle, PathEntry, PathKind, PathListRoot, ProtocolMessage};

    let pane = PaneId(51);
    let (view, controller, _sink, cx) = wired_view(cx, pane);
    ready_with_an_image(cx, &view, &controller, pane);
    let asked = cx.update(|_, cx| {
        view.read(cx)
            .mux
            .clone()
            .update(cx, |mux, _| mux.record_path_lists_for_test())
    });
    let names = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .resume(cx)
                .0
                .into_iter()
                .map(|row| match row {
                    ResumeRow::Folder { cwd, .. } => cwd.to_string_lossy().into_owned(),
                    row => format!("{row:?}"),
                })
                .collect::<Vec<_>>()
        })
    };
    let host = |cx: &mut VisualTestContext, message: ProtocolMessage| {
        cx.update(|_, cx| {
            view.read(cx)
                .mux
                .clone()
                .update(cx, |mux, cx| mux.handle_message_for_test(message, cx));
        });
        cx.run_until_parked();
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_menu(Menu::Folders, window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    cx.run_until_parked();

    cx.simulate_input("~/dev/");
    cx.run_until_parked();
    let list = |request_id, dir: &str| ProtocolMessage::PathListRequest {
        request_id,
        pane: None,
        dir: Some(dir.to_owned()),
    };
    assert_eq!(
        &*asked.borrow(),
        &[
            list(1, "~/"),
            ProtocolMessage::PathListCancel { request_id: 1 },
            list(2, "~/dev/"),
        ]
    );
    assert_eq!(names(cx), ["Loading"]);

    host(
        cx,
        ProtocolMessage::PathListBegin {
            request_id: 2,
            result: Ok(PathListRoot {
                root: "/home/me/dev".to_owned(),
                display_root: "~/dev/".to_owned(),
                cwd: None,
                insert: InsertStyle::Claude,
            }),
        },
    );
    let entry = |rel: &str| PathEntry {
        rel: rel.to_owned(),
        kind: PathKind::Dir,
        symlink: false,
    };
    host(
        cx,
        ProtocolMessage::PathListChunk {
            request_id: 2,
            entries: vec![entry("zz"), entry("ala"), entry("ala/src")],
            done: false,
            truncated: false,
        },
    );
    assert_eq!(
        names(cx),
        ["/home/me/dev", "/home/me/dev/ala", "/home/me/dev/zz"]
    );
    assert_eq!(
        asked.borrow().last(),
        Some(&ProtocolMessage::PathListCancel { request_id: 2 })
    );

    cx.simulate_keystrokes("down tab");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).search.read(cx).value().to_string()),
        "~/dev/ala/"
    );
    assert_eq!(asked.borrow().last(), Some(&list(3, "~/dev/ala/")));
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn a_card_resolved_elsewhere_hands_the_keyboard_back(cx: &mut TestAppContext) {
    let pane = PaneId(46);
    let (view, controller, _sink, cx) = wired_view(cx, pane);
    ask(
        cx,
        &controller,
        pane,
        1,
        vec![asked("fruit", &["apple", "pear"], false)],
    );
    cx.simulate_keystrokes("3");
    assert!(cx.update(|window, cx| {
        !view
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    assert!(cx.update(|window, cx| {
        view.read(cx)
            .question
            .as_ref()
            .is_some_and(|card| card.editing(window, cx))
    }));

    stream(
        cx,
        &controller,
        pane,
        vec![(
            2,
            zz_daemon::AgentStreamPayload::PermissionResolved {
                request_id: 1,
                canceled: true,
            },
        )],
    );

    assert!(cx.update(|_, cx| view.read(cx).question.is_none()));
    assert!(cx.update(|window, cx| view.read(cx).focus(cx).is_focused(window)));
}
