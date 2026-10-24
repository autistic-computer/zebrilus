//! The lead chat: the conversation with the lead agent, read from its pane's
//! lens stream on whichever host runs it. The lead is the first terminal
//! running Claude Code or Codex in a session named `lead`; zz follows it from
//! every window, attached to that session or not.

use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use zz_client::{
    agent_transcript::{AgentToolKindModel, ToolPayload},
    program::Program,
    window_summary::{pane_agent, pane_program},
};
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use zz_protocol::{AgentProvider, PaneId, PaneKindSnapshot};
use zz_ui::{
    ActiveTheme as _, Ground,
    agent::{
        chat::{ChatBubble, ChatSender, ChatTyping, Receipt, TYPING_STEPS},
        presentation::{permission_card, permission_option},
    },
    button::Button,
};

use super::controller::{
    AgentController, AgentPaneState, AgentPermissionKind, AgentPermissionRequest, AgentThreadEntry,
};
use crate::mux::{client::MuxClient, hosts::HostId};

/// The session zz looks in for the lead.
pub(crate) const LEAD_SESSION: &str = "lead";
/// Entries that arrive this soon after zz starts following a lead are its
/// history being replayed, not messages arriving, so they get no time.
const REPLAY_SETTLE: Duration = Duration::from_secs(3);
const AGENT_NOTICE: &str = "[zz] Agent ";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Lead {
    pub(crate) host: HostId,
    pub(crate) pane: PaneId,
    pub(crate) provider: AgentProvider,
    pub(crate) cwd: PathBuf,
    pub(crate) host_name: SharedString,
    pub(crate) program: Option<&'static Program>,
}

