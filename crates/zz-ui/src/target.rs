use std::{cmp::Reverse, ops::Range, rc::Rc, sync::Arc};

use ala::{
    Action, AnyElement, App, ClickEvent, ClipboardItem, Context, ElementId, Entity, Focusable as _,
    InteractiveElement, IntoElement, MouseButton, Pixels, Point, Render, ScrollStrategy,
    SharedString, Subscription, Task, UniformListScrollHandle, Window, canvas, div, prelude::*, px,
    uniform_list,
};
use zz_client::{
    path_rank,
    pick::{self, Candidate as PickCandidate, Closeness, Kind as PickKind, Prior, Query},
    recents::{MAX_BOOST, RecentKey, Recents, now_secs},
};
use zz_protocol::{GitMark, PaneId, PathEntry, PathKind, PathListRoot, SessionId, WindowId};

use penna::{
    Ground, Panel, PanelLook, TextStyle,
    blocks::chat::composer_add_button,
    components::{button::Button, popover::PopoverTrigger as _},
};

use penna::blocks::palette::{
    PaletteKeys, PaletteRow, PaletteStatus, Twin, palette_chord, palette_footer, palette_row,
    palette_row_height, palette_section,
};

use crate::{
    ActiveTheme as _, Colorize as _, IconName, StyledExt as _,
    command::{CommandPaletteView, PaletteHostId, PaletteTarget, palette_recents, record_recent},
    files::{
        FileRow, GitMarks, Listing, ListingUpdate, Recency, file_rows, mark_letter, plain_path,
        recency, scored_rows,
    },
    h_flex,
    input::{Backspace, Enter, Escape, IndentInline, InputState, MoveDown, MoveUp, OutdentInline},
    mention::{Mentions, ref_highlight},
};

pub const TARGET_MAX_ROWS: usize = 200;
pub const TARGET_VISIBLE_ROWS: usize = 8;
const HEADER_HEIGHT: f32 = 28.0;
const BOTTOM_PADDING: f32 = 6.0;
const CHROME_HEIGHT: f32 =
    HEADER_HEIGHT + 2.0 * BOTTOM_PADDING + 2.0 + penna::blocks::palette::PALETTE_FOOTER_HEIGHT;
const ROOM_MARGIN: f32 = 8.0;
const CHANGED_ROWS: usize = 5;

