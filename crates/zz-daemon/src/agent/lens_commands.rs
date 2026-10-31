use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use zz_protocol::AgentProvider;

const MAX_COMMANDS: usize = 240;
const MAX_DESCRIPTION: usize = 160;
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_COMMAND_DEPTH: usize = 3;

struct Builtin {
    name: &'static str,
    description: &'static str,
    hint: Option<&'static str>,
    terminal: bool,
}

const fn lens(
    name: &'static str,
    description: &'static str,
    hint: Option<&'static str>,
) -> Builtin {
    Builtin {
        name,
        description,
        hint,
        terminal: false,
    }
}

const fn tui(name: &'static str, description: &'static str) -> Builtin {
    Builtin {
        name,
        description,
        hint: None,
        terminal: true,
    }
}

const CLAUDE: &[Builtin] = &[
    lens("clear", "Start a new session with empty context", None),
    lens(
        "compact",
        "Free up context by summarizing the conversation so far",
        Some("instructions"),
    ),
    lens(
        "goal",
        "Set a goal Claude checks before stopping",
        Some("goal"),
    ),
    lens(
        "subtask",
        "Send a subagent off with your full context",
        Some("task"),
    ),
    lens(
        "cd",
        "Move this session to a new working directory",
        Some("path"),
    ),
    lens("add-dir", "Add a new working directory", Some("path")),
    lens("export", "Export the conversation to a file", Some("file")),
    lens("copy", "Copy Claude's last response to the clipboard", None),
    lens(
        "reload-skills",
        "Pick up skills added or changed on disk",
        None,
    ),
    lens("exit", "Quit Claude Code", None),
    tui("model", "Pick the model in the terminal"),
    tui("effort", "Pick the effort level in the terminal"),
    tui("plan", "Plan mode, in the terminal"),
    tui(
        "status",
        "Version, model, account and connectivity, in the terminal",
    ),
    tui("config", "Open Claude Code's settings in the terminal"),
    tui("context", "Context usage grid, in the terminal"),
    tui(
        "usage",
        "Session cost, plan usage and stats, in the terminal",
    ),
    tui("permissions", "Allow and deny rules, in the terminal"),
    tui("mcp", "Manage MCP servers in the terminal"),
    tui("hooks", "Hook configurations, in the terminal"),
    tui("memory", "Edit CLAUDE.md files in the terminal"),
    tui("resume", "Resume a previous conversation in the terminal"),
    tui("rewind", "Go back to an earlier prompt in the terminal"),
    tui("plugin", "Manage plugins in the terminal"),
    tui("skills", "Browse skills in the terminal"),
    tui("btw", "Ask a side question in the terminal"),
    tui("doctor", "Check the installation in the terminal"),
    tui("theme", "Change Claude Code's theme in the terminal"),
    tui("login", "Sign in in the terminal"),
    tui("logout", "Sign out in the terminal"),
    tui("help", "Claude Code's help, in the terminal"),
];

const CODEX: &[Builtin] = &[
    lens("new", "Start a new chat", None),
    lens(
        "compact",
        "Summarize the conversation to free context",
        None,
    ),
    lens("review", "Review the current changes and find issues", None),
    lens(
        "init",
        "Create an AGENTS.md file with instructions for Codex",
        None,
    ),
    lens("diff", "Show the git diff, untracked files included", None),
    lens("rename", "Rename the current thread", Some("name")),
    lens("quit", "Quit Codex", None),
    tui(
        "model",
        "Pick the model and reasoning effort in the terminal",
    ),
    tui("permissions", "Choose what Codex may do, in the terminal"),
    tui(
        "status",
        "Session configuration and token usage, in the terminal",
    ),
    tui("mcp", "Configured MCP tools, in the terminal"),
    tui("resume", "Resume a saved chat in the terminal"),
    tui("fork", "Fork the current chat in the terminal"),
    tui("mention", "Mention a file, in the terminal"),
    tui("logout", "Sign out of Codex in the terminal"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) name: String,
    pub(crate) description: String,
}

