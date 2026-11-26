use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error as _, SeqAccess, Visitor},
};
use zz_terminal::{
    AppearanceProvenance, ClipboardTarget, DEFAULT_HISTORY_LIMIT, DEFAULT_WORD_SEPARATORS,
    KeyInput, PackedCell, SearchDirection, TerminalAppearance, TerminalColorScheme,
    TerminalDictionary, TerminalViewAction, TerminalViewport, TerminalViewportPatch,
};

use crate::{
    Axis, ClientId, ClientInstanceId, GitMark, MuxSnapshot, PaneId, PathEntry, PathListRoot,
    SessionId, SplitId, WindowId,
    path_list::{
        deserialize_git_marks, deserialize_optional_path_list_text, deserialize_path_entries,
        deserialize_path_list_result,
    },
};

mod limits;
pub use limits::*;

mod options;
pub use options::*;

mod presentation;
pub use presentation::*;

mod raw_text;
pub use raw_text::*;

mod hello;
pub use hello::*;

mod commands;
pub use commands::*;

mod input;
pub use input::*;

mod agent;
pub use agent::*;

mod overlays;
pub use overlays::*;

mod transfers;
pub use transfers::*;

pub const PROTOCOL_VERSION: u16 = 125;
pub const REVISION: u32 = 11;
pub const MIN_DAEMON_REVISION: u32 = 1;
pub const MIN_CLIENT_REVISION: u32 = 1;

fn deserialize_agent_prompt_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_PROMPT_BYTES)
}

fn deserialize_agent_images<'de, D>(deserializer: D) -> Result<Vec<AgentImage>, D::Error>
where
    D: Deserializer<'de>,
{
    let images = Vec::<AgentImage>::deserialize(deserializer)?;
    if images.len() > MAX_AGENT_PROMPT_IMAGES {
        return Err(D::Error::invalid_length(
            images.len(),
            &"an agent prompt within the image count limit",
        ));
    }
    Ok(images)
}

fn deserialize_agent_question_answers<'de, D>(
    deserializer: D,
) -> Result<Vec<AgentQuestionAnswer>, D::Error>
where
    D: Deserializer<'de>,
{
    let answers = Vec::<AgentQuestionAnswer>::deserialize(deserializer)?;
    if answers.len() > MAX_AGENT_QUESTION_ANSWERS {
        return Err(D::Error::invalid_length(
            answers.len(),
            &"answers within the question count limit",
        ));
    }
    Ok(answers)
}

fn deserialize_agent_result<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_RESULT_BYTES)
}

fn deserialize_agent_update_items<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    let items = deserialize_byte_vecs(deserializer)?;
    if agent_update_batch_bytes(&items) > MAX_AGENT_UPDATES_BYTES {
        return Err(D::Error::invalid_length(
            items.len(),
            &"an agent update batch within the wire byte limit",
        ));
    }
    Ok(items)
}

#[cfg(test)]
#[path = "wire_bytes_tests.rs"]
mod wire_bytes_tests;

fn deserialize_paste_upload_extension<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let extension = String::deserialize(deserializer)?;
    if !paste_upload_extension_is_valid(&extension) {
        return Err(D::Error::invalid_value(
            serde::de::Unexpected::Str(&extension),
            &"1 to 8 lowercase ASCII alphanumerics",
        ));
    }
    Ok(extension)
}

fn deserialize_paste_upload_total<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let total_bytes = u32::deserialize(deserializer)?;
    if total_bytes == 0 || total_bytes > MAX_PASTE_UPLOAD_BYTES {
        return Err(D::Error::invalid_length(
            total_bytes as usize,
            &"a nonempty paste upload within the wire byte limit",
        ));
    }
    Ok(total_bytes)
}

fn deserialize_browser_screenshot<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_BROWSER_SCREENSHOT_CHUNK_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a browser screenshot chunk within the wire byte limit",
        ));
    }
    Ok(bytes)
}

fn deserialize_artifact_chunk<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_ARTIFACT_CHUNK_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"an artifact chunk within the wire byte limit",
        ));
    }
    Ok(bytes)
}

fn deserialize_artifact_token<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_ARTIFACT_TOKEN_BYTES)
}

fn deserialize_artifact_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_ARTIFACT_PATH_BYTES)
}

fn deserialize_artifact_mime<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_ARTIFACT_MIME_BYTES)
}

fn deserialize_artifact_reply<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_ARTIFACT_REPLY_BYTES)
}

fn deserialize_paste_upload_chunk<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_PASTE_UPLOAD_CHUNK_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a paste upload chunk within the wire byte limit",
        ));
    }
    Ok(bytes)
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientPath(
    #[serde(
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "deserialize_client_path"
    )]
    Vec<u8>,
);

impl ClientPath {
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;

            Some(Self(path.as_os_str().as_bytes().to_vec()))
        }
        #[cfg(not(unix))]
        {
            Some(Self(path.to_str()?.as_bytes().to_vec()))
        }
    }

    #[must_use]
    pub fn to_path_buf(&self) -> Option<PathBuf> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;

            Some(PathBuf::from(std::ffi::OsString::from_vec(self.0.clone())))
        }
        #[cfg(not(unix))]
        {
            String::from_utf8(self.0.clone()).ok().map(PathBuf::from)
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for ClientPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_path_buf() {
            Some(path) => fmt::Debug::fmt(&path, formatter),
            None => formatter.write_str("<unrepresentable client path>"),
        }
    }
}

