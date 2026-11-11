use std::{
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;
use zz_protocol::{AgentConnectionPhase, AgentPaneWire, AgentProvider};

pub(crate) use crate::agent::codex::codex_home;
use crate::agent::{
    accounts,
    claude::sessions,
    claude_peers::{self, PeerRecord},
    codex::rollout::RolloutMeta,
};

const FALLBACK_SHELL: &str = "/bin/sh";
const MAX_VALUE_BYTES: usize = 128;
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;
const TAIL_BYTES: u64 = 2 * 1024 * 1024;
const ROLLOUT_DAYS: usize = 7;
const CLAUDE_ALIASES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
const CODEX_EMBEDDED: [&str; 8] = [
    "--search",
    "--profile",
    "-p",
    "--oss",
    "--no-daemon",
    "--strict-config",
    "--remote",
    "--dangerously-bypass-hook-trust",
];
const CODEX_SHARED: [&str; 2] = [
    "tui.fullscreen_transcript",
    "suppress_unstable_features_warning",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CliSession {
    pub(crate) pid: Option<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) running: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) cwd: Option<String>,
}

impl Facts {
    pub(crate) fn claude_entry(&mut self, entry: &Value) {
        if entry["isSidechain"] == true {
            return;
        }
        if let Some(cwd) = entry["cwd"].as_str().filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(cwd.to_owned());
        }
        if entry["type"] == "assistant" {
            if let Some(model) = entry["message"]["model"]
                .as_str()
                .filter(|model| model.starts_with("claude-"))
            {
                self.model = Some(model.to_owned());
            }
            if let Some(effort) = entry["effort"].as_str().filter(|effort| !effort.is_empty()) {
                self.effort = Some(effort.to_owned());
            }
            return;
        }
        if entry["type"] != "user" {
            return;
        }
        let Some(output) = local_output(entry) else {
            return;
        };
        if let Some(rest) = output.strip_prefix("Set model to ") {
            if let Some(model) = rest.split('`').nth(1).filter(|model| !model.is_empty()) {
                self.model = Some(model.to_owned());
            }
        } else if let Some(rest) = output.strip_prefix("Set effort level to ")
            && let Some(effort) = rest
                .split(|character: char| !character.is_ascii_alphanumeric())
                .next()
                .filter(|effort| !effort.is_empty())
        {
            self.effort = Some(effort.to_owned());
        }
    }

    pub(crate) fn codex_entry(&mut self, entry: &Value) {
        let payload = &entry["payload"];
        let settings = match (entry["type"].as_str(), payload["type"].as_str()) {
            (Some("turn_context"), _) => payload,
            (Some("event_msg"), Some("thread_settings_applied")) => &payload["thread_settings"],
            _ => return,
        };
        if let Some(model) = settings["model"].as_str().filter(|model| !model.is_empty()) {
            self.model = Some(model.to_owned());
        }
        let effort = settings["effort"]
            .as_str()
            .or_else(|| settings["reasoning_effort"].as_str())
            .or_else(|| settings["collaboration_mode"]["settings"]["reasoning_effort"].as_str());
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            self.effort = Some(effort.to_owned());
        }
        if let Some(cwd) = settings["cwd"].as_str().filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(cwd.to_owned());
        }
    }

    pub(crate) fn or(&self, launch: &Self) -> Self {
        Self {
            model: self.model.clone().or_else(|| launch.model.clone()),
            effort: self.effort.clone().or_else(|| launch.effort.clone()),
            cwd: self.cwd.clone().or_else(|| launch.cwd.clone()),
        }
    }

    pub(crate) fn claude_launch(arguments: &[String], config: Option<&Path>, cwd: &str) -> Self {
        let mut facts = Self {
            cwd: Some(cwd.to_owned()).filter(|cwd| !cwd.is_empty()),
            ..Self::default()
        };
        let project = Path::new(cwd).join(".claude");
        let files = config
            .map(|config| config.join("settings.json"))
            .into_iter()
            .chain(["settings.json", "settings.local.json"].map(|name| project.join(name)));
        for settings in files
            .filter_map(|path| read_small(&path))
            .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        {
            if let Some(model) = settings["model"].as_str().and_then(setting_text) {
                facts.model = Some(model);
            }
            if let Some(effort) = settings["effortLevel"].as_str().and_then(setting_text) {
                facts.effort = Some(effort);
            }
        }
        if let Some(model) = option_values(arguments, &["--model"])
            .pop()
            .and_then(setting_text)
        {
            facts.model = Some(model);
        }
        if let Some(effort) = option_values(arguments, &["--effort"])
            .pop()
            .and_then(setting_text)
        {
            facts.effort = Some(effort);
        }
        facts
    }

    pub(crate) fn codex_launch(
        arguments: &[String],
        home: Option<&Path>,
        cwd: Option<String>,
    ) -> Self {
        let mut facts = Self {
            cwd,
            ..Self::default()
        };
        if let Some(text) = home.and_then(|home| read_small(&home.join("config.toml"))) {
            facts.model = toml_top_level(&text, "model");
            facts.effort = toml_top_level(&text, "model_reasoning_effort");
        }
        for (key, value) in option_values(arguments, &["-c", "--config"])
            .into_iter()
            .filter_map(|value| value.split_once('='))
        {
            match (key.trim(), setting_text(value)) {
                ("model", Some(model)) => facts.model = Some(model),
                ("model_reasoning_effort", Some(effort)) => facts.effort = Some(effort),
                _ => {}
            }
        }
        if let Some(model) = option_values(arguments, &["-m", "--model"])
            .pop()
            .and_then(setting_text)
        {
            facts.model = Some(model);
        }
        facts
    }
}

