use std::{collections::BTreeMap, error::Error, path::PathBuf};

use serde_json::{Value, json};
use zz_protocol::{
    AgentDescriptor, AgentProvider, AttachOperation, Axis, Batch, BrowserDescriptor,
    CAPABILITY_NAMES, ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeSearchState,
    ChooseTreeState, ChooseTreeTarget, ChooserPresentation, ChooserPreview, ChooserPreviewSize,
    ChooserPreviewTile, ChooserRow, ClientEnvironmentBlob, ClientHello, ClientId, ClientInstanceId,
    ClientKind, ClientView, ClientViewport, EditorDescriptor, Event, EventPayload,
    FloatingPaneSnapshot, Hello, HostPlatform, InputMessage, KeySubscription, LayoutNode,
    LinuxDistro, ModalPaneSnapshot, MuxSnapshot, PROTOCOL_VERSION, PaneBorderIndicators,
    PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot, PaneMode, PaneSnapshot,
    PaneStatus, PaneStatusKind, PaneStatusState, PaneSummary, PanesModeArea, PanesModeBorder,
    PanesModeClear, PromptCursor, ProtocolMessage, SessionId, SessionSnapshot, SessionViewer,
    ShellCommand, SplitId, Subscriptions, TmuxColour, TreeDelta, TreeOp, TreeSubscription, Welcome,
    WindowId, WindowSnapshot, decode_protocol_frame, encode_protocol_message,
};
use zz_terminal::{SessionStatus, TerminalViewport};

fn fixture(name: &str, message: &ProtocolMessage) -> Result<Value, Box<dyn Error>> {
    let bytes = encode_protocol_message(message)?;
    let decoded = decode_protocol_frame(&bytes)?;
    let reencoded = encode_protocol_message(&decoded)?;
    if reencoded != bytes || decoded != *message {
        return Err(std::io::Error::other(format!("fixture does not round trip: {name}")).into());
    }
    let (message_tag, remaining) = postcard::take_from_bytes::<u32>(&bytes[8..])?;
    let mut tags = json!({"message": message_tag});
    if matches!(message, ProtocolMessage::Event(_)) {
        let (_, remaining) = postcard::take_from_bytes::<u64>(remaining)?;
        let (event_tag, _) = postcard::take_from_bytes::<u32>(remaining)?;
        tags["event"] = json!(event_tag);
    }
    if matches!(message, ProtocolMessage::Input(_)) {
        let (input_tag, _) = postcard::take_from_bytes::<u32>(remaining)?;
        tags["input"] = json!(input_tag);
    }
    let mut modes = Vec::new();
    if let ProtocolMessage::Event(Event {
        payload: EventPayload::Snapshot(snapshot),
        ..
    }) = message
    {
        for session in &snapshot.sessions {
            for window in &session.windows {
                for pane in window.panes.values() {
                    modes.push(json!({"session":session.id.0.to_string(),"window":window.id.0.to_string(),"pane":pane.id.0.to_string(),"bytes":postcard::to_stdvec(&pane.mode)?}));
                }
            }
        }
    }
    Ok(json!({"name": name, "bytes": bytes, "value": message, "pane_modes": modes, "tags": tags}))
}

fn event_bytes(payload: EventPayload) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload,
    }))?)
}

fn repair_length(mut frame: Vec<u8>) -> Result<Vec<u8>, Box<dyn Error>> {
    let length = u32::try_from(frame.len() - 4)?;
    frame[..4].copy_from_slice(&length.to_le_bytes());
    Ok(frame)
}

fn replace_field(
    frame: &[u8],
    offset: usize,
    length: usize,
    replacement: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut changed = frame.to_vec();
    changed.splice(offset..offset + length, replacement.iter().copied());
    repair_length(changed)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Result<usize, Box<dyn Error>> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
        .ok_or_else(|| std::io::Error::other("fixture field was not found").into())
}

fn unchecked_control(message: &ProtocolMessage) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut frame = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    frame.truncate(8);
    frame.extend(postcard::to_stdvec(message)?);
    repair_length(frame)
}

fn add_decoded_case(
    rejected: &mut Vec<Value>,
    accepted: &mut Vec<Value>,
    name: &str,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    match decode_protocol_frame(bytes) {
        Ok(message) => {
            if let ProtocolMessage::Batch(batch) = &message
                && let Err(error) = batch.messages()
            {
                rejected.push(json!({"name":name,"bytes":bytes,"error":error.to_string(),"validation":"batch-messages"}));
                return Ok(());
            }
            let canonical = encode_protocol_message(&message)?;
            accepted.push(json!({"name":name,"bytes":bytes,"canonical_bytes":canonical}));
        }
        Err(error) => rejected.push(json!({"name":name,"bytes":bytes,"error":error.to_string(),"validation":"decode-protocol-frame"})),
    }
    Ok(())
}

fn balanced_layout(depth: u8) -> LayoutNode {
    if depth == 0 {
        LayoutNode::Empty
    } else {
        LayoutNode::Split {
            id: SplitId(0),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(balanced_layout(depth - 1)),
            second: Box::new(balanced_layout(depth - 1)),
        }
    }
}

