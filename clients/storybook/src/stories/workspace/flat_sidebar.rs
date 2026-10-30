use penna::sidebar::SIDEBAR_DEFAULT_WIDTH;
use std::rc::Rc;

use ala::{
    AnyElement, App, AppContext as _, Context, IntoElement, Keystroke, ParentElement as _, Render,
    ScrollHandle, Styled as _, Window, div, prelude::*, px,
};
use penna::{ActiveTheme as _, Colorize as _, menu::PopupMenuItem, scroll::ScrollableElement as _};
use zz_client::{
    program::{Program, resolve},
    window_summary::{
        AttentionRollup,
        PaneTag::{self, Browser, Editor, Picker, Terminal},
        PaneTile, WindowSummary, age_label, summarize_chats, summarize_host,
    },
};
use zz_protocol::{
    Axis, LayoutNode, MuxSnapshot, PaneBorderIndicators, PaneBorderLines, PaneBorderStatus, PaneId,
    PaneKindSnapshot, PaneSnapshot, PaneStatus, PaneStatusKind, PaneStatusState, PaneSummary,
    SessionId, SessionSnapshot, ShellCommand, SplitId, WindowId, WindowSnapshot,
};
use zz_ui::{
    command::PaletteHostId,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatRowState, FlatState, FlatTarget,
            FlatWindow, HostConnection, flat_row_ground, flat_sidebar, flat_tile_stack,
            flat_window_row,
        },
    },
};

use super::{
    fixtures::{browser_kind, editor_kind},
    tree::flat_view_sidebar,
};
use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "flat-sidebar",
    name: "Chat sidebar",
    group: "Workspace",
    summary: "Chat mode's sidebar: each machine over one two-line row per agent, drawn from the window the agent runs in. The agent's tile leads the pane stack and the panes it split stay behind it; sessions and windows without an agent stay in the session tree.",
    sections: &[
        Section {
            id: "sidebar",
            name: "Sidebar",
            summary: "Two hosts built from mux snapshots and run through summarize_chats: only windows running Claude Code or Codex make rows, and a window holding only a picker is a chat being started. Click a row to make it current, click a host header to fold it; hover is real state; a right click opens the row's menu. Pointing at a host shows New agent; nas is connected with no agents. The footer adds a host and switches between the session tree and the chats.",
            build: |_, cx| cx.new(|_| Live::new(Some(SIDEBAR_HEIGHT))).into(),
        },
        Section {
            id: "shortcuts",
            name: "Window shortcuts",
            summary: "Holding the window shortcuts' modifier (⌘ on macOS, Ctrl elsewhere) for a moment puts a key on each chat it reaches, in place of the age: the first nine chats of the connected hosts, top to bottom, so a chat keeps its number while you move around.",
            build: |_, cx| {
                cx.new(|_| Live::new(Some(SIDEBAR_HEIGHT)).shortcuts())
                    .into()
            },
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
            summary: "flat_tile_stack for one to five panes on the rest, hover, current and selected grounds. The active pane's tile leads with its glyph; up to two blank tiles fan out behind it, 7px apart, each with its own outline, so the one in front reads against the one behind. Point at a stack to open every pane in a row.",
            build: |_, cx| stateless(stacks, cx),
        },
        Section {
            id: "programs",
            name: "Program icons",
            summary: "A terminal's tile wears the program running in it: the daemon reports the foreground process and its parents up to the shell (script, invoked path, argv0, name, executable), and the first one whose path ends in a known command at a / picks the icon, the longest command winning. A shell with nothing running keeps the plain terminal tile. Every icon in the catalog, with the commands that select it.",
            build: |_, cx| stateless(programs, cx),
        },
        Section {
            id: "hosts",
            name: "Hosts",
            summary: "A folded host keeps a rollup of what needs you on the right. A connected host shows New agent at its right edge on hover. A host that is not connected shows a status icon, with Connect, Retry or Upgrade when there is something to do; hover the icon for the reason. Its rows dim and keep their last known state, with no spinners.",
            build: |_, cx| stateless(hosts, cx),
        },
    ],
};

const NOW: u64 = 1_792_000_000;
const MINUTE: u64 = 60;
const HOUR: u64 = 3_600;
const SIDEBAR_HEIGHT: f32 = 1040.0;
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
    agent_cli(id, title, "claude")
}

fn codex(id: u64, title: &str) -> PaneSnapshot {
    agent_cli(id, title, "codex")
}

fn agent_cli(id: u64, title: &str, command: &str) -> PaneSnapshot {
    let mut pane = terminal(id, title);
    pane.summary.process = vec![command.to_owned()];
    pane
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
        pane_base_index: 0,
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

type Running<'a> = &'a [(u64, &'a [&'a str])];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rows {
    Chats,
    Windows,
}