/// The lead: the first terminal running Claude Code or Codex in a session
/// named [`LEAD_SESSION`], on the first host in fleet order that has one.
pub(crate) fn find_lead(mux: &MuxClient, local_name: &SharedString) -> Option<Lead> {
    mux.fleet_hosts().find_map(|(host, name, _, snapshot)| {
        let session = snapshot?
            .sessions
            .iter()
            .find(|session| session.name == LEAD_SESSION)?;
        session
            .windows
            .iter()
            .flat_map(|window| window.panes.values())
            .filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal))
            .find_map(|pane| {
                Some(Lead {
                    host,
                    pane: pane.id,
                    provider: pane_agent(pane)?,
                    cwd: Some(PathBuf::from(&pane.summary.cwd))
                        .filter(|cwd| cwd.is_absolute())
                        .unwrap_or_else(|| PathBuf::from("/")),
                    host_name: if host == HostId::LOCAL {
                        local_name.clone()
                    } else {
                        SharedString::from(name.to_owned())
                    },
                    program: pane_program(pane),
                })
            })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LeadMessageKind {
    Mine(Receipt),
    Lead,
    /// A notice zz typed into the lead about one of its agents.
    Agent(PaneId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeadMessage {
    pub(crate) id: u64,
    pub(crate) kind: LeadMessageKind,
    pub(crate) text: String,
}

/// The chat in a lead's transcript: your turns, the lead's last answer of each
/// finished turn, and zz's notices about agents, which zz types into the
/// lead's pane as `[zz] Agent %N …`. A turn of yours reads as seen once the
/// lead has done anything after it. Tool calls, thoughts and plans are left
/// to the agents' own rows in the sidebar.
pub(crate) fn lead_messages(entries: &[AgentThreadEntry], turn_live: bool) -> Vec<LeadMessage> {
    let mut messages = Vec::new();
    let mut answer: Option<(u64, &str)> = None;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            AgentThreadEntry::User { id, markdown, .. } => {
                if let Some((id, text)) = answer.take() {
                    messages.push(LeadMessage {
                        id,
                        kind: LeadMessageKind::Lead,
                        text: text.to_owned(),
                    });
                }
                if let Some((pane, text)) = agent_notice(markdown) {
                    messages.push(LeadMessage {
                        id: *id,
                        kind: LeadMessageKind::Agent(pane),
                        text,
                    });
                    continue;
                }
                let answered = entries[index + 1..]
                    .iter()
                    .any(|later| !matches!(later, AgentThreadEntry::User { .. }));
                messages.push(LeadMessage {
                    id: *id,
                    kind: LeadMessageKind::Mine(if answered {
                        Receipt::Read
                    } else {
                        Receipt::Delivered
                    }),
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
        messages.push(LeadMessage {
            id,
            kind: LeadMessageKind::Lead,
            text: text.to_owned(),
        });
    }
    messages
}

/// What the lead is doing in its live turn, one short line per step, oldest
/// first: the item its plan has in progress, then each step since the last
/// message it got, in the words its CLI gives the step where it gives any
/// (a Bash call's description, an agent's task, a reasoning summary's title)
/// and from the tool and what it touches otherwise.
pub(crate) fn lead_steps(entries: &[AgentThreadEntry]) -> Vec<String> {
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

/// The agent and the words of a notice zz typed into the lead, or `None`
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

pub(crate) struct LeadChat {
    mux: Entity<MuxClient>,
    controller: Entity<AgentController>,
    local_name: SharedString,
    lead: Option<Lead>,
    following_since: Instant,
    times: HashMap<u64, Option<SharedString>>,
    read: usize,
    error: Option<Arc<str>>,
}

impl LeadChat {
    pub(crate) fn new(
        mux: Entity<MuxClient>,
        local_name: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        let controller = cx.new(|_| AgentController::for_lead());
        controller.update(cx, |controller, _| controller.attach_mux(mux.clone()));
        cx.observe(&mux, |chat, _, cx| chat.synchronize(cx))
            .detach();
        cx.observe(&controller, |chat, _, cx| {
            chat.stamp(cx);
            cx.notify();
        })
        .detach();
        let mut chat = Self {
            mux,
            controller,
            local_name,
            lead: None,
            following_since: Instant::now(),
            times: HashMap::new(),
            read: 0,
            error: None,
        };
        chat.synchronize(cx);
        chat
    }

    fn synchronize(&mut self, cx: &mut Context<Self>) {
        let lead = find_lead(self.mux.read(cx), &self.local_name);
        if lead != self.lead {
            let pane = lead.as_ref().map(|lead| lead.pane);
            let same_pane = pane.is_some()
                && self.lead.as_ref().map(|lead| (lead.host, lead.pane))
                    == lead.as_ref().map(|lead| (lead.host, lead.pane));
            if !same_pane {
                self.times.clear();
                self.read = 0;
                self.error = None;
                self.following_since = Instant::now();
            }
            self.controller.update(cx, |controller, cx| {
                controller.retain_panes(&pane.into_iter().collect::<BTreeSet<_>>(), cx);
                if let Some(lead) = &lead {
                    controller.ensure_pane(lead.pane, lead.provider, lead.cwd.clone());
                }
            });
            self.lead = lead;
            cx.notify();
        }
        let followed = self.lead.as_ref().map(|lead| (lead.host, lead.pane));
        let events = self.mux.update(cx, |mux, _| {
            mux.follow_lead(followed);
            mux.has_lead_events().then(|| mux.take_lead_events())
        });
        if let Some(events) = events {
            self.controller.update(cx, |controller, cx| {
                for (pane, items) in events.items {
                    controller.apply_stream_items(pane, items, cx);
                }
                for (pane, state) in events.states {
                    controller.apply_pane_state(pane, &state, cx);
                }
            });
        }
    }

    fn stamp(&mut self, cx: &App) {
        let Some(pane) = self.lead.as_ref().map(|lead| lead.pane) else {
            return;
        };
        let Some((entries, _, _)) = self.controller.read(cx).pane_entries(pane) else {
            return;
        };
        let replaying = self.following_since.elapsed() < REPLAY_SETTLE;
        for entry in entries {
            self.times.entry(entry.id()).or_insert_with(|| {
                (!replaying).then(|| chrono::Local::now().format("%-H:%M").to_string().into())
            });
        }
    }

    pub(crate) fn lead(&self) -> Option<&Lead> {
        self.lead.as_ref()
    }

    fn state(&self, cx: &App) -> Option<AgentPaneState> {
        self.controller
            .read(cx)
            .pane_state(self.lead.as_ref()?.pane)
    }

    /// Whether the lead is in a turn, which the composer's button queues
    /// behind.
    pub(crate) fn running(&self, cx: &App) -> bool {
        self.state(cx)
            .is_some_and(|state| state.connection.has_active_turn())
    }

    pub(crate) fn messages(&self, cx: &App) -> Vec<LeadMessage> {
        let Some(lead) = &self.lead else {
            return Vec::new();
        };
        self.controller
            .read(cx)
            .pane_entries(lead.pane)
            .map(|(entries, _, _)| lead_messages(entries, self.running(cx)))
            .unwrap_or_default()
    }

    /// The lead's steps in its live turn, or `None` while it is not working.
    pub(crate) fn steps(&self, cx: &App) -> Option<Vec<String>> {
        let lead = self.lead.as_ref()?;
        if !self.running(cx) {
            return None;
        }
        Some(
            self.controller
                .read(cx)
                .pane_entries(lead.pane)
                .map(|(entries, _, _)| lead_steps(entries))
                .unwrap_or_default(),
        )
    }

    /// The thread as a surface draws it: the bubbles, then the lead typing
    /// with its latest steps while it works. The count grows with every
    /// bubble and step, and when an answer takes the typing bubble's place,
    /// so a surface can follow the bottom as it grows.
    pub(crate) fn thread(
        &self,
        scope: &'static str,
        ground: Ground,
        cx: &App,
    ) -> (Vec<AnyElement>, usize) {
        let mut bubbles = self.bubbles(scope, ground, cx);
        let mut depth = bubbles.len() * (TYPING_STEPS + 2);
        if let Some(steps) = self.steps(cx) {
            depth += 1 + steps.len().min(TYPING_STEPS);
            bubbles.push(
                ChatTyping::new((scope, usize::MAX))
                    .steps(steps)
                    .on(ground)
                    .into_any_element(),
            );
        }
        (bubbles, depth)
    }

    /// The color of the dot on the chat's title-bar button: an approval waiting
    /// on you, or messages you have not seen.
    pub(crate) fn attention(&self, cx: &App) -> Option<Hsla> {
        if self
            .state(cx)
            .is_some_and(|state| !state.pending_permissions.is_empty())
        {
            return Some(cx.theme().warning);
        }
        (self.messages(cx).len() > self.read).then_some(cx.theme().accent)
    }

    /// Count everything up to now as seen. Called by the surfaces that show
    /// the chat; it does not notify, so a render can call it.
    pub(crate) fn mark_read(&mut self, cx: &App) {
        self.read = self.messages(cx).len();
    }

    pub(crate) fn error(&self) -> Option<&Arc<str>> {
        self.error.as_ref()
    }

    /// Type `text` into the lead's pane as your turn.
    pub(crate) fn send(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.lead.as_ref().map(|lead| lead.pane) else {
            return false;
        };
        let sent = self.controller.update(cx, |controller, cx| {
            controller.prompt(pane, text, Vec::new(), cx)
        });
        self.error = sent.err();
        cx.notify();
        self.error.is_none()
    }

    /// Pick option `index` of the oldest approval waiting on you, the way a
    /// digit does in the lens. Whether there was one to answer.
    pub(crate) fn answer_waiting(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some((request_id, option_id)) = self.state(cx).and_then(|state| {
            let request = state.pending_permissions.first()?;
            Some((request.request_id, request.options.get(index)?.id.clone()))
        }) else {
            return false;
        };
        self.respond(request_id, Some(option_id), cx);
        true
    }

    fn respond(&mut self, request_id: u64, option_id: Option<String>, cx: &mut Context<Self>) {
        let Some(pane) = self.lead.as_ref().map(|lead| lead.pane) else {
            return;
        };
        self.controller.update(cx, |controller, cx| {
            controller.respond_permission(pane, request_id, option_id, cx);
        });
    }

    /// Who an agent notice is about: its window's name on the lead's host and
    /// the program its tile shows.
    fn sender(&self, pane: PaneId, cx: &App) -> ChatSender {
        let found = self.lead.as_ref().and_then(|lead| {
            let mux = self.mux.read(cx);
            let snapshot = mux.host_snapshot(lead.host)?;
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find_map(|window| {
                    window
                        .panes
                        .get(&pane)
                        .map(|snapshot| (window.name.clone(), pane_program(snapshot)))
                })
        });
        match found {
            Some((name, program)) => ChatSender {
                name: name.into(),
                program,
            },
            None => ChatSender {
                name: pane.to_string().into(),
                program: None,
            },
        }
    }

    /// The lead's tile and name for the chat header.
    pub(crate) fn lead_sender(&self) -> Option<ChatSender> {
        self.lead.as_ref().map(|lead| ChatSender {
            name: "Lead".into(),
            program: lead.program,
        })
    }

    /// The bubbles, drawn on `ground`, with ids under `scope`.
    pub(crate) fn bubbles(&self, scope: &'static str, ground: Ground, cx: &App) -> Vec<AnyElement> {
        self.messages(cx)
            .into_iter()
            .map(|message| {
                let time = self
                    .times
                    .get(&message.id)
                    .cloned()
                    .flatten()
                    .unwrap_or_default();
                let id = (scope, usize::try_from(message.id).unwrap_or(usize::MAX));
                match message.kind {
                    LeadMessageKind::Mine(receipt) => ChatBubble::mine(id, message.text, time)
                        .receipt(receipt)
                        .on(ground)
                        .into_any_element(),
                    LeadMessageKind::Lead => ChatBubble::theirs(id, message.text, time)
                        .on(ground)
                        .into_any_element(),
                    LeadMessageKind::Agent(pane) => ChatBubble::theirs(id, message.text, time)
                        .sender(self.sender(pane, cx))
                        .on(ground)
                        .into_any_element(),
                }
            })
            .collect()
    }

    /// The oldest approval or question waiting on you, as the lens's
    /// permission card; picking an option answers it in the lead's pane.
    pub(crate) fn ask(chat: &Entity<Self>, scope: &'static str, cx: &App) -> Option<AnyElement> {
        let request: AgentPermissionRequest = chat
            .read(cx)
            .state(cx)?
            .pending_permissions
            .first()?
            .clone();
        let options = request
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let chat = chat.clone();
                let option_id = option.id.clone();
                let request_id = request.request_id;
                permission_option(
                    (SharedString::from(format!("{scope}-option")), index),
                    index,
                    option.name.clone(),
                    matches!(
                        option.kind,
                        AgentPermissionKind::RejectOnce | AgentPermissionKind::RejectAlways
                    ),
                    index == 0,
                    true,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    let option_id = option_id.clone();
                    chat.update(cx, |chat, cx| chat.respond(request_id, Some(option_id), cx));
                })
                .into_any_element()
            })
            .collect();
        let cancel = {
            let chat = chat.clone();
            let request_id = request.request_id;
            Button::new(SharedString::from(format!("{scope}-cancel")))
                .tooltip("Cancel request")
                .on_click(move |_, _, cx| {
                    chat.update(cx, |chat, cx| chat.respond(request_id, None, cx));
                })
        };
        Some(
            div()
                .pb(px(8.0))
                .child(permission_card(request.title, None, options, cancel, cx))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: u64, text: &str) -> AgentThreadEntry {
        AgentThreadEntry::User {
            id,
            markdown: text.to_owned(),
            images: Vec::new(),
            message_id: None,
        }
    }

    fn answer(id: u64, text: &str) -> AgentThreadEntry {
        AgentThreadEntry::Assistant {
            id,
            markdown: text.to_owned(),
            aside: None,
        }
    }

    #[test]
    fn a_turn_shows_only_the_leads_last_answer_once_it_ends() {
        let entries = [
            user(1, "status?"),
            answer(2, "Let me check the panes."),
            answer(3, "Three agents are working."),
            user(4, "thanks"),
        ];
        assert_eq!(
            lead_messages(&entries, false),
            vec![
                LeadMessage {
                    id: 1,
                    kind: LeadMessageKind::Mine(Receipt::Read),
                    text: "status?".into()
                },
                LeadMessage {
                    id: 3,
                    kind: LeadMessageKind::Lead,
                    text: "Three agents are working.".into()
                },
                LeadMessage {
                    id: 4,
                    kind: LeadMessageKind::Mine(Receipt::Delivered),
                    text: "thanks".into()
                },
            ]
        );
        let live = lead_messages(&entries[..3], true);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].kind, LeadMessageKind::Mine(Receipt::Read));
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
        let messages = lead_messages(&entries, false);
        assert_eq!(
            messages
                .iter()
                .map(|message| (message.kind.clone(), message.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (
                    LeadMessageKind::Agent(PaneId(5)),
                    "Sprite sheet is done at 32px."
                ),
                (
                    LeadMessageKind::Agent(PaneId(7)),
                    "Waiting for a permission answer."
                ),
                (LeadMessageKind::Agent(PaneId(9)), "Half done."),
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
    ) -> AgentThreadEntry {
        AgentThreadEntry::Tool {
            id,
            protocol_id: id.to_string(),
            kind,
            status: zz_client::agent_transcript::AgentToolStatusModel::Completed,
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
                "Read crates/zz/src/agent/lead.rs",
                Some("/home/me/zz/crates/zz/src/agent/lead.rs:40"),
                &serde_json::json!({"file_path": "/home/me/zz/crates/zz/src/agent/lead.rs"}),
            ),
            tool(
                7,
                AgentToolKindModel::Read,
                "Read crates/zz/src/agent/lead.rs",
                Some("/home/me/zz/crates/zz/src/agent/lead.rs"),
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
            lead_steps(&entries),
            [
                "Committing the changes",
                "Show dirty files",
                "Reading lead.rs",
                "Writing the message",
                "Searching for pinned",
                "Running git commit -m 'x'",
            ]
        );
        assert_eq!(lead_steps(&entries[..2]), ["Running zz ls"]);
        assert!(lead_steps(&entries[..3]).is_empty());
    }
}
