use async_channel::Receiver;
use zz_protocol::chat::{ItemId, ItemKind, ItemStatus};

use super::*;
use crate::agent::cut_points::{Tape, Taping};

const ORIGINAL: &str = "5ea128f9-c87f-42ce-898a-b86101954ace";

fn setup() -> (Runtime, Receiver<AgentStreamPayload>) {
    let (mut runtime, receiver) = fresh();
    smol::block_on(runtime.apply(&Fact::SessionReady)).expect("ready");
    (runtime, receiver)
}

fn fresh() -> (Runtime, Receiver<AgentStreamPayload>) {
    let command = ClaudeCommand::parse("claude").expect("command");
    let (events, receiver) = async_channel::unbounded();
    let (child_tx, _) = async_channel::unbounded();
    let runtime = Runtime {
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
        resumed_quiet: false,
        session: Some(Session {
            id: new_uuid(),
            cwd: PathBuf::from("/work"),
        }),
        starting: None,
        translator: Translator::new(PathBuf::from("/work")),
        requests: HashMap::new(),
        next_request: 0,
        permissions: HashMap::new(),
        background_asks: HashSet::new(),
        turns: TurnState::default(),
        chat: Mutex::new(Shadow::recording().marking_turns()),
        last_turn: 0,
        deferred_cancels: HashSet::new(),
        tasks: Vec::new(),
        parents: HashMap::new(),
        verbs: 0,
        reply: None,
        title: Title::default(),
        listed: HashMap::new(),
    };
    (runtime, receiver)
}

