use agent_client_protocol::v1::schema::{
    ContentBlock, ContentChunk, MessageId, SessionUpdate, TextContent, ToolKind,
};

use super::*;
use crate::{
    fixture::{Behavior, fixture_runner},
    journal::AgentJournal,
    stream::{AgentImage, AgentStreamPayload},
};
use std::{fs, process::Command};

const DEADLINE: Duration = Duration::from_secs(10);

#[test]
fn git_refresh_gate_is_single_flight_and_latest_wins() {
    let mut gate = GitRefreshGate::default();

    assert_eq!(gate.request(), Some(1));
    assert_eq!(gate.request(), None);
    assert_eq!(gate.request(), None);
    assert_eq!(gate.running, Some(1));
    assert_eq!(gate.pending, Some(3));

    assert_eq!(gate.complete(2), None);
    assert_eq!(gate.running, Some(1));
    assert_eq!(gate.complete(1), Some(3));
    assert_eq!(gate.running, Some(3));
    assert_eq!(gate.pending, None);
    assert_eq!(gate.complete(3), None);
    assert_eq!(gate.running, None);
}

#[test]
fn git_refresh_gate_invalidation_discards_only_older_pending_work() {
    let mut gate = GitRefreshGate::default();

    assert_eq!(gate.request(), Some(1));
    assert_eq!(gate.request(), None);
    gate.invalidate();
    assert_eq!(gate.latest, 3);
    assert_eq!(gate.pending, None);
    assert_eq!(gate.complete(1), None);
    assert_eq!(gate.running, None);

    assert_eq!(gate.request(), Some(4));
    gate.invalidate();
    assert_eq!(gate.request(), None);
    assert_eq!(gate.complete(4), Some(6));
    assert_eq!(gate.running, Some(6));
}

#[test]
fn git_refresh_gate_recovers_after_spawn_failure() {
    let mut gate = GitRefreshGate::default();

    assert_eq!(gate.request(), Some(1));
    gate.spawn_failed(2);
    assert_eq!(gate.running, Some(1));
    gate.spawn_failed(1);
    assert_eq!(gate.running, None);
    assert_eq!(gate.pending, None);
    assert_eq!(gate.request(), Some(2));
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn seeded_git_repo() -> Option<(tempfile::TempDir, PathBuf)> {
    if !git_available() {
        return None;
    }
    let scratch = tempfile::tempdir().expect("temporary directory");
    let root = scratch.path().canonicalize().expect("resolved path");
    git(&root, &["init", "--quiet", "--initial-branch=main"]);
    git(&root, &["config", "user.email", "git-summary@zz.test"]);
    git(&root, &["config", "user.name", "zz git summary"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("tracked.txt"), "one\ntwo\n").expect("seed file");
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "seed"]);
    Some((scratch, root))
}

#[derive(Clone, Default)]
struct Recorder {
    items: Arc<Mutex<Vec<AgentStreamItem>>>,
    states: Arc<Mutex<Vec<(AgentPaneState, bool)>>>,
}

impl Recorder {
    fn sink(&self) -> AgentStreamSink {
        let items = Arc::clone(&self.items);
        let states = Arc::clone(&self.states);
        Box::new(move |_pane, _generation, state, item| {
            states.lock().push((state, item.is_some()));
            if let Some(item) = item {
                items.lock().push(item);
            }
        })
    }

    fn items(&self) -> Vec<AgentStreamItem> {
        self.items.lock().clone()
    }

    fn payloads(&self) -> Vec<AgentStreamPayload> {
        self.items().into_iter().map(|item| item.payload).collect()
    }

    fn wait<F>(&self, what: &str, accept: F) -> Vec<AgentStreamPayload>
    where
        F: Fn(&AgentStreamPayload) -> bool,
    {
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            let payloads = self.payloads();
            if payloads.iter().any(&accept) {
                return payloads;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("timed out waiting for {what}: {:?}", self.payloads());
    }
}

struct Fixture {
    host: AgentHost,
    recorder: Recorder,
    pane: PaneId,
}

impl Fixture {
    fn open(behavior: Behavior, auto_approve: AgentAutoApprove) -> Self {
        Self::build(behavior, auto_approve, true, None, None, None)
    }

    fn open_with_runner(runner: PaneRunner) -> Self {
        let recorder = Recorder::default();
        let host = AgentHost::with_journal(AgentSpawnConfig::default(), recorder.sink(), None);
        let pane = PaneId(8);
        assert!(host.open_with(
            pane,
            1,
            AgentPaneSpec {
                model: None,
                effort: None,
                provider: AgentProvider::Codex,
                cwd: PathBuf::from("/"),
                resume_session: None,
                auto_approve: None,
                workspace: AgentWorkspaceEnvironment::default(),
            },
            runner,
        ));
        Self {
            host,
            recorder,
            pane,
        }
    }

    fn build(
        behavior: Behavior,
        auto_approve: AgentAutoApprove,
        load: bool,
        journal: Option<&Arc<AgentJournal>>,
        resume_session: Option<String>,
        cwd: Option<PathBuf>,
    ) -> Self {
        let recorder = Recorder::default();
        let host = AgentHost::with_journal(
            AgentSpawnConfig::default(),
            recorder.sink(),
            journal.cloned(),
        );
        let pane = PaneId(7);
        let spec = AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: cwd.unwrap_or_else(|| PathBuf::from("/")),
            resume_session,
            auto_approve: Some(auto_approve),
            workspace: AgentWorkspaceEnvironment::default(),
        };
        let runner = fixture_runner(AgentProvider::Codex, behavior, auto_approve, load);
        assert!(host.open_with(pane, 1, spec, runner));
        Self {
            host,
            recorder,
            pane,
        }
    }

    fn command(&self, command: HostCommand) {
        assert!(self.host.command(self.pane, command).is_ok());
    }

    fn prompt(&self, text: &str) {
        self.command(HostCommand::Prompt(
            AgentPrompt {
                owner: ClientInstanceId::default(),
                text: text.to_owned(),
                images: Vec::new(),
            }
            .into(),
        ));
    }

    fn steer(&self, text: &str) {
        self.command(HostCommand::Steer(AgentPrompt {
            owner: ClientInstanceId::default(),
            text: text.to_owned(),
            images: Vec::new(),
        }));
    }

    fn prompt_waiting(&self, text: &str) -> crossbeam_channel::Receiver<AgentTurnResult> {
        self.prompt_with_policy(text, AgentBlockPolicy::Wait, Arc::default())
    }

    fn prompt_with_policy(
        &self,
        text: &str,
        on_block: AgentBlockPolicy,
        audit: Arc<Mutex<Vec<String>>>,
    ) -> crossbeam_channel::Receiver<AgentTurnResult> {
        let (waiter, reply) = crossbeam_channel::bounded(1);
        self.command(HostCommand::Prompt(QueuedPrompt {
            prompt: AgentPrompt {
                owner: ClientInstanceId::default(),
                text: text.to_owned(),
                images: Vec::new(),
            },
            waiter: Some(AgentTurnWaiter {
                reply: waiter.into(),
                on_block,
                audit,
                asks: None,
            }),
            source: PromptSource::Composer,
            note: None,
            ask: None,
        }));
        reply
    }

    fn state(&self) -> AgentPaneState {
        self.host.snapshot_state(self.pane).expect("pane state")
    }

    fn wait_for_session(&self) {
        self.recorder.wait("the session to be ready", |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        });
    }

    fn close(self) {
        if let Some(thread) = self.host.close(self.pane) {
            let _ = thread.join();
        }
    }
}

fn chunk_texts(payloads: &[AgentStreamPayload]) -> Vec<String> {
    payloads
        .iter()
        .filter_map(|payload| chunk_text(payload).map(ToOwned::to_owned))
        .collect()
}

fn chunk_text(payload: &AgentStreamPayload) -> Option<&str> {
    let AgentStreamPayload::Update { update } = payload else {
        return None;
    };
    update
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
}