fn deserialize_client_path<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_CLIENT_WORKING_DIRECTORY_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a client path within the wire byte limit",
        ));
    }
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub source: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    pub args: Vec<RawText>,
    pub source: Option<SourceSpan>,
    command_blocks: Vec<u32>,
    expanded_alias_group: bool,
    stdin: Option<RawText>,
    #[serde(skip)]
    stdin_spent: bool,
    #[serde(default)]
    stdin_available: bool,
}

impl CommandInvocation {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<RawText>>,
    ) -> Self {
        Self {
            name: name.into(),
            args: args.into_iter().map(Into::into).collect(),
            source: None,
            command_blocks: Vec::new(),
            expanded_alias_group: false,
            stdin: None,
            stdin_spent: false,
            stdin_available: false,
        }
    }

    pub fn set_stdin(&mut self, stdin: impl Into<RawText>) {
        self.stdin = Some(stdin.into());
        self.stdin_spent = false;
    }

    pub fn set_stdin_available(&mut self, available: bool) {
        self.stdin_available = available;
    }

    #[must_use]
    pub const fn stdin_available(&self) -> bool {
        self.stdin_available
    }

    pub fn set_stdin_spent(&mut self) {
        self.stdin = None;
        self.stdin_spent = true;
    }

    #[must_use]
    pub const fn stdin_was_spent(&self) -> bool {
        self.stdin_spent
    }

    #[must_use]
    pub const fn stdin(&self) -> Option<&RawText> {
        self.stdin.as_ref()
    }

    #[must_use]
    pub fn with_source(mut self, source: SourceSpan) -> Self {
        self.source = Some(source);
        self
    }

    #[must_use]
    pub fn with_command_blocks(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        self.command_blocks = indices
            .into_iter()
            .filter(|index| *index < self.args.len())
            .filter_map(|index| u32::try_from(index).ok())
            .collect();
        self.command_blocks.sort_unstable();
        self.command_blocks.dedup();
        self
    }

    #[must_use]
    pub fn into_expanded_alias_group(mut self) -> Self {
        self.expanded_alias_group = true;
        self
    }

    #[must_use]
    pub const fn is_expanded_alias_group(&self) -> bool {
        self.expanded_alias_group
    }

    #[must_use]
    pub fn argument_is_command_block(&self, index: usize) -> bool {
        u32::try_from(index).is_ok_and(|index| self.command_blocks.contains(&index))
    }

    pub fn append_args(&mut self, command: Self) {
        let offset = self.args.len();
        let appended_len = command.args.len();
        self.args.extend(command.args);
        self.command_blocks
            .extend(command.command_blocks.into_iter().filter_map(|index| {
                let index = usize::try_from(index).ok()?;
                (index < appended_len)
                    .then(|| offset.checked_add(index))
                    .flatten()
                    .and_then(|index| u32::try_from(index).ok())
            }));
        self.command_blocks
            .retain(|index| usize::try_from(*index).is_ok_and(|index| index < self.args.len()));
        self.command_blocks.sort_unstable();
        self.command_blocks.dedup();
    }

    #[must_use]
    pub fn split_commands_from(&self, start: usize) -> Vec<Self> {
        split_tagged_command_words(
            self.args
                .iter()
                .cloned()
                .enumerate()
                .skip(start)
                .map(|(index, word)| (word, self.argument_is_command_block(index))),
        )
        .into_iter()
        .filter_map(|words| {
            let mut words = words.into_iter();
            let (name, name_is_command_block) = words.next()?;
            if name_is_command_block {
                return None;
            }
            let mut args = Vec::new();
            let mut command_blocks = Vec::new();
            for (index, (word, is_command_block)) in words.enumerate() {
                args.push(word);
                if is_command_block {
                    command_blocks.push(index);
                }
            }
            Some(Self::new(name, args).with_command_blocks(command_blocks))
        })
        .collect()
    }
}

#[must_use]
pub fn split_command_words(words: impl IntoIterator<Item = RawText>) -> Vec<Vec<RawText>> {
    split_tagged_command_words(words.into_iter().map(|word| (word, ())))
        .into_iter()
        .map(|words| words.into_iter().map(|(word, ())| word).collect())
        .collect()
}

