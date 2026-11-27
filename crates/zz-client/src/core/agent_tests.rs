use super::test_support::{drain, event, hello, snapshot_with};
use super::*;
use zz_protocol::AgentConnectionPhase;

fn agent_state(title: &str) -> AgentPaneWire {
    AgentPaneWire {
        phase: AgentConnectionPhase::Running,
        queued_prompts: 2,
        title: Some(title.to_owned()),
        ..AgentPaneWire::default()
    }
}

#[test]
fn agent_state_is_stored_and_notified() {
    let pane = PaneId(7);
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("first"),
    }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::AgentStateChanged {
            pane,
            attention: None,
        }]
    );
    assert_eq!(core.agent_state(pane), Some(&agent_state("first")));

    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("second"),
    }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::AgentStateChanged {
            pane,
            attention: None,
        }]
    );
    assert_eq!(
        core.agent_state(pane).and_then(|state| state.title.clone()),
        Some("second".to_owned())
    );
    assert_eq!(core.agent_state(PaneId(8)), None);
}

#[test]
fn agent_attention_edges_are_lossless_core_events() {
    let pane = PaneId(7);
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("work"),
    }));
    drain(&mut core);

    let ready = AgentPaneWire {
        phase: AgentConnectionPhase::Ready,
        ..agent_state("done")
    };
    core.handle_message(event(EventPayload::AgentState { pane, state: ready }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::AgentStateChanged {
            pane,
            attention: Some(AgentAttentionEdge::Done),
        }]
    );

    let permission = AgentPaneWire {
        phase: AgentConnectionPhase::AwaitingPermission,
        pending_permission: Some(zz_protocol::AgentPermissionWire {
            request_id: 9,
            payload: "{}".to_owned(),
        }),
        ..agent_state("permission")
    };
    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: permission,
    }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::AgentStateChanged {
            pane,
            attention: Some(AgentAttentionEdge::Request),
        }]
    );

    let failed = AgentPaneWire {
        phase: AgentConnectionPhase::Failed {
            message: "boom".to_owned(),
        },
        ..agent_state("failed")
    };
    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: failed,
    }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::AgentStateChanged {
            pane,
            attention: Some(AgentAttentionEdge::Failed),
        }]
    );
}

#[test]
fn agent_stream_payloads_pass_through_without_retention() {
    let pane = PaneId(3);
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::AgentUpdates {
        pane,
        first_seq: 41,
        items: vec![b"{\"kind\":\"chunk\"}".to_vec(), b"{}".to_vec()],
    }));
    core.handle_message(event(EventPayload::AgentLagged { pane, next_seq: 43 }));
    core.handle_message(event(EventPayload::AgentSessions {
        pane,
        request_id: 9,
        result: "[]".to_owned(),
    }));

    assert_eq!(
        drain(&mut core),
        vec![
            CoreEvent::AgentUpdates {
                pane,
                first_seq: 41,
                items: vec![b"{\"kind\":\"chunk\"}".to_vec(), b"{}".to_vec()],
            },
            CoreEvent::AgentLagged { pane, next_seq: 43 },
            CoreEvent::AgentSessions {
                pane,
                request_id: 9,
                result: "[]".to_owned(),
            },
        ]
    );
    assert_eq!(core.agent_state(pane), None);
    assert!(core.poll_outbound().is_none());
}

#[test]
fn providers_report_is_kept_until_the_next_one() {
    let mut core = ClientCore::new();
    let mut status = zz_protocol::ProviderStatus::new(zz_protocol::AgentProvider::Codex);
    status.version = Some("0.163.0".into());
    let report = zz_protocol::ProvidersReport {
        checking: false,
        providers: vec![status],
    };
    core.handle_message(event(EventPayload::Providers(report.clone())));
    assert_eq!(drain(&mut core), vec![CoreEvent::ProvidersChanged]);
    assert_eq!(core.providers(), Some(&report));
}

#[test]
fn agent_state_drops_with_its_pane() {
    let kept = PaneId(1);
    let lost = PaneId(2);
    let mut core = ClientCore::new();
    for pane in [kept, lost] {
        core.handle_message(event(EventPayload::AgentState {
            pane,
            state: agent_state("live"),
        }));
    }

    core.handle_message(event(EventPayload::Snapshot(snapshot_with(&[kept]))));
    assert!(core.agent_state(kept).is_some());
    assert_eq!(core.agent_state(lost), None);

    core.handle_message(event(EventPayload::PaneRemoved(kept)));
    assert_eq!(core.agent_state(kept), None);
}

#[test]
fn reconnect_and_session_reset_clear_agent_state() {
    let pane = PaneId(5);
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("live"),
    }));
    core.reset_session();
    assert_eq!(core.agent_state(pane), None);

    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("live"),
    }));
    core.handle_message(hello());
    assert_eq!(core.agent_state(pane), None);

    core.handle_message(event(EventPayload::AgentState {
        pane,
        state: agent_state("live"),
    }));
    core.handle_message(ProtocolMessage::Attached {
        session: SessionId(0),
        snapshot: snapshot_with(&[pane]),
        read_only: false,
        client_flags: String::new(),
    });
    assert_eq!(core.agent_state(pane), None);
}
