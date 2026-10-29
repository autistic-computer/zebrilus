//! A folder on a host as the daemon walks it, and the rows a filter picks
//! from it: the folder's own entries while the filter is empty, the best
//! matches anywhere under it once you type, with git's letter on each.
//! [`FilesLevel`] is the palette's `&`: it starts in the place's folder,
//! reads the text after `&` as a path, opens folders and goes up, and puts
//! a pick at the pane's cursor or on the clipboard.

use std::{collections::HashMap, ops::Range, rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, IntoElement,
    Pixels, Render, ScrollStrategy, SharedString, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, div, prelude::*, px, uniform_list,
};
use zz_client::{
    path_rank,
    place::{Level, Place},
    recents::RecentKey,
};
use zz_protocol::{
    GitMark, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PathEntry, PathKind,
    PathListRoot, SessionId, WindowId,
};

use crate::{
    ActiveTheme as _, Colorize as _, IconName,
    button::{Button, ButtonVariants as _},
    command::{
        ActionContext, ActionKey, CommandPaletteView, LevelEffect, PaletteActionLevel,
        PaletteBackend, PaletteHint, PaletteHostId, PaletteRow, command_palette_entry,
        command_palette_list_height, command_palette_row_height, record_recent,
    },
    h_flex,
    touch::CoarsePointer,
};

const MAX_ROWS: usize = 200;

#[derive(Clone, Debug, Default)]
pub(crate) struct GitMarks {
    exact: HashMap<String, GitMark>,
    dirs: Vec<(String, GitMark)>,
}

impl GitMarks {
    pub(crate) fn extend(&mut self, marks: Vec<(String, GitMark)>) {
        for (rel, mark) in marks {
            if rel.ends_with('/') {
                self.dirs.push((rel, mark));
            } else {
                self.exact.insert(rel, mark);
            }
        }
    }

    pub(crate) fn get(&self, rel: &str) -> Option<GitMark> {
        self.exact.get(rel).copied().or_else(|| {
            self.dirs
                .iter()
                .find(|(prefix, _)| {
                    rel.starts_with(prefix.as_str()) || rel == prefix.trim_end_matches('/')
                })
                .map(|(_, mark)| *mark)
        })
    }
}

pub(crate) const fn mark_letter(mark: GitMark) -> &'static str {
    match mark {
        GitMark::Modified => "M",
        GitMark::Added => "A",
        GitMark::Untracked => "U",
        GitMark::Conflicted => "C",
    }
}

/// Everything under one folder on one host, as the daemon walks it.
#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) host: String,
    /// The folder asked for, backslashes resolved.
    pub(crate) dir: String,
    pub(crate) request: Option<u64>,
    pub(crate) root: Option<PathListRoot>,
    pub(crate) chunks: Vec<Arc<[PathEntry]>>,
    pub(crate) marks: Arc<GitMarks>,
    pub(crate) error: Option<SharedString>,
    pub(crate) done: bool,
    pub(crate) truncated: bool,
}

impl Listing {
    pub(crate) fn new(host: &str, dir: String) -> Self {
        Self {
            host: host.to_owned(),
            dir,
            ..Self::default()
        }
    }

