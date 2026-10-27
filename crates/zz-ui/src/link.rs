//! zz links: a Markdown link whose destination is a target,
//! `[the build](@xps:work:2)` or `[main.rs:42](@xps:~/dev/zz/src/main.rs:42)`,
//! and a bare `@xps:work:2` in prose, which [`autolink`] turns into one. A
//! link reads the live fleet: it underlines while what it names is there,
//! mutes while its host is offline and strikes through once it is gone.
//! Hovering shows a card, a click goes there, a click with the platform key
//! mentions it in the composer, and a right click copies it.

use std::{ops::Range, sync::Arc};

use zz_gpui::{
    Action, AnyView, App, AppContext as _, ClipboardItem, Context, HighlightStyle, Hsla,
    IntoElement, MouseButton, ParentElement as _, Render, SharedString, StrikethroughStyle,
    Styled as _, UnderlineStyle, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Icon, IconName, PaintState, StyledExt as _, SurfaceRole,
    command::{PaletteHostId, PaletteStatus, PaletteTarget},
    h_flex,
    kbd::key_icon,
    picker::muted_on,
    target::{
        TargetHost, TargetKind, count, find_session, find_window, pane_detail, session_ref,
        unescape,
    },
    text::{LinkClick, TextLinks},
    v_flex,
};

const CARD_WIDTH: f32 = 280.0;
const CARD_ICON: f32 = 14.0;
const CARD_DOT: f32 = 6.0;
const MENTION_KEY: &str = if cfg!(any(target_os = "macos", target_os = "ios")) {
    "cmd"
} else {
    "ctrl"
};

/// Go to what a zz link names.
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = zz_link, no_json)]
pub struct OpenLink {
    pub address: SharedString,
}

/// Mention what a zz link names in the composer.
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = zz_link, no_json)]
pub struct MentionLink {
    pub address: SharedString,
}

/// Whether what a link names is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Live,
    /// Its host is offline or still connecting, so nothing under it can be
    /// told apart from gone.
    Offline,
    Gone,
}

/// What a zz link names, read against the fleet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetLink {
    pub kind: TargetKind,
    pub host: String,
    pub host_id: PaletteHostId,
    pub host_status: PaletteStatus,
    pub state: LinkState,
    pub target: Option<PaletteTarget>,
    /// The file or folder as written, `~` kept.
    pub path: Option<String>,
    pub line: Option<u32>,
    /// What the card leads with: the target without its host.
    pub name: String,
    pub detail: String,
    pub icon: IconName,
}

impl TargetLink {
    fn new(host: &TargetHost, kind: TargetKind, name: impl Into<String>) -> Self {
        let state = match host.status {
            PaletteStatus::Online | PaletteStatus::Running => LinkState::Live,
            PaletteStatus::Waiting | PaletteStatus::Offline => LinkState::Offline,
        };
        Self {
            kind,
            host: host.name.clone(),
            host_id: host.id,
            host_status: host.status,
            state,
            target: None,
            path: None,
            line: None,
            name: name.into(),
            detail: String::new(),
            icon: match kind {
                TargetKind::Host => IconName::HardDrive,
                TargetKind::Session | TargetKind::Dir => IconName::Folder,
                TargetKind::Window => IconName::PanelsTopLeft,
                TargetKind::Pane => IconName::SquareTerminal,
                TargetKind::File => IconName::File,
            },
        }
    }

    fn found(mut self, target: PaletteTarget, detail: String) -> Self {
        self.target = Some(target);
        self.detail = detail;
        self
    }

    /// What a live host lacks: the session, window or pane is gone.
    fn gone(mut self) -> Self {
        if self.state == LinkState::Live {
            self.state = LinkState::Gone;
            self.detail = format!("Not on {} any more", self.host);
        }
        self
    }

    /// The path a click pastes: plain on the host you are on, in scp form on
    /// another, a backslash before each space.
    #[must_use]
    pub fn paste_text(&self, here: bool) -> Option<String> {
        let path = crate::target::escape(self.path.as_deref()?);
        Some(if here {
            format!("{path} ")
        } else {
            format!("{}:{path} ", self.host)
        })
    }
}