#[test]
fn a_turn_the_agent_starts_itself_holds_prompts_and_stops_on_cancel() {
    let fixture = Fixture::open_with_runner(Box::new(|channels| {
        Box::pin(async move {
            let send = |payload| channels.events.send(payload);
            send(AgentStreamPayload::SessionReady {
                session_id: "s".to_owned(),
                modes: None,
                config_options: None,
            })
            .await
            .map_err(|error| error.to_string())?;
            send(AgentStreamPayload::Activity { busy: true })
                .await
                .map_err(|error| error.to_string())?;
            let Ok(RuntimeControl::Cancel { turn_id: 0, .. }) = channels.controls.recv().await
            else {
                return Err("expected an interrupt for the agent's own turn".to_owned());
            };
            send(AgentStreamPayload::Activity { busy: false })
                .await
                .map_err(|error| error.to_string())?;
            while let Ok(command) = channels.commands.recv().await {
                match command {
                    RuntimeCommand::Prompt {
                        turn_id, prompt, ..
                    } => {
                        send(AgentStreamPayload::Update {
                            update: serde_json::json!({
                                "sessionUpdate": "agent_message_chunk",
                                "content": { "type": "text", "text": format!("got {}", prompt.text) },
                            }),
                        })
                        .await
                        .map_err(|error| error.to_string())?;
                        send(AgentStreamPayload::PromptFinished {
                            turn_id,
                            outcome: AgentPromptOutcome::Finished {
                                stop_reason: serde_json::json!("end_turn"),
                            },
                        })
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                    RuntimeCommand::Shutdown => break,
                    _ => {}
                }
            }
            Ok(())
        })
    }));
    fixture.wait_for_session();
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().phase != AgentConnectionPhase::Running {
        assert!(
            Instant::now() < deadline,
            "the pane never showed the agent's own turn"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.prompt("after it");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fixture.state().queued_prompts, 1);
    assert!(!chunk_texts(&fixture.recorder.payloads()).contains(&"got after it".to_owned()));
    fixture.command(HostCommand::Cancel);
    fixture.recorder.wait("the queued prompt", |payload| {
        chunk_text(payload) == Some("got after it")
    });
    fixture.close();
}

#[test]
fn zz_commands_skip_the_turn_queue_and_idle_steering_becomes_a_prompt() {
    let fixture = Fixture::open_with_runner(Box::new(|channels| {
        Box::pin(async move {
            let send = |text: String| {
                channels.events.send(AgentStreamPayload::Update {
                    update: serde_json::json!({
                        "sessionUpdate": "agent_message_chunk",
                        "content": { "type": "text", "text": text },
                    }),
                })
            };
            channels
                .events
                .send(AgentStreamPayload::Ready {
                    agent_name: "Claude Code".to_owned(),
                    agent_key: "claude-code".to_owned(),
                    auth_methods: Vec::new(),
                    capabilities: AgentSessionCapabilities {
                        verbs: true,
                        watch: false,
                        ..AgentSessionCapabilities::default()
                    },
                })
                .await
                .map_err(|error| error.to_string())?;
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "s".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            while let Ok(command) = channels.commands.recv().await {
                match command {
                    RuntimeCommand::Verb { prompt } => {
                        send(format!("verb {}", prompt.text))
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    RuntimeCommand::Prompt {
                        turn_id, prompt, ..
                    } => {
                        send(format!("prompt {}", prompt.text))
                            .await
                            .map_err(|error| error.to_string())?;
                        channels
                            .events
                            .send(AgentStreamPayload::PromptFinished {
                                turn_id,
                                outcome: AgentPromptOutcome::Finished {
                                    stop_reason: serde_json::json!("end_turn"),
                                },
                            })
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    RuntimeCommand::Shutdown => break,
                    _ => {}
                }
            }
            Ok(())
        })
    }));
    fixture.wait_for_session();
    fixture.prompt("/btw what now");
    fixture.recorder.wait("the side question", |payload| {
        chunk_text(payload) == Some("verb /btw what now")
    });
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Ready);
    fixture.prompt("/steer go left");
    let payloads = fixture.recorder.wait("the steer prompt", |payload| {
        chunk_text(payload) == Some("prompt go left")
    });
    assert!(
        !chunk_texts(&payloads)
            .iter()
            .any(|text| text.starts_with("verb /steer"))
    );
    fixture.close();
}

#[test]
fn a_steer_rides_the_running_turn_and_is_a_prompt_when_idle() {
    let fixture = Fixture::open_with_runner(Box::new(|channels| {
        Box::pin(async move {
            let send = |payload| channels.events.send(payload);
            let text = |text: String| AgentStreamPayload::Update {
                update: serde_json::json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": text },
                }),
            };
            send(AgentStreamPayload::Ready {
                agent_name: "Claude Code".to_owned(),
                agent_key: "claude-code".to_owned(),
                auth_methods: Vec::new(),
                capabilities: AgentSessionCapabilities {
                    verbs: true,
                    ..AgentSessionCapabilities::default()
                },
            })
            .await
            .map_err(|error| error.to_string())?;
            send(AgentStreamPayload::SessionReady {
                session_id: "s".to_owned(),
                modes: None,
                config_options: None,
            })
            .await
            .map_err(|error| error.to_string())?;
            let mut running = None;
            while let Ok(command) = channels.commands.recv().await {
                let payloads = match command {
                    RuntimeCommand::Prompt {
                        turn_id, prompt, ..
                    } => {
                        running = Some(turn_id);
                        vec![text(format!("prompt {}", prompt.text))]
                    }
                    RuntimeCommand::Verb { prompt } => {
                        let mut payloads = vec![text(format!("verb {}", prompt.text))];
                        payloads.extend(running.take().map(|turn_id| {
                            AgentStreamPayload::PromptFinished {
                                turn_id,
                                outcome: AgentPromptOutcome::Finished {
                                    stop_reason: serde_json::json!("end_turn"),
                                },
                            }
                        }));
                        payloads
                    }
                    RuntimeCommand::Shutdown => break,
                    _ => Vec::new(),
                };
                for payload in payloads {
                    send(payload).await.map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        })
    }));
    fixture.wait_for_session();
    fixture.steer("go left");
    fixture
        .recorder
        .wait("the idle steer as a prompt", |payload| {
            chunk_text(payload) == Some("prompt go left")
        });
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Running);
    fixture.steer("  go right ");
    fixture
        .recorder
        .wait("the turn the steer finished", |payload| {
            matches!(payload, AgentStreamPayload::PromptFinished { .. })
        });
    let texts = chunk_texts(&fixture.recorder.payloads());
    assert_eq!(texts, ["prompt go left", "verb /steer go right"]);
    assert_eq!(fixture.state().queued_prompts, 0);
    fixture.close();
}

#[test]
fn a_report_from_another_chat_rides_the_running_turn_and_a_missed_one_waits_its_turn() {
    let fixture = Fixture::open_with_runner(Box::new(|channels| {
        Box::pin(async move {
            let send = |payload| channels.events.send(payload);
            let text = |text: String| AgentStreamPayload::Update {
                update: serde_json::json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": text },
                }),
            };
            send(AgentStreamPayload::Ready {
                agent_name: "Codex".to_owned(),
                agent_key: "codex".to_owned(),
                auth_methods: Vec::new(),
                capabilities: AgentSessionCapabilities {
                    verbs: true,
                    ..AgentSessionCapabilities::default()
                },
            })
            .await
            .map_err(|error| error.to_string())?;
            send(AgentStreamPayload::SessionReady {
                session_id: "s".to_owned(),
                modes: None,
                config_options: None,
            })
            .await
            .map_err(|error| error.to_string())?;
            let mut running = None;
            while let Ok(command) = channels.commands.recv().await {
                let payloads = match command {
                    RuntimeCommand::Prompt {
                        turn_id, prompt, ..
                    } => {
                        running = Some(turn_id);
                        vec![text(format!("prompt {}", prompt.text))]
                    }
                    RuntimeCommand::Verb { prompt } if prompt.text.contains("missed") => {
                        let missed = AgentPrompt {
                            text: prompt.text.trim_start_matches("/steer ").to_owned(),
                            ..prompt
                        };
                        let mut payloads = vec![AgentStreamPayload::PromptsReclaimed {
                            prompts: vec![missed],
                        }];
                        payloads.extend(running.take().map(|turn_id| {
                            AgentStreamPayload::PromptFinished {
                                turn_id,
                                outcome: AgentPromptOutcome::Finished {
                                    stop_reason: serde_json::json!("end_turn"),
                                },
                            }
                        }));
                        payloads
                    }
                    RuntimeCommand::Verb { prompt } => {
                        vec![text(format!("verb {}", prompt.text))]
                    }
                    RuntimeCommand::Shutdown => break,
                    _ => Vec::new(),
                };
                for payload in payloads {
                    send(payload).await.map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        })
    }));
    let report = |text: &str| {
        zz_protocol::agent_mention::AgentReport::Finished {
            pane: PaneId(2),
            provider: AgentProvider::Codex,
            text: text.to_owned(),
        }
        .to_string()
    };
    let inject = |text: String| {
        fixture.command(HostCommand::Prompt(QueuedPrompt {
            prompt: AgentPrompt {
                owner: ClientInstanceId::default(),
                text,
                images: Vec::new(),
            },
            waiter: None,
            source: PromptSource::Injected,
            note: None,
            ask: None,
        }));
    };
    fixture.wait_for_session();
    fixture.prompt("busy");
    fixture.recorder.wait("the running turn", |payload| {
        chunk_text(payload) == Some("prompt busy")
    });
    inject(report("done"));
    let in_turn = timed_report(&report("done"), ReportTiming::InTurn).expect("a report");
    assert!(in_turn.contains("Carry on with what you were doing"));
    assert_eq!(
        zz_protocol::agent_mention::AgentReport::parse(&in_turn),
        zz_protocol::agent_mention::AgentReport::parse(&report("done"))
    );
    let steered = format!("verb /steer {in_turn}");
    fixture
        .recorder
        .wait("the report inside the turn", |payload| {
            chunk_text(payload) == Some(steered.as_str())
        });
    assert_eq!(fixture.state().queued_prompts, 0);
    inject(report("missed"));
    let prompted = format!("prompt {}", report("missed"));
    let payloads = fixture
        .recorder
        .wait("the missed report as a turn", |payload| {
            chunk_text(payload) == Some(prompted.as_str())
        });
    assert_eq!(queued_prompt_texts(&payloads), Vec::<String>::new());
    fixture.close();
}

fn queued_prompt_texts(payloads: &[AgentStreamPayload]) -> Vec<String> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PromptsReclaimed { prompts } => Some(prompts),
            _ => None,
        })
        .flatten()
        .map(|prompt| prompt.text.clone())
        .collect()
}