    /// Whether the listing holds `dir` on `host` whole: `dir` is its folder
    /// or one under it, and the walk finished without stopping short.
    pub(crate) fn covers(&self, host: &str, dir: &str) -> bool {
        self.host == host
            && dir.starts_with(&self.dir)
            && (dir == self.dir || (self.done && !self.truncated && self.error.is_none()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chunks.iter().all(|chunk| chunk.is_empty())
    }

    pub(crate) fn begin(&mut self, result: Result<PathListRoot, String>) {
        match result {
            Ok(root) => self.root = Some(root),
            Err(error) => {
                self.error = Some(error.into());
                self.done = true;
            }
        }
    }

    /// Take a chunk of entries. Whether the rows may have changed.
    pub(crate) fn chunk(
        &mut self,
        entries: &Arc<[PathEntry]>,
        done: bool,
        truncated: bool,
    ) -> bool {
        let added = !entries.is_empty();
        if added {
            self.chunks.push(Arc::clone(entries));
        }
        let finished = done && !self.done;
        self.done |= done;
        self.truncated |= truncated;
        added || finished
    }

    pub(crate) fn git(&mut self, marks: Vec<(String, GitMark)>) {
        Arc::make_mut(&mut self.marks).extend(marks);
    }
}

/// One file or folder a filter kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub entry: PathEntry,
    /// Its path under the folder listed, without a trailing `/`.
    pub name: String,
    pub matches: Vec<Range<usize>>,
    pub mark: Option<GitMark>,
}

/// The rows under `base`, a folder relative to the listing's own: its
/// entries, folders first, while `filter` is empty, and the best matches
/// anywhere under it once you type.
pub(crate) fn file_rows(
    chunks: &[Arc<[PathEntry]>],
    marks: &GitMarks,
    base: &str,
    filter: &str,
) -> Vec<FileRow> {
    let entries = chunks
        .iter()
        .flat_map(|chunk| chunk.iter())
        .filter(|entry| entry.rel.len() > base.len() && entry.rel.starts_with(base))
        .collect::<Vec<_>>();
    let label = |entry: &PathEntry| entry.rel[base.len()..].trim_end_matches('/').to_owned();
    let row = |entry: &PathEntry, matches| FileRow {
        entry: entry.clone(),
        name: label(entry),
        matches,
        mark: marks.get(&entry.rel),
    };
    if filter.is_empty() {
        let mut shown = entries
            .into_iter()
            .filter(|entry| !label(entry).contains('/'))
            .collect::<Vec<_>>();
        shown.sort_by(|left, right| {
            path_rank::browse_order(
                (left.rel.as_str(), left.kind == PathKind::Dir),
                (right.rel.as_str(), right.kind == PathKind::Dir),
            )
        });
        shown.truncate(MAX_ROWS);
        return shown
            .into_iter()
            .map(|entry| row(entry, Vec::new()))
            .collect();
    }
    let labels = entries.iter().map(|entry| label(entry)).collect::<Vec<_>>();
    let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let kept = path_rank::rank(
        filter,
        &labels,
        |index| {
            path_rank::path_prior(
                filter,
                labels[index],
                marks.get(&entries[index].rel).is_some(),
            )
        },
        MAX_ROWS,
    );
    let kept_labels = kept.iter().map(|index| labels[*index]).collect::<Vec<_>>();
    kept.into_iter()
        .zip(path_rank::highlights(filter, &kept_labels))
        .map(|(index, matches)| row(entries[index], matches))
        .collect()
}

/// What the text after `&` lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilesPath {
    /// The folder to ask the host for: absolute or under `~`, `.` and `..`
    /// folded, ending in `/`.
    pub dir: String,
    /// The text up to and with the last `/`, as typed.
    pub typed: String,
    /// What follows the last `/`.
    pub filter: String,
}

/// Read `rest`, the text after `&`, against `start`. A folder starting with
/// `/` or `~` is taken as written; any other is under `start`. Spaces are
/// plain characters.
#[must_use]
pub fn split_rest(rest: &str, start: &str) -> FilesPath {
    let (typed, filter) = match rest.rfind('/') {
        Some(cut) => rest.split_at(cut + 1),
        None if rest.starts_with('~') => ("~/", &rest[1..]),
        None => ("", rest),
    };
    let dir = if typed.starts_with(['/', '~']) {
        normalize_dir(typed)
    } else {
        normalize_dir(&format!("{}/{typed}", start.trim_end_matches('/')))
    };
    FilesPath {
        dir,
        typed: typed.to_owned(),
        filter: filter.to_owned(),
    }
}

/// `dir` with `.` and `..` folded and each `/` single, ending in `/`. A `..`
/// above `/` stays at `/`; one above `~` is kept for the host to resolve.
#[must_use]
pub fn normalize_dir(dir: &str) -> String {
    let (anchor, rest) = if let Some(rest) = dir.strip_prefix('/') {
        ("/".to_owned(), rest)
    } else {
        let (first, rest) = dir.split_once('/').unwrap_or((dir, ""));
        (format!("{first}/"), rest)
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if anchor == "/" => {}
            part => parts.push(part),
        }
    }
    let mut normal = anchor;
    for part in parts {
        normal.push_str(part);
        normal.push('/');
    }
    normal
}

/// The folder above `dir`, or `None` at `/`. `root` is where the host
/// says `dir` is, which takes `~/` up to the folder holding the home.
#[must_use]
pub fn parent_dir(dir: &str, root: Option<&str>) -> Option<String> {
    if dir == "/" {
        return None;
    }
    let up = normalize_dir(&format!("{dir}../"));
    if up.ends_with("../")
        && let Some(root) = root.filter(|root| root.starts_with('/'))
    {
        return parent_dir(&normalize_dir(root), None);
    }
    Some(up)
}

/// The last folder name in `dir`, for highlighting it after going up.
fn last_name(dir: &str) -> Option<String> {
    let name = dir.trim_end_matches('/').rsplit('/').next()?;
    (!name.is_empty() && !name.starts_with('~') && name != "..").then(|| name.to_owned())
}