fn split_tagged_command_words<T>(
    words: impl IntoIterator<Item = (RawText, T)>,
) -> Vec<Vec<(RawText, T)>> {
    let mut commands = Vec::new();
    let mut current = Vec::new();
    for (word, tag) in words {
        let mut bytes = word.into_bytes();
        let mut end = false;
        if bytes.last() == Some(&b';') {
            bytes.pop();
            if bytes.last() == Some(&b'\\') {
                bytes.pop();
                bytes.push(b';');
            } else {
                end = true;
            }
        }
        let word = RawText::from_bytes(bytes);
        if !end || !word.as_bytes().is_empty() {
            current.push((word, tag));
        }
        if end && !current.is_empty() {
            commands.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        commands.push(current);
    }
    commands
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub payload: EventPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetachReason {
    Requested,
    Evicted,
    SessionDestroyed,
    ServerStopping,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientExitAction {
    #[default]
    Exit,
    ParentHangup,
    Exec {
        command: String,
        shell: String,
    },
}

impl ClientExitAction {
    #[must_use]
    pub const fn is_exit(&self) -> bool {
        matches!(self, Self::Exit)
    }

    #[must_use]
    pub const fn is_parent_hangup(&self) -> bool {
        matches!(self, Self::ParentHangup)
    }
}

impl DetachReason {
    #[must_use]
    pub const fn is_requested(self) -> bool {
        matches!(self, Self::Requested)
    }

    #[must_use]
    pub const fn is_evicted(self) -> bool {
        matches!(self, Self::Evicted)
    }

    #[must_use]
    pub const fn is_session_destroyed(self) -> bool {
        matches!(self, Self::SessionDestroyed)
    }

    #[must_use]
    pub const fn is_server_stopping(self) -> bool {
        matches!(self, Self::ServerStopping)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ControlSourceFileEvent {
    ReadError(String),
    Complete,
}

struct BoundedStartupConfigCause(String);

impl<'de> Deserialize<'de> for BoundedStartupConfigCause {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StartupConfigCauseVisitor;

        impl<'de> Visitor<'de> for StartupConfigCauseVisitor {
            type Value = BoundedStartupConfigCause;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "a startup configuration cause no longer than {MAX_STARTUP_CONFIG_CAUSE_BYTES} bytes"
                )
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(value)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.len() > MAX_STARTUP_CONFIG_CAUSE_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedStartupConfigCause(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.len() > MAX_STARTUP_CONFIG_CAUSE_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedStartupConfigCause(value))
            }
        }

        deserializer.deserialize_str(StartupConfigCauseVisitor)
    }
}

fn deserialize_startup_config_causes<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StartupConfigCausesVisitor;

    impl<'de> Visitor<'de> for StartupConfigCausesVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {MAX_STARTUP_CONFIG_CAUSES} startup configuration causes totaling at most {MAX_STARTUP_CONFIG_CAUSES_BYTES} bytes"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let hint = sequence.size_hint();
            if hint.is_some_and(|length| length > MAX_STARTUP_CONFIG_CAUSES) {
                return Err(A::Error::invalid_length(
                    hint.unwrap_or(MAX_STARTUP_CONFIG_CAUSES.saturating_add(1)),
                    &self,
                ));
            }
            let mut causes = Vec::with_capacity(hint.unwrap_or(0).min(MAX_STARTUP_CONFIG_CAUSES));
            let mut total_bytes = 0usize;
            while causes.len() < MAX_STARTUP_CONFIG_CAUSES {
                let Some(cause) = sequence.next_element::<BoundedStartupConfigCause>()? else {
                    return Ok(causes);
                };
                total_bytes = total_bytes
                    .checked_add(cause.0.len())
                    .ok_or_else(|| A::Error::invalid_length(usize::MAX, &self))?;
                if total_bytes > MAX_STARTUP_CONFIG_CAUSES_BYTES {
                    return Err(A::Error::invalid_length(total_bytes, &self));
                }
                causes.push(cause.0);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::invalid_length(
                    MAX_STARTUP_CONFIG_CAUSES.saturating_add(1),
                    &self,
                ));
            }
            Ok(causes)
        }
    }

    deserializer.deserialize_seq(StartupConfigCausesVisitor)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardProducer {
    #[default]
    Server,
    Application,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EventPayload {
    Snapshot(MuxSnapshot),
    AppearanceChanged {
        appearance: Box<TerminalAppearance>,
        provenance: AppearanceProvenance,
    },
    MuxOptionsChanged {
        options: MuxOptions,
    },
    StatusChanged {
        status: StatusLine,
    },
    TerminalViewport {
        pane: PaneId,
        #[serde(with = "crate::pane_frame::viewport_bytes")]
        viewport: TerminalViewport,
    },
    TerminalPatch {
        pane: PaneId,
        patch: TerminalViewportPatch,
    },
    Clipboard {
        pane: PaneId,
        request_id: u64,
        target: ClipboardTarget,
        text: String,
        producer: ClipboardProducer,
    },
    BrowserCommand {
        pane: PaneId,
        command: BrowserCommand,
    },
    TerminalUiCommand {
        pane: PaneId,
        command: TerminalUiCommand,
    },
    CommandPrompt {
        state: Option<CommandPromptState>,
    },
    CommandOutput {
        pane: PaneId,
        output_id: u64,
        #[serde(with = "crate::pane_frame::optional_viewport_bytes")]
        viewport: Option<TerminalViewport>,
    },
    ChooseTree {
        state: Option<ChooseTreeState>,
    },
    ChooseTreeUpdate {
        search: Option<ChooseTreeSearchState>,
        selected: u32,
    },
    ChooseBuffer {
        state: Option<ChooseBufferState>,
    },
    ChooseBufferUpdate {
        search: Option<ChooseBufferSearchState>,
        selected: u32,
    },
    DisplayPanes {
        state: Option<DisplayPanesState>,
    },
    ClientMessage {
        pane: Option<PaneId>,
        kind: ClientMessageKind,
        text: String,
    },
    PaneRemoved(PaneId),
    ServerStopping,
    OpenUri {
        pane: PaneId,
        uri: String,
    },
    FocusSidebar,
    PrefixArmed {
        armed: bool,
    },
    Detached {
        session: SessionId,
        by: Option<String>,
        reason: DetachReason,
        action: ClientExitAction,
    },
    HistoryChunk {
        pane: PaneId,
        start: u32,
        total: u32,
        offset: u32,
        columns: u16,
        rows: Vec<Vec<PackedCell>>,
        dictionary: TerminalDictionary,
    },
    Bell {
        pane: PaneId,
    },
    KeyTablesChanged {
        tables: Vec<KeyTableSnapshot>,
    },
    KittyImageBegin {
        pane: PaneId,
        image_id: u32,
        generation: u64,
        width: u32,
        height: u32,
        total_bytes: u32,
    },
    KittyImageChunk {
        pane: PaneId,
        image_id: u32,
        generation: u64,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    KittyImagesRemoved {
        pane: PaneId,
        image_ids: Vec<u32>,
    },
    AgentUpdates {
        pane: PaneId,
        first_seq: u64,
        #[serde(
            serialize_with = "serialize_byte_vecs",
            deserialize_with = "deserialize_agent_update_items"
        )]
        items: Vec<Vec<u8>>,
    },
    AgentState {
        pane: PaneId,
        state: AgentPaneWire,
    },
    AgentLagged {
        pane: PaneId,
        next_seq: u64,
    },
    AgentSessions {
        pane: PaneId,
        request_id: u64,
        #[serde(deserialize_with = "deserialize_agent_result")]
        result: String,
    },
    TimedClientMessage {
        pane: Option<PaneId>,
        kind: ClientMessageKind,
        text: String,
        duration_ms: u32,
        message_id: u64,
    },
    PrefixCancelled {
        request_id: u64,
    },
    #[serde(rename = "Popup")]
    RetiredPopup {
        state: Option<PopupState>,
    },
    Menu {
        state: Option<MenuState>,
    },
    Confirm {
        state: Option<ConfirmState>,
    },
    ControlExit {
        reason: String,
    },
    HookEvent {
        name: String,
        variables: BTreeMap<String, String>,
    },
    PaneOutput {
        pane: PaneId,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    PaneOutputState {
        pane: PaneId,
        paused: bool,
    },
    PaneOutputAged {
        pane: PaneId,
        age_ms: u64,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    ControlFlags {
        wait_exit: bool,
        pause_after_ms: Option<u64>,
        no_output: bool,
        #[serde(default)]
        new_layouts: bool,
    },
    SubscriptionChanged {
        name: String,
        session: SessionId,
        window: Option<WindowId>,
        window_index: Option<u32>,
        pane: Option<PaneId>,
        value: String,
    },
    TimedClientMessageCleared {
        message_id: u64,
    },
    ControlCommandGuard {
        output: RawText,
        error: bool,
        sticky_failure: bool,
        flags: u8,
    },
    ControlSourceFile {
        event: ControlSourceFileEvent,
    },
    StartupConfigCauses {
        #[serde(deserialize_with = "deserialize_startup_config_causes")]
        causes: Vec<String>,
    },
    ControlCommandOutput {
        output: String,
    },
    ControlConfigError {
        text: String,
    },
    ChooserPresentation {
        presentation: Option<Box<ChooserPresentation>>,
    },
    CommandStdout {
        output: RawText,
    },
    CommandClientExit,
    KeyTableActive {
        table: Option<String>,
        repeat: bool,
    },
    OpenPathPicker {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_optional_path_list_text")]
        start_dir: Option<String>,
    },
    KeyTablesPatched {
        tables: Vec<KeyTableSnapshot>,
        removed: Vec<String>,
    },
    TreeDelta(crate::TreeDelta),
    ClientView(crate::ClientView),
    KeyTablesHashChanged {
        hash: u64,
        mouse: crate::MouseBindings,
    },
    MuxOptionsPatched {
        options: MuxOptions,
    },
    ControlCommandGuardRaw {
        output: RawText,
        error: bool,
        sticky_failure: bool,
        flags: u8,
    },
    ControlCommandStarted {
        request_id: u64,
        flags: u32,
        canonical_name: Option<String>,
        guard: bool,
    },
    ClipboardQuery,
    TerminalNegotiation {
        features: Vec<String>,
        user_keys: Vec<String>,
        application_escape: Vec<String>,
    },
    Providers(crate::ProvidersReport),
    AgentAccounts(Vec<crate::AgentAccount>),
    AgentQuotas(Vec<crate::AgentQuota>),
    AgentLogin(crate::AgentLogin),
    SimulatorPane {
        pane: PaneId,
        descriptor: crate::SimulatorDescriptor,
    },
    SimulatorCommand {
        pane: PaneId,
        command: SimulatorCommand,
    },
    AgentReplayed {
        pane: PaneId,
        through_seq: u64,
    },
    AgentParents {
        parents: BTreeMap<PaneId, PaneId>,
    },
}

impl EventPayload {
    #[must_use]
    pub const fn required_capability(&self) -> Option<&'static str> {
        match self {
            Self::Snapshot(_)
            | Self::AppearanceChanged { .. }
            | Self::MuxOptionsChanged { .. }
            | Self::StatusChanged { .. }
            | Self::TerminalViewport { .. }
            | Self::TerminalPatch { .. }
            | Self::Clipboard { .. }
            | Self::BrowserCommand { .. }
            | Self::TerminalUiCommand { .. }
            | Self::CommandPrompt { .. }
            | Self::CommandOutput { .. }
            | Self::ChooseTree { .. }
            | Self::ChooseTreeUpdate { .. }
            | Self::ChooseBuffer { .. }
            | Self::ChooseBufferUpdate { .. }
            | Self::DisplayPanes { .. }
            | Self::ClientMessage { .. }
            | Self::PaneRemoved(_)
            | Self::ServerStopping
            | Self::OpenUri { .. }
            | Self::FocusSidebar
            | Self::PrefixArmed { .. }
            | Self::Detached { .. }
            | Self::HistoryChunk { .. }
            | Self::Bell { .. }
            | Self::KeyTablesChanged { .. }
            | Self::KittyImageBegin { .. }
            | Self::KittyImageChunk { .. }
            | Self::KittyImagesRemoved { .. }
            | Self::AgentUpdates { .. }
            | Self::AgentState { .. }
            | Self::AgentLagged { .. }
            | Self::AgentSessions { .. }
            | Self::TimedClientMessage { .. }
            | Self::PrefixCancelled { .. }
            | Self::RetiredPopup { .. }
            | Self::Menu { .. }
            | Self::Confirm { .. }
            | Self::ControlExit { .. }
            | Self::HookEvent { .. }
            | Self::PaneOutput { .. }
            | Self::PaneOutputState { .. }
            | Self::PaneOutputAged { .. }
            | Self::ControlFlags { .. }
            | Self::SubscriptionChanged { .. }
            | Self::TimedClientMessageCleared { .. }
            | Self::ControlCommandGuard { .. }
            | Self::ControlSourceFile { .. }
            | Self::StartupConfigCauses { .. }
            | Self::ControlCommandOutput { .. }
            | Self::ControlConfigError { .. }
            | Self::ChooserPresentation { .. }
            | Self::CommandStdout { .. }
            | Self::CommandClientExit
            | Self::KeyTableActive { .. }
            | Self::OpenPathPicker { .. }
            | Self::KeyTablesPatched { .. }
            | Self::TreeDelta(_)
            | Self::ClientView(_)
            | Self::KeyTablesHashChanged { .. }
            | Self::MuxOptionsPatched { .. }
            | Self::ControlCommandGuardRaw { .. }
            | Self::ControlCommandStarted { .. }
            | Self::ClipboardQuery
            | Self::TerminalNegotiation { .. }
            | Self::Providers(_)
            | Self::AgentAccounts(_) => None,
            Self::AgentQuotas(_) => Some(crate::AGENT_QUOTAS_CAPABILITY),
            Self::AgentLogin(_) => Some(crate::AGENT_LOGIN_CAPABILITY),
            Self::SimulatorPane { .. } | Self::SimulatorCommand { .. } => {
                Some(crate::SIMULATOR_PANE_CAPABILITY)
            }
            Self::AgentReplayed { .. } => Some(crate::AGENT_REPLAYED_CAPABILITY),
            Self::AgentParents { .. } => Some(crate::AGENT_PARENTS_CAPABILITY),
        }
    }

    #[must_use]
    pub fn detached_requested(session: SessionId, by: Option<String>) -> Self {
        Self::detached_requested_with_action(session, by, ClientExitAction::Exit)
    }

    #[must_use]
    pub fn detached_requested_with_action(
        session: SessionId,
        by: Option<String>,
        action: ClientExitAction,
    ) -> Self {
        Self::Detached {
            session,
            by,
            reason: DetachReason::Requested,
            action,
        }
    }

    #[must_use]
    pub fn detached_evicted(session: SessionId, by: Option<String>) -> Self {
        Self::detached_evicted_with_action(session, by, ClientExitAction::Exit)
    }

    #[must_use]
    pub fn detached_evicted_with_action(
        session: SessionId,
        by: Option<String>,
        action: ClientExitAction,
    ) -> Self {
        Self::Detached {
            session,
            by,
            reason: DetachReason::Evicted,
            action,
        }
    }

    #[must_use]
    pub fn detached_session_destroyed(session: SessionId) -> Self {
        Self::Detached {
            session,
            by: None,
            reason: DetachReason::SessionDestroyed,
            action: ClientExitAction::Exit,
        }
    }

    #[must_use]
    pub fn detached_server_stopping(session: SessionId) -> Self {
        Self::Detached {
            session,
            by: None,
            reason: DetachReason::ServerStopping,
            action: ClientExitAction::Exit,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(
    clippy::large_enum_variant,
    reason = "messages are moved into framing immediately and enum layout must stay allocation-free for terminal events"
)]
pub enum ProtocolMessage {
    ClientHello(ClientHello),
    ServerHello(Box<ServerHello>),
    CommandRequest(CommandRequest),
    CommandResponse(CommandResponse),
    Attach {
        session: String,
    },
    Attached {
        session: SessionId,
        snapshot: MuxSnapshot,
        read_only: bool,
        client_flags: String,
    },
    Detach,
    SetColorScheme(TerminalColorScheme),
    SetConfigOverrides {
        entries: Vec<ConfigOverrideEntry>,
    },
    Input(InputMessage),
    Event(Event),
    GuiResponse(GuiResponse),
    Resync,
    RequestFull {
        pane: PaneId,
    },
    HistoryRequest {
        pane: PaneId,
        start: u32,
        count: u32,
    },
    PasteUploadBegin {
        upload_id: u64,
        pane: PaneId,
        purpose: PasteUploadPurpose,
        #[serde(deserialize_with = "deserialize_paste_upload_extension")]
        extension: String,
        #[serde(deserialize_with = "deserialize_paste_upload_total")]
        total_bytes: u32,
    },
    PasteUploadChunk {
        upload_id: u64,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_paste_upload_chunk"
        )]
        bytes: Vec<u8>,
    },
    FetchPastedImage {
        pane: PaneId,
        number: u32,
    },
    PastedImageBegin {
        pane: PaneId,
        number: u32,
        format: PastedImageFormat,
        #[serde(deserialize_with = "deserialize_paste_upload_total")]
        total_bytes: u32,
    },
    PastedImageChunk {
        pane: PaneId,
        number: u32,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_paste_upload_chunk"
        )]
        bytes: Vec<u8>,
    },
    PastedImageUnavailable {
        pane: PaneId,
        number: u32,
    },
    AgentPrompt {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_prompt_text")]
        text: String,
        #[serde(deserialize_with = "deserialize_agent_images")]
        images: Vec<AgentImage>,
    },
    AgentCancel {
        pane: PaneId,
    },
    AgentUnqueue {
        pane: PaneId,
    },
    AgentRespondPermission {
        pane: PaneId,
        request_id: u64,
        #[serde(deserialize_with = "deserialize_optional_agent_option_text")]
        option_id: Option<String>,
    },
    AgentSetConfigOption {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_option_text")]
        option_id: String,
        #[serde(deserialize_with = "deserialize_agent_option_text")]
        value: String,
    },
    AgentSetMode {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_option_text")]
        mode_id: String,
    },
    AgentAuthenticate {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_option_text")]
        method_id: String,
    },
    AgentSessionOp {
        pane: PaneId,
        op: AgentSessionOpKind,
    },
    AgentReplay {
        pane: PaneId,
        from_seq: u64,
    },
    AgentAcknowledgePromptRestore {
        pane: PaneId,
        reclaim_id: u64,
    },
    PrepareCommandList {
        request_id: u64,
        commands: Vec<CommandInvocation>,
    },
    PreparedCommandList {
        request_id: u64,
        commands: Vec<PreparedCommand>,
    },
    SetTerminalPreview {
        enabled: bool,
    },
    ClientFileRequest(ClientFileRequest),
    ClientFileResponse(ClientFileResponse),
    HomeDirectoryRequest {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_home_directory_users")]
        users: Vec<String>,
    },
    HomeDirectoryResponse {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_home_directories")]
        homes: Vec<Option<String>>,
    },
    CommandQueueParked {
        request_id: u64,
    },
    EnvironmentRequest {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_environment_names")]
        names: Vec<String>,
    },
    EnvironmentResponse {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_environment_values")]
        values: Vec<Option<String>>,
    },
    ClientTerminalFeatures {
        #[serde(deserialize_with = "deserialize_client_terminal_features")]
        features: Vec<String>,
    },
    ClientTerminalType {
        #[serde(deserialize_with = "deserialize_client_terminal_type")]
        term_type: String,
    },
    PathListRequest {
        request_id: u64,
        pane: Option<PaneId>,
        #[serde(deserialize_with = "deserialize_optional_path_list_text")]
        dir: Option<String>,
    },
    PathListBegin {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_path_list_result")]
        result: Result<PathListRoot, String>,
    },
    PathListChunk {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_path_entries")]
        entries: Vec<PathEntry>,
        done: bool,
        truncated: bool,
    },
    PathListGit {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_git_marks")]
        marks: Vec<(String, GitMark)>,
    },
    PathListCancel {
        request_id: u64,
    },
    Exec(crate::ExecRequest),
    ExecExit(crate::ExecExit),
    Hello(crate::Hello),
    Welcome(crate::Welcome),
    Batch(crate::Batch),
    TreeSync,
    GetKeyTables,
    ControlStdio,
    ControlStdin {
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
        submitted: bool,
        closed: bool,
        error: Option<String>,
    },
    ControlStdioSync {
        next_number: u64,
    },
    ControlWrite {
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
        idle: Option<(u64, u64)>,
        close: bool,
    },
    ControlStdioClosed,
    TtyInput {
        handoff: u64,
    },
    TtyInputStarted {
        handoff: u64,
    },
    TtyInputBytes {
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    TtyInputReady {
        received: u64,
        pane: Option<PaneId>,
    },
    TtyInputRelease {
        handoff: u64,
    },
    TtyInputClosed {
        handoff: u64,
    },
    AgentAnswerQuestion {
        pane: PaneId,
        request_id: u64,
        #[serde(deserialize_with = "deserialize_agent_question_answers")]
        answers: Vec<AgentQuestionAnswer>,
    },
    AgentStopTask {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_option_text")]
        task_id: String,
    },
    Ping {
        token: u64,
        rtt_us: u32,
    },
    Pong {
        token: u64,
    },
    FetchArtifact {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_artifact_token")]
        token: String,
        #[serde(deserialize_with = "deserialize_artifact_path")]
        path: String,
    },
    ArtifactBegin {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_artifact_mime")]
        mime: String,
        total_bytes: u32,
        reply: bool,
    },
    ArtifactChunk {
        request_id: u64,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_artifact_chunk"
        )]
        bytes: Vec<u8>,
    },
    ArtifactUnavailable {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_artifact_path")]
        reason: String,
    },
    ArtifactReply {
        #[serde(deserialize_with = "deserialize_artifact_token")]
        token: String,
        #[serde(deserialize_with = "deserialize_artifact_reply")]
        text: String,
    },
    AgentSteer {
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_agent_prompt_text")]
        text: String,
    },
    AgentLogin(crate::AgentLoginRequest),
    TerminalThemesRequest {
        request_id: u64,
    },
    TerminalThemesResponse {
        request_id: u64,
        #[serde(deserialize_with = "crate::terminal_theme::deserialize_terminal_themes")]
        themes: Vec<crate::TerminalTheme>,
    },
    BrowserScreenshot {
        request_id: u64,
        last: bool,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_browser_screenshot"
        )]
        png: Vec<u8>,
    },
    FileReadRequest {
        request_id: u64,
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_file_edit_path")]
        path: String,
    },
    FileReadResponse {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_file_read_result")]
        result: Result<String, String>,
    },
    FileWriteRequest {
        request_id: u64,
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_file_edit_path")]
        path: String,
        offset: u64,
        last: bool,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_file_write_chunk"
        )]
        bytes: Vec<u8>,
    },
    FileWriteResponse {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_file_write_result")]
        result: Result<(), String>,
    },
    IdentityChallenge {
        nonce: [u8; 32],
    },
    IdentityProof {
        key: [u8; 32],
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_identity_signature"
        )]
        signature: Vec<u8>,
    },
    AgentAttachFile {
        request_id: u64,
        pane: PaneId,
        #[serde(deserialize_with = "deserialize_attachment_name")]
        name: String,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_attachment_bytes"
        )]
        bytes: Vec<u8>,
    },
    AgentFileAttached {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_attachment_result")]
        result: Result<String, String>,
    },
    AgentUnwatch {
        pane: PaneId,
    },
}