fn summaries(snapshot: &MuxSnapshot, rows: Rows) -> (Vec<WindowSummary>, AttentionRollup) {
    match rows {
        Rows::Chats => summarize_chats(&snapshot.sessions),
        Rows::Windows => {
            let (sessions, rollup) = summarize_host(snapshot);
            (
                sessions
                    .into_iter()
                    .flat_map(|session| session.windows)
                    .collect(),
                rollup,
            )
        }
    }
}

fn host(
    rows: Rows,
    id: PaletteHostId,
    name: &str,
    remote: bool,
    connection: HostConnection,
    sessions: Vec<SessionSnapshot>,
    running: Running,
) -> FlatHost {
    let snapshot = MuxSnapshot {
        generation: 1,
        sessions,
        focused_window: None,
    };
    let (mut windows, rollup) = summaries(&snapshot, rows);
    for tile in windows.iter_mut().flat_map(|window| &mut window.tiles) {
        if let Some((_, process)) = running.iter().find(|(pane, _)| PaneId(*pane) == tile.pane) {
            tile.program = resolve(process.iter().copied());
        }
    }
    FlatHost::new(
        id,
        name,
        remote,
        connection,
        (windows, rollup),
        |changed_at| age_label(changed_at, NOW),
    )
}

fn macbook(rows: Rows) -> FlatHost {
    use PaneStatusKind::{Permission, Question};
    use PaneStatusState::{Blocked, Done, Error, Working};
    host(
        rows,
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
                    window(
                        9,
                        None,
                        vec![terminal(10, "nvim flat.rs").at(MAC, "main").ago(7 * MINUTE)],
                    ),
                    window(
                        10,
                        None,
                        vec![terminal(11, "lazygit").at(MAC, "main").ago(MINUTE)],
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
                    window(11, None, vec![terminal(12, "btop").ago(HOUR)]),
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
        &[
            (7, &["/opt/homebrew/bin/just", "just", "just"]),
            (
                10,
                &[
                    "/opt/homebrew/bin/nvim",
                    "nvim",
                    "nvim",
                    "/opt/homebrew/Cellar/neovim/0.12.1/bin/nvim",
                ],
            ),
            (11, &["/opt/homebrew/bin/lazygit", "lazygit", "lazygit"]),
            (12, &["/opt/homebrew/bin/btop", "btop", "btop"]),
        ],
    )
}

fn ubuntu(rows: Rows) -> FlatHost {
    use PaneStatusState::{Done, Working};
    host(
        rows,
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
                            terminal(8, "cargo test -p zz-daemon").at("/home/fabrico/zz", "main"),
                            browser(9, "CI", "https://github.com/zz/actions"),
                        ],
                    ),
                    window(
                        2,
                        Some("GPU bench"),
                        vec![
                            terminal(2, "cargo bench")
                                .at("/home/fabrico/zz/bench", "main")
                                .ago(3 * HOUR),
                            codex(3, "codex"),
                            browser(4, "Grafana", "http://localhost:3000/"),
                        ],
                    ),
                    window(
                        4,
                        None,
                        vec![
                            terminal(6, "copilot")
                                .at("/home/fabrico/zz", "main")
                                .ago(8 * MINUTE),
                        ],
                    ),
                    window(
                        5,
                        None,
                        vec![terminal(7, "docker compose logs -f").ago(2 * HOUR)],
                    ),
                    window(6, None, vec![pane(10, "", PaneKindSnapshot::Picker)]),
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
        &[
            (2, &["/home/fabrico/.cargo/bin/cargo", "cargo", "cargo"]),
            (
                6,
                &[
                    "/usr/lib/node_modules/@github/copilot/npm-loader.js",
                    "/usr/lib/node_modules/@github/copilot/npm-loader",
                    "/usr/bin/node",
                    "node",
                    "node",
                ],
            ),
            (7, &["/usr/bin/docker", "docker", "docker"]),
        ],
    )
}

fn extras(rows: Rows) -> FlatHost {
    host(
        rows,
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
                        browser(24, "docs.rs", "https://docs.rs/ala"),
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
        &[
            (20, &["/Users/fabrico/.cargo/bin/cargo", "cargo", "cargo"]),
            (
                26,
                &[
                    "/Users/fabrico/.cargo/bin/cargo-watch",
                    "cargo-watch",
                    "/Users/fabrico/.cargo/bin/cargo",
                    "cargo",
                ],
            ),
        ],
    )
}

fn devbox(rows: Rows) -> FlatHost {
    host(
        rows,
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
                        terminal(2, "python manage.py runserver")
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
        &[(
            2,
            &[
                "/home/fabrico/src/api/manage.py",
                "/home/fabrico/src/api/manage",
                "/usr/bin/python3.12",
                "python3",
                "python3.12",
            ],
        )],
    )
}

