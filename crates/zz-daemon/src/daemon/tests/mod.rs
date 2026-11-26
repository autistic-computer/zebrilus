use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Receiver;
use zz_protocol::{
    EventPayload, LayoutNode, ProtocolMessage, TerminalUiCommand, decode_protocol_frame,
    encode_protocol_message,
};
use zz_terminal::{
    GRAPHEME_TABLE_BIT, KeyAction, KeyCode, KeyInput, Modifiers, PointerCellEvent, SearchDirection,
    SessionStatus, TerminalDictionary, TerminalMode, TerminalMouseButton, TerminalMouseInput,
    TerminalMousePhase, TerminalViewAction,
};

use super::*;
use zz_daemon_client::{CommandClient, InteractiveClient};

mod agent_send_and_tools;
mod alias_and_shutdown;
mod appearance;
mod bells_and_alerts;
mod buffers;
mod choosers;
mod client_flags_and_attach;
mod client_focus;
mod command_catalog;
mod committed_text_and_chooser_activity;
mod connections;
mod control_clients;
mod copy_mode;
mod daemon_lifecycle;
mod display_message;
mod display_panes;
mod formats;
mod hooks;
mod key_tables;
mod menus_and_confirm;
mod multi_client_attach;
mod nested_source;
mod new_session;
mod outbound_mailbox;
mod pane_input;
mod pane_lifecycle;
mod paste_uploads;
mod pipe_pane;
mod popups;
mod prompts;
mod publication;
mod read_only_and_prepared_commands;
mod run_and_wait_pane;
mod run_shell;
mod shell_jobs;
mod source_callbacks;
mod source_file;
mod source_parse_errors;
mod source_replay;
mod startup_config;
mod switch_client_and_destroy;
mod terminal_spawn;
mod wait_for_and_agent_state;
mod window_sizing;

#[cfg(feature = "agent")]
mod agent;

fn register_wait_clients(shared: &Shared, clients: impl IntoIterator<Item = u64>) {
    let mut inner = shared.inner.lock();
    for client in clients {
        inner.client_entry(ClientId(client)).instance_id = Some(ClientInstanceId(client));
    }
}

fn read_global_option(shared: &Arc<Shared>, name: &str) -> RawText {
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gqv", name]),
        )
        .expect("read global option")
        .output
}

fn control_stdin_command(
    name: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<zz_protocol::RawText>>,
) -> CommandInvocation {
    CommandInvocation::new(name, args).with_source(SourceSpan {
        source: "<control>".to_owned(),
        line: 1,
        column: 1,
    })
}

fn retained_startup_causes(shared: &Shared) -> Vec<String> {
    shared
        .startup_config_causes
        .lock()
        .clone()
        .unwrap_or_default()
}

fn startup_config_cause_events(messages: &[ProtocolMessage]) -> Vec<Vec<String>> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::StartupConfigCauses { causes },
                ..
            }) => Some(causes.clone()),
            _ => None,
        })
        .collect()
}

fn startup_attach_prefix_bytes(kind: ClientKind) -> (usize, usize) {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec!["byte pressure".to_owned()]);
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(kind, None, None, Arc::clone(&mailbox));
    let (session, _, _) = switch_test_session(&shared, "startup-byte-pressure");
    let snapshot = shared
        .attach(client, session)
        .expect("attach calibration client");
    take_reliable_messages(&mailbox);
    assert!(shared.send_attached(client, &mailbox, session, snapshot));
    let state = mailbox.state.lock();
    assert!(state.reliable.len() >= 2);
    assert!(matches!(
        decode_protocol_frame(&state.reliable[0]),
        Ok(ProtocolMessage::Attached { .. })
    ));
    assert!(match decode_protocol_frame(&state.reliable[1]) {
        Ok(ProtocolMessage::Event(Event {
            payload: EventPayload::StartupConfigCauses { .. },
            ..
        })) => kind == ClientKind::Control,
        Ok(ProtocolMessage::Event(Event {
            payload:
                EventPayload::CommandOutput {
                    viewport: Some(_), ..
                },
            ..
        })) => kind == ClientKind::Interactive,
        _ => false,
    });
    (state.reliable[0].len(), state.reliable[1].len())
}

fn bind_focus_any(shared: &Shared, table: &str, command: CommandInvocation) {
    shared.inner.lock().engine.keys.bind(
        table,
        "Any",
        zz_mux::Binding {
            commands: vec![command],
            repeat: false,
            note: None,
        },
    );
}

fn install_client_event_log_hooks(
    shared: &Arc<Shared>,
    context: &mut ExecutionContext,
    names: &[&str],
) {
    for name in names {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new(
                    "set-hook",
                    [
                        "-g",
                        *name,
                        "display-message '#{hook}|#{hook_client}|#{session_id}|#{window_id}|#{pane_id}'",
                    ],
                ),
            )
            .expect("install client event hook");
    }
}

fn client_event_log(shared: &Shared) -> Vec<String> {
    shared
        .inner
        .lock()
        .history
        .message_log
        .iter()
        .map(|message| message.text.clone())
        .collect()
}

#[cfg(unix)]
pub(super) fn key_table_fixture(
    name: &str,
) -> (
    Arc<Shared>,
    ClientId,
    ExecutionContext,
    PaneId,
    Arc<OutboundMailbox>,
) {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", name, "exec /bin/cat"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);
    (shared, client, context, pane, mailbox)
}

pub(super) fn key_table_events(mailbox: &OutboundMailbox) -> Vec<EventPayload> {
    take_reliable_messages(mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    payload @ (EventPayload::PrefixArmed { .. }
                    | EventPayload::KeyTableActive { .. }),
                ..
            }) => Some(payload),
            _ => None,
        })
        .collect()
}

pub(super) fn table_active(table: Option<&str>, repeat: bool) -> EventPayload {
    EventPayload::KeyTableActive {
        table: table.map(str::to_owned),
        repeat,
    }
}

#[cfg(unix)]
pub(super) fn run_test_command(
    shared: &Arc<Shared>,
    client: ClientId,
    context: &mut ExecutionContext,
    command: &[&str],
) {
    shared
        .execute(
            client,
            ClientKind::Interactive,
            context,
            &CommandInvocation::new(command[0], command[1..].iter().copied()),
        )
        .unwrap_or_else(|error| panic!("{command:?}: {error:?}"));
}

#[cfg(unix)]
fn send_text_fixture(name: &str, command: &str) -> (Arc<Shared>, ClientId, String) {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", name, command]),
        )
        .expect("seed the send-text terminal");
    let target = context.pane.expect("send-text pane").to_string();
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("zz-ready")
    });
    (shared, client, target)
}

