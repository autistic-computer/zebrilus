use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use zz_protocol::{
    AgentSessionOpKind,
    agent_stream::{AgentSessionSummary, AgentStreamPayload},
};

use crate::{providers::home_relative, window_summary::short_age};

pub const HERE_ROWS: usize = 3;
pub const PROJECT_ROWS: usize = 5;
pub const UNTITLED: &str = "Untitled chat";
pub const NOTHING_MATCHES: &str = "Nothing matches";
const BACKFILL_PAGES: usize = 8;
const FILTER_PAGES: usize = 40;
const FOUND_FOLDERS: usize = 8;
const FOUND_CHATS: usize = 40;
const TEMP_ROOTS: [&str; 4] = [
    "/tmp",
    "/private/tmp",
    "/var/folders",
    "/private/var/folders",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Listing {
    chats: Vec<AgentSessionSummary>,
    more: Option<String>,
    asked: Option<String>,
    pages: usize,
}

impl Listing {
    fn take(&mut self, chats: Vec<AgentSessionSummary>, more: Option<String>, replace: bool) {
        if replace {
            self.chats = chats;
            self.pages = 1;
        } else {
            self.chats.extend(chats);
            self.pages += 1;
        }
        self.more = more;
    }

    fn page(&mut self, cwd: Option<PathBuf>, limit: usize) -> Option<AgentSessionOpKind> {
        let more = self.more.clone()?;
        if self.pages >= limit || self.asked.as_ref() == Some(&more) {
            return None;
        }
        self.asked = Some(more.clone());
        Some(list(cwd, Some(more)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeRow {
    Heading {
        text: String,
        back: bool,
    },
    Chat {
        chat: AgentSessionSummary,
        title: String,
        place: Option<String>,
        age: Option<String>,
    },
    Project {
        cwd: PathBuf,
        name: String,
        path: String,
        age: Option<String>,
    },
    More(More),
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum More {
    Here,
    Projects,
    Chats,
}

impl More {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Here => "More chats here",
            Self::Projects => "More projects",
            Self::Chats => "More chats",
        }
    }
}

impl ResumeRow {
    #[must_use]
    pub const fn picks(&self) -> bool {
        !matches!(self, Self::Heading { .. } | Self::Empty)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resume {
    here: Listing,
    recent: Listing,
    open: Option<(PathBuf, Listing)>,
    every_project: bool,
    filter: String,
    selected: Option<usize>,
}

impl Resume {
    #[must_use]
    pub fn start(cwd: &Path) -> [AgentSessionOpKind; 2] {
        [list(Some(cwd.to_path_buf()), None), list(None, None)]
    }

    pub fn apply(&mut self, cwd: &Path, reply: &str) -> bool {
        let Ok(AgentStreamPayload::SessionsListed {
            sessions,
            next_cursor,
            cwd_filter,
            replace,
            ..
        }) = serde_json::from_str(reply)
        else {
            return false;
        };
        match cwd_filter {
            None => self.recent.take(sessions, next_cursor, replace),
            Some(filter) => {
                if let Some((open, listing)) = &mut self.open
                    && *open == filter
                {
                    listing.take(sessions.clone(), next_cursor.clone(), replace);
                }
                if filter == cwd {
                    self.here.take(sessions, next_cursor, replace);
                }
            }
        }
        self.selected = None;
        true
    }

    pub fn wanted(&mut self, cwd: &Path) -> Option<AgentSessionOpKind> {
        let filtering = !self.query().is_empty();
        if let Some((folder, listing)) = &mut self.open {
            return filtering
                .then(|| listing.page(Some(folder.clone()), FILTER_PAGES))
                .flatten();
        }
        if filtering {
            return self.recent.page(None, FILTER_PAGES);
        }
        if self.projects(cwd, false).len() > PROJECT_ROWS {
            return None;
        }
        self.recent.page(None, BACKFILL_PAGES)
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_filter(&mut self, text: &str) -> bool {
        if self.filter == text {
            return false;
        }
        text.clone_into(&mut self.filter);
        self.selected = None;
        true
    }

    fn query(&self) -> String {
        self.filter.trim().to_lowercase()
    }

    #[must_use]
    pub fn rows(&self, cwd: &Path, session: Option<&str>, now: u64) -> Vec<ResumeRow> {
        let query = self.query();
        let has = |text: &str| text.to_lowercase().contains(&query);
        let chat = |chat: &AgentSessionSummary, place: bool| ResumeRow::Chat {
            chat: chat.clone(),
            title: title(chat),
            place: place.then(|| folder_name(&chat.cwd)),
            age: age(chat.updated_at.as_deref(), now),
        };
        let project = |(folder, updated): &(&Path, Option<&str>)| ResumeRow::Project {
            cwd: folder.to_path_buf(),
            name: folder_name(folder),
            path: folder
                .parent()
                .map(|parent| home_relative(&parent.to_string_lossy()))
                .unwrap_or_default(),
            age: age(*updated, now),
        };
        let other = |chat: &&AgentSessionSummary| Some(chat.session_id.as_str()) != session;
        let mut rows = Vec::new();
        if let Some((folder, listing)) = &self.open {
            rows.push(ResumeRow::Heading {
                text: folder_name(folder),
                back: true,
            });
            rows.extend(
                listing
                    .chats
                    .iter()
                    .filter(other)
                    .filter(|chat| query.is_empty() || has(&title(chat)))
                    .map(|found| chat(found, false)),
            );
            if query.is_empty() {
                if listing.more.is_some() {
                    rows.push(ResumeRow::More(More::Chats));
                }
            } else if rows.len() == 1 {
                rows.push(ResumeRow::Empty);
            }
            return rows;
        }
        if !query.is_empty() {
            let folders = self
                .projects(cwd, true)
                .into_iter()
                .filter(|(folder, _)| has(&home_relative(&folder.to_string_lossy())))
                .take(FOUND_FOLDERS)
                .collect::<Vec<_>>();
            let mut seen = HashSet::new();
            let mut chats = self
                .here
                .chats
                .iter()
                .chain(&self.recent.chats)
                .filter(other)
                .filter(|chat| seen.insert(chat.session_id.as_str()))
                .filter(|chat| has(&title(chat)) || has(&folder_name(&chat.cwd)))
                .collect::<Vec<_>>();
            chats.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            chats.truncate(FOUND_CHATS);
            if !folders.is_empty() {
                rows.push(ResumeRow::Heading {
                    text: "Folders".to_owned(),
                    back: false,
                });
                rows.extend(folders.iter().map(project));
            }
            if !chats.is_empty() {
                rows.push(ResumeRow::Heading {
                    text: "Chats".to_owned(),
                    back: false,
                });
                rows.extend(chats.into_iter().map(|found| chat(found, true)));
            }
            if rows.is_empty() {
                rows.push(ResumeRow::Empty);
            }
            return rows;
        }
        let here = self.here.chats.iter().filter(other).collect::<Vec<_>>();
        if !here.is_empty() {
            rows.push(ResumeRow::Heading {
                text: "Recent here".to_owned(),
                back: false,
            });
            rows.extend(here.iter().take(HERE_ROWS).map(|found| chat(found, false)));
            if here.len() > HERE_ROWS || self.here.more.is_some() {
                rows.push(ResumeRow::More(More::Here));
            }
        }
        let projects = self.projects(cwd, false);
        if !projects.is_empty() {
            rows.push(ResumeRow::Heading {
                text: "Projects".to_owned(),
                back: false,
            });
            let shown = if self.every_project {
                projects.len()
            } else {
                PROJECT_ROWS
            };
            rows.extend(projects.iter().take(shown).map(project));
            if projects.len() > shown || self.recent.more.is_some() {
                rows.push(ResumeRow::More(More::Projects));
            }
        }
        rows
    }

    fn projects(&self, cwd: &Path, scratch_too: bool) -> Vec<(&Path, Option<&str>)> {
        let mut projects: Vec<(&Path, Option<&str>)> = Vec::new();
        for chat in &self.recent.chats {
            if chat.cwd != cwd
                && (scratch_too || !scratch(&chat.cwd))
                && !projects.iter().any(|(folder, _)| *folder == chat.cwd)
            {
                projects.push((&chat.cwd, chat.updated_at.as_deref()));
            }
        }
        projects
    }

    #[must_use]
    pub fn selection(&self, rows: &[ResumeRow]) -> Option<usize> {
        self.selected
            .filter(|at| rows.get(*at).is_some_and(ResumeRow::picks))
            .or_else(|| {
                if self.query().is_empty() {
                    None
                } else {
                    rows.iter().position(ResumeRow::picks)
                }
            })
    }

    pub fn clear_selection(&mut self) -> bool {
        self.selected.take().is_some()
    }

    pub fn select(&mut self, index: usize) {
        self.selected = Some(index);
    }

    pub fn step(&mut self, rows: &[ResumeRow], down: bool) -> bool {
        let picks = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.picks())
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let current = self.selection(rows);
        let filtering = !self.query().is_empty();
        let next = match (current, down) {
            (None, false) => picks.last().copied(),
            (None, true) => None,
            (Some(at), false) => picks
                .iter()
                .rev()
                .find(|index| **index < at)
                .copied()
                .or(Some(at)),
            (Some(at), true) => picks
                .iter()
                .find(|index| **index > at)
                .copied()
                .or_else(|| filtering.then_some(at)),
        };
        let moved = next != current;
        self.selected = next;
        moved
    }

    pub fn back(&mut self) -> bool {
        self.selected = None;
        self.open.take().is_some()
    }

    pub fn pick(&mut self, row: &ResumeRow, cwd: &Path) -> Option<AgentSessionOpKind> {
        match row {
            ResumeRow::Heading { back: true, .. } => {
                self.back();
                None
            }
            ResumeRow::Heading { .. } | ResumeRow::Empty => None,
            ResumeRow::Chat { chat, .. } => Some(AgentSessionOpKind::Switch {
                session_id: chat.session_id.clone(),
                cwd: chat.cwd.clone(),
                additional_directories: chat.additional_directories.clone(),
            }),
            ResumeRow::Project { cwd: folder, .. } => {
                self.selected = None;
                self.filter.clear();
                self.open = Some((folder.clone(), Listing::default()));
                Some(list(Some(folder.clone()), None))
            }
            ResumeRow::More(More::Here) => {
                self.selected = None;
                self.open = Some((cwd.to_path_buf(), self.here.clone()));
                None
            }
            ResumeRow::More(More::Chats) => {
                self.selected = None;
                let (folder, listing) = self.open.as_mut()?;
                listing.page(Some(folder.clone()), usize::MAX)
            }
            ResumeRow::More(More::Projects) => {
                self.selected = None;
                let loaded = self.projects(cwd, false).len();
                if !self.every_project && loaded > PROJECT_ROWS {
                    self.every_project = true;
                    return None;
                }
                self.every_project = true;
                self.recent.page(None, usize::MAX)
            }
        }
    }
}

fn list(cwd: Option<PathBuf>, cursor: Option<String>) -> AgentSessionOpKind {
    AgentSessionOpKind::List {
        cwd,
        replace: cursor.is_none(),
        cursor,
    }
}

fn title(chat: &AgentSessionSummary) -> String {
    chat.title
        .clone()
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| UNTITLED.to_owned())
}

fn scratch(folder: &Path) -> bool {
    TEMP_ROOTS.iter().any(|root| folder.starts_with(root))
        || folder
            .components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        || home_relative(&folder.to_string_lossy()).starts_with("~/Library/")
}

fn folder_name(folder: &Path) -> String {
    folder.file_name().map_or_else(
        || folder.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn age(updated: Option<&str>, now: u64) -> Option<String> {
    updated
        .and_then(unix_seconds)
        .map(|at| short_age(now.saturating_sub(at)))
}

fn unix_seconds(stamp: &str) -> Option<u64> {
    let number = |range: std::ops::Range<usize>| stamp.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hour * 3_600 + minute * 60 + second).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str, cwd: &str, title: &str, updated: &str) -> AgentSessionSummary {
        AgentSessionSummary {
            session_id: id.to_owned(),
            cwd: PathBuf::from(cwd),
            additional_directories: Vec::new(),
            title: Some(title.to_owned()),
            updated_at: Some(updated.to_owned()),
        }
    }

    fn listed(chats: Vec<AgentSessionSummary>, filter: Option<&str>, more: Option<&str>) -> String {
        serde_json::to_string(&AgentStreamPayload::SessionsListed {
            client: zz_protocol::ClientId(1),
            sessions: chats,
            next_cursor: more.map(str::to_owned),
            cwd_filter: filter.map(PathBuf::from),
            replace: true,
        })
        .expect("reply")
    }

    const NOW: u64 = 1_794_312_000;

    #[test]
    fn the_empty_chat_lists_this_folders_chats_then_the_other_folders() {
        let cwd = Path::new("/src/zz");
        let mut resume = Resume::default();
        assert!(resume.apply(
            cwd,
            &listed(
                vec![
                    chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
                    chat("own", "/src/zz", "This chat", "2026-11-10T09:00:00Z"),
                ],
                Some("/src/zz"),
                None,
            ),
        ));
        assert!(resume.apply(
            cwd,
            &listed(
                vec![
                    chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
                    chat("b", "/src/penna", "Focus ring", "2026-11-10T09:00:00Z"),
                    chat("c", "/src/penna", "Chip tones", "2026-11-08T12:00:00Z"),
                    chat("d", "/src/nidus", "", "2026-11-08T12:00:00Z"),
                ],
                None,
                None,
            ),
        ));
        assert_eq!(
            resume.rows(cwd, Some("own"), NOW),
            [
                ResumeRow::Heading {
                    text: "Recent here".to_owned(),
                    back: false
                },
                ResumeRow::Chat {
                    chat: chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
                    title: "Fix the stack".to_owned(),
                    place: None,
                    age: Some("2h".to_owned()),
                },
                ResumeRow::Heading {
                    text: "Projects".to_owned(),
                    back: false
                },
                ResumeRow::Project {
                    cwd: PathBuf::from("/src/penna"),
                    name: "penna".to_owned(),
                    path: "/src".to_owned(),
                    age: Some("3h".to_owned()),
                },
                ResumeRow::Project {
                    cwd: PathBuf::from("/src/nidus"),
                    name: "nidus".to_owned(),
                    path: "/src".to_owned(),
                    age: Some("2d".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn a_project_opens_its_chats_and_a_chat_switches_the_pane_to_it() {
        let cwd = Path::new("/src/zz");
        let mut resume = Resume::default();
        resume.apply(
            cwd,
            &listed(
                vec![chat(
                    "b",
                    "/src/penna",
                    "Focus ring",
                    "2026-11-10T09:00:00Z",
                )],
                None,
                None,
            ),
        );
        let rows = resume.rows(cwd, None, NOW);
        assert!(!resume.step(&rows, true));
        assert!(resume.step(&rows, false));
        assert_eq!(resume.selection(&rows), Some(1));
        assert_eq!(
            resume.pick(&rows[1], cwd),
            Some(AgentSessionOpKind::List {
                cwd: Some(PathBuf::from("/src/penna")),
                cursor: None,
                replace: true,
            })
        );
        resume.apply(
            cwd,
            &listed(
                vec![chat(
                    "b",
                    "/src/penna",
                    "Focus ring",
                    "2026-11-10T09:00:00Z",
                )],
                Some("/src/penna"),
                Some("50"),
            ),
        );
        let rows = resume.rows(cwd, None, NOW);
        assert_eq!(
            rows[0],
            ResumeRow::Heading {
                text: "penna".to_owned(),
                back: true
            }
        );
        assert_eq!(rows[2], ResumeRow::More(More::Chats));
        assert_eq!(
            resume.pick(&rows[2], cwd),
            Some(AgentSessionOpKind::List {
                cwd: Some(PathBuf::from("/src/penna")),
                cursor: Some("50".to_owned()),
                replace: false,
            })
        );
        assert_eq!(
            resume.pick(&rows[1], cwd),
            Some(AgentSessionOpKind::Switch {
                session_id: "b".to_owned(),
                cwd: PathBuf::from("/src/penna"),
                additional_directories: Vec::new(),
            })
        );
        assert!(resume.back());
        assert_eq!(resume.rows(cwd, None, NOW).len(), 2);
    }
}
