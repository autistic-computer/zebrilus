use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use zz_protocol::{
    AgentSessionOpKind,
    agent_stream::{AgentSessionSummary, AgentStreamPayload},
};

use crate::{fuzzy::fuzzy_match, providers::home_relative, window_summary::short_age};

pub const UNTITLED: &str = "Untitled chat";
pub const NOTHING_MATCHES: &str = "Nothing matches";
pub const NO_CHATS_HERE: &str = "No chats here yet";
pub const OTHER_FOLDERS: &str = "Other folders";
pub const LOADING: &str = "Loading";
const HERE_CHATS: usize = 8;
const OTHER_CHATS: usize = 8;
const PATH_MATCH: usize = 1_000;
const PAGES: usize = 40;
const FOUND: usize = 40;
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

    fn waiting(&self) -> bool {
        self.pages == 0 || (self.asked.is_some() && self.asked == self.more)
    }

    fn fail(&mut self) {
        if self.waiting() {
            self.pages = self.pages.max(1);
            if self.asked == self.more {
                self.more = None;
            }
        }
    }

    fn page(&mut self, cwd: Option<PathBuf>) -> Option<AgentSessionOpKind> {
        let more = self.more.clone()?;
        if self.pages >= PAGES || self.asked.as_ref() == Some(&more) {
            return None;
        }
        self.asked = Some(more.clone());
        Some(list(cwd, Some(more)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Folders,
    Chats,
}

impl Menu {
    #[must_use]
    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::Folders => "Search folders",
            Self::Chats => "Search chats",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeRow {
    Label(String),
    Folder {
        cwd: PathBuf,
        name: String,
        path: String,
        age: Option<String>,
        current: bool,
    },
    Chat {
        chat: AgentSessionSummary,
        title: String,
        place: Option<String>,
        age: Option<String>,
    },
    Loading,
    Note(&'static str),
}

impl ResumeRow {
    #[must_use]
    pub const fn picks(&self) -> bool {
        matches!(self, Self::Folder { .. } | Self::Chat { .. })
    }

    fn key(&self) -> Option<String> {
        match self {
            Self::Folder { cwd, .. } => Some(cwd.to_string_lossy().into_owned()),
            Self::Chat { chat, .. } => Some(chat.session_id.clone()),
            Self::Label(_) | Self::Loading | Self::Note(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resume {
    here: Listing,
    recent: Listing,
    menu: Option<Menu>,
    search: String,
    picked: Option<String>,
}

impl Resume {
    #[must_use]
    pub fn start(cwd: &Path) -> [AgentSessionOpKind; 2] {
        [list(Some(cwd.to_path_buf()), None), list(None, None)]
    }

    pub fn apply(&mut self, cwd: &Path, reply: &str) -> bool {
        let (sessions, next_cursor, cwd_filter, replace) = match serde_json::from_str(reply) {
            Ok(AgentStreamPayload::SessionsListed {
                sessions,
                next_cursor,
                cwd_filter,
                replace,
                ..
            }) => (sessions, next_cursor, cwd_filter, replace),
            Ok(AgentStreamPayload::SessionListFailed { .. }) => {
                self.here.fail();
                self.recent.fail();
                return true;
            }
            _ => return false,
        };
        match cwd_filter {
            None => self.recent.take(sessions, next_cursor, replace),
            Some(filter) if filter == cwd => self.here.take(sessions, next_cursor, replace),
            Some(_) => return false,
        }
        true
    }

    pub fn wanted(&mut self, cwd: &Path) -> Option<AgentSessionOpKind> {
        let menu = self.menu?;
        if self.here.waiting() || self.recent.waiting() {
            return None;
        }
        (menu == Menu::Chats)
            .then(|| self.here.page(Some(cwd.to_path_buf())))
            .flatten()
            .or_else(|| self.recent.page(None))
    }

    #[must_use]
    pub const fn menu(&self) -> Option<Menu> {
        self.menu
    }

    pub fn open(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.search.clear();
        self.picked = None;
    }

    pub fn close(&mut self) -> bool {
        self.search.clear();
        self.picked = None;
        self.menu.take().is_some()
    }

    #[must_use]
    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn set_search(&mut self, text: &str) -> bool {
        if self.search == text {
            return false;
        }
        text.clone_into(&mut self.search);
        self.picked = None;
        true
    }

    fn query(&self) -> String {
        self.search.trim().to_lowercase()
    }

    #[must_use]
    pub fn rows(&self, cwd: &Path, session: Option<&str>, now: u64) -> Vec<ResumeRow> {
        match self.menu {
            None => Vec::new(),
            Some(Menu::Folders) => self.folders(cwd, now),
            Some(Menu::Chats) => self.chats(cwd, session, now),
        }
    }

    fn folders(&self, cwd: &Path, now: u64) -> Vec<ResumeRow> {
        if self.recent.pages == 0 {
            return vec![ResumeRow::Loading];
        }
        let query = self.query();
        let score = |text: &str| fuzzy_match(&query, text).map(|(score, _)| score);
        let row = |(folder, updated): (&Path, Option<&str>)| ResumeRow::Folder {
            cwd: folder.to_path_buf(),
            name: folder_name(folder),
            path: folder
                .parent()
                .map(|parent| home_relative(&parent.to_string_lossy()))
                .unwrap_or_default(),
            age: age(updated, now).filter(|_| folder != cwd),
            current: folder == cwd,
        };
        let folders = std::iter::once((cwd, None)).chain(self.projects(cwd, !query.is_empty()));
        let mut rows = if query.is_empty() {
            folders.map(row).collect::<Vec<_>>()
        } else {
            let mut found = folders
                .filter_map(|folder| {
                    let named = score(&folder_name(folder.0));
                    let pathed = || {
                        score(&home_relative(&folder.0.to_string_lossy()))
                            .map(|score| score + PATH_MATCH)
                    };
                    Some((named.or_else(pathed)?, folder))
                })
                .collect::<Vec<_>>();
            found.sort_by_key(|(score, _)| *score);
            found
                .into_iter()
                .take(FOUND)
                .map(|(_, folder)| row(folder))
                .collect()
        };
        if self.recent.waiting() {
            rows.push(ResumeRow::Loading);
        } else if rows.is_empty() {
            rows.push(ResumeRow::Note(NOTHING_MATCHES));
        }
        rows
    }

    fn chats(&self, cwd: &Path, session: Option<&str>, now: u64) -> Vec<ResumeRow> {
        if self.here.pages == 0 || self.recent.pages == 0 {
            return vec![ResumeRow::Loading];
        }
        let query = self.query();
        let score = |text: &str| fuzzy_match(&query, text).map(|(score, _)| score);
        let row = |chat: &AgentSessionSummary, place: bool| ResumeRow::Chat {
            chat: chat.clone(),
            title: title(chat),
            place: place.then(|| folder_name(&chat.cwd)),
            age: age(chat.updated_at.as_deref(), now),
        };
        let other = |chat: &&AgentSessionSummary| Some(chat.session_id.as_str()) != session;
        let mut rows = Vec::new();
        if query.is_empty() {
            rows.push(ResumeRow::Label(format!("In {}", folder_name(cwd))));
            let here = self
                .here
                .chats
                .iter()
                .filter(other)
                .take(HERE_CHATS)
                .collect::<Vec<_>>();
            if here.is_empty() {
                rows.push(ResumeRow::Note(NO_CHATS_HERE));
            }
            rows.extend(here.into_iter().map(|chat| row(chat, false)));
            let others = self
                .recent
                .chats
                .iter()
                .filter(other)
                .filter(|chat| chat.cwd != cwd)
                .take(OTHER_CHATS)
                .collect::<Vec<_>>();
            if !others.is_empty() {
                rows.push(ResumeRow::Label(OTHER_FOLDERS.to_owned()));
                rows.extend(others.into_iter().map(|chat| row(chat, true)));
            }
        } else {
            let mut seen = HashSet::new();
            let mut found = self
                .here
                .chats
                .iter()
                .chain(&self.recent.chats)
                .filter(other)
                .filter(|chat| seen.insert(chat.session_id.as_str()))
                .filter_map(|chat| {
                    let titled = score(&title(chat));
                    let placed = score(&folder_name(&chat.cwd)).map(|score| score + PATH_MATCH);
                    Some((titled.into_iter().chain(placed).min()?, chat))
                })
                .collect::<Vec<_>>();
            found.sort_by(|(left_score, left), (right_score, right)| {
                left_score
                    .cmp(right_score)
                    .then_with(|| right.updated_at.cmp(&left.updated_at))
            });
            rows.extend(
                found
                    .into_iter()
                    .take(FOUND)
                    .map(|(_, chat)| row(chat, true)),
            );
        }
        if self.here.waiting() || self.recent.waiting() {
            rows.push(ResumeRow::Loading);
        } else if rows.is_empty() {
            rows.push(ResumeRow::Note(NOTHING_MATCHES));
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
        self.picked
            .as_ref()
            .and_then(|picked| {
                rows.iter()
                    .position(|row| row.key().as_ref() == Some(picked))
            })
            .or_else(|| rows.iter().position(ResumeRow::picks))
    }

    pub fn select(&mut self, rows: &[ResumeRow], index: usize) -> bool {
        let Some(key) = rows.get(index).and_then(ResumeRow::key) else {
            return false;
        };
        let changed = self.picked.as_ref() != Some(&key);
        self.picked = Some(key);
        changed
    }

    pub fn step(&mut self, rows: &[ResumeRow], down: bool) -> Option<usize> {
        let at = self.selection(rows)?;
        let picks = |(_, row): &(usize, &ResumeRow)| row.picks();
        let next = if down {
            rows.iter().enumerate().skip(at + 1).find(picks)
        } else {
            rows.iter().enumerate().take(at).rev().find(picks)
        }
        .map_or(at, |(index, _)| index);
        self.select(rows, next);
        Some(next)
    }

    pub fn pick(&mut self, row: &ResumeRow, cwd: &Path) -> Option<AgentSessionOpKind> {
        let op = match row {
            ResumeRow::Folder { cwd: folder, .. } => {
                (folder != cwd).then(|| AgentSessionOpKind::New {
                    cwd: folder.clone(),
                })
            }
            ResumeRow::Chat { chat, .. } => Some(AgentSessionOpKind::Switch {
                session_id: chat.session_id.clone(),
                cwd: chat.cwd.clone(),
                additional_directories: chat.additional_directories.clone(),
            }),
            ResumeRow::Label(_) | ResumeRow::Loading | ResumeRow::Note(_) => return None,
        };
        self.close();
        op
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

#[must_use]
pub fn folder_name(folder: &Path) -> String {
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
        paged(chats, filter, more, true)
    }

    fn paged(
        chats: Vec<AgentSessionSummary>,
        filter: Option<&str>,
        more: Option<&str>,
        replace: bool,
    ) -> String {
        serde_json::to_string(&AgentStreamPayload::SessionsListed {
            client: zz_protocol::ClientId(1),
            sessions: chats,
            next_cursor: more.map(str::to_owned),
            cwd_filter: filter.map(PathBuf::from),
            replace,
        })
        .expect("reply")
    }

    const NOW: u64 = 1_794_312_000;

    fn folder(name: &str, path: &str, age: Option<&str>, current: bool) -> ResumeRow {
        ResumeRow::Folder {
            cwd: PathBuf::from(format!("{path}/{name}")),
            name: name.to_owned(),
            path: path.to_owned(),
            age: age.map(str::to_owned),
            current,
        }
    }

    fn listed_both(
        resume: &mut Resume,
        cwd: &Path,
        here: Vec<AgentSessionSummary>,
        recent: Vec<AgentSessionSummary>,
    ) {
        assert!(resume.apply(cwd, &listed(here, Some(&cwd.to_string_lossy()), None)));
        assert!(resume.apply(cwd, &listed(recent, None, None)));
    }

    #[test]
    fn the_folder_menu_lists_this_folder_checked_then_the_others_and_a_pick_starts_a_chat_there() {
        let cwd = Path::new("/src/zz");
        let mut resume = Resume::default();
        listed_both(
            &mut resume,
            cwd,
            Vec::new(),
            vec![
                chat("f", "/scratch/old", "Old notes", "2026-11-10T11:00:00Z"),
                chat("a", "/src/penna", "Focus ring", "2026-11-10T09:00:00Z"),
                chat("b", "/tmp/scratch", "Try it", "2026-11-10T08:00:00Z"),
                chat("c", "/src/zz", "Fix the stack", "2026-11-10T07:00:00Z"),
                chat("d", "/src/nidus", "Wayland", "2026-11-08T12:00:00Z"),
                chat("e", "/src/scratchpad", "Notes", "2026-11-01T12:00:00Z"),
            ],
        );
        resume.open(Menu::Folders);
        let rows = resume.rows(cwd, None, NOW);
        assert_eq!(
            rows,
            [
                folder("zz", "/src", None, true),
                folder("old", "/scratch", Some("1h"), false),
                folder("penna", "/src", Some("3h"), false),
                folder("nidus", "/src", Some("2d"), false),
                folder("scratchpad", "/src", Some("9d"), false),
            ]
        );
        assert_eq!(resume.selection(&rows), Some(0));
        assert_eq!(resume.pick(&rows[0], cwd), None);
        assert_eq!(resume.menu(), None);

        resume.open(Menu::Folders);
        resume.set_search("scratch");
        let rows = resume.rows(cwd, None, NOW);
        assert_eq!(
            rows,
            [
                folder("scratch", "/tmp", Some("4h"), false),
                folder("scratchpad", "/src", Some("9d"), false),
                folder("old", "/scratch", Some("1h"), false),
            ]
        );
        assert_eq!(resume.step(&rows, true), Some(1));
        assert_eq!(resume.step(&rows, false), Some(0));
        assert_eq!(resume.step(&rows, false), Some(0));
        assert_eq!(
            resume.pick(&rows[0], cwd),
            Some(AgentSessionOpKind::New {
                cwd: PathBuf::from("/tmp/scratch"),
            })
        );
        assert_eq!(resume.menu(), None);
    }

    #[test]
    fn resume_lists_this_folders_chats_then_the_newest_elsewhere_and_search_ranks_titles_first() {
        let cwd = Path::new("/src/zz");
        let mut resume = Resume::default();
        let elsewhere = (0..10)
            .map(|n| {
                chat(
                    &format!("p{n}"),
                    "/src/penna",
                    &format!("Penna {n}"),
                    &format!("2026-11-09T{:02}:00:00Z", 20 - n),
                )
            })
            .collect::<Vec<_>>();
        listed_both(
            &mut resume,
            cwd,
            vec![
                chat("own", "/src/zz", "This chat", "2026-11-10T11:00:00Z"),
                chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
            ],
            [
                vec![chat(
                    "a",
                    "/src/zz",
                    "Fix the stack",
                    "2026-11-10T10:00:00Z",
                )],
                elsewhere,
            ]
            .concat(),
        );
        resume.open(Menu::Chats);
        let rows = resume.rows(cwd, Some("own"), NOW);
        let titles = rows
            .iter()
            .map(|row| match row {
                ResumeRow::Label(text) => format!("# {text}"),
                ResumeRow::Chat { title, place, .. } => {
                    format!("{title} {}", place.as_deref().unwrap_or("-"))
                }
                other => format!("{other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            [
                "# In zz",
                "Fix the stack -",
                "# Other folders",
                "Penna 0 penna",
                "Penna 1 penna",
                "Penna 2 penna",
                "Penna 3 penna",
                "Penna 4 penna",
                "Penna 5 penna",
                "Penna 6 penna",
                "Penna 7 penna",
            ]
        );
        assert_eq!(resume.selection(&rows), Some(1));
        assert_eq!(resume.step(&rows, false), Some(1));
        assert_eq!(resume.step(&rows, true), Some(3));

        resume.set_search("penna 9");
        let rows = resume.rows(cwd, Some("own"), NOW);
        assert_eq!(rows.len(), 1);
        resume.set_search("pen");
        let rows = resume.rows(cwd, Some("own"), NOW);
        assert_eq!(rows.len(), 10);
        assert_eq!(resume.selection(&rows), Some(0));
        resume.set_search("zz");
        assert_eq!(
            resume.rows(cwd, Some("own"), NOW),
            [ResumeRow::Chat {
                chat: chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
                title: "Fix the stack".to_owned(),
                place: Some("zz".to_owned()),
                age: Some("2h".to_owned()),
            }]
        );
        resume.set_search("qqq");
        assert_eq!(
            resume.rows(cwd, Some("own"), NOW),
            [ResumeRow::Note(NOTHING_MATCHES)]
        );
        let rows = [ResumeRow::Chat {
            chat: chat("a", "/src/zz", "Fix the stack", "2026-11-10T10:00:00Z"),
            title: "Fix the stack".to_owned(),
            place: None,
            age: None,
        }];
        assert_eq!(
            resume.pick(&rows[0], cwd),
            Some(AgentSessionOpKind::Switch {
                session_id: "a".to_owned(),
                cwd: PathBuf::from("/src/zz"),
                additional_directories: Vec::new(),
            })
        );
    }

    #[test]
    fn an_open_menu_pages_the_listing_one_request_at_a_time_until_it_runs_out() {
        let cwd = Path::new("/src/zz");
        let folders = |from: usize, to: usize| {
            (from..to)
                .map(|n| {
                    chat(
                        &n.to_string(),
                        &format!("/src/p{n}"),
                        "Work",
                        "2026-11-10T09:00:00Z",
                    )
                })
                .collect::<Vec<_>>()
        };
        let page = |cwd: Option<&str>, cursor: &str| {
            Some(AgentSessionOpKind::List {
                cwd: cwd.map(PathBuf::from),
                cursor: Some(cursor.to_owned()),
                replace: false,
            })
        };
        let mut resume = Resume::default();
        resume.open(Menu::Folders);
        assert_eq!(resume.rows(cwd, None, NOW), [ResumeRow::Loading]);
        assert_eq!(resume.wanted(cwd), None);
        resume.apply(cwd, &listed(Vec::new(), Some("/src/zz"), Some("h50")));
        resume.apply(cwd, &listed(folders(0, 3), None, Some("50")));
        resume.close();
        assert_eq!(resume.wanted(cwd), None);

        resume.open(Menu::Folders);
        assert_eq!(resume.wanted(cwd), page(None, "50"));
        assert_eq!(resume.wanted(cwd), None);
        assert_eq!(
            resume.rows(cwd, None, NOW).last(),
            Some(&ResumeRow::Loading)
        );
        resume.apply(cwd, &paged(folders(3, 6), None, Some("100"), false));
        assert_eq!(resume.wanted(cwd), page(None, "100"));
        resume.apply(cwd, &paged(folders(6, 7), None, None, false));
        assert_eq!(resume.wanted(cwd), None);
        let rows = resume.rows(cwd, None, NOW);
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[7], folder("p6", "/src", Some("3h"), false));

        resume.open(Menu::Chats);
        assert_eq!(resume.wanted(cwd), page(Some("/src/zz"), "h50"));
        assert_eq!(resume.wanted(cwd), None);
    }
}
