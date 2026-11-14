use serde::{Deserialize, Serialize};
use zz_protocol::AgentProvider;

use crate::agent_settings::{EFFORT, LensSettings, MODEL};

const FALLBACK_SHELL: &str = "/bin/sh";
const MAX_VALUE_BYTES: usize = 128;

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

fn shell_line(words: &[String]) -> String {
    words
        .iter()
        .map(|word| shell_word(word))
        .collect::<Vec<_>>()
        .join(" ")
}

#[must_use]
pub fn interactive_shell(words: &[String]) -> Vec<String> {
    vec![
        FALLBACK_SHELL.to_owned(),
        "-c".to_owned(),
        format!(
            "exec \"${{SHELL:-{FALLBACK_SHELL}}}\" -ic {}",
            quoted(&shell_line(words))
        ),
    ]
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentLaunch {
    pub model: String,
    pub effort: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentLaunches {
    pub claude: AgentLaunch,
    pub codex: AgentLaunch,
}

impl AgentLaunches {
    #[must_use]
    pub const fn get(&self, provider: AgentProvider) -> &AgentLaunch {
        match provider {
            AgentProvider::ClaudeCode => &self.claude,
            AgentProvider::Codex => &self.codex,
        }
    }

    const fn get_mut(&mut self, provider: AgentProvider) -> &mut AgentLaunch {
        match provider {
            AgentProvider::ClaudeCode => &mut self.claude,
            AgentProvider::Codex => &mut self.codex,
        }
    }

    pub fn pick(&mut self, provider: AgentProvider, option: &str, value: &str) -> bool {
        let value = value.trim();
        if !launch_value(value) {
            return false;
        }
        let launch = self.get_mut(provider);
        let slot = match option {
            MODEL => &mut launch.model,
            EFFORT => &mut launch.effort,
            _ => return false,
        };
        if slot == value {
            return false;
        }
        value.clone_into(slot);
        true
    }

    fn clear(&mut self, provider: AgentProvider, option: &str) -> bool {
        let launch = self.get_mut(provider);
        let slot = match option {
            MODEL => &mut launch.model,
            EFFORT => &mut launch.effort,
            _ => return false,
        };
        let had = !slot.is_empty();
        slot.clear();
        had
    }

    pub fn follow(
        &mut self,
        provider: AgentProvider,
        before: Option<&LensSettings>,
        after: &LensSettings,
    ) -> bool {
        let Some(before) = before else {
            return false;
        };
        let mut changed = false;
        for (option, then, now, picked) in [
            (
                MODEL,
                before.shown_model(),
                after.shown_model(),
                &after.model,
            ),
            (
                EFFORT,
                before.shown_effort(),
                after.shown_effort(),
                &after.effort,
            ),
        ] {
            if !then.is_empty() && then != now {
                changed |= if picked.is_empty() {
                    self.clear(provider, option)
                } else {
                    self.pick(provider, option, picked)
                };
            }
        }
        changed
    }

    #[must_use]
    pub fn agent_flags(&self, provider: AgentProvider) -> Vec<String> {
        let launch = self.get(provider);
        [("--model", &launch.model), ("--effort", &launch.effort)]
            .into_iter()
            .filter(|(_, value)| launch_value(value))
            .flat_map(|(flag, value)| [flag.to_owned(), value.clone()])
            .collect()
    }
}

fn launch_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE_BYTES
        && !value.starts_with('-')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._[]:/".contains(character))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    use super::*;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn settings(model: &str, effort: &str) -> LensSettings {
        LensSettings {
            model: model.to_owned(),
            effort: effort.to_owned(),
            ..LensSettings::default()
        }
    }

    #[test]
    fn agent_commands_run_through_the_users_interactive_shell() {
        assert_eq!(
            shell_line(&words(&[
                "claude", "--model", "opus[1m]", "it's", "-c", "a=\"b\""
            ])),
            r#"claude --model 'opus[1m]' 'it'\''s' -c 'a="b"'"#
        );
        assert_eq!(
            interactive_shell(&words(&["claude", "--resume", "s"])),
            [
                "/bin/sh",
                "-c",
                r#"exec "${SHELL:-/bin/sh}" -ic 'claude --resume s'"#
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_shell_expands_aliases_and_falls_back_without_shell() {
        let directory = tempfile::tempdir().expect("tempdir");
        let rc = directory.path().join("rc");
        std::fs::write(&rc, "alias agent='printf \"%s|\"'\n").expect("rc");
        let shell = directory.path().join("shell");
        std::fs::write(
            &shell,
            format!("#!/bin/sh\nexec bash --rcfile '{}' \"$@\"\n", rc.display()),
        )
        .expect("shell");
        let mut permissions = std::fs::metadata(&shell).expect("shell").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&shell, permissions).expect("chmod");
        let run = |shell: Option<&std::path::Path>, line: &[&str]| {
            let argv = interactive_shell(&words(line));
            let mut command = Command::new(&argv[0]);
            command
                .args(&argv[1..])
                .env("HOME", directory.path())
                .env_remove("ENV")
                .env_remove("BASH_ENV")
                .stdin(std::process::Stdio::null());
            match shell {
                Some(shell) => command.env("SHELL", shell),
                None => command.env_remove("SHELL"),
            };
            let output = command.output_unmasked().expect("run");
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        assert_eq!(
            run(Some(&shell), &["agent", "--model", "opus[1m]", "it's"]),
            "--model|opus[1m]|it's|"
        );
        assert_eq!(run(None, &["agent", "x"]), "");
        assert_eq!(run(None, &["printf", "%s|", "x"]), "x|");
    }

    #[test]
    fn new_chats_start_with_the_model_and_effort_last_picked() {
        let mut launches = AgentLaunches::default();
        assert!(launches.agent_flags(AgentProvider::ClaudeCode).is_empty());
        assert!(launches.pick(AgentProvider::ClaudeCode, MODEL, "opus"));
        assert!(!launches.pick(AgentProvider::ClaudeCode, MODEL, "opus"));
        assert!(!launches.pick(AgentProvider::ClaudeCode, MODEL, "--danger"));
        assert!(!launches.pick(AgentProvider::ClaudeCode, MODEL, "opus; rm"));
        assert!(!launches.pick(AgentProvider::ClaudeCode, "cwd", "/tmp"));
        assert!(launches.follow(
            AgentProvider::ClaudeCode,
            Some(&settings("opus", "high")),
            &settings("opus", "xhigh"),
        ));
        assert!(!launches.follow(AgentProvider::Codex, None, &settings("gpt-5.5", "high")));
        assert!(!launches.follow(
            AgentProvider::Codex,
            Some(&settings("", "")),
            &settings("gpt-5.5", "high"),
        ));
        assert!(launches.follow(
            AgentProvider::Codex,
            Some(&settings("gpt-5.5", "")),
            &settings("gpt-5.5-codex", "high"),
        ));
        assert_eq!(
            launches.agent_flags(AgentProvider::ClaudeCode),
            ["--model", "opus", "--effort", "xhigh"]
        );
        assert_eq!(
            launches.agent_flags(AgentProvider::Codex),
            ["--model", "gpt-5.5-codex"]
        );
        let saved = serde_json::to_string(&launches).expect("json");
        assert_eq!(
            serde_json::from_str::<AgentLaunches>(&saved).expect("read back"),
            launches
        );
        assert_eq!(
            serde_json::from_str::<AgentLaunches>("{}").expect("empty"),
            AgentLaunches::default()
        );
    }
}
