use std::rc::Rc;

use zz_client::program::Program;
use zz_gpui::{
    AnyElement, App, AppContext as _, Entity, IntoElement, Keystroke, ParentElement as _,
    ScrollHandle, Styled as _, Window, div, prelude::*, px, relative,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    ActiveTheme as _,
    agent::{
        chat::{
            ChatBubble, ChatComposer, ChatSender, Receipt, chat_button, chat_header, chat_section,
            chat_seen, chat_thread,
        },
        controls::{composer_action, composer_action_button},
    },
    input::InputState,
    kbd::Kbd,
    navigation::{
        WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER,
        flat::{FlatHandler, FlatMenu, FlatState, flat_sidebar},
        workspace_chat_button, workspace_sidebar_ground, workspace_sidebar_panel,
        workspace_sidebar_slideover, workspace_sidebar_titlebar,
    },
    pane::{PaneSplitAxis, pane_split_surface},
    shell::{app_shell_surface, app_workspace_surface},
};

use crate::stories::workspace::{
    fixtures::{CLAUDE, CODEX, TerminalPane, gap, stateful},
    flat_sidebar::sidebar_hosts,
    status_bar::{Bar, titlebar},
    tree::chrome_controls,
};
use crate::story::{Framed as _, Section, Story, stateless, states};

/// Wide enough for a two-line answer; the sidebar opens at 256px.
const CHAT_WIDTH: f32 = 340.0;
const WINDOW_HEIGHT: f32 = 640.0;
const PLACEHOLDER: &str = "Message the lead…";

pub const STORY: Story = Story {
    id: "lead-chat",
    name: "Lead chat",
    group: "Agent",
    summary: "The chat with the lead agent, at the bottom of the sidebar or in its slideover in titlebar mode. Your messages sit right in the accent's wash with read ticks; the lead's answers and the agents' messages sit left, an agent's under its sidebar tile and name. Agents at work show as sidebar rows above, so the chat carries no tool calls, and the composer is one line: the lead's model is a setting.",
    sections: &[
        Section {
            id: "messages",
            name: "Messages",
            summary: "ChatBubble on the sidebar's ground. One tick: zz has it. Two: typed into the lead's pane. Two in the accent: the lead started a turn on it. The time and ticks keep the bubble's corner; a last line that would run into them wraps first.",
            build: |_, cx| stateless(messages, cx),
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "ChatComposer: one line and the round send button, which queues behind a running turn like the lens composer's. Click it to focus.",
            build: |window, cx| stateful(window, cx, Chat::new, composers),
        },
        Section {
            id: "header",
            name: "Header",
            summary: "chat_header names the lead and the host it runs on. In the slideover it ends in the key that closes it.",
            build: |_, cx| stateless(headers, cx),
        },
        Section {
            id: "sidebar",
            name: "In the sidebar",
            summary: "chat_section under the flat list: who works and who needs you stays in the rows above, the conversation with the lead below, the panes on the right are the agents' own terminals.",
            build: |window, cx| stateful(window, cx, Chat::new, in_sidebar),
        },
        Section {
            id: "slideover",
            name: "In titlebar mode",
            summary: "With the sidebar hidden the title bar carries the chat's button, dotted in the color of what waits there; the sidebar slides over the panes with the chat at its bottom.",
            build: |window, cx| stateful(window, cx, Chat::new, in_slideover),
        },
    ],
};

pub(crate) struct Chat {
    inputs: [Entity<InputState>; 4],
    scrolls: [ScrollHandle; 2],
}

impl Chat {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let values = ["", "ship it once checks pass", "", ""];
        Self {
            inputs: values.map(|value| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(PLACEHOLDER)
                        .default_value(value)
                })
            }),
            scrolls: std::array::from_fn(|_| {
                let scroll = ScrollHandle::new();
                scroll.scroll_to_bottom();
                scroll
            }),
        }
    }
}

fn sender(name: &'static str, program: &str) -> ChatSender {
    ChatSender {
        name: name.into(),
        program: Program::by_id(program),
    }
}

fn lead() -> ChatSender {
    sender("Lead", "claude")
}

fn agentbound() -> ChatSender {
    sender("agentbound", "claude")
}

fn clairvo() -> ChatSender {
    sender("clairvo", "codex")
}