fn malformed_frames() -> Result<(Vec<Value>, Vec<Value>), Box<dyn Error>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::new();
    let mut add =
        |name: &str, bytes: Vec<u8>| add_decoded_case(&mut rejected, &mut accepted, name, &bytes);
    let minimal = minimal_snapshot();
    let minimal_frame = event_bytes(EventPayload::Snapshot(minimal.clone()))?;
    let window = &minimal.sessions[0].windows[0];
    let window_bytes = postcard::to_stdvec(window)?;
    let window_offset = find_bytes(&minimal_frame, &window_bytes)?;
    let automatic_offset =
        window_offset + postcard::to_stdvec(&(window.id, window.index, &window.name))?.len();
    add(
        "invalid-boolean",
        replace_field(&minimal_frame, automatic_offset, 1, &[2])?,
    )?;
    let index_offset = window_offset + postcard::to_stdvec(&window.id)?.len();
    add(
        "u32-overflow",
        replace_field(
            &minimal_frame,
            index_offset,
            1,
            &postcard::to_stdvec(&(u64::from(u32::MAX) + 1))?,
        )?,
    )?;
    let suffix = postcard::to_stdvec(&(window.sx, window.sy, window.pane_base_index))?;
    let sx_offset = window_offset + window_bytes.len() - suffix.len();
    add(
        "u16-overflow",
        replace_field(
            &minimal_frame,
            sx_offset,
            1,
            &postcard::to_stdvec(&65_536_u32)?,
        )?,
    )?;
    let layout_offset = window_offset
        + postcard::to_stdvec(&(
            window.id,
            window.index,
            &window.name,
            window.automatic_rename,
            window.active_pane,
            window.zoomed_pane,
        ))?
        .len();
    add(
        "invalid-layout-enum",
        replace_field(&minimal_frame, layout_offset, 1, &[127])?,
    )?;
    let pane = &window.panes[&window.active_pane];
    let pane_bytes = postcard::to_stdvec(pane)?;
    let pane_offset = find_bytes(&minimal_frame, &pane_bytes)?;
    let kind_offset = pane_offset + postcard::to_stdvec(&(pane.id, &pane.title))?.len();
    add(
        "invalid-pane-kind-enum",
        replace_field(&minimal_frame, kind_offset, 1, &[127])?,
    )?;
    let mut unknown = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    unknown[8] = 127;
    add("invalid-protocol-enum", unknown)?;
    let full = event_bytes(EventPayload::Snapshot(snapshot()))?;
    for removed in [1, 5, full.len() / 2] {
        let mut truncated = full.clone();
        truncated.truncate(truncated.len() - removed);
        add(
            &format!("truncated-snapshot-{removed}"),
            repair_length(truncated)?,
        )?;
    }
    let mut trailing = minimal_frame.clone();
    trailing.push(0);
    add("trailing-snapshot-byte", repair_length(trailing)?)?;
    let view_frame = event_bytes(EventPayload::ClientView(ClientView::default()))?;
    let (_, remaining) = postcard::take_from_bytes::<u32>(&view_frame[8..])?;
    let (_, remaining) = postcard::take_from_bytes::<u64>(remaining)?;
    let (_, remaining) = postcard::take_from_bytes::<u32>(remaining)?;
    let view_offset = view_frame.len() - remaining.len();
    add(
        "invalid-option-tag",
        replace_field(&view_frame, view_offset, 1, &[2])?,
    )?;
    let welcome = Welcome {
        protocol_version: PROTOCOL_VERSION,
        server_id: 1,
        client_id: ClientId(1),
        client_instance_id: ClientInstanceId(1),
        caps: (1 << CAPABILITY_NAMES.len()) - 1,
        clock: 0,
        platform: HostPlatform::default(),
    };
    let welcome_frame = encode_protocol_message(&ProtocolMessage::Welcome(welcome))?;
    let (_, remaining) = postcard::take_from_bytes::<u32>(&welcome_frame[8..])?;
    let welcome_offset = welcome_frame.len() - remaining.len();
    add(
        "welcome-version-u16-overflow",
        replace_field(
            &welcome_frame,
            welcome_offset,
            1,
            &postcard::to_stdvec(&65_536_u32)?,
        )?,
    )?;
    add(
        "welcome-missing-capabilities",
        unchecked_control(&ProtocolMessage::Welcome(Welcome { caps: 0, ..welcome }))?,
    )?;
    for missing in [
        zz_protocol::CONTROL_CAPABILITY,
        zz_protocol::PANE_FRAME_CAPABILITY,
    ] {
        let capabilities = CAPABILITY_NAMES
            .iter()
            .filter(|name| **name != missing)
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        add(
            &format!("welcome-missing-{missing}"),
            unchecked_control(&ProtocolMessage::Welcome(Welcome {
                caps: Welcome::caps_from_strings(&capabilities),
                ..welcome
            }))?,
        )?;
    }
    let mut unplatformed = welcome_frame.clone();
    unplatformed.pop();
    add("welcome-missing-platform", repair_length(unplatformed)?)?;
    add(
        "welcome-platform-tag",
        replace_field(&welcome_frame, welcome_frame.len() - 1, 1, &[4])?,
    )?;
    let linux_frame = encode_protocol_message(&ProtocolMessage::Welcome(Welcome {
        platform: HostPlatform::Linux(LinuxDistro::Other),
        ..welcome
    }))?;
    add(
        "welcome-linux-distro-tag",
        replace_field(&linux_frame, linux_frame.len() - 1, 1, &[5])?,
    )?;
    let ping = encode_protocol_message(&ProtocolMessage::Ping {
        token: 1,
        rtt_us: 0,
    })?;
    let (_, remaining) = postcard::take_from_bytes::<u32>(&ping[8..])?;
    let (_, remaining) = postcard::take_from_bytes::<u64>(remaining)?;
    add(
        "ping-rtt-u32-overflow",
        replace_field(
            &ping,
            ping.len() - remaining.len(),
            1,
            &postcard::to_stdvec(&(u64::from(u32::MAX) + 1))?,
        )?,
    )?;
    for index in 0..10 {
        let mut tree = minimal.clone();
        let mut item = pane.clone();
        let name = match index {
            0 => {
                tree.sessions[0].windows[0].status_label = "x".repeat(1025);
                "window-status-limit"
            }
            1 => {
                item.summary.name = "x".repeat(257);
                "pane-name-limit"
            }
            2 => {
                item.summary.process = vec![String::new(); 33];
                "process-count-limit"
            }
            3 => {
                item.status = Some(PaneStatus {
                    state: PaneStatusState::Idle,
                    kind: None,
                    progress: None,
                    app: String::new(),
                    title: String::new(),
                    message: "x".repeat(2049),
                });
                "pane-status-message-limit"
            }
            4 => {
                item.kind = PaneKindSnapshot::Editor(EditorDescriptor {
                    path: None,
                    cwd: "relative".to_owned(),
                });
                "editor-relative-cwd"
            }
            5 => {
                item.border_colour = Some(TmuxColour::Rgb(1 << 24));
                "rgb-24bit-limit"
            }
            6 | 7 => {
                let mut mode = customize_mode(2);
                if let PaneMode::Customize { state, .. } = &mut mode {
                    if index == 6 {
                        state.items[0].key = "x".repeat(65);
                    } else {
                        state.items[0].text = "x".repeat(513);
                    }
                }
                item.mode = Some(mode);
                if index == 6 {
                    "chooser-key-limit"
                } else {
                    "chooser-text-limit"
                }
            }
            8 => {
                item.summary.shell = "x".repeat(257);
                "pane-shell-limit"
            }
            9 => {
                item.summary.shell = "x".repeat(256);
                "pane-shell-boundary"
            }
            _ => unreachable!(),
        };
        tree.sessions[0].windows[0].panes.insert(item.id, item);
        add(
            name,
            unchecked_control(&ProtocolMessage::Event(Event {
                sequence: 0,
                payload: EventPayload::Snapshot(tree),
            }))?,
        )?;
    }
    let viewport = TerminalViewport::blank(2, 1, SessionStatus::Running);
    let viewport_frame = zz_protocol::encode_terminal_viewport_event(PaneId(0), 0, &viewport)?;
    let viewport_body = &viewport_frame[11..];
    let viewport_blob = postcard::to_stdvec(serde_bytes::Bytes::new(viewport_body))?;
    let mut preview_tree = minimal.clone();
    let mut preview_pane = pane.clone();
    preview_pane.mode = Some(mode(3));
    preview_tree.sessions[0].windows[0]
        .panes
        .insert(preview_pane.id, preview_pane);
    let preview_frame = event_bytes(EventPayload::Snapshot(preview_tree))?;
    let viewport_offset = find_bytes(&preview_frame, &viewport_blob)?;
    add(
        "embedded-viewport-truncated",
        replace_field(
            &preview_frame,
            viewport_offset,
            viewport_blob.len(),
            &postcard::to_stdvec(serde_bytes::Bytes::new(&[0]))?,
        )?,
    )?;
    let mut trailing_viewport = viewport_body.to_vec();
    trailing_viewport.push(0);
    add(
        "embedded-viewport-trailing",
        replace_field(
            &preview_frame,
            viewport_offset,
            viewport_blob.len(),
            &postcard::to_stdvec(serde_bytes::Bytes::new(&trailing_viewport))?,
        )?,
    )?;
    let nested = Batch {
        sequence: 1,
        frames: vec![encode_protocol_message(&ProtocolMessage::Batch(
            Batch::default(),
        ))?],
    };
    add(
        "nested-batch",
        unchecked_control(&ProtocolMessage::Batch(nested))?,
    )?;
    let excessive = Batch {
        sequence: 1,
        frames: vec![encode_protocol_message(&ProtocolMessage::TreeSync)?; 4097],
    };
    add(
        "batch-count-4097",
        unchecked_control(&ProtocolMessage::Batch(excessive))?,
    )?;
    let mut depth = LayoutNode::Empty;
    for count in 1..=256 {
        depth = LayoutNode::Split {
            id: SplitId(0),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(depth),
            second: Box::new(LayoutNode::Empty),
        };
        if count == 255 || count == 256 {
            let mut tree = minimal.clone();
            tree.sessions[0].windows[0].layout = depth.clone();
            let frame = unchecked_control(&ProtocolMessage::Event(Event {
                sequence: 0,
                payload: EventPayload::Snapshot(tree),
            }))?;
            add(&format!("layout-depth-{}", count + 1), frame)?;
        }
    }
    let mut boundary = minimal.clone();
    boundary.sessions[0].windows[0].layout = balanced_layout(15);
    add(
        "layout-nodes-65535",
        unchecked_control(&ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::Snapshot(boundary.clone()),
        }))?,
    )?;
    boundary.sessions[0].windows[0].layout = LayoutNode::Split {
        id: SplitId(0),
        axis: Axis::Vertical,
        ratio: 0.5,
        first: Box::new(std::mem::replace(
            &mut boundary.sessions[0].windows[0].layout,
            LayoutNode::Empty,
        )),
        second: Box::new(LayoutNode::Empty),
    };
    add(
        "layout-nodes-65537",
        unchecked_control(&ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::Snapshot(boundary),
        }))?,
    )?;
    Ok((rejected, accepted))
}

