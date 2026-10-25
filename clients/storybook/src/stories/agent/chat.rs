use std::rc::Rc;

use zz_client::program::Program;
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, IntoElement, MouseButton,
    ParentElement as _, Render, ScrollHandle, Styled as _, Window, div, prelude::*, px,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    ActiveTheme as _, Ground, PaintState,
    agent::{
        chat::{
            ChatBubble, ChatComposer, ChatSender, ChatTyping, DRIVER_PALETTE_HINTS, Receipt,
            chat_button, chat_header, chat_latest, chat_seen, chat_thread, driver_palette_body,
        },
        controls::{composer_action, composer_action_button},
    },
    command::{CommandPaletteSurface, PalettePill, PaletteStatus, unified_command_palette_input},
    input::InputState,
    navigation::{
        WORKSPACE_SIDEBAR_DEFAULT_WIDTH, WORKSPACE_SIDEBAR_FADE_HEIGHT,
        WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER,
        flat::{FlatHandler, FlatMenu, FlatState, flat_sidebar},
        workspace_chat_button, workspace_sidebar_fade,
    },
    pane::{PaneSplitAxis, pane_split_surface},
    roles,
    scroll::ScrollableElement as _,
    shell::{app_shell_surface, app_workspace_surface},
};

use super::cards::permission;
use crate::stories::workspace::{
    fixtures::{CLAUDE, CODEX, TerminalPane, gap, stateful},
    flat_sidebar::sidebar_hosts,
    status_bar::{Bar, titlebar},
    tree::flat_view_sidebar,
};
use crate::story::{Framed as _, Section, Story, stateless, states};

/// Wide enough for a two-line answer; the sidebar opens at 256px.
const CHAT_WIDTH: f32 = 340.0;
const WINDOW_HEIGHT: f32 = 640.0;
const PALETTE_TOP: f32 = 72.0;
const PLACEHOLDER: &str = "Message the driver…";

pub const STORY: Story = Story {
    id: "driver-chat",
    name: "Driver chat",
    group: "Agent",
    summary: "The chat with the Driver Agent lives in the command palette's Driver mode: type > in an empty palette, click the title bar's chat button, or click the driver's last message at the bottom of the sidebar. Your messages sit right in the accent's wash with read ticks; the driver's answers and the agents' messages sit left, an agent's under its sidebar tile and name. Agents at work show as sidebar rows, so the chat carries no tool calls. The driver's model and effort are settings, under Agents.",
    sections: &[
        Section {
            id: "messages",
            name: "Messages",
            summary: "ChatBubble on the sidebar's ground. One tick: zz has it. Two: typed into the driver's pane. Two in the accent: the driver started a turn on it. The time and ticks keep the bubble's corner; a last line that would run into them wraps first. While the driver works, ChatTyping takes the place of its answer: three dots that rise in turn, and its latest steps in the words its CLI gives them (a Bash call's description, the plan item in progress), the newest beside the dots.",
            build: |_, cx| stateless(messages, cx),
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "ChatComposer: one line and the round send button, which queues behind a running turn like the lens composer's. Click it to focus.",
            build: |window, cx| stateful(window, cx, Composers::new, composers),
        },
        Section {
            id: "header",
            name: "Header",
            summary: "chat_header names the driver and the host it runs on.",
            build: |_, cx| stateless(headers, cx),
        },
        Section {
            id: "latest",
            name: "Last message",
            summary: "chat_latest: what the sidebar keeps of the chat. The driver's header, then its last answer cut to four lines, or ChatTyping while it works. A dot in the header while an approval waits on you. A click opens the command palette in Driver mode.",
            build: |_, cx| stateless(latest, cx),
        },
        Section {
            id: "sidebar",
            name: "In the sidebar",
            summary: "The last message under the flat list, over the footer's buttons. The panes on the right are the agents' own terminals.",
            build: |_, cx| cx.new(|_| SidebarChat::new()).into(),
        },
        Section {
            id: "titlebar",
            name: "In titlebar mode",
            summary: "With the sidebar hidden, the title bar's chat button opens the command palette in Driver mode: the Driver pill in the input, the thread under it with the same fade, an approval waiting on you, and Driver mode's keys in the footer. In either layout, > typed into an empty palette switches to Driver mode too. Click the scrim to close it and the button to open it again; the button carries a dot while something waits.",
            build: |window, cx| cx.new(|cx| TitlebarChat::new(window, cx)).into(),
        },
    ],
};

pub(crate) fn driver_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let value = value.to_owned();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(PLACEHOLDER)
            .default_value(value)
    })
}

pub(crate) fn bottom_scroll() -> ScrollHandle {
    let scroll = ScrollHandle::new();
    scroll.scroll_to_bottom();
    scroll
}