#[cfg(unix)]
fn attach_sink(runtime: &mut Runtime) {
    runtime.process = Some(
        Process::launch(
            &mut Command::new("/bin/cat"),
            AgentProvider::ClaudeCode,
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

fn run(recording: &str) -> (Runtime, Vec<(usize, Seen)>) {
    run_with(recording, false)
}

fn run_with(recording: &str, foreign: bool) -> (Runtime, Vec<(usize, Seen)>) {
    let (mut runtime, receiver) = setup();
    let mut seen = Vec::new();
    let mut turn = 0;
    for (index, line) in recording.lines().enumerate() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        run_line(&mut runtime, &entry, foreign, &mut turn);
        seen.extend(
            turn_events(&receiver)
                .into_iter()
                .map(|event| (index + 1, event)),
        );
    }
    (runtime, seen)
}

fn run_line(runtime: &mut Runtime, entry: &Value, foreign: bool, turn: &mut u64) {
    let message = &entry["msg"];
    if entry["dir"] == "in" && message["type"] == "user" {
        let command = message["uuid"].as_str().expect("uuid").to_owned();
        if message["priority"] == "now" {
            smol::block_on(runtime.apply(&Fact::Steer { command })).expect("steer");
        } else if runtime.turns.accepts_prompt() {
            *turn += 1;
            prompt(runtime, *turn, &command);
        }
    } else if foreign
        && entry["dir"] == "in"
        && message["type"] == "control_request"
        && message["request"]["subtype"] == "interrupt"
    {
        let _ = smol::block_on(runtime.apply(&Fact::Cancel));
    } else if entry["dir"] == "out" && message.is_object() {
        smol::block_on(runtime.frame(message.clone())).expect("frame");
    } else if foreign && entry["dir"] == "in" && message["type"] == "query.open" {
        let _ = smol::block_on(runtime.apply(&Fact::SessionReady));
    } else if entry["dir"] == "exit" {
        let _ = smol::block_on(runtime.apply(&Fact::Exited {
            message: "the recording ended the process".to_owned(),
        }));
    }
}

fn replay(recording: &str) -> Vec<(usize, Seen)> {
    run(recording).1
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

#[cfg(unix)]
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
fn a_refused_reply_finishes_its_turn_as_refused_and_a_steered_one_as_done() {
    let (mut runtime, receiver) = setup();
    let mut finish = |turn: u64, command: &str, stop_reason: &str| {
        prompt(&mut runtime, turn, command);
        for frame in [
            json!({
                "type": "result",
                "subtype": "success",
                "is_error": false,
                "num_turns": 1,
                "stop_reason": stop_reason,
                "user_message_uuids": [command],
            }),
            json!({ "type": "command_lifecycle", "command_uuid": command, "state": "completed" }),
        ] {
            smol::block_on(runtime.frame(frame)).expect("frame");
        }
        turn_events(&receiver)
    };
    assert_eq!(
        finish(1, "refused", "refusal"),
        [Seen::Finished(
            1,
            AgentPromptOutcome::Finished {
                stop_reason: Value::from("refusal"),
            },
        )]
    );
    assert_eq!(finish(2, "steered", "tool_use"), [end_turn(2)]);
}

#[cfg(unix)]
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

fn ask_bash(runtime: &mut Runtime, receiver: &Receiver<AgentStreamPayload>) -> u64 {
    smol::block_on(runtime.frame(json!({
        "type": "control_request",
        "request_id": "perm-1",
        "request": {
            "subtype": "can_use_tool",
            "tool_name": "Bash",
            "input": { "command": "rm -rf build" },
            "tool_use_id": "t1",
            "permission_suggestions": [],
        },
    })))
    .expect("permission");
    std::iter::from_fn(|| receiver.try_recv().ok())
        .find_map(|event| match event {
            AgentStreamPayload::PermissionRequested { request_id, .. } => Some(request_id),
            _ => None,
        })
        .expect("permission card")
}

fn resolved(receiver: &Receiver<AgentStreamPayload>) -> Vec<(u64, bool)> {
    std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|event| match event {
            AgentStreamPayload::PermissionResolved {
                request_id,
                canceled,
            } => Some((request_id, canceled)),
            _ => None,
        })
        .collect()
}

#[test]
fn question_answers_for_a_permission_card_leave_the_card_open() {
    let (mut runtime, receiver) = setup();
    let request = ask_bash(&mut runtime, &receiver);
    smol::block_on(runtime.answer_questions(request, &[])).expect("answers");
    assert_eq!(resolved(&receiver), []);
    let open = |runtime: &Runtime| {
        chat_of(runtime)
            .0
            .request(zz_protocol::chat::RequestId(request))
            .is_some_and(zz_protocol::chat::RequestView::is_open)
    };
    assert!(
        open(&runtime),
        "the chat closed a card the agent still waits on"
    );
    smol::block_on(runtime.answer(request, Some("allow".to_owned()))).expect("allow");
    assert_eq!(resolved(&receiver), [(request, false)]);
    assert!(!open(&runtime));
}

#[test]
fn an_exit_resolves_the_open_permission_cards() {
    let (mut runtime, receiver) = setup();
    let request = ask_bash(&mut runtime, &receiver);
    smol::block_on(runtime.closed()).expect("closed");
    assert_eq!(resolved(&receiver), [(request, true)]);
}

fn play(runtime: &mut Runtime, recording: &[Value]) {
    for entry in recording {
        play_line(runtime, entry);
    }
}

fn play_line(runtime: &mut Runtime, entry: &Value) {
    let message = &entry["msg"];
    if entry["dir"] == "in" && message["type"] == "user" {
        prompt(runtime, 1, message["uuid"].as_str().expect("uuid"));
    } else if entry["dir"] == "out" && message.is_object() {
        smol::block_on(runtime.frame(message.clone())).expect("frame");
    }
}

pub(crate) fn tapes() -> Vec<Tape> {
    let run = |name, recording| {
        let mut turn = 0;
        tape(name, recording, move |runtime, entry| {
            run_line(runtime, entry, false, &mut turn);
        })
    };
    let mut tapes = vec![
        run("claude hello", include_str!("fixtures/hello.ndjson")),
        run("claude tools", include_str!("fixtures/tools.ndjson")),
        run("claude steer", include_str!("fixtures/steer.ndjson")),
        run("claude resume", include_str!("fixtures/resume.ndjson")),
        tape(
            "claude background",
            include_str!("fixtures/background.ndjson"),
            play_line,
        ),
        tape(
            "claude nested",
            include_str!("fixtures/nested.ndjson"),
            play_line,
        ),
    ];
    #[cfg(unix)]
    for (file, recording) in NATIVE {
        tapes.extend(native(
            file,
            recording,
            &tempfile::tempdir().expect("journal directory"),
        ));
    }
    tapes
}

#[cfg(unix)]
const NATIVE: [(&str, &str); 4] = [
    (
        "claude-hello",
        include_str!("../fixtures/claude-hello.ndjson"),
    ),
    (
        "claude-tools",
        include_str!("../fixtures/claude-tools.ndjson"),
    ),
    (
        "claude-queue",
        include_str!("../fixtures/claude-queue.ndjson"),
    ),
    (
        "claude-reload",
        include_str!("../fixtures/claude-reload.ndjson"),
    ),
];

#[cfg(unix)]
fn segments(recording: &str) -> Vec<Vec<Value>> {
    let mut segments: Vec<Vec<Value>> = Vec::new();
    for line in recording.lines() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        if entry["dir"] == "meta" || segments.is_empty() {
            segments.push(Vec::new());
        }
        segments.last_mut().expect("segment").push(entry);
    }
    segments
}

#[cfg(unix)]
fn launched(meta: &Value) -> Launch {
    let args = meta["msg"]["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let after = |flag: &str| {
        args.iter()
            .position(|arg| *arg == flag)
            .and_then(|at| args.get(at + 1))
            .map(|id| (*id).to_owned())
    };
    after("--resume").map_or_else(
        || Launch::New(after("--session-id").expect("recorded session id")),
        Launch::Resume,
    )
}

#[cfg(unix)]
fn launch(launch: Launch, journal: &Arc<AgentJournal>) -> (Runtime, Receiver<AgentStreamPayload>) {
    let (mut runtime, receiver) = fresh();
    runtime.journal = Some(Arc::clone(journal));
    runtime.session = None;
    runtime.ready_sent = false;
    runtime.next_request = 1 << 32;
    attach_sink(&mut runtime);
    let resume = matches!(launch, Launch::Resume(_));
    runtime.resumed_quiet = resume;
    runtime.settings.asks = true;
    runtime.starting = Some(Starting {
        start: Start::Open { resume },
        launch,
        cwd: PathBuf::from("/work"),
    });
    (runtime, receiver)
}

#[cfg(unix)]
pub(crate) fn native(
    file: &'static str,
    recording: &str,
    directory: &tempfile::TempDir,
) -> Vec<Tape> {
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let segments = segments(recording);
    let count = segments.len();
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let name: &'static str = if count == 1 {
                file
            } else {
                Box::leak(format!("{file} {}/{count}", index + 1).into_boxed_str())
            };
            let launch_as = launched(&segment[0]);
            let session = launch_as.session_id().to_owned();
            let mut taping = Taping::sharing(
                name,
                AgentProvider::ClaudeCode,
                &session,
                Arc::clone(&journal),
            );
            let (mut runtime, receiver) = launch(launch_as, &journal);
            taping.line(None, drain(&receiver));
            let mut turn = 0;
            for entry in segment {
                drive(&mut runtime, entry, &session, &mut turn);
                taping.line(Some(entry), drain(&receiver));
            }
            let emitted = runtime.chat.lock().events.clone();
            taping.finish(&emitted, reload)
        })
        .collect()
}

