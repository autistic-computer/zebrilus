use std::rc::Rc;

use zz_client::window_summary::{
    PaneTag::{self, Agent, Browser, Editor, Picker, Terminal},
    PaneTile, age_label, summarize_host,
};
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    ScrollHandle, Styled as _, Window, div, prelude::*, px,
};
use zz_protocol::{
    AgentProvider, Axis, LayoutNode, MuxSnapshot, PaneBorderIndicators, PaneBorderLines,
    PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus, PaneStatusKind,
    PaneStatusState, PaneSummary, SessionId, SessionSnapshot, ShellCommand, SplitId, WindowId,
    WindowSnapshot,
};
use zz_ui::{
    ActiveTheme as _,
    command::PaletteHostId,
    menu::PopupMenuItem,
    navigation::{
        WORKSPACE_SIDEBAR_DEFAULT_WIDTH, WORKSPACE_SIDEBAR_FADE_HEIGHT,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatRowState, FlatState, FlatTarget,
            FlatWindow, HostConnection, flat_row_ground, flat_sidebar, flat_tile_stack,
            flat_window_row,
        },
    },
    scroll::ScrollableElement as _,
};

use super::{
    fixtures::{agent_kind, browser_kind, editor_kind},
    tree::flat_view_sidebar,
};
use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "flat-sidebar",
    name: "Flat sidebar",
    group: "Workspace",
    summary: "The flat session sidebar: host and session headers over one two-line row per window, every row drawn from the window summary the daemon's pane statuses produce.",
    sections: &[
        Section {
            id: "sidebar",
            name: "Sidebar",
            summary: "Two hosts built from mux snapshots and run through summarize_host. Click a row to make it current, click a host or session header to fold it; hover is real state, so the actions button and the brighter session label follow the pointer.",
            build: |_, cx| cx.new(|_| Live::new(Some(SIDEBAR_HEIGHT))).into(),
        },
        Section {
            id: "rows",
            name: "Window rows",
            summary: "flat_window_row in every state the summary can produce. Line 1 names the window, line 2 says what it is doing or where it is, its lead in the tone of what needs you; the spinner is a turn in progress. The stack is the active pane's tile with up to two blank tiles fanned out behind it; point at it to open every pane.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "stacks",
            name: "Tile stacks",
            summary: "flat_tile_stack for one to five panes on the rest, hover, current and selected grounds. The active pane's tile leads with its glyph; up to two blank tiles fan out behind it, 7px apart, each cut out of the one behind by a ring in the row's color. Point at a stack to open every pane in a row.",
            build: |_, cx| stateless(stacks, cx),
        },
        Section {
            id: "hosts",
            name: "Hosts",
            summary: "Folded sessions and hosts keep a rollup of what needs you on the right. A host that is not connected says so, with Connect, Retry or Upgrade when there is something to do; hover Unreachable for the reason. Its rows dim and keep their last known state, with no spinners.",
            build: |_, cx| stateless(hosts, cx),
        },
    ],
};

const NOW: u64 = 1_792_000_000;
const MINUTE: u64 = 60;
const HOUR: u64 = 3_600;
const SIDEBAR_HEIGHT: f32 = 800.0;
const STACK_CELL: f32 = 48.0;
const MAC: &str = "/Users/fabrico/dev/zz";
const MACBOOK: PaletteHostId = PaletteHostId(0);
const UBUNTU: PaletteHostId = PaletteHostId(1);
const EXTRAS: PaletteHostId = PaletteHostId(2);
const DEVBOX: PaletteHostId = PaletteHostId(3);

trait PaneFixture: Sized {
    fn ago(self, seconds: u64) -> Self;
    fn status(self, state: PaneStatusState, kind: Option<PaneStatusKind>, message: &str) -> Self;
    fn unseen(self) -> Self;
    fn at(self, cwd: &str, branch: &str) -> Self;
    fn exit(self, code: u8) -> Self;
    fn bell(self) -> Self;
}

impl PaneFixture for PaneSnapshot {
    fn ago(mut self, seconds: u64) -> Self {
        self.summary.changed_at = NOW - seconds;
        self
    }