fn sender(name: &'static str, program: &str) -> ChatSender {
    ChatSender {
        name: name.into(),
        program: Program::by_id(program),
    }
}

fn driver() -> ChatSender {
    sender("Driver", "claude")
}

fn agentbound() -> ChatSender {
    sender("agentbound", "claude")
}

fn clairvo() -> ChatSender {
    sender("clairvo", "codex")
}

fn conversation(scope: &'static str, ground: Ground, cx: &App) -> Vec<AnyElement> {
    let id = |n: usize| (scope, n);
    vec![
        ChatBubble::mine(id(0), "morning. what's everyone doing?", "9:40")
            .receipt(Receipt::Read)
            .on(ground)
            .into_any_element(),
        ChatBubble::theirs(
            id(1),
            "Three agents are working. zz on alienware needs you, its question is open on the right.",
            "9:40",
        )
        .on(ground)
        .into_any_element(),
        ChatBubble::theirs(id(2), "Sprite sheet is half done. 16 or 32px tiles?", "9:41")
            .sender(agentbound())
            .on(ground)
            .into_any_element(),
        ChatBubble::mine(id(3), "32px", "9:42")
            .receipt(Receipt::Read)
            .on(ground)
            .into_any_element(),
        chat_seen(&[agentbound()], cx),
        ChatBubble::theirs(id(4), "PR is up. Merging once the last 2 checks pass.", "9:43")
            .sender(clairvo())
            .on(ground)
            .into_any_element(),
        ChatBubble::mine(id(5), "commit the sprite sheet", "9:44")
            .receipt(Receipt::Read)
            .on(ground)
            .into_any_element(),
        ChatTyping::new(id(6))
            .steps(COMMIT_STEPS)
            .on(ground)
            .into_any_element(),
    ]
}

const COMMIT_STEPS: [&str; 4] = [
    "Committing the sprite sheet",
    "Show dirty files",
    "Reading atlas.rs",
    "Write the commit message",
];

fn column(content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .w_full()
        .max_w(px(CHAT_WIDTH))
        .bg(cx.theme().background)
        .rounded(cx.theme().radius)
        .framed(cx)
        .child(content)
        .into_any_element()
}

fn messages(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "your messages: read, delivered, sent",
            column(
                chat_thread([
                    ChatBubble::mine("read", "read", "9:40")
                        .receipt(Receipt::Read)
                        .into_any_element(),
                    ChatBubble::mine("delivered", "delivered", "9:41")
                        .receipt(Receipt::Delivered)
                        .into_any_element(),
                    ChatBubble::mine("sent", "sent", "9:41")
                        .receipt(Receipt::Sent)
                        .into_any_element(),
                ]),
                cx,
            ),
        )
        .state(
            "the driver's answer",
            column(
                chat_thread([ChatBubble::theirs(
                    "answer",
                    "Three agents are working. zz needs you for one thing.",
                    "9:40",
                )
                .into_any_element()]),
                cx,
            ),
        )
        .state(
            "an agent's message, seen by the agent you answered",
            column(
                chat_thread([
                    ChatBubble::theirs(
                        "agent",
                        "Sprite sheet is half done. 16 or 32px tiles?",
                        "9:41",
                    )
                    .sender(agentbound())
                    .into_any_element(),
                    ChatBubble::mine("answer-agent", "32px", "9:42")
                        .receipt(Receipt::Read)
                        .into_any_element(),
                    chat_seen(&[agentbound()], cx),
                ]),
                cx,
            ),
        )
        .state(
            "the driver at work, before its first step",
            column(
                chat_thread([
                    ChatBubble::mine("typing-sent", "commit this", "9:44")
                        .receipt(Receipt::Read)
                        .into_any_element(),
                    ChatTyping::new("typing-dots").into_any_element(),
                ]),
                cx,
            ),
        )
        .state(
            "the driver at work: its last three steps, the newest beside the dots",
            column(
                chat_thread([ChatTyping::new("typing-steps")
                    .steps(COMMIT_STEPS)
                    .into_any_element()]),
                cx,
            ),
        )
        .state(
            "a long step is cut to one line",
            column(
                chat_thread([ChatTyping::new("typing-long")
                    .steps([
                        "Run the whole browser suite with the GPU tests on lavapipe and report the failures",
                    ])
                    .into_any_element()]),
                cx,
            ),
        )
        .state(
            "long lines wrap before the time",
            column(
                chat_thread([
                    ChatBubble::mine(
                        "long-mine",
                        "check the atlas failure on alienware first, then the A2P retry; skip the sprite sheet until tomorrow",
                        "10:02",
                    )
                    .receipt(Receipt::Delivered)
                    .into_any_element(),
                    ChatBubble::theirs(
                        "long-theirs",
                        "On it. Atlas first: EGL picked the NVIDIA vendor, so I asked the worker to retry on lavapipe.",
                        "10:03",
                    )
                    .into_any_element(),
                ]),
                cx,
            ),
        )
        .into_any_element()
}