#[cfg(unix)]
fn recorded_request(request: &Value, session: &str) -> Option<Outgoing> {
    Some(match request["subtype"].as_str()? {
        "initialize" => Outgoing::Initialize,
        "interrupt" => Outgoing::Interrupt,
        "get_settings" => Outgoing::Applied,
        "get_usage" => Outgoing::Usage,
        "generate_session_title" => Outgoing::Title {
            session: session.to_owned(),
        },
        "set_model" => Outgoing::Setting {
            option_id: "model".to_owned(),
            value: request["model"].as_str()?.to_owned(),
        },
        "set_permission_mode" => Outgoing::Mode {
            mode_id: request["mode"].as_str()?.to_owned(),
        },
        "stop_task" => Outgoing::StopTask,
        _ => return None,
    })
}

#[cfg(unix)]
fn asked(runtime: &Runtime, claude: &str) -> Option<u64> {
    runtime
        .permissions
        .iter()
        .find(|(_, permission)| {
            matches!(permission,
                Permission::Approval { claude_id, .. }
                | Permission::Plan { claude_id, .. }
                | Permission::Question { claude_id, .. } if claude_id == claude)
        })
        .map(|(request_id, _)| *request_id)
}

#[cfg(unix)]
fn answered_with(response: &Value) -> Option<String> {
    match response["behavior"].as_str() {
        Some("allow") if response.get("updatedPermissions").is_some() => {
            Some("allow_always".to_owned())
        }
        Some("allow") => Some("allow".to_owned()),
        _ if response["interrupt"] == true => None,
        _ => Some("reject".to_owned()),
    }
}