pub(crate) fn claude_session(
    registry: &Path,
    config: Option<&Path>,
    pane_pid: u32,
    parent_of: impl Fn(u32) -> Option<u32>,
    alive: impl Fn(u32) -> bool,
    arguments: impl Fn(u32) -> Vec<String>,
) -> Option<CliSession> {
    let mut records = claude_peers::read_records_in(registry).ok()?;
    records.retain(|record| record.zz.is_none() && alive(record.pid));
    let record = claude_peers::record_in_pane_with(&records, pane_pid, parent_of)?;
    Some(claude_record_session(
        record,
        config,
        &arguments(record.pid),
    ))
}

fn claude_record_session(
    record: &PeerRecord,
    config: Option<&Path>,
    args: &[String],
) -> CliSession {
    let cwd = PathBuf::from(&record.cwd);
    let launch = Facts::claude_launch(args, config, &record.cwd);
    let mut facts = Facts::default();
    if let Some(path) =
        config.and_then(|config| sessions::session_file(config, &cwd, &record.session_id))
    {
        for entry in tail_lines(&path) {
            facts.claude_entry(&entry);
        }
    }
    let facts = facts.or(&launch);
    let running = match record.status.as_str() {
        "busy" => vec!["a turn".to_owned()],
        "shell" => vec!["a background shell".to_owned()],
        "waiting" if record.waiting_for.is_empty() => vec!["a turn waiting for you".to_owned()],
        "waiting" => vec![format!("a turn waiting for a {}", record.waiting_for)],
        _ => Vec::new(),
    };
    CliSession {
        pid: Some(record.pid),
        session_id: Some(record.session_id.clone()),
        cwd: Some(cwd),
        model: facts.model.as_deref().and_then(claude_model),
        effort: facts.effort.filter(|effort| launch_value(effort)),
        running,
    }
}

pub(crate) fn claude_fresh(
    pid: Option<u32>,
    config: Option<&Path>,
    args: &[String],
    cwd: Option<PathBuf>,
) -> CliSession {
    let launch = Facts::claude_launch(
        args,
        config,
        &cwd.as_deref()
            .map(Path::display)
            .map(|cwd| cwd.to_string())
            .unwrap_or_default(),
    );
    CliSession {
        pid,
        session_id: None,
        cwd,
        model: launch.model.as_deref().and_then(claude_model),
        effort: launch.effort.filter(|effort| launch_value(effort)),
        running: Vec::new(),
    }
}

