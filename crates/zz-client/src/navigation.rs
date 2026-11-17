use std::{collections::BTreeSet, fmt};

use zz_protocol::{
    CommandInvocation, PaneId, PaneKindSnapshot, PaneSnapshot, SessionId, SessionSnapshot,
    WindowId, WindowSnapshot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    Session(SessionId),
    Window(WindowId),
}

pub fn rename_prompt_command(
    target: RenameTarget,
    current_name: &str,
) -> (&'static str, CommandInvocation) {
    let (label, prompt, template) = match target {
        RenameTarget::Session(session) => (
            "Rename Session…",
            "rename-session: ",
            format!("rename-session -t '{session}' -- '%%'"),
        ),
        RenameTarget::Window(window) => (
            "Rename Window…",
            "rename-window: ",
            format!("rename-window -t '{window}' -- '%%'"),
        ),
    };
    (
        label,
        CommandInvocation::new(
            "command-prompt",
            ["-p", prompt, "-I", current_name, &template],
        ),
    )
}

#[must_use]
pub fn move_window_command(
    windows: &[crate::StatusBarWindow],
    window: WindowId,
    to: usize,
) -> Option<CommandInvocation> {
    let from = windows.iter().position(|entry| entry.id == window)?;
    let target = windows.get(to).filter(|_| to != from)?;
    let mut args = vec![if from < to { "-a" } else { "-b" }.to_owned()];
    if !windows[from].active {
        args.push("-d".to_owned());
    }
    args.extend([
        "-s".to_owned(),
        window.to_string(),
        "-t".to_owned(),
        target.id.to_string(),
    ]);
    Some(CommandInvocation::new("move-window", args))
}

#[must_use]
pub fn tab_order(local: &[&str], incoming: &[String]) -> Option<Vec<usize>> {
    if local.len() != incoming.len() {
        return None;
    }
    let mut taken = vec![false; local.len()];
    incoming
        .iter()
        .map(|url| {
            let index = (0..local.len()).find(|&index| !taken[index] && local[index] == url)?;
            taken[index] = true;
            Some(index)
        })
        .collect()
}

#[must_use]
pub fn kill_session_prompt_command(session: SessionId, name: &str) -> CommandInvocation {
    CommandInvocation::new(
        "confirm-before",
        [
            "-p".to_owned(),
            format!("Kill session {name}? (y/n)"),
            format!("kill-session -t {session}"),
        ],
    )
}

#[must_use]
pub fn ordered_panes(window: &WindowSnapshot) -> Vec<&PaneSnapshot> {
    let mut layout_order = Vec::with_capacity(window.panes.len());
    window.layout.panes(&mut layout_order);
    let mut seen = BTreeSet::new();
    let mut panes = Vec::with_capacity(window.panes.len());
    for id in layout_order {
        if seen.insert(id)
            && let Some(pane) = window.panes.get(&id)
        {
            panes.push(pane);
        }
    }
    for (id, pane) in &window.panes {
        if seen.insert(*id) {
            panes.push(pane);
        }
    }
    panes
}

#[must_use]
pub fn pane_index(window: &WindowSnapshot, pane: PaneId) -> u32 {
    let position = window
        .pane_order
        .iter()
        .position(|id| *id == pane)
        .or_else(|| {
            ordered_panes(window)
                .iter()
                .position(|candidate| candidate.id == pane)
        })
        .unwrap_or_default();
    u32::try_from(position)
        .unwrap_or(u32::MAX)
        .saturating_add(window.pane_base_index)
}

#[must_use]
pub fn pane_label(pane: &PaneSnapshot) -> String {
    let title = pane.title.trim();
    let untitled_agent = matches!(pane.kind, PaneKindSnapshot::Agent(_)) && title == "agent";
    if !title.is_empty() && !untitled_agent {
        return title.to_owned();
    }
    match &pane.kind {
        PaneKindSnapshot::Picker => "new pane".to_owned(),
        PaneKindSnapshot::Terminal => "terminal".to_owned(),
        PaneKindSnapshot::Browser(browser) if !browser.url().trim().is_empty() => {
            browser.url().trim().to_owned()
        }
        PaneKindSnapshot::Browser(_) => "browser".to_owned(),
        PaneKindSnapshot::Editor(_) => "editor".to_owned(),
        PaneKindSnapshot::Agent(agent) => agent.provider.label().to_owned(),
        PaneKindSnapshot::View(view) => view.title.clone(),
        PaneKindSnapshot::Other { kind } => kind.clone(),
    }
}

#[must_use]
pub fn newer_pane_text(kind: &str) -> String {
    format!("A newer zz shows this {kind} pane")
}

#[must_use]
pub fn pane_folder(pane: &PaneSnapshot) -> Option<String> {
    let cwd = match &pane.kind {
        PaneKindSnapshot::Agent(agent) if agent.cwd.is_some() => {
            agent.cwd.as_deref()?.to_string_lossy().into_owned()
        }
        _ if !pane.summary.cwd.is_empty() => pane.summary.cwd.clone(),
        PaneKindSnapshot::Editor(editor) if !editor.cwd.is_empty() => editor.cwd.clone(),
        _ => return None,
    };
    Some(crate::providers::home_relative(&cwd))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLabel {
    pub text: String,
    pub index: Option<String>,
}

impl fmt::Display for SessionLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.index {
            Some(index) => write!(f, "{} {index}", self.text),
            None => f.write_str(&self.text),
        }
    }
}