impl ProtocolMessage {
    #[must_use]
    pub const fn required_capability(&self) -> Option<&'static str> {
        match self {
            Self::Event(event) => event.payload.required_capability(),
            Self::ClientHello(_)
            | Self::ServerHello(_)
            | Self::CommandRequest(_)
            | Self::CommandResponse(_)
            | Self::Attach { .. }
            | Self::Attached { .. }
            | Self::Detach
            | Self::SetColorScheme(_)
            | Self::SetConfigOverrides { .. }
            | Self::Input(_)
            | Self::GuiResponse(_)
            | Self::Resync
            | Self::RequestFull { .. }
            | Self::HistoryRequest { .. }
            | Self::PasteUploadBegin { .. }
            | Self::PasteUploadChunk { .. }
            | Self::FetchPastedImage { .. }
            | Self::PastedImageBegin { .. }
            | Self::PastedImageChunk { .. }
            | Self::PastedImageUnavailable { .. }
            | Self::AgentPrompt { .. }
            | Self::AgentCancel { .. }
            | Self::AgentUnqueue { .. }
            | Self::AgentRespondPermission { .. }
            | Self::AgentSetConfigOption { .. }
            | Self::AgentSetMode { .. }
            | Self::AgentAuthenticate { .. }
            | Self::AgentSessionOp { .. }
            | Self::AgentReplay { .. }
            | Self::AgentAcknowledgePromptRestore { .. }
            | Self::PrepareCommandList { .. }
            | Self::PreparedCommandList { .. }
            | Self::SetTerminalPreview { .. }
            | Self::ClientFileRequest(_)
            | Self::ClientFileResponse(_)
            | Self::HomeDirectoryRequest { .. }
            | Self::HomeDirectoryResponse { .. }
            | Self::CommandQueueParked { .. }
            | Self::EnvironmentRequest { .. }
            | Self::EnvironmentResponse { .. }
            | Self::ClientTerminalFeatures { .. }
            | Self::ClientTerminalType { .. }
            | Self::PathListRequest { .. }
            | Self::PathListBegin { .. }
            | Self::PathListChunk { .. }
            | Self::PathListGit { .. }
            | Self::PathListCancel { .. }
            | Self::Exec(_)
            | Self::ExecExit(_)
            | Self::Hello(_)
            | Self::Welcome(_)
            | Self::Batch(_)
            | Self::TreeSync
            | Self::GetKeyTables
            | Self::ControlStdio
            | Self::ControlStdin { .. }
            | Self::ControlStdioSync { .. }
            | Self::ControlWrite { .. }
            | Self::ControlStdioClosed
            | Self::TtyInput { .. }
            | Self::TtyInputStarted { .. }
            | Self::TtyInputBytes { .. }
            | Self::TtyInputReady { .. }
            | Self::TtyInputRelease { .. }
            | Self::TtyInputClosed { .. }
            | Self::AgentAnswerQuestion { .. }
            | Self::AgentStopTask { .. }
            | Self::Ping { .. }
            | Self::Pong { .. }
            | Self::FetchArtifact { .. }
            | Self::ArtifactBegin { .. }
            | Self::ArtifactChunk { .. }
            | Self::ArtifactUnavailable { .. }
            | Self::ArtifactReply { .. }
            | Self::AgentSteer { .. } => None,
            Self::AgentLogin(_) => Some(crate::AGENT_LOGIN_CAPABILITY),
            Self::TerminalThemesRequest { .. } | Self::TerminalThemesResponse { .. } => {
                Some(crate::TERMINAL_THEMES_CAPABILITY)
            }
            Self::BrowserScreenshot { .. } => Some(crate::BROWSER_SCREENSHOT_CAPABILITY),
            Self::FileReadRequest { .. }
            | Self::FileReadResponse { .. }
            | Self::FileWriteRequest { .. }
            | Self::FileWriteResponse { .. } => Some(crate::FILE_EDIT_CAPABILITY),
            Self::IdentityChallenge { .. } | Self::IdentityProof { .. } => {
                Some(crate::DAEMON_IDENTITY_CAPABILITY)
            }
            Self::AgentAttachFile { .. } | Self::AgentFileAttached { .. } => {
                Some(crate::AGENT_FILES_CAPABILITY)
            }
            Self::AgentUnwatch { .. } => Some(crate::AGENT_UNWATCH_CAPABILITY),
        }
    }
}