/// A picked path as the clipboard takes it: absolute, and `host:` in front
/// when the file is on another host. Never quoted.
#[must_use]
pub fn plain_path(root: &str, rel: &str, host: Option<&str>) -> String {
    let path = format!(
        "{}/{}",
        root.trim_end_matches('/'),
        rel.trim_end_matches('/')
    );
    match host {
        Some(host) => format!("{host}:{path}"),
        None => path,
    }
}

/// The folder `&` starts in for a pane whose folder is `cwd`, under `~`
/// when that is in `home`, and the home when there is none.
#[must_use]
pub fn start_dir(cwd: Option<&str>, home: Option<&str>) -> String {
    let Some(cwd) = cwd.filter(|cwd| cwd.starts_with(['/', '~'])) else {
        return "~/".to_owned();
    };
    let home = home
        .map(|home| home.trim_end_matches('/'))
        .filter(|home| !home.is_empty());
    let dir = match home.and_then(|home| cwd.strip_prefix(home)) {
        Some("") => "~".to_owned(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => cwd.to_owned(),
    };
    normalize_dir(&dir)
}

/// A place as `&` reads it: its host and the parts written under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilesPlace {
    pub host: PaletteHostId,
    pub session: Option<SessionId>,
    pub window: Option<WindowId>,
    pub pane: Option<PaneId>,
}

impl FilesPlace {
    #[must_use]
    pub const fn host(host: PaletteHostId) -> Self {
        Self {
            host,
            session: None,
            window: None,
            pane: None,
        }
    }

    const fn below_host(&self) -> bool {
        self.session.is_some() || self.window.is_some() || self.pane.is_some()
    }
}

/// The pane a place starts in: the pane, else the window's active pane,
/// else the session's active window's.
fn place_pane(snapshot: &MuxSnapshot, place: &FilesPlace) -> Option<PaneId> {
    if place.pane.is_some() {
        return place.pane;
    }
    let windows = snapshot
        .sessions
        .iter()
        .filter(|session| place.session.is_none_or(|id| session.id == id))
        .flat_map(|session| {
            session.windows.iter().filter(move |window| {
                place
                    .window
                    .map_or(window.id == session.active_window, |id| window.id == id)
            })
        });
    windows.map(|window| window.active_pane).next()
}

fn pane_folder(snapshot: &MuxSnapshot, pane: PaneId) -> Option<&str> {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find_map(|window| window.panes.get(&pane))
        .filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal))
        .map(|pane| pane.summary.cwd.as_str())
        .filter(|cwd| !cwd.is_empty())
}

/// Where `&` starts after `place`: a pane, window or session's folder; on
/// the host you are on with nothing under it, `insert`'s folder; on another
/// host, or for a pane with no folder, the home there.
#[must_use]
pub fn start_folder(
    place: &FilesPlace,
    here: bool,
    snapshot: Option<&MuxSnapshot>,
    insert: Option<PaneId>,
    home: Option<&str>,
) -> String {
    let pane = if place.below_host() {
        snapshot.and_then(|snapshot| place_pane(snapshot, place))
    } else if here {
        insert
    } else {
        None
    };
    let cwd = pane.and_then(|pane| snapshot.and_then(|snapshot| pane_folder(snapshot, pane)));
    start_dir(cwd, home)
}

/// Foreground programs whose files are on another machine: the daemon will
/// not list through a pane running one.
const REMOTE_FOREGROUND: &[&str] = &["ssh", "mosh", "mosh-client", "tmux", "zz_cli"];

fn runs_remote(pane: &PaneSnapshot) -> bool {
    pane.summary.process.iter().any(|process| {
        let name = process.rsplit('/').next().unwrap_or(process);
        REMOTE_FOREGROUND.contains(&name)
    })
}

fn terminals(snapshot: &MuxSnapshot) -> impl Iterator<Item = &PaneSnapshot> {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .flat_map(|window| window.panes.values())
        .filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal) && !pane.dead)
}

#[must_use]
/// The terminal on the host to list through: `anchor` when it is one there,
/// else one not running ssh or the like.
pub fn listing_pane(snapshot: &MuxSnapshot, anchor: Option<PaneId>) -> Option<PaneId> {
    let terminals = terminals(snapshot).collect::<Vec<_>>();
    anchor
        .filter(|anchor| terminals.iter().any(|pane| pane.id == *anchor))
        .or_else(|| {
            terminals
                .iter()
                .find(|pane| !runs_remote(pane))
                .or_else(|| terminals.first())
                .map(|pane| pane.id)
        })
}