pub trait TargetBackend {
    fn hosts(&self, cx: &App) -> Vec<TargetHost>;
    fn list(
        &self,
        host: &str,
        dir: &str,
        anchor: Option<PaneId>,
        cx: &mut App,
    ) -> Result<u64, SharedString>;
    fn cancel(&self, host: &str, request_id: u64, cx: &mut App);
    fn folder(&self, _pane: PaneId, _cx: &App) -> Option<String> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetHost {
    pub id: PaletteHostId,
    pub name: String,
    pub status: PaletteStatus,
    pub here: bool,
    pub sessions: Vec<TargetSession>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetSession {
    pub id: SessionId,
    pub name: String,
    pub windows: Vec<TargetWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetWindow {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    pub panes: Vec<TargetPane>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetPane {
    pub id: PaneId,
    pub title: String,
    pub program: Option<SharedString>,
    pub icon: IconName,
    pub chat: Option<TargetChat>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetChat {
    pub title: SharedString,
    pub icon: IconName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Host,
    Session,
    Window,
    Pane,
    Chat,
    Dir,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetToken {
    pub range: Range<usize>,
    pub body: String,
}

#[must_use]
pub fn target_at(text: &str, cursor: usize) -> Option<TargetToken> {
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let mut start = 0;
    let mut escaped = false;
    for (index, character) in text[..cursor].char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character.is_whitespace() {
            start = index + character.len_utf8();
        }
    }
    if start >= cursor || !text[start..].starts_with('@') {
        return None;
    }
    let mut end = text.len();
    let mut escaped = false;
    for (index, character) in text[start..].char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character.is_whitespace() {
            end = start + index;
            break;
        }
    }
    Some(TargetToken {
        range: start..end,
        body: text[start + 1..cursor].to_owned(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetLevel {
    Top,
    Sessions {
        host: String,
    },
    Windows {
        host: String,
        session: String,
    },
    Panes {
        host: String,
        session: String,
        window: String,
    },
    Files {
        host: String,
        dir: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetQuery {
    pub level: TargetLevel,
    pub filter: String,
    pub written: bool,
}

#[must_use]
pub fn parse_target(body: &str, hosts: &[TargetHost]) -> TargetQuery {
    let here = hosts
        .iter()
        .find(|host| host.here)
        .map_or_else(String::new, |host| host.name.clone());
    let query = |level, filter: &str, written| TargetQuery {
        level,
        filter: filter.to_owned(),
        written,
    };
    if body.starts_with(['/', '~']) {
        return files(here, body, false);
    }
    let Some((first, rest)) = body.split_once(':') else {
        return query(TargetLevel::Top, body, false);
    };
    if !hosts.iter().any(|host| host.name == first) {
        let session = first.to_owned();
        return match rest.split_once('.') {
            Some((window, filter)) => query(
                TargetLevel::Panes {
                    host: here,
                    session,
                    window: window.to_owned(),
                },
                filter,
                false,
            ),
            None => query(
                TargetLevel::Windows {
                    host: here,
                    session,
                },
                rest,
                false,
            ),
        };
    }
    let host = first.to_owned();
    if rest.starts_with(['/', '~']) {
        return files(host, rest, true);
    }
    let Some((session, rest)) = rest.split_once(':') else {
        return query(TargetLevel::Sessions { host }, rest, true);
    };
    let session = session.to_owned();
    match rest.split_once('.') {
        Some((window, filter)) => query(
            TargetLevel::Panes {
                host,
                session,
                window: window.to_owned(),
            },
            filter,
            true,
        ),
        None => query(TargetLevel::Windows { host, session }, rest, true),
    }
}

fn files(host: String, path: &str, written: bool) -> TargetQuery {
    let (dir, filter) = path
        .rfind('/')
        .map_or(("~/", &path[1..]), |cut| (&path[..=cut], &path[cut + 1..]));
    TargetQuery {
        level: TargetLevel::Files {
            host,
            dir: dir.to_owned(),
        },
        filter: unescape(filter),
        written,
    }
}

#[must_use]
pub fn pop_level(body: &str) -> Option<String> {
    let trimmed = body.strip_suffix([':', '.', '/'])?;
    let cut = trimmed
        .char_indices()
        .rev()
        .find(|(index, character)| {
            matches!(character, ':' | '.' | '/') && !trimmed[..*index].ends_with('\\')
        })
        .map_or(0, |(index, _)| index + 1);
    Some(trimmed[..cut].to_owned())
}

pub(crate) fn unescape(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        plain.push(if character == '\\' {
            characters.next().unwrap_or(character)
        } else {
            character
        });
    }
    plain
}

#[must_use]
pub fn escape(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for character in name.chars() {
        if character == '\\' || character.is_whitespace() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetItem {
    pub kind: TargetKind,
    pub label: SharedString,
    pub matches: Vec<Range<usize>>,
    pub detail: SharedString,
    pub status: Option<PaletteStatus>,
    pub mark: Option<GitMark>,
    pub icon: IconName,
    pub host: String,
    pub host_id: PaletteHostId,
    pub target: Option<PaletteTarget>,
    pub entry: Option<PathEntry>,
    pub typed: String,
    pub token: String,
    pub shown: Option<SharedString>,
    pub meta: SharedString,
    score: i64,
    recent: Option<RecentKey>,
}

impl TargetItem {
    fn new(
        kind: TargetKind,
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        place: &Place,
        rest: &str,
        icon: IconName,
    ) -> Self {
        let (typed, token) = place.target(rest);
        Self {
            kind,
            label: label.into(),
            matches: Vec::new(),
            detail: detail.into(),
            status: None,
            mark: None,
            icon,
            host: place.host.name.clone(),
            host_id: place.host.id,
            target: None,
            entry: None,
            typed,
            token,
            shown: None,
            meta: SharedString::default(),
            score: 0,
            recent: None,
        }
    }

    fn at(mut self, target: PaletteTarget) -> Self {
        self.target = Some(target);
        self
    }

    fn near(mut self, prior: Prior, recent: Option<RecentKey>) -> Self {
        self.score = prior.score();
        self.recent = recent;
        self
    }

    const fn pick_kind(&self) -> PickKind {
        match self.kind {
            TargetKind::File => PickKind::File,
            TargetKind::Dir => PickKind::Dir,
            TargetKind::Chat => PickKind::Chat,
            TargetKind::Host | TargetKind::Session | TargetKind::Window | TargetKind::Pane => {
                PickKind::Place
            }
        }
    }

    fn scored(mut self, query: &Query, path: &str) -> Option<Self> {
        let found = query.matched(&PickCandidate {
            kind: self.pick_kind(),
            label: &self.label,
            path,
            prior: Prior::default(),
        })?;
        self.score += found.score;
        self.matches = found.label;
        Some(self)
    }

    #[must_use]
    pub const fn recent_key(&self) -> Option<&RecentKey> {
        self.recent.as_ref()
    }

    #[must_use]
    pub const fn drills(&self) -> bool {
        matches!(
            self.kind,
            TargetKind::Host | TargetKind::Session | TargetKind::Window | TargetKind::Dir
        )
    }

    #[must_use]
    pub fn completion(&self) -> String {
        let next = match self.kind {
            TargetKind::Host | TargetKind::Session => ":",
            TargetKind::Window => ".",
            TargetKind::Dir if !self.typed.ends_with('/') => "/",
            TargetKind::Dir | TargetKind::Pane | TargetKind::Chat | TargetKind::File => "",
        };
        format!("{}{next}", self.typed)
    }

    #[must_use]
    pub fn name(&self) -> SharedString {
        self.shown.clone().unwrap_or_else(|| self.label.clone())
    }

    #[must_use]
    pub fn reference(&self) -> String {
        format!("@{}", self.token)
    }

    #[must_use]
    pub fn insertion(&self) -> String {
        format!("@{} ", self.token)
    }
}

struct Place<'a> {
    host: &'a TargetHost,
    written: bool,
}

impl Place<'_> {
    fn target(&self, rest: &str) -> (String, String) {
        let token = if rest.is_empty() {
            self.host.name.clone()
        } else {
            format!("{}:{rest}", self.host.name)
        };
        let typed = if self.written || !self.host.here || rest.is_empty() {
            token.clone()
        } else {
            rest.to_owned()
        };
        (typed, token)
    }
}

struct FileQuery {
    chunks: Vec<Arc<[PathEntry]>>,
    marks: Arc<GitMarks>,
    base: String,
    filter: String,
    host: TargetHost,
    written: bool,
    dir: String,
    root: Option<String>,
    recent: Arc<Recency>,
    top: bool,
}

fn file_item(query: &FileQuery, place: &Place, row: FileRow, score: i64) -> TargetItem {
    let FileRow {
        entry,
        name,
        matches,
        mark,
    } = row;
    let (kind, icon) = match entry.kind {
        PathKind::Dir => (TargetKind::Dir, IconName::Folder),
        PathKind::File => (TargetKind::File, IconName::File),
    };
    let slash = if kind == TargetKind::Dir { "/" } else { "" };
    let rest = format!("{}{}{slash}", query.dir, escape(&name));
    let mut item = TargetItem::new(kind, name.clone(), "", place, &rest, icon);
    item.matches = matches;
    if query.top {
        let (parent, label) = path_rank::split(&name);
        let cut = parent.len();
        item.label = label.to_owned().into();
        item.detail = parent.trim_end_matches('/').to_owned().into();
        item.matches = item
            .matches
            .iter()
            .filter(|range| range.end > cut)
            .map(|range| range.start.max(cut) - cut..range.end - cut)
            .collect();
    }
    item.score = score;
    item.mark = mark;
    item.meta = mark.map(mark_letter).unwrap_or_default().into();
    item.recent = query.root.as_ref().map(|root| RecentKey::File {
        host: query.host.name.clone(),
        path: plain_path(root, &entry.rel, None),
        dir: kind == TargetKind::Dir,
    });
    item.entry = Some(entry);
    item
}

fn file_items(query: &FileQuery) -> Vec<TargetItem> {
    let place = Place {
        host: &query.host,
        written: query.written,
    };
    if query.top {
        return scored_rows(
            &query.chunks,
            &query.marks,
            &query.base,
            &query.filter,
            &query.recent,
        )
        .into_iter()
        .map(|(row, score)| file_item(query, &place, row, score))
        .collect();
    }
    file_rows(
        &query.chunks,
        &query.marks,
        &query.base,
        &query.filter,
        &query.recent,
    )
    .into_iter()
    .map(|row| file_item(query, &place, row, 0))
    .collect()
}

fn changed_items(query: &FileQuery) -> Vec<TargetItem> {
    let place = Place {
        host: &query.host,
        written: query.written,
    };
    let mut changed = query
        .chunks
        .iter()
        .flat_map(|chunk| chunk.iter())
        .filter(|entry| {
            entry.kind == PathKind::File
                && entry.rel.len() > query.base.len()
                && entry.rel.starts_with(&query.base)
        })
        .filter_map(|entry| Some((entry, query.marks.get(&entry.rel)?)))
        .collect::<Vec<_>>();
    changed.sort_by(|(left, _), (right, _)| left.rel.cmp(&right.rel));
    changed
        .into_iter()
        .take(CHANGED_ROWS)
        .map(|(entry, mark)| {
            let row = FileRow {
                entry: entry.clone(),
                name: entry.rel[query.base.len()..].to_owned(),
                matches: Vec::new(),
                mark: Some(mark),
            };
            file_item(query, &place, row, 0)
        })
        .collect()
}

pub(crate) fn count(value: usize, noun: &str) -> String {
    format!("{value} {noun}{}", if value == 1 { "" } else { "s" })
}

pub(crate) fn session_ref(session: &TargetSession) -> String {
    let plain = !session.name.is_empty()
        && !session.name.starts_with(['$', '@', '%', '~', '/'])
        && !session
            .name
            .contains(|character: char| character.is_whitespace() || ":.\\".contains(character));
    if plain {
        session.name.clone()
    } else {
        session.id.to_string()
    }
}

pub(crate) fn find_session<'a>(host: &'a TargetHost, text: &str) -> Option<&'a TargetSession> {
    host.sessions
        .iter()
        .find(|session| session.name == text)
        .or_else(|| {
            host.sessions
                .iter()
                .find(|session| session.id.to_string() == text)
        })
}

pub(crate) fn find_window<'a>(session: &'a TargetSession, text: &str) -> Option<&'a TargetWindow> {
    session
        .windows
        .iter()
        .find(|window| window.index.to_string() == text || window.id.to_string() == text)
        .or_else(|| session.windows.iter().find(|window| window.name == text))
}

fn pane_name(pane: &TargetPane) -> String {
    let title = pane.title.trim();
    pane.program
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| (!title.is_empty()).then(|| title.to_owned()))
        .unwrap_or_else(|| pane.id.to_string())
}

fn pane_note(pane: &TargetPane, place: Option<String>) -> String {
    let title = pane.title.trim();
    let named = pane.program.is_some() || !title.is_empty();
    (pane.program.is_some() && !title.is_empty())
        .then(|| title.to_owned())
        .into_iter()
        .chain(place)
        .chain(named.then(|| pane.id.to_string()))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefTarget {
    Host,
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
    Path { path: String, line: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub kind: TargetKind,
    pub host: PaletteHostId,
    pub here: bool,
    pub label: String,
    pub icon: IconName,
    pub target: RefTarget,
}

#[must_use]
pub fn resolve_target(body: &str, hosts: &[TargetHost]) -> Option<ResolvedTarget> {
    let body = unescape(body);
    let (host, rest) = match body.split_once(':') {
        Some((first, rest)) => match hosts.iter().find(|host| host.name == first) {
            Some(host) => (host, rest),
            None => (hosts.iter().find(|host| host.here)?, body.as_str()),
        },
        None => match hosts.iter().find(|host| host.name == body) {
            Some(host) => (host, ""),
            None => (hosts.iter().find(|host| host.here)?, body.as_str()),
        },
    };
    let resolved = |kind, label: String, icon, target| {
        let label = if host.here {
            label
        } else {
            format!("{label} · {}", host.name)
        };
        Some(ResolvedTarget {
            kind,
            host: host.id,
            here: host.here,
            label,
            icon,
            target,
        })
    };
    if rest.is_empty() {
        return resolved(
            TargetKind::Host,
            host.name.clone(),
            host_icon(host),
            RefTarget::Host,
        );
    }
    if rest.starts_with(['/', '~']) || drive_path(rest) {
        let (path, line) = split_line(rest);
        let dir = path.ends_with('/');
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path);
        let name = if name.is_empty() || name == "~" {
            path.to_owned()
        } else if dir {
            format!("{name}/")
        } else {
            name.to_owned()
        };
        let label = line.map_or_else(|| name.clone(), |line| format!("{name}:{line}"));
        let (kind, icon) = if dir {
            (TargetKind::Dir, IconName::Folder)
        } else {
            (TargetKind::File, IconName::File)
        };
        return resolved(
            kind,
            label,
            icon,
            RefTarget::Path {
                path: path.to_owned(),
                line,
            },
        );
    }
    if rest.starts_with('%') {
        let (session, window, pane) = host.sessions.iter().find_map(|session| {
            session.windows.iter().find_map(|window| {
                window
                    .panes
                    .iter()
                    .find(|pane| pane.id.to_string() == rest)
                    .map(|pane| (session, window, pane))
            })
        })?;
        return pane_ref(session, window, pane, resolved);
    }
    let (session, rest) = rest
        .split_once(':')
        .map_or((rest, None), |(session, rest)| (session, Some(rest)));
    let session = find_session(host, session)?;
    let Some(rest) = rest else {
        return resolved(
            TargetKind::Session,
            session.name.clone(),
            IconName::Layers,
            RefTarget::Session(session.id),
        );
    };
    let (window, pane) = rest
        .split_once('.')
        .map_or((rest, None), |(window, pane)| (window, Some(pane)));
    let window = find_window(session, window)?;
    let Some(pane) = pane else {
        return resolved(
            TargetKind::Window,
            format!("{} › {}", session.name, window.name),
            IconName::AppWindow,
            RefTarget::Window(window.id),
        );
    };
    let pane = window
        .panes
        .iter()
        .find(|candidate| candidate.id.to_string() == pane)?;
    pane_ref(session, window, pane, resolved)
}

fn pane_ref(
    session: &TargetSession,
    window: &TargetWindow,
    pane: &TargetPane,
    resolved: impl Fn(TargetKind, String, IconName, RefTarget) -> Option<ResolvedTarget>,
) -> Option<ResolvedTarget> {
    if let Some(chat) = &pane.chat {
        return resolved(
            TargetKind::Chat,
            chat.title.to_string(),
            chat.icon.clone(),
            RefTarget::Pane(pane.id),
        );
    }
    resolved(
        TargetKind::Pane,
        format!("{} · {}:{}", pane_name(pane), session.name, window.index),
        pane.icon.clone(),
        RefTarget::Pane(pane.id),
    )
}

pub(crate) fn drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn split_line(path: &str) -> (&str, Option<u32>) {
    let mut rest = path;
    let mut line = None;
    for _ in 0..2 {
        let Some((head, tail)) = rest.rsplit_once(':') else {
            break;
        };
        let Ok(number) = tail.parse::<u32>() else {
            break;
        };
        line = Some(number);
        rest = head;
    }
    (rest, line)
}

fn host_icon(host: &TargetHost) -> IconName {
    if host.here {
        IconName::Laptop
    } else {
        IconName::Server
    }
}

#[derive(Clone, Default)]
pub(crate) struct Around {
    pub(crate) own: Option<PaneId>,
    pub(crate) folder: Option<String>,
    pub(crate) recents: Recents,
    pub(crate) now: u64,
}

struct Near<'a> {
    around: &'a Around,
    session: Option<SessionId>,
    window: Option<WindowId>,
}

impl Around {
    fn near(&self, hosts: &[TargetHost]) -> Near<'_> {
        let spot = self.own.and_then(|own| {
            hosts
                .iter()
                .filter(|host| host.here)
                .flat_map(|host| &host.sessions)
                .find_map(|session| {
                    session
                        .windows
                        .iter()
                        .find(|window| window.panes.iter().any(|pane| pane.id == own))
                        .map(|window| (session.id, window.id))
                })
        });
        Near {
            around: self,
            session: spot.map(|(session, _)| session),
            window: spot.map(|(_, window)| window),
        }
    }
}

impl Near<'_> {
    fn prior(
        &self,
        host: &TargetHost,
        session: Option<SessionId>,
        window: Option<WindowId>,
        depth: u32,
        key: Option<&RecentKey>,
    ) -> Prior {
        let closeness = if !host.here {
            Closeness::Elsewhere
        } else if window.is_some() && window == self.window {
            Closeness::Window
        } else if session.is_some() && session == self.session {
            Closeness::Session
        } else {
            Closeness::Host
        };
        let recent = key.map_or(0.0, |key| {
            self.around.recents.boost(key, self.around.now) as f32 / MAX_BOOST as f32
        });
        Prior {
            closeness,
            recent,
            depth,
            changed: false,
        }
    }
}

fn place_key(
    host: &TargetHost,
    session: SessionId,
    window: Option<WindowId>,
    pane: Option<PaneId>,
) -> RecentKey {
    RecentKey::Place {
        host: host.name.clone(),
        session,
        window,
        pane,
    }
}

fn host_item(host: &TargetHost, place: &Place, near: &Near) -> TargetItem {
    let detail = match host.status {
        PaletteStatus::Online | PaletteStatus::Running => count(host.sessions.len(), "session"),
        PaletteStatus::Waiting => "connecting".to_owned(),
        PaletteStatus::Offline => "offline".to_owned(),
    };
    let mut item = TargetItem::new(
        TargetKind::Host,
        host.name.clone(),
        "",
        place,
        "",
        host_icon(host),
    )
    .near(near.prior(host, None, None, 0, None), None);
    item.status = Some(host.status);
    item.meta = detail.into();
    item
}

fn session_item(
    host: &TargetHost,
    session: &TargetSession,
    place: &Place,
    label: String,
    near: &Near,
) -> TargetItem {
    let key = place_key(host, session.id, None, None);
    let prior = near.prior(host, Some(session.id), None, 1, Some(&key));
    let mut item = TargetItem::new(
        TargetKind::Session,
        label,
        "",
        place,
        &session_ref(session),
        IconName::Layers,
    )
    .at(PaletteTarget::Session(session.id))
    .near(prior, Some(key));
    item.meta = count(session.windows.len(), "window").into();
    item
}

fn window_item(
    host: &TargetHost,
    session: &TargetSession,
    window: &TargetWindow,
    place: &Place,
    label: String,
    near: &Near,
) -> TargetItem {
    let key = place_key(host, session.id, Some(window.id), None);
    let prior = near.prior(host, Some(session.id), Some(window.id), 2, Some(&key));
    let mut item = TargetItem::new(
        TargetKind::Window,
        label,
        "",
        place,
        &format!("{}:{}", session_ref(session), window.index),
        IconName::AppWindow,
    )
    .at(PaletteTarget::Window(window.id))
    .near(prior, Some(key));
    item.meta = count(window.panes.len(), "pane").into();
    item
}

#[derive(Clone, Copy)]
struct Spot<'a> {
    host: &'a TargetHost,
    session: &'a TargetSession,
    window: &'a TargetWindow,
}

impl Spot<'_> {
    fn crumbs(&self, pane: &TargetPane) -> String {
        let title = pane.title.trim();
        [self.session.name.as_str(), self.window.name.as_str()]
            .into_iter()
            .chain((pane.program.is_some() && !title.is_empty()).then_some(title))
            .collect::<Vec<_>>()
            .join(" › ")
    }
}

