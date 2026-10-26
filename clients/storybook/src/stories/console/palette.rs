use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use zz_client::completion::PaneKindAvailability;
use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, KeyDownEvent,
    Keystroke, ParentElement as _, Render, SharedString, Styled as _, Subscription, WeakEntity,
    Window, div, prelude::*, px,
};
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeState, ChooseTreeTarget,
    CommandInvocation, CommandPromptKind, CommandPromptMode, CommandPromptState, CommandPromptType,
    GitMark, InputMessage, InsertStyle, MuxSnapshot, PaneId, PathEntry, PathKind, PathListRoot,
    PromptCursor, SessionId, ShellKind, WindowId,
};
use zz_ui::{
    ActiveTheme as _, IconName, StyledExt as _,
    command::{
        CommandPaletteSurface, CommandPaletteView, PaletteBackend, PaletteHint, PaletteHostId,
        PaletteMode, PalettePill, PaletteRow, PaletteSettings, PaletteStatus, PaletteTarget,
        PaletteTree, PaletteTreeHost, PaletteTreePane, PaletteTreeSession, PaletteTreeWindow,
        command_kind_badge, command_palette_empty, command_palette_entry, command_palette_input,
        command_palette_row, command_palette_section, command_palette_tree_entry,
        palette_shortcut_hint, unified_command_palette_input,
    },
    h_flex,
    input::InputState,
    target::TargetPicker,
    v_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "command-palette",
    name: "Command palette",
    group: "Commands",
    summary: "One palette for going places and running commands. @ picks a target the way tmux's -t names one, with an optional host in front, and : picks a command. Pills show the scope, and daemon prompts reuse the same surface.",
    sections: &[
        Section {
            id: "rows",
            name: "Rows",
            summary: "command_palette_entry with every PaletteRow field, at rest and selected.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "parts",
            name: "Parts",
            summary: "Section headers, the empty result, completion rows with kind badges, and shortcut hints.",
            build: |_, cx| stateless(parts, cx),
        },
        Section {
            id: "surface",
            name: "Surface",
            summary: "CommandPaletteSurface assembled by hand: the unified input with pills, the prompt input, the usage line and the hint footer.",
            build: |window, cx| cx.new(|cx| Surfaces::new(window, cx)).into(),
        },
        Section {
            id: "workspace",
            name: "Workspace",
            summary: "The palette as it opens, with no query: the attached host's sessions as a tree, then every host.",
            build: |window, cx| unified(window, cx, online_tree(), true, None, &[]),
        },
        Section {
            id: "workspace-flat",
            name: "Workspace, flat",
            summary: "The same workspace with grouping off: one row per target, its path muted in front.",
            build: |window, cx| unified(window, cx, online_tree(), false, None, &[]),
        },
        Section {
            id: "search",
            name: "Search everything",
            summary: "A typed query with no prefix searches navigation, commands and hosts together, each under its own header.",
            build: |window, cx| unified(window, cx, online_tree(), true, None, &["e", "d"]),
        },
        Section {
            id: "navigate-mode",
            name: "Go to mode",
            summary: "The @ prefix, Go to: with nothing typed, the sessions on the host here, then every other host with its status dot, then ~/ for the files here. A row with more under it ends in ›: Tab opens that level, and : . or / typed by hand do the same. Enter goes to a session, window or pane, Backspace on a level's edge goes up one.",
            build: |window, cx| {
                unified(
                    window,
                    cx,
                    online_tree(),
                    true,
                    Some(PaletteMode::Target),
                    &[],
                )
            },
        },
        Section {
            id: "target-host",
            name: "Into a host",
            summary: "@gpu-box: names a host, so the level under it lists that host's sessions, then its home folder. Without a host a target means the host here: @zz: lists the windows of zz on local.",
            build: |window, cx| target(window, cx, "gpu-box:"),
        },
        Section {
            id: "target-search",
            name: "Search the fleet",
            summary: "Before any colon, typing searches every connected host at once: hosts, sessions, windows and panes, each labelled the way you would type it, so a target on the host here leaves its host out. @agents finds a window named agents here and one on gpu-box.",
            build: |window, cx| target(window, cx, "agents"),
        },
        Section {
            id: "offline",
            name: "Offline host",
            summary: "The attached daemon is not reachable: nothing to navigate, only the host to reconnect.",
            build: |window, cx| unified(window, cx, offline_tree(), true, None, &[]),
        },
        Section {
            id: "command-mode",
            name: "Command mode",
            summary: "The : prefix lists every command with its description, its key when it has one, and the usage of the selected command.",
            build: |window, cx| {
                unified(
                    window,
                    cx,
                    online_tree(),
                    true,
                    Some(PaletteMode::Command),
                    &[],
                )
            },
        },
        Section {
            id: "command-query",
            name: "Command query",
            summary: "A fuzzy query highlights the matched letters and ranks the tighter matches first.",
            build: |window, cx| command_query(window, cx, "split"),
        },
        Section {
            id: "no-matches",
            name: "No matches",
            summary: "A query nothing matches.",
            build: |window, cx| command_query(window, cx, "qxqz"),
        },
    ],
};