/// A listing reply from a host, for [`PaletteBackend::feed_listings`].
#[derive(Clone, Debug)]
pub enum ListingUpdate {
    Begin(Result<PathListRoot, String>),
    Chunk {
        entries: Arc<[PathEntry]>,
        done: bool,
        truncated: bool,
    },
    Git(Vec<(String, GitMark)>),
}

/// Where a backend sends the listing replies it gets: the host, the request
/// and the reply.
pub type ListingSink = Box<dyn Fn(PaletteHostId, u64, ListingUpdate, &mut App)>;

pub enum FilesEvent {
    /// A row was clicked: open the folder, or with `open` false pick it.
    Picked { open: bool },
}

impl EventEmitter<FilesEvent> for FilesLevel {}

/// The palette's `&`: the files under a folder on one host. Its owner hands
/// it the text after `&` with [`Self::set_rest`] and applies the text its
/// keys give back.
pub struct FilesLevel {
    backend: Rc<dyn PaletteBackend>,
    host: PaletteHostId,
    host_name: String,
    here: bool,
    start: String,
    insert_pane: Option<PaneId>,
    path: FilesPath,
    listing: Option<Listing>,
    rows: Rc<[FileRow]>,
    selected: usize,
    pending: Option<String>,
    wanted: Option<String>,
    scroll: UniformListScrollHandle,
    visible: usize,
    generation: u64,
    ranking: Option<Task<()>>,
    dirty: bool,
    _feed: Option<Subscription>,
}

impl FilesLevel {
    /// `&` after `place`, putting what you pick into `insert_pane`, the pane
    /// `prefix F` came from or the focused one. `start` overrides the folder
    /// the place gives, as `choose-path -c` does.
    pub fn new(
        backend: Rc<dyn PaletteBackend>,
        place: FilesPlace,
        insert_pane: Option<PaneId>,
        start: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree = backend.tree(cx);
        let here = place.host == tree.attached;
        let host_name = tree
            .hosts
            .iter()
            .find(|host| host.id == place.host)
            .map_or_else(String::new, |host| host.name.clone());
        let home = backend.home_dir(place.host, cx);
        let start = start.map_or_else(
            || {
                start_folder(
                    &place,
                    here,
                    backend.host_snapshot(place.host, cx),
                    insert_pane,
                    home.as_deref(),
                )
            },
            |start| start_dir(Some(&start), home.as_deref()),
        );
        let level = cx.entity().downgrade();
        let feed = backend.feed_listings(
            Box::new(move |host, request, update, cx| {
                level
                    .update(cx, |level, cx| level.apply(host, request, update, cx))
                    .ok();
            }),
            cx,
        );
        cx.on_release(|level, cx| {
            if let Some(listing) = level.listing.take() {
                level.cancel(&listing, cx);
            }
        })
        .detach();
        Self {
            backend,
            host: place.host,
            host_name,
            here,
            path: split_rest("", &start),
            start,
            insert_pane,
            listing: None,
            rows: Rc::from([]),
            selected: 0,
            pending: None,
            wanted: None,
            scroll: UniformListScrollHandle::new(),
            visible: usize::MAX,
            generation: 0,
            ranking: None,
            dirty: false,
            _feed: feed,
        }
    }

    /// The folder `&` starts in, which its owner writes after the `&`.
    #[must_use]
    pub fn start(&self) -> &str {
        &self.start
    }

    #[must_use]
    pub const fn path(&self) -> &FilesPath {
        &self.path
    }

    #[must_use]
    pub fn rows(&self) -> &[FileRow] {
        &self.rows
    }

    #[must_use]
    pub fn selected_row(&self) -> Option<&FileRow> {
        self.rows.get(self.selected)
    }

    /// Show only as many rows as fit in `room`.
    pub fn fit(&mut self, room: Pixels) {
        let rows = (f32::from(room) / command_palette_row_height()).floor();
        self.visible = rows.max(1.0) as usize;
    }

    /// The text after `&`: list the folder it names and filter it.
    pub fn set_rest(&mut self, rest: &str, cx: &mut Context<Self>) {
        let path = split_rest(rest, &self.start);
        if path == self.path && self.listing.is_some() {
            return;
        }
        let moved = path.dir != self.path.dir;
        self.path = path;
        if !self
            .listing
            .as_ref()
            .is_some_and(|listing| listing.covers(&self.host_name, &self.path.dir))
        {
            if let Some(listing) = self.listing.take() {
                self.cancel(&listing, cx);
            }
            let mut listing = Listing::new(&self.host_name, self.path.dir.clone());
            match self
                .backend
                .list_path(self.host, &listing.dir, self.insert_pane, cx)
            {
                Ok(request) => listing.request = Some(request),
                Err(error) => {
                    listing.error = Some(error);
                    listing.done = true;
                }
            }
            self.generation += 1;
            self.listing = Some(listing);
        }
        self.refresh(!moved, cx);
    }