#[test]
fn a_failed_turn_leaves_the_pane_ready_for_the_next_prompt() {
    let fixture = Fixture::open(Behavior::FailFirst, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("first");
    fixture.recorder.wait("the failed turn", |payload| {
        matches!(
            payload,
            AgentStreamPayload::PromptFinished {
                outcome: AgentPromptOutcome::Failed { .. },
                ..
            }
        )
    });
    let state = fixture.state();
    assert_eq!(state.phase, AgentConnectionPhase::Ready);
    assert!(state.error.is_some());
    fixture.prompt("second");
    fixture.recorder.wait("the second turn", |payload| {
        matches!(
            payload,
            AgentStreamPayload::PromptFinished {
                outcome: AgentPromptOutcome::Finished { .. },
                ..
            }
        )
    });
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Ready);
    assert_eq!(fixture.state().error, None);
    fixture.close();
}

#[test]
fn a_pane_streams_from_spawn_through_a_prompt_to_a_finished_turn() {
    let fixture = Fixture::open(Behavior::Chunk, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("go");
    let payloads = fixture.recorder.wait("the turn to finish", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });

    assert!(matches!(payloads[0], AgentStreamPayload::Ready { .. }));
    assert!(matches!(
        payloads[1],
        AgentStreamPayload::SessionReset { restoring: false }
    ));
    assert!(
        matches!(&payloads[2], AgentStreamPayload::SessionReady { session_id, .. } if session_id == "fixture-session")
    );
    assert_eq!(chunk_texts(&payloads), ["go", "turn 0"]);

    let seqs = fixture
        .recorder
        .items()
        .into_iter()
        .map(|item| item.seq)
        .collect::<Vec<_>>();
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());

    let state = fixture.state();
    assert_eq!(state.phase, AgentConnectionPhase::Ready);
    assert_eq!(state.session_id.as_deref(), Some("fixture-session"));
    assert_eq!(state.last_seq, seqs.len() as u64);
    assert_eq!(state.queued_prompts, 0);
    fixture.close();
}

