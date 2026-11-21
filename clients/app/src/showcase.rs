use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
};

use serde_json::{Value, json};
use zz_protocol::{
    AgentConnectionPhase, AgentDescriptor, AgentGitSummary, AgentPaneWire, AgentProvider,
    AttachOperation, Axis, BrowserDescriptor, ClientId, CommandRequest, CommandResponse,
    DEFAULT_BROWSER_PROFILE, Event, EventPayload, FloatingPaneSnapshot, HostPlatform, InputMessage,
    LayoutNode, LinuxDistro, MuxOptions, MuxSnapshot, PaneBorderIndicators, PaneBorderLines,
    PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus, PaneStatusState,
    PaneSummary, ProtocolMessage, ProviderStatus, ProviderTask, ProvidersReport, RawText,
    ServerError, ServerHello, SessionId, SessionSnapshot, SessionViewer, ShellCommand, SplitId,
    StatusLine, StdoutClaim, ViewContent, ViewDescriptor, WindowId, WindowSnapshot,
    chat::{ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId, TurnOutcome, Usage},
};
use zz_terminal::{
    ATTR_BOLD, ATTR_FAINT, AppearanceProvenance, CellWidth, Color, ColourClass, Cursor,
    CursorStyle, KeyAction, KeyCode, PackedCell, PackedStyle, ScrollbarState, SessionStatus,
    TerminalAppearance, TerminalColorScheme, TerminalDictionary, TerminalMode,
    TerminalPresentation, TerminalViewport, UnderlineStyle,
};

pub const SHOWCASE_HOSTS: [(&str, &str, bool); 2] = [
    ("local", "This computer", true),
    ("desktop", "desktop", false),
];

const START_MS: u64 = 1_795_100_000_000;
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

#[derive(Clone, Copy)]
enum Tone {
    Plain,
    Bold,
    Muted,
    Accent,
    Success,
    Info,
    Warn,
}

#[derive(Clone)]
struct Span {
    tone: Tone,
    text: String,
}

impl Span {
    fn new(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
        }
    }
}

#[derive(Clone)]
struct Line {
    spans: Vec<Span>,
}

impl Line {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span::new(Tone::Plain, text)],
        }
    }

    fn styled(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span::new(tone, text)],
        }
    }

    fn spans(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    fn empty() -> Self {
        Self { spans: Vec::new() }
    }
}

struct DemoTerminal {
    title: String,
    cwd: String,
    prompt_prefix: String,
    lines: Vec<Line>,
    input: String,
    interactive: bool,
    columns: u16,
    rows: u16,
    generation: u64,
}

impl DemoTerminal {
    fn new(
        title: impl Into<String>,
        cwd: impl Into<String>,
        prompt_prefix: impl Into<String>,
        lines: Vec<Line>,
        interactive: bool,
    ) -> Self {
        Self {
            title: title.into(),
            cwd: cwd.into(),
            prompt_prefix: prompt_prefix.into(),
            lines,
            input: String::new(),
            interactive,
            columns: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            generation: 1,
        }
    }

    fn push_line(&mut self, line: Line) {
        self.lines.push(line);
        if self.lines.len() > 200 {
            let extra = self.lines.len() - 200;
            self.lines.drain(..extra);
        }
        self.generation += 1;
    }

    fn render(&self, appearance: &TerminalAppearance) -> TerminalViewport {
        let cols = self.columns.max(20);
        let rows = self.rows.max(6);
        let fg = appearance.foreground;
        let bg = appearance.background;
        let pal = appearance.palette.as_array();
        let style = |color: Color, class: ColourClass, attrs: u16| {
            PackedStyle::new(color, bg, None, attrs, UnderlineStyle::None)
                .with_classes(class, ColourClass::Default)
        };
        let styles = vec![
            style(fg, ColourClass::Default, 0),
            style(fg, ColourClass::Default, ATTR_BOLD),
            style(pal[8], ColourClass::Palette(8), ATTR_FAINT),
            style(pal[1], ColourClass::Palette(1), ATTR_BOLD),
            style(pal[2], ColourClass::Palette(2), 0),
            style(pal[4], ColourClass::Palette(4), 0),
            style(pal[3], ColourClass::Palette(3), 0),
        ];
        let dictionary = Arc::new(TerminalDictionary::from_shared(
            Arc::from(styles),
            Arc::from([0u32]),
            Arc::from(Vec::new()),
        ));

        let mut all_lines = self.lines.clone();
        let mut cursor_col = 0u16;
        let mut cursor_row = 0u16;
        if self.interactive {
            let prompt = format!("{} ", self.prompt_prefix);
            let prompt_len = prompt.chars().count();
            let input_len = self.input.chars().count();
            all_lines.push(Line::spans(vec![
                Span::new(Tone::Accent, prompt),
                Span::new(Tone::Plain, self.input.clone()),
            ]));
            cursor_col = u16::try_from(prompt_len + input_len)
                .unwrap_or(cols.saturating_sub(1))
                .min(cols.saturating_sub(1));
        }

        let visible_rows = usize::from(rows);
        let start = all_lines.len().saturating_sub(visible_rows);
        let shown = &all_lines[start..];
        if self.interactive && !shown.is_empty() {
            cursor_row = u16::try_from(shown.len() - 1).unwrap_or(0);
        }

        let total_cells = usize::from(cols) * usize::from(rows);
        let mut cells = vec![PackedCell::new(u32::from(b' '), 0, CellWidth::Narrow); total_cells];

        for (row_index, line) in shown.iter().enumerate() {
            let mut col = 0usize;
            for span in &line.spans {
                let style_id = match span.tone {
                    Tone::Plain => 0,
                    Tone::Bold => 1,
                    Tone::Muted => 2,
                    Tone::Accent => 3,
                    Tone::Success => 4,
                    Tone::Info => 5,
                    Tone::Warn => 6,
                };
                for ch in span.text.chars() {
                    if col >= usize::from(cols) {
                        break;
                    }
                    let offset = row_index * usize::from(cols) + col;
                    cells[offset] = PackedCell::new(ch as u32, style_id, CellWidth::Narrow);
                    col += 1;
                }
            }
        }

        let cursor = self.interactive.then(|| {
            Cursor::new(
                cursor_col,
                cursor_row,
                true,
                true,
                false,
                CursorStyle::Block,
                appearance.cursor_color,
            )
        });

        TerminalViewport {
            generation: self.generation,
            view_generation: self.generation,
            dictionary_generation: 1,
            columns: cols,
            rows,
            foreground: fg,
            background: bg,
            presentation: Arc::new(TerminalPresentation::new(
                Arc::from(self.title.as_str()),
                Some(Arc::from(self.cwd.as_str())),
                None,
            )),
            cells: Arc::from(cells),
            dictionary,
            overlays: Arc::from(Vec::new()),
            kitty_placements: Arc::from(Vec::new()),
            cursor,
            scrollbar: ScrollbarState {
                total: u32::try_from(all_lines.len().max(visible_rows)).unwrap_or(u32::MAX),
                offset: u32::try_from(start).unwrap_or(0),
                len: u32::from(rows),
            },
            mode: TerminalMode::Live,
            search: None,
            unseen_output: 0,
            kitty_keyboard: false,
            mouse_tracking: false,
            status: SessionStatus::Running,
        }
    }
}

struct DemoAgent {
    provider: AgentProvider,
    session_id: String,
    title: String,
    seq: u64,
    turn: u64,
    clock_ms: u64,
    items: Vec<(u64, Vec<u8>)>,
}

impl DemoAgent {
    fn new(provider: AgentProvider, session_id: &str, title: &str) -> Self {
        let mut agent = Self {
            provider,
            session_id: session_id.to_owned(),
            title: title.to_owned(),
            seq: 0,
            turn: 0,
            clock_ms: START_MS,
            items: Vec::new(),
        };
        agent.push_raw(json!({
            "item": "sessionReady",
            "session_id": agent.session_id,
            "config_options": [
                {
                    "id": "model",
                    "name": "Model",
                    "category": "model",
                    "type": "select",
                    "currentValue": "claude-sonnet-4-6",
                    "options": [
                        { "value": "claude-sonnet-4-6", "name": "Sonnet 4.6" },
                        { "value": "claude-opus-4-6", "name": "Opus 4.6" }
                    ]
                }
            ]
        }));
        agent.push_chat(&ChatEvent::SessionReady {
            session: agent.session_id.clone(),
        });
        agent.seed_intro_turn();
        agent
    }

    fn push_raw(&mut self, mut value: Value) -> (u64, Vec<u8>) {
        self.seq += 1;
        if let Value::Object(map) = &mut value {
            map.insert("seq".to_owned(), Value::from(self.seq));
        }
        let bytes = serde_json::to_vec(&value).unwrap_or_default();
        self.items.push((self.seq, bytes.clone()));
        (self.seq, bytes)
    }

    fn push_chat(&mut self, event: &ChatEvent) -> (u64, Vec<u8>) {
        self.clock_ms += 900;
        let payload = serde_json::to_value(event).unwrap_or(Value::Null);
        self.push_raw(json!({
            "item": "chat",
            "event": payload
        }))
    }

