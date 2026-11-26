use serde::Serialize;
use zz_terminal::{
    Modifiers, PointerCellEvent, TerminalMouseButton, TerminalMouseInput, TerminalMousePhase,
};

use super::{AgentAutoApprove, DEFAULT_AGENT_AUTO_APPROVE, MuxOptionKey, MuxOptions};

#[derive(Serialize)]
struct LegacyTerminalMouseInput {
    phase: TerminalMousePhase,
    button: Option<TerminalMouseButton>,
    cell: PointerCellEvent,
    x: u32,
    y: u32,
    screen_width: u32,
    screen_height: u32,
    cell_width: u32,
    cell_height: u32,
    modifiers: Modifiers,
    force_selection: bool,
}

#[test]
fn server_error_tmux_message_strips_only_invalid_command_prefixes() {
    assert_eq!(
        super::ServerError::InvalidCommand("width too small".to_owned()).tmux_message(),
        "width too small"
    );
    assert_eq!(
        super::ServerError::CommandParse("usage: split-window".to_owned()).tmux_message(),
        "usage: split-window"
    );
    assert_eq!(
        super::ServerError::SessionNotFound("missing".to_owned()).tmux_message(),
        "can't find session: missing"
    );
}

#[test]
fn command_parse_error_holds_wire_tag_twelve() {
    let error = super::ServerError::CommandParse("usage: split-window".to_owned());
    let bytes = postcard::to_stdvec(&error).expect("command parse error encodes");
    assert_eq!(bytes[0], 12);
    assert_eq!(
        postcard::from_bytes::<super::ServerError>(&bytes).expect("command parse error decodes"),
        error
    );
    assert!(error.is_command_parse());
    assert!(!super::ServerError::InvalidCommand(String::new()).is_command_parse());
}

#[test]
fn native_usage_errors_preserve_messages_phases_and_wire_tags() {
    use super::ServerError;

    let message = "command list-panes: --json cannot be combined with -F".to_owned();
    let error = ServerError::NativeCommandParse(message.clone());
    let bytes = postcard::to_stdvec(&error).unwrap();
    assert_eq!(bytes[0], 14);
    assert_eq!(postcard::from_bytes::<ServerError>(&bytes).unwrap(), error);
    for error in [
        error.clone(),
        ServerError::PostAdmissionCallback(Box::new(error)),
    ] {
        assert_eq!(error.exit_code(), 2);
        assert_eq!(error.tmux_message(), message);
        assert!(error.is_command_parse());
    }
    let unsupported =
        ServerError::NativeUnsupportedCommand("set-option -a history-trickle".to_owned());
    let bytes = postcard::to_stdvec(&unsupported).unwrap();
    assert_eq!(bytes[0], 15);
    assert_eq!(
        postcard::from_bytes::<ServerError>(&bytes).unwrap(),
        unsupported
    );
    assert_eq!(unsupported.exit_code(), 2);
    assert!(!unsupported.is_command_parse());
    assert_eq!(
        unsupported.tmux_message(),
        "unsupported command: set-option -a history-trickle"
    );
    let invalid = ServerError::NativeInvalidCommand(message.clone());
    let bytes = postcard::to_stdvec(&invalid).unwrap();
    assert_eq!(bytes[0], 16);
    assert_eq!(
        postcard::from_bytes::<ServerError>(&bytes).unwrap(),
        invalid
    );
    assert_eq!(invalid.exit_code(), 2);
    assert_eq!(invalid.tmux_message(), message);
    assert!(!invalid.is_command_parse());
    assert_eq!(ServerError::CommandParse(message).exit_code(), 1);
    assert_eq!(
        ServerError::UnsupportedCommand("clock-mode".to_owned()).exit_code(),
        1
    );
}

#[test]
fn post_admission_callback_holds_wire_tag_thirteen() {
    let error = super::ServerError::PostAdmissionCallback(Box::new(
        super::ServerError::SessionNotFound("missing".to_owned()),
    ));
    let bytes = postcard::to_stdvec(&error).expect("callback error encodes");
    assert_eq!(bytes[0], 13);
    assert_eq!(
        postcard::from_bytes::<super::ServerError>(&bytes).expect("callback error decodes"),
        error
    );
    assert!(error.is_post_admission_callback());
    assert_eq!(error.tmux_message(), "can't find session: missing");
}

#[test]
fn command_invocation_command_blocks_round_trip_and_append() {
    let mut command = super::CommandInvocation::new("bind-key", ["x", "{ first }"])
        .with_command_blocks([1, 1, 9]);
    let appended =
        super::CommandInvocation::new("unused", ["plain", "{ second }"]).with_command_blocks([1]);
    command.append_args(appended);

    assert_eq!(command.args, ["x", "{ first }", "plain", "{ second }"]);
    assert!(!command.argument_is_command_block(0));
    assert!(command.argument_is_command_block(1));
    assert!(!command.argument_is_command_block(2));
    assert!(command.argument_is_command_block(3));
    assert!(!command.argument_is_command_block(4));
    assert_eq!(command.command_blocks, [1, 3]);

    let bytes = postcard::to_stdvec(&command).expect("command invocation encodes");
    let decoded = postcard::from_bytes::<super::CommandInvocation>(&bytes)
        .expect("command invocation decodes");
    assert_eq!(decoded, command);
}

#[test]
fn command_invocation_new_has_no_typed_blocks() {
    let command = super::CommandInvocation::new("display-message", ["{ literal }"]);
    assert!(!command.argument_is_command_block(0));
    assert!(command.command_blocks.is_empty());
}

#[test]
fn command_invocation_split_preserves_typed_positions_across_chains() {
    let command = super::CommandInvocation::new(
        "bind-key",
        [
            "F10",
            "display-message",
            "first",
            ";",
            "if-shell",
            "-F",
            "1",
            "{ display-message -p true }",
            "{ display-message -p false }",
        ],
    )
    .with_command_blocks([7, 8]);

    let commands = command.split_commands_from(1);

    assert_eq!(
        commands,
        [
            super::CommandInvocation::new("display-message", ["first"]),
            super::CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "1",
                    "{ display-message -p true }",
                    "{ display-message -p false }",
                ],
            )
            .with_command_blocks([2, 3]),
        ]
    );
}

#[test]
fn command_invocation_split_omits_typed_command_names() {
    let command = super::CommandInvocation::new(
        "bind-key",
        [
            "F4",
            "display-message",
            "first",
            ";",
            "{ display-message second }",
        ],
    )
    .with_command_blocks([4]);

    assert_eq!(
        command.split_commands_from(1),
        [super::CommandInvocation::new("display-message", ["first"],)]
    );
}

