use std::ops::Range;

use zz_protocol::{
    CommandInvocation, CommandSpec, CommandValueKind, catalog_command_spec, command_specs,
};

use crate::{
    completion::PaneKindAvailability,
    fuzzy::fuzzy_match,
    gui_actions::gui_action,
    palette::needs_arguments,
    place::{self, Fleet, Level, Place},
};

const KEYWORD_PENALTY: i64 = 1000;

const TMUX_DEFAULT_ALIASES: [&str; 6] = [
    "split-pane=split-window",
    "splitp=split-window",
    "server-info=show-messages -JT",
    "info=show-messages -JT",
    "choose-window=choose-tree -w",
    "choose-session=choose-tree -s",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Target(Level),
    Text(String),
    Value(&'static str),
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    Tmux { command: String, args: Vec<String> },
    Gui(&'static str),
    InsertPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbKind {
    Tmux,
    Gui,
    Zz,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verb {
    pub name: String,
    pub keywords: String,
    pub description: String,
    pub kind: VerbKind,
    pub run: Run,
    pub slots: Vec<Slot>,
    pub raw: bool,
}

impl Verb {
    #[must_use]
    pub fn spelled(&self) -> String {
        match &self.run {
            Run::Tmux { command, args } => std::iter::once(command.as_str())
                .chain(args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" "),
            Run::Gui(_) => "gui".to_owned(),
            Run::InsertPath => "zz".to_owned(),
        }
    }

    #[must_use]
    pub fn command(&self) -> Option<&str> {
        match &self.run {
            Run::Tmux { command, .. } => Some(command),
            Run::Gui(_) | Run::InsertPath => None,
        }
    }

    #[must_use]
    pub fn target(&self) -> Option<Level> {
        self.slots.iter().find_map(|slot| match slot {
            Slot::Target(level) => Some(*level),
            _ => None,
        })
    }
}

struct Curated {
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    keywords: &'static str,
    text: Option<&'static str>,
}

const CURATED: [Curated; 6] = [
    Curated {
        name: "read pane",
        command: "capture-pane",
        args: &["-p"],
        keywords: "capture print output scrollback",
        text: None,
    },
    Curated {
        name: "split right",
        command: "split-window",
        args: &["-h"],
        keywords: "split horizontal new pane",
        text: None,
    },
    Curated {
        name: "split down",
        command: "split-window",
        args: &["-v"],
        keywords: "split vertical new pane",
        text: None,
    },
    Curated {
        name: "zoom pane",
        command: "resize-pane",
        args: &["-Z"],
        keywords: "zoom maximize unzoom",
        text: None,
    },
    Curated {
        name: "send keys",
        command: "send-keys",
        args: &[],
        keywords: "type text",
        text: Some("keys"),
    },
    Curated {
        name: "detach",
        command: "detach-client",
        args: &[],
        keywords: "leave",
        text: None,
    },
];

fn target_level(spec: &CommandSpec) -> Option<Level> {
    match spec.name {
        "new-session" => return None,
        "new-window" => return Some(Level::Session),
        _ => {}
    }
    let option = spec.option("-t").filter(|option| !option.unsupported)?;
    match option.value? {
        CommandValueKind::Session => Some(Level::Session),
        CommandValueKind::Window => Some(Level::Window),
        CommandValueKind::Pane => Some(Level::Pane),
        _ => None,
    }
}

fn positional_label(spec: &CommandSpec) -> String {
    let mut depth = 0_u32;
    let mut outside = String::new();
    for character in spec.usage.chars() {
        match character {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            character if depth == 0 => outside.push(character),
            _ => {}
        }
    }
    let label = outside
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('-', " ");
    if label.is_empty() {
        "arguments".to_owned()
    } else {
        label
    }
}

fn tmux_verb(name: String, keywords: String, spec: &CommandSpec, args: &[&str]) -> Verb {
    let mut slots = Vec::new();
    if let Some(level) = target_level(spec) {
        slots.push(Slot::Target(level));
    }
    let text = spec.positional_minimum() > 0;
    if text {
        slots.push(Slot::Text(positional_label(spec)));
    }
    Verb {
        name,
        keywords,
        description: spec.description.to_owned(),
        kind: VerbKind::Tmux,
        run: Run::Tmux {
            command: spec.name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        slots,
        raw: !text && args.is_empty() && needs_arguments(spec),
    }
}

#[must_use]
pub fn verbs(
    availability: PaneKindAvailability,
    gui: &[&'static str],
    insert_path: bool,
    aliases: &str,
) -> Vec<Verb> {
    let mut verbs = Vec::new();
    for curated in &CURATED {
        let Some(spec) = catalog_command_spec(curated.command)
            .filter(|spec| availability.allows_command(spec.name))
        else {
            continue;
        };
        let mut verb = tmux_verb(
            curated.name.to_owned(),
            format!("{} {}", curated.command, curated.keywords),
            spec,
            curated.args,
        );
        if let Some(label) = curated.text {
            verb.slots.retain(|slot| !matches!(slot, Slot::Text(_)));
            verb.slots.push(Slot::Text(label.to_owned()));
        }
        verbs.push(verb);
    }
    if insert_path {
        verbs.push(Verb {
            name: "insert path".to_owned(),
            keywords: "file folder paste browse".to_owned(),
            description: "Pick a file or folder and type its path into the pane".to_owned(),
            kind: VerbKind::Zz,
            run: Run::InsertPath,
            slots: vec![Slot::Path],
            raw: false,
        });
    }
    for id in gui {
        let Some(action) = gui_action(id) else {
            continue;
        };
        verbs.push(Verb {
            name: action.verb.to_owned(),
            keywords: action.aliases.join(" "),
            description: action.title.to_owned(),
            kind: VerbKind::Gui,
            run: Run::Gui(action.id),
            slots: vec![Slot::Value(action.id)],
            raw: false,
        });
    }
    for spec in command_specs().filter(|spec| availability.allows_command(spec.name)) {
        let name = spec.name.replace('-', " ");
        if verbs.iter().any(|verb| verb.name == name) {
            continue;
        }
        let keywords = std::iter::once(spec.name)
            .chain(spec.aliases.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        verbs.push(tmux_verb(name, keywords, spec, &[]));
    }
    verbs.extend(alias_verbs(aliases, availability));
    verbs
}

#[must_use]
pub fn alias_verbs(aliases: &str, availability: PaneKindAvailability) -> Vec<Verb> {
    aliases
        .lines()
        .map(str::trim)
        .filter(|line| !TMUX_DEFAULT_ALIASES.contains(line))
        .filter_map(|line| {
            let (name, expansion) = line.split_once('=')?;
            let mut words = expansion.split_whitespace();
            let command = words.next()?;
            let spec = catalog_command_spec(command)?;
            if !availability.allows_command(spec.name) || name.trim().is_empty() {
                return None;
            }
            let args = words.map(str::to_owned).collect::<Vec<_>>();
            let slots = target_level(spec)
                .filter(|_| !place::has_target(expansion))
                .map(Slot::Target)
                .into_iter()
                .collect();
            Some(Verb {
                name: name.trim().to_owned(),
                keywords: expansion.trim().to_owned(),
                description: spec.description.to_owned(),
                kind: VerbKind::Alias,
                run: Run::Tmux {
                    command: spec.name.to_owned(),
                    args,
                },
                slots,
                raw: false,
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerbMatch {
    pub index: usize,
    pub score: i64,
    pub matches: Vec<Range<usize>>,
}

#[must_use]
pub fn match_verbs(verbs: &[Verb], query: &str, keep: impl Fn(&Verb) -> bool) -> Vec<VerbMatch> {
    let mut found = verbs
        .iter()
        .enumerate()
        .filter(|(_, verb)| keep(verb))
        .filter_map(|(index, verb)| {
            if let Some((score, matches)) = fuzzy_match(query, &verb.name) {
                return Some(VerbMatch {
                    index,
                    score: score as i64,
                    matches,
                });
            }
            fuzzy_match(query, &verb.keywords).map(|(score, _)| VerbMatch {
                index,
                score: score as i64 + KEYWORD_PENALTY,
                matches: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|found| (found.score, found.index));
    found
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Filled {
    Place(Place),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    pub host: Option<u32>,
    pub line: String,
    pub display: String,
    pub invocation: Option<CommandInvocation>,
}

#[must_use]
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:%@+,=".contains(character))
    {
        return text.to_owned();
    }
    if !text.contains('\'') {
        return format!("'{text}'");
    }
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    )
}

#[must_use]
pub fn target_name(fleet: &Fleet, place: &Place) -> Option<String> {
    let session = fleet.session(place)?;
    let Some(window) = fleet.window(place) else {
        return Some(session.name.clone());
    };
    Some(match fleet.pane(place) {
        Some(pane) => format!("{}:{}.{}", session.name, window.index, pane.index),
        None => format!("{}:{}", session.name, window.index),
    })
}

#[must_use]
pub fn go_to_line(fleet: &Fleet, place: &Place) -> Option<String> {
    let name = quote(&target_name(fleet, place)?);
    Some(match place.level()? {
        Level::Host => return None,
        Level::Session => format!("switch-client -t {name}"),
        Level::Window => format!("select-window -t {name}"),
        Level::Pane => format!("select-pane -t {name}"),
    })
}

fn level_target(level: Level, place: &Place) -> Option<String> {
    let session = place.session.map(|id| id.to_string());
    let window = place.window.map(|id| id.to_string());
    let pane = place.pane.map(|id| id.to_string());
    match level {
        Level::Host => None,
        Level::Session => session,
        Level::Window => window.or(session),
        Level::Pane => pane.or(window).or(session),
    }
}

#[must_use]
pub fn compile(verb: &Verb, filled: &[Filled], fleet: &Fleet) -> Option<Compiled> {
    let Run::Tmux { command, args } = &verb.run else {
        return None;
    };
    let spec = catalog_command_spec(command);
    let mut line = std::iter::once(command.clone())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let mut display = line.clone();
    let mut args = Some(args.clone());
    let mut host = None;
    for (slot, value) in verb.slots.iter().zip(filled) {
        match (slot, value) {
            (Slot::Target(level), Filled::Place(place)) => {
                let place = &level.deeper().map_or(*place, |deeper| place.above(deeper));
                host = place.host;
                let id = spec
                    .and_then(|spec| place::command_target(spec, place))
                    .or_else(|| level_target(*level, place))?;
                line.extend(["-t".to_owned(), quote(&id)]);
                display.extend(["-t".to_owned(), quote(&target_name(fleet, place)?)]);
                if let Some(args) = &mut args {
                    args.extend(["-t".to_owned(), id]);
                }
            }
            (Slot::Text(_), Filled::Text(text)) => {
                let single =
                    spec.is_some_and(|spec| spec.positionals.len() == 1 && spec.variadic.is_none());
                let piece = if single { quote(text) } else { text.clone() };
                line.push(piece.clone());
                display.push(piece);
                match &mut args {
                    Some(args) if single => args.push(text.clone()),
                    _ => args = None,
                }
            }
            _ => return None,
        }
    }
    if filled.len() < verb.slots.len() {
        return None;
    }
    Some(Compiled {
        host,
        line: line.join(" "),
        display: display.join(" "),
        invocation: args.map(|args| CommandInvocation::new(command, args)),
    })
}

#[cfg(test)]
#[path = "verbs_tests.rs"]
mod tests;
