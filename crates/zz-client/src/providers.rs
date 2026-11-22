use zz_protocol::{AgentAccount, AgentProvider, ProviderStatus, ProviderTask, ProvidersReport};

use crate::agent_settings::{SIGNED_IN, account_name, plan_name};

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
    if let Some(short) = ["/Users/", "/home/"].iter().find_map(|root| {
        let rest = path.strip_prefix(root)?;
        let tail = rest.find('/').map_or("", |slash| &rest[slash..]);
        Some(format!("~{tail}"))
    }) {
        return short;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 9 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = &path[2..];
        let lower = rest.to_ascii_lowercase();
        if let Some(after) = lower
            .strip_prefix("\\users\\")
            .or_else(|| lower.strip_prefix("/users/"))
        {
            let tail_offset = rest.len() - after.len();
            let user_and_tail = &rest[tail_offset..];
            let tail = user_and_tail
                .find(['\\', '/'])
                .map_or("", |sep| &user_and_tail[sep..]);
            return format!("~{tail}");
        }
    }
    path.to_owned()
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
pub fn row_notes(status: &ProviderStatus) -> Vec<(String, NoteTone)> {
    let mut notes = Vec::new();
    if let Some(manual) = status.manual.as_ref().filter(|_| status.outdated()) {
        notes.push((
            manual
                .command
                .clone()
                .unwrap_or_else(|| format!("Installed by {}. Update it there.", manual.owner)),
            NoteTone::Muted,
        ));
    }
    if let ProviderTask::Failed { message } = &status.task {
        notes.push((message.clone(), NoteTone::Warning));
    }
    notes
}

#[must_use]
pub fn accounts_text(accounts: &[AgentAccount], provider: AgentProvider) -> Option<String> {
    let signed_in = accounts
        .iter()
        .filter(|account| account.provider == provider && account.signed_in)
        .collect::<Vec<_>>();
    match signed_in.as_slice() {
        [] => None,
        [account] => {
            let name = account_name(account);
            let plan = account
                .plan
                .as_deref()
                .map(plan_name)
                .filter(|plan| !plan.is_empty());
            Some(match plan {
                Some(plan) if name == SIGNED_IN => plan,
                Some(plan) => format!("{name} · {plan}"),
                None => name,
            })
        }
        many => Some(
            many.iter()
                .map(|account| account_name(account))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    }
}

#[must_use]
pub fn has_update(report: &ProvidersReport) -> bool {
    report.providers.iter().any(ProviderStatus::outdated)
}

#[must_use]
pub fn update_all<'a, H: Copy + 'a>(
    reports: impl IntoIterator<Item = (H, &'a ProvidersReport)>,
) -> Vec<(H, AgentProvider)> {
    reports
        .into_iter()
        .flat_map(|(host, report)| {
            report
                .providers
                .iter()
                .filter(|status| {
                    status.installed()
                        && matches!(
                            provider_action(status, report.checking),
                            Some(ProviderAction::Update {
                                disabled: false,
                                ..
                            })
                        )
                })
                .map(move |status| (host, status.provider))
        })
        .collect()
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

    fn account(provider: AgentProvider, email: &str, plan: Option<&str>) -> AgentAccount {
        AgentAccount {
            provider,
            id: None,
            signed_in: true,
            email: Some(email.to_owned()),
            organization: Some("Acme".to_owned()),
            plan: plan.map(str::to_owned),
        }
    }

    #[test]
    fn accounts_text_names_one_account_with_its_plan_and_several_by_email() {
        let claude = AgentProvider::ClaudeCode;
        let mut accounts = vec![
            account(claude, "me@example.com", Some("max_20x")),
            account(AgentProvider::Codex, "me@example.com", Some("pro")),
        ];
        assert_eq!(
            accounts_text(&accounts, claude).as_deref(),
            Some("me@example.com · Max 20x")
        );
        accounts.push(account(claude, "work@acme.example", Some("max_20x")));
        assert_eq!(
            accounts_text(&accounts, claude).as_deref(),
            Some("me@example.com, work@acme.example")
        );
        for account in &mut accounts {
            account.signed_in = false;
        }
        assert_eq!(accounts_text(&accounts, claude), None);
    }

    fn status(provider: AgentProvider, version: &str, latest: &str) -> ProviderStatus {
        ProviderStatus {
            path: Some(format!("/home/me/.local/bin/{}", provider.as_str())),
            version: Some(version.to_owned()),
            latest: Some(latest.to_owned()),
            ..ProviderStatus::new(provider)
        }
    }

    #[test]
    fn update_all_takes_only_agents_zz_can_update_and_the_dot_marks_any_behind() {
        let mut running = status(AgentProvider::ClaudeCode, "2.1.329", "2.1.330");
        running.task = ProviderTask::Running {
            output: String::new(),
        };
        let mut manual = status(AgentProvider::Codex, "0.167.0", "0.171.0");
        manual.manual = Some(zz_protocol::ManualUpdate {
            owner: "pacman".to_owned(),
            command: Some("sudo pacman -Syu openai-codex".to_owned()),
        });
        let xps = ProvidersReport {
            checking: false,
            providers: vec![
                running,
                manual,
                status(AgentProvider::Antigravity, "1.5.13", "1.5.14"),
            ],
        };
        let mac = ProvidersReport {
            checking: true,
            providers: vec![
                status(AgentProvider::ClaudeCode, "2.1.330", "2.1.330"),
                status(AgentProvider::Codex, "0.167.0", "0.171.0"),
                ProviderStatus {
                    latest: Some("1.0.93".to_owned()),
                    ..ProviderStatus::new(AgentProvider::Grok)
                },
            ],
        };
        let current = ProvidersReport {
            checking: false,
            providers: vec![status(AgentProvider::ClaudeCode, "2.1.330", "2.1.330")],
        };
        assert_eq!(
            update_all([(0, &mac), (1, &xps), (2, &current)]),
            [(0, AgentProvider::Codex), (1, AgentProvider::Antigravity)]
        );
        assert_eq!([&mac, &xps, &current].map(has_update), [true, true, false]);
    }

    #[test]
    fn home_relative_shortens_unix_and_windows_user_paths() {
        assert_eq!(
            home_relative("/Users/me/.local/bin/claude"),
            "~/.local/bin/claude"
        );
        assert_eq!(
            home_relative(r"C:\Users\me\.local\bin\claude.exe"),
            r"~\.local\bin\claude.exe"
        );
        assert_eq!(
            home_relative("C:/Users/me/AppData/Roaming/npm/codex.cmd"),
            "~/AppData/Roaming/npm/codex.cmd"
        );
    }
}