#[track_caller]
fn wait_for_capture(
    shared: &Arc<Shared>,
    client: ClientId,
    target: &str,
    accept: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let screen = shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("capture-pane", ["-p", "-t", target]),
            )
            .expect("capture the screen")
            .output;
        if accept(&screen) {
            return screen.to_string();
        }
        assert!(
            Instant::now() < deadline,
            "the screen never satisfied the check: {screen:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn run_pane_stderr(shared: &Shared, client: ClientId) -> String {
    shared
        .inner
        .lock()
        .client_entry(client)
        .command_streams
        .replace(CommandStreams::default())
        .map(|streams| streams.stderr)
        .unwrap_or_default()
}

#[cfg(unix)]
fn run_pane_exit(
    shared: &Arc<Shared>,
    client: ClientId,
    target: &str,
    args: &[&str],
) -> (Result<String, (String, u8)>, String, Duration) {
    run_pane_stderr(shared, client);
    let started = Instant::now();
    let result = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "run-pane",
                ["-t", target].into_iter().chain(args.iter().copied()),
            ),
        )
        .map(|execution| execution.output.to_string())
        .map_err(|error| match error {
            DaemonError::CommandExit { output, exit_code } => (output.to_string(), exit_code),
            error => panic!("{error:?}"),
        });
    let elapsed = started.elapsed();
    (result, run_pane_stderr(shared, client), elapsed)
}

#[cfg(unix)]
fn send_keys(shared: &Arc<Shared>, client: ClientId, target: &str, keys: &[&str]) {
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "send-keys",
                ["-t", target].into_iter().chain(keys.iter().copied()),
            ),
        )
        .expect("send keys");
}

fn armed_client_message(shared: &Arc<Shared>, client: ClientId) -> ActiveClientMessage {
    *shared.inner.lock().clients[&client]
        .message
        .as_ref()
        .unwrap()
}

fn take_timed_client_messages(mailbox: &OutboundMailbox) -> Vec<(Option<PaneId>, String, u32)> {
    take_reliable_messages(mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::TimedClientMessage {
                        pane,
                        text,
                        duration_ms,
                        ..
                    },
                ..
            }) => Some((pane, text, duration_ms)),
            _ => None,
        })
        .collect()
}

fn cleared_message_ids(messages: &[ProtocolMessage]) -> Vec<u64> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TimedClientMessageCleared { message_id },
                ..
            }) => Some(*message_id),
            _ => None,
        })
        .collect()
}

fn prompt_state(shared: &Arc<Shared>, client: ClientId) -> Option<CommandPromptState> {
    command_prompt_state(&shared.inner.lock(), client)
}

fn attached_message_fixture(
    shared: &Arc<Shared>,
    name: &str,
    clients: &[ClientId],
) -> (SessionId, PaneId, Arc<TerminalSession>) {
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", name]),
        )
        .expect("create message session");
    let session = context.session.expect("message session");
    let pane = context.pane.expect("message pane");
    for client in clients {
        shared.attach(*client, session).expect("attach client");
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    for client in clients {
        loop {
            let ready = {
                let inner = shared.inner.lock();
                let streamed = inner
                    .client(*client)
                    .and_then(|c| c.streamed_terminals.as_ref())
                    .is_some_and(|streamed| streamed.contains_key(&pane));
                let terminal = &inner.terminals[&pane];
                if streamed {
                    terminal
                        .latest_viewport_for(TerminalViewId(client.0))
                        .is_some()
                } else {
                    matches!(terminal.latest_viewport().status, SessionStatus::Running)
                }
            };
            if ready {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "terminal {pane} did not publish its attached viewport"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    (session, pane, terminal)
}

fn drain_terminal_lane(mailbox: &OutboundMailbox) {
    let mut state = mailbox.state.lock();
    let bytes = state
        .terminals
        .values()
        .map(|pending| pending.encoded.len())
        .sum::<usize>();
    state.queued_bytes = state.queued_bytes.saturating_sub(bytes);
    state.terminals.clear();
    state.terminal_order.clear();
}

fn wait_for_pane_runtime_facts(shared: &Arc<Shared>, panes: &[PaneId]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let settled = {
            let inner = shared.inner.lock();
            panes.iter().all(|pane| {
                inner.engine.pane_runtime_facts(*pane).is_some_and(|facts| {
                    !facts.current_command.is_empty()
                        && facts.pid.is_some()
                        && (cfg!(windows) || !facts.tty.is_empty())
                })
            })
        };
        if settled {
            wait_for_quiet_panes(shared, panes);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "pane runtime facts did not settle"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn two_session_pair(
    shared: &Arc<Shared>,
) -> (
    (ClientId, Arc<OutboundMailbox>, SessionId),
    (ClientId, Arc<OutboundMailbox>, SessionId),
) {
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "a", QUIET_PANE_COMMAND]),
        )
        .expect("session a");
    let a = context.session.expect("session a id");
    let pane_a = context.pane.expect("session a pane");
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "b", QUIET_PANE_COMMAND]),
        )
        .expect("session b");
    let b = context.session.expect("session b id");
    let pane_b = context.pane.expect("session b pane");
    wait_for_pane_runtime_facts(shared, &[pane_a, pane_b]);
    let alpha_mailbox = OutboundMailbox::new();
    let beta_mailbox = OutboundMailbox::new();
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        None,
        Arc::clone(&alpha_mailbox),
    );
    let (beta, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("beta".to_owned()),
        None,
        Arc::clone(&beta_mailbox),
    );
    shared.attach(alpha, a).expect("attach alpha");
    shared.attach(beta, b).expect("attach beta");
    take_reliable_messages(&alpha_mailbox);
    take_reliable_messages(&beta_mailbox);
    ((alpha, alpha_mailbox, a), (beta, beta_mailbox, b))
}

#[cfg(unix)]
fn progress_bar_pane(shared: &Arc<Shared>) -> (PaneId, String) {
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "progress", QUIET_PANE_COMMAND]),
        )
        .expect("session");
    let pane = context.pane.expect("pane");
    wait_for_pane_runtime_facts(shared, &[pane]);
    let target = pane.to_string();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "send-keys",
                ["-t", &target, "printf 'PROGRESS_%s\\n' READY", "Enter"],
            ),
        )
        .expect("ready probe");
    wait_for_capture(shared, ClientId(1), &target, |screen| {
        screen.contains("PROGRESS_READY")
    });
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let command = terminal_current_command(&terminal);
    assert!(!command.is_empty());
    (pane, command)
}