#[test]
fn terminal_modifiers_use_one_validated_control_byte() {
    let modifiers = Modifiers::new(true, true, true, true);
    assert_eq!(postcard::to_stdvec(&modifiers).expect("encode"), [0x0f]);
    assert_eq!(
        postcard::from_bytes::<Modifiers>(&[0x0f]).expect("decode"),
        modifiers
    );
    assert!(postcard::from_bytes::<Modifiers>(&[0x10]).is_err());
}

#[test]
fn packed_terminal_mouse_input_preserves_the_control_wire_layout() {
    let phase = TerminalMousePhase::Motion;
    let button = Some(TerminalMouseButton::Middle);
    let cell = PointerCellEvent {
        column: 41,
        row: 17,
        click_count: 3,
        rectangle: true,
    };
    let modifiers = Modifiers::new(true, false, true, true);
    let compact = TerminalMouseInput::new(
        phase, button, cell, 321, 654, 1_920, 1_080, 9, 18, modifiers, true,
    );
    let legacy = LegacyTerminalMouseInput {
        phase,
        button,
        cell,
        x: 321,
        y: 654,
        screen_width: 1_920,
        screen_height: 1_080,
        cell_width: 9,
        cell_height: 18,
        modifiers,
        force_selection: true,
    };
    let compact_bytes = postcard::to_stdvec(&compact).expect("compact mouse input");
    let legacy_bytes = postcard::to_stdvec(&legacy).expect("legacy mouse input");

    assert_eq!(compact_bytes, legacy_bytes);
    assert_eq!(
        postcard::from_bytes::<TerminalMouseInput>(&compact_bytes).expect("decoded mouse input"),
        compact
    );
}

#[test]
fn agent_runtime_variants_hold_the_wire_tails_they_were_appended_to() {
    let pane = crate::PaneId(1);
    let message_tag = |message: &super::ProtocolMessage| {
        postcard::to_stdvec(message).expect("message encodes")[0]
    };
    let payload_tag = |payload: super::EventPayload| {
        postcard::to_stdvec(&super::Event {
            sequence: 0,
            payload,
        })
        .expect("event encodes")[1]
    };

    assert_eq!(
        message_tag(&super::ProtocolMessage::PastedImageUnavailable { pane, number: 0 }),
        20
    );
    for (index, message) in [
        super::ProtocolMessage::AgentPrompt {
            pane,
            text: String::new(),
            images: Vec::new(),
        },
        super::ProtocolMessage::AgentCancel { pane },
        super::ProtocolMessage::AgentUnqueue { pane },
        super::ProtocolMessage::AgentRespondPermission {
            pane,
            request_id: 0,
            option_id: None,
        },
        super::ProtocolMessage::AgentSetConfigOption {
            pane,
            option_id: String::new(),
            value: String::new(),
        },
        super::ProtocolMessage::AgentSetMode {
            pane,
            mode_id: String::new(),
        },
        super::ProtocolMessage::AgentAuthenticate {
            pane,
            method_id: String::new(),
        },
        super::ProtocolMessage::AgentSessionOp {
            pane,
            op: super::AgentSessionOpKind::List {
                cwd: None,
                cursor: None,
                replace: true,
            },
        },
        super::ProtocolMessage::AgentReplay { pane, from_seq: 0 },
        super::ProtocolMessage::AgentAcknowledgePromptRestore {
            pane,
            reclaim_id: 0,
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            u8::try_from(21 + index).expect("tag"),
            message_tag(&message)
        );
    }

    let invocation = super::CommandInvocation::new("list-sessions", std::iter::empty::<&str>());
    let prepared = super::PreparedCommand {
        invocation: invocation.clone(),
        canonical_name: Some("list-sessions".to_owned()),
        alias_matched: false,
        result: super::PreparedCommandResult::Ready,
    };
    for (message, tag) in [
        (
            super::ProtocolMessage::PrepareCommandList {
                request_id: 7,
                commands: vec![invocation],
            },
            31,
        ),
        (
            super::ProtocolMessage::PreparedCommandList {
                request_id: 7,
                commands: vec![prepared],
            },
            32,
        ),
    ] {
        let bytes = postcard::to_stdvec(&message).expect("prepare message encodes");
        assert_eq!(bytes[0], tag);
        assert_eq!(
            postcard::from_bytes::<super::ProtocolMessage>(&bytes)
                .expect("prepare message decodes"),
            message
        );
    }

    assert_eq!(
        payload_tag(super::EventPayload::KittyImagesRemoved {
            pane,
            image_ids: Vec::new(),
        }),
        28
    );
    for (index, payload) in [
        super::EventPayload::AgentUpdates {
            pane,
            first_seq: 0,
            items: Vec::new(),
        },
        super::EventPayload::AgentState {
            pane,
            state: super::AgentPaneWire::default(),
        },
        super::EventPayload::AgentLagged { pane, next_seq: 0 },
        super::EventPayload::AgentSessions {
            pane,
            request_id: 0,
            result: String::new(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(u8::try_from(29 + index).expect("tag"), payload_tag(payload));
    }
    assert_eq!(
        payload_tag(super::EventPayload::TimedClientMessage {
            pane: None,
            kind: super::ClientMessageKind::Info,
            text: String::new(),
            duration_ms: 0,
            message_id: 0,
        }),
        33
    );
    assert_eq!(
        payload_tag(super::EventPayload::PrefixCancelled { request_id: 0 }),
        34
    );
}

#[test]
fn recent_mux_options_are_appended_with_their_defaults() {
    assert_eq!(
        postcard::to_stdvec(&MuxOptionKey::HistoryTrickle).expect("history option"),
        [9]
    );
    assert_eq!(
        MuxOptionKey::from_config_key("history-trickle"),
        Some(MuxOptionKey::HistoryTrickle)
    );
    assert_eq!(
        MuxOptions::default()
            .get(MuxOptionKey::HistoryTrickle)
            .expect("history trickle default")
            .value,
        "2000"
    );
    assert_eq!(
        postcard::to_stdvec(&MuxOptionKey::AgentAutoApprove).expect("agent option"),
        [12]
    );
    assert_eq!(
        MuxOptionKey::from_config_key("agent-command"),
        Some(MuxOptionKey::AgentCommand)
    );
    assert_eq!(
        MuxOptions::default()
            .get(MuxOptionKey::AgentCommand)
            .expect("agent command default")
            .value,
        super::DEFAULT_AGENT_COMMAND
    );
    assert_eq!(
        MuxOptions::default()
            .get(MuxOptionKey::AgentAutoApprove)
            .expect("agent auto-approve default")
            .value,
        "reads"
    );
}

#[test]
fn the_auto_approve_tiers_keep_the_flag_spellings_they_shipped_with() {
    assert_eq!(AgentAutoApprove::parse("on"), Some(AgentAutoApprove::All));
    assert_eq!(AgentAutoApprove::parse("off"), Some(AgentAutoApprove::Off));
    assert_eq!(
        AgentAutoApprove::parse("FALSE"),
        Some(AgentAutoApprove::Off)
    );
    assert_eq!(AgentAutoApprove::parse("1"), Some(AgentAutoApprove::All));
    assert_eq!(
        AgentAutoApprove::parse("reads"),
        Some(AgentAutoApprove::Reads)
    );
    assert_eq!(AgentAutoApprove::parse("all"), Some(AgentAutoApprove::All));
    assert_eq!(AgentAutoApprove::parse("sometimes"), None);
    assert_eq!(AgentAutoApprove::default(), DEFAULT_AGENT_AUTO_APPROVE);
}

#[test]
fn popup_variants_hold_the_appended_wire_tails() {
    let input = super::InputMessage::RetiredPopup {
        action: super::PopupAction::Close,
    };
    let input_bytes = postcard::to_stdvec(&input).expect("popup input encodes");
    assert_eq!(input_bytes[0], 14);
    assert_eq!(
        postcard::from_bytes::<super::InputMessage>(&input_bytes).expect("popup input decodes"),
        input
    );

    let event = super::Event {
        sequence: 7,
        payload: super::EventPayload::RetiredPopup { state: None },
    };
    let event_bytes = postcard::to_stdvec(&event).expect("popup event encodes");
    assert_eq!(event_bytes[1], 35);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&event_bytes).expect("popup event decodes"),
        event
    );

    for (input, tag) in [
        (
            super::InputMessage::Menu {
                action: super::MenuAction::Cancel,
            },
            15,
        ),
        (
            super::InputMessage::Confirm {
                action: super::ConfirmAction::Reply(false),
            },
            16,
        ),
    ] {
        let bytes = postcard::to_stdvec(&input).expect("overlay input encodes");
        assert_eq!(bytes[0], tag);
        assert_eq!(
            postcard::from_bytes::<super::InputMessage>(&bytes).expect("overlay input decodes"),
            input
        );
    }
    for (payload, tag) in [
        (super::EventPayload::Menu { state: None }, 36),
        (super::EventPayload::Confirm { state: None }, 37),
    ] {
        let event = super::Event {
            sequence: 7,
            payload,
        };
        let bytes = postcard::to_stdvec(&event).expect("overlay event encodes");
        assert_eq!(bytes[1], tag);
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes).expect("overlay event decodes"),
            event
        );
    }
}

