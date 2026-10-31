use std::rc::Rc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{ActiveTheme as _, input::InputState};
use zz_protocol::chat::{ItemKind, ItemStatus};
use zz_protocol::{AgentProvider, PaneId};
use zz_ui::{
    agent::{
        presentation::{lens_welcome, spinner_phase, terminal_pip},
        tasks::{TaskTrayAction, TrayPanel, task_tray},
    },
    pane::{PaneDragSource, PaneLens, TerminalPaneAction, terminal_pane_header},
    pane_look::pane_color,
    pip::PipSurface,
};

use super::{
    chat::{Frame, Show, Thread, Views, empty_column},
    composer::{Draft, claude_settings, codex_settings, composer, composer_input},
    fixtures::{PLAN, PLAN_STEPS, Script, completed, lens_approval, tasks, terminal_ask},
};
use crate::stories::workspace::fixtures::{CLAUDE, CODEX, Tone, terminal_body};
use crate::story::{Framed as _, states};

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

const CODEX_FIRST: &[(Tone, &str)] = &[(Tone::Muted, "~/dev/zz main"), (Tone::Plain, "› ")];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scene {
    Claude,
    Codex,
    ClaudeAsks,
    ClaudePlans,
    CodexFirst,
}

impl Scene {
    const fn provider(self) -> AgentProvider {
        match self {
            Self::Claude | Self::ClaudeAsks | Self::ClaudePlans => AgentProvider::ClaudeCode,
            Self::Codex | Self::CodexFirst => AgentProvider::Codex,
        }
    }

    const fn pane(self) -> u64 {
        match self {
            Self::Claude => 14,
            Self::Codex => 15,
            Self::ClaudeAsks => 16,
            Self::ClaudePlans => 18,
            Self::CodexFirst => 19,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Claude => "Pinned tabs keep their icon width",
            Self::ClaudeAsks => "Open the pinned tabs pull request",
            Self::ClaudePlans => "Plan the pinned tabs fix",
            Self::Codex | Self::CodexFirst => "codex",
        }
    }