#[test]
fn submitted_prompts_are_streamed_and_journalled_once_with_or_without_adapter_echoes() {
    for provider in [AgentProvider::Codex, AgentProvider::ClaudeCode] {
        for behavior in [Behavior::Chunk, Behavior::Echo] {
            let directory = tempfile::tempdir().expect("journal directory");
            let journal = Arc::new(AgentJournal::open(directory.path()).expect("journal"));
            let recorder = Recorder::default();
            let host = AgentHost::with_journal(
                AgentSpawnConfig::default(),
                recorder.sink(),
                Some(journal.clone()),
            );
            let pane = PaneId(7);
            assert!(host.open_with(
                pane,
                1,
                AgentPaneSpec {
                    model: None,
                    effort: None,
                    provider,
                    cwd: directory.path().to_owned(),
                    resume_session: None,
                    auto_approve: Some(AgentAutoApprove::Off),
                    workspace: AgentWorkspaceEnvironment::default(),
                },
                fixture_runner(provider, behavior, AgentAutoApprove::Off, false),
            ));
            let fixture = Fixture {
                host,
                recorder,
                pane,
            };
            fixture.wait_for_session();
            for turn_id in 1..=2 {
                fixture.command(HostCommand::Prompt(
                    AgentPrompt {
                        owner: ClientInstanceId::default(),
                        text: "look again".to_owned(),
                        images: vec![AgentImage {
                            format: "image/png".to_owned(),
                            data: b"image".to_vec(),
                        }],
                    }
                    .into(),
                ));
                fixture.recorder.wait("the completed prompt", |payload| {
                    matches!(payload, AgentStreamPayload::PromptFinished { turn_id: id, .. } if *id == turn_id)
                });
            }
            let users = fixture
                .recorder
                .payloads()
                .into_iter()
                .filter_map(|payload| match payload {
                    AgentStreamPayload::Update { update }
                        if update["sessionUpdate"] == "user_message_chunk" =>
                    {
                        Some(update)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(users.len(), 4, "{provider:?} {behavior:?}");
            for pair in users.as_chunks::<2>().0 {
                assert_eq!(pair[0]["content"]["text"], "look again");
                assert_eq!(pair[1]["content"]["data"], "aW1hZ2U=");
                assert_eq!(pair[0]["messageId"], pair[1]["messageId"]);
            }
            assert_ne!(users[0]["messageId"], users[2]["messageId"]);
            let replayed_users = journal
                .replay_for(provider, "fixture-session")
                .expect("replay")
                .into_iter()
                .filter_map(|(_, entry)| match entry {
                    crate::journal::JournalEntry::Update(update)
                        if update["sessionUpdate"] == "user_message_chunk" =>
                    {
                        Some(update)
                    }
                    crate::journal::JournalEntry::Update(_) => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(replayed_users, users);
            fixture.close();
        }
    }
}

fn scripted_reply_fixture(updates: Vec<AgentStreamPayload>, reason: &'static str) -> Fixture {
    Fixture::open_with_runner(Box::new(move |channels| {
        Box::pin(async move {
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "fixture-session".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            while let Ok(command) = channels.commands.recv().await {
                match command {
                    RuntimeCommand::Prompt { turn_id, .. } => {
                        channels
                            .events
                            .send(AgentStreamPayload::TurnStarted { turn_id })
                            .await
                            .map_err(|error| error.to_string())?;
                        for update in &updates {
                            channels
                                .events
                                .send(update.clone())
                                .await
                                .map_err(|error| error.to_string())?;
                            if let AgentStreamPayload::PermissionRequested { request_id, .. } =
                                update
                            {
                                let RuntimeControl::RespondPermission {
                                    request_id: answered,
                                    option_id,
                                } = channels
                                    .controls
                                    .recv()
                                    .await
                                    .map_err(|error| error.to_string())?
                                else {
                                    return Err("expected permission response".to_owned());
                                };
                                assert_eq!(*request_id, answered);
                                channels
                                    .events
                                    .send(AgentStreamPayload::PermissionResolved {
                                        request_id: answered,
                                        canceled: option_id.is_none(),
                                    })
                                    .await
                                    .map_err(|error| error.to_string())?;
                            }
                        }
                        channels
                            .events
                            .send(AgentStreamPayload::PromptFinished {
                                turn_id,
                                outcome: AgentPromptOutcome::Finished {
                                    stop_reason: serde_json::json!(reason),
                                },
                            })
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    RuntimeCommand::Shutdown => return Ok(()),
                    _ => {}
                }
            }
            Ok(())
        })
    }))
}

fn reply_chunk(text: &str) -> AgentStreamPayload {
    AgentStreamPayload::Update {
        update: serde_json::json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}),
    }
}

#[test]
fn agent_reply_final_text_follows_the_last_tool_update() {
    let fixture = scripted_reply_fixture(
        vec![
            reply_chunk("narration"),
            AgentStreamPayload::Update {
                update: serde_json::json!({"sessionUpdate": "tool_call"}),
            },
            reply_chunk("more narration"),
            AgentStreamPayload::Update {
                update: serde_json::json!({"sessionUpdate": "tool_call"}),
            },
            reply_chunk("pending"),
            AgentStreamPayload::Update {
                update: serde_json::json!({"sessionUpdate": "tool_call_update"}),
            },
            reply_chunk("final "),
            reply_chunk("answer"),
        ],
        "end_turn",
    );
    fixture.wait_for_session();
    let reply = fixture
        .prompt_waiting("go")
        .recv_timeout(DEADLINE)
        .expect("reply")
        .expect("success");
    assert_eq!(reply.final_text, "final answer");
    assert_eq!(reply.text, "narrationmore narrationpendingfinal answer");
    assert_eq!(reply.tool_calls, 2);
    assert_eq!(reply.stop_reason, "end_turn");
    assert!(reply.duration > Duration::ZERO);
    fixture.close();
}

#[test]
fn agent_reply_refusal_and_unknown_stop_reasons_keep_the_reply() {
    for reason in ["refusal", "max_tokens", "max_turn_requests", "new_reason"] {
        let fixture = scripted_reply_fixture(vec![reply_chunk("answer")], reason);
        fixture.wait_for_session();
        let result = fixture
            .prompt_waiting("go")
            .recv_timeout(DEADLINE)
            .expect("reply");
        let Err(AgentTurnFailure::Stopped {
            reason: actual,
            reply,
        }) = result
        else {
            panic!("expected stopped reply");
        };
        assert_eq!(actual, reason);
        assert_eq!(reply.stop_reason, reason);
        assert_eq!(reply.text, "answer");
        assert_eq!(reply.final_text, "answer");
        fixture.close();
    }
}

#[test]
fn agent_reply_caps_keep_utf8_tails() {
    let text = format!("{}last", "é".repeat(MAX_AGENT_TURN_REPLY_BYTES));
    let fixture = scripted_reply_fixture(vec![reply_chunk(&text), reply_chunk("!")], "end_turn");
    fixture.wait_for_session();
    let reply = fixture
        .prompt_waiting("go")
        .recv_timeout(DEADLINE)
        .expect("reply")
        .expect("success");
    assert!(reply.truncated);
    assert!(reply.text.len() <= MAX_AGENT_TURN_REPLY_BYTES);
    assert!(reply.text.len() >= MAX_AGENT_TURN_REPLY_BYTES - 1);
    assert!(reply.final_text.len() <= MAX_AGENT_FINAL_REPLY_BYTES);
    assert!(reply.final_text.len() >= MAX_AGENT_FINAL_REPLY_BYTES - 1);
    assert!(reply.text.ends_with("last!"));
    assert!(reply.final_text.ends_with("last!"));
    assert!(text[..text.len() - 4].ends_with(reply.text.trim_end_matches("last!")));
    fixture.close();
}

#[test]
fn agent_reply_allow_and_deny_answer_and_count_permissions() {
    for (policy, allowed, denied, action) in [
        (AgentBlockPolicy::Allow, 1, 0, "allowed"),
        (AgentBlockPolicy::Deny, 0, 1, "denied"),
    ] {
        let fixture = Fixture::open(
            Behavior::AskKindedPermission(ToolKind::Execute),
            AgentAutoApprove::Off,
        );
        fixture.wait_for_session();
        let audit = Arc::default();
        let reply = fixture
            .prompt_with_policy("go", policy, Arc::clone(&audit))
            .recv_timeout(DEADLINE)
            .expect("reply")
            .expect("success");
        assert_eq!(reply.permissions_requested, 1);
        assert_eq!(reply.permissions_allowed, allowed);
        assert_eq!(reply.permissions_denied, denied);
        assert_eq!(
            *audit.lock(),
            [format!("agent-send: {action} execute run it")]
        );
        fixture.close();
    }
}

#[test]
fn agent_reply_allow_waits_for_user_questions_and_deny_cancels_them() {
    for policy in [AgentBlockPolicy::Allow, AgentBlockPolicy::Deny] {
        let fixture = scripted_reply_fixture(
            vec![AgentStreamPayload::PermissionRequested {
                request_id: 42,
                tool_call: serde_json::json!({"toolCallId": "question"}),
                options: serde_json::json!([{"optionId": "yes", "name": "Yes", "kind": "answer"}]),
                questions: Vec::new(),
            }],
            "end_turn",
        );
        fixture.wait_for_session();
        let audit = Arc::default();
        let reply = fixture.prompt_with_policy("go", policy, Arc::clone(&audit));
        fixture.recorder.wait("question", |payload| {
            matches!(payload, AgentStreamPayload::PermissionRequested { .. })
        });
        if policy == AgentBlockPolicy::Allow {
            assert!(matches!(
                reply.recv_timeout(Duration::from_millis(30)),
                Err(crossbeam_channel::RecvTimeoutError::Timeout)
            ));
            assert!(audit.lock().is_empty());
            fixture.command(HostCommand::RespondPermission {
                response: PermissionResponse::Exact {
                    request_id: 42,
                    option_id: Some("yes".to_owned()),
                },
            });
        }
        let reply = reply
            .recv_timeout(DEADLINE)
            .expect("reply")
            .expect("success");
        assert_eq!(reply.permissions_requested, 1);
        assert_eq!(reply.permissions_allowed, 0);
        assert_eq!(
            reply.permissions_denied,
            u32::from(policy == AgentBlockPolicy::Deny)
        );
        if policy == AgentBlockPolicy::Deny {
            assert_eq!(*audit.lock(), ["agent-send: denied ? question"]);
        }
        fixture.close();
    }
}

#[test]
fn denying_a_question_card_dismisses_it() {
    let fixture = scripted_reply_fixture(
        vec![AgentStreamPayload::PermissionRequested {
            request_id: 42,
            tool_call: serde_json::json!({"toolCallId": "question"}),
            options: serde_json::json!([]),
            questions: vec![crate::stream::AgentQuestion {
                id: "q0".to_owned(),
                question: "Which fruit?".to_owned(),
                ..Default::default()
            }],
        }],
        "end_turn",
    );
    fixture.wait_for_session();
    fixture.prompt("go");
    fixture.recorder.wait("question", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });
    let (sender, receiver) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Select {
            request_id: None,
            choice: PermissionChoice::Deny,
            reply: zz_daemon_client::reply::Reply::new(move |result| {
                let _ = sender.send(result);
            }),
        },
    });
    assert_eq!(
        receiver.recv_timeout(DEADLINE).expect("reply"),
        Some(Ok("dismissed".to_owned()))
    );
    fixture.recorder.wait("dismissal", |payload| {
        matches!(
            payload,
            AgentStreamPayload::PermissionResolved {
                request_id: 42,
                canceled: true
            }
        )
    });
    fixture.close();
}

#[test]
fn a_waiting_prompt_receives_the_turns_text() {
    let fixture = Fixture::open(Behavior::Chunk, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let reply = fixture.prompt_waiting("go");
    let reply = reply
        .recv_timeout(DEADLINE)
        .expect("the turn settles")
        .expect("successful turn");
    assert_eq!(reply.text, "turn 0");
    assert_eq!(reply.final_text, "turn 0");
    assert_eq!(reply.stop_reason, "end_turn");
    assert!(!reply.truncated);
    fixture.close();
}

#[test]
fn a_prompt_queued_behind_a_hung_turn_is_handed_back_on_cancel() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let hung = fixture.prompt_waiting("first");
    fixture.recorder.wait("the turn to start", |payload| {
        matches!(payload, AgentStreamPayload::TurnStarted { .. })
    });
    let queued = fixture.prompt_waiting("second");
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts != 1 {
        assert!(Instant::now() < deadline, "the second prompt queues");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        queued.try_recv().is_err(),
        "a queued prompt is still pending"
    );

    fixture.command(HostCommand::Cancel);
    assert_eq!(
        queued
            .recv_timeout(DEADLINE)
            .expect("cancel hands the queue back"),
        Err(AgentTurnFailure::Reclaimed)
    );
    fixture.close();
    assert!(matches!(
        hung.recv_timeout(DEADLINE)
            .expect("closing settles the hung turn"),
        Err(AgentTurnFailure::Cancelled | AgentTurnFailure::Closed)
    ));
}

#[test]
fn a_prompt_past_the_queue_limit_settles_as_reclaimed() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let _hung = fixture.prompt_waiting("first");
    fixture.recorder.wait("the turn to start", |payload| {
        matches!(payload, AgentStreamPayload::TurnStarted { .. })
    });
    let queued = (0..MAX_AGENT_QUEUED_PROMPTS)
        .map(|index| fixture.prompt_waiting(&format!("queued {index}")))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts != MAX_AGENT_QUEUED_PROMPTS {
        assert!(Instant::now() < deadline, "the queue fills");
        thread::sleep(Duration::from_millis(5));
    }

    let overflow = fixture.prompt_waiting("one too many");
    assert_eq!(
        overflow
            .recv_timeout(DEADLINE)
            .expect("overflow settles at once"),
        Err(AgentTurnFailure::Reclaimed)
    );
    let payloads = fixture.recorder.wait("the overflow to be handed back", |payload| {
        matches!(payload, AgentStreamPayload::PromptsReclaimed { prompts } if prompts.iter().any(|prompt| prompt.text == "one too many"))
    });
    assert!(queued_prompt_texts(&payloads).contains(&"one too many".to_owned()));
    assert!(queued.iter().all(|reply| reply.try_recv().is_err()));
    fixture.close();
}

