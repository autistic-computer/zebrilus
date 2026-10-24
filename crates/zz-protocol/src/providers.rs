use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::AgentProvider;

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
