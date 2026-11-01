use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{
    super::{Codex, Found, Read, arguments},
    Call, JOIN, PAGES, folder, hello, others_in, same_dir,
};
use crate::agent::codex::control::{self, Connection, Next};

const EVERY: Duration = Duration::from_secs(1);
const NAME_CHARS: usize = 48;
const PROJECT_CHARS: usize = 24;
const GONE: &str = "thread not loaded";

pub(in super::super) enum Claim {
    Thread(Found),
    Waiting,
    Unsure,
}

#[derive(Default)]
pub(super) struct Claims {
    pub(super) idle: Option<Idle>,
    pub(super) chosen: Option<Chosen>,
    known: HashMap<String, Option<Known>>,
    pub(super) next: Option<Instant>,
    pub(super) drained: Option<Read>,
}

pub(super) struct Idle {
    pub(super) connection: Connection,
    pub(super) calls: HashMap<u64, Call>,
}

#[derive(Clone)]
pub(super) struct Chosen {
    pub(super) id: String,
    path: Option<PathBuf>,
    cwd: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct Known {
    pub(super) id: String,
    pub(super) born: u64,
    pub(super) name: Option<String>,
    pub(super) path: Option<PathBuf>,
    pub(super) cwd: PathBuf,
}

impl Known {
    fn of(thread: &Value) -> Option<(String, Option<Self>)> {
        let id = thread["id"].as_str()?.to_owned();
        let top = thread["ephemeral"] != true
            && thread["parentThreadId"].is_null()
            && thread["threadSource"]
                .as_str()
                .is_none_or(|source| source == "user");
        let cwd = thread["cwd"].as_str().map(PathBuf::from);
        let born = born(&id).or_else(|| {
            thread["createdAt"]
                .as_u64()
                .map(|at| at.saturating_mul(1000))
        });
        let (true, Some(cwd), Some(born)) = (top, cwd, born) else {
            return Some((id, None));
        };
        Some((
            id.clone(),
            Some(Self {
                id,
                born,
                name: thread["name"]
                    .as_str()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned),
                path: thread["path"].as_str().map(PathBuf::from),
                cwd,
            }),
        ))
    }
}

fn born(id: &str) -> Option<u64> {
    let hex = id.replace('-', "");
    if hex.len() != 32 || hex.as_bytes()[12] != b'7' {
        return None;
    }
    u64::from_str_radix(&hex[..12], 16).ok()
}

pub(super) fn choose<'a>(
    threads: &'a [Known],
    mine: u64,
    others: &[u64],
    title: Option<&str>,
    project: &str,
) -> Option<&'a Known> {
    let titled = title.map_or_else(Vec::new, |title| {
        threads
            .iter()
            .filter(|thread| {
                thread
                    .name
                    .as_deref()
                    .is_some_and(|name| shows(title, name, project))
            })
            .collect::<Vec<_>>()
    });
    if let [only] = titled[..] {
        return Some(only);
    }
    in_order(threads, mine, others)
        .filter(|thread| titled.is_empty() || titled.iter().any(|named| named.id == thread.id))
}

fn in_order<'a>(threads: &'a [Known], mine: u64, others: &[u64]) -> Option<&'a Known> {
    let mut starts = others
        .iter()
        .chain([&mine])
        .map(|start| start.saturating_mul(1000))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    let first = *starts.first()?;
    let mut fresh = threads
        .iter()
        .filter(|thread| thread.born >= first)
        .collect::<Vec<_>>();
    fresh.sort_by_key(|thread| thread.born);
    if fresh.len() != starts.len() {
        return None;
    }
    let fits = fresh.iter().enumerate().all(|(index, thread)| {
        thread.born >= starts[index] && starts.get(index + 1).is_none_or(|next| thread.born < *next)
    });
    if !fits {
        return None;
    }
    let index = starts
        .iter()
        .position(|start| *start == mine.saturating_mul(1000))?;
    fresh.get(index).copied()
}

fn shows(title: &str, name: &str, project: &str) -> bool {
    let name = shown(name, NAME_CHARS);
    if name.is_empty() || name == shown(project, PROJECT_CHARS) {
        return false;
    }
    title.split(" | ").any(|part| {
        let mut part = part.trim();
        while let Some((glyph, rest)) = part.split_once(' ')
            && glyph.chars().count() == 1
            && !glyph.chars().all(char::is_alphanumeric)
        {
            part = rest;
        }
        part == name
    })
}

fn shown(text: &str, limit: usize) -> String {
    let text = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if text.chars().count() <= limit {
        return text;
    }
    text.chars().take(limit - 3).chain("...".chars()).collect()
}

fn loaded(connection: &mut Connection) -> Result<Vec<String>, String> {
    let mut threads = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = connection.call("thread/loaded/list", &json!({ "cursor": cursor }), JOIN)?;
        threads.extend(
            page["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            return Ok(threads);
        }
    }
    Ok(threads)
}