pub const PROMPTS: Story = Story {
    id: "daemon-prompts",
    name: "Daemon prompts",
    group: "Commands",
    summary: "Prompts and choosers the daemon drives: command-prompt with its modes, and choose-tree drawn by the palette.",
    sections: &[
        Section {
            id: "window-chooser",
            name: "Window chooser",
            summary: "choose-tree from the daemon, drawn by the palette: the daemon owns the rows and the selection.",
            build: |window, cx| {
                let backend = backend(online_tree(), true);
                let palette = cx.new(|cx| {
                    CommandPaletteView::new_window_chooser(
                        backend,
                        &window_chooser_state(),
                        1,
                        window,
                        cx,
                    )
                });
                mount(window, cx, palette, 440.0, &[])
            },
        },
        Section {
            id: "completions",
            name: "Completions",
            summary: "command-prompt with a custom prompt: the plain input, and option completions for the command typed so far.",
            build: |window, cx| {
                let palette = prompt(
                    window,
                    cx,
                    "(goto) ",
                    "select-window -",
                    CommandPromptKind::Command,
                    CommandPromptMode::Text,
                );
                mount(window, cx, palette, 360.0, &[])
            },
        },
        Section {
            id: "history",
            name: "History",
            summary: "An empty command prompt offers its history first, badged as such.",
            build: |window, cx| {
                let backend = backend(online_tree(), true);
                let state = CommandPromptState {
                    history: vec![
                        "split-window -h -c '#{pane_current_path}'".to_owned(),
                        "new-window -n logs".to_owned(),
                        "select-layout tiled".to_owned(),
                    ],
                    ..prompt_state(
                        "(command) ",
                        "",
                        CommandPromptKind::Command,
                        CommandPromptMode::Text,
                    )
                };
                let palette = cx.new(|cx| {
                    CommandPaletteView::new(backend, &state, 1, Arc::default(), window, cx)
                });
                mount(window, cx, palette, 360.0, &[])
            },
        },
        Section {
            id: "modes",
            name: "Modes",
            summary: "Value prompts by mode. Numeric, key and single prompts relay keys, so their footer names what a key does instead.",
            build: |window, cx| cx.new(|cx| PromptModes::new(window, cx)).into(),
        },
    ],
};

const LOCAL: PaletteHostId = PaletteHostId(0);
const GPU: PaletteHostId = PaletteHostId(1);
const STAGING: PaletteHostId = PaletteHostId(2);
const PI: PaletteHostId = PaletteHostId(3);

const SHORTCUTS: &[(&str, &str)] = &[
    ("new-window", "C-b c"),
    ("split-window", "C-b %"),
    ("kill-pane", "C-b x"),
    ("detach-client", "C-b d"),
    ("rename-window", "C-b ,"),
    ("choose-tree", "C-b w"),
    ("select-layout", "C-b Space"),
];

/// What a host answers when the palette asks it for a folder.
#[derive(Clone, Copy)]
pub(super) enum Files {
    Listed,
    Listing,
    Refused,
    Empty,
}

struct StoryBackend {
    tree: PaletteTree,
    settings: PaletteSettings,
    files: Files,
    picker: RefCell<Option<WeakEntity<TargetPicker>>>,
    requests: Cell<u64>,
}

impl PaletteBackend for StoryBackend {
    fn list_path(
        &self,
        host: PaletteHostId,
        dir: &str,
        _: Option<PaneId>,
        cx: &mut App,
    ) -> Result<u64, SharedString> {
        let request = self.requests.get() + 1;
        self.requests.set(request);
        let host = self
            .tree
            .host(host)
            .map(|host| host.name.clone())
            .unwrap_or_default();
        let picker = self.picker.borrow().clone();
        let files = self.files;
        let dir = dir.to_owned();
        cx.defer(move |cx| {
            if let Some(picker) = picker {
                picker
                    .update(cx, |picker, cx| {
                        answer(picker, &host, request, &dir, files, cx);
                    })
                    .ok();
            }
        });
        Ok(request)
    }

    fn feed_targets(&self, picker: &Entity<TargetPicker>, _: &mut App) -> Option<Subscription> {
        self.picker.replace(Some(picker.downgrade()));
        None
    }