fn composer(
    scope: &'static str,
    input: &Entity<InputState>,
    running: bool,
    ground: Option<Ground>,
    cx: &App,
) -> ChatComposer {
    let has_content = !input.read(cx).value().trim().is_empty();
    ChatComposer {
        input: input.clone(),
        action: composer_action_button(
            (scope, 0_usize),
            composer_action(running, has_content),
            running || has_content,
        )
        .into_any_element(),
        ground,
    }
}

pub(crate) struct Composers {
    empty: Entity<InputState>,
    typed: Entity<InputState>,
}

impl Composers {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            empty: driver_input("", window, cx),
            typed: driver_input("ship it once checks pass", window, cx),
        }
    }
}

fn composers(state: &Composers, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "empty",
            column(
                div()
                    .pt(px(10.0))
                    .child(composer("empty", &state.empty, false, None, cx)),
                cx,
            ),
        )
        .state(
            "typed while the driver works: queues",
            column(
                div()
                    .pt(px(10.0))
                    .child(composer("typed", &state.typed, true, None, cx)),
                cx,
            ),
        )
        .into_any_element()
}

fn headers(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "in the sidebar",
            column(chat_header(&driver(), "xps", None, cx), cx),
        )
        .into_any_element()
}

fn list(scroll: &ScrollHandle, fade: zz_gpui::Hsla, cx: &App) -> AnyElement {
    let mut hosts = sidebar_hosts();
    if let Some(first) = hosts.first_mut() {
        first.folded = true;
    }
    let handler: FlatHandler = Rc::new(|_, _, _| {});
    let menu: FlatMenu = Rc::new(|_, _, menu, _, _| menu);
    div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .child(
            div()
                .id("driver-sidebar-list")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .pl(WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER)
                .child(flat_sidebar(
                    &hosts,
                    FlatState::default(),
                    &handler,
                    &menu,
                    cx.theme().ground(),
                    cx,
                )),
        )
        .bottom_fade(scroll, fade, WORKSPACE_SIDEBAR_FADE_HEIGHT)
        .into_any_element()
}

fn panes(scope: &'static str, cx: &App) -> AnyElement {
    let gaps = crate::pane().gaps;
    let claude = TerminalPane::new(1, "Atlas tests", CLAUDE)
        .active(true)
        .gaps(gaps)
        .lens(AgentProvider::ClaudeCode)
        .render((scope, 1_usize), Vec::new(), cx);
    let codex = TerminalPane::new(2, "A2P retry on 429", CODEX)
        .gaps(gaps)
        .lens(AgentProvider::Codex)
        .render((scope, 2_usize), Vec::new(), cx);
    div()
        .id((scope, 3_usize))
        .size_full()
        .p(gap(gaps))
        .child(pane_split_surface(
            (scope, 4_usize),
            PaneSplitAxis::Horizontal,
            0.5,
            false,
            gaps,
            gap(gaps),
            None,
            None,
            claude,
            codex,
            div().absolute(),
            cx,
        ))
        .into_any_element()
}

fn window_frame(content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .w_full()
        .h(px(WINDOW_HEIGHT))
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .child(content)
        .into_any_element()
}

const LATEST_ANSWER: &str = "Three agents are working. zz on alienware needs you: it asks whether to keep the old socket path. agentbound finished the sprite sheet at 32px, and clairvo merges its PR once the last two checks pass.";
/// How many lines of the driver's last answer the sidebar shows, as the app.
const LATEST_LINES: usize = 4;
const ATTENTION_DOT: f32 = 7.0;

/// The driver's last word as the sidebar shows it under its list: its last
/// answer cut to four lines, or the driver at work, with a dot in the header
/// while something waits on you.
pub(crate) fn sidebar_latest(
    scope: &'static str,
    working: bool,
    waiting: bool,
    cx: &App,
) -> AnyElement {
    let ground = cx.theme().ground();
    let message = if working {
        ChatTyping::new((scope, 0_usize))
            .steps(COMMIT_STEPS)
            .on(ground)
            .into_any_element()
    } else {
        ChatBubble::theirs((scope, 1_usize), LATEST_ANSWER, "")
            .lines(LATEST_LINES)
            .on(ground)
            .into_any_element()
    };
    let dot = waiting.then(|| {
        div()
            .size(px(ATTENTION_DOT))
            .rounded_full()
            .bg(cx.theme().warning)
            .into_any_element()
    });
    chat_latest(
        (scope, 2_usize),
        chat_header(&driver(), "xps", dot, cx),
        message,
        cx,
    )
    .into_any_element()
}

