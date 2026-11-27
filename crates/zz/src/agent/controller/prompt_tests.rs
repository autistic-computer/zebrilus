use ala::TestAppContext;

use super::test_support::*;
use super::*;

#[ala::test]
fn a_prompt_reaches_the_wire_with_normalized_attachments(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(6);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller
                .prompt(pane, "  ship it  ", vec![attachment()], cx)
                .expect("the prompt is accepted");
        });
    });

    assert_eq!(
        &*sink.borrow(),
        &[(
            pane,
            AgentRequest::Prompt {
                text: "ship it".to_owned(),
                images: vec![zz_protocol::AgentImage {
                    format: "image/png".to_owned(),
                    data: PNG_PIXEL.to_vec(),
                }],
            }
        )],
        "the daemon receives bytes plus format, never a ala image"
    );
}

#[ala::test]
fn a_folder_pick_starts_a_chat_there_and_the_pane_follows_the_reply(cx: &mut TestAppContext) {
    use zz_client::agent_resume::{Menu, ResumeRow};
    use zz_daemon::AgentSessionSummary;
    use zz_protocol::AgentSessionOpKind;

    let list = |cwd: Option<PathBuf>| {
        AgentRequest::SessionOp(AgentSessionOpKind::List {
            cwd,
            cursor: None,
            replace: true,
        })
    };
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(8);
    let penna = PathBuf::from("/src/penna");
    let cwd = cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.request_resume(pane, cx);
            controller.panes[&pane].cwd.clone()
        })
    });
    assert_eq!(
        &*sink.borrow(),
        &[(pane, list(Some(cwd.clone()))), (pane, list(None))]
    );
    sink.borrow_mut().clear();

    let reply = |sessions, cwd_filter| {
        serde_json::to_string(&AgentStreamPayload::SessionsListed {
            client: zz_protocol::ClientId(1),
            sessions,
            next_cursor: None,
            cwd_filter,
            replace: true,
        })
        .expect("reply")
    };
    let chat = AgentSessionSummary {
        session_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        cwd: penna.clone(),
        additional_directories: Vec::new(),
        title: Some("Focus ring".to_owned()),
        updated_at: None,
    };
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.apply_sessions(pane, &reply(Vec::new(), Some(cwd.clone())), cx);
            controller.apply_sessions(pane, &reply(vec![chat], None), cx);
            controller.update_resume(pane, cx, |resume, _| {
                resume.open(Menu::Folders);
                None
            });
            let rows = controller.panes[&pane].resume.rows(&cwd, None, 0);
            assert!(matches!(&rows[1], ResumeRow::Folder { cwd, .. } if *cwd == penna));
            controller.update_resume(pane, cx, |resume, cwd| resume.pick(&rows[1], cwd));
        });
    });
    assert_eq!(
        &*sink.borrow(),
        &[(
            pane,
            AgentRequest::SessionOp(AgentSessionOpKind::New { cwd: penna.clone() })
        )]
    );
    sink.borrow_mut().clear();

    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.apply_stream_items(
                pane,
                vec![item(
                    1,
                    AgentStreamPayload::SessionSwitched {
                        session_id: "s-new".to_owned(),
                        cwd: penna.clone(),
                        modes: None,
                        config_options: None,
                        replay: Vec::new(),
                    },
                )],
                cx,
            );
            assert_eq!(controller.pane_cwd(pane), Some(penna.as_path()));
            assert_eq!(controller.moves(), 1);
            assert_eq!(controller.panes[&pane].resume.menu(), None);
            controller.move_pane(pane, PathBuf::from("/src/nidus"), cx);
            controller.move_pane(pane, penna.clone(), cx);
            controller.move_pane(pane, penna.clone(), cx);
            assert_eq!(controller.pane_cwd(pane), Some(penna.as_path()));
            assert_eq!(controller.moves(), 3);
            controller.request_resume(pane, cx);
        });
    });
    assert_eq!(
        &*sink.borrow(),
        &[(pane, list(Some(penna))), (pane, list(None))]
    );
}

#[ala::test]
fn a_prompt_typed_mid_turn_is_queued_by_the_daemon(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(7);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller
                .prompt(pane, "first", Vec::new(), cx)
                .expect("first prompt");
            controller
                .prompt(pane, "second", Vec::new(), cx)
                .expect("second prompt is queued rather than refused");

            assert_eq!(controller.queued_count(pane), 1);
            controller.unqueue_prompts(pane, cx);
        });
    });

    assert_eq!(
        sink.borrow()
            .iter()
            .map(|(_, request)| request.clone())
            .collect::<Vec<_>>(),
        vec![
            AgentRequest::Prompt {
                text: "first".to_owned(),
                images: Vec::new(),
            },
            AgentRequest::Prompt {
                text: "second".to_owned(),
                images: Vec::new(),
            },
            AgentRequest::Unqueue,
        ]
    );
}

#[ala::test]
fn a_zz_command_opens_no_turn_and_joins_no_queue(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(7);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let thread = controller.panes.get_mut(&pane).expect("pane");
            thread.session_capabilities.verbs = true;
            controller
                .prompt(pane, "/rewind u-1", Vec::new(), cx)
                .expect("the command is sent");
            let thread = &controller.panes[&pane];
            assert_eq!(thread.connection, AgentConnectionState::Ready);
            assert!(
                thread.transcript.entries().is_empty(),
                "the daemon echoes the command itself"
            );
            controller
                .prompt(pane, "first", Vec::new(), cx)
                .expect("first prompt");
            controller
                .prompt(pane, "/steer faster", Vec::new(), cx)
                .expect("a command goes out mid-turn");
            assert_eq!(controller.queued_count(pane), 0);
            let thread = controller.panes.get_mut(&pane).expect("pane");
            thread.session_capabilities.verbs = false;
            controller
                .prompt(pane, "/steer later", Vec::new(), cx)
                .expect("a plain prompt is queued");
            assert_eq!(controller.queued_count(pane), 1);
        });
    });

    assert_eq!(
        sink.borrow()
            .iter()
            .map(|(_, request)| match request {
                AgentRequest::Prompt { text, .. } => text.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>(),
        ["/rewind u-1", "first", "/steer faster", "/steer later"]
    );
}
