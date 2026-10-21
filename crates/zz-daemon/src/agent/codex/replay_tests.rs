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

fn replay(recording: &str, thread: &str) -> Vec<(usize, Seen)> {
    let (mut runtime, receiver) = setup(thread);
    let mut seen = Vec::new();
    let mut prompts = HashMap::new();
    for (index, line) in recording.lines().enumerate() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        let message = &entry["msg"];
        if entry["dir"] == "in" && message["method"] == "turn/start" {
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
    seen
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