pub const IDENTITY_SIGNATURE_BYTES: usize = 64;

fn deserialize_identity_signature<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() != IDENTITY_SIGNATURE_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a 64-byte identity signature",
        ));
    }
    Ok(bytes)
}

fn deserialize_file_edit_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, crate::MAX_EDITOR_PATH_BYTES)
}

fn deserialize_file_write_chunk<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_FILE_WRITE_CHUNK_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a file write chunk within the wire byte limit",
        ));
    }
    Ok(bytes)
}

fn bounded_file_edit_error<E: serde::de::Error>(error: &str) -> Result<(), E> {
    if error.len() > MAX_FILE_EDIT_ERROR_BYTES {
        return Err(E::invalid_length(
            error.len(),
            &"a file error within the wire byte limit",
        ));
    }
    Ok(())
}

fn deserialize_file_read_result<'de, D>(deserializer: D) -> Result<Result<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let result = Result::<String, String>::deserialize(deserializer)?;
    match &result {
        Ok(text) if text.len() > MAX_FILE_EDIT_BYTES => Err(D::Error::invalid_length(
            text.len(),
            &"file contents within the wire byte limit",
        )),
        Ok(_) => Ok(result),
        Err(error) => bounded_file_edit_error(error).map(|()| result),
    }
}

pub const MAX_ATTACHMENT_NAME_BYTES: usize = 255;