    fn snapshot(&self, _: &App) -> Arc<MuxSnapshot> {
        Arc::default()
    }

    fn host_snapshot<'a>(&self, _: PaletteHostId, _: &'a App) -> Option<&'a MuxSnapshot> {
        None
    }

    fn tree(&self, _: &App) -> PaletteTree {
        self.tree.clone()
    }

    fn settings(&self, _: &App) -> PaletteSettings {
        self.settings
    }

    fn availability(&self, _: &App) -> PaneKindAvailability {
        PaneKindAvailability {
            browser: true,
            editor: true,
        }
    }

    fn command_shortcut(&self, command: &str, _: &App) -> Option<SharedString> {
        SHORTCUTS
            .iter()
            .find(|(name, _)| *name == command)
            .map(|(_, keys)| SharedString::from(*keys))
    }

    fn mono_font(&self, cx: &App) -> SharedString {
        cx.theme().mono_font_family.clone()
    }

    fn send_input(&self, _: InputMessage, _: &mut App) {}

    fn send_key(&self, _: PaneId, _: &KeyDownEvent, _: &mut App) {}

    fn active_pane(&self, _: &App) -> Option<PaneId> {
        None
    }

    fn execute(&self, _: PaletteHostId, _: CommandInvocation, _: &mut App) {}

    fn connect_host(&self, _: PaletteHostId, _: &mut App) {}

    fn activate(&self, _: PaletteHostId, _: PaletteTarget, _: &mut App) -> bool {
        false
    }
}

fn backend(tree: PaletteTree, grouped: bool) -> Rc<dyn PaletteBackend> {
    files_backend(tree, grouped, Files::Listed)
}

fn files_backend(tree: PaletteTree, grouped: bool, files: Files) -> Rc<dyn PaletteBackend> {
    Rc::new(StoryBackend {
        tree,
        settings: PaletteSettings {
            grouped,
            ..PaletteSettings::default()
        },
        files,
        picker: RefCell::new(None),
        requests: Cell::new(0),
    })
}

/// Everything under the home folder of every host, as the daemon walks it:
/// folders end in `/`.
const HOME: &[&str] = &[
    "Downloads/",
    "dev/",
    "dev/blog/",
    "dev/blog/astro.config.mjs",
    "dev/zz/",
    "dev/zz/.github/",
    "dev/zz/clients/",
    "dev/zz/clients/storybook/",
    "dev/zz/clients/storybook/src/",
    "dev/zz/clients/storybook/src/story.rs",
    "dev/zz/crates/",
    "dev/zz/crates/zz-ui/",
    "dev/zz/crates/zz-ui/src/",
    "dev/zz/crates/zz-ui/src/command/",
    "dev/zz/crates/zz-ui/src/command/palette.rs",
    "dev/zz/crates/zz-ui/src/command/palette_model.rs",
    "dev/zz/crates/zz-ui/src/command/palette_view.rs",
    "dev/zz/crates/zz-ui/src/pane/",
    "dev/zz/crates/zz-ui/src/settings/",
    "dev/zz/crates/zz-ui/src/lib.rs",
    "dev/zz/crates/zz-ui/src/target.rs",
    "dev/zz/crates/zz-ui/src/target_tests.rs",
    "dev/zz/crates/zz-ui/src/terminal.rs",
    "dev/zz/crates/zz-ui/src/which_key.rs",
    "dev/zz/knowledge/",
    "dev/zz/knowledge/designs/",
    "dev/zz/knowledge/designs/command-palette.md",
    "dev/zz/scripts/",
    "dev/zz/scripts/build-storybook.sh",
    "dev/zz/site/",
    "dev/zz/third_party/",
    "dev/zz/Cargo.lock",
    "dev/zz/Cargo.toml",
    "dev/zz/Justfile",
    "dev/zz/NOTES.md",
    "dev/zz/README.md",
    "dev/zz/mise.toml",
    "dev/zz/rust-toolchain.toml",
    "notes.md",
];

const MARKS: &[(&str, GitMark)] = &[
    ("dev/zz/Cargo.toml", GitMark::Modified),
    ("dev/zz/rust-toolchain.toml", GitMark::Added),
    ("dev/zz/NOTES.md", GitMark::Untracked),
    ("dev/zz/Justfile", GitMark::Conflicted),
    ("dev/zz/crates/", GitMark::Modified),
    ("dev/zz/clients/storybook/", GitMark::Added),
    ("dev/zz/crates/zz-ui/src/target.rs", GitMark::Added),
];

/// `dir` under the home folder, or `None` for a folder outside it.
fn under_home(dir: &str) -> Option<&str> {
    dir.strip_prefix("~/")
}