#[cfg(unix)]
fn wait_for_process_group_exit(group: rustix::process::Pid, timeout: Duration, what: &str) {
    let deadline = Instant::now() + timeout;
    loop {
        match rustix::process::test_kill_process_group(group) {
            Err(rustix::io::Errno::SRCH | rustix::io::Errno::PERM) => return,
            Ok(()) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(()) => panic!("{what} process group survived"),
            Err(error) => panic!("could not query {what} process group: {error}"),
        }
    }
}

#[cfg(all(feature = "agent", unix))]
fn wait_for_runtime_pid(shared: &Shared, pane: PaneId, pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let runtime = shared
            .inner
            .lock()
            .engine
            .pane_runtime_facts(pane)
            .and_then(|runtime| runtime.pid);
        if runtime == Some(pid) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "daemon never recorded shell pid {pid} for {pane}: {runtime:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(all(feature = "agent", unix))]
fn with_claude_peer_status(
    test: &str,
    check: impl FnOnce(&Arc<Shared>, PaneId, &Path, serde_json::Value),
) {
    const CHILD: &str = "ZZ_TEST_CLAUDE_PEER_STATUS";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().expect("peer registry directory");
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                &format!("daemon::tests::{test}"),
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env("CLAUDE_CONFIG_DIR", directory.path())
            .spawn()
            .expect("isolated peer status regression");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().expect("poll peer status regression") {
                assert!(status.success());
                return;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop stalled peer status regression");
                child.wait().expect("reap stalled peer status regression");
                panic!("isolated peer status regression stalled");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let directory = PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").expect("peer config"));
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1, 2]);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "claude-status"]),
        )
        .expect("terminal session");
    let pane = context.pane.expect("terminal pane");
    let terminal = shared.inner.lock().terminals[&pane].clone();
    wait_for_terminal_identity(&terminal);
    let pid = terminal.process_id().expect("shell pid");
    wait_for_runtime_pid(&shared, pane, pid);
    let start = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .env("TZ", "UTC")
        .output()
        .expect("process start");
    assert!(start.status.success());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64;
    let record = serde_json::json!({
        "pid": pid, "sessionId": "peer-status-test", "cwd": directory,
        "startedAt": now, "procStart": String::from_utf8_lossy(&start.stdout).trim(),
        "version": "2.1.273", "peerProtocol": 1, "peerFeatures": [],
        "kind": "interactive", "entrypoint": "cli",
        "pidDomain": if cfg!(target_os = "macos") { "darwin" } else { "linux" },
        "tmux": format!("claude-status:@0.{pane}"),
        "messagingSocketPath": directory.join("peer.sock"),
        "name": "terminal-peer", "status": "idle", "updatedAt": now,
        "statusUpdatedAt": now
    });
    fs::create_dir_all(directory.join("sessions")).expect("registry");
    let path = directory.join("sessions").join(format!("{pid}.json"));
    check(&shared, pane, &path, record);
    shared.request_shutdown();
}

#[cfg(all(feature = "agent", unix))]
#[test]
fn claude_peer_status_drives_agent_state_for_terminal_panes() {
    with_claude_peer_status(
        "claude_peer_status_drives_agent_state_for_terminal_panes",
        |shared, pane, path, mut record| {
            let target = pane.to_string();
            let channel = format!("@agent_state@{pane}");
            let run = |args: &[&str]| {
                shared
                    .execute(
                        ClientId(1),
                        ClientKind::Command,
                        &mut ExecutionContext::default(),
                        &CommandInvocation::new(args[0], args[1..].iter().copied()),
                    )
                    .expect("command")
            };
            run(&[
                "set-hook",
                "-g",
                "@option-changed",
                "set-option -gF @peer-observed '#{hook_option}:#{hook_target}'",
            ]);
            for (status, expected) in [("busy", "working"), ("idle", "idle")] {
                record["status"] = status.into();
                fs::write(path, serde_json::to_vec(&record).expect("record JSON"))
                    .expect("peer record");
                let parked = Arc::clone(shared);
                let wait_channel = channel.clone();
                let (completed, completion) = mpsc::channel();
                let waiter = thread::spawn(move || {
                    completed
                        .send(parked.wait_for(
                            ClientId(2),
                            ClientKind::Command,
                            &[wait_channel.into()],
                        ))
                        .expect("wait completion");
                });
                let deadline = Instant::now() + Duration::from_secs(10);
                while shared
                    .inner
                    .lock()
                    .waits
                    .wait_channels
                    .get(&channel)
                    .is_none_or(|channel| channel.waiters.is_empty())
                {
                    assert!(Instant::now() < deadline, "waiter did not park");
                    thread::sleep(Duration::from_millis(10));
                }
                shared.sync_claude_peer_states();
                assert_eq!(
                    run(&["show-options", "-p", "-qv", "-t", &target, "@agent_state"])
                        .output
                        .trim(),
                    expected
                );
                completion
                    .recv_timeout(Duration::from_secs(10))
                    .expect("peer status transition did not wake waiter")
                    .expect("wait-for");
                waiter.join().expect("waiter");
                assert_eq!(
                    read_global_option(shared, "@peer-observed").trim(),
                    format!("@agent_state:{pane}")
                );
            }
            shared.sync_claude_peer_states();
            assert!(
                !shared
                    .inner
                    .lock()
                    .waits
                    .wait_channels
                    .get(&channel)
                    .is_some_and(|channel| channel.woken),
                "unchanged peer status must not signal again"
            );
            fs::remove_file(path).expect("remove peer record");
            shared.sync_claude_peer_states();
            assert_eq!(
                run(&["show-options", "-p", "-qv", "-t", &target, "@agent_state"])
                    .output
                    .trim(),
                "idle"
            );
            assert!(!shared.inner.lock().claude_peer_states.contains_key(&pane));
        },
    );
}

#[cfg(all(feature = "agent", unix))]
#[test]
fn claude_peer_status_respects_the_opt_out() {
    with_claude_peer_status(
        "claude_peer_status_respects_the_opt_out",
        |shared, pane, path, mut record| {
            let target = pane.to_string();
            record["status"] = "busy".into();
            fs::write(path, serde_json::to_vec(&record).expect("record JSON"))
                .expect("peer record");
            for scope in ["-g", "-p"] {
                shared
                    .execute(
                        ClientId(1),
                        ClientKind::Command,
                        &mut ExecutionContext::default(),
                        &CommandInvocation::new(
                            "set-option",
                            [scope, "-t", &target, "@agent-peer-state", "off"],
                        ),
                    )
                    .expect("disable peer state");
                shared.sync_claude_peer_states();
                assert!(!shared.inner.lock().claude_peer_states.contains_key(&pane));
                shared
                    .execute(
                        ClientId(1),
                        ClientKind::Command,
                        &mut ExecutionContext::default(),
                        &CommandInvocation::new(
                            "set-option",
                            [scope, "-u", "-t", &target, "@agent-peer-state"],
                        ),
                    )
                    .expect("clear peer state opt-out");
            }
            let state = shared
                .execute(
                    ClientId(1),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "show-options",
                        ["-p", "-qv", "-t", &target, "@agent_state"],
                    ),
                )
                .expect("read agent state");
            assert!(state.output.is_empty());
            assert!(!shared.inner.lock().claude_peer_states.contains_key(&pane));
        },
    );
}