#[test]
fn dispatch_publishes_running_with_a_turn_boundary_before_the_agent_speaks() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("think quietly");
    let deadline = Instant::now() + DEADLINE;
    while !fixture
        .recorder
        .states
        .lock()
        .iter()
        .any(|(state, has_item)| state.phase == AgentConnectionPhase::Running && *has_item)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    let published = fixture
        .recorder
        .states
        .lock()
        .iter()
        .map(|(state, has_item)| (state.phase, *has_item))
        .collect::<Vec<_>>();
    let running = published
        .iter()
        .position(|(phase, _)| *phase == AgentConnectionPhase::Running)
        .expect("a running state is published");
    let boundary = published[..running]
        .iter()
        .filter(|(_, has_item)| *has_item)
        .count();
    assert_eq!(
        (
            published[running].1,
            fixture
                .recorder
                .items()
                .get(boundary)
                .map(|item| item.payload.clone())
        ),
        (true, Some(AgentStreamPayload::TurnStarted { turn_id: 1 })),
        "{published:?}"
    );
    fixture.close();
}

#[test]
fn an_auto_approved_tool_never_reaches_a_client() {
    let fixture = Fixture::open(Behavior::AskPermission, AgentAutoApprove::All);
    fixture.wait_for_session();
    fixture.prompt("go");
    let payloads = fixture.recorder.wait("the turn to finish", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });

    assert!(
        !payloads
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. })),
        "an auto-approved tool is answered daemon-side: {payloads:?}"
    );
    assert!(
        payloads.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::Update { update } if update.get("sessionUpdate")
                .and_then(Value::as_str) == Some("tool_call_update")
        )),
        "the approved tool call still reaches the transcript: {payloads:?}"
    );
    assert!(fixture.state().pending_permissions.is_empty());
    fixture.close();
}

#[test]
fn the_reads_tier_answers_a_read_tool_daemon_side() {
    let fixture = Fixture::open(
        Behavior::AskKindedPermission(ToolKind::Read),
        AgentAutoApprove::Reads,
    );
    fixture.wait_for_session();
    fixture.prompt("go");
    let payloads = fixture.recorder.wait("the turn to finish", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });

    assert!(
        !payloads
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. })),
        "a read-only kind is answered daemon-side: {payloads:?}"
    );
    assert!(fixture.state().pending_permissions.is_empty());
    fixture.close();
}

#[test]
fn the_reads_tier_sends_an_execute_tool_to_the_wizard() {
    let fixture = Fixture::open(
        Behavior::AskKindedPermission(ToolKind::Execute),
        AgentAutoApprove::Reads,
    );
    fixture.wait_for_session();
    fixture.prompt("go");
    fixture.recorder.wait("the permission request", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });

    assert_eq!(fixture.state().pending_permissions.len(), 1);
    assert!(
        !fixture
            .recorder
            .payloads()
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })),
        "running a command waits for the human"
    );
    fixture.close();
}

#[test]
fn permission_selection_uses_advertised_ids_and_kinds() {
    let pending = vec![
        AgentPendingPermission {
            request_id: 7,
            tool_call: serde_json::json!({}),
            options: serde_json::json!([
                { "optionId": "always", "kind": "allow_always" },
                { "optionId": "once", "kind": "allow_once" },
                { "optionId": "reject", "kind": "reject_once" },
                { "optionId": "later", "kind": "allow_once" }
            ]),
            questions: Vec::new(),
        },
        AgentPendingPermission {
            request_id: 8,
            tool_call: serde_json::json!({}),
            options: serde_json::json!([{ "optionId": "other", "kind": "allow_always" }]),
            questions: Vec::new(),
        },
    ];
    assert_eq!(
        select_permission(&pending, None, &PermissionChoice::Allow),
        Ok((7, "once".to_owned()))
    );
    assert_eq!(
        select_permission(&pending, None, &PermissionChoice::Deny),
        Ok((7, "reject".to_owned()))
    );
    assert_eq!(
        select_permission(&pending, Some(8), &PermissionChoice::Allow),
        Ok((8, "other".to_owned()))
    );
    assert_eq!(
        select_permission(
            &pending,
            None,
            &PermissionChoice::Option("always".to_owned())
        ),
        Ok((7, "always".to_owned()))
    );
    let error = select_permission(
        &pending,
        None,
        &PermissionChoice::Option("missing".to_owned()),
    )
    .expect_err("unknown option");
    assert!(error.contains("valid ids: always, once, reject, later"));
    assert_eq!(
        select_permission(&[], None, &PermissionChoice::Allow),
        Err("no pending permission".to_owned())
    );
    assert_eq!(
        select_permission(&pending, Some(99), &PermissionChoice::Allow),
        Err("no pending permission".to_owned())
    );
}

#[test]
fn a_typed_answer_fills_the_one_question_of_a_request() {
    let question = |id: &str| AgentQuestion {
        id: id.to_owned(),
        header: None,
        question: "Which color?".to_owned(),
        options: Vec::new(),
        multi_select: false,
        allow_other: true,
        secret: false,
    };
    let pending = |questions| AgentPendingPermission {
        request_id: 7,
        tool_call: Value::Null,
        options: Value::Array(Vec::new()),
        questions,
    };
    assert_eq!(
        typed_answer(&[pending(vec![question("q1")])], None, "blue"),
        Ok((
            7,
            vec![AgentQuestionAnswer {
                id: "q1".to_owned(),
                answers: vec!["blue".to_owned()],
            }]
        ))
    );
    assert_eq!(
        typed_answer(
            &[pending(vec![question("q1"), question("q2")])],
            Some(7),
            "blue"
        ),
        Err("request 7 asks 2 questions; --answer answers one".to_owned())
    );
    assert_eq!(
        typed_answer(&[], None, "blue"),
        Err("no pending permission".to_owned())
    );
}

#[test]
fn unqueueing_hands_back_what_the_user_queued_and_keeps_the_rest_waiting() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let queued = |text: &str, source| {
        HostCommand::Prompt(QueuedPrompt {
            prompt: AgentPrompt {
                owner: ClientInstanceId::default(),
                text: text.to_owned(),
                images: Vec::new(),
            },
            waiter: None,
            source,
            note: None,
            ask: None,
        })
    };
    fixture.prompt("busy");
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().phase != AgentConnectionPhase::Running {
        assert!(Instant::now() < deadline, "the first turn never ran");
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.command(queued(
        "[zz] %2 (codex) finished:\nok",
        PromptSource::Injected,
    ));
    fixture.command(queued("sent by a command", PromptSource::Command));
    fixture.command(queued("mine", PromptSource::Composer));
    while fixture.state().queued_prompts != 3 {
        assert!(Instant::now() < deadline, "the prompts never queued");
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.command(HostCommand::Unqueue);
    let reclaimed = fixture
        .recorder
        .wait("the reclaimed prompt", |payload| {
            matches!(payload, AgentStreamPayload::PromptsReclaimed { .. })
        })
        .into_iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PromptsReclaimed { prompts } => Some(prompts),
            _ => None,
        })
        .flatten()
        .map(|prompt| prompt.text)
        .collect::<Vec<_>>();
    assert_eq!(reclaimed, ["mine"]);
    let state = fixture.state();
    assert_eq!((state.queued_prompts, state.own_prompts), (2, 0));
}

#[test]
fn blocked_waiter_returns_without_settling_the_turn() {
    let fixture = Fixture::open(Behavior::AskPermission, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let (reply, result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::Prompt(QueuedPrompt {
        prompt: AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "first".to_owned(),
            images: Vec::new(),
        },
        waiter: Some(AgentTurnWaiter {
            reply: reply.into(),
            on_block: AgentBlockPolicy::Fail,
            audit: Arc::default(),
            asks: None,
        }),
        source: PromptSource::Composer,
        note: None,
        ask: None,
    }));
    let Err(AgentTurnFailure::Blocked { permission, reply }) =
        result.recv_timeout(DEADLINE).expect("blocked waiter")
    else {
        panic!("expected blocked permission");
    };
    assert_eq!(reply.stop_reason, "blocked");
    assert_eq!(reply.permissions_requested, 1);
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Running);
    assert_eq!(
        fixture.state().pending_permissions[0].request_id,
        permission.request_id
    );
    let second = fixture.prompt_waiting("second");
    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Exact {
            request_id: permission.request_id,
            option_id: Some("allow".to_owned()),
        },
    });
    let deadline = Instant::now() + DEADLINE;
    let next = loop {
        if let Some(next) = fixture.state().pending_permissions.first()
            && next.request_id != permission.request_id
        {
            break next.request_id;
        }
        assert!(Instant::now() < deadline, "second permission");
        thread::sleep(Duration::from_millis(2));
    };
    assert!(matches!(
        second.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Exact {
            request_id: next,
            option_id: Some("allow".to_owned()),
        },
    });
    assert_eq!(
        second
            .recv_timeout(DEADLINE)
            .expect("second waiter")
            .expect("second turn")
            .text,
        "turn 1"
    );
    fixture.close();
}