/// Read `address`, `@` and all, against `hosts`. `None` when it is not a
/// target: no `@`, or no host to read it on.
#[must_use]
pub fn resolve_link(address: &str, hosts: &[TargetHost]) -> Option<TargetLink> {
    let body = address.strip_prefix('@')?;
    if body.is_empty() || body.contains(char::is_control) {
        return None;
    }
    let named = |name: &str| hosts.iter().find(|host| host.name == name);
    let here = || hosts.iter().find(|host| host.here);
    let (host, rest) = if body.starts_with(['/', '~']) {
        (here()?, body)
    } else if let Some(host) = named(body) {
        (host, "")
    } else {
        match body
            .split_once(':')
            .and_then(|(first, rest)| Some((named(first)?, rest)))
        {
            Some(written) => written,
            None => (here()?, body),
        }
    };
    if rest.is_empty() {
        let mut link = TargetLink::new(host, TargetKind::Host, host.name.clone());
        link.detail = match host.status {
            PaletteStatus::Online | PaletteStatus::Running => count(host.sessions.len(), "session"),
            PaletteStatus::Waiting => "Connecting".to_owned(),
            PaletteStatus::Offline => "Offline".to_owned(),
        };
        return Some(link);
    }
    if rest.starts_with(['/', '~']) {
        return Some(file_link(host, rest));
    }
    Some(fleet_link(host, rest))
}

fn file_link(host: &TargetHost, rest: &str) -> TargetLink {
    let (written, line) = split_line(rest);
    let path = unescape(written);
    let kind = if path == "~" || path.ends_with('/') {
        TargetKind::Dir
    } else {
        TargetKind::File
    };
    let leaf = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|leaf| !leaf.is_empty())
        .unwrap_or(&path);
    let name = match line {
        Some(line) => format!("{leaf}:{line}"),
        None => leaf.to_owned(),
    };
    let mut file = TargetLink::new(host, kind, name);
    file.detail = match line {
        Some(line) => format!("{path}, line {line}"),
        None => path.clone(),
    };
    file.path = Some(path);
    file.line = line;
    file
}

/// `path:42` or `path:42:7` as the path and its line.
fn split_line(rest: &str) -> (&str, Option<u32>) {
    let mut path = rest;
    let mut numbers = Vec::new();
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => {
                numbers.push(tail);
                path = head;
            }
            _ => break,
        }
    }
    (path, numbers.last().and_then(|line| line.parse().ok()))
}

fn fleet_link(host: &TargetHost, rest: &str) -> TargetLink {
    if rest.starts_with('%') {
        let link = TargetLink::new(host, TargetKind::Pane, rest);
        let found = host
            .sessions
            .iter()
            .flat_map(|session| {
                session.windows.iter().flat_map(move |window| {
                    window.panes.iter().map(move |pane| (session, window, pane))
                })
            })
            .find(|(.., pane)| pane.id.to_string() == rest);
        return match found {
            Some((session, window, pane)) => {
                let place = format!("{}:{}", session_ref(session), window.index);
                link.found(PaletteTarget::Pane(pane.id), pane_detail(pane, Some(place)))
            }
            None => link.gone(),
        };
    }
    if rest.starts_with('@') {
        let link = TargetLink::new(host, TargetKind::Window, rest);
        let found = host.sessions.iter().find_map(|session| {
            session
                .windows
                .iter()
                .find(|window| window.id.to_string() == rest)
                .map(|window| (session, window))
        });
        return match found {
            Some((session, window)) => {
                let detail = format!(
                    "{}:{} · {} · {}",
                    session_ref(session),
                    window.index,
                    window.name,
                    count(window.panes.len(), "pane")
                );
                link.found(PaletteTarget::Window(window.id), detail)
            }
            None => link.gone(),
        };
    }
    let (session_text, window_text) = match rest.split_once(':') {
        Some((session, window)) => (session, Some(window)),
        None => (rest, None),
    };
    let (window_text, pane_text) = match window_text.map(|window| window.split_once('.')) {
        Some(Some((window, pane))) => (Some(window), Some(pane)),
        Some(None) => (window_text, None),
        None => (None, None),
    };
    let kind = match (window_text, pane_text) {
        (_, Some(_)) => TargetKind::Pane,
        (Some(_), None) => TargetKind::Window,
        (None, None) => TargetKind::Session,
    };
    let link = TargetLink::new(host, kind, unescape(rest));
    let Some(session) = find_session(host, &unescape(session_text)) else {
        return link.gone();
    };
    let Some(window_text) = window_text else {
        let detail = count(session.windows.len(), "window");
        return link.found(PaletteTarget::Session(session.id), detail);
    };
    let Some(window) = find_window(session, &unescape(window_text)) else {
        return link.gone();
    };
    let Some(pane_text) = pane_text else {
        let detail = format!("{} · {}", window.name, count(window.panes.len(), "pane"));
        return link.found(PaletteTarget::Window(window.id), detail);
    };
    let pane = window
        .panes
        .iter()
        .find(|pane| pane.id.to_string() == pane_text)
        .or_else(|| {
            pane_text
                .parse::<usize>()
                .ok()
                .and_then(|index| window.panes.get(index))
        });
    match pane {
        Some(pane) => {
            let place = format!("{}:{}", session_ref(session), window.index);
            link.found(PaletteTarget::Pane(pane.id), pane_detail(pane, Some(place)))
        }
        None => link.gone(),
    }
}