#[cfg(unix)]
fn drive(runtime: &mut Runtime, entry: &Value, session: &str, turn: &mut u64) {
    let message = &entry["msg"];
    match (entry["dir"].as_str(), message["type"].as_str()) {
        (Some("in"), Some("user")) if message["priority"] == "now" => {
            run_line(runtime, entry, false, turn);
        }
        (Some("in"), Some("user")) => {
            *turn += 1;
            let content = &message["message"]["content"];
            let text = content.as_str().map_or_else(
                || {
                    content
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|block| block["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                },
                str::to_owned,
            );
            let prompt = AgentPrompt {
                owner: zz_protocol::ClientInstanceId::default(),
                text,
                images: Vec::new(),
            };
            let uuid = message["uuid"].as_str().expect("recorded uuid").to_owned();
            smol::block_on(runtime.prompt(*turn, prompt, None, uuid)).expect("prompt");
        }
        (Some("in"), Some("control_request")) => {
            let request = &message["request"];
            if request["subtype"] == "interrupt" {
                smol::block_on(runtime.control(RuntimeControl::Cancel {
                    turn_id: *turn,
                    session_id: None,
                }))
                .expect("cancel");
            }
            if let (Some(id), Some(outgoing)) = (
                message["request_id"].as_str(),
                recorded_request(request, session),
            ) {
                runtime
                    .requests
                    .insert(id.to_owned(), (outgoing, Instant::now() + CONTROL_TIMEOUT));
            }
        }
        (Some("in"), Some("control_response")) => {
            let response = &message["response"];
            if let Some(request_id) = response["request_id"]
                .as_str()
                .and_then(|claude| asked(runtime, claude))
            {
                smol::block_on(runtime.control(RuntimeControl::RespondPermission {
                    request_id,
                    option_id: answered_with(&response["response"]),
                }))
                .expect("answer");
            }
        }
        (Some("out"), Some(_)) => {
            smol::block_on(runtime.frame(message.clone())).expect("frame");
        }
        _ => {}
    }
}

fn tape(name: &'static str, recording: &str, mut step: impl FnMut(&mut Runtime, &Value)) -> Tape {
    let session = recording
        .lines()
        .find_map(|line| {
            let entry = serde_json::from_str::<Value>(line).ok()?;
            entry["msg"]["session_id"]
                .as_str()
                .filter(|session| sessions::valid_uuid(session))
                .map(str::to_owned)
        })
        .expect("recorded session");
    let (mut runtime, receiver) = fresh();
    runtime.session = Some(Session {
        id: session.clone(),
        cwd: PathBuf::from("/work"),
    });
    let mut taping = Taping::new(name, AgentProvider::ClaudeCode, &session);
    runtime.journal = Some(taping.journal());
    smol::block_on(runtime.apply(&Fact::SessionReady)).expect("ready");
    taping.line(None, drain(&receiver));
    for line in recording.lines() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        step(&mut runtime, &entry);
        taping.line(Some(&entry), drain(&receiver));
    }
    let emitted = runtime.chat.lock().events.clone();
    taping.finish(&emitted, reload)
}

fn drain(receiver: &Receiver<AgentStreamPayload>) -> Vec<AgentStreamPayload> {
    std::iter::from_fn(|| receiver.try_recv().ok()).collect()
}

fn reload(_provider: AgentProvider, session: &str, journal: &[Value]) -> Vec<ChatEvent> {
    let directory = tempfile::tempdir().expect("journal directory");
    let held = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    for update in journal {
        held.append_for(AgentProvider::ClaudeCode, session, update)
            .expect("journal");
    }
    let (mut runtime, _receiver) = fresh();
    runtime.journal = Some(held);
    runtime.starting = Some(Starting {
        start: Start::Open { resume: true },
        launch: Launch::Resume(session.to_owned()),
        cwd: PathBuf::from("/work"),
    });
    smol::block_on(runtime.initialized(&json!({}))).expect("reload");
    runtime.chat.lock().events.clone()
}

fn background_recording() -> Vec<Value> {
    include_str!("fixtures/background.ndjson")
        .lines()
        .map(|line| serde_json::from_str(line).expect("recorded line"))
        .collect()
}

#[test]
fn an_ask_from_a_background_agent_or_one_it_started_outlives_the_turn_and_a_foreground_ask_does_not()
 {
    for (recording, expected) in [
        (
            include_str!("fixtures/background.ndjson"),
            vec![
                (json!("toolu_01LPpEBtdDfksG9zSnXyV14S"), false),
                (json!("toolu_01CMVtk5BeuLNryViYvdStZN"), true),
            ],
        ),
        (
            include_str!("fixtures/nested.ndjson"),
            vec![(json!("toolu_013isakmv9aNq6LLL2rSWRDX"), true)],
        ),
    ] {
        let (mut runtime, receiver) = setup();
        let recording = recording
            .lines()
            .map(|line| serde_json::from_str(line).expect("recorded line"))
            .collect::<Vec<Value>>();
        play(&mut runtime, &recording);
        let asks = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|event| match event {
                AgentStreamPayload::PermissionRequested {
                    request_id,
                    tool_call,
                    options,
                    questions,
                } => Some(crate::agent::host::AgentPendingPermission {
                    request_id,
                    tool_call,
                    options,
                    questions,
                }),
                _ => None,
            })
            .map(|ask| (ask.tool_call["toolCallId"].clone(), ask.outlives_turn()))
            .collect::<Vec<_>>();
        assert_eq!(asks, expected);
    }
}