#[test]
fn a_surfaced_permission_waits_for_an_answer_and_then_resolves() {
    let fixture = Fixture::open(Behavior::AskPermission, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("go");
    let payloads = fixture.recorder.wait("the permission request", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });

    let AgentStreamPayload::PermissionRequested {
        request_id,
        options,
        ..
    } = payloads
        .iter()
        .find(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
        .expect("permission request")
        .clone()
    else {
        unreachable!()
    };
    assert_eq!(options.as_array().map(Vec::len), Some(2));
    let pending = fixture.state().pending_permissions;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request_id, request_id);
    assert!(
        !fixture
            .recorder
            .payloads()
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })),
        "the turn waits for the human"
    );

    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Exact {
            request_id,
            option_id: Some("allow".to_owned()),
        },
    });
    let payloads = fixture.recorder.wait("the turn to finish", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });
    assert!(payloads.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PermissionResolved { request_id: resolved, canceled: false }
            if *resolved == request_id
    )));
    assert!(fixture.state().pending_permissions.is_empty());
    fixture.close();
}

#[test]
fn a_prompt_typed_during_a_turn_queues_and_dispatches_when_it_settles() {
    let fixture = Fixture::open(Behavior::AskPermission, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("first");
    let payloads = fixture.recorder.wait("the permission request", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });
    fixture.prompt("second");

    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.state().queued_prompts, 1);
    assert_eq!(
        chunk_texts(&fixture.recorder.payloads()),
        ["first", "turn 0"]
    );

    let AgentStreamPayload::PermissionRequested { request_id, .. } = payloads
        .iter()
        .find(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
        .expect("permission request")
        .clone()
    else {
        unreachable!()
    };
    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Exact {
            request_id,
            option_id: Some("allow".to_owned()),
        },
    });

    let payloads = fixture.recorder.wait("the queued turn", |payload| {
        matches!(
            payload,
            AgentStreamPayload::Update { update }
                if update.get("content").and_then(|content| content.get("text"))
                    .and_then(Value::as_str) == Some("turn 1")
        )
    });
    assert_eq!(
        chunk_texts(&payloads),
        ["first", "turn 0", "second", "turn 1"]
    );
    assert_eq!(fixture.state().queued_prompts, 0);
    fixture.close();
}

#[test]
fn a_new_agent_session_waiter_finishes_after_session_switched() {
    let fixture = Fixture::open(Behavior::Chunk, AgentAutoApprove::Off);
    fixture.wait_for_session();
    let previous = fixture.state().session_id;
    let (reply, result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/other"),
        reply: Some(reply.into()),
    });
    assert_eq!(result.recv_timeout(DEADLINE).unwrap(), Ok(()));
    let state = fixture.state();
    assert_eq!(state.phase, AgentConnectionPhase::Ready);
    assert_ne!(state.session_id, previous);
    assert_eq!(state.cwd, PathBuf::from("/other"));
    assert!(
        fixture
            .recorder
            .payloads()
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::SessionSwitched { .. }))
    );
    assert!(
        fixture
            .prompt_waiting("fresh")
            .recv_timeout(DEADLINE)
            .unwrap()
            .is_ok()
    );
    fixture.close();
}

#[test]
fn a_new_agent_session_reclaims_queued_prompts_before_becoming_ready() {
    let (started, received) = crossbeam_channel::bounded(1);
    let (release, resume) = async_channel::bounded(1);
    let runner: PaneRunner = Box::new(move |channels| {
        Box::pin(async move {
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::Open { .. }
            ));
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "before".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .unwrap();
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::NewSession { .. }
            ));
            started.send(()).unwrap();
            resume.recv().await.unwrap();
            channels
                .events
                .send(AgentStreamPayload::SessionReset { restoring: true })
                .await
                .unwrap();
            channels
                .events
                .send(AgentStreamPayload::SessionSwitched {
                    session_id: "after".to_owned(),
                    cwd: PathBuf::from("/"),
                    modes: None,
                    config_options: None,
                    replay: Vec::new(),
                })
                .await
                .unwrap();
            if let Ok(command) = channels.commands.recv().await {
                assert!(matches!(command, RuntimeCommand::Shutdown));
            }
            Ok(())
        })
    });
    let fixture = Fixture::open_with_runner(runner);
    fixture.wait_for_session();
    let (reply, result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/"),
        reply: Some(reply.into()),
    });
    received.recv_timeout(DEADLINE).unwrap();
    let prompt = fixture.prompt_waiting("old session prompt");
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.state().queued_prompts, 1);
    release.try_send(()).unwrap();
    assert_eq!(result.recv_timeout(DEADLINE).unwrap(), Ok(()));
    assert_eq!(
        prompt.recv_timeout(DEADLINE).unwrap(),
        Err(AgentTurnFailure::Reclaimed)
    );
    assert_eq!(fixture.state().queued_prompts, 0);
    assert_eq!(
        queued_prompt_texts(&fixture.recorder.payloads()),
        ["old session prompt"]
    );
    fixture.close();
}

#[test]
fn a_new_agent_session_reset_reclaims_prompts_still_in_the_mailbox() {
    let recorder = Recorder::default();
    let spec = AgentPaneSpec {
        model: None,
        effort: None,
        provider: AgentProvider::Codex,
        cwd: PathBuf::from("/"),
        resume_session: None,
        auto_approve: None,
        workspace: AgentWorkspaceEnvironment::default(),
    };
    let (inbox, _inbox_rx) = async_channel::bounded(1);
    let (commands, command_rx) = async_channel::bounded(1);
    let (prompts, prompt_rx) = async_channel::bounded(MAX_AGENT_QUEUED_PROMPTS);
    let (controls, _control_rx) = async_channel::bounded(1);
    let (close, _close_rx) = async_channel::bounded(1);
    let mut pump = PanePump {
        pane: PaneId(7),
        generation: 1,
        state: Arc::new(Mutex::new(AgentPaneState::new(&spec))),
        spec,
        sink: Arc::new(recorder.sink()),
        inbox,
        commands,
        prompts: prompt_rx,
        controls,
        close,
        seq: 0,
        queue: VecDeque::new(),
        git_refresh: GitRefreshGate::default(),
        next_turn_id: 0,
        active_turn: None,
        dispatched_prompt: None,
        active_waiter: None,
        joined_waiters: Vec::new(),
        session_waiter: None,
        new_session_pending: true,
        turn_reply: AgentTurnReply::default(),
        turn_started: Instant::now(),
        closing: false,
    };
    let (reply, result) = crossbeam_channel::bounded(1);
    prompts
        .try_send(QueuedPrompt {
            prompt: AgentPrompt {
                owner: ClientInstanceId::default(),
                text: "still in mailbox".to_owned(),
                images: Vec::new(),
            },
            waiter: Some(AgentTurnWaiter {
                reply: reply.into(),
                on_block: AgentBlockPolicy::Wait,
                audit: Arc::new(Mutex::new(Vec::new())),
                asks: None,
            }),
            source: PromptSource::Composer,
            note: None,
            ask: None,
        })
        .unwrap();
    pump.observe(AgentStreamPayload::SessionReset { restoring: true });
    assert_eq!(result.try_recv().unwrap(), Err(AgentTurnFailure::Reclaimed));
    assert!(pump.prompts.is_empty());
    assert!(command_rx.is_empty());
    assert_eq!(
        queued_prompt_texts(&recorder.payloads()),
        ["still in mailbox"]
    );
}

#[test]
fn a_new_agent_session_waiter_reports_adapter_failure() {
    let runner: PaneRunner = Box::new(|channels| {
        Box::pin(async move {
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::Open { .. }
            ));
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "before".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .unwrap();
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::NewSession { .. }
            ));
            Err("fixture adapter exited".to_owned())
        })
    });
    let fixture = Fixture::open_with_runner(runner);
    fixture.wait_for_session();
    let (reply, result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/"),
        reply: Some(reply.into()),
    });
    assert_eq!(
        result.recv_timeout(DEADLINE).unwrap(),
        Err("fixture adapter exited".to_owned())
    );
    fixture.close();
}