fn offline(id: u32, name: &str, connection: HostConnection) -> FlatHost {
    host(
        Rows::Chats,
        PaletteHostId(id),
        name,
        true,
        connection,
        Vec::new(),
        &[],
    )
}

fn empty(id: u32, name: &str) -> FlatHost {
    host(
        Rows::Chats,
        PaletteHostId(id),
        name,
        true,
        HostConnection::Connected,
        Vec::new(),
        &[],
    )
}

pub(crate) fn sidebar_hosts() -> Vec<FlatHost> {
    vec![macbook(Rows::Chats), ubuntu(Rows::Chats), empty(9, "nas")]
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
    host.chats
        .iter()
        .find(|window| window.summary.window == WindowId(id))
        .expect("fixture window")
}

pub(crate) struct Live {
    hosts: Vec<FlatHost>,
    state: FlatState,
    scroll: ScrollHandle,
    height: Option<f32>,
    shortcuts: bool,
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
            shortcuts: false,
        }
    }

    pub(crate) const fn shortcuts(mut self) -> Self {
        self.shortcuts = true;
        self
    }

    pub(crate) fn cursor_and_offline(mut self) -> Self {
        self.state.selected = Some((MACBOOK, WindowId(3)));
        self.hosts
            .insert(0, offline(4, "studio", HostConnection::Disconnected));
        self
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
            FlatEvent::Hover(target, hovered) => self.state.hover(target, hovered),
            FlatEvent::NewChat(_) | FlatEvent::Connect(_) | FlatEvent::Upgrade(_) => false,
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
        let hosts = if self.shortcuts {
            numbered(&self.hosts)
        } else {
            self.hosts.clone()
        };
        let list = flat_sidebar(
            &hosts,
            self.state,
            &handler,
            &window_menu(),
            cx.theme().ground(),
            cx,
        );
        let navigation = div()
            .relative()
            .w_full()
            .flex_1()
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
            .vertical_scrollbar(&self.scroll);
        div()
            .id("flat-live")
            .flex()
            .map(|this| match self.height {
                Some(height) => this.h(px(height)),
                None => this.h_full(),
            })
            .child(flat_view_sidebar(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .min_h_0()
                    .child(navigation)
                    .into_any_element(),
                cx,
            ))
    }
}

fn numbered(hosts: &[FlatHost]) -> Vec<FlatHost> {
    let mut hosts = hosts.to_vec();
    let mut keys = (1..=9).filter_map(|key| Keystroke::parse(&format!("secondary-{key}")).ok());
    for host in hosts
        .iter_mut()
        .filter(|host| host.connection == HostConnection::Connected)
    {
        for window in &mut host.chats {
            window.shortcut = keys.next();
        }
    }
    hosts
}

fn row_frame(index: usize, content: impl IntoElement) -> AnyElement {
    div()
        .id(("flat-row-case", index))
        .w(px(SIDEBAR_DEFAULT_WIDTH))
        .child(content)
        .into_any_element()
}

