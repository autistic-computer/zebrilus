use ala::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, Keystroke, ParentElement as _,
    Render, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, IconName, Panel, PanelLook, Space, StyledExt as _, TextStyle,
    blocks::palette::{
        PaletteHint, PalettePill, PaletteRow, PaletteShortcut, PaletteStatus, PaletteSurface,
        PaletteVimMode, palette_badged_row, palette_empty, palette_input, palette_key_hint,
        palette_kind_badge, palette_prompt_input, palette_row, palette_row_height, palette_section,
        palette_tree_row, palette_vim_badge,
    },
    components::input::InputState,
    h_flex, v_flex,
};

use crate::story::{Section, States, Story, stateless, states};

pub const STORY: Story = Story {
    id: "palette",
    name: "Palette",
    group: "Kit",
    summary: "A command palette: an input with pills, sections of rows that match as you type, and a footer of key hints.",
    sections: &[
        Section {
            id: "rows",
            name: "Rows",
            summary: "palette_row with every PaletteRow field, at rest and selected, and palette_tree_row with disclosures.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "parts",
            name: "Parts",
            summary: "Section headers, the empty result, rows with kind badges, key hints and the vim badge.",
            build: |_, cx| stateless(parts, cx),
        },
        Section {
            id: "surface",
            name: "Surface",
            summary: "PaletteSurface with the pill input, the usage line, rows and the hint footer; a prompt input; and no matches.",
            build: |window, cx| cx.new(|cx| Surfaces::new(window, cx)).into(),
        },
    ],
};

