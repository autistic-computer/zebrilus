use zz_protocol::{AgentProvider, PaneId, agent_mention::AgentReport, chat::ItemId};

use crate::chat_rows::{Run, duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub host: Option<String>,
    pub pane: PaneId,
    pub agent: Option<AgentProvider>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerState {
    Working,
    Asking,
    Answered,
    Replied,
    Stopped,
    Failed,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerSent {
    pub run: Run,
    pub peer: Peer,
    pub state: PeerState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerMessage {
    Replied { reply: String },
    Stopped { reason: String, reply: String },
    Failed { message: String },
    Closed,
    Asks { title: String },
    Sent { host: Option<String>, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerNote {
    pub item: ItemId,
    pub peer: Option<Peer>,
    pub waited: Option<u64>,
    pub state: PeerState,
    pub message: PeerMessage,
}

impl PeerMessage {
    #[must_use]
    pub fn of(report: AgentReport) -> Self {
        match report {
            AgentReport::Finished { text, .. } => Self::Replied { reply: text },
            AgentReport::Stopped { reason, text, .. } => Self::Stopped {
                reason,
                reply: text,
            },
            AgentReport::Failed { message, .. } => Self::Failed { message },
            AgentReport::Closed { .. } => Self::Closed,
        }
    }

    #[must_use]
    pub const fn state(&self) -> PeerState {
        match self {
            Self::Replied { .. } | Self::Sent { .. } => PeerState::Replied,
            Self::Stopped { .. } => PeerState::Stopped,
            Self::Failed { .. } => PeerState::Failed,
            Self::Closed => PeerState::Closed,
            Self::Asks { .. } => PeerState::Asking,
        }
    }
}

impl PeerNote {
    #[must_use]
    pub fn time(&self) -> Option<String> {
        match self.message {
            PeerMessage::Replied { .. } | PeerMessage::Stopped { .. } => self.waited.map(duration),
            _ => None,
        }
    }

    #[must_use]
    pub fn line(&self) -> Option<&str> {
        let text = match &self.message {
            PeerMessage::Replied { reply } | PeerMessage::Sent { text: reply, .. } => reply,
            PeerMessage::Stopped { reason, .. } => reason,
            PeerMessage::Failed { message } => message,
            PeerMessage::Asks { title } => title,
            PeerMessage::Closed => return None,
        };
        text.lines().map(str::trim).find(|line| !line.is_empty())
    }

    #[must_use]
    pub fn body(&self) -> Option<&str> {
        match &self.message {
            PeerMessage::Replied { reply }
            | PeerMessage::Stopped { reply, .. }
            | PeerMessage::Sent { text: reply, .. } => Some(reply),
            PeerMessage::Failed { message } => Some(message),
            PeerMessage::Closed | PeerMessage::Asks { .. } => None,
        }
        .map(|body| body.trim())
        .filter(|body| !body.is_empty())
    }
}

#[must_use]
pub fn around_pane(label: &str, pane: PaneId) -> Option<(&str, &str)> {
    let token = pane.to_string();
    label.match_indices(&token).find_map(|(at, _)| {
        let end = at + token.len();
        let whole = !label[end..].starts_with(|c: char| c.is_ascii_digit());
        whole.then(|| (&label[..at], &label[end..]))
    })
}
