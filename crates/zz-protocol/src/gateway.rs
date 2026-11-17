use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AskpassPromptKind {
    Secret,
    HostKey,
    AgentConfirm,
    SaveKey,
    Notification,
    KeyAdded,
    KeyFailed,
}

impl AskpassPromptKind {
    #[must_use]
    pub const fn is_notice(self) -> bool {
        matches!(self, Self::Notification | Self::KeyAdded | Self::KeyFailed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInPrompt {
    pub kind: AskpassPromptKind,
    pub text: String,
    pub echo: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignInAnswer {
    Answer(String),
    Cancel,
}