#[must_use]
pub fn session_label(session: &SessionSnapshot) -> SessionLabel {
    let name = session.name.trim();
    let numbered = name.bytes().all(|byte| byte.is_ascii_digit());
    match numbered.then(|| session_place(session)).flatten() {
        Some(place) => SessionLabel {
            text: last_component(place).to_owned(),
            index: (!name.is_empty()).then(|| name.to_owned()),
        },
        None if name.is_empty() => SessionLabel {
            text: format!("session {}", session.id),
            index: None,
        },
        None => SessionLabel {
            text: session.name.clone(),
            index: None,
        },
    }
}

pub(crate) fn session_place(session: &SessionSnapshot) -> Option<&str> {
    let active = session
        .windows
        .iter()
        .find(|window| window.id == session.active_window)
        .and_then(|window| window.panes.get(&window.active_pane))
        .and_then(pane_place);
    let mut places: Vec<(&str, usize)> = Vec::new();
    for place in session
        .windows
        .iter()
        .flat_map(|window| window.panes.values())
        .filter_map(pane_place)
    {
        match places.iter_mut().find(|(seen, _)| *seen == place) {
            Some((_, count)) => *count += 1,
            None => places.push((place, 1)),
        }
    }
    places
        .into_iter()
        .rev()
        .max_by_key(|&(place, count)| (count, Some(place) == active))
        .map(|(place, _)| place)
}

pub(crate) fn pane_place(pane: &PaneSnapshot) -> Option<&str> {
    [&pane.summary.repo, &pane.summary.cwd]
        .into_iter()
        .map(String::as_str)
        .find(|place| !place.is_empty())
}

pub(crate) fn last_component(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|component| !component.is_empty())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use zz_mux::{ExecutionContext, MuxEngine};
    use zz_protocol::{AgentDescriptor, PaneId, PaneSummary};

    use super::*;
    use crate::StatusBarModel;

    #[test]
    fn dropping_a_window_on_another_takes_its_slot_and_keeps_the_current_window() {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        let run = |engine: &mut MuxEngine, context: &mut ExecutionContext, command| {
            engine.execute(context, &command).unwrap();
        };
        run(
            &mut engine,
            &mut context,
            CommandInvocation::new("new-session", ["-s", "s", "-n", "a"]),
        );
        for name in ["b", "c", "d"] {
            run(
                &mut engine,
                &mut context,
                CommandInvocation::new("new-window", ["-d", "-n", name]),
            );
        }
        let session = context.session;
        let windows = |engine: &MuxEngine| {
            StatusBarModel::from_snapshot(&engine.state.snapshot(), session).windows
        };
        let layout = |engine: &MuxEngine| {
            windows(engine)
                .iter()
                .map(|window| {
                    let current = if window.active { "*" } else { "" };
                    format!("{}:{}{current}", window.index, window.name)
                })
                .collect::<Vec<_>>()
        };
        let ids = ["a", "b", "c", "d"].map(|name| {
            windows(&engine)
                .iter()
                .find(|window| window.name == name)
                .unwrap()
                .id
        });
        let [a, b, c, d] = ids.map(|id| id.to_string());

        let right = move_window_command(&windows(&engine), ids[1], 3).unwrap();
        assert_eq!(
            right,
            CommandInvocation::new("move-window", ["-a", "-d", "-s", &b, "-t", &d])
        );
        run(&mut engine, &mut context, right);
        assert_eq!(layout(&engine), ["0:a*", "2:c", "3:d", "4:b"]);

        let left = move_window_command(&windows(&engine), ids[1], 1).unwrap();
        assert_eq!(
            left,
            CommandInvocation::new("move-window", ["-b", "-d", "-s", &b, "-t", &c])
        );
        run(&mut engine, &mut context, left);
        assert_eq!(layout(&engine), ["0:a*", "2:b", "3:c", "4:d"]);

        let current = move_window_command(&windows(&engine), ids[0], 3).unwrap();
        assert_eq!(
            current,
            CommandInvocation::new("move-window", ["-a", "-s", &a, "-t", &d])
        );
        run(&mut engine, &mut context, current);
        assert_eq!(layout(&engine), ["2:b", "3:c", "4:d", "5:a*"]);
        assert_eq!(move_window_command(&windows(&engine), ids[0], 3), None);
    }

    #[test]
    fn a_reordered_url_list_maps_each_slot_to_a_local_tab_once() {
        let incoming = ["c", "a", "b", "a"].map(String::from);
        assert_eq!(
            tab_order(&["a", "b", "a", "c"], &incoming),
            Some(vec![3, 0, 1, 2])
        );
        assert_eq!(tab_order(&["a", "b", "b", "c"], &incoming), None);
        assert_eq!(tab_order(&["a", "b", "a"], &incoming), None);
    }

    #[test]
    fn a_chat_in_a_former_terminal_pane_names_its_own_folder() {
        let pane = PaneSnapshot {
            id: PaneId(1),
            title: String::new(),
            kind: PaneKindSnapshot::Agent(AgentDescriptor {
                cwd: Some(PathBuf::from("/Users/me/dev/clairvo")),
                ..AgentDescriptor::default()
            }),
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode: None,
            status: None,
            summary: PaneSummary {
                cwd: "~/dev/autistic-computer/zebrilus".to_owned(),
                ..PaneSummary::default()
            },
        };
        assert_eq!(pane_folder(&pane).as_deref(), Some("~/dev/clairvo"));
    }
}
