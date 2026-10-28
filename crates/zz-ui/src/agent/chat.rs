//! The driver chat: your messages and the driver's answers as rounded bubbles
//! on the speaker's side, an agent's message under its sidebar tile and name,
//! and read ticks on what you sent. It is typed into with the lens's
//! [`super::composer::Composer`]. The driver's model is a setting, so the
//! composer carries no pickers.

use std::sync::Arc;

use zz_client::program::Program;
use zz_gpui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, HighlightStyle, Hsla, IntoElement,
    Pixels, RenderOnce, ScrollHandle, SharedString, StyledText, Window, div, prelude::*, px,
    relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Edge, Ground, Icon, IconName, PaintState, Sizable as _,
    StyledExt as _, Surface, Tone,
    command::PaletteHint,
    h_flex,
    link::{LinkInk, TargetLinks, autolink, autolink_plain},
    program::program_tile,
    roles,
    scroll::ScrollableElement as _,
    target::TargetHost,
    text::{TextView, TextViewStyle},
    v_flex,
};

pub const CHAT_GAP: f32 = 10.0;
pub const CHAT_PADDING_X: f32 = 14.0;
pub const CHAT_PADDING_TOP: f32 = 8.0;
pub const CHAT_PADDING_BOTTOM: f32 = 12.0;
pub const CHAT_HEADER_HEIGHT: f32 = 36.0;
pub const CHAT_COMPOSER_INSET: f32 = 10.0;

const BUBBLE_RADIUS: f32 = 17.0;
const BUBBLE_PADDING_X: f32 = 11.0;
const BUBBLE_PADDING_TOP: f32 = 7.0;
const BUBBLE_PADDING_BOTTOM: f32 = 6.0;
const MINE_MAX_WIDTH: f32 = 0.82;
const THEIRS_MAX_WIDTH: f32 = 0.9;
const TEXT_SIZE: f32 = 13.0;
const TEXT_LINE: f32 = 20.0;
const STAMP_SIZE: f32 = 11.0;
const STAMP_LINE: f32 = 14.0;
const STAMP_RIGHT: f32 = 8.0;
const STAMP_BOTTOM: f32 = 4.0;
const TICK_SIZE: f32 = 16.0;
const SENDER_TILE: f32 = 16.0;
const SENDER_SIZE: f32 = 12.0;
const SENDER_LINE: f32 = 16.0;
const SEEN_TILE: f32 = 14.0;
const HEADER_TILE: f32 = 16.0;
const THREAD_FADE: f32 = 28.0;
const PHONE_TITLE_SIZE: f32 = 24.0;
const PHONE_TITLE_LINE: f32 = 30.0;
const PHONE_PLACE_SIZE: f32 = 13.0;
const PHONE_CARD_RADIUS: f32 = 22.0;
/// How long the thread takes to open under the palette's input, pushing
/// the composer down to the bottom.
const DRIVER_THREAD_OPEN: std::time::Duration = std::time::Duration::from_millis(280);
/// How many of the driver's steps a [`ChatTyping`] shows.
pub const TYPING_STEPS: usize = 3;
const TYPING_PERIOD: std::time::Duration = std::time::Duration::from_millis(1300);
const TYPING_BEAT: f32 = 0.14;
const TYPING_FPS: f32 = 30.0;
const TYPING_RISE_SHARE: f32 = 0.42;
const TYPING_DOT: f32 = 4.0;
const TYPING_DOT_GAP: f32 = 3.0;
const TYPING_LIFT: f32 = 1.5;
const TYPING_REST: f32 = 0.4;
const TYPING_PADDING_Y: f32 = 8.0;
const STEP_SIZE: f32 = 12.0;
const STEP_LINE: f32 = 18.0;
const STEP_CHECK: f32 = 12.0;

/// How far a message you sent has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receipt {
    /// zz has it; the driver's pane does not yet.
    Sent,
    /// Typed into the driver's pane.
    Delivered,
    /// The driver started a turn on it.
    Read,
}