#[cfg(all(feature = "agent", unix))]
#[test]
fn a_long_claude_turn_stays_working() {
    with_claude_peer_status(
        "a_long_claude_turn_stays_working",
        |shared, pane, path, mut record| {
            record["status"] = "busy".into();
            record["statusUpdatedAt"] = (record["updatedAt"].as_u64().expect("timestamp")
                - Duration::from_hours(1).as_millis() as u64)
                .into();
            fs::write(path, serde_json::to_vec(&record).expect("record JSON"))
                .expect("peer record");
            shared.sync_claude_peer_states();
            let state = shared
                .execute(
                    ClientId(1),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "show-options",
                        ["-p", "-qv", "-t", &pane.to_string(), "@agent_state"],
                    ),
                )
                .expect("read agent state");
            assert_eq!(state.output.trim(), "working");
        },
    );
}

#[cfg(unix)]
#[test]
fn inherited_claude_session_environment_does_not_reach_terminal_spawn() {
    const CHILD: &str = "ZZ_TEST_PARENT_CLAUDE_SESSION_ENVIRONMENT";
    if std::env::var_os(CHILD).is_none() {
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "daemon::tests::inherited_claude_session_environment_does_not_reach_terminal_spawn",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .envs(
                crate::PARENT_CLAUDE_SESSION_ENVIRONMENT
                    .iter()
                    .map(|name| (*name, "1")),
            )
            .env("CLAUDE_CODE_SESSION_ID", "abc")
            .spawn()
            .expect("isolated environment regression");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().expect("poll environment regression") {
                assert!(status.success());
                return;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop stalled environment regression");
                child.wait().expect("reap stalled environment regression");
                panic!("isolated environment regression stalled");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    assert_eq!(std::env::var("CLAUDE_CODE_CHILD_SESSION").unwrap(), "1");
    assert_eq!(std::env::var("CLAUDE_CODE_SESSION_ID").unwrap(), "abc");
    let shared = Arc::new(Shared::configured_with_boot_environment(
        1,
        Arc::new(TerminalAppearance::default()),
        AppearanceProvenance::default(),
        false,
        std::env::temp_dir().join("zz-test-paste"),
        std::env::temp_dir().join("zz-test.sock"),
        None,
        "/usr/bin/vi",
        daemon_environment(),
    ));
    let mut context = ExecutionContext::default();
    for name in crate::PARENT_CLAUDE_SESSION_ENVIRONMENT {
        assert!(matches!(
            shared.execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", *name]),
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                if message == format!("unknown variable: {name}")
        ));
    }
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d", "-s", "scrubbed-environment", "--", "/bin/sh", "-c",
                    "printf 'CHILD=[%s] SESSION=[%s] ZZ_PANE=[%s]\\n' \"${CLAUDE_CODE_CHILD_SESSION-}\" \"${CLAUDE_CODE_SESSION_ID-}\" \"$ZZ_PANE\"; sleep 30",
                ],
            ),
        )
        .expect("session");
    let pane = context.pane.expect("terminal pane");
    let expected = format!("CHILD=[] SESSION=[] ZZ_PANE=[{pane}]");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let captured = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
            )
            .expect("capture terminal")
            .output;
        if captured.contains(&expected) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "inherited Claude session environment reached terminal spawn: {captured:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn paste_upload_fixture(
    directory: &Path,
    name: &str,
) -> (Arc<Shared>, Arc<OutboundMailbox>, ClientId, PaneId) {
    let mut shared = Shared::new(1);
    Arc::get_mut(&mut shared.server).unwrap().paste_directory = directory.to_path_buf();
    let shared = Arc::new(shared);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", name]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");
    (shared, mailbox, client, pane)
}

fn client_message_texts(messages: Vec<ProtocolMessage>) -> Vec<String> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ClientMessage {
                        kind: ClientMessageKind::Error,
                        text,
                        ..
                    },
                ..
            }) => Some(text),
            _ => None,
        })
        .collect()
}

fn wait_for_pasted_path(shared: &Arc<Shared>, client: ClientId, pane: PaneId, file_name: &str) {
    let mut context = ExecutionContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let captured = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
            )
            .expect("capture pane")
            .output;
        if captured.contains(file_name) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the uploaded image path never reached the pane; captured {captured:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn test_copy_pipe_environment() -> CopyPipeEnvironment {
    CopyPipeEnvironment {
        variables: std::env::vars()
            .map(|(name, value)| (RawText::from(name), Some(RawText::from(value))))
            .collect(),
        default_terminal: "screen-256color".to_owned(),
        tmux: tmux_environment(Path::new("/tmp/zz-copy-pipe.sock"), None),
        zz_socket: PathBuf::from("/tmp/zz-copy-pipe.sock"),
    }
}

#[cfg(not(windows))]
fn shell_quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("'{}'", path.replace('\'', "'\"'\"'"))
}

fn history_chunk_text(
    rows: &[Vec<zz_terminal::PackedCell>],
    dictionary: &TerminalDictionary,
) -> String {
    let mut output = String::new();
    for row in rows {
        for cell in row {
            let glyph = cell.glyph();
            if glyph == 0 {
                continue;
            }
            if glyph & GRAPHEME_TABLE_BIT == 0 {
                if let Some(character) = char::from_u32(glyph) {
                    output.push(character);
                }
                continue;
            }
            let index = usize::try_from(glyph & !GRAPHEME_TABLE_BIT).unwrap_or(usize::MAX);
            let Some((&start, &end)) = dictionary
                .grapheme_offsets
                .get(index)
                .zip(dictionary.grapheme_offsets.get(index.saturating_add(1)))
            else {
                continue;
            };
            let Some(bytes) = dictionary.grapheme_bytes.get(
                usize::try_from(start).unwrap_or(usize::MAX)
                    ..usize::try_from(end).unwrap_or(usize::MAX),
            ) else {
                continue;
            };
            if let Ok(grapheme) = std::str::from_utf8(bytes) {
                output.push_str(grapheme);
            }
        }
        output.push('\n');
    }
    output
}

