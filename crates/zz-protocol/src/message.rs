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

pub const PROTOCOL_VERSION: u16 = 125;
pub const REVISION: u32 = 11;
pub const MIN_DAEMON_REVISION: u32 = 1;
pub const MIN_CLIENT_REVISION: u32 = 1;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientKind {
    Interactive,
    Command,
    Control,
}

fn deserialize_gui_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_GUI_TEXT_BYTES)
}

fn deserialize_agent_prompt_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_PROMPT_BYTES)
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

fn deserialize_agent_option_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_AGENT_OPTION_BYTES)
}

fn deserialize_optional_agent_option_text<'de, D>(
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

#[must_use]
pub fn agent_update_batch_bytes(items: &[Vec<u8>]) -> usize {
    items
        .iter()
        .fold(0_usize, |total, item| total.saturating_add(item.len()))
}

#[must_use]
pub fn paste_upload_extension_is_valid(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= MAX_PASTE_UPLOAD_EXTENSION_BYTES
        && extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteUploadPurpose {
    PastePath,
    RecordPastedImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PastedImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl PastedImageFormat {
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            _ => return None,
        })
    }
}

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

fn deserialize_client_file_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CLIENT_FILE_PATH_BYTES)
}

fn deserialize_client_file_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_CLIENT_FILE_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a client file payload within the wire byte limit",
        ));
    }
    Ok(bytes)
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

#[derive(Clone, Default)]
pub struct RawText {
    text: String,
    bytes: Option<Vec<u8>>,
}

