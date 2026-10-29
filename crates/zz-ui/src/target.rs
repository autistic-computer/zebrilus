use std::{ops::Range, rc::Rc, sync::Arc};

use ala::{
    Action, App, Context, Entity, EventEmitter, InteractiveElement, IntoElement, MouseButton,
    Pixels, Render, ScrollStrategy, SharedString, Subscription, Task, UniformListScrollHandle,
    Window, div, prelude::*, px, uniform_list,
};
use zz_protocol::{GitMark, PaneId, PathEntry, PathKind, PathListRoot, SessionId, WindowId};

use crate::{
    ActiveTheme as _, Colorize as _, IconName, StyledExt as _, SurfaceRole,
    command::{
        PaletteHint, PaletteHostId, PaletteRow, PaletteStatus, PaletteTarget,
        command_palette_entry, command_palette_row_height, fuzzy_match, palette_hint,
    },
    files::{FileRow, GitMarks, Listing, file_rows, mark_letter},
    h_flex,
    input::{Enter, Escape, IndentInline, InputState, MoveDown, MoveUp},
    v_flex,
};

pub const TARGET_MAX_ROWS: usize = 200;
pub const TARGET_VISIBLE_ROWS: usize = 8;
const HEADER_HEIGHT: f32 = 28.0;
const BOTTOM_PADDING: f32 = 6.0;
const CHROME_HEIGHT: f32 = HEADER_HEIGHT + 2.0 * BOTTOM_PADDING + 2.0;

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
}

#[derive(Clone, Debug)]
pub struct TargetHost {
    pub id: PaletteHostId,
    pub name: String,
    pub status: PaletteStatus,
    pub here: bool,
    pub sessions: Vec<TargetSession>,
}

#[derive(Clone, Debug)]
pub struct TargetSession {
    pub id: SessionId,
    pub name: String,
    pub windows: Vec<TargetWindow>,
}

#[derive(Clone, Debug)]
pub struct TargetWindow {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    pub panes: Vec<TargetPane>,
}

#[derive(Clone, Debug)]
pub struct TargetPane {
    pub id: PaneId,
    pub title: String,
    pub program: Option<SharedString>,
    pub icon: IconName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Host,
    Session,
    Window,
    Pane,
    Dir,
    File,
}

impl TargetKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Session => "session",
            Self::Window => "window",
            Self::Pane => "pane",
            Self::Dir => "dir",
            Self::File => "file",
        }
    }
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
    score: usize,
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
            score: 0,
        }
    }

    fn at(mut self, target: PaletteTarget) -> Self {
        self.target = Some(target);
        self
    }

    fn matching(mut self, filter: &str, metadata: &str) -> Option<Self> {
        if let Some((score, matches)) = fuzzy_match(filter, &self.label) {
            self.score = score;
            self.matches = matches;
        } else if !filter.is_empty() && !metadata.is_empty() {
            self.score = fuzzy_match(filter, metadata)?.0.saturating_add(1000);
        } else {
            return None;
        }
        Some(self)
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
            TargetKind::Dir => "/",
            TargetKind::Pane | TargetKind::File => "",
        };
        format!("{}{next}", self.typed)
    }

    #[must_use]
    pub fn insertion(&self) -> String {
        format!("@{} ", self.token)
    }

    fn right(&self) -> SharedString {
        let kind = match self.mark {
            Some(mark) => format!("{} {}", mark_letter(mark), self.kind.label()),
            None => self.kind.label().to_owned(),
        };
        if self.drills() {
            format!("{kind} ›").into()
        } else {
            kind.into()
        }
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
}