    const fn terminal(self) -> &'static [(Tone, &'static str)] {
        match self {
            Self::Claude => CLAUDE,
            Self::Codex => CODEX,
            Self::ClaudeAsks => CLAUDE_ASKS,
            Self::ClaudePlans => CLAUDE_PLANS,
            Self::CodexFirst => CODEX_FIRST,
        }
    }

    const fn running(self) -> bool {
        !matches!(self, Self::Codex | Self::CodexFirst)
    }

    fn icon(self) -> penna::IconName {
        match self.provider() {
            AgentProvider::ClaudeCode => zz_ui::program::CLAUDE_ICON,
            AgentProvider::Codex => zz_ui::program::OPENAI_ICON,
        }
    }

    fn script(self) -> Script {
        let mut script = Script::new();
        match self {
            Self::Claude => {
                script
                    .turn("Pinned tabs shrink to nothing when I open thirty tabs. Can you make them keep their icon width?")
                    .think(
                        "t1",
                        "**Reading the tab strip.** It hands every tab the same share of the width, so pinned tabs shrink with the rest.",
                    )
                    .done("t1")
                    .ran(
                        "s1",
                        ItemKind::Search,
                        "rg -n \"pinned\" crates/zz-ui/src/browser",
                    )
                    .ran("r1", ItemKind::Read, "crates/zz-ui/src/browser/tab_strip.rs")
                    .ran("e1", ItemKind::Edit, "crates/zz-ui/src/browser/tab_strip.rs")
                    .ran(
                        "x1",
                        ItemKind::Execute,
                        "cargo test -p zz-ui browser::tab_strip",
                    )
                    .pause(64_000)
                    .reply("a1", FIRST_ANSWER)
                    .finish(completed())
                    .turn("Run the whole browser suite and fix whatever breaks.")
                    .plan("p1", PLAN_STEPS)
                    .tool("x2", None, ItemKind::Execute, "cargo test -p zz-ui browser")
                    .end(
                        "x2",
                        ItemStatus::Failed {
                            message: Some("2 tests failed".to_owned()),
                        },
                    )
                    .think(
                        "t2",
                        "**Two failures, both in drag.** The drop preview still measures each tab itself instead of reading the widths the layout pass stored.",
                    )
                    .done("t2")
                    .ran("r2", ItemKind::Read, "crates/zz-ui/src/browser/drag.rs")
                    .ran("e2", ItemKind::Edit, "crates/zz-ui/src/browser/drag.rs")
                    .tool(
                        "x3",
                        None,
                        ItemKind::Execute,
                        "cargo test -p zz-ui browser::drag",
                    );
            }
            Self::Codex => {
                script
                    .turn("Fix the flaky tab drag test")
                    .think(
                        "t1",
                        "**Finding the drift.** The drop preview measures each tab itself, so it disagrees with the layout once pinned tabs stop shrinking.",
                    )
                    .done("t1")
                    .ran(
                        "s1",
                        ItemKind::Search,
                        "rg -n \"drop_preview\" crates/zz-ui/src/browser",
                    )
                    .ran("r1", ItemKind::Read, "crates/zz-ui/src/browser/drag.rs")
                    .ran("e1", ItemKind::Edit, "crates/zz-ui/src/browser/drag.rs")
                    .ran(
                        "x1",
                        ItemKind::Execute,
                        "cargo test -p zz-ui browser::drag",
                    )
                    .reply("a1", CODEX_ANSWER)
                    .finish(completed());
            }
            Self::ClaudeAsks => {
                script
                    .turn("Tests are green. Push the branch and open a pull request.")
                    .ran("x1", ItemKind::Execute, "git status --short")
                    .tool(
                        "x2",
                        None,
                        ItemKind::Execute,
                        "git push -u origin fix/pinned-tab-width",
                    )
                    .ask(
                        1,
                        Some("x2"),
                        lens_approval("git push -u origin fix/pinned-tab-width"),
                    );
            }
            Self::ClaudePlans => {
                script
                    .turn("Plan how pinned tabs keep their width, then wait for me.")
                    .ran(
                        "s1",
                        ItemKind::Search,
                        "rg -n \"pinned\" crates/zz-ui/src/browser",
                    )
                    .ran(
                        "r1",
                        ItemKind::Read,
                        "crates/zz-ui/src/browser/tab_strip.rs",
                    )
                    .ran("r2", ItemKind::Read, "crates/zz-ui/src/browser/drag.rs")
                    .ask(
                        1,
                        None,
                        terminal_ask("Waiting for a plan approval in the terminal"),
                    );
            }
            Self::CodexFirst => {}
        }
        script
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Look {
    Page(f32),
    Tiled,
    Floating,
    Phone,
}

fn pane_frame(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .relative()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(theme.radius)
        .framed(cx)
        .bg(pane_color(theme, theme.background))
}

pub struct LensPane {
    scene: Scene,
    look: Look,
    open: bool,
    thread: Entity<Thread>,
    input: Entity<InputState>,
    tray: Option<TrayPanel>,
    sheet: Option<SharedString>,
}

fn build(scene: Scene, look: Look, open: bool, window: &mut Window, cx: &mut App) -> AnyView {
    build_with(scene, look, open, None, window, cx)
}

fn build_with(
    scene: Scene,
    look: Look,
    open: bool,
    sheet: Option<&'static str>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let input = composer_input("", window, cx);
    input.update(cx, |input, cx| {
        input.set_placeholder(format!("Message {}…", scene.provider().label()), cx);
    });
    cx.new(|cx: &mut Context<LensPane>| {
        let lens = cx.weak_entity();
        let show: Show = Rc::new(move |_: &mut Window, cx: &mut App| {
            let _ = lens.update(cx, |lens, cx| {
                lens.sheet = Some(lens.scene.provider().label().into());
                cx.notify();
            });
        });
        let thread = Thread::new(scene.script())
            .framed(Frame::Scroll(None))
            .vendor(scene.icon())
            .terminal(show)
            .view(window, cx);
        LensPane {
            scene,
            look,
            open,
            thread,
            input,
            tray: None,
            sheet: sheet.map(SharedString::from),
        }
    })
    .into()
}

pub(crate) fn tiled(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Tiled, true, window, cx)
}