fn deserialize_attachment_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_ATTACHMENT_NAME_BYTES)
}

fn deserialize_attachment_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_AGENT_PROMPT_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"an attachment within the prompt byte limit",
        ));
    }
    Ok(bytes)
}

fn deserialize_attachment_result<'de, D>(
    deserializer: D,
) -> Result<Result<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let result = Result::<String, String>::deserialize(deserializer)?;
    match &result {
        Ok(path) if path.len() > MAX_FILE_EDIT_ERROR_BYTES => Err(D::Error::invalid_length(
            path.len(),
            &"an attachment path within the wire byte limit",
        )),
        Ok(_) => Ok(result),
        Err(error) => bounded_file_edit_error(error).map(|()| result),
    }
}

fn deserialize_file_write_result<'de, D>(deserializer: D) -> Result<Result<(), String>, D::Error>
where
    D: Deserializer<'de>,
{
    let result = Result::<(), String>::deserialize(deserializer)?;
    if let Err(error) = &result {
        bounded_file_edit_error(error)?;
    }
    Ok(result)
}

fn deserialize_client_terminal_type<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let term_type = String::deserialize(deserializer)?;
    if term_type.len() > MAX_CLIENT_TERMINAL_TYPE_BYTES {
        return Err(D::Error::invalid_length(
            term_type.len(),
            &"a terminal type within the wire entry limit",
        ));
    }
    Ok(term_type)
}

