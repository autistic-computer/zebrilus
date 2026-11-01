use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{ActiveTheme as _, input::InputState};
use zz_client::agent_flip::{AgentFlips, Flip, FlipMode};
use zz_protocol::chat::{ItemKind, ItemStatus};
use zz_protocol::{AgentProvider, PaneId};
use zz_ui::{
    agent::{
        presentation::{agent_welcome, spinner_phase},
        tasks::{TaskTrayAction, TrayPanel, task_tray},
    },
    pane::{
        FlipChoice, FlipTo, PaneDragSource, PaneFlip, PaneOverlayCorner, TerminalPaneAction,
        agent_provider_icon, pane_flip_overlay, pane_header, pane_overlay_stack,
    },
    pane_look::pane_color,
    pip::PipSurface,
};

use super::{
    chat::{Frame, Thread, Views, empty_column},
    composer::{Draft, claude_settings, codex_settings, composer, composer_input},
    fixtures::{PLAN, PLAN_STEPS, Script, approval_of, completed, plan_approval, tasks},
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

    const fn title(self, side: Side) -> &'static str {
        match (self, side) {
            (Self::Claude, _) => "Pinned tabs keep their icon width",
            (Self::ClaudeAsks, _) => "Open the pinned tabs pull request",
            (Self::ClaudePlans, _) => "Plan the pinned tabs fix",
            (Self::Codex, _) => "Fix the tab drop preview",
            (Self::CodexFirst, Side::Chat) => "agent",
            (Self::CodexFirst, Side::Cli) => "codex",
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

    const fn busy(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("a turn and 1 background task"),
            Self::ClaudeAsks | Self::ClaudePlans => Some("a turn"),
            Self::Codex | Self::CodexFirst => None,
        }
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
                        approval_of("git push -u origin fix/pinned-tab-width"),
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
                    .ask(1, None, plan_approval());
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Chat,
    Cli,
}

impl Side {
    const fn flip_to(self) -> FlipTo {
        match self {
            Self::Chat => FlipTo::Cli,
            Self::Cli => FlipTo::Chat,
        }
    }

    const fn flipped(self) -> Self {
        match self {
            Self::Chat => Self::Cli,
            Self::Cli => Self::Chat,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Start {
    Idle,
    Busy,
    Armed,
}

pub struct AgentPaneStory {
    scene: Scene,
    look: Look,
    side: Side,
    flips: AgentFlips,
    next_request: u64,
    armed: bool,
    thread: Entity<Thread>,
    input: Entity<InputState>,
    tray: Option<TrayPanel>,
}

fn build(scene: Scene, look: Look, side: Side, window: &mut Window, cx: &mut App) -> AnyView {
    build_with(scene, look, side, Start::Idle, window, cx)
}

fn build_with(
    scene: Scene,
    look: Look,
    side: Side,
    start: Start,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let input = composer_input("", window, cx);
    input.update(cx, |input, cx| {
        input.set_placeholder(format!("Message {}…", scene.provider().label()), cx);
    });
    cx.new(|cx: &mut Context<AgentPaneStory>| {
        let thread = Thread::new(scene.script())
            .framed(Frame::Scroll(None))
            .vendor(scene.icon())
            .view(window, cx);
        let mut story = AgentPaneStory {
            scene,
            look,
            side,
            flips: AgentFlips::default(),
            next_request: 1,
            armed: false,
            thread,
            input,
            tray: None,
        };
        if start != Start::Idle {
            story.flip();
        }
        if start == Start::Armed {
            story.choose(FlipChoice::WhenIdle);
        }
        story
    })
    .into()
}

pub(crate) fn tiled(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Tiled, Side::Chat, window, cx)
}

pub(crate) fn tiled_plans(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::ClaudePlans, Look::Tiled, Side::Chat, window, cx)
}

pub(crate) fn tiled_first(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::CodexFirst, Look::Tiled, Side::Chat, window, cx)
}

pub(crate) fn floating_claude(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Floating, Side::Chat, window, cx)
}

pub(crate) fn floating_codex(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Codex, Look::Floating, Side::Cli, window, cx)
}

pub(crate) fn phone_claude(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Claude, Look::Phone, Side::Chat, window, cx)
}

pub(crate) fn phone_claude_asks(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::ClaudeAsks, Look::Phone, Side::Chat, window, cx)
}

pub(crate) fn phone_codex(window: &mut Window, cx: &mut App) -> AnyView {
    build(Scene::Codex, Look::Phone, Side::Cli, window, cx)
}

