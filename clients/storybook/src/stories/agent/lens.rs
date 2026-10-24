use std::sync::Arc;

use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ListAlignment,
    ListState, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
    prelude::*, px,
};
use zz_protocol::{AgentProvider, PaneId};
use zz_ui::{
    IconName,
    agent::{
        AgentEntry, AgentTimeline, AgentTimelineStore, AgentToolKind as Kind,
        AgentToolStatus as Status, TimelineRow,
        composer::COMPOSER_OUTER_PADDING,
        fold_timeline_rows,
        presentation::{spinner_phase, terminal_wait_card},
        tasks::{TaskTrayAction, TrayPanel, task_tray},
    },
    compact::compact_pane_header,
    input::InputState,
    pane::{
        PaneDragSource, PaneLens, TerminalPaneAction, pane_header_icon_button, pane_lens_button,
        terminal_pane_header,
    },
    pip::PipSurface,
    scroll::Scrollbar,
};

use super::{
    cards::permission,
    composer::{Draft, composer, composer_input},
    fixtures::{
        PLAN, TEST_FAILURE, assistant, done, plan, tab_strip_screenshot, tasks, thought, tool,
        user, user_with_images,
    },
    timeline::pane_frame,
};
use crate::stories::workspace::fixtures::{CLAUDE, CODEX, Tone, terminal_body};
use crate::story::states;

const FIRST_ANSWER: &str = "Pinned tabs now keep a **28px floor**, and only the unpinned tabs \
share what is left of the strip:\n\n\
```rust\n\
let floor = px(28.0) * pinned;\n\
let share = (width - floor).max(px(0.0)) / unpinned.max(1.0);\n\
```\n\n\
The new `thirty_tabs_three_pinned` test covers it.";

const CODEX_ANSWER: &str = "The drop preview measured each tab itself, so once pinned tabs \
stopped shrinking it landed one slot off. It now reads the widths the layout pass stored, and \
`drop_preview_tracks_pointer` passes ten runs in a row.";

const CLAUDE_ASKS: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz fix/pinned-tab-width"),
    (
        Tone::Plain,
        "> Tests are green. Push the branch and open a pull request.",
    ),
    (Tone::Muted, "• Bash(git status --short)"),
    (Tone::Plain, "  Bash command"),
    (Tone::Plain, "    git push -u origin fix/pinned-tab-width"),
    (Tone::Plain, "  Do you want to proceed?"),
    (Tone::Success, "  › 1. Yes"),
    (Tone::Muted, "    2. Yes, and don't ask again for git push"),
    (Tone::Muted, "    3. No, and tell Claude what to do (esc)"),
];

const CLAUDE_PLANS: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz fix/pinned-tab-width"),
    (
        Tone::Plain,
        "> Plan how pinned tabs keep their width, then wait for me.",
    ),
    (Tone::Plain, "  Ready to code?"),
    (Tone::Plain, "  Here is Claude's plan:"),
    (
        Tone::Muted,
        "  1. Give pinned tabs a 28px floor in tab_strip.rs",
    ),
    (Tone::Muted, "  2. Read stored widths in the drop preview"),
    (Tone::Plain, "  Would you like to proceed?"),
    (Tone::Success, "  › 1. Yes, and auto-accept edits"),
    (Tone::Muted, "    2. Yes, and manually approve edits"),
    (Tone::Muted, "    3. No, keep planning"),
];

const CODEX_ASKS: &[(Tone, &str)] = &[
    (Tone::Muted, "~/dev/zz main*"),
    (Tone::Plain, "› Bump the storybook lock and commit it"),
    (
        Tone::Muted,
        "• Ran cargo update --manifest-path clients/storybook/Cargo.toml",
    ),
    (Tone::Plain, "  Allow command?"),
    (Tone::Plain, "    git commit -am 'Bump the storybook lock'"),
    (Tone::Success, "  › Yes (y)"),
    (Tone::Muted, "    Always for this session (p)"),
    (Tone::Muted, "    No, tell Codex what to do (esc)"),
];