fn cursor() -> PromptCursor {
    PromptCursor {
        style: 2,
        colour: Some(TmuxColour::Rgb(0x12_3456)),
    }
}

fn preview(index: usize) -> ChooserPreview {
    match index % 5 {
        0 => ChooserPreview::Tiles {
            tiles: vec![ChooserPreviewTile {
                label: "tile 世界".to_owned(),
                label_style: "fg=red".to_owned(),
                border_style: "fg=blue".to_owned(),
                viewport: Some(TerminalViewport::blank(2, 1, SessionStatus::Running)),
            }],
            current: 0,
        },
        1 => ChooserPreview::Screen {
            viewport: TerminalViewport::blank(2, 1, SessionStatus::Running),
        },
        2 => ChooserPreview::Text {
            lines: vec!["first".to_owned(), "世界".to_owned()],
        },
        3 => ChooserPreview::Client {
            viewport: Some(TerminalViewport::blank(2, 1, SessionStatus::Running)),
            status: vec!["status".to_owned()],
            status_style: "bold".to_owned(),
            status_width: 80,
            border_style: "fg=green".to_owned(),
        },
        _ => ChooserPreview::Markup {
            lines: vec!["#[bold]markup".to_owned()],
        },
    }
}

fn customize_mode(index: usize) -> PaneMode {
    PaneMode::Customize {
        state: ChooseTreeState {
            items: vec![ChooseTreeItem {
                label: "item".to_owned(),
                detail: "detail 世界".to_owned(),
                target: ChooseTreeTarget::Pane(PaneId(u64::MAX)),
                depth: 2,
                flags: 15,
                pane_kind: Some(ChooseTreePaneKind::Terminal),
                key: "1".to_owned(),
                text: "entry".to_owned(),
            }],
            search: Some(ChooseTreeSearchState {
                query: "世界".to_owned(),
                reverse: true,
            }),
            selected: 0,
            kind: ChooseTreeKind::Windows,
            filter_no_matches: true,
            prompt: "choose:".to_owned(),
            help: true,
        },
        presentation: Box::new(ChooserPresentation {
            selected: 0,
            rows: vec![ChooserRow {
                name: "name".to_owned(),
                text: "text".to_owned(),
                align: true,
            }],
            sort: "index".to_owned(),
            view: "tree".to_owned(),
            filter: true,
            selection_style: "reverse".to_owned(),
            border_style: "fg=blue".to_owned(),
            prompt_style: "bold".to_owned(),
            preview_size: ChooserPreviewSize::Big,
            preview: Some(preview(index)),
            prompt_cursor: cursor(),
            prompt_column: 3,
            prompt_line: "prompt 世界".to_owned(),
        }),
        offset: 12,
        prompt: "find:".to_owned(),
        prompt_cursor: 3,
        prompt_top: true,
    }
}

