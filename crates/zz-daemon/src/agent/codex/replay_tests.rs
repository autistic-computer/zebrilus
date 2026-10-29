use async_channel::Receiver;

use super::*;

fn setup(thread: &str) -> (Runtime, Receiver<AgentStreamPayload>) {
    let command = CodexCommand::parse("codex").expect("command");
    let (events, receiver) = async_channel::unbounded();
    let (child_tx, _) = async_channel::unbounded();
    let mut runtime = Runtime {
        provider: AgentProvider::Codex,
        command,
        workspace: AgentWorkspaceEnvironment::default(),
        auto_approve: Arc::new(Mutex::new(AgentAutoApprove::Off)),
        permission_ids: Arc::new(AtomicU64::new(1)),
        journal: None,
        events,
        child_tx,
        process: None,
        generation: 1,
        stderr: StderrTail::default(),
        initialized: true,
        ready_sent: true,
        session: Some(Session {
            id: thread.to_owned(),
            cwd: PathBuf::from("/work"),
        }),
        starting: None,
        translator: Translator::default(),
        requests: HashMap::new(),
        next_request: 0,
        permissions: HashMap::new(),
        turns: TurnState::default(),
        chat: Mutex::new(Shadow::recording().codex()),
        last_turn: 0,
        deferred_cancels: HashSet::new(),
        settings: Settings::default(),
        notices: 0,
        reply: None,
        sides: HashMap::new(),
        tasks: Vec::new(),
        loading: None,
        instructions: None,
    };
    smol::block_on(runtime.apply(&Fact::SessionReady)).expect("ready");
    (runtime, receiver)
}

#[derive(Debug, PartialEq)]
enum Seen {
    Finished(u64, AgentPromptOutcome),
    Busy(bool),
}

fn turn_events(receiver: &Receiver<AgentStreamPayload>) -> Vec<Seen> {
    let mut seen = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        match event {
            AgentStreamPayload::PromptFinished { turn_id, outcome } => {
                seen.push(Seen::Finished(turn_id, outcome));
            }
            AgentStreamPayload::Activity { busy } => seen.push(Seen::Busy(busy)),
            _ => {}
        }
    }
    seen
}

fn end_turn(turn: u64) -> Seen {
    Seen::Finished(
        turn,
        AgentPromptOutcome::Finished {
            stop_reason: Value::from("end_turn"),
        },
    )
}

fn run(recording: &str, thread: &str) -> (Runtime, Vec<(usize, Seen)>) {
    let (mut runtime, receiver) = setup(thread);
    let mut seen = Vec::new();
    let mut prompts = HashMap::new();
    for (index, line) in recording.lines().enumerate() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        let message = &entry["msg"];
        if entry["dir"] == "in" && message["method"] == "turn/interrupt" {
            let _ = smol::block_on(runtime.apply(&Fact::Cancel));
        } else if entry["dir"] == "in" && message["method"] == "turn/steer" {
            if let Some(command) = message["params"]["expectedTurnId"].as_str() {
                let _ = smol::block_on(runtime.apply(&Fact::Steer {
                    command: command.to_owned(),
                }));
            }
        } else if entry["dir"] == "exit" {
            let _ = smol::block_on(runtime.apply(&Fact::Exited {
                message: "the recording ended the process".to_owned(),
            }));
        } else if entry["dir"] == "in" && message["method"] == "turn/start" {
            let turn = prompts.len() as u64 + 1;
            prompts.insert(message["id"].clone(), turn);
            let fact = Fact::Prompt {
                turn,
                command: None,
            };
            assert_eq!(smol::block_on(runtime.apply(&fact)), Ok(None));
        } else if entry["dir"] == "out" {
            match (message.get("method"), prompts.get(&message["id"])) {
                (None, Some(turn)) => {
                    let fact = Fact::Accepted {
                        turn: *turn,
                        command: message["result"]["turn"]["id"]
                            .as_str()
                            .expect("turn id")
                            .to_owned(),
                    };
                    smol::block_on(runtime.apply(&fact)).expect("accepted");
                }
                (Some(_), _) => {
                    smol::block_on(runtime.message(message.clone())).expect("message");
                }
                (None, None) => {}
            }
        }
        seen.extend(
            turn_events(&receiver)
                .into_iter()
                .map(|event| (index + 1, event)),
        );
    }
    (runtime, seen)
}

fn replay(recording: &str, thread: &str) -> Vec<(usize, Seen)> {
    run(recording, thread).1
}

#[test]
fn the_tools_recording_settles_each_turn_on_its_completion() {
    assert_eq!(
        replay(
            include_str!("fixtures/tools.ndjson"),
            "01a117e9-e87d-7431-9e95-14e3ab67fc26"
        ),
        [(19, end_turn(1)), (92, end_turn(2))]
    );
}

#[test]
fn a_child_thread_finishing_does_not_touch_the_parent_turn() {
    assert_eq!(
        replay(
            include_str!("fixtures/subagent.ndjson"),
            "01a11854-86d3-7e22-bf6e-748a315d3a61"
        ),
        [(1, Seen::Busy(true))]
    );
}

#[test]
fn an_idle_thread_before_a_failed_completion_reports_the_failure() {
    let thread = "thread-1";
    let (mut runtime, receiver) = setup(thread);
    smol::block_on(runtime.apply(&Fact::Prompt {
        turn: 1,
        command: None,
    }))
    .expect("prompt");
    for message in [
        json!({ "method": "turn/started", "params": { "threadId": thread, "turn": { "id": "t1" } } }),
        json!({ "method": "thread/status/changed", "params": { "threadId": thread, "status": { "type": "idle" } } }),
        json!({ "method": "turn/completed", "params": { "threadId": thread, "turn": { "id": "t1", "status": "failed", "error": { "message": "usage limit reached" } } } }),
    ] {
        smol::block_on(runtime.message(message)).expect("message");
    }
    assert_eq!(
        turn_events(&receiver),
        [Seen::Finished(
            1,
            AgentPromptOutcome::Failed {
                message: "usage limit reached".to_owned()
            }
        )]
    );
}

