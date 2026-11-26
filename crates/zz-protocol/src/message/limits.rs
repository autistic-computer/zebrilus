use super::*;

pub const NEW_SESSION_ATTACH_CAPABILITY: &str = "new-session-attach-v1";
pub const PANE_FRAME_CAPABILITY: &str = "pane-frame-v1";
pub const CLIENT_TERMINAL_CAPABILITY: &str = "client-terminal-v1";
pub const CLIENT_NESTED_CAPABILITY: &str = "client-nested-v1";
pub const CLIENT_TTY_CAPABILITY_PREFIX: &str = "client-tty-v1:";
pub const CLIENT_SIZE_CAPABILITY_PREFIX: &str = "client-size-v1:";
pub const CLIENT_CELL_CAPABILITY_PREFIX: &str = "client-cell-v1:";
pub const SERVER_OPTION_CAPABILITY_PREFIX: &str = "server-option-v1:";
pub const CLIENT_FEATURES_CAPABILITY_PREFIX: &str = "client-features-v1:";
pub const MAX_CLIENT_TERMINAL_FEATURES: usize = 64;
pub const MAX_CLIENT_TERMINAL_FEATURE_BYTES: usize = 64;
pub const MAX_CLIENT_TERMINAL_TYPE_BYTES: usize = 128;
pub const SPLIT_RATIO_BASIS: u16 = 10_000;
pub const MAX_COMMAND_PROMPT_BYTES: usize = 64 * 1024;
pub const MAX_CHOOSE_TREE_QUERY_BYTES: usize = 4 * 1024;
pub const MAX_CHOOSE_BUFFER_QUERY_BYTES: usize = 4 * 1024;
pub const MAX_BROWSER_KEY_REPEAT: u32 = 9_999;
pub const MAX_STATUS_TEXT_BYTES: usize = 4096;
pub const MAX_STATUS_ROWS: usize = 5;
pub const MAX_PANE_INDICATOR_LABEL_BYTES: usize = 1024;
pub const MAX_CHOOSE_ITEM_KEY_BYTES: usize = 64;
pub const MAX_CHOOSE_ITEM_TEXT_BYTES: usize = 512;
pub const MAX_AGENT_SEND_BYTES: usize = 1024 * 1024;
pub const MAX_GUI_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_CLIENT_WORKING_DIRECTORY_BYTES: usize = 16 * 1024;
pub const MAX_CLIENT_ENVIRONMENT_ENTRIES: usize = 4096;
pub const MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES: usize = 16_367;
pub const MAX_CLIENT_ENVIRONMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_HOME_DIRECTORY_USERS: usize = 1024;
pub const MAX_HOME_DIRECTORY_USER_BYTES: usize = 1024;
pub const MAX_HOME_DIRECTORY_BYTES: usize = 16 * 1024;
pub const MAX_ENVIRONMENT_NAMES: usize = 1024;
pub const MAX_ENVIRONMENT_NAME_BYTES: usize = 1024;
pub const MAX_ENVIRONMENT_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_CLIENT_FILE_PATH_BYTES: usize = 16 * 1024;
pub const MAX_CLIENT_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_BROWSER_SCREENSHOT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_BROWSER_SCREENSHOT_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_FILE_EDIT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FILE_EDIT_ERROR_BYTES: usize = 4096;
pub const MAX_FILE_WRITE_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_STARTUP_CONFIG_CAUSES: usize = 1024;
pub const MAX_STARTUP_CONFIG_CAUSE_BYTES: usize = 64 * 1024;
pub const MAX_STARTUP_CONFIG_CAUSES_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_PROMPT_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_AGENT_PROMPT_IMAGES: usize = 64;
pub const MAX_AGENT_QUEUED_PROMPTS: usize = 4;
pub const MAX_AGENT_AUTH_METHODS: usize = 32;
pub const MAX_AGENT_PERMISSION_OPTIONS: usize = 32;
pub const MAX_AGENT_AVAILABLE_COMMANDS: usize = 256;
pub const MAX_AGENT_CONFIG_OPTIONS: usize = 64;
pub const MAX_AGENT_CONFIG_CHOICES: usize = 128;
pub const MAX_AGENT_MODES: usize = 64;
pub const MAX_AGENT_TOOL_CONTENT_ITEMS: usize = 128;
pub const MAX_AGENT_IMAGE_FORMAT_BYTES: usize = 64;
pub const MAX_AGENT_OPTION_BYTES: usize = 4 * 1024;
pub const MAX_AGENT_SESSION_ID_BYTES: usize = 16 * 1024;
pub const MAX_AGENT_SESSION_DIRECTORIES: usize = 256;
pub const MAX_AGENT_UPDATES_BYTES: usize = 9 * 1024 * 1024;
pub const MAX_AGENT_STATE_BLOB_BYTES: usize = 256 * 1024;
pub const MAX_AGENT_PERMISSION_BYTES: usize = 64 * 1024;
pub const MAX_AGENT_TASKS: usize = 64;
pub const MAX_AGENT_QUESTION_ANSWERS: usize = 32;
pub const MAX_AGENT_ANSWER_BYTES: usize = 16 * 1024;
pub const MAX_AGENT_RESULT_BYTES: usize = 1024 * 1024;
pub const MAX_PASTE_UPLOAD_BYTES: u32 = 6 * 1024 * 1024;
pub const MAX_PASTE_UPLOAD_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACT_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_ARTIFACT_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACT_TOKEN_BYTES: usize = 64;
pub const MAX_ARTIFACT_PATH_BYTES: usize = 4096;
pub const MAX_ARTIFACT_MIME_BYTES: usize = 128;
pub const MAX_ARTIFACT_REPLY_BYTES: usize = 64 * 1024;
pub const MAX_KITTY_IMAGE_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_KITTY_IMAGE_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_KITTY_IMAGE_REMOVALS: usize = 512;
pub const MAX_PASTE_UPLOAD_EXTENSION_BYTES: usize = 8;
pub(crate) const MAX_SERVER_CAPABILITIES: usize = 64;
pub(crate) const MAX_SERVER_CAPABILITY_BYTES: usize = 256;
pub(crate) const MAX_DEVICE_NAME_BYTES: usize = 256;
pub(crate) const MAX_CONFIG_OVERRIDE_ENTRIES: usize = 1024;
pub(crate) const MAX_CONFIG_OVERRIDE_KEY_BYTES: usize = 128;
pub(crate) const MAX_CONFIG_OVERRIDE_VALUE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_MUX_OPTION_VALUE_BYTES: usize = 64 * 1024;

pub const MAX_AGENT_COMMAND_BYTES: usize = 4 * 1024;

pub(crate) fn serialize_byte_vecs<S>(items: &[Vec<u8>], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.collect_seq(items.iter().map(|item| serde_bytes::Bytes::new(item)))
}

pub(crate) fn deserialize_byte_vecs<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<serde_bytes::ByteBuf>::deserialize(deserializer).map(|items| {
        items
            .into_iter()
            .map(serde_bytes::ByteBuf::into_vec)
            .collect()
    })
}

pub(crate) fn deserialize_bounded_text<'de, D>(
    deserializer: D,
    limit: usize,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    if text.len() > limit {
        return Err(D::Error::invalid_length(
            text.len(),
            &"text within the wire byte limit",
        ));
    }
    Ok(text)
}

pub(crate) fn deserialize_bounded_optional_text<'de, D>(
    deserializer: D,
    limit: usize,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let text = Option::<String>::deserialize(deserializer)?;
    if let Some(text) = &text
        && text.len() > limit
    {
        return Err(D::Error::invalid_length(
            text.len(),
            &"text within the wire byte limit",
        ));
    }
    Ok(text)
}
