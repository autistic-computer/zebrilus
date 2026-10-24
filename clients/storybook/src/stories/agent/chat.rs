use std::rc::Rc;

use zz_client::program::Program;
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, Entity, Focusable as _, IntoElement,
    MouseButton, ParentElement as _, Pixels, Render, ScrollHandle, Styled as _, Window, div,
    prelude::*, px,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    ActiveTheme as _, Ground, PaintState,
    agent::{
        chat::{
            ChatBubble, ChatComposer, ChatSection, ChatSender, ChatSplitDrag, LEAD_PALETTE_HINTS,
            Receipt, chat_button, chat_header, chat_seen, chat_split_height, chat_thread,
            lead_palette_body,
        },
        controls::{composer_action, composer_action_button},
    },
    command::{CommandPaletteSurface, PalettePill, PaletteStatus, unified_command_palette_input},
    input::InputState,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT, WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER,
        flat::{FlatHandler, FlatMenu, FlatState, flat_sidebar},
        workspace_chat_button, workspace_sidebar_fade, workspace_sidebar_panel,
        workspace_sidebar_titlebar,
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
    tree::sidebar_top_controls,
};
use crate::story::{Framed as _, Section, Story, stateless, states};

/// Wide enough for a two-line answer; the sidebar opens at 256px.
const CHAT_WIDTH: f32 = 340.0;
const WINDOW_HEIGHT: f32 = 640.0;
const PALETTE_TOP: f32 = 72.0;
const PLACEHOLDER: &str = "Message the lead…";

pub const STORY: Story = Story {
    id: "lead-chat",
    name: "Lead chat",
    group: "Agent",
    summary: "The chat with the lead agent: at the bottom of the sidebar, and in titlebar mode in the command palette's Lead mode. Your messages sit right in the accent's wash with read ticks; the lead's answers and the agents' messages sit left, an agent's under its sidebar tile and name. Agents at work show as sidebar rows, so the chat carries no tool calls, and the composer is one line: the lead's model is a setting.",
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
            build: |window, cx| stateful(window, cx, Composers::new, composers),
        },
        Section {
            id: "header",
            name: "Header",
            summary: "chat_header names the lead and the host it runs on.",
            build: |_, cx| stateless(headers, cx),
        },
        Section {
            id: "sidebar",
            name: "In the sidebar",
            summary: "ChatSection under the flat list. Drag the rule over the chat to move the split; the list and the chat keep a minimum height. Scroll the thread up and it fades under the header. The panes on the right are the agents' own terminals.",
            build: |window, cx| cx.new(|cx| SidebarChat::new(window, cx)).into(),
        },
        Section {
            id: "titlebar",
            name: "In titlebar mode",
            summary: "With the sidebar hidden, the title bar's chat button opens the command palette in Lead mode: the Lead pill in the input, the thread under it with the same fade, an approval waiting on you, and Lead mode's keys in the footer. Click the scrim to close it and the button to open it again; the button carries a dot while something waits.",
            build: |window, cx| cx.new(|cx| TitlebarChat::new(window, cx)).into(),
        },
    ],
};

pub(crate) fn lead_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
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

fn lead() -> ChatSender {
    sender("Lead", "claude")
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
            empty: lead_input("", window, cx),
            typed: lead_input("ship it once checks pass", window, cx),
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
            "typed while the lead works: queues",
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
            column(chat_header(&lead(), "xps", None, cx), cx),
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
                .id("lead-sidebar-list")
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

/// The lead chat as it sits under a sidebar's list, holding the split you
/// drag. [`with_lead`] puts it under a list in a column that moves it.
pub(crate) struct SidebarLead {
    input: Entity<InputState>,
    thread: ScrollHandle,
    height: Option<Pixels>,
}

impl SidebarLead {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let input = lead_input("", window, cx);
        cx.new(|_| Self {
            input,
            thread: bottom_scroll(),
            height: None,
        })
    }
}

impl Render for SidebarLead {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = cx.theme().ground();
        ChatSection::new(
            chat_header(&lead(), "xps", None, cx),
            chat_thread(conversation("lead-sidebar", ground, cx)),
            composer("lead-sidebar", &self.input, false, Some(ground), cx),
            &self.thread,
            workspace_sidebar_fade(ground, cx),
        )
        .height(self.height)
    }
}

/// `list` over the lead chat, in a column that moves the split as you drag.
pub(crate) fn with_lead(
    id: &'static str,
    list: impl IntoElement,
    lead: &Entity<SidebarLead>,
) -> AnyElement {
    let split = lead.clone();
    div()
        .id(id)
        .flex()
        .flex_col()
        .size_full()
        .min_h(px(0.0))
        .on_drag_move::<ChatSplitDrag>(move |event: &DragMoveEvent<ChatSplitDrag>, _, cx| {
            split.update(cx, |lead, cx| {
                lead.height = Some(chat_split_height(event.event.position.y, event.bounds));
                cx.notify();
            });
        })
        .child(list)
        .child(lead.clone())
        .into_any_element()
}

pub(crate) struct SidebarChat {
    list: ScrollHandle,
    lead: Entity<SidebarLead>,
}

impl SidebarChat {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            list: ScrollHandle::new(),
            lead: SidebarLead::new(window, cx),
        }
    }
}

impl Render for SidebarChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = cx.theme().ground();
        let fade = workspace_sidebar_fade(ground, cx);
        window_frame(
            app_shell_surface(
                "lead-sidebar-shell",
                cx.theme().background,
                workspace_sidebar_panel(
                    "lead-sidebar",
                    CHAT_WIDTH,
                    workspace_sidebar_titlebar(
                        "lead-sidebar-titlebar",
                        sidebar_top_controls(true),
                        cx,
                    ),
                    with_lead(
                        "lead-sidebar-column",
                        list(&self.list, fade, cx),
                        &self.lead,
                    ),
                    ground,
                    true,
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
}

pub(crate) struct TitlebarChat {
    input: Entity<InputState>,
    thread: ScrollHandle,
    open: bool,
}

impl TitlebarChat {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            input: lead_input("", window, cx),
            thread: bottom_scroll(),
            open: true,
        }
    }

    fn palette(&self, cx: &App) -> AnyElement {
        lead_palette(&self.input, &self.thread, cx)
    }
}

/// The command palette in Lead mode: the Lead pill in the input, the thread
/// under it and an approval waiting on you.
pub(crate) fn lead_palette(
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
            label: "Lead".into(),
            program,
        }),
    );
    let ask = permission(
        "lead-ask",
        "Atlas tests wants to run sudo pacman -S vulkan-swrast",
        None,
        &[("Yes", Some(true)), ("No, tell it what to do", Some(false))],
        0,
        true,
        cx,
    );
    CommandPaletteSurface::new(unified_command_palette_input(input, pills, cx), 1)
        .rows(lead_palette_body(
            chat_thread(conversation("lead-palette", layer.ground, cx)),
            Some(ask),
            thread,
            layer.ground.fill,
        ))
        .hints(LEAD_PALETTE_HINTS)
        .into_any_element()
}

impl Render for TitlebarChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let waiting = (!self.open).then_some(cx.theme().warning);
        let button = workspace_chat_button("lead-chat-button").on_click(cx.listener(
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
                .id("lead-palette-scrim")
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
                "lead-titlebar-shell",
                cx.theme().background,
                div(),
                Some(bar.into_any_element()),
                app_workspace_surface(
                    "lead-titlebar-workspace",
                    panes("lead-titlebar-panes", cx),
                    Vec::new(),
                    cx,
                ),
                overlay,
            ),
            cx,
        )
    }
}
