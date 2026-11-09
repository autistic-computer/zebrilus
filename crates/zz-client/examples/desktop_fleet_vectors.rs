use std::{collections::BTreeMap, error::Error, fs};

use serde_json::json;
use zz_client::{
    desktop_fleet::{self, FleetPaneKind, FleetSession},
    desktop_host::{self, HostIndicator, HostState, InstallOffer},
    desktop_nav::{NavActivation, NavTarget},
    desktop_nav_tree::{NavNode, project_tree},
};
use zz_protocol::{
    AgentDescriptor, AgentProvider, Axis, BrowserDescriptor, CommandInvocation, CommandRequest,
    EditorDescriptor, Event, EventPayload, LayoutNode, MuxSnapshot, PaneBorderIndicators,
    PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus,
    PaneStatusState, PaneSummary, ProtocolMessage, SessionId, SessionSnapshot, SplitId, WindowId,
    WindowSnapshot, decode_protocol_frame, encode_protocol_message,
};

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }

    fn pick<T: Clone>(&mut self, values: &[T]) -> T {
        values[usize::try_from(self.below(values.len() as u64)).unwrap_or_default()].clone()
    }
}

fn uint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        bytes.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn flag(bytes: &mut Vec<u8>, value: bool) {
    bytes.push(u8::from(value));
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    uint(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn optional_text(bytes: &mut Vec<u8>, value: Option<&str>) {
    flag(bytes, value.is_some());
    if let Some(value) = value {
        text(bytes, value);
    }
}

fn optional_uint(bytes: &mut Vec<u8>, value: Option<u64>) {
    flag(bytes, value.is_some());
    if let Some(value) = value {
        uint(bytes, value);
    }
}

fn node(bytes: &mut Vec<u8>, value: NavNode<u32>) {
    let (host, kind, id) = match value {
        NavNode::Host(host) => (host, 0, 0),
        NavNode::Target(host, NavTarget::Session(id)) => (host, 1, id.0),
        NavNode::Target(host, NavTarget::Window(id)) => (host, 2, id.0),
        NavNode::Target(host, NavTarget::Pane(id)) => (host, 3, id.0),
    };
    uint(bytes, u64::from(host));
    bytes.push(kind);
    uint(bytes, id);
}

fn command_frame(command: &CommandInvocation) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::CommandRequest(
        CommandRequest {
            request_id: 0,
            command: command.clone(),
            prepared: false,
        },
    ))?)
}

fn activation(
    bytes: &mut Vec<u8>,
    value: Option<NavActivation<u32>>,
) -> Result<(), Box<dyn Error>> {
    let Some(value) = value else {
        flag(bytes, false);
        return Ok(());
    };
    flag(bytes, true);
    let (kind, host, session, target, commands) = match value {
        NavActivation::AttachHost(host) => (0, host, None, String::new(), Vec::new()),
        NavActivation::Attach { host, session } => {
            (1, host, Some(session), String::new(), Vec::new())
        }
        NavActivation::AttachAt { host, target } => (2, host, None, target, Vec::new()),
        NavActivation::Execute { host, commands } => (3, host, None, String::new(), commands),
        NavActivation::AttachThenExecute {
            host,
            session,
            commands,
        } => (4, host, Some(session), String::new(), commands),
        NavActivation::Reconnect(host) => (5, host, None, String::new(), Vec::new()),
    };
    bytes.push(kind);
    uint(bytes, u64::from(host));
    optional_uint(bytes, session.map(|session| session.0));
    text(bytes, &target);
    uint(bytes, commands.len() as u64);
    for command in &commands {
        let frame = command_frame(command)?;
        uint(bytes, frame.len() as u64);
        bytes.extend(frame);
    }
    Ok(())
}

fn state_input(bytes: &mut Vec<u8>, state: &HostState) {
    match state {
        HostState::Disconnected => bytes.push(0),
        HostState::Connecting => bytes.push(1),
        HostState::Reconnecting { attempt } => {
            bytes.push(2);
            uint(bytes, u64::from(*attempt));
        }
        HostState::Connected => bytes.push(3),
        HostState::Unreachable { reason } => {
            bytes.push(4);
            text(bytes, reason);
        }
        HostState::Incompatible { local, remote } => {
            bytes.push(5);
            uint(bytes, u64::from(*local));
            optional_uint(bytes, remote.map(u64::from));
        }
        HostState::Missing => bytes.push(6),
        HostState::Installing => bytes.push(7),
    }
}

