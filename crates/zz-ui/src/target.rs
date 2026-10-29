//! zz's target grammar and the picker that lists it. A target is tmux's `-t`
//! with an optional host in front, `host:session:window.pane`, or a file on a
//! host in scp form, `host:/abs/path` or `host:~/path`. Without a host it means
//! the host the window is attached to, the way tmux's `-t` means the current
//! server, and a host's name wins over a session of the same name: `work:2`,
//! `~/dev/`, `xps:work:2.%7`, `%7`. Typing filters the level the cursor is in,
//! and after the last `/` it searches everything under that folder. Tab opens
//! the level under the highlighted row, and `:`, `.` and `/` typed by hand do
//! the same. One [`TargetPicker`] serves the command palette's `@` mode,
//! `prefix F`, and the lens composer's `@` mentions, which go out with their
//! host written.

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
/// The floating picker's height besides its rows: the header, the paddings
/// under the rows and the surface's edge.
const CHROME_HEIGHT: f32 = HEADER_HEIGHT + 2.0 * BOTTOM_PADDING + 2.0;

/// Where the picker reads the fleet and lists a host's files.
pub trait TargetBackend {
    /// Every host in fleet order, this machine under its own name.
    fn hosts(&self, cx: &App) -> Vec<TargetHost>;
    /// Ask `host` for everything under `dir`, an absolute or `~` path, through
    /// `anchor` when that is a terminal on `host`, whose shell then decides how
    /// a path is inserted. The listing arrives through
    /// [`TargetPicker::apply_begin`] and the rest.
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
    /// The host the window is attached to, which a target without a host means.
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

/// The `@` word the cursor is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetToken {
    /// From the `@` to the end of the word, which may run past the cursor.
    pub range: Range<usize>,
    /// What follows the `@` up to the cursor.
    pub body: String,
}

/// The `@` word around `cursor`: one that starts the text or follows
/// whitespace. A backslash keeps the whitespace after it in the word.
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
    /// Before any `:`, `/` or `~`: the sessions here, the other hosts and the
    /// files here, and anything in the fleet once you type.
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
    /// `dir` as typed, ending in `/`, backslashes kept.
    Files {
        host: String,
        dir: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetQuery {
    pub level: TargetLevel,
    pub filter: String,
    /// Whether the host was written, which completions then keep.
    pub written: bool,
}

/// Read a target: the level it names and the text filtering it. A first
/// part before `:` is a host when one has that name, else a session here.
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

/// The target one level up from `body` when it ends on a level's edge:
/// `xps:work:` gives `xps:`, `~/dev/zz/` gives `~/dev/`, `~/` and `xps:`
/// give nothing. `None` while `body` filters a level.
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

/// `name` as a target spells it: a backslash before each space or backslash.
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

/// One row: what it is, how it shows and the target it finishes.
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
    /// The session, window or pane, for the palette to go to.
    pub target: Option<PaletteTarget>,
    /// The file or folder, relative to the listing's root.
    pub entry: Option<PathEntry>,
    /// The target as you would type it: without its host when that is the
    /// host here and you did not write it.
    pub typed: String,
    /// The target with its host, which a mention carries.
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

    /// Match `filter` on the label, else on `metadata` behind every label
    /// match.
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

    /// Whether the row has a level under it that Tab opens.
    #[must_use]
    pub const fn drills(&self) -> bool {
        matches!(
            self.kind,
            TargetKind::Host | TargetKind::Session | TargetKind::Window | TargetKind::Dir
        )
    }

    /// What Tab leaves: the target as typed, and after a row with more under
    /// it the character that opens that level, so the picker lists it.
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

    /// What Enter leaves in a composer: the finished mention, its host
    /// written, and a space.
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

/// Where rows sit: their host, and whether the target was written with it.
struct Place<'a> {
    host: &'a TargetHost,
    written: bool,
}

impl Place<'_> {
    /// `rest` behind the host as typed here, and with the host written.
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

/// What the files level ranks off the main thread.
struct FileQuery {
    chunks: Vec<Arc<[PathEntry]>>,
    marks: Arc<GitMarks>,
    /// Where `dir` sits under the listing's folder.
    base: String,
    filter: String,
    host: TargetHost,
    written: bool,
    /// The folder as typed, ending in `/`.
    dir: String,
}

/// The rows under `query.dir`: its own entries, folders first, while the
/// filter is empty, and the best matches anywhere under it once you type.
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

/// A session as a target names it: its name when tmux can read that back, else
/// its id.
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

/// Every pane on `host` whose id `filter` matches, with where it sits.
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

/// Everything in the fleet `filter` matches: hosts, then each connected
/// host's sessions, windows and panes, labelled as you would type them.
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

/// The rows for `query` above the files level, best first.
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

/// Where the cursor is, for the floating picker's header: `@ ubuntu › work › 2`.
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
    /// A row was clicked in the palette: go to it, or with `open` list what
    /// is under it.
    Picked { open: bool },
}

impl EventEmitter<TargetPickerEvent> for TargetPicker {}

/// The picker over a target. In a composer it follows the input on its own:
/// open while the cursor is in an `@` word, closed by Esc until the cursor
/// leaves it, and drawn floating over the input. In the palette its owner sets
/// what is typed with [`Self::set_body`], and it draws as the palette's rows.
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
    /// The palette's picker; its owner feeds it what is typed.
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

    /// The picker over a composer's `input`, for `@` mentions.
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

    /// Keep `subscription`, such as the one that feeds listings, for as long
    /// as the picker lives.
    pub fn keep(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    /// The terminal files are listed through, and so inserted into, on its host.
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

    /// The folder the files level lists, once its host said where that is.
    #[must_use]
    pub fn listing_root(&self) -> Option<&PathListRoot> {
        self.listing.as_ref()?.root.as_ref()
    }

    /// Show only as many rows as fit in `room`: for the floating picker the
    /// height above the input it floats over, so it never reaches past the
    /// window, and for the palette the height its rows may take.
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

    /// The palette's text: list what it names.
    pub fn set_body(&mut self, body: &str, cx: &mut Context<Self>) {
        if self.body.as_deref() == Some(body) {
            return;
        }
        self.body = Some(body.to_owned());
        self.rebuild(cx);
    }

    /// Follow the composer: open, refilter or close for the word at the cursor.
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

    /// Up or Down: move the highlight. Whether the picker took the key.
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

    /// Highlight the first or the last row.
    pub fn select_edge(&mut self, first: bool, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.selected = if first { 0 } else { self.items.len() - 1 };
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Tab in a composer: complete the highlighted row into the word, opening
    /// the level under it when it has one.
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

    /// Enter in a composer: put the finished mention in with a space after it.
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

    /// Esc in a composer: close until the cursor leaves this word.
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

/// Put `text` at the caret of `input`, a space before it unless one is
/// there, and focus the field: what a composer's `+` does with `@`.
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

/// Route a composer's keys to `picker` while it is open: Up and Down move the
/// highlight, Tab opens the level under the row, Enter inserts and Esc closes
/// only the picker. Hang it on an element around the input; it captures the
/// input's own actions.
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