fn mode(index: usize) -> PaneMode {
    match index % 4 {
        0 => PaneMode::Clock {
            time: "17:08".to_owned(),
            colour: "theme0".to_owned(),
        },
        1 => PaneMode::Switch {
            rows: vec!["one".to_owned(), "世界".to_owned()],
            selected: 1,
            offset: 2,
            selection_style: "reverse".to_owned(),
            prompt: "search".to_owned(),
            prompt_style: "bold".to_owned(),
            prompt_cursor: 2,
            matches: vec![vec![0, 2], vec![]],
            match_style: "fg=red".to_owned(),
            prompt_shape: cursor(),
        },
        2 => customize_mode(index),
        _ => PaneMode::Panes {
            areas: vec![PanesModeArea {
                pane: PaneId(u64::MAX),
                number: 17,
                x: 1,
                y: 2,
                width: 80,
                height: 24,
                colour: "colour123".to_owned(),
                label: "area 世界".to_owned(),
                viewport: Some(TerminalViewport::blank(2, 1, SessionStatus::Running)),
            }],
            borders: vec![PanesModeBorder {
                x: 3,
                y: 4,
                cell: b'|',
            }],
            border_style: "fg=red".to_owned(),
            copy: true,
            format: true,
            clears: vec![PanesModeClear {
                before: 1,
                x: 5,
                y: 6,
                width: 7,
                height: 8,
            }],
        },
    }
}