fn pane(rng: &mut Rng, id: u64) -> PaneSnapshot {
    let kind = match rng.below(7) {
        0 => PaneKindSnapshot::Picker,
        1 | 2 => PaneKindSnapshot::Terminal,
        3 => PaneKindSnapshot::Browser(BrowserDescriptor {
            tabs: rng.pick(&[
                Vec::new(),
                vec![" https://example.test/λ ".to_owned()],
                vec![" ".to_owned(), "url".to_owned()],
            ]),
            active_tab: usize::try_from(rng.below(3)).unwrap_or_default(),
            profile: String::new(),
        }),
        4 => PaneKindSnapshot::Editor(EditorDescriptor {
            path: None,
            cwd: "/tmp".to_owned(),
        }),
        _ => PaneKindSnapshot::Agent(AgentDescriptor {
            provider: if rng.below(2) == 0 {
                AgentProvider::Codex
            } else {
                AgentProvider::ClaudeCode
            },
            cwd: None,
            session_id: None,
            model: None,
            effort: None,
        }),
    };
    let process: Vec<String> = rng
        .pick(&[
            &[][..],
            &["/opt/homebrew/bin/nvim", "nvim"][..],
            &["/usr/local/bin/frobnicate", "frobnicate"][..],
            &["C:\\bin\\NVIM.EXE"][..],
            &["claude"][..],
            &["codex", "node"][..],
            &["/usr/bin/"][..],
            &["python3.12"][..],
        ])
        .iter()
        .map(|entry| (*entry).to_owned())
        .collect();
    PaneSnapshot {
        id: PaneId(id),
        title: rng
            .pick(&[
                "",
                " ",
                "agent",
                "  vim ~/dev/zz λ ",
                "Fix the build",
                "bash",
            ])
            .to_owned(),
        kind,
        synchronized_input: false,
        bell: rng.below(4) == 0,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: (rng.below(3) == 0).then(|| PaneStatus {
            state: rng.pick(&[
                PaneStatusState::Idle,
                PaneStatusState::Working,
                PaneStatusState::Done,
                PaneStatusState::Blocked,
                PaneStatusState::Error,
            ]),
            kind: None,
            progress: None,
            app: String::new(),
            title: String::new(),
            message: String::new(),
        }),
        summary: PaneSummary {
            cwd: rng.pick(&["", "/home/me/dev/zz", "////"]).to_owned(),
            repo: rng.pick(&["", "/repo/one/"]).to_owned(),
            process,
            shell: rng.pick(&["", "fish", "zsh"]).to_owned(),
            ..PaneSummary::default()
        },
    }
}

fn window(rng: &mut Rng, number: usize) -> WindowSnapshot {
    let mut panes = BTreeMap::new();
    for _ in 0..rng.below(5) {
        let id = rng.below(8);
        let pane = pane(rng, id);
        panes.insert(pane.id, pane);
    }
    let mut layout = LayoutNode::Empty;
    for (index, id) in panes.keys().rev().copied().enumerate() {
        if rng.below(4) == 0 {
            continue;
        }
        layout = match layout {
            LayoutNode::Empty => LayoutNode::Pane(id),
            other => LayoutNode::Split {
                id: SplitId(index as u64),
                axis: Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(id)),
                second: Box::new(other),
            },
        };
    }
    if rng.below(5) == 0 {
        layout = LayoutNode::Split {
            id: SplitId(99),
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Box::new(layout),
            second: Box::new(LayoutNode::Pane(PaneId(99))),
        };
    }
    let pane_order = if rng.below(3) == 0 {
        panes.keys().rev().copied().collect()
    } else {
        Vec::new()
    };
    WindowSnapshot {
        id: WindowId(rng.below(6)),
        index: u32::try_from(rng.below(4)).unwrap_or_default(),
        name: format!("w{number} {}", rng.pick(&["zsh", "λ", ""])),
        automatic_rename: false,
        active_pane: PaneId(rng.below(8)),
        zoomed_pane: None,
        layout,
        panes,
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::default(),
        pane_border_lines: PaneBorderLines::default(),
        pane_border_indicators: PaneBorderIndicators::default(),
        pane_order,
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: rng.pick(&[0, 1, u32::MAX]),
    }
}