    fn status(
        mut self,
        state: PaneStatusState,
        kind: Option<PaneStatusKind>,
        message: &str,
    ) -> Self {
        self.status = Some(PaneStatus {
            state,
            kind,
            progress: None,
            app: String::new(),
            title: String::new(),
            message: message.to_owned(),
        });
        self
    }

    fn unseen(mut self) -> Self {
        self.summary.unseen = true;
        self
    }

    fn at(mut self, cwd: &str, branch: &str) -> Self {
        cwd.clone_into(&mut self.summary.cwd);
        cwd.clone_into(&mut self.summary.repo);
        branch.clone_into(&mut self.summary.branch);
        self
    }

    fn exit(mut self, code: u8) -> Self {
        self.summary.command = Some(ShellCommand::Finished(code));
        self
    }

    fn bell(mut self) -> Self {
        self.bell = true;
        self
    }
}

fn pane(id: u64, title: &str, kind: PaneKindSnapshot) -> PaneSnapshot {
    PaneSnapshot {
        id: PaneId(id),
        title: title.to_owned(),
        kind,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary::default(),
    }
}

fn claude(id: u64, title: &str) -> PaneSnapshot {
    pane(id, title, agent_kind(AgentProvider::ClaudeCode))
}

fn codex(id: u64, title: &str) -> PaneSnapshot {
    pane(id, title, agent_kind(AgentProvider::Codex))
}

fn terminal(id: u64, title: &str) -> PaneSnapshot {
    pane(id, title, PaneKindSnapshot::Terminal)
}

fn browser(id: u64, title: &str, url: &str) -> PaneSnapshot {
    pane(id, title, browser_kind(url))
}

fn window(id: u64, name: Option<&str>, panes: Vec<PaneSnapshot>) -> WindowSnapshot {
    let ids = panes.iter().map(|pane| pane.id).collect::<Vec<_>>();
    let layout = ids
        .iter()
        .rev()
        .map(|id| LayoutNode::Pane(*id))
        .reduce(|second, first| LayoutNode::Split {
            id: SplitId(id),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(first),
            second: Box::new(second),
        })
        .unwrap_or(LayoutNode::Empty);
    WindowSnapshot {
        id: WindowId(id),
        index: id as u32,
        name: name.unwrap_or("zsh").to_owned(),
        automatic_rename: name.is_none(),
        active_pane: ids.first().copied().unwrap_or_default(),
        zoomed_pane: None,
        layout,
        panes: panes.into_iter().map(|pane| (pane.id, pane)).collect(),
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::Off,
        pane_border_lines: PaneBorderLines::Single,
        pane_border_indicators: PaneBorderIndicators::Colour,
        pane_order: ids.clone(),
        pane_z_order: ids,
        floating: Vec::new(),
        modal: None,
        sx: 120,
        sy: 40,
    }
}

fn session(id: u64, name: &str, windows: Vec<WindowSnapshot>) -> SessionSnapshot {
    SessionSnapshot {
        id: SessionId(id),
        name: name.to_owned(),
        active_window: windows.first().map(|window| window.id).unwrap_or_default(),
        windows,
        viewers: Vec::new(),
    }
}

fn host(
    id: PaletteHostId,
    name: &str,
    remote: bool,
    connection: HostConnection,
    sessions: Vec<SessionSnapshot>,
) -> FlatHost {
    let snapshot = MuxSnapshot {
        generation: 1,
        sessions,
        focused_window: None,
    };
    FlatHost::new(
        id,
        name,
        remote,
        connection,
        summarize_host(&snapshot),
        |changed_at| age_label(changed_at, NOW),
    )
}