    fn cancel(&self, listing: &Listing, cx: &mut App) {
        if let Some(request) = listing.request
            && !listing.done
        {
            self.backend.cancel_path(self.host, request, cx);
        }
    }

    fn apply(
        &mut self,
        host: PaletteHostId,
        request: u64,
        update: ListingUpdate,
        cx: &mut Context<Self>,
    ) {
        let Some(listing) = self
            .listing
            .as_mut()
            .filter(|listing| host == self.host && listing.request == Some(request))
        else {
            return;
        };
        let changed = match update {
            ListingUpdate::Begin(result) => {
                listing.begin(result);
                true
            }
            ListingUpdate::Chunk {
                entries,
                done,
                truncated,
            } => listing.chunk(&entries, done, truncated),
            ListingUpdate::Git(marks) => {
                listing.git(marks);
                true
            }
        };
        if changed {
            self.refresh(true, cx);
        }
    }

    fn refresh(&mut self, keep: bool, cx: &mut Context<Self>) {
        self.dirty = true;
        if self.ranking.is_none() {
            self.rank(keep, cx);
        }
    }

    fn rank(&mut self, keep: bool, cx: &mut Context<Self>) {
        let Some(listing) = &self.listing else {
            return;
        };
        self.dirty = false;
        let generation = self.generation;
        let chunks = listing.chunks.clone();
        let marks = Arc::clone(&listing.marks);
        let base = self
            .path
            .dir
            .strip_prefix(listing.dir.as_str())
            .unwrap_or_default()
            .to_owned();
        let filter = self.path.filter.clone();
        let ranked = cx
            .background_executor()
            .spawn(async move { file_rows(&chunks, &marks, &base, &filter) });
        self.ranking = Some(cx.spawn(async move |level, cx| {
            let rows = ranked.await;
            level
                .update(cx, |level, cx| {
                    level.ranking = None;
                    if level.generation == generation {
                        level.land(keep, rows, cx);
                    }
                    if level.dirty {
                        level.rank(true, cx);
                    }
                })
                .ok();
        }));
    }

    fn land(&mut self, keep: bool, rows: Vec<FileRow>, cx: &mut Context<Self>) {
        let kept = keep
            .then(|| self.selected_row().map(|row| row.name.clone()))
            .flatten();
        self.rows = rows.into();
        let pending = self
            .pending
            .as_ref()
            .and_then(|name| self.rows.iter().position(|row| row.name == *name));
        if pending.is_some()
            || self
                .listing
                .as_ref()
                .is_none_or(|listing| listing.done || listing.error.is_some())
        {
            self.pending = None;
        }
        self.selected = pending
            .or_else(|| kept.and_then(|name| self.rows.iter().position(|row| row.name == name)))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        self.pick_wanted(cx);
        cx.notify();
    }

    /// Pick `path`, absolute on this level's host, as soon as it is listed:
    /// what a recent file does.
    pub fn want(&mut self, path: String) {
        self.wanted = Some(path);
    }

    fn pick_wanted(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = &self.wanted else {
            return;
        };
        let Some(listing) = &self.listing else {
            return;
        };
        let found = listing.root.as_ref().and_then(|root| {
            self.rows
                .iter()
                .position(|row| plain_path(&root.root, &row.entry.rel, None) == *wanted)
        });
        if let Some(index) = found {
            self.wanted = None;
            self.selected = index;
            cx.emit(FilesEvent::Picked { open: false });
        } else if listing.done || listing.error.is_some() {
            self.wanted = None;
        }
    }

    /// The first or, with `last`, the last row.
    pub fn select_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        self.selected = if last {
            self.rows.len().saturating_sub(1)
        } else {
            0
        };
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Up or Down: move the highlight, wrapping.
    pub fn navigate(&mut self, direction: isize, cx: &mut Context<Self>) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        self.selected = if direction < 0 {
            self.selected.checked_sub(1).unwrap_or(count - 1)
        } else {
            (self.selected + 1) % count
        };
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Tab or Right: the text that opens the highlighted folder, `None` on a
    /// file or an empty list.
    #[must_use]
    pub fn open(&self) -> Option<String> {
        let row = self
            .selected_row()
            .filter(|row| row.entry.kind == PathKind::Dir)?;
        Some(format!("{}{}/", self.path.typed, row.name))
    }