fn pane(index: usize) -> PaneSnapshot {
    let kind = match index % 5 {
        0 => PaneKindSnapshot::Picker,
        1 => PaneKindSnapshot::Terminal,
        2 => PaneKindSnapshot::Browser(BrowserDescriptor {
            tabs: vec![
                "https://example.com/世界".to_owned(),
                "about:blank".to_owned(),
            ],
            active_tab: 1,
            profile: "work".to_owned(),
        }),
        3 => PaneKindSnapshot::Editor(EditorDescriptor {
            path: Some("/tmp/zz/file 世界.rs".to_owned()),
            cwd: "/tmp/zz".to_owned(),
        }),
        _ => PaneKindSnapshot::Agent(AgentDescriptor {
            provider: AgentProvider::ClaudeCode,
            cwd: Some(PathBuf::from("/tmp/zz")),
            session_id: Some("session 世界".to_owned()),
            model: Some("model".to_owned()),
            effort: Some("high".to_owned()),
        }),
    };
    let states = [
        PaneStatusState::Idle,
        PaneStatusState::Working,
        PaneStatusState::Done,
        PaneStatusState::Blocked,
        PaneStatusState::Error,
    ];
    let kinds = [
        PaneStatusKind::Permission,
        PaneStatusKind::Question,
        PaneStatusKind::Auth,
    ];
    PaneSnapshot {
        id: PaneId(u64::MAX - u64::try_from(index).expect("small fixture index")),
        title: format!("pane {index} 世界"),
        kind,
        synchronized_input: true,
        bell: true,
        dead: true,
        dead_status: Some(u32::MAX),
        border_colour: Some(TmuxColour::Basic(7)),
        active_border_colour: Some(TmuxColour::Rgb(0x12_3456)),
        border_status_text: "pane status 世界".to_owned(),
        mode: Some(mode(index)),
        status: Some(PaneStatus {
            state: states[index % states.len()],
            kind: Some(kinds[index % kinds.len()]),
            progress: Some(99),
            app: "app".to_owned(),
            title: "status title".to_owned(),
            message: "status message 世界".to_owned(),
        }),
        summary: PaneSummary {
            changed_at: u64::MAX,
            unseen: true,
            command: Some(if index.is_multiple_of(2) {
                ShellCommand::Running
            } else {
                ShellCommand::Finished(255)
            }),
            name: "@pane".to_owned(),
            cwd: "/tmp/zz/世界".to_owned(),
            repo: "/tmp/zz".to_owned(),
            branch: "main".to_owned(),
            process: vec!["shell".to_owned(), "editor 世界".to_owned()],
            flip_armed: true,
            shell: if index.is_multiple_of(2) {
                String::new()
            } else {
                "fish".to_owned()
            },
        },
    }
}

fn window() -> WindowSnapshot {
    let panes = (0..5)
        .map(pane)
        .map(|pane| (pane.id, pane))
        .collect::<BTreeMap<_, _>>();
    let first = PaneId(u64::MAX);
    let second = PaneId(u64::MAX - 1);
    WindowSnapshot {
        id: WindowId(u64::MAX - 10),
        index: u32::MAX,
        name: "window 世界".to_owned(),
        automatic_rename: true,
        active_pane: first,
        zoomed_pane: Some(second),
        layout: LayoutNode::Split {
            id: SplitId(u64::MAX - 20),
            axis: Axis::Horizontal,
            ratio: 0.375,
            first: Box::new(LayoutNode::Pane(first)),
            second: Box::new(LayoutNode::Split {
                id: SplitId(128),
                axis: Axis::Vertical,
                ratio: 0.625,
                first: Box::new(LayoutNode::Pane(second)),
                second: Box::new(LayoutNode::Empty),
            }),
        },
        panes,
        layout_dump: "layout".to_owned(),
        visible_layout_dump: "visible".to_owned(),
        status_label: "status 世界".to_owned(),
        activity: true,
        silence: true,
        pane_border_status: PaneBorderStatus::Top,
        pane_border_lines: PaneBorderLines::Double,
        pane_border_indicators: PaneBorderIndicators::Both,
        pane_order: vec![first, second],
        pane_z_order: vec![second, first],
        floating: vec![FloatingPaneSnapshot {
            pane: second,
            xoff: i32::MIN,
            yoff: i32::MAX,
            sx: u16::MAX,
            sy: 128,
            visible: true,
            border_lines: PaneBorderLines::Heavy,
            border_status: PaneBorderStatus::Bottom,
        }],
        modal: Some(ModalPaneSnapshot {
            pane: first,
            capture_keys: true,
            close_on_click: true,
            close_on_cancel: true,
        }),
        sx: u16::MAX,
        sy: 127,
        pane_base_index: u32::MAX,
    }
}

