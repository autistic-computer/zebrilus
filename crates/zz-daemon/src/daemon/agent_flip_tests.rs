use std::time::{Duration, Instant};

use zz_protocol::AgentTaskWire;

use super::*;
use crate::agent::{
    host::{HostCommand, RuntimeChannels},
    runtime::RuntimeCommand,
    stream::AgentStreamPayload,
};

const SESSION: &str = "0b1c2d3e-4f50-4a6b-8c7d-9e0f1a2b3c4d";

fn run(shared: &Arc<Shared>, client: ClientId, args: &[&str]) -> Result<Execution, DaemonError> {
    let mut context = ExecutionContext::default();
    let (name, args) = args.split_first().expect("command");
    shared.execute(
        client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new(*name, args.iter().copied()),
    )
}

fn fixture(name: &str) -> (Arc<Shared>, ClientId, PaneId) {
    fixture_running(name, "exec sleep 30")
}

fn fixture_running(name: &str, command: &str) -> (Arc<Shared>, ClientId, PaneId) {
    let shared = Arc::new(Shared::new(11));
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", name, command]),
        )
        .expect("session");
    let target = format!("{name}:");
    for args in [
        &["set-option", "-g", "remain-on-exit", "on"][..],
        &["set-environment", "-t", &target, "SHELL", "/usr/bin/true"],
    ] {
        run(&shared, client, args).expect("setup");
    }
    (shared, client, context.pane.expect("pane"))
}

fn busy_runtime(shared: &Arc<Shared>) -> Arc<AgentRuntime> {
    let publisher: Arc<dyn AgentPublisher> = Arc::new(shared.agent_tx.publisher());
    let runtime = Arc::new(AgentRuntime::new(
        &publisher,
        AgentSpawnConfig::default(),
        None,
    ));
    runtime.set_runner_factory(Box::new(|_| {
        Box::new(|channels: RuntimeChannels| {
            Box::pin(async move {
                let task = AgentTaskWire {
                    id: "b1".to_owned(),
                    kind: "shell".to_owned(),
                    description: "sleep 30".to_owned(),
                    tool_call_id: None,
                };
                let _ = channels
                    .events
                    .send(AgentStreamPayload::TasksChanged { tasks: vec![task] })
                    .await;
                while let Ok(command) = channels.commands.recv().await {
                    if matches!(command, RuntimeCommand::StopTask { .. }) {
                        let _ = channels
                            .events
                            .send(AgentStreamPayload::TasksChanged { tasks: Vec::new() })
                            .await;
                    }
                }
                Ok(())
            })
        })
    }));
    *shared.agent.lock() = Some(Arc::clone(&runtime));
    runtime
}

fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn kind(shared: &Shared, pane: PaneId) -> Option<PaneKind> {
    shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .map(|pane| pane.kind.clone())
}

fn spawned_command(shared: &Shared, pane: PaneId) -> String {
    shared
        .inner
        .lock()
        .terminal_spawns
        .get(&pane)
        .and_then(|spawn| spawn.command.clone())
        .unwrap_or_default()
        .join(" ")
}

fn flip_armed(shared: &Shared, pane: PaneId) -> bool {
    shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .is_some_and(|pane| pane.summary.flip_armed)
}

#[test]
fn a_terminal_running_the_cli_flips_into_a_chat_once_the_cli_exits() {
    let folder = tempfile::tempdir().expect("folder");
    let cli = folder.path().join("claude");
    std::fs::write(&cli, "#!/bin/sh\nsleep 30\n").expect("fake CLI");
    std::fs::set_permissions(&cli, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("executable");
    let (shared, client, pane) = fixture_running("flip-to-chat", &cli.display().to_string());
    wait_for("the CLI's pid", || {
        shared
            .inner
            .lock()
            .engine
            .pane_runtime_facts(pane)
            .and_then(|facts| facts.pid)
            .is_some()
    });
    let process = vec![cli.display().to_string()];
    shared
        .inner
        .lock()
        .engine
        .state
        .update_pane_summary(pane, |summary| summary.process = process);
    run(&shared, client, &["flip-agent", "-t", &pane.to_string()]).expect("flip");
    wait_for("the chat", || {
        matches!(kind(&shared, pane), Some(PaneKind::Agent(agent)) if agent.provider == AgentProvider::ClaudeCode)
            && shared.inner.lock().agent_panes.contains(&pane)
    });
    assert!(!shared.inner.lock().terminals.contains_key(&pane));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .pane_runtime_facts(pane)
            .is_none_or(|facts| facts.pid.is_none() && facts.current_command.is_empty()),
        "the CLI's process facts leave with it"
    );
    assert!(
        run(
            &shared,
            client,
            &["agent-send", "-t", &pane.to_string(), "hi"]
        )
        .is_ok(),
        "a chat takes agent-send"
    );
    shared.request_shutdown();
}