fn pane_item(
    spot: &Spot,
    pane: &TargetPane,
    place: &Place,
    rest: &str,
    where_: Option<String>,
    near: &Near,
) -> TargetItem {
    let key = place_key(
        spot.host,
        spot.session.id,
        Some(spot.window.id),
        Some(pane.id),
    );
    let prior = near.prior(
        spot.host,
        Some(spot.session.id),
        Some(spot.window.id),
        3,
        Some(&key),
    );
    TargetItem::new(
        TargetKind::Pane,
        pane_name(pane),
        pane_note(pane, where_),
        place,
        rest,
        pane.icon.clone(),
    )
    .at(PaletteTarget::Pane(pane.id))
    .near(prior, Some(key))
}

fn chat_items(hosts: &[TargetHost], query: &Query, near: &Near) -> Vec<TargetItem> {
    let mut items = Vec::new();
    let own = near.around.own;
    for host in hosts
        .iter()
        .filter(|host| host.here)
        .chain(hosts.iter().filter(|host| !host.here))
        .filter(|host| matches!(host.status, PaletteStatus::Online | PaletteStatus::Running))
    {
        let place = Place {
            host,
            written: false,
        };
        for session in &host.sessions {
            for window in &session.windows {
                for pane in &window.panes {
                    let mine = host.here && Some(pane.id) == own;
                    let Some(chat) = pane.chat.as_ref().filter(|_| !mine) else {
                        continue;
                    };
                    let detail = pane
                        .program
                        .iter()
                        .map(ToString::to_string)
                        .chain(std::iter::once(format!(
                            "{}:{}",
                            session.name, window.index
                        )))
                        .chain((!host.here).then(|| host.name.clone()))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    let key = place_key(host, session.id, Some(window.id), Some(pane.id));
                    let prior = near.prior(host, Some(session.id), Some(window.id), 3, Some(&key));
                    let item = TargetItem::new(
                        TargetKind::Chat,
                        chat.title.clone(),
                        detail.clone(),
                        &place,
                        &pane.id.to_string(),
                        chat.icon.clone(),
                    )
                    .at(PaletteTarget::Pane(pane.id))
                    .near(prior, Some(key));
                    items.extend(item.scored(query, &detail));
                }
            }
        }
    }
    items
}

