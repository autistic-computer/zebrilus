use std::{collections::BTreeSet, fmt};

use zz_protocol::{
    CommandInvocation, PaneKindSnapshot, PaneSnapshot, SessionId, SessionSnapshot, WindowId,
    WindowSnapshot,
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
pub fn pane_label(pane: &PaneSnapshot) -> String {
    let title = pane.title.trim();
    if !(title.is_empty() || title == "agent" && matches!(pane.kind, PaneKindSnapshot::Agent(_))) {
        return title.to_owned();
    }
    match &pane.kind {
        PaneKindSnapshot::Picker => "new pane".to_owned(),
        PaneKindSnapshot::Terminal => "terminal".to_owned(),
        PaneKindSnapshot::Browser(browser) if !browser.url().trim().is_empty() => {
            browser.url().trim().to_owned()
        }
        PaneKindSnapshot::Browser(_) => "browser".to_owned(),
        PaneKindSnapshot::Agent(_) => "New session".to_owned(),
        PaneKindSnapshot::Editor(_) => "editor".to_owned(),
    }
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

fn session_place(session: &SessionSnapshot) -> Option<&str> {
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

fn pane_place(pane: &PaneSnapshot) -> Option<&str> {
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
