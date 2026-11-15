use std::{cmp::Ordering, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AgentProvider, PaneId};

pub const MAX_PROVIDER_TEXT_BYTES: usize = 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvidersReport {
    pub checking: bool,
    pub providers: Vec<ProviderStatus>,
}

impl ProvidersReport {
    #[must_use]
    pub fn provider(&self, provider: AgentProvider) -> Option<&ProviderStatus> {
        self.providers
            .iter()
            .find(|status| status.provider == provider)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: AgentProvider,
    pub path: Option<String>,
    pub version: Option<String>,
    pub latest: Option<String>,
    pub manual: Option<ManualUpdate>,
    pub task: ProviderTask,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualUpdate {
    pub owner: String,
    pub command: Option<String>,
}

impl ProviderStatus {
    #[must_use]
    pub const fn new(provider: AgentProvider) -> Self {
        Self {
            provider,
            path: None,
            version: None,
            latest: None,
            manual: None,
            task: ProviderTask::Idle,
        }
    }

    #[must_use]
    pub const fn installed(&self) -> bool {
        self.path.is_some()
    }

    #[must_use]
    pub fn outdated(&self) -> bool {
        match (&self.version, &self.latest) {
            (Some(version), Some(latest)) => compare_versions(version, latest) == Ordering::Less,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAccount {
    pub provider: AgentProvider,
    pub id: Option<String>,
    pub signed_in: bool,
    pub email: Option<String>,
    pub organization: Option<String>,
    pub plan: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub used: f32,
    pub resets_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindows {
    pub five_hour: Option<QuotaWindow>,
    pub week: Option<QuotaWindow>,
}

impl QuotaWindows {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.five_hour.is_none() && self.week.is_none()
    }

    pub fn merge(&mut self, update: Self) -> bool {
        let before = *self;
        self.five_hour = update.five_hour.or(self.five_hour);
        self.week = update.week.or(self.week);
        *self != before
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentQuota {
    pub provider: AgentProvider,
    pub account: Option<String>,
    pub windows: QuotaWindows,
}

pub const MAX_AGENT_LOGIN_CODE_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentLoginBrowser {
    #[default]
    Elsewhere,
    Local,
}

impl AgentLoginBrowser {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Elsewhere => "elsewhere",
            Self::Local => "local",
        }
    }
}

impl FromStr for AgentLoginBrowser {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local" => Ok(Self::Local),
            "elsewhere" => Ok(Self::Elsewhere),
            _ => Err(format!(
                "unknown browser `{value}`; expected local or elsewhere"
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLogin {
    pub id: u64,
    pub provider: AgentProvider,
    pub account: Option<String>,
    pub pane: Option<PaneId>,
    pub browser: AgentLoginBrowser,
    pub step: AgentLoginStep,
}

impl AgentLogin {
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.step.finished()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLoginStep {
    Starting,
    OpenUrl {
        url: String,
    },
    DeviceCode {
        url: String,
        code: String,
    },
    PasteCode {
        url: String,
    },
    Verifying,
    Done {
        email: Option<String>,
        organization: Option<String>,
        plan: Option<String>,
        pane: Option<AgentLoginPane>,
    },
    Failed(AgentLoginFailure),
    Cancelled,
}

impl AgentLoginStep {
    #[must_use]
    pub const fn finished(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Failed(_) | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLoginPane {
    Switched,
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLoginFailure {
    NotInstalled,
    DeviceCodeDisabled,
    TimedOut,
    Cli(String),
}

impl AgentLoginFailure {
    #[must_use]
    pub fn message(&self, provider: AgentProvider) -> String {
        let label = provider.label();
        match self {
            Self::NotInstalled => format!("{label} is not installed on this machine."),
            Self::DeviceCodeDisabled => format!(
                "This account cannot sign in to {label} with a device code. Sign in from a browser on the machine zz runs on."
            ),
            Self::TimedOut => "The sign-in timed out. Start it again.".to_owned(),
            Self::Cli(message) => message.clone(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct AgentLoginCode(String);

impl AgentLoginCode {
    #[must_use]
    pub fn new(code: &str) -> Option<Self> {
        let code = code.trim();
        (!code.is_empty() && code.len() <= MAX_AGENT_LOGIN_CODE_BYTES)
            .then(|| Self(code.to_owned()))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AgentLoginCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AgentLoginCode(..)")
    }
}

impl<'de> Deserialize<'de> for AgentLoginCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::new(&code).ok_or_else(|| serde::de::Error::custom("invalid sign-in code"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLoginRequest {
    Code { login: u64, code: AgentLoginCode },
    Cancel { login: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderTask {
    #[default]
    Idle,
    Running {
        output: String,
    },
    Failed {
        message: String,
    },
}

#[must_use]
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    fn parts(version: &str) -> (Vec<u64>, bool) {
        let version = version.trim().trim_start_matches('v');
        let (release, pre) = version
            .split_once(['-', '+'])
            .map_or((version, false), |(release, _)| (release, true));
        let numbers = release
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect();
        (numbers, pre)
    }
    let (mut left, left_pre) = parts(left);
    let (mut right, right_pre) = parts(right);
    let width = left.len().max(right.len());
    left.resize(width, 0);
    right.resize(width, 0);
    left.cmp(&right).then_with(|| right_pre.cmp(&left_pre))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_not_text() {
        assert_eq!(compare_versions("2.1.9", "2.1.10"), Ordering::Less);
        assert_eq!(compare_versions("0.163.0", "0.159.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.0-beta.1", "1.2.0"), Ordering::Less);
        assert_eq!(compare_versions("v1.3.0", "1.2.9"), Ordering::Greater);
    }

    #[test]
    fn only_a_known_older_version_is_outdated() {
        let mut status = ProviderStatus::new(AgentProvider::Codex);
        status.version = Some("0.159.0".into());
        assert!(!status.outdated());
        status.latest = Some("0.163.0".into());
        assert!(status.outdated());
        status.version = Some("0.163.0".into());
        assert!(!status.outdated());
    }
}