fn home_item(place: &Place, near: &Near) -> TargetItem {
    TargetItem::new(
        TargetKind::Dir,
        "~/",
        "files",
        place,
        "~/",
        IconName::Folder,
    )
    .near(near.prior(place.host, None, None, 0, None), None)
}

fn folder_item(place: &Place, folder: &str, near: &Near) -> TargetItem {
    let name = folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(folder)
        .to_owned();
    TargetItem::new(
        TargetKind::Dir,
        "./",
        name,
        place,
        &escape(folder),
        IconName::Folder,
    )
    .near(near.prior(place.host, None, None, 0, None), None)
}

fn spots(host: &TargetHost) -> impl Iterator<Item = Spot<'_>> {
    host.sessions.iter().flat_map(move |session| {
        session.windows.iter().map(move |window| Spot {
            host,
            session,
            window,
        })
    })
}

fn panes_by_id(host: &TargetHost, place: &Place, query: &Query, near: &Near) -> Vec<TargetItem> {
    spots(host)
        .flat_map(|spot| spot.window.panes.iter().map(move |pane| (spot, pane)))
        .filter_map(|(spot, pane)| {
            let mut item = pane_item(
                &spot,
                pane,
                place,
                &pane.id.to_string(),
                Some(format!("{}:{}", spot.session.name, spot.window.index)),
                near,
            );
            item.shown = Some(item.label.clone());
            item.label = pane.id.to_string().into();
            item.scored(query, "")
        })
        .collect()
}

fn fleet_items(hosts: &[TargetHost], query: &Query, near: &Near) -> Vec<TargetItem> {
    let mut items = Vec::new();
    for host in hosts {
        let place = Place {
            host,
            written: false,
        };
        items.extend(host_item(host, &place, near).scored(query, ""));
        if !matches!(host.status, PaletteStatus::Online | PaletteStatus::Running) {
            continue;
        }
        for session in &host.sessions {
            let (typed, _) = place.target(&session_ref(session));
            items.extend(
                session_item(host, session, &place, typed.clone(), near).scored(query, &host.name),
            );
            for window in &session.windows {
                let label = format!("{typed}:{}", window.index);
                let mut item = window_item(host, session, window, &place, label.clone(), near);
                item.detail = window.name.clone().into();
                let crumbs = format!("{} › {}", session.name, window.name);
                items.extend(item.scored(query, &crumbs));
                let spot = Spot {
                    host,
                    session,
                    window,
                };
                for pane in &window.panes {
                    let rest = format!("{}:{}.{}", session_ref(session), window.index, pane.id);
                    let item = pane_item(&spot, pane, &place, &rest, Some(label.clone()), near);
                    items.extend(item.scored(query, &spot.crumbs(pane)));
                }
            }
        }
    }
    items
}

fn ranked(mut items: Vec<TargetItem>, typed: bool) -> Vec<TargetItem> {
    if typed {
        items.sort_by_key(|item| Reverse(item.score));
        let best = items.first().map_or(0, |item| item.score);
        items.retain(|item| pick::keeps(item.score, best));
    }
    items.truncate(TARGET_MAX_ROWS);
    items
}