fn macbook() -> FlatHost {
    use PaneStatusKind::{Permission, Question};
    use PaneStatusState::{Blocked, Done, Error, Working};
    host(
        MACBOOK,
        "macbook",
        false,
        HostConnection::Connected,
        vec![
            session(
                1,
                "zz",
                vec![
                    window(
                        1,
                        None,
                        vec![
                            claude(1, "Flat sidebar design")
                                .status(Working, None, "Drawing the row states on the canvas")
                                .ago(4),
                        ],
                    ),
                    window(
                        2,
                        None,
                        vec![
                            codex(2, "codex-review")
                                .status(Blocked, Some(Permission), "cargo clippy --workspace")
                                .ago(MINUTE + 15),
                        ],
                    ),
                    window(
                        3,
                        None,
                        vec![
                            claude(3, "Pane glass fallback")
                                .status(Blocked, Some(Question), "Keep the hairline on WebGL?")
                                .ago(6 * MINUTE),
                        ],
                    ),
                    window(
                        4,
                        None,
                        vec![terminal(4, "zsh").at(MAC, "main").ago(4 * MINUTE)],
                    ),
                ],
            ),
            session(
                2,
                "gpu-usage-hunt",
                vec![
                    window(
                        5,
                        None,
                        vec![
                            claude(5, "GPU usage hunt")
                                .status(Done, None, "glow shader −39% GPU")
                                .unseen()
                                .ago(2 * MINUTE),
                        ],
                    ),
                    window(
                        6,
                        None,
                        vec![
                            browser(6, "zz Storybook", "http://localhost:8097/").ago(HOUR),
                            terminal(7, "just storybook run").at(MAC, "main"),
                        ],
                    ),
                ],
            ),
            session(
                3,
                "1",
                vec![
                    window(
                        7,
                        None,
                        vec![
                            terminal(8, "astro build")
                                .at("/Users/fabrico/dev/zz/site", "main")
                                .exit(1)
                                .ago(12 * MINUTE),
                        ],
                    ),
                    window(
                        8,
                        None,
                        vec![
                            claude(9, "Release notes")
                                .status(Error, None, "rate limited")
                                .ago(9 * MINUTE),
                        ],
                    ),
                ],
            ),
        ],
    )
}

fn ubuntu() -> FlatHost {
    use PaneStatusState::{Done, Working};
    host(
        UBUNTU,
        "ubuntu",
        true,
        HostConnection::Connected,
        vec![
            session(
                1,
                "zz-ci",
                vec![
                    window(
                        1,
                        None,
                        vec![
                            claude(1, "Daemon flaky tests")
                                .status(Working, None, "Running cargo test -p zz-daemon")
                                .ago(12),
                        ],
                    ),
                    window(
                        2,
                        Some("GPU bench"),
                        vec![
                            terminal(2, "bench/run.sh")
                                .at("/home/fabrico/zz/bench", "main")
                                .ago(3 * HOUR),
                            codex(3, "codex"),
                            browser(4, "Grafana", "http://localhost:3000/"),
                        ],
                    ),
                ],
            ),
            session(
                2,
                "bench",
                vec![window(
                    3,
                    None,
                    vec![
                        codex(5, "Throughput bench")
                            .status(Done, None, "")
                            .ago(11 * HOUR),
                    ],
                )],
            ),
        ],
    )
}

fn extras() -> FlatHost {
    host(
        EXTRAS,
        "extras",
        false,
        HostConnection::Connected,
        vec![session(
            9,
            "rows",
            vec![
                window(
                    20,
                    None,
                    vec![terminal(20, "cargo test").at(MAC, "main").bell().ago(30)],
                ),
                window(
                    21,
                    None,
                    vec![browser(21, "zz Storybook", "http://localhost:8097/").ago(HOUR)],
                ),
                window(
                    22,
                    None,
                    vec![
                        claude(22, "Agent CLI borrows")
                            .status(
                                PaneStatusState::Working,
                                None,
                                "Reading crates/zz-cli/src/main.rs",
                            )
                            .ago(3),
                        terminal(23, "zsh").at(MAC, "main"),
                        browser(24, "docs.rs", "https://docs.rs/zz-gpui"),
                    ],
                ),
                window(
                    23,
                    None,
                    vec![
                        pane(
                            25,
                            "flat.rs",
                            editor_kind("crates/zz-ui/src/navigation/flat.rs"),
                        )
                        .ago(20 * MINUTE),
                    ],
                ),
                window(
                    24,
                    None,
                    vec![
                        terminal(26, "cargo watch")
                            .at(MAC, "feat/sidebar-data")
                            .ago(5 * MINUTE),
                        claude(27, "Sidebar stacks"),
                        browser(28, "zz Storybook", "http://localhost:8098/"),
                        pane(
                            29,
                            "flat.rs",
                            editor_kind("crates/zz-ui/src/navigation/flat.rs"),
                        ),
                    ],
                ),
            ],
        )],
    )
}

