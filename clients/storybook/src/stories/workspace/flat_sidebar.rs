use penna::blocks::sidebar::SIDEBAR_DEFAULT_WIDTH;
use std::{collections::BTreeSet, rc::Rc};
use zz_protocol::{HostPlatform, LinuxDistro};

use ala::{
    AnyElement, App, AppContext as _, Context, IntoElement, Keystroke, ParentElement as _, Render,
    ScrollHandle, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Label, TextStyle, components::menu::PopupMenuItem,
    components::scroll::ScrollableElement as _,
};
use zz_client::{
    program::Program,
    window_summary::{
        Attention, ChatHints, ChatMachine, ChatParent, ChatRow,
        PaneTag::{self, Browser, Editor, Picker, Terminal},
        PaneTile, number_chat_rows, summarize_chat_machine,
    },
};
use zz_protocol::{
    AgentDescriptor, AgentProvider, Axis, LayoutNode, PaneBorderIndicators, PaneBorderLines,
    PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus, PaneStatusKind,
    PaneStatusState, PaneSummary, SessionId, SessionSnapshot, ShellCommand, SplitId, WindowId,
    WindowSnapshot,
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
        workspace_sidebar_fade,
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
    summary: "Chat mode's sidebar: each machine lists its chat windows as rows, then folds the windows without a chat into one last row, the pile, or shows a lone one as a row of its own. Every stack shows a face per pane with its own status dot. A window with two or more panes, and the pile, open into one-line rows on a track.",
    sections: &[
        Section {
            id: "sidebar",
            name: "At rest",
            summary: "The board's two machines run through summarize_chat_machine: the chat windows in session and window order, then the pile of macbook's three plain windows, its front the plain window used last (nvim, the current one), and ubuntu's lone bash. Click a row to make it current, a chevron to open its window or the pile, a host header to fold it; hover is real state.",
            build: |_, cx| cx.new(|_| Live::new(Opened::None, 0)).into(),
        },
        Section {
            id: "split-open",
            name: "The split chats opened",
            summary: "The window with two chats and cargo test opened: one 32px row per pane in window order on a track from the stack's foot, each with its face, title and age. The parent row keeps its face, title and age; only the chevron turns.",
            build: |_, cx| cx.new(|_| Live::new(Opened::Split, 1)).into(),
        },
        Section {
            id: "pile-open",
            name: "The pile opened",
            summary: "The pile opened with nvim current: one row per plain window, each leading with its own small stack, the current marker on nvim's row and the track lit.",
            build: |_, cx| cx.new(|_| Live::new(Opened::Pile, 2)).into(),
        },
        Section {
            id: "both-open",
            name: "Both opened",
            summary: "The split chats and the pile opened at once.",
            build: |_, cx| cx.new(|_| Live::new(Opened::Both, 3)).into(),
        },
        Section {
            id: "attention",
            name: "Attention",
            summary: "The fork behind needs approval and takes the window row's second line while closed; zsh rang its bell, so the pile shows Bell and that window's title behind its front. Opened, each nested row says its short status word.",
            build: |_, cx| cx.new(|_| Live::attention(4)).into(),
        },
        Section {
            id: "shortcuts",
            name: "Window shortcuts",
            summary: "Holding the window shortcuts' modifier (⌘ on macOS, Ctrl elsewhere) for a moment puts a key on each chat window row in place of its age, top to bottom across the connected machines. The pile, the windows in it and nested rows take none.",
            build: |_, cx| cx.new(|_| Live::new(Opened::Both, 5).shortcuts()).into(),
        },
        Section {
            id: "rows",
            name: "Window rows",
            summary: "flat_window_row in every state the summary can produce. Line 1 names the window, line 2 says what it is doing or where it is, its lead in the tone of what needs you; the spinner is the front chat's turn in progress. The stack shows a face per pane, each with its own dot; point at it to open every pane.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "stacks",
            name: "Tile stacks",
            summary: "flat_tile_stack for one to five panes on the rest, hover, current and selected grounds. The front pane's tile leads with its glyph; up to two blank tiles fan out behind it, 7px apart, each with its own outline and its own dot, so the one in front reads against the one behind. Point at a stack to open every pane in a row.",
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
            summary: "A folded host keeps a rollup of what needs you on the right. A host that is not connected shows a status icon, with Connect, Retry or Upgrade when there is something to do; hover the icon for the reason. Its rows dim and keep their last known state, with no spinners.",
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
    fn running(self, process: &[&str]) -> Self;
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

    fn running(mut self, process: &[&str]) -> Self {
        self.summary.process = process.iter().map(|entry| (*entry).to_owned()).collect();
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

fn chat(id: u64, provider: AgentProvider, title: &str) -> PaneSnapshot {
    pane(
        id,
        title,
        PaneKindSnapshot::Agent(AgentDescriptor {
            provider,
            ..AgentDescriptor::default()
        }),
    )
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

#[derive(Clone)]
struct Machine {
    id: PaletteHostId,
    name: &'static str,
    remote: bool,
    connection: HostConnection,
    sessions: Vec<SessionSnapshot>,
    hints: ChatHints,
}

impl Machine {
    fn new(
        id: PaletteHostId,
        name: &'static str,
        remote: bool,
        connection: HostConnection,
        mut sessions: Vec<SessionSnapshot>,
        running: Running,
    ) -> Self {
        for pane in sessions
            .iter_mut()
            .flat_map(|session| &mut session.windows)
            .flat_map(|window| window.panes.values_mut())
        {
            if let Some((_, process)) = running.iter().find(|(id, _)| PaneId(*id) == pane.id) {
                *pane = pane.clone().running(process);
            }
        }
        Self {
            id,
            name,
            remote,
            connection,
            sessions,
            hints: ChatHints::default(),
        }
    }

    fn window(&self, id: WindowId) -> Option<&WindowSnapshot> {
        self.sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == id)
    }

    fn focus(&mut self, id: WindowId, pane: Option<PaneId>) {
        let Some(window) = self
            .sessions
            .iter_mut()
            .flat_map(|session| &mut session.windows)
            .find(|window| window.id == id)
        else {
            return;
        };
        if let Some(pane) = pane {
            window.active_pane = pane;
        }
        self.hints.observe(window);
    }

    fn machine(&self, open: impl Fn(ChatParent) -> bool) -> ChatMachine {
        summarize_chat_machine(&self.sessions, &self.hints, open)
    }

    fn live(&self) -> bool {
        self.connection == HostConnection::Connected
    }

    fn host(&self, machine: ChatMachine) -> FlatHost {
        let mut host = FlatHost::new(
            self.id,
            self.name,
            self.remote,
            self.connection.clone(),
            machine,
            NOW,
        );
        host.platform = match self.name {
            "macbook" | "studio" => HostPlatform::Macos,
            "ubuntu" => HostPlatform::Linux(LinuxDistro::Ubuntu),
            "devbox" => HostPlatform::Linux(LinuxDistro::Arch),
            "extras" => HostPlatform::Linux(LinuxDistro::Debian),
            "nas" => HostPlatform::Linux(LinuxDistro::Other),
            "alienware" => HostPlatform::Windows,
            _ => HostPlatform::Unknown,
        };
        host
    }

    fn flat(&self) -> FlatHost {
        self.host(self.machine(|_| false))
    }
}

const BOARD_MAC: &str = "/Users/fabrico/dev/zebrilus";
const SPLIT: WindowId = WindowId(5);
const NVIM: WindowId = WindowId(2);

fn board_macbook() -> Machine {
    use PaneStatusKind::Permission;
    use PaneStatusState::{Blocked, Working};
    let mut split = window(
        5,
        None,
        vec![
            claude(51, "Pill menu New chat")
                .at(BOARD_MAC, "pill-menu")
                .ago(4 * MINUTE),
            claude(52, "Review the pill diff")
                .at(BOARD_MAC, "pill-menu")
                .ago(20),
            terminal(53, "cargo test -p zz-ui")
                .at(BOARD_MAC, "pill-menu")
                .running(&["/Users/fabrico/.cargo/bin/cargo", "cargo"])
                .ago(6 * MINUTE),
        ],
    );
    split.active_pane = PaneId(52);
    let mut fork = window(
        6,
        None,
        vec![
            claude(61, "Plan the chat rows")
                .at(BOARD_MAC, "main")
                .ago(HOUR),
        ],
    );
    fork.panes.insert(
        PaneId(62),
        claude(62, "What the kit draws")
            .at(BOARD_MAC, "main")
            .status(Blocked, Some(Permission), "rg guide")
            .ago(10),
    );
    fork.active_pane = PaneId(62);
    let mut machine = Machine::new(
        MACBOOK,
        "macbook",
        false,
        HostConnection::Connected,
        vec![session(
            1,
            "zebrilus",
            vec![
                window(
                    1,
                    None,
                    vec![
                        claude(11, "Fix stack hit tests")
                            .at(BOARD_MAC, "hit-tests")
                            .status(Working, None, "Running the tests")
                            .ago(5),
                    ],
                ),
                window(
                    2,
                    None,
                    vec![
                        terminal(21, "nvim flat.rs")
                            .at(BOARD_MAC, "main")
                            .running(&["/opt/homebrew/bin/nvim", "nvim"])
                            .ago(2 * MINUTE),
                        terminal(22, "cargo watch -x check")
                            .at(BOARD_MAC, "main")
                            .running(&["/Users/fabrico/.cargo/bin/cargo-watch", "cargo"])
                            .ago(2 * MINUTE),
                    ],
                ),
                window(
                    3,
                    None,
                    vec![
                        claude(31, "Port bento to penna")
                            .at(BOARD_MAC, "bento")
                            .ago(12 * MINUTE),
                        terminal(32, "vite").at(BOARD_MAC, "bento").ago(12 * MINUTE),
                        terminal(33, "zsh").at(BOARD_MAC, "bento").ago(HOUR),
                    ],
                ),
                window(
                    4,
                    None,
                    vec![
                        browser(41, "ala - Rust", "https://docs.rs/ala/latest/ala/")
                            .ago(40 * MINUTE),
                    ],
                ),
                split,
                fork,
                window(
                    7,
                    None,
                    vec![terminal(71, "zsh").at("~/dev", "").ago(3 * HOUR)],
                ),
            ],
        )],
        &[],
    );
    machine.focus(NVIM, None);
    machine
}

fn board_ubuntu() -> Machine {
    Machine::new(
        UBUNTU,
        "ubuntu",
        true,
        HostConnection::Connected,
        vec![session(
            1,
            "work",
            vec![
                window(
                    1,
                    None,
                    vec![terminal(11, "bash").at("~", "").ago(5 * 24 * HOUR)],
                ),
                window(
                    2,
                    None,
                    vec![
                        codex(21, "Bump arch package")
                            .at("/home/fabrico/packaging", "main")
                            .status(PaneStatusState::Done, None, "PKGBUILD")
                            .unseen()
                            .ago(2 * HOUR),
                        browser(22, "AUR (en) - zz", "https://aur.archlinux.org/packages/zz")
                            .ago(2 * HOUR),
                    ],
                ),
                window(
                    3,
                    None,
                    vec![
                        claude(31, "Tidy release notes")
                            .at("/home/fabrico/release", "main")
                            .ago(24 * HOUR),
                    ],
                ),
            ],
        )],
        &[],
    )
}

fn board_attention() -> Machine {
    let mut machine = board_macbook();
    for window in machine
        .sessions
        .iter_mut()
        .flat_map(|session| &mut session.windows)
    {
        if window.id == WindowId(6) {
            window.active_pane = PaneId(61);
        }
        if let Some(zsh) = window.panes.get_mut(&PaneId(71)) {
            *zsh = zsh.clone().bell().ago(30);
        }
    }
    machine
}

fn rows_macbook() -> Machine {
    use PaneStatusKind::{Permission, Question};
    use PaneStatusState::{Blocked, Done, Error, Working};
    Machine::new(
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
                            chat(2, AgentProvider::Codex, "Review the clippy fixes")
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

fn rows_ubuntu() -> Machine {
    use PaneStatusState::{Done, Working};
    Machine::new(
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
                            chat(3, AgentProvider::Codex, "Profile the bench run"),
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

fn extras() -> Machine {
    Machine::new(
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

fn devbox() -> Machine {
    Machine::new(
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

fn offline(id: u32, name: &'static str, connection: HostConnection) -> Machine {
    Machine::new(
        PaletteHostId(id),
        name,
        true,
        connection,
        vec![session(
            1,
            name,
            vec![window(
                1,
                None,
                vec![claude(u64::from(id) * 10, "Release notes").ago(3 * HOUR)],
            )],
        )],
        &[],
    )
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

fn find(machine: &Machine, id: u64) -> FlatWindow {
    machine
        .machine(|parent| parent == ChatParent::Pile)
        .rows
        .into_iter()
        .flat_map(|row| match row {
            ChatRow::Window(row) => vec![row],
            ChatRow::Pile(pile) => pile.windows,
        })
        .find(|row| row.summary.window == WindowId(id))
        .map(FlatWindow::new)
        .expect("fixture window")
}

fn flip<T: Ord>(set: &mut BTreeSet<T>, key: T) {
    if !set.remove(&key) {
        set.insert(key);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Opened {
    None,
    Split,
    Pile,
    Both,
}

pub(crate) struct Live {
    machines: Vec<Machine>,
    focus: (PaletteHostId, WindowId),
    open: BTreeSet<(PaletteHostId, WindowId)>,
    piles: BTreeSet<PaletteHostId>,
    folded: BTreeSet<PaletteHostId>,
    hovered: Option<FlatTarget>,
    scroll: ScrollHandle,
    shortcuts: bool,
}

impl Live {
    fn new(opened: Opened, section: u32) -> Self {
        Self::with(vec![board_macbook(), board_ubuntu()], opened, section)
    }

    fn attention(section: u32) -> Self {
        Self::with(
            vec![board_attention(), board_ubuntu()],
            Opened::None,
            section,
        )
    }

    fn with(mut machines: Vec<Machine>, opened: Opened, section: u32) -> Self {
        let base = section * 10;
        for machine in &mut machines {
            machine.id = PaletteHostId(base + machine.id.0);
        }
        let mac = PaletteHostId(base + MACBOOK.0);
        Self {
            machines,
            focus: (mac, NVIM),
            open: if matches!(opened, Opened::Split | Opened::Both) {
                BTreeSet::from([(mac, SPLIT)])
            } else {
                BTreeSet::new()
            },
            piles: if matches!(opened, Opened::Pile | Opened::Both) {
                BTreeSet::from([mac])
            } else {
                BTreeSet::new()
            },
            folded: BTreeSet::new(),
            hovered: None,
            scroll: ScrollHandle::new(),
            shortcuts: false,
        }
    }

    const fn shortcuts(mut self) -> Self {
        self.shortcuts = true;
        self
    }

    fn handle(&mut self, event: FlatEvent, cx: &mut Context<Self>) {
        let mut state = FlatState {
            hovered: self.hovered,
            ..FlatState::default()
        };
        match event {
            FlatEvent::Open(host, window) => self.focus_on(host, window, None),
            FlatEvent::OpenPane(host, window, pane) => self.focus_on(host, window, Some(pane)),
            FlatEvent::ToggleWindow(host, window) => flip(&mut self.open, (host, window)),
            FlatEvent::TogglePile(host) => flip(&mut self.piles, host),
            FlatEvent::FoldHost(host) => flip(&mut self.folded, host),
            FlatEvent::Hover(target, hovered) => {
                if !state.hover(target, hovered) {
                    return;
                }
                self.hovered = state.hovered;
            }
            FlatEvent::Connect(_) | FlatEvent::Upgrade(_) => return,
        }
        cx.notify();
    }

    fn focus_on(&mut self, host: PaletteHostId, window: WindowId, pane: Option<PaneId>) {
        if let Some(machine) = self.machines.iter_mut().find(|machine| machine.id == host) {
            machine.focus(window, pane);
            self.focus = (host, window);
        }
    }

    fn hosts(&self) -> (Vec<FlatHost>, FlatState) {
        let mut machines = self
            .machines
            .iter()
            .map(|machine| {
                let id = machine.id;
                let rows = machine.machine(|parent| match parent {
                    ChatParent::Window(window) => self.open.contains(&(id, window)),
                    ChatParent::Pile => self.piles.contains(&id),
                });
                (machine, rows)
            })
            .collect::<Vec<_>>();
        number_chat_rows(
            machines
                .iter_mut()
                .filter(|(machine, _)| machine.live())
                .map(|(_, rows)| rows),
        );
        let (host, window) = self.focus;
        let current = machines
            .iter()
            .find(|(machine, _)| machine.id == host)
            .and_then(|(machine, rows)| {
                let pane = machine.window(window)?.active_pane;
                Some((host, rows.current(window, pane)?))
            });
        let hosts = machines
            .into_iter()
            .map(|(machine, rows)| {
                let id = machine.id;
                let mut host = machine.host(rows);
                if self.shortcuts {
                    host = host.shortcuts(|position| {
                        Keystroke::parse(&format!("secondary-{}", position + 1)).ok()
                    });
                }
                host.folded = self.folded.contains(&id);
                host
            })
            .collect();
        let state = FlatState {
            current,
            hovered: self.hovered,
            ..FlatState::default()
        };
        (hosts, state)
    }
}

impl Render for Live {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let handler: FlatHandler = Rc::new(move |event, _, cx| {
            let _ = view.update(cx, |this, cx| this.handle(event, cx));
        });
        let (hosts, state) = self.hosts();
        let list = flat_sidebar(
            &hosts,
            state,
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
                workspace_sidebar_fade(cx.theme().ground(), cx),
                WORKSPACE_SIDEBAR_FADE_HEIGHT,
            )
            .vertical_scrollbar(&self.scroll);
        div()
            .id("flat-live")
            .flex()
            .h(px(SIDEBAR_HEIGHT))
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
    let (mac, ubuntu, extras, devbox) = (rows_macbook(), rows_ubuntu(), extras(), devbox());
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
    let cases: [(&str, PaletteHostId, FlatWindow, FlatRowState); 27] = [
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
                        flat_window_row(host, &window, state, &handler, &window_menu(), NOW, cx),
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
    let stack_cell =
        |id: u64, tiles: &[(PaneTag, Option<&str>)], state: FlatRowState, attention: Attention| {
            let tiles = tiles
                .iter()
                .enumerate()
                .map(|(index, (tag, program))| PaneTile {
                    pane: PaneId(index as u64),
                    tag: *tag,
                    label: program.map_or_else(|| format!("{tag:?}"), str::to_owned),
                    program: program.and_then(Program::by_id),
                    attention: if index == 0 {
                        attention
                    } else {
                        Attention::None
                    },
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
        stack_cell(id, &tiles, state, Attention::None)
    };
    let claude = (Terminal, Some("claude"));
    let codex = (Terminal, Some("codex"));
    let (terminal, browser, editor) = ((Terminal, None), (Browser, None), (Editor, None));
    let kinds = [claude, codex, terminal, browser, editor, (Picker, None)];
    let on_every_ground = |id: u64, tiles: &[(PaneTag, Option<&str>)]| {
        row().children(grounds.iter().enumerate().map(|(index, state)| {
            stack_cell(id * 10 + index as u64, tiles, *state, Attention::None)
        }))
    };
    states()
        .state(
            "one pane: Claude Code and Codex in terminals, then every kind",
            row().children(kinds.iter().enumerate().map(|(index, kind)| {
                stack_cell(index as u64, &[*kind], grounds[0], Attention::None)
            })),
        )
        .state(
            "status dots: working, needs you, failed, done (unseen) and bell",
            row().children(
                [
                    Attention::Working,
                    Attention::NeedsYou,
                    Attention::Failed,
                    Attention::Done,
                    Attention::Bell,
                ]
                .into_iter()
                .enumerate()
                .map(|(index, attention)| {
                    stack_cell(
                        40 + index as u64,
                        &[claude, terminal],
                        grounds[0],
                        attention,
                    )
                }),
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
                attention: Attention::None,
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
                        Label::new(TextStyle::Caption, program.commands.join(" "))
                            .mono()
                            .muted()
                            .truncate()
                            .min_w_0(),
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
    let board = board_macbook();
    let current = board
        .machine(|_| false)
        .current(NVIM, PaneId(21))
        .map(|at| (MACBOOK, at));
    let mut ubuntu = board_ubuntu().flat();
    ubuntu.folded = true;
    let folding = [board.flat(), ubuntu];
    let folding_state = FlatState {
        current,
        hovered: Some(FlatTarget::Host(UBUNTU)),
        ..FlatState::default()
    };
    let refused = "ssh: connect to host studio port 22: Connection refused";
    let connections = [
        devbox().flat(),
        offline(4, "studio", HostConnection::Unreachable(refused.into())).flat(),
        offline(5, "pi", HostConnection::Incompatible("zz 0.15".into())).flat(),
        offline(6, "alienware", HostConnection::Connecting).flat(),
        offline(7, "mini", HostConnection::Disconnected).flat(),
    ];
    let mut stale = devbox().flat();
    stale.folded = true;
    let mut lost = devbox().flat();
    lost.id = PaletteHostId(8);
    lost.name = "buildbox".into();
    lost.platform = HostPlatform::Linux(LinuxDistro::Fedora);
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