pub(crate) fn codex_session(
    sessions: Option<&Path>,
    home: Option<&Path>,
    args: &[String],
    cwd: Option<PathBuf>,
    rollout: Option<PathBuf>,
) -> CliSession {
    let thread = resumed_thread(args);
    let rollout = rollout.or_else(|| {
        let thread = thread.as_deref()?;
        find_rollout(sessions?, thread)
    });
    let meta = rollout.as_deref().and_then(RolloutMeta::read);
    let cwd = meta.as_ref().map(|meta| meta.cwd.clone()).or(cwd);
    let launch = Facts::codex_launch(
        args,
        home,
        cwd.as_deref().map(|cwd| cwd.display().to_string()),
    );
    let mut facts = Facts::default();
    let mut running = false;
    if let Some(path) = &rollout {
        for entry in tail_lines(path) {
            facts.codex_entry(&entry);
            match entry["payload"]["type"].as_str() {
                Some("task_started") if entry["type"] == "event_msg" => running = true,
                Some("task_complete" | "turn_aborted") if entry["type"] == "event_msg" => {
                    running = false;
                }
                _ => {}
            }
        }
    }
    let facts = facts.or(&launch);
    CliSession {
        pid: None,
        session_id: meta.map(|meta| meta.thread).or(thread),
        cwd,
        model: facts.model.filter(|model| launch_value(model)),
        effort: facts.effort.filter(|effort| launch_value(effort)),
        running: if running {
            vec!["a turn".to_owned()]
        } else {
            Vec::new()
        },
    }
}

pub(crate) fn codex_embedded(arguments: &[String]) -> bool {
    let mut words = arguments.iter().skip(1).map(String::as_str);
    while let Some(word) = words.next() {
        let config = match word {
            "-c" | "--config" => words.next(),
            _ => word
                .strip_prefix("--config=")
                .or_else(|| word.strip_prefix("-c").filter(|rest| !rest.is_empty())),
        };
        if let Some(config) = config {
            if !codex_shared_config(config) {
                return true;
            }
            continue;
        }
        if CODEX_EMBEDDED.iter().any(|flag| {
            word == *flag
                || word
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.starts_with('=') || !flag.starts_with("--"))
        }) {
            return true;
        }
    }
    false
}

fn codex_shared_config(config: &str) -> bool {
    let Some((key, value)) = config.split_once('=') else {
        return false;
    };
    let key = key.trim();
    matches!(value.trim(), "true" | "false")
        && (CODEX_SHARED.contains(&key)
            || key
                .strip_prefix("features.")
                .is_some_and(|name| !name.is_empty() && !name.contains('.')))
}

fn resumed_thread(args: &[String]) -> Option<String> {
    let mut words = args.iter().skip(1);
    let mut after_resume = false;
    while let Some(word) = words.next() {
        if matches!(
            word.as_str(),
            "-c" | "--config" | "-m" | "--model" | "-C" | "--cd" | "-p" | "--profile"
        ) {
            words.next();
            continue;
        }
        if word.starts_with('-') {
            continue;
        }
        if after_resume {
            return Some(word.clone()).filter(|word| launch_value(word));
        }
        if word == "resume" {
            after_resume = true;
        } else {
            return None;
        }
    }
    None
}

fn find_rollout(sessions: &Path, thread: &str) -> Option<PathBuf> {
    let suffix = format!("-{thread}.jsonl");
    recent_days(sessions)
        .into_iter()
        .flat_map(|day| std::fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&suffix))
        })
}

pub(crate) fn rollouts_since(sessions: &Path, cwd: &Path, since: SystemTime) -> Vec<PathBuf> {
    let Ok(cwd) = std::fs::canonicalize(cwd) else {
        return Vec::new();
    };
    let mut found = recent_days(sessions)
        .into_iter()
        .flat_map(|day| std::fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| is_rollout(path))
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .filter(|(modified, _)| *modified >= since)
        .filter_map(|(modified, path)| RolloutMeta::read(&path).map(|meta| (modified, path, meta)))
        .filter(|(_, _, meta)| {
            meta.parent.is_none()
                && std::fs::canonicalize(&meta.cwd).is_ok_and(|session| session == cwd)
        })
        .map(|(modified, path, _)| (modified, path))
        .collect::<Vec<_>>();
    found.sort();
    found.into_iter().rev().map(|(_, path)| path).collect()
}