struct Found {
    name: String,
    description: String,
    hint: Option<String>,
    skill: bool,
}

pub(crate) fn skill_listing(entry: &Value) -> Option<Vec<Listed>> {
    let attachment = &entry["attachment"];
    if entry["type"] != "attachment" || attachment["type"] != "skill_listing" {
        return None;
    }
    let content = attachment["content"].as_str()?;
    Some(
        content
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .filter_map(|line| line.split_once(": "))
            .filter(|(name, _)| !name.is_empty() && !name.contains(char::is_whitespace))
            .map(|(name, description)| Listed {
                name: name.to_owned(),
                description: description.trim().to_owned(),
            })
            .collect(),
    )
}

pub(crate) fn update(
    provider: AgentProvider,
    cwd: &Path,
    home: Option<&Path>,
    listed: &[Listed],
) -> Value {
    let (builtins, found) = match provider {
        AgentProvider::ClaudeCode => (CLAUDE, claude_found(cwd, home, listed)),
        AgentProvider::Codex => (CODEX, codex_found(cwd, home)),
    };
    let mut seen = BTreeSet::new();
    let mut commands = Vec::new();
    for builtin in builtins {
        if seen.insert(builtin.name.to_owned()) {
            commands.push(entry(
                builtin.name,
                builtin.description,
                builtin.hint,
                builtin.terminal,
                false,
            ));
        }
    }
    for found in found {
        if commands.len() >= MAX_COMMANDS {
            break;
        }
        if seen.insert(found.name.clone()) {
            commands.push(entry(
                &found.name,
                &found.description,
                found.hint.as_deref(),
                false,
                found.skill,
            ));
        }
    }
    json!({ "sessionUpdate": "available_commands_update", "availableCommands": commands })
}

fn entry(name: &str, description: &str, hint: Option<&str>, terminal: bool, skill: bool) -> Value {
    let mut entry = json!({
        "name": name,
        "description": clip(description),
    });
    if let Some(hint) = hint.filter(|hint| !hint.is_empty()) {
        entry["input"] = json!({ "hint": hint });
    }
    if terminal || skill {
        entry["_meta"] = json!({ "zz": { "terminal": terminal, "skill": skill } });
    }
    entry
}

fn clip(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(MAX_DESCRIPTION) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text,
    }
}

fn claude_found(cwd: &Path, home: Option<&Path>, listed: &[Listed]) -> Vec<Found> {
    let config = claude_config(home);
    let projects = project_dirs(cwd, home);
    let mut found = Vec::new();
    for root in projects
        .iter()
        .map(|dir| dir.join(".claude"))
        .chain(config.clone())
    {
        found.extend(skills(&root.join("skills"), ""));
        found.extend(commands(&root.join("commands"), ""));
    }
    if let Some(config) = &config {
        for (plugin, root) in plugins(config, &projects) {
            let prefix = format!("{plugin}:");
            found.extend(skills(&root.join("skills"), &prefix));
            found.extend(commands(&root.join("commands"), &prefix));
        }
    }
    found.extend(listed.iter().map(|listed| Found {
        name: listed.name.clone(),
        description: listed.description.clone(),
        hint: None,
        skill: true,
    }));
    found
}

fn codex_found(cwd: &Path, home: Option<&Path>) -> Vec<Found> {
    let config = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".codex")));
    let mut found = Vec::new();
    for root in project_dirs(cwd, home)
        .iter()
        .map(|dir| dir.join(".codex"))
        .chain(config.clone())
    {
        found.extend(skills(&root.join("skills"), "").map(|skill| Found {
            name: format!("${}", skill.name),
            ..skill
        }));
    }
    if let Some(config) = &config {
        found.extend(commands(&config.join("prompts"), "prompts:"));
    }
    found
}

fn claude_config(home: Option<&Path>) -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".claude")))
}

fn project_dirs(cwd: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in cwd.ancestors() {
        if Some(dir) == home || dir.parent().is_none() {
            break;
        }
        dirs.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            break;
        }
    }
    dirs
}

