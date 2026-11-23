use zz_protocol::{
    AgentProvider, CommandInvocation, PaneId,
    agent_mention::{AgentAsk, AgentDecision, AskAnswers},
    chat::{Ask, ChatView, ItemId, ItemKind, ItemStatus, RequestView},
};

use crate::{
    agent_settings::agent_program,
    chat_rows::{duration, live_line, step_label},
};

pub const PEEK_LINES: usize = 3;

#[must_use]
pub fn reopen_command(host: PaneId, child: PaneId) -> CommandInvocation {
    CommandInvocation::new(
        "select-child-chat",
        ["-t".to_owned(), host.to_string(), child.to_string()],
    )
}

#[must_use]
pub fn current_pane(
    original: PaneId,
    remembered: Option<PaneId>,
    alive: impl Fn(PaneId) -> bool,
) -> Option<PaneId> {
    std::iter::once(original)
        .chain(remembered)
        .find(|pane| alive(*pane))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCard {
    pub anchor: ItemId,
    pub pane: PaneId,
    pub agent: AgentProvider,
    pub latest: bool,
    pub state: ChildState,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub decisions: Vec<Decision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChildState {
    Working,
    Asking(ChildAsk),
    Done { reply: String },
    Failed { error: String },
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildAsk {
    pub kind: ItemKind,
    pub title: String,
}

impl ChildAsk {
    fn pending(chat: &ChatView, request: &RequestView) -> Self {
        let title = match &request.ask {
            Ask::Approval { title, .. } => title.clone(),
            Ask::Questions { questions } => questions
                .first()
                .map(|question| {
                    question
                        .header
                        .clone()
                        .filter(|header| !header.is_empty())
                        .unwrap_or_else(|| question.question.clone())
                })
                .unwrap_or_default(),
            Ask::Plan { .. } => "Plan".to_owned(),
        };
        let kind = match &request.ask {
            Ask::Questions { .. } => ItemKind::Tool,
            Ask::Approval { .. } | Ask::Plan { .. } => request
                .item
                .as_ref()
                .and_then(|id| chat.item(id))
                .map_or(ItemKind::Tool, |item| item.kind),
        };
        Self { kind, title }
    }

    #[must_use]
    pub fn of(ask: &AgentAsk) -> Self {
        let (word, rest) = ask.what.split_once(' ').unwrap_or((&ask.what, ""));
        let kind = match word {
            "read" => Some(ItemKind::Read),
            "edit" | "delete" | "move" => Some(ItemKind::Edit),
            "search" => Some(ItemKind::Search),
            "execute" => Some(ItemKind::Execute),
            "fetch" => Some(ItemKind::Fetch),
            "think" | "switch_mode" | "other" => Some(ItemKind::Tool),
            _ => None,
        };
        match kind {
            Some(kind) if !rest.trim().is_empty() => Self {
                kind,
                title: rest.trim().to_owned(),
            },
            _ => Self {
                kind: ItemKind::Tool,
                title: ask.what.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Denied,
    Picked(String),
    Answered,
}

impl Verdict {
    #[must_use]
    pub fn of(decision: &AgentDecision, ask: Option<&AgentAsk>) -> Self {
        match decision {
            AgentDecision::Allow => Self::Allowed,
            AgentDecision::Deny => Self::Denied,
            AgentDecision::Option(id) => Self::Picked(
                ask.and_then(|ask| match &ask.answers {
                    AskAnswers::Options(options) => options
                        .iter()
                        .find(|option| option.id == *id && !option.label.is_empty())
                        .map(|option| option.label.clone()),
                    AskAnswers::AllowDeny | AskAnswers::FreeText => None,
                })
                .unwrap_or_else(|| id.clone()),
            ),
            AgentDecision::Answer(_) => Self::Answered,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub kind: ItemKind,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildHeader {
    pub name: &'static str,
    pub word: &'static str,
    pub allowed: Option<String>,
    pub time: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekMark {
    Step(ItemKind),
    Running,
    Failed(ItemKind),
    Declined(ItemKind),
    Live,
    Asking(ItemKind),
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeekLine {
    pub mark: PeekMark,
    pub label: String,
    pub detail: Option<String>,
    pub code: bool,
    pub verdict: Option<Verdict>,
}

impl ChildCard {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        agent_program(self.agent)
    }

    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self.state {
            ChildState::Working => "working",
            ChildState::Asking(_) => "asking",
            ChildState::Done { .. } => "done",
            ChildState::Failed { .. } => "failed",
            ChildState::Closed => "closed",
        }
    }

    #[must_use]
    pub fn header(&self, now: u64) -> ChildHeader {
        let count = |kept: fn(&Verdict) -> bool| {
            self.decisions
                .iter()
                .filter(|decision| kept(&decision.verdict))
                .count()
        };
        let counts = [
            (count(|verdict| *verdict == Verdict::Allowed), "allowed"),
            (count(|verdict| *verdict == Verdict::Denied), "denied"),
            (
                count(|verdict| matches!(verdict, Verdict::Picked(_) | Verdict::Answered)),
                "answered",
            ),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, word)| format!("{count} {word}"))
        .collect::<Vec<_>>();
        ChildHeader {
            name: self.name(),
            word: self.word(),
            allowed: (matches!(self.state, ChildState::Done { .. }) && !counts.is_empty())
                .then(|| counts.join(" · ")),
            time: (self.state != ChildState::Closed)
                .then(|| duration(self.ended_at.unwrap_or(now).saturating_sub(self.started_at))),
        }
    }

    #[must_use]
    pub fn verdict(&self, title: &str) -> Option<Verdict> {
        self.decisions
            .iter()
            .rev()
            .find(|decision| decision.title == title)
            .map(|decision| decision.verdict.clone())
    }

    #[must_use]
    pub fn corrected(&self, chat: Option<&ChatView>, alive: Option<bool>) -> Self {
        let mut card = self.clone();
        if !self.latest || !matches!(self.state, ChildState::Working | ChildState::Asking(_)) {
            return card;
        }
        if alive == Some(false) {
            card.state = ChildState::Closed;
            return card;
        }
        let Some(chat) = chat else {
            return card;
        };
        match chat.open_requests().next() {
            None if matches!(self.state, ChildState::Asking(_)) => {
                card.state = ChildState::Working;
            }
            Some(request) if self.state == ChildState::Working => {
                card.state = ChildState::Asking(ChildAsk::pending(chat, request));
            }
            _ => {}
        }
        card
    }

    #[must_use]
    pub fn peek(&self, chat: Option<&ChatView>) -> Vec<PeekLine> {
        if matches!(self.state, ChildState::Done { .. } | ChildState::Closed) {
            return Vec::new();
        }
        let turn = chat
            .filter(|_| self.latest)
            .and_then(|chat| Some((chat, chat.turns.last()?)));
        let mut lines = turn.map_or_else(Vec::new, |(chat, turn)| self.steps(chat, turn));
        let running = turn.is_some_and(|(chat, turn)| {
            turn.items
                .iter()
                .filter_map(|id| chat.item(id))
                .rfind(|item| item.kind.is_tool())
                .is_some_and(|item| item.status.is_open())
        });
        match &self.state {
            ChildState::Asking(ask) => {
                if lines.last().is_some_and(|line| line.label == ask.title) {
                    lines.pop();
                }
                lines.push(PeekLine {
                    mark: PeekMark::Asking(ask.kind),
                    label: ask.title.clone(),
                    detail: None,
                    code: ask.kind == ItemKind::Execute,
                    verdict: None,
                });
            }
            ChildState::Failed { error } => lines.push(PeekLine {
                mark: PeekMark::Error,
                label: error.clone(),
                detail: None,
                code: false,
                verdict: None,
            }),
            ChildState::Working => {
                if let Some(decision) = self
                    .decisions
                    .last()
                    .filter(|decision| !lines.iter().any(|line| line.label == decision.title))
                {
                    lines.push(PeekLine {
                        mark: PeekMark::Step(decision.kind),
                        label: decision.title.clone(),
                        detail: None,
                        code: decision.kind == ItemKind::Execute,
                        verdict: Some(decision.verdict.clone()),
                    });
                }
                if let Some(live) = turn
                    .filter(|(_, turn)| !running && turn.is_open())
                    .and_then(|(chat, turn)| live_line(chat, turn))
                {
                    lines.push(PeekLine {
                        mark: PeekMark::Live,
                        label: live,
                        detail: None,
                        code: false,
                        verdict: None,
                    });
                }
            }
            ChildState::Done { .. } | ChildState::Closed => {}
        }
        let skip = lines.len().saturating_sub(PEEK_LINES);
        lines.split_off(skip)
    }

    fn steps(&self, chat: &ChatView, turn: &zz_protocol::chat::TurnView) -> Vec<PeekLine> {
        turn.items
            .iter()
            .filter_map(|id| chat.item(id))
            .filter(|item| item.kind.is_tool())
            .map(|item| {
                let step = step_label(chat, item);
                let mark = match &item.status {
                    ItemStatus::Running => PeekMark::Running,
                    ItemStatus::Failed { .. } => PeekMark::Failed(item.kind),
                    ItemStatus::Declined => PeekMark::Declined(item.kind),
                    _ => PeekMark::Step(item.kind),
                };
                PeekLine {
                    mark,
                    verdict: self.verdict(&step.label),
                    code: item.kind == ItemKind::Execute,
                    label: step.label,
                    detail: step.detail,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, TurnBy, TurnId};

    use super::*;

    fn card(state: ChildState) -> ChildCard {
        ChildCard {
            anchor: ItemId::new("open"),
            pane: PaneId(4),
            agent: AgentProvider::Codex,
            latest: true,
            state,
            started_at: 10_000,
            ended_at: None,
            decisions: vec![Decision {
                verdict: Verdict::Allowed,
                kind: ItemKind::Execute,
                title: "cargo test -p zz-daemon".to_owned(),
            }],
        }
    }

    fn parts(header: ChildHeader) -> (&'static str, &'static str, Option<String>, Option<String>) {
        (header.name, header.word, header.allowed, header.time)
    }

    #[test]
    fn a_card_reopens_its_child_wherever_the_child_is_now() {
        let command = reopen_command(PaneId(0), PaneId(2));
        assert_eq!(
            std::iter::once(command.name.as_str())
                .chain(command.args.iter().map(AsRef::as_ref))
                .collect::<Vec<_>>(),
            ["select-child-chat", "-t", "%0", "%2"]
        );
        let live = [PaneId(2), PaneId(7)];
        let current = |original, remembered, live: &[PaneId]| {
            current_pane(original, remembered, |pane| live.contains(&pane))
        };
        assert_eq!(current(PaneId(2), Some(PaneId(7)), &live), Some(PaneId(2)));
        assert_eq!(
            current(PaneId(2), Some(PaneId(7)), &live[1..]),
            Some(PaneId(7))
        );
        assert_eq!(current(PaneId(2), Some(PaneId(9)), &live[1..]), None);
        assert_eq!(current(PaneId(2), None, &[]), None);
        let done = ChildCard {
            ended_at: Some(161_000),
            ..card(ChildState::Done {
                reply: "Yes.".to_owned(),
            })
        }
        .corrected(None, Some(false));
        assert_eq!(
            (done.word(), done.header(999_000).time),
            ("done", Some("2m 31s".to_owned()))
        );
        assert_eq!(
            card(ChildState::Working)
                .corrected(None, Some(false))
                .word(),
            "closed"
        );
    }

    #[test]
    fn the_header_names_the_agent_its_state_and_its_time() {
        let ask = ChildState::Asking(ChildAsk {
            kind: ItemKind::Execute,
            title: "cargo test -p zz-daemon".to_owned(),
        });
        assert_eq!(
            parts(card(ask).header(75_000)),
            ("codex", "asking", None, Some("1m 5s".to_owned()))
        );
        let done = ChildCard {
            ended_at: Some(161_000),
            ..card(ChildState::Done {
                reply: "Yes.".to_owned(),
            })
        };
        assert_eq!(
            parts(done.header(999_000)),
            (
                "codex",
                "done",
                Some("1 allowed".to_owned()),
                Some("2m 31s".to_owned())
            )
        );
        assert_eq!(
            parts(card(ChildState::Closed).header(68_000)),
            ("codex", "closed", None, None)
        );
        let asked = AgentAsk::parse(
            &AgentAsk {
                pane: PaneId(4),
                provider: AgentProvider::Codex,
                what: "Which color?".to_owned(),
                answers: AskAnswers::Options(vec![zz_protocol::agent_mention::AskOption {
                    id: "answer-1".to_owned(),
                    label: "Dark red".to_owned(),
                }]),
                request: Some(7),
            }
            .to_string(),
        );
        assert_eq!(
            Verdict::of(
                &AgentDecision::Option("answer-1".to_owned()),
                asked.as_ref()
            ),
            Verdict::Picked("Dark red".to_owned())
        );
    }

    #[test]
    fn the_peek_keeps_the_last_three_steps_and_ends_on_the_ask() {
        let mut chat = ChatView::default();
        let at = 1000;
        let mut apply = |event: ChatEvent| {
            chat.apply(&event).expect("valid event");
        };
        apply(ChatEvent::SessionReady {
            session: "s".to_owned(),
        });
        apply(ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::User { prompt: 1 },
            at,
        });
        for (id, kind, target) in [
            ("t1", ItemKind::Read, "lib.rs"),
            ("t2", ItemKind::Read, "turn.rs"),
            ("t3", ItemKind::Search, "ack_turn"),
            ("t4", ItemKind::Execute, "cargo test -p zz-daemon"),
        ] {
            apply(ChatEvent::ItemStarted {
                item: ItemId::new(id),
                turn: Some(TurnId(1)),
                parent: None,
                kind,
                body: ItemBody {
                    target: Some(target.to_owned()),
                    ..ItemBody::default()
                },
                at,
            });
            if id != "t4" {
                apply(ChatEvent::ItemEnded {
                    item: ItemId::new(id),
                    status: ItemStatus::Completed,
                    at,
                });
            }
        }
        let shown = |card: &ChildCard| {
            card.peek(Some(&chat))
                .into_iter()
                .map(|line| (line.mark, line.label, line.verdict))
                .collect::<Vec<_>>()
        };
        let ask = card(ChildState::Asking(ChildAsk {
            kind: ItemKind::Execute,
            title: "cargo test -p zz-daemon".to_owned(),
        }));
        assert_eq!(
            shown(&ask),
            [
                (PeekMark::Step(ItemKind::Read), "turn.rs".to_owned(), None),
                (
                    PeekMark::Step(ItemKind::Search),
                    "ack_turn".to_owned(),
                    None
                ),
                (
                    PeekMark::Asking(ItemKind::Execute),
                    "cargo test -p zz-daemon".to_owned(),
                    None
                ),
            ]
        );
        assert_eq!(
            shown(&card(ChildState::Working)).last(),
            Some(&(
                PeekMark::Running,
                "cargo test -p zz-daemon".to_owned(),
                Some(Verdict::Allowed)
            ))
        );
    }
}
