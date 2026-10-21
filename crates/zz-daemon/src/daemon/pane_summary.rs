use super::*;
use std::collections::btree_map::Entry;
use zz_daemon_client::process_info;
use zz_protocol::{
    MAX_PANE_BRANCH_BYTES, MAX_PANE_CWD_BYTES, MAX_PANE_NAME_BYTES, MAX_PANE_PROCESS_BYTES,
    MAX_PANE_PROCESS_ENTRIES, ShellCommand,
};
use zz_terminal::{ShellMark, ShellMarkKind};

pub(super) fn pane_window_watched(inner: &ServerState, pane: PaneId) -> bool {
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return false;
    };
    let Some(session) = inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|window| window.session)
    else {
        return false;
    };
    inner
        .engine
        .state
        .sessions
        .get(&session)
        .is_some_and(|state| state.active_window == window)
        && inner
            .attached
            .get(&session)
            .is_some_and(|clients| !clients.is_empty())
}

pub(super) fn pane_window_seen(inner: &ServerState, pane: PaneId) -> bool {
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return false;
    };
    let Some(session) = inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|window| window.session)
    else {
        return false;
    };
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return false;
    };
    inner.attached.get(&session).is_some_and(|clients| {
        clients.iter().any(|&client| {
            inner.client(client).and_then(|client| client.focused) != Some(false)
                && client_focused_window(inner, client, session_state) == window
        })
    })
}

pub(super) fn mark_focused_window_seen(inner: &mut ServerState, client: ClientId) -> bool {
    let Some(session) = client_attached_session(inner, client) else {
        return false;
    };
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return false;
    };
    let window = client_focused_window(inner, client, session_state);
    let panes = inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|window| window.pane_order().to_vec())
        .unwrap_or_default();
    let mut changed = false;
    for pane in panes {
        changed |= inner
            .engine
            .state
            .update_pane_summary(pane, |summary| summary.unseen = false);
    }
    changed
}

pub(super) fn apply_pane_status(
    inner: &mut ServerState,
    pane: PaneId,
    status: Option<PaneStatus>,
    now: u64,
) -> bool {
    let Some((previous, reports_done)) = inner.engine.state.pane(pane).map(|pane| {
        (
            status_edge(pane.status.as_ref()),
            matches!(pane.kind, PaneKind::Agent(_)),
        )
    }) else {
        return false;
    };
    let next = status_edge(status.as_ref());
    let changed = inner.engine.state.set_pane_status(pane, status);
    if previous == next
        || previous.is_none() && next.is_some_and(|(state, _)| state == PaneStatusState::Idle)
    {
        return changed;
    }
    let unseen = match (
        previous.map(|(state, _)| state),
        next.map(|(state, _)| state),
    ) {
        (_, Some(PaneStatusState::Working | PaneStatusState::Blocked)) => Some(false),
        (
            Some(PaneStatusState::Working | PaneStatusState::Blocked),
            Some(PaneStatusState::Idle),
        ) if !reports_done => Some(!pane_window_seen(inner, pane)),
        (previous, Some(PaneStatusState::Done)) if previous != Some(PaneStatusState::Done) => {
            Some(!pane_window_seen(inner, pane))
        }
        _ => None,
    };
    let stamped = inner.engine.state.update_pane_summary(pane, |summary| {
        summary.changed_at = now;
        if let Some(unseen) = unseen {
            summary.unseen = unseen;
        }
    });
    changed || stamped
}

fn status_edge(status: Option<&PaneStatus>) -> Option<(PaneStatusState, Option<PaneStatusKind>)> {
    status.map(|status| (status.state, status.kind))
}

pub(super) fn agent_state_status(value: Option<&str>) -> Option<PaneStatus> {
    let (state, kind) = match value? {
        "working" => (PaneStatusState::Working, None),
        "blocked" => (PaneStatusState::Blocked, None),
        "needs-approval" => (PaneStatusState::Blocked, Some(PaneStatusKind::Permission)),
        "needs-input" => (PaneStatusState::Blocked, Some(PaneStatusKind::Question)),
        "failed" | "error" => (PaneStatusState::Error, None),
        "done" => (PaneStatusState::Done, None),
        "idle" | "starting" => (PaneStatusState::Idle, None),
        _ => return None,
    };
    Some(PaneStatus {
        state,
        kind,
        progress: None,
        app: String::new(),
        title: String::new(),
        message: String::new(),
    })
}

