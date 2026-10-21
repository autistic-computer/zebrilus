use async_channel::Receiver;

use super::*;

const ORIGINAL: &str = "5ea128f9-c87f-42ce-898a-b86101954ace";

fn setup() -> (Runtime, Receiver<AgentStreamPayload>) {
    let command = ClaudeCommand::parse("claude").expect("command");
    let (events, receiver) = async_channel::unbounded();
    let (child_tx, _) = async_channel::unbounded();
    let mut runtime = Runtime {
        provider: AgentProvider::ClaudeCode,
        settings: Settings::new(&command),
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
        ready_sent: true,
        session: Some(Session {
            id: new_uuid(),
            cwd: PathBuf::from("/work"),
        }),
        starting: None,
        translator: Translator::new(PathBuf::from("/work")),
        requests: HashMap::new(),
        next_request: 0,
        permissions: HashMap::new(),
        turns: TurnState::default(),
        last_turn: 0,
        deferred_cancels: HashSet::new(),
        tasks: Vec::new(),
        verbs: 0,
        reply: None,
    };
    smol::block_on(runtime.apply(&Fact::SessionReady)).expect("ready");
    (runtime, receiver)
}

fn attach_sink(runtime: &mut Runtime) {
    runtime.process = Some(
        Process::launch(
            &mut Command::new("/bin/cat"),
            "replay sink",
            runtime.generation,
            &runtime.child_tx,
            &runtime.stderr,
        )
        .expect("sink"),
    );
}

fn prompt(runtime: &mut Runtime, turn: u64, command: &str) {
    let fact = Fact::Prompt {
        turn,
        command: Some(command.to_owned()),
    };
    assert_eq!(smol::block_on(runtime.apply(&fact)), Ok(None));
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

fn replay(recording: &str) -> Vec<(usize, Seen)> {
    let (mut runtime, receiver) = setup();
    let mut seen = Vec::new();
    let mut turn = 0;
    for (index, line) in recording.lines().enumerate() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        let message = &entry["msg"];
        if entry["dir"] == "in" && message["type"] == "user" {
            let command = message["uuid"].as_str().expect("uuid").to_owned();
            if message["priority"] == "now" {
                smol::block_on(runtime.apply(&Fact::Steer { command })).expect("steer");
            } else if runtime.turns.accepts_prompt() {
                turn += 1;
                prompt(&mut runtime, turn, &command);
            }
        } else if entry["dir"] == "out" && message.is_object() {
            smol::block_on(runtime.frame(message.clone())).expect("frame");
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
fn the_hello_recording_settles_once_on_its_lifecycle() {
    assert_eq!(
        replay(include_str!("fixtures/hello.ndjson")),
        [(23, end_turn(1))]
    );
}

#[test]
fn the_tools_recording_settles_once_on_its_lifecycle() {
    assert_eq!(
        replay(include_str!("fixtures/tools.ndjson")),
        [(175, end_turn(1))]
    );
}

#[test]
fn the_steer_recording_runs_one_turn_then_a_background_wake() {
    assert_eq!(
        replay(include_str!("fixtures/steer.ndjson")),
        [
            (142, end_turn(1)),
            (147, Seen::Busy(true)),
            (168, Seen::Busy(false)),
        ]
    );
}

#[test]
fn a_steer_keeps_the_turn_when_the_original_command_is_cancelled() {
    let (mut runtime, receiver) = setup();
    attach_sink(&mut runtime);
    prompt(&mut runtime, 1, ORIGINAL);
    smol::block_on(runtime.answer_verb("steer", "change of plan", "zz-command-1")).expect("steer");
    let cancelled = include_str!("fixtures/steer.ndjson")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|entry| {
            entry["msg"]["type"] == "command_lifecycle"
                && entry["msg"]["command_uuid"] == ORIGINAL
                && entry["msg"]["state"] == "cancelled"
        })
        .expect("recorded cancel");
    smol::block_on(runtime.frame(cancelled["msg"].clone())).expect("frame");
    assert_eq!(turn_events(&receiver), []);
    assert!(runtime.turns.turn().is_some());
}

#[test]
fn a_hard_failure_is_reported_as_failed_not_cancelled() {
    let (mut runtime, receiver) = setup();
    prompt(&mut runtime, 1, ORIGINAL);
    smol::block_on(runtime.frame(json!({
        "type": "result",
        "subtype": "error_during_execution",
        "is_error": true,
        "errors": ["model_error: request failed"],
        "user_message_uuid": ORIGINAL,
        "user_message_uuids": [ORIGINAL],
    })))
    .expect("result");
    smol::block_on(runtime.frame(json!({
        "type": "command_lifecycle",
        "command_uuid": ORIGINAL,
        "state": "cancelled",
    })))
    .expect("lifecycle");
    let seen = turn_events(&receiver);
    assert!(
        matches!(&seen[..], [Seen::Finished(1, AgentPromptOutcome::Failed { message })] if message.contains("model_error")),
        "{seen:?}"
    );
}

#[test]
fn a_late_result_from_a_cancelled_turn_cannot_fail_the_next_prompt() {
    let (mut runtime, receiver) = setup();
    attach_sink(&mut runtime);
    prompt(&mut runtime, 1, ORIGINAL);
    smol::block_on(runtime.control(RuntimeControl::Cancel {
        turn_id: 1,
        session_id: None,
    }))
    .expect("cancel");
    assert_eq!(turn_events(&receiver), []);
    smol::block_on(runtime.frame(json!({
        "type": "command_lifecycle",
        "command_uuid": ORIGINAL,
        "state": "cancelled",
    })))
    .expect("lifecycle");
    assert_eq!(turn_events(&receiver), [Seen::Finished(1, cancelled())]);
    prompt(&mut runtime, 2, "next");
    smol::block_on(runtime.frame(json!({
        "type": "result",
        "subtype": "error_during_execution",
        "is_error": true,
        "errors": ["old turn error"],
        "user_message_uuids": [ORIGINAL],
    })))
    .expect("late result");
    smol::block_on(runtime.frame(json!({
        "type": "command_lifecycle",
        "command_uuid": "next",
        "state": "completed",
    })))
    .expect("lifecycle");
    assert_eq!(turn_events(&receiver), [end_turn(2)]);
}

#[test]
fn a_task_registered_before_its_start_gets_its_launch_tool() {
    let (mut runtime, receiver) = setup();
    for line in include_str!("fixtures/steer.ndjson").lines().take(125) {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        if entry["dir"] == "out" && entry["msg"].is_object() {
            smol::block_on(runtime.frame(entry["msg"].clone())).expect("frame");
        }
    }
    let mut task = None;
    while let Ok(event) = receiver.try_recv() {
        if let AgentStreamPayload::TasksChanged { tasks } = event {
            task = tasks.into_iter().find(|task| task.id == "bq8vnxiga");
        }
    }
    assert_eq!(
        task.expect("task").tool_call_id.as_deref(),
        Some("toolu_011C2jqx3vLKCFQJnQo3JzMo")
    );
}