pub(crate) fn is_rollout(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-"))
}

fn subdirectories(path: &Path) -> Vec<PathBuf> {
    let mut directories = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

fn recent_days(sessions: &Path) -> Vec<PathBuf> {
    let mut days = subdirectories(sessions)
        .iter()
        .flat_map(|year| subdirectories(year))
        .flat_map(|month| subdirectories(&month))
        .collect::<Vec<_>>();
    days.sort();
    days.into_iter().rev().take(ROLLOUT_DAYS).collect()
}

pub(crate) fn agent_pid(pane_pid: u32, name: &str) -> Option<u32> {
    std::iter::once(pane_pid)
        .chain(zz_daemon_client::process_info::descendants(pane_pid))
        .find(|pid| {
            arguments(*pid).iter().take(2).any(|argument| {
                Path::new(argument)
                    .file_name()
                    .is_some_and(|file| file == name)
            })
        })
}

pub(crate) fn arguments(pid: u32) -> Vec<String> {
    zz_daemon_client::process_info::record(pid)
        .map(|record| {
            record
                .arguments
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn open_rollout(pid: u32) -> Option<PathBuf> {
    open_files(pid).into_iter().find(|path| {
        is_rollout(path) && RolloutMeta::read(path).is_some_and(|meta| meta.parent.is_none())
    })
}

#[cfg(target_os = "linux")]
fn open_files(pid: u32) -> Vec<PathBuf> {
    std::fs::read_dir(format!("/proc/{pid}/fd"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn open_files(pid: u32) -> Vec<PathBuf> {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    std::process::Command::new("lsof")
        .args(["-n", "-P", "-Fn", "-p", &pid.to_string()])
        .output_unmasked()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.strip_prefix('n'))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn tail_lines(path: &Path) -> Vec<Value> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    let start = length.saturating_sub(TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if start > 0 {
        lines.next();
    }
    lines
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

pub(crate) fn chat_settings(state: &AgentPaneWire) -> (Option<String>, Option<String>) {
    let options = serde_json::from_str::<Value>(&state.config_options).unwrap_or_default();
    let current = |id: &str| {
        options
            .as_array()?
            .iter()
            .find(|option| option["id"] == id)?["currentValue"]
            .as_str()
            .filter(|value| *value != "default" && launch_value(value))
            .map(str::to_owned)
    };
    (current("model"), current("effort"))
}

pub(crate) fn chat_running(state: &AgentPaneWire) -> Vec<String> {
    let mut running = Vec::new();
    match state.phase {
        AgentConnectionPhase::Running => running.push("a turn".to_owned()),
        AgentConnectionPhase::AwaitingPermission => {
            running.push("a turn waiting for a permission".to_owned());
        }
        _ => {}
    }
    let subagents = state
        .tasks
        .iter()
        .filter(|task| task.kind == "agent")
        .count();
    let tasks = state.tasks.len() - subagents;
    if tasks > 0 {
        running.push(plural(tasks, "background task"));
    }
    if subagents > 0 {
        running.push(plural(subagents, "background subagent"));
    }
    if state.queued_prompts > 0 {
        running.push(plural(state.queued_prompts as usize, "queued prompt"));
    }
    running
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

pub(crate) fn busy(running: &[String]) -> Option<String> {
    let (last, rest) = running.split_last()?;
    if rest.is_empty() {
        return Some(format!("busy: {last}"));
    }
    Some(format!("busy: {} and {last}", rest.join(", ")))
}

pub(crate) fn cli_command(
    provider: AgentProvider,
    session_id: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Vec<String> {
    let model = model.filter(|model| launch_value(model));
    let effort = effort.filter(|effort| launch_value(effort));
    let session_id = session_id.filter(|session| launch_value(session));
    let mut words = Vec::new();
    match provider {
        AgentProvider::ClaudeCode => {
            words.push("claude".to_owned());
            if let Some(session) = session_id {
                words.extend(["--resume".to_owned(), session.to_owned()]);
            }
            if let Some(model) = model {
                words.extend(["--model".to_owned(), model.to_owned()]);
            }
            if let Some(effort) = effort {
                words.extend(["--effort".to_owned(), effort.to_owned()]);
            }
        }
        AgentProvider::Codex => {
            words.push("codex".to_owned());
            match (session_id, model) {
                (Some(session), _) => words.extend(["resume".to_owned(), session.to_owned()]),
                (None, Some(model)) => words.extend(["-m".to_owned(), model.to_owned()]),
                (None, None) => {}
            }
        }
    }
    words
}

pub(crate) fn interactive_shell(
    account: Option<(AgentProvider, &Path)>,
    words: &[String],
) -> Vec<String> {
    let command = account
        .map(|(provider, home)| {
            format!(
                "{}={}",
                accounts::variable(provider),
                shell_word(&home.to_string_lossy())
            )
        })
        .into_iter()
        .chain(words.iter().map(|word| shell_word(word)))
        .collect::<Vec<_>>()
        .join(" ");
    let line = match account {
        Some((provider, _)) => format!(
            "unset {}; {command}",
            accounts::login_overrides(provider).join(" ")
        ),
        None => command,
    };
    vec![
        FALLBACK_SHELL.to_owned(),
        "-c".to_owned(),
        format!(
            "exec \"${{SHELL:-{FALLBACK_SHELL}}}\" -ic {}",
            quoted(&line)
        ),
    ]
}

fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && !word.starts_with('=')
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=".contains(character));
    if plain { word.to_owned() } else { quoted(word) }
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn claude_model(model: &str) -> Option<String> {
    if model.starts_with("claude-") && launch_value(model) {
        return Some(model.to_owned());
    }
    let lower = model.to_ascii_lowercase();
    if launch_value(&lower)
        && CLAUDE_ALIASES.iter().any(|alias| {
            lower == *alias
                || lower
                    .strip_prefix(alias)
                    .is_some_and(|rest| rest.starts_with('['))
        })
    {
        return Some(lower);
    }
    CLAUDE_ALIASES
        .iter()
        .find(|alias| lower.contains(*alias))
        .map(|alias| (*alias).to_owned())
}

fn launch_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE_BYTES
        && !value.starts_with('-')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._[]:/".contains(character))
}

fn read_small(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    std::io::read_to_string(file).ok()
}

fn local_output(entry: &Value) -> Option<&str> {
    let content = &entry["message"]["content"];
    let text = content.as_str().or_else(|| {
        content
            .as_array()?
            .iter()
            .find_map(|block| block["text"].as_str())
    })?;
    text.trim()
        .strip_prefix("<local-command-stdout>")?
        .strip_suffix("</local-command-stdout>")
        .map(str::trim)
}

fn option_values<'a>(arguments: &'a [String], names: &[&str]) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        if names.contains(&word.as_str()) {
            values.extend(words.next().map(String::as_str));
        } else if let Some(value) = names
            .iter()
            .filter(|name| name.starts_with("--"))
            .find_map(|name| word.strip_prefix(name)?.strip_prefix('='))
        {
            values.push(value);
        }
    }
    values
}

fn setting_text(value: &str) -> Option<String> {
    let value = value.trim();
    let text = match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or_default(),
        _ => value.split('#').next().unwrap_or_default().trim(),
    };
    (!text.is_empty() && text.len() <= MAX_VALUE_BYTES && !text.chars().any(char::is_control))
        .then(|| text.to_owned())
}

fn toml_top_level(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim().trim_matches('"') == key)
        .find_map(|(_, value)| setting_text(value))
}

#[cfg(test)]
#[path = "cli_session_tests.rs"]
mod tests;