#[cfg(unix)]
pub(super) fn copy_mode_fixture(
    name: &str,
    producer: &str,
) -> (
    Arc<Shared>,
    ClientId,
    ExecutionContext,
    PaneId,
    Arc<TerminalSession>,
    Arc<OutboundMailbox>,
) {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let command = format!("read _; {producer}; printf '\\033]0;zz-copy-ready\\007'; exec /bin/cat");
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", name, &command]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    terminal.resize(16, 4, 8, 18);
    wait_for_terminal_dimensions(&terminal, TerminalViewId(client.0), 16, 4);
    terminal.send_text("ready\r");
    wait_for_viewport(
        &terminal,
        TerminalViewId(client.0),
        "copy fixture producer never became ready",
        |viewport| viewport.title() == "zz-copy-ready",
    );
    (shared, client, context, pane, terminal, mailbox)
}

fn wait_for_viewport(
    terminal: &Arc<TerminalSession>,
    view: TerminalViewId,
    expected: &str,
    predicate: impl Fn(&TerminalViewport) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let latest = terminal.latest_viewport_for(view);
        if latest.as_ref().is_some_and(|viewport| predicate(viewport)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{expected}; latest={:?} text={:?}",
            latest.as_ref().map(|viewport| (
                viewport.status.clone(),
                viewport.columns,
                viewport.rows,
                viewport.generation,
                viewport.view_generation,
            )),
            latest.as_ref().map(|viewport| viewport_text(viewport)),
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_view_mode(
    terminal: &Arc<TerminalSession>,
    view: TerminalViewId,
    expected: &str,
    predicate: impl Fn(TerminalMode) -> bool,
) {
    wait_for_viewport(terminal, view, expected, |viewport| {
        predicate(viewport.mode)
    });
}

fn wait_for_observed_copy_session(shared: &Arc<Shared>, client: ClientId) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if shared.read_client(client, |c| {
            c.and_then(|c| c.copy_session.as_ref())
                .is_some_and(|session| session.observed)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "daemon never observed the pane frozen"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn set_test_window_mode_keys(
    shared: &Arc<Shared>,
    source: u64,
    context: &mut ExecutionContext,
    window: WindowId,
    value: &str,
) {
    shared
        .execute(
            ClientId(source),
            ClientKind::Command,
            context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &window.to_string(), "mode-keys", value],
            ),
        )
        .expect("set test mode-keys");
}

pub(super) fn input_test_key(
    shared: &Arc<Shared>,
    client: ClientId,
    context: &mut ExecutionContext,
    pane: PaneId,
    input: zz_terminal::KeyInput,
) {
    shared
        .input(
            client,
            ClientKind::Interactive,
            context,
            InputMessage::Key {
                pane,
                input,
                text_follows: false,
            },
        )
        .expect("send test key");
}

#[cfg(unix)]
fn enter_observed_copy_mode(
    shared: &Arc<Shared>,
    client: ClientId,
    context: &mut ExecutionContext,
    pane: PaneId,
    terminal: &Arc<TerminalSession>,
) {
    shared
        .execute(
            client,
            ClientKind::Interactive,
            context,
            &CommandInvocation::new("copy-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter test copy mode");
    wait_for_view_mode(
        terminal,
        TerminalViewId(client.0),
        "test copy mode did not open",
        |mode| matches!(mode, TerminalMode::Copy { .. }),
    );
    wait_for_observed_copy_session(shared, client);
}

#[cfg(unix)]
fn wait_for_root_key_table(shared: &Arc<Shared>, client: ClientId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        {
            let inner = shared.inner.lock();
            if inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table)
                .is_none()
                && inner
                    .client(client)
                    .is_none_or(|c| c.copy_session.is_none())
            {
                return;
            }
        }
        assert!(Instant::now() < deadline, "{expected}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn test_drag(column: u16, phase: TerminalMousePhase) -> TerminalMouseInput {
    TerminalMouseInput::new(
        phase,
        Some(TerminalMouseButton::Left),
        PointerCellEvent {
            column,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        u32::from(column) * 8,
        0,
        128,
        72,
        8,
        18,
        Modifiers::default(),
        false,
    )
}

fn chooser_key_press(name: &str) -> KeyInput {
    let (key, modifiers) = match name.strip_prefix("M-") {
        Some(rest) => (rest, Modifiers::new(false, false, true, false)),
        None => (name, Modifiers::default()),
    };
    KeyInput {
        action: KeyAction::Press,
        key: KeyCode::Character(key.chars().next().expect("a single-character key")),
        modifiers,
        text: (modifiers == Modifiers::default()).then(|| key.to_owned().into_boxed_str()),
        unshifted_codepoint: None,
    }
}

fn panes_overlay_fixture(
    name: &str,
) -> (
    Arc<Shared>,
    ClientId,
    ExecutionContext,
    impl Fn(&str, &[&str]) -> Execution,
) {
    let (shared, client, _, context) = popup_test_workspace(name);
    let base = context.clone();
    let runner = Arc::clone(&shared);
    let run = move |name: &str, args: &[&str]| {
        runner
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut base.clone(),
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{name} {args:?}: {error:?}"))
    };
    (shared, client, context, run)
}

fn top_is_panes_mode(shared: &Shared, pane: PaneId) -> bool {
    matches!(
        shared
            .inner
            .lock()
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last()),
        Some(PaneModeRequest::Panes(_))
    )
}

fn panes_overlay_host(shared: &Shared, client: ClientId) -> Option<PaneId> {
    shared.read_client(client, |c| {
        c.and_then(|c| c.display_panes.as_ref())
            .and_then(|overlay| overlay.mode)
    })
}

pub(super) fn switch_test_session(shared: &Shared, name: &str) -> (SessionId, WindowId, PaneId) {
    shared
        .inner
        .lock()
        .engine
        .state
        .create_session(name)
        .expect("create switch-client session")
}

fn switch_lifetime_mode(kill_source: bool, zoom: bool) -> PaneModeRequest {
    PaneModeRequest::Switch(Box::new(zz_mux::SwitchMode::new(
        false,
        None,
        None,
        kill_source,
        zoom,
        " ",
    )))
}

#[cfg(unix)]
pub(super) fn spawn_foreground_shell_job(
    shared: &Arc<Shared>,
    client: ClientId,
    command: String,
) -> thread::JoinHandle<Result<Execution, DaemonError>> {
    let worker_shared = Arc::clone(shared);
    let worker = thread::spawn(move || {
        worker_shared.execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", [command]),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let registered = {
            let inner = shared.inner.lock();
            inner.jobs.shell_jobs.len() == 1
                && inner
                    .jobs
                    .shell_jobs
                    .values()
                    .all(|process| process.lock().is_some())
        };
        if registered && shared.active_shutdown_blockers() == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "foreground shell job never started"
        );
        thread::sleep(Duration::from_millis(10));
    }
    worker
}

fn choose_path_error(result: Result<(), DaemonError>) -> String {
    match result {
        Err(DaemonError::Server(ServerError::InvalidCommand(message))) => message,
        other => panic!("expected an invalid command, got {other:?}"),
    }
}

pub(super) const QUIET_PANE_COMMAND: &str = if cfg!(windows) {
    "cmd /Q /K rem"
} else {
    "while read -r line; do eval \"$line\"; done"
};

fn belled_session(
    shared: &Arc<Shared>,
    mailbox: &Arc<OutboundMailbox>,
) -> (ClientId, ExecutionContext, PaneId, PaneId) {
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", [QUIET_PANE_COMMAND]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let first = context.pane.expect("first terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", [QUIET_PANE_COMMAND]),
        )
        .expect("second terminal");
    let second = context.pane.expect("second terminal");
    shared.attach(client, session).expect("attach session");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", ":0"]),
        )
        .expect("select the first window");
    wait_for_quiet_panes(shared, &[first, second]);
    take_reliable_messages(mailbox);
    (client, context, first, second)
}

