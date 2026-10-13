use std::collections::BTreeMap;

use crate::*;

fn pane(summary: PaneSummary) -> PaneSnapshot {
    PaneSnapshot {
        id: PaneId(7),
        title: "title".to_owned(),
        kind: PaneKindSnapshot::Terminal,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary,
    }
}

fn filled() -> PaneSummary {
    PaneSummary {
        changed_at: 300,
        unseen: true,
        command: Some(ShellCommand::Finished(2)),
        name: "rv".to_owned(),
        cwd: "~".to_owned(),
        repo: "~".to_owned(),
        branch: "main".to_owned(),
    }
}

fn snapshot(pane: PaneSnapshot) -> MuxSnapshot {
    let id = pane.id;
    MuxSnapshot {
        generation: 1,
        focused_window: None,
        sessions: vec![SessionSnapshot {
            id: SessionId(3),
            name: "session".to_owned(),
            active_window: WindowId(5),
            viewers: Vec::new(),
            windows: vec![WindowSnapshot {
                id: WindowId(5),
                index: 0,
                name: "window".to_owned(),
                automatic_rename: true,
                active_pane: id,
                zoomed_pane: None,
                layout: LayoutNode::Pane(id),
                panes: BTreeMap::from([(id, pane)]),
                layout_dump: String::new(),
                visible_layout_dump: String::new(),
                status_label: String::new(),
                activity: false,
                silence: false,
                pane_border_status: PaneBorderStatus::Off,
                pane_border_lines: PaneBorderLines::Single,
                pane_border_indicators: PaneBorderIndicators::Colour,
                pane_order: vec![id],
                pane_z_order: vec![id],
                floating: Vec::new(),
                modal: None,
                sx: 80,
                sy: 24,
            }],
        }],
    }
}

fn status(app: String, title: String, message: String) -> PaneStatus {
    PaneStatus {
        state: PaneStatusState::Blocked,
        kind: Some(PaneStatusKind::Permission),
        progress: None,
        app,
        title,
        message,
    }
}

const SUMMARY_TAIL: [u8; 19] = [
    0, 172, 2, 1, 1, 1, 2, 2, b'r', b'v', 1, b'~', 1, b'~', 4, b'm', b'a', b'i', b'n',
];

