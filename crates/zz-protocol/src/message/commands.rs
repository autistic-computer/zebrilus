use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub request_id: u64,
    pub command: CommandInvocation,
    pub prepared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedCommand {
    pub invocation: CommandInvocation,
    pub canonical_name: Option<String>,
    pub alias_matched: bool,
    pub result: PreparedCommandResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreparedCommandResult {
    Ready,
    Error(ServerError),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StdoutClaim {
    #[default]
    None,
    Print,
    Raw,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandResponse {
    Success {
        request_id: u64,
        output: RawText,
        exit_code: u8,
        stderr: String,
        stdout_claim: StdoutClaim,
    },
    Error {
        request_id: u64,
        error: ServerError,
        output: RawText,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ServerError {
    #[error("server protocol mismatch: client {client}, server {server}")]
    ProtocolMismatch { client: u16, server: u16 },
    #[error("target not found: {0}")]
    MissingTarget(String),
    #[error("ambiguous target: {0}")]
    AmbiguousTarget(String),
    #[error("invalid target: {0}")]
    InvalidTarget(String),
    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
    #[error("pane is not attached: {0}")]
    PaneNotAttached(PaneId),
    #[error("pane has exited: {0}")]
    PaneExited(PaneId),
    #[error("internal server error: {0}")]
    Internal(String),
    #[error("can't find session: {0}")]
    SessionNotFound(String),
    #[error("can't find window: {0}")]
    WindowNotFound(String),
    #[error("can't find pane: {0}")]
    PaneNotFound(String),
    #[error("invalid command: {0}")]
    CommandParse(String),
    #[error("{0}")]
    PostAdmissionCallback(Box<ServerError>),
    #[error("invalid command: {0}")]
    NativeCommandParse(String),
    #[error("unsupported command: {0}")]
    NativeUnsupportedCommand(String),
    #[error("invalid command: {0}")]
    NativeInvalidCommand(String),
    #[error(
        "this zz is revision {client} and the running daemon needs revision {minimum} or newer; update zz"
    )]
    ClientTooOld { client: u32, minimum: u32 },
}

impl ServerError {
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NativeCommandParse(_)
            | Self::NativeUnsupportedCommand(_)
            | Self::NativeInvalidCommand(_) => 2,
            Self::PostAdmissionCallback(error) => error.exit_code(),
            _ => 1,
        }
    }

    #[must_use]
    pub const fn is_command_parse(&self) -> bool {
        match self {
            Self::CommandParse(_) | Self::NativeCommandParse(_) => true,
            Self::PostAdmissionCallback(error) => error.is_command_parse(),
            _ => false,
        }
    }

    #[must_use]
    pub const fn is_post_admission_callback(&self) -> bool {
        matches!(self, Self::PostAdmissionCallback(_))
    }

    #[must_use]
    pub fn tmux_message(&self) -> String {
        match self {
            Self::InvalidCommand(message)
            | Self::CommandParse(message)
            | Self::NativeCommandParse(message)
            | Self::NativeInvalidCommand(message) => message.clone(),
            Self::PostAdmissionCallback(error) => error.tmux_message(),
            error => error.to_string(),
        }
    }
}
