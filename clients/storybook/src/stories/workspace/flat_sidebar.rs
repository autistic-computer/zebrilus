use std::rc::Rc;

use zz_client::window_summary::{
    PaneTag::{self, Agent, Browser, Editor, Picker, Terminal},
    age_label, summarize_host,
};
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, prelude::*, px,
};
use zz_protocol::{
    AgentProvider, Axis, LayoutNode, MuxSnapshot, PaneBorderIndicators, PaneBorderLines,
    PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus, PaneStatusKind,
    PaneStatusState, PaneSummary, SessionId, SessionSnapshot, ShellCommand, SplitId, WindowId,
    WindowSnapshot,
};
use zz_ui::{
    ActiveTheme as _, Ground,
    navigation::{
        WORKSPACE_SIDEBAR_DEFAULT_WIDTH,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatRowState, FlatState, FlatTarget, FlatWindow,
            HostConnection, flat_row_ground, flat_sidebar, flat_tile_stack, flat_window_row,
        },
    },
};

use super::{
    fixtures::{agent_kind, browser_kind, editor_kind},
    tree::sidebar,
};
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
            build: |_, cx| cx.new(|_| Live::new()).into(),
        },
        Section {
            id: "rows",
            name: "Window rows",
            summary: "flat_window_row in every state the summary can produce. Line 1 names the window, line 2 says what it is doing or where it is; the dot is what needs you, the spinner a turn in progress.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "stacks",
            name: "Tile stacks",
            summary: "flat_tile_stack for one, two and three or more panes, the active pane in front, on the rest, hover and current grounds. Stacked tiles cut a ring of the ground around themselves.",
            build: |_, cx| stateless(stacks, cx),
        },
        Section {
            id: "hosts",
            name: "Hosts",
            summary: "Folded sessions and hosts keep a rollup of what needs you on the right. A host that is not connected says so, with Retry or Upgrade when there is something to do, and dims the rows it still knows.",
            build: |_, cx| stateless(hosts, cx),
        },
    ],
};

const NOW: u64 = 1_792_000_000;
const MINUTE: u64 = 60;
const HOUR: u64 = 3_600;
const SIDEBAR_HEIGHT: f32 = 800.0;
const MAC: &str = "/Users/fabrico/dev/zz";

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
                "site",
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
            ],
        )],
    )
}

fn devbox() -> FlatHost {
    host(
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
            ],
        )],
    )
}

fn offline(name: &str, connection: HostConnection) -> FlatHost {
    host(name, true, connection, Vec::new())
}

