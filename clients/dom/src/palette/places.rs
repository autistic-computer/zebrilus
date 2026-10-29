use std::collections::BTreeSet;

use serde_json::{Value, json};
use zz_client::{
    completion::{PaneKindAvailability, apply_completion},
    palette::{count, has_command_arguments, needs_arguments},
    place::{
        self, COMMAND_SIGIL, Candidate, Fleet, FleetHost, FleetPane, FleetSession, FleetWindow,
        Level, Part, Place, Written,
    },
    recents::{RecentKind, Recents, now_secs},
    verbs::{self, Filled, Slot, Verb, VerbKind, VerbMatch},
};
use zz_protocol::{
    CommandInvocation, MAX_COMMAND_PROMPT_BYTES, MuxOptionKey, catalog_command_spec,
};

use super::{
    Action, Entry, HOST, Palette, Row, Target, TreePane, TreeSession, append_section, hint,
};
use crate::Client;

const PLACE_ROWS: usize = 50;
const VERB_ROWS: usize = 30;
const RECENT_PLACES: usize = 5;
const RECENT_ACTIONS: usize = 3;
const PLACEHOLDER: &str = "Type a place or an action";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Chip {
    level: Level,
    place: Place,
    label: String,
    auto: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Draft {
    verb: Verb,
    filled: Vec<(Filled, String)>,
}

impl Draft {
    const fn new(verb: Verb) -> Self {
        Self {
            verb,
            filled: Vec::new(),
        }
    }

    fn next(&self) -> Option<&Slot> {
        self.verb.slots.get(self.filled.len())
    }

    fn values(&self) -> Vec<Filled> {
        self.filled.iter().map(|(value, _)| value.clone()).collect()
    }
}

#[derive(Default)]
pub(super) struct Places {
    chips: Vec<Chip>,
    draft: Option<Draft>,
    verbs: Vec<Verb>,
    fleet: Fleet,
    recents: Recents,
}

fn fleet(tree: &[TreeSession], online: bool) -> Fleet {
    Fleet {
        here: 0,
        hosts: vec![FleetHost {
            id: 0,
            name: HOST.to_owned(),
            online,
            current: tree
                .iter()
                .find(|session| session.active)
                .map(|session| session.id),
            sessions: tree
                .iter()
                .filter(|_| online)
                .map(|session| FleetSession {
                    id: session.id,
                    name: session.tmux_name.clone(),
                    label: session.name.clone(),
                    windows: session
                        .windows
                        .iter()
                        .map(|window| FleetWindow {
                            id: window.id,
                            index: window.index,
                            name: window.name.clone(),
                            active: window.active,
                            active_pane: window.active_pane,
                            panes: window
                                .panes
                                .iter()
                                .map(|pane| FleetPane {
                                    id: pane.id,
                                    index: pane.index,
                                    title: pane.label.clone(),
                                    program: pane.program.map(str::to_owned),
                                    cwd: pane.cwd.clone(),
                                    terminal: pane.terminal,
                                    needs_you: pane.needs_you,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }],
    }
}

fn place_label(fleet: &Fleet, place: &Place) -> Option<String> {
    match place.level()? {
        Level::Host => fleet.host(place.host?).map(|host| host.name.clone()),
        Level::Session => fleet.session(place).map(|session| session.label.clone()),
        Level::Window => fleet
            .window(place)
            .map(|window| format!("{} {}", window.index, window.name)),
        Level::Pane => fleet
            .pane(place)
            .map(|pane| format!("{} {}", pane.index, pane.name())),
    }
}

fn chip_for(fleet: &Fleet, place: Place) -> Option<Chip> {
    Some(Chip {
        level: place.level()?,
        label: place_label(fleet, &place)?,
        place,
        auto: false,
    })
}

fn chips_to(fleet: &Fleet, place: Place) -> Vec<Chip> {
    Level::ALL
        .into_iter()
        .filter(|level| match level {
            Level::Host => place.host.is_some(),
            Level::Session => place.session.is_some(),
            Level::Window => place.window.is_some(),
            Level::Pane => place.pane.is_some(),
        })
        .filter_map(|level| {
            let at = level.deeper().map_or(place, |deeper| place.above(deeper));
            chip_for(fleet, at)
        })
        .collect()
}

fn level_header(fleet: &Fleet, level: Level, parent: &Place) -> String {
    let host = parent
        .host
        .and_then(|id| fleet.host(id))
        .map(|host| host.name.as_str());
    let on_host =
        |noun: &str| host.map_or_else(|| noun.to_owned(), |host| format!("{noun} on {host}"));
    match level {
        Level::Host => "Hosts".to_owned(),
        Level::Session => on_host("Sessions"),
        Level::Window => fleet.session(parent).map_or_else(
            || on_host("Windows"),
            |session| format!("Windows in {}", session.label),
        ),
        Level::Pane => fleet
            .window(parent)
            .map(|window| format!("Panes in {} {}", window.index, window.name))
            .or_else(|| {
                fleet
                    .session(parent)
                    .map(|session| format!("Panes in {}", session.label))
            })
            .unwrap_or_else(|| on_host("Panes")),
    }
}

fn crumbs(fleet: &Fleet, place: &Place, level: Level, parent: &Place) -> String {
    let from = parent.level();
    let mut parts = Vec::new();
    for between in [Level::Host, Level::Session, Level::Window] {
        if between >= level || from.is_some_and(|from| between <= from) {
            continue;
        }
        if between == Level::Host && fleet.hosts.len() <= 1 {
            continue;
        }
        if let Some(label) = place_label(fleet, &place.above(between.deeper().unwrap_or(level))) {
            parts.push(label);
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{} › ", parts.join(" › "))
    }
}

fn tree_pane<'a>(tree: &'a [TreeSession], place: &Place) -> Option<&'a TreePane> {
    let pane = place.pane?;
    tree.iter()
        .flat_map(|session| &session.windows)
        .flat_map(|window| &window.panes)
        .find(|candidate| candidate.id == pane)
}

fn running(tree: &[TreeSession], place: &Place) -> bool {
    tree.iter()
        .filter(|session| place.session.is_none_or(|id| id == session.id))
        .flat_map(|session| &session.windows)
        .filter(|window| place.window.is_none_or(|id| id == window.id))
        .flat_map(|window| &window.panes)
        .filter(|pane| place.pane.is_none_or(|id| id == pane.id))
        .any(|pane| pane.running)
}

fn is_current(current: &Place, place: &Place, level: Level) -> bool {
    place.host == current.host
        && match level {
            Level::Host => true,
            Level::Session => place.session == current.session,
            Level::Window => place.window == current.window,
            Level::Pane => place.pane == current.pane,
        }
}

fn place_row(fleet: &Fleet, tree: &[TreeSession], candidate: &Candidate, parent: &Place) -> Row {
    let place = candidate.place;
    let prefix = crumbs(fleet, &place, candidate.level, parent);
    let shift = prefix.len();
    let (detail, icon, status) = match candidate.level {
        Level::Host => {
            let host = place.host.and_then(|id| fleet.host(id));
            let online = host.is_some_and(|host| host.online);
            (
                host.filter(|_| online).map_or_else(
                    || "offline".to_owned(),
                    |host| count(host.sessions.len(), "session"),
                ),
                "hard-drive",
                Some(if online { "online" } else { "offline" }),
            )
        }
        Level::Session => (
            fleet.session(&place).map_or_else(String::new, |session| {
                count(session.windows.len(), "window")
            }),
            "folder",
            None,
        ),
        Level::Window => (
            fleet
                .window(&place)
                .map_or_else(String::new, |window| count(window.panes.len(), "pane")),
            "panels-top-left",
            None,
        ),
        Level::Pane => tree_pane(tree, &place).map_or_else(
            || (String::new(), "square-terminal", None),
            |pane| (pane.detail.to_owned(), pane.icon, None),
        ),
    };
    Row {
        label: format!("{prefix}{}", candidate.label),
        matches: candidate
            .matches
            .iter()
            .map(|range| range.start + shift..range.end + shift)
            .collect(),
        muted_prefix: shift,
        detail,
        right: if is_current(&fleet.current(), &place, candidate.level) {
            "current"
        } else {
            ""
        }
        .to_owned(),
        status,
        running: candidate.level != Level::Host && running(tree, &place),
        icon: Some(icon),
        ..Row::default()
    }
}

fn place_entry(
    fleet: &Fleet,
    tree: &[TreeSession],
    candidate: &Candidate,
    parent: &Place,
) -> Entry {
    Entry::row(
        place_row(fleet, tree, candidate, parent),
        Action::Place(candidate.place),
    )
}

fn candidate_at(fleet: &Fleet, place: Place) -> Option<Candidate> {
    Some(Candidate {
        level: place.level()?,
        label: place_label(fleet, &place)?,
        place,
        matches: Vec::new(),
        score: 0,
    })
}

const fn verb_icon(verb: &Verb) -> &'static str {
    match verb.kind {
        VerbKind::Tmux | VerbKind::Alias => "terminal",
        VerbKind::Gui => "settings",
        VerbKind::Zz => "file",
    }
}

fn verb_entry(
    verbs: &[Verb],
    found: &VerbMatch,
    shortcut: &dyn Fn(&str) -> Option<String>,
) -> Entry {
    let verb = &verbs[found.index];
    let plain = matches!(&verb.run, verbs::Run::Tmux { args, .. } if args.is_empty());
    Entry::row(
        Row {
            label: verb.name.clone(),
            matches: found.matches.clone(),
            detail: verb.description.clone(),
            right: verb.spelled(),
            icon: Some(verb_icon(verb)),
            shortcut: verb.command().filter(|_| plain).and_then(shortcut),
            ..Row::default()
        },
        Action::Verb(found.index),
    )
}

const fn slot_noun(slot: &Slot) -> &'static str {
    match slot {
        Slot::Target(Level::Host) => "Pick a host",
        Slot::Target(Level::Session) => "Pick a session",
        Slot::Target(Level::Window) => "Pick a window",
        Slot::Target(Level::Pane) => "Pick a pane",
        Slot::Text(_) => "",
        Slot::Value(_) => "Pick a value",
        Slot::Path => "A folder or file",
    }
}

impl Palette {
    pub(super) fn raw_command(&self) -> Option<String> {
        if self.places.draft.is_some() {
            return None;
        }
        place::parse(&self.input).command
    }

    pub(super) fn is_places(&self) -> bool {
        self.unified
            && self.window_chooser.is_none()
            && self.mode.is_none()
            && self.command.is_none()
    }

    fn scope(&self) -> Place {
        self.places
            .chips
            .last()
            .map_or_else(Place::default, |chip| chip.place)
    }

    pub(super) fn place_pills(&self) -> Vec<Value> {
        let mut pills = self
            .places
            .chips
            .iter()
            .map(|chip| match chip.level {
                Level::Host => json!({
                    "kind": "host",
                    "label": chip.label,
                    "status": if self.connected { "online" } else { "offline" },
                }),
                _ => json!({ "kind": "place", "label": chip.label }),
            })
            .collect::<Vec<_>>();
        if let Some(draft) = &self.places.draft {
            pills.push(json!({ "kind": "verb", "label": draft.verb.name }));
            pills.extend(
                draft
                    .filled
                    .iter()
                    .filter(|(_, label)| !label.is_empty())
                    .map(|(_, label)| json!({ "kind": "place", "label": label })),
            );
        }
        pills
    }

    pub(super) fn place_placeholder(&self) -> String {
        match self.places.draft.as_ref().and_then(Draft::next) {
            Some(Slot::Text(label)) => {
                let mut label = label.clone();
                if let Some(first) = label.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                label
            }
            Some(slot) => slot_noun(slot).to_owned(),
            None => PLACEHOLDER.to_owned(),
        }
    }

    pub(super) fn place_usage(&self) -> Option<String> {
        if let Some(rest) = self.raw_command() {
            return self
                .selected_action()
                .and_then(|action| match action {
                    Action::Command(spec) => Some(spec.usage),
                    _ => None,
                })
                .or_else(|| {
                    rest.split_whitespace()
                        .next()
                        .and_then(catalog_command_spec)
                        .map(|spec| spec.usage)
                })
                .map(str::to_owned);
        }
        self.draft_usage().or_else(|| self.row_usage())
    }

    fn draft_usage(&self) -> Option<String> {
        let draft = self.places.draft.as_ref()?;
        let mut values = draft.values();
        match (draft.next(), self.selected_action()) {
            (Some(Slot::Target(_)), Some(Action::Fill(place))) => {
                values.push(Filled::Place(*place));
            }
            (Some(Slot::Text(_)), _) if !self.input.trim().is_empty() => {
                values.push(Filled::Text(self.input.trim().to_owned()));
            }
            _ => {}
        }
        Some(format!(
            "runs  {}",
            verbs::compile(&draft.verb, &values, &self.places.fleet)
                .map_or_else(|| draft.verb.spelled(), |compiled| compiled.display)
        ))
    }

    fn row_usage(&self) -> Option<String> {
        match self.selected_action()? {
            Action::Verb(index) => {
                let verb = self.places.verbs.get(*index)?;
                let scope = self.scope();
                let filled = if matches!(verb.slots.first(), Some(Slot::Target(_)))
                    && scope.level().is_some_and(|level| level > Level::Host)
                {
                    vec![Filled::Place(scope)]
                } else {
                    Vec::new()
                };
                let line = verbs::compile(verb, &filled, &self.places.fleet)
                    .map_or_else(|| verb.spelled(), |compiled| compiled.display);
                Some(format!("runs  {line}"))
            }
            Action::Place(place) => {
                verbs::go_to_line(&self.places.fleet, place).map(|line| format!("runs  {line}"))
            }
            _ => None,
        }
    }

    pub(super) fn place_empty(&self) -> Option<&'static str> {
        let text = self.places.draft.as_ref().and_then(Draft::next);
        (!matches!(text, Some(Slot::Text(_)))).then_some("No matches")
    }

    pub(super) fn rebuild_places(
        &mut self,
        tree: &[TreeSession],
        history: &[String],
        availability: PaneKindAvailability,
        shortcut: &dyn Fn(&str) -> Option<String>,
    ) {
        self.places.fleet = fleet(tree, self.connected);
        let line = place::parse(&self.input);
        if let Some(command) = self.raw_command() {
            self.rows = self.command_entries(command.trim(), availability, shortcut);
            return;
        }
        let now = now_secs();
        let boost = |place: &Place| {
            place::recent_key(&self.places.fleet, place)
                .map_or(0, |key| self.places.recents.boost(&key, now))
        };
        let pane = self.scope().level() == Some(Level::Pane);
        let search = line.search.or_else(|| pane.then(String::new));
        let rows = if let Some(draft) = &self.places.draft {
            self.slot_rows(tree, draft, self.input.trim(), &boost)
        } else if let Some(search) = &search {
            self.search_rows(tree, search, &boost, shortcut)
        } else if self.places.chips.iter().all(|chip| chip.auto) {
            self.empty_rows(tree, history, availability, now, &boost)
        } else {
            let fleet = &self.places.fleet;
            let scope = self.scope();
            let level = scope.level().and_then(Level::deeper).unwrap_or(Level::Host);
            let entries = place::children(fleet, &scope, &boost)
                .iter()
                .take(PLACE_ROWS)
                .map(|candidate| place_entry(fleet, tree, candidate, &scope))
                .collect();
            let mut rows = Vec::new();
            append_section(&mut rows, &level_header(fleet, level, &scope), entries);
            rows
        };
        self.rows = rows;
    }

    fn search_rows(
        &self,
        tree: &[TreeSession],
        search: &str,
        boost: &dyn Fn(&Place) -> i64,
        shortcut: &dyn Fn(&str) -> Option<String>,
    ) -> Vec<Entry> {
        let fleet = &self.places.fleet;
        let verbs = &self.places.verbs;
        let scope = self.scope();
        let drilled = scope.level().is_some_and(|level| level > Level::Host);
        let found = verbs::match_verbs(verbs, search, |verb| !drilled || verb.target().is_some());
        let places = place::search(fleet, &scope, search, boost);
        let keys = |name: &str| (!self.hide_keys).then(|| shortcut(name)).flatten();
        let verb_rows = found
            .iter()
            .take(VERB_ROWS)
            .map(|found| verb_entry(verbs, found, &keys))
            .collect();
        let place_rows = places
            .iter()
            .take(PLACE_ROWS)
            .map(|candidate| place_entry(fleet, tree, candidate, &scope))
            .collect();
        let actions = drilled
            .then(|| place_label(fleet, &scope))
            .flatten()
            .map_or_else(
                || "Actions".to_owned(),
                |label| format!("Actions on {label}"),
            );
        let verbs_first = drilled
            || match (found.first(), places.first()) {
                (Some(verb), Some(place)) => verb.score < place.score,
                (Some(_), None) => true,
                _ => false,
            };
        let mut rows = Vec::new();
        if verbs_first {
            append_section(&mut rows, &actions, verb_rows);
            append_section(&mut rows, "Places", place_rows);
        } else {
            append_section(&mut rows, "Places", place_rows);
            append_section(&mut rows, &actions, verb_rows);
        }
        if !self
            .places
            .chips
            .iter()
            .any(|chip| chip.level == Level::Host)
        {
            let part = Part {
                level: Level::Host,
                query: search.to_owned(),
            };
            let root = Place::default();
            let hosts = place::candidates(fleet, &root, &part, boost)
                .iter()
                .map(|candidate| place_entry(fleet, tree, candidate, &root))
                .collect();
            append_section(&mut rows, "Hosts", hosts);
        }
        rows
    }

    fn slot_rows(
        &self,
        tree: &[TreeSession],
        draft: &Draft,
        query: &str,
        boost: &dyn Fn(&Place) -> i64,
    ) -> Vec<Entry> {
        let fleet = &self.places.fleet;
        match draft.next() {
            Some(Slot::Target(level)) => {
                let host = Place::host(self.scope().host.unwrap_or(fleet.here));
                let part = Part {
                    level: *level,
                    query: query.to_owned(),
                };
                let current = fleet.current();
                let mut found = place::candidates(fleet, &host, &part, boost);
                if query.is_empty() {
                    found.sort_by_key(|candidate| !is_current(&current, &candidate.place, *level));
                }
                let entries = found
                    .iter()
                    .take(PLACE_ROWS)
                    .map(|candidate| {
                        Entry::row(
                            place_row(fleet, tree, candidate, &host),
                            Action::Fill(candidate.place),
                        )
                    })
                    .collect();
                let mut rows = Vec::new();
                append_section(&mut rows, &level_header(fleet, *level, &host), entries);
                rows
            }
            Some(Slot::Text(label)) if !query.is_empty() => vec![Entry::row(
                Row {
                    label: query.to_owned(),
                    detail: label.clone(),
                    right: draft.verb.name.clone(),
                    icon: Some("pencil"),
                    ..Row::default()
                },
                Action::Submit,
            )],
            _ => Vec::new(),
        }
    }

    fn empty_rows(
        &self,
        tree: &[TreeSession],
        history: &[String],
        availability: PaneKindAvailability,
        now: u64,
        boost: &dyn Fn(&Place) -> i64,
    ) -> Vec<Entry> {
        let fleet = &self.places.fleet;
        let root = Place::default();
        let mut rows = Vec::new();
        let needs = fleet
            .hosts
            .iter()
            .filter(|host| host.online)
            .flat_map(|host| {
                host.sessions.iter().flat_map(move |session| {
                    session.windows.iter().filter_map(move |window| {
                        let pane = window.panes.iter().find(|pane| pane.needs_you)?;
                        Some(Place {
                            host: Some(host.id),
                            session: Some(session.id),
                            window: Some(window.id),
                            pane: Some(pane.id),
                        })
                    })
                })
            })
            .filter_map(|place| {
                let mut candidate = candidate_at(fleet, place.above(Level::Pane))?;
                candidate.place = place;
                candidate.level = Level::Window;
                let mut entry = place_entry(fleet, tree, &candidate, &root);
                if let Some(pane) = tree_pane(tree, &place) {
                    pane.detail.clone_into(&mut entry.row.detail);
                }
                Some(entry)
            })
            .collect();
        append_section(&mut rows, "Needs you", needs);
        let current = fleet.current();
        let recent = self
            .places
            .recents
            .top(RecentKind::Place, RECENT_PLACES * 4, now)
            .into_iter()
            .filter_map(|recent| place::recent_place(fleet, &recent.key))
            .filter(|place| place.window.is_none() || place.window != current.window)
            .take(RECENT_PLACES)
            .filter_map(|place| {
                let mut candidate = candidate_at(fleet, place.above(Level::Pane))?;
                candidate.place = place;
                Some(place_entry(fleet, tree, &candidate, &root))
            })
            .collect();
        append_section(&mut rows, "Recent places", recent);
        let mut seen = BTreeSet::new();
        let actions = history
            .iter()
            .filter(|command| {
                let name = command.split_whitespace().next().unwrap_or(command);
                availability.allows_command(name) && seen.insert(command.as_str())
            })
            .take(RECENT_ACTIONS)
            .map(|command| {
                Entry::row(
                    Row {
                        label: command
                            .split_whitespace()
                            .next()
                            .unwrap_or(command)
                            .to_owned(),
                        icon: Some("terminal"),
                        right: command.clone(),
                        ..Row::default()
                    },
                    Action::History(command.clone()),
                )
            })
            .collect();
        append_section(&mut rows, "Recent actions", actions);
        let scope = self.scope();
        if scope.host.is_some() {
            let sessions = place::children(fleet, &scope, boost)
                .iter()
                .take(PLACE_ROWS)
                .map(|candidate| place_entry(fleet, tree, candidate, &scope))
                .collect();
            append_section(
                &mut rows,
                &level_header(fleet, Level::Session, &scope),
                sessions,
            );
        }
        let hosts = fleet
            .hosts
            .iter()
            .filter(|host| Some(host.id) != scope.host)
            .filter_map(|host| candidate_at(fleet, Place::host(host.id)))
            .map(|candidate| place_entry(fleet, tree, &candidate, &root))
            .collect();
        append_section(
            &mut rows,
            if scope.host.is_some() {
                "Other hosts"
            } else {
                "Hosts"
            },
            hosts,
        );
        rows
    }
}

impl Client {
    pub(super) fn start_places(&mut self, field: &str) {
        self.palette.connected = self.core.attached_session().is_some();
        let fleet = fleet(&self.palette_tree(), self.palette.connected);
        let aliases = self
            .core
            .mux_options()
            .get(MuxOptionKey::CommandAlias)
            .map(|option| option.value.clone())
            .unwrap_or_default();
        let places = &mut self.palette.places;
        places.verbs = verbs::verbs(Self::availability(), &[], false, &aliases);
        places.draft = None;
        places.chips = chip_for(&fleet, Place::host(fleet.here))
            .map(|chip| Chip { auto: true, ..chip })
            .into_iter()
            .collect();
        places.fleet = fleet;
        self.set_query(field.to_owned());
    }

    pub(super) fn commit_typed(&mut self, value: &str) -> bool {
        let palette = &self.palette;
        if !palette.is_places()
            || !palette.input.is_empty()
            || value.chars().count() < 2
            || palette.places.draft.is_some()
        {
            return false;
        }
        let Some(Written::Place(place)) = place::read_written(value, &palette.places.fleet) else {
            return false;
        };
        self.palette.places.chips = chips_to(&self.palette.places.fleet, place);
        self.set_query(String::new());
        true
    }

    pub(super) fn activate_places(&mut self, complete: bool) {
        if let Some(rest) = self.palette.raw_command() {
            self.activate_place_command(rest, complete);
            return;
        }
        if let Some(slot) = self
            .palette
            .places
            .draft
            .as_ref()
            .and_then(Draft::next)
            .cloned()
        {
            match slot {
                Slot::Target(_) => {
                    if let Some(&Action::Fill(place)) = self.palette.selected_action() {
                        let label =
                            place_label(&self.palette.places.fleet, &place).unwrap_or_default();
                        self.fill(Filled::Place(place), label);
                    }
                }
                Slot::Text(_) => {
                    let text = self.palette.input.trim().to_owned();
                    if !text.is_empty() {
                        self.fill(Filled::Text(text.clone()), text);
                    }
                }
                Slot::Value(_) | Slot::Path => {}
            }
            return;
        }
        match self.palette.selected_action().cloned() {
            Some(Action::Place(place)) => {
                if complete {
                    self.enter_place(place);
                } else {
                    self.go_to(place);
                }
            }
            Some(Action::Verb(index)) => {
                if let Some(verb) = self.palette.places.verbs.get(index).cloned() {
                    self.start_verb(verb);
                }
            }
            Some(Action::History(command)) if !complete => self.run_place_command(command, None),
            None if !complete => {
                let scope = self.palette.scope();
                if scope != Place::default()
                    && !self.palette.places.chips.iter().all(|chip| chip.auto)
                {
                    self.go_to(scope);
                }
            }
            _ => {}
        }
    }

    fn activate_place_command(&mut self, rest: String, complete: bool) {
        if has_command_arguments(&rest) && !complete && !self.palette.engaged {
            self.run_place_command(rest, None);
            return;
        }
        match self.palette.selected_action().cloned() {
            Some(Action::Command(spec)) => {
                if complete || needs_arguments(spec) {
                    self.set_query(format!("{COMMAND_SIGIL}{} ", spec.name));
                } else {
                    self.run_place_command(
                        spec.name.to_owned(),
                        Some(CommandInvocation::new(spec.name, [] as [&str; 0])),
                    );
                }
            }
            Some(Action::Completion(suggestion)) => {
                let (completed, _) = apply_completion(&rest, &suggestion);
                if completed.len() <= MAX_COMMAND_PROMPT_BYTES {
                    self.set_query(format!("{COMMAND_SIGIL}{completed}"));
                }
            }
            Some(Action::History(command)) => self.run_place_command(command, None),
            _ if !complete && !rest.trim().is_empty() => self.run_place_command(rest, None),
            _ => {}
        }
    }

    fn run_place_command(&mut self, input: String, invocation: Option<CommandInvocation>) {
        let place = self.palette.scope();
        let name = input
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let target = catalog_command_spec(&name)
            .and_then(|spec| place::command_target(spec, &place))
            .filter(|_| !place::has_target(&input));
        match target {
            Some(target) => {
                let invocation = invocation
                    .filter(|invocation| invocation.args.is_empty())
                    .map(|_| CommandInvocation::new(&name, ["-t", target.as_str()]));
                self.run_command(place::with_target(&input, &target), invocation);
            }
            None => self.run_command(input, invocation),
        }
    }

    fn start_verb(&mut self, verb: Verb) {
        if verb.raw
            && let Some(command) = verb.command()
        {
            self.set_query(format!("{COMMAND_SIGIL}{command} "));
            return;
        }
        let scope = self.palette.scope();
        let mut draft = Draft::new(verb);
        if matches!(draft.next(), Some(Slot::Target(_)))
            && scope.level().is_some_and(|level| level > Level::Host)
        {
            draft.filled.push((Filled::Place(scope), String::new()));
        }
        self.palette.places.draft = Some(draft);
        self.advance();
    }

    fn advance(&mut self) {
        if self
            .palette
            .places
            .draft
            .as_ref()
            .is_some_and(|draft| draft.next().is_none())
        {
            self.run_draft();
        } else {
            self.set_query(String::new());
        }
    }

    fn fill(&mut self, value: Filled, label: String) {
        if let Some(draft) = &mut self.palette.places.draft {
            draft.filled.push((value, label));
        }
        self.advance();
    }

    fn run_draft(&mut self) {
        let places = &self.palette.places;
        let Some(compiled) = places
            .draft
            .as_ref()
            .and_then(|draft| verbs::compile(&draft.verb, &draft.values(), &places.fleet))
        else {
            return;
        };
        self.run_command(compiled.line, compiled.invocation);
    }

    fn enter_place(&mut self, place: Place) {
        let places = &mut self.palette.places;
        let path = chips_to(&places.fleet, place);
        if path.is_empty() {
            return;
        }
        places.chips = path
            .into_iter()
            .map(|chip| {
                places
                    .chips
                    .iter()
                    .find(|existing| existing.level == chip.level && existing.place == chip.place)
                    .cloned()
                    .unwrap_or(chip)
            })
            .collect();
        self.set_query(String::new());
    }

    fn go_to(&mut self, place: Place) {
        let fleet = &self.palette.places.fleet;
        let label = place_label(fleet, &place).unwrap_or_default();
        let key = place::recent_key(fleet, &place);
        let target = place
            .pane
            .map(Target::Pane)
            .or(place.window.map(Target::Window))
            .or(place.session.map(Target::Session));
        let Some(target) = target else {
            self.enter_place(place);
            return;
        };
        if self.activate_target(target) {
            if let Some(key) = key {
                self.palette
                    .places
                    .recents
                    .record(key, label.clone(), now_secs());
            }
            self.palette_close();
            self.palette.notices.push(format!("Switched to {label}"));
        }
    }

    pub(super) fn step_back(&mut self) -> bool {
        let places = &mut self.palette.places;
        if let Some(draft) = &mut places.draft {
            if draft.filled.pop().is_none_or(|(_, label)| label.is_empty()) {
                places.draft = None;
            }
            self.set_query(String::new());
            return true;
        }
        let Some(chip) = places.chips.pop() else {
            return false;
        };
        self.set_query(String::new());
        if let Some(index) = self.palette.rows.iter().position(
            |entry| matches!(entry.action, Some(Action::Place(place)) if place == chip.place),
        ) {
            self.palette.selected = Some(index);
            self.palette.engaged = true;
        }
        true
    }

    pub(super) fn place_into(&mut self) -> bool {
        if self.palette.places.draft.is_some()
            || place::parse(&self.palette.input).command.is_some()
        {
            return false;
        }
        match self.palette.selected_action() {
            Some(&Action::Place(place)) => {
                self.enter_place(place);
                true
            }
            _ => false,
        }
    }

    pub(super) fn place_hints(&self) -> Vec<Value> {
        let palette = &self.palette;
        if palette.raw_command().is_some() {
            return vec![
                hint("tab", "complete"),
                hint("enter", "run"),
                hint("escape", "close"),
            ];
        }
        let mut hints = Vec::new();
        if matches!(palette.selected_action(), Some(Action::Place(_))) {
            hints.push(hint("tab", "into"));
        }
        hints.push(hint("enter", self.enter_hint()));
        if palette.input.is_empty()
            && (!palette.places.chips.is_empty() || palette.places.draft.is_some())
        {
            hints.push(hint("backspace", "back"));
        }
        hints.push(hint("escape", "close"));
        hints
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use zz_protocol::{
        CommandInvocation, Event, EventPayload, LayoutNode, MuxOptionKey, MuxOptionSource,
        MuxOptionValue, MuxOptions, MuxSnapshot, PaneBorderIndicators, PaneBorderLines,
        PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneSummary, ProtocolMessage,
        SessionId, SessionSnapshot, WindowId, WindowSnapshot, decode_protocol_frame,
    };

    use crate::Client;

    fn window(id: u64, index: u32, name: &str, pane: (u64, &str)) -> WindowSnapshot {
        let pane_id = PaneId(pane.0);
        WindowSnapshot {
            id: WindowId(id),
            index,
            name: name.to_owned(),
            automatic_rename: false,
            active_pane: pane_id,
            zoomed_pane: None,
            layout: LayoutNode::Pane(pane_id),
            panes: [(
                pane_id,
                PaneSnapshot {
                    id: pane_id,
                    title: pane.1.to_owned(),
                    kind: PaneKindSnapshot::Terminal,
                    synchronized_input: false,
                    bell: false,
                    dead: false,
                    dead_status: None,
                    border_colour: None,
                    active_border_colour: None,
                    border_status_text: String::new(),
                    mode: None,
                    status: None,
                    summary: PaneSummary::default(),
                },
            )]
            .into(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: PaneBorderStatus::default(),
            pane_border_lines: PaneBorderLines::default(),
            pane_border_indicators: PaneBorderIndicators::default(),
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 80,
            sy: 24,
            pane_base_index: 0,
        }
    }

    fn client(aliases: &str) -> Client {
        let session = |id: u64, name: &str, windows: Vec<WindowSnapshot>| SessionSnapshot {
            id: SessionId(id),
            name: name.to_owned(),
            active_window: windows[0].id,
            windows,
            viewers: Vec::new(),
        };
        let mut client = Client::new(7.0);
        client.core.handle_message(ProtocolMessage::Attached {
            session: SessionId(1),
            snapshot: MuxSnapshot {
                generation: 1,
                sessions: vec![
                    session(
                        1,
                        "work",
                        vec![
                            window(1, 1, "claude", (1, "review")),
                            window(2, 2, "cargo", (2, "watch")),
                        ],
                    ),
                    session(2, "notes", vec![window(3, 1, "nvim", (3, "todo.md"))]),
                ],
                focused_window: None,
            },
            read_only: false,
            client_flags: String::new(),
        });
        client.core.handle_message(ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::MuxOptionsChanged {
                options: MuxOptions::from_entries([(
                    MuxOptionKey::CommandAlias,
                    MuxOptionValue {
                        value: aliases.to_owned(),
                        source: MuxOptionSource::TmuxConfig,
                    },
                )]),
            },
        }));
        assert!(client.palette_open(None));
        client
    }

    fn view(client: &mut Client) -> Value {
        serde_json::from_str(&client.palette_view(800.0).unwrap()).unwrap()
    }

    fn type_text(client: &mut Client, text: &str) {
        client.palette_input(text.to_owned(), text.encode_utf16().count());
    }

    fn texts(view: &Value, key: &str) -> Vec<String> {
        view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row[key].as_str().map(str::to_owned))
            .collect()
    }

    fn pills(view: &Value) -> Vec<String> {
        view["pills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pill| {
                format!(
                    "{}:{}",
                    pill["kind"].as_str().unwrap(),
                    pill["label"].as_str().unwrap()
                )
            })
            .collect()
    }

    fn select(client: &mut Client, view: &Value, text: &str) {
        let index = view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .position(|row| row["text"] == text)
            .unwrap();
        client.palette_hover(index);
    }

    fn sent(client: &mut Client) -> Vec<CommandInvocation> {
        std::iter::from_fn(|| client.outbound())
            .filter_map(|frame| match decode_protocol_frame(&frame).ok()? {
                ProtocolMessage::CommandRequest(request) => Some(request.command),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn words_search_places_and_actions_and_a_drilled_session_offers_its_actions() {
        let mut client = client("");
        let empty = view(&mut client);
        assert_eq!(empty["input"]["placeholder"], "Type a place or an action");
        assert_eq!(pills(&empty), ["host:zz daemon"]);
        assert_eq!(texts(&empty, "header"), ["Sessions on zz daemon"]);

        type_text(&mut client, "claude");
        let places = view(&mut client);
        assert_eq!(texts(&places, "header")[0], "Places");
        assert_eq!(texts(&places, "text")[0], "work › 1 claude");
        assert_eq!(places["usage"], "runs  select-window -t work:1");

        type_text(&mut client, "split right");
        let actions = view(&mut client);
        assert_eq!(texts(&actions, "header")[0], "Actions");
        assert_eq!(texts(&actions, "text")[0], "split right");
        assert_eq!(actions["usage"], "runs  split-window -h");

        type_text(&mut client, "work");
        assert!(client.palette_key("Tab"));
        let drilled = view(&mut client);
        assert_eq!(pills(&drilled), ["host:zz daemon", "place:work"]);
        assert_eq!(texts(&drilled, "header"), ["Windows in work"]);

        type_text(&mut client, "zoom");
        let on_work = view(&mut client);
        assert_eq!(texts(&on_work, "header")[0], "Actions on work");
        assert_eq!(texts(&on_work, "text")[0], "zoom pane");
        assert_eq!(on_work["usage"], "runs  resize-pane -Z -t work");
        assert!(client.palette_key("Enter"));
        assert_eq!(
            sent(&mut client),
            [CommandInvocation::new("resize-pane", ["-Z", "-t", "$1"])]
        );
        assert!(!client.palette_is_open());
    }

    #[test]
    fn a_verb_asks_for_its_slots_in_order_and_backspace_takes_them_back() {
        let mut client = client("");
        type_text(&mut client, "rename window");
        assert!(client.palette_key("Enter"));
        let target = view(&mut client);
        assert_eq!(target["input"]["placeholder"], "Pick a window");
        assert_eq!(pills(&target), ["host:zz daemon", "verb:rename window"]);
        assert_eq!(texts(&target, "text")[0], "work › 1 claude");
        assert_eq!(target["usage"], "runs  rename-window");

        assert!(client.palette_key("Enter"));
        type_text(&mut client, "api logs");
        let text = view(&mut client);
        assert_eq!(text["input"]["placeholder"], "New name");
        assert_eq!(
            pills(&text),
            ["host:zz daemon", "verb:rename window", "place:1 claude"]
        );
        assert_eq!(text["usage"], "runs  rename-window -t work:1 'api logs'");

        type_text(&mut client, "");
        assert!(client.palette_key("Backspace"));
        assert_eq!(view(&mut client)["input"]["placeholder"], "Pick a window");
        assert!(client.palette_key("Backspace"));
        let dropped = view(&mut client);
        assert_eq!(dropped["input"]["placeholder"], "Type a place or an action");
        assert_eq!(pills(&dropped), ["host:zz daemon"]);

        type_text(&mut client, "rename window");
        client.palette_key("Enter");
        client.palette_key("Enter");
        type_text(&mut client, "api logs");
        client.palette_key("Enter");
        assert_eq!(
            sent(&mut client),
            [CommandInvocation::new(
                "rename-window",
                ["-t", "@1", "api logs"]
            )]
        );
    }

    #[test]
    fn tab_drills_into_a_pane_whose_empty_field_lists_the_actions_on_it() {
        let mut client = client("");
        type_text(&mut client, "watch");
        let found = view(&mut client);
        select(&mut client, &found, "work › 2 cargo › 0 watch");
        assert_eq!(view(&mut client)["hints"][0]["label"], "into");
        assert!(client.palette_key("Tab"));
        let pane = view(&mut client);
        assert_eq!(
            pills(&pane),
            [
                "host:zz daemon",
                "place:work",
                "place:2 cargo",
                "place:0 watch"
            ]
        );
        assert_eq!(texts(&pane, "header"), ["Actions on 0 watch"]);
        assert_eq!(texts(&pane, "text")[0], "read pane");
        assert_eq!(pane["usage"], "runs  capture-pane -p -t work:2.0");
    }

    #[test]
    fn a_pasted_place_becomes_chips_and_command_aliases_become_verbs() {
        let mut client = client("rs=resize-pane -Z");
        type_text(&mut client, "@zz daemon:%1");
        let pasted = view(&mut client);
        assert_eq!(pasted["input"]["value"], "");
        assert_eq!(
            pills(&pasted),
            [
                "host:zz daemon",
                "place:work",
                "place:1 claude",
                "place:0 review"
            ]
        );

        type_text(&mut client, "rs");
        let aliased = view(&mut client);
        assert_eq!(texts(&aliased, "header")[0], "Actions on 0 review");
        assert_eq!(texts(&aliased, "text")[0], "rs");
        assert_eq!(aliased["usage"], "runs  resize-pane -Z -t work:1.0");
        assert!(client.palette_key("Enter"));
        assert_eq!(
            sent(&mut client),
            [CommandInvocation::new("rs", ["-t", "%1"])]
        );
    }

    #[test]
    fn a_colon_in_a_text_slot_is_text_not_a_tmux_command() {
        let mut client = client("");
        type_text(&mut client, "rename window");
        assert!(client.palette_key("Enter"));
        assert!(client.palette_key("Enter"));
        type_text(&mut client, ":kill-server");
        let text = view(&mut client);
        assert_eq!(text["input"]["placeholder"], "New name");
        assert_eq!(text["usage"], "runs  rename-window -t work:1 :kill-server");
        assert!(client.palette_key("Enter"));
        assert_eq!(
            sent(&mut client),
            [CommandInvocation::new(
                "rename-window",
                ["-t", "@1", ":kill-server"]
            )]
        );
    }
}