#[test]
fn a_cancel_before_codex_names_the_turn_still_interrupts_it() {
    let thread = "thread-1";
    let (mut runtime, receiver) = setup(thread);
    smol::block_on(runtime.apply(&Fact::Prompt {
        turn: 1,
        command: None,
    }))
    .expect("prompt");
    smol::block_on(runtime.control(RuntimeControl::Cancel {
        turn_id: 1,
        session_id: None,
    }))
    .expect("cancel");
    assert!(runtime.requests.is_empty());
    smol::block_on(runtime.message(json!({
        "method": "turn/started",
        "params": { "threadId": thread, "turn": { "id": "t1" } },
    })))
    .expect("started");
    assert_eq!(runtime.requests.len(), 1);
    smol::block_on(runtime.message(json!({
        "method": "turn/completed",
        "params": { "threadId": thread, "turn": { "id": "t1", "status": "interrupted" } },
    })))
    .expect("completed");
    assert_eq!(turn_events(&receiver), [Seen::Finished(1, cancelled())]);
}

fn chat_of(runtime: &Runtime) -> (zz_protocol::chat::ChatView, Vec<String>) {
    let shadow = runtime.chat.lock();
    let mut view = zz_protocol::chat::ChatView::default();
    for event in &shadow.events {
        if let Err(skip) = view.apply(event) {
            panic!("the view skipped {event:?}: {skip:?}");
        }
    }
    let rejects = shadow
        .rejects
        .iter()
        .map(|(fact, reject)| {
            let fact = format!("{fact:?}");
            format!("{reject:?} <- {}", &fact[..fact.len().min(160)])
        })
        .collect();
    (view, rejects)
}

#[test]
fn every_recording_drives_the_chat_machine() {
    for (name, recording, thread) in [
        (
            "subagent",
            include_str!("fixtures/subagent.ndjson"),
            "01a11854-86d3-7e22-bf6e-748a315d3a61",
        ),
        (
            "tools",
            include_str!("fixtures/tools.ndjson"),
            "01a117e9-e87d-7431-9e95-14e3ab67fc26",
        ),
    ] {
        let (runtime, _) = run(recording, thread);
        let (view, rejects) = chat_of(&runtime);
        assert!(rejects.is_empty(), "{name}: {rejects:#?}");
        assert_eq!(
            view.running().is_some(),
            runtime.turns.turn().is_some(),
            "{name}: the chat and the driver disagree on a running turn"
        );
        if view.running().is_none() {
            assert!(
                view.open_items().all(|item| item.background),
                "{name}: an item outlived its turn"
            );
        }
        assert!(!view.turns.is_empty(), "{name}: no turn seen");
    }
}

#[test]
#[ignore = "replays every recording in $ZZ_REPLAY_DIR, named <scenario>__<thread>.ndjson"]
fn recordings_in_a_folder_drive_the_chat_machine() {
    let Ok(dir) = std::env::var("ZZ_REPLAY_DIR") else {
        return;
    };
    let mut paths = std::fs::read_dir(dir)
        .expect("replay dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let recording = std::fs::read_to_string(&path).expect("recording");
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let (name, thread) = stem.split_once("__").unwrap_or((stem.as_str(), "thread"));
        let thread = thread.to_owned();
        let outcome = std::panic::catch_unwind(|| {
            let (runtime, _) = run(&recording, &thread);
            let (view, rejects) = chat_of(&runtime);
            let stuck = view
                .open_items()
                .filter(|item| view.running().is_none() && !item.background)
                .map(|item| format!("{:?} {:?}", item.id, item.kind))
                .collect::<Vec<_>>();
            (
                view.turns.len(),
                view.phase(),
                runtime.turns.turn().is_some(),
                rejects,
                stuck,
            )
        });
        match outcome {
            Ok((turns, phase, driver_running, rejects, stuck)) => {
                eprintln!("== {name}: {turns} turns, {phase:?}, driver running {driver_running}");
                for reject in rejects {
                    eprintln!("   reject {reject}");
                }
                for item in stuck {
                    eprintln!("   stuck {item}");
                }
            }
            Err(_) => eprintln!("== {name}: PANIC"),
        }
    }
}

#[test]
fn a_child_thread_approval_resolved_elsewhere_closes_its_card() {
    let thread = "main-thread";
    let (mut runtime, receiver) = setup(thread);
    smol::block_on(runtime.message(serde_json::json!({
        "id": 77,
        "method": "item/commandExecution/requestApproval",
        "params": { "threadId": "child-thread", "turnId": "t", "itemId": "cmd", "command": "ls" },
    })))
    .expect("request");
    smol::block_on(runtime.message(serde_json::json!({
        "method": "serverRequest/resolved",
        "params": { "threadId": "child-thread", "requestId": 77 },
    })))
    .expect("resolved");
    let mut requested = None;
    let mut resolved = None;
    while let Ok(event) = receiver.try_recv() {
        match event {
            AgentStreamPayload::PermissionRequested { request_id, .. } => {
                requested = Some(request_id);
            }
            AgentStreamPayload::PermissionResolved { request_id, .. } => {
                resolved = Some(request_id);
            }
            _ => {}
        }
    }
    assert!(requested.is_some());
    assert_eq!(resolved, requested);
    assert!(runtime.permissions.is_empty());
    let (view, _) = chat_of(&runtime);
    assert!(view.open_requests().next().is_none());
}