    fn seed_intro_turn(&mut self) {
        self.turn += 1;
        let turn = TurnId(self.turn);
        let at = self.clock_ms;
        self.push_chat(&ChatEvent::TurnStarted {
            turn,
            by: TurnBy::User { prompt: self.turn },
            at,
        });
        let user_id = ItemId::new("u:1");
        self.push_chat(&ChatEvent::ItemStarted {
            item: user_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::User,
            body: ItemBody::text("How does zz work?"),
            at: self.clock_ms,
        });
        self.push_chat(&ChatEvent::ItemEnded {
            item: user_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        });

        let read_id = ItemId::new("t:read-1");
        self.push_chat(&ChatEvent::ItemStarted {
            item: read_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::Read,
            body: ItemBody {
                target: Some("crates/zz-daemon/src/server.rs".to_owned()),
                ..ItemBody::default()
            },
            at: self.clock_ms,
        });
        self.push_chat(&ChatEvent::ItemEnded {
            item: read_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        });

        let reply_id = ItemId::new("a:1");
        let reply = concat!(
            "One daemon owns every pane. Close the app — nothing stops.\n\n",
            "```mermaid\n",
            "flowchart LR\n",
            "  C[Mac / Linux / Web] --> D[zz-daemon]\n",
            "  D --> T[PTY]\n",
            "  D --> B[Browser]\n",
            "  D --> A[Agent]\n",
            "```\n\n",
            "Click the diagram for PiP, or flip to CLI in the header."
        );
        self.push_chat(&ChatEvent::ItemStarted {
            item: reply_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::Agent,
            body: ItemBody::text(reply),
            at: self.clock_ms,
        });
        self.push_chat(&ChatEvent::ItemEnded {
            item: reply_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        });
        self.push_chat(&ChatEvent::UsageChanged {
            usage: Usage {
                used: 24_800,
                size: 200_000,
            },
        });
        self.push_chat(&ChatEvent::TurnEnded {
            turn,
            outcome: TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: self.clock_ms,
        });
    }

    fn respond(&mut self, prompt: &str) -> Vec<(u64, Vec<u8>)> {
        let mut batch = Vec::new();
        self.turn += 1;
        let turn = TurnId(self.turn);
        let at = self.clock_ms;
        batch.push(self.push_chat(&ChatEvent::TurnStarted {
            turn,
            by: TurnBy::User { prompt: self.turn },
            at,
        }));
        let user_id = ItemId::new(format!("u:{}", self.turn));
        batch.push(self.push_chat(&ChatEvent::ItemStarted {
            item: user_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::User,
            body: ItemBody::text(prompt),
            at: self.clock_ms,
        }));
        batch.push(self.push_chat(&ChatEvent::ItemEnded {
            item: user_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        }));

        let lower = prompt.to_ascii_lowercase();
        let (target_file, answer) = if lower.contains("bench")
            || lower.contains("fast")
            || lower.contains("speed")
        {
            (
                "bench/summarize.sh",
                "**645 MB/s** and **1285 fps** on Linux Vulkan (~8-11x faster than `ghostty+tmux`). Open **benchmarks** in the sidebar to compare.",
            )
        } else if lower.contains("install") || lower.contains("download") {
            (
                "install.sh",
                "```sh\ncurl -fsSL https://autistic.computer/zz/install.sh | sh\n```\n\nmacOS (Apple Silicon) and Linux. Add `-s -- --headless` for SSH servers.",
            )
        } else if lower.contains("ssh") || lower.contains("fleet") || lower.contains("remote") {
            (
                "crates/zz-daemon-client/src/route.rs",
                "Click **desktop -> browser** in the sidebar. Remote SSH hosts join the same sidebar, splits, and command palette.",
            )
        } else if lower.contains("browser") || lower.contains("cef") || lower.contains("chrome") {
            (
                "libs/ala-cef/src/lib.rs",
                "Real Chromium rendered onto the same GPU surface. Open **desktop -> browser** in the sidebar to try it with `ssh -D`.",
            )
        } else {
            (
                "crates/zz-mux/src/lib.rs",
                "Try **benchmarks** or **desktop -> browser** in the sidebar, click the diagram above for PiP, or press `Cmd+K`.",
            )
        };

        let tool_id = ItemId::new(format!("t:read-{}", self.turn));
        batch.push(self.push_chat(&ChatEvent::ItemStarted {
            item: tool_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::Read,
            body: ItemBody {
                target: Some(target_file.to_owned()),
                ..ItemBody::default()
            },
            at: self.clock_ms,
        }));
        batch.push(self.push_chat(&ChatEvent::ItemEnded {
            item: tool_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        }));

        let reply_id = ItemId::new(format!("a:{}", self.turn));
        batch.push(self.push_chat(&ChatEvent::ItemStarted {
            item: reply_id.clone(),
            turn: Some(turn),
            parent: None,
            kind: ItemKind::Agent,
            body: ItemBody::text(answer),
            at: self.clock_ms,
        }));
        batch.push(self.push_chat(&ChatEvent::ItemEnded {
            item: reply_id,
            status: ItemStatus::Completed,
            at: self.clock_ms,
        }));
        batch.push(self.push_chat(&ChatEvent::TurnEnded {
            turn,
            outcome: TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: self.clock_ms,
        }));
        batch
    }

    fn wire_state(&self) -> AgentPaneWire {
        AgentPaneWire {
            phase: AgentConnectionPhase::Ready,
            queued_prompts: 0,
            session_id: Some(self.session_id.clone()),
            title: Some(self.title.clone()),
            error: None,
            auth_methods: "[]".to_owned(),
            config_options: r#"[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"claude-sonnet-4-6","options":[{"value":"claude-sonnet-4-6","name":"Sonnet 4.6"},{"value":"claude-opus-4-6","name":"Opus 4.6"}]}]"#.to_owned(),
            modes: "[]".to_owned(),
            pending_permission: None,
            git: Some(AgentGitSummary {
                branch: Some("main".to_owned()),
                changed_files: 2,
                additions: 48,
                deletions: 12,
            }),
            tasks: Vec::new(),
        }
    }

    fn updates_since(&self, from_seq: u64) -> Option<(u64, Vec<Vec<u8>>)> {
        let slice: Vec<(u64, Vec<u8>)> = self
            .items
            .iter()
            .filter(|(seq, _)| *seq > from_seq)
            .cloned()
            .collect();
        let first_seq = slice.first()?.0;
        Some((first_seq, slice.into_iter().map(|(_, b)| b).collect()))
    }
}

pub struct DemoHost {
    host: String,
    platform: HostPlatform,
    appearance: TerminalAppearance,
    snapshot: MuxSnapshot,
    active_session: SessionId,
    terminals: HashMap<PaneId, DemoTerminal>,
    agents: HashMap<PaneId, DemoAgent>,
    files: HashMap<String, String>,
    sequence: u64,
    next_pane: u64,
    next_window: u64,
    next_split: u64,
}

impl DemoHost {
    #[must_use]
    pub fn for_host(host: &str) -> Self {
        match host {
            "desktop" => Self::desktop(),
            _ => Self::local(),
        }
    }

    fn local() -> Self {
        let p0 = PaneId(0);
        let p1 = PaneId(1);
        let p3 = PaneId(3);

        let w0 = WindowId(0);
        let w1 = WindowId(1);
        let s0 = SessionId(0);

        let mut terminals = HashMap::new();
        terminals.insert(p0, overview_terminal());
        terminals.insert(p3, benchmarks_terminal());

        let mut agents = HashMap::new();
        agents.insert(
            p1,
            DemoAgent::new(AgentProvider::ClaudeCode, "zz-showcase-1", "start"),
        );

        let mut files = HashMap::new();
        files.insert(
            "/Users/demo/.config/zz/config".to_owned(),
            sample_config_file().to_owned(),
        );

        let start_panes = BTreeMap::from([
            (
                p0,
                terminal_pane(p0, "zsh", "~/dev/zebrilus", "main", "zsh", Vec::new()),
            ),
            (
                p1,
                agent_pane(p1, AgentProvider::ClaudeCode, "start", "zz-showcase-1"),
            ),
        ]);
        let start_layout = LayoutNode::Split {
            id: SplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.48,
            first: Box::new(LayoutNode::Pane(p0)),
            second: Box::new(LayoutNode::Pane(p1)),
        };

        let bench_panes = BTreeMap::from([(
            p3,
            terminal_pane(
                p3,
                "benchmarks",
                "~/dev/zebrilus",
                "main",
                "zsh",
                Vec::new(),
            ),
        )]);

        let windows = vec![
            make_window(w0, 0, "start", p0, start_layout, start_panes),
            make_window(w1, 1, "benchmarks", p3, LayoutNode::Pane(p3), bench_panes),
        ];

        let snapshot = MuxSnapshot {
            generation: 1,
            sessions: vec![SessionSnapshot {
                id: s0,
                name: "zebrilus".to_owned(),
                active_window: w0,
                windows,
                viewers: vec![
                    SessionViewer {
                        name: "Browser".to_owned(),
                        window: w0,
                        is_self: true,
                    },
                    SessionViewer {
                        name: "MacBook M4".to_owned(),
                        window: w0,
                        is_self: false,
                    },
                ],
            }],
            focused_window: Some(w0),
        };

        Self {
            host: "local".to_owned(),
            platform: HostPlatform::Macos,
            appearance: TerminalAppearance::default_for(TerminalColorScheme::Dark),
            snapshot,
            active_session: s0,
            terminals,
            agents,
            files,
            sequence: 1,
            next_pane: 100,
            next_window: 10,
            next_split: 50,
        }
    }

    fn desktop() -> Self {
        let p0 = PaneId(10);
        let p1 = PaneId(11);
        let w0 = WindowId(10);
        let s0 = SessionId(10);

        let mut terminals = HashMap::new();
        terminals.insert(p0, desktop_terminal());

        let panes = BTreeMap::from([
            (
                p0,
                terminal_pane(p0, "vite dev", "~/src/app", "ssh -D", "zsh", Vec::new()),
            ),
            (
                p1,
                browser_pane_tabs(
                    p1,
                    "localhost:3000",
                    &["http://localhost:3000", "https://autistic.computer"],
                ),
            ),
        ]);
        let layout = LayoutNode::Split {
            id: SplitId(10),
            axis: Axis::Horizontal,
            ratio: 0.42,
            first: Box::new(LayoutNode::Pane(p0)),
            second: Box::new(LayoutNode::Pane(p1)),
        };

        let snapshot = MuxSnapshot {
            generation: 1,
            sessions: vec![SessionSnapshot {
                id: s0,
                name: "remote-dev".to_owned(),
                active_window: w0,
                windows: vec![make_window(w0, 0, "browser", p0, layout, panes)],
                viewers: vec![SessionViewer {
                    name: "Browser".to_owned(),
                    window: w0,
                    is_self: true,
                }],
            }],
            focused_window: Some(w0),
        };

        Self {
            host: "desktop".to_owned(),
            platform: HostPlatform::Linux(LinuxDistro::Arch),
            appearance: TerminalAppearance::default_for(TerminalColorScheme::Dark),
            snapshot,
            active_session: s0,
            terminals,
            agents: HashMap::new(),
            files: HashMap::new(),
            sequence: 1,
            next_pane: 100,
            next_window: 20,
            next_split: 50,
        }
    }