pub(crate) type Sections = Vec<(&'static str, Vec<TargetItem>)>;

fn root_sections(hosts: &[TargetHost], around: &Around, changed: Vec<TargetItem>) -> Sections {
    let near = around.near(hosts);
    let Some(here) = hosts.iter().find(|host| host.here) else {
        return vec![("", fleet_items(hosts, &Query::new(""), &near))];
    };
    let place = Place {
        host: here,
        written: false,
    };
    let everything = Query::new("");
    let window = spots(here).find(|spot| Some(spot.window.id) == near.window);
    let panes = window.as_ref().map_or_else(Vec::new, |spot| {
        spot.window
            .panes
            .iter()
            .filter(|pane| Some(pane.id) != around.own)
            .map(|pane| {
                let rest = format!(
                    "{}:{}.{}",
                    session_ref(spot.session),
                    spot.window.index,
                    pane.id
                );
                pane_item(spot, pane, &place, &rest, None, &near)
            })
            .collect()
    });
    let sessions = here
        .sessions
        .iter()
        .filter(|session| near.session.is_none_or(|id| id == session.id))
        .map(|session| session_item(here, session, &place, session.name.clone(), &near));
    let others = hosts.iter().filter(|host| !host.here).map(|host| {
        host_item(
            host,
            &Place {
                host,
                written: true,
            },
            &near,
        )
    });
    let files = around
        .folder
        .as_deref()
        .map(|folder| folder_item(&place, folder, &near))
        .into_iter()
        .chain(std::iter::once(home_item(&place, &near)))
        .collect();
    vec![
        ("Here", panes),
        ("Changed", changed),
        ("Chats", chat_items(hosts, &everything, &near)),
        ("Places", sessions.chain(others).collect()),
        ("Files", files),
    ]
}

fn target_items(query: &TargetQuery, hosts: &[TargetHost], around: &Around) -> Vec<TargetItem> {
    let filter = query.filter.as_str();
    let typed = !filter.is_empty();
    let near = around.near(hosts);
    let matcher = Query::new(filter);
    let host_named = |name: &str| hosts.iter().find(|host| host.name == name);
    let items: Vec<TargetItem> = match &query.level {
        TargetLevel::Top => {
            let Some(here) = hosts.iter().find(|host| host.here) else {
                return ranked(fleet_items(hosts, &matcher, &near), typed);
            };
            let place = Place {
                host: here,
                written: false,
            };
            if filter.starts_with('%') {
                panes_by_id(here, &place, &matcher, &near)
            } else if !typed {
                return root_sections(hosts, around, Vec::new())
                    .into_iter()
                    .flat_map(|(_, items)| items)
                    .collect();
            } else {
                let mut items = fleet_items(hosts, &matcher, &near);
                items.extend(chat_items(hosts, &matcher, &near));
                items.extend(home_item(&place, &near).scored(&matcher, ""));
                if let Some(folder) = &around.folder {
                    items.extend(folder_item(&place, folder, &near).scored(&matcher, ""));
                }
                items
            }
        }
        TargetLevel::Sessions { host } => {
            let Some(found) = host_named(host) else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            if filter.starts_with('%') {
                panes_by_id(found, &place, &matcher, &near)
            } else {
                found
                    .sessions
                    .iter()
                    .filter_map(|session| {
                        session_item(found, session, &place, session.name.clone(), &near)
                            .scored(&matcher, "")
                    })
                    .chain(home_item(&place, &near).scored(&matcher, ""))
                    .collect()
            }
        }
        TargetLevel::Windows { host, session } => {
            let Some((found, session)) =
                host_named(host).and_then(|found| Some((found, find_session(found, session)?)))
            else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            session
                .windows
                .iter()
                .filter_map(|window| {
                    window_item(
                        found,
                        session,
                        window,
                        &place,
                        format!("{}:{}", window.index, window.name),
                        &near,
                    )
                    .scored(&matcher, "")
                })
                .collect()
        }
        TargetLevel::Panes {
            host,
            session,
            window,
        } => {
            let Some((found, session, window)) = host_named(host).and_then(|found| {
                let session = find_session(found, session)?;
                Some((found, session, find_window(session, window)?))
            }) else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            let spot = Spot {
                host: found,
                session,
                window,
            };
            let target = format!("{}:{}", session_ref(session), window.index);
            window
                .panes
                .iter()
                .filter_map(|pane| {
                    pane_item(
                        &spot,
                        pane,
                        &place,
                        &format!("{target}.{}", pane.id),
                        None,
                        &near,
                    )
                    .scored(&matcher, &spot.crumbs(pane))
                })
                .collect()
        }
        TargetLevel::Files { .. } => return Vec::new(),
    };
    let own = around.own.map(PaletteTarget::Pane);
    let mine = |item: &TargetItem| {
        own.is_some()
            && item.target == own
            && hosts
                .iter()
                .any(|host| host.here && host.id == item.host_id)
    };
    ranked(
        items.into_iter().filter(|item| !mine(item)).collect(),
        typed,
    )
}

fn breadcrumb(query: &TargetQuery, root: Option<&PathListRoot>) -> String {
    let parts: Vec<String> = match &query.level {
        TargetLevel::Top => Vec::new(),
        TargetLevel::Sessions { host } => vec![host.clone()],
        TargetLevel::Windows { host, session } => vec![host.clone(), session.clone()],
        TargetLevel::Panes {
            host,
            session,
            window,
        } => vec![host.clone(), session.clone(), window.clone()],
        TargetLevel::Files { host, dir } => {
            let root = root.map_or_else(|| unescape(dir), |root| root.root.clone());
            vec![host.clone(), root]
        }
    };
    std::iter::once("@".to_owned())
        .chain(parts)
        .collect::<Vec<_>>()
        .join(" › ")
        .replacen(" › ", " ", 1)
}

const MENTION_VERBS: &[&str] = &["mention"];
const MENTION_TABS: &[&str] = &["into"];

fn level_of(body: &str) -> &str {
    let mut end = 0;
    let mut escaped = false;
    for (index, character) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, ':' | '.' | '/') {
            end = index + 1;
        }
    }
    &body[..end]
}

pub struct TargetPicker {
    backend: Rc<dyn TargetBackend>,
    composer: Entity<InputState>,
    own: Option<PaneId>,
    body: Option<String>,
    token: Option<TargetToken>,
    dismissed: Option<usize>,
    query: Option<TargetQuery>,
    listing: Option<Listing>,
    items: Rc<[TargetItem]>,
    headers: Rc<[(usize, SharedString)]>,
    found: Vec<TargetItem>,
    top_files: Option<(String, Vec<TargetItem>)>,
    selected: usize,
    scroll: UniformListScrollHandle,
    rows: usize,
    generation: u64,
    ranking: Option<Task<()>>,
    dirty: bool,
    mentions: Mentions,
    planted: Option<Range<usize>>,
    closed_at: Option<Point<Pixels>>,
    raw_before: Option<String>,
    room: Option<Pixels>,
    pending: Option<String>,
    subscriptions: Vec<Subscription>,
}

impl TargetPicker {
    pub fn for_composer(
        backend: Rc<dyn TargetBackend>,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_in(&input, window, |picker, _, window, cx| {
            picker.follow(window, cx);
        })
        .detach();
        cx.on_release(|picker, cx| {
            if let Some(listing) = picker.listing.take() {
                picker.cancel(&listing, cx);
            }
        })
        .detach();
        let focus = input.read(cx).focus_handle(cx);
        let blurred = cx.on_blur(&focus, window, |picker, window, cx| {
            picker.dismiss(window, cx);
        });
        Self {
            backend,
            composer: input,
            own: None,
            body: None,
            token: None,
            dismissed: None,
            query: None,
            listing: None,
            items: Rc::from([]),
            headers: Rc::from([]),
            found: Vec::new(),
            top_files: None,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            rows: TARGET_VISIBLE_ROWS,
            generation: 0,
            ranking: None,
            dirty: false,
            mentions: Mentions::default(),
            planted: None,
            closed_at: None,
            raw_before: None,
            room: None,
            pending: None,
            subscriptions: vec![blurred],
        }
    }

