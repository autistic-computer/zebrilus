use zz_protocol::{
    AgentProvider, PaneId,
    chat::{ChatView, ItemKind, ItemStatus},
};

use crate::{
    agent_settings::agent_program,
    chat_rows::{duration, live_line, step_label},
};

pub const PEEK_LINES: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCard {
    pub pane: PaneId,
    pub agent: AgentProvider,
    pub host: AgentProvider,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
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
    pub const fn host_name(&self) -> &'static str {
        agent_program(self.host)
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
    pub fn allowed(&self) -> usize {
        self.decisions
            .iter()
            .filter(|decision| decision.verdict == Verdict::Allowed)
            .count()
    }

    #[must_use]
    pub fn header(&self, now: u64) -> ChildHeader {
        let allowed = self.allowed();
        ChildHeader {
            name: self.name(),
            word: self.word(),
            allowed: (matches!(self.state, ChildState::Done { .. }) && allowed > 0)
                .then(|| format!("{allowed} allowed")),
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
            .map(|decision| decision.verdict)
    }

    #[must_use]
    pub fn peek(&self, chat: &ChatView) -> Vec<PeekLine> {
        if matches!(self.state, ChildState::Done { .. } | ChildState::Closed) {
            return Vec::new();
        }
        let Some(turn) = chat.turns.last() else {
            return Vec::new();
        };
        let tools = turn
            .items
            .iter()
            .filter_map(|id| chat.item(id))
            .filter(|item| item.kind.is_tool())
            .collect::<Vec<_>>();
        let running = tools.last().is_some_and(|item| item.status.is_open());
        let mut lines = tools
            .into_iter()
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
            .collect::<Vec<_>>();
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
            ChildState::Working if !running && turn.is_open() => {
                if let Some(live) = live_line(chat, turn) {
                    lines.push(PeekLine {
                        mark: PeekMark::Live,
                        label: live,
                        detail: None,
                        code: false,
                        verdict: None,
                    });
                }
            }
            _ => {}
        }
        let skip = lines.len().saturating_sub(PEEK_LINES);
        lines.split_off(skip)
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, TurnBy, TurnId};

    use super::*;

    fn card(state: ChildState) -> ChildCard {
        ChildCard {
            pane: PaneId(4),
            agent: AgentProvider::Codex,
            host: AgentProvider::ClaudeCode,
            state,
            started_at: 10_000,
            ended_at: None,
            decisions: vec![Decision {
                verdict: Verdict::Allowed,
                title: "cargo test -p zz-daemon".to_owned(),
            }],
        }
    }

    fn parts(header: ChildHeader) -> (&'static str, &'static str, Option<String>, Option<String>) {
        (header.name, header.word, header.allowed, header.time)
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
            card.peek(&chat)
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
