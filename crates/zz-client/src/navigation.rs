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
pub fn move_window_commands(
    windows: &[crate::StatusBarWindow],
    window: WindowId,
    to: usize,
) -> Vec<CommandInvocation> {
    let Some(from) = windows.iter().position(|entry| entry.id == window) else {
        return Vec::new();
    };
    if to >= windows.len() {
        return Vec::new();
    }
    let current = windows
        .iter()
        .find(|entry| entry.active)
        .map(|entry| entry.id);
    let passed: Vec<WindowId> = if from < to {
        windows[from + 1..=to]
            .iter()
            .map(|entry| entry.id)
            .collect()
    } else {
        windows[to..from]
            .iter()
            .rev()
            .map(|entry| entry.id)
            .collect()
    };
    passed
        .into_iter()
        .map(|neighbour| {
            let (select, source, target) = if current == Some(window) {
                (true, window, neighbour)
            } else if current == Some(neighbour) {
                (true, neighbour, window)
            } else {
                (false, window, neighbour)
            };
            let mut args = Vec::with_capacity(5);
            if select {
                args.push("-d".to_owned());
            }
            args.extend([
                "-s".to_owned(),
                source.to_string(),
                "-t".to_owned(),
                target.to_string(),
            ]);
            CommandInvocation::new("swap-window", args)
        })
        .collect()
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

    fn session(windows: &[(&str, u32)], current: &str) -> (MuxEngine, ExecutionContext) {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        let mut run = |args: &[&str]| {
            engine
                .execute(
                    &mut context,
                    &CommandInvocation::new(args[0], args[1..].iter().copied()),
                )
                .unwrap();
        };
        let (first, rest) = windows.split_first().unwrap();
        run(&["new-session", "-s", "s", "-n", first.0]);
        for (name, index) in rest {
            run(&["new-window", "-d", "-t", &format!("s:{index}"), "-n", name]);
        }
        run(&["select-window", "-t", &format!("s:{current}")]);
        (engine, context)
    }

    fn windows(engine: &MuxEngine, context: &ExecutionContext) -> Vec<crate::StatusBarWindow> {
        StatusBarModel::from_snapshot(&engine.state.snapshot(), context.session).windows
    }

    fn layout(engine: &MuxEngine, context: &ExecutionContext) -> Vec<String> {
        windows(engine, context)
            .iter()
            .map(|window| {
                let current = if window.active { "*" } else { "" };
                format!("{}:{}{current}", window.index, window.name)
            })
            .collect()
    }

    fn drag(
        engine: &mut MuxEngine,
        context: &mut ExecutionContext,
        name: &str,
        to: usize,
    ) -> Vec<String> {
        let shown = windows(engine, context);
        let window = shown.iter().find(|window| window.name == name).unwrap().id;
        let names = |id: &str| {
            shown
                .iter()
                .find(|window| window.id.to_string() == id)
                .map_or_else(|| id.to_owned(), |window| window.name.clone())
        };
        let commands = move_window_commands(&shown, window, to);
        let shown_commands = commands
            .iter()
            .map(|command| {
                let args = command
                    .args
                    .iter()
                    .map(|arg| names(arg.as_ref()))
                    .collect::<Vec<_>>();
                format!("{} {}", command.name, args.join(" "))
            })
            .collect();
        for command in &commands {
            engine.execute(context, command).unwrap();
        }
        shown_commands
    }

    #[test]
    fn dropping_a_window_on_another_takes_its_slot_and_keeps_indices_and_the_current_window() {
        let (mut engine, mut context) = session(&[("a", 0), ("b", 1), ("c", 2), ("d", 3)], "a");
        assert_eq!(
            drag(&mut engine, &mut context, "b", 3),
            ["swap-window -s b -t c", "swap-window -s b -t d"]
        );
        assert_eq!(layout(&engine, &context), ["0:a*", "1:c", "2:d", "3:b"]);
        assert_eq!(
            drag(&mut engine, &mut context, "b", 1),
            ["swap-window -s b -t d", "swap-window -s b -t c"]
        );
        assert_eq!(layout(&engine, &context), ["0:a*", "1:b", "2:c", "3:d"]);
        assert_eq!(
            drag(&mut engine, &mut context, "a", 3),
            [
                "swap-window -d -s a -t b",
                "swap-window -d -s a -t c",
                "swap-window -d -s a -t d"
            ]
        );
        assert_eq!(layout(&engine, &context), ["0:b", "1:c", "2:d", "3:a*"]);
        assert!(drag(&mut engine, &mut context, "a", 3).is_empty());

        let (mut engine, mut context) = session(&[("p", 0), ("q", 2), ("r", 5), ("s", 6)], "5");
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-option", ["-g", "renumber-windows", "on"]),
            )
            .unwrap();
        assert_eq!(
            drag(&mut engine, &mut context, "p", 2),
            ["swap-window -s p -t q", "swap-window -d -s r -t p"]
        );
        assert_eq!(layout(&engine, &context), ["0:q", "2:r*", "5:p", "6:s"]);
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
