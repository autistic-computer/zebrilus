//! The chat with the Driver Agent as every client shows it: which pane is the
//! driver, and which of its transcript's entries read as messages and steps.

use zz_protocol::{AgentProvider, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot};

use crate::{
    agent_transcript::{AgentThreadEntry, AgentToolKindModel, ToolPayload},
    driver_log::{AGENT_NOTICE, PACKET_OPENING},
    window_summary::pane_agent,
};

/// The session clients look in for the driver.
pub const DRIVER_SESSION: &str = "driver";
/// What Claude Code writes as your turn when you stop it.
const INTERRUPTED: &str = "[Request interrupted";

/// The driver on one host: the first terminal running Claude Code or Codex in
/// a session named [`DRIVER_SESSION`].
pub fn driver_pane(snapshot: &MuxSnapshot) -> Option<(&PaneSnapshot, AgentProvider)> {
    snapshot
        .sessions
        .iter()
        .find(|session| session.name == DRIVER_SESSION)?
        .windows
        .iter()
        .flat_map(|window| window.panes.values())
        .filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal))
        .find_map(|pane| Some((pane, pane_agent(pane)?)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverMessageKind {
    /// Yours; `read` once the driver has done anything after it.
    Mine {
        read: bool,
    },
    Driver,
    /// A notice zz typed into the driver about one of its agents.
    Agent(PaneId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverMessage {
    pub id: u64,
    pub kind: DriverMessageKind,
    pub text: String,
}

/// The chat in a driver's transcript: your turns, the driver's last answer of each
/// finished turn, and zz's notices about agents, which zz types into the
/// driver's pane as `[zz] Agent %N …`. Tool calls, thoughts and plans are left
/// to the agents' own rows.
pub fn driver_messages<I>(entries: &[AgentThreadEntry<I>], turn_live: bool) -> Vec<DriverMessage> {
    let mut messages = Vec::new();
    let mut answer: Option<(u64, &str)> = None;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            AgentThreadEntry::User { id, markdown, .. } => {
                if let Some((id, text)) = answer.take() {
                    messages.push(DriverMessage {
                        id,
                        kind: DriverMessageKind::Driver,
                        text: text.to_owned(),
                    });
                }
                if markdown.trim().starts_with(PACKET_OPENING)
                    || markdown.trim().starts_with(INTERRUPTED)
                {
                    continue;
                }
                if let Some((pane, text)) = agent_notice(markdown) {
                    messages.push(DriverMessage {
                        id: *id,
                        kind: DriverMessageKind::Agent(pane),
                        text,
                    });
                    continue;
                }
                let read = entries[index + 1..]
                    .iter()
                    .any(|later| !matches!(later, AgentThreadEntry::User { .. }));
                messages.push(DriverMessage {
                    id: *id,
                    kind: DriverMessageKind::Mine { read },
                    text: markdown.trim().to_owned(),
                });
            }
            AgentThreadEntry::Assistant {
                id,
                markdown,
                aside,
                ..
            } if aside.is_none() && !markdown.trim().is_empty() => {
                answer = Some((*id, markdown.trim()));
            }
            _ => {}
        }
    }
    if let Some((id, text)) = answer.filter(|_| !turn_live) {
        messages.push(DriverMessage {
            id,
            kind: DriverMessageKind::Driver,
            text: text.to_owned(),
        });
    }
    messages
}

/// What the driver is doing in its live turn, one short line per step, oldest
/// first: the item its plan has in progress, then each step since the last
/// message it got, in the words its CLI gives the step where it gives any
/// (a Bash call's description, an agent's task, a reasoning summary's title)
/// and from the tool and what it touches otherwise.
pub fn driver_steps<I>(entries: &[AgentThreadEntry<I>]) -> Vec<String> {
    let start = entries
        .iter()
        .rposition(|entry| matches!(entry, AgentThreadEntry::User { .. }))
        .map_or(0, |index| index + 1);
    let plan = entries.iter().rev().find_map(|entry| match entry {
        AgentThreadEntry::Plan { markdown, .. } => markdown
            .lines()
            .find_map(|line| line.trim().strip_prefix("- [~]"))
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned),
        _ => None,
    });
    let mut steps = Vec::<String>::new();
    let later = entries[start..].iter().filter_map(|entry| match entry {
        AgentThreadEntry::Tool {
            kind,
            label,
            location,
            input,
            ..
        } => Some(tool_step(*kind, label, location.as_deref(), input.as_ref())),
        AgentThreadEntry::Reasoning { markdown, .. } => reasoning_title(markdown),
        _ => None,
    });
    for step in plan.into_iter().chain(later) {
        if !step.is_empty() && steps.last() != Some(&step) {
            steps.push(step);
        }
    }
    steps
}

fn tool_step(
    kind: AgentToolKindModel,
    label: &str,
    location: Option<&str>,
    input: Option<&ToolPayload>,
) -> String {
    let input = match input {
        Some(ToolPayload::Json(json)) => serde_json::from_str::<serde_json::Value>(json).ok(),
        _ => None,
    };
    let field = |name: &str| {
        input
            .as_ref()
            .and_then(|input| input[name].as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    if let Some(description) = field("description") {
        return single_line(&description);
    }
    let label = single_line(label);
    let file = location
        .map(|location| {
            location
                .rsplit_once(':')
                .filter(|(_, line)| line.chars().all(|digit| digit.is_ascii_digit()))
                .map_or(location, |(path, _)| path)
                .to_owned()
        })
        .or_else(|| field("file_path"))
        .or_else(|| field("path"))
        .map(|path| {
            std::path::Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| path.clone(), str::to_owned)
        });
    let touched = file.unwrap_or_else(|| label.clone());
    match kind {
        AgentToolKindModel::Read => format!("Reading {touched}"),
        AgentToolKindModel::Edit => format!("Editing {touched}"),
        AgentToolKindModel::Delete => format!("Deleting {touched}"),
        AgentToolKindModel::Move => format!("Moving {touched}"),
        AgentToolKindModel::Search => field("pattern").or_else(|| field("query")).map_or_else(
            || "Searching".to_owned(),
            |pattern| format!("Searching for {pattern}"),
        ),
        AgentToolKindModel::Execute => format!("Running {label}"),
        AgentToolKindModel::SwitchMode => "Planning".to_owned(),
        AgentToolKindModel::Fetch | AgentToolKindModel::Think | AgentToolKindModel::Other => label,
    }
}

/// The title a reasoning summary opens with, `**Checking the tests**`, the
/// way Codex shows it while it works.
fn reasoning_title(markdown: &str) -> Option<String> {
    let rest = markdown.trim_start().strip_prefix("**")?;
    let (title, _) = rest.split_once("**")?;
    Some(single_line(title)).filter(|title| !title.is_empty())
}

fn single_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

/// The agent and the words of a notice zz typed into the driver, or `None`
/// for anything else. `finished` and `stopped` notices carry the agent's
/// reply after a blank line; the rest are one sentence about the agent.
fn agent_notice(text: &str) -> Option<(PaneId, String)> {
    let rest = text.trim().strip_prefix(AGENT_NOTICE)?;
    let (pane, rest) = rest.split_once(' ')?;
    let pane = PaneId(pane.strip_prefix('%')?.parse().ok()?);
    let body = if rest.starts_with("finished:") || rest.starts_with("stopped (") {
        rest.split_once("\n\n")
            .map_or_else(|| rest.to_owned(), |(_, reply)| reply.trim().to_owned())
    } else {
        let sentence = rest.strip_prefix("is ").unwrap_or(rest);
        let mut characters = sentence.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(characters).collect()
        })
    };
    Some((pane, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_transcript::AgentToolStatusModel;

    type Entry = AgentThreadEntry<()>;

    fn user(id: u64, text: &str) -> Entry {
        AgentThreadEntry::User {
            id,
            markdown: text.to_owned(),
            images: Vec::new(),
            message_id: None,
        }
    }

    fn answer(id: u64, text: &str) -> Entry {
        AgentThreadEntry::Assistant {
            id,
            markdown: text.to_owned(),
            aside: None,
        }
    }

    #[test]
    fn a_turn_shows_only_the_drivers_last_answer_once_it_ends() {
        let entries = [
            user(1, "status?"),
            answer(2, "Let me check the panes."),
            answer(3, "Three agents are working."),
            user(4, "thanks"),
        ];
        assert_eq!(
            driver_messages(&entries, false),
            vec![
                DriverMessage {
                    id: 1,
                    kind: DriverMessageKind::Mine { read: true },
                    text: "status?".into()
                },
                DriverMessage {
                    id: 3,
                    kind: DriverMessageKind::Driver,
                    text: "Three agents are working.".into()
                },
                DriverMessage {
                    id: 4,
                    kind: DriverMessageKind::Mine { read: false },
                    text: "thanks".into()
                },
            ]
        );
        let live = driver_messages(&entries[..3], true);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].kind, DriverMessageKind::Mine { read: true });
    }

    #[test]
    fn stopping_the_driver_leaves_no_bubble_of_its_own() {
        let entries = [
            user(1, "status?"),
            user(2, "[Request interrupted by user]"),
            user(3, "status of xps?"),
        ];
        assert_eq!(
            driver_messages(&entries, false)
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>(),
            ["status?", "status of xps?"]
        );
    }

    #[test]
    fn zz_notices_become_the_agents_messages() {
        let entries = [
            user(
                1,
                "[zz] Agent %5 finished:\n\nSprite sheet is done at 32px.",
            ),
            user(2, "[zz] Agent %7 is waiting for a permission answer."),
            user(3, "[zz] Agent %9 stopped (max turns):\n\nHalf done."),
        ];
        let messages = driver_messages(&entries, false);
        assert_eq!(
            messages
                .iter()
                .map(|message| (message.kind.clone(), message.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (
                    DriverMessageKind::Agent(PaneId(5)),
                    "Sprite sheet is done at 32px."
                ),
                (
                    DriverMessageKind::Agent(PaneId(7)),
                    "Waiting for a permission answer."
                ),
                (DriverMessageKind::Agent(PaneId(9)), "Half done."),
            ]
        );
        assert_eq!(agent_notice("[zz] Agentless"), None);
        assert_eq!(agent_notice("hello"), None);
    }

    fn tool(
        id: u64,
        kind: AgentToolKindModel,
        label: &str,
        location: Option<&str>,
        input: &serde_json::Value,
    ) -> Entry {
        AgentThreadEntry::Tool {
            id,
            protocol_id: id.to_string(),
            kind,
            status: AgentToolStatusModel::Completed,
            label: label.to_owned(),
            location: location.map(str::to_owned),
            input: Some(ToolPayload::Json(input.to_string())),
            output: Vec::new(),
            default_expanded: false,
            exit_code: None,
        }
    }

    #[test]
    fn steps_take_the_clis_own_words_and_start_over_with_each_message() {
        let entries = [
            user(1, "status?"),
            tool(
                2,
                AgentToolKindModel::Execute,
                "zz ls",
                None,
                &serde_json::json!({}),
            ),
            user(3, "commit this"),
            AgentThreadEntry::Plan {
                id: 4,
                markdown: "- [x] Look at the diff\n- [~] Committing the changes".to_owned(),
            },
            tool(
                5,
                AgentToolKindModel::Execute,
                "git status --short",
                None,
                &serde_json::json!({"command": "git status --short", "description": "Show dirty files"}),
            ),
            tool(
                6,
                AgentToolKindModel::Read,
                "Read crates/zz/src/agent/driver.rs",
                Some("/home/me/zz/crates/zz/src/agent/driver.rs:40"),
                &serde_json::json!({"file_path": "/home/me/zz/crates/zz/src/agent/driver.rs"}),
            ),
            tool(
                7,
                AgentToolKindModel::Read,
                "Read crates/zz/src/agent/driver.rs",
                Some("/home/me/zz/crates/zz/src/agent/driver.rs"),
                &serde_json::json!({}),
            ),
            AgentThreadEntry::Reasoning {
                id: 8,
                label: "Thinking".to_owned(),
                markdown: "**Writing the message**\n\nOne line, then the why.".to_owned(),
                default_expanded: false,
            },
            tool(
                9,
                AgentToolKindModel::Search,
                "rg pinned",
                None,
                &serde_json::json!({"pattern": "pinned"}),
            ),
            tool(
                10,
                AgentToolKindModel::Execute,
                "git commit -m 'x'\nmore",
                None,
                &serde_json::json!({}),
            ),
        ];
        assert_eq!(
            driver_steps(&entries),
            [
                "Committing the changes",
                "Show dirty files",
                "Reading driver.rs",
                "Writing the message",
                "Searching for pinned",
                "Running git commit -m 'x'",
            ]
        );
        assert_eq!(driver_steps(&entries[..2]), ["Running zz ls"]);
        assert!(driver_steps(&entries[..3]).is_empty());
    }
}