    /// Shift-Tab, Left or Backspace: the text that lists the folder above,
    /// with the one you leave highlighted once it lands. `None` at `/`.
    pub fn up(&mut self) -> Option<String> {
        let root = self.absolute_dir();
        let parent = parent_dir(&self.path.dir, root.as_deref())?;
        self.pending = last_name(root.as_deref().unwrap_or(&self.path.dir));
        Some(parent)
    }

    /// Where the listed folder is on its host, once the host said.
    fn absolute_dir(&self) -> Option<String> {
        let listing = self.listing.as_ref()?;
        let root = listing.root.as_ref()?;
        let under = self.path.dir.strip_prefix(listing.dir.as_str())?;
        Some(format!("{}/{under}", root.root.trim_end_matches('/')))
    }

    /// Whether Enter puts the pick at the pane's cursor: there is a pane and
    /// it is not one known to take no text, such as a browser. Otherwise
    /// Enter copies it.
    #[must_use]
    pub fn inserts(&self, cx: &App) -> bool {
        let Some(pane) = self.insert_pane else {
            return false;
        };
        let snapshot = self.backend.snapshot(cx);
        snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .is_none_or(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal) && !pane.dead)
    }

    /// Enter: put the highlighted path at the pane's cursor. Whether it went.
    pub fn insert(&self, cx: &mut App) -> bool {
        let (Some(pane), Some(row), Some(root)) = (
            self.insert_pane,
            self.selected_row(),
            self.listing
                .as_ref()
                .and_then(|listing| listing.root.as_ref()),
        ) else {
            return false;
        };
        self.backend
            .insert_path(pane, self.host, root, &row.entry, cx);
        self.remember(cx);
        true
    }

    #[must_use]
    pub fn picked_folder(&self) -> Option<String> {
        let root = self.listing.as_ref()?.root.as_ref()?;
        let rel = self.selected_row().map_or_else(String::new, |row| {
            let rel = row.entry.rel.trim_end_matches('/');
            match row.entry.kind {
                PathKind::Dir => rel.to_owned(),
                PathKind::File => rel
                    .rsplit_once('/')
                    .map_or_else(String::new, |(parent, _)| parent.to_owned()),
            }
        });
        Some(if rel.is_empty() {
            root.display_root.clone()
        } else {
            format!("{}/{rel}", root.display_root.trim_end_matches('/'))
        })
    }

    /// Alt-Enter: the highlighted path as the clipboard takes it.
    #[must_use]
    pub fn copy_text(&self) -> Option<String> {
        let row = self.selected_row()?;
        let root = self.listing.as_ref()?.root.as_ref()?;
        Some(plain_path(
            &root.root,
            &row.entry.rel,
            (!self.here).then_some(self.host_name.as_str()),
        ))
    }

    /// Put the highlighted path on the clipboard, and give it.
    pub fn copy(&self, cx: &mut App) -> Option<String> {
        let text = self.copy_text()?;
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.remember(cx);
        Some(text)
    }

    /// Keep the highlighted path among the palette's recent files.
    fn remember(&self, cx: &mut App) {
        let (Some(row), Some(root)) = (
            self.selected_row(),
            self.listing
                .as_ref()
                .and_then(|listing| listing.root.as_ref()),
        ) else {
            return;
        };
        record_recent(
            RecentKey::File {
                host: self.host_name.clone(),
                path: plain_path(&root.root, &row.entry.rel, None),
                dir: row.entry.kind == PathKind::Dir,
            },
            plain_path(&root.display_root, &row.entry.rel, None),
            cx,
        );
    }

    /// The footer's keys, Enter's saying what it does.
    #[must_use]
    pub fn hints(&self, cx: &App) -> Vec<PaletteHint> {
        let inserts = self.inserts(cx);
        let mut hints = vec![
            PaletteHint {
                key: "tab right",
                label: "open",
            },
            PaletteHint {
                key: "shift-tab left",
                label: "parent",
            },
            PaletteHint {
                key: "enter",
                label: if inserts { "insert" } else { "copy" },
            },
        ];
        if inserts {
            hints.push(PaletteHint {
                key: "alt-enter",
                label: "copy",
            });
        }
        hints
    }

    fn empty_message(&self) -> SharedString {
        let Some(listing) = &self.listing else {
            return "Listing…".into();
        };
        if let Some(error) = &listing.error {
            return error.clone();
        }
        if !listing.done || self.ranking.is_some() {
            return "Listing…".into();
        }
        if listing.is_empty() {
            return "This folder is empty".into();
        }
        "No matches".into()
    }

    fn row(
        row: &FileRow,
        index: usize,
        selected: bool,
        level: ala::Entity<Self>,
        cx: &App,
    ) -> ala::Div {
        let hover = level.clone();
        let folder = row.entry.kind == PathKind::Dir;
        let insert = (folder && CoarsePointer::get(cx)).then(|| {
            let level = level.clone();
            Button::new(("files-insert", index))
                .icon(IconName::KeyReturn)
                .ghost()
                .tooltip("Insert")
                .on_click(move |_, _, cx| {
                    level.update(cx, |level, cx| {
                        level.selected = index;
                        cx.emit(FilesEvent::Picked { open: false });
                    });
                    cx.stop_propagation();
                })
        });
        let kind = if folder { "dir ›" } else { "file" };
        let right = match row.mark {
            Some(mark) => format!("{} {kind}", mark_letter(mark)),
            None => kind.to_owned(),
        };
        command_palette_entry(
            ("files-row", index),
            &PaletteRow {
                label: row.name.clone().into(),
                matches: row.matches.clone(),
                right: right.into(),
                icon: Some(if folder {
                    IconName::Folder
                } else {
                    IconName::File
                }),
                ..Default::default()
            },
            selected,
            cx,
        )
        .on_mouse_enter(move |_, _, cx| {
            hover.update(cx, |level, cx| {
                if level.selected != index {
                    level.selected = index;
                    cx.notify();
                }
            });
        })
        .on_click(move |_, _, cx| {
            level.update(cx, |level, cx| {
                level.selected = index;
                cx.emit(FilesEvent::Picked { open: folder });
            });
            cx.stop_propagation();
        })
        .map(|row| {
            let row = div().h(px(command_palette_row_height())).child(row);
            match insert {
                Some(insert) => h_flex().child(row.flex_1().min_w_0()).child(insert),
                None => row,
            }
        })
    }
}

