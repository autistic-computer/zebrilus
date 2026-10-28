use std::{ops::Range, str::FromStr};

use zz_protocol::{CommandSpec, CommandValueKind, PaneId, SessionId, WindowId};

use crate::{fuzzy::fuzzy_match, recents::RecentKey};

const DISTANCE_STEP: i64 = 4;
const METADATA_PENALTY: i64 = 1000;

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
    pub const fn sigil(self) -> char {
        match self {
            Self::Host => '^',
            Self::Session => '$',
            Self::Window => '@',
            Self::Pane => '%',
        }
    }

    #[must_use]
    pub const fn from_sigil(character: char) -> Option<Self> {
        match character {
            '^' => Some(Self::Host),
            '$' => Some(Self::Session),
            '@' => Some(Self::Window),
            '%' => Some(Self::Pane),
            _ => None,
        }
    }

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
    Ask,
}

impl ActionKind {
    #[must_use]
    pub const fn sigil(self) -> char {
        match self {
            Self::Command => ':',
            Self::Gui => '>',
            Self::Files => '&',
            Self::Ask => '?',
        }
    }

    #[must_use]
    pub const fn from_sigil(character: char) -> Option<Self> {
        match character {
            ':' => Some(Self::Command),
            '>' => Some(Self::Gui),
            '&' => Some(Self::Files),
            '?' => Some(Self::Ask),
            _ => None,
        }
    }
}

#[must_use]
pub const fn is_sigil(character: char) -> bool {
    Level::from_sigil(character).is_some() || ActionKind::from_sigil(character).is_some()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Text(String),
    Id(u64),
}