#[test]
fn stop_answers_the_turn_s_ask_and_leaves_a_background_agent_s_ask_open() {
    let (mut runtime, receiver) = setup();
    let recording = background_recording();
    let at = |dir: &str, subtype: &str| {
        recording
            .iter()
            .position(|entry| {
                entry["dir"] == dir
                    && (entry["msg"]["request"]["subtype"] == subtype
                        || entry["msg"]["response"]["response"]["behavior"] == subtype)
            })
            .expect("recorded step")
    };
    let (interrupt, answer) = (at("in", "interrupt"), at("in", "allow"));
    play(&mut runtime, &recording[..interrupt]);
    let asks = std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|event| match event {
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                ..
            } => Some((tool_call["toolCallId"].as_str()?.to_owned(), request_id)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let foreground = asks["toolu_01LPpEBtdDfksG9zSnXyV14S"];
    let background = asks["toolu_01CMVtk5BeuLNryViYvdStZN"];
    smol::block_on(runtime.control(RuntimeControl::Cancel {
        turn_id: 1,
        session_id: None,
    }))
    .expect("stop");
    assert_eq!(resolved(&receiver), [(foreground, true)]);
    play(&mut runtime, &recording[interrupt..answer]);
    assert_eq!(turn_events(&receiver), [Seen::Finished(1, cancelled())]);
    smol::block_on(runtime.answer(background, Some("allow".to_owned()))).expect("allow");
    assert_eq!(resolved(&receiver), [(background, false)]);
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
fn tool_rows_read_as_plain_steps_with_their_failures() {
    let (mut runtime, receiver) = setup();
    prompt(&mut runtime, 1, ORIGINAL);
    let call = |id: &str, name: &str, input: Value| {
        json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":format!("m-{id}"),
            "content":[{"type":"tool_use","id":id,"name":name,"input":input}]}})
    };
    let result = |id: &str, content: &str, error: bool| {
        json!({"type":"user","parent_tool_use_id":null,"message":{"role":"user",
            "content":[{"type":"tool_result","tool_use_id":id,"content":content,"is_error":error}]}})
    };
    let frames = [
        json!({"type":"command_lifecycle","command_uuid":ORIGINAL,"state":"started"}),
        call(
            "t1",
            "ToolSearch",
            json!({"query":"select:EnterPlanMode,ExitPlanMode","max_results":2}),
        ),
        result("t1", "", false),
        call("t2", "EnterPlanMode", json!({})),
        result("t2", "Entered plan mode.", false),
        call(
            "t3",
            "Write",
            json!({"file_path":"/elsewhere/plan.md","content":"# Plan"}),
        ),
        result("t3", "File created successfully", false),
        call(
            "t4",
            "Bash",
            json!({"command":"cargo build","description":"Build the crate"}),
        ),
        result(
            "t4",
            "Exit code 101\nerror: could not find `Cargo.toml` in `/work` or any parent directory",
            true,
        ),
        call("t5", "Read", json!({"file_path":"/work/src/missing.rs"})),
        result(
            "t5",
            "<tool_use_error>File does not exist. Note: your current working directory is /work.</tool_use_error>",
            true,
        ),
        call(
            "t6",
            "WebSearch",
            json!({"query":"Rust edition 2024 changes"}),
        ),
        result("t6", "Links: []", false),
        call(
            "t7",
            "WebFetch",
            json!({"url":"https://example.com/notes","prompt":"Summarize"}),
        ),
        result("t7", "A short page.", false),
        call(
            "t8",
            "Bash",
            json!({"command":"sleep 90","description":"Sleep","run_in_background":true}),
        ),
        json!({"type":"system","subtype":"task_started","task_id":"bg1","tool_use_id":"t8",
            "is_backgrounded":true,"task_type":"local_bash","description":"Sleep"}),
        result("t8", "Command running in background with ID: bg1.", false),
        call("t9", "TaskStop", json!({"task_id":"bg1"})),
        result("t9", "Successfully stopped task: bg1", false),
        json!({"type":"system","subtype":"task_notification","task_id":"bg1","tool_use_id":"t8",
            "status":"failed","summary":"Background command \"Sleep\" failed with exit code 144"}),
        call("t10", "ExitPlanMode", json!({"plan":"# Plan"})),
        json!({"type":"control_request","request_id":"perm-plan","request":{
            "subtype":"can_use_tool","tool_name":"ExitPlanMode","input":{"plan":"# Plan"},
            "tool_use_id":"t10"}}),
    ];
    for frame in frames {
        smol::block_on(runtime.frame(frame)).expect("frame");
    }
    let plan = std::iter::from_fn(|| receiver.try_recv().ok())
        .find_map(|event| match event {
            AgentStreamPayload::PermissionRequested { request_id, .. } => Some(request_id),
            _ => None,
        })
        .expect("plan card");
    smol::block_on(runtime.answer(plan, Some("keep_planning".to_owned()))).expect("answer");
    smol::block_on(runtime.frame(result("t10", "The user wants to keep planning.", true)))
        .expect("frame");
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    let failed = |message: &str| ItemStatus::Failed {
        message: Some(message.to_owned()),
    };
    let rows = ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10"].map(|id| {
        let item = view.item(&ItemId::new(id)).expect(id);
        (
            item.kind,
            item.body.title.clone(),
            item.body.target.clone(),
            item.body.activity.clone(),
            item.body.exit_code,
            item.status.clone(),
        )
    });
    let row = |kind, title: &str, target: Option<&str>, activity: Option<&str>, code, status| {
        (
            kind,
            title.to_owned(),
            target.map(str::to_owned),
            activity.map(str::to_owned),
            code,
            status,
        )
    };
    assert_eq!(
        rows,
        [
            row(
                ItemKind::Tool,
                "Load tools",
                Some("EnterPlanMode, ExitPlanMode"),
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Tool,
                "Enter plan mode",
                None,
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Edit,
                "Write /elsewhere/plan.md",
                Some("/elsewhere/plan.md"),
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Execute,
                "cargo build",
                Some("cargo build"),
                Some("Build the crate"),
                Some(101),
                failed("error: could not find `Cargo.toml` in `/work` or any parent directory"),
            ),
            row(
                ItemKind::Read,
                "Read src/missing.rs",
                Some("src/missing.rs"),
                None,
                None,
                failed("File does not exist. Note: your current working directory is /work."),
            ),
            row(
                ItemKind::Web,
                "Search \"Rust edition 2024 changes\"",
                Some("Rust edition 2024 changes"),
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Fetch,
                "Fetch https://example.com/notes",
                Some("https://example.com/notes"),
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Execute,
                "sleep 90",
                Some("sleep 90"),
                Some("Sleep"),
                Some(144),
                failed("Background command \"Sleep\" failed with exit code 144"),
            ),
            row(
                ItemKind::Tool,
                "Stop sleep 90",
                None,
                None,
                None,
                ItemStatus::Completed,
            ),
            row(
                ItemKind::Tool,
                "Exit plan mode",
                None,
                None,
                None,
                ItemStatus::Declined,
            ),
        ]
    );
}