fn wait_for_quiet_panes(shared: &Shared, panes: &[PaneId]) {
    let terminals = panes
        .iter()
        .map(|pane| Arc::clone(&shared.inner.lock().terminals[pane]))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !terminals.iter().all(|terminal| {
        terminal.last_output().map_or(cfg!(unix), |output| {
            output.elapsed() > Duration::from_millis(300)
        })
    }) {
        assert!(Instant::now() < deadline, "quiet panes kept printing");
        thread::sleep(Duration::from_millis(10));
    }
}

fn pane_bell(messages: &[ProtocolMessage], pane: PaneId) -> bool {
    latest_reliable_snapshot(messages).sessions[0]
        .windows
        .iter()
        .find_map(|window| window.panes.get(&pane))
        .expect("snapshot pane")
        .bell
}

fn bell_events(messages: &[ProtocolMessage]) -> Vec<PaneId> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Bell { pane },
                ..
            }) => Some(*pane),
            _ => None,
        })
        .collect()
}

fn bell_messages(messages: &[ProtocolMessage]) -> Vec<(Option<PaneId>, String, u32)> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::TimedClientMessage {
                        pane,
                        kind: ClientMessageKind::Info,
                        text,
                        duration_ms,
                        ..
                    },
                ..
            }) => Some((*pane, text.clone(), *duration_ms)),
            _ => None,
        })
        .collect()
}

fn timed_message_events(messages: &[ProtocolMessage]) -> Vec<(Option<PaneId>, String, u32, u64)> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::TimedClientMessage {
                        pane,
                        kind: ClientMessageKind::Info,
                        text,
                        duration_ms,
                        message_id,
                    },
                ..
            }) => Some((*pane, text.clone(), *duration_ms, *message_id)),
            _ => None,
        })
        .collect()
}

fn message_lifecycle_events(messages: &[ProtocolMessage]) -> Vec<(bool, u64)> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TimedClientMessage { message_id, .. },
                ..
            }) => Some((true, *message_id)),
            ProtocolMessage::Event(Event {
                payload: EventPayload::TimedClientMessageCleared { message_id },
                ..
            }) => Some((false, *message_id)),
            _ => None,
        })
        .collect()
}

#[cfg(unix)]
fn ring_terminal_and_wait_for_bell(
    shared: &Arc<Shared>,
    mailbox: &OutboundMailbox,
    pane: PaneId,
    terminal: &Arc<TerminalSession>,
    expect_flag: bool,
) {
    terminal.send_text("printf '\\007'\n");
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut saw_edge = false;
    loop {
        saw_edge |= bell_events(&take_reliable_messages(mailbox)).contains(&pane);
        let state_belled = shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.bell);
        if saw_edge && state_belled == expect_flag {
            return;
        }
        assert!(Instant::now() < deadline, "bell did not publish");
        thread::sleep(Duration::from_millis(10));
    }
}

fn window_activity_flag(shared: &Arc<Shared>, window: WindowId) -> bool {
    shared.inner.lock().engine.state.windows[&window].activity_flag
}

fn window_silence_flag(shared: &Arc<Shared>, window: WindowId) -> bool {
    shared.inner.lock().engine.state.windows[&window].silence_flag
}

pub(super) fn connect_command_retry(path: &Path) -> CommandClient {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match CommandClient::connect(path) {
            Ok(client) => return client,
            Err(error) if Instant::now() >= deadline => panic!("daemon did not start: {error}"),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

pub(super) fn daemon_test_endpoint(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!(
            r"\\.\pipe\zz-{name}-{}-{}",
            std::process::id(),
            server_id()
        ))
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join(format!(
            "zz-{name}-{}-{}.sock",
            std::process::id(),
            server_id()
        ))
    }
}

#[cfg(not(windows))]
fn terminal_line_echo_command() -> &'static str {
    "printf 'zz-terminal-ready\\r\\n'; exec /bin/cat"
}

#[cfg(windows)]
fn terminal_line_echo_command() -> &'static str {
    "echo zz-terminal-ready & findstr .*"
}

pub(super) fn output_view_session_fixture(
    shared: &Arc<Shared>,
    name: &str,
    text: impl Into<String>,
) -> (SessionId, PaneId, Arc<TerminalSession>) {
    let (session, _, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session(name)
        .expect("create output-view session");
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        format!("{name} fixture"),
        text.into(),
    ));
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(pane, Arc::clone(&terminal));
    shared
        .watch_terminal(pane, &terminal)
        .expect("watch output-view terminal");
    (session, pane, terminal)
}

pub(super) fn terminal_test_message(
    pane: PaneId,
    sequence: u64,
    generation: u64,
) -> ProtocolMessage {
    let mut viewport = zz_terminal::TerminalViewport::blank(2, 2, SessionStatus::Running);
    viewport.generation = generation;
    viewport.view_generation = generation;
    ProtocolMessage::Event(Event {
        sequence,
        payload: EventPayload::TerminalViewport { pane, viewport },
    })
}

pub(super) fn terminal_patch_test_message(
    pane: PaneId,
    sequence: u64,
    base_generation: u64,
    generation: u64,
) -> ProtocolMessage {
    let mut previous = zz_terminal::TerminalViewport::blank(2, 2, SessionStatus::Running);
    previous.generation = base_generation;
    previous.view_generation = base_generation;
    let mut current = previous.clone();
    current.generation = generation;
    current.view_generation = generation;
    let patch = zz_terminal::TerminalViewport::diff(&previous, &current).expect("test patch");
    ProtocolMessage::Event(Event {
        sequence,
        payload: EventPayload::TerminalPatch { pane, patch },
    })
}