/// What `host` answers for `dir`, the way the daemon streams it.
fn answer(
    picker: &mut TargetPicker,
    host: &str,
    request: u64,
    dir: &str,
    files: Files,
    cx: &mut Context<TargetPicker>,
) {
    let root = PathListRoot {
        root: format!("/Users/fabrico/{}", under_home(dir).unwrap_or(dir))
            .trim_end_matches('/')
            .to_owned(),
        display_root: dir.to_owned(),
        cwd: None,
        insert: InsertStyle::Shell(ShellKind::Posix),
    };
    let under = |path: &'static str| {
        under_home(dir)
            .and_then(|base| path.strip_prefix(base))
            .filter(|rel| !rel.is_empty())
    };
    match files {
        Files::Listing => {}
        Files::Refused => picker.apply_begin(
            host,
            request,
            Err("Permission denied: ~/Library/Mail".to_owned()),
            cx,
        ),
        Files::Empty => {
            picker.apply_begin(host, request, Ok(root), cx);
            picker.apply_chunk(host, request, &Arc::from([]), true, false, cx);
        }
        Files::Listed => {
            let entries: Arc<[PathEntry]> = HOME
                .iter()
                .filter_map(|path| under(path))
                .map(|rel| PathEntry {
                    rel: rel.to_owned(),
                    kind: if rel.ends_with('/') {
                        PathKind::Dir
                    } else {
                        PathKind::File
                    },
                    symlink: false,
                })
                .collect();
            let marks = MARKS
                .iter()
                .filter_map(|(path, mark)| Some((under(path)?.to_owned(), *mark)))
                .collect();
            picker.apply_begin(host, request, Ok(root), cx);
            picker.apply_chunk(host, request, &entries, true, false, cx);
            picker.apply_git(host, request, marks, cx);
        }
    }
}

fn pane(
    id: u64,
    label: &str,
    detail: &str,
    program: Option<&'static str>,
    icon: IconName,
    running: bool,
) -> PaletteTreePane {
    PaletteTreePane {
        id: PaneId(id),
        label: label.to_owned(),
        detail: detail.to_owned().into(),
        program: program.map(SharedString::from),
        icon,
        running,
    }
}

fn window(
    id: u64,
    index: u32,
    name: &str,
    active: bool,
    active_pane: u64,
    panes: Vec<PaletteTreePane>,
) -> PaletteTreeWindow {
    PaletteTreeWindow {
        id: WindowId(id),
        index,
        name: name.to_owned(),
        active,
        active_pane: PaneId(active_pane),
        panes,
    }
}

fn session(
    id: u64,
    name: &str,
    active: bool,
    windows: Vec<PaletteTreeWindow>,
) -> PaletteTreeSession {
    PaletteTreeSession {
        id: SessionId(id),
        name: name.to_owned(),
        active,
        windows,
    }
}

fn host(
    id: PaletteHostId,
    name: &str,
    detail: &str,
    right: &str,
    status: PaletteStatus,
    sessions: Vec<PaletteTreeSession>,
) -> PaletteTreeHost {
    PaletteTreeHost {
        id,
        name: name.to_owned(),
        detail: detail.to_owned(),
        right: right.to_owned(),
        status,
        sessions,
    }
}

fn online_tree() -> PaletteTree {
    PaletteTree {
        attached: LOCAL,
        hosts: vec![
            host(
                LOCAL,
                "local",
                "This computer",
                "attached",
                PaletteStatus::Online,
                vec![
                    session(
                        1,
                        "zz",
                        true,
                        vec![
                            window(
                                1,
                                1,
                                "editor",
                                true,
                                10,
                                vec![
                                    pane(
                                        10,
                                        "nvim",
                                        "Terminal",
                                        None,
                                        IconName::SquareTerminal,
                                        false,
                                    ),
                                    pane(
                                        11,
                                        "cargo watch",
                                        "Terminal",
                                        None,
                                        IconName::SquareTerminal,
                                        true,
                                    ),
                                ],
                            ),
                            window(
                                2,
                                2,
                                "agents",
                                false,
                                12,
                                vec![
                                    pane(
                                        12,
                                        "fix the atlas tests",
                                        "Terminal",
                                        Some("Claude Code"),
                                        IconName::SquareTerminal,
                                        true,
                                    ),
                                    pane(
                                        13,
                                        "localhost:8097",
                                        "Browser",
                                        None,
                                        IconName::Globe,
                                        false,
                                    ),
                                ],
                            ),
                        ],
                    ),
                    session(
                        2,
                        "notes",
                        false,
                        vec![window(
                            3,
                            1,
                            "scratch",
                            false,
                            14,
                            vec![pane(14, "README.md", "Editor", None, IconName::File, false)],
                        )],
                    ),
                ],
            ),
            host(
                GPU,
                "gpu-box",
                "ssh gpu-box",
                "1 session",
                PaletteStatus::Online,
                vec![session(
                    3,
                    "train",
                    false,
                    vec![
                        window(
                            4,
                            1,
                            "logs",
                            true,
                            15,
                            vec![pane(
                                15,
                                "python train.py",
                                "Terminal",
                                None,
                                IconName::SquareTerminal,
                                true,
                            )],
                        ),
                        window(
                            5,
                            2,
                            "agents",
                            false,
                            16,
                            vec![pane(
                                16,
                                "tune the lr schedule",
                                "Terminal",
                                Some("Codex"),
                                IconName::SquareTerminal,
                                true,
                            )],
                        ),
                    ],
                )],
            ),
            host(
                STAGING,
                "staging",
                "ssh deploy@staging",
                "connecting",
                PaletteStatus::Waiting,
                Vec::new(),
            ),
            host(
                PI,
                "pi",
                "ssh pi.local",
                "offline",
                PaletteStatus::Offline,
                Vec::new(),
            ),
        ],
    }
}

