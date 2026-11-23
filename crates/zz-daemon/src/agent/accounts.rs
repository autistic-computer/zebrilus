use std::process::Command;
#[cfg(unix)]
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use zz_protocol::AgentProvider;
#[cfg(unix)]
use zz_protocol::valid_agent_account;

#[cfg(unix)]
#[must_use]
pub(crate) const fn variable(provider: AgentProvider) -> Option<&'static str> {
    match provider {
        AgentProvider::ClaudeCode => Some("CLAUDE_CONFIG_DIR"),
        AgentProvider::Codex => Some("CODEX_HOME"),
        AgentProvider::Antigravity | AgentProvider::Grok | AgentProvider::Cursor => None,
    }
}

#[must_use]
pub(crate) const fn login_overrides(provider: AgentProvider) -> &'static [&'static str] {
    match provider {
        AgentProvider::ClaudeCode => &[
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_USER_EMAIL",
            "CLAUDE_CODE_ACCOUNT_UUID",
            "CLAUDE_CODE_ORGANIZATION_UUID",
        ],
        AgentProvider::Codex => &[
            "CODEX_ACCESS_TOKEN",
            "OPENAI_FEDERATION_RULE_ID",
            "OPENAI_IDENTITY_TOKEN_FILE",
        ],
        AgentProvider::Antigravity | AgentProvider::Grok | AgentProvider::Cursor => &[],
    }
}

pub(crate) fn strip_logins(command: &mut Command, provider: AgentProvider) {
    for name in login_overrides(provider) {
        command.env_remove(name);
    }
}

#[cfg(unix)]
pub(crate) fn run_on(command: &mut Command, provider: AgentProvider, home: &Path) {
    strip_logins(command, provider);
    if let Some(variable) = variable(provider) {
        command.env(variable, home);
    }
}

#[cfg(unix)]
const fn directory_name(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
        AgentProvider::Antigravity => "antigravity",
        AgentProvider::Grok => "grok",
        AgentProvider::Cursor => "cursor",
    }
}

#[cfg(unix)]
pub(crate) fn root(provider: AgentProvider) -> io::Result<PathBuf> {
    Ok(crate::agent::paths::accounts_dir()?.join(directory_name(provider)))
}

#[cfg(unix)]
pub(crate) fn home(provider: AgentProvider, id: &str) -> io::Result<PathBuf> {
    if variable(provider).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{} runs on its own login only", provider.label()),
        ));
    }
    if !valid_agent_account(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{id}` is not an account id"),
        ));
    }
    Ok(root(provider)?.join(id))
}

#[cfg(unix)]
pub(crate) fn ids(provider: AgentProvider) -> Vec<String> {
    let Ok(root) = root(provider) else {
        return Vec::new();
    };
    let mut ids = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|id| valid_agent_account(id))
        .collect::<Vec<_>>();
    ids.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
    ids
}