pub(crate) fn tiled_plans(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::ClaudePlans, Look::Tiled, true, window, cx)
}

pub(crate) fn tiled_first(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::CodexFirst, Look::Tiled, true, window, cx)
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
        if self.scene != Scene::Claude {
            return Vec::new();
        }
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

    fn lens(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefix = self.prefix(cx);
        let composer = composer(
            Draft {
                id: SharedString::from(format!("lens-composer-{}", self.scene.pane())),
                input: self.input.clone(),
                running: self.scene.running(),
                writable: true,
                images: Vec::new(),
                prefix,
                settings: match self.scene.provider() {
                    AgentProvider::Codex => codex_settings(),
                    AgentProvider::ClaudeCode => claude_settings(),
                },
            },
            cx,
        );
        let chat = self.thread.read(cx).shows_chat();
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(if chat {
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.thread.clone())
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_h_0()
                    .child(empty_column(lens_welcome(self.scene.provider(), cx)))
                    .into_any_element()
            })
            .child(composer)
            .when_some(self.sheet.clone(), |lens, title| {
                lens.child(terminal_pip(
                    ("lens-sheet", self.scene.pane()),
                    title,
                    self.scene.icon(),
                    terminal_body(self.scene.terminal(), false, cx).into_any_element(),
                    cx.theme().background,
                    move |_, cx| {
                        entity.update(cx, |view, cx| {
                            view.sheet = None;
                            cx.notify();
                        });
                    },
                    cx,
                ))
            })
            .into_any_element()
    }
}

impl Render for LensPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.open {
            self.lens(cx)
        } else {
            terminal_body(self.scene.terminal(), false, cx).into_any_element()
        };
        let id = ("lens-pane", self.scene.pane());
        if self.look == Look::Phone {
            return div()
                .id(id)
                .size_full()
                .flex()
                .flex_col()
                .child(body)
                .into_any_element();
        }
        let header = self.header(cx);
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

fn panes_of(cases: Vec<(&'static str, AnyView)>, cx: &mut App) -> AnyView {
    let views = cases
        .into_iter()
        .map(|(label, view)| (SharedString::from(label), view))
        .collect();
    cx.new(|_| Views(views)).into()
}

pub fn panes(window: &mut Window, cx: &mut App) -> AnyView {
    let cases = vec![
        (
            "Claude Code with the lens open: a turn running, the plan and a background task above the composer",
            build(Scene::Claude, Look::Page(640.0), true, window, cx),
        ),
        (
            "Claude Code at its permission prompt: the card sits on the tool row; Allow types 1, Deny presses Esc",
            build(Scene::ClaudeAsks, Look::Page(480.0), true, window, cx),
        ),
        (
            "Codex showing its terminal: the toggle opens the lens on the same session",
            build(Scene::Codex, Look::Page(360.0), false, window, cx),
        ),
        (
            "Codex before its first prompt: the welcome names it, and what you send is typed into its terminal",
            build(Scene::CodexFirst, Look::Page(360.0), true, window, cx),
        ),
    ];
    panes_of(cases, cx)
}

pub fn sheets(window: &mut Window, cx: &mut App) -> AnyView {
    let cases = vec![
        (
            "Claude Code running /config: its own dialog opens in a wide PiP over the chat, keys go to it, and the PiP closes when Claude's input box comes back, or on Done",
            build_with(
                Scene::Claude,
                Look::Page(560.0),
                true,
                Some("/config · Claude Code"),
                window,
                cx,
            ),
        ),
        (
            "Claude Code at a plan approval the lens cannot answer: Show terminal on the card opens the dialog in the PiP",
            build_with(
                Scene::ClaudePlans,
                Look::Page(520.0),
                true,
                Some("Claude Code"),
                window,
                cx,
            ),
        ),
        (
            "Codex showing its usage: commands only its own screen draws (/usage, /skills, /side) open in the PiP; Esc or Done closes it",
            build_with(
                Scene::Codex,
                Look::Page(480.0),
                true,
                Some("/usage · Codex"),
                window,
                cx,
            ),
        ),
    ];
    panes_of(cases, cx)
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
        label: "inactive pane: the toggle and the actions wait for the pointer",
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
