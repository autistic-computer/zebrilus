use std::{
    fs,
    io::{BufRead as _, BufReader, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    sync::mpsc,
    time::Duration,
};

use serde_json::{Value, json};
use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_protocol::RawText;

const USAGE: &str = "usage: zz tools --fork-chat <claude|codex> <session> <message>";
const CODEX_REPLY: Duration = Duration::from_secs(20);
const MAX_ROLLOUT_DEPTH: usize = 4;

pub(crate) fn run(arguments: &[RawText]) -> ExitCode {
    let arguments = arguments.iter().map(RawText::as_str).collect::<Vec<_>>();
    let command = match arguments.as_slice() {
        ["claude", session, message] => claude(session, message),
        ["codex", thread, item] => codex(thread, item),
        _ => Err(USAGE.to_owned()),
    };
    match command {
        Ok(mut command) => {
            let error = exec(&mut command);
            report(&format!(
                "could not start {}: {error}",
                command.get_program().display()
            ))
        }
        Err(message) => report(&message),
    }
}

#[cfg(unix)]
fn exec(command: &mut Command) -> std::io::Error {
    std::os::unix::process::CommandExt::exec(command)
}

#[cfg(not(unix))]
fn exec(command: &mut Command) -> std::io::Error {
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => error,
    }
}

fn report(message: &str) -> ExitCode {
    eprintln!("zz tools --fork-chat: {message}");
    eprintln!("Press Enter to close this pane.");
    let _ = std::io::stdin().read_line(&mut String::new());
    ExitCode::from(1)
}

fn checked_id(id: &str) -> Result<&str, String> {
    if !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        Ok(id)
    } else {
        Err(format!("{id:?} is not a session or message id"))
    }
}

fn claude(session: &str, message: &str) -> Result<Command, String> {
    let session = checked_id(session)?;
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".claude")))
        .ok_or("HOME is not set")?;
    let source = claude_transcript(&config.join("projects"), session)?;
    let text = fs::read_to_string(&source)
        .map_err(|error| format!("could not read {}: {error}", source.display()))?;
    let fork = uuid::Uuid::new_v4().to_string();
    let lines = claude_fork_lines(&text, message, &fork)?;
    let target = source.with_file_name(format!("{fork}.jsonl"));
    write_new(&target, &(lines.join("\n") + "\n"))?;
    let mut command = Command::new("claude");
    command.args(["--resume", &fork]);
    Ok(command)
}

fn claude_transcript(projects: &Path, session: &str) -> Result<PathBuf, String> {
    let name = format!("{session}.jsonl");
    fs::read_dir(projects)
        .map_err(|error| format!("could not read {}: {error}", projects.display()))?
        .flatten()
        .map(|entry| entry.path().join(&name))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("no Claude Code transcript for session {session}"))
}

pub(crate) fn claude_fork_lines(
    text: &str,
    message: &str,
    fork: &str,
) -> Result<Vec<String>, String> {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let entries = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let start = entries
        .iter()
        .position(|entry| entry.as_ref().is_some_and(|entry| entry["uuid"] == message))
        .ok_or("that message is not in the transcript")?;
    let end = entries
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, entry)| entry.as_ref().is_some_and(is_claude_prompt))
        .map_or(lines.len(), |(index, _)| index);
    Ok(lines[..end]
        .iter()
        .zip(&entries)
        .map(|(line, entry)| match entry {
            Some(entry) if entry.get("sessionId").is_some() => {
                let mut entry = entry.clone();
                entry["sessionId"] = Value::from(fork);
                entry.to_string()
            }
            _ => (*line).to_owned(),
        })
        .collect())
}

fn is_claude_prompt(entry: &Value) -> bool {
    if entry["type"] != "user"
        || entry["isSidechain"] == true
        || entry["isMeta"] == true
        || entry["isCompactSummary"] == true
        || entry.get("toolUseResult").is_some()
    {
        return false;
    }
    let typed = |text: &str| !text.trim_start().starts_with('<');
    match &entry["message"]["content"] {
        Value::String(text) => typed(text),
        Value::Array(blocks) => {
            !blocks.iter().any(|block| block["type"] == "tool_result")
                && blocks.iter().any(|block| {
                    block["type"] == "text" && block["text"].as_str().is_some_and(typed)
                })
        }
        _ => false,
    }
}

fn write_new(path: &Path, text: &str) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn codex(thread: &str, item: &str) -> Result<Command, String> {
    let thread = checked_id(thread)?;
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".codex")))
        .ok_or("HOME is not set")?;
    let rollout = codex_rollout(&home.join("sessions"), thread, 0)
        .ok_or_else(|| format!("no Codex rollout for thread {thread}"))?;
    let text = fs::read_to_string(&rollout)
        .map_err(|error| format!("could not read {}: {error}", rollout.display()))?;
    let turn = codex_turn(&text, item).ok_or("that message is not in the chat")?;
    let fork = codex_fork(thread, &turn)?;
    let mut command = Command::new("codex");
    command.args(["resume", &fork]);
    Ok(command)
}