impl Receipt {
    const fn icon(self) -> IconName {
        match self {
            Self::Sent => IconName::Check,
            Self::Delivered | Self::Read => IconName::CheckRead,
        }
    }

    /// Room the tick takes after the time, in the text's own characters, so
    /// the last line of a message leaves it a gap.
    const fn room(self) -> &'static str {
        match self {
            Self::Sent => "00",
            Self::Delivered | Self::Read => "000",
        }
    }
}

/// Who wrote a message the driver passed on: the agent's name as the sidebar
/// shows it, and the program its tile draws.
#[derive(Clone)]
pub struct ChatSender {
    pub name: SharedString,
    pub program: Option<&'static Program>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatSide {
    Mine,
    Theirs,
}

/// One message. Yours sit right, washed in the accent, with ticks after the
/// time; the driver's and the agents' sit left, and an agent's carries its
/// tile and name on top.
#[derive(IntoElement)]
pub struct ChatBubble {
    id: ElementId,
    side: ChatSide,
    text: SharedString,
    time: SharedString,
    receipt: Option<Receipt>,
    sender: Option<ChatSender>,
    ground: Option<Ground>,
    lines: Option<usize>,
    hosts: Option<Arc<[TargetHost]>>,
}

impl ChatBubble {
    pub fn mine(
        id: impl Into<ElementId>,
        text: impl Into<SharedString>,
        time: impl Into<SharedString>,
    ) -> Self {
        Self::new(id, ChatSide::Mine, text, time)
    }

    pub fn theirs(
        id: impl Into<ElementId>,
        text: impl Into<SharedString>,
        time: impl Into<SharedString>,
    ) -> Self {
        Self::new(id, ChatSide::Theirs, text, time)
    }

    fn new(
        id: impl Into<ElementId>,
        side: ChatSide,
        text: impl Into<SharedString>,
        time: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            side,
            text: text.into(),
            time: time.into(),
            receipt: None,
            sender: None,
            ground: None,
            lines: None,
            hosts: None,
        }
    }

    #[must_use]
    pub fn receipt(mut self, receipt: Receipt) -> Self {
        self.receipt = Some(receipt);
        self
    }

    #[must_use]
    pub fn sender(mut self, sender: ChatSender) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Draw on `ground`, the paint the thread sits on, instead of the window.
    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }

    /// Cut the message to its first `lines` lines.
    #[must_use]
    pub fn lines(mut self, lines: usize) -> Self {
        self.lines = Some(lines);
        self
    }

    /// Read zz links and bare `@` mentions against `hosts`, the fleet as it
    /// is now. A cut message keeps its mentions as text.
    #[must_use]
    pub fn links(mut self, hosts: Arc<[TargetHost]>) -> Self {
        self.hosts = Some(hosts);
        self
    }
}

impl RenderOnce for ChatBubble {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let mine = self.side == ChatSide::Mine;
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let surface = if mine {
            Surface::new(roles::CHAT_MINE)
                .tone(Tone::Accent)
                .prominent(true)
        } else {
            Surface::new(roles::CHAT_BUBBLE)
        }
        .on(ground);
        let layer = theme.layer(surface, PaintState::Rest);
        let radius = layer.radius.min(px(BUBBLE_RADIUS));
        let ink = layer.ground.ink;
        let muted = crate::picker::muted_on(&layer, cx);
        let read = if mine { ink } else { theme.accent };
        let receipt = self.receipt.filter(|_| mine);
        let hosts = self.hosts.filter(|_| self.lines.is_none());
        let linked = hosts.as_ref().and_then(|hosts| {
            if mine {
                autolink_plain(&self.text, hosts)
            } else {
                autolink(&self.text, hosts)
            }
        });
        let rich = linked.is_some() || (!mine && self.lines.is_none() && is_rich(&self.text));
        let stamped = !self.time.is_empty() || receipt.is_some();