pub(crate) fn phone_claude_busy(window: &mut Window, cx: &mut App) -> AnyView {
    build_with(
        Scene::Claude,
        Look::Phone,
        Side::Chat,
        Start::Busy,
        window,
        cx,
    )
}

impl AgentPaneStory {
    const fn pane(&self) -> PaneId {
        PaneId(self.scene.pane())
    }

    fn send(&mut self) -> impl FnOnce(zz_protocol::CommandInvocation) -> Option<u64> + use<> {
        let request = self.next_request;
        self.next_request += 1;
        move |_| Some(request)
    }

    fn answer(&mut self) {
        let request = self.next_request - 1;
        let pane = self.pane();
        let Flip::Sending(mode) = *self.flips.state(pane) else {
            return;
        };
        let reply = match mode {
            FlipMode::Ask => self.scene.busy().map(|busy| format!("busy: {busy}")),
            _ => None,
        };
        let flips_now = matches!(mode, FlipMode::Ask | FlipMode::Now) && reply.is_none();
        match mode {
            FlipMode::WhenIdle => self.armed = true,
            FlipMode::Cancel => self.armed = false,
            FlipMode::Ask | FlipMode::Now => {}
        }
        if flips_now {
            self.side = self.side.flipped();
            self.armed = false;
        }
        self.flips
            .observe_panes([(pane, self.side.flip_to(), self.armed)]);
        self.flips
            .settle(request, reply.as_deref().map_or(Ok(()), Err));
    }

    fn flip(&mut self) {
        let pane = self.pane();
        let send = self.send();
        if self.flips.press(pane, self.side.flip_to(), send) {
            self.answer();
        }
    }

    fn choose(&mut self, choice: FlipChoice) {
        let pane = self.pane();
        let Some(mode) = choice.mode() else {
            self.flips.dismiss(pane);
            return;
        };
        let send = self.send();
        if self.flips.choose(pane, mode, send) {
            self.answer();
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let title = self.scene.title(self.side);
        let flip = self.flips.state(self.pane());
        pane_header(
            match self.side {
                Side::Chat => agent_provider_icon(self.scene.provider()),
                Side::Cli => penna::IconName::SquareTerminal,
            },
            true,
            title,
            PaneDragSource::new(self.pane(), title, true, |_, _, _| {}),
            self.look == Look::Floating,
            Some(PaneFlip {
                provider: self.scene.provider(),
                to: self.side.flip_to(),
                armed: flip.armed(),
                pending: flip.pending(),
            }),
            move |action, _, cx| {
                if action == TerminalPaneAction::Flip {
                    entity.update(cx, |view, cx| {
                        view.flip();
                        cx.notify();
                    });
                }
            },
            cx,
        )
        .into_any_element()
    }

    fn overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let entity = cx.entity();
        pane_flip_overlay(
            self.pane(),
            self.side.flip_to(),
            self.flips.state(self.pane()),
            move |choice, _, cx| {
                entity.update(cx, |view, cx| {
                    view.choose(choice);
                    cx.notify();
                });
            },
            cx,
        )
        .map(|overlay| {
            pane_overlay_stack(PaneOverlayCorner::TopRight, [overlay]).into_any_element()
        })
    }

    fn prefix(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.scene != Scene::Claude || self.side != Side::Chat {
            return Vec::new();
        }
        let phase = spinner_phase(cx.entity_id(), cx);
        let entity = cx.entity();
        let tasks = tasks().into_iter().take(1).collect::<Vec<_>>();
        task_tray(
            "agent-pane-tray",
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

    fn chat(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefix = self.prefix(cx);
        let composer = composer(
            Draft {
                id: SharedString::from(format!("agent-pane-composer-{}", self.scene.pane())),
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
                    .child(empty_column(agent_welcome(self.scene.provider(), cx)))
                    .into_any_element()
            })
            .child(composer)
            .into_any_element()
    }
}

impl Render for AgentPaneStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.side {
            Side::Chat => self.chat(cx),
            Side::Cli => terminal_body(self.scene.terminal(), false, cx).into_any_element(),
        };
        let overlay = self.overlay(cx);
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(body)
            .children(overlay)
            .into_any_element();
        let id = ("agent-pane", self.scene.pane());
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
            "Claude Code as a chat: a turn running, the plan and a background task above the composer. The flip button switches to the CLI; with work running it asks first",
            build(Scene::Claude, Look::Page(640.0), Side::Chat, window, cx),
        ),
        (
            "Claude Code at its permission prompt: the card sits on the tool row, and the answer goes to the CLI",
            build(Scene::ClaudeAsks, Look::Page(480.0), Side::Chat, window, cx),
        ),
        (
            "Codex in a terminal: the flip button turns the pane into a chat on the same session, since nothing is running",
            build(Scene::Codex, Look::Page(360.0), Side::Cli, window, cx),
        ),
        (
            "a new Codex chat before its first prompt",
            build(Scene::CodexFirst, Look::Page(360.0), Side::Chat, window, cx),
        ),
    ];
    panes_of(cases, cx)
}