const ANSWERS: [(&str, Option<bool>); 2] = [("Allow", Some(true)), ("Deny", Some(false))];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scene {
    Claude,
    Codex,
    ClaudeAsks,
    ClaudePlans,
    CodexAsks,
}

impl Scene {
    const fn provider(self) -> AgentProvider {
        match self {
            Self::Claude | Self::ClaudeAsks | Self::ClaudePlans => AgentProvider::ClaudeCode,
            Self::Codex | Self::CodexAsks => AgentProvider::Codex,
        }
    }

    const fn pane(self) -> u64 {
        match self {
            Self::Claude => 14,
            Self::Codex => 15,
            Self::ClaudeAsks => 16,
            Self::CodexAsks => 17,
            Self::ClaudePlans => 18,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Claude => "Pinned tabs keep their icon width",
            Self::ClaudeAsks => "Open the pinned tabs pull request",
            Self::ClaudePlans => "Plan the pinned tabs fix",
            Self::Codex | Self::CodexAsks => "codex",
        }
    }

    const fn terminal(self) -> &'static [(Tone, &'static str)] {
        match self {
            Self::Claude => CLAUDE,
            Self::Codex => CODEX,
            Self::ClaudeAsks => CLAUDE_ASKS,
            Self::ClaudePlans => CLAUDE_PLANS,
            Self::CodexAsks => CODEX_ASKS,
        }
    }

    const fn running(self) -> bool {
        !matches!(self, Self::Codex)
    }

    fn entries(self) -> Vec<AgentEntry> {
        match self {
            Self::Claude => vec![
                user_with_images(
                    1,
                    "Pinned tabs shrink to nothing when I open thirty tabs. Can you make them keep their icon width?",
                    vec![tab_strip_screenshot()],
                ),
                thought(
                    2,
                    "The tab strip hands every tab the same share of the width, so pinned tabs shrink with the rest.",
                ),
                done(3, Kind::Search, "rg -n \"pinned\" crates/zz-ui/src/browser"),
                done(4, Kind::Read, "Read crates/zz-ui/src/browser/tab_strip.rs"),
                done(5, Kind::Edit, "Edit crates/zz-ui/src/browser/tab_strip.rs"),
                tool(
                    6,
                    Kind::Execute,
                    Status::Completed,
                    "cargo test -p zz-ui browser::tab_strip",
                )
                .terminal("running 6 tests\ntest result: ok. 6 passed; 0 failed")
                .entry(),
                assistant(7, FIRST_ANSWER),
                user(8, "Run the whole browser suite and fix whatever breaks."),
                plan(14, PLAN),
                tool(
                    9,
                    Kind::Execute,
                    Status::Failed,
                    "cargo test -p zz-ui browser",
                )
                .terminal(TEST_FAILURE)
                .exit(101)
                .entry(),
                thought(
                    10,
                    "Two failures, both in drag. The drop preview still measures each tab itself instead of reading the widths the layout pass stored.",
                ),
                done(11, Kind::Read, "Read crates/zz-ui/src/browser/drag.rs"),
                done(12, Kind::Edit, "Edit crates/zz-ui/src/browser/drag.rs"),
                tool(
                    13,
                    Kind::Execute,
                    Status::Running,
                    "cargo test -p zz-ui browser::drag",
                )
                .entry(),
            ],
            Self::Codex => vec![
                user(1, "Fix the flaky tab drag test"),
                thought(
                    2,
                    "The drop preview measures each tab itself, so it disagrees with the layout once pinned tabs stop shrinking.",
                ),
                done(
                    3,
                    Kind::Search,
                    "rg -n \"drop_preview\" crates/zz-ui/src/browser",
                ),
                done(4, Kind::Read, "Read crates/zz-ui/src/browser/drag.rs"),
                done(5, Kind::Edit, "Edit crates/zz-ui/src/browser/drag.rs"),
                tool(
                    6,
                    Kind::Execute,
                    Status::Completed,
                    "cargo test -p zz-ui browser::drag",
                )
                .terminal("running 9 tests\ntest result: ok. 9 passed; 0 failed")
                .entry(),
                assistant(7, CODEX_ANSWER),
            ],
            Self::ClaudeAsks => vec![
                user(
                    1,
                    "Tests are green. Push the branch and open a pull request.",
                ),
                done(2, Kind::Execute, "git status --short"),
                tool(
                    3,
                    Kind::Execute,
                    Status::NeedsApproval,
                    "git push -u origin fix/pinned-tab-width",
                )
                .entry(),
            ],
            Self::ClaudePlans => vec![
                user(
                    1,
                    "Plan how pinned tabs keep their width, then wait for me.",
                ),
                done(2, Kind::Search, "rg -n \"pinned\" crates/zz-ui/src/browser"),
                done(3, Kind::Read, "Read crates/zz-ui/src/browser/tab_strip.rs"),
                done(4, Kind::Read, "Read crates/zz-ui/src/browser/drag.rs"),
            ],
            Self::CodexAsks => vec![
                user(1, "Bump the storybook lock and commit it"),
                done(
                    2,
                    Kind::Execute,
                    "cargo update --manifest-path clients/storybook/Cargo.toml",
                ),
                tool(
                    3,
                    Kind::Execute,
                    Status::Running,
                    "git commit -am 'Bump the storybook lock'",
                )
                .entry(),
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Look {
    Page(f32),
    Tiled,
    Floating,
    Phone,
}

pub struct LensPane {
    scene: Scene,
    look: Look,
    open: bool,
    store: Entity<AgentTimelineStore>,
    rows: Arc<Vec<TimelineRow>>,
    list: ListState,
    input: Entity<InputState>,
    tray: Option<TrayPanel>,
    _observe: Subscription,
}

fn build(scene: Scene, look: Look, open: bool, window: &mut Window, cx: &mut App) -> AnyView {
    let input = composer_input("", window, cx);
    input.update(cx, |input, cx| {
        input.set_placeholder(format!("Message {}…", scene.provider().label()), cx);
    });
    cx.new(|cx| {
        let rows = fold_timeline_rows(&scene.entries()).rows;
        let store = cx.new(|_| AgentTimelineStore::default());
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        LensPane {
            scene,
            look,
            open,
            list: ListState::new(rows.len(), ListAlignment::Bottom, px(1200.0)),
            rows,
            store,
            input,
            tray: None,
            _observe: observe,
        }
    })
    .into()
}

pub(crate) fn tiled(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Tiled, true, window, cx)
}

pub(crate) fn floating_claude(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Floating, true, window, cx)
}

pub(crate) fn floating_codex(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Codex, Look::Floating, false, window, cx)
}

