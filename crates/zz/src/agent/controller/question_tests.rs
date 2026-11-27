use ala::TestAppContext;

use super::test_support::*;
use super::*;

fn asked(id: &str, labels: &[&str], multi_select: bool) -> AgentQuestion {
    AgentQuestion {
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
        ..AgentQuestion::default()
    }
}

#[ala::test]
fn a_question_card_answers_through_its_own_request(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(8);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![item(
                    1,
                    AgentStreamPayload::PermissionRequested {
                        request_id: 21,
                        tool_call: serde_json::json!({
                            "toolCallId": "ask-1",
                            "title": "Claude has questions",
                        }),
                        options: serde_json::json!([]),
                        questions: vec![
                            asked("fruit", &["apple", "pear"], false),
                            asked("tools", &["saw", "drill"], true),
                        ],
                    },
                )],
                cx,
            );
            let pending = controller
                .pane_state(pane)
                .expect("the pane is registered")
                .pending_permissions;
            assert_eq!(pending[0].questions.len(), 2);
            assert!(pending[0].options.is_empty());

            let answers = vec![
                reply("fruit", &["pear"]),
                reply("tools", &["saw", "chisel"]),
            ];
            assert!(controller.answer_question(pane, 21, answers.clone(), cx));
            assert!(
                controller
                    .pane_state(pane)
                    .expect("the pane is registered")
                    .pending_permissions
                    .is_empty(),
                "an answered card leaves this client at once"
            );
            assert_eq!(
                sink.borrow().as_slice(),
                &[(
                    pane,
                    AgentRequest::AnswerQuestion {
                        request_id: 21,
                        answers,
                    }
                )]
            );
        });
    });
}

#[ala::test]
fn published_tasks_and_a_parked_card_reach_the_pane(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(9);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let questions = vec![asked("fruit", &["apple"], true)];
            let state = AgentPaneWire {
                phase: AgentConnectionPhase::AwaitingPermission,
                pending_permission: Some(zz_protocol::AgentPermissionWire {
                    request_id: 4,
                    payload: serde_json::json!({
                        "toolCall": {"toolCallId": "ask-2", "title": "fruit?"},
                        "options": [],
                        "questions": questions,
                    })
                    .to_string(),
                }),
                tasks: vec![AgentTaskWire {
                    id: "b1".to_owned(),
                    kind: "shell".to_owned(),
                    description: "cargo build".to_owned(),
                    tool_call_id: Some("bash-1".to_owned()),
                }],
                ..AgentPaneWire::default()
            };

            controller.apply_pane_state(pane, &state, cx);

            let pane_state = controller.pane_state(pane).expect("the pane is registered");
            assert_eq!(
                pane_state.pending_permissions[0].questions, questions,
                "a client that attaches mid-card still sees every question"
            );
            assert_eq!(pane_state.tasks.as_ref(), state.tasks.as_slice());

            controller.apply_pane_state(
                pane,
                &AgentPaneWire {
                    phase: AgentConnectionPhase::Ready,
                    ..AgentPaneWire::default()
                },
                cx,
            );
            let pane_state = controller.pane_state(pane).expect("the pane is registered");
            assert!(pane_state.tasks.is_empty());
            assert!(pane_state.pending_permissions.is_empty());
        });
    });
}

#[ala::test]
fn a_card_the_daemon_keeps_past_the_turn_stays_after_it_ends(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(10);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let parked = |request_id| AgentPaneWire {
                phase: AgentConnectionPhase::Ready,
                pending_permission: Some(zz_protocol::AgentPermissionWire {
                    request_id,
                    payload: serde_json::json!({
                        "toolCall": {"toolCallId": "ask-3", "title": "fruit?"},
                        "options": [],
                        "questions": [asked("fruit", &["apple"], false)],
                    })
                    .to_string(),
                }),
                ..AgentPaneWire::default()
            };
            let pending = |controller: &AgentController| {
                controller
                    .pane_state(pane)
                    .expect("the pane is registered")
                    .pending_permissions
                    .iter()
                    .map(|request| request.request_id)
                    .collect::<Vec<_>>()
            };

            controller.apply_pane_state(pane, &parked(6), cx);
            controller.apply_stream_items(
                pane,
                vec![item(1, turn_finished(StopReason::EndTurn))],
                cx,
            );
            assert_eq!(pending(controller), [6]);

            controller.apply_pane_state(
                pane,
                &AgentPaneWire {
                    phase: AgentConnectionPhase::Ready,
                    ..AgentPaneWire::default()
                },
                cx,
            );
            controller.apply_stream_items(
                pane,
                vec![item(2, turn_finished(StopReason::EndTurn))],
                cx,
            );
            assert_eq!(pending(controller), Vec::<u64>::new());
        });
    });
}

#[ala::test]
fn an_answered_card_stays_gone_when_an_older_state_still_holds_it(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(11);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            let holding = |request_id| AgentPaneWire {
                phase: AgentConnectionPhase::AwaitingPermission,
                pending_permission: Some(zz_protocol::AgentPermissionWire {
                    request_id,
                    payload: serde_json::json!({
                        "toolCall": {"toolCallId": "ask-4", "title": "fruit?"},
                        "options": [],
                        "questions": [asked("fruit", &["apple"], false)],
                    })
                    .to_string(),
                }),
                ..AgentPaneWire::default()
            };
            let pending = |controller: &AgentController| {
                controller
                    .pane_state(pane)
                    .expect("the pane is registered")
                    .pending_permissions
                    .iter()
                    .map(|request| request.request_id)
                    .collect::<Vec<_>>()
            };

            controller.apply_pane_state(pane, &holding(6), cx);
            assert_eq!(pending(controller), [6]);
            assert!(controller.answer_question(pane, 6, vec![reply("fruit", &["apple"])], cx));
            assert_eq!(pending(controller), Vec::<u64>::new());

            controller.apply_pane_state(pane, &holding(6), cx);
            assert_eq!(pending(controller), Vec::<u64>::new());
            controller.apply_stream_items(
                pane,
                vec![item(1, turn_finished(StopReason::EndTurn))],
                cx,
            );
            assert_eq!(pending(controller), Vec::<u64>::new());

            controller.apply_pane_state(pane, &holding(7), cx);
            assert_eq!(pending(controller), [7]);
        });
    });
}