fn file_items(query: &FileQuery) -> Vec<TargetItem> {
    let place = Place {
        host: &query.host,
        written: query.written,
    };
    file_rows(&query.chunks, &query.marks, &query.base, &query.filter)
        .into_iter()
        .map(|row| {
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
            let rest = format!("{}{}", query.dir, escape(&name));
            let mut item = TargetItem::new(kind, name, "", &place, &rest, icon);
            item.matches = matches;
            item.mark = mark;
            item.entry = Some(entry);
            item
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

pub(crate) fn pane_detail(pane: &TargetPane, place: Option<String>) -> String {
    let title = pane.title.trim();
    place
        .into_iter()
        .chain(pane.program.as_ref().map(ToString::to_string))
        .chain((!title.is_empty()).then(|| title.to_owned()))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn host_item(host: &TargetHost, place: &Place) -> TargetItem {
    let detail = match host.status {
        PaletteStatus::Online | PaletteStatus::Running => count(host.sessions.len(), "session"),
        PaletteStatus::Waiting => "connecting".to_owned(),
        PaletteStatus::Offline => "offline".to_owned(),
    };
    let mut item = TargetItem::new(
        TargetKind::Host,
        host.name.clone(),
        detail,
        place,
        "",
        IconName::HardDrive,
    );
    item.status = Some(host.status);
    item
}

fn session_item(session: &TargetSession, place: &Place, label: String) -> TargetItem {
    TargetItem::new(
        TargetKind::Session,
        label,
        count(session.windows.len(), "window"),
        place,
        &session_ref(session),
        IconName::Folder,
    )
    .at(PaletteTarget::Session(session.id))
}

fn window_item(
    session: &TargetSession,
    window: &TargetWindow,
    place: &Place,
    label: String,
) -> TargetItem {
    TargetItem::new(
        TargetKind::Window,
        label,
        count(window.panes.len(), "pane"),
        place,
        &format!("{}:{}", session_ref(session), window.index),
        IconName::PanelsTopLeft,
    )
    .at(PaletteTarget::Window(window.id))
}

fn pane_item(pane: &TargetPane, place: &Place, rest: &str, where_: Option<String>) -> TargetItem {
    TargetItem::new(
        TargetKind::Pane,
        pane.id.to_string(),
        pane_detail(pane, where_),
        place,
        rest,
        pane.icon.clone(),
    )
    .at(PaletteTarget::Pane(pane.id))
}

fn home_item(place: &Place) -> TargetItem {
    TargetItem::new(TargetKind::Dir, "~/", "files", place, "~", IconName::Folder)
}

fn panes_by_id(host: &TargetHost, place: &Place, filter: &str) -> Vec<TargetItem> {
    host.sessions
        .iter()
        .flat_map(|session| {
            session.windows.iter().flat_map(move |window| {
                window.panes.iter().map(move |pane| (session, window, pane))
            })
        })
        .filter_map(|(session, window, pane)| {
            pane_item(
                pane,
                place,
                &pane.id.to_string(),
                Some(format!("{}:{}", session.name, window.index)),
            )
            .matching(filter, "")
        })
        .collect()
}

fn fleet_items(hosts: &[TargetHost], filter: &str) -> Vec<TargetItem> {
    let mut items = Vec::new();
    for host in hosts {
        let place = Place {
            host,
            written: false,
        };
        items.extend(host_item(host, &place).matching(filter, ""));
        if !matches!(host.status, PaletteStatus::Online | PaletteStatus::Running) {
            continue;
        }
        for session in &host.sessions {
            let (typed, _) = place.target(&session_ref(session));
            items.extend(session_item(session, &place, typed.clone()).matching(filter, ""));
            for window in &session.windows {
                let label = format!("{typed}:{}", window.index);
                let mut item = window_item(session, window, &place, label.clone());
                item.detail = format!("{} · {}", window.name, item.detail).into();
                items.extend(item.matching(filter, &format!("{} {}", session.name, window.name)));
                for pane in &window.panes {
                    let rest = format!("{}:{}.{}", session_ref(session), window.index, pane.id);
                    let mut item = pane_item(pane, &place, &rest, None);
                    item.label = format!("{label}.{}", pane.id).into();
                    let metadata = item.detail.to_string();
                    items.extend(item.matching(filter, &metadata));
                }
            }
        }
    }
    items.sort_by_key(|item| {
        (
            item.score,
            !hosts.iter().any(|host| host.here && host.name == item.host),
        )
    });
    items
}

fn target_items(query: &TargetQuery, hosts: &[TargetHost]) -> Vec<TargetItem> {
    let filter = query.filter.as_str();
    let host_named = |name: &str| hosts.iter().find(|host| host.name == name);
    let mut items: Vec<TargetItem> = match &query.level {
        TargetLevel::Top => {
            let Some(here) = hosts.iter().find(|host| host.here) else {
                return fleet_items(hosts, filter);
            };
            let place = Place {
                host: here,
                written: false,
            };
            if filter.starts_with('%') {
                panes_by_id(here, &place, filter)
            } else if filter.is_empty() {
                here.sessions
                    .iter()
                    .map(|session| session_item(session, &place, session.name.clone()))
                    .chain(hosts.iter().filter(|host| !host.here).map(|host| {
                        host_item(
                            host,
                            &Place {
                                host,
                                written: true,
                            },
                        )
                    }))
                    .chain(std::iter::once(home_item(&place)))
                    .collect()
            } else {
                let mut items = fleet_items(hosts, filter);
                items.extend(home_item(&place).matching(filter, ""));
                items.truncate(TARGET_MAX_ROWS);
                return items;
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
                panes_by_id(found, &place, filter)
            } else {
                found
                    .sessions
                    .iter()
                    .map(|session| session_item(session, &place, session.name.clone()))
                    .chain(std::iter::once(home_item(&place)))
                    .filter_map(|item| item.matching(filter, ""))
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
                        session,
                        window,
                        &place,
                        format!("{}:{}", window.index, window.name),
                    )
                    .matching(filter, "")
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
            let target = format!("{}:{}", session_ref(session), window.index);
            window
                .panes
                .iter()
                .filter_map(|pane| {
                    pane_item(pane, &place, &format!("{target}.{}", pane.id), None)
                        .matching(filter, "")
                })
                .collect()
        }
        TargetLevel::Files { .. } => return Vec::new(),
    };
    if !filter.is_empty() {
        items.sort_by_key(|item| item.score);
    }
    items.truncate(TARGET_MAX_ROWS);
    items
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

const HINTS: [PaletteHint; 3] = [
    PaletteHint {
        key: "tab",
        label: "open",
    },
    PaletteHint {
        key: "enter",
        label: "insert",
    },
    PaletteHint {
        key: "escape",
        label: "close",
    },
];

pub enum TargetPickerEvent {
    Picked { open: bool },
}

impl EventEmitter<TargetPickerEvent> for TargetPicker {}

pub struct TargetPicker {
    backend: Rc<dyn TargetBackend>,
    composer: Option<Entity<InputState>>,
    anchor: Option<PaneId>,
    body: Option<String>,
    token: Option<TargetToken>,
    dismissed: Option<usize>,
    query: Option<TargetQuery>,
    listing: Option<Listing>,
    items: Rc<[TargetItem]>,
    selected: usize,
    scroll: UniformListScrollHandle,
    rows: usize,
    generation: u64,
    ranking: Option<Task<()>>,
    dirty: bool,
    subscriptions: Vec<Subscription>,
}

impl TargetPicker {
    pub fn new(backend: Rc<dyn TargetBackend>, cx: &mut Context<Self>) -> Self {
        cx.on_release(|picker, cx| {
            if let Some(listing) = picker.listing.take() {
                picker.cancel(&listing, cx);
            }
        })
        .detach();
        Self {
            backend,
            composer: None,
            anchor: None,
            body: None,
            token: None,
            dismissed: None,
            query: None,
            listing: None,
            items: Rc::from([]),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            rows: TARGET_VISIBLE_ROWS,
            generation: 0,
            ranking: None,
            dirty: false,
            subscriptions: Vec::new(),
        }
    }

    pub fn for_composer(
        backend: Rc<dyn TargetBackend>,
        input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&input, |picker, _, cx| picker.sync(cx)).detach();
        Self {
            composer: Some(input),
            ..Self::new(backend, cx)
        }
    }

    pub fn keep(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    pub fn set_anchor(&mut self, anchor: Option<PaneId>) {
        self.anchor = anchor;
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

    #[must_use]
    pub fn listing_root(&self) -> Option<&PathListRoot> {
        self.listing.as_ref()?.root.as_ref()
    }

    pub fn fit(&mut self, room: Pixels) {
        let chrome = if self.composer.is_some() {
            CHROME_HEIGHT
        } else {
            0.0
        };
        let rows = ((f32::from(room) - chrome) / command_palette_row_height()).floor();
        self.rows = (rows.max(1.0) as usize).min(if self.composer.is_some() {
            TARGET_VISIBLE_ROWS
        } else {
            usize::MAX
        });
    }

    pub fn set_body(&mut self, body: &str, cx: &mut Context<Self>) {
        if self.body.as_deref() == Some(body) {
            return;
        }
        self.body = Some(body.to_owned());
        self.rebuild(cx);
    }

    pub fn sync(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.composer else {
            return;
        };
        let token = {
            let input = input.read(cx);
            target_at(&input.value(), input.cursor())
        };
        let start = token.as_ref().map(|token| token.range.start);
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
            cx.notify();
            return;
        };
        let hosts = self.backend.hosts(cx);
        let query = parse_target(body, &hosts);
        if let TargetLevel::Files { host, dir } = &query.level {
            let dir = unescape(dir);
            if !self
                .listing
                .as_ref()
                .is_some_and(|listing| listing.covers(host, &dir))
            {
                if let Some(listing) = self.listing.take() {
                    self.cancel(&listing, cx);
                }
                let mut listing = Listing::new(host, dir);
                match self.backend.list(host, &listing.dir, self.anchor, cx) {
                    Ok(request) => listing.request = Some(request),
                    Err(error) => {
                        listing.error = Some(error);
                        listing.done = true;
                    }
                }
                self.generation += 1;
                self.listing = Some(listing);
            }
        } else if let Some(listing) = self.listing.take() {
            self.cancel(&listing, cx);
        }
        self.query = Some(query);
        self.refresh(false, &hosts, cx);
    }

    fn cancel(&self, listing: &Listing, cx: &mut App) {
        if let Some(request) = listing.request
            && !listing.done
        {
            self.backend.cancel(&listing.host, request, cx);
        }
    }

    fn refresh(&mut self, keep: bool, hosts: &[TargetHost], cx: &mut Context<Self>) {
        let Some(query) = &self.query else {
            return;
        };
        if let TargetLevel::Files { host, dir } = &query.level {
            let Some(found) = hosts.iter().find(|candidate| candidate.name == *host) else {
                self.land(keep, Vec::new(), cx);
                return;
            };
            let Some(listing) = &self.listing else {
                return;
            };
            let resolved = unescape(dir);
            let file_query = FileQuery {
                chunks: listing.chunks.clone(),
                marks: Arc::clone(&listing.marks),
                base: resolved
                    .strip_prefix(listing.dir.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                filter: query.filter.clone(),
                host: found.clone(),
                written: query.written,
                dir: dir.clone(),
            };
            self.dirty = true;
            if self.ranking.is_none() {
                self.rank(file_query, keep, cx);
            }
            return;
        }
        let items = target_items(query, hosts);
        self.land(keep, items, cx);
    }

    fn rank(&mut self, query: FileQuery, keep: bool, cx: &mut Context<Self>) {
        self.dirty = false;
        let generation = self.generation;
        let ranked = cx
            .background_executor()
            .spawn(async move { file_items(&query) });
        self.ranking = Some(cx.spawn(async move |picker, cx| {
            let items = ranked.await;
            picker
                .update(cx, |picker, cx| {
                    picker.ranking = None;
                    if picker.generation == generation
                        && matches!(
                            picker.query.as_ref().map(|query| &query.level),
                            Some(TargetLevel::Files { .. })
                        )
                    {
                        picker.land(keep, items, cx);
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
        let kept = keep
            .then(|| self.items.get(self.selected).map(|item| item.token.clone()))
            .flatten();
        self.items = items.into();
        self.selected = kept
            .and_then(|token| self.items.iter().position(|item| item.token == token))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
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
                .scroll_to_item(self.selected, ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    pub fn select_edge(&mut self, first: bool, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.selected = if first { 0 } else { self.items.len() - 1 };
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub fn complete(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(cx);
        let Some(token) = self.token.clone() else {
            return false;
        };
        if let Some(text) = self.selected_item().map(TargetItem::completion) {
            self.replace(token.range, format!("@{text}"), window, cx);
        }
        true
    }

    pub fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(cx);
        let Some(token) = self.token.clone() else {
            return false;
        };
        let text = self.selected_item().map_or_else(
            || {
                self.composer.as_ref().map_or_else(String::new, |input| {
                    format!("{} ", &input.read(cx).value()[token.range.clone()])
                })
            },
            TargetItem::insertion,
        );
        self.replace(token.range, text, window, cx);
        true
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(token) = self.token.take() else {
            return false;
        };
        self.dismissed = Some(token.range.start);
        self.body = None;
        self.rebuild(cx);
        true
    }

    fn replace(
        &self,
        range: Range<usize>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = &self.composer {
            input.update(cx, |input, cx| {
                input.set_selected_range(range, cx);
                input.replace(text, window, cx);
            });
        }
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
        command_palette_entry(
            ("target-row", index),
            &PaletteRow {
                label: item.label.clone(),
                matches: item.matches.clone(),
                detail: item.detail.clone(),
                right: item.right(),
                status: item.status,
                icon: Some(item.icon.clone()),
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
                if picker.composer.is_none() {
                    cx.emit(TargetPickerEvent::Picked { open: drills });
                } else if drills {
                    picker.complete(window, cx);
                } else {
                    picker.accept(window, cx);
                }
            });
            cx.stop_propagation();
        })
        .map(|row| div().h(px(command_palette_row_height())).child(row))
    }

    fn list(&self, cx: &mut Context<Self>) -> ala::AnyElement {
        let items = Rc::clone(&self.items);
        if items.is_empty() {
            return div()
                .debug_selector(|| "target-picker-empty".to_owned())
                .py(px(if self.composer.is_some() { 10.0 } else { 16.0 }))
                .text_center()
                .text_size(crate::rems_from_px(12.0))
                .text_color(cx.theme().foreground.muted())
                .child(self.empty_message())
                .into_any_element();
        }
        let selected = self.selected;
        let picker = cx.entity();
        let visible = items.len().min(self.rows);
        let visible = f32::from(u16::try_from(visible).unwrap_or(u16::MAX));
        uniform_list(
            "target-rows",
            items.len(),
            cx.processor(move |_, range: Range<usize>, _, cx| {
                range
                    .filter_map(|index| {
                        items.get(index).map(|item| {
                            Self::row(item, index, selected == index, picker.clone(), cx)
                        })
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .h(px(command_palette_row_height() * visible))
        .track_scroll(&self.scroll)
        .into_any_element()
    }
}

impl Render for TargetPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(query) = &self.query else {
            return div().into_any_element();
        };
        if self.composer.is_none() {
            return div()
                .id("target-picker")
                .debug_selector(|| "target-picker".to_owned())
                .w_full()
                .child(self.list(cx))
                .into_any_element();
        }
        let crumb = breadcrumb(query, self.listing_root());
        let header = h_flex()
            .h(px(HEADER_HEIGHT))
            .px(px(14.0))
            .gap(px(14.0))
            .text_size(crate::rems_from_px(10.0))
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
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(crate::rems_from_px(11.0))
                    .child(crumb),
            )
            .children(HINTS.map(|hint| palette_hint(hint, cx)));
        let body = self.list(cx);
        div()
            .id("target-picker")
            .debug_selector(|| "target-picker".to_owned())
            .w_full()
            .px(px(8.0))
            .pb(px(BOTTOM_PADDING))
            .child(
                v_flex()
                    .w_full()
                    .overflow_hidden()
                    .surface(SurfaceRole::POPOVER, cx)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(header)
                    .child(div().list_inset(cx).pb(px(BOTTOM_PADDING)).child(body)),
            )
            .into_any_element()
    }
}

pub fn mention_at_caret(input: &Entity<InputState>, text: &str, window: &mut Window, cx: &mut App) {
    input.update(cx, |input, cx| {
        let before = input.value()[..input.cursor()].chars().next_back();
        if !before.is_none_or(char::is_whitespace) {
            input.insert(" ", window, cx);
        }
        input.insert(text, window, cx);
        input.focus(window, cx);
    });
}

pub fn mention_keys<E: InteractiveElement>(element: E, picker: &Entity<TargetPicker>) -> E {
    let enter = picker.clone();
    element
        .capture_action(on::<MoveUp>(picker, |picker, _, cx| {
            picker.navigate(-1, cx)
        }))
        .capture_action(on::<MoveDown>(picker, |picker, _, cx| {
            picker.navigate(1, cx)
        }))
        .capture_action(on::<IndentInline>(picker, TargetPicker::complete))
        .capture_action(on::<Escape>(picker, |picker, _, cx| picker.dismiss(cx)))
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