impl Render for FilesLevel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = Rc::clone(&self.rows);
        if rows.is_empty() {
            return div()
                .id("files-level")
                .debug_selector(|| "files-level-empty".to_owned())
                .w_full()
                .py(px(16.0))
                .text_center()
                .text_size(crate::rems_from_px(12.0))
                .text_color(cx.theme().foreground.muted())
                .child(self.empty_message())
                .into_any_element();
        }
        let selected = self.selected;
        let level = cx.entity();
        let visible = f32::from(u16::try_from(rows.len().min(self.visible)).unwrap_or(u16::MAX));
        div()
            .id("files-level")
            .debug_selector(|| "files-level".to_owned())
            .w_full()
            .child(
                uniform_list(
                    "files-rows",
                    rows.len(),
                    cx.processor(move |_, range: Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                rows.get(index).map(|row| {
                                    Self::row(row, index, selected == index, level.clone(), cx)
                                })
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h(px(command_palette_row_height() * visible))
                .track_scroll(&self.scroll),
            )
            .into_any_element()
    }
}

/// `&` as a palette action level: a [`FilesLevel`] for the place, fed the
/// text after the sigil.
pub struct FilesAction {
    backend: Rc<dyn PaletteBackend>,
    level: Option<(Entity<FilesLevel>, FilesPlace)>,
    palette: Option<WeakEntity<CommandPaletteView>>,
    clicks: Option<Subscription>,
}