#[test]
fn agent_send_refuses_a_terminal_running_the_cli() {
    let (shared, client, pane) = fixture("send-to-cli");
    shared
        .inner
        .lock()
        .engine
        .state
        .update_pane_summary(pane, |summary| summary.process = vec!["codex".to_owned()]);
    let error = run(
        &shared,
        client,
        &["agent-send", "-t", &pane.to_string(), "hi"],
    )
    .expect_err("a TUI takes no typed prompts");
    assert!(
        error.to_string().contains("flip the pane to chat first"),
        "{error}"
    );
    shared.request_shutdown();
}

#[test]
fn a_busy_chat_refuses_then_flips_to_its_cli_when_idle() {
    let (shared, client, pane) = fixture("flip-to-cli");
    let runtime = busy_runtime(&shared);
    let target = pane.to_string();
    run(
        &shared,
        client,
        &[
            "select-pane-kind",
            "-t",
            &target,
            "--resume",
            SESSION,
            "--model",
            "haiku",
            "--effort",
            "low",
            "agent",
            "claude",
        ],
    )
    .expect("terminal to chat");
    wait_for("the busy driver", || {
        runtime
            .wire_state(pane)
            .is_some_and(|state| !state.tasks.is_empty())
    });

    let error = run(&shared, client, &["flip-agent", "-t", &target]).expect_err("busy");
    assert_eq!(
        server_error_text(&daemon_server_error(error)),
        "busy: 1 background task"
    );
    assert!(matches!(kind(&shared, pane), Some(PaneKind::Agent(_))));

    run(
        &shared,
        client,
        &["flip-agent", "-t", &target, "--when-idle"],
    )
    .expect("armed");
    assert!(flip_armed(&shared, pane));
    run(&shared, client, &["flip-agent", "-t", &target, "--cancel"]).expect("disarmed");
    assert!(!flip_armed(&shared, pane));
    run(
        &shared,
        client,
        &["flip-agent", "-t", &target, "--when-idle"],
    )
    .expect("armed again");
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        matches!(kind(&shared, pane), Some(PaneKind::Agent(_))),
        "an armed flip waits while work runs"
    );

    assert!(runtime.command(
        pane,
        HostCommand::StopTask {
            task_id: "b1".to_owned()
        }
    ));
    wait_for("the CLI", || {
        matches!(kind(&shared, pane), Some(PaneKind::Terminal))
    });
    assert!(!flip_armed(&shared, pane));
    let command = spawned_command(&shared, pane);
    assert!(
        command.contains(&format!(
            "claude --resume {SESSION} --model haiku --effort low"
        )),
        "{command}"
    );
    assert!(runtime.pane_generation(pane).is_none(), "the driver closed");
    assert!(!shared.inner.lock().agent_panes.contains(&pane));
    shared.request_shutdown();
}

#[test]
fn a_cli_on_an_account_flips_into_a_chat_on_that_account() {
    let step = FlipStep::ToChat {
        provider: AgentProvider::ClaudeCode,
        session: crate::agent::cli_session::CliSession {
            session_id: Some(SESSION.to_owned()),
            cwd: Some(PathBuf::from("/work")),
            model: Some("haiku".to_owned()),
            ..Default::default()
        },
        account: Some("a1".to_owned()),
    };
    assert_eq!(
        step.command(PaneId(3)),
        CommandInvocation::new(
            "select-pane-kind",
            [
                "-t",
                "%3",
                "-c",
                "/work",
                "--resume",
                SESSION,
                "--model",
                "haiku",
                "--account",
                "a1",
                "agent",
                "claude-code",
            ],
        )
    );
}