fn offline_tree() -> PaletteTree {
    PaletteTree {
        attached: LOCAL,
        hosts: vec![host(
            LOCAL,
            "local",
            "This computer",
            "not running",
            PaletteStatus::Offline,
            Vec::new(),
        )],
    }
}

fn chooser_item(
    label: &str,
    detail: &str,
    target: ChooseTreeTarget,
    depth: u8,
    flags: u8,
    pane_kind: Option<ChooseTreePaneKind>,
) -> ChooseTreeItem {
    ChooseTreeItem {
        label: label.to_owned(),
        detail: detail.to_owned(),
        target,
        depth,
        flags,
        pane_kind,
        key: String::new(),
        text: String::new(),
    }
}

fn window_chooser_state() -> ChooseTreeState {
    let open = ChooseTreeItem::HAS_CHILDREN | ChooseTreeItem::EXPANDED;
    let active = ChooseTreeItem::ACTIVE;
    ChooseTreeState {
        items: vec![
            chooser_item(
                "zz",
                "2 windows",
                ChooseTreeTarget::Session(SessionId(1)),
                0,
                open | active,
                None,
            ),
            chooser_item(
                "1:editor",
                "2 panes",
                ChooseTreeTarget::Window(WindowId(1)),
                1,
                open | active,
                None,
            ),
            chooser_item(
                "nvim",
                "~/dev/zz",
                ChooseTreeTarget::Pane(PaneId(10)),
                2,
                active,
                Some(ChooseTreePaneKind::Terminal),
            ),
            chooser_item(
                "cargo watch",
                "~/dev/zz",
                ChooseTreeTarget::Pane(PaneId(11)),
                2,
                0,
                Some(ChooseTreePaneKind::Terminal),
            ),
            chooser_item(
                "2:agents",
                "2 panes",
                ChooseTreeTarget::Window(WindowId(2)),
                1,
                ChooseTreeItem::HAS_CHILDREN,
                None,
            ),
            chooser_item(
                "notes",
                "1 window",
                ChooseTreeTarget::Session(SessionId(2)),
                0,
                open,
                None,
            ),
            chooser_item(
                "1:scratch",
                "1 pane",
                ChooseTreeTarget::Window(WindowId(3)),
                1,
                ChooseTreeItem::HAS_CHILDREN,
                None,
            ),
        ],
        search: None,
        selected: 1,
        kind: ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    }
}

fn prompt_state(
    prompt: &str,
    input: &str,
    kind: CommandPromptKind,
    mode: CommandPromptMode,
) -> CommandPromptState {
    CommandPromptState {
        prompt: prompt.to_owned(),
        input: input.to_owned(),
        cursor: u32::try_from(input.chars().count()).unwrap_or(u32::MAX),
        kind,
        mode,
        history: Vec::new(),
        prompt_type: CommandPromptType::Command,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: PromptCursor::default(),
    }
}

fn prompt(
    window: &mut Window,
    cx: &mut App,
    label: &str,
    input: &str,
    kind: CommandPromptKind,
    mode: CommandPromptMode,
) -> Entity<CommandPaletteView> {
    let backend = backend(online_tree(), true);
    let state = prompt_state(label, input, kind, mode);
    cx.new(|cx| CommandPaletteView::new(backend, &state, 1, Arc::default(), window, cx))
}