/// `text` with every bare `@target` outside code and links made a link, or
/// `None` when it has none. A mention is linked when its host is written or
/// what it names is there, so `@here` in a sentence stays a word.
#[must_use]
pub fn autolink(text: &str, hosts: &[TargetHost]) -> Option<String> {
    if !text.contains('@') {
        return None;
    }
    let root = markdown::to_mdast(text, &markdown::ParseOptions::gfm()).ok()?;
    let mut spans = Vec::new();
    prose_spans(&root, &mut spans);
    let mentions: Vec<Range<usize>> = spans
        .into_iter()
        .flat_map(|span| {
            mentions(text, span)
                .into_iter()
                .filter(|range| linkable(&text[range.clone()], hosts))
        })
        .collect();
    (!mentions.is_empty()).then(|| linked(text, &mentions, str::to_owned))
}

/// `text` as Markdown that draws it as typed, bare mentions made links, or
/// `None` when it has none. Your messages are plain text, so everything else
/// is escaped and each line break kept.
#[must_use]
pub fn autolink_plain(text: &str, hosts: &[TargetHost]) -> Option<String> {
    if !text.contains('@') {
        return None;
    }
    let mentions: Vec<Range<usize>> = mentions(text, 0..text.len())
        .into_iter()
        .filter(|range| linkable(&text[range.clone()], hosts))
        .collect();
    (!mentions.is_empty()).then(|| linked(text, &mentions, escape_plain))
}

fn linkable(mention: &str, hosts: &[TargetHost]) -> bool {
    let Some(link) = resolve_link(mention, hosts) else {
        return false;
    };
    let written = mention[1..]
        .split(':')
        .next()
        .is_some_and(|first| hosts.iter().any(|host| host.name == first));
    written || link.state == LinkState::Live || link.kind == TargetKind::File
}

fn linked(text: &str, mentions: &[Range<usize>], between: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len() + mentions.len() * 16);
    let mut last = 0;
    for range in mentions {
        out.push_str(&between(&text[last..range.start]));
        let mention = &text[range.clone()];
        out.push('[');
        out.push_str(&escape_plain(mention));
        out.push_str("](<");
        out.push_str(mention);
        out.push_str(">)");
        last = range.end;
    }
    out.push_str(&between(&text[last..]));
    out
}

/// Plain text as Markdown that draws it as is: punctuation escaped, a hard
/// break at each line end, and a line's leading spaces kept.
fn escape_plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut line_start = true;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' if characters.peek().is_some_and(|next| *next != '\n') && !out.ends_with('\n') => {
                out.push_str("\\\n");
                line_start = true;
            }
            '\n' => {
                out.push('\n');
                line_start = true;
            }
            ' ' if line_start => out.push_str("&#32;"),
            character if character.is_ascii_punctuation() => {
                out.push('\\');
                out.push(character);
                line_start = false;
            }
            character => {
                out.push(character);
                line_start = false;
            }
        }
    }
    out
}

/// The byte ranges of prose in `node`: its text, outside code, links and
/// raw HTML.
fn prose_spans(node: &markdown::mdast::Node, spans: &mut Vec<Range<usize>>) {
    use markdown::mdast::Node;
    match node {
        Node::Text(text) => spans.extend(
            text.position
                .as_ref()
                .map(|position| position.start.offset..position.end.offset),
        ),
        Node::Link(_)
        | Node::LinkReference(_)
        | Node::InlineCode(_)
        | Node::Code(_)
        | Node::Html(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::Definition(_) => {}
        node => {
            for child in node.children().into_iter().flatten() {
                prose_spans(child, spans);
            }
        }
    }
}

/// The `@` words in `span` of `text`: an `@` no letter, digit or path
/// character sits before, up to the next space or bracket, without the
/// sentence's last `.`, `:` or `,`.
fn mentions(text: &str, span: Range<usize>) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let slice = &text[span.clone()];
    let mut skip_to = 0;
    for (offset, character) in slice.char_indices() {
        if character != '@' || offset < skip_to {
            continue;
        }
        let start = span.start + offset;
        let before = text[..start].chars().next_back();
        if before.is_some_and(|before| before.is_alphanumeric() || "@._-/\\:%$~".contains(before)) {
            continue;
        }
        let word = slice[offset..]
            .find(|character: char| {
                character.is_whitespace() || "()[]{}<>,;!?\"'`*|".contains(character)
            })
            .map_or(slice.len(), |end| offset + end);
        let trimmed = slice[offset..word].trim_end_matches(['.', ':']);
        skip_to = word;
        if trimmed.len() > 1 {
            found.push(start..start + trimmed.len());
        }
    }
    found
}