        let body = if rich {
            let ink = LinkInk {
                live: if mine { ink } else { theme.accent },
                muted,
            };
            TextView::markdown(
                ElementId::NamedChild(Arc::new(self.id.clone()), "markdown".into()),
                linked.map_or_else(|| self.text.clone(), SharedString::from),
            )
            .style(chat_markdown_style(cx))
            .selectable(true)
            .when_some(hosts, |view, hosts| {
                view.links(TargetLinks::new(hosts, ink))
            })
            .min_w_0()
            .into_any_element()
        } else {
            let text = if self.lines.is_some() && !mine {
                SharedString::from(plain_text(&self.text))
            } else {
                self.text.clone()
            };
            let room = format!(" {}{}", self.time, receipt.map_or("", Receipt::room));
            let shown = format!("{text}{room}");
            let hidden = text.len()..shown.len();
            StyledText::new(shown)
                .with_highlights([(
                    hidden,
                    HighlightStyle {
                        fade_out: Some(1.0),
                        ..HighlightStyle::default()
                    },
                )])
                .into_any_element()
        };

        let stamp = h_flex()
            .absolute()
            .right(px(STAMP_RIGHT))
            .bottom(px(STAMP_BOTTOM))
            .gap(px(2.0))
            .items_center()
            .text_size(crate::rems_from_px(STAMP_SIZE))
            .line_height(px(STAMP_LINE))
            .text_color(muted)
            .child(self.time.clone())
            .when_some(receipt, |stamp, receipt| {
                stamp.child(
                    div()
                        .debug_selector(move || format!("chat-receipt-{receipt:?}"))
                        .child(Icon::new(receipt.icon()).size(px(TICK_SIZE)).text_color(
                            if receipt == Receipt::Read {
                                read
                            } else {
                                muted
                            },
                        )),
                )
            });

        let bubble = v_flex()
            .id(self.id)
            .debug_selector(move || if mine { "chat-mine" } else { "chat-theirs" }.to_owned())
            .relative()
            .min_w_0()
            .max_w(relative(if mine {
                MINE_MAX_WIDTH
            } else {
                THEIRS_MAX_WIDTH
            }))
            .px(px(BUBBLE_PADDING_X))
            .pt(px(BUBBLE_PADDING_TOP))
            .pb(px(if rich && stamped {
                BUBBLE_PADDING_BOTTOM + STAMP_LINE
            } else {
                BUBBLE_PADDING_BOTTOM
            }))
            .text_size(crate::rems_from_px(TEXT_SIZE))
            .line_height(px(TEXT_LINE))
            .text_color(ink)
            .child(bubble_back(&layer, radius))
            .when_some(self.sender, |bubble, sender| {
                bubble.child(sender_line(&sender, cx))
            })
            .child(
                div()
                    .min_w_0()
                    .when_some(self.lines, |body, lines| {
                        body.line_clamp(lines).text_ellipsis()
                    })
                    .child(body),
            )
            .child(stamp);

        h_flex()
            .w_full()
            .when(mine, zz_gpui::Styled::justify_end)
            .child(bubble)
    }
}

/// Whether `text` needs the markdown renderer: anything but one paragraph of
/// plain words, which keeps the bubble as narrow as its line and the time
/// tucked after it.
fn is_rich(text: &str) -> bool {
    use markdown::mdast::Node;
    let Ok(Node::Root(root)) = markdown::to_mdast(text, &markdown::ParseOptions::gfm()) else {
        return false;
    };
    match root.children.as_slice() {
        [Node::Paragraph(paragraph)] => !paragraph
            .children
            .iter()
            .all(|node| matches!(node, Node::Text(_) | Node::Break(_))),
        _ => true,
    }
}

