use std::{ops::Range, str::FromStr};

use zz_protocol::{CommandSpec, CommandValueKind, PaneId, SessionId, WindowId};

use crate::{
    pick::{self, Closeness, Kind, Match, Prior, Query},
    recents::{MAX_BOOST, RecentKey},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Host,
    Session,
    Window,
    Pane,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Host, Self::Session, Self::Window, Self::Pane];

    #[must_use]
    pub const fn deeper(self) -> Option<Self> {
        match self {
            Self::Host => Some(Self::Session),
            Self::Session => Some(Self::Window),
            Self::Window => Some(Self::Pane),
            Self::Pane => None,
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LevelSet(u8);

impl LevelSet {
    #[must_use]
    pub const fn with(self, level: Level) -> Self {
        Self(self.0 | level.bit())
    }

    #[must_use]
    pub const fn contains(self, level: Level) -> bool {
        self.0 & level.bit() != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Command,
    Gui,
    Files,
}

pub const COMMAND_SIGIL: char = ':';

#[must_use]
pub const fn is_sigil(character: char) -> bool {
    character == COMMAND_SIGIL
}

fn digits(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub level: Level,
    pub query: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub command: Option<String>,
    pub search: Option<String>,
}

#[must_use]
pub fn parse(text: &str) -> Line {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix(COMMAND_SIGIL) {
        return Line {
            command: Some(rest.to_owned()),
            search: None,
        };
    }
    let search = trimmed.trim_end();
    Line {
        command: None,
        search: (!search.is_empty()).then(|| search.to_owned()),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fleet {
    pub hosts: Vec<FleetHost>,
    pub here: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetHost {
    pub id: u32,
    pub name: String,
    pub online: bool,
    pub current: Option<SessionId>,
    pub sessions: Vec<FleetSession>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetSession {
    pub id: SessionId,
    pub name: String,
    pub label: String,
    pub windows: Vec<FleetWindow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetWindow {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    pub active: bool,
    pub active_pane: PaneId,
    pub panes: Vec<FleetPane>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetPane {
    pub id: PaneId,
    pub index: u32,
    pub title: String,
    pub program: Option<String>,
    pub cwd: String,
    pub terminal: bool,
    pub needs_you: bool,
}

impl FleetPane {
    #[must_use]
    pub fn name(&self) -> &str {
        let title = self.title.trim();
        if !title.is_empty() {
            return title;
        }
        self.program.as_deref().unwrap_or("pane")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Place {
    pub host: Option<u32>,
    pub session: Option<SessionId>,
    pub window: Option<WindowId>,
    pub pane: Option<PaneId>,
}

impl Place {
    #[must_use]
    pub const fn host(id: u32) -> Self {
        Self {
            host: Some(id),
            session: None,
            window: None,
            pane: None,
        }
    }

    #[must_use]
    pub const fn level(&self) -> Option<Level> {
        if self.pane.is_some() {
            Some(Level::Pane)
        } else if self.window.is_some() {
            Some(Level::Window)
        } else if self.session.is_some() {
            Some(Level::Session)
        } else if self.host.is_some() {
            Some(Level::Host)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn above(mut self, level: Level) -> Self {
        match level {
            Level::Host => {
                self.host = None;
                self.session = None;
                self.window = None;
                self.pane = None;
            }
            Level::Session => {
                self.session = None;
                self.window = None;
                self.pane = None;
            }
            Level::Window => {
                self.window = None;
                self.pane = None;
            }
            Level::Pane => self.pane = None,
        }
        self
    }
}

struct Spot<'a> {
    host: &'a FleetHost,
    session: &'a FleetSession,
    window: &'a FleetWindow,
}

impl Fleet {
    #[must_use]
    pub fn host(&self, id: u32) -> Option<&FleetHost> {
        self.hosts.iter().find(|host| host.id == id)
    }

    #[must_use]
    pub fn host_named(&self, name: &str) -> Option<&FleetHost> {
        self.hosts.iter().find(|host| host.name == name)
    }

    #[must_use]
    pub fn current(&self) -> Place {
        let Some(host) = self.host(self.here) else {
            return Place::default();
        };
        let mut place = Place::host(host.id);
        let Some(session) = host
            .current
            .and_then(|id| host.sessions.iter().find(|session| session.id == id))
        else {
            return place;
        };
        place.session = Some(session.id);
        if let Some(window) = active_window(session) {
            place.window = Some(window.id);
            place.pane = Some(window.active_pane);
        }
        place
    }

    #[must_use]
    pub fn session(&self, place: &Place) -> Option<&FleetSession> {
        let id = place.session?;
        self.host(place.host?)?
            .sessions
            .iter()
            .find(|session| session.id == id)
    }

    #[must_use]
    pub fn window(&self, place: &Place) -> Option<&FleetWindow> {
        let id = place.window?;
        self.session(place)?
            .windows
            .iter()
            .find(|window| window.id == id)
    }

    #[must_use]
    pub fn pane(&self, place: &Place) -> Option<&FleetPane> {
        let id = place.pane?;
        self.window(place)?.panes.iter().find(|pane| pane.id == id)
    }

    #[must_use]
    pub fn landing_pane(&self, place: &Place) -> Option<&FleetPane> {
        if place.pane.is_some() {
            return self.pane(place);
        }
        let window = if let Some(window) = self.window(place) {
            window
        } else {
            let session = if let Some(session) = self.session(place) {
                session
            } else {
                let host = self.host(place.host?)?;
                let current = host.current?;
                host.sessions.iter().find(|session| session.id == current)?
            };
            active_window(session)?
        };
        window
            .panes
            .iter()
            .find(|pane| pane.id == window.active_pane)
    }

    fn spots(&self) -> impl Iterator<Item = Spot<'_>> {
        self.hosts.iter().flat_map(|host| {
            host.sessions.iter().flat_map(move |session| {
                session.windows.iter().map(move |window| Spot {
                    host,
                    session,
                    window,
                })
            })
        })
    }

    fn spot_of(&self, host: u32, find: impl Fn(&Spot) -> bool) -> Option<Spot<'_>> {
        self.spots()
            .filter(|spot| spot.host.id == host)
            .find(|spot| find(spot))
    }
}

fn active_window(session: &FleetSession) -> Option<&FleetWindow> {
    session
        .windows
        .iter()
        .find(|window| window.active)
        .or_else(|| session.windows.first())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub place: Place,
    pub level: Level,
    pub label: String,
    pub matches: Vec<Range<usize>>,
    pub score: i64,
}

struct Entry {
    place: Place,
    level: Level,
    label: String,
    name_at: usize,
    path: String,
    metadata: String,
    indexes: Vec<u32>,
    exact: bool,
}

impl Entry {
    fn new(place: Place, level: Level, label: String, path: String) -> Self {
        Self {
            place,
            level,
            label,
            name_at: 0,
            path,
            metadata: String::new(),
            indexes: Vec::new(),
            exact: false,
        }
    }

    fn indexed(mut self, indexes: Vec<u32>, metadata: String) -> Self {
        self.name_at = indexes
            .last()
            .map_or(0, |index| index.to_string().len() + 1);
        self.indexes = indexes;
        self.metadata = metadata;
        self
    }
}

#[must_use]
pub fn closeness(current: &Place, place: &Place) -> Closeness {
    if place.host != current.host {
        Closeness::Elsewhere
    } else if place.window.is_some() && place.window == current.window {
        Closeness::Window
    } else if place.session.is_some() && place.session == current.session {
        Closeness::Session
    } else {
        Closeness::Host
    }
}

fn depth(level: Level, scope: &Place) -> u32 {
    let from = scope.level().map_or(0, |level| level as u32 + 1);
    (level as u32).saturating_sub(from)
}

fn index_points(digits: &str) -> i64 {
    let candidate = pick::Candidate {
        kind: Kind::Place,
        label: digits,
        path: "",
        prior: Prior::default(),
    };
    Query::new(digits)
        .matched(&candidate)
        .map_or(0, |found| found.score)
}

fn host_scope<'a>(fleet: &'a Fleet, scope: &Place) -> impl Iterator<Item = &'a FleetHost> {
    let wanted = scope.host;
    fleet
        .hosts
        .iter()
        .filter(move |host| wanted.map_or(host.online, |wanted| host.id == wanted))
}

fn parent_session(fleet: &Fleet, scope: &Place) -> Option<(u32, SessionId)> {
    if let (Some(host), Some(session)) = (scope.host, scope.session) {
        return Some((host, session));
    }
    let host = fleet.host(scope.host.unwrap_or(fleet.here))?;
    Some((host.id, host.current?))
}

fn entries(fleet: &Fleet, scope: &Place, level: Level, query: &str) -> Vec<Entry> {
    let scope = scope.above(level);
    let index = digits(query);
    let mut found = Vec::new();
    match level {
        Level::Host => {
            for host in &fleet.hosts {
                found.push(Entry::new(
                    Place::host(host.id),
                    level,
                    host.name.clone(),
                    String::new(),
                ));
            }
        }
        Level::Session => {
            let exact = index.is_some()
                && host_scope(fleet, &scope)
                    .flat_map(|host| &host.sessions)
                    .any(|session| session.name == query);
            for host in host_scope(fleet, &scope) {
                for session in &host.sessions {
                    if exact && session.name != query {
                        continue;
                    }
                    let place = Place {
                        host: Some(host.id),
                        session: Some(session.id),
                        window: None,
                        pane: None,
                    };
                    let mut entry =
                        Entry::new(place, level, session.label.clone(), host.name.clone());
                    entry.metadata.clone_from(&session.name);
                    entry.exact = exact;
                    found.push(entry);
                }
            }
        }
        Level::Window => {
            let parent = if scope.session.is_some() || index.is_some() {
                parent_session(fleet, &scope)
            } else {
                None
            };
            for spot in fleet.spots() {
                let in_scope = match parent {
                    Some((host, session)) => spot.host.id == host && spot.session.id == session,
                    None => scope
                        .host
                        .map_or(spot.host.online, |host| spot.host.id == host),
                };
                let window = spot.window;
                if !in_scope || index.is_some_and(|index| window.index != index) {
                    continue;
                }
                let place = Place {
                    host: Some(spot.host.id),
                    session: Some(spot.session.id),
                    window: Some(window.id),
                    pane: None,
                };
                let programs = window
                    .panes
                    .iter()
                    .filter_map(|pane| pane.program.as_deref())
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut entry = Entry::new(
                    place,
                    level,
                    format!("{} {}", window.index, window.name),
                    format!("{} › {}", spot.host.name, spot.session.label),
                )
                .indexed(vec![window.index], programs);
                entry.exact = index.is_some();
                found.push(entry);
            }
        }
        Level::Pane => {
            let parent_window = if scope.window.is_some() {
                scope.window.zip(scope.session).zip(scope.host)
            } else if index.is_some() {
                parent_session(fleet, &scope).and_then(|(host, session)| {
                    let found = fleet
                        .host(host)?
                        .sessions
                        .iter()
                        .find(|s| s.id == session)?;
                    Some(((active_window(found)?.id, session), host))
                })
            } else {
                None
            };
            for spot in fleet.spots() {
                let in_scope = match parent_window {
                    Some(((window, _), host)) => spot.host.id == host && spot.window.id == window,
                    None => {
                        scope
                            .host
                            .map_or(spot.host.online, |host| spot.host.id == host)
                            && scope
                                .session
                                .is_none_or(|session| spot.session.id == session)
                    }
                };
                if !in_scope {
                    continue;
                }
                for pane in &spot.window.panes {
                    if index.is_some_and(|index| pane.index != index) {
                        continue;
                    }
                    let place = Place {
                        host: Some(spot.host.id),
                        session: Some(spot.session.id),
                        window: Some(spot.window.id),
                        pane: Some(pane.id),
                    };
                    let mut entry = Entry::new(
                        place,
                        level,
                        format!("{} {}", pane.index, pane.name()),
                        format!(
                            "{} › {} › {}",
                            spot.host.name, spot.session.label, spot.window.name
                        ),
                    )
                    .indexed(
                        vec![spot.window.index, pane.index],
                        pane.program.clone().unwrap_or_default(),
                    );
                    entry.exact = index.is_some();
                    found.push(entry);
                }
            }
        }
    }
    found
}

fn ranked(
    fleet: &Fleet,
    scope: &Place,
    query: &str,
    entries: &[Entry],
    boost: &dyn Fn(&Place) -> i64,
) -> Vec<Candidate> {
    let current = fleet.current();
    let words = query
        .split(|character: char| character.is_whitespace() || character == '/')
        .filter(|word| !word.is_empty());
    let typed = words.clone().filter_map(digits).collect::<Vec<_>>();
    let rest = words
        .filter(|word| digits(word).is_none())
        .collect::<Vec<_>>()
        .join(" ");
    let (whole, rest) = (Query::new(query), Query::new(&rest));
    let typed_points = typed
        .iter()
        .map(|index| index_points(&index.to_string()))
        .sum::<i64>();
    let picks = entries
        .iter()
        .map(|entry| pick::Candidate {
            kind: Kind::Place,
            label: &entry.label[entry.name_at..],
            path: &entry.path,
            prior: Prior {
                closeness: closeness(&current, &entry.place),
                recent: (boost(&entry.place) as f64 / MAX_BOOST) as f32,
                depth: depth(entry.level, scope),
                changed: false,
            },
        })
        .collect::<Vec<_>>();
    let hits = pick::rank_by(&picks, |at, candidate| {
        let entry = &entries[at];
        let owned = entry.name_at.saturating_sub(1);
        if entry.exact {
            let label = (entry.name_at > 0)
                .then_some(0..owned)
                .into_iter()
                .collect();
            return Some(Match {
                score: index_points(query),
                label,
                path: Vec::new(),
            });
        }
        let metadata = pick::Candidate {
            label: "",
            path: &entry.metadata,
            ..*candidate
        };
        let fallback = |query: &Query| {
            query.matched(candidate).or_else(|| {
                query.matched(&metadata).map(|found| Match {
                    label: Vec::new(),
                    ..found
                })
            })
        };
        if entry.indexes.is_empty() {
            return fallback(&whole);
        }
        if !entry.indexes.starts_with(&typed) {
            return None;
        }
        let mut found = fallback(&rest)?;
        for range in &mut found.label {
            *range = range.start + entry.name_at..range.end + entry.name_at;
        }
        if !typed.is_empty() && typed.len() == entry.indexes.len() {
            found.label.insert(0, 0..owned);
        }
        found.score += typed_points;
        Some(found)
    });
    hits.into_iter()
        .map(|hit| {
            let entry = &entries[hit.index];
            Candidate {
                place: entry.place,
                level: entry.level,
                label: entry.label.clone(),
                matches: hit.label,
                score: hit.score,
            }
        })
        .collect()
}

#[must_use]
pub fn candidates(
    fleet: &Fleet,
    scope: &Place,
    part: &Part,
    boost: &dyn Fn(&Place) -> i64,
) -> Vec<Candidate> {
    let found = entries(fleet, scope, part.level, &part.query);
    ranked(fleet, scope, &part.query, &found, boost)
}

#[must_use]
pub fn children(fleet: &Fleet, scope: &Place, boost: &dyn Fn(&Place) -> i64) -> Vec<Candidate> {
    let level = scope.level().map_or(Some(Level::Host), Level::deeper);
    let Some(level) = level else {
        return Vec::new();
    };
    let part = Part {
        level,
        query: String::new(),
    };
    candidates(fleet, scope, &part, boost)
}

#[must_use]
pub fn search(
    fleet: &Fleet,
    scope: &Place,
    text: &str,
    boost: &dyn Fn(&Place) -> i64,
) -> Vec<Candidate> {
    let below = scope.level().unwrap_or(Level::Host);
    let found = [Level::Session, Level::Window, Level::Pane]
        .into_iter()
        .filter(|level| *level > below)
        .flat_map(|level| entries(fleet, scope, level, text))
        .collect::<Vec<_>>();
    ranked(fleet, scope, text, &found, boost)
}

#[must_use]
pub fn command_target(spec: &CommandSpec, place: &Place) -> Option<String> {
    if place.session.is_none() || spec.name == "new-session" {
        return None;
    }
    let option = spec.option("-t").filter(|option| !option.unsupported)?;
    let session = place.session.map(|id| id.to_string());
    let window = place.window.map(|id| id.to_string());
    let pane = place.pane.map(|id| id.to_string());
    if spec.name == "new-window" {
        return session;
    }
    match option.value {
        Some(CommandValueKind::Session) => session,
        Some(CommandValueKind::Window) => window.or(session),
        _ => pane.or(window).or(session),
    }
}

#[must_use]
pub fn has_target(line: &str) -> bool {
    line.split_whitespace()
        .skip(1)
        .take_while(|token| *token != ";" && *token != "\\;")
        .any(|token| {
            token.starts_with("-t")
                || (token.len() > 1
                    && token.starts_with('-')
                    && !token.starts_with("--")
                    && token[1..].bytes().all(|byte| byte.is_ascii_alphabetic())
                    && token.contains('t'))
        })
}

#[must_use]
pub fn with_target(line: &str, target: &str) -> String {
    if has_target(line) {
        return line.to_owned();
    }
    let lead = line.len() - line.trim_start().len();
    let end = line[lead..]
        .find(char::is_whitespace)
        .map_or(line.len(), |offset| lead + offset);
    let quoted = if target.contains('$') {
        format!("'{target}'")
    } else {
        target.to_owned()
    };
    format!("{} -t {quoted}{}", &line[..end], &line[end..])
}

#[must_use]
pub fn written(fleet: &Fleet, place: &Place) -> Option<String> {
    let host = fleet.host(place.host?)?;
    let tail = place
        .pane
        .map(|id| id.to_string())
        .or_else(|| place.window.map(|id| id.to_string()))
        .or_else(|| place.session.map(|id| id.to_string()));
    Some(match tail {
        Some(tail) => format!("@{}:{tail}", host.name),
        None => format!("@{}", host.name),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Written {
    Place(Place),
    Path {
        host: u32,
        path: String,
        line: Option<u32>,
    },
}

#[must_use]
pub fn read_written(text: &str, fleet: &Fleet) -> Option<Written> {
    let body = text.trim().strip_prefix('@')?;
    if body.is_empty() || body.contains(char::is_control) {
        return None;
    }
    if let Some(host) = fleet.host_named(body) {
        return Some(Written::Place(Place::host(host.id)));
    }
    let (first, rest) = body.split_once(':')?;
    let host = fleet.host_named(first)?;
    if rest.starts_with(['/', '~']) {
        let (path, line) = split_line(rest);
        return Some(Written::Path {
            host: host.id,
            path: unescape(path),
            line,
        });
    }
    if rest.contains(char::is_whitespace) && !rest.contains('\\') {
        return None;
    }
    read_target(fleet, host.id, rest).map(Written::Place)
}

fn read_target(fleet: &Fleet, host: u32, target: &str) -> Option<Place> {
    let place = |spot: &Spot, pane: Option<PaneId>| Place {
        host: Some(host),
        session: Some(spot.session.id),
        window: Some(spot.window.id),
        pane,
    };
    if let Ok(pane) = PaneId::from_str(target) {
        let spot = fleet.spot_of(host, |spot| spot.window.panes.iter().any(|p| p.id == pane))?;
        return Some(place(&spot, Some(pane)));
    }
    if let Ok(window) = WindowId::from_str(target) {
        let spot = fleet.spot_of(host, |spot| spot.window.id == window)?;
        return Some(place(&spot, None));
    }
    let (session_text, window_text) = match target.split_once(':') {
        Some((session, window)) => (session, Some(window)),
        None => (target, None),
    };
    let session_text = unescape(session_text);
    let found =
        fleet.host(host)?.sessions.iter().find(|session| {
            session.name == session_text || session.id.to_string() == session_text
        })?;
    let Some(window_text) = window_text else {
        return Some(Place {
            host: Some(host),
            session: Some(found.id),
            window: None,
            pane: None,
        });
    };
    let (window_text, pane_text) = match window_text.split_once('.') {
        Some((window, pane)) => (window, Some(pane)),
        None => (window_text, None),
    };
    let window_text = unescape(window_text);
    let window = found
        .windows
        .iter()
        .find(|window| {
            window.index.to_string() == window_text || window.id.to_string() == window_text
        })
        .or_else(|| {
            found
                .windows
                .iter()
                .find(|window| window.name == window_text)
        })?;
    let pane = match pane_text {
        Some(text) => Some(
            window
                .panes
                .iter()
                .find(|pane| pane.id.to_string() == text || pane.index.to_string() == text)?
                .id,
        ),
        None => None,
    };
    Some(Place {
        host: Some(host),
        session: Some(found.id),
        window: Some(window.id),
        pane,
    })
}

fn split_line(rest: &str) -> (&str, Option<u32>) {
    let mut path = rest;
    let mut numbers = Vec::new();
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((head, tail))
                if !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                numbers.push(tail);
                path = head;
            }
            _ => break,
        }
    }
    (path, numbers.last().and_then(|line| line.parse().ok()))
}

fn unescape(text: &str) -> String {
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
pub fn recent_key(fleet: &Fleet, place: &Place) -> Option<RecentKey> {
    let host = fleet.host(place.host?)?;
    Some(RecentKey::Place {
        host: host.name.clone(),
        session: place.session?,
        window: place.window,
        pane: place.pane,
    })
}

#[must_use]
pub fn recent_place(fleet: &Fleet, key: &RecentKey) -> Option<Place> {
    let RecentKey::Place {
        host,
        session,
        window,
        pane,
    } = key
    else {
        return None;
    };
    let host = fleet.host_named(host)?;
    let place = Place {
        host: Some(host.id),
        session: Some(*session),
        window: *window,
        pane: *pane,
    };
    let exists = match (window, pane) {
        (_, Some(_)) => fleet.pane(&place).is_some(),
        (Some(_), None) => fleet.window(&place).is_some(),
        (None, None) => fleet.session(&place).is_some(),
    };
    exists.then_some(place)
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
