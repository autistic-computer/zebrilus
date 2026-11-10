use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};
use zz_daemon_client::user_data::{restrict_directory_to_current_user, restrict_to_current_user};
use zz_protocol::{AgentProvider, valid_agent_account};

const CLAUDE_LINKS: [&str; 22] = [
    "projects",
    "settings.json",
    "settings.local.json",
    "skills",
    "agents",
    "commands",
    "output-styles",
    "plugins",
    "history.jsonl",
    "file-history",
    "session-env",
    "plans",
    "paste-cache",
    "uploads",
    "todos",
    "tasks",
    "sessions",
    "teams",
    "dev-mods",
    "scripts",
    "CLAUDE.md",
    "keybindings.json",
];
const CODEX_LINKS: [&str; 17] = [
    "sessions",
    "archived_sessions",
    "session_index.jsonl",
    "session_index.jsonl.lock",
    "thread-writer-locks",
    "rollout-migrations",
    ".sqlite-maintenance.lock",
    "config.toml",
    "AGENTS.md",
    "skills",
    "agents",
    "plugins",
    "hooks.json",
    "keybindings.json",
    "history.jsonl",
    "attachments",
    "worktrees",
];
const CODEX_DATABASES: [&str; 4] = ["state_", "thread_history_", "goals_", "queue_"];
const CLAUDE_GLOBAL_CONFIG: &str = ".claude.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Account {
    pub(crate) id: String,
    pub(crate) home: Option<PathBuf>,
}

impl Account {
    pub(crate) fn resolve(provider: AgentProvider, id: &str) -> Self {
        Self {
            id: id.to_owned(),
            home: home(provider, id).ok(),
        }
    }
}

#[must_use]
pub(crate) const fn variable(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "CLAUDE_CONFIG_DIR",
        AgentProvider::Codex => "CODEX_HOME",
    }
}

const fn directory_name(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    }
}

pub(crate) fn root(provider: AgentProvider) -> io::Result<PathBuf> {
    Ok(crate::agent::paths::accounts_dir()?.join(directory_name(provider)))
}

