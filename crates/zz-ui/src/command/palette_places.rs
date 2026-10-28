use std::{collections::BTreeSet, rc::Rc};

use zz_client::{
    completion::PaneKindAvailability,
    place::{
        self, ActionKind, Candidate, Fleet, FleetHost, FleetPane, FleetSession, FleetWindow, Level,
        Place, Query,
    },
    recents::{RecentKey, RecentKind, Recents},
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
const RECENT_PLACES: usize = 5;
const RECENT_OTHERS: usize = 3;
pub const PLACE_PLACEHOLDER_KEYS: &str = "^ $ @ % : > & ?";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chip {
    pub level: Level,
    pub place: Place,
    pub label: String,
    pub auto: bool,
    pub typed: Option<String>,
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

pub(crate) fn chip_for(fleet: &Fleet, place: Place, typed: Option<String>) -> Option<Chip> {
    Some(Chip {
        level: place.level()?,
        label: place_label(fleet, &place)?,
        place,
        auto: false,
        typed,
    })
}

const fn sigil_hint(level: Level) -> &'static str {
    match level {
        Level::Host => "^",
        Level::Session => "$",
        Level::Window => "@",
        Level::Pane => "%",
    }
}

fn section(
    rows: &mut Vec<PaletteEntry>,
    label: impl Into<SharedString>,
    hint: &'static str,
    entries: Vec<PaletteEntry>,
) {
    if entries.is_empty() {
        return;
    }
    rows.push(PaletteEntry::header(label, hint));
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
        format!("{} / ", parts.join(" / "))
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

fn is_current(current: &Place, place: &Place, level: Level) -> bool {
    match level {
        Level::Host => place.host == current.host,
        Level::Session => place.host == current.host && place.session == current.session,
        Level::Window => place.host == current.host && place.window == current.window,
        Level::Pane => place.host == current.host && place.pane == current.pane,
    }
}

fn place_entry(
    fleet: &Fleet,
    tree: &PaletteTree,
    candidate: &Candidate,
    parent: &Place,
) -> PaletteEntry {
    let place = candidate.place;
    let crumb = crumbs(fleet, &place, candidate.level, parent);
    let prefix = format!("{crumb}{}", candidate.level.sigil());
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
    PaletteEntry::row(
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
        },
        PaletteAction::Place(place),
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
        self.chips
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
                level => PalettePill::Mode {
                    prefix: level.sigil().to_string().into(),
                    label: chip.label.clone().into(),
                },
            })
            .collect()
    }

    pub fn place_placeholder(&self) -> String {
        match self.chips.last() {
            Some(chip) => format!("Search {}, or type {PLACE_PLACEHOLDER_KEYS}", chip.label),
            None => format!("Search everything, or type {PLACE_PLACEHOLDER_KEYS}"),
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
        let mut rows = Vec::new();
        if let Some(action) = &line.action {
            if action.kind == ActionKind::Command {
                rows = command_entries(action.rest.trim(), availability, shortcut);
            }
            self.rows = rows;
            return;
        }
        if let Some((last, parents)) = line.parts.split_last() {
            let resolved = place::resolve(&fleet, &scope, parents, &boost);
            if resolved.failed.is_none() {
                let parent = resolved.place.above(last.level);
                let entries = place::candidates(&fleet, &resolved.place, last, &boost)
                    .iter()
                    .take(PLACE_ROWS)
                    .map(|candidate| place_entry(&fleet, &self.tree, candidate, &parent))
                    .collect();
                section(
                    &mut rows,
                    level_header(&fleet, last.level, &parent),
                    sigil_hint(last.level),
                    entries,
                );
            }
            self.rows = rows;
            return;
        }
        if let Some(search) = &line.search {
            let entries = place::search(&fleet, &scope, search, &boost)
                .iter()
                .take(PLACE_ROWS)
                .map(|candidate| place_entry(&fleet, &self.tree, candidate, &scope))
                .collect();
            section(&mut rows, "Places", "", entries);
            section(
                &mut rows,
                "Commands",
                ":",
                command_entries(search, availability, shortcut),
            );
            if !self.chips.iter().any(|chip| chip.level == Level::Host) {
                let part = place::Part {
                    level: Level::Host,
                    query: Query::Text(search.clone()),
                    range: 0..0,
                };
                let hosts = place::candidates(&fleet, &Place::default(), &part, &boost)
                    .iter()
                    .map(|candidate| place_entry(&fleet, &self.tree, candidate, &Place::default()))
                    .collect();
                section(&mut rows, "Hosts", "^", hosts);
            }
            if search.trim_end().ends_with('?') {
                let mut asked = super::ask_level::ask_entries(self.driver, search);
                asked.append(&mut rows);
                rows = asked;
            } else if rows.is_empty() {
                rows = super::ask_level::ask_entries(self.driver, search);
            }
            self.rows = rows;
            return;
        }
        if self.chips.iter().all(|chip| chip.auto) {
            self.rows = self.empty_rows(&fleet, history, recents, now, availability);
            return;
        }
        let level = scope.level().and_then(Level::deeper).unwrap_or(Level::Host);
        let entries = place::children(&fleet, &scope, &boost)
            .iter()
            .take(PLACE_ROWS)
            .map(|candidate| place_entry(&fleet, &self.tree, candidate, &scope))
            .collect();
        section(
            &mut rows,
            level_header(&fleet, level, &scope),
            sigil_hint(level),
            entries,
        );
        self.rows = rows;
    }

    fn empty_rows(
        &self,
        fleet: &Fleet,
        history: &[String],
        recents: &Recents,
        now: u64,
        availability: PaneKindAvailability,
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
        section(&mut rows, "Needs you", "", needs);
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
        section(&mut rows, "Recent places", "@", recent);
        let mut seen = BTreeSet::new();
        let commands = recents
            .top(RecentKind::Command, RECENT_PLACES * 4, now)
            .into_iter()
            .filter_map(|recent| match &recent.key {
                RecentKey::Command(command) => Some(command.clone()),
                _ => None,
            })
            .chain(history.iter().cloned())
            .filter(|value| {
                let name = value.split_whitespace().next().unwrap_or(value);
                availability.allows_command(name) && seen.insert(name.to_owned())
            })
            .take(RECENT_OTHERS)
            .map(|value| {
                let name = value.split_whitespace().next().unwrap_or(&value).to_owned();
                let spec = zz_protocol::catalog_command_spec(&name);
                PaletteEntry::row(
                    PaletteRow {
                        label: name.into(),
                        icon: Some(IconName::Terminal),
                        detail: spec.map_or("", |spec| spec.description).into(),
                        ..Default::default()
                    },
                    spec.map_or_else(
                        || PaletteAction::History(value.clone()),
                        PaletteAction::Command,
                    ),
                )
            })
            .collect();
        section(&mut rows, "Recent commands", ":", commands);
        for (kind, label, hint, icon) in [
            (RecentKind::Gui, "Recent actions", ">", IconName::Settings),
            (RecentKind::File, "Recent files", "&", IconName::File),
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
            section(&mut rows, label, hint, entries);
        }
        let hosts = fleet
            .hosts
            .iter()
            .filter_map(|host| candidate_at(fleet, Place::host(host.id)))
            .map(|candidate| place_entry(fleet, &self.tree, &candidate, &root))
            .collect();
        section(&mut rows, "Hosts", "^", hosts);
        rows
    }
}

#[cfg(test)]
#[path = "palette_places_tests.rs"]
mod tests;
