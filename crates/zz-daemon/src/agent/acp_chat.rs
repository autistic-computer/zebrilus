use std::time::Instant;

use serde_json::Value;
use zz_protocol::agent_stream::{AgentPromptOutcome, AgentStreamPayload};
use zz_protocol::chat::ChatEvent;

use super::chat_bridge::{Shadow, chat_answer, chat_answers, turn_mark};
use super::runtime::RuntimeControl;
use super::turn::{End, Fact};

const PROMPT_ECHO: &str = "zz-prompt-";
const UNNAMED: &str = "zz-run-";
const USER: &str = "user_message_chunk";
const CHUNK: &str = "_chunk";
const DRAWN: [&str; 6] = [
    USER,
    "agent_message_chunk",
    "agent_thought_chunk",
    "tool_call",
    "tool_call_update",
    "plan",
];

#[derive(Default)]
pub(crate) struct Transcript {
    shadow: Shadow,
    live: bool,
    turn: Option<u64>,
    drawn: String,
    runs: u64,
}

impl Transcript {
    pub(crate) fn payload(&mut self, payload: &AgentStreamPayload) -> Vec<ChatEvent> {
        let now = Instant::now();
        match payload {
            AgentStreamPayload::SessionReset { .. } => {
                if self.live {
                    *self = Self::default();
                    return vec![ChatEvent::Restart];
                }
                self.turn = None;
                self.drawn.clear();
                self.shadow.payload(payload);
            }
            AgentStreamPayload::SessionReady { session_id, .. }
            | AgentStreamPayload::SessionSwitched { session_id, .. } => {
                self.live = true;
                self.shadow.turn(&Fact::SessionReady, Some(session_id), now);
            }
            AgentStreamPayload::Update { update } => {
                let kind = update["sessionUpdate"].as_str().unwrap_or_default();
                if !DRAWN.contains(&kind) {
                    self.shadow.payload(payload);
                    return self.shadow.drain();
                }
                let user = kind == USER;
                let next = self.drawn != kind;
                if next {
                    self.runs += 1;
                    kind.clone_into(&mut self.drawn);
                }
                if self.live {
                    if let Some(turn) = prompt_echo(update).filter(|_| user) {
                        self.start(turn, now);
                    }
                } else if user && next {
                    self.shadow.payload(&AgentStreamPayload::Update {
                        update: turn_mark("started", None),
                    });
                }
                if kind.ends_with(CHUNK) && update["messageId"].is_null() {
                    let mut update = update.clone();
                    update["messageId"] = Value::from(format!("{UNNAMED}{}", self.runs));
                    self.shadow.payload(&AgentStreamPayload::Update { update });
                } else {
                    self.shadow.payload(payload);
                }
            }
            AgentStreamPayload::PromptAccepted { turn_id } => self.start(*turn_id, now),
            AgentStreamPayload::PromptFinished { turn_id, outcome } => {
                self.start(*turn_id, now);
                self.turn = None;
                let end = match outcome {
                    AgentPromptOutcome::Finished { stop_reason } if stop_reason == "cancelled" => {
                        End::Cancelled
                    }
                    AgentPromptOutcome::Finished { .. } => End::Completed,
                    AgentPromptOutcome::Failed { message } => End::Failed(message.clone()),
                };
                self.shadow.turn(
                    &Fact::Ended {
                        command: turn_id.to_string(),
                        end,
                    },
                    None,
                    now,
                );
            }
            AgentStreamPayload::PermissionRequested { .. }
            | AgentStreamPayload::PermissionResolved { .. }
            | AgentStreamPayload::TasksChanged { .. } => self.shadow.payload(payload),
            _ => {}
        }
        self.shadow.drain()
    }

    pub(crate) fn control(&mut self, control: &RuntimeControl) -> Vec<ChatEvent> {
        match control {
            RuntimeControl::RespondPermission {
                request_id,
                option_id,
            } => self
                .shadow
                .answered(*request_id, chat_answer(option_id.as_deref())),
            RuntimeControl::AnswerQuestion {
                request_id,
                answers,
            } => self.shadow.answered(*request_id, chat_answers(answers)),
            RuntimeControl::Cancel { .. } => {}
        }
        self.shadow.drain()
    }

    pub(crate) fn exited(&mut self, message: &str) -> Vec<ChatEvent> {
        self.shadow.turn(
            &Fact::Exited {
                message: message.to_owned(),
            },
            None,
            Instant::now(),
        );
        self.shadow.drain()
    }

    fn start(&mut self, turn: u64, now: Instant) {
        if self.turn == Some(turn) {
            return;
        }
        self.turn = Some(turn);
        let command = turn.to_string();
        self.shadow.turn(
            &Fact::Prompt {
                turn,
                command: Some(command.clone()),
            },
            None,
            now,
        );
        self.shadow.turn(&Fact::Started { command }, None, now);
    }
}

