use super::*;

pub const DEFAULT_AGENT_COMMAND: &str = "codex";
pub const DEFAULT_AGENT_CLAUDE_CODE_COMMAND: &str = "claude";
pub const AGENT_ANTIGRAVITY_COMMAND: &str = "agy";
pub const AGENT_GROK_COMMAND: &str = "grok";
pub const AGENT_CURSOR_COMMAND: &str = "cursor-agent";
pub const AGENT_FLOAT_WIDTH: &str = "46%";
pub const AGENT_FLOAT_HEIGHT: &str = "72%";
pub const DEFAULT_AGENT_AUTO_APPROVE: AgentAutoApprove = AgentAutoApprove::Reads;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentAutoApprove {
    Off,
    #[default]
    Reads,
    All,
}

impl AgentAutoApprove {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Reads => "reads",
            Self::All => "all",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let matches_any = |spellings: &[&str]| {
            spellings
                .iter()
                .any(|spelling| value.eq_ignore_ascii_case(spelling))
        };
        if value == "0" || matches_any(&["off", "no", "false"]) {
            return Some(Self::Off);
        }
        if value.eq_ignore_ascii_case("reads") {
            return Some(Self::Reads);
        }
        if value == "1" || matches_any(&["all", "on", "yes", "true"]) {
            return Some(Self::All);
        }
        None
    }
}

fn deserialize_agent_image_format<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_IMAGE_FORMAT_BYTES)
}

fn deserialize_agent_image_data<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_AGENT_PROMPT_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a prompt image within the wire byte limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn deserialize_agent_option_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_OPTION_BYTES)
}

pub(super) fn deserialize_optional_agent_option_text<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_optional_text(deserializer, MAX_AGENT_OPTION_BYTES)
}

fn deserialize_agent_session_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_SESSION_ID_BYTES)
}

fn deserialize_optional_agent_session_id<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_optional_text(deserializer, MAX_AGENT_SESSION_ID_BYTES)
}

fn deserialize_agent_state_blob<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_STATE_BLOB_BYTES)
}

fn deserialize_optional_agent_state_blob<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_optional_text(deserializer, MAX_AGENT_STATE_BLOB_BYTES)
}

fn deserialize_agent_tasks<'de, D>(deserializer: D) -> Result<Vec<AgentTaskWire>, D::Error>
where
    D: Deserializer<'de>,
{
    let tasks = Vec::<AgentTaskWire>::deserialize(deserializer)?;
    if tasks.len() > MAX_AGENT_TASKS {
        return Err(D::Error::invalid_length(
            tasks.len(),
            &"agent tasks within the task count limit",
        ));
    }
    Ok(tasks)
}

fn deserialize_agent_answer_texts<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let answers = Vec::<String>::deserialize(deserializer)?;
    if answers.len() > MAX_AGENT_QUESTION_ANSWERS
        || answers
            .iter()
            .any(|answer| answer.len() > MAX_AGENT_ANSWER_BYTES)
    {
        return Err(D::Error::invalid_length(
            answers.len(),
            &"answers within the answer limits",
        ));
    }
    Ok(answers)
}

fn deserialize_agent_permission_payload<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_PERMISSION_BYTES)
}