#[test]
fn control_event_variants_hold_the_appended_wire_tails() {
    let payload_tag = |payload| {
        postcard::to_stdvec(&super::Event {
            sequence: 0,
            payload,
        })
        .expect("control event encodes")[1]
    };

    assert_eq!(
        payload_tag(super::EventPayload::ControlExit {
            reason: String::new(),
        }),
        38
    );
    assert_eq!(
        payload_tag(super::EventPayload::HookEvent {
            name: String::new(),
            variables: std::collections::BTreeMap::new(),
        }),
        39
    );
    assert_eq!(
        payload_tag(super::EventPayload::PaneOutput {
            pane: crate::PaneId(1),
            bytes: Vec::new(),
        }),
        40
    );
    assert_eq!(
        payload_tag(super::EventPayload::PaneOutputState {
            pane: crate::PaneId(1),
            paused: false,
        }),
        41
    );
    assert_eq!(
        payload_tag(super::EventPayload::PaneOutputAged {
            pane: crate::PaneId(1),
            age_ms: 0,
            bytes: Vec::new(),
        }),
        42
    );
    assert_eq!(
        payload_tag(super::EventPayload::ControlFlags {
            wait_exit: false,
            pause_after_ms: None,
            no_output: false,
            new_layouts: false,
        }),
        43
    );
    assert_eq!(
        payload_tag(super::EventPayload::SubscriptionChanged {
            name: String::new(),
            session: crate::SessionId(1),
            window: None,
            window_index: None,
            pane: None,
            value: String::new(),
        }),
        44
    );
}

#[test]
fn v71_mux_option_keys_hold_appended_tags_and_defaults() {
    for (key, tag, name, config_key) in [
        (
            MuxOptionKey::Mouse,
            13_u8,
            "mouse",
            Some(MuxOptionKey::Mouse),
        ),
        (
            MuxOptionKey::EscapeTime,
            14,
            "escape-time",
            Some(MuxOptionKey::EscapeTime),
        ),
        (
            MuxOptionKey::Prefix2,
            15,
            "prefix2",
            Some(MuxOptionKey::Prefix2),
        ),
    ] {
        assert_eq!(postcard::to_stdvec(&key).expect("mux option key"), [tag]);
        assert_eq!(key.as_str(), name);
        assert_eq!(MuxOptionKey::from_config_key(name), config_key);
    }
    let defaults = MuxOptions::default();
    assert_eq!(
        defaults.get(MuxOptionKey::Mouse).expect("mouse").value,
        "on"
    );
    assert_eq!(
        defaults
            .get(MuxOptionKey::EscapeTime)
            .expect("escape-time")
            .value,
        "10"
    );
    assert_eq!(
        defaults.get(MuxOptionKey::Prefix2).expect("prefix2").value,
        "None"
    );
    assert_eq!(defaults.validate(), Ok(()));
}

#[test]
fn status_position_default_bottom_is_the_tag_one_variant() {
    assert_eq!(
        super::StatusPosition::default(),
        super::StatusPosition::Bottom
    );
    assert_eq!(
        postcard::to_stdvec(&super::StatusPosition::Top).expect("top"),
        [0]
    );
    assert_eq!(
        postcard::to_stdvec(&super::StatusPosition::Bottom).expect("bottom"),
        [1]
    );
    assert_eq!(
        postcard::from_bytes::<super::StatusPosition>(&[1]).expect("bottom decodes"),
        super::StatusPosition::Bottom
    );
    assert!(postcard::from_bytes::<super::StatusPosition>(&[2]).is_err());
}

#[derive(Serialize)]
struct UnboundedStatusLine {
    left: String,
    right: String,
    title: String,
    base_style: String,
    rows: Vec<String>,
    position: super::StatusPosition,
    message_line: u8,
    customized: bool,
    theme: crate::ThemeColours,
    message_style: String,
    message_command_style: String,
    modes: Vec<super::ModePresentation>,
    pane_borders: Vec<super::PaneBorderPresentation>,
}

