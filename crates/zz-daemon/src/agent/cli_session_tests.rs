use serde_json::json;
use zz_protocol::AgentTaskWire;

use super::*;

const SESSION: &str = "0b1c2d3e-4f50-4a6b-8c7d-9e0f1a2b3c4d";
const PANE_PID: u32 = 4242;

fn words(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

fn write_lines(path: &Path, lines: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, text + "\n").unwrap();
}

fn claude_fixture(status: &str) -> (tempfile::TempDir, PathBuf, PathBuf, u32) {
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("sessions");
    let config = root.path().join("config");
    let project = root.path().join("project");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let cwd = std::fs::canonicalize(&project).unwrap();
    let pid = std::process::id();
    std::fs::write(
        registry.join(format!("{pid}.json")),
        json!({
            "pid": pid,
            "sessionId": SESSION,
            "cwd": cwd,
            "status": status,
            "waitingFor": if status == "waiting" { "permission prompt" } else { "" },
        })
        .to_string(),
    )
    .unwrap();
    let sanitized = cwd
        .to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    write_lines(
        &config
            .join("projects")
            .join(sanitized)
            .join(format!("{SESSION}.jsonl")),
        &[
            json!({ "type": "user", "cwd": cwd, "message": { "role": "user", "content": "hi" } }),
            json!({
                "type": "assistant",
                "cwd": cwd,
                "effort": "high",
                "message": { "model": "claude-haiku-4-5", "content": [] },
            }),
        ],
    );
    (root, registry, config, pid)
}

#[test]
fn a_claude_session_record_names_the_session_folder_settings_and_running_turn() {
    let (_root, registry, config, pid) = claude_fixture("idle");
    let parent_of = |child: u32| (child == pid).then_some(PANE_PID);
    let found = claude_session(
        &registry,
        Some(&config),
        PANE_PID,
        parent_of,
        |_| true,
        |_| words(&["claude", "--model", "opus"]),
    )
    .expect("the record under the pane");
    assert_eq!(found.session_id.as_deref(), Some(SESSION));
    assert_eq!(found.pid, Some(pid));
    assert!(
        found
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.ends_with("project"))
    );
    assert_eq!(found.model.as_deref(), Some("claude-haiku-4-5"));
    assert_eq!(found.effort.as_deref(), Some("high"));
    assert!(found.running.is_empty());

    assert!(
        claude_session(
            &registry,
            Some(&config),
            1,
            parent_of,
            |_| true,
            |_| Vec::new()
        )
        .is_none(),
        "a record under another pane is not this pane's"
    );
    assert!(
        claude_session(
            &registry,
            Some(&config),
            PANE_PID,
            parent_of,
            |_| false,
            |_| Vec::new()
        )
        .is_none(),
        "a dead process's record is stale"
    );

    let (_root, registry, config, pid) = claude_fixture("waiting");
    let parent_of = |child: u32| (child == pid).then_some(PANE_PID);
    let busy = claude_session(
        &registry,
        Some(&config),
        PANE_PID,
        parent_of,
        |_| true,
        |_| Vec::new(),
    )
    .unwrap();
    assert_eq!(busy.running, ["a turn waiting for a permission prompt"]);
}

#[test]
fn a_fresh_claude_keeps_its_launch_model_and_names_no_session() {
    let fresh = claude_fresh(
        Some(7),
        None,
        &words(&["claude", "--model", "Sonnet", "--effort", "low"]),
        Some(PathBuf::from("/work")),
    );
    assert_eq!(
        fresh,
        CliSession {
            pid: Some(7),
            session_id: None,
            cwd: Some(PathBuf::from("/work")),
            model: Some("sonnet".to_owned()),
            effort: Some("low".to_owned()),
            running: Vec::new(),
        }
    );
}