fn skills<'a>(dir: &Path, prefix: &'a str) -> impl Iterator<Item = Found> + 'a {
    let mut entries = read_dir(dir);
    entries.sort();
    entries.into_iter().filter_map(move |path| {
        let text = read_small(&path.join("SKILL.md"))?;
        let front = frontmatter(&text);
        let name = front
            .iter()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| path.file_name()?.to_str().map(str::to_owned))?;
        Some(Found {
            name: format!("{prefix}{name}"),
            description: value(&front, "description").unwrap_or_default(),
            hint: value(&front, "argument-hint"),
            skill: true,
        })
    })
}

fn commands(dir: &Path, prefix: &str) -> Vec<Found> {
    let mut found = Vec::new();
    walk_commands(dir, prefix, 0, &mut found);
    found
}

fn walk_commands(dir: &Path, prefix: &str, depth: usize, found: &mut Vec<Found>) {
    let mut entries = read_dir(dir);
    entries.sort();
    for path in entries {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if depth < MAX_COMMAND_DEPTH {
                walk_commands(&path, &format!("{prefix}{stem}:"), depth + 1, found);
            }
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(text) = read_small(&path) else {
            continue;
        };
        let front = frontmatter(&text);
        let description = value(&front, "description").unwrap_or_else(|| first_line(&text));
        found.push(Found {
            name: format!("{prefix}{stem}"),
            description,
            hint: value(&front, "argument-hint"),
            skill: false,
        });
    }
}

fn plugins(config: &Path, projects: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let read = |path: PathBuf| {
        read_small(&path).and_then(|text| serde_json::from_str::<Value>(&text).ok())
    };
    let mut enabled = BTreeSet::new();
    for settings in
        std::iter::once(config.join("settings.json")).chain(projects.iter().flat_map(|dir| {
            ["settings.json", "settings.local.json"].map(|file| dir.join(".claude").join(file))
        }))
    {
        for (key, on) in read(settings)
            .and_then(|settings| settings["enabledPlugins"].as_object().cloned())
            .into_iter()
            .flatten()
        {
            if on == true {
                enabled.insert(key);
            } else {
                enabled.remove(&key);
            }
        }
    }
    let Some(installed) = read(config.join("plugins").join("installed_plugins.json")) else {
        return Vec::new();
    };
    let Some(installed) = installed["plugins"].as_object() else {
        return Vec::new();
    };
    enabled
        .into_iter()
        .filter_map(|key| {
            let installs = installed.get(&key)?.as_array()?;
            let install = installs
                .iter()
                .find(|install| {
                    install["projectPath"]
                        .as_str()
                        .is_some_and(|path| projects.iter().any(|dir| dir == Path::new(path)))
                })
                .or_else(|| installs.iter().find(|install| install["scope"] == "user"))?;
            let root = PathBuf::from(install["installPath"].as_str()?);
            let name = key.split_once('@').map_or(key.as_str(), |(name, _)| name);
            Some((name.to_owned(), root))
        })
        .collect()
}

fn read_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.'))
        })
        .collect()
}

fn read_small(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    std::io::read_to_string(file).ok()
}

fn frontmatter(text: &str) -> Vec<(String, String)> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Vec::new();
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = pairs.last_mut() {
                if !value.is_empty() {
                    value.push(' ');
                }
                value.push_str(line.trim());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let value = if matches!(value, ">" | "|" | ">-" | "|-") {
            ""
        } else {
            value
        };
        pairs.push((key.trim().to_owned(), unquote(value).to_owned()));
    }
    pairs
}