#[test]
fn a_send_message_that_resumes_an_agent_runs_as_its_card_until_the_notification() {
    let recording = include_str!("fixtures/resume.ndjson");
    let original = ItemId::new("toolu_01BdLYjCUyvH5UtK2E9y6v31");
    let resume = ItemId::new("toolu_01PabzHgZypSdK6PirEx1vpB");
    let handback = ItemId::new("toolu_01HAcDaX3g4UMG6J4Tf4nCoj");
    let card = |lines: usize, card: &ItemId| {
        let prefix = recording.lines().take(lines).collect::<Vec<_>>().join("\n");
        let (runtime, _) = run(&prefix);
        let (view, rejects) = chat_of(&runtime);
        let dropped = rejects
            .iter()
            .filter(|reject| !reject.starts_with("Turn("))
            .collect::<Vec<_>>();
        assert!(dropped.is_empty(), "{lines}: {dropped:#?}");
        let item = view.item(card).expect("card").clone();
        (
            (
                item.kind,
                item.body.title,
                item.body.activity,
                item.body.target,
            ),
            (item.body.text, item.status, item.background, item.children),
        )
    };
    let shown = (
        ItemKind::Subagent,
        "pong check".to_owned(),
        Some("pong again".to_owned()),
        Some("Reply with exactly: pong again".to_owned()),
    );
    let after_result = card(114, &resume);
    assert_eq!(after_result.0, shown);
    assert_eq!(
        after_result.1,
        (String::new(), ItemStatus::Running, true, Vec::new())
    );
    assert_eq!(
        card(132, &resume).1,
        (
            String::new(),
            ItemStatus::Running,
            true,
            vec![handback.clone()]
        )
    );
    let (_, (text, status, _, children)) = card(149, &resume);
    assert_eq!(
        (text.as_str(), status, children),
        ("pong again", ItemStatus::Completed, vec![handback])
    );
    let (_, (text, status, ..)) = card(149, &original);
    assert_eq!((text.as_str(), status), ("pong", ItemStatus::Completed));
}