impl Codex {
    pub(in super::super) fn claim(&mut self) -> Claim {
        let now = Instant::now();
        if self.link.claims.next.is_none_or(|next| now >= next) {
            self.link.claims.next = Some(now + EVERY);
            self.link.claims.chosen = self.choose_thread();
        }
        let Some(chosen) = self.link.claims.chosen.clone() else {
            return Claim::Unsure;
        };
        let Some(path) = chosen.path.filter(|path| path.exists()) else {
            return Claim::Waiting;
        };
        if self.link.live.is_none() && self.tail.as_ref().is_some_and(|tail| tail.path == path) {
            self.link.relive = true;
        }
        Claim::Thread(Found {
            session: chosen.id,
            path,
            cwd: chosen.cwd,
        })
    }

    pub(super) fn unclaim(&mut self) {
        self.link.claims.chosen = None;
        self.link.claims.next = None;
    }

    pub(super) fn claimed(&self, thread: &str) -> bool {
        self.link
            .claims
            .chosen
            .as_ref()
            .is_some_and(|chosen| chosen.id == thread)
    }

    fn choose_thread(&mut self) -> Option<Chosen> {
        let mine = self.born?;
        let here = self.here()?;
        let threads = match self.survey(&here) {
            Ok(threads) => threads,
            Err(error) => {
                log::debug!(target: "zz::agent", "could not list the Codex app server's threads: {error}");
                self.link.claims.idle = None;
                return None;
            }
        };
        if threads.is_empty() {
            return None;
        }
        let others = self.neighbors_in(&here)?;
        let title = self.link.pane.as_ref().and_then(|pane| pane.title());
        let project = here
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let chosen = choose(&threads, mine, &others, title.as_deref(), &project)?;
        Some(Chosen {
            id: chosen.id.clone(),
            path: chosen.path.clone(),
            cwd: chosen.cwd.clone(),
        })
    }

    fn here(&self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(here) = &self.link.here {
            return Some(here.clone());
        }
        let pid = self.pid?;
        folder(pid, &arguments(pid))
    }

    pub(super) fn neighbors_in(&self, cwd: &Path) -> Option<Vec<u64>> {
        #[cfg(test)]
        if let Some(neighbors) = &self.link.neighbors {
            return Some(neighbors.clone());
        }
        others_in(self.pid?, cwd)
    }

    fn survey(&mut self, here: &Path) -> Result<Vec<Known>, String> {
        let mut known = std::mem::take(&mut self.link.claims.known);
        let Some(idle) = self.idle() else {
            self.link.claims.known = known;
            return Ok(Vec::new());
        };
        let loaded = loaded(&mut idle.connection)?;
        known.retain(|id, _| loaded.contains(id));
        for id in &loaded {
            if known.get(id).is_some_and(Option::is_none) {
                continue;
            }
            let read = match idle.connection.call(
                "thread/read",
                &json!({ "threadId": id, "includeTurns": false }),
                JOIN,
            ) {
                Ok(read) => read,
                Err(error) if error.starts_with(GONE) => continue,
                Err(error) => return Err(error),
            };
            if let Some((read, thread)) = Known::of(&read["thread"])
                && read == *id
            {
                known.insert(read, thread);
            }
        }
        let threads = known
            .values()
            .flatten()
            .filter(|thread| same_dir(&thread.cwd, here))
            .cloned()
            .collect();
        self.link.claims.known = known;
        Ok(threads)
    }

    fn idle(&mut self) -> Option<&mut Idle> {
        if self.link.claims.idle.is_none() {
            let mut connection = self.dial()?;
            if let Err(error) = hello(&mut connection) {
                log::debug!(target: "zz::agent", "the Codex app server did not answer: {error}");
                return None;
            }
            self.link.claims.idle = Some(Idle {
                connection,
                calls: HashMap::new(),
            });
        }
        self.link.claims.idle.as_mut()
    }

    pub(super) fn connect(&mut self) -> Option<(Connection, HashMap<u64, Call>)> {
        if let Some(idle) = self.link.claims.idle.take() {
            return Some((idle.connection, idle.calls));
        }
        let mut connection = self.dial()?;
        match hello(&mut connection) {
            Ok(()) => Some((connection, HashMap::new())),
            Err(error) => {
                log::info!(target: "zz::agent", "the Codex lens reads the rollout instead: {error}");
                None
            }
        }
    }

    pub(super) fn waiting(&mut self) -> Option<(&mut Idle, String)> {
        let thread = self.link.claims.chosen.as_ref()?.id.clone();
        Some((self.link.claims.idle.as_mut()?, thread))
    }

    pub(in super::super) fn read_idle(&mut self, read: &mut Read) {
        if let Some(drained) = self.link.claims.drained.take() {
            read.updates.splice(0..0, drained.updates);
            read.payloads.splice(0..0, drained.payloads);
            read.finished |= drained.finished;
            read.failed = read.failed.take().or(drained.failed);
        }
        let Some(idle) = self.link.claims.idle.as_mut() else {
            return;
        };
        loop {
            match idle.connection.next() {
                Next::Message(message) => {
                    let Some(call) = message
                        .get("method")
                        .is_none()
                        .then(|| message["id"].as_u64())
                        .flatten()
                        .and_then(|id| idle.calls.remove(&id))
                    else {
                        continue;
                    };
                    if let (Call::Start, Some(error)) = (call, message.get("error")) {
                        read.finished = true;
                        read.failed = Some(control::error_text(error));
                    }
                }
                Next::Empty => return,
                Next::Gone => {
                    self.link.claims.idle = None;
                    self.link.claims.next = None;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "lens_codex_claim_tests.rs"]
mod tests;