fn devbox() -> FlatHost {
    host(
        DEVBOX,
        "devbox",
        true,
        HostConnection::Reconnecting,
        vec![session(
            1,
            "api",
            vec![
                window(
                    1,
                    None,
                    vec![
                        claude(1, "Auth middleware")
                            .status(PaneStatusState::Idle, None, "Running pytest -x")
                            .ago(2 * MINUTE),
                    ],
                ),
                window(
                    2,
                    None,
                    vec![
                        terminal(2, "zsh")
                            .at("/home/fabrico/src/api", "main")
                            .ago(20 * MINUTE),
                    ],
                ),
                window(
                    3,
                    None,
                    vec![
                        codex(3, "Migrations")
                            .status(PaneStatusState::Done, None, "Wrote 0042_sessions.sql")
                            .unseen()
                            .ago(25 * MINUTE),
                        terminal(4, "zsh").at("/home/fabrico/src/api", "main"),
                    ],
                ),
                window(
                    4,
                    None,
                    vec![
                        claude(5, "Rate limiter")
                            .status(PaneStatusState::Working, None, "Running the load test")
                            .ago(3 * MINUTE),
                    ],
                ),
            ],
        )],
    )
}

fn offline(id: u32, name: &str, connection: HostConnection) -> FlatHost {
    host(PaletteHostId(id), name, true, connection, Vec::new())
}

fn sidebar_hosts() -> Vec<FlatHost> {
    vec![macbook(), ubuntu()]
}

fn window_menu() -> FlatMenu {
    Rc::new(|_, _, menu, _, _| {
        menu.item(PopupMenuItem::new("Rename window"))
            .item(PopupMenuItem::new("Close window"))
    })
}

fn noop() -> FlatHandler {
    Rc::new(|_, _, _| {})
}

fn find(host: &FlatHost, id: u64) -> &FlatWindow {
    host.sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find(|window| window.summary.window == WindowId(id))
        .expect("fixture window")
}

pub(crate) struct Live {
    hosts: Vec<FlatHost>,
    state: FlatState,
    scroll: ScrollHandle,
    height: Option<f32>,
}

impl Live {
    pub(crate) fn new(height: Option<f32>) -> Self {
        Self {
            hosts: sidebar_hosts(),
            state: FlatState {
                current: Some((MACBOOK, WindowId(1))),
                ..FlatState::default()
            },
            scroll: ScrollHandle::new(),
            height,
        }
    }

    fn handle(&mut self, event: FlatEvent, cx: &mut Context<Self>) {
        let changed = match event {
            FlatEvent::Open(host, window) | FlatEvent::OpenPane(host, window, _) => {
                self.state.current = Some((host, window));
                true
            }
            FlatEvent::FoldHost(host) => {
                if let Some(host) = self.hosts.iter_mut().find(|item| item.id == host) {
                    host.folded = !host.folded;
                }
                true
            }
            FlatEvent::FoldSession(host, session) => {
                if let Some(session) = self
                    .hosts
                    .iter_mut()
                    .find(|item| item.id == host)
                    .and_then(|host| host.sessions.iter_mut().find(|item| item.id == session))
                {
                    session.folded = !session.folded;
                }
                true
            }
            FlatEvent::Hover(target, hovered) => self.state.hover(target, hovered),
            FlatEvent::NewWindow(..) | FlatEvent::Connect(_) | FlatEvent::Upgrade(_) => false,
        };
        if changed {
            cx.notify();
        }
    }
}