impl Query {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Text(text) if text.is_empty())
    }

    #[must_use]
    pub fn index(&self) -> Option<u32> {
        match self {
            Self::Text(text)
                if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                text.parse().ok()
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub level: Level,
    pub query: Query,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub kind: ActionKind,
    pub rest: String,
    pub start: usize,
}

impl Action {
    #[must_use]
    pub const fn rest_range(&self) -> Range<usize> {
        self.start + 1..self.start + 1 + self.rest.len()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub parts: Vec<Part>,
    pub action: Option<Action>,
    pub search: Option<String>,
}

#[must_use]
pub fn parse(text: &str) -> Line {
    let start = text.len() - text.trim_start().len();
    let mut line = Line::default();
    let Some(first) = text[start..].chars().next() else {
        return line;
    };
    if let Some(kind) = ActionKind::from_sigil(first) {
        line.action = Some(Action {
            kind,
            rest: text[start + 1..].to_owned(),
            start,
        });
        return line;
    }
    let Some(mut level) = Level::from_sigil(first) else {
        line.search = Some(text[start..].trim_end().to_owned());
        return line;
    };
    let mut part_start = start;
    let mut query = String::new();
    let mut quoted = false;
    let mut literal = false;
    for (offset, character) in text[start + 1..].char_indices() {
        let index = start + 1 + offset;
        if character == '"' {
            quoted = !quoted;
            literal = true;
            continue;
        }
        if !quoted {
            if let Some(kind) = ActionKind::from_sigil(character) {
                line.parts
                    .push(part(level, &query, literal, part_start..index));
                line.action = Some(Action {
                    kind,
                    rest: text[index + 1..].to_owned(),
                    start: index,
                });
                return line;
            }
            if let Some(next) = Level::from_sigil(character).filter(|next| *next > level) {
                line.parts
                    .push(part(level, &query, literal, part_start..index));
                level = next;
                part_start = index;
                query.clear();
                literal = false;
                continue;
            }
        }
        query.push(character);
    }
    line.parts
        .push(part(level, &query, literal, part_start..text.len()));
    line
}

fn part(level: Level, query: &str, literal: bool, range: Range<usize>) -> Part {
    let text = query.trim();
    let id = text
        .strip_prefix('#')
        .filter(|digits| {
            !literal && !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
        .and_then(|digits| digits.parse().ok());
    Part {
        level,
        query: id.map_or_else(|| Query::Text(text.to_owned()), Query::Id),
        range,
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

fn distance(current: &Place, place: &Place) -> i64 {
    if place.host != current.host {
        3
    } else if place.session.is_some() && place.session != current.session {
        2
    } else {
        i64::from(place.window.is_some() && place.window != current.window)
    }
}

fn rate(query: &Query, label: &str, metadata: &str, id: u64) -> Option<(i64, Vec<Range<usize>>)> {
    match query {
        Query::Id(wanted) => (*wanted == id).then(|| (0, Vec::new())),
        Query::Text(text) => {
            if let Some((score, matches)) = fuzzy_match(text, label) {
                return Some((score as i64, matches));
            }
            if text.is_empty() || metadata.is_empty() {
                return None;
            }
            fuzzy_match(text, metadata)
                .map(|(score, _)| (score as i64 + METADATA_PENALTY, Vec::new()))
        }
    }
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

#[must_use]
pub fn candidates(
    fleet: &Fleet,
    scope: &Place,
    part: &Part,
    boost: &dyn Fn(&Place) -> i64,
) -> Vec<Candidate> {
    let scope = scope.above(part.level);
    let current = fleet.current();
    let index = part.query.index();
    let mut found = Vec::new();
    let mut push = |place: Place, label: String, scored: Option<(i64, Vec<Range<usize>>)>| {
        if let Some((score, matches)) = scored {
            found.push(Candidate {
                place,
                level: part.level,
                label,
                matches,
                score: score + DISTANCE_STEP * distance(&current, &place) - boost(&place),
            });
        }
    };
    match part.level {
        Level::Host => {
            for host in &fleet.hosts {
                let scored = match &part.query {
                    Query::Text(text) => fuzzy_match(text, &host.name)
                        .map(|(score, matches)| (score as i64, matches)),
                    Query::Id(_) => None,
                };
                push(Place::host(host.id), host.name.clone(), scored);
            }
        }
        Level::Session => {
            let exact = index.is_some()
                && host_scope(fleet, &scope)
                    .flat_map(|host| &host.sessions)
                    .any(|session| Query::Text(session.name.clone()) == part.query);
            for host in host_scope(fleet, &scope) {
                for session in &host.sessions {
                    let place = Place {
                        host: Some(host.id),
                        session: Some(session.id),
                        window: None,
                        pane: None,
                    };
                    let scored = if exact {
                        (Query::Text(session.name.clone()) == part.query).then(|| (0, Vec::new()))
                    } else {
                        rate(&part.query, &session.label, &session.name, session.id.0)
                    };
                    push(place, session.label.clone(), scored);
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
                if !in_scope {
                    continue;
                }
                let window = spot.window;
                let label = format!("{} {}", window.index, window.name);
                let scored = if let Some(index) = index {
                    (window.index == index).then(|| (0, Vec::new()))
                } else {
                    let programs = window
                        .panes
                        .iter()
                        .filter_map(|pane| pane.program.as_deref())
                        .collect::<Vec<_>>()
                        .join(" ");
                    rate(&part.query, &label, &programs, window.id.0)
                };
                let place = Place {
                    host: Some(spot.host.id),
                    session: Some(spot.session.id),
                    window: Some(window.id),
                    pane: None,
                };
                push(place, label, scored);
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
                    let label = format!("{} {}", pane.index, pane.name());
                    let scored = match index {
                        Some(index) => (pane.index == index).then(|| (0, Vec::new())),
                        None => rate(
                            &part.query,
                            &label,
                            pane.program.as_deref().unwrap_or_default(),
                            pane.id.0,
                        ),
                    };
                    let place = Place {
                        host: Some(spot.host.id),
                        session: Some(spot.session.id),
                        window: Some(spot.window.id),
                        pane: Some(pane.id),
                    };
                    push(place, label, scored);
                }
            }
        }
    }
    found.sort_by_key(|candidate| candidate.score);
    found
}

#[must_use]
pub fn children(fleet: &Fleet, scope: &Place, boost: &dyn Fn(&Place) -> i64) -> Vec<Candidate> {
    let level = scope.level().map_or(Some(Level::Host), Level::deeper);
    let Some(level) = level else {
        return Vec::new();
    };
    let part = Part {
        level,
        query: Query::Text(String::new()),
        range: 0..0,
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
    let mut found = [Level::Session, Level::Window, Level::Pane]
        .into_iter()
        .filter(|level| *level > below)
        .flat_map(|level| {
            let part = Part {
                level,
                query: Query::Text(text.to_owned()),
                range: 0..0,
            };
            candidates(fleet, scope, &part, boost)
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|candidate| candidate.score);
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub place: Place,
    pub failed: Option<usize>,
}

#[must_use]
pub fn resolve(
    fleet: &Fleet,
    scope: &Place,
    parts: &[Part],
    boost: &dyn Fn(&Place) -> i64,
) -> Resolution {
    let mut place = *scope;
    for (index, part) in parts.iter().enumerate() {
        match candidates(fleet, &place, part, boost).into_iter().next() {
            Some(best) => place = best.place,
            None => {
                return Resolution {
                    place,
                    failed: Some(index),
                };
            }
        }
    }
    Resolution {
        place,
        failed: None,
    }
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