fn conversation(scope: &'static str, cx: &App) -> Vec<AnyElement> {
    let id = |n: usize| (scope, n);
    vec![
        ChatBubble::mine(id(0), "morning. what's everyone doing?", "9:40")
            .receipt(Receipt::Read)
            .into_any_element(),
        ChatBubble::theirs(
            id(1),
            "Three agents are working. zz on alienware needs you, its question is open on the right.",
            "9:40",
        )
        .into_any_element(),
        ChatBubble::theirs(id(2), "Sprite sheet is half done. 16 or 32px tiles?", "9:41")
            .sender(agentbound())
            .into_any_element(),
        ChatBubble::mine(id(3), "32px", "9:42")
            .receipt(Receipt::Read)
            .into_any_element(),
        chat_seen(&[agentbound()], cx),
        ChatBubble::theirs(id(4), "PR is up. Merging once the last 2 checks pass.", "9:43")
            .sender(clairvo())
            .into_any_element(),
    ]
}

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
            "the lead's answer",
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
                    ChatBubble::theirs("agent", "Sprite sheet is half done. 16 or 32px tiles?", "9:41")
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
        ground: None,
    }
}

fn composers(state: &Chat, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "empty",
            column(
                div()
                    .pt(px(10.0))
                    .child(composer("empty", &state.inputs[0], false, cx)),
                cx,
            ),
        )
        .state(
            "typed while the lead works: queues",
            column(
                div()
                    .pt(px(10.0))
                    .child(composer("typed", &state.inputs[1], true, cx)),
                cx,
            ),
        )
        .into_any_element()
}

fn escape() -> AnyElement {
    Keystroke::parse("escape").map_or_else(
        |_| div().into_any_element(),
        |stroke| Kbd::new(stroke).into_any_element(),
    )
}

fn headers(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "in the sidebar",
            column(chat_header(&lead(), "xps", None, cx), cx),
        )
        .state(
            "in the slideover",
            column(chat_header(&lead(), "xps", Some(escape()), cx), cx),
        )
        .into_any_element()
}

fn list(scope: &'static str, cx: &App) -> AnyElement {
    let mut hosts = sidebar_hosts();
    if let Some(first) = hosts.first_mut() {
        first.folded = true;
    }
    let handler: FlatHandler = Rc::new(|_, _, _| {});
    let menu: FlatMenu = Rc::new(|_, _, menu, _, _| menu);
    div()
        .id(scope)
        .flex_none()
        .max_h(relative(0.42))
        .overflow_y_scroll()
        .pl(WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER)
        .child(flat_sidebar(
            &hosts,
            FlatState::default(),
            &handler,
            &menu,
            cx.theme().ground(),
            cx,
        ))
        .into_any_element()
}

fn chat_sidebar(
    scope: &'static str,
    input: &Entity<InputState>,
    scroll: &ScrollHandle,
    slideover: bool,
    cx: &App,
) -> AnyElement {
    let ground = workspace_sidebar_ground(cx.theme().ground(), slideover, cx);
    let trailing = slideover.then(escape);
    workspace_sidebar_panel(
        scope,
        CHAT_WIDTH,
        workspace_sidebar_titlebar((scope, 0_usize), chrome_controls(), cx),
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(list(scope, cx))
            .child(chat_section(
                chat_header(&lead(), "xps", trailing, cx),
                chat_thread(conversation(scope, cx)),
                ChatComposer {
                    ground: Some(ground),
                    ..composer(scope, input, false, cx)
                },
                scroll,
                cx,
            )),
        ground,
        !slideover,
        cx,
    )
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

fn in_sidebar(state: &Chat, _: &mut Window, cx: &mut App) -> AnyElement {
    window_frame(
        app_shell_surface(
            "lead-sidebar-shell",
            cx.theme().background,
            chat_sidebar(
                "lead-sidebar",
                &state.inputs[2],
                &state.scrolls[0],
                false,
                cx,
            ),
            None,
            app_workspace_surface(
                "lead-sidebar-workspace",
                panes("lead-sidebar-panes", cx),
                Vec::new(),
                cx,
            ),
            Vec::new(),
        ),
        cx,
    )
}

fn in_slideover(state: &Chat, _: &mut Window, cx: &mut App) -> AnyElement {
    let bar = titlebar(
        &Bar {
            windows: 2,
            ..Bar::FULL
        },
        cx,
    )
    .child(chat_button(
        workspace_chat_button("lead-chat-button"),
        Some(cx.theme().warning),
    ));
    window_frame(
        app_shell_surface(
            "lead-slideover-shell",
            cx.theme().background,
            div(),
            Some(bar.into_any_element()),
            app_workspace_surface(
                "lead-slideover-workspace",
                panes("lead-slideover-panes", cx),
                Vec::new(),
                cx,
            ),
            [workspace_sidebar_slideover(
                "lead-slideover",
                chat_sidebar(
                    "lead-slideover-sidebar",
                    &state.inputs[3],
                    &state.scrolls[1],
                    true,
                    cx,
                ),
                |_, _, _| {},
                cx,
            )
            .into_any_element()],
        ),
        cx,
    )
}