/// `text` with its markdown taken out, one line per block, for a bubble cut
/// to a few lines.
fn plain_text(text: &str) -> String {
    use markdown::mdast::Node;
    match markdown::to_mdast(text, &markdown::ParseOptions::gfm()) {
        Ok(Node::Root(root)) => root
            .children
            .iter()
            .flat_map(|block| match block {
                Node::List(list) => list.children.iter().map(ToString::to_string).collect(),
                block => vec![block.to_string()],
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => text.to_owned(),
    }
}

/// Markdown the way chat apps draw it: headings at the body's size, so an
/// answer with `#` titles reads as a message, not a document.
fn chat_markdown_style(cx: &App) -> TextViewStyle {
    TextViewStyle {
        paragraph_gap: zz_gpui::rems(0.5),
        heading_base_font_size: px(TEXT_SIZE),
        heading_font_size: Some(Arc::new(|_, base| base)),
        highlight_theme: Arc::clone(&cx.theme().highlight_theme),
        is_dark: cx.theme().is_dark(),
        ..TextViewStyle::default()
    }
    .code_role(roles::CODE)
    .figure_role(roles::FIGURE)
}

/// The bubble's fill and edge behind what it holds, so the edge takes no
/// room from the text.
fn bubble_back(layer: &crate::Layer, radius: Pixels) -> zz_gpui::Div {
    div().absolute().inset_0().layer(layer).rounded(radius)
}

/// How high the dot at `index` sits at `phase`, from 0 at rest to 1: each
/// rises and falls in the first part of the period, a beat after the one
/// before it, then waits for the next round.
fn dot_lift(phase: f32, index: usize) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "index is one of three dots")]
    let at = (phase - index as f32 * TYPING_BEAT).rem_euclid(1.0);
    if at < TYPING_RISE_SHARE {
        (std::f32::consts::PI * at / TYPING_RISE_SHARE).sin()
    } else {
        0.0
    }
}

/// The driver at work: a bubble on its side whose three dots rise in turn,
/// with its latest steps, the newest beside the dots and the ones before it
/// above, muted and ticked. A step is what the driver's CLI says it is doing,
/// cut to one line; with none yet the bubble holds only the dots.
#[derive(IntoElement)]
pub struct ChatTyping {
    id: ElementId,
    steps: Vec<SharedString>,
    ground: Option<Ground>,
}

impl ChatTyping {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            steps: Vec::new(),
            ground: None,
        }
    }

    /// The driver's steps so far, oldest first; the last [`TYPING_STEPS`] show.
    #[must_use]
    pub fn steps(mut self, steps: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.steps = steps.into_iter().map(Into::into).collect();
        self
    }

    /// Draw on `ground`, the paint the thread sits on, instead of the window.
    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl RenderOnce for ChatTyping {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let layer = theme.layer(
            Surface::new(roles::CHAT_BUBBLE).on(ground),
            PaintState::Rest,
        );
        let radius = layer.radius.min(px(BUBBLE_RADIUS));
        let ink = ground.ink;
        let muted = ink.muted();
        let shown = self.steps.len().saturating_sub(TYPING_STEPS);
        let mut steps = self.steps.into_iter().skip(shown).collect::<Vec<_>>();
        let current = steps.pop();
        let dots = h_flex()
            .debug_selector(|| "chat-typing-dots".to_owned())
            .flex_none()
            .h(px(STEP_LINE))
            .gap(px(TYPING_DOT_GAP))
            .items_center()
            .children((0..3).map(|index| {
                div()
                    .relative()
                    .size(px(TYPING_DOT))
                    .rounded_full()
                    .bg(muted)
                    .with_animation(
                        ("chat-typing-dot", index),
                        Animation::new(TYPING_PERIOD)
                            .repeat()
                            .with_max_fps(TYPING_FPS),
                        move |dot, phase| {
                            let lift = dot_lift(phase, index);
                            dot.top(px(-TYPING_LIFT * lift))
                                .opacity(TYPING_REST + (1.0 - TYPING_REST) * lift)
                        },
                    )
            }));
        let line = |text: SharedString| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text)
        };
        let bubble = v_flex()
            .id(self.id)
            .debug_selector(|| "chat-typing".to_owned())
            .relative()
            .min_w_0()
            .max_w(relative(THEIRS_MAX_WIDTH))
            .px(px(BUBBLE_PADDING_X + 2.0))
            .py(px(TYPING_PADDING_Y))
            .gap(px(2.0))
            .text_size(crate::rems_from_px(STEP_SIZE))
            .line_height(px(STEP_LINE))
            .child(bubble_back(&layer, radius))
            .children(steps.into_iter().map(|step| {
                h_flex()
                    .debug_selector(|| "chat-typing-done".to_owned())
                    .min_w_0()
                    .gap(px(6.0))
                    .text_color(muted)
                    .child(
                        Icon::new(IconName::Check)
                            .size(px(STEP_CHECK))
                            .flex_none()
                            .text_color(muted),
                    )
                    .child(line(step))
            }))
            .child(
                h_flex()
                    .debug_selector(|| "chat-typing-now".to_owned())
                    .min_w_0()
                    .gap(px(8.0))
                    .text_color(ink)
                    .child(dots)
                    .children(current.map(line)),
            );
        h_flex().w_full().child(bubble)
    }
}