    fn event(&mut self, payload: EventPayload) -> ProtocolMessage {
        self.sequence += 1;
        ProtocolMessage::Event(Event {
            sequence: self.sequence,
            payload,
        })
    }

    fn server_hello(&self, instance: zz_protocol::ClientInstanceId) -> ProtocolMessage {
        let capabilities = [
            zz_protocol::PANE_FRAME_CAPABILITY,
            zz_protocol::CONTROL_CAPABILITY,
            zz_protocol::FILE_EDIT_CAPABILITY,
            zz_protocol::AGENT_LOGIN_CAPABILITY,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        ProtocolMessage::ServerHello(Box::new(ServerHello {
            protocol_version: zz_protocol::PROTOCOL_VERSION,
            server_id: if self.host == "desktop" { 2 } else { 1 },
            client_id: ClientId(1),
            client_instance_id: instance,
            capabilities,
            appearance: self.appearance.clone(),
            appearance_provenance: AppearanceProvenance::default(),
            mux_options: MuxOptions::default(),
            status: StatusLine::default(),
            key_tables: Vec::new(),
            clock: START_MS / 1000,
            platform: self.platform,
            revision: zz_protocol::REVISION,
        }))
    }

    fn all_panes_messages(&mut self) -> Vec<ProtocolMessage> {
        let mut out = Vec::new();
        let pane_ids: Vec<PaneId> = self.terminals.keys().copied().collect();
        for pane in pane_ids {
            if let Some(term) = self.terminals.get(&pane) {
                let viewport = term.render(&self.appearance);
                out.push(self.event(EventPayload::TerminalViewport { pane, viewport }));
            }
        }
        let agent_ids: Vec<PaneId> = self.agents.keys().copied().collect();
        for pane in agent_ids {
            if let Some(agent) = self.agents.get(&pane) {
                let state = agent.wire_state();
                out.push(self.event(EventPayload::AgentState { pane, state }));
            }
        }
        out
    }

    pub fn handle(&mut self, message: ProtocolMessage) -> Vec<ProtocolMessage> {
        match message {
            ProtocolMessage::Hello(hello) => {
                let instance = hello.client.client_instance_id;
                let attach = hello.attach;
                let mut out = vec![self.server_hello(instance)];
                if let Some(AttachOperation::Session(target)) = attach {
                    let session = self.resolve_session(&target);
                    self.active_session = session;
                    out.push(ProtocolMessage::Attached {
                        session,
                        snapshot: self.snapshot.clone(),
                        read_only: false,
                        client_flags: String::new(),
                    });
                    out.extend(self.all_panes_messages());
                } else {
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
                out
            }
            ProtocolMessage::Attach { session } => {
                let session = self.resolve_session(&session);
                self.active_session = session;
                let mut out = vec![ProtocolMessage::Attached {
                    session,
                    snapshot: self.snapshot.clone(),
                    read_only: false,
                    client_flags: String::new(),
                }];
                out.extend(self.all_panes_messages());
                out
            }
            ProtocolMessage::TreeSync | ProtocolMessage::Resync => {
                let mut out = vec![self.event(EventPayload::Snapshot(self.snapshot.clone()))];
                out.extend(self.all_panes_messages());
                out
            }
            ProtocolMessage::RequestFull { pane } => {
                let Some(viewport) = self
                    .terminals
                    .get(&pane)
                    .map(|term| term.render(&self.appearance))
                else {
                    return Vec::new();
                };
                vec![self.event(EventPayload::TerminalViewport { pane, viewport })]
            }
            ProtocolMessage::SetColorScheme(scheme) => {
                self.appearance = TerminalAppearance::default_for(scheme);
                let mut out = vec![self.event(EventPayload::AppearanceChanged {
                    appearance: Box::new(self.appearance.clone()),
                    provenance: AppearanceProvenance::default(),
                })];
                let pane_ids: Vec<PaneId> = self.terminals.keys().copied().collect();
                for pane in pane_ids {
                    if let Some(term) = self.terminals.get(&pane) {
                        let viewport = term.render(&self.appearance);
                        out.push(self.event(EventPayload::TerminalViewport { pane, viewport }));
                    }
                }
                out
            }
            ProtocolMessage::AgentReplay { pane, from_seq } => {
                let mut out = Vec::new();
                if let Some(agent) = self.agents.get(&pane) {
                    let state = agent.wire_state();
                    let updates = agent.updates_since(from_seq);
                    out.push(self.event(EventPayload::AgentState { pane, state }));
                    if let Some((first_seq, items)) = updates {
                        out.push(self.event(EventPayload::AgentUpdates {
                            pane,
                            first_seq,
                            items,
                        }));
                    }
                }
                out
            }
            ProtocolMessage::AgentPrompt { pane, text, .. } => {
                let Some(agent) = self.agents.get_mut(&pane) else {
                    return Vec::new();
                };
                let added = agent.respond(&text);
                let Some((first_seq, _)) = added.first() else {
                    return Vec::new();
                };
                let first_seq = *first_seq;
                let items = added.into_iter().map(|(_, bytes)| bytes).collect();
                let state = agent.wire_state();
                vec![
                    self.event(EventPayload::AgentState { pane, state }),
                    self.event(EventPayload::AgentUpdates {
                        pane,
                        first_seq,
                        items,
                    }),
                ]
            }
            ProtocolMessage::FileReadRequest {
                request_id, path, ..
            } => {
                let result = self
                    .files
                    .get(&path)
                    .cloned()
                    .ok_or_else(|| format!("file not found in showcase: {path}"));
                vec![ProtocolMessage::FileReadResponse { request_id, result }]
            }
            ProtocolMessage::FileWriteRequest {
                request_id,
                path,
                offset,
                bytes,
                ..
            } => {
                let entry = self.files.entry(path).or_default();
                if offset == 0 {
                    entry.clear();
                }
                entry.push_str(&String::from_utf8_lossy(&bytes));
                vec![ProtocolMessage::FileWriteResponse {
                    request_id,
                    result: Ok(()),
                }]
            }
            ProtocolMessage::Input(input) => self.handle_input(input),
            ProtocolMessage::CommandRequest(request) => self.handle_command(request),
            ProtocolMessage::Ping { token, .. } => vec![ProtocolMessage::Pong { token }],
            _ => Vec::new(),
        }
    }

    fn resolve_session(&self, target: &str) -> SessionId {
        if let Some(stripped) = target.strip_prefix('$')
            && let Ok(id) = stripped.parse::<u64>()
            && self.snapshot.sessions.iter().any(|s| s.id == SessionId(id))
        {
            return SessionId(id);
        }
        if !target.is_empty()
            && let Some(found) = self.snapshot.sessions.iter().find(|s| s.name == target)
        {
            return found.id;
        }
        self.snapshot
            .sessions
            .first()
            .map_or(self.active_session, |s| s.id)
    }

    fn handle_input(&mut self, input: InputMessage) -> Vec<ProtocolMessage> {
        match input {
            InputMessage::ResizeTerminal {
                pane,
                columns,
                rows,
                ..
            }
            | InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                ..
            } => {
                let Some(term) = self.terminals.get_mut(&pane) else {
                    return Vec::new();
                };
                if term.columns == columns && term.rows == rows {
                    return Vec::new();
                }
                term.columns = columns;
                term.rows = rows;
                term.generation += 1;
                let viewport = term.render(&self.appearance);
                vec![self.event(EventPayload::TerminalViewport { pane, viewport })]
            }
            InputMessage::Text { pane, text } => self.handle_terminal_text(pane, &text),
            InputMessage::Key { pane, input, .. } => {
                if input.action == KeyAction::Release {
                    return Vec::new();
                }
                if input.modifiers.control() {
                    match input.key {
                        KeyCode::Character('c' | 'C') => {
                            if let Some(term) = self.terminals.get_mut(&pane)
                                && term.interactive
                            {
                                let line = format!("{} {}^C", term.prompt_prefix, term.input);
                                term.input.clear();
                                term.push_line(Line::styled(Tone::Muted, line));
                                let viewport = term.render(&self.appearance);
                                return vec![
                                    self.event(EventPayload::TerminalViewport { pane, viewport }),
                                ];
                            }
                            return Vec::new();
                        }
                        KeyCode::Character('l' | 'L') => {
                            if let Some(term) = self.terminals.get_mut(&pane)
                                && term.interactive
                            {
                                term.lines.clear();
                                term.generation += 1;
                                let viewport = term.render(&self.appearance);
                                return vec![
                                    self.event(EventPayload::TerminalViewport { pane, viewport }),
                                ];
                            }
                            return Vec::new();
                        }
                        _ => return Vec::new(),
                    }
                }
                match input.key {
                    KeyCode::Enter => self.execute_terminal_line(pane),
                    KeyCode::Backspace => {
                        if let Some(term) = self.terminals.get_mut(&pane)
                            && term.interactive
                        {
                            term.input.pop();
                            term.generation += 1;
                            let viewport = term.render(&self.appearance);
                            return vec![
                                self.event(EventPayload::TerminalViewport { pane, viewport }),
                            ];
                        }
                        Vec::new()
                    }
                    KeyCode::Character(ch) => {
                        let mut buf = [0u8; 4];
                        let s = ch.encode_utf8(&mut buf);
                        self.handle_terminal_text(pane, s)
                    }
                    _ => Vec::new(),
                }
            }
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points,
            } => {
                let ratio = (f32::from(ratio_basis_points) / 10_000.0).clamp(0.15, 0.85);
                if self.set_split_ratio(window, split, ratio) {
                    self.snapshot.generation += 1;
                    return vec![self.event(EventPayload::Snapshot(self.snapshot.clone()))];
                }
                Vec::new()
            }
            InputMessage::ClientTerminalSize { columns, rows }
            | InputMessage::ClientTerminalSizeV2 { columns, rows, .. } => {
                let sx = columns.max(40);
                let sy = rows.max(16);
                let mut changed = false;
                for session in &mut self.snapshot.sessions {
                    for window in &mut session.windows {
                        if window.sx != sx || window.sy != sy {
                            window.sx = sx;
                            window.sy = sy;
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.snapshot.generation += 1;
                    return vec![self.event(EventPayload::Snapshot(self.snapshot.clone()))];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_terminal_text(&mut self, pane: PaneId, text: &str) -> Vec<ProtocolMessage> {
        let Some(term) = self.terminals.get_mut(&pane) else {
            return Vec::new();
        };
        if !term.interactive {
            return Vec::new();
        }
        let mut submitted = false;
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => submitted = true,
                '\x7f' | '\x08' => {
                    term.input.pop();
                }
                c if !c.is_control() => term.input.push(c),
                _ => {}
            }
        }
        term.generation += 1;
        if submitted {
            return self.execute_terminal_line(pane);
        }
        let viewport = term.render(&self.appearance);
        vec![self.event(EventPayload::TerminalViewport { pane, viewport })]
    }

    fn execute_terminal_line(&mut self, pane: PaneId) -> Vec<ProtocolMessage> {
        let Some(term) = self.terminals.get_mut(&pane) else {
            return Vec::new();
        };
        if !term.interactive {
            return Vec::new();
        }
        let command = term.input.trim().to_owned();
        let prompt_line = Line::spans(vec![
            Span::new(Tone::Accent, format!("{} ", term.prompt_prefix)),
            Span::new(Tone::Plain, command.clone()),
        ]);
        term.input.clear();
        term.push_line(prompt_line);

        match command.as_str() {
            "" => {}
            "clear" => {
                term.lines.clear();
                term.generation += 1;
            }
            "help" => {
                for line in [
                    Line::plain("  install       macOS / Linux / SSH install commands"),
                    Line::plain("  bench         PTY speed numbers"),
                    Line::plain("  zz list-panes panes in this session"),
                    Line::plain("  clear         clear screen"),
                ] {
                    term.push_line(line);
                }
            }
            "install" => {
                for line in [
                    Line::styled(
                        Tone::Success,
                        "curl -fsSL https://autistic.computer/zz/install.sh | sh",
                    ),
                    Line::plain("brew install --cask demfabris/zz/zz    # macOS"),
                    Line::plain("paru zz-bin                            # Arch"),
                    Line::styled(Tone::Muted, "add `-s -- --headless` for SSH servers"),
                ] {
                    term.push_line(line);
                }
            }
            "bench" => {
                for line in [
                    Line::styled(
                        Tone::Success,
                        "Linux Vulkan  zz 645 MB/s · 1285 fps  (ghostty+tmux 56 MB/s)",
                    ),
                    Line::styled(
                        Tone::Info,
                        "macOS Metal   zz 328 MB/s ·  668 fps  (ghostty+tmux 44 MB/s)",
                    ),
                ] {
                    term.push_line(line);
                }
            }
            "features" => {
                term.push_line(Line::plain(
                    "tmux + ghostty + chromium + agents, one daemon.",
                ));
            }
            "whoami" => {
                let host = self.host.clone();
                let version = env!("CARGO_PKG_VERSION");
                term.push_line(Line::plain(format!("demo@{host} (zz v{version})")));
            }
            "ls" => {
                term.push_line(Line::styled(
                    Tone::Info,
                    "Cargo.toml  crates/  clients/  libs/  bench/  install.sh",
                ));
            }
            "zz list-panes" | "list-panes" => {
                let summary = format_panes_list(&self.snapshot);
                for row in summary {
                    term.push_line(Line::plain(row));
                }
            }
            other => {
                term.push_line(Line::styled(
                    Tone::Warn,
                    format!("'{other}' — try: help, install, bench, clear"),
                ));
            }
        }

        let viewport = term.render(&self.appearance);
        vec![self.event(EventPayload::TerminalViewport { pane, viewport })]
    }

    fn handle_command(&mut self, request: CommandRequest) -> Vec<ProtocolMessage> {
        let CommandRequest {
            request_id,
            command,
            ..
        } = request;
        let name = command.name.as_str();
        let args: Vec<&str> = command.args.iter().map(RawText::as_str).collect();
        let mut out = Vec::new();
        if let Some(early) = self.dispatch_command(request_id, name, &args, &mut out) {
            return early;
        }
        out.push(success_response(request_id, String::new()));
        out
    }

    fn dispatch_command(
        &mut self,
        request_id: u64,
        name: &str,
        args: &[&str],
        out: &mut Vec<ProtocolMessage>,
    ) -> Option<Vec<ProtocolMessage>> {
        match name {
            "select-window" => {
                if let Some(target) = flag_value(args, "-t")
                    && self.select_window_target(target)
                {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "next-window" => {
                self.step_window(1);
                self.snapshot.generation += 1;
                out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
            }
            "previous-window" => {
                self.step_window(-1);
                self.snapshot.generation += 1;
                out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
            }
            "select-pane" => {
                if let Some(target) = flag_value(args, "-t")
                    && self.select_pane_target(target)
                {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "resize-pane" => {
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                let changed = if args.contains(&"-Z") {
                    self.toggle_zoom(target)
                } else {
                    self.resize_floating_pane(
                        target,
                        flag_value(args, "-x"),
                        flag_value(args, "-y"),
                    )
                };
                if changed {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "move-pane" => {
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                let changed = if let Some(place) = flag_value(args, "-P") {
                    self.place_floating_pane(target, place)
                } else {
                    self.move_floating_pane(target, flag_value(args, "-X"), flag_value(args, "-Y"))
                };
                if changed {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "break-pane" => {
                if args.contains(&"-W") {
                    let target = flag_value(args, "-s")
                        .or_else(|| flag_value(args, "-t"))
                        .and_then(parse_pane_id);
                    if self.float_tiled_pane(target, flag_value(args, "-x"), flag_value(args, "-y"))
                    {
                        self.snapshot.generation += 1;
                        out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                    }
                }
            }
            "join-pane" => {
                let src = flag_value(args, "-s").and_then(parse_pane_id);
                let dst = flag_value(args, "-t").and_then(parse_pane_id);
                let axis = if args.contains(&"-v") {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                };
                let before = args.contains(&"-b");
                if self.join_or_tile_pane(src, dst, axis, before) {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "view-pane" => {
                if self.open_view_pane(args) {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "run-shell" => {
                if let Some(script) = flag_value(args, "-C") {
                    for part in script.split(';') {
                        let tokens: Vec<&str> = part.split_whitespace().collect();
                        if let Some((cmd, rest)) = tokens.split_first() {
                            let _ = self.dispatch_command(request_id, cmd, rest, out);
                        }
                    }
                }
            }
            "if-shell" => {
                if let Some(script) = args.last()
                    && script.contains("new-pane")
                    && script.contains("--fork")
                    && let Some(forked) = self.fork_agent_pip(script)
                {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                    if let Some(agent) = self.agents.get(&forked) {
                        let state = agent.wire_state();
                        let updates = agent.updates_since(0);
                        out.push(self.event(EventPayload::AgentState {
                            pane: forked,
                            state,
                        }));
                        if let Some((first_seq, items)) = updates {
                            out.push(self.event(EventPayload::AgentUpdates {
                                pane: forked,
                                first_seq,
                                items,
                            }));
                        }
                    }
                }
            }
            "split-window" => {
                let axis = if args.contains(&"-v") {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                };
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                let browser = flag_value(args, "--kind") == Some("browser");
                if let Some(new_pane) = self.split_pane(target, axis, browser) {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                    if let Some(term) = self.terminals.get(&new_pane) {
                        let viewport = term.render(&self.appearance);
                        out.push(self.event(EventPayload::TerminalViewport {
                            pane: new_pane,
                            viewport,
                        }));
                    }
                }
            }
            "new-window" => {
                let new_pane = self.add_window();
                self.snapshot.generation += 1;
                out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                if let Some(term) = self.terminals.get(&new_pane) {
                    let viewport = term.render(&self.appearance);
                    out.push(self.event(EventPayload::TerminalViewport {
                        pane: new_pane,
                        viewport,
                    }));
                }
            }
            "kill-pane" => {
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                if let Some(removed) = self.kill_pane(target) {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::PaneRemoved(removed)));
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "swap-pane" => {
                let src = flag_value(args, "-s").and_then(parse_pane_id);
                let dst = flag_value(args, "-t").and_then(parse_pane_id);
                if let (Some(a), Some(b)) = (src, dst)
                    && self.swap_panes(a, b)
                {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "flip-agent" => {
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                if let Some(flipped) = self.flip_agent_pane(target) {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                    if let Some(term) = self.terminals.get(&flipped) {
                        let viewport = term.render(&self.appearance);
                        out.push(self.event(EventPayload::TerminalViewport {
                            pane: flipped,
                            viewport,
                        }));
                    } else if let Some(agent) = self.agents.get(&flipped) {
                        let state = agent.wire_state();
                        let updates = agent.updates_since(0);
                        out.push(self.event(EventPayload::AgentState {
                            pane: flipped,
                            state,
                        }));
                        if let Some((first_seq, items)) = updates {
                            out.push(self.event(EventPayload::AgentUpdates {
                                pane: flipped,
                                first_seq,
                                items,
                            }));
                        }
                    }
                }
            }
            "set-browser-tabs" => {
                let target = flag_value(args, "-t").and_then(parse_pane_id);
                if let Some(pane_id) = target
                    && let Some(url) = args.iter().rev().find(|arg| arg.starts_with("http"))
                    && self.update_browser_url(pane_id, url)
                {
                    self.snapshot.generation += 1;
                    out.push(self.event(EventPayload::Snapshot(self.snapshot.clone())));
                }
            }
            "providers" => {
                let report = ProvidersReport {
                    checking: false,
                    providers: vec![
                        ProviderStatus {
                            provider: AgentProvider::ClaudeCode,
                            path: Some("/usr/local/bin/claude".to_owned()),
                            version: Some("2.1.0".to_owned()),
                            latest: Some("2.1.0".to_owned()),
                            manual: None,
                            task: ProviderTask::Idle,
                        },
                        ProviderStatus {
                            provider: AgentProvider::Codex,
                            path: Some("/usr/local/bin/codex".to_owned()),
                            version: Some("0.98.0".to_owned()),
                            latest: Some("0.98.0".to_owned()),
                            manual: None,
                            task: ProviderTask::Idle,
                        },
                    ],
                };
                let body = serde_json::to_string(&report).unwrap_or_default();
                out.push(success_response(request_id, body));
                return Some(std::mem::take(out));
            }
            "list-panes" => {
                let body = format_panes_list(&self.snapshot).join("\n");
                out.push(success_response(request_id, body));
                return Some(std::mem::take(out));
            }
            "detach-client" => {
                out.push(
                    self.event(EventPayload::ClientMessage {
                        pane: None,
                        kind: zz_protocol::ClientMessageKind::Info,
                        text: "Showcase session stays alive — use <- autistic.computer to leave."
                            .to_owned(),
                    }),
                );
            }
            _ => {
                if !name.is_empty() && name == "unknown" {
                    out.push(ProtocolMessage::CommandResponse(CommandResponse::Error {
                        request_id,
                        error: ServerError::InvalidCommand(name.to_owned()),
                        output: RawText::default(),
                    }));
                    return Some(std::mem::take(out));
                }
            }
        }
        None
    }

    fn select_window_target(&mut self, target: &str) -> bool {
        let win_id = parse_window_id(target);
        let pane_id = parse_pane_id(target);
        for session in &mut self.snapshot.sessions {
            for window in &session.windows {
                let matches = win_id == Some(window.id)
                    || pane_id.is_some_and(|p| window.panes.contains_key(&p))
                    || window.name == target;
                if matches {
                    session.active_window = window.id;
                    self.snapshot.focused_window = Some(window.id);
                    return true;
                }
            }
        }
        false
    }

    fn step_window(&mut self, delta: isize) {
        let Some(session) = self.snapshot.sessions.first_mut() else {
            return;
        };
        if session.windows.is_empty() {
            return;
        }
        let current = session
            .windows
            .iter()
            .position(|w| w.id == session.active_window)
            .unwrap_or(0);
        let len = session.windows.len();
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        let id = session.windows[next].id;
        session.active_window = id;
        self.snapshot.focused_window = Some(id);
    }

    fn select_pane_target(&mut self, target: &str) -> bool {
        let Some(pane_id) = parse_pane_id(target) else {
            return false;
        };
        for session in &mut self.snapshot.sessions {
            for window in &mut session.windows {
                if window.panes.contains_key(&pane_id) {
                    window.active_pane = pane_id;
                    if window.floating.iter().any(|float| float.pane == pane_id) {
                        window.pane_z_order.retain(|id| *id != pane_id);
                        window.pane_z_order.push(pane_id);
                    }
                    session.active_window = window.id;
                    self.snapshot.focused_window = Some(window.id);
                    return true;
                }
            }
        }
        false
    }

    fn toggle_zoom(&mut self, target: Option<PaneId>) -> bool {
        let Some(window) = self.active_window_mut() else {
            return false;
        };
        let pane = target.unwrap_or(window.active_pane);
        if !window.panes.contains_key(&pane) {
            return false;
        }
        window.zoomed_pane = if window.zoomed_pane == Some(pane) {
            None
        } else {
            Some(pane)
        };
        true
    }

    fn split_pane(&mut self, target: Option<PaneId>, axis: Axis, browser: bool) -> Option<PaneId> {
        let new_id = PaneId(self.next_pane);
        self.next_pane += 1;
        let split_id = SplitId(self.next_split);
        self.next_split += 1;
        let window = self.active_window_mut()?;
        let anchor = target.unwrap_or(window.active_pane);
        if !insert_split(&mut window.layout, anchor, new_id, split_id, axis) {
            return None;
        }
        let pane = if browser {
            browser_pane_tabs(
                new_id,
                "localhost:3000",
                &["http://localhost:3000", "https://autistic.computer"],
            )
        } else {
            terminal_pane(new_id, "zsh", "~/dev/zebrilus", "main", "zsh", Vec::new())
        };
        window.panes.insert(new_id, pane);
        window.pane_order.push(new_id);
        window.active_pane = new_id;
        window.zoomed_pane = None;
        if !browser {
            self.terminals.insert(
                new_id,
                DemoTerminal::new(
                    "zsh",
                    "~/dev/zebrilus",
                    "demo@zz $",
                    vec![
                        Line::styled(Tone::Success, format!("split pane {new_id}")),
                        Line::styled(Tone::Muted, "type help · or drag headers to rearrange"),
                    ],
                    true,
                ),
            );
        }
        Some(new_id)
    }

    fn add_window(&mut self) -> PaneId {
        let new_pane = PaneId(self.next_pane);
        self.next_pane += 1;
        let new_win = WindowId(self.next_window);
        self.next_window += 1;
        let index = self
            .snapshot
            .sessions
            .first()
            .map_or(0, |s| u32::try_from(s.windows.len()).unwrap_or(0));
        let name = format!("shell-{index}");
        let panes = BTreeMap::from([(
            new_pane,
            terminal_pane(new_pane, "zsh", "~", "", "zsh", Vec::new()),
        )]);
        let window = make_window(
            new_win,
            index,
            &name,
            new_pane,
            LayoutNode::Pane(new_pane),
            panes,
        );
        if let Some(session) = self.snapshot.sessions.first_mut() {
            session.windows.push(window);
            session.active_window = new_win;
            self.snapshot.focused_window = Some(new_win);
        }
        self.terminals.insert(
            new_pane,
            DemoTerminal::new(
                "zsh",
                "~",
                "demo@zz $",
                vec![Line::styled(
                    Tone::Muted,
                    "New showcase window. Type `help` for commands.",
                )],
                true,
            ),
        );
        new_pane
    }

    fn kill_pane(&mut self, target: Option<PaneId>) -> Option<PaneId> {
        let window = self.active_window_mut()?;
        let pane = target.unwrap_or(window.active_pane);
        if !window.panes.contains_key(&pane) {
            return None;
        }
        let is_floating = window.floating.iter().any(|float| float.pane == pane);
        if is_floating {
            window.floating.retain(|float| float.pane != pane);
            window.pane_z_order.retain(|id| *id != pane);
        } else {
            let tiled_count = window
                .panes
                .keys()
                .filter(|id| !window.floating.iter().any(|float| float.pane == **id))
                .count();
            if tiled_count <= 1 {
                return None;
            }
            remove_pane_from_layout(&mut window.layout, pane);
        }
        window.panes.remove(&pane);
        window.pane_order.retain(|id| *id != pane);
        if window.zoomed_pane == Some(pane) {
            window.zoomed_pane = None;
        }
        if window.active_pane == pane
            && let Some(&next) = window
                .pane_order
                .iter()
                .find(|id| !window.floating.iter().any(|float| float.pane == **id))
                .or_else(|| window.panes.keys().next())
        {
            window.active_pane = next;
        }
        self.terminals.remove(&pane);
        self.agents.remove(&pane);
        Some(pane)
    }

    fn open_view_pane(&mut self, args: &[&str]) -> bool {
        let origin = flag_value(args, "-t")
            .and_then(parse_pane_id)
            .or_else(|| self.active_window().map(|w| w.active_pane));
        let kind = flag_value(args, "-k").unwrap_or("markdown");
        let text = args
            .iter()
            .position(|arg| *arg == "--")
            .and_then(|pos| args.get(pos + 1))
            .copied()
            .unwrap_or_default()
            .to_owned();
        let content = match kind {
            "diagram" => ViewContent::Diagram {
                language: flag_value(args, "-l").unwrap_or("mermaid").to_owned(),
                source: text,
            },
            "code" => ViewContent::Code {
                language: flag_value(args, "-l").map(ToOwned::to_owned),
                text,
            },
            "image" => ViewContent::Image {
                item: flag_value(args, "-i").unwrap_or_default().to_owned(),
                index: flag_value(args, "-n")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0),
            },
            "subagent" => ViewContent::Subagent {
                item: flag_value(args, "-i").unwrap_or_default().to_owned(),
            },
            _ => ViewContent::Markdown { text },
        };
        let title = flag_value(args, "-T")
            .map_or_else(|| content.kind_name().to_owned(), ToOwned::to_owned);
        let descriptor = ViewDescriptor {
            origin,
            title: title.clone(),
            content,
        };
        if descriptor.validate().is_err() {
            return false;
        }

        let requested_rows = flag_value(args, "-y");
        let next_id = PaneId(self.next_pane);

        let Some(window) = self.window_for_pane_or_active_mut(origin) else {
            return false;
        };

        if let Some(existing_id) = window.panes.iter().find_map(|(id, pane)| {
            matches!(&pane.kind, PaneKindSnapshot::View(view) if view.origin == origin)
                .then_some(*id)
        }) {
            if let Some(pane) = window.panes.get_mut(&existing_id) {
                pane.title.clone_from(&title);
                pane.summary.name.clone_from(&title);
                pane.kind = PaneKindSnapshot::View(descriptor);
            }
            window.active_pane = existing_id;
            window.pane_z_order.retain(|id| *id != existing_id);
            window.pane_z_order.push(existing_id);
            return true;
        }

        let sx = (window.sx.saturating_mul(46) / 100)
            .max(44)
            .min(window.sx.saturating_sub(4).max(20));
        let sy = requested_rows
            .and_then(|raw| parse_size_spec(raw, window.sy))
            .unwrap_or_else(|| window.sy.saturating_mul(78) / 100)
            .clamp(12, window.sy.saturating_sub(2).max(12));
        let xoff = i32::from(window.sx)
            .saturating_sub(i32::from(sx))
            .saturating_sub(1)
            .max(1);
        let yoff = i32::from(window.sy)
            .saturating_sub(i32::from(sy))
            .saturating_sub(1)
            .max(1);

        window
            .panes
            .insert(next_id, view_pane_snapshot(next_id, &title, descriptor));
        window.pane_order.push(next_id);
        window.floating.push(FloatingPaneSnapshot {
            pane: next_id,
            xoff,
            yoff,
            sx,
            sy,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        });
        window.pane_z_order.push(next_id);
        window.active_pane = next_id;
        self.next_pane += 1;
        true
    }

    fn float_tiled_pane(
        &mut self,
        target: Option<PaneId>,
        width_spec: Option<&str>,
        height_spec: Option<&str>,
    ) -> bool {
        let Some(window) = self.window_for_pane_or_active_mut(target) else {
            return false;
        };
        let pane = target.unwrap_or(window.active_pane);
        if !window.panes.contains_key(&pane)
            || window.floating.iter().any(|float| float.pane == pane)
        {
            return false;
        }
        let tiled_count = window
            .panes
            .keys()
            .filter(|id| !window.floating.iter().any(|float| float.pane == **id))
            .count();
        if tiled_count <= 1 {
            return false;
        }
        if !remove_pane_from_layout(&mut window.layout, pane) {
            return false;
        }
        let sx = width_spec
            .and_then(|raw| parse_size_spec(raw, window.sx))
            .unwrap_or(window.sx / 3)
            .clamp(28, window.sx.saturating_sub(4).max(28));
        let sy = height_spec
            .and_then(|raw| parse_size_spec(raw, window.sy))
            .unwrap_or_else(|| window.sy.saturating_sub(2))
            .clamp(10, window.sy.saturating_sub(2).max(10));
        let xoff = i32::from(window.sx)
            .saturating_sub(i32::from(sx))
            .saturating_sub(1)
            .max(1);
        let yoff = i32::from(window.sy)
            .saturating_sub(i32::from(sy))
            .saturating_sub(1)
            .max(1);
        window.floating.push(FloatingPaneSnapshot {
            pane,
            xoff,
            yoff,
            sx,
            sy,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        });
        window.pane_z_order.retain(|id| *id != pane);
        window.pane_z_order.push(pane);
        window.active_pane = pane;
        window.zoomed_pane = None;
        true
    }

    fn move_floating_pane(
        &mut self,
        target: Option<PaneId>,
        x: Option<&str>,
        y: Option<&str>,
    ) -> bool {
        let Some(window) = self.window_for_pane_or_active_mut(target) else {
            return false;
        };
        let pane = target.unwrap_or(window.active_pane);
        let Some(float) = window.floating.iter_mut().find(|f| f.pane == pane) else {
            return false;
        };
        let pad = i32::from(float.border_lines != PaneBorderLines::None);
        let mut changed = false;
        if let Some(raw_x) = x
            && let Ok(parsed) = raw_x.parse::<i32>()
        {
            float.xoff = parsed + pad;
            changed = true;
        }
        if let Some(raw_y) = y
            && let Ok(parsed) = raw_y.parse::<i32>()
        {
            float.yoff = parsed + pad;
            changed = true;
        }
        changed
    }

    fn place_floating_pane(&mut self, target: Option<PaneId>, place: &str) -> bool {
        let Some(window) = self.window_for_pane_or_active_mut(target) else {
            return false;
        };
        let pane = target.unwrap_or(window.active_pane);
        let (wx, wy) = (i32::from(window.sx), i32::from(window.sy));
        let Some(float) = window.floating.iter_mut().find(|f| f.pane == pane) else {
            return false;
        };
        let border = i32::from(float.border_lines != PaneBorderLines::None);
        let (px, py) = (i32::from(float.sx), i32::from(float.sy));
        let left = border;
        let centre_x = (wx - px) / 2;
        let right = wx - px - border;
        let top = border;
        let centre_y = (wy - py) / 2;
        let bottom = wy - py - border;
        let (xoff, yoff) = match place {
            "top-left" => (left, top),
            "top-centre" => (centre_x, top),
            "top-right" => (right, top),
            "centre-left" => (left, centre_y),
            "centre" => (centre_x, centre_y),
            "centre-right" => (right, centre_y),
            "bottom-left" => (left, bottom),
            "bottom-centre" => (centre_x, bottom),
            "bottom-right" => (right, bottom),
            _ => return false,
        };
        float.xoff = xoff;
        float.yoff = yoff;
        true
    }

    fn resize_floating_pane(
        &mut self,
        target: Option<PaneId>,
        width: Option<&str>,
        height: Option<&str>,
    ) -> bool {
        let Some(window) = self.window_for_pane_or_active_mut(target) else {
            return false;
        };
        let pane = target.unwrap_or(window.active_pane);
        let (wx, wy) = (window.sx, window.sy);
        let Some(float) = window.floating.iter_mut().find(|f| f.pane == pane) else {
            return false;
        };
        let pad = u16::from(float.border_lines != PaneBorderLines::None) * 2;
        let mut changed = false;
        if let Some(raw_w) = width
            && let Some(parsed) = parse_size_spec(raw_w, wx)
        {
            float.sx = parsed
                .saturating_sub(pad)
                .clamp(16, wx.saturating_sub(pad).max(16));
            changed = true;
        }
        if let Some(raw_h) = height
            && let Some(parsed) = parse_size_spec(raw_h, wy)
        {
            float.sy = parsed
                .saturating_sub(pad)
                .clamp(6, wy.saturating_sub(pad).max(6));
            changed = true;
        }
        changed
    }

    fn join_or_tile_pane(
        &mut self,
        src: Option<PaneId>,
        dst: Option<PaneId>,
        axis: Axis,
        before: bool,
    ) -> bool {
        let split_id = SplitId(self.next_split);
        let Some(window) = self.window_for_pane_or_active_mut(src.or(dst)) else {
            return false;
        };
        let source = src.unwrap_or(window.active_pane);
        if !window.panes.contains_key(&source) {
            return false;
        }
        let was_floating = window.floating.iter().any(|float| float.pane == source);
        if was_floating {
            window.floating.retain(|float| float.pane != source);
            window.pane_z_order.retain(|id| *id != source);
            let anchor = dst
                .filter(|id| *id != source && window.layout.contains(*id))
                .or_else(|| {
                    let mut tiled = Vec::new();
                    window.layout.panes(&mut tiled);
                    tiled.into_iter().next()
                });
            if let Some(anchor) = anchor {
                if !insert_split_oriented(
                    &mut window.layout,
                    anchor,
                    source,
                    split_id,
                    axis,
                    before,
                ) {
                    return false;
                }
                window.active_pane = source;
                self.next_split += 1;
                return true;
            }
            window.layout = LayoutNode::Pane(source);
            window.active_pane = source;
            return true;
        }

        let Some(anchor) = dst.filter(|id| *id != source && window.layout.contains(*id)) else {
            return false;
        };
        if !remove_pane_from_layout(&mut window.layout, source) {
            return false;
        }
        if !insert_split_oriented(&mut window.layout, anchor, source, split_id, axis, before) {
            return false;
        }
        window.active_pane = source;
        self.next_split += 1;
        true
    }

    fn fork_agent_pip(&mut self, script: &str) -> Option<PaneId> {
        let origin = script
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|pair| (pair[0] == "-t").then_some(pair[1]))
            .and_then(|tok| parse_pane_id(tok.trim_matches('\'')));
        let new_id = PaneId(self.next_pane);
        self.next_pane += 1;
        let session_id = format!("zz-fork-{}", new_id.0);
        let mut forked = DemoAgent::new(AgentProvider::ClaudeCode, &session_id, "side chat (fork)");
        let _ = forked.respond("This is a forked side-chat PiP pane over the same session.");

        let window = self.window_for_pane_or_active_mut(origin)?;
        let sx = (window.sx.saturating_mul(46) / 100)
            .max(40)
            .min(window.sx.saturating_sub(4).max(20));
        let sy = (window.sy.saturating_mul(72) / 100)
            .max(14)
            .min(window.sy.saturating_sub(2).max(14));
        let xoff = i32::from(window.sx)
            .saturating_sub(i32::from(sx))
            .saturating_sub(1)
            .max(1);
        let yoff = i32::from(window.sy)
            .saturating_sub(i32::from(sy))
            .saturating_sub(1)
            .max(1);

        window.panes.insert(
            new_id,
            agent_pane(
                new_id,
                AgentProvider::ClaudeCode,
                "side chat (fork)",
                &session_id,
            ),
        );
        window.pane_order.push(new_id);
        window.floating.push(FloatingPaneSnapshot {
            pane: new_id,
            xoff,
            yoff,
            sx,
            sy,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        });
        window.pane_z_order.push(new_id);
        window.active_pane = new_id;
        self.agents.insert(new_id, forked);
        Some(new_id)
    }

    fn window_for_pane_or_active_mut(
        &mut self,
        pane: Option<PaneId>,
    ) -> Option<&mut WindowSnapshot> {
        if let Some(pane_id) = pane {
            let found = self
                .snapshot
                .sessions
                .iter()
                .enumerate()
                .find_map(|(si, s)| {
                    s.windows
                        .iter()
                        .position(|w| w.panes.contains_key(&pane_id))
                        .map(|wi| (si, wi))
                });
            if let Some((si, wi)) = found {
                return Some(&mut self.snapshot.sessions[si].windows[wi]);
            }
        }
        self.active_window_mut()
    }

    fn swap_panes(&mut self, a: PaneId, b: PaneId) -> bool {
        if a == b {
            return false;
        }
        let Some(window) = self.active_window_mut() else {
            return false;
        };
        if !window.panes.contains_key(&a) || !window.panes.contains_key(&b) {
            return false;
        }
        swap_layout_panes(&mut window.layout, a, b);
        true
    }

    fn flip_agent_pane(&mut self, target: Option<PaneId>) -> Option<PaneId> {
        let pane_id = target.or_else(|| self.active_window().map(|w| w.active_pane))?;
        let agent = self.agents.get(&pane_id)?;
        let provider = agent.provider;
        let title = agent.title.clone();
        let session_id = agent.session_id.clone();

        let mut became_terminal = false;
        for session in &mut self.snapshot.sessions {
            for window in &mut session.windows {
                if let Some(pane) = window.panes.get_mut(&pane_id) {
                    match pane.kind {
                        PaneKindSnapshot::Agent(_) => {
                            *pane = terminal_pane(
                                pane_id,
                                "claude",
                                "~/dev/zebrilus",
                                "main",
                                "zsh",
                                vec!["claude".to_owned()],
                            );
                            became_terminal = true;
                        }
                        PaneKindSnapshot::Terminal => {
                            *pane = agent_pane(pane_id, provider, &title, &session_id);
                            became_terminal = false;
                        }
                        _ => return None,
                    }
                    break;
                }
            }
        }

        if became_terminal {
            let tui = claude_tui_terminal();
            self.terminals.insert(pane_id, tui);
        } else {
            self.terminals.remove(&pane_id);
        }
        Some(pane_id)
    }

    fn update_browser_url(&mut self, pane_id: PaneId, url: &str) -> bool {
        for session in &mut self.snapshot.sessions {
            for window in &mut session.windows {
                if let Some(pane) = window.panes.get_mut(&pane_id)
                    && let PaneKindSnapshot::Browser(desc) = &mut pane.kind
                {
                    desc.tabs = vec![url.to_owned()];
                    desc.active_tab = 0;
                    url.trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .clone_into(&mut pane.title);
                    return true;
                }
            }
        }
        false
    }

    fn set_split_ratio(&mut self, window_id: WindowId, split: SplitId, ratio: f32) -> bool {
        for session in &mut self.snapshot.sessions {
            if let Some(window) = session.windows.iter_mut().find(|w| w.id == window_id) {
                return update_split_ratio(&mut window.layout, split, ratio);
            }
        }
        false
    }

    fn active_window(&self) -> Option<&WindowSnapshot> {
        let session = self.snapshot.sessions.first()?;
        let active = self
            .snapshot
            .focused_window
            .unwrap_or(session.active_window);
        session
            .windows
            .iter()
            .find(|w| w.id == active)
            .or_else(|| session.windows.first())
    }

    fn active_window_mut(&mut self) -> Option<&mut WindowSnapshot> {
        let focused = self.snapshot.focused_window;
        let session = self.snapshot.sessions.first_mut()?;
        let active = focused.unwrap_or(session.active_window);
        if session.windows.iter().any(|w| w.id == active) {
            session.windows.iter_mut().find(|w| w.id == active)
        } else {
            session.windows.first_mut()
        }
    }
}

fn success_response(request_id: u64, output: String) -> ProtocolMessage {
    ProtocolMessage::CommandResponse(CommandResponse::Success {
        request_id,
        output: RawText::from(output),
        exit_code: 0,
        stderr: String::new(),
        stdout_claim: StdoutClaim::default(),
    })
}

fn flag_value<'a>(args: &[&'a str], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == flag).then_some(pair[1]))
}

fn format_panes_list(snapshot: &MuxSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    for session in &snapshot.sessions {
        for window in &session.windows {
            for pane in window.panes.values() {
                let kind = match &pane.kind {
                    PaneKindSnapshot::Terminal => "terminal",
                    PaneKindSnapshot::Browser(_) => "browser",
                    PaneKindSnapshot::Agent(_) => "agent",
                    PaneKindSnapshot::Editor(_) => "editor",
                    PaneKindSnapshot::Picker => "picker",
                    PaneKindSnapshot::View(_) => "view",
                    PaneKindSnapshot::Other { kind } => kind.as_str(),
                };
                let active = if window.active_pane == pane.id {
                    "*"
                } else {
                    " "
                };
                lines.push(format!(
                    "{}:{} {} [{}] {}{}",
                    session.name, window.name, pane.id, kind, pane.title, active
                ));
            }
        }
    }
    lines
}

fn parse_pane_id(text: &str) -> Option<PaneId> {
    text.strip_prefix('%')?.parse::<u64>().ok().map(PaneId)
}

fn parse_window_id(text: &str) -> Option<WindowId> {
    text.strip_prefix('@')?.parse::<u64>().ok().map(WindowId)
}

fn parse_size_spec(spec: &str, total: u16) -> Option<u16> {
    if let Some(pct) = spec.strip_suffix('%') {
        let pct = pct.parse::<u32>().ok()?;
        let computed = u32::from(total).saturating_mul(pct) / 100;
        return u16::try_from(computed).ok();
    }
    spec.parse::<u16>().ok()
}

fn insert_split(
    node: &mut LayoutNode,
    anchor: PaneId,
    new_pane: PaneId,
    split_id: SplitId,
    axis: Axis,
) -> bool {
    insert_split_oriented(node, anchor, new_pane, split_id, axis, false)
}

fn insert_split_oriented(
    node: &mut LayoutNode,
    anchor: PaneId,
    new_pane: PaneId,
    split_id: SplitId,
    axis: Axis,
    before: bool,
) -> bool {
    match node {
        LayoutNode::Pane(id) if *id == anchor => {
            let (first, second) = if before {
                (LayoutNode::Pane(new_pane), LayoutNode::Pane(anchor))
            } else {
                (LayoutNode::Pane(anchor), LayoutNode::Pane(new_pane))
            };
            *node = LayoutNode::Split {
                id: split_id,
                axis,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            };
            true
        }
        LayoutNode::Split { first, second, .. } => {
            insert_split_oriented(first, anchor, new_pane, split_id, axis, before)
                || insert_split_oriented(second, anchor, new_pane, split_id, axis, before)
        }
        _ => false,
    }
}

fn view_pane_snapshot(id: PaneId, title: &str, descriptor: ViewDescriptor) -> PaneSnapshot {
    PaneSnapshot {
        id,
        title: title.to_owned(),
        kind: PaneKindSnapshot::View(descriptor),
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary {
            changed_at: START_MS / 1000,
            name: title.to_owned(),
            ..PaneSummary::default()
        },
    }
}

fn remove_pane_from_layout(node: &mut LayoutNode, target: PaneId) -> bool {
    let LayoutNode::Split { first, second, .. } = node else {
        return false;
    };
    if matches!(first.as_ref(), LayoutNode::Pane(id) if *id == target) {
        *node = std::mem::replace(second.as_mut(), LayoutNode::Empty);
        return true;
    }
    if matches!(second.as_ref(), LayoutNode::Pane(id) if *id == target) {
        *node = std::mem::replace(first.as_mut(), LayoutNode::Empty);
        return true;
    }
    remove_pane_from_layout(first, target) || remove_pane_from_layout(second, target)
}

fn swap_layout_panes(node: &mut LayoutNode, a: PaneId, b: PaneId) {
    match node {
        LayoutNode::Pane(id) => {
            if *id == a {
                *id = b;
            } else if *id == b {
                *id = a;
            }
        }
        LayoutNode::Split { first, second, .. } => {
            swap_layout_panes(first, a, b);
            swap_layout_panes(second, a, b);
        }
        LayoutNode::Empty => {}
    }
}

fn update_split_ratio(node: &mut LayoutNode, target: SplitId, next_ratio: f32) -> bool {
    match node {
        LayoutNode::Split {
            id,
            ratio,
            first,
            second,
            ..
        } => {
            if *id == target {
                *ratio = next_ratio;
                true
            } else {
                update_split_ratio(first, target, next_ratio)
                    || update_split_ratio(second, target, next_ratio)
            }
        }
        _ => false,
    }
}

fn make_window(
    id: WindowId,
    index: u32,
    name: &str,
    active_pane: PaneId,
    layout: LayoutNode,
    panes: BTreeMap<PaneId, PaneSnapshot>,
) -> WindowSnapshot {
    let pane_order: Vec<PaneId> = panes.keys().copied().collect();
    WindowSnapshot {
        id,
        index,
        name: name.to_owned(),
        automatic_rename: false,
        active_pane,
        zoomed_pane: None,
        layout,
        panes,
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: format!("{index}:{name}"),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::Off,
        pane_border_lines: PaneBorderLines::Single,
        pane_border_indicators: PaneBorderIndicators::Colour,
        pane_order,
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 160,
        sy: 48,
        pane_base_index: 0,
    }
}

fn terminal_pane(
    id: PaneId,
    title: &str,
    cwd: &str,
    branch: &str,
    shell: &str,
    process: Vec<String>,
) -> PaneSnapshot {
    PaneSnapshot {
        id,
        title: title.to_owned(),
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
        summary: PaneSummary {
            changed_at: START_MS / 1000,
            unseen: false,
            command: Some(ShellCommand::Finished(0)),
            name: title.to_owned(),
            cwd: cwd.to_owned(),
            repo: if branch.is_empty() {
                String::new()
            } else {
                "zebrilus".to_owned()
            },
            branch: branch.to_owned(),
            process,
            flip_armed: false,
            shell: shell.to_owned(),
        },
    }
}

fn agent_pane(id: PaneId, provider: AgentProvider, title: &str, session_id: &str) -> PaneSnapshot {
    PaneSnapshot {
        id,
        title: title.to_owned(),
        kind: PaneKindSnapshot::Agent(AgentDescriptor {
            provider,
            cwd: Some(PathBuf::from("/Users/demo/dev/zebrilus")),
            session_id: Some(session_id.to_owned()),
            model: Some("claude-sonnet-4-6".to_owned()),
            effort: None,
            account: None,
        }),
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: Some(PaneStatus {
            state: PaneStatusState::Idle,
            kind: None,
            progress: None,
            app: provider.as_str().to_owned(),
            title: title.to_owned(),
            message: String::new(),
        }),
        summary: PaneSummary {
            changed_at: START_MS / 1000,
            unseen: false,
            command: None,
            name: title.to_owned(),
            cwd: "~/dev/zebrilus".to_owned(),
            repo: "zebrilus".to_owned(),
            branch: "main".to_owned(),
            process: vec![provider.command_name().to_owned()],
            flip_armed: false,
            shell: "zsh".to_owned(),
        },
    }
}

fn browser_pane_tabs(id: PaneId, title: &str, urls: &[&str]) -> PaneSnapshot {
    PaneSnapshot {
        id,
        title: title.to_owned(),
        kind: PaneKindSnapshot::Browser(BrowserDescriptor {
            tabs: urls.iter().map(|url| (*url).to_owned()).collect(),
            active_tab: 0,
            profile: DEFAULT_BROWSER_PROFILE.to_owned(),
        }),
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary {
            changed_at: START_MS / 1000,
            name: title.to_owned(),
            ..PaneSummary::default()
        },
    }
}

fn overview_terminal() -> DemoTerminal {
    DemoTerminal::new(
        "zsh",
        "~/dev/zebrilus",
        "demo@zz $",
        vec![
            Line::spans(vec![
                Span::new(Tone::Accent, "zz"),
                Span::new(Tone::Bold, "  zebrilus"),
            ]),
            Line::empty(),
            Line::plain("tmux + ghostty + browser + agents."),
            Line::styled(Tone::Muted, "one daemon. any device."),
            Line::empty(),
            Line::styled(
                Tone::Success,
                "curl -fsSL https://autistic.computer/zz/install.sh | sh",
            ),
            Line::empty(),
            Line::styled(Tone::Muted, "type help · or pick a room on the left"),
        ],
        true,
    )
}

fn claude_tui_terminal() -> DemoTerminal {
    DemoTerminal::new(
        "claude",
        "~/dev/zebrilus",
        ">",
        vec![
            Line::spans(vec![
                Span::new(Tone::Accent, "Claude Code"),
                Span::new(Tone::Muted, "  ·  same daemon session"),
            ]),
            Line::empty(),
            Line::plain("> How does zz work?"),
            Line::styled(
                Tone::Success,
                "  One daemon owns every pane. Close the app — nothing stops.",
            ),
            Line::empty(),
            Line::styled(Tone::Muted, "Click 'Switch to chat' above to flip back."),
        ],
        true,
    )
}

fn benchmarks_terminal() -> DemoTerminal {
    DemoTerminal::new(
        "benchmarks",
        "~/dev/zebrilus",
        "demo@zz $",
        vec![
            Line::styled(
                Tone::Bold,
                "Linux · Vulkan                         cat-ascii   doom-fire",
            ),
            Line::styled(
                Tone::Muted,
                "────────────────────────────────────────────────────────────",
            ),
            Line::styled(
                Tone::Success,
                "zz            ████████████████████      645 MB/s    1285 fps",
            ),
            Line::plain("ghostty-tip   ███████████████████▍      637 MB/s    1216 fps"),
            Line::plain("alacritty     ████                      129 MB/s    1255 fps"),
            Line::styled(
                Tone::Warn,
                "ghostty+tmux  █▊                         56 MB/s     201 fps",
            ),
            Line::empty(),
            Line::styled(
                Tone::Bold,
                "macOS · Metal (M4 Max)                 cat-ascii   doom-fire",
            ),
            Line::styled(
                Tone::Muted,
                "────────────────────────────────────────────────────────────",
            ),
            Line::styled(
                Tone::Success,
                "zz            ████████████████████      328 MB/s     668 fps",
            ),
            Line::plain("cmux          █████████████████▍        285 MB/s     627 fps"),
            Line::plain("ghostty       █████▋                     94 MB/s     630 fps"),
            Line::styled(
                Tone::Warn,
                "ghostty+tmux  ██▋                        44 MB/s     194 fps",
            ),
            Line::empty(),
            Line::styled(
                Tone::Accent,
                "-> ~8-11x faster throughput than ghostty + tmux",
            ),
        ],
        false,
    )
}

fn desktop_terminal() -> DemoTerminal {
    DemoTerminal::new(
        "vite dev",
        "~/src/app",
        "you@desktop $",
        vec![
            Line::styled(Tone::Info, "ssh://you@desktop · Arch Linux"),
            Line::empty(),
            Line::plain("$ npm run dev"),
            Line::styled(Tone::Success, "  VITE v6.2  ready in 148 ms"),
            Line::plain("  -> http://localhost:3000/"),
            Line::empty(),
        ],
        true,
    )
}

fn sample_config_file() -> &'static str {
    concat!(
        "# ~/.config/zz/config\n",
        "host-desktop = ssh://you@desktop\n",
        "browser-egress = true\n"
    )
}

#[cfg(test)]
mod tests {
    use zz_protocol::{
        AttachOperation, ClientHello, ClientInstanceId, ClientKind, CommandInvocation,
        CommandRequest, Hello, PROTOCOL_VERSION, PaneId, PaneKindSnapshot, ProtocolMessage,
    };

    use super::DemoHost;

    fn sample_hello() -> Hello {
        let mut hello = Hello::from_client(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Interactive,
            device_name: Some("Test".into()),
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            working_directory: None,
            environment: Vec::new(),
            process_id: 0,
            revision: zz_protocol::REVISION,
        });
        hello.attach = Some(AttachOperation::Session(String::new()));
        hello
    }

    #[test]
    fn showcase_handshake_flip_and_split_round_trip() {
        let mut host = DemoHost::for_host("local");
        let replies = host.handle(ProtocolMessage::Hello(sample_hello()));
        assert!(
            replies
                .iter()
                .any(|m| matches!(m, ProtocolMessage::ServerHello(_)))
        );
        assert!(
            replies
                .iter()
                .any(|m| matches!(m, ProtocolMessage::Attached { .. }))
        );

        for reply in &replies {
            let encoded =
                zz_protocol::encode_protocol_message(reply).expect("encode showcase frame");
            let _ = zz_protocol::decode_protocol_frame(&encoded).expect("decode showcase frame");
        }

        let flip = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 1,
            command: CommandInvocation::new("flip-agent", ["-t", "%1"]),
            prepared: false,
        }));
        assert!(flip.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s)
                    if matches!(s.sessions[0].windows[0].panes[&PaneId(1)].kind, PaneKindSnapshot::Terminal)
            )
        )));

        let split = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 2,
            command: CommandInvocation::new("split-window", ["-h", "-t", "%0"]),
            prepared: false,
        }));
        assert!(split.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s) if s.sessions[0].windows[0].panes.len() == 3
            )
        )));

        let view = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 10,
            command: CommandInvocation::new(
                "view-pane",
                [
                    "-t",
                    "%1",
                    "-T",
                    "Flowchart",
                    "-y",
                    "32",
                    "-k",
                    "diagram",
                    "-l",
                    "mermaid",
                    "--",
                    "flowchart LR\n  A --> B",
                ],
            ),
            prepared: false,
        }));
        assert!(view.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s) if s.sessions[0].windows[0].floating.len() == 1
            )
        )));

        let float_cmd = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 11,
            command: CommandInvocation::new(
                "break-pane",
                ["-W", "-x", "33%", "-y", "30", "-s", "%0"],
            ),
            prepared: false,
        }));
        assert!(float_cmd.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s)
                    if s.sessions[0].windows[0].floating.iter().any(|f| f.pane == PaneId(0))
            )
        )));

        let place_cmd = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 12,
            command: CommandInvocation::new("move-pane", ["-t", "%0", "-P", "bottom-left"]),
            prepared: false,
        }));
        assert!(place_cmd.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s)
                    if s.sessions[0].windows[0].floating.iter().any(|f| f.pane == PaneId(0) && f.xoff == 1)
            )
        )));

        let tile_cmd = host.handle(ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 13,
            command: CommandInvocation::new("join-pane", ["-s", "%0", "-t", "%0"]),
            prepared: false,
        }));
        assert!(tile_cmd.iter().any(|m| matches!(
            m,
            ProtocolMessage::Event(e) if matches!(
                &e.payload,
                zz_protocol::EventPayload::Snapshot(s)
                    if !s.sessions[0].windows[0].floating.iter().any(|f| f.pane == PaneId(0))
            )
        )));

        for message in [
            ProtocolMessage::AgentReplay {
                pane: PaneId(1),
                from_seq: 0,
            },
            ProtocolMessage::SetColorScheme(zz_terminal::TerminalColorScheme::Dark),
            ProtocolMessage::SetColorScheme(zz_terminal::TerminalColorScheme::Light),
            ProtocolMessage::CommandRequest(CommandRequest {
                request_id: 3,
                command: CommandInvocation::new("providers", Vec::<String>::new()),
                prepared: false,
            }),
            ProtocolMessage::FileReadRequest {
                request_id: 4,
                pane: PaneId(6),
                path: "/Users/demo/.config/zz/config".to_owned(),
            },
            ProtocolMessage::AgentPrompt {
                pane: PaneId(1),
                text: "benchmarks".to_owned(),
                images: Vec::new(),
            },
        ] {
            let replies = host.handle(message);
            for reply in &replies {
                let encoded =
                    zz_protocol::encode_protocol_message(reply).expect("encode reply frame");
                let _ = zz_protocol::decode_protocol_frame(&encoded).expect("decode reply frame");
            }
        }
    }
}