pub(crate) fn home(provider: AgentProvider, id: &str) -> io::Result<PathBuf> {
    if !valid_agent_account(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{id}` is not an account id"),
        ));
    }
    Ok(root(provider)?.join(id))
}

pub(crate) fn default_home(provider: AgentProvider) -> Option<PathBuf> {
    match provider {
        AgentProvider::ClaudeCode => crate::agent::claude::sessions::config_home(),
        AgentProvider::Codex => crate::agent::codex::codex_home(),
    }
}

#[expect(dead_code, reason = "the accounts list reads it")]
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

pub(crate) fn account_of(provider: AgentProvider, directory: &Path) -> Option<String> {
    let id = directory.file_name()?.to_str()?;
    (valid_agent_account(id) && directory.parent()? == root(provider).ok()? && directory.is_dir())
        .then(|| id.to_owned())
}

#[expect(dead_code, reason = "adding an account creates its directory")]
pub(crate) fn create(provider: AgentProvider, id: &str) -> io::Result<PathBuf> {
    let home = home(provider, id)?;
    fs::create_dir_all(&home)?;
    restrict_directory_to_current_user(&home)?;
    if let Some(default) = default_home(provider) {
        link(provider, &default, &home)?;
    }
    if provider == AgentProvider::ClaudeCode {
        seed_claude(claude_global_config().as_deref(), &home)?;
    }
    Ok(home)
}

pub(crate) fn existing(provider: AgentProvider, account: &Account) -> Result<PathBuf, String> {
    let label = provider.label();
    let id = &account.id;
    let Some(home) = &account.home else {
        return Err(format!(
            "{label} account {id} has no directory: the application-data directory is unknown"
        ));
    };
    if !home.is_dir() {
        return Err(format!(
            "{label} account {id} has no directory at {}; add the account again or move this chat to another with agent-account",
            home.display()
        ));
    }
    Ok(home.clone())
}

pub(crate) fn prepare(provider: AgentProvider, account: &Account) -> Result<PathBuf, String> {
    let home = existing(provider, account)?;
    if let Some(default) = default_home(provider)
        && let Err(error) = link(provider, &default, &home)
    {
        log::warn!(target: "zz::agent", "could not link {} into {}: {error}", default.display(), home.display());
    }
    Ok(home)
}

pub(crate) fn link(provider: AgentProvider, default: &Path, home: &Path) -> io::Result<()> {
    for name in linked_names(provider, default) {
        let target = default.join(&name);
        let path = home.join(&name);
        if fs::symlink_metadata(&target).is_err() || fs::symlink_metadata(&path).is_ok() {
            continue;
        }
        symlink(&target, &path)?;
    }
    Ok(())
}

fn linked_names(provider: AgentProvider, default: &Path) -> Vec<OsString> {
    match provider {
        AgentProvider::ClaudeCode => CLAUDE_LINKS.iter().map(OsString::from).collect(),
        AgentProvider::Codex => CODEX_LINKS
            .iter()
            .map(OsString::from)
            .chain(
                fs::read_dir(default)
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name())
                    .filter(|name| name.to_str().is_some_and(codex_database)),
            )
            .collect(),
    }
}

fn codex_database(name: &str) -> bool {
    name.strip_suffix(".sqlite").is_some_and(|stem| {
        CODEX_DATABASES
            .iter()
            .any(|prefix| stem.starts_with(prefix))
    })
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(windows)]
fn symlink(target: &Path, path: &Path) -> io::Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    }
}

#[cfg(not(any(unix, windows)))]
fn symlink(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

fn claude_global_config() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|value| !value.is_empty()) {
        Some(config) => Some(PathBuf::from(config).join(CLAUDE_GLOBAL_CONFIG)),
        None => std::env::home_dir().map(|home| home.join(CLAUDE_GLOBAL_CONFIG)),
    }
}

pub(crate) fn seed_claude(global: Option<&Path>, home: &Path) -> io::Result<()> {
    let path = home.join(CLAUDE_GLOBAL_CONFIG);
    if fs::symlink_metadata(&path).is_ok() {
        return Ok(());
    }
    let global = global
        .and_then(|global| fs::read(global).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let seed = claude_seed(global.as_ref());
    let mut bytes = serde_json::to_vec_pretty(&seed).map_err(io::Error::other)?;
    bytes.push(b'\n');
    fs::write(&path, bytes)?;
    restrict_to_current_user(&path)
}

fn claude_seed(global: Option<&Value>) -> Value {
    let Some(global) = global.and_then(Value::as_object) else {
        return json!({ "hasCompletedOnboarding": true });
    };
    let mcp_servers = global
        .get("mcpServers")
        .filter(|servers| servers.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let projects = global
        .get("projects")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(path, project)| {
            let trusted = project.get("hasTrustDialogAccepted")?;
            Some((
                path.clone(),
                json!({ "hasTrustDialogAccepted": trusted.clone() }),
            ))
        })
        .collect::<Map<_, _>>();
    json!({
        "hasCompletedOnboarding": true,
        "mcpServers": mcp_servers,
        "projects": projects,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn entries(directory: &Path) -> Vec<String> {
        let mut names = fs::read_dir(directory)
            .expect("read directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("utf-8 name")
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn linking_adds_only_the_shared_entries_that_are_missing() {
        let root = tempfile::tempdir().expect("temp root");
        let default = root.path().join("default");
        let home = root.path().join("home");
        fs::create_dir_all(default.join("projects")).expect("projects");
        fs::create_dir_all(default.join("sessions")).expect("sessions");
        fs::create_dir_all(&home).expect("home");
        for name in [
            "settings.json",
            ".credentials.json",
            ".claude.json",
            "auth.json",
            "config.toml",
            "state_5.sqlite",
            "state_5.sqlite-wal",
            "state_5.sqlite-shm",
            "logs_2.sqlite",
            "goals_1.sqlite",
            "memories_1.sqlite",
        ] {
            fs::write(default.join(name), name).expect("default entry");
        }
        fs::write(home.join("settings.json"), "mine").expect("own settings");

        link(AgentProvider::ClaudeCode, &default, &home).expect("link claude");
        assert_eq!(entries(&home), ["projects", "sessions", "settings.json"]);
        assert_eq!(
            fs::read_link(home.join("projects")).expect("projects link"),
            default.join("projects")
        );
        assert_eq!(
            fs::read_to_string(home.join("settings.json")).expect("settings"),
            "mine"
        );

        let codex = root.path().join("codex");
        fs::create_dir_all(&codex).expect("codex home");
        link(AgentProvider::Codex, &default, &codex).expect("link codex");
        assert_eq!(
            entries(&codex),
            [
                "config.toml",
                "goals_1.sqlite",
                "sessions",
                "state_5.sqlite"
            ]
        );
        assert!(
            fs::symlink_metadata(codex.join("state_5.sqlite"))
                .expect("state link")
                .file_type()
                .is_symlink()
        );
        fs::remove_file(default.join("config.toml")).expect("drop config");
        fs::write(default.join("config.toml"), "new").expect("new config");
        link(AgentProvider::Codex, &default, &codex).expect("relink codex");
        assert_eq!(
            fs::read_to_string(codex.join("config.toml")).expect("config through the link"),
            "new"
        );
    }

    #[test]
    fn the_claude_seed_keeps_onboarding_mcp_servers_and_trust_only() {
        let root = tempfile::tempdir().expect("temp root");
        let global = root.path().join(".claude.json");
        fs::write(
            &global,
            json!({
                "numStartups": 40,
                "userID": "u-1",
                "oauthAccount": { "emailAddress": "me@example.com" },
                "hasCompletedOnboarding": true,
                "mcpServers": { "docs": { "command": "docs-mcp" } },
                "projects": {
                    "/work/a": { "hasTrustDialogAccepted": true, "allowedTools": ["Bash"] },
                    "/work/b": { "lastCost": 1.5 },
                },
            })
            .to_string(),
        )
        .expect("global config");
        let home = root.path().join("home");
        fs::create_dir_all(&home).expect("home");
        seed_claude(Some(&global), &home).expect("seed");
        let seeded = |home: &Path| {
            serde_json::from_slice::<Value>(
                &fs::read(home.join(".claude.json")).expect("seeded config"),
            )
            .expect("seeded json")
        };
        assert_eq!(
            seeded(&home),
            json!({
                "hasCompletedOnboarding": true,
                "mcpServers": { "docs": { "command": "docs-mcp" } },
                "projects": { "/work/a": { "hasTrustDialogAccepted": true } },
            })
        );

        fs::write(home.join(".claude.json"), "{}").expect("the CLI's own config");
        seed_claude(Some(&global), &home).expect("seed again");
        assert_eq!(seeded(&home), json!({}));

        let bare = root.path().join("bare");
        fs::create_dir_all(&bare).expect("bare home");
        fs::write(&global, "not json").expect("broken global config");
        seed_claude(Some(&global), &bare).expect("seed from a broken config");
        assert_eq!(seeded(&bare), json!({ "hasCompletedOnboarding": true }));
    }
}