pub fn choices(window: &mut Window, cx: &mut App) -> AnyView {
    let cases = vec![
        (
            "the flip button pressed during a turn: the pane says what switching now stops. Switch when idle arms the flip, Switch now stops the work and switches, and the X keeps the pane as it is",
            build_with(
                Scene::Claude,
                Look::Page(560.0),
                Side::Chat,
                Start::Busy,
                window,
                cx,
            ),
        ),
        (
            "an armed flip: the button stays lit and the pane says it switches when idle. Cancel, or the button again, disarms it",
            build_with(
                Scene::Claude,
                Look::Page(560.0),
                Side::Chat,
                Start::Armed,
                window,
                cx,
            ),
        ),
        (
            "the same choice on a terminal running Claude Code at its own prompt",
            build_with(
                Scene::ClaudeAsks,
                Look::Page(360.0),
                Side::Cli,
                Start::Busy,
                window,
                cx,
            ),
        ),
    ];
    panes_of(cases, cx)
}

struct FlipCase {
    label: &'static str,
    provider: AgentProvider,
    title: &'static str,
    to: FlipTo,
    armed: bool,
    pending: bool,
    active: bool,
    floats: bool,
    width: Option<f32>,
}

const FLIP: FlipCase = FlipCase {
    label: "",
    provider: AgentProvider::ClaudeCode,
    title: "Pinned tabs keep their icon width",
    to: FlipTo::Chat,
    armed: false,
    pending: false,
    active: true,
    floats: false,
    width: None,
};

const FLIPS: [FlipCase; 8] = [
    FlipCase {
        label: "a terminal running Claude Code: Switch to chat",
        ..FLIP
    },
    FlipCase {
        label: "a Claude Code chat: Switch to the CLI",
        to: FlipTo::Cli,
        ..FLIP
    },
    FlipCase {
        label: "a terminal running Codex",
        provider: AgentProvider::Codex,
        title: "codex",
        ..FLIP
    },
    FlipCase {
        label: "armed: lit until the flip runs or is cancelled",
        to: FlipTo::Cli,
        armed: true,
        ..FLIP
    },
    FlipCase {
        label: "waiting for the daemon's answer",
        pending: true,
        ..FLIP
    },
    FlipCase {
        label: "inactive pane: the button and the actions wait for the pointer",
        active: false,
        ..FLIP
    },
    FlipCase {
        label: "floating: Tile back and Zoom take the place of the splits",
        to: FlipTo::Cli,
        floats: true,
        ..FLIP
    },
    FlipCase {
        label: "narrow pane, long title, a chat",
        title: "Investigate why the browser pane drops frames on the secondary display at 120 Hz",
        to: FlipTo::Cli,
        width: Some(380.0),
        ..FLIP
    },
];

pub fn flip_buttons(_: &mut Window, cx: &mut App) -> AnyView {
    crate::story::stateless(render_flip_buttons, cx)
}

fn render_flip_buttons(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    FLIPS
        .iter()
        .enumerate()
        .fold(states(), |states, (index, case)| {
            let header = pane_header(
                match case.to {
                    FlipTo::Chat => penna::IconName::SquareTerminal,
                    FlipTo::Cli => agent_provider_icon(case.provider),
                },
                case.active,
                case.title,
                PaneDragSource::new(PaneId(index as u64 + 1), case.title, true, |_, _, _| {}),
                case.floats,
                Some(PaneFlip {
                    provider: case.provider,
                    to: case.to,
                    armed: case.armed,
                    pending: case.pending,
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
                    .id(("flip-button", index))
                    .when_some(case.width, |frame, width| frame.w(px(width)))
                    .child(div().opacity(opacity).child(header))
                    .child(div().h(px(20.0))),
            )
        })
        .into_any_element()
}