fn snapshot() -> MuxSnapshot {
    let window = window();
    MuxSnapshot {
        generation: 17,
        focused_window: Some(window.id),
        sessions: vec![SessionSnapshot {
            id: SessionId(u64::MAX - 30),
            name: "session 世界".to_owned(),
            active_window: window.id,
            viewers: vec![SessionViewer {
                name: "viewer 世界".to_owned(),
                window: window.id,
                is_self: true,
            }],
            windows: vec![window],
        }],
    }
}

fn minimal_snapshot() -> MuxSnapshot {
    let mut pane = pane(1);
    pane.id = PaneId(0);
    pane.title.clear();
    pane.synchronized_input = false;
    pane.bell = false;
    pane.dead = false;
    pane.dead_status = None;
    pane.border_colour = None;
    pane.active_border_colour = None;
    pane.border_status_text.clear();
    pane.mode = None;
    pane.status = None;
    pane.summary = PaneSummary::default();
    let mut window = window();
    window.id = WindowId(0);
    window.index = 0;
    window.name.clear();
    window.active_pane = pane.id;
    window.zoomed_pane = None;
    window.layout = LayoutNode::Pane(pane.id);
    window.panes = BTreeMap::from([(pane.id, pane)]);
    window.layout_dump.clear();
    window.visible_layout_dump.clear();
    window.status_label.clear();
    window.activity = false;
    window.silence = false;
    window.pane_border_status = PaneBorderStatus::Off;
    window.pane_border_lines = PaneBorderLines::None;
    window.pane_border_indicators = PaneBorderIndicators::Off;
    window.pane_order.clear();
    window.pane_z_order.clear();
    window.floating.clear();
    window.modal = None;
    window.sx = 0;
    window.sy = 0;
    window.pane_base_index = 0;
    MuxSnapshot {
        generation: 0,
        sessions: vec![SessionSnapshot {
            id: SessionId(0),
            name: String::new(),
            active_window: window.id,
            windows: vec![window],
            viewers: Vec::new(),
        }],
        focused_window: None,
    }
}