struct Frame {
    palette: Entity<CommandPaletteView>,
    height: f32,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w_full()
            .h(px(self.height))
            .child(self.palette.clone())
    }
}

pub(super) fn mount(
    window: &mut Window,
    cx: &mut App,
    palette: Entity<CommandPaletteView>,
    height: f32,
    keys: &'static [&'static str],
) -> AnyView {
    if !keys.is_empty() {
        let focus = palette.read(cx).focus(cx);
        focus.focus(window, cx);
        window.on_next_frame(move |window, cx| {
            for key in keys {
                if let Ok(keystroke) = Keystroke::parse(key) {
                    window.dispatch_keystroke(keystroke, cx);
                }
            }
        });
    }
    cx.new(|_| Frame { palette, height }).into()
}

fn unified(
    window: &mut Window,
    cx: &mut App,
    tree: PaletteTree,
    grouped: bool,
    mode: Option<PaletteMode>,
    keys: &'static [&'static str],
) -> AnyView {
    let backend = backend(tree, grouped);
    let palette = cx.new(|cx| CommandPaletteView::new_unified(backend, mode, window, cx));
    mount(window, cx, palette, 560.0, keys)
}

/// The palette in `@` mode with `body` typed, a file picked going into
/// `pane`: what `prefix F` opens on a pane's folder.
pub(super) fn target_palette(
    window: &mut Window,
    cx: &mut App,
    files: Files,
    body: &str,
    pane: Option<PaneId>,
) -> Entity<CommandPaletteView> {
    let backend = files_backend(online_tree(), true, files);
    let body = body.to_owned();
    cx.new(|cx| CommandPaletteView::new_target(backend, body, pane, window, cx))
}

fn target(window: &mut Window, cx: &mut App, body: &str) -> AnyView {
    let palette = target_palette(window, cx, Files::Listed, body, None);
    mount(window, cx, palette, 560.0, &[])
}

fn command_query(window: &mut Window, cx: &mut App, query: &str) -> AnyView {
    let palette = prompt(
        window,
        cx,
        ":",
        query,
        CommandPromptKind::Command,
        CommandPromptMode::Text,
    );
    mount(window, cx, palette, 560.0, &[])
}

struct PromptModes {
    palettes: Vec<(&'static str, Entity<CommandPaletteView>)>,
}

impl PromptModes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let modes = [
            (
                "value, text",
                "(rename-window) ",
                "editor",
                CommandPromptMode::Text,
            ),
            (
                "incremental search",
                "(search down) ",
                "panic",
                CommandPromptMode::Incremental,
            ),
            ("numeric", "(index) ", "", CommandPromptMode::Numeric),
            ("key", "(key) ", "", CommandPromptMode::Key),
            (
                "single key",
                "kill-pane 2? (y/n) ",
                "",
                CommandPromptMode::Single,
            ),
        ];
        let palettes = modes
            .into_iter()
            .map(|(caption, label, input, mode)| {
                (
                    caption,
                    prompt(window, cx, label, input, CommandPromptKind::Value, mode),
                )
            })
            .collect();
        Self { palettes }
    }
}

impl Render for PromptModes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.palettes
            .iter()
            .enumerate()
            .fold(states(), |states, (index, (caption, palette))| {
                states.state(
                    *caption,
                    div()
                        .id(("prompt-mode", index))
                        .relative()
                        .w_full()
                        .h(px(100.0))
                        .child(palette.clone()),
                )
            })
    }
}

fn label_matches(label: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let mut needle = query.chars().peekable();
    label
        .char_indices()
        .filter(|(_, character)| {
            let hit = needle.peek() == Some(character);
            if hit {
                needle.next();
            }
            hit
        })
        .map(|(start, character)| start..start + character.len_utf8())
        .collect()
}

fn on_surface(content: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .w_full()
        .p(px(8.0))
        .surface(zz_ui::roles::PALETTE, cx)
        .child(content)
}

fn pair(
    states: crate::story::States,
    caption: &str,
    id: &'static str,
    row: &PaletteRow,
    cx: &App,
) -> crate::story::States {
    states
        .state(
            caption.to_owned(),
            on_surface(command_palette_entry((id, 0_usize), row, false, cx), cx),
        )
        .state(
            format!("{caption}, selected"),
            on_surface(command_palette_entry((id, 1_usize), row, true, cx), cx),
        )
}