/// The paint a link takes on a bubble: `live` while what it names is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkInk {
    pub live: Hsla,
    pub muted: Hsla,
}

/// zz links for a [`crate::text::TextView`], read against `hosts`.
#[derive(Clone)]
pub struct TargetLinks {
    hosts: Arc<[TargetHost]>,
    ink: LinkInk,
}

impl TargetLinks {
    #[must_use]
    pub fn new(hosts: Arc<[TargetHost]>, ink: LinkInk) -> Self {
        Self { hosts, ink }
    }

    #[must_use]
    pub fn hosts(&self) -> &[TargetHost] {
        &self.hosts
    }
}

impl TextLinks for TargetLinks {
    fn style(&self, url: &str, _: &App) -> Option<HighlightStyle> {
        let link = resolve_link(url, &self.hosts)?;
        let underline = |color| UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        };
        Some(match link.state {
            LinkState::Live => HighlightStyle {
                color: Some(self.ink.live),
                underline: Some(underline(self.ink.live)),
                ..HighlightStyle::default()
            },
            LinkState::Offline => HighlightStyle {
                color: Some(self.ink.muted),
                underline: Some(underline(self.ink.muted)),
                ..HighlightStyle::default()
            },
            LinkState::Gone => HighlightStyle {
                color: Some(self.ink.muted),
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(self.ink.muted),
                }),
                ..HighlightStyle::default()
            },
        })
    }

    fn tooltip(&self, url: &str, _: &mut Window, cx: &mut App) -> Option<AnyView> {
        let link = resolve_link(url, &self.hosts)?;
        Some(cx.new(|_| LinkCard { link }).into())
    }

    fn click(&self, click: &LinkClick, window: &mut Window, cx: &mut App) -> bool {
        if !click.url.starts_with('@') {
            return false;
        }
        let address = click.url.clone();
        match click.button {
            MouseButton::Right => {
                cx.write_to_clipboard(ClipboardItem::new_string(address.to_string()));
            }
            MouseButton::Left if click.modifiers.secondary() => {
                window.dispatch_action(Box::new(MentionLink { address }), cx);
            }
            MouseButton::Left => window.dispatch_action(Box::new(OpenLink { address }), cx),
            _ => return false,
        }
        true
    }
}

/// The card hovering a link shows: what it names, where, and whether it is
/// still there.
struct LinkCard {
    link: TargetLink,
}

impl Render for LinkCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let link = &self.link;
        let theme = cx.theme();
        let layer = theme.layer(SurfaceRole::TOOLTIP, PaintState::Rest);
        let muted = muted_on(&layer, cx);
        let title = layer.ground.ink;
        let gone = link.state == LinkState::Gone;
        let mention_key = match key_icon(MENTION_KEY) {
            Some(icon) => Icon::new(icon).size(px(CARD_ICON)).into_any_element(),
            None => MENTION_KEY.into_any_element(),
        };
        div().child(
            v_flex()
                .m_3()
                .w(px(CARD_WIDTH))
                .px_3()
                .py_2()
                .gap(px(4.0))
                .font_family(theme.font_family.clone())
                .text_xs()
                .surface(SurfaceRole::TOOLTIP, cx)
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .min_w_0()
                        .text_color(title)
                        .child(Icon::new(link.icon.clone()).size(px(CARD_ICON)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_weight(zz_gpui::FontWeight::MEDIUM)
                                .child(link.name.clone()),
                        )
                        .child(div().text_color(muted).child(link.kind.label())),
                )
                .when(!link.detail.is_empty(), |card| {
                    card.child(
                        div()
                            .text_color(if gone { theme.danger } else { muted })
                            .child(link.detail.clone()),
                    )
                })
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .text_color(muted)
                        .child(
                            div()
                                .size(px(CARD_DOT))
                                .rounded_full()
                                .bg(link.host_status.color(cx)),
                        )
                        .child(format!("{} · {}", link.host, status_word(link.host_status))),
                )
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .pt(px(2.0))
                        .text_color(muted)
                        .when(!gone, |hints| hints.child("Click to go ·"))
                        .child(mention_key)
                        .child("click to mention"),
                ),
        )
    }
}

const fn status_word(status: PaletteStatus) -> &'static str {
    match status {
        PaletteStatus::Online | PaletteStatus::Running => "online",
        PaletteStatus::Waiting => "connecting",
        PaletteStatus::Offline => "offline",
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
