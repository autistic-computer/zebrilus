use super::*;

pub type ConfigOverrideEntry = (String, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MuxOptionKey {
    Prefix,
    ModeKeys,
    HistoryLimit,
    WordSeparators,
    CopyCommand,
    SetClipboard,
    BufferLimit,
    SynchronizePanes,
    ExperimentalEditorPane,
    HistoryTrickle,
    AgentCommand,
    AgentClaudeCodeCommand,
    AgentAutoApprove,
    Mouse,
    EscapeTime,
    Prefix2,
    FocusFollowsMouse,
    ExtendedKeys,
    FocusEvents,
    ClearOnAttach,
    CommandAlias,
}

impl MuxOptionKey {
    pub const ALL: [Self; 21] = [
        Self::Prefix,
        Self::ModeKeys,
        Self::HistoryLimit,
        Self::WordSeparators,
        Self::CopyCommand,
        Self::SetClipboard,
        Self::BufferLimit,
        Self::SynchronizePanes,
        Self::ExperimentalEditorPane,
        Self::HistoryTrickle,
        Self::AgentCommand,
        Self::AgentClaudeCodeCommand,
        Self::AgentAutoApprove,
        Self::Mouse,
        Self::EscapeTime,
        Self::Prefix2,
        Self::FocusFollowsMouse,
        Self::ExtendedKeys,
        Self::FocusEvents,
        Self::ClearOnAttach,
        Self::CommandAlias,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::ModeKeys => "mode-keys",
            Self::HistoryLimit => "history-limit",
            Self::WordSeparators => "word-separators",
            Self::CopyCommand => "copy-command",
            Self::SetClipboard => "set-clipboard",
            Self::BufferLimit => "buffer-limit",
            Self::SynchronizePanes => "synchronize-panes",
            Self::ExperimentalEditorPane => "experimental-editor-pane",
            Self::HistoryTrickle => "history-trickle",
            Self::AgentCommand => "agent-command",
            Self::AgentClaudeCodeCommand => "agent-claude-code-command",
            Self::AgentAutoApprove => "agent-auto-approve",
            Self::Mouse => "mouse",
            Self::EscapeTime => "escape-time",
            Self::Prefix2 => "prefix2",
            Self::FocusFollowsMouse => "focus-follows-mouse",
            Self::ExtendedKeys => "extended-keys",
            Self::FocusEvents => "focus-events",
            Self::ClearOnAttach => "clear-on-attach",
            Self::CommandAlias => "command-alias",
        }
    }

    #[must_use]
    pub fn from_config_key(key: &str) -> Option<Self> {
        match key {
            "prefix" => Some(Self::Prefix),
            "mode-keys" => Some(Self::ModeKeys),
            "history-limit" => Some(Self::HistoryLimit),
            "word-separators" => Some(Self::WordSeparators),
            "copy-command" => Some(Self::CopyCommand),
            "set-clipboard" => Some(Self::SetClipboard),
            "buffer-limit" => Some(Self::BufferLimit),
            "synchronize-panes" => Some(Self::SynchronizePanes),
            "experimental-editor-pane" => Some(Self::ExperimentalEditorPane),
            "history-trickle" => Some(Self::HistoryTrickle),
            "agent-command" => Some(Self::AgentCommand),
            "agent-claude-code-command" => Some(Self::AgentClaudeCodeCommand),
            "agent-auto-approve" => Some(Self::AgentAutoApprove),
            "mouse" => Some(Self::Mouse),
            "escape-time" => Some(Self::EscapeTime),
            "prefix2" => Some(Self::Prefix2),
            "focus-follows-mouse" => Some(Self::FocusFollowsMouse),
            "extended-keys" => Some(Self::ExtendedKeys),
            "focus-events" => Some(Self::FocusEvents),
            "clear-on-attach" => Some(Self::ClearOnAttach),
            _ => None,
        }
    }

    fn default_display_value(self) -> String {
        match self {
            Self::Prefix => "C-b".to_owned(),
            Self::ModeKeys => "emacs".to_owned(),
            Self::HistoryLimit => DEFAULT_HISTORY_LIMIT.to_string(),
            Self::WordSeparators => DEFAULT_WORD_SEPARATORS.to_owned(),
            Self::CopyCommand | Self::CommandAlias => String::new(),
            Self::SetClipboard => "external".to_owned(),
            Self::BufferLimit => "50".to_owned(),
            Self::SynchronizePanes
            | Self::ExperimentalEditorPane
            | Self::FocusFollowsMouse
            | Self::ExtendedKeys
            | Self::FocusEvents => "off".to_owned(),
            Self::HistoryTrickle => "2000".to_owned(),
            Self::Mouse | Self::ClearOnAttach => "on".to_owned(),
            Self::EscapeTime => "10".to_owned(),
            Self::Prefix2 => "None".to_owned(),
            Self::AgentCommand => DEFAULT_AGENT_COMMAND.to_owned(),
            Self::AgentClaudeCodeCommand => DEFAULT_AGENT_CLAUDE_CODE_COMMAND.to_owned(),
            Self::AgentAutoApprove => DEFAULT_AGENT_AUTO_APPROVE.as_str().to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MuxOptionSource {
    #[default]
    Default,
    TmuxConfig,
    Override,
    RuntimeCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuxOptionValue {
    #[serde(deserialize_with = "deserialize_mux_option_value")]
    pub value: String,
    pub source: MuxOptionSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MuxOptions(BTreeMap<MuxOptionKey, MuxOptionValue>);

impl Default for MuxOptions {
    fn default() -> Self {
        Self(
            MuxOptionKey::ALL
                .into_iter()
                .map(|key| {
                    (
                        key,
                        MuxOptionValue {
                            value: key.default_display_value(),
                            source: MuxOptionSource::Default,
                        },
                    )
                })
                .collect(),
        )
    }
}

impl MuxOptions {
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (MuxOptionKey, MuxOptionValue)>) -> Self {
        Self(entries.into_iter().collect())
    }

    #[must_use]
    pub fn get(&self, key: MuxOptionKey) -> Option<&MuxOptionValue> {
        self.0.get(&key)
    }

    pub fn set(
        &mut self,
        key: MuxOptionKey,
        value: impl Into<String>,
        source: MuxOptionSource,
    ) -> bool {
        let next = MuxOptionValue {
            value: value.into(),
            source,
        };
        if self.0.get(&key) == Some(&next) {
            return false;
        }
        self.0.insert(key, next);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = (MuxOptionKey, &MuxOptionValue)> {
        self.0.iter().map(|(key, value)| (*key, value))
    }

    pub fn merge(&mut self, patch: Self) {
        self.0.extend(patch.0);
    }

    pub fn validate_partial(&self) -> Result<(), &'static str> {
        if self
            .0
            .values()
            .any(|value| value.value.len() > MAX_MUX_OPTION_VALUE_BYTES)
        {
            return Err("mux option values exceed the wire byte limit");
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.0.len() != MuxOptionKey::ALL.len()
            || MuxOptionKey::ALL
                .iter()
                .any(|key| !self.0.contains_key(key))
        {
            return Err("mux options must contain every supported key exactly once");
        }
        if self
            .0
            .values()
            .any(|value| value.value.len() > MAX_MUX_OPTION_VALUE_BYTES)
        {
            return Err("mux option values exceed the wire byte limit");
        }
        Ok(())
    }
}

fn deserialize_mux_option_value<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct MuxOptionValueVisitor;

    impl<'de> Visitor<'de> for MuxOptionValueVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "a mux option value no longer than {MAX_MUX_OPTION_VALUE_BYTES} bytes"
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
            if value.len() > MAX_MUX_OPTION_VALUE_BYTES {
                return Err(E::invalid_length(value.len(), &self));
            }
            Ok(value.to_owned())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.len() > MAX_MUX_OPTION_VALUE_BYTES {
                return Err(E::invalid_length(value.len(), &self));
            }
            Ok(value)
        }
    }

    deserializer.deserialize_str(MuxOptionValueVisitor)
}