fn unbounded_status(rows: Vec<String>, message_line: u8) -> Vec<u8> {
    postcard::to_stdvec(&UnboundedStatusLine {
        left: String::new(),
        right: String::new(),
        title: String::new(),
        base_style: String::new(),
        rows,
        position: super::StatusPosition::Bottom,
        message_line,
        customized: false,
        theme: crate::ThemeColours::default(),
        message_style: String::new(),
        message_command_style: String::new(),
        modes: Vec::new(),
        pane_borders: Vec::new(),
    })
    .expect("status line shape")
}

#[test]
fn status_line_appends_after_the_v70_halves() {
    #[derive(Serialize)]
    struct LegacyStatusLine {
        left: String,
        right: String,
    }

    let status = super::StatusLine {
        left: "L".to_owned(),
        right: "R".to_owned(),
        ..super::StatusLine::default()
    };
    let bytes = postcard::to_stdvec(&status).expect("status");
    let legacy = postcard::to_stdvec(&LegacyStatusLine {
        left: "L".to_owned(),
        right: "R".to_owned(),
    })
    .expect("legacy halves");
    assert!(bytes.starts_with(&legacy));
    assert_eq!(
        postcard::from_bytes::<super::StatusLine>(&bytes).expect("status decodes"),
        status
    );
    assert!(status.is_empty(), "no rows means the status block is off");
    assert!(super::StatusLine::default().is_empty());
    assert!(
        !super::StatusLine {
            rows: vec![String::new()],
            ..super::StatusLine::default()
        }
        .is_empty(),
        "a blank row still counts as an on status block"
    );
}

#[test]
fn status_rows_accept_zero_through_five_and_reject_a_sixth_before_allocation() {
    for count in 0..=super::MAX_STATUS_ROWS {
        let rows = (0..count)
            .map(|index| {
                if index % 2 == 0 {
                    String::new()
                } else {
                    format!("row {index}")
                }
            })
            .collect::<Vec<_>>();
        let message_line = u8::try_from(count.saturating_sub(1)).expect("row index");
        let bytes = unbounded_status(rows.clone(), message_line);
        let decoded =
            postcard::from_bytes::<super::StatusLine>(&bytes).expect("bounded rows decode");
        assert_eq!(decoded.rows, rows);
        assert_eq!(decoded.validate(), Ok(()));
    }
    let blanks = unbounded_status(vec![String::new(); super::MAX_STATUS_ROWS], 4);
    let decoded = postcard::from_bytes::<super::StatusLine>(&blanks).expect("blank rows decode");
    assert_eq!(decoded.rows.len(), super::MAX_STATUS_ROWS);
    assert_eq!(decoded.validate(), Ok(()));

    let sixth = unbounded_status(vec![String::new(); super::MAX_STATUS_ROWS + 1], 0);
    assert!(postcard::from_bytes::<super::StatusLine>(&sixth).is_err());
    let exact = unbounded_status(vec!["x".repeat(super::MAX_STATUS_TEXT_BYTES)], 0);
    assert!(postcard::from_bytes::<super::StatusLine>(&exact).is_ok());
    let oversized = unbounded_status(vec!["x".repeat(super::MAX_STATUS_TEXT_BYTES + 1)], 0);
    assert!(postcard::from_bytes::<super::StatusLine>(&oversized).is_err());
}

#[test]
fn status_line_validation_enforces_rows_style_and_message_line_rules() {
    let mut status = super::StatusLine::default();
    assert_eq!(status.validate(), Ok(()));
    status.title = "zz".to_owned();
    assert_eq!(status.validate(), Ok(()));
    status.message_line = 1;
    assert!(status.validate().is_err());
    status.rows = vec![String::new(), "#[fg=red]row".to_owned()];
    assert_eq!(status.validate(), Ok(()));
    status.message_line = 2;
    assert!(status.validate().is_err());
    status.message_line = 0;
    status.base_style = "bg=blue,fg=white".to_owned();
    assert_eq!(status.validate(), Ok(()));
    status.base_style = "fg=nope".to_owned();
    assert!(status.validate().is_err());
    status.base_style = String::new();
    status.title = "x".repeat(super::MAX_STATUS_TEXT_BYTES + 1);
    assert!(status.validate().is_err());
    status.title = String::new();
    status.rows = vec![String::new(); super::MAX_STATUS_ROWS + 1];
    status.message_line = 0;
    assert!(status.validate().is_err());
}

#[test]
fn pane_indicator_labels_are_bounded_at_one_kibibyte() {
    let indicator = super::PaneIndicator {
        pane: crate::PaneId(3),
        index: 1,
        select_key: b'1',
        flags: super::PaneIndicator::ACTIVE,
        label: "x".repeat(super::MAX_PANE_INDICATOR_LABEL_BYTES),
    };
    let bytes = postcard::to_stdvec(&indicator).expect("indicator");
    assert_eq!(
        postcard::from_bytes::<super::PaneIndicator>(&bytes).expect("indicator decodes"),
        indicator
    );
    assert!(indicator.active());
    assert_eq!(indicator.selection_key(), Some('1'));

    #[derive(Serialize)]
    struct UnboundedPaneIndicator {
        pane: crate::PaneId,
        index: u32,
        select_key: u8,
        flags: u8,
        label: String,
    }
    let oversized = postcard::to_stdvec(&UnboundedPaneIndicator {
        pane: crate::PaneId(3),
        index: 1,
        select_key: 0,
        flags: 0,
        label: "x".repeat(super::MAX_PANE_INDICATOR_LABEL_BYTES + 1),
    })
    .expect("oversized shape");
    assert!(postcard::from_bytes::<super::PaneIndicator>(&oversized).is_err());
}