fn sender_tile(sender: &ChatSender, size: f32, cx: &App) -> Option<AnyElement> {
    let program = sender.program?;
    Some(
        program_tile(program, px(size), px(size), 0.0, cx)
            .flex_none()
            .into_any_element(),
    )
}

fn sender_line(sender: &ChatSender, cx: &App) -> AnyElement {
    h_flex()
        .debug_selector(|| "chat-sender".to_owned())
        .gap(px(6.0))
        .mb(px(2.0))
        .items_center()
        .text_size(crate::rems_from_px(SENDER_SIZE))
        .line_height(px(SENDER_LINE))
        .font_weight(zz_gpui::FontWeight::MEDIUM)
        .children(sender_tile(sender, SENDER_TILE, cx))
        .child(sender.name.clone())
        .into_any_element()
}

/// Who has read a message the driver routed on, under it on your side.
pub fn chat_seen(senders: &[ChatSender], cx: &App) -> AnyElement {
    h_flex()
        .debug_selector(|| "chat-seen".to_owned())
        .w_full()
        .justify_end()
        .items_center()
        .gap(px(4.0))
        .mt(px(4.0 - CHAT_GAP))
        .text_size(crate::rems_from_px(STAMP_SIZE))
        .line_height(px(STAMP_LINE))
        .text_color(cx.theme().foreground.muted())
        .child("Seen by")
        .children(
            senders
                .iter()
                .filter_map(|sender| sender_tile(sender, SEEN_TILE, cx)),
        )
        .into_any_element()
}

/// The messages, newest at the bottom.
pub fn chat_thread(messages: impl IntoIterator<Item = AnyElement>) -> zz_gpui::Div {
    v_flex()
        .w_full()
        .justify_end()
        .gap(px(CHAT_GAP))
        .px(px(CHAT_PADDING_X))
        .pt(px(CHAT_PADDING_TOP))
        .pb(px(CHAT_PADDING_BOTTOM))
        .children(messages)
}