impl Render for Live {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let handler: FlatHandler = Rc::new(move |event, _, cx| {
            let _ = view.update(cx, |this, cx| this.handle(event, cx));
        });
        let list = flat_sidebar(
            &self.hosts,
            self.state,
            &handler,
            &window_menu(),
            cx.theme().ground(),
            cx,
        );
        div()
            .id("flat-live")
            .flex()
            .map(|this| match self.height {
                Some(height) => this.h(px(height)),
                None => this.h_full(),
            })
            .child(flat_view_sidebar(
                div()
                    .relative()
                    .size_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("flat-live-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(list),
                    )
                    .bottom_fade(
                        &self.scroll,
                        cx.theme().background,
                        WORKSPACE_SIDEBAR_FADE_HEIGHT,
                    )
                    .vertical_scrollbar(&self.scroll),
                cx,
            ))
    }
}

fn row_frame(index: usize, content: impl IntoElement) -> AnyElement {
    div()
        .id(("flat-row-case", index))
        .w(px(WORKSPACE_SIDEBAR_DEFAULT_WIDTH))
        .child(content)
        .into_any_element()
}

fn rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let handler = noop();
    let (mac, ubuntu, extras, devbox) = (macbook(), ubuntu(), extras(), devbox());
    let rest = FlatRowState::default();
    let current = FlatRowState {
        current: true,
        ..rest
    };
    let selected = FlatRowState {
        selected: true,
        ..rest
    };
    let hovered = FlatRowState {
        hovered: true,
        ..rest
    };
    let dimmed = FlatRowState {
        dimmed: true,
        ..rest
    };
    let cases: [(&str, PaletteHostId, &FlatWindow, FlatRowState); 23] = [
        ("working", MACBOOK, find(&mac, 1), rest),
        ("needs approval", MACBOOK, find(&mac, 2), rest),
        ("asked a question", MACBOOK, find(&mac, 3), rest),
        ("done, unseen", MACBOOK, find(&mac, 5), rest),
        ("done, seen", UBUNTU, find(&ubuntu, 3), rest),
        ("failed", MACBOOK, find(&mac, 8), rest),
        ("shell, idle", MACBOOK, find(&mac, 4), rest),
        ("shell, exit code", MACBOOK, find(&mac, 7), rest),
        ("bell", EXTRAS, find(&extras, 20), rest),
        ("browser", EXTRAS, find(&extras, 21), rest),
        ("editor", EXTRAS, find(&extras, 23), rest),
        ("split, two panes", MACBOOK, find(&mac, 6), rest),
        (
            "split, three panes, agent working",
            EXTRAS,
            find(&extras, 22),
            rest,
        ),
        (
            "split, four panes: still two cards",
            EXTRAS,
            find(&extras, 24),
            rest,
        ),
        ("current", MACBOOK, find(&mac, 1), current),
        ("current, three panes", EXTRAS, find(&extras, 22), current),
        (
            "selected: the keyboard cursor, inked for the selection fill",
            MACBOOK,
            find(&mac, 2),
            selected,
        ),
        ("selected, two panes", MACBOOK, find(&mac, 6), selected),
        ("hover", MACBOOK, find(&mac, 2), hovered),
        ("hover on a working row", MACBOOK, find(&mac, 1), hovered),
        ("hover, four panes", EXTRAS, find(&extras, 24), hovered),
        (
            "host reconnecting: a stale turn shows no spinner",
            DEVBOX,
            find(&devbox, 4),
            dimmed,
        ),
        (
            "host reconnecting: a split, dimmed",
            DEVBOX,
            find(&devbox, 3),
            dimmed,
        ),
    ];
    cases
        .into_iter()
        .enumerate()
        .fold(
            states().columns(2),
            |list, (index, (label, host, window, state))| {
                list.state(
                    label,
                    row_frame(
                        index,
                        flat_window_row(host, window, state, &handler, &window_menu(), cx),
                    ),
                )
            },
        )
        .into_any_element()
}