#[test]
fn chooser_row_keys_are_bounded_at_sixty_four_bytes() {
    let tree = super::ChooseTreeItem {
        label: "dev".to_owned(),
        detail: "2 windows".to_owned(),
        target: super::ChooseTreeTarget::Session(crate::SessionId(2)),
        depth: 0,
        flags: 0,
        pane_kind: None,
        key: "M".repeat(super::MAX_CHOOSE_ITEM_KEY_BYTES),
        text: "t".repeat(super::MAX_CHOOSE_ITEM_TEXT_BYTES),
    };
    let bytes = postcard::to_stdvec(&tree).expect("tree item");
    assert_eq!(
        postcard::from_bytes::<super::ChooseTreeItem>(&bytes).expect("tree item decodes"),
        tree
    );

    let buffer = super::ChooseBufferItem {
        name: "buffer0001".to_owned(),
        preview: "hello".to_owned(),
        size_bytes: 5,
        created_unix_seconds: 42,
        key: "0".to_owned(),
        text: String::new(),
        tagged: false,
    };
    let bytes = postcard::to_stdvec(&buffer).expect("buffer item");
    assert_eq!(
        postcard::from_bytes::<super::ChooseBufferItem>(&bytes).expect("buffer item decodes"),
        buffer
    );

    #[derive(Serialize)]
    struct UnboundedBufferItem {
        name: String,
        preview: String,
        size_bytes: u64,
        created_unix_seconds: u64,
        key: String,
        text: String,
    }
    let oversized = postcard::to_stdvec(&UnboundedBufferItem {
        name: String::new(),
        preview: String::new(),
        size_bytes: 0,
        created_unix_seconds: 0,
        key: "k".repeat(super::MAX_CHOOSE_ITEM_KEY_BYTES + 1),
        text: String::new(),
    })
    .expect("oversized shape");
    assert!(postcard::from_bytes::<super::ChooseBufferItem>(&oversized).is_err());
    let oversized = postcard::to_stdvec(&UnboundedBufferItem {
        name: String::new(),
        preview: String::new(),
        size_bytes: 0,
        created_unix_seconds: 0,
        key: String::new(),
        text: "t".repeat(super::MAX_CHOOSE_ITEM_TEXT_BYTES + 1),
    })
    .expect("oversized shape");
    assert!(postcard::from_bytes::<super::ChooseBufferItem>(&oversized).is_err());

    #[derive(Serialize)]
    struct UnboundedTreeItem {
        label: String,
        detail: String,
        target: super::ChooseTreeTarget,
        depth: u8,
        flags: u8,
        pane_kind: Option<super::ChooseTreePaneKind>,
        key: String,
        text: String,
    }
    let oversized = postcard::to_stdvec(&UnboundedTreeItem {
        label: String::new(),
        detail: String::new(),
        target: super::ChooseTreeTarget::Session(crate::SessionId(2)),
        depth: 0,
        flags: 0,
        pane_kind: None,
        key: "k".repeat(super::MAX_CHOOSE_ITEM_KEY_BYTES + 1),
        text: String::new(),
    })
    .expect("oversized shape");
    assert!(postcard::from_bytes::<super::ChooseTreeItem>(&oversized).is_err());
    let oversized = postcard::to_stdvec(&UnboundedTreeItem {
        label: String::new(),
        detail: String::new(),
        target: super::ChooseTreeTarget::Session(crate::SessionId(2)),
        depth: 0,
        flags: 0,
        pane_kind: None,
        key: String::new(),
        text: "t".repeat(super::MAX_CHOOSE_ITEM_TEXT_BYTES + 1),
    })
    .expect("oversized shape");
    assert!(postcard::from_bytes::<super::ChooseTreeItem>(&oversized).is_err());
}

#[test]
fn deferred_stdin_availability_and_request_roundtrip() {
    let mut command = super::CommandInvocation::new("source-file", ["config"]);
    assert!(!command.stdin_available());
    command.set_stdin_available(true);
    let bytes = postcard::to_stdvec(&command).expect("encode available stream");
    assert_eq!(
        postcard::from_bytes::<super::CommandInvocation>(&bytes).expect("decode stream"),
        command
    );
    let request = super::ClientFileOperation::ReadStdin { binary: true };
    let bytes = postcard::to_stdvec(&request).expect("encode stdin request");
    assert_eq!(bytes, [2, 1]);
    assert_eq!(
        postcard::from_bytes::<super::ClientFileOperation>(&bytes).expect("decode request"),
        request
    );
    let chunk = super::ClientFileOperation::ReadStdinChunk;
    let bytes = postcard::to_stdvec(&chunk).expect("encode stdin chunk request");
    assert_eq!(bytes, [3]);
    assert_eq!(
        postcard::from_bytes::<super::ClientFileOperation>(&bytes).expect("decode chunk"),
        chunk
    );
}

#[test]
fn caller_stream_spent_marker_stays_in_process() {
    let absent = super::CommandInvocation::new("source-file", ["-"]);
    let mut spent = absent.clone();
    spent.set_stdin("payload");
    spent.set_stdin_spent();
    assert!(spent.stdin().is_none());
    assert!(spent.stdin_was_spent());
    let bytes = postcard::to_stdvec(&spent).expect("encode spent stream");
    assert_eq!(
        bytes,
        postcard::to_stdvec(&absent).expect("encode absent stream")
    );
    let decoded =
        postcard::from_bytes::<super::CommandInvocation>(&bytes).expect("decode absent stream");
    assert_eq!(decoded, absent);
    spent.set_stdin("");
    assert!(spent.stdin().is_some());
    assert!(!spent.stdin_was_spent());
}

#[test]
fn released_command_stdout_and_client_exit_append_after_the_chooser_presentation() {
    let presentation = super::Event {
        sequence: 0,
        payload: super::EventPayload::ChooserPresentation { presentation: None },
    };
    let tag = postcard::to_stdvec(&presentation).expect("encode chooser presentation")[1];
    let released = super::Event {
        sequence: 0,
        payload: super::EventPayload::CommandStdout {
            output: super::RawText::from("%1\n"),
        },
    };
    let bytes = postcard::to_stdvec(&released).expect("encode released stdout");
    assert_eq!(bytes[1], tag + 1);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("decode released stdout"),
        released
    );
    let exit = super::Event {
        sequence: 0,
        payload: super::EventPayload::CommandClientExit,
    };
    let bytes = postcard::to_stdvec(&exit).expect("encode client exit");
    assert_eq!(bytes, [0, tag + 2]);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("decode client exit"),
        exit
    );
}

#[test]
fn key_table_active_appends_after_the_command_client_exit() {
    let exit = postcard::to_stdvec(&super::Event {
        sequence: 0,
        payload: super::EventPayload::CommandClientExit,
    })
    .expect("encode client exit")[1];
    for (table, repeat) in [
        (Some("prefix".to_owned()), false),
        (Some("resize".to_owned()), true),
        (None, false),
    ] {
        let event = super::Event {
            sequence: 3,
            payload: super::EventPayload::KeyTableActive { table, repeat },
        };
        let bytes = postcard::to_stdvec(&event).expect("encode key table active");
        assert_eq!(bytes[1], exit + 1);
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes).expect("decode key table active"),
            event
        );
    }
}

#[test]
fn key_tables_patched_appends_after_the_path_picker() {
    let open = postcard::to_stdvec(&super::Event {
        sequence: 0,
        payload: super::EventPayload::OpenPathPicker {
            pane: crate::PaneId(1),
            start_dir: None,
        },
    })
    .expect("encode open path picker")[1];
    let event = super::Event {
        sequence: 5,
        payload: super::EventPayload::KeyTablesPatched {
            tables: vec![super::KeyTableSnapshot {
                name: "root".to_owned(),
                bindings: Vec::new(),
            }],
            removed: vec!["resize".to_owned()],
        },
    };
    let bytes = postcard::to_stdvec(&event).expect("encode key tables patched");
    assert_eq!(bytes[1], open + 1);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("decode key tables patched"),
        event
    );
}