pub(crate) fn phone_claude(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Phone, true, window, cx)
}

pub(crate) fn phone_claude_asks(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::ClaudeAsks, Look::Phone, true, window, cx)
}

pub(crate) fn phone_codex(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Codex, Look::Phone, false, window, cx)
}

impl LensPane {
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let title = self.scene.title();
        if self.look == Look::Phone {
            let toggle = pane_lens_button(
                PaneLens {
                    provider: self.scene.provider(),
                    open: self.open,
                },
                move |_, cx| {
                    entity.update(cx, |view, cx| {
                        view.open = !view.open;
                        cx.notify();
                    });
                },
                cx,
            )
            .hit_slop(10.0, 10.0)
            .into_any_element();
            let close = pane_header_icon_button(
                ("lens-phone-close", self.scene.pane()),
                IconName::Xmark,
                true,
                cx,
            )
            .hit_slop(10.0, 10.0)
            .into_any_element();
            return compact_pane_header(IconName::SquareTerminal, title, [toggle, close], cx)
                .into_any_element();
        }
        terminal_pane_header(
            true,
            title,
            PaneDragSource::new(PaneId(self.scene.pane()), title, true, |_, _, _| {}),
            self.look == Look::Floating,
            Some(PaneLens {
                provider: self.scene.provider(),
                open: self.open,
            }),
            move |action, _, cx| {
                if action == TerminalPaneAction::Lens {
                    entity.update(cx, |view, cx| {
                        view.open = !view.open;
                        cx.notify();
                    });
                }
            },
            cx,
        )
        .into_any_element()
    }

    fn prefix(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match self.scene {
            Scene::Claude => {
                let phase = spinner_phase(cx.entity_id(), cx);
                let entity = cx.entity();
                let tasks = tasks().into_iter().take(1).collect::<Vec<_>>();
                task_tray(
                    "lens-tray",
                    Some(PLAN),
                    &tasks,
                    self.tray,
                    true,
                    phase,
                    move |action, _, cx| {
                        if let TaskTrayAction::Toggle(panel) = action {
                            entity.update(cx, |view, cx| {
                                view.tray = (view.tray != Some(panel)).then_some(panel);
                                cx.notify();
                            });
                        }
                    },
                    cx,
                )
                .map(IntoElement::into_any_element)
                .into_iter()
                .collect()
            }
            Scene::ClaudeAsks => vec![permission(
                "lens-permission",
                "git push -u origin fix/pinned-tab-width",
                None,
                &ANSWERS,
                0,
                true,
                cx,
            )],
            Scene::ClaudePlans => {
                let entity = cx.entity();
                vec![
                    terminal_wait_card(
                        "lens-wait",
                        "Waiting for a plan approval in the terminal",
                        move |_, cx| {
                            entity.update(cx, |view, cx| {
                                view.open = false;
                                cx.notify();
                            });
                        },
                        cx,
                    )
                    .into_any_element(),
                ]
            }
            Scene::Codex | Scene::CodexAsks => Vec::new(),
        }
    }

    fn lens(&self, cx: &mut Context<Self>) -> AnyElement {
        let timeline = AgentTimeline::new(self.rows.clone(), self.list.clone(), self.store.clone())
            .active_turn(self.scene.running())
            .bottom_padding(COMPOSER_OUTER_PADDING)
            .open_output(|_, _, _| {});
        let prefix = self.prefix(cx);
        let composer = composer(
            Draft {
                id: SharedString::from(format!("lens-composer-{}", self.scene.pane())),
                input: self.input.clone(),
                running: self.scene.running(),
                writable: true,
                attach: false,
                images: Vec::new(),
                prefix,
            },
            cx,
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .id("lens-timeline")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(timeline)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom(px(COMPOSER_OUTER_PADDING))
                            .child(Scrollbar::vertical(&self.list)),
                    ),
            )
            .child(composer)
            .into_any_element()
    }
}