#[must_use]
pub fn agent_update_batch_bytes(items: &[Vec<u8>]) -> usize {
    items
        .iter()
        .fold(0_usize, |total, item| total.saturating_add(item.len()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentImage {
    #[serde(deserialize_with = "deserialize_agent_image_format")]
    pub format: String,
    #[serde(
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "deserialize_agent_image_data"
    )]
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSessionOpKind {
    List {
        cwd: Option<PathBuf>,
        #[serde(deserialize_with = "deserialize_optional_agent_session_id")]
        cursor: Option<String>,
        replace: bool,
    },
    New {
        cwd: PathBuf,
    },
    Switch {
        #[serde(deserialize_with = "deserialize_agent_session_id")]
        session_id: String,
        cwd: PathBuf,
        additional_directories: Vec<PathBuf>,
    },
    Delete {
        #[serde(deserialize_with = "deserialize_agent_session_id")]
        session_id: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentConnectionPhase {
    #[default]
    Starting,
    Ready,
    Running,
    AwaitingPermission,
    Failed {
        #[serde(deserialize_with = "deserialize_agent_state_blob")]
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPermissionWire {
    pub request_id: u64,
    #[serde(deserialize_with = "deserialize_agent_permission_payload")]
    pub payload: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskWire {
    #[serde(deserialize_with = "deserialize_agent_option_text")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_agent_option_text")]
    pub kind: String,
    #[serde(deserialize_with = "deserialize_agent_option_text")]
    pub description: String,
    #[serde(deserialize_with = "deserialize_optional_agent_option_text")]
    pub tool_call_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentQuestionAnswer {
    #[serde(deserialize_with = "deserialize_agent_option_text")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_agent_answer_texts")]
    pub answers: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGitSummary {
    #[serde(deserialize_with = "deserialize_optional_agent_option_text")]
    pub branch: Option<String>,
    pub changed_files: u32,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPaneWire {
    pub phase: AgentConnectionPhase,
    pub queued_prompts: u32,
    #[serde(deserialize_with = "deserialize_optional_agent_session_id")]
    pub session_id: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_agent_option_text")]
    pub title: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_agent_state_blob")]
    pub error: Option<String>,
    #[serde(deserialize_with = "deserialize_agent_state_blob")]
    pub auth_methods: String,
    #[serde(deserialize_with = "deserialize_agent_state_blob")]
    pub config_options: String,
    #[serde(deserialize_with = "deserialize_agent_state_blob")]
    pub modes: String,
    pub pending_permission: Option<AgentPermissionWire>,
    pub git: Option<AgentGitSummary>,
    #[serde(deserialize_with = "deserialize_agent_tasks")]
    pub tasks: Vec<AgentTaskWire>,
}

impl AgentPaneWire {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let AgentConnectionPhase::Failed { message } = &self.phase
            && message.len() > MAX_AGENT_STATE_BLOB_BYTES
        {
            return Err("agent failure message exceeds the wire byte limit");
        }
        if self
            .session_id
            .as_ref()
            .is_some_and(|session_id| session_id.len() > MAX_AGENT_SESSION_ID_BYTES)
        {
            return Err("agent session ID exceeds the wire byte limit");
        }
        if self
            .title
            .as_ref()
            .is_some_and(|title| title.len() > MAX_AGENT_OPTION_BYTES)
        {
            return Err("agent title exceeds the wire byte limit");
        }
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.len() > MAX_AGENT_STATE_BLOB_BYTES)
        {
            return Err("agent error exceeds the wire byte limit");
        }
        if self.auth_methods.len() > MAX_AGENT_STATE_BLOB_BYTES
            || self.config_options.len() > MAX_AGENT_STATE_BLOB_BYTES
            || self.modes.len() > MAX_AGENT_STATE_BLOB_BYTES
        {
            return Err("agent state blob exceeds the wire byte limit");
        }
        if self
            .pending_permission
            .as_ref()
            .is_some_and(|permission| permission.payload.len() > MAX_AGENT_PERMISSION_BYTES)
        {
            return Err("agent permission payload exceeds the wire byte limit");
        }
        if self
            .git
            .as_ref()
            .and_then(|git| git.branch.as_ref())
            .is_some_and(|branch| branch.len() > MAX_AGENT_OPTION_BYTES)
        {
            return Err("agent Git branch exceeds the wire byte limit");
        }
        if self.tasks.len() > MAX_AGENT_TASKS
            || self.tasks.iter().any(|task| {
                task.id.len() > MAX_AGENT_OPTION_BYTES
                    || task.kind.len() > MAX_AGENT_OPTION_BYTES
                    || task.description.len() > MAX_AGENT_OPTION_BYTES
                    || task
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| id.len() > MAX_AGENT_OPTION_BYTES)
            })
        {
            return Err("agent tasks exceed the wire limits");
        }
        Ok(())
    }
}