fn snapshot(rng: &mut Rng) -> MuxSnapshot {
    let sessions = (0..rng.below(4))
        .map(|number| SessionSnapshot {
            id: SessionId(rng.below(5)),
            name: rng.pick(&["", "0", "12", " work ", "λ", "08"]).to_owned(),
            active_window: WindowId(rng.below(6)),
            windows: (0..rng.below(4))
                .map(|index| window(rng, usize::try_from(number * 4 + index).unwrap_or_default()))
                .collect(),
            viewers: Vec::new(),
        })
        .collect();
    MuxSnapshot {
        generation: 1,
        sessions,
        focused_window: (rng.below(2) == 0).then(|| WindowId(rng.below(6))),
    }
}

fn state(rng: &mut Rng) -> HostState {
    match rng.below(10) {
        0 => HostState::Disconnected,
        1 => HostState::Connecting,
        2 => HostState::Reconnecting {
            attempt: u32::try_from(rng.below(5)).unwrap_or_default(),
        },
        3 => HostState::Unreachable {
            reason: "ssh: λ timed out\nretry".to_owned(),
        },
        4 => HostState::Incompatible {
            local: 120,
            remote: rng.pick(&[None, Some(118), Some(120), Some(121)]),
        },
        5 => HostState::Missing,
        6 => HostState::Installing,
        _ => HostState::Connected,
    }
}

struct Host {
    id: u32,
    name: String,
    state: HostState,
    snapshot: Option<MuxSnapshot>,
    sessions: Vec<FleetSession>,
}

fn host(hosts: &[Host], id: u32) -> Option<&Host> {
    hosts.iter().find(|host| host.id == id)
}

fn kind_bytes(kind: FleetPaneKind) -> [u8; 2] {
    match kind {
        FleetPaneKind::Picker => [0, 0],
        FleetPaneKind::Terminal => [1, 0],
        FleetPaneKind::Browser => [2, 0],
        FleetPaneKind::Editor => [3, 0],
        FleetPaneKind::Agent(AgentProvider::Codex) => [4, 0],
        FleetPaneKind::Agent(AgentProvider::ClaudeCode) => [4, 1],
    }
}

fn host_presentation(bytes: &mut Vec<u8>, host: &Host) {
    text(bytes, host.state.label());
    flag(bytes, host.state.install_offer().is_some());
    if let Some(offer) = host.state.install_offer() {
        bytes.push(u8::from(offer == InstallOffer::Update));
    }
    optional_text(bytes, host.state.failure_detail().as_deref());
    match desktop_host::indicator(&host.state, host.snapshot.is_some()) {
        None => bytes.push(0),
        Some(HostIndicator::Connecting) => bytes.push(1),
        Some(HostIndicator::Failed { detail }) => {
            bytes.push(2);
            optional_text(bytes, detail.as_deref());
        }
    }
    uint(bytes, host.sessions.len() as u64);
    for session in &host.sessions {
        uint(bytes, session.id.0);
        text(bytes, &session.name);
        text(bytes, &session.label.text);
        optional_text(bytes, session.label.index.as_deref());
        flag(bytes, session.active);
        flag(bytes, session.has_pending_bell());
        uint(bytes, session.windows.len() as u64);
        for window in &session.windows {
            uint(bytes, window.id.0);
            uint(bytes, u64::from(window.index));
            text(bytes, &window.label());
            uint(bytes, window.active_pane.0);
            flag(bytes, window.active);
            optional_text(bytes, window.program().map(|program| program.id));
            uint(bytes, window.panes.len() as u64);
            for pane in &window.panes {
                uint(bytes, pane.id.0);
                uint(bytes, u64::from(pane.index));
                text(bytes, &pane.label);
                text(bytes, &pane.cwd);
                bytes.extend(kind_bytes(pane.kind));
                flag(bytes, pane.bell);
                optional_uint(bytes, pane.status.map(|state| state as u64));
                optional_text(bytes, pane.program.map(|program| program.id));
            }
        }
    }
}