fn ops(snapshot: &MuxSnapshot) -> Vec<TreeOp> {
    let session_value = &snapshot.sessions[0];
    let window_value = &session_value.windows[0];
    let pane_value = &window_value.panes[&window_value.active_pane];
    let session = session_value.id;
    let window = window_value.id;
    let pane = pane_value.id;
    vec![
        TreeOp::UpsertSession(session_value.clone()),
        TreeOp::RemoveSession(session),
        TreeOp::SessionName {
            session,
            name: "renamed 世界".to_owned(),
        },
        TreeOp::SessionActiveWindow { session, window },
        TreeOp::SessionViewers {
            session,
            viewers: session_value.viewers.clone(),
        },
        TreeOp::UpsertWindow {
            session,
            window: window_value.clone(),
        },
        TreeOp::RemoveWindow { session, window },
        TreeOp::WindowName {
            session,
            window,
            name: "window renamed".to_owned(),
        },
        TreeOp::WindowActivePane {
            session,
            window,
            pane,
        },
        TreeOp::WindowLayout {
            session,
            window,
            layout: window_value.layout.clone(),
            zoomed_pane: window_value.zoomed_pane,
            layout_dump: window_value.layout_dump.clone(),
            visible_layout_dump: window_value.visible_layout_dump.clone(),
            pane_order: window_value.pane_order.clone(),
            pane_z_order: window_value.pane_z_order.clone(),
            floating: window_value.floating.clone(),
            modal: window_value.modal,
            sx: window_value.sx,
            sy: window_value.sy,
        },
        TreeOp::WindowFlags {
            session,
            window,
            index: 127,
            automatic_rename: false,
            activity: false,
            silence: false,
            pane_base_index: 3,
        },
        TreeOp::WindowPresentation {
            session,
            window,
            status_label: "new status".to_owned(),
            pane_border_status: PaneBorderStatus::Bottom,
            pane_border_lines: PaneBorderLines::Number,
            pane_border_indicators: PaneBorderIndicators::Arrows,
        },
        TreeOp::UpsertPane {
            session,
            window,
            pane: Box::new(pane_value.clone()),
        },
        TreeOp::RemovePane {
            session,
            window,
            pane,
        },
        TreeOp::PaneTitle {
            session,
            window,
            pane,
            title: "title renamed".to_owned(),
        },
        TreeOp::PaneState {
            session,
            window,
            pane,
            kind: PaneKindSnapshot::Terminal,
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
        },
        TreeOp::PanePresentation {
            session,
            window,
            pane,
            border_colour: Some(TmuxColour::Indexed(255)),
            active_border_colour: Some(TmuxColour::Theme(9)),
            border_status_text: "border status".to_owned(),
            mode: Some(mode(1)),
        },
        TreeOp::PaneStatus {
            session,
            window,
            pane,
            status: None,
        },
        TreeOp::PaneSummary {
            session,
            window,
            pane,
            summary: PaneSummary::default(),
        },
    ]
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut messages = Vec::new();
    let client = ClientHello {
        protocol_version: PROTOCOL_VERSION,
        client_instance_id: ClientInstanceId(u64::MAX),
        kind: ClientKind::Interactive,
        device_name: Some("zz Mojo".to_owned()),
        capabilities: vec![
            ClientHello::CLIENT_TERMINAL_CAPABILITY.to_owned(),
            ClientHello::CLIENT_UTF8_CAPABILITY.to_owned(),
            "control-plane-v2".to_owned(),
        ],
        color_scheme: None,
        origin: None,
        working_directory: None,
        environment: Vec::new(),
        process_id: u32::MAX,
    };
    let hello = Hello {
        client,
        viewport: Some(ClientViewport {
            columns: 120,
            rows: 40,
            cell_width_px: 9,
            cell_height_px: 18,
        }),
        subscriptions: Subscriptions {
            tree: TreeSubscription::All,
            status: true,
            options: 0,
            keys: KeySubscription::None,
            pane_stream: true,
        },
        attach: Some(AttachOperation::Session("$18446744073709551585".to_owned())),
        environment: ClientEnvironmentBlob::from_bytes(Vec::new()),
    };
    messages.push(fixture("hello", &ProtocolMessage::Hello(hello))?);
    messages.push(fixture(
        "welcome",
        &ProtocolMessage::Welcome(Welcome {
            protocol_version: PROTOCOL_VERSION,
            server_id: u64::MAX,
            client_id: ClientId(128),
            client_instance_id: ClientInstanceId(u64::MAX),
            caps: (1 << CAPABILITY_NAMES.len()) - 1,
            clock: u64::MAX,
            platform: HostPlatform::default(),
        }),
    )?);
    for platform in [
        HostPlatform::Macos,
        HostPlatform::Windows,
        HostPlatform::Linux(LinuxDistro::Other),
        HostPlatform::Linux(LinuxDistro::Ubuntu),
        HostPlatform::Linux(LinuxDistro::Debian),
        HostPlatform::Linux(LinuxDistro::Arch),
        HostPlatform::Linux(LinuxDistro::Fedora),
    ] {
        messages.push(fixture(
            &format!("welcome-{}", platform.slug().unwrap_or("unknown")),
            &ProtocolMessage::Welcome(Welcome {
                protocol_version: PROTOCOL_VERSION,
                server_id: 1,
                client_id: ClientId(2),
                client_instance_id: ClientInstanceId(3),
                caps: (1 << CAPABILITY_NAMES.len()) - 1,
                clock: 4,
                platform,
            }),
        )?);
    }
    let base = snapshot();
    let base_bytes = event_bytes(EventPayload::Snapshot(base.clone()))?;
    messages.push(fixture(
        "snapshot-empty",
        &ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::Snapshot(MuxSnapshot::default()),
        }),
    )?);
    messages.push(fixture(
        "snapshot-minimal",
        &ProtocolMessage::Event(Event {
            sequence: 1,
            payload: EventPayload::Snapshot(minimal_snapshot()),
        }),
    )?);
    messages.push(fixture(
        "snapshot-full",
        &ProtocolMessage::Event(Event {
            sequence: u64::MAX,
            payload: EventPayload::Snapshot(base.clone()),
        }),
    )?);
    for index in 0..5 {
        let mut variant = base.clone();
        let pane = variant.sessions[0].windows[0]
            .panes
            .values_mut()
            .next()
            .expect("fixture has panes");
        pane.mode = Some(customize_mode(index));
        messages.push(fixture(
            &format!("chooser-preview-{index}"),
            &ProtocolMessage::Event(Event {
                sequence: u64::try_from(index).expect("small fixture index"),
                payload: EventPayload::Snapshot(variant),
            }),
        )?);
    }
    let mut reductions = Vec::new();
    for (index, op) in ops(&base).into_iter().enumerate() {
        let delta = TreeDelta {
            base: base.generation,
            version: 18,
            ops: vec![op],
        };
        let mut result = base.clone();
        delta.apply(&mut result).map_err(|error| {
            std::io::Error::other(format!("fixture reduction failed: {error:?}"))
        })?;
        messages.push(fixture(
            &format!("tree-op-{index}"),
            &ProtocolMessage::Event(Event {
                sequence: u64::try_from(index).expect("small fixture index"),
                payload: EventPayload::TreeDelta(delta.clone()),
            }),
        )?);
        let delta_bytes = event_bytes(EventPayload::TreeDelta(delta.clone()))?;
        let after_bytes = event_bytes(EventPayload::Snapshot(result.clone()))?;
        reductions.push(json!({"name": format!("tree-op-{index}"), "before": &base, "delta": delta, "after": result, "before_bytes": &base_bytes, "delta_bytes": delta_bytes, "after_bytes": after_bytes}));
    }
    let session_id = base.sessions[0].id;
    let window_id = base.sessions[0].windows[0].id;
    let bad = PaneId(1234);
    let mut rejected = Vec::new();
    for (name, delta) in [
        (
            "base",
            TreeDelta {
                base: 18,
                version: 19,
                ops: Vec::new(),
            },
        ),
        (
            "session",
            TreeDelta {
                base: 17,
                version: 18,
                ops: vec![TreeOp::SessionName {
                    session: SessionId(1234),
                    name: "bad".to_owned(),
                }],
            },
        ),
        (
            "window",
            TreeDelta {
                base: 17,
                version: 18,
                ops: vec![TreeOp::WindowName {
                    session: session_id,
                    window: WindowId(1234),
                    name: "bad".to_owned(),
                }],
            },
        ),
        (
            "pane",
            TreeDelta {
                base: 17,
                version: 18,
                ops: vec![TreeOp::PaneTitle {
                    session: session_id,
                    window: window_id,
                    pane: bad,
                    title: "bad".to_owned(),
                }],
            },
        ),
        (
            "atomic",
            TreeDelta {
                base: 17,
                version: 18,
                ops: vec![
                    TreeOp::SessionName {
                        session: session_id,
                        name: "must roll back".to_owned(),
                    },
                    TreeOp::PaneTitle {
                        session: session_id,
                        window: window_id,
                        pane: bad,
                        title: "bad".to_owned(),
                    },
                ],
            },
        ),
    ] {
        let mut result = base.clone();
        let error = delta
            .apply(&mut result)
            .expect_err("rejected fixture must fail");
        assert_eq!(result, base, "rejected reduction {name} mutated its input");
        let message = ProtocolMessage::Event(Event {
            sequence: 18,
            payload: EventPayload::TreeDelta(delta.clone()),
        });
        let encoded = fixture(&format!("rejected-{name}"), &message)?;
        let delta_bytes = event_bytes(EventPayload::TreeDelta(delta.clone()))?;
        rejected.push(json!({"name":name,"before":&base,"delta":delta,"bytes":encoded["bytes"],"error":format!("{error:?}"),"after":result, "before_bytes": &base_bytes, "delta_bytes": delta_bytes, "after_bytes": &base_bytes}));
    }
    let view = ClientView {
        session: Some(base.sessions[0].id),
        focused_window: Some(base.sessions[0].windows[0].id),
        read_only: true,
        client_flags: "read-only,ignore-size".to_owned(),
        layout_generation: u64::MAX,
        attachment_generation: 128,
        overlay: vec![TreeOp::PaneTitle {
            session: base.sessions[0].id,
            window: base.sessions[0].windows[0].id,
            pane: base.sessions[0].windows[0].active_pane,
            title: "local title 世界".to_owned(),
        }],
    };
    let mut viewed = base.clone();
    view.apply(&mut viewed)
        .map_err(|error| std::io::Error::other(format!("fixture client view failed: {error:?}")))?;
    let view_bytes = event_bytes(EventPayload::ClientView(view.clone()))?;
    let viewed_bytes = event_bytes(EventPayload::Snapshot(viewed.clone()))?;
    let view_reductions = vec![
        json!({"before":&base,"view":&view,"after":viewed, "before_bytes": &base_bytes, "view_bytes": view_bytes, "after_bytes": viewed_bytes}),
    ];
    messages.push(fixture(
        "client-view",
        &ProtocolMessage::Event(Event {
            sequence: 127,
            payload: EventPayload::ClientView(view),
        }),
    )?);
    messages.push(fixture(
        "request-full",
        &ProtocolMessage::RequestFull {
            pane: PaneId(u64::MAX),
        },
    )?);
    messages.push(fixture("tree-sync", &ProtocolMessage::TreeSync)?);
    messages.push(fixture(
        "resize-terminal-v2",
        &ProtocolMessage::Input(InputMessage::ResizeTerminalV2 {
            pane: PaneId(u64::MAX),
            columns: 120,
            rows: 40,
            cell_width_px: 9,
            cell_height_px: 18,
            layout_generation: u64::MAX,
        }),
    )?);
    messages.push(fixture(
        "client-size-v2",
        &ProtocolMessage::Input(InputMessage::ClientTerminalSizeV2 {
            columns: 120,
            rows: 40,
            layout_generation: u64::MAX,
        }),
    )?);
    messages.push(fixture(
        "client-focus",
        &ProtocolMessage::Input(InputMessage::ClientFocus { focused: true }),
    )?);
    messages.push(fixture(
        "text-input",
        &ProtocolMessage::Input(InputMessage::Text {
            pane: PaneId(u64::MAX),
            text: "世界\n".to_owned(),
        }),
    )?);
    messages.push(fixture(
        "ping",
        &ProtocolMessage::Ping {
            token: u64::MAX,
            rtt_us: u32::MAX,
        },
    )?);
    messages.push(fixture("pong", &ProtocolMessage::Pong { token: u64::MAX })?);
    let batch = Batch::from_messages(
        128,
        [
            ProtocolMessage::Event(Event {
                sequence: 17,
                payload: EventPayload::Snapshot(base),
            }),
            ProtocolMessage::RequestFull {
                pane: PaneId(u64::MAX),
            },
        ],
    )?;
    messages.push(fixture("batch", &ProtocolMessage::Batch(batch))?);
    let (rejected_frames, accepted_frames) = malformed_frames()?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"protocol_version": PROTOCOL_VERSION, "messages": messages, "reductions": reductions,"rejected_reductions":rejected,"view_reductions":view_reductions,"rejected_frames":rejected_frames,"accepted_frames":accepted_frames})
        )?
    );
    Ok(())
}
