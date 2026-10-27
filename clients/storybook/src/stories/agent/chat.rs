use std::{rc::Rc, sync::Arc};

use zz_client::program::Program;
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, IntoElement, MouseButton,
    ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window, div, prelude::*,
    px,
};
use zz_protocol::{
    AgentProvider, GitMark, InsertStyle, PaneId, PathEntry, PathKind, PathListRoot, SessionId,
    ShellKind, WindowId,
};
use zz_ui::{
    ActiveTheme as _, Ground, IconName, PaintState,
    agent::{
        chat::{
            CHAT_COMPOSER_INSET, ChatBubble, ChatSender, ChatTyping, Receipt, chat_asleep,
            chat_button, chat_header, chat_latest, chat_seen, chat_thread, chat_unreachable,
            driver_palette_body, driver_palette_hints, driver_palette_place,
            driver_palette_thread_least,
        },
        composer::{Composer, composer_ground},
        controls::{
            ComposerAction, composer_action_button, composer_add_button, context_usage_meter,
        },
    },
    command::{
        COMMAND_PALETTE_MAX_WIDTH, CommandPaletteSurface, PaletteHostId, PalettePill,
        PaletteStatus, unified_command_palette_input,
    },
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
    shell::{app_shell_surface, app_workspace_surface, titlebar_over_panes},
    target::{TargetBackend, TargetHost, TargetPane, TargetPicker, TargetSession, TargetWindow},
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
    summary: "The chat with the Driver Agent lives in the command palette's Driver mode: type > in an empty palette, click the title bar's chat button, or click the driver's last message at the bottom of the sidebar. Your messages sit right in the accent with read ticks; the driver's answers and the agents' messages sit left, an agent's under its sidebar tile and name. Agents at work show as sidebar rows, so the chat carries no tool calls. The driver's model and effort are settings, under Agents.",
    sections: &[
        Section {
            id: "messages",
            name: "Messages",
            summary: "ChatBubble on the sidebar's ground. One tick: zz has it. Two: typed into the driver's pane. Two in full ink: the driver started a turn on it. The driver's answers render markdown the way chat apps do: lists, bold, code, with headings at the body's size. The time and ticks keep the bubble's corner; a last line that would run into them wraps first. While the driver works, ChatTyping takes the place of its answer: three dots that rise in turn, and its latest steps in the words its CLI gives them (a Bash call's description, the plan item in progress), the newest beside the dots.",
            build: |_, cx| stateless(messages, cx),
        },
        Section {
            id: "links",
            name: "Links",
            summary: "zz links: a Markdown link whose destination is a target, [cargo test](@xps:work:1.%8) or [atlas.rs:212](@xps:~/dev/zz/crates/atlas/src/atlas.rs:212), and a bare @xps:work:2 in prose, which the bubble makes a link when its host is written or what it names is there. Your own mentions are links too. A link reads the fleet as it is: underlined in the accent (your ink on your bubbles) while what it names is there, muted while its host is offline or connecting, struck through once it is gone. Hover one for its card: what it names, where, and whether it is up. A click goes there (attaches the host, switches to the session, window or pane, or pastes a file's path into the pane you came from), the platform key with a click mentions it in the composer, and a right click copies it.",
            build: |_, cx| stateless(links, cx),
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "The Composer the agent lens uses too, as Driver mode draws it: a pill on the palette with + to start an @ mention, the field, the driver's context ring and the round send button. While the driver works, the button steers it with what you typed, and with the field empty it stops the driver. Click a field to focus it.",
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
            summary: "chat_latest: what the sidebar keeps of the chat. The driver's header, then its last answer cut to four lines, or ChatTyping while it works. A dot in the header while an approval waits on you. A click opens the command palette in Driver mode. With no driver running, chat_asleep offers Wake: zz never starts one on its own, so a restart spends no tokens. When too few hosts answer for zz to start one, chat_unreachable says so and Start here starts one on this machine.",
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
            summary: "With the sidebar hidden, the title bar's chat button opens the command palette in Driver mode, wider than the other modes: the thread with the same fade, which opens from nothing so the input slides down to the bottom like a chat composer, an approval waiting on you, the composer pill (+ starts a mention, the round button sends, steers, or stops while the driver works), and the footer: where the driver runs on the left (a click shows its terminal), Driver mode's keys on the right (while the driver works, Enter steers it, Tab queues and Ctrl-C stops it; Up takes your last message back to edit). In either layout, > typed into an empty palette switches to Driver mode too. Click the scrim to close it and the button to open it again; the button carries a dot while something waits.",
            build: |window, cx| cx.new(|cx| TitlebarChat::new(window, cx)).into(),
        },
        Section {
            id: "fades",
            name: "Thread fades",
            summary: "Driver mode's thread scrolled halfway: it fades at the top while there is more above and at the bottom while there is more below, and starts one inset under the palette's top edge so nothing it holds draws into the rounded corners. Scroll it to see each fade go at its end.",
            build: |window, cx| stateful(window, cx, Fades::new, fades),
        },
        Section {
            id: "phone",
            name: "On a phone",
            summary: "The phone's Hosts page keeps the driver's last answer, cut to two lines, in phone_chat_card above the hosts, or ChatTyping while it works; a dot in its header while something new waits. The card, the Message the driver field in the Hosts bar, the chat button on a host's page and on a pane (dotted while the driver works or waits on you) and Driver chat in the prefix menu open the chat in a full-height drawer that drags down to close: phone_chat_place as its heading, the driver's name over where it runs (a tap shows its terminal), a close button, the thread, an approval waiting on you, and the composer pill riding on the keyboard. The driver's session stays out of the overview.",
            build: |window, cx| stateful(window, cx, Phones::new, phones),
        },
        Section {
            id: "mentions",
            name: "Mentions",
            summary: "@ in the input opens the target picker floating over it, the one the command palette's @ mode and prefix F use too. A mention is a target the way tmux's -t names one, session:window.pane, with an optional host in front, or a file in scp form, @xps:~/dev/zz/Cargo.toml, with git's letter. Without a host it means the host here, so @ lists the sessions here first, then the other hosts with their status dot, then ~/. A row with more under it ends in ›, and Tab (or a click) opens that level: a host its sessions and its home folder, a session its windows, a window its panes, a folder its entries. Typing :, . or / does the same by hand. Typing filters the level the cursor is in, and after the last / it searches everything under that folder. Up and Down move, Enter puts the highlighted row in as a finished mention and a space, its host always written, so @main:1 goes out as @macbook:main:1. Esc closes only the picker. The header says where the cursor is.",
            build: |window, cx| stateful(window, cx, Mentions::new, mentions),
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
            "Three agents are working:\n\n- **zz** on alienware needs you, its question is open on the right\n- `atlas` tests are green on xps\n- the sprite sheet is half done",
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
            "an answer with headings and nested bullets: headings at the body's size",
            column(
                chat_thread([ChatBubble::theirs(
                    "headings",
                    "## Status\n\nThree agents are working:\n\n- **zz** on alienware needs you\n  - it asks whether to keep the old socket path\n- `atlas` tests are green on xps\n\n### Next\n\nI merge clairvo's PR once the last two checks pass.",
                    "9:45",
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

const LINKED_ANSWER: &str = "The atlas tests broke in [cargo test](@xps:work:1.%8), see [atlas.rs:212](@xps:~/dev/zz/crates/atlas/src/atlas.rs:212). [Codex](@xps:%9) is reviewing it on @xps:work:2.\n\n@ubuntu is still connecting, and [the old logs window](@xps:work:7) is gone.";

fn links(_: &mut Window, cx: &mut App) -> AnyElement {
    let hosts: Arc<[TargetHost]> = StoryMentions.hosts(cx).into();
    states()
        .columns(2)
        .state(
            "the driver's answer: a pane, a file, a bare window, an offline host and a gone window",
            column(
                chat_thread([ChatBubble::theirs("links-answer", LINKED_ANSWER, "9:45")
                    .links(hosts.clone())
                    .into_any_element()]),
                cx,
            ),
        )
        .state(
            "your message: what you mention is a link too",
            column(
                chat_thread([ChatBubble::mine(
                    "links-mine",
                    "what is @xps:work:1.%8 stuck on? and read @macbook:~/dev/zz/Cargo.toml",
                    "9:44",
                )
                .receipt(Receipt::Read)
                .links(hosts)
                .into_any_element()]),
                cx,
            ),
        )
        .into_any_element()
}

pub(crate) struct Composers {
    empty: Entity<InputState>,
    typed: Entity<InputState>,
    working: Entity<InputState>,
}

impl Composers {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            empty: driver_input("", window, cx),
            typed: driver_input("ship it once checks pass", window, cx),
            working: driver_input("", window, cx),
        }
    }
}

/// A column on the palette's paint, where Driver mode's composer sits.
fn palette_column(content: impl IntoElement, cx: &App) -> AnyElement {
    let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
    div()
        .w_full()
        .max_w(px(CHAT_WIDTH))
        .pt(px(10.0))
        .bg(layer.fill)
        .rounded(cx.theme().radius)
        .framed(cx)
        .child(content)
        .into_any_element()
}

fn composers(state: &Composers, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "empty: + starts a mention, nothing to send yet",
            palette_column(palette_composer("empty", &state.empty, false, cx), cx),
        )
        .state(
            "typed while the driver works: the button steers",
            palette_column(palette_composer("typed", &state.typed, true, cx), cx),
        )
        .state(
            "the driver works and the field is empty: the button stops it",
            palette_column(palette_composer("working", &state.working, true, cx), cx),
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
        .state(
            "asleep until you wake it",
            frame(
                chat_asleep(
                    "latest-asleep",
                    false,
                    cx.theme().ground(),
                    |_, _, _| {},
                    cx,
                )
                .into_any_element(),
            ),
        )
        .state(
            "waking",
            frame(
                chat_asleep("latest-waking", true, cx.theme().ground(), |_, _, _| {}, cx)
                    .into_any_element(),
            ),
        )
        .state(
            "no driver, too few hosts",
            frame(
                chat_unreachable(
                    "latest-unreachable",
                    1,
                    3,
                    cx.theme().ground(),
                    |_, _, _| {},
                    cx,
                )
                .into_any_element(),
            ),
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

/// The command palette in Driver mode: the thread, an approval waiting on you,
/// and the input with the Driver pill at the bottom.
pub(crate) fn driver_palette(
    input: &Entity<InputState>,
    thread: &ScrollHandle,
    cx: &App,
) -> AnyElement {
    let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
    let ask = permission(
        "driver-ask",
        "Atlas tests wants to run sudo pacman -S vulkan-swrast",
        None,
        &[("Yes", Some(true)), ("No, tell it what to do", Some(false))],
        0,
        true,
        cx,
    );
    CommandPaletteSurface::new(palette_composer("driver-palette", input, true, cx), 1)
        .input_last()
        .rows(driver_palette_body(
            chat_thread(conversation("driver-palette", layer.ground, cx)),
            Some(ask),
            thread,
            layer.ground.fill,
            driver_palette_thread_least(px(WINDOW_HEIGHT)),
            px(240.0),
        ))
        .hints(driver_palette_hints(true, true))
        .footer_leading(palette_place(cx))
        .into_any_element()
}

/// Driver mode's composer: `+` to start a mention, the field, the driver's
/// context ring, and send or stop.
pub(crate) fn palette_composer(
    scope: &'static str,
    input: &Entity<InputState>,
    running: bool,
    cx: &App,
) -> AnyElement {
    let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
    let typed = !input.read(cx).value().trim().is_empty();
    let stop = running && !typed;
    div()
        .flex_none()
        .w_full()
        .px(px(CHAT_COMPOSER_INSET))
        .pb(px(CHAT_COMPOSER_INSET))
        .child(Composer {
            input: input.clone(),
            leading: Some(
                composer_add_button((scope, 1_usize), "Mention a host, session or file", cx)
                    .into_any_element(),
            ),
            usage: Some(context_usage_meter((scope, 2_usize), 41_000, 200_000, cx)),
            action: composer_action_button(
                (scope, 0_usize),
                if stop {
                    ComposerAction::Stop
                } else {
                    ComposerAction::Send
                },
                stop || typed,
                cx,
            )
            .on(composer_ground(cx))
            .into_any_element(),
            attachments: None,
            command_hint: None,
            ground: Some(layer.ground),
        })
        .into_any_element()
}

/// Where the driver runs, at the left of Driver mode's footer.
pub(crate) fn palette_place(cx: &App) -> AnyElement {
    driver_palette_place(
        "xps",
        PaletteStatus::Online.color(cx),
        Program::by_id("claude"),
        |_, _, _| {},
        cx,
    )
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
                Some(titlebar_over_panes(bar, gap(crate::pane().gaps)).into_any_element()),
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

struct StoryMentions;

impl TargetBackend for StoryMentions {
    fn hosts(&self, _: &App) -> Vec<TargetHost> {
        let pane = |id: u64, title: &str, program: Option<&'static str>| TargetPane {
            id: PaneId(id),
            title: title.to_owned(),
            program: program.map(SharedString::from),
            icon: IconName::SquareTerminal,
        };
        let window = |id: u64, index: u32, name: &str, panes: Vec<TargetPane>| TargetWindow {
            id: WindowId(id),
            index,
            name: name.to_owned(),
            panes,
        };
        let session = |id: u64, name: &str, windows: Vec<TargetWindow>| TargetSession {
            id: SessionId(id),
            name: name.to_owned(),
            windows,
        };
        let host = |id: u32, name: &str, status, sessions: Vec<TargetSession>| TargetHost {
            id: PaletteHostId(id),
            name: name.to_owned(),
            status,
            here: id == 0,
            sessions,
        };
        vec![
            host(
                0,
                "macbook",
                PaletteStatus::Online,
                vec![
                    session(
                        0,
                        "main",
                        vec![
                            window(
                                0,
                                1,
                                "zz",
                                vec![pane(0, "nvim", None), pane(12, "cargo watch", None)],
                            ),
                            window(
                                8,
                                2,
                                "agents",
                                vec![pane(13, "fix the sidebar fade", Some("Claude Code"))],
                            ),
                        ],
                    ),
                    session(
                        4,
                        "dotfiles",
                        vec![window(9, 1, "nvim", vec![pane(14, "init.lua", None)])],
                    ),
                ],
            ),
            host(
                1,
                "xps",
                PaletteStatus::Online,
                vec![
                    session(
                        2,
                        "work",
                        vec![
                            window(
                                4,
                                1,
                                "zz",
                                vec![
                                    pane(7, "fix the atlas tests", Some("Claude Code")),
                                    pane(8, "cargo test", None),
                                ],
                            ),
                            window(5, 2, "atlas", vec![pane(9, "review", Some("Codex"))]),
                            window(6, 3, "logs", vec![pane(10, "journalctl -f", None)]),
                        ],
                    ),
                    session(
                        1,
                        "blog",
                        vec![window(1, 1, "astro", vec![pane(1, "npm run dev", None)])],
                    ),
                ],
            ),
            host(
                2,
                "alienware",
                PaletteStatus::Online,
                vec![session(
                    3,
                    "sprites",
                    vec![window(7, 1, "sheet", vec![pane(11, "", Some("Codex"))])],
                )],
            ),
            host(3, "ubuntu", PaletteStatus::Waiting, Vec::new()),
        ]
    }

    fn list(&self, _: &str, _: &str, _: Option<PaneId>, _: &mut App) -> Result<u64, SharedString> {
        Ok(1)
    }

    fn cancel(&self, _: &str, _: u64, _: &mut App) {}
}

/// Everything under `~/dev/zz/` on xps, as the daemon walks it: folders end
/// in `/`.
const ZZ: &[&str] = &[
    "crates/",
    "crates/zz/",
    "crates/zz/src/",
    "crates/zz/src/agent/",
    "crates/zz/src/agent/driver_palette.rs",
    "crates/zz-ui/",
    "crates/zz-ui/src/",
    "crates/zz-ui/src/lib.rs",
    "crates/zz-ui/src/target.rs",
    "crates/zz-ui/src/target_tests.rs",
    "knowledge/",
    "knowledge/index.md",
    "scripts/",
    "scripts/build-storybook.sh",
    "Cargo.toml",
    "CLAUDE.md",
    "Justfile",
    "README.md",
];

/// xps answering the listing of `~/dev/zz/`.
fn list_zz(picker: &mut TargetPicker, cx: &mut Context<TargetPicker>) {
    let root = PathListRoot {
        root: "/home/fab/dev/zz".to_owned(),
        display_root: "~/dev/zz/".to_owned(),
        cwd: None,
        insert: InsertStyle::Shell(ShellKind::Posix),
    };
    let entries: Arc<[PathEntry]> = ZZ
        .iter()
        .map(|rel| PathEntry {
            rel: (*rel).to_owned(),
            kind: if rel.ends_with('/') {
                PathKind::Dir
            } else {
                PathKind::File
            },
            symlink: false,
        })
        .collect();
    picker.apply_begin("xps", 1, Ok(root), cx);
    picker.apply_chunk("xps", 1, &entries, true, false, cx);
    picker.apply_git(
        "xps",
        1,
        vec![
            ("Cargo.toml".to_owned(), GitMark::Modified),
            ("crates/zz-ui/src/target.rs".to_owned(), GitMark::Added),
            ("scripts/".to_owned(), GitMark::Untracked),
        ],
        cx,
    );
}

struct MentionDemo {
    input: Entity<InputState>,
    picker: Entity<TargetPicker>,
    thread: ScrollHandle,
}

impl MentionDemo {
    fn new(value: &str, window: &mut Window, cx: &mut App) -> Self {
        let input = driver_input(value, window, cx);
        let picker =
            cx.new(|cx| TargetPicker::for_composer(Rc::new(StoryMentions), input.clone(), cx));
        picker.update(cx, TargetPicker::sync);
        Self {
            input,
            picker,
            thread: bottom_scroll(),
        }
    }

    fn listed(self, cx: &mut App) -> Self {
        self.picker.update(cx, list_zz);
        self
    }
}

struct Mentions {
    top: MentionDemo,
    here: MentionDemo,
    host: MentionDemo,
    windows: MentionDemo,
    files: MentionDemo,
    search: MentionDemo,
}

impl Mentions {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            top: MentionDemo::new("@", window, cx),
            here: MentionDemo::new("ask @main:", window, cx),
            host: MentionDemo::new("ask @xps:", window, cx),
            windows: MentionDemo::new("ask @xps:work:", window, cx),
            files: MentionDemo::new("compare with @xps:~/dev/zz/", window, cx).listed(cx),
            search: MentionDemo::new("compare with @xps:~/dev/zz/target", window, cx).listed(cx),
        }
    }
}

/// The palette in Driver mode with the `@` picker open over its input.
fn mention_palette(scope: &'static str, demo: &MentionDemo, cx: &App) -> AnyElement {
    let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
    let pills = Program::by_id("claude").map(|program| PalettePill::Program {
        label: "Driver".into(),
        program,
    });
    div()
        .w(px(COMMAND_PALETTE_MAX_WIDTH))
        .child(
            CommandPaletteSurface::new(unified_command_palette_input(&demo.input, pills, cx), 1)
                .input_last()
                .rows(driver_palette_body(
                    chat_thread(conversation(scope, layer.ground, cx)),
                    None,
                    &demo.thread,
                    layer.ground.fill,
                    driver_palette_thread_least(px(WINDOW_HEIGHT)),
                    px(340.0),
                ))
                .hints(driver_palette_hints(false, false))
                .over_input(demo.picker.clone()),
        )
        .into_any_element()
}

fn mentions(state: &Mentions, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "@ lists the sessions here, then the other hosts, then ~/",
            mention_palette("mention-hosts", &state.top, cx),
        )
        .state(
            "@main: needs no host: session main on macbook, the host here",
            mention_palette("mention-here", &state.here, cx),
        )
        .state(
            "Tab on xps opens @xps: with its sessions, then its home folder",
            mention_palette("mention-host", &state.host, cx),
        )
        .state(
            "Tab on work opens @xps:work: with the session's windows",
            mention_palette("mention-windows", &state.windows, cx),
        )
        .state(
            "@xps:~/dev/zz/ lists a folder on xps",
            mention_palette("mention-files", &state.files, cx),
        )
        .state(
            "@xps:~/dev/zz/target searches everything under the folder",
            mention_palette("mention-search", &state.search, cx),
        )
        .into_any_element()
}

pub(crate) struct Fades {
    input: Entity<InputState>,
    thread: ScrollHandle,
}

impl Fades {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let thread = ScrollHandle::new();
        thread.set_offset(zz_gpui::point(px(0.0), px(-160.0)));
        Self {
            input: driver_input("", window, cx),
            thread,
        }
    }
}

fn fades(state: &Fades, _: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(driver_palette(&state.input, &state.thread, cx))
        .into_any_element()
}

const PHONE_WIDTH: f32 = 340.0;
const PHONE_HEIGHT: f32 = 620.0;
const PHONE_SAFE_TOP: f32 = 24.0;
const PHONE_SAFE_BOTTOM: f32 = 20.0;

pub(crate) struct Phones {
    empty: Entity<InputState>,
}

impl Phones {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            empty: driver_input("", window, cx),
        }
    }
}

fn phone_frame(content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .relative()
        .w_full()
        .max_w(px(PHONE_WIDTH))
        .h(px(PHONE_HEIGHT))
        .overflow_hidden()
        .bg(cx.theme().background)
        .rounded(px(36.0))
        .framed(cx)
        .child(content)
        .into_any_element()
}

fn overview_card(scope: &'static str, working: bool, unread: bool, cx: &App) -> AnyElement {
    let ground = cx.theme().ground_of(roles::PHONE_PILL);
    let message = if working {
        ChatTyping::new((scope, 0_usize))
            .steps(COMMIT_STEPS)
            .on(ground)
            .into_any_element()
    } else {
        ChatBubble::theirs((scope, 1_usize), LATEST_ANSWER, "")
            .lines(2)
            .on(ground)
            .into_any_element()
    };
    let dot = unread.then(|| {
        div()
            .size(px(ATTENTION_DOT))
            .rounded_full()
            .bg(cx.theme().accent)
            .into_any_element()
    });
    phone_frame(
        div()
            .absolute()
            .left(px(16.0))
            .right(px(16.0))
            .bottom(px(24.0))
            .child(zz_ui::agent::chat::phone_chat_card(
                (scope, 2_usize),
                chat_header(&driver(), "xps", dot, cx),
                message,
                cx,
            )),
        cx,
    )
}

fn phone_sheet(input: &Entity<InputState>, cx: &App) -> AnyElement {
    let ground = cx.theme().ground_of(roles::DRIVER_DRAWER);
    let body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .justify_end()
                .overflow_hidden()
                .child(chat_thread(conversation("phone-sheet", ground, cx))),
        )
        .child(
            div()
                .flex_none()
                .w_full()
                .px(px(CHAT_COMPOSER_INSET))
                .pb(px(CHAT_COMPOSER_INSET))
                .child(Composer {
                    input: input.clone(),
                    leading: Some(
                        composer_add_button(
                            "phone-sheet-mention",
                            "Mention a host, session or file",
                            cx,
                        )
                        .into_any_element(),
                    ),
                    usage: Some(context_usage_meter(
                        "phone-sheet-usage",
                        41_000,
                        200_000,
                        cx,
                    )),
                    action: composer_action_button(
                        "phone-sheet-send",
                        ComposerAction::Stop,
                        true,
                        cx,
                    )
                    .into_any_element(),
                    attachments: None,
                    command_hint: None,
                    ground: Some(ground),
                }),
        );
    phone_frame(
        zz_ui::compact::bottom_sheet(
            "phone-sheet",
            "Driver",
            [zz_ui::compact::sheet_close("phone-sheet-close").into_any_element()],
            body,
            px(PHONE_SAFE_BOTTOM),
            |_, _| {},
        )
        .surface_role(roles::DRIVER_DRAWER)
        .header_role(roles::PHONE_BAR)
        .full(px(PHONE_SAFE_TOP))
        .heading(zz_ui::agent::chat::phone_chat_place(
            "xps",
            Program::by_id("claude"),
            |_, _, _| {},
            cx,
        )),
        cx,
    )
}

fn phones(state: &Phones, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(3)
        .state(
            "overview: its last answer, unread",
            overview_card("phone-latest", false, true, cx),
        )
        .state(
            "overview: at work",
            overview_card("phone-working", true, false, cx),
        )
        .state(
            "the drawer, the driver at work",
            phone_sheet(&state.empty, cx),
        )
        .into_any_element()
}