fn command_output_test_message(
    pane: PaneId,
    output_id: u64,
    sequence: u64,
    generation: u64,
) -> ProtocolMessage {
    let mut viewport = zz_terminal::TerminalViewport::blank(2, 2, SessionStatus::Running);
    viewport.generation = generation;
    viewport.view_generation = generation;
    viewport.mode = zz_terminal::TerminalMode::View {
        position: 1,
        total: 1,
    };
    ProtocolMessage::Event(Event {
        sequence,
        payload: EventPayload::CommandOutput {
            output_id,
            pane,
            viewport: Some(viewport),
        },
    })
}

fn take_command_output_message(mailbox: &OutboundMailbox) -> ProtocolMessage {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let frame = {
            let mut state = mailbox.state.lock();
            let pending = state.command_output.take();
            if let Some(pending) = &pending {
                state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
            }
            pending.map(|pending| pending.encoded)
        };
        if let Some(frame) = frame {
            return decode_protocol_frame(&frame).expect("decode command-output viewport");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for command-output viewport"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn command_output_message_id(message: &ProtocolMessage) -> u64 {
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::CommandOutput {
                output_id,
                viewport: Some(_),
                ..
            },
        ..
    }) = message
    else {
        panic!("expected command-output viewport");
    };
    *output_id
}

fn current_command_output_id(shared: &Arc<Shared>, client: ClientId) -> u64 {
    shared.inner.lock().clients[&client]
        .command_output
        .as_ref()
        .unwrap()
        .output_id
}

fn wait_for_command_output_close(mailbox: &OutboundMailbox, output_id: u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let closed = take_reliable_messages(mailbox).into_iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandOutput {
                        output_id: closed,
                        viewport: None,
                        ..
                    },
                    ..
                }) if closed == output_id
            )
        });
        if closed {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "output view {output_id} did not close"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[track_caller]
fn wait_for_mailbox_terminal(
    mailbox: &OutboundMailbox,
    terminal_state: &mut TerminalTestState,
    mut predicate: impl FnMut(&ProtocolMessage, &TerminalTestState) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let available = {
            let state = mailbox.state.lock();
            !state.reliable.is_empty()
                || state.command_output.is_some()
                || !state.terminals.is_empty()
        };
        if available {
            let frame = mailbox.recv().expect("available mailbox frame");
            let message = decode_protocol_frame(&frame).expect("decode mailbox frame");
            terminal_state.observe(&message);
            if predicate(&message, terminal_state) {
                return;
            }
        } else {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for terminal mailbox update"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn wait_for_terminal_dimensions(
    terminal: &TerminalSession,
    view: TerminalViewId,
    columns: u16,
    rows: u16,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if terminal
            .latest_viewport_for(view)
            .is_some_and(|viewport| viewport.columns == columns && viewport.rows == rows)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "terminal did not reach {columns}x{rows}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn modal_pane(shared: &Shared, context: &ExecutionContext) -> Option<PaneId> {
    let inner = shared.inner.lock();
    inner.engine.state.windows[&context.window.expect("popup test window")].modal_pane()
}

fn window_pane_count(shared: &Shared, context: &ExecutionContext) -> usize {
    let inner = shared.inner.lock();
    inner.engine.state.windows[&context.window.expect("popup test window")]
        .panes
        .len()
}

fn wait_for_terminal_size(shared: &Shared, pane: PaneId, size: (u16, u16)) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let current = shared
            .inner
            .lock()
            .terminals
            .get(&pane)
            .map(|terminal| terminal.size());
        if current == Some(size) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "pane {pane} stayed at {current:?}, wanted {size:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn popup_test_workspace(
    name: &str,
) -> (
    Arc<Shared>,
    ClientId,
    Arc<OutboundMailbox>,
    ExecutionContext,
) {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some(name.to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "popup-test"]),
        )
        .expect("popup test session");
    shared
        .attach(client, context.session.expect("popup test session id"))
        .expect("attach popup test client");
    let pane = context.pane.expect("popup test pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("size popup test client");
    take_reliable_messages(&mailbox);
    (shared, client, mailbox, context)
}

fn wait_for_menu_state(shared: &Shared, client: ClientId) -> MenuState {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(state) = shared.read_client(client, |c| {
            c.and_then(|c| c.menu.as_ref())
                .map(|menu| menu.state.clone())
        }) {
            return state;
        }
        assert!(Instant::now() < deadline, "menu state did not converge");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_confirm_state(shared: &Shared, client: ClientId) -> ConfirmState {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(state) = shared.read_client(client, |c| {
            c.and_then(|c| c.confirm.as_ref())
                .map(|confirm| confirm.state.clone())
        }) {
            return state;
        }
        assert!(Instant::now() < deadline, "confirm state did not converge");
        thread::sleep(Duration::from_millis(10));
    }
}

fn popup_command_with_timeout(
    shared: &Arc<Shared>,
    client: ClientId,
    context: &ExecutionContext,
    args: Vec<String>,
) -> Result<Execution, DaemonError> {
    let shared = Arc::clone(shared);
    let mut context = context.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    let worker = thread::spawn(move || {
        let result = shared.execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("display-popup", args),
        );
        send.send(result).expect("return popup command result");
    });
    let result = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("popup command timed out");
    worker.join().expect("popup command worker");
    result
}

fn wait_for_file_text(path: &Path, deadline: Instant) -> String {
    loop {
        let text = fs::read_to_string(path).unwrap_or_default();
        if !text.is_empty() || Instant::now() >= deadline {
            return text;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_pending_terminal(mailbox: &OutboundMailbox, pane: PaneId) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !mailbox.state.lock().terminals.contains_key(&pane) {
        assert!(
            Instant::now() < deadline,
            "no terminal frame for {pane} reached the mailbox"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn take_reliable_messages(mailbox: &OutboundMailbox) -> Vec<ProtocolMessage> {
    if let Some(feed) = mailbox.control_feed() {
        feed.pump(mailbox, feed.pause_after_ms(), Instant::now());
    }
    let frames = {
        let mut state = mailbox.state.lock();
        let frames = state.reliable.drain(..).collect::<Vec<_>>();
        let bytes = frames.iter().map(super::OutboundFrame::len).sum::<usize>();
        state.queued_bytes = state.queued_bytes.saturating_sub(bytes);
        frames
    };
    frames
        .into_iter()
        .map(|frame| decode_protocol_frame(&frame.into_vec()).expect("decode reliable message"))
        .collect()
}

fn control_command_guards(messages: Vec<ProtocolMessage>) -> Vec<(String, bool, bool)> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        ..
                    },
                ..
            }) => Some((String::from(output), error, sticky_failure)),
            _ => None,
        })
        .collect()
}

