use zz_protocol::{AgentProvider, ProviderStatus, ProviderTask, ProvidersReport};

pub const PROVIDERS: [AgentProvider; 5] = [
    AgentProvider::ClaudeCode,
    AgentProvider::Codex,
    AgentProvider::Antigravity,
    AgentProvider::Grok,
    AgentProvider::Cursor,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteTone {
    Muted,
    Foreground,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ProviderAction {
    Busy {
        label: &'static str,
    },
    Copy {
        command: String,
    },
    Update {
        label: &'static str,
        primary: bool,
        disabled: bool,
    },
}

#[must_use]
pub fn home_relative(path: &str) -> String {
    ["/Users/", "/home/"]
        .iter()
        .find_map(|root| {
            let rest = path.strip_prefix(root)?;
            let tail = rest.find('/').map_or("", |slash| &rest[slash..]);
            Some(format!("~{tail}"))
        })
        .unwrap_or_else(|| path.to_owned())
}

#[must_use]
pub fn provider_description(status: &ProviderStatus) -> String {
    status
        .path
        .as_deref()
        .map_or_else(|| "Not installed".to_owned(), home_relative)
}

#[must_use]
pub fn version_text(status: &ProviderStatus) -> String {
    match (&status.version, &status.latest) {
        (Some(version), Some(latest)) if status.outdated() => format!("{version} → {latest}"),
        (Some(version), _) => version.clone(),
        (None, Some(latest)) if !status.installed() => format!("Latest {latest}"),
        (None, _) if status.installed() => "Unknown version".to_owned(),
        (None, _) => String::new(),
    }
}

#[must_use]
pub fn version_current(status: &ProviderStatus) -> bool {
    status.version.is_some() && status.latest.is_some() && !status.outdated()
}

#[must_use]
pub fn provider_action(status: &ProviderStatus, checking: bool) -> Option<ProviderAction> {
    let installed = status.installed();
    if matches!(status.task, ProviderTask::Running { .. }) {
        return Some(ProviderAction::Busy {
            label: if installed { "Updating" } else { "Installing" },
        });
    }
    if installed && !status.outdated() {
        return None;
    }
    if let Some(manual) = &status.manual {
        return Some(ProviderAction::Copy {
            command: manual.command.clone()?,
        });
    }
    Some(ProviderAction::Update {
        label: if installed { "Update" } else { "Install" },
        primary: !installed || status.outdated(),
        disabled: checking && status.latest.is_none() && installed,
    })
}

#[must_use]
pub fn provider_notes(status: &ProviderStatus) -> Vec<(String, NoteTone)> {
    let mut notes = Vec::new();
    if let Some(manual) = status.manual.as_ref().filter(|_| status.outdated()) {
        match &manual.command {
            Some(command) => {
                notes.push((
                    format!("Installed by {}. Update it with:", manual.owner),
                    NoteTone::Muted,
                ));
                notes.push((command.clone(), NoteTone::Foreground));
            }
            None => notes.push((
                format!("Installed by {}. Update it there.", manual.owner),
                NoteTone::Muted,
            )),
        }
    }
    match &status.task {
        ProviderTask::Running { output } if !output.is_empty() => {
            notes.push((output.clone(), NoteTone::Muted));
        }
        ProviderTask::Failed { message } => notes.push((message.clone(), NoteTone::Warning)),
        ProviderTask::Running { .. } | ProviderTask::Idle => {}
    }
    notes
}

#[must_use]
pub fn provider_rows(report: &ProvidersReport) -> Vec<ProviderStatus> {
    PROVIDERS
        .into_iter()
        .map(|provider| {
            report
                .provider(provider)
                .cloned()
                .unwrap_or_else(|| ProviderStatus::new(provider))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_known_newer_release_offers_an_update() {
        let mut grok = ProviderStatus::new(AgentProvider::Grok);
        grok.path = Some("/Users/me/.local/bin/grok".to_owned());
        grok.version = Some("1.0.92".to_owned());
        assert_eq!(provider_action(&grok, false), None);
        assert!(!version_current(&grok));
        grok.latest = Some("1.0.92".to_owned());
        assert_eq!(provider_action(&grok, true), None);
        assert!(version_current(&grok));
        grok.latest = Some("1.0.93".to_owned());
        assert_eq!(
            provider_action(&grok, false),
            Some(ProviderAction::Update {
                label: "Update",
                primary: true,
                disabled: false,
            })
        );
        grok.version = None;
        assert_eq!(provider_action(&grok, false), None);
        assert!(!version_current(&grok));
        assert_eq!(version_text(&grok), "Unknown version");
    }
}
