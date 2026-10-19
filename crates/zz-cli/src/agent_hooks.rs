use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde_json::{Map, Value, json};
use zz_daemon::{
    WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, WORKSPACE_STUB_SKILL, workspace_context,
};

const HOOK_MARKER: &str = "zz-agent-context";
const HOOK_COMMAND: &str = r#"/bin/sh -c ': zz-agent-context; [ -n "${ZZ_PANE-}" ] && "${ZZ_TMUX_EXECUTABLE:-zz}" tools --context 2>/dev/null; true'"#;
const HOOK_TIMEOUT_SECONDS: u64 = 5;
const STUB_MARKER: &str = "zz-stub:";
const SKILL_NAME: &str = "zz-workspace";

pub(crate) fn run(flag: &str, remote: bool) -> Option<ExitCode> {
    let install = match flag {
        "--context" => {
            if let Some(text) = context(|name| std::env::var_os(name)) {
                print!("{text}");
            }
            return Some(ExitCode::SUCCESS);
        }
        "--install-hooks" => true,
        "--remove-hooks" => false,
        _ => return None,
    };
    if remote {
        eprintln!("tools {flag} is local only");
        return Some(ExitCode::from(2));
    }
    let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
        eprintln!("tools {flag}: HOME is not set");
        return Some(ExitCode::from(1));
    };
    let home = PathBuf::from(home);
    let mut failed = false;
    for agent in agents(&home, |name| std::env::var_os(name)) {
        for line in if install {
            agent.install()
        } else {
            agent.remove()
        } {
            match line {
                Ok(line) => println!("{}: {line}", agent.name),
                Err(line) => {
                    failed = true;
                    eprintln!("{}: {line}", agent.name);
                }
            }
        }
    }
    Some(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn context(var: impl Fn(&str) -> Option<OsString>) -> Option<String> {
    let set = |name: &str| var(name).filter(|value| !value.is_empty());
    if set(WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE).is_some() {
        return None;
    }
    let pane = set("ZZ_PANE")?;
    let session = set("ZZ_SESSION").unwrap_or_default();
    Some(workspace_context(
        &pane.to_string_lossy(),
        &session.to_string_lossy(),
        false,
    ))
}

struct Agent {
    name: &'static str,
    config: PathBuf,
    hooks: PathBuf,
    skill: PathBuf,
}

fn agents(home: &Path, var: impl Fn(&str) -> Option<OsString>) -> [Agent; 2] {
    let dir = |name: &str, default: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map_or_else(|| home.join(default), PathBuf::from)
    };
    let claude = dir("CLAUDE_CONFIG_DIR", ".claude");
    let codex = dir("CODEX_HOME", ".codex");
    [
        Agent {
            name: "Claude Code",
            hooks: claude.join("settings.json"),
            skill: claude.join("skills").join(SKILL_NAME).join("SKILL.md"),
            config: claude,
        },
        Agent {
            name: "Codex",
            hooks: codex.join("hooks.json"),
            skill: home
                .join(".agents")
                .join("skills")
                .join(SKILL_NAME)
                .join("SKILL.md"),
            config: codex,
        },
    ]
}

type Line = Result<String, String>;

impl Agent {
    fn install(&self) -> Vec<Line> {
        if !self.config.is_dir() {
            return vec![Ok(format!(
                "skipped, {} does not exist",
                self.config.display()
            ))];
        }
        vec![self.install_hook(), self.install_skill()]
    }

    fn remove(&self) -> Vec<Line> {
        vec![self.remove_hook(), self.remove_skill()]
    }

    fn install_hook(&self) -> Line {
        let path = self.hooks.display();
        let mut root = read_object(&self.hooks)?;
        let starts = session_start(&mut root)?;
        let ours = starts
            .iter_mut()
            .filter_map(|entry| entry.get_mut("hooks")?.as_array_mut())
            .flatten()
            .find(|hook| is_ours(hook));
        let change = match ours {
            Some(hook) if hook["command"] == HOOK_COMMAND => {
                return Ok(format!("hook current in {path}"));
            }
            Some(hook) => {
                hook["command"] = HOOK_COMMAND.into();
                hook["timeout"] = HOOK_TIMEOUT_SECONDS.into();
                "updated"
            }
            None => {
                starts.push(json!({
                    "hooks": [{
                        "type": "command",
                        "command": HOOK_COMMAND,
                        "timeout": HOOK_TIMEOUT_SECONDS,
                    }],
                }));
                "added"
            }
        };
        write_object(&self.hooks, &root)
            .map_err(|error| format!("could not write {path}: {error}"))?;
        Ok(format!("hook {change} in {path}"))
    }