#[test]
fn a_send_message_to_an_unknown_recipient_stays_a_plain_row() {
    let (mut runtime, _receiver) = setup();
    prompt(&mut runtime, 1, ORIGINAL);
    let frames = [
        json!({"type":"command_lifecycle","command_uuid":ORIGINAL,"state":"started"}),
        json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"m1","content":[
            {"type":"tool_use","id":"s1","name":"SendMessage",
                "input":{"to":"reviewer","summary":"ping","message":"ping"}}]}}),
        json!({"type":"user","parent_tool_use_id":null,"message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"s1","content":"Message sent to reviewer"}]},
            "tool_use_result":{"success":true,"message":"Message sent to reviewer"}}),
    ];
    for frame in frames {
        smol::block_on(runtime.frame(frame)).expect("frame");
    }
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    let item = view.item(&ItemId::new("s1")).expect("row");
    assert_eq!(
        (
            item.kind,
            item.body.title.clone(),
            item.body.activity.clone(),
            item.body.text.clone(),
            item.status.clone(),
        ),
        (
            ItemKind::Tool,
            "Send message".to_owned(),
            None,
            "Message sent to reviewer".to_owned(),
            ItemStatus::Completed,
        )
    );
}

#[test]
fn every_recording_drives_the_chat_machine() {
    for (name, recording) in [
        ("hello", include_str!("fixtures/hello.ndjson")),
        ("steer", include_str!("fixtures/steer.ndjson")),
        ("tools", include_str!("fixtures/tools.ndjson")),
    ] {
        let (runtime, _) = run(recording);
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
#[ignore = "replays every recording in $ZZ_REPLAY_DIR"]
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
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let outcome = std::panic::catch_unwind(|| {
            let (runtime, _) = run_with(&recording, true);
            if let Ok(watch) = std::env::var("ZZ_REPLAY_ITEM") {
                for event in &runtime.chat.lock().events {
                    let line = format!("{event:?}");
                    if line.contains(&watch) || line.contains("Turn") {
                        eprintln!("   event {}", &line[..line.len().min(200)]);
                    }
                }
            }
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

#[cfg(unix)]
const TRANSCRIPTS: [&str; 4] = [
    include_str!("../fixtures/claude-hello.transcript.jsonl"),
    include_str!("../fixtures/claude-tools.transcript.jsonl"),
    include_str!("../fixtures/claude-queue.transcript.jsonl"),
    include_str!("../fixtures/claude-reload.transcript.jsonl"),
];

#[cfg(unix)]
fn reloaded_from(config: &Path, session: &str, transcript: &str) -> zz_protocol::chat::ChatView {
    let folder = config.join("projects").join("-work");
    std::fs::create_dir_all(&folder).expect("project folder");
    std::fs::write(folder.join(format!("{session}.jsonl")), transcript).expect("transcript");
    let (mut runtime, _receiver) = fresh();
    runtime.session = None;
    runtime.starting = Some(Starting {
        start: Start::Open { resume: true },
        launch: Launch::Resume(session.to_owned()),
        cwd: PathBuf::from("/work"),
    });
    smol::block_on(runtime.initialized(&json!({}))).expect("reload");
    let events = runtime.chat.lock().events.clone();
    crate::agent::cut_points::fold(&events)
}

#[cfg(unix)]
#[test]
fn a_session_reloads_from_its_own_transcript_at_every_line() {
    use crate::agent::cut_points::{
        Findings, fold, partial_reload_difference, resume_difference, resumed_view,
    };

    const CHILD: &str = "ZZ_TEST_CLAUDE_TRANSCRIPTS";
    let Some(config) = std::env::var_os(CHILD).map(PathBuf::from) else {
        let config = tempfile::tempdir().expect("config directory");
        let status = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "agent::claude::replay_tests::a_session_reloads_from_its_own_transcript_at_every_line",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, config.path())
            .env("CLAUDE_CONFIG_DIR", config.path())
            .status()
            .expect("isolated Claude config directory");
        assert!(status.success());
        return;
    };
    let mut transcripts = Findings::new("T transcript reload");
    let mut reloads = Findings::new("R resume from the transcript");
    for ((file, recording), transcript) in NATIVE.iter().zip(TRANSCRIPTS) {
        let session = launched(&segments(recording)[0][0]).session_id().to_owned();
        reloaded_from(&config, &session, transcript);
        let directory = tempfile::tempdir().expect("journal directory");
        let tapes = native(file, recording, &directory);
        let live = fold(&tapes[0].events());
        let lines = transcript.lines().collect::<Vec<_>>();
        for k in 0..=lines.len() {
            let mut prefix = lines[..k].join("\n");
            prefix.push('\n');
            let reloaded = reloaded_from(&config, &session, &prefix);
            transcripts.checked();
            let full = k == lines.len();
            let difference = if full {
                resume_difference(&live, &reloaded)
                    .or_else(|| partial_reload_difference(&live, &reloaded))
            } else {
                partial_reload_difference(&live, &reloaded)
            };
            if let Some(diff) = difference {
                let scope = if full { "full" } else { "prefix" };
                transcripts.add(
                    format!("T | {scope} | {}", diff.field),
                    (*file).to_owned(),
                    (lines.len(), k),
                    || {
                        format!(
                            "{file} transcript lines {k}/{}: {}",
                            lines.len(),
                            diff.describe()
                        )
                    },
                );
            }
        }
        reloaded_from(&config, &session, transcript);
        if let [first, second] = &tapes[..] {
            let (loaded, resumed) = resumed_view(second);
            reloads.checked();
            if let Some(diff) = resume_difference(&fold(&first.events()), &resumed) {
                reloads.add(
                    format!("R | {}", diff.field),
                    second.name.to_owned(),
                    (second.lines.len(), loaded),
                    || format!("{} line {loaded}: {}", second.name, diff.describe()),
                );
            }
        }
    }
    transcripts.finish(&[]);
    reloads.finish(&[]);
}
