use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientKind {
    Interactive,
    Command,
    Control,
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