#[test]
fn a_concurrent_new_agent_session_keeps_the_first_waiter_until_close() {
    let (started, received) = crossbeam_channel::bounded(1);
    let runner: PaneRunner = Box::new(move |channels| {
        Box::pin(async move {
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::Open { .. }
            ));
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "before".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .unwrap();
            assert!(matches!(
                channels.commands.recv().await.unwrap(),
                RuntimeCommand::NewSession { .. }
            ));
            started.send(()).unwrap();
            std::future::pending::<Result<(), String>>().await
        })
    });
    let fixture = Fixture::open_with_runner(runner);
    fixture.wait_for_session();
    let (first, first_result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/"),
        reply: Some(first.into()),
    });
    received.recv_timeout(DEADLINE).unwrap();
    let (second, second_result) = crossbeam_channel::bounded(1);
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/"),
        reply: Some(second.into()),
    });
    assert_eq!(
        second_result.recv_timeout(DEADLINE).unwrap(),
        Err("finish or cancel the current turn before changing sessions".to_owned())
    );
    assert_eq!(
        first_result.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    );
    fixture.close();
    assert!(first_result.recv_timeout(DEADLINE).unwrap().is_err());
}

#[test]
fn a_session_change_cannot_cross_an_active_turn() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("running");
    fixture
        .recorder
        .wait("the turn", |payload| chunk_text(payload) == Some("turn 0"));
    fixture.command(HostCommand::NewSession {
        cwd: PathBuf::from("/other"),
        reply: None,
    });
    fixture.command(HostCommand::SwitchSession {
        session: AgentSessionSummary {
            session_id: "other".to_owned(),
            cwd: PathBuf::from("/other"),
            additional_directories: Vec::new(),
            title: None,
            updated_at: None,
        },
    });
    let deadline = Instant::now() + DEADLINE;
    loop {
        let payloads = fixture.recorder.payloads();
        if payloads
            .iter()
            .filter(|payload| matches!(payload, AgentStreamPayload::SessionSwitchFailed { .. }))
            .count()
            == 2
        {
            assert!(
                !payloads.iter().any(|payload| {
                    matches!(payload, AgentStreamPayload::SessionSwitched { .. })
                })
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "session commands were not rejected"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Running);
    fixture.close();
}

#[test]
fn close_preempts_a_runner_that_never_reads_shutdown() {
    let recorder = Recorder::default();
    let host = AgentHost::with_journal(AgentSpawnConfig::default(), recorder.sink(), None);
    let pane = PaneId(30);
    let runner: PaneRunner = Box::new(|_| Box::pin(std::future::pending()));
    assert!(host.open_with(
        pane,
        1,
        AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
        },
        runner,
    ));
    let thread = host.close(pane).expect("pane thread");
    let (finished, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = finished.send(thread.join().is_ok());
    });
    assert_eq!(wait.recv_timeout(Duration::from_secs(1)), Ok(true));
}

#[test]
fn close_reclaims_a_prompt_the_runtime_has_not_accepted() {
    let runner: PaneRunner = Box::new(|channels| {
        Box::pin(async move {
            channels
                .events
                .send(AgentStreamPayload::Ready {
                    agent_name: "fixture".to_owned(),
                    agent_key: "fixture".to_owned(),
                    auth_methods: Vec::new(),
                    capabilities: AgentSessionCapabilities::default(),
                })
                .await
                .map_err(|error| error.to_string())?;
            channels
                .events
                .send(AgentStreamPayload::SessionReady {
                    session_id: "fixture-session".to_owned(),
                    modes: None,
                    config_options: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            std::future::pending::<Result<(), String>>().await
        })
    });
    let fixture = Fixture::open_with_runner(runner);
    fixture.wait_for_session();
    fixture.prompt("keep me");
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().phase != AgentConnectionPhase::Running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }

    let recorder = fixture.recorder.clone();
    let thread = fixture.host.close(fixture.pane).expect("the pane thread");
    thread.join().expect("the pane thread should settle");
    assert_eq!(queued_prompt_texts(&recorder.payloads()), ["keep me"]);
}

#[test]
fn a_pane_rejects_commands_beyond_its_bounded_backlog() {
    let recorder = Recorder::default();
    let host = AgentHost::with_journal(AgentSpawnConfig::default(), recorder.sink(), None);
    let pane = PaneId(31);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let runner: PaneRunner = Box::new(move |_| {
        Box::pin(async move {
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            std::future::pending().await
        })
    });
    assert!(host.open_with(
        pane,
        1,
        AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
        },
        runner,
    ));
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runner entered");
    for text in ["first", "second"] {
        assert!(
            host.command(
                pane,
                HostCommand::Prompt(
                    AgentPrompt {
                        owner: ClientInstanceId::default(),
                        text: text.to_owned(),
                        images: Vec::new(),
                    }
                    .into()
                )
            )
            .is_ok()
        );
    }
    for request_id in 0..PANE_INBOX_CAPACITY {
        assert!(
            host.command(
                pane,
                HostCommand::SetMode {
                    mode_id: request_id.to_string(),
                }
            )
            .is_ok()
        );
    }
    assert!(
        host.command(
            pane,
            HostCommand::SetMode {
                mode_id: PANE_INBOX_CAPACITY.to_string(),
            }
        )
        .is_err()
    );
    assert!(host.command(pane, HostCommand::Cancel).is_ok());
    assert!(
        host.command(
            pane,
            HostCommand::RespondPermission {
                response: PermissionResponse::Exact {
                    request_id: 9,
                    option_id: None,
                }
            },
        )
        .is_ok()
    );

    let thread = host.close(pane).expect("pane thread");
    release_tx.send(()).expect("release runner");
    let (finished, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = finished.send(thread.join().is_ok());
    });
    assert_eq!(wait.recv_timeout(Duration::from_secs(1)), Ok(true));
    assert_eq!(
        queued_prompt_texts(&recorder.payloads()),
        ["first", "second"]
    );
}

#[test]
fn unqueueing_hands_the_queued_prompts_back_with_their_images() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("running");
    fixture.command(HostCommand::Prompt(
        AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "queued".to_owned(),
            images: vec![AgentImage {
                format: "image/png".to_owned(),
                data: b"zz".to_vec(),
            }],
        }
        .into(),
    ));
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }

    fixture.command(HostCommand::Unqueue);
    let payloads = fixture.recorder.wait("the reclaimed prompts", |payload| {
        matches!(payload, AgentStreamPayload::PromptsReclaimed { .. })
    });
    assert_eq!(queued_prompt_texts(&payloads), ["queued"]);
    let AgentStreamPayload::PromptsReclaimed { prompts } = payloads
        .iter()
        .find(|payload| matches!(payload, AgentStreamPayload::PromptsReclaimed { .. }))
        .expect("reclaimed prompts")
        .clone()
    else {
        unreachable!()
    };
    assert_eq!(prompts[0].images[0].data, b"zz");
    assert_eq!(fixture.state().queued_prompts, 0);
    fixture.close();
}

#[test]
fn queued_prompts_stay_inside_one_reclaim_frame() {
    let fixture = Fixture::open(Behavior::Hang, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("running");
    fixture.command(HostCommand::Prompt(
        AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "queued".to_owned(),
            images: vec![AgentImage {
                format: "image/png".to_owned(),
                data: vec![0; MAX_AGENT_PROMPT_BYTES - 1024],
            }],
        }
        .into(),
    ));
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }

    fixture.command(HostCommand::Prompt(
        AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "returned".to_owned(),
            images: vec![AgentImage {
                format: "image/png".to_owned(),
                data: vec![0; 2048],
            }],
        }
        .into(),
    ));
    let payloads = fixture.recorder.wait("the overflow prompt", |payload| {
        matches!(payload, AgentStreamPayload::PromptsReclaimed { .. })
    });
    assert_eq!(queued_prompt_texts(&payloads), ["returned"]);
    assert_eq!(fixture.state().queued_prompts, 1);
    fixture.command(HostCommand::Unqueue);
    fixture.close();
}

#[test]
fn a_session_the_agent_cannot_load_is_replayed_out_of_the_journal() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    for (index, text) in ["first restored", "second restored"]
        .into_iter()
        .enumerate()
    {
        let mut chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
        chunk.message_id = Some(MessageId::new(format!("restored-{index}")));
        let update =
            serde_json::to_value(SessionUpdate::AgentMessageChunk(chunk)).expect("encode update");
        journal.append("stale-session", &update).expect("append");
    }

    let fixture = Fixture::build(
        Behavior::Chunk,
        AgentAutoApprove::Off,
        false,
        Some(&journal),
        Some("stale-session".to_owned()),
        None,
    );
    let payloads = fixture.recorder.wait("the restored session", |payload| {
        matches!(payload, AgentStreamPayload::SessionReady { .. })
    });

    assert!(matches!(
        payloads[1],
        AgentStreamPayload::SessionReset { restoring: true }
    ));
    let AgentStreamPayload::SessionReady { session_id, .. } = payloads
        .iter()
        .find(|payload| matches!(payload, AgentStreamPayload::SessionReady { .. }))
        .expect("restored session")
        .clone()
    else {
        unreachable!()
    };
    assert_eq!(session_id, "fixture-session");
    assert_eq!(
        chunk_texts(&payloads),
        ["first restored", "second restored"]
    );
    assert!(
        journal.replay("stale-session").expect("replay").is_empty(),
        "the superseded journal is not left behind to be restored twice"
    );
    assert_eq!(fixture.state().phase, AgentConnectionPhase::Ready);
    fixture.close();
}

