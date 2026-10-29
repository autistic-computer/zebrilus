use std::{collections::BTreeSet, rc::Rc};

use zz_client::{
    completion::PaneKindAvailability,
    place::{
        self, Candidate, Fleet, FleetHost, FleetPane, FleetSession, FleetWindow, Level, Part, Place,
    },
    recents::{RecentKey, RecentKind, Recents},
    verbs::{self, Filled, Slot, Verb, VerbKind, VerbMatch},
};
use zz_gpui::SharedString;

use super::{
    PalettePill, PaletteRow, PaletteStatus,
    palette_model::{
        PaletteAction, PaletteEntry, PaletteTree, PaletteTreePane, UnifiedPalette, command_entries,
        count,
    },
};
use crate::IconName;

pub const PLACE_ROWS: usize = 50;
const VERB_ROWS: usize = 30;
const RECENT_PLACES: usize = 5;
const RECENT_OTHERS: usize = 3;
pub const PLACE_PLACEHOLDER: &str = "Type a place or an action";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chip {
    pub level: Level,
    pub place: Place,
    pub label: String,
    pub auto: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draft {
    pub verb: Verb,
    pub filled: Vec<(Filled, String)>,
}

impl Draft {
    pub(crate) const fn new(verb: Verb) -> Self {
        Self {
            verb,
            filled: Vec::new(),
        }
    }

    pub(crate) fn next(&self) -> Option<&Slot> {
        self.verb.slots.get(self.filled.len())
    }

    pub(crate) fn values(&self) -> Vec<Filled> {
        self.filled.iter().map(|(value, _)| value.clone()).collect()
    }
}

#[must_use]
pub fn palette_fleet(tree: &PaletteTree) -> Fleet {
    Fleet {
        here: tree.attached.0,
        hosts: tree
            .hosts
            .iter()
            .map(|host| FleetHost {
                id: host.id.0,
                name: host.name.clone(),
                online: host.connected(),
                current: host
                    .sessions
                    .iter()
                    .find(|session| session.active)
                    .map(|session| session.id),
                sessions: host
                    .sessions
                    .iter()
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
                                        program: pane.program.as_ref().map(ToString::to_string),
                                        cwd: pane.cwd.clone(),
                                        terminal: pane.terminal,
                                        needs_you: pane.needs_you,
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub(crate) fn place_label(fleet: &Fleet, place: &Place) -> Option<String> {
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

pub(crate) fn chip_for(fleet: &Fleet, place: Place) -> Option<Chip> {
    Some(Chip {
        level: place.level()?,
        label: place_label(fleet, &place)?,
        place,
        auto: false,
    })
}

fn section(
    rows: &mut Vec<PaletteEntry>,
    label: impl Into<SharedString>,
    entries: Vec<PaletteEntry>,
) {
    if entries.is_empty() {
        return;
    }
    rows.push(PaletteEntry::header(label, ""));
    rows.extend(entries);
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

fn tree_pane<'a>(tree: &'a PaletteTree, place: &Place) -> Option<&'a PaletteTreePane> {
    let host = tree
        .hosts
        .iter()
        .find(|host| Some(host.id.0) == place.host)?;
    let pane = place.pane?;
    host.sessions
        .iter()
        .flat_map(|session| &session.windows)
        .flat_map(|window| &window.panes)
        .find(|candidate| candidate.id == pane)
}

fn running(tree: &PaletteTree, place: &Place) -> bool {
    let Some(host) = tree.hosts.iter().find(|host| Some(host.id.0) == place.host) else {
        return false;
    };
    host.sessions
        .iter()
        .filter(|session| place.session.is_none_or(|id| id == session.id))
        .flat_map(|session| &session.windows)
        .filter(|window| place.window.is_none_or(|id| id == window.id))
        .flat_map(|window| &window.panes)
        .filter(|pane| place.pane.is_none_or(|id| id == pane.id))
        .any(|pane| pane.running)
}

pub(crate) fn is_current(current: &Place, place: &Place, level: Level) -> bool {
    match level {
        Level::Host => place.host == current.host,
        Level::Session => place.host == current.host && place.session == current.session,
        Level::Window => place.host == current.host && place.window == current.window,
        Level::Pane => place.host == current.host && place.pane == current.pane,
    }
}

fn place_row(
    fleet: &Fleet,
    tree: &PaletteTree,
    candidate: &Candidate,
    parent: &Place,
) -> PaletteRow {
    let place = candidate.place;
    let prefix = crumbs(fleet, &place, candidate.level, parent);
    let shift = prefix.len();
    let (detail, icon, status) = match candidate.level {
        Level::Host => {
            let host = tree.hosts.iter().find(|host| Some(host.id.0) == place.host);
            (
                host.map_or_else(String::new, |host| host.right.clone()),
                IconName::HardDrive,
                host.map(|host| host.status),
            )
        }
        Level::Session => (
            fleet.session(&place).map_or_else(String::new, |session| {
                count(session.windows.len(), "window")
            }),
            IconName::Folder,
            None,
        ),
        Level::Window => (
            fleet
                .window(&place)
                .map_or_else(String::new, |window| count(window.panes.len(), "pane")),
            IconName::PanelsTopLeft,
            None,
        ),
        Level::Pane => tree_pane(tree, &place).map_or_else(
            || (String::new(), IconName::SquareTerminal, None),
            |pane| (pane.detail.to_string(), pane.icon.clone(), None),
        ),
    };
    PaletteRow {
        label: format!("{prefix}{}", candidate.label).into(),
        matches: candidate
            .matches
            .iter()
            .map(|range| range.start + shift..range.end + shift)
            .collect(),
        muted_prefix: shift,
        detail: detail.into(),
        right: if is_current(&fleet.current(), &place, candidate.level) {
            "current"
        } else {
            ""
        }
        .into(),
        status,
        running: candidate.level != Level::Host && running(tree, &place),
        icon: Some(icon),
        ..Default::default()
    }
}

fn place_entry(
    fleet: &Fleet,
    tree: &PaletteTree,
    candidate: &Candidate,
    parent: &Place,
) -> PaletteEntry {
    PaletteEntry::row(
        place_row(fleet, tree, candidate, parent),
        PaletteAction::Place(candidate.place),
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

const fn verb_icon(verb: &Verb) -> IconName {
    match verb.kind {
        VerbKind::Tmux | VerbKind::Alias => IconName::Terminal,
        VerbKind::Gui => IconName::Settings,
        VerbKind::Zz => IconName::File,
    }
}

fn verb_entry(
    verbs: &[Verb],
    found: &VerbMatch,
    shortcut: &dyn Fn(&str) -> Option<SharedString>,
) -> PaletteEntry {
    let verb = &verbs[found.index];
    let plain = matches!(&verb.run, verbs::Run::Tmux { args, .. } if args.is_empty());
    PaletteEntry::row(
        PaletteRow {
            label: verb.name.clone().into(),
            matches: found.matches.clone(),
            detail: verb.description.clone().into(),
            right: verb.spelled().into(),
            icon: Some(verb_icon(verb)),
            shortcut: verb.command().filter(|_| plain).and_then(shortcut),
            ..Default::default()
        },
        PaletteAction::Verb(found.index),
    )
}

pub(crate) const fn slot_noun(slot: &Slot) -> &'static str {
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

impl UnifiedPalette {
    pub fn scope(&self) -> Place {
        self.chips
            .last()
            .map_or_else(Place::default, |chip| chip.place)
    }

    pub fn is_places(&self) -> bool {
        self.mode.is_none() && self.command.is_none()
    }

    pub fn place_pills(&self) -> Vec<PalettePill> {
        let mut pills = self
            .chips
            .iter()
            .map(|chip| match chip.level {
                Level::Host => PalettePill::Host {
                    label: chip.label.clone().into(),
                    status: self
                        .tree
                        .hosts
                        .iter()
                        .find(|host| Some(host.id.0) == chip.place.host)
                        .map_or(PaletteStatus::Offline, |host| host.status),
                },
                _ => PalettePill::Place(chip.label.clone().into()),
            })
            .collect::<Vec<_>>();
        if let Some(draft) = &self.draft {
            pills.push(PalettePill::Verb(draft.verb.name.clone().into()));
            pills.extend(
                draft
                    .filled
                    .iter()
                    .filter(|(_, label)| !label.is_empty())
                    .map(|(_, label)| PalettePill::Place(label.clone().into())),
            );
        }
        pills
    }

    pub fn place_placeholder(&self) -> String {
        match self.draft.as_ref().and_then(Draft::next) {
            Some(Slot::Text(label)) => {
                let mut label = label.clone();
                if let Some(first) = label.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                label
            }
            Some(slot) => slot_noun(slot).to_owned(),
            None => PLACE_PLACEHOLDER.to_owned(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_places(
        &mut self,
        text: &str,
        history: &[String],
        recents: &Recents,
        now: u64,
        tree: PaletteTree,
        availability: PaneKindAvailability,
        shortcut: &dyn Fn(&str) -> Option<SharedString>,
    ) {
        self.tree = tree;
        let fleet = Rc::new(palette_fleet(&self.tree));
        self.fleet = Rc::clone(&fleet);
        let boost = |place: &Place| {
            place::recent_key(&fleet, place).map_or(0, |key| recents.boost(&key, now))
        };
        let scope = self.scope();
        let line = place::parse(text);
        if let Some(command) = &line.command {
            self.rows = command_entries(command.trim(), availability, shortcut);
            return;
        }
        if let Some(draft) = &self.draft {
            self.rows = self.slot_rows(&fleet, draft, text.trim(), &boost);
            return;
        }
        let pane = scope.level() == Some(Level::Pane);
        let Some(search) = line.search.or_else(|| pane.then(String::new)) else {
            if self.chips.iter().all(|chip| chip.auto) {
                self.rows = self.empty_rows(&fleet, history, recents, now, availability, &boost);
                return;
            }
            let level = scope.level().and_then(Level::deeper).unwrap_or(Level::Host);
            let entries = place::children(&fleet, &scope, &boost)
                .iter()
                .take(PLACE_ROWS)
                .map(|candidate| place_entry(&fleet, &self.tree, candidate, &scope))
                .collect();
            let mut rows = Vec::new();
            section(&mut rows, level_header(&fleet, level, &scope), entries);
            self.rows = rows;
            return;
        };
        let drilled = scope.level().is_some_and(|level| level > Level::Host);
        let verbs = verbs::match_verbs(&self.verbs, &search, |verb| {
            !drilled || verb.target().is_some()
        });
        let places = place::search(&fleet, &scope, &search, &boost);
        let verb_rows = verbs
            .iter()
            .take(VERB_ROWS)
            .map(|found| verb_entry(&self.verbs, found, shortcut))
            .collect::<Vec<_>>();
        let place_rows = places
            .iter()
            .take(PLACE_ROWS)
            .map(|candidate| place_entry(&fleet, &self.tree, candidate, &scope))
            .collect::<Vec<_>>();
        let actions = if drilled {
            place_label(&fleet, &scope).map_or_else(
                || "Actions".to_owned(),
                |label| format!("Actions on {label}"),
            )
        } else {
            "Actions".to_owned()
        };
        let verbs_first = drilled
            || match (verbs.first(), places.first()) {
                (Some(verb), Some(place)) => verb.score < place.score,
                (Some(_), None) => true,
                _ => false,
            };
        let mut rows = Vec::new();
        if verbs_first {
            section(&mut rows, actions, verb_rows);
            section(&mut rows, "Places", place_rows);
        } else {
            section(&mut rows, "Places", place_rows);
            section(&mut rows, actions, verb_rows);
        }
        if !self.chips.iter().any(|chip| chip.level == Level::Host) {
            let part = Part {
                level: Level::Host,
                query: search,
            };
            let hosts = place::candidates(&fleet, &Place::default(), &part, &boost)
                .iter()
                .map(|candidate| place_entry(&fleet, &self.tree, candidate, &Place::default()))
                .collect();
            section(&mut rows, "Hosts", hosts);
        }
        self.rows = rows;
    }

    fn slot_rows(
        &self,
        fleet: &Fleet,
        draft: &Draft,
        query: &str,
        boost: &dyn Fn(&Place) -> i64,
    ) -> Vec<PaletteEntry> {
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
                        PaletteEntry::row(
                            place_row(fleet, &self.tree, candidate, &host),
                            PaletteAction::Fill(candidate.place),
                        )
                    })
                    .collect();
                let mut rows = Vec::new();
                section(&mut rows, level_header(fleet, *level, &host), entries);
                rows
            }
            Some(Slot::Text(label)) if !query.is_empty() => vec![PaletteEntry::row(
                PaletteRow {
                    label: query.to_owned().into(),
                    detail: label.clone().into(),
                    right: draft.verb.name.clone().into(),
                    icon: Some(IconName::Pencil),
                    ..Default::default()
                },
                PaletteAction::Submit,
            )],
            _ => Vec::new(),
        }
    }

    fn empty_rows(
        &self,
        fleet: &Fleet,
        history: &[String],
        recents: &Recents,
        now: u64,
        availability: PaneKindAvailability,
        boost: &dyn Fn(&Place) -> i64,
    ) -> Vec<PaletteEntry> {
        let mut rows = Vec::new();
        let root = Place::default();
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
                let mut entry = place_entry(fleet, &self.tree, &candidate, &root);
                if let Some(pane) = tree_pane(&self.tree, &place) {
                    entry.row.detail = pane.detail.clone();
                }
                Some(entry)
            })
            .collect();
        section(&mut rows, "Needs you", needs);
        let current = fleet.current();
        let recent = recents
            .top(RecentKind::Place, RECENT_PLACES * 4, now)
            .into_iter()
            .filter_map(|recent| place::recent_place(fleet, &recent.key))
            .filter(|place| place.window.is_none() || place.window != current.window)
            .take(RECENT_PLACES)
            .filter_map(|place| {
                let mut candidate = candidate_at(fleet, place.above(Level::Pane))?;
                candidate.place = place;
                Some(place_entry(fleet, &self.tree, &candidate, &root))
            })
            .collect();
        section(&mut rows, "Recent places", recent);
        let mut seen = BTreeSet::new();
        let recent_actions = recents
            .top(RecentKind::Command, RECENT_PLACES * 4, now)
            .into_iter()
            .filter_map(|recent| match &recent.key {
                RecentKey::Command(command) => Some((command.clone(), recent.label.clone())),
                _ => None,
            })
            .chain(
                history
                    .iter()
                    .map(|command| (command.clone(), String::new())),
            )
            .filter(|(command, _)| {
                let name = command.split_whitespace().next().unwrap_or(command);
                availability.allows_command(name) && seen.insert(command.clone())
            })
            .take(RECENT_OTHERS)
            .map(|(command, label)| {
                let name = command
                    .split_whitespace()
                    .next()
                    .unwrap_or(&command)
                    .to_owned();
                PaletteEntry::row(
                    PaletteRow {
                        label: if label.is_empty() { name } else { label }.into(),
                        icon: Some(IconName::Terminal),
                        right: command.clone().into(),
                        ..Default::default()
                    },
                    PaletteAction::History(command),
                )
            })
            .collect();
        section(&mut rows, "Recent actions", recent_actions);
        for (kind, label, icon) in [
            (RecentKind::Gui, "Recent settings", IconName::Settings),
            (RecentKind::File, "Recent files", IconName::File),
        ] {
            let entries = recents
                .top(kind, RECENT_OTHERS, now)
                .into_iter()
                .map(|recent| {
                    PaletteEntry::row(
                        PaletteRow {
                            label: recent.label.clone().into(),
                            icon: Some(icon.clone()),
                            ..Default::default()
                        },
                        PaletteAction::Recent(recent.key.clone()),
                    )
                })
                .collect();
            section(&mut rows, label, entries);
        }
        let scope = self.scope();
        if scope.host.is_some() {
            let sessions = place::children(fleet, &scope, boost)
                .iter()
                .take(PLACE_ROWS)
                .map(|candidate| place_entry(fleet, &self.tree, candidate, &scope))
                .collect();
            section(
                &mut rows,
                level_header(fleet, Level::Session, &scope),
                sessions,
            );
        }
        let hosts = fleet
            .hosts
            .iter()
            .filter(|host| Some(host.id) != scope.host)
            .filter_map(|host| candidate_at(fleet, Place::host(host.id)))
            .map(|candidate| place_entry(fleet, &self.tree, &candidate, &root))
            .collect();
        section(
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

#[cfg(test)]
#[path = "palette_places_tests.rs"]
mod tests;