fn sidebar_hosts() -> Vec<FlatHost> {
    vec![macbook(), ubuntu()]
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

struct Live {
    hosts: Vec<FlatHost>,
    state: FlatState,
}

impl Live {
    fn new() -> Self {
        Self {
            hosts: sidebar_hosts(),
            state: FlatState {
                current: Some((0, WindowId(1))),
                hovered: None,
            },
        }
    }

    fn handle(&mut self, event: FlatEvent, cx: &mut Context<Self>) {
        let changed = match event {
            FlatEvent::Open(host, window) => {
                self.state.current = Some((host, window));
                true
            }
            FlatEvent::Fold(FlatTarget::Host(host)) => {
                if let Some(host) = self.hosts.get_mut(host) {
                    host.folded = !host.folded;
                }
                true
            }
            FlatEvent::Fold(FlatTarget::Session(host, session)) => {
                if let Some(session) = self
                    .hosts
                    .get_mut(host)
                    .and_then(|host| host.sessions.iter_mut().find(|item| item.id == session))
                {
                    session.folded = !session.folded;
                }
                true
            }
            FlatEvent::Hover(target, hovered) => self.state.hover(target, hovered),
            FlatEvent::Fold(FlatTarget::Window(..))
            | FlatEvent::Actions(..)
            | FlatEvent::NewWindow(..)
            | FlatEvent::Retry(_)
            | FlatEvent::Upgrade(_) => false,
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
        let list = flat_sidebar(&self.hosts, self.state, &handler, cx);
        div()
            .id("flat-live")
            .flex()
            .h(px(SIDEBAR_HEIGHT))
            .child(sidebar(
                div()
                    .id("flat-live-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(list),
                false,
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
    let (mac, ubuntu, extras) = (macbook(), ubuntu(), extras());
    let rest = FlatRowState::default();
    let cases: [(&str, usize, &FlatWindow, FlatRowState); 17] = [
        ("working", 0, find(&mac, 1), rest),
        ("needs approval", 0, find(&mac, 2), rest),
        ("asked a question", 0, find(&mac, 3), rest),
        ("done, unseen", 0, find(&mac, 5), rest),
        ("done, seen", 1, find(&ubuntu, 3), rest),
        ("failed", 0, find(&mac, 8), rest),
        ("shell, idle", 0, find(&mac, 4), rest),
        ("shell, exit code", 0, find(&mac, 7), rest),
        ("bell", 2, find(&extras, 20), rest),
        ("browser", 2, find(&extras, 21), rest),
        ("editor", 2, find(&extras, 23), rest),
        ("split, two panes", 0, find(&mac, 6), rest),
        (
            "split, three panes, agent working",
            2,
            find(&extras, 22),
            rest,
        ),
        (
            "current",
            0,
            find(&mac, 1),
            FlatRowState {
                current: true,
                ..rest
            },
        ),
        (
            "hover",
            0,
            find(&mac, 2),
            FlatRowState {
                hovered: true,
                ..rest
            },
        ),
        (
            "hover on a working row",
            0,
            find(&mac, 1),
            FlatRowState {
                hovered: true,
                ..rest
            },
        ),
        (
            "host reconnecting",
            1,
            find(&ubuntu, 1),
            FlatRowState {
                dimmed: true,
                ..rest
            },
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
                    row_frame(index, flat_window_row(host, window, state, &handler, cx)),
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
    ]
    .map(|state| flat_row_ground(state, cx));
    let cell = |tags: &[PaneTag], ground: Ground| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(44.0))
            .rounded(cx.theme().control_radius())
            .bg(ground.fill)
            .child(flat_tile_stack(tags, ground, cx))
    };
    let claude = Agent(AgentProvider::ClaudeCode);
    let codex = Agent(AgentProvider::Codex);
    let kinds = [claude, codex, Terminal, Browser, Editor, Picker];
    let on_every_ground =
        |tags: &[PaneTag]| row().children(grounds.iter().map(|ground| cell(tags, *ground)));
    states()
        .state(
            "one pane, every kind",
            row().children(kinds.iter().map(|kind| cell(&[*kind], grounds[0]))),
        )
        .state(
            "one pane on rest, hover and current",
            on_every_ground(&[claude]),
        )
        .state(
            "two panes, the active one in front",
            row()
                .child(on_every_ground(&[Browser, Terminal]))
                .child(on_every_ground(&[codex, Terminal])),
        )
        .state(
            "three panes, the active one in front; four show three",
            row()
                .child(on_every_ground(&[claude, Terminal, Browser]))
                .child(on_every_ground(&[Terminal, codex, Browser, Editor])),
        )
        .into_any_element()
}

fn host_frame(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .w(px(WORKSPACE_SIDEBAR_DEFAULT_WIDTH))
        .py(px(8.0))
        .rounded(px(12.0))
        .border(px(1.0))
        .border_color(cx.theme().divider_color())
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
        current: Some((0, WindowId(1))),
        hovered: Some(FlatTarget::Session(0, SessionId(1))),
    };
    let connections = [
        devbox(),
        offline("studio", HostConnection::Unreachable),
        offline("pi", HostConnection::Incompatible("zz 0.15".into())),
        offline("alienware", HostConnection::Connecting),
    ];
    states()
        .columns(2)
        .state(
            "folding: session zz hovered, the rest folded to their rollups",
            host_frame(
                "flat-hosts-folding",
                flat_sidebar(&folding, folding_state, &handler, cx),
                cx,
            ),
        )
        .state(
            "connection states",
            host_frame(
                "flat-hosts-connections",
                flat_sidebar(&connections, FlatState::default(), &handler, cx),
                cx,
            ),
        )
        .into_any_element()
}
