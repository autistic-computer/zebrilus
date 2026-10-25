use std::path::Path;

use agent_client_protocol_schema::v1::SessionUpdate;
use serde::{Deserialize, Serialize};
use zz_protocol::{AgentConnectionPhase, agent_stream::AgentStreamPayload};

use crate::agent_transcript::{AgentThreadEntry, AgentTranscript, ToolPayload};

pub const AGENT_NOTICE: &str = "[zz] Agent ";
const LEDGER: &str = "ledger.md";
const TAIL_WITHOUT_LEDGER: usize = 20;
const LINE_LIMIT: usize = 2000;
const PACKET_LIMIT: usize = 12 * 1024;
pub const PACKET_OPENING: &str = "You are the Driver Agent and you were restarted: your previous process \
                         died, and with it every wait you had running. Follow the Restarted \
                         section of your instructions.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Driver,
    Notice,
    Ledger,
}

impl ChatRole {
    const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Driver => "driver",
            Self::Notice => "notice",
            Self::Ledger => "ledger",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChatLine {
    #[serde(rename = "gen")]
    pub generation: u64,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    pub role: ChatRole,
    pub text: String,
}

impl ChatLine {
    #[must_use]
    pub const fn position(&self) -> (u64, u64) {
        (self.generation, self.seq)
    }
}

pub struct DriverLog {
    transcript: AgentTranscript<()>,
    replaying: bool,
    live: bool,
    session: Option<String>,
}

impl Default for DriverLog {
    fn default() -> Self {
        Self {
            transcript: AgentTranscript::new(|_, _| Some(())),
            replaying: false,
            live: false,
            session: None,
        }
    }
}

impl DriverLog {
    pub fn apply(&mut self, payload: AgentStreamPayload) {
        match payload {
            AgentStreamPayload::SessionReset { .. } => {
                self.transcript.reset();
                self.replaying = true;
            }
            AgentStreamPayload::SessionReady { session_id, .. } => {
                self.session = Some(session_id);
                self.replaying = false;
                self.transcript.finish_replay();
                self.transcript.finish_turn();
            }
            AgentStreamPayload::SessionSwitched {
                session_id, replay, ..
            } => {
                if !self.replaying {
                    self.transcript.reset();
                }
                for update in replay {
                    self.update(update);
                }
                self.session = Some(session_id);
                self.replaying = false;
                self.transcript.finish_replay();
            }
            AgentStreamPayload::Update { update } => self.update(update),
            AgentStreamPayload::StateSynced { state } => self.set_phase(&state.phase),
            AgentStreamPayload::PromptFinished { .. } => {
                self.transcript.finish_turn();
                self.transcript.finish_replay();
            }
            _ => {}
        }
    }

    pub const fn set_phase(&mut self, phase: &AgentConnectionPhase) {
        self.live = matches!(
            phase,
            AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
        );
    }

    #[must_use]
    pub const fn settled(&self) -> bool {
        !self.replaying
    }

    #[must_use]
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    #[must_use]
    pub fn lines(&self) -> Vec<(ChatRole, String)> {
        chat_lines(self.transcript.entries(), self.live)
    }

    fn update(&mut self, update: serde_json::Value) {
        if let Ok(update) = serde_json::from_value::<SessionUpdate>(update) {
            self.transcript.apply_update(update);
        }
    }
}

#[must_use]
pub fn chat_lines<I>(entries: &[AgentThreadEntry<I>], live: bool) -> Vec<(ChatRole, String)> {
    let mut lines = Vec::new();
    let mut answer: Option<&str> = None;
    for entry in entries {
        match entry {
            AgentThreadEntry::User { markdown, .. } => {
                if let Some(text) = answer.take() {
                    lines.push((ChatRole::Driver, text.to_owned()));
                }
                let text = markdown.trim();
                if text.starts_with(PACKET_OPENING) {
                    continue;
                }
                let role = if text.starts_with(AGENT_NOTICE) {
                    ChatRole::Notice
                } else {
                    ChatRole::User
                };
                lines.push((role, text.to_owned()));
            }
            AgentThreadEntry::Assistant {
                markdown,
                aside: None,
                ..
            } if !markdown.trim().is_empty() => answer = Some(markdown.trim()),
            AgentThreadEntry::Tool {
                input: Some(ToolPayload::Json(json)),
                ..
            } => {
                if let Some(ledger) = ledger_write(json) {
                    lines.push((ChatRole::Ledger, ledger));
                }
            }
            _ => {}
        }
    }
    if let Some(text) = answer.filter(|_| !live) {
        lines.push((ChatRole::Driver, text.to_owned()));
    }
    lines
}

fn ledger_write(json: &str) -> Option<String> {
    let input = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let path = Path::new(input.get("file_path")?.as_str()?);
    if path.file_name()? != LEDGER {
        return None;
    }
    input.get("content")?.as_str().map(str::to_owned)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    pub ledger: Option<String>,
    pub prompt: String,
}

#[must_use]
pub fn packet(lines: &[ChatLine]) -> Option<Packet> {
    if lines.is_empty() {
        return None;
    }
    let last_ledger = lines.iter().rposition(|line| line.role == ChatRole::Ledger);
    let ledger = last_ledger.map(|index| lines[index].text.clone());
    let since = last_ledger.map_or_else(
        || lines.len().saturating_sub(TAIL_WITHOUT_LEDGER),
        |index| index + 1,
    );
    let request = lines
        .iter()
        .rposition(|line| line.role == ChatRole::User)
        .filter(|index| *index < since)
        .map(|index| packet_line(&lines[index]));
    let mut conversation = lines[since..]
        .iter()
        .filter(|line| line.role != ChatRole::Ledger)
        .map(packet_line)
        .collect::<Vec<_>>();
    let head = format!(
        "{PACKET_OPENING}\n\nYour ledger.md as you last wrote it:\n\n{}\n\nThe conversation since then, \
         the user's last request first:\n\n{}",
        ledger.as_deref().unwrap_or("(you had not written one)"),
        request
            .as_deref()
            .map_or_else(String::new, |line| format!("{line}\n")),
    );
    let mut size = head.len()
        + conversation
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>();
    while size > PACKET_LIMIT && !conversation.is_empty() {
        size -= conversation.remove(0).len() + 1;
    }
    let mut prompt = head;
    for line in conversation {
        prompt.push_str(&line);
        prompt.push('\n');
    }
    Some(Packet {
        ledger,
        prompt: prompt.trim_end().to_owned(),
    })
}

fn packet_line(line: &ChatLine) -> String {
    let stamp = line
        .ts
        .as_deref()
        .map_or_else(String::new, |ts| format!(" {ts}"));
    let mut text = line.text.trim().to_owned();
    if text.len() > LINE_LIMIT {
        let mut end = LINE_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    format!("[{}{stamp}] {text}", line.role.as_str())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zz_protocol::{AgentConnectionPhase, agent_stream::AgentStreamPayload};

    use super::{ChatLine, ChatRole, DriverLog, packet};

    fn chunk(kind: &str, id: &str, text: &str) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: json!({
                "sessionUpdate": kind,
                "messageId": id,
                "content": {"type": "text", "text": text},
            }),
        }
    }