#[test]
fn a_codex_rollout_names_its_thread_settings_and_open_turn() {
    let root = tempfile::tempdir().unwrap();
    let sessions = root.path().join("sessions");
    let thread = "019a0000-0000-7000-8000-000000000001";
    let rollout = sessions
        .join("2026/11/01")
        .join(format!("rollout-2026-11-01T10-00-00-{thread}.jsonl"));
    let mut lines = vec![
        json!({ "type": "session_meta", "payload": { "id": thread, "cwd": "/repo" } }),
        json!({ "type": "turn_context", "payload": { "model": "gpt-6-luna", "effort": "low", "cwd": "/repo" } }),
        json!({ "type": "event_msg", "payload": { "type": "task_started" } }),
    ];
    write_lines(&rollout, &lines);
    let args = words(&["codex", "-m", "gpt-x", "resume", thread]);
    let found = codex_session(Some(&sessions), None, &args, None, None);
    assert_eq!(found.session_id.as_deref(), Some(thread));
    assert_eq!(found.cwd.as_deref(), Some(Path::new("/repo")));
    assert_eq!(found.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(found.effort.as_deref(), Some("low"));
    assert_eq!(found.running, ["a turn"]);

    lines.push(json!({ "type": "event_msg", "payload": { "type": "task_complete" } }));
    write_lines(&rollout, &lines);
    let idle = codex_session(None, None, &words(&["codex"]), None, Some(rollout));
    assert_eq!(idle.session_id.as_deref(), Some(thread));
    assert!(idle.running.is_empty());

    let fresh = codex_session(
        Some(&sessions),
        None,
        &words(&["codex", "-c", "model=\"o3\""]),
        Some(PathBuf::from("/elsewhere")),
        None,
    );
    assert_eq!(fresh.session_id, None);
    assert_eq!(fresh.model.as_deref(), Some("o3"));
}

#[test]
fn resume_arguments_carry_the_session_model_and_effort_through_the_users_shell() {
    assert_eq!(
        cli_command(
            AgentProvider::ClaudeCode,
            Some(SESSION),
            Some("haiku"),
            Some("low")
        ),
        words(&[
            "claude", "--resume", SESSION, "--model", "haiku", "--effort", "low"
        ])
    );
    assert_eq!(
        cli_command(
            AgentProvider::Codex,
            Some("t-1"),
            Some("gpt-6-luna"),
            Some("low")
        ),
        words(&["codex", "resume", "t-1"]),
        "a resumed thread brings its own settings, and -c would cut the TUI off the shared app server"
    );
    assert_eq!(
        cli_command(AgentProvider::Codex, None, Some("gpt-6-luna"), Some("low")),
        words(&["codex", "-m", "gpt-6-luna"])
    );
    assert_eq!(
        cli_command(AgentProvider::ClaudeCode, None, Some("--evil"), Some("a b")),
        words(&["claude"])
    );
    assert_eq!(
        interactive_shell(&cli_command(
            AgentProvider::ClaudeCode,
            Some(SESSION),
            Some("opus[1m]"),
            None
        )),
        words(&[
            "/bin/sh",
            "-c",
            &format!(
                r#"exec "${{SHELL:-/bin/sh}}" -ic 'claude --resume {SESSION} --model '\''opus[1m]'\'''"#
            ),
        ])
    );
}

#[test]
fn a_codex_tui_with_its_own_config_runs_without_the_shared_app_server() {
    assert!(!codex_embedded(&words(&["codex"])));
    assert!(!codex_embedded(&words(&[
        "codex", "-m", "o3", "resume", "t"
    ])));
    assert!(!codex_embedded(&words(&[
        "codex",
        "-c",
        "features.web=true"
    ])));
    assert!(codex_embedded(&words(&["codex", "-c", "model=\"o3\""])));
    assert!(codex_embedded(&words(&["codex", "--profile=fast"])));
    assert!(codex_embedded(&words(&["codex", "--no-daemon"])));
}

#[test]
fn chat_work_reads_as_one_busy_line() {
    let mut state = AgentPaneWire::default();
    assert!(chat_running(&state).is_empty());
    assert_eq!(busy(&chat_running(&state)), None);
    state.phase = AgentConnectionPhase::Running;
    state.tasks = vec![
        AgentTaskWire {
            kind: "shell".to_owned(),
            ..AgentTaskWire::default()
        },
        AgentTaskWire {
            kind: "shell".to_owned(),
            ..AgentTaskWire::default()
        },
    ];
    assert_eq!(
        busy(&chat_running(&state)).as_deref(),
        Some("busy: a turn and 2 background tasks")
    );
    state.tasks.push(AgentTaskWire {
        kind: "agent".to_owned(),
        ..AgentTaskWire::default()
    });
    state.queued_prompts = 1;
    assert_eq!(
        busy(&chat_running(&state)).as_deref(),
        Some("busy: a turn, 2 background tasks, 1 background subagent and 1 queued prompt")
    );
    state.config_options = json!([
        { "id": "model", "currentValue": "haiku" },
        { "id": "effort", "currentValue": "default" },
    ])
    .to_string();
    assert_eq!(chat_settings(&state), (Some("haiku".to_owned()), None));
}

#[test]
fn claude_facts_follow_the_transcript_and_its_set_commands() {
    let local = |text: &str| {
        json!({
            "type": "user",
            "cwd": "/work",
            "message": { "role": "user", "content": format!("<local-command-stdout>{text}</local-command-stdout>") },
        })
    };
    let mut facts = Facts::default();
    facts.claude_entry(&json!({
        "type": "assistant",
        "cwd": "/work",
        "effort": "xhigh",
        "message": { "model": "claude-opus-5-5", "content": [] },
    }));
    assert_eq!(facts.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(facts.effort.as_deref(), Some("xhigh"));
    facts.claude_entry(&local("Set model to `Haiku 5.5` and saved as your default"));
    facts.claude_entry(&local("Set effort level to max (this session only)"));
    assert_eq!(facts.model.as_deref(), Some("Haiku 5.5"));
    assert_eq!(facts.effort.as_deref(), Some("max"));
    assert_eq!(claude_model("Haiku 5.5").as_deref(), Some("haiku"));
    assert_eq!(claude_model("opus[1m]").as_deref(), Some("opus[1m]"));
    assert_eq!(claude_model("mystery"), None);
}

#[test]
fn launch_facts_read_the_command_line_over_the_clis_config() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("claude");
    let project = root.path().join("project");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(project.join(".claude")).unwrap();
    std::fs::write(
        config.join("settings.json"),
        r#"{"model":"sonnet","effortLevel":"xhigh"}"#,
    )
    .unwrap();
    std::fs::write(
        project.join(".claude/settings.local.json"),
        r#"{"model":"opus[1m]"}"#,
    )
    .unwrap();
    let cwd = project.to_str().unwrap();
    let claude = Facts::claude_launch(&words(&["claude"]), Some(&config), cwd);
    assert_eq!(claude.model.as_deref(), Some("opus[1m]"));
    assert_eq!(claude.effort.as_deref(), Some("xhigh"));
    let claude = Facts::claude_launch(
        &words(&[
            "claude",
            "--effort",
            "xhigh",
            "--model=haiku",
            "--effort",
            "high",
        ]),
        None,
        cwd,
    );
    assert_eq!(claude.model.as_deref(), Some("haiku"));
    assert_eq!(claude.effort.as_deref(), Some("high"));

    let home = root.path().join("codex");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.toml"),
        "model = \"gpt-5.5\" # default\nmodel_reasoning_effort = 'medium'\n\n[profiles.fast]\nmodel = \"gpt-5.5-mini\"\n",
    )
    .unwrap();
    let codex = Facts::codex_launch(&words(&["codex"]), Some(&home), None);
    assert_eq!(codex.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(codex.effort.as_deref(), Some("medium"));
    let codex = Facts::codex_launch(
        &words(&["codex", "-c", "model=o3", "-m", "gpt-5.5-codex"]),
        None,
        Some("/repo".to_owned()),
    );
    assert_eq!(codex.model.as_deref(), Some("gpt-5.5-codex"));
    assert_eq!(codex.cwd.as_deref(), Some("/repo"));
}