/// The line over the thread: the driver's tile, its name and the host it runs
/// on, and whatever the place it sits in puts at the end.
pub fn chat_header(
    driver: &ChatSender,
    host: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> zz_gpui::Div {
    let theme = cx.theme();
    h_flex()
        .debug_selector(|| "chat-header".to_owned())
        .flex_none()
        .w_full()
        .h(px(CHAT_HEADER_HEIGHT))
        .items_center()
        .gap(px(8.0))
        .pl(px(18.0))
        .pr(px(12.0))
        .text_size(crate::rems_from_px(12.0))
        .text_color(theme.foreground.muted())
        .children(sender_tile(driver, HEADER_TILE, cx))
        .child(
            div()
                .text_size(crate::rems_from_px(13.0))
                .font_weight(zz_gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(driver.name.clone()),
        )
        .child(format!("on {}", host.into()))
        .child(div().flex_1())
        .children(trailing)
}

/// The driver's last word at the bottom of the sidebar, under the chat's rule:
/// `header`, then `message`, its last answer or [`ChatTyping`] while it
/// works. The caller makes a click open the chat.
pub fn chat_latest(
    id: impl Into<ElementId>,
    header: impl IntoElement,
    message: impl IntoElement,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    v_flex()
        .id(id)
        .debug_selector(|| "chat-latest".to_owned())
        .flex_none()
        .w_full()
        .cursor_pointer()
        .child(div().mx(px(16.0)).rule_on(roles::CHAT_RULE, Edge::Top, cx))
        .child(header)
        .child(
            div()
                .px(px(CHAT_PADDING_X))
                .pb(px(CHAT_PADDING_BOTTOM))
                .child(message),
        )
}

/// What the sidebar keeps of the chat when no driver runs and too few hosts
/// answered for zz to start one: why, and the button that starts one here.
pub fn chat_unreachable(
    scope: &'static str,
    answered: usize,
    hosts: usize,
    ground: Ground,
    on_start: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    chat_idle(
        scope,
        "not running",
        Some(format!(
            "Only {answered} of {hosts} hosts answered, so zz did not start another. \
             One may still run on a host this machine cannot reach."
        )),
        ("Start here", false),
        ground,
        on_start,
        cx,
    )
}

/// What the sidebar keeps of the chat while no driver runs: zz starts one only
/// when you press Wake, so restarting zz spends no tokens.
pub fn chat_asleep(
    scope: &'static str,
    waking: bool,
    ground: Ground,
    on_wake: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    chat_idle(
        scope,
        if waking { "waking" } else { "asleep" },
        None,
        (if waking { "Waking" } else { "Wake" }, waking),
        ground,
        on_wake,
        cx,
    )
}

fn chat_idle(
    scope: &'static str,
    status: &'static str,
    note: Option<String>,
    (action, busy): (&'static str, bool),
    ground: Ground,
    on_click: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let theme = cx.theme();
    let header = h_flex()
        .debug_selector(move || format!("chat-{}", status.replace(' ', "-")))
        .flex_none()
        .w_full()
        .h(px(CHAT_HEADER_HEIGHT))
        .items_center()
        .gap(px(8.0))
        .pl(px(18.0))
        .pr(px(12.0))
        .text_size(crate::rems_from_px(12.0))
        .text_color(theme.foreground.muted())
        .child(
            div()
                .text_size(crate::rems_from_px(13.0))
                .font_weight(zz_gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child("Driver Agent"),
        )
        .child(status);
    let message = v_flex()
        .gap(px(8.0))
        .items_start()
        .children(note.map(|note| {
            div()
                .text_size(crate::rems_from_px(12.0))
                .text_color(theme.foreground.muted())
                .child(note)
        }))
        .child(
            crate::button::Button::new(SharedString::from(format!("{scope}-start")))
                .tone(Tone::Primary)
                .emphasis(crate::Emphasis::Solid)
                .small()
                .on(ground)
                .label(action)
                .loading(busy)
                .on_click(move |event, window, cx| {
                    if !busy {
                        on_click(event, window, cx);
                    }
                }),
        );
    chat_latest(scope, header, message, cx)
}

/// The palette's footer keys in Driver mode: while the driver works, Enter
/// steers it, Tab queues for after its turn and Ctrl-C stops it; digits
/// answer what it asks.
pub fn driver_palette_hints(working: bool, asking: bool) -> Vec<PaletteHint> {
    let hint = |key, label| PaletteHint { key, label };
    let mut hints = if working {
        vec![
            hint("enter", "steer"),
            hint("tab", "queue"),
            hint("ctrl-c", "stop"),
        ]
    } else {
        vec![hint("enter", "send")]
    };
    hints.push(hint("up", "edit last"));
    if asking {
        hints.push(hint("1 2", "answer"));
    }
    hints.push(hint("escape", "close"));
    hints
}

/// The least height of Driver mode's thread in a window `viewport` tall: what
/// keeps the palette as tall as it is with its rows full, so switching to
/// Driver mode keeps its size.
pub fn driver_palette_thread_least(viewport: Pixels) -> Pixels {
    px(crate::command::command_palette_full_height(viewport)
        - crate::command::COMMAND_PALETTE_INSET
        - super::composer::COMPOSER_HEIGHT
        - CHAT_COMPOSER_INSET
        - crate::command::COMMAND_PALETTE_FOOTER_HEIGHT)
}

/// What the palette shows in Driver mode, over its input: the thread, from
/// `min_height` to `max_height` tall with its newest message on the input and
/// scrolling with a fade at either end while there is more that way, then an
/// approval or question waiting on you. `fade` is the palette's fill. The
/// thread opens from nothing, so the input slides from the palette's top to
/// its bottom as the chat appears.
pub fn driver_palette_body(
    thread: impl IntoElement,
    ask: Option<AnyElement>,
    scroll: &ScrollHandle,
    fade: Hsla,
    min_height: Pixels,
    max_height: Pixels,
) -> zz_gpui::Div {
    let min_height = min_height.min(max_height);
    v_flex()
        .debug_selector(|| "driver-palette".to_owned())
        .w_full()
        .child(
            div()
                .relative()
                .w_full()
                .child(
                    div()
                        .id("driver-palette-thread")
                        .min_h(min_height)
                        .max_h(max_height)
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .child(v_flex().min_h(min_height).justify_end().child(thread))
                        .with_animation(
                            "driver-palette-open",
                            Animation::new(DRIVER_THREAD_OPEN)
                                .with_easing(zz_gpui::ease_out_quint()),
                            move |thread, delta| {
                                thread.min_h(min_height * delta).max_h(max_height * delta)
                            },
                        ),
                )
                .top_fade(scroll, fade, px(THREAD_FADE))
                .bottom_fade(scroll, fade, px(THREAD_FADE)),
        )
        .children(ask.map(|ask| div().px(px(4.0)).pt(px(4.0)).child(ask)))
}

/// Where the driver runs, at the left of Driver mode's footer: its host's
/// status dot and name, then its tile. `on_click` opens its terminal.
pub fn driver_palette_place(
    host: impl Into<SharedString>,
    status: Hsla,
    program: Option<&'static Program>,
    on_click: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .id("driver-palette-place")
        .debug_selector(|| "driver-palette-place".to_owned())
        .flex_none()
        .min_w_0()
        .gap(px(6.0))
        .items_center()
        .cursor_pointer()
        .text_color(theme.foreground.muted())
        .hover(|place| place.text_color(theme.foreground))
        .child(crate::navigation::status_dot(status))
        .child(host.into())
        .children(program.map(|program| {
            program_tile(program, px(SEEN_TILE), px(SEEN_TILE), 0.0, cx).flex_none()
        }))
        .child("Driver")
        .on_click(on_click)
        .into_any_element()
}

/// The chat's button in the title bar, with a dot in the color of what waits
/// in the chat while it is closed.
pub fn chat_button(button: impl IntoElement, unread: Option<Hsla>) -> zz_gpui::Div {
    div()
        .relative()
        .flex_none()
        .child(button)
        .when_some(unread, |this, color| {
            this.child(
                div()
                    .debug_selector(|| "chat-unread".to_owned())
                    .absolute()
                    .top(px(3.0))
                    .right(px(3.0))
                    .child(crate::navigation::status_dot(color)),
            )
        })
}

pub fn phone_chat_place(
    host: impl Into<SharedString>,
    program: Option<&'static Program>,
    on_click: impl Fn(&zz_gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id("phone-chat-place")
        .debug_selector(|| "phone-chat-place".to_owned())
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(px(PHONE_TITLE_SIZE))
                .line_height(px(PHONE_TITLE_LINE))
                .font_bold()
                .text_color(theme.foreground)
                .child("Driver"),
        )
        .child(
            h_flex()
                .gap(px(5.0))
                .items_center()
                .text_size(crate::rems_from_px(PHONE_PLACE_SIZE))
                .text_color(theme.foreground.muted())
                .child(host.into())
                .children(program.map(|program| {
                    program_tile(program, px(SEEN_TILE), px(SEEN_TILE), 0.0, cx).flex_none()
                }))
                .child(Icon::new(IconName::ChevronRight).size(crate::rems_from_px(12.0))),
        )
        .on_click(on_click)
        .into_any_element()
}

pub fn phone_chat_card(
    id: impl Into<ElementId>,
    header: impl IntoElement,
    message: impl IntoElement,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let layer = cx.theme().layer(roles::PHONE_DRIVER_CARD, PaintState::Rest);
    v_flex()
        .id(id)
        .debug_selector(|| "phone-chat-card".to_owned())
        .w_full()
        .overflow_hidden()
        .cursor_pointer()
        .layer(&layer)
        .rounded(px(PHONE_CARD_RADIUS))
        .child(header)
        .child(
            div()
                .px(px(CHAT_PADDING_X))
                .pb(px(CHAT_PADDING_BOTTOM))
                .child(message),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[zz_gpui::test]
    fn yours_sit_right_with_ticks_and_theirs_sit_left_with_none(cx: &mut zz_gpui::TestAppContext) {
        struct Thread;

        impl zz_gpui::Render for Thread {
            fn render(
                &mut self,
                _: &mut Window,
                _: &mut zz_gpui::Context<Self>,
            ) -> impl IntoElement {
                div().w(px(320.0)).child(chat_thread([
                    ChatBubble::mine("mine", "morning", "9:40")
                        .receipt(Receipt::Read)
                        .into_any_element(),
                    ChatBubble::theirs("theirs", "Three agents are working.", "9:40")
                        .receipt(Receipt::Read)
                        .into_any_element(),
                ]))
            }
        }

        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| Thread);
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let mine = cx.debug_bounds("chat-mine").unwrap();
        let theirs = cx.debug_bounds("chat-theirs").unwrap();
        assert_eq!(mine.right(), px(320.0 - CHAT_PADDING_X));
        assert_eq!(theirs.left(), px(CHAT_PADDING_X));
        assert!(cx.debug_bounds("chat-receipt-Read").is_some());
        assert_eq!(
            cx.debug_bounds("chat-receipt-Read").unwrap().right(),
            mine.right() - px(STAMP_RIGHT)
        );
    }

    #[zz_gpui::test]
    fn a_bubble_cut_to_lines_stops_growing_there(cx: &mut zz_gpui::TestAppContext) {
        struct Latest;

        impl zz_gpui::Render for Latest {
            fn render(
                &mut self,
                _: &mut Window,
                _: &mut zz_gpui::Context<Self>,
            ) -> impl IntoElement {
                div().w(px(200.0)).child(
                    ChatBubble::theirs("latest", "Three agents are working. ".repeat(12), "")
                        .lines(2),
                )
            }
        }

        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| Latest);
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let bubble = cx.debug_bounds("chat-theirs").unwrap();
        assert_eq!(
            bubble.size.height,
            px(2.0 * TEXT_LINE + BUBBLE_PADDING_TOP + BUBBLE_PADDING_BOTTOM)
        );
    }

    #[test]
    fn only_markdown_beyond_plain_words_takes_the_renderer() {
        assert!(!is_rich("Three agents are working."));
        assert!(!is_rich("line one\nline two"));
        assert!(is_rich("Three agents:\n\n- zz\n- atlas"));
        assert!(is_rich("the **sprite** sheet"));
        assert!(is_rich("run `just ios`"));
        assert_eq!(
            plain_text("Three agents:\n\n- **zz** waits\n- `atlas` is green"),
            "Three agents:\nzz waits\natlas is green"
        );
    }
}