fn deserialize_client_terminal_features<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let features = Vec::<String>::deserialize(deserializer)?;
    if features.len() > MAX_CLIENT_TERMINAL_FEATURES {
        return Err(D::Error::invalid_length(
            features.len(),
            &"a terminal feature batch within the wire entry limit",
        ));
    }
    if let Some(feature) = features
        .iter()
        .find(|feature| feature.len() > MAX_CLIENT_TERMINAL_FEATURE_BYTES)
    {
        return Err(D::Error::invalid_length(
            feature.len(),
            &"a terminal feature name within the wire byte limit",
        ));
    }
    Ok(features)
}

fn deserialize_environment_names<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let names = Vec::<String>::deserialize(deserializer)?;
    if names.len() > MAX_ENVIRONMENT_NAMES {
        return Err(D::Error::invalid_length(
            names.len(),
            &"an environment batch within the wire entry limit",
        ));
    }
    if let Some(name) = names
        .iter()
        .find(|name| name.len() > MAX_ENVIRONMENT_NAME_BYTES)
    {
        return Err(D::Error::invalid_length(
            name.len(),
            &"a variable name within the wire byte limit",
        ));
    }
    Ok(names)
}

fn deserialize_environment_values<'de, D>(deserializer: D) -> Result<Vec<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<Option<String>>::deserialize(deserializer)?;
    if values.len() > MAX_ENVIRONMENT_NAMES {
        return Err(D::Error::invalid_length(
            values.len(),
            &"an environment batch within the wire entry limit",
        ));
    }
    if let Some(value) = values
        .iter()
        .flatten()
        .find(|value| value.len() > MAX_ENVIRONMENT_VALUE_BYTES)
    {
        return Err(D::Error::invalid_length(
            value.len(),
            &"an environment value within the wire byte limit",
        ));
    }
    Ok(values)
}

fn deserialize_home_directory_users<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let users = Vec::<String>::deserialize(deserializer)?;
    if users.len() > MAX_HOME_DIRECTORY_USERS {
        return Err(D::Error::invalid_length(
            users.len(),
            &"a home directory batch within the wire entry limit",
        ));
    }
    if let Some(user) = users
        .iter()
        .find(|user| user.len() > MAX_HOME_DIRECTORY_USER_BYTES)
    {
        return Err(D::Error::invalid_length(
            user.len(),
            &"a user name within the wire byte limit",
        ));
    }
    Ok(users)
}

fn deserialize_home_directories<'de, D>(deserializer: D) -> Result<Vec<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let homes = Vec::<Option<String>>::deserialize(deserializer)?;
    if homes.len() > MAX_HOME_DIRECTORY_USERS {
        return Err(D::Error::invalid_length(
            homes.len(),
            &"a home directory batch within the wire entry limit",
        ));
    }
    if let Some(home) = homes
        .iter()
        .flatten()
        .find(|home| home.len() > MAX_HOME_DIRECTORY_BYTES)
    {
        return Err(D::Error::invalid_length(
            home.len(),
            &"a home directory within the wire byte limit",
        ));
    }
    Ok(homes)
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