#[test]
fn pane_snapshot_appends_summary_after_status() {
    let pane = pane(filled());
    let bytes = postcard::to_stdvec(&pane).unwrap();
    assert!(bytes.ends_with(&SUMMARY_TAIL));
    assert_eq!(postcard::from_bytes::<PaneSnapshot>(&bytes).unwrap(), pane);

    let mut json = serde_json::to_value(&pane).unwrap();
    json.as_object_mut().unwrap().remove("summary");
    let old = serde_json::from_value::<PaneSnapshot>(json).unwrap();
    assert_eq!(old.summary, PaneSummary::default());

    let partial = serde_json::from_str::<PaneSummary>(r#"{"unseen":true}"#).unwrap();
    assert_eq!(
        partial,
        PaneSummary {
            unseen: true,
            ..PaneSummary::default()
        }
    );
}

#[test]
fn tree_op_pane_summary_holds_tag_eighteen() {
    let op = TreeOp::PaneSummary {
        session: SessionId(3),
        window: WindowId(5),
        pane: PaneId(7),
        summary: filled(),
    };
    let bytes = postcard::to_stdvec(&op).unwrap();
    assert_eq!(bytes[0], 18);
    assert!(bytes.ends_with(&SUMMARY_TAIL[1..]));
    assert_eq!(postcard::from_bytes::<TreeOp>(&bytes).unwrap(), op);
}

#[test]
fn a_summary_change_alone_sends_one_pane_summary_op() {
    let before = snapshot(pane(filled()));
    let mut after = snapshot(pane(PaneSummary {
        changed_at: 301,
        ..filled()
    }));
    after.generation = 2;
    let delta = TreeDelta::between(&before, &after);
    assert_eq!(
        delta.ops,
        vec![TreeOp::PaneSummary {
            session: SessionId(3),
            window: WindowId(5),
            pane: PaneId(7),
            summary: PaneSummary {
                changed_at: 301,
                ..filled()
            },
        }]
    );
    let mut applied = before;
    delta.apply(&mut applied).unwrap();
    assert_eq!(applied, after);
}

#[test]
fn pane_summary_text_decode_is_bounded() {
    let cases: [(usize, fn(&mut PaneSummary, String)); 4] = [
        (MAX_PANE_NAME_BYTES, |summary, text| summary.name = text),
        (MAX_PANE_CWD_BYTES, |summary, text| summary.cwd = text),
        (MAX_PANE_CWD_BYTES, |summary, text| summary.repo = text),
        (MAX_PANE_BRANCH_BYTES, |summary, text| summary.branch = text),
    ];
    for (limit, set) in cases {
        let mut boundary = filled();
        set(&mut boundary, "x".repeat(limit));
        let bytes = postcard::to_stdvec(&pane(boundary.clone())).unwrap();
        assert_eq!(
            postcard::from_bytes::<PaneSnapshot>(&bytes)
                .unwrap()
                .summary,
            boundary
        );

        let mut oversized = filled();
        set(&mut oversized, "x".repeat(limit + 1));
        let bytes = postcard::to_stdvec(&pane(oversized)).unwrap();
        assert!(postcard::from_bytes::<PaneSnapshot>(&bytes).is_err());
    }
}

#[test]
fn pane_status_text_decode_is_bounded() {
    let text = |len: usize| "x".repeat(len);
    let boundary = status(
        text(MAX_PANE_STATUS_APP_BYTES),
        text(MAX_PANE_STATUS_TITLE_BYTES),
        text(MAX_PANE_STATUS_MESSAGE_BYTES),
    );
    let bytes = postcard::to_stdvec(&boundary).unwrap();
    assert_eq!(
        postcard::from_bytes::<PaneStatus>(&bytes).unwrap(),
        boundary
    );

    for oversized in [
        status(
            text(MAX_PANE_STATUS_APP_BYTES + 1),
            String::new(),
            String::new(),
        ),
        status(
            String::new(),
            text(MAX_PANE_STATUS_TITLE_BYTES + 1),
            String::new(),
        ),
        status(
            String::new(),
            String::new(),
            text(MAX_PANE_STATUS_MESSAGE_BYTES + 1),
        ),
    ] {
        let bytes = postcard::to_stdvec(&oversized).unwrap();
        assert!(postcard::from_bytes::<PaneStatus>(&bytes).is_err());
        let mut pane = pane(PaneSummary::default());
        pane.status = Some(oversized);
        let bytes = postcard::to_stdvec(&pane).unwrap();
        assert!(postcard::from_bytes::<PaneSnapshot>(&bytes).is_err());
    }
}

#[test]
fn welcome_carries_the_daemon_clock() {
    let capabilities = CAPABILITY_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let welcome = Welcome {
        protocol_version: PROTOCOL_VERSION,
        server_id: u64::MAX,
        client_id: ClientId(42),
        client_instance_id: ClientInstanceId(u64::MAX),
        caps: Welcome::caps_from_strings(&capabilities),
        clock: 1_760_000_000,
    };
    let message = ProtocolMessage::Welcome(welcome);
    let encoded = encode_protocol_message(&message).unwrap();
    assert!(encoded.len() <= 48, "welcome bytes: {}", encoded.len());
    let ProtocolMessage::Welcome(decoded) = decode_protocol_frame(&encoded).unwrap() else {
        panic!("a welcome decodes as a welcome");
    };
    assert_eq!(decoded.clock, 1_760_000_000);
    assert_eq!(decoded, welcome);

    let mut json = serde_json::to_value(welcome).unwrap();
    json.as_object_mut().unwrap().remove("clock");
    assert_eq!(serde_json::from_value::<Welcome>(json).unwrap().clock, 0);
}