fn rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let command = PaletteRow {
        label: "new-window".into(),
        detail: "Create a new window".into(),
        icon: Some(IconName::Terminal),
        shortcut: Some("C-b c".into()),
        ..Default::default()
    };
    let matched = PaletteRow {
        label: "split-window".into(),
        detail: "Split a pane into two".into(),
        icon: Some(IconName::Terminal),
        shortcut: Some("C-b %".into()),
        matches: label_matches("split-window", "splt"),
        ..Default::default()
    };
    let path = PaletteRow {
        label: "zz / editor / cargo watch".into(),
        detail: "Terminal".into(),
        icon: Some(IconName::SquareTerminal),
        muted_prefix: "zz / editor / ".len(),
        ..Default::default()
    };
    let current = PaletteRow {
        label: "editor".into(),
        detail: "2 panes".into(),
        right: "current".into(),
        icon: Some(IconName::PanelsTopLeft),
        running: true,
        indent: 18.0,
        expanded: Some(false),
        ..Default::default()
    };
    let host_row = |label: &str, detail: &str, right: &str, status| PaletteRow {
        label: label.to_owned().into(),
        detail: detail.to_owned().into(),
        right: right.to_owned().into(),
        icon: Some(IconName::HardDrive),
        status: Some(status),
        ..Default::default()
    };
    let hosts = [
        (
            "online",
            host_row("gpu-box", "ssh gpu-box", "1 session", PaletteStatus::Online),
        ),
        (
            "running",
            host_row("ci", "ssh ci", "building", PaletteStatus::Running),
        ),
        (
            "waiting",
            host_row(
                "staging",
                "ssh deploy@staging",
                "connecting",
                PaletteStatus::Waiting,
            ),
        ),
        (
            "offline",
            host_row("pi", "ssh pi.local", "offline", PaletteStatus::Offline),
        ),
    ];
    let tree = [
        PaletteRow {
            label: "zz".into(),
            detail: "2 windows".into(),
            right: "current".into(),
            icon: Some(IconName::Folder),
            expanded: Some(true),
            ..Default::default()
        },
        PaletteRow {
            label: "editor".into(),
            detail: "2 panes".into(),
            icon: Some(IconName::PanelsTopLeft),
            indent: 18.0,
            expanded: Some(true),
            ..Default::default()
        },
        PaletteRow {
            label: "nvim".into(),
            detail: "Terminal".into(),
            right: "current".into(),
            icon: Some(IconName::SquareTerminal),
            indent: 36.0,
            ..Default::default()
        },
        PaletteRow {
            label: "agents".into(),
            detail: "2 panes".into(),
            icon: Some(IconName::PanelsTopLeft),
            indent: 18.0,
            expanded: Some(false),
            running: true,
            ..Default::default()
        },
    ];
    let mut states = states().columns(2);
    states = pair(states, "command with key", "row-command", &command, cx);
    states = pair(states, "fuzzy match", "row-matched", &matched, cx);
    states = pair(states, "muted path prefix", "row-path", &path, cx);
    states = pair(states, "current and running", "row-current", &current, cx);
    for (index, (caption, row)) in hosts.iter().enumerate() {
        states = states
            .state(
                format!("host {caption}"),
                on_surface(
                    command_palette_entry(("row-host", index * 2), row, false, cx),
                    cx,
                ),
            )
            .state(
                format!("host {caption}, selected"),
                on_surface(
                    command_palette_entry(("row-host", index * 2 + 1), row, true, cx),
                    cx,
                ),
            );
    }
    states
        .state(
            "tree with disclosures",
            on_surface(
                v_flex().children(tree.iter().enumerate().map(|(index, row)| {
                    command_palette_tree_entry(("row-tree", index), row, false, |_, _, _| {}, cx)
                })),
                cx,
            ),
        )
        .state(
            "tree, window selected",
            on_surface(
                v_flex().children(tree.iter().enumerate().map(|(index, row)| {
                    command_palette_tree_entry(
                        ("row-tree-selected", index),
                        row,
                        index == 1,
                        |_, _, _| {},
                        cx,
                    )
                })),
                cx,
            ),
        )
        .into_any_element()
}