pub(super) fn mirror_agent_state(inner: &mut ServerState, pane: PaneId, now: u64) -> bool {
    let Some(current) = inner
        .engine
        .state
        .pane(pane)
        .filter(|pane| matches!(pane.kind, PaneKind::Terminal))
        .map(|pane| pane.status.is_some())
    else {
        return false;
    };
    if inner.program_status_panes.contains(&pane) {
        return false;
    }
    let status = agent_state_status(inner.engine.pane_user_option(pane, "@agent_state"));
    if !current
        && status
            .as_ref()
            .is_some_and(|status| status.state == PaneStatusState::Idle)
    {
        return false;
    }
    apply_pane_status(inner, pane, status, now)
}

pub(super) fn mirror_pane_name(inner: &mut ServerState, pane: PaneId) -> bool {
    let mut name = inner
        .engine
        .pane_user_option(pane, "@name")
        .unwrap_or_default()
        .to_owned();
    name.truncate(name.floor_char_boundary(MAX_PANE_NAME_BYTES));
    inner
        .engine
        .state
        .update_pane_summary(pane, |summary| summary.name = name)
}

pub(super) fn note_shell_mark(inner: &mut ServerState, pane: PaneId, mark: &ShellMark) -> bool {
    let (command, at) = match mark.kind {
        ShellMarkKind::Prompt => return false,
        ShellMarkKind::CommandStarted => (ShellCommand::Running, mark.facts.command_start_time),
        ShellMarkKind::CommandFinished => (
            ShellCommand::Finished(mark.facts.command_status.unwrap_or_default()),
            mark.facts.command_end_time,
        ),
    };
    if !inner
        .engine
        .state
        .pane(pane)
        .is_some_and(|pane| matches!(pane.kind, PaneKind::Terminal))
    {
        return false;
    }
    let at = if at == 0 { unix_timestamp() } else { at };
    inner.engine.state.update_pane_summary(pane, |summary| {
        summary.command = Some(command);
        summary.changed_at = at;
    })
}

pub(super) fn stamp_pane(inner: &mut ServerState, pane: PaneId, now: u64) -> bool {
    inner
        .engine
        .state
        .update_pane_summary(pane, |summary| summary.changed_at = now)
}

const LOCATION_FRESH: Duration = Duration::from_secs(10);
const PROGRAM_SETTLE: Duration = Duration::from_millis(250);
const PROGRAM_RECHECK: Duration = Duration::from_secs(2);
const LOCATION_RECHECK: Duration = Duration::from_secs(2);
const KNOWN_LOCATIONS: usize = 64;

type LocationSource = (String, PathBuf);
type ProgramSource = (u32, Option<u32>, String);

enum Job {
    Location(PaneId, LocationSource),
    Program(PaneId, ProgramSource),
}

struct KnownLocation {
    source: LocationSource,
    cwd: String,
    repo: String,
    branch: String,
    read_at: Instant,
}

#[derive(Default)]
pub(super) struct Locations {
    jobs: BTreeMap<PaneId, bool>,
    queue: VecDeque<(PaneId, LocationSource)>,
    running: bool,
    known: VecDeque<KnownLocation>,
    unread: BTreeSet<PaneId>,
    recheck: bool,
    programs: BTreeMap<PaneId, ProgramSource>,
    program_jobs: BTreeMap<PaneId, bool>,
    program_queue: VecDeque<(PaneId, ProgramSource)>,
    program_settling: BTreeMap<PaneId, Instant>,
    program_checked: BTreeMap<PaneId, Instant>,
}

impl Shared {
    pub(super) fn publish_unpublished_tree(self: &Arc<Self>) {
        let unpublished = {
            let inner = self.inner.lock();
            inner.last_published_mux_generation != inner.engine.state.generation()
        };
        if unpublished {
            self.request_publish(timers::PublishReason::Tree);
        }
    }