fn rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let handler = noop();
    let (mac, ubuntu, extras, devbox) = (
        macbook(Rows::Windows),
        ubuntu(Rows::Windows),
        extras(Rows::Windows),
        devbox(Rows::Windows),
    );
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
    let cases: [(&str, PaletteHostId, &FlatWindow, FlatRowState); 27] = [
        ("working", MACBOOK, find(&mac, 1), rest),
        ("needs approval", MACBOOK, find(&mac, 2), rest),
        ("asked a question", MACBOOK, find(&mac, 3), rest),
        ("done, unseen", MACBOOK, find(&mac, 5), rest),
        ("done, seen", UBUNTU, find(&ubuntu, 3), rest),
        ("failed", MACBOOK, find(&mac, 8), rest),
        ("shell, idle", MACBOOK, find(&mac, 4), rest),
        ("shell, exit code", MACBOOK, find(&mac, 7), rest),
        ("program: nvim", MACBOOK, find(&mac, 9), rest),
        (
            "program: node running copilot",
            UBUNTU,
            find(&ubuntu, 4),
            rest,
        ),
        ("program, current", MACBOOK, find(&mac, 10), current),
        (
            "program, host reconnecting: dimmed",
            DEVBOX,
            find(&devbox, 2),
            dimmed,
        ),
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
    let stack_cell = |id: u64, tiles: &[(PaneTag, Option<&str>)], state: FlatRowState| {
        let tiles = tiles
            .iter()
            .enumerate()
            .map(|(index, (tag, program))| PaneTile {
                pane: PaneId(index as u64),
                tag: *tag,
                label: program.map_or_else(|| format!("{tag:?}"), str::to_owned),
                program: program.and_then(Program::by_id),
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
    let running = |id: u64, programs: &[&'static str], state: FlatRowState| {
        let tiles = programs
            .iter()
            .map(|program| (Terminal, Some(*program)))
            .collect::<Vec<_>>();
        stack_cell(id, &tiles, state)
    };
    let claude = (Terminal, Some("claude"));
    let codex = (Terminal, Some("codex"));
    let (terminal, browser, editor) = ((Terminal, None), (Browser, None), (Editor, None));
    let kinds = [claude, codex, terminal, browser, editor, (Picker, None)];
    let on_every_ground = |id: u64, tiles: &[(PaneTag, Option<&str>)]| {
        row().children(
            grounds
                .iter()
                .enumerate()
                .map(|(index, state)| stack_cell(id * 10 + index as u64, tiles, *state)),
        )
    };
    states()
        .state(
            "one pane: Claude Code and Codex in terminals, then every kind",
            row().children(
                kinds
                    .iter()
                    .enumerate()
                    .map(|(index, kind)| stack_cell(index as u64, &[*kind], grounds[0])),
            ),
        )
        .state(
            "one pane on rest, hover, current and selected",
            on_every_ground(1, &[claude]),
        )
        .state(
            "two panes: one tile behind",
            on_every_ground(2, &[browser, terminal]),
        )
        .state(
            "three panes: two tiles behind",
            on_every_ground(3, &[claude, terminal, browser]),
        )
        .state(
            "five panes: still two behind, all five when open",
            on_every_ground(5, &[terminal, codex, browser, editor, claude]),
        )
        .state(
            "terminals running programs: the front tile wears the program",
            row().children(
                [
                    "neovim",
                    "gitchanges",
                    "rust",
                    "btop",
                    "docker",
                    "python",
                    "just",
                    "bottom",
                ]
                .iter()
                .enumerate()
                .map(|(index, program)| running(60 + index as u64, &[program], grounds[0])),
            ),
        )
        .state(
            "a program on rest, hover, current and selected",
            row().children(
                grounds
                    .iter()
                    .enumerate()
                    .map(|(index, state)| running(70 + index as u64, &["neovim"], *state)),
            ),
        )
        .state(
            "programs in a split, open on hover",
            row().children(grounds.iter().enumerate().map(|(index, state)| {
                running(80 + index as u64, &["neovim", "rust", "gitchanges"], *state)
            })),
        )
        .into_any_element()
}

fn programs(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let muted = cx.theme().foreground.muted();
    let mono = cx.theme().mono_font_family.clone();
    let state = FlatRowState::default();
    Program::all()
        .iter()
        .enumerate()
        .fold(states().columns(4), |list, (index, program)| {
            let tile = PaneTile {
                pane: PaneId(0),
                tag: Terminal,
                label: program.name.to_owned(),
                program: Some(program),
            };
            list.state(
                program.name,
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(flat_tile_stack(
                        MACBOOK,
                        WindowId(1_000 + index as u64),
                        &[tile],
                        state,
                        cx,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(program.commands.join(" ")),
                    ),
            )
        })
        .into_any_element()
}

fn host_frame(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .w(px(SIDEBAR_DEFAULT_WIDTH))
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
    let mac = macbook(Rows::Chats);
    let mut ubuntu = ubuntu(Rows::Chats);
    ubuntu.folded = true;
    let folding = [mac, ubuntu];
    let folding_state = FlatState {
        current: Some((MACBOOK, WindowId(1))),
        hovered: Some(FlatTarget::Host(UBUNTU)),
        ..FlatState::default()
    };
    let refused = "ssh: connect to host studio port 22: Connection refused";
    let connections = [
        devbox(Rows::Chats),
        offline(4, "studio", HostConnection::Unreachable(refused.into())),
        offline(5, "pi", HostConnection::Incompatible("zz 0.15".into())),
        offline(6, "alienware", HostConnection::Connecting),
        offline(7, "mini", HostConnection::Disconnected),
    ];
    let mut stale = devbox(Rows::Chats);
    stale.folded = true;
    let mut lost = devbox(Rows::Chats);
    lost.id = PaletteHostId(8);
    lost.name = "buildbox".into();
    lost.connection = HostConnection::Unreachable(refused.into());
    lost.folded = true;
    let folded = [stale, lost];
    let mut quiet = macbook(Rows::Chats);
    quiet.folded = true;
    let adding = [quiet, empty(9, "nas")];
    let adding_state = FlatState {
        hovered: Some(FlatTarget::Host(MACBOOK)),
        ..FlatState::default()
    };
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
            "new session: on the hovered host only, even one with no sessions",
            host_frame(
                "flat-hosts-new-session",
                flat_sidebar(
                    &adding,
                    adding_state,
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
