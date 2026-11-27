use agent_client_protocol::v1::schema::{
    PermissionOptionKind, ToolCall, ToolCallStatus, ToolCallUpdateFields, ToolKind,
};

use ala::TestAppContext;

use super::test_support::*;
use super::*;

#[ala::test]
fn published_pane_state_drives_the_composer_and_the_wizard(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(5);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.panes.get_mut(&pane).expect("pane").session_id = Some(Arc::from("s-9"));
            let state = AgentPaneWire {
                phase: AgentConnectionPhase::AwaitingPermission,
                queued_prompts: 2,
                session_id: Some("s-9".to_owned()),
                title: Some("ship it".to_owned()),
                error: None,
                auth_methods: serde_json::to_string(&vec![zz_daemon::AgentAuthMethod {
                    id: "api".to_owned(),
                    name: "API key".to_owned(),
                    description: None,
                }])
                .expect("auth methods encode"),
                config_options: String::new(),
                modes: String::new(),
                pending_permission: Some(zz_protocol::AgentPermissionWire {
                    request_id: 11,
                    payload: serde_json::json!({
                        "toolCall": json(&ToolCallUpdate::new(
                            "tool-9",
                            ToolCallUpdateFields::new(),
                        )),
                        "options": json(&vec![PermissionOption::new(
                            "allow-once",
                            "Allow once",
                            PermissionOptionKind::AllowOnce,
                        )]),
                    })
                    .to_string(),
                }),
                git: Some(AgentGitSummary {
                    branch: Some("main".to_owned()),
                    changed_files: 3,
                    additions: 21,
                    deletions: 8,
                }),
                tasks: Vec::new(),
            };

            controller.apply_pane_state(pane, &state, cx);

            assert_eq!(controller.queued_count(pane), 2);
            let pane_state = controller.pane_state(pane).expect("the pane is registered");
            assert_eq!(pane_state.queued_prompts, 2);
            assert_eq!(pane_state.session_id.as_deref(), Some("s-9"));
            assert_eq!(pane_state.git, state.git);
            assert_eq!(
                pane_state
                    .pending_permissions
                    .first()
                    .map(|request| request.request_id),
                Some(11),
                "a client that attaches mid-question still sees the wizard"
            );

            controller.apply_pane_state(
                pane,
                &AgentPaneWire {
                    phase: AgentConnectionPhase::Ready,
                    queued_prompts: 0,
                    session_id: None,
                    title: None,
                    error: None,
                    auth_methods: String::new(),
                    config_options: String::new(),
                    modes: String::new(),
                    pending_permission: None,
                    git: None,
                    tasks: Vec::new(),
                },
                cx,
            );
            let pane_state = controller.pane_state(pane).expect("the pane is registered");
            assert_eq!(pane_state.queued_prompts, 0);
            assert_eq!(pane_state.session_id.as_deref(), Some("s-9"));
            assert!(pane_state.pending_permissions.is_empty());
            assert_eq!(pane_state.git, None);
        });
    });
}

#[ala::test]
fn a_turn_the_agent_starts_on_its_own_ends_when_the_daemon_is_ready(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(5);
    let wire = |phase| AgentPaneWire {
        phase,
        queued_prompts: 0,
        session_id: None,
        title: None,
        error: None,
        auth_methods: String::new(),
        config_options: String::new(),
        modes: String::new(),
        pending_permission: None,
        git: None,
        tasks: Vec::new(),
    };
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let connection = |controller: &AgentController| {
                controller.pane_state(pane).expect("pane").connection
            };

            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
            assert_eq!(connection(controller), AgentConnectionState::Running);
            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
            assert_eq!(connection(controller), AgentConnectionState::Ready);

            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
            controller.cancel(pane, cx);
            assert_eq!(connection(controller), AgentConnectionState::Cancelling);
            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
            assert_eq!(connection(controller), AgentConnectionState::Ready);

            controller
                .prompt(pane, "hello", Vec::new(), cx)
                .expect("a ready pane takes a prompt");
            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
            assert_eq!(
                connection(controller),
                AgentConnectionState::Running,
                "a Ready published before the daemon saw the prompt does not end it"
            );
            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
            controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
            assert_eq!(
                connection(controller),
                AgentConnectionState::Ready,
                "once the daemon ran the prompt, its Ready ends the turn without waiting for PromptFinished"
            );
        });
    });
}

#[ala::test]
fn ordered_state_sync_wins_after_a_synthesized_replay(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(16);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(1, AgentStreamPayload::SessionReset { restoring: true }),
                    item(
                        2,
                        AgentStreamPayload::SessionReady {
                            session_id: "s-1".to_owned(),
                            modes: None,
                            config_options: None,
                        },
                    ),
                    item(
                        3,
                        AgentStreamPayload::StateSynced {
                            state: AgentPaneWire {
                                phase: AgentConnectionPhase::Failed {
                                    message: "adapter exited".to_owned(),
                                },
                                session_id: Some("s-1".to_owned()),
                                ..AgentPaneWire::default()
                            },
                        },
                    ),
                ],
                cx,
            );

            let state = controller.pane_state(pane).expect("pane state");
            assert_eq!(state.connection, AgentConnectionState::Failed);
            assert_eq!(state.error.as_deref(), Some("adapter exited"));
            assert_eq!(controller.viewports[&pane].last_applied, 3);
        });
    });
}

#[ala::test]
fn nonfatal_wire_error_survives_the_ordered_ready_state(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(21);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.handle_runtime_event(
                pane,
                RuntimeEvent::SessionSwitchFailed {
                    pane,
                    message: "could not restore".to_owned(),
                },
                cx,
            );
            controller.apply_pane_state(
                pane,
                &AgentPaneWire {
                    phase: AgentConnectionPhase::Ready,
                    error: Some("could not restore".to_owned()),
                    ..AgentPaneWire::default()
                },
                cx,
            );
            assert_eq!(
                controller.pane_state(pane).and_then(|state| state.error),
                Some(Arc::from("could not restore"))
            );
        });
    });
}

#[ala::test]
fn fatal_state_fails_inflight_tools(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(22);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let thread = controller.panes.get_mut(&pane).expect("pane");
            thread.apply_update(SessionUpdate::ToolCall(
                ToolCall::new("task-tool", "Research")
                    .kind(ToolKind::Think)
                    .status(ToolCallStatus::InProgress),
            ));
            controller.apply_pane_state(
                pane,
                &AgentPaneWire {
                    phase: AgentConnectionPhase::Failed {
                        message: "adapter exited".to_owned(),
                    },
                    ..AgentPaneWire::default()
                },
                cx,
            );
            let thread = controller.panes.get(&pane).expect("pane");
            assert!(matches!(
                thread.transcript.entries().first(),
                Some(AgentThreadEntry::Tool {
                    status: AgentToolStatusModel::Failed,
                    ..
                })
            ));
        });
    });
}
