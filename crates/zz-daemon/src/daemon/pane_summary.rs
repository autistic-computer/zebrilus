use super::*;
use std::collections::btree_map::Entry;
use zz_protocol::{MAX_PANE_BRANCH_BYTES, MAX_PANE_CWD_BYTES, MAX_PANE_NAME_BYTES, ShellCommand};
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

pub(super) fn apply_pane_status(
    inner: &mut ServerState,
    pane: PaneId,
    status: Option<PaneStatus>,
    now: u64,
) -> bool {
    let Some(previous) = inner
        .engine
        .state
        .pane(pane)
        .map(|pane| status_edge(pane.status.as_ref()))
    else {
        return false;
    };
    let next = status_edge(status.as_ref());
    let changed = inner.engine.state.set_pane_status(pane, status);
    if previous == next {
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
        ) => Some(!pane_window_watched(inner, pane)),
        (previous, Some(PaneStatusState::Done)) if previous != Some(PaneStatusState::Done) => {
            Some(!pane_window_watched(inner, pane))
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

impl Shared {
    pub(super) fn request_pane_location(self: &Arc<Self>, pane: PaneId) {
        let Some((reported, live)) = claim_location_job(&mut self.inner.lock(), pane) else {
            return;
        };
        let owner = Arc::downgrade(&self.server_owner());
        if let Err(error) = self.helpers.submit(helpers::Task::PaneLocation {
            owner,
            pane,
            reported,
            live,
        }) {
            self.inner.lock().location_jobs.remove(&pane);
            log::debug!("could not resolve the location of {pane}: {error}");
        }
    }

    pub(super) fn finish_pane_location(
        self: &Arc<Self>,
        pane: PaneId,
        cwd: String,
        branch: String,
    ) {
        let (changed, again) = {
            let mut inner = self.inner.lock();
            let again = inner.location_jobs.remove(&pane).unwrap_or_default();
            let changed = inner.engine.state.update_pane_summary(pane, |summary| {
                summary.cwd = cwd;
                summary.branch = branch;
            });
            (changed, again)
        };
        if changed {
            self.request_publish(timers::PublishReason::Tree);
        }
        if again {
            self.request_pane_location(pane);
        }
    }
}

pub(super) fn claim_location_job(
    inner: &mut ServerState,
    pane: PaneId,
) -> Option<(String, PathBuf)> {
    let source = match &inner.engine.state.pane(pane)?.kind {
        PaneKind::Terminal => inner
            .engine
            .pane_runtime_facts(pane)
            .map(|facts| {
                (
                    facts.reported_path.clone(),
                    PathBuf::from(&facts.current_path),
                )
            })
            .unwrap_or_default(),
        PaneKind::Agent(agent) => (String::new(), agent.cwd.clone().unwrap_or_default()),
        _ => return None,
    };
    match inner.location_jobs.entry(pane) {
        Entry::Occupied(mut job) => {
            job.insert(true);
            None
        }
        Entry::Vacant(job) => {
            job.insert(false);
            Some(source)
        }
    }
}

pub(super) fn resolve_location(
    reported: &str,
    live: &Path,
    home: Option<&Path>,
) -> (String, String) {
    let dir = if reported.is_empty() {
        Some(live.to_path_buf()).filter(|live| live.is_absolute() && live.is_dir())
    } else {
        path_listing::osc7_directory(reported)
    };
    let Some(dir) = dir else {
        return (String::new(), String::new());
    };
    let mut branch = git_branch(&dir).unwrap_or_default();
    branch.truncate(branch.floor_char_boundary(MAX_PANE_BRANCH_BYTES));
    (display_path(&dir, home), branch)
}

pub(super) fn daemon_home() -> Option<&'static Path> {
    static HOME: LazyLock<Option<PathBuf>> = LazyLock::new(home_directory);
    HOME.as_deref()
}

pub(super) fn display_path(dir: &Path, home: Option<&Path>) -> String {
    let text = match home.and_then(|home| dir.strip_prefix(home).ok()) {
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

pub(super) fn git_branch(dir: &Path) -> Option<String> {
    let mut git_dir = None;
    for level in dir.ancestors() {
        let dot = level.join(".git");
        match fs::metadata(&dot) {
            Ok(metadata) if metadata.is_dir() => git_dir = Some(dot),
            Ok(metadata) if metadata.is_file() => {
                let link = read_capped(&dot)?;
                git_dir = Some(level.join(link.strip_prefix("gitdir:")?.trim()));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            _ => return None,
        }
        break;
    }
    head_branch(&read_capped(&git_dir?.join("HEAD"))?)
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
        return (!branch.is_empty()).then(|| branch.to_owned());
    }
    (head.len() >= 7 && head.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| head[..7].to_owned())
}

#[cfg(test)]
#[path = "pane_summary_tests.rs"]
mod tests;