fn nodes(hosts: &[Host]) -> Vec<NavNode<u32>> {
    let mut result = Vec::new();
    for host in hosts {
        result.push(NavNode::Host(host.id));
        for session in &host.sessions {
            result.push(NavNode::Target(host.id, NavTarget::Session(session.id)));
            for window in &session.windows {
                result.push(NavNode::Target(host.id, NavTarget::Window(window.id)));
                for pane in &window.panes {
                    result.push(NavNode::Target(host.id, NavTarget::Pane(pane.id)));
                }
            }
        }
    }
    result.extend([
        NavNode::Host(9),
        NavNode::Target(0, NavTarget::Session(SessionId(77))),
        NavNode::Target(0, NavTarget::Window(WindowId(77))),
        NavNode::Target(0, NavTarget::Pane(PaneId(77))),
        NavNode::Target(1, NavTarget::Pane(PaneId(3))),
    ]);
    result
}

fn query(
    bytes: &mut Vec<u8>,
    hosts: &[Host],
    value: NavNode<u32>,
    attached_host: u32,
    attached: Option<SessionId>,
) -> Result<(), Box<dyn Error>> {
    let (id, target) = match value {
        NavNode::Host(id) => (id, None),
        NavNode::Target(id, target) => (id, Some(target)),
    };
    let found = host(hosts, id);
    flag(
        bytes,
        found.is_some_and(|host| {
            target.is_none_or(|target| desktop_fleet::expandable(&host.sessions, target))
        }),
    );
    flag(
        bytes,
        found.is_some_and(|host| desktop_fleet::pending_bell(&host.sessions, target)),
    );
    optional_uint(
        bytes,
        found
            .zip(target)
            .and_then(|(host, target)| desktop_fleet::session_for_target(&host.sessions, target))
            .map(|session| session.0),
    );
    optional_text(
        bytes,
        found
            .zip(target)
            .and_then(|(host, target)| desktop_fleet::renameable_name(&host.sessions, target)),
    );
    activation(
        bytes,
        found.and_then(|host| {
            desktop_fleet::activation(value, &host.state, &host.sessions, attached_host, attached)
        }),
    )?;
    let rename = found.zip(target).and_then(|(host, target)| {
        desktop_fleet::rename_activation(id, &host.state, &host.sessions, target, attached_host)
    });
    optional_text(bytes, rename.as_ref().map(|(label, _)| *label));
    activation(bytes, rename.map(|(_, activation)| activation))
}