fn codex_rollout(directory: &Path, thread: &str, depth: usize) -> Option<PathBuf> {
    let suffix = format!("-{thread}.jsonl");
    let mut entries = fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| right.cmp(left));
    entries.iter().find_map(|path| {
        let name = path.file_name()?.to_str()?;
        if path.is_file() && name.starts_with("rollout-") && name.ends_with(&suffix) {
            Some(path.clone())
        } else if path.is_dir() && depth < MAX_ROLLOUT_DEPTH {
            codex_rollout(path, thread, depth + 1)
        } else {
            None
        }
    })
}

pub(crate) fn codex_turn(text: &str, item: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|entry| entry["payload"].clone())
        .find(|payload| {
            payload["type"] == "item_completed"
                && (payload["item"]["id"] == item || payload["item"]["clientId"] == item)
        })
        .and_then(|payload| payload["turn_id"].as_str().map(str::to_owned))
}

fn codex_fork(thread: &str, turn: &str) -> Result<String, String> {
    let mut failures = Vec::new();
    for arguments in [&["app-server", "proxy"][..], &["app-server"][..]] {
        match codex_fork_with(arguments, thread, turn) {
            Ok(fork) => return Ok(fork),
            Err(error) => failures.push(format!("codex {}: {error}", arguments.join(" "))),
        }
    }
    Err(failures.join("; "))
}

fn codex_fork_with(arguments: &[&str], thread: &str, turn: &str) -> Result<String, String> {
    let mut child = Command::new("codex")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn_unmasked()
        .map_err(|error| error.to_string())?;
    let result = codex_exchange(&mut child, thread, turn);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn codex_exchange(child: &mut Child, thread: &str, turn: &str) -> Result<String, String> {
    let mut input = child.stdin.take().ok_or("no stdin")?;
    let output = child.stdout.take().ok_or("no stdout")?;
    let (sender, replies) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<Value>(&line)
                && sender.send(message).is_err()
            {
                break;
            }
        }
    });
    let mut send = |message: Value| {
        writeln!(input, "{message}")
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())
    };
    let reply = |id: u64| -> Result<Value, String> {
        loop {
            let message = replies
                .recv_timeout(CODEX_REPLY)
                .map_err(|_| "no answer from the app server".to_owned())?;
            if message["id"] != id {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error["message"].as_str().unwrap_or("refused").to_owned());
            }
            return Ok(message["result"].clone());
        }
    };
    send(json!({
        "id": 1,
        "method": "initialize",
        "params": { "clientInfo": { "name": "zz", "version": env!("CARGO_PKG_VERSION") } },
    }))?;
    reply(1)?;
    send(json!({ "method": "initialized" }))?;
    send(json!({
        "id": 2,
        "method": "thread/fork",
        "params": { "threadId": thread, "lastTurnId": turn, "excludeTurns": true },
    }))?;
    reply(2)?["thread"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "the fork came back without a thread id".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(value: &Value) -> String {
        value.to_string()
    }

    #[test]
    fn a_claude_fork_keeps_the_chosen_turn_and_drops_the_next_prompt_on() {
        let text = [
            json!({"type":"user","uuid":"u1","sessionId":"s","message":{"role":"user","content":"first"}}),
            json!({"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"s","message":{"content":[]}}),
            json!({"type":"user","uuid":"t1","parentUuid":"a1","sessionId":"s","toolUseResult":{},"message":{"role":"user","content":[{"type":"tool_result"}]}}),
            json!({"type":"assistant","uuid":"a2","parentUuid":"t1","sessionId":"s","message":{"content":[]}}),
            json!({"type":"user","uuid":"u2","parentUuid":"a2","sessionId":"s","message":{"role":"user","content":"second"}}),
            json!({"type":"assistant","uuid":"a3","parentUuid":"u2","sessionId":"s","message":{"content":[]}}),
        ]
        .iter()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n");
        let forked = claude_fork_lines(&text, "u1", "f").expect("fork");
        let uuids = forked
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .map(|entry| {
                assert_eq!(entry["sessionId"], "f");
                entry["uuid"].as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(uuids, ["u1", "a1", "t1", "a2"]);
        assert!(claude_fork_lines(&text, "missing", "f").is_err());
    }

    #[test]
    fn a_codex_message_names_the_turn_it_belongs_to() {
        let text = [
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}),
            json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"t1","item":{"type":"UserMessage","id":"i1"}}}),
            json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"t2","item":{"type":"UserMessage","id":"i2","clientId":"c2"}}}),
        ]
        .iter()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n");
        assert_eq!(codex_turn(&text, "i1").as_deref(), Some("t1"));
        assert_eq!(codex_turn(&text, "c2").as_deref(), Some("t2"));
        assert_eq!(codex_turn(&text, "i3"), None);
    }

    #[test]
    fn ids_that_could_name_another_path_are_refused() {
        assert!(checked_id("01a191e0-981a-7553").is_ok());
        assert!(checked_id("../etc").is_err());
        assert!(checked_id("").is_err());
    }
}