impl Render for LensPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.header(cx);
        let body = if self.open {
            self.lens(cx)
        } else {
            terminal_body(self.scene.terminal(), false, cx).into_any_element()
        };
        let id = ("lens-pane", self.scene.pane());
        match self.look {
            Look::Page(height) => pane_frame(cx)
                .id(id)
                .h(px(height))
                .flex()
                .flex_col()
                .child(header)
                .child(body)
                .into_any_element(),
            Look::Tiled | Look::Phone => div()
                .id(id)
                .size_full()
                .flex()
                .flex_col()
                .child(header)
                .child(body)
                .into_any_element(),
            Look::Floating => PipSurface::new(id, div().size_full().flex().flex_col().child(body))
                .header(header)
                .content_inset(px(8.0), px(16.0))
                .into_any_element(),
        }
    }
}

pub struct Panes(Vec<(&'static str, AnyView)>);

impl Render for Panes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.iter().fold(states(), |states, (label, view)| {
            states.state(*label, view.clone())
        })
    }
}

pub fn panes(window: &mut Window, cx: &mut App) -> AnyView {
    let panes = vec![
        (
            "Claude Code with the lens open: a turn running, the plan and a background task above the composer",
            build(Scene::Claude, Look::Page(640.0), true, window, cx),
        ),
        (
            "Codex showing its terminal: the toggle opens the lens on the same session",
            build(Scene::Codex, Look::Page(360.0), false, window, cx),
        ),
    ];
    cx.new(|_| Panes(panes)).into()
}