#[test]
fn command_prompt_appends_type_mode_and_no_freeze() {
    assert_eq!(
        super::CommandPromptType::default(),
        super::CommandPromptType::Command
    );
    assert_eq!(
        super::CommandPromptMode::default(),
        super::CommandPromptMode::Text
    );
    assert_eq!(
        postcard::to_stdvec(&super::CommandPromptType::Search).expect("search"),
        [1]
    );
    assert_eq!(
        postcard::to_stdvec(&super::CommandPromptMode::BackspaceExit).expect("backspace exit"),
        [5]
    );
    assert!(postcard::from_bytes::<super::CommandPromptType>(&[2]).is_err());
    assert!(postcard::from_bytes::<super::CommandPromptMode>(&[6]).is_err());

    let state = super::CommandPromptState {
        prompt: "(search up)".to_owned(),
        input: "needle".to_owned(),
        cursor: 6,
        kind: super::CommandPromptKind::Value,
        history: Vec::new(),
        prompt_type: super::CommandPromptType::Search,
        mode: super::CommandPromptMode::Incremental,
        no_freeze: true,
        pane: None,
        command_mode: false,
        prompt_cursor: crate::PromptCursor::default(),
    };
    let bytes = postcard::to_stdvec(&state).expect("prompt state");
    assert_eq!(
        postcard::from_bytes::<super::CommandPromptState>(&bytes).expect("state decodes"),
        state
    );
    assert_eq!(bytes[bytes.len() - 4..], [0, 0, 0, 0]);
    let on_pane = super::CommandPromptState {
        pane: Some(crate::PaneId(7)),
        ..state.clone()
    };
    let pane_bytes = postcard::to_stdvec(&on_pane).expect("pane prompt state");
    assert_eq!(pane_bytes[..bytes.len() - 4], bytes[..bytes.len() - 4]);
    assert_eq!(pane_bytes[bytes.len() - 4..], [1, 7, 0, 0, 0]);
    assert_eq!(
        postcard::from_bytes::<super::CommandPromptState>(&pane_bytes).expect("pane decodes"),
        on_pane
    );
    let command = super::CommandPromptState {
        command_mode: true,
        prompt_cursor: super::PromptCursor {
            style: 6,
            colour: Some(crate::TmuxColour::Basic(1)),
        },
        ..state.clone()
    };
    let command_bytes = postcard::to_stdvec(&command).expect("command mode state");
    assert_eq!(command_bytes[..bytes.len() - 3], bytes[..bytes.len() - 3]);
    assert_eq!(command_bytes[bytes.len() - 3..], [1, 6, 1, 0, 1]);
    assert_eq!(
        postcard::from_bytes::<super::CommandPromptState>(&command_bytes)
            .expect("command mode decodes"),
        command
    );
}

#[test]
fn command_success_appends_stderr_after_exit_code() {
    #[derive(Serialize)]
    struct LegacySuccess {
        request_id: u64,
        output: String,
        exit_code: u8,
    }

    let success = super::CommandResponse::Success {
        request_id: 7,
        output: "out".into(),
        exit_code: 1,
        stderr: "err".to_owned(),
        stdout_claim: super::StdoutClaim::Raw,
    };
    let bytes = postcard::to_stdvec(&success).expect("success");
    let mut legacy = vec![0];
    legacy.extend(
        postcard::to_stdvec(&LegacySuccess {
            request_id: 7,
            output: "out".to_owned(),
            exit_code: 1,
        })
        .expect("legacy success"),
    );
    assert!(bytes.starts_with(&legacy));
    assert_eq!(
        postcard::from_bytes::<super::CommandResponse>(&bytes).expect("success decodes"),
        success
    );
}

#[test]
fn timed_message_identity_and_clear_hold_the_appended_wire_tail() {
    let message = super::EventPayload::TimedClientMessage {
        pane: None,
        kind: super::ClientMessageKind::Info,
        text: "hello".to_owned(),
        duration_ms: 750,
        message_id: 9,
    };
    let event = super::Event {
        sequence: 5,
        payload: message,
    };
    let bytes = postcard::to_stdvec(&event).expect("timed message encodes");
    assert_eq!(bytes[1], 33);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("timed message decodes"),
        event
    );

    let cleared = super::Event {
        sequence: 6,
        payload: super::EventPayload::TimedClientMessageCleared { message_id: 9 },
    };
    let bytes = postcard::to_stdvec(&cleared).expect("cleared encodes");
    assert_eq!(bytes[1], 45);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("cleared decodes"),
        cleared
    );
}

#[test]
fn control_command_guard_output_round_trips_bytes_that_are_not_utf8() {
    let event = super::Event {
        sequence: 7,
        payload: super::EventPayload::ControlCommandGuard {
            output: super::RawText::from_bytes(b"a\xfeb\n".to_vec()),
            error: false,
            sticky_failure: false,
            flags: 1,
        },
    };
    let bytes = postcard::to_stdvec(&event).expect("control command guard encodes");
    let decoded = postcard::from_bytes::<super::Event>(&bytes).expect("guard decodes");
    assert!(matches!(
        decoded.payload,
        super::EventPayload::ControlCommandGuard { output, .. } if output.as_bytes() == b"a\xfeb\n"
    ));
}

#[test]
fn control_command_guard_holds_wire_tag_forty_six_and_round_trips_flags() {
    for flags in [0, 1] {
        let event = super::Event {
            sequence: 7,
            payload: super::EventPayload::ControlCommandGuard {
                output: "diagnostic\n".into(),
                error: true,
                sticky_failure: false,
                flags,
            },
        };
        let bytes = postcard::to_stdvec(&event).expect("control command guard encodes");
        assert_eq!(bytes[1], 46);
        assert_eq!(bytes.last(), Some(&flags));
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes).expect("control command guard decodes"),
            event
        );
    }
}

#[test]
fn control_source_file_holds_wire_tag_forty_seven_and_round_trips_events() {
    for source_event in [
        super::ControlSourceFileEvent::ReadError("Is a directory: source.conf".to_owned()),
        super::ControlSourceFileEvent::Complete,
    ] {
        let event = super::Event {
            sequence: 8,
            payload: super::EventPayload::ControlSourceFile {
                event: source_event,
            },
        };
        let bytes = postcard::to_stdvec(&event).expect("control source-file event encodes");
        assert_eq!(bytes[1], 47);
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes)
                .expect("control source-file event decodes"),
            event
        );
    }
}