fn value(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn first_line(text: &str) -> String {
    let body = if text.starts_with("---") {
        text.split_once("\n---").map_or("", |(_, body)| body)
    } else {
        text
    };
    body.lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn names(update: &Value) -> Vec<String> {
        update["availableCommands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn command<'a>(update: &'a Value, name: &str) -> &'a Value {
        update["availableCommands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == name)
            .unwrap()
    }

    #[test]
    fn claude_lists_builtins_skills_commands_and_plugins_from_disk() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let repo = home.join("dev/zz");
        let cwd = repo.join("crates");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        write(
            &home.join(".claude/skills/okf/SKILL.md"),
            "---\nname: okf\ndescription: >\n  Author knowledge\n  bundles\n---\n# OKF\n",
        );
        write(
            &repo.join(".claude/skills/ghostty-sync/SKILL.md"),
            "---\nname: ghostty-sync\ndescription: \"Keep Ghostty in sync\"\n---\n",
        );
        write(
            &repo.join(".claude/commands/frontend/lint.md"),
            "---\nargument-hint: [path]\n---\nLint the frontend\n",
        );
        write(
            &home.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"codex@codex-cc":true,"off@x":false}}"#,
        );
        let plugin = root.path().join("plugins/codex");
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &format!(
                r#"{{"plugins":{{"codex@codex-cc":[{{"scope":"user","installPath":"{}"}}],"off@x":[{{"scope":"user","installPath":"/nowhere"}}]}}}}"#,
                plugin.display()
            ),
        );
        write(
            &plugin.join("skills/setup/SKILL.md"),
            "---\ndescription: Check the Codex CLI\n---\n",
        );
        let listed = [Listed {
            name: "simplify".to_owned(),
            description: "Review the changed code".to_owned(),
        }];
        let update = update(AgentProvider::ClaudeCode, &cwd, Some(&home), &listed);
        let names = names(&update);
        for expected in [
            "clear",
            "status",
            "okf",
            "ghostty-sync",
            "frontend:lint",
            "codex:setup",
            "simplify",
        ] {
            assert!(
                names.contains(&expected.to_owned()),
                "{expected} in {names:?}"
            );
        }
        assert_eq!(
            command(&update, "okf")["description"],
            "Author knowledge bundles"
        );
        assert_eq!(command(&update, "okf")["_meta"]["zz"]["skill"], true);
        assert_eq!(
            command(&update, "frontend:lint")["description"],
            "Lint the frontend"
        );
        assert_eq!(command(&update, "frontend:lint")["input"]["hint"], "[path]");
        assert_eq!(command(&update, "status")["_meta"]["zz"]["terminal"], true);
        assert!(command(&update, "compact")["_meta"].is_null());
        assert_eq!(names.iter().filter(|name| *name == "okf").count(), 1);
    }

    #[test]
    fn codex_skills_are_dollar_mentions_and_prompts_are_namespaced() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        write(
            &home.join(".codex/skills/hatch-pet/SKILL.md"),
            "---\nname: hatch-pet\ndescription: Make a pet\n---\n",
        );
        write(&home.join(".codex/prompts/ship.md"), "Ship the branch\n");
        let cwd = home.join("dev");
        std::fs::create_dir_all(&cwd).unwrap();
        let update = update(AgentProvider::Codex, &cwd, Some(&home), &[]);
        let names = names(&update);
        assert!(names.contains(&"$hatch-pet".to_owned()), "{names:?}");
        assert!(names.contains(&"prompts:ship".to_owned()), "{names:?}");
        assert_eq!(command(&update, "model")["_meta"]["zz"]["terminal"], true);
    }

    #[test]
    fn a_skill_listing_attachment_names_the_skills_claude_loaded() {
        let entry = json!({
            "type": "attachment",
            "attachment": {
                "type": "skill_listing",
                "content": "- okf: Author bundles\n- codex:setup: Check the CLI\nnot a skill\n",
            },
        });
        assert_eq!(
            skill_listing(&entry).unwrap(),
            [
                Listed {
                    name: "okf".to_owned(),
                    description: "Author bundles".to_owned(),
                },
                Listed {
                    name: "codex:setup".to_owned(),
                    description: "Check the CLI".to_owned(),
                },
            ]
        );
        assert!(skill_listing(&json!({ "type": "user" })).is_none());
    }
}