    fn write(id: &str, path: &str, content: &str) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: json!({
                "sessionUpdate": "tool_call",
                "toolCallId": id,
                "title": format!("Write {path}"),
                "kind": "edit",
                "status": "completed",
                "rawInput": {"file_path": path, "content": content},
            }),
        }
    }

    fn ready() -> AgentStreamPayload {
        AgentStreamPayload::SessionReady {
            session_id: "s1".to_owned(),
            modes: None,
            config_options: None,
        }
    }

    fn line(seq: u64, role: ChatRole, text: &str) -> ChatLine {
        ChatLine {
            generation: 2,
            seq,
            ts: None,
            role,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_turn_becomes_the_request_the_ledger_and_the_last_answer() {
        let mut log = DriverLog::default();
        log.apply(AgentStreamPayload::SessionReset { restoring: true });
        log.apply(chunk("user_message_chunk", "u1", "ship it"));
        log.apply(chunk("agent_message_chunk", "a1", "looking"));
        log.apply(write("t1", "/w/ledger.md", "# Goal\nship it"));
        log.apply(write("t2", "/w/brief.md", "do the thing"));
        log.apply(chunk("agent_message_chunk", "a2", "shipped"));
        assert!(!log.settled());
        log.apply(ready());
        assert!(log.settled());
        assert_eq!(log.session(), Some("s1"));
        assert_eq!(
            log.lines(),
            [
                (ChatRole::User, "ship it".to_owned()),
                (ChatRole::Ledger, "# Goal\nship it".to_owned()),
                (ChatRole::Driver, "shipped".to_owned()),
            ]
        );
    }

    #[test]
    fn a_live_turn_keeps_its_answer_back_and_notices_are_their_own_role() {
        let mut log = DriverLog::default();
        log.apply(ready());
        log.apply(chunk(
            "user_message_chunk",
            "u1",
            "[zz] Agent %4 finished: done",
        ));
        log.set_phase(&AgentConnectionPhase::Running);
        log.apply(chunk("agent_message_chunk", "a1", "checking"));
        assert_eq!(
            log.lines(),
            [(ChatRole::Notice, "[zz] Agent %4 finished: done".to_owned())]
        );
        log.set_phase(&AgentConnectionPhase::Ready);
        assert_eq!(log.lines().len(), 2);
    }

    #[test]
    fn the_packet_starts_from_the_last_ledger() {
        let lines = [
            line(1, ChatRole::User, "old request"),
            line(2, ChatRole::Ledger, "# Goal\nold"),
            line(3, ChatRole::User, "new request"),
            line(4, ChatRole::Ledger, "# Goal\nnew"),
            line(5, ChatRole::Driver, "on it"),
        ];
        let restart = packet(&lines).unwrap();
        assert_eq!(restart.ledger.as_deref(), Some("# Goal\nnew"));
        assert!(restart.prompt.contains("# Goal\nnew"));
        assert!(!restart.prompt.contains("old request"));
        assert!(
            restart
                .prompt
                .contains("[user] new request\n[driver] on it")
        );
        assert_eq!(restart.prompt.matches("new request").count(), 1);
        assert!(packet(&[]).is_none());
    }
}
