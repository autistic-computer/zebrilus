use super::*;
use zz_protocol::{MAX_PANE_NAME_BYTES, ShellCommand};
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

#[cfg(test)]
#[path = "pane_summary_tests.rs"]
mod tests;