    pub fn keep(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    pub fn set_own_chat(&mut self, pane: Option<PaneId>) {
        self.own = pane;
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.body.is_some()
    }

    #[must_use]
    pub fn items(&self) -> &[TargetItem] {
        &self.items
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_item(&self) -> Option<&TargetItem> {
        self.items.get(self.selected)
    }

    #[must_use]
    pub fn query(&self) -> Option<&TargetQuery> {
        self.query.as_ref()
    }

    fn listing_root(&self) -> Option<&PathListRoot> {
        self.listing.as_ref()?.root.as_ref()
    }

    pub fn fit(&mut self, room: Pixels) {
        let rows = ((f32::from(room) - CHROME_HEIGHT) / palette_row_height()).floor();
        self.rows = (rows.max(1.0) as usize).min(TARGET_VISIBLE_ROWS);
    }

    pub fn raw_text(&mut self, text: &str) -> String {
        self.mentions.follow(text);
        self.mentions.raw()
    }

    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        {
            let input = self.composer.clone();
            let (value, cursor) = {
                let input = input.read(cx);
                (input.value(), input.cursor())
            };
            let changed = self.mentions.follow(&value);
            if self.raw_before.as_deref() == Some(value.as_ref()) {
                self.raw_before = None;
            } else if changed.len() > 1 && value[changed.clone()].contains('@') {
                let hosts = self.backend.hosts(cx);
                let named = self.mentions.name_refs(changed.clone(), |body| {
                    resolve_target(body, &hosts).map(|target| target.label)
                });
                if let Some(named) = named {
                    let caret = if cursor >= changed.end {
                        cursor - changed.len() + named.len()
                    } else if cursor > changed.start {
                        changed.start + named.len()
                    } else {
                        cursor
                    };
                    self.raw_before = Some(value.to_string());
                    input.update(cx, |input, cx| {
                        input.set_selected_range(changed, cx);
                        input.replace(named, window, cx);
                        input.set_selected_range(caret..caret, cx);
                    });
                }
            }
        }
        self.sync(cx);
    }

    pub fn sync(&mut self, cx: &mut Context<Self>) {
        let input = self.composer.clone();
        let (value, cursor) = {
            let input = input.read(cx);
            (input.value(), input.cursor())
        };
        self.mentions.follow(&value);
        let style = ref_highlight(cx);
        let highlights = self
            .mentions
            .spans()
            .into_iter()
            .map(|span| (span, style))
            .collect();
        input.update(cx, |input, cx| input.set_highlights(highlights, cx));
        let token = target_at(&value, cursor).filter(|_| !self.mentions.covers(cursor));
        let start = token.as_ref().map(|token| token.range.start);
        if self
            .planted
            .as_ref()
            .is_some_and(|planted| start != Some(planted.end - 1))
        {
            self.planted = None;
        }
        if self.dismissed.is_some() && self.dismissed != start {
            self.dismissed = None;
        }
        let token = token.filter(|_| self.dismissed.is_none());
        if token == self.token {
            return;
        }
        self.body = token.as_ref().map(|token| token.body.clone());
        self.token = token;
        self.rebuild(cx);
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let Some(body) = &self.body else {
            if let Some(listing) = self.listing.take() {
                self.cancel(&listing, cx);
            }
            self.query = None;
            self.items = Rc::from([]);
            self.headers = Rc::from([]);
            self.top_files = None;
            cx.notify();
            return;
        };
        let hosts = self.backend.hosts(cx);
        let query = parse_target(body, &hosts);
        let wanted = match &query.level {
            TargetLevel::Files { host, dir } => Some((host.clone(), unescape(dir))),
            TargetLevel::Top => self.folder(cx).and_then(|folder| {
                let here = hosts.iter().find(|host| host.here)?;
                Some((here.name.clone(), folder))
            }),
            _ => None,
        };
        if let Some((host, dir)) = wanted
            && !self
                .listing
                .as_ref()
                .is_some_and(|listing| listing.covers(&host, &dir))
        {
            if let Some(listing) = self.listing.take() {
                self.cancel(&listing, cx);
            }
            let mut listing = Listing::new(&host, dir);
            match self.backend.list(&host, &listing.dir, None, cx) {
                Ok(request) => listing.request = Some(request),
                Err(error) => {
                    listing.error = Some(error);
                    listing.done = true;
                }
            }
            self.generation += 1;
            self.top_files = None;
            self.listing = Some(listing);
        }
        self.query = Some(query);
        self.refresh(false, &hosts, cx);
    }

    fn folder(&self, cx: &App) -> Option<String> {
        self.backend.folder(self.own?, cx)
    }

    fn around(&self, cx: &App) -> Around {
        Around {
            own: self.own,
            folder: self.folder(cx),
            recents: palette_recents(cx),
            now: now_secs(),
        }
    }

    fn cancel(&self, listing: &Listing, cx: &mut App) {
        if let Some(request) = listing.request
            && !listing.done
        {
            self.backend.cancel(&listing.host, request, cx);
        }
    }

    fn refresh(&mut self, keep: bool, hosts: &[TargetHost], cx: &mut Context<Self>) {
        let Some(query) = self.query.clone() else {
            return;
        };
        let around = self.around(cx);
        match &query.level {
            TargetLevel::Files { host, dir } => {
                let Some(found) = hosts.iter().find(|candidate| candidate.name == *host) else {
                    self.land(keep, Vec::new(), cx);
                    return;
                };
                let Some(file_query) = self.file_query(found, &unescape(dir), dir, &query, cx)
                else {
                    return;
                };
                self.dirty = true;
                if self.ranking.is_none() {
                    self.rank(file_query, keep, cx);
                }
            }
            TargetLevel::Top if query.filter.is_empty() => {
                let changed = self
                    .top_query(hosts, &around, &query, cx)
                    .map_or_else(Vec::new, |file_query| changed_items(&file_query));
                let mut items = Vec::new();
                let mut headers = Vec::new();
                for (label, section) in root_sections(hosts, &around, changed) {
                    if section.is_empty() {
                        continue;
                    }
                    if !label.is_empty() {
                        headers.push((items.len(), SharedString::from(label)));
                    }
                    items.extend(section);
                }
                self.headers = headers.into();
                self.place(keep, items, cx);
            }
            TargetLevel::Top => {
                self.found = target_items(&query, hosts, &around);
                self.merge(keep, cx);
                if let Some(file_query) = self.top_query(hosts, &around, &query, cx) {
                    self.dirty = true;
                    if self.ranking.is_none() {
                        self.rank(file_query, keep, cx);
                    }
                }
            }
            _ => {
                let items = target_items(&query, hosts, &around);
                self.land(keep, items, cx);
            }
        }
    }

    fn file_query(
        &self,
        host: &TargetHost,
        resolved: &str,
        dir: &str,
        query: &TargetQuery,
        cx: &App,
    ) -> Option<FileQuery> {
        let listing = self
            .listing
            .as_ref()
            .filter(|listing| listing.host == host.name)?;
        let base = resolved.strip_prefix(listing.dir.as_str())?.to_owned();
        let root = listing.root.as_ref();
        Some(FileQuery {
            chunks: listing.chunks.clone(),
            marks: Arc::clone(&listing.marks),
            base,
            filter: query.filter.clone(),
            host: host.clone(),
            written: query.written,
            dir: dir.to_owned(),
            root: root.map(|root| root.root.clone()),
            recent: recency(&host.name, root, cx),
            top: matches!(query.level, TargetLevel::Top),
        })
    }

    fn top_query(
        &self,
        hosts: &[TargetHost],
        around: &Around,
        query: &TargetQuery,
        cx: &App,
    ) -> Option<FileQuery> {
        let folder = around.folder.as_ref()?;
        let here = hosts.iter().find(|host| host.here)?;
        self.file_query(here, folder, &escape(folder), query, cx)
            .filter(|file_query| !file_query.chunks.is_empty())
    }

    fn merge(&mut self, keep: bool, cx: &mut Context<Self>) {
        let filter = self.query.as_ref().map(|query| query.filter.clone());
        let mut items = self.found.clone();
        if let Some((ranked_for, files)) = &self.top_files
            && Some(ranked_for) == filter.as_ref()
        {
            items.extend(files.iter().cloned());
        }
        self.land(keep, ranked(items, true), cx);
    }

    fn rank(&mut self, query: FileQuery, keep: bool, cx: &mut Context<Self>) {
        self.dirty = false;
        let generation = self.generation;
        let top = query.top;
        let filter = query.filter.clone();
        let ranked = cx
            .background_executor()
            .spawn(async move { file_items(&query) });
        self.ranking = Some(cx.spawn(async move |picker, cx| {
            let items = ranked.await;
            picker
                .update(cx, |picker, cx| {
                    picker.ranking = None;
                    let level = picker.query.as_ref().map(|query| &query.level);
                    if picker.generation == generation {
                        match level {
                            Some(TargetLevel::Files { .. }) if !top => picker.land(keep, items, cx),
                            Some(TargetLevel::Top) if top => {
                                picker.top_files = Some((filter, items));
                                picker.merge(true, cx);
                            }
                            _ => {}
                        }
                    }
                    if picker.dirty {
                        let hosts = picker.backend.hosts(cx);
                        picker.refresh(true, &hosts, cx);
                    }
                })
                .ok();
        }));
    }

    fn land(&mut self, keep: bool, items: Vec<TargetItem>, cx: &mut Context<Self>) {
        self.headers = Rc::from([]);
        self.place(keep, items, cx);
    }

    fn place(&mut self, keep: bool, items: Vec<TargetItem>, cx: &mut Context<Self>) {
        let kept = keep
            .then(|| self.items.get(self.selected).map(|item| item.token.clone()))
            .flatten();
        self.items = items.into();
        let pending = self.pending.as_ref().and_then(|wanted| {
            self.items
                .iter()
                .position(|item| item.completion() == *wanted)
        });
        let listing = self.listing.as_ref().filter(|_| {
            matches!(
                self.query.as_ref().map(|query| &query.level),
                Some(TargetLevel::Files { .. })
            )
        });
        if pending.is_some()
            || listing.is_none_or(|listing| listing.done || listing.error.is_some())
        {
            self.pending = None;
        }
        self.selected = pending
            .or_else(|| {
                kept.and_then(|token| self.items.iter().position(|item| item.token == token))
            })
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.shown_at(self.selected), ScrollStrategy::Nearest);
        cx.notify();
    }