fn control_config_timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        flags,
                    },
                ..
            }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::Complete,
                    },
                ..
            }) => Some("complete".to_owned()),
            _ => None,
        })
        .collect()
}

fn control_command_frames(messages: Vec<ProtocolMessage>) -> Vec<(String, bool, bool, u8)> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        flags,
                    },
                ..
            }) => Some((String::from(output), error, sticky_failure, flags)),
            _ => None,
        })
        .collect()
}

fn control_command_outputs(messages: Vec<ProtocolMessage>) -> Vec<String> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandOutput { output },
                ..
            }) => Some(output),
            _ => None,
        })
        .collect()
}

fn wait_for_control_command_frames(
    mailbox: &OutboundMailbox,
    expected: usize,
) -> Vec<(String, bool, bool, u8)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut frames = Vec::new();
    while frames.len() < expected {
        frames.extend(control_command_frames(take_reliable_messages(mailbox)));
        assert!(
            Instant::now() < deadline,
            "expected {expected} Control command frames, got {frames:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
    frames
}

fn drain_reliable_messages_until_quiet(mailboxes: &[&OutboundMailbox]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let quiet_period = Duration::from_millis(100);
    let mut quiet_since = Instant::now();
    loop {
        let drained = mailboxes
            .iter()
            .map(|mailbox| take_reliable_messages(mailbox).len())
            .sum::<usize>();
        if drained == 0 {
            if quiet_since.elapsed() >= quiet_period {
                return;
            }
        } else {
            quiet_since = Instant::now();
        }
        assert!(Instant::now() < deadline, "mailboxes did not become quiet");
        thread::sleep(Duration::from_millis(10));
    }
}

fn latest_reliable_snapshot(messages: &[ProtocolMessage]) -> &MuxSnapshot {
    messages
        .iter()
        .rev()
        .find_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .expect("reliable snapshot event")
}

fn take_clipboard_writes(
    mailbox: &OutboundMailbox,
    pane: PaneId,
) -> Vec<(ClipboardTarget, String)> {
    take_reliable_messages(mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::Clipboard {
                        pane: written,
                        target,
                        text,
                        ..
                    },
                ..
            }) if written == pane => Some((target, text)),
            _ => None,
        })
        .collect()
}

fn take_browser_literals(mailbox: &OutboundMailbox, pane: PaneId) -> Vec<String> {
    take_reliable_messages(mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::BrowserCommand {
                        pane: target,
                        command: BrowserCommand::SendKeys(keys),
                    },
                ..
            }) if target == pane => Some(keys),
            _ => None,
        })
        .flatten()
        .filter_map(|key| match key {
            zz_protocol::KeyToken::Literal(text) => Some(text),
            zz_protocol::KeyToken::Named(_) | zz_protocol::KeyToken::Raw(_) => None,
        })
        .collect()
}

pub(super) fn test_key(key: KeyCode, modifiers: Modifiers, text: Option<&str>) -> KeyInput {
    KeyInput {
        action: KeyAction::Press,
        key,
        modifiers,
        text: text.map(Box::<str>::from),
        unshifted_codepoint: match key {
            KeyCode::Character(character) => Some(character),
            _ => None,
        },
    }
}

fn connect_interactive_retry(path: &Path) -> InteractiveClient {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match InteractiveClient::connect(path) {
            Ok(client) => return client,
            Err(error) if Instant::now() >= deadline => panic!("daemon did not start: {error}"),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn spawn_reader(
    client: Arc<InteractiveClient>,
) -> (Receiver<ProtocolMessage>, thread::JoinHandle<()>) {
    let (sender, receiver) = crossbeam_channel::unbounded();
    let reader = thread::spawn(move || {
        while let Ok(message) = client.recv() {
            let messages = match message {
                ProtocolMessage::Batch(batch) => batch.messages().expect("decode event group"),
                message => vec![message],
            };
            for message in messages {
                if sender.send(message).is_err() {
                    return;
                }
            }
        }
    });
    (receiver, reader)
}

#[derive(Default)]
struct TerminalTestState {
    viewports: BTreeMap<PaneId, zz_terminal::TerminalViewport>,
    snapshot: MuxSnapshot,
    synced: bool,
    attachment: Option<(SessionId, u64)>,
    attached: bool,
}

impl TerminalTestState {
    fn observe(&mut self, message: &ProtocolMessage) {
        self.attached = false;
        if let ProtocolMessage::Attached { session, .. } = message {
            self.attachment = Some((*session, 0));
            self.attached = true;
            self.viewports.clear();
            return;
        }
        let ProtocolMessage::Event(Event { payload, .. }) = message else {
            return;
        };
        match payload {
            EventPayload::Snapshot(snapshot) => {
                self.snapshot = snapshot.clone();
                self.synced = true;
            }
            EventPayload::TreeDelta(delta) if self.synced => {
                let generation = self.snapshot.generation;
                if let Err(error) = delta.apply(&mut self.snapshot) {
                    panic!(
                        "daemon emitted an inapplicable tree delta: {error:?}, \
                             snapshot generation {generation}, delta {}..{} with {:?}",
                        delta.base, delta.version, delta.ops
                    );
                }
            }
            EventPayload::ClientView(view) => {
                let attachment = view
                    .session
                    .map(|session| (session, view.attachment_generation));
                if self.attachment != attachment {
                    self.viewports.clear();
                    self.attached = attachment.is_some();
                    self.attachment = attachment;
                }
            }
            EventPayload::TerminalViewport { pane, viewport } => {
                self.viewports.insert(*pane, viewport.clone());
            }
            EventPayload::TerminalPatch { pane, patch } => {
                if let Some(viewport) = self.viewports.get_mut(pane) {
                    viewport
                        .apply_patch(patch.clone())
                        .expect("daemon emitted an applicable terminal patch");
                }
            }
            _ => {}
        }
    }
}

#[track_caller]
fn wait_for(
    messages: &Receiver<ProtocolMessage>,
    terminal_state: &mut TerminalTestState,
    mut predicate: impl FnMut(&ProtocolMessage, &TerminalTestState) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let message = messages
            .recv_timeout(remaining)
            .expect("timed out waiting for daemon event");
        terminal_state.observe(&message);
        if predicate(&message, terminal_state) {
            return;
        }
    }
}

fn viewport_text(viewport: &zz_terminal::TerminalViewport) -> String {
    let mut output = String::new();
    for cell in viewport.cells.iter() {
        viewport.push_glyph(*cell, &mut output);
    }
    output
}