    fn remove_hook(&self) -> Line {
        let path = self.hooks.display();
        if !self.hooks.exists() {
            return Ok(format!("no hook, {path} does not exist"));
        }
        let mut root = read_object(&self.hooks)?;
        let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
            return Ok(format!("no hook in {path}"));
        };
        let Some(starts) = hooks.get_mut("SessionStart").and_then(Value::as_array_mut) else {
            return Ok(format!("no hook in {path}"));
        };
        let mut removed = false;
        for entry in starts.iter_mut() {
            if let Some(inner) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                let count = inner.len();
                inner.retain(|hook| !is_ours(hook));
                removed |= inner.len() != count;
            }
        }
        if !removed {
            return Ok(format!("no hook in {path}"));
        }
        starts.retain(|entry| {
            entry
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|inner| !inner.is_empty())
        });
        if starts.is_empty() {
            hooks.remove("SessionStart");
        }
        if hooks.is_empty() {
            root.remove("hooks");
        }
        write_object(&self.hooks, &root)
            .map_err(|error| format!("could not write {path}: {error}"))?;
        Ok(format!("hook removed from {path}"))
    }

    fn install_skill(&self) -> Line {
        let path = self.skill.display();
        let change = match fs::read_to_string(&self.skill) {
            Ok(text) if text == WORKSPACE_STUB_SKILL => {
                return Ok(format!("skill current at {path}"));
            }
            Ok(text) if !text.contains(STUB_MARKER) => {
                return Ok(format!("skill left alone at {path}, zz did not install it"));
            }
            Ok(_) => "updated",
            Err(error) if error.kind() == io::ErrorKind::NotFound => "added",
            Err(error) => return Err(format!("could not read {path}: {error}")),
        };
        if let Some(parent) = self.skill.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        write_atomically(&self.skill, WORKSPACE_STUB_SKILL.as_bytes())
            .map_err(|error| format!("could not write {path}: {error}"))?;
        Ok(format!("skill {change} at {path}"))
    }

    fn remove_skill(&self) -> Line {
        let path = self.skill.display();
        match fs::read_to_string(&self.skill) {
            Ok(text) if text.contains(STUB_MARKER) => {
                fs::remove_file(&self.skill)
                    .map_err(|error| format!("could not remove {path}: {error}"))?;
                if let Some(parent) = self.skill.parent() {
                    let _ = fs::remove_dir(parent);
                }
                Ok(format!("skill removed from {path}"))
            }
            Ok(_) => Ok(format!("skill left alone at {path}, zz did not install it")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(format!("no skill at {path}"))
            }
            Err(error) => Err(format!("could not read {path}: {error}")),
        }
    }
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(HOOK_MARKER))
}

fn session_start(root: &mut Map<String, Value>) -> Result<&mut Vec<Value>, String> {
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("\"hooks\" is not an object; left alone")?;
    hooks
        .entry("SessionStart")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "\"hooks.SessionStart\" is not an array; left alone".to_owned())
}

fn read_object(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(&text) {
        Ok(Value::Object(root)) => Ok(root),
        Ok(_) => Err(format!(
            "{} is not a JSON object; left alone",
            path.display()
        )),
        Err(error) => Err(format!(
            "could not parse {}: {error}; left alone",
            path.display()
        )),
    }
}