    fn shown_at(&self, item: usize) -> usize {
        item + self.headers.iter().filter(|(at, _)| *at <= item).count()
    }

    #[must_use]
    pub fn headers(&self) -> &[(usize, SharedString)] {
        &self.headers
    }

    pub fn apply(
        &mut self,
        host: &str,
        request_id: u64,
        update: ListingUpdate,
        cx: &mut Context<Self>,
    ) {
        match update {
            ListingUpdate::Begin(result) => self.apply_begin(host, request_id, result, cx),
            ListingUpdate::Chunk {
                entries,
                done,
                truncated,
            } => self.apply_chunk(host, request_id, &entries, done, truncated, cx),
            ListingUpdate::Git(marks) => self.apply_git(host, request_id, marks, cx),
        }
    }

    fn listing_for(&mut self, host: &str, request_id: u64) -> Option<&mut Listing> {
        self.listing
            .as_mut()
            .filter(|listing| listing.host == host && listing.request == Some(request_id))
    }

    fn relist(&mut self, cx: &mut Context<Self>) {
        let hosts = self.backend.hosts(cx);
        self.refresh(true, &hosts, cx);
    }

    pub fn apply_begin(
        &mut self,
        host: &str,
        request_id: u64,
        result: Result<PathListRoot, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(listing) = self.listing_for(host, request_id) else {
            return;
        };
        listing.begin(result);
        self.relist(cx);
    }

    pub fn apply_chunk(
        &mut self,
        host: &str,
        request_id: u64,
        entries: &Arc<[PathEntry]>,
        done: bool,
        truncated: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(listing) = self.listing_for(host, request_id) else {
            return;
        };
        if listing.chunk(entries, done, truncated) {
            self.relist(cx);
        }
    }

    pub fn apply_git(
        &mut self,
        host: &str,
        request_id: u64,
        marks: Vec<(String, GitMark)>,
        cx: &mut Context<Self>,
    ) {
        let Some(listing) = self.listing_for(host, request_id) else {
            return;
        };
        listing.git(marks);
        self.relist(cx);
    }

    pub fn navigate(&mut self, direction: isize, cx: &mut Context<Self>) -> bool {
        if !self.is_open() {
            return false;
        }
        let count = self.items.len();
        if count > 0 {
            self.selected = if direction < 0 {
                self.selected.checked_sub(1).unwrap_or(count - 1)
            } else {
                (self.selected + 1) % count
            };
            self.scroll
                .scroll_to_item(self.shown_at(self.selected), ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    pub fn complete(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(cx);
        let Some(token) = self.token.clone() else {
            return false;
        };
        if let Some(text) = self
            .selected_item()
            .filter(|item| item.drills())
            .map(TargetItem::completion)
        {
            self.replace(token.range, format!("@{text}"), window, cx);
        }
        true
    }

    pub fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let item = self.selected_item().cloned();
        self.accept_item(item, window, cx)
    }

    pub fn accept_item(
        &mut self,
        item: Option<TargetItem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync(cx);
        let Some(token) = self.token.clone() else {
            return false;
        };
        self.planted = None;
        if let Some(key) = item.as_ref().and_then(TargetItem::recent_key) {
            let label = item
                .as_ref()
                .map_or_else(String::new, |item| match item.kind {
                    TargetKind::File | TargetKind::Dir => item.typed.clone(),
                    _ => item.name().to_string(),
                });
            record_recent(key.clone(), label, cx);
        }
        let text = match item {
            Some(item) => {
                let hosts = self.backend.hosts(cx);
                match resolve_target(&item.token, &hosts) {
                    Some(target) => self.mentions.place(
                        token.range.clone(),
                        &item.reference(),
                        &target.label,
                        " ",
                    ),
                    None => item.insertion(),
                }
            }
            None => format!("{} ", &self.composer.read(cx).value()[token.range.clone()]),
        };
        self.replace(token.range, text, window, cx);
        true
    }

    pub fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.is_open() {
            return false;
        }
        if let Some(text) = self.selected_item().map(TargetItem::reference) {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            CommandPaletteView::notify_action(format!("Copied {text}"), window, cx);
        }
        true
    }

    pub fn drill(&mut self, item: &TargetItem, window: &mut Window, cx: &mut Context<Self>) {
        self.sync(cx);
        if let Some(token) = self.token.clone() {
            self.replace(token.range, format!("@{}", item.completion()), window, cx);
        }
    }

    pub fn climb(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(cx);
        let Some(token) = self.token.clone() else {
            return false;
        };
        let level = level_of(&token.body);
        if let Some(up) = pop_level(level) {
            self.pending = Some(level.to_owned());
            self.replace(token.range, format!("@{up}"), window, cx);
        }
        true
    }

    pub fn back_from_empty(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(cx);
        let Some(token) = self.token.as_ref() else {
            return false;
        };
        let (cursor, selecting) = {
            let input = self.composer.read(cx);
            (input.cursor(), !input.selected_range().is_empty())
        };
        let empty = !token.body.is_empty() && level_of(&token.body) == token.body;
        if !empty || selecting || cursor != token.range.end {
            return false;
        }
        self.climb(window, cx)
    }

    #[must_use]
    pub fn keys(&self) -> PaletteKeys {
        let item = self.selected_item();
        let level = self
            .token
            .as_ref()
            .is_some_and(|token| !level_of(&token.body).is_empty());
        PaletteKeys::new(MENTION_VERBS, "mention", MENTION_TABS)
            .take(item.is_some())
            .copy(item.is_some())
            .inside(item.is_some_and(TargetItem::drills))
            .back(level)
    }

    pub fn open_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.composer.clone();
        if self.is_open() {
            return;
        }
        let (value, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let text = if value[..cursor]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
        {
            "@"
        } else {
            " @"
        };
        self.dismissed = None;
        self.planted = Some(cursor..cursor + text.len());
        input.update(cx, |input, cx| input.insert(text, window, cx));
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(token) = self.token.take() else {
            return false;
        };
        self.dismissed = Some(token.range.start);
        self.body = None;
        self.unplant(window, cx);
        self.rebuild(cx);
        true
    }

    pub fn toggle(
        &mut self,
        press: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.composer.clone();
        let closed_at = self.closed_at.take();
        input.update(cx, |input, cx| input.focus(window, cx));
        if press.is_some() && closed_at == press {
            return;
        }
        if self.is_open() {
            self.dismiss(window, cx);
            return;
        }
        let (value, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let bare = target_at(&value, cursor)
            .is_some_and(|token| token.range == (cursor.saturating_sub(1)..cursor))
            && !self.mentions.covers(cursor);
        if bare {
            self.dismissed = None;
            self.sync(cx);
            return;
        }
        let text = if value[..cursor]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
        {
            "@"
        } else {
            " @"
        };
        self.planted = Some(cursor..cursor + text.len());
        input.update(cx, |input, cx| input.insert(text, window, cx));
    }

    fn close_from(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismiss(window, cx) {
            self.closed_at = Some(at);
        }
    }

    fn unplant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(planted) = self.planted.take() else {
            return;
        };
        let input = self.composer.clone();
        let (value, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let bare = value
            .get(planted.clone())
            .is_some_and(|text| text == "@" || text == " @")
            && value[planted.end..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace);
        if !bare {
            return;
        }
        let caret = if cursor >= planted.end {
            cursor - planted.len()
        } else {
            cursor.min(planted.start)
        };
        input.update(cx, |input, cx| {
            input.set_selected_range(planted, cx);
            input.replace("", window, cx);
            input.set_selected_range(caret..caret, cx);
        });
    }

    fn replace(
        &self,
        range: Range<usize>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            input.set_selected_range(range, cx);
            input.replace(text, window, cx);
        });
    }