#[test]
fn startup_config_causes_hold_wire_tag_forty_eight_and_round_trip() {
    let event = super::Event {
        sequence: 9,
        payload: super::EventPayload::StartupConfigCauses {
            causes: vec!["one".to_owned(), "two\ncontinued".to_owned()],
        },
    };
    let bytes = postcard::to_stdvec(&event).expect("startup config causes encode");
    assert_eq!(bytes[1], 48);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("startup config causes decode"),
        event
    );
}

#[test]
fn control_command_output_holds_wire_tag_forty_nine() {
    let event = super::Event {
        sequence: 10,
        payload: super::EventPayload::ControlCommandOutput {
            output: "child output\n'exit 3' returned 3".to_owned(),
        },
    };
    let bytes = postcard::to_stdvec(&event).expect("control command output encodes");
    assert_eq!(bytes[1], 49);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("control command output decodes"),
        event
    );
}

#[test]
fn client_terminal_size_input_holds_wire_tag_seventeen() {
    assert_eq!(super::CLIENT_NESTED_CAPABILITY, "client-nested-v1");
    assert_eq!(super::CLIENT_TTY_CAPABILITY_PREFIX, "client-tty-v1:");
    assert_eq!(super::CLIENT_SIZE_CAPABILITY_PREFIX, "client-size-v1:");
    let input = super::InputMessage::ClientTerminalSize {
        columns: 120,
        rows: 40,
    };
    let bytes = postcard::to_stdvec(&input).expect("client size encodes");
    assert_eq!(bytes[0], 17);
    assert_eq!(
        postcard::from_bytes::<super::InputMessage>(&bytes).expect("client size decodes"),
        input
    );
}

#[test]
fn client_focus_input_holds_wire_tag_eighteen() {
    let input = super::InputMessage::ClientFocus { focused: true };
    let bytes = postcard::to_stdvec(&input).expect("client focus encodes");
    assert_eq!(bytes[0], 18);
    assert_eq!(
        postcard::from_bytes::<super::InputMessage>(&bytes).expect("client focus decodes"),
        input
    );
}

#[test]
fn command_request_carries_the_v74_prepared_bit() {
    let request = |prepared| {
        super::ProtocolMessage::CommandRequest(super::CommandRequest {
            request_id: 7,
            command: super::CommandInvocation::new("list-sessions", std::iter::empty::<&str>()),
            prepared,
        })
    };
    let ordinary = postcard::to_stdvec(&request(false)).expect("ordinary request encodes");
    let prepared = postcard::to_stdvec(&request(true)).expect("prepared request encodes");
    assert_eq!(ordinary.last(), Some(&0));
    assert_eq!(prepared.last(), Some(&1));
    assert_eq!(
        postcard::from_bytes::<super::ProtocolMessage>(&ordinary)
            .expect("ordinary request decodes"),
        request(false)
    );
    assert_eq!(
        postcard::from_bytes::<super::ProtocolMessage>(&prepared)
            .expect("prepared request decodes"),
        request(true)
    );
}

#[test]
fn enter_copy_mode_with_holds_wire_tag_twenty_seven() {
    use zz_terminal::TerminalViewAction;

    assert_eq!(
        postcard::to_stdvec(&TerminalViewAction::EnterCopyMode).expect("enter")[0],
        15
    );
    assert_eq!(
        postcard::to_stdvec(&TerminalViewAction::EnterCopyModeScrollExit).expect("scroll exit")[0],
        26
    );
    let action = TerminalViewAction::EnterCopyModeWith {
        scroll_exit: true,
        hide_position: true,
    };
    let bytes = postcard::to_stdvec(&action).expect("composed enter");
    assert_eq!(bytes[0], 27);
    assert_eq!(
        postcard::from_bytes::<TerminalViewAction>(&bytes).expect("composed enter decodes"),
        action
    );
}

#[test]
fn counted_copy_mode_action_holds_wire_tag_twenty_eight_without_recursive_actions() {
    use zz_terminal::{CopyModeAction, TerminalViewAction};

    let action = TerminalViewAction::CopyModeCounted {
        action: CopyModeAction::NextMatchingBracket,
        count: u32::MAX,
    };
    let bytes = postcard::to_stdvec(&action).expect("counted copy action encodes");
    assert_eq!(bytes[0], 28);
    assert_eq!(bytes[1], 45);
    assert_eq!(
        postcard::from_bytes::<TerminalViewAction>(&bytes).expect("counted copy action decodes"),
        action
    );
    assert!(postcard::from_bytes::<CopyModeAction>(&[200, 1, 1]).is_err());
    assert!(postcard::from_bytes::<TerminalViewAction>(&[28, 200, 1, 1]).is_err());
}

#[test]
fn repeated_browser_keys_hold_wire_tag_seven() {
    let command = super::BrowserCommand::SendKeysRepeated {
        keys: vec![super::KeyToken::Literal("x".to_owned())],
        count: u32::MAX,
    };
    let bytes = postcard::to_stdvec(&command).expect("repeated browser keys encode");
    assert_eq!(bytes[0], 7);
    assert_eq!(
        postcard::from_bytes::<super::BrowserCommand>(&bytes)
            .expect("repeated browser keys decode"),
        command
    );
}

#[test]
fn detached_reason_holds_its_appended_wire_field() {
    assert_eq!(super::PROTOCOL_VERSION, 125);
    for (reason, tag) in [
        (super::DetachReason::Requested, 0),
        (super::DetachReason::Evicted, 1),
        (super::DetachReason::SessionDestroyed, 2),
        (super::DetachReason::ServerStopping, 3),
    ] {
        let event = super::Event {
            sequence: 0,
            payload: super::EventPayload::Detached {
                session: crate::SessionId(1),
                by: None,
                reason,
                action: super::ClientExitAction::Exit,
            },
        };
        let bytes = postcard::to_stdvec(&event).expect("detached event encodes");
        assert_eq!(bytes[1], 22);
        assert_eq!(bytes[bytes.len() - 2], tag);
        assert_eq!(bytes.last(), Some(&0));
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes).expect("detached event decodes"),
            event
        );
    }
}

#[test]
fn client_exit_actions_append_after_the_detach_reason() {
    for (action, tail) in [
        (super::ClientExitAction::Exit, vec![0]),
        (super::ClientExitAction::ParentHangup, vec![1]),
        (
            super::ClientExitAction::Exec {
                command: "id".to_owned(),
                shell: "/bin/sh".to_owned(),
            },
            vec![
                2, 2, b'i', b'd', 7, b'/', b'b', b'i', b'n', b'/', b's', b'h',
            ],
        ),
    ] {
        let event = super::Event {
            sequence: 0,
            payload: super::EventPayload::Detached {
                session: crate::SessionId(1),
                by: None,
                reason: super::DetachReason::Requested,
                action: action.clone(),
            },
        };
        let bytes = postcard::to_stdvec(&event).expect("detached event encodes");
        assert_eq!(bytes[1], 22);
        assert_eq!(&bytes[bytes.len() - tail.len()..], tail.as_slice());
        assert_eq!(
            postcard::from_bytes::<super::Event>(&bytes).expect("detached event decodes"),
            event
        );
    }
}