impl RawText {
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        match String::from_utf8(bytes) {
            Ok(text) => Self { text, bytes: None },
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Self {
                    text,
                    bytes: Some(bytes),
                }
            }
        }
    }

    #[cfg(unix)]
    #[must_use]
    pub fn from_os_str(value: &std::ffi::OsStr) -> Self {
        use std::os::unix::ffi::OsStrExt as _;

        Self::from_bytes(value.as_bytes())
    }

    #[cfg(not(unix))]
    #[must_use]
    pub fn from_os_str(value: &std::ffi::OsStr) -> Self {
        Self::from(value.to_string_lossy().into_owned())
    }

    #[cfg(unix)]
    #[must_use]
    pub fn to_os_string(&self) -> std::ffi::OsString {
        use std::os::unix::ffi::OsStringExt as _;

        std::ffi::OsString::from_vec(self.as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    #[must_use]
    pub fn to_os_string(&self) -> std::ffi::OsString {
        std::ffi::OsString::from(self.text.clone())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_deref().unwrap_or(self.text.as_bytes())
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.unwrap_or_else(|| self.text.into_bytes())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn to_utf8(&self) -> Option<&str> {
        self.bytes.is_none().then_some(self.text.as_str())
    }

    #[must_use]
    pub fn is_utf8(&self) -> bool {
        self.bytes.is_none()
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.as_bytes().len()
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let mut current = std::mem::take(self).into_bytes();
        current.extend_from_slice(bytes);
        *self = Self::from_bytes(current);
    }

    #[must_use]
    pub fn join(parts: &[Self], separator: &[u8]) -> Self {
        let mut bytes = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            if index != 0 {
                bytes.extend_from_slice(separator);
            }
            bytes.extend_from_slice(part.as_bytes());
        }
        Self::from_bytes(bytes)
    }

    #[must_use]
    pub fn split_once_byte(&self, separator: u8) -> Option<(Self, Self)> {
        let bytes = self.as_bytes();
        let position = bytes.iter().position(|byte| *byte == separator)?;
        Some((
            Self::from_bytes(&bytes[..position]),
            Self::from_bytes(&bytes[position + 1..]),
        ))
    }
}

impl std::ops::Deref for RawText {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl AsRef<str> for RawText {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl AsRef<[u8]> for RawText {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for RawText {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for RawText {}

impl PartialOrd for RawText {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RawText {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.text
            .cmp(&other.text)
            .then_with(|| self.as_bytes().cmp(other.as_bytes()))
    }
}

impl std::hash::Hash for RawText {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl PartialEq<str> for RawText {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<&str> for RawText {
    fn eq(&self, other: &&str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<String> for RawText {
    fn eq(&self, other: &String) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for str {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for &str {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for String {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl From<String> for RawText {
    fn from(value: String) -> Self {
        Self {
            text: value,
            bytes: None,
        }
    }
}

impl From<&String> for RawText {
    fn from(value: &String) -> Self {
        Self::from(value.clone())
    }
}

impl From<&str> for RawText {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<&RawText> for RawText {
    fn from(value: &RawText) -> Self {
        value.clone()
    }
}

impl From<Vec<u8>> for RawText {
    fn from(value: Vec<u8>) -> Self {
        Self::from_bytes(value)
    }
}

impl From<&[u8]> for RawText {
    fn from(value: &[u8]) -> Self {
        Self::from_bytes(value)
    }
}

impl From<RawText> for String {
    fn from(value: RawText) -> Self {
        value.text
    }
}

impl fmt::Display for RawText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

impl fmt::Debug for RawText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.bytes {
            Some(bytes) => write!(formatter, "{:?}<{} bytes>", self.text, bytes.len()),
            None => fmt::Debug::fmt(&self.text, formatter),
        }
    }
}

impl std::borrow::Borrow<str> for RawText {
    fn borrow(&self) -> &str {
        &self.text
    }
}

impl FromIterator<RawText> for String {
    fn from_iter<I: IntoIterator<Item = RawText>>(iterator: I) -> Self {
        iterator.into_iter().map(String::from).collect()
    }
}

impl Serialize for RawText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.as_bytes())
    }
}

impl<'de> Deserialize<'de> for RawText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from_bytes(
            serde_bytes::ByteBuf::deserialize(deserializer)?.into_vec(),
        ))
    }
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

struct BoundedClientEnvironmentEntry(RawText);

impl<'de> Deserialize<'de> for BoundedClientEnvironmentEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entry = RawText::deserialize(deserializer)?;
        if !client_environment_entry_is_valid(&entry) {
            return Err(D::Error::custom("invalid client environment entry"));
        }
        Ok(BoundedClientEnvironmentEntry(entry))
    }
}

fn client_environment_entry_is_valid(entry: &RawText) -> bool {
    let bytes = entry.as_bytes();
    bytes.len() <= MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES
        && !bytes.contains(&0)
        && bytes
            .iter()
            .position(|byte| *byte == b'=')
            .is_some_and(|separator| separator > 0)
}

pub(crate) fn client_environment_is_valid(environment: &[RawText]) -> bool {
    environment.len() <= MAX_CLIENT_ENVIRONMENT_ENTRIES
        && environment.iter().all(client_environment_entry_is_valid)
        && environment
            .iter()
            .try_fold(0usize, |total, entry| total.checked_add(entry.byte_len()))
            .is_some_and(|total| total <= MAX_CLIENT_ENVIRONMENT_BYTES)
}

fn deserialize_client_environment<'de, D>(deserializer: D) -> Result<Vec<RawText>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ClientEnvironmentVisitor;

    impl<'de> Visitor<'de> for ClientEnvironmentVisitor {
        type Value = Vec<RawText>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {MAX_CLIENT_ENVIRONMENT_ENTRIES} environment entries totaling at most \
                 {MAX_CLIENT_ENVIRONMENT_BYTES} bytes"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let hint = sequence.size_hint();
            if hint.is_some_and(|length| length > MAX_CLIENT_ENVIRONMENT_ENTRIES) {
                return Err(A::Error::invalid_length(
                    hint.unwrap_or(MAX_CLIENT_ENVIRONMENT_ENTRIES.saturating_add(1)),
                    &self,
                ));
            }
            let mut environment =
                Vec::with_capacity(hint.unwrap_or(0).min(MAX_CLIENT_ENVIRONMENT_ENTRIES));
            let mut total_bytes = 0usize;
            while environment.len() < MAX_CLIENT_ENVIRONMENT_ENTRIES {
                let Some(entry) = sequence.next_element::<BoundedClientEnvironmentEntry>()? else {
                    return Ok(environment);
                };
                total_bytes = total_bytes
                    .checked_add(entry.0.byte_len())
                    .ok_or_else(|| A::Error::invalid_length(usize::MAX, &self))?;
                if total_bytes > MAX_CLIENT_ENVIRONMENT_BYTES {
                    return Err(A::Error::invalid_length(total_bytes, &self));
                }
                environment.push(entry.0);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::invalid_length(
                    MAX_CLIENT_ENVIRONMENT_ENTRIES.saturating_add(1),
                    &self,
                ));
            }
            Ok(environment)
        }
    }

    deserializer.deserialize_seq(ClientEnvironmentVisitor)
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub client_instance_id: ClientInstanceId,
    pub kind: ClientKind,
    #[serde(deserialize_with = "deserialize_device_name")]
    pub device_name: Option<String>,
    #[serde(deserialize_with = "deserialize_capabilities")]
    pub capabilities: Vec<String>,
    pub color_scheme: Option<TerminalColorScheme>,
    pub origin: Option<PaneId>,
    pub working_directory: Option<ClientPath>,
    #[serde(deserialize_with = "deserialize_client_environment")]
    pub environment: Vec<RawText>,
    pub process_id: u32,
    pub revision: u32,
}

impl fmt::Debug for ClientHello {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientHello")
            .field("protocol_version", &self.protocol_version)
            .field("client_instance_id", &self.client_instance_id)
            .field("kind", &self.kind)
            .field("device_name", &self.device_name)
            .field("capabilities", &self.capabilities)
            .field("color_scheme", &self.color_scheme)
            .field("origin", &self.origin)
            .field("working_directory", &self.working_directory)
            .field("environment_entries", &self.environment.len())
            .field("process_id", &self.process_id)
            .field("revision", &self.revision)
            .finish()
    }
}

impl ClientHello {
    pub const CLIENT_TERMINAL_CAPABILITY: &'static str = CLIENT_TERMINAL_CAPABILITY;
    pub const CLIENT_NATIVE_TERMINAL_SEARCH_CAPABILITY: &'static str =
        "client-native-terminal-search-v1";
    pub const CLIENT_NATIVE_CHOOSER_CAPABILITY: &'static str = "client-native-chooser-v1";
    pub const CLIENT_PATH_PICKER_CAPABILITY: &'static str = "client-path-picker-v1";
    pub const CLIENT_NESTED_CAPABILITY: &'static str = CLIENT_NESTED_CAPABILITY;
    pub const CLIENT_REMOTE_CAPABILITY: &'static str = "client-remote-v1";
    pub const CLIENT_TTY_CAPABILITY_PREFIX: &'static str = CLIENT_TTY_CAPABILITY_PREFIX;
    pub const CLIENT_SIZE_CAPABILITY_PREFIX: &'static str = CLIENT_SIZE_CAPABILITY_PREFIX;
    pub const CLIENT_CELL_CAPABILITY_PREFIX: &'static str = CLIENT_CELL_CAPABILITY_PREFIX;
    pub const CLIENT_FEATURES_CAPABILITY_PREFIX: &'static str = CLIENT_FEATURES_CAPABILITY_PREFIX;
    pub const STARTUP_CONFIG_OWNER_CAPABILITY: &'static str = "startup-config-owner-v1";
    pub const CLIENT_UTF8_CAPABILITY: &'static str = "client-utf8-v1";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerHello {
    pub protocol_version: u16,
    pub server_id: u64,
    pub client_id: ClientId,
    pub client_instance_id: ClientInstanceId,
    #[serde(deserialize_with = "deserialize_capabilities")]
    pub capabilities: Vec<String>,
    pub appearance: TerminalAppearance,
    pub appearance_provenance: AppearanceProvenance,
    pub mux_options: MuxOptions,
    pub status: StatusLine,
    pub key_tables: Vec<KeyTableSnapshot>,
    #[serde(default)]
    pub clock: u64,
    #[serde(default)]
    pub platform: crate::HostPlatform,
    #[serde(default)]
    pub revision: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyTableSnapshot {
    pub name: String,
    pub bindings: Vec<KeyBindingSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBindingSnapshot {
    pub key: String,
    pub commands: Vec<CommandInvocation>,
    pub repeat: bool,
    pub note: Option<String>,
}

struct BoundedCapability(String);

impl<'de> Deserialize<'de> for BoundedCapability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CapabilityVisitor;

        impl<'de> Visitor<'de> for CapabilityVisitor {
            type Value = BoundedCapability;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "a capability no longer than {MAX_SERVER_CAPABILITY_BYTES} bytes"
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
                if value.len() > MAX_SERVER_CAPABILITY_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedCapability(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.len() > MAX_SERVER_CAPABILITY_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedCapability(value))
            }
        }

        deserializer.deserialize_str(CapabilityVisitor)
    }
}