    fn empty_message(&self) -> SharedString {
        let files = matches!(
            self.query.as_ref().map(|query| &query.level),
            Some(TargetLevel::Files { .. })
        );
        if let Some(listing) = self.listing.as_ref().filter(|_| files) {
            if let Some(error) = &listing.error {
                return error.clone();
            }
            if !listing.done || self.ranking.is_some() {
                return "Listing…".into();
            }
            if listing.is_empty() {
                return "This folder is empty".into();
            }
        }
        "No matches".into()
    }

    fn row(
        item: &TargetItem,
        index: usize,
        selected: bool,
        picker: Entity<Self>,
        cx: &App,
    ) -> ala::Div {
        let hover = picker.clone();
        let drills = item.drills();
        palette_row(
            ("target-row", index),
            &PaletteRow {
                label: item.name(),
                matches: if item.shown.is_some() {
                    Vec::new()
                } else {
                    item.matches.clone()
                },
                detail: item.detail.clone(),
                right: item.meta.clone(),
                status: item.status,
                icon: Some(item.icon.clone()),
                inside: Some(drills),
                ..Default::default()
            },
            selected,
            cx,
        )
        .on_mouse_enter(move |_, _, cx| {
            hover.update(cx, |picker, cx| {
                if picker.selected != index {
                    picker.selected = index;
                    cx.notify();
                }
            });
        })
        .on_click(move |_, window, cx| {
            picker.update(cx, |picker, cx| {
                picker.selected = index;
                if drills {
                    picker.complete(window, cx);
                } else {
                    picker.accept(window, cx);
                }
            });
            cx.stop_propagation();
        })
        .map(|row| div().h(px(palette_row_height())).child(row))
    }

    fn list(&self, cx: &mut Context<Self>) -> ala::AnyElement {
        let items = Rc::clone(&self.items);
        if items.is_empty() {
            return div()
                .debug_selector(|| "target-picker-empty".to_owned())
                .py(px(10.0))
                .text_center()
                .type_style(TextStyle::Small, cx)
                .text_color(cx.theme().foreground.muted())
                .child(self.empty_message())
                .into_any_element();
        }
        let selected = self.selected;
        let picker = cx.entity();
        let headers = Rc::clone(&self.headers);
        let mut shown = Vec::with_capacity(items.len() + headers.len());
        let mut next = headers.iter().peekable();
        for index in 0..items.len() {
            while let Some((_, label)) = next.next_if(|(at, _)| *at == index) {
                shown.push(Err(label.clone()));
            }
            shown.push(Ok(index));
        }
        let shown: Rc<[Result<usize, SharedString>]> = shown.into();
        let visible = shown.len().min(self.rows);
        let visible = f32::from(u16::try_from(visible).unwrap_or(u16::MAX));
        let rows = Rc::clone(&shown);
        uniform_list(
            "target-rows",
            shown.len(),
            cx.processor(move |_, range: Range<usize>, _, cx| {
                range
                    .filter_map(|at| match rows.get(at)? {
                        Ok(index) => items.get(*index).map(|item| {
                            Self::row(item, *index, selected == *index, picker.clone(), cx)
                        }),
                        Err(label) => {
                            Some(div().h(px(palette_row_height())).child(palette_section(
                                label.clone(),
                                "",
                                cx,
                            )))
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .h(px(palette_row_height() * visible))
        .track_scroll(&self.scroll)
        .into_any_element()
    }
}

impl Render for TargetPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(room) = self.room {
            self.fit(room);
        }
        let Some(query) = &self.query else {
            return div().into_any_element();
        };
        let crumb = breadcrumb(query, self.listing_root());
        let header = h_flex()
            .h(px(HEADER_HEIGHT))
            .px(px(14.0))
            .type_style(TextStyle::Micro, cx)
            .line_height(px(16.0))
            .text_color(cx.theme().foreground.muted())
            .child(
                div()
                    .debug_selector(|| "target-picker-crumb".to_owned())
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .type_style_mono(TextStyle::Caption, cx)
                    .child(crumb),
            );
        let footer = palette_footer(cx).child(self.keys());
        let body = self.list(cx);
        let measure = cx.entity();
        let outside = cx.entity();
        div()
            .id("target-picker")
            .debug_selector(|| "target-picker".to_owned())
            .relative()
            .w_full()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        measure.update(cx, |picker, _| {
                            picker.room = Some(bounds.bottom() - px(ROOM_MARGIN));
                        });
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(
                Panel::new("target-picker-panel", PanelLook::Popover)
                    .v_flex()
                    .w_full()
                    .overflow_hidden()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down_out(move |event, window, cx| {
                        let at = event.position;
                        let picker = outside.clone();
                        window.defer(cx, move |window, cx| {
                            picker.update(cx, |picker, cx| picker.close_from(at, window, cx));
                        });
                    })
                    .child(header)
                    .child(div().list_inset(cx).pb(px(BOTTOM_PADDING)).child(body))
                    .child(footer),
            )
            .into_any_element()
    }
}

pub fn mention_button(
    id: impl Into<ElementId>,
    picker: &Entity<TargetPicker>,
    on: Ground,
    cx: &App,
) -> Button {
    let open = picker.read(cx).is_open();
    let picker = picker.clone();
    composer_add_button(id, "Mention a chat, host, session or file", on)
        .opened(open)
        .on_click(move |event, window, cx| {
            let press = match event {
                ClickEvent::Mouse(click) => Some(click.down.position),
                ClickEvent::Touch(tap) => Some(tap.position),
                ClickEvent::Keyboard(_) => None,
            };
            picker.update(cx, |picker, cx| picker.toggle(press, window, cx));
        })
}

#[must_use]
pub fn mention_layer(picker: &Entity<TargetPicker>, cx: &App) -> Option<AnyElement> {
    picker
        .read(cx)
        .is_open()
        .then(|| picker.clone().into_any_element())
}

pub fn mention_keys<E: InteractiveElement>(element: E, picker: &Entity<TargetPicker>) -> E {
    let enter = picker.clone();
    let keys = picker.clone();
    element
        .capture_key_down(move |event, window, cx| {
            let stroke = &event.keystroke;
            let held = stroke.modifiers;
            let copy =
                stroke.key == "enter" && held.alt && !held.control && !held.platform && !held.shift;
            let handled = keys.update(cx, |picker, cx| {
                if copy {
                    return picker.copy(window, cx);
                }
                match palette_chord(stroke) {
                    Some(Twin::Down) => picker.navigate(1, cx),
                    Some(Twin::Up) => picker.navigate(-1, cx),
                    _ => false,
                }
            });
            if handled {
                cx.stop_propagation();
            }
        })
        .capture_action(on::<MoveUp>(picker, |picker, _, cx| {
            picker.navigate(-1, cx)
        }))
        .capture_action(on::<MoveDown>(picker, |picker, _, cx| {
            picker.navigate(1, cx)
        }))
        .capture_action(on::<IndentInline>(picker, TargetPicker::complete))
        .capture_action(on::<OutdentInline>(picker, TargetPicker::climb))
        .capture_action(on::<Backspace>(picker, TargetPicker::back_from_empty))
        .capture_action(on::<Escape>(picker, TargetPicker::dismiss))
        .capture_action(move |action: &Enter, window, cx| {
            if !action.shift && enter.update(cx, |picker, cx| picker.accept(window, cx)) {
                cx.stop_propagation();
            }
        })
}

fn on<A: Action>(
    picker: &Entity<TargetPicker>,
    handle: fn(&mut TargetPicker, &mut Window, &mut Context<TargetPicker>) -> bool,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let picker = picker.clone();
    move |_, window, cx| {
        if picker.update(cx, |picker, cx| handle(picker, window, cx)) {
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
#[path = "target_tests.rs"]
pub(crate) mod tests;