fn prompt_echo(update: &Value) -> Option<u64> {
    let (turn, _) = update["messageId"]
        .as_str()?
        .strip_prefix(PROMPT_ECHO)?
        .split_once('-')?;
    turn.parse().ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zz_protocol::chat::{ChatView, ItemKind};

    use super::*;

    fn update(update: Value) -> AgentStreamPayload {
        AgentStreamPayload::Update { update }
    }

    fn text(kind: &str, id: &str, text: &str) -> AgentStreamPayload {
        update(json!({
            "sessionUpdate": kind,
            "messageId": id,
            "content": { "type": "text", "text": text },
        }))
    }

    fn unnamed(kind: &str, text: &str) -> AgentStreamPayload {
        update(json!({
            "sessionUpdate": kind,
            "content": { "type": "text", "text": text },
        }))
    }

    fn kinds(view: &ChatView, turn: usize) -> Vec<(ItemKind, String)> {
        view.turns[turn]
            .items
            .iter()
            .map(|id| {
                let item = &view.items[id];
                (item.kind, item.body.text.clone())
            })
            .collect()
    }

    #[test]
    fn a_loaded_session_and_a_live_turn_reach_the_chat_as_turns() {
        let mut transcript = Transcript::default();
        let mut view = ChatView::default();
        let payloads = [
            AgentStreamPayload::SessionReset { restoring: true },
            text("user_message_chunk", "old-1", "first question"),
            text("agent_message_chunk", "old-2", "first answer"),
            text("user_message_chunk", "old-3", "second question"),
            text("agent_message_chunk", "old-4", "second answer"),
            AgentStreamPayload::SessionReady {
                session_id: "s1".to_owned(),
                modes: None,
                config_options: None,
            },
            text(
                "user_message_chunk",
                "zz-prompt-7-00000000000000aa",
                "third question",
            ),
            AgentStreamPayload::PromptAccepted { turn_id: 7 },
            unnamed("agent_message_chunk", "third "),
            update(
                json!({ "sessionUpdate": "available_commands_update", "availableCommands": [] }),
            ),
            unnamed("agent_message_chunk", "answer"),
            update(json!({
                "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "read_file",
                "kind": "read", "status": "completed",
            })),
            unnamed("agent_message_chunk", "after the tool"),
            AgentStreamPayload::PromptFinished {
                turn_id: 7,
                outcome: AgentPromptOutcome::Finished {
                    stop_reason: json!("end_turn"),
                },
            },
        ];
        for payload in &payloads {
            for event in transcript.payload(payload) {
                view.apply(&event)
                    .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
            }
        }
        assert_eq!(view.turns.len(), 3);
        assert_eq!(
            kinds(&view, 0),
            [
                (ItemKind::User, "first question".to_owned()),
                (ItemKind::Agent, "first answer".to_owned()),
            ]
        );
        assert_eq!(
            kinds(&view, 2),
            [
                (ItemKind::User, "third question".to_owned()),
                (ItemKind::Agent, "third answer".to_owned()),
                (ItemKind::Read, String::new()),
                (ItemKind::Agent, "after the tool".to_owned()),
            ]
        );
        assert!(view.running().is_none());
    }

    #[test]
    fn a_session_loaded_again_shows_its_history_once() {
        let ready = || AgentStreamPayload::SessionReady {
            session_id: "s1".to_owned(),
            modes: None,
            config_options: None,
        };
        let history = |turns: &[(&str, &str)]| {
            let mut payloads = vec![AgentStreamPayload::SessionReset { restoring: true }];
            for (index, (question, answer)) in turns.iter().enumerate() {
                payloads.push(text(
                    "user_message_chunk",
                    &format!("old-{index}"),
                    question,
                ));
                payloads.push(unnamed("agent_message_chunk", answer));
            }
            payloads.push(ready());
            payloads
        };
        let mut payloads = history(&[("first question", "first answer")]);
        payloads.extend([
            text(
                "user_message_chunk",
                "zz-prompt-7-00000000000000aa",
                "second question",
            ),
            AgentStreamPayload::PromptAccepted { turn_id: 7 },
            unnamed("agent_message_chunk", "second answer"),
            AgentStreamPayload::PromptFinished {
                turn_id: 7,
                outcome: AgentPromptOutcome::Finished {
                    stop_reason: json!("end_turn"),
                },
            },
        ]);
        payloads.extend(history(&[
            ("first question", "first answer"),
            ("second question", "second answer"),
        ]));

        let mut transcript = Transcript::default();
        let mut view = ChatView::default();
        for payload in &payloads {
            for event in transcript.payload(payload) {
                let _ = view.apply(&event);
            }
        }
        assert_eq!(
            (0..view.turns.len())
                .map(|turn| kinds(&view, turn))
                .collect::<Vec<_>>(),
            [
                [
                    (ItemKind::User, "first question".to_owned()),
                    (ItemKind::Agent, "first answer".to_owned()),
                ],
                [
                    (ItemKind::User, "second question".to_owned()),
                    (ItemKind::Agent, "second answer".to_owned()),
                ],
            ]
        );
        assert_eq!(view.items.len(), 4);
        assert_eq!(view.skipped, 0);
    }
}