/// Enter, or Tab with `open`, on `level`'s highlighted row: open a folder,
/// put the path at the pane's cursor, or copy it when the pane takes no
/// text.
fn pick(
    level: &Entity<FilesLevel>,
    open: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<LevelEffect> {
    if open {
        return level.read(cx).open().map(LevelEffect::SetRest);
    }
    if level.read(cx).inserts(cx) {
        return level
            .update(cx, |level, cx| level.insert(cx))
            .then_some(LevelEffect::Close);
    }
    let text = level.update(cx, |level, cx| level.copy(cx))?;
    CommandPaletteView::notify_action(format!("Copied {text}"), window, cx);
    Some(LevelEffect::Close)
}

impl FilesAction {
    #[must_use]
    pub fn new(backend: Rc<dyn PaletteBackend>) -> Self {
        Self {
            backend,
            level: None,
            palette: None,
            clicks: None,
        }
    }

    /// Act on a clicked row through the palette, as the keys do.
    fn follow_clicks(&mut self, level: &Entity<FilesLevel>, window: &Window, cx: &mut App) {
        let Some(palette) = self.palette.clone() else {
            return;
        };
        let handle = window.window_handle();
        self.clicks = Some(cx.subscribe(level, move |level, event: &FilesEvent, cx| {
            let FilesEvent::Picked { open } = *event;
            let palette = palette.clone();
            handle
                .update(cx, |_, window, cx| {
                    if let Some(effect) = pick(&level, open, window, cx) {
                        palette
                            .update(cx, |palette, cx| {
                                palette.apply_level_effect(effect, window, cx);
                            })
                            .ok();
                    }
                })
                .ok();
        }));
    }

    #[must_use]
    pub fn level(&self) -> Option<&Entity<FilesLevel>> {
        self.level.as_ref().map(|(level, _)| level)
    }
}

/// The parts of `context`'s place that were written, which pick where `&`
/// starts.
fn files_place(context: &ActionContext) -> FilesPlace {
    let Place {
        host,
        session,
        window,
        pane,
    } = context.place;
    let written = |level| context.written.contains(level);
    FilesPlace {
        host: PaletteHostId(host.unwrap_or(context.fleet.here)),
        session: session.filter(|_| written(Level::Session)),
        window: window.filter(|_| written(Level::Window)),
        pane: pane.filter(|_| written(Level::Pane)),
    }
}

impl PaletteActionLevel for FilesAction {
    fn attach(&mut self, palette: WeakEntity<CommandPaletteView>, _: &mut App) {
        self.palette = Some(palette);
    }

    fn update(
        &mut self,
        context: &ActionContext,
        window: &mut Window,
        cx: &mut App,
    ) -> LevelEffect {
        let place = files_place(context);
        if self.level.as_ref().is_none_or(|(_, last)| *last != place) {
            let backend = Rc::clone(&self.backend);
            let insert = context.insert_pane;
            let level = cx.new(|cx| FilesLevel::new(backend, place, insert, None, cx));
            let start = level.read(cx).start().to_owned();
            self.follow_clicks(&level, window, cx);
            self.level = Some((level, place));
            if context.rest.is_empty() {
                return LevelEffect::SetRest(start);
            }
        }
        if let Some((level, _)) = &self.level {
            level.update(cx, |level, cx| level.set_rest(&context.rest, cx));
        }
        LevelEffect::Stay
    }

    fn key(&mut self, key: ActionKey, window: &mut Window, cx: &mut App) -> Option<LevelEffect> {
        let level = self.level()?.clone();
        let rest = |text: Option<String>| text.map(LevelEffect::SetRest);
        match key {
            ActionKey::Up | ActionKey::Down => {
                let direction = if key == ActionKey::Up { -1 } else { 1 };
                level.update(cx, |level, cx| level.navigate(direction, cx));
                Some(LevelEffect::Stay)
            }
            ActionKey::First | ActionKey::Last => {
                level.update(cx, |level, cx| {
                    level.select_edge(key == ActionKey::Last, cx);
                });
                Some(LevelEffect::Stay)
            }
            ActionKey::Tab => pick(&level, true, window, cx).or(Some(LevelEffect::Stay)),
            ActionKey::Right => rest(level.read(cx).open()),
            ActionKey::ShiftTab => {
                rest(level.update(cx, |level, _| level.up())).or(Some(LevelEffect::Stay))
            }
            ActionKey::Left | ActionKey::Backspace => {
                let path = level.read(cx).path();
                let edge =
                    path.filter.is_empty() && (key == ActionKey::Left || !path.typed.is_empty());
                if edge {
                    rest(level.update(cx, |level, _| level.up()))
                } else {
                    None
                }
            }
            ActionKey::Enter => pick(&level, false, window, cx),
            ActionKey::Copy => level
                .update(cx, |level, cx| level.copy(cx))
                .map(LevelEffect::Copied),
        }
    }

    fn rows(&mut self, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(level) = self.level().cloned() else {
            return div().into_any_element();
        };
        let room = px(command_palette_list_height(
            window.viewport_size().height,
            false,
        ));
        level.update(cx, |level, _| level.fit(room));
        level.into_any_element()
    }

    fn hints(&self, cx: &App) -> Vec<PaletteHint> {
        self.level()
            .map(|level| level.read(cx).hints(cx))
            .unwrap_or_default()
    }

    fn copy_text(&self, cx: &App) -> Option<String> {
        self.level()?.read(cx).copy_text()
    }

    fn picked_folder(&self, cx: &App) -> Option<(PaletteHostId, String)> {
        let (level, place) = self.level.as_ref()?;
        Some((place.host, level.read(cx).picked_folder()?))
    }

    fn open_recent(&mut self, key: &RecentKey, _: &mut Window, cx: &mut App) -> LevelEffect {
        if let (RecentKey::File { path, .. }, Some(level)) = (key, self.level()) {
            level.update(cx, |level, _| level.want(path.clone()));
        }
        LevelEffect::Stay
    }
}

#[cfg(test)]
#[path = "files_tests.rs"]
pub(crate) mod tests;