#[test]
fn adapter_user_history_survives_loading_after_a_locally_submitted_prompt() {
    let fixture = Fixture::build(
        Behavior::Echo,
        AgentAutoApprove::Off,
        true,
        None,
        Some("prior-session".to_owned()),
        None,
    );
    fixture.wait_for_session();
    assert_eq!(
        chunk_texts(&fixture.recorder.payloads()),
        ["loaded user history"]
    );
    fixture.prompt("new user message");
    fixture.recorder.wait("the completed prompt", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });
    assert_eq!(
        chunk_texts(&fixture.recorder.payloads()),
        ["loaded user history", "new user message", "turn 0"],
    );
    fixture.command(HostCommand::SwitchSession {
        session: AgentSessionSummary {
            session_id: "other-session".to_owned(),
            cwd: PathBuf::from("/"),
            additional_directories: Vec::new(),
            title: None,
            updated_at: None,
        },
    });
    let switched = fixture.recorder.wait("loaded session history", |payload| {
        matches!(payload, AgentStreamPayload::SessionSwitched { session_id, .. } if session_id == "other-session")
    });
    assert_eq!(
        chunk_texts(&switched),
        [
            "loaded user history",
            "new user message",
            "turn 0",
            "loaded user history"
        ]
    );
    fixture.close();
}

#[test]
fn pane_outside_git_has_no_summary_on_session_ready() {
    let scratch = tempfile::tempdir().expect("temporary directory");
    let fixture = Fixture::build(
        Behavior::Chunk,
        AgentAutoApprove::Off,
        true,
        None,
        None,
        Some(scratch.path().to_path_buf()),
    );
    fixture.wait_for_session();

    assert_eq!(fixture.state().git, None);
    fixture.close();
}

#[test]
fn git_summary_refreshes_on_session_ready_and_prompt_finished() {
    let Some((_scratch, root)) = seeded_git_repo() else {
        return;
    };
    let fixture = Fixture::build(
        Behavior::AskPermission,
        AgentAutoApprove::Off,
        true,
        None,
        None,
        Some(root.clone()),
    );
    fixture.wait_for_session();
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().git.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        fixture.state().git,
        Some(AgentGitSummary {
            branch: Some("main".to_owned()),
            changed_files: 0,
            additions: 0,
            deletions: 0,
        })
    );
    let states = fixture.recorder.states.lock();
    let ready = states
        .iter()
        .position(|(state, has_item)| {
            *has_item && state.phase == AgentConnectionPhase::Ready && state.git.is_none()
        })
        .expect("session readiness was published before Git capture");
    let summarized = states
        .iter()
        .position(|(state, has_item)| {
            !*has_item
                && state
                    .git
                    .as_ref()
                    .is_some_and(|git| git.branch.as_deref() == Some("main"))
        })
        .expect("Git summary was published as a state-only update");
    assert!(ready < summarized);
    drop(states);

    fs::write(root.join("tracked.txt"), "one\nchanged\n").expect("tracked edit");
    fs::write(root.join("fresh.txt"), "new\nfile\n").expect("untracked file");
    let state_marker = fixture.recorder.states.lock().len();
    fixture.prompt("inspect the changes");
    let payloads = fixture.recorder.wait("the permission request", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });
    let request_id = payloads
        .iter()
        .find_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .expect("permission request");
    fixture.command(HostCommand::RespondPermission {
        response: PermissionResponse::Exact {
            request_id,
            option_id: Some("allow".to_owned()),
        },
    });
    fixture.recorder.wait("the turn to finish", |payload| {
        matches!(payload, AgentStreamPayload::PromptFinished { .. })
    });
    let deadline = Instant::now() + DEADLINE;
    while fixture
        .state()
        .git
        .as_ref()
        .is_none_or(|git| git.changed_files != 2)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(
        fixture.state().git,
        Some(AgentGitSummary {
            branch: Some("main".to_owned()),
            changed_files: 2,
            additions: 3,
            deletions: 1,
        })
    );
    let states = fixture.recorder.states.lock();
    let settled = states[state_marker..]
        .iter()
        .position(|(state, has_item)| {
            *has_item
                && state.phase == AgentConnectionPhase::Ready
                && state.git.as_ref().is_some_and(|git| git.changed_files == 0)
        })
        .expect("turn settlement was published with the previous summary");
    let refreshed = states[state_marker..]
        .iter()
        .position(|(state, has_item)| {
            !*has_item && state.git.as_ref().is_some_and(|git| git.changed_files == 2)
        })
        .expect("Git refresh was published as a state-only update");
    assert!(settled < refreshed);
    drop(states);
    fixture.close();
}

#[test]
fn git_refresh_command_follows_a_branch_switched_outside_the_agent() {
    let Some((_scratch, root)) = seeded_git_repo() else {
        return;
    };
    let fixture = Fixture::build(
        Behavior::AskPermission,
        AgentAutoApprove::Off,
        true,
        None,
        None,
        Some(root.clone()),
    );
    fixture.wait_for_session();
    let branch = || fixture.state().git.and_then(|git| git.branch);
    let wait_for = |expected: &str| {
        let deadline = Instant::now() + DEADLINE;
        while branch().as_deref() != Some(expected) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    wait_for("main");
    assert_eq!(branch().as_deref(), Some("main"));

    git(&root, &["switch", "--quiet", "-c", "feature"]);
    fixture.command(HostCommand::RefreshGit);
    wait_for("feature");

    assert_eq!(branch().as_deref(), Some("feature"));
    fixture.close();
}

#[test]
fn stale_git_summary_results_are_dropped_by_generation_token_and_cwd() {
    let Some((_scratch, root)) = seeded_git_repo() else {
        return;
    };
    let fixture = Fixture::build(
        Behavior::Hang,
        AgentAutoApprove::Off,
        true,
        None,
        None,
        Some(root.clone()),
    );
    fixture.wait_for_session();
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().git.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let marker = fixture.recorder.states.lock().len();
    let inbox = fixture
        .host
        .registry
        .panes
        .lock()
        .get(&fixture.pane)
        .expect("pane")
        .inbox
        .clone();
    for (generation, refresh, cwd, branch) in [
        (0, 2, root.clone(), "wrong-generation"),
        (1, 1, root.clone(), "wrong-token"),
        (1, 2, root.join("elsewhere"), "wrong-cwd"),
        (1, 2, root.clone(), "accepted"),
    ] {
        inbox
            .send_blocking(PaneInput::GitSummary {
                generation,
                refresh,
                cwd,
                summary: Some(AgentGitSummary {
                    branch: Some(branch.to_owned()),
                    changed_files: 0,
                    additions: 0,
                    deletions: 0,
                }),
            })
            .expect("Git result reaches pane");
    }
    let deadline = Instant::now() + DEADLINE;
    while fixture
        .state()
        .git
        .as_ref()
        .and_then(|git| git.branch.as_deref())
        != Some("accepted")
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }

    let states = fixture.recorder.states.lock();
    let branches = states[marker..]
        .iter()
        .filter_map(|(state, _)| state.git.as_ref()?.branch.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(branches, ["accepted"]);
    drop(states);
    fixture.close();
}

#[test]
fn closing_a_pane_cancels_what_it_still_owed() {
    let fixture = Fixture::open(Behavior::AskPermission, AgentAutoApprove::Off);
    fixture.wait_for_session();
    fixture.prompt("go");
    fixture.recorder.wait("the permission request", |payload| {
        matches!(payload, AgentStreamPayload::PermissionRequested { .. })
    });
    fixture.command(HostCommand::Prompt(
        AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "queued".to_owned(),
            images: Vec::new(),
        }
        .into(),
    ));
    let deadline = Instant::now() + DEADLINE;
    while fixture.state().queued_prompts == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }

    let recorder = fixture.recorder.clone();
    let pane = fixture.pane;
    let thread = fixture.host.close(pane).expect("the pane thread");
    thread.join().expect("the pane thread should settle");

    let payloads = recorder.payloads();
    assert_eq!(queued_prompt_texts(&payloads), ["go", "queued"]);
    assert!(
        payloads.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PermissionResolved { canceled: true, .. }
        )),
        "a closed pane owes no permission answer: {payloads:?}"
    );
    assert!(
        !payloads
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::PaneFailed { .. })),
        "closing on purpose is not a failure: {payloads:?}"
    );
    assert!(fixture.host.snapshot_state(pane).is_none());
}