/// `list` over the driver's last message, as the sidebar holds them.
pub(crate) fn with_driver(id: &'static str, list: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .size_full()
        .min_h(px(0.0))
        .child(div().flex().flex_col().flex_1().min_h(px(0.0)).child(list))
        .child(sidebar_latest(id, false, false, cx))
        .into_any_element()
}

fn latest(_: &mut Window, cx: &mut App) -> AnyElement {
    let frame = |content: AnyElement| {
        div()
            .w(px(WORKSPACE_SIDEBAR_DEFAULT_WIDTH))
            .pt(px(10.0))
            .bg(cx.theme().background)
            .rounded(cx.theme().radius)
            .framed(cx)
            .child(content)
            .into_any_element()
    };
    states()
        .columns(3)
        .state(
            "its last answer",
            frame(sidebar_latest("latest-answer", false, false, cx)),
        )
        .state(
            "at work",
            frame(sidebar_latest("latest-working", true, false, cx)),
        )
        .state(
            "waiting on you",
            frame(sidebar_latest("latest-waiting", false, true, cx)),
        )
        .into_any_element()
}

pub(crate) struct SidebarChat {
    list: ScrollHandle,
}

impl SidebarChat {
    fn new() -> Self {
        Self {
            list: ScrollHandle::new(),
        }
    }
}

impl Render for SidebarChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fade = workspace_sidebar_fade(cx.theme().ground(), cx);
        window_frame(
            app_shell_surface(
                "driver-sidebar-shell",
                cx.theme().background,
                flat_view_sidebar(
                    with_driver("driver-sidebar-column", list(&self.list, fade, cx), cx),
                    cx,
                ),
                None,
                app_workspace_surface(
                    "driver-sidebar-workspace",
                    panes("driver-sidebar-panes", cx),
                    Vec::new(),
                    cx,
                ),
                Vec::new(),
            ),
            cx,
        )
    }
}

pub(crate) struct TitlebarChat {
    input: Entity<InputState>,
    thread: ScrollHandle,
    open: bool,
}

impl TitlebarChat {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            input: driver_input("", window, cx),
            thread: bottom_scroll(),
            open: true,
        }
    }

    fn palette(&self, cx: &App) -> AnyElement {
        driver_palette(&self.input, &self.thread, cx)
    }
}

/// The command palette in Driver mode: the Driver pill in the input, the thread
/// under it and an approval waiting on you.
pub(crate) fn driver_palette(
    input: &Entity<InputState>,
    thread: &ScrollHandle,
    cx: &App,
) -> AnyElement {
    let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
    let pills = [PalettePill::Host {
        label: "xps".into(),
        status: PaletteStatus::Online,
    }]
    .into_iter()
    .chain(
        Program::by_id("claude").map(|program| PalettePill::Program {
            label: "Driver".into(),
            program,
        }),
    );
    let ask = permission(
        "driver-ask",
        "Atlas tests wants to run sudo pacman -S vulkan-swrast",
        None,
        &[("Yes", Some(true)), ("No, tell it what to do", Some(false))],
        0,
        true,
        cx,
    );
    CommandPaletteSurface::new(unified_command_palette_input(input, pills, cx), 1)
        .rows(driver_palette_body(
            chat_thread(conversation("driver-palette", layer.ground, cx)),
            Some(ask),
            thread,
            layer.ground.fill,
        ))
        .hints(DRIVER_PALETTE_HINTS)
        .into_any_element()
}

impl Render for TitlebarChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let waiting = (!self.open).then_some(cx.theme().warning);
        let button = workspace_chat_button("driver-chat-button").on_click(cx.listener(
            |this, _, window, cx| {
                this.open = true;
                this.input.read(cx).focus_handle(cx).focus(window, cx);
                cx.notify();
            },
        ));
        let bar = titlebar(
            &Bar {
                windows: 2,
                ..Bar::FULL
            },
            cx,
        )
        .child(chat_button(button, waiting));
        let overlay = self.open.then(|| {
            div()
                .id("driver-palette-scrim")
                .absolute()
                .inset_0()
                .bg(cx.theme().scrim)
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(PALETTE_TOP))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .px(px(16.0))
                        .child(self.palette(cx)),
                )
                .into_any_element()
        });
        window_frame(
            app_shell_surface(
                "driver-titlebar-shell",
                cx.theme().background,
                div(),
                Some(bar.into_any_element()),
                app_workspace_surface(
                    "driver-titlebar-workspace",
                    panes("driver-titlebar-panes", cx),
                    Vec::new(),
                    cx,
                ),
                overlay,
            ),
            cx,
        )
    }
}