    pub(super) fn request_pane_location(self: &Arc<Self>, pane: PaneId) {
        if queue_location(&mut self.inner.lock(), pane) {
            self.start_location_worker();
        }
    }

    pub(super) fn locate_moved_pane(self: &Arc<Self>, pane: PaneId) -> bool {
        let (changed, start, recheck) = place_pane(&mut self.inner.lock(), pane);
        if start {
            self.start_location_worker();
        }
        if recheck {
            let owner = Arc::downgrade(&self.server_owner());
            let _ = self.timer_tx.send(timers::TimerInput::Callback {
                deadline: Some(Instant::now() + LOCATION_RECHECK),
                callback: Box::new(move || {
                    if let Some(shared) = owner.upgrade() {
                        shared.recheck_pane_locations();
                    }
                }),
            });
        }
        changed
    }

    fn recheck_pane_locations(self: &Arc<Self>) {
        let start = {
            let mut inner = self.inner.lock();
            inner.locations.recheck = false;
            let mut sources = Vec::new();
            let mut start = false;
            for pane in std::mem::take(&mut inner.locations.unread) {
                if let Some(source) = location_source(&inner, pane)
                    && !sources.contains(&source)
                {
                    sources.push(source.clone());
                    start |= queue_source(&mut inner, pane, source);
                }
            }
            start
        };
        if start {
            self.start_location_worker();
        }
    }

    pub(super) fn finish_pane_location(
        self: &Arc<Self>,
        pane: PaneId,
        source: LocationSource,
        (cwd, repo, branch): (String, String, String),
    ) {
        let (changed, start) = {
            let mut inner = self.inner.lock();
            if inner.locations.jobs.remove(&pane) == Some(true) {
                (false, queue_location(&mut inner, pane))
            } else {
                remember_location(&mut inner.locations.known, source, &cwd, &repo, &branch);
                let mut changed = share_branch(&mut inner, &cwd, &repo, &branch);
                changed |= inner.engine.state.update_pane_summary(pane, |summary| {
                    summary.cwd = cwd;
                    summary.repo = repo;
                    summary.branch = branch;
                });
                (changed, false)
            }
        };
        if changed {
            self.request_publish(timers::PublishReason::Tree);
        }
        if start {
            self.start_location_worker();
        }
    }