fn stacks(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let grounds = [
        FlatRowState::default(),
        FlatRowState {
            hovered: true,
            ..FlatRowState::default()
        },
        FlatRowState {
            current: true,
            ..FlatRowState::default()
        },
        FlatRowState {
            selected: true,
            ..FlatRowState::default()
        },
    ];
    let cell = |id: u64, tags: &[PaneTag], state: FlatRowState| {
        let tiles = tags
            .iter()
            .enumerate()
            .map(|(index, tag)| PaneTile {
                pane: PaneId(index as u64),
                tag: *tag,
                label: format!("{tag:?}"),
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(STACK_CELL))
            .rounded(cx.theme().radius)
            .bg(flat_row_ground(state, cx).fill)
            .child(flat_tile_stack(MACBOOK, WindowId(id), &tiles, state, cx))
    };
    let claude = Agent(AgentProvider::ClaudeCode);
    let codex = Agent(AgentProvider::Codex);
    let kinds = [claude, codex, Terminal, Browser, Editor, Picker];
    let on_every_ground = |id: u64, tags: &[PaneTag]| {
        row().children(
            grounds
                .iter()
                .enumerate()
                .map(|(index, state)| cell(id * 10 + index as u64, tags, *state)),
        )
    };
    states()
        .state(
            "one pane, every kind",
            row().children(
                kinds
                    .iter()
                    .enumerate()
                    .map(|(index, kind)| cell(index as u64, &[*kind], grounds[0])),
            ),
        )
        .state(
            "one pane on rest, hover, current and selected",
            on_every_ground(1, &[claude]),
        )
        .state(
            "two panes: one tile behind",
            on_every_ground(2, &[Browser, Terminal]),
        )
        .state(
            "three panes: two tiles behind",
            on_every_ground(3, &[claude, Terminal, Browser]),
        )
        .state(
            "five panes: still two behind, all five when open",
            on_every_ground(5, &[Terminal, codex, Browser, Editor, claude]),
        )
        .into_any_element()
}

fn host_frame(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .w(px(WORKSPACE_SIDEBAR_DEFAULT_WIDTH))
        .py(px(8.0))
        .rounded(px(12.0))
        .framed(cx)
        .overflow_hidden()
        .child(content)
        .into_any_element()
}

fn hosts(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let handler = noop();
    let mut mac = macbook();
    for session in &mut mac.sessions[1..] {
        session.folded = true;
    }
    let mut ubuntu = ubuntu();
    ubuntu.folded = true;
    let folding = [mac, ubuntu];
    let folding_state = FlatState {
        current: Some((MACBOOK, WindowId(1))),
        hovered: Some(FlatTarget::Session(MACBOOK, SessionId(1))),
        ..FlatState::default()
    };
    let refused = "ssh: connect to host studio port 22: Connection refused";
    let connections = [
        devbox(),
        offline(4, "studio", HostConnection::Unreachable(refused.into())),
        offline(5, "pi", HostConnection::Incompatible("zz 0.15".into())),
        offline(6, "alienware", HostConnection::Connecting),
        offline(7, "mini", HostConnection::Disconnected),
    ];
    let mut stale = devbox();
    stale.folded = true;
    let mut lost = devbox();
    lost.id = PaletteHostId(8);
    lost.name = "buildbox".into();
    lost.connection = HostConnection::Unreachable(refused.into());
    lost.folded = true;
    let folded = [stale, lost];
    states()
        .columns(2)
        .state(
            "folding: session zz hovered, the rest folded to their rollups",
            host_frame(
                "flat-hosts-folding",
                flat_sidebar(
                    &folding,
                    folding_state,
                    &handler,
                    &window_menu(),
                    cx.theme().ground(),
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "connection states",
            host_frame(
                "flat-hosts-connections",
                flat_sidebar(
                    &connections,
                    FlatState::default(),
                    &handler,
                    &window_menu(),
                    cx.theme().ground(),
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "folded while not connected: no spinner; a host with an action drops its rollup",
            host_frame(
                "flat-hosts-folded-stale",
                flat_sidebar(
                    &folded,
                    FlatState::default(),
                    &handler,
                    &window_menu(),
                    cx.theme().ground(),
                    cx,
                ),
                cx,
            ),
        )
        .into_any_element()
}