fn keys(spellings: &[&str]) -> PaletteShortcut {
    PaletteShortcut::Keys(
        spellings
            .iter()
            .filter_map(|spelling| Keystroke::parse(spelling).ok())
            .collect(),
    )
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

fn on_surface(content: impl IntoElement) -> impl IntoElement {
    Panel::new("palette-story-surface", PanelLook::Popover)
        .v_flex()
        .w_full()
        .p(Space::S8)
        .child(content)
}

fn pair(states: States, caption: &str, id: &'static str, row: &PaletteRow, cx: &App) -> States {
    states
        .state(
            caption.to_owned(),
            on_surface(palette_row((id, 0_usize), row, false, cx)),
        )
        .state(
            format!("{caption}, selected"),
            on_surface(palette_row((id, 1_usize), row, true, cx)),
        )
}

fn rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let command = PaletteRow {
        label: "new-window".into(),
        detail: "Create a new window".into(),
        icon: Some(IconName::Terminal),
        shortcut: Some(keys(&["ctrl-b", "c"])),
        ..Default::default()
    };
    let matched = PaletteRow {
        label: "split-window".into(),
        detail: "Split a pane into two".into(),
        icon: Some(IconName::Terminal),
        shortcut: Some(PaletteShortcut::Text("prefix %".into())),
        matches: label_matches("split-window", "splt"),
        ..Default::default()
    };
    let path = PaletteRow {
        label: "work / editor / cargo watch".into(),
        detail: "Terminal".into(),
        icon: Some(IconName::SquareTerminal),
        muted_prefix: "work / editor / ".len(),
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
            label: "work".into(),
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
    states = pair(states, "command with keys", "row-command", &command, cx);
    states = pair(
        states,
        "fuzzy match, text shortcut",
        "row-matched",
        &matched,
        cx,
    );
    states = pair(states, "muted path prefix", "row-path", &path, cx);
    states = pair(states, "current and running", "row-current", &current, cx);
    for (index, (caption, row)) in hosts.iter().enumerate() {
        states = states
            .state(
                format!("status {caption}"),
                on_surface(palette_row(("row-host", index * 2), row, false, cx)),
            )
            .state(
                format!("status {caption}, selected"),
                on_surface(palette_row(("row-host", index * 2 + 1), row, true, cx)),
            );
    }
    states
        .state(
            "tree with disclosures",
            on_surface(
                v_flex().children(tree.iter().enumerate().map(|(index, row)| {
                    palette_tree_row(("row-tree", index), row, false, |_, _, _| {}, cx)
                })),
            ),
        )
        .state(
            "tree, window selected",
            on_surface(
                v_flex().children(tree.iter().enumerate().map(|(index, row)| {
                    palette_tree_row(
                        ("row-tree-selected", index),
                        row,
                        index == 1,
                        |_, _, _| {},
                        cx,
                    )
                })),
            ),
        )
        .into_any_element()
}

fn parts(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let mono = cx.theme().mono_font_family.clone();
    let completion = |id: usize, label: &str, detail: &str, kind: &str, selected: bool| {
        palette_badged_row(
            ("completion", id),
            &PaletteRow {
                label: label.to_owned().into(),
                detail: detail.to_owned().into(),
                ..Default::default()
            },
            selected,
            palette_kind_badge(kind.to_owned(), mono.clone()),
            cx,
        )
    };
    states()
        .columns(2)
        .state(
            "section headers",
            on_surface(
                v_flex()
                    .child(palette_section("Workspace", "@", cx))
                    .child(palette_section("Commands", ":", cx))
                    .child(palette_section("Hosts", "@", cx)),
            ),
        )
        .state("empty result", on_surface(palette_empty("No matches", cx)))
        .state(
            "kind badges",
            on_surface(
                v_flex()
                    .child(palette_row(
                        ("completion", 0_usize),
                        &PaletteRow {
                            label: "select-window".into(),
                            detail: "Select a window".into(),
                            ..Default::default()
                        },
                        false,
                        cx,
                    ))
                    .child(completion(1, "-t", "Target window", "OPTION", false))
                    .child(completion(2, "work:1", "editor", "VALUE", false))
                    .child(completion(3, "select-window -t 2", "", "HISTORY", false)),
            ),
        )
        .state(
            "kind badges, selected",
            on_surface(
                v_flex()
                    .child(completion(4, "-t", "Target window", "OPTION", true))
                    .child(completion(5, "-T", "Key table", "OPTION", false)),
            ),
        )
        .state(
            "key hints",
            h_flex()
                .gap_4()
                .type_style(TextStyle::Small, cx)
                .text_color(cx.theme().foreground)
                .child(palette_key_hint(["cmd-k"], "palette"))
                .child(palette_key_hint(["ctrl-b", "w"], "choose window"))
                .child(palette_key_hint(["shift-enter"], "new line")),
        )
        .state(
            "vim badge",
            h_flex()
                .gap_2()
                .child(palette_vim_badge(PaletteVimMode::Insert, None, cx))
                .child(palette_vim_badge(PaletteVimMode::Normal, None, cx))
                .child(palette_vim_badge(PaletteVimMode::Normal, Some('g'), cx)),
        )
        .into_any_element()
}

struct Surfaces {
    pills: Entity<InputState>,
    prompt: Entity<InputState>,
    empty: Entity<InputState>,
}

impl Surfaces {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            pills: cx.new(|cx| InputState::new(window, cx).placeholder("Target window")),
            prompt: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Type a command")
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
        let rows = |id: &'static str, cx: &App| {
            v_flex().children(targets.iter().enumerate().map(|(index, row)| {
                div().h(px(palette_row_height())).child(palette_row(
                    (id, index),
                    row,
                    index == 0,
                    cx,
                ))
            }))
        };
        states()
            .state(
                "status, mode and code pills, usage line, rows",
                div().id("surface-pills").child(
                    PaletteSurface::new(
                        palette_input(
                            &self.pills,
                            [
                                PalettePill::Status {
                                    label: "gpu-box".into(),
                                    status: PaletteStatus::Online,
                                },
                                PalettePill::Mode {
                                    prefix: ":".into(),
                                    label: "Command".into(),
                                },
                                PalettePill::Code("join-pane".into()),
                            ],
                            cx,
                        ),
                        1,
                    )
                    .usage("join-pane [-bdfhv] [-l size] [-s src-pane] [-t dst-pane]")
                    .rows(rows("surface-target", cx))
                    .hints(TARGET_HINTS),
                ),
            )
            .state(
                "crumb and action pills",
                div().id("surface-action").child(
                    PaletteSurface::new(
                        palette_input(
                            &self.pills,
                            [
                                PalettePill::Status {
                                    label: "gpu-box".into(),
                                    status: PaletteStatus::Online,
                                },
                                PalettePill::Crumb("train".into()),
                                PalettePill::Action("read pane".into()),
                            ],
                            cx,
                        ),
                        4,
                    )
                    .usage("runs  capture-pane -p -t train:1.0")
                    .rows(rows("surface-action-target", cx))
                    .hints(TARGET_HINTS),
                ),
            )
            .state(
                "a row that runs nothing keeps the usage line's room",
                div().id("surface-reserved").child(
                    PaletteSurface::new(
                        palette_input(
                            &self.empty,
                            [PalettePill::Status {
                                label: "local".into(),
                                status: PaletteStatus::Online,
                            }],
                            cx,
                        ),
                        5,
                    )
                    .reserve_usage()
                    .rows(rows("surface-reserved-target", cx))
                    .hints(TARGET_HINTS),
                ),
            )
            .state(
                "prompt input, no rows",
                div().id("surface-prompt").child(
                    PaletteSurface::new(palette_prompt_input(&self.prompt, ":", mono, cx), 2)
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
                    PaletteSurface::new(
                        palette_input(
                            &self.empty,
                            [PalettePill::Mode {
                                prefix: ":".into(),
                                label: "Command".into(),
                            }],
                            cx,
                        ),
                        3,
                    )
                    .rows(palette_empty("No matches", cx))
                    .hints(COMMAND_HINTS),
                ),
            )
    }
}