fn write_object(path: &Path, root: &Map<String, Value>) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(root).map_err(io::Error::other)?;
    text.push('\n');
    write_atomically(path, text.as_bytes())
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".zz-tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect::<Vec<_>>();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    fn agents_in(home: &Path) -> [Agent; 2] {
        agents(home, env(&[]))
    }

    fn read(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
    }

    fn session_start_commands(path: &Path) -> Vec<String> {
        read(path)["hooks"]["SessionStart"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
            .filter_map(|hook| hook["command"].as_str().map(str::to_owned))
            .collect()
    }

    #[test]
    fn context_needs_a_pane_and_stays_quiet_once_sent() {
        assert_eq!(context(env(&[])), None);
        assert_eq!(context(env(&[("ZZ_PANE", "")])), None);
        let text = context(env(&[("ZZ_PANE", "%7"), ("ZZ_SESSION", "work")])).expect("context");
        assert!(text.contains("ZZ_PANE=%7\n"));
        assert!(text.contains("ZZ_SESSION=work\n"));
        assert!(text.contains("'#{pane_id}'"));
        assert_eq!(
            context(env(&[
                ("ZZ_PANE", "%7"),
                (WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, "1")
            ])),
            None
        );
    }

    #[test]
    fn install_adds_one_hook_beside_existing_ones_and_is_idempotent() {
        let home = tempfile::tempdir().expect("home");
        let claude = home.path().join(".claude");
        fs::create_dir_all(&claude).expect("claude dir");
        let settings = claude.join("settings.json");
        fs::write(
            &settings,
            r#"{"model":"opus","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"rex hook"}]}],"Stop":[]}}"#,
        )
        .expect("settings");
        let [claude_agent, codex_agent] = agents_in(home.path());
        let lines = claude_agent.install();
        assert!(lines.iter().all(Result::is_ok), "{lines:?}");
        assert_eq!(
            session_start_commands(&settings),
            ["rex hook", HOOK_COMMAND]
        );
        let root = read(&settings);
        assert_eq!(root["model"], "opus");
        assert_eq!(
            root.as_object()
                .expect("object")
                .keys()
                .next()
                .map(String::as_str),
            Some("model")
        );
        assert!(root["hooks"]["Stop"].is_array());
        assert_eq!(
            fs::read_to_string(claude.join("skills/zz-workspace/SKILL.md")).expect("skill"),
            WORKSPACE_STUB_SKILL
        );
        let again = claude_agent.install();
        assert_eq!(
            again[0].as_deref(),
            Ok(format!("hook current in {}", settings.display()).as_str())
        );
        assert_eq!(session_start_commands(&settings).len(), 2);
        assert!(
            codex_agent.install()[0]
                .as_ref()
                .expect("skip")
                .starts_with("skipped")
        );
        assert!(!home.path().join(".agents").exists());
    }

    #[test]
    fn install_updates_an_older_zz_hook_and_leaves_a_foreign_skill() {
        let home = tempfile::tempdir().expect("home");
        let codex = home.path().join(".codex");
        fs::create_dir_all(&codex).expect("codex dir");
        let hooks = codex.join("hooks.json");
        fs::write(
            &hooks,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"sh -c ': zz-agent-context; old'"}]}]}}"#,
        )
        .expect("hooks");
        let skill = home.path().join(".agents/skills/zz-workspace/SKILL.md");
        fs::create_dir_all(skill.parent().expect("parent")).expect("skill dir");
        fs::write(&skill, "---\nname: zz-workspace\n---\nmine\n").expect("skill");
        let [_, codex_agent] = agents_in(home.path());
        let lines = codex_agent.install();
        assert!(lines[0].as_ref().expect("hook").starts_with("hook updated"));
        assert!(lines[1].as_ref().expect("skill").contains("left alone"));
        assert_eq!(session_start_commands(&hooks), [HOOK_COMMAND]);
        assert_eq!(
            fs::read_to_string(&skill).expect("skill"),
            "---\nname: zz-workspace\n---\nmine\n"
        );
    }

    #[test]
    fn remove_takes_out_only_what_zz_installed() {
        let home = tempfile::tempdir().expect("home");
        let claude = home.path().join(".claude");
        fs::create_dir_all(&claude).expect("claude dir");
        let settings = claude.join("settings.json");
        fs::write(
            &settings,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"rex hook"}]}]}}"#,
        )
        .expect("settings");
        let [claude_agent, codex_agent] = agents_in(home.path());
        assert!(claude_agent.install().iter().all(Result::is_ok));
        let lines = claude_agent.remove();
        assert!(lines.iter().all(Result::is_ok), "{lines:?}");
        assert_eq!(session_start_commands(&settings), ["rex hook"]);
        assert!(!claude.join("skills/zz-workspace").exists());
        assert!(
            claude_agent.remove()[0]
                .as_ref()
                .expect("again")
                .starts_with("no hook")
        );
        assert!(codex_agent.remove().iter().all(Result::is_ok));
    }

    #[test]
    fn remove_drops_containers_zz_created() {
        let home = tempfile::tempdir().expect("home");
        let claude = home.path().join(".claude");
        fs::create_dir_all(&claude).expect("claude dir");
        let settings = claude.join("settings.json");
        fs::write(&settings, r#"{"theme":"dark"}"#).expect("settings");
        let [claude_agent, _] = agents_in(home.path());
        assert!(claude_agent.install().iter().all(Result::is_ok));
        assert!(claude_agent.remove().iter().all(Result::is_ok));
        assert_eq!(read(&settings), json!({"theme": "dark"}));
    }

    #[test]
    fn unparsable_settings_are_left_alone() {
        let home = tempfile::tempdir().expect("home");
        let claude = home.path().join(".claude");
        fs::create_dir_all(&claude).expect("claude dir");
        let settings = claude.join("settings.json");
        fs::write(&settings, "{ not json").expect("settings");
        let [claude_agent, _] = agents_in(home.path());
        assert!(claude_agent.install()[0].is_err());
        assert_eq!(
            fs::read_to_string(&settings).expect("settings"),
            "{ not json"
        );
    }

    #[test]
    fn agent_config_directories_follow_their_environment_variables() {
        let home = Path::new("/home/u");
        let [claude, codex] = agents(
            home,
            env(&[("CLAUDE_CONFIG_DIR", "/c"), ("CODEX_HOME", "/x")]),
        );
        assert_eq!(claude.hooks, Path::new("/c/settings.json"));
        assert_eq!(claude.skill, Path::new("/c/skills/zz-workspace/SKILL.md"));
        assert_eq!(codex.hooks, Path::new("/x/hooks.json"));
        assert_eq!(
            codex.skill,
            Path::new("/home/u/.agents/skills/zz-workspace/SKILL.md")
        );
    }
}