fn parts(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let mono = cx.theme().mono_font_family.clone();
    let completion = |id: usize, label: &str, detail: &str, kind: &str, selected: bool| {
        command_palette_row(
            ("completion", id),
            label.to_owned(),
            detail.to_owned(),
            Some(command_kind_badge(kind.to_owned(), mono.clone()).into_any_element()),
            selected,
            cx,
        )
    };
    states()
        .columns(2)
        .state(
            "section headers",
            on_surface(
                v_flex()
                    .child(command_palette_section("Workspace", "@", cx))
                    .child(command_palette_section("Commands", ":", cx))
                    .child(command_palette_section("Hosts", "@", cx)),
                cx,
            ),
        )
        .state("empty result", on_surface(command_palette_empty(cx), cx))
        .state(
            "completion kinds",
            on_surface(
                v_flex()
                    .child(command_palette_row(
                        ("completion", 0_usize),
                        "select-window",
                        "Select a window",
                        None,
                        false,
                        cx,
                    ))
                    .child(completion(1, "-t", "Target window", "OPTION", false))
                    .child(completion(2, "zz:1", "editor", "VALUE", false))
                    .child(completion(3, "select-window -t 2", "", "HISTORY", false)),
                cx,
            ),
        )
        .state(
            "completion, selected",
            on_surface(
                v_flex()
                    .child(completion(4, "-t", "Target window", "OPTION", true))
                    .child(completion(5, "-T", "Key table", "OPTION", false)),
                cx,
            ),
        )
        .state(
            "shortcut hints",
            h_flex()
                .gap_4()
                .text_size(px(12.0))
                .text_color(cx.theme().foreground)
                .child(palette_shortcut_hint(["cmd-k"], "palette"))
                .child(palette_shortcut_hint(["ctrl-b", "w"], "choose window"))
                .child(palette_shortcut_hint(["shift-enter"], "new line")),
        )
        .into_any_element()
}

struct Surfaces {
    unified: Entity<InputState>,
    prompt: Entity<InputState>,
    empty: Entity<InputState>,
}

impl Surfaces {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            unified: cx.new(|cx| InputState::new(window, cx).placeholder("Target window")),
            prompt: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Type a zz command…")
                    .default_value("new-window -n logs")
            }),
            empty: cx.new(|cx| InputState::new(window, cx).default_value("qxqz")),
        }
    }
}

const TARGET_HINTS: [PaletteHint; 3] = [
    PaletteHint {
        key: "up down",
        label: "navigate",
    },
    PaletteHint {
        key: "enter",
        label: "run",
    },
    PaletteHint {
        key: "backspace",
        label: "back",
    },
];

const COMMAND_HINTS: [PaletteHint; 4] = [
    PaletteHint {
        key: "tab",
        label: "complete",
    },
    PaletteHint {
        key: "enter",
        label: "run",
    },
    PaletteHint {
        key: "backspace",
        label: "leave mode",
    },
    PaletteHint {
        key: "escape",
        label: "close",
    },
];

impl Render for Surfaces {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = cx.theme().mono_font_family.clone();
        let targets = [
            PaletteRow {
                label: "editor".into(),
                detail: "2 panes".into(),
                right: "current".into(),
                icon: Some(IconName::PanelsTopLeft),
                ..Default::default()
            },
            PaletteRow {
                label: "agents".into(),
                detail: "2 panes".into(),
                icon: Some(IconName::PanelsTopLeft),
                running: true,
                ..Default::default()
            },
            PaletteRow {
                label: "scratch".into(),
                detail: "1 pane".into(),
                icon: Some(IconName::PanelsTopLeft),
                ..Default::default()
            },
        ];
        states()
            .state(
                "host, mode and command pills, usage line, rows",
                div().id("surface-unified").child(
                    CommandPaletteSurface::new(
                        unified_command_palette_input(
                            &self.unified,
                            [
                                PalettePill::Host {
                                    label: "gpu-box".into(),
                                    status: PaletteStatus::Online,
                                },
                                PalettePill::Mode {
                                    prefix: ":".into(),
                                    label: "Command".into(),
                                },
                                PalettePill::Command("join-pane".into()),
                            ],
                            cx,
                        ),
                        1,
                    )
                    .usage("join-pane [-bdfhv] [-l size] [-s src-pane] [-t dst-pane]")
                    .rows(
                        v_flex().children(targets.iter().enumerate().map(|(index, row)| {
                            div().h(px(28.0)).child(command_palette_entry(
                                ("surface-target", index),
                                row,
                                index == 0,
                                cx,
                            ))
                        })),
                    )
                    .hints(TARGET_HINTS),
                ),
            )
            .state(
                "prompt input, no rows",
                div().id("surface-prompt").child(
                    CommandPaletteSurface::new(
                        command_palette_input(&self.prompt, ":", mono, cx),
                        2,
                    )
                    .hints([
                        PaletteHint {
                            key: "enter",
                            label: "run",
                        },
                        PaletteHint {
                            key: "escape",
                            label: "close",
                        },
                    ]),
                ),
            )
            .state(
                "mode pill, no matches",
                div().id("surface-empty").child(
                    CommandPaletteSurface::new(
                        unified_command_palette_input(
                            &self.empty,
                            [PalettePill::Mode {
                                prefix: ":".into(),
                                label: "Command".into(),
                            }],
                            cx,
                        ),
                        3,
                    )
                    .rows(command_palette_empty(cx))
                    .hints(COMMAND_HINTS),
                ),
            )
    }
}