#[test]
fn agent_question_and_task_messages_append_at_the_wire_tail_and_round_trip() {
    let tag = |message: &super::ProtocolMessage| {
        let bytes = postcard::to_stdvec(message).expect("message encodes");
        let mut value = 0u32;
        for (index, byte) in bytes.iter().enumerate() {
            value |= u32::from(byte & 0x7f) << (7 * index);
            if byte & 0x80 == 0 {
                break;
            }
        }
        value
    };
    let base = tag(&super::ProtocolMessage::TtyInputClosed { handoff: 0 });
    let pane = crate::PaneId(4);
    let answer = super::ProtocolMessage::AgentAnswerQuestion {
        pane,
        request_id: 9,
        answers: vec![super::AgentQuestionAnswer {
            id: "color".to_owned(),
            answers: vec!["red".to_owned(), "blue".to_owned()],
        }],
    };
    let stop = super::ProtocolMessage::AgentStopTask {
        pane,
        task_id: "b7".to_owned(),
    };
    assert_eq!(tag(&answer), base + 1);
    assert_eq!(tag(&stop), base + 2);
    for message in [answer, stop] {
        let bytes = postcard::to_stdvec(&message).expect("encode");
        assert_eq!(
            postcard::from_bytes::<super::ProtocolMessage>(&bytes).expect("decode"),
            message
        );
    }
    let state = super::AgentPaneWire {
        tasks: vec![super::AgentTaskWire {
            id: "b7".to_owned(),
            kind: "shell".to_owned(),
            description: "sleep 30".to_owned(),
            tool_call_id: Some("toolu_1".to_owned()),
        }],
        ..super::AgentPaneWire::default()
    };
    let bytes = postcard::to_stdvec(&state).expect("encode state");
    assert_eq!(
        postcard::from_bytes::<super::AgentPaneWire>(&bytes).expect("decode state"),
        state
    );
    let too_many = super::AgentPaneWire {
        tasks: vec![super::AgentTaskWire::default(); super::MAX_AGENT_TASKS + 1],
        ..super::AgentPaneWire::default()
    };
    assert!(too_many.validate().is_err());
}

#[test]
fn path_picker_variants_append_at_the_wire_tails_and_round_trip() {
    let exit = super::Event {
        sequence: 0,
        payload: super::EventPayload::CommandClientExit,
    };
    let exit_tag = postcard::to_stdvec(&exit).expect("encode client exit")[1];
    let open = super::Event {
        sequence: 0,
        payload: super::EventPayload::OpenPathPicker {
            pane: crate::PaneId(4),
            start_dir: Some("~/dev".to_owned()),
        },
    };
    let bytes = postcard::to_stdvec(&open).expect("encode open path picker");
    assert_eq!(bytes[1], exit_tag + 2);
    assert_eq!(
        postcard::from_bytes::<super::Event>(&bytes).expect("decode open path picker"),
        open
    );

    let message_tag = |message: &super::ProtocolMessage| {
        postcard::to_stdvec(message).expect("message encodes")[0]
    };
    let base = message_tag(&super::ProtocolMessage::ClientTerminalType {
        term_type: String::new(),
    });
    let root = crate::PathListRoot {
        root: "/home/u/dev".to_owned(),
        display_root: "~/dev".to_owned(),
        cwd: Some("/home/u/dev".to_owned()),
        insert: crate::InsertStyle::Shell(crate::ShellKind::Fish),
    };
    for (offset, message) in [
        super::ProtocolMessage::PathListRequest {
            request_id: 7,
            pane: Some(crate::PaneId(4)),
            dir: Some("../x".to_owned()),
        },
        super::ProtocolMessage::PathListBegin {
            request_id: 7,
            result: Ok(root),
        },
        super::ProtocolMessage::PathListChunk {
            request_id: 7,
            entries: vec![crate::PathEntry {
                rel: "src/main.rs".to_owned(),
                kind: crate::PathKind::File,
                symlink: false,
            }],
            done: true,
            truncated: false,
        },
        super::ProtocolMessage::PathListGit {
            request_id: 7,
            marks: vec![("src/".to_owned(), crate::GitMark::Untracked)],
        },
        super::ProtocolMessage::PathListCancel { request_id: 7 },
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = postcard::to_stdvec(&message).expect("path list message encodes");
        assert_eq!(
            bytes[0],
            base + 1 + u8::try_from(offset).expect("small offset")
        );
        assert_eq!(
            postcard::from_bytes::<super::ProtocolMessage>(&bytes)
                .expect("path list message decodes"),
            message
        );
    }
    let error = super::ProtocolMessage::PathListBegin {
        request_id: 8,
        result: Err("no such directory".to_owned()),
    };
    let bytes = postcard::to_stdvec(&error).expect("error begin encodes");
    assert_eq!(
        postcard::from_bytes::<super::ProtocolMessage>(&bytes).expect("error begin decodes"),
        error
    );
}

#[test]
fn path_picker_text_is_bounded_on_the_wire() {
    let long = "a".repeat(crate::MAX_PATH_LIST_TEXT_BYTES + 1);
    for message in [
        super::ProtocolMessage::PathListRequest {
            request_id: 1,
            pane: Some(crate::PaneId(1)),
            dir: Some(long.clone()),
        },
        super::ProtocolMessage::PathListBegin {
            request_id: 1,
            result: Err(long.clone()),
        },
        super::ProtocolMessage::PathListBegin {
            request_id: 1,
            result: Ok(crate::PathListRoot {
                root: long.clone(),
                display_root: String::new(),
                cwd: None,
                insert: crate::InsertStyle::Claude,
            }),
        },
        super::ProtocolMessage::PathListChunk {
            request_id: 1,
            entries: vec![crate::PathEntry {
                rel: long.clone(),
                kind: crate::PathKind::Dir,
                symlink: true,
            }],
            done: false,
            truncated: false,
        },
        super::ProtocolMessage::PathListGit {
            request_id: 1,
            marks: vec![(long.clone(), crate::GitMark::Modified)],
        },
    ] {
        let bytes = postcard::to_stdvec(&message).expect("oversized message encodes");
        assert!(postcard::from_bytes::<super::ProtocolMessage>(&bytes).is_err());
    }
}