pub fn approvals(window: &mut Window, cx: &mut App) -> AnyView {
    let panes = vec![
        (
            "Claude Code at its permission prompt: Allow types 1, Deny presses Esc",
            build(Scene::ClaudeAsks, Look::Page(500.0), true, window, cx),
        ),
        (
            "Claude Code at a plan approval or question: the lens cannot answer it, so Show terminal switches back to the TUI",
            build(Scene::ClaudePlans, Look::Page(440.0), true, window, cx),
        ),
        (
            "Codex at its own dialog: the rollout says nothing while it waits, so the lens shows the running step and no card",
            build(Scene::CodexAsks, Look::Page(380.0), true, window, cx),
        ),
    ];
    cx.new(|_| Panes(panes)).into()
}

struct ToggleCase {
    label: &'static str,
    provider: AgentProvider,
    title: &'static str,
    open: bool,
    active: bool,
    floats: bool,
    width: Option<f32>,
}

const TOGGLE: ToggleCase = ToggleCase {
    label: "",
    provider: AgentProvider::ClaudeCode,
    title: "Pinned tabs keep their icon width",
    open: false,
    active: true,
    floats: false,
    width: None,
};

const TOGGLES: [ToggleCase; 7] = [
    ToggleCase {
        label: "Claude Code, terminal showing",
        ..TOGGLE
    },
    ToggleCase {
        label: "Claude Code, lens open",
        open: true,
        ..TOGGLE
    },
    ToggleCase {
        label: "Codex, terminal showing",
        provider: AgentProvider::Codex,
        title: "codex",
        ..TOGGLE
    },
    ToggleCase {
        label: "Codex, lens open",
        provider: AgentProvider::Codex,
        title: "codex",
        open: true,
        ..TOGGLE
    },
    ToggleCase {
        label: "inactive pane: the toggle stays, the actions wait for the pointer",
        active: false,
        ..TOGGLE
    },
    ToggleCase {
        label: "floating: Tile back and Zoom take the place of the splits",
        open: true,
        floats: true,
        ..TOGGLE
    },
    ToggleCase {
        label: "narrow pane, long title, lens open",
        title: "Investigate why the browser pane drops frames on the secondary display at 120 Hz",
        open: true,
        width: Some(380.0),
        ..TOGGLE
    },
];

pub fn toggles(_: &mut Window, cx: &mut App) -> AnyView {
    crate::story::stateless(render_toggles, cx)
}

fn render_toggles(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    TOGGLES
        .iter()
        .enumerate()
        .fold(states(), |states, (index, case)| {
            let header = terminal_pane_header(
                case.active,
                case.title,
                PaneDragSource::new(PaneId(index as u64 + 1), case.title, true, |_, _, _| {}),
                case.floats,
                Some(PaneLens {
                    provider: case.provider,
                    open: case.open,
                }),
                |_, _, _| {},
                cx,
            );
            let opacity = if case.active {
                1.0
            } else {
                crate::pane().inactive
            };
            states.state(
                case.label,
                pane_frame(cx)
                    .id(("lens-toggle", index))
                    .when_some(case.width, |frame, width| frame.w(px(width)))
                    .child(div().opacity(opacity).child(header))
                    .child(div().h(px(20.0))),
            )
        })
        .into_any_element()
}