fn deserialize_device_name<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let name = Option::<String>::deserialize(deserializer)?;
    if let Some(name) = &name
        && name.len() > MAX_DEVICE_NAME_BYTES
    {
        return Err(serde::de::Error::invalid_length(
            name.len(),
            &"a device name no longer than 256 bytes",
        ));
    }
    Ok(name)
}

fn deserialize_capabilities<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct CapabilitiesVisitor;

    impl<'de> Visitor<'de> for CapabilitiesVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {MAX_SERVER_CAPABILITIES} bounded capabilities"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let hint = sequence.size_hint();
            if hint.is_some_and(|length| length > MAX_SERVER_CAPABILITIES) {
                return Err(A::Error::invalid_length(
                    hint.unwrap_or(MAX_SERVER_CAPABILITIES.saturating_add(1)),
                    &self,
                ));
            }
            let mut capabilities =
                Vec::with_capacity(hint.unwrap_or(0).min(MAX_SERVER_CAPABILITIES));
            while capabilities.len() < MAX_SERVER_CAPABILITIES {
                let Some(capability) = sequence.next_element::<BoundedCapability>()? else {
                    return Ok(capabilities);
                };
                capabilities.push(capability.0);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::invalid_length(
                    MAX_SERVER_CAPABILITIES.saturating_add(1),
                    &self,
                ));
            }
            Ok(capabilities)
        }
    }

    deserializer.deserialize_seq(CapabilitiesVisitor)
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyToken {
    Literal(String),
    Named(String),
    Raw(u8),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMessage {
    Text {
        pane: PaneId,
        text: String,
    },
    Key {
        pane: PaneId,
        input: KeyInput,
        text_follows: bool,
    },
    BrowserSurfaceText {
        pane: PaneId,
        text: String,
    },
    BrowserSurfaceKey {
        pane: PaneId,
        input: KeyInput,
        text_follows: bool,
    },
    ResizeTerminal {
        pane: PaneId,
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    },
    TerminalView {
        pane: PaneId,
        action: TerminalViewAction,
    },
    ResizeCommandOutput {
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    },
    CommandOutputView {
        action: TerminalViewAction,
    },
    ChooseTree {
        action: ChooseTreeAction,
    },
    ChooseBuffer {
        action: ChooseBufferAction,
    },
    DisplayPanes {
        action: DisplayPanesAction,
    },
    CommandPrompt {
        action: CommandPromptAction,
    },
    ResizeSplit {
        window: WindowId,
        split: SplitId,
        ratio_basis_points: u16,
    },
    CancelPrefix {
        request_id: u64,
    },
    #[serde(rename = "Popup")]
    RetiredPopup {
        action: PopupAction,
    },
    Menu {
        action: MenuAction,
    },
    Confirm {
        action: ConfirmAction,
    },
    ClientTerminalSize {
        columns: u16,
        rows: u16,
    },
    ClientFocus {
        focused: bool,
    },
    DismissClientMessage,
    MouseKey {
        key: String,
        pane: Option<PaneId>,
        window: Option<WindowId>,
        column: u16,
        row: u16,
        border: Option<Axis>,
        view_action: Option<TerminalViewAction>,
        press_action: Option<TerminalViewAction>,
        #[serde(default)]
        status_range_start: Option<u16>,
        #[serde(default)]
        press: Option<(u16, u16)>,
    },
    ClientSuspendState {
        suspended: bool,
    },
    ResizeTerminalV2 {
        pane: PaneId,
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
        layout_generation: u64,
    },
    ClientTerminalSizeV2 {
        columns: u16,
        rows: u16,
        layout_generation: u64,
    },
    ClipboardReply {
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptAction {
    Update { input: String, cursor: u32 },
    Submit { input: String },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserCommand {
    Navigate(String),
    Reload,
    Back,
    Forward,
    SendKeys(Vec<KeyToken>),
    Key(KeyInput),
    Screenshot {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        path: String,
    },
    SendKeysRepeated {
        keys: Vec<KeyToken>,
        count: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatorButton {
    Home,
    Lock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatorCommand {
    Screenshot {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        path: String,
    },
    Tap {
        request_id: u64,
        x: u32,
        y: u32,
    },
    Swipe {
        request_id: u64,
        from: (u32, u32),
        to: (u32, u32),
    },
    Type {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        text: String,
    },
    Press {
        request_id: u64,
        button: SimulatorButton,
    },
}

impl SimulatorCommand {
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Screenshot { request_id, .. }
            | Self::Tap { request_id, .. }
            | Self::Swipe { request_id, .. }
            | Self::Type { request_id, .. }
            | Self::Press { request_id, .. } => *request_id,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuiResponse {
    Success {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        output: String,
    },
    Error {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        message: String,
    },
}

impl GuiResponse {
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Success { request_id, .. } | Self::Error { request_id, .. } => *request_id,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let text = match self {
            Self::Success { output, .. } => output,
            Self::Error { message, .. } => message,
        };
        if text.len() > MAX_GUI_TEXT_BYTES {
            return Err("GUI response text exceeds the wire byte limit");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalUiCommand {
    BeginSearch { direction: SearchDirection },
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptKind {
    Command,
    Value,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptType {
    #[default]
    Command,
    Search,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptMode {
    #[default]
    Text,
    Single,
    Numeric,
    Incremental,
    Key,
    BackspaceExit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandPromptState {
    pub prompt: String,
    pub input: String,
    pub cursor: u32,
    pub kind: CommandPromptKind,
    pub history: Vec<String>,
    pub prompt_type: CommandPromptType,
    pub mode: CommandPromptMode,
    pub no_freeze: bool,
    pub pane: Option<PaneId>,
    pub command_mode: bool,
    pub prompt_cursor: PromptCursor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptCursor {
    pub style: u8,
    pub colour: Option<crate::TmuxColour>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreeKind {
    Windows,
    Panes,
    Clients,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreePaneKind {
    Terminal,
    Browser,
    Editor,
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChooseTreeTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
    Client(ClientId),
}

impl std::fmt::Display for ChooseTreeTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(id) => id.fmt(formatter),
            Self::Window(id) => id.fmt(formatter),
            Self::Pane(id) => id.fmt(formatter),
            Self::Client(id) => id.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeItem {
    pub label: String,
    pub detail: String,
    pub target: ChooseTreeTarget,
    pub depth: u8,
    pub flags: u8,
    pub pane_kind: Option<ChooseTreePaneKind>,
    #[serde(deserialize_with = "deserialize_choose_item_key")]
    pub key: String,
    #[serde(deserialize_with = "deserialize_choose_item_text")]
    pub text: String,
}

fn deserialize_choose_item_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CHOOSE_ITEM_KEY_BYTES)
}

fn deserialize_choose_item_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CHOOSE_ITEM_TEXT_BYTES)
}

impl ChooseTreeItem {
    pub const EXPANDED: u8 = 1 << 0;
    pub const HAS_CHILDREN: u8 = 1 << 1;
    pub const ACTIVE: u8 = 1 << 2;
    pub const TAGGED: u8 = 1 << 3;

    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.flags & Self::EXPANDED != 0
    }

    #[must_use]
    pub const fn has_children(&self) -> bool {
        self.flags & Self::HAS_CHILDREN != 0
    }

    #[must_use]
    pub const fn active(&self) -> bool {
        self.flags & Self::ACTIVE != 0
    }

    #[must_use]
    pub const fn tagged(&self) -> bool {
        self.flags & Self::TAGGED != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeSearchState {
    pub query: String,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeState {
    pub items: Vec<ChooseTreeItem>,
    pub search: Option<ChooseTreeSearchState>,
    pub selected: u32,
    pub kind: ChooseTreeKind,
    pub filter_no_matches: bool,
    #[serde(default, deserialize_with = "deserialize_choose_item_text")]
    pub prompt: String,
    #[serde(default)]
    pub help: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreeAction {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
    Collapse,
    Expand,
    Activate,
    Select(u32),
    ActivateIndex(u32),
    SearchStart { reverse: bool },
    SearchAppend(String),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    SearchNext { reverse: bool },
    Close,
    Key(KeyInput),
    Tag,
    TagNone,
    TagAll,
    KillCurrent,
    KillTagged,
    SwapUp,
    SwapDown,
    SortNext,
    SortReverse,
    Help,
    CollapseAll,
    ExpandAll,
    Mark,
    MarkClear,
    CommandPrompt,
    PreviewCycle,
    FilterPrompt,
    ClearFilter,
    ClientDetach,
    ClientDetachTagged,
    ClientInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferItem {
    pub name: String,
    pub preview: String,
    pub size_bytes: u64,
    pub created_unix_seconds: u64,
    #[serde(deserialize_with = "deserialize_choose_item_key")]
    pub key: String,
    #[serde(deserialize_with = "deserialize_choose_item_text")]
    pub text: String,
    #[serde(default)]
    pub tagged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferSearchState {
    pub query: String,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferState {
    pub items: Vec<ChooseBufferItem>,
    pub search: Option<ChooseBufferSearchState>,
    pub selected: u32,
    pub filter_no_matches: bool,
    #[serde(default)]
    pub help: bool,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChooserPresentation {
    pub selected: u32,
    pub rows: Vec<ChooserRow>,
    pub sort: String,
    pub view: String,
    pub filter: bool,
    pub selection_style: String,
    pub border_style: String,
    pub prompt_style: String,
    pub preview_size: ChooserPreviewSize,
    pub preview: Option<ChooserPreview>,
    #[serde(default)]
    pub prompt_cursor: PromptCursor,
    #[serde(default)]
    pub prompt_column: u16,
    #[serde(default)]
    pub prompt_line: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooserRow {
    pub name: String,
    pub text: String,
    pub align: bool,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooserPreviewSize {
    #[default]
    Normal,
    Off,
    Big,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChooserPreview {
    Tiles {
        tiles: Vec<ChooserPreviewTile>,
        current: u32,
    },
    Screen {
        #[serde(with = "crate::pane_frame::viewport_bytes")]
        viewport: TerminalViewport,
    },
    Text {
        lines: Vec<String>,
    },
    Client {
        #[serde(with = "crate::pane_frame::optional_viewport_bytes")]
        viewport: Option<TerminalViewport>,
        status: Vec<String>,
        status_style: String,
        status_width: u32,
        border_style: String,
    },
    Markup {
        lines: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChooserPreviewTile {
    pub label: String,
    pub label_style: String,
    pub border_style: String,
    #[serde(with = "crate::pane_frame::optional_viewport_bytes")]
    pub viewport: Option<TerminalViewport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseBufferAction {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
    Paste,
    Delete,
    Select(u32),
    PasteIndex(u32),
    SearchStart { reverse: bool },
    SearchAppend(String),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    SearchNext { reverse: bool },
    Close,
    Key(KeyInput),
    Tag,
    TagNone,
    TagAll,
    SortNext,
    SortReverse,
    Help,
    CollapseAll,
    ExpandAll,
    DeleteTagged,
    PasteTagged,
    PreviewCycle,
    FilterPrompt,
    ClearFilter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneIndicator {
    pub pane: PaneId,
    pub index: u32,
    pub select_key: u8,
    pub flags: u8,
    #[serde(deserialize_with = "deserialize_pane_indicator_label")]
    pub label: String,
}

impl PaneIndicator {
    pub const ACTIVE: u8 = 1 << 0;

    #[must_use]
    pub const fn active(&self) -> bool {
        self.flags & Self::ACTIVE != 0
    }

    #[must_use]
    pub const fn selection_key(&self) -> Option<char> {
        if self.select_key == 0 {
            None
        } else {
            Some(self.select_key as char)
        }
    }
}

fn deserialize_pane_indicator_label<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_PANE_INDICATOR_LABEL_BYTES)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayPanesState {
    pub window: WindowId,
    pub duration_ms: u32,
    pub indicators: Vec<PaneIndicator>,
    pub colour: Option<crate::TmuxColour>,
    pub active_colour: Option<crate::TmuxColour>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayPanesAction {
    Key(KeyInput),
    Select(PaneId),
    Close,
    Dismiss,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupBorderLines {
    #[default]
    Single,
    Double,
    Heavy,
    Simple,
    Rounded,
    Padded,
    None,
}

impl PopupBorderLines {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Double => "double",
            Self::Heavy => "heavy",
            Self::Simple => "simple",
            Self::Rounded => "rounded",
            Self::Padded => "padded",
            Self::None => "none",
        }
    }
}

impl std::str::FromStr for PopupBorderLines {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "single" => Ok(Self::Single),
            "double" => Ok(Self::Double),
            "heavy" => Ok(Self::Heavy),
            "simple" => Ok(Self::Simple),
            "rounded" => Ok(Self::Rounded),
            "padded" => Ok(Self::Padded),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupState {
    pub pane: PaneId,
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub client_columns: u16,
    pub client_rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub title: String,
    pub style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
    pub close_on_exit: bool,
    pub close_on_exit_zero: bool,
    pub close_on_any_key: bool,
    pub dead: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupAction {
    Text(String),
    Key {
        input: KeyInput,
        text_follows: bool,
    },
    TerminalView(TerminalViewAction),
    Close,
    Pointer {
        pointer: PopupPointer,
        view: Option<TerminalViewAction>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupPointer {
    pub column: u16,
    pub row: u16,
    pub button: PopupPointerButton,
    pub drag: bool,
    pub release: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupPointerButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    pub name: String,
    pub key: Option<String>,
    pub annotation: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuState {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub client_columns: u16,
    pub client_rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub title: String,
    pub style: String,
    pub selected_style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
    pub items: Vec<Option<MenuItem>>,
    pub selected: Option<u32>,
    pub stay_open: bool,
    #[serde(default)]
    pub mouse_keys: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuAction {
    Choose(u32),
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmState {
    pub prompt: String,
    pub confirm_key: u8,
    pub default_yes: bool,
    pub prompt_cursor: PromptCursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmAction {
    Reply(bool),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientFileOperation {
    Read,
    Write {
        append: bool,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_client_file_bytes"
        )]
        data: Vec<u8>,
    },
    ReadStdin {
        binary: bool,
    },
    ReadStdinChunk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientFileRequest {
    pub request_id: u64,
    #[serde(deserialize_with = "deserialize_client_file_path")]
    pub path: String,
    pub operation: ClientFileOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientFileResponse {
    pub request_id: u64,
    #[serde(
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "deserialize_client_file_bytes"
    )]
    pub data: Vec<u8>,
    #[serde(deserialize_with = "deserialize_optional_client_file_error")]
    pub error: Option<String>,
}

fn deserialize_optional_client_file_error<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let text = Option::<String>::deserialize(deserializer)?;
    match text {
        Some(text) if text.len() > MAX_GUI_TEXT_BYTES => Err(D::Error::invalid_length(
            text.len(),
            &"a client file error within the wire byte limit",
        )),
        text => Ok(text),
    }
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