fn rows(bytes: &mut Vec<u8>, hosts: &[Host], expand_windows: bool) -> usize {
    let projection = project_tree(
        hosts.iter().map(|host| {
            (
                host.id,
                host.sessions.iter().map(|session| {
                    (
                        session.id,
                        session.active,
                        session.windows.iter().map(|window| {
                            (
                                window.id,
                                window.active,
                                window.active_pane,
                                window.panes.iter().map(|pane| pane.id),
                            )
                        }),
                    )
                }),
            )
        }),
        |value| expand_windows || !matches!(value, NavNode::Target(_, NavTarget::Window(_))),
    );
    uint(bytes, projection.entries.len() as u64);
    for entry in &projection.entries {
        let host = &hosts[entry.host_index];
        node(bytes, entry.node);
        bytes.push(entry.depth);
        flag(bytes, entry.expandable);
        flag(bytes, entry.expanded);
        flag(bytes, entry.on_active_path);
        let (label, kind, program) = match entry.node {
            NavNode::Host(_) => (host.name.clone(), [0, 0], None),
            NavNode::Target(_, NavTarget::Session(_)) => {
                let session = &host.sessions[entry.session_index.unwrap_or_default()];
                (session.label.text.clone(), [0, 0], None)
            }
            NavNode::Target(_, NavTarget::Window(_)) => {
                let window = &host.sessions[entry.session_index.unwrap_or_default()].windows
                    [entry.window_index.unwrap_or_default()];
                (
                    window.label(),
                    [0, 0],
                    if entry.expanded {
                        None
                    } else {
                        window.program()
                    },
                )
            }
            NavNode::Target(_, NavTarget::Pane(_)) => {
                let pane = &host.sessions[entry.session_index.unwrap_or_default()].windows
                    [entry.window_index.unwrap_or_default()]
                .panes[entry.pane_index.unwrap_or_default()];
                (pane.label.clone(), kind_bytes(pane.kind), pane.program)
            }
        };
        text(bytes, &label);
        bytes.extend(kind);
        optional_text(bytes, program.map(|program| program.id));
    }
    projection.entries.len()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let request_path = arguments.next().ok_or("request path required")?;
    let expected_path = arguments.next().ok_or("expected path required")?;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut request = Vec::new();
    let mut expected = Vec::new();
    let mut ends = Vec::new();
    let mut counts = BTreeMap::from([
        ("hosts", 0_usize),
        ("panes", 0),
        ("queries", 0),
        ("rows", 0),
        ("shortcuts", 0),
    ]);
    let cases = 320;
    uint(&mut request, cases);
    for _ in 0..cases {
        let attached_host = u32::try_from(rng.below(4))?;
        let attached = (rng.below(4) != 0).then(|| SessionId(rng.below(5)));
        let mut hosts = Vec::new();
        for _ in 0..rng.below(5) {
            let snapshot = (rng.below(5) != 0).then(|| snapshot(&mut rng));
            hosts.push(Host {
                id: u32::try_from(rng.below(4))?,
                name: rng.pick(&["local", "box", "λ-desk", ""]).to_owned(),
                state: state(&mut rng),
                snapshot,
                sessions: Vec::new(),
            });
        }
        uint(&mut request, u64::from(attached_host));
        optional_uint(&mut request, attached.map(|session| session.0));
        uint(&mut request, hosts.len() as u64);
        let mut active = None;
        for host in &mut hosts {
            uint(&mut request, u64::from(host.id));
            text(&mut request, &host.name);
            state_input(&mut request, &host.state);
            flag(&mut request, host.snapshot.is_some());
            if let Some(snapshot) = &host.snapshot {
                let message = ProtocolMessage::Event(Event {
                    sequence: 0,
                    payload: EventPayload::Snapshot(snapshot.clone()),
                });
                let frame = encode_protocol_message(&message)?;
                if decode_protocol_frame(&frame)? != message {
                    return Err("snapshot round trip changed".into());
                }
                uint(&mut request, frame.len() as u64);
                request.extend(frame);
            }
            let (sessions, next) = desktop_fleet::sessions(
                active,
                host.id,
                attached_host,
                attached,
                host.snapshot.as_ref(),
            );
            active = next;
            host.sessions = sessions;
        }
        let hosts = hosts;
        *counts.entry("hosts").or_default() += hosts.len();
        for host in &hosts {
            host_presentation(&mut expected, host);
            *counts.entry("panes").or_default() += host
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .map(|window| window.panes.len())
                .sum::<usize>();
        }
        flag(&mut expected, active.is_some());
        if let Some(active) = active {
            node(&mut expected, active);
        }
        let queries = nodes(&hosts);
        uint(&mut request, queries.len() as u64);
        for value in &queries {
            node(&mut request, *value);
            query(&mut expected, &hosts, *value, attached_host, attached)?;
        }
        *counts.entry("queries").or_default() += queries.len();
        let shortcuts: Vec<_> = desktop_fleet::shortcut_windows(
            hosts
                .iter()
                .map(|host| (host.id, &host.state, host.sessions.as_slice())),
        )
        .collect();
        uint(&mut expected, shortcuts.len() as u64);
        for (position, (id, window)) in shortcuts.iter().copied().enumerate() {
            uint(&mut expected, u64::from(id));
            uint(&mut expected, window.0);
            let value = NavNode::Target(id, NavTarget::Window(window));
            activation(
                &mut expected,
                host(&hosts, id).and_then(|host| {
                    desktop_fleet::activation(
                        value,
                        &host.state,
                        &host.sessions,
                        attached_host,
                        attached,
                    )
                }),
            )?;
            *counts.entry("shortcuts").or_default() += usize::from(position < 9);
        }
        for expand_windows in [true, false] {
            *counts.entry("rows").or_default() += rows(&mut expected, &hosts, expand_windows);
        }
        ends.push(expected.len());
    }
    let attach = encode_protocol_message(&ProtocolMessage::Attach {
        session: SessionId(7).to_string(),
    })?;
    uint(&mut expected, attach.len() as u64);
    expected.extend(attach);
    fs::write(request_path, &request)?;
    fs::write(expected_path, &expected)?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"cases":cases,"ends":ends,"counts":counts,"bytes":expected.len()})
        )?
    );
    Ok(())
}