    pub(super) fn watch_pane_program(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        foreground: u32,
        shell: Option<u32>,
        command: &str,
        settle: bool,
    ) {
        if let Some(age) = process_info::running_for(foreground).filter(|age| *age < PROGRAM_SETTLE)
        {
            if settle {
                self.settle_pane_program(pane, terminal, PROGRAM_SETTLE.saturating_sub(age));
            }
            return;
        }
        let start = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            let locations = &mut inner.locations;
            locations
                .programs
                .insert(pane, (foreground, shell, command.to_owned()));
            queue_program(locations, pane)
        };
        if start {
            self.start_location_worker();
        }
    }

    fn settle_pane_program(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        wait: Duration,
    ) {
        let due = Instant::now() + wait;
        {
            let settling = &mut self.inner.lock().locations.program_settling;
            if settling
                .get(&pane)
                .is_some_and(|pending| *pending > Instant::now())
            {
                return;
            }
            settling.insert(pane, due);
        }
        let owner = Arc::downgrade(&self.server_owner());
        let terminal = Arc::downgrade(terminal);
        let _ = self.timer_tx.send(timers::TimerInput::Callback {
            deadline: Some(due),
            callback: Box::new(move || {
                if let (Some(shared), Some(terminal)) = (owner.upgrade(), terminal.upgrade()) {
                    shared.resight_pane_program(pane, &terminal);
                }
            }),
        });
    }

    fn resight_pane_program(self: &Arc<Self>, pane: PaneId, terminal: &Arc<TerminalSession>) {
        let foreground = terminal_foreground_process(terminal);
        let command = foreground
            .and_then(process_info::command_name)
            .unwrap_or_default();
        let sighting = {
            let mut inner = self.inner.lock();
            inner.locations.program_settling.remove(&pane);
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            sight_program(&mut inner, pane, foreground, &command, Instant::now())
        };
        match sighting {
            ProgramSighting::Unchanged => {}
            ProgramSighting::Cleared => self.request_publish(timers::PublishReason::Tree),
            ProgramSighting::Changed(foreground) => {
                self.watch_pane_program(
                    pane,
                    terminal,
                    foreground,
                    terminal.process_id(),
                    &command,
                    false,
                );
            }
        }
    }

    pub(super) fn finish_pane_program(
        self: &Arc<Self>,
        pane: PaneId,
        source: &ProgramSource,
        process: Vec<String>,
    ) {
        let (changed, start) = {
            let mut inner = self.inner.lock();
            let locations = &mut inner.locations;
            let current = locations.programs.get(&pane) == Some(source);
            let start = locations.program_jobs.remove(&pane) == Some(true)
                && queue_program(locations, pane);
            let changed = current
                && inner
                    .engine
                    .state
                    .update_pane_summary(pane, |summary| summary.process = process);
            (changed, start)
        };
        if changed {
            self.request_publish(timers::PublishReason::Tree);
        }
        if start {
            self.start_location_worker();
        }
    }

    fn start_location_worker(self: &Arc<Self>) {
        let owner = Arc::downgrade(&self.server_owner());
        if let Err(error) = thread::Builder::new()
            .name("zz-location".to_owned())
            .spawn(move || locate_panes(&owner))
        {
            let mut inner = self.inner.lock();
            let Locations {
                jobs,
                queue,
                running,
                programs,
                program_jobs,
                program_queue,
                ..
            } = &mut inner.locations;
            for (pane, ..) in queue.drain(..) {
                jobs.remove(&pane);
            }
            for (pane, ..) in program_queue.drain(..) {
                program_jobs.remove(&pane);
                programs.remove(&pane);
            }
            *running = false;
            log::debug!("could not start the pane location worker: {error}");
        }
    }
}

#[cfg(test)]
impl Shared {
    pub(super) fn settle_pane_locations(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while {
            let locations = &self.inner.lock().locations;
            !locations.jobs.is_empty() || !locations.program_jobs.is_empty()
        } {
            assert!(Instant::now() < deadline, "pane locations never settled");
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn locate_panes(owner: &Weak<Shared>) {
    loop {
        let job = {
            let Some(shared) = owner.upgrade() else {
                return;
            };
            let mut inner = shared.inner.lock();
            let locations = &mut inner.locations;
            let job = locations
                .queue
                .pop_front()
                .map(|(pane, source)| Job::Location(pane, source))
                .or_else(|| {
                    locations
                        .program_queue
                        .pop_front()
                        .map(|(pane, source)| Job::Program(pane, source))
                });
            locations.running = job.is_some();
            job
        };
        match job {
            None => return,
            Some(Job::Location(pane, source)) => {
                let location = resolve_location(&source.0, &source.1, daemon_home());
                let Some(shared) = owner.upgrade() else {
                    return;
                };
                shared.finish_pane_location(pane, source, location);
            }
            Some(Job::Program(pane, source)) => {
                let process = pane_process(&source);
                let Some(shared) = owner.upgrade() else {
                    return;
                };
                shared.finish_pane_program(pane, &source, process);
            }
        }
    }
}

pub(super) fn forget_pane_program(inner: &mut ServerState, pane: PaneId) {
    inner.locations.programs.remove(&pane);
    inner.locations.program_settling.remove(&pane);
    inner.locations.program_checked.remove(&pane);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgramSighting {
    Unchanged,
    Cleared,
    Changed(u32),
}

pub(super) fn sight_program(
    inner: &mut ServerState,
    pane: PaneId,
    foreground: Option<u32>,
    command: &str,
    now: Instant,
) -> ProgramSighting {
    let running = foreground.filter(|_| {
        cfg!(windows) || (!command.is_empty() && !process_info::is_shell_name(command))
    });
    let Some(foreground) = running else {
        inner.locations.programs.remove(&pane);
        let shown = inner
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| !pane.summary.process.is_empty());
        return if shown
            && inner
                .engine
                .state
                .update_pane_summary(pane, |summary| summary.process.clear())
        {
            ProgramSighting::Cleared
        } else {
            ProgramSighting::Unchanged
        };
    };
    if inner
        .locations
        .program_settling
        .get(&pane)
        .is_some_and(|due| *due > now)
    {
        return ProgramSighting::Unchanged;
    }
    let known = inner
        .locations
        .programs
        .get(&pane)
        .is_some_and(|(seen, _, name)| *seen == foreground && name == command);
    if !cfg!(windows) {
        return if known {
            ProgramSighting::Unchanged
        } else {
            ProgramSighting::Changed(foreground)
        };
    }
    let checked = &mut inner.locations.program_checked;
    if checked
        .get(&pane)
        .is_some_and(|at| now.saturating_duration_since(*at) < PROGRAM_RECHECK)
    {
        return ProgramSighting::Unchanged;
    }
    checked.insert(pane, now);
    ProgramSighting::Changed(foreground)
}

fn queue_program(locations: &mut Locations, pane: PaneId) -> bool {
    let Some(source) = locations.programs.get(&pane).cloned() else {
        return false;
    };
    match locations.program_jobs.entry(pane) {
        Entry::Occupied(mut job) => {
            job.insert(true);
            false
        }
        Entry::Vacant(job) => {
            job.insert(false);
            locations.program_queue.push_back((pane, source));
            !std::mem::replace(&mut locations.running, true)
        }
    }
}

fn pane_process((foreground, shell, _): &ProgramSource) -> Vec<String> {
    let mut process = process_info::program_candidates(*foreground, *shell);
    process.retain(|entry| entry.len() <= MAX_PANE_PROCESS_BYTES);
    process.truncate(MAX_PANE_PROCESS_ENTRIES);
    process
}

fn place_pane(inner: &mut ServerState, pane: PaneId) -> (bool, bool, bool) {
    let Some(source) = location_source(inner, pane) else {
        return (false, false, false);
    };
    let mut fresh = false;
    let mut changed = false;
    if let Some(known) = inner
        .locations
        .known
        .iter()
        .find(|known| known.source == source)
    {
        fresh = known.read_at.elapsed() < LOCATION_FRESH;
        changed = inner.engine.state.update_pane_summary(pane, |summary| {
            summary.cwd.clone_from(&known.cwd);
            summary.repo.clone_from(&known.repo);
            summary.branch.clone_from(&known.branch);
        });
    }
    let locations = &mut inner.locations;
    if fresh && !locations.jobs.contains_key(&pane) {
        locations.unread.insert(pane);
        return (
            changed,
            false,
            !std::mem::replace(&mut locations.recheck, true),
        );
    }
    (changed, queue_source(inner, pane, source), false)
}

fn queue_location(inner: &mut ServerState, pane: PaneId) -> bool {
    location_source(inner, pane).is_some_and(|source| queue_source(inner, pane, source))
}

fn queue_source(inner: &mut ServerState, pane: PaneId, source: LocationSource) -> bool {
    let locations = &mut inner.locations;
    match locations.jobs.entry(pane) {
        Entry::Occupied(mut job) => {
            job.insert(true);
            false
        }
        Entry::Vacant(job) => {
            job.insert(false);
            locations.queue.push_back((pane, source));
            !std::mem::replace(&mut locations.running, true)
        }
    }
}

fn location_source(inner: &ServerState, pane: PaneId) -> Option<LocationSource> {
    match &inner.engine.state.pane(pane)?.kind {
        PaneKind::Terminal => Some(
            inner
                .engine
                .pane_runtime_facts(pane)
                .map(|facts| {
                    if facts.reported_path.is_empty() {
                        (String::new(), PathBuf::from(&facts.current_path))
                    } else {
                        (facts.reported_path.clone(), PathBuf::new())
                    }
                })
                .unwrap_or_default(),
        ),
        PaneKind::Agent(agent) => Some((String::new(), agent.cwd.clone().unwrap_or_default())),
        _ => None,
    }
}

fn remember_location(
    known: &mut VecDeque<KnownLocation>,
    source: LocationSource,
    cwd: &str,
    repo: &str,
    branch: &str,
) {
    let read_at = Instant::now();
    known.retain(|known| known.source != source);
    for alias in known
        .iter_mut()
        .filter(|known| !cwd.is_empty() && known.cwd == cwd)
    {
        repo.clone_into(&mut alias.repo);
        branch.clone_into(&mut alias.branch);
        alias.read_at = read_at;
    }
    known.push_front(KnownLocation {
        source,
        cwd: cwd.to_owned(),
        repo: repo.to_owned(),
        branch: branch.to_owned(),
        read_at,
    });
    known.truncate(KNOWN_LOCATIONS);
}

fn share_branch(inner: &mut ServerState, cwd: &str, repo: &str, branch: &str) -> bool {
    if cwd.is_empty() {
        return false;
    }
    let panes = inner
        .engine
        .state
        .windows
        .values()
        .flat_map(|window| &window.panes)
        .filter(|(_, pane)| {
            pane.summary.cwd == cwd && (pane.summary.repo != repo || pane.summary.branch != branch)
        })
        .map(|(&pane, _)| pane)
        .collect::<Vec<_>>();
    let mut changed = false;
    for pane in panes {
        changed |= inner.engine.state.update_pane_summary(pane, |summary| {
            repo.clone_into(&mut summary.repo);
            branch.clone_into(&mut summary.branch);
        });
    }
    changed
}

fn resolve_location(reported: &str, live: &Path, homes: &[PathBuf]) -> (String, String, String) {
    let dir = if reported.is_empty() {
        Some(live.to_path_buf()).filter(|live| live.is_absolute() && live.is_dir())
    } else {
        path_listing::osc7_directory(reported)
    };
    let Some(dir) = dir else {
        return Default::default();
    };
    let (repo, mut branch) = find_repo(&dir).map_or_else(Default::default, |(root, branch)| {
        (display_path(root, homes), branch.unwrap_or_default())
    });
    branch.truncate(branch.floor_char_boundary(MAX_PANE_BRANCH_BYTES));
    (display_path(&dir, homes), repo, branch)
}

fn daemon_home() -> &'static [PathBuf] {
    static HOME: LazyLock<Vec<PathBuf>> =
        LazyLock::new(|| home_directory().map(home_forms).unwrap_or_default());
    &HOME
}

fn home_forms(home: PathBuf) -> Vec<PathBuf> {
    let resolved = fs::canonicalize(&home)
        .ok()
        .filter(|resolved| *resolved != home);
    std::iter::once(home).chain(resolved).collect()
}

pub(super) fn display_path(dir: &Path, homes: &[PathBuf]) -> String {
    let text = match homes.iter().find_map(|home| dir.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => Path::new("~").join(rest).to_string_lossy().into_owned(),
        None => dir.to_string_lossy().into_owned(),
    };
    if text.len() > MAX_PANE_CWD_BYTES {
        String::new()
    } else {
        text
    }
}

pub(super) fn find_repo(dir: &Path) -> Option<(&Path, Option<String>)> {
    for level in dir.ancestors() {
        let dot = level.join(".git");
        let git_dir = match fs::metadata(&dot) {
            Ok(metadata) if metadata.is_dir() => dot,
            Ok(metadata) if metadata.is_file() => {
                let link = read_capped(&dot)?;
                level.join(link.strip_prefix("gitdir:")?.trim())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            _ => return None,
        };
        let branch = read_capped(&git_dir.join("HEAD")).and_then(|head| head_branch(&head));
        return Some((level, branch));
    }
    None
}

fn read_capped(path: &Path) -> Option<String> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn head_branch(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return (!branch.is_empty() && reference != "refs/heads/.invalid")
            .then(|| branch.to_owned());
    }
    (head.len() >= 7 && head.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| head[..7].to_owned())
}

#[cfg(test)]
#[path = "pane_summary_tests.rs"]
mod tests;
