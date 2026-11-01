use async_channel::Receiver;
use zz_protocol::chat::{Answer, ItemId, ItemStatus, QuestionAnswer, RequestClose, RequestId};

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

const LIVE_THREAD: &str = "01a1943b-e5c3-7841-880b-f6c0a9341ec8";
const LIVE_CALL: &str = "call_a04acfd0ee5f43b584adae85ea080afd";

fn feed_step(runtime: &mut Runtime, step: &str) {
    for line in include_str!("fixtures/app-server-live.jsonl").lines() {
        let entry: Value = serde_json::from_str(line).expect("recorded line");
        if entry["step"] == step && entry["dir"] == "in" && entry["msg"]["method"].is_string() {
            smol::block_on(runtime.message(entry["msg"].clone())).expect("message");
        }
    }
}

fn payloads(receiver: &Receiver<AgentStreamPayload>) -> Vec<AgentStreamPayload> {
    std::iter::from_fn(|| receiver.try_recv().ok()).collect()
}

fn asked_requests(payloads: &[AgentStreamPayload]) -> Vec<(u64, Value, Vec<AgentQuestion>)> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                questions,
                ..
            } => Some((*request_id, tool_call.clone(), questions.clone())),
            _ => None,
        })
        .collect()
}

fn answer_out(runtime: &Runtime) -> Vec<(String, bool)> {
    runtime
        .requests
        .values()
        .filter_map(|(outgoing, _)| match outgoing {
            Outgoing::Answer { text, steered } => Some((text.clone(), *steered)),
            _ => None,
        })
        .collect()
}

fn answer_card(runtime: &mut Runtime, request_id: u64, answer: &str) {
    smol::block_on(runtime.control(RuntimeControl::AnswerQuestion {
        request_id,
        answers: vec![AgentQuestionAnswer {
            id: "q0".to_owned(),
            answers: vec![answer.to_owned()],
        }],
    }))
    .expect("answer");
}

fn async_item(id: &str, title: &str) -> Value {
    json!({ "type": "agentMessage", "id": id, "text": format!("{title}\n- A\n- B"),
        "phase": "final_answer", "delivery": "async",
        "questions": [{ "title": title, "options": ["A", "B"] }] })
}

#[test]
fn the_recorded_async_question_becomes_a_card_and_its_answer_starts_a_turn() {
    let (mut runtime, receiver) = setup(LIVE_THREAD);
    feed_step(&mut runtime, "question");
    let seen = payloads(&receiver);
    assert!(
        !seen.iter().any(|payload| matches!(payload,
            AgentStreamPayload::Update { update }
                if update["content"]["text"] == "Which color?\n- Red\n- Blue")),
        "the fallback text was drawn as a message"
    );
    let asked = asked_requests(&seen);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let (request_id, tool_call, questions) = asked[0].clone();
    assert_eq!(
        tool_call,
        json!({ "toolCallId": LIVE_CALL, "title": "Which color?", "kind": "other",
            "_meta": { "zz": { "background": true } } })
    );
    let labels = questions[0]
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        (
            questions[0].question.as_str(),
            labels,
            questions[0].allow_other
        ),
        ("Which color?", vec!["Red", "Blue"], true)
    );
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    assert!(view.running().is_none());
    assert_eq!(view.asking().map(|request| request.id.0), Some(request_id));
    answer_card(&mut runtime, request_id, "Blue");
    assert_eq!(
        answer_out(&runtime),
        [(
            format!(
                "<send_user_message_question_reply>\n[{{\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"{LIVE_CALL}\\\",0]\",\"question\":\"Which color?\",\"answer\":\"Blue\"}}]\n</send_user_message_question_reply>\n"
            ),
            false,
        )]
    );
    feed_step(&mut runtime, "question-reply");
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    let request = view.request(RequestId(request_id)).expect("request");
    assert_eq!(request.close, Some(answered(&["Blue"])));
    assert_eq!(view.requests.len(), 1, "{:?}", view.requests);
    assert_eq!(
        view.item(&ItemId::new(LIVE_CALL))
            .map(|item| item.status.clone()),
        Some(ItemStatus::Completed)
    );
    assert!(
        !view
            .items
            .values()
            .any(|item| item.body.text.contains("send_user_message_question_reply")),
        "the reply showed as a message"
    );
    assert!(runtime.permissions.is_empty());
}

#[test]
fn an_answer_steers_a_running_turn_and_starts_one_if_the_steer_misses() {
    let thread = "thread-1";
    let (mut runtime, receiver) = setup(thread);
    for message in [
        json!({ "method": "turn/started", "params": { "threadId": thread, "turn": { "id": "t1" } } }),
        json!({ "method": "item/started", "params": { "threadId": thread, "item": async_item("call_q", "Which?") } }),
    ] {
        smol::block_on(runtime.message(message)).expect("message");
    }
    let (request_id, _, _) = asked_requests(&payloads(&receiver))[0].clone();
    answer_card(&mut runtime, request_id, "B");
    let steered = answer_out(&runtime);
    assert_eq!(steered.len(), 1);
    assert!(steered[0].1, "a running turn takes the answer as a steer");
    let rpc = runtime
        .requests
        .iter()
        .find(|(_, (outgoing, _))| matches!(outgoing, Outgoing::Answer { .. }))
        .map(|(id, _)| *id)
        .expect("steer request");
    smol::block_on(runtime.message(json!({ "id": rpc, "error": { "message": "no active turn" } })))
        .expect("steer refused");
    assert_eq!(answer_out(&runtime), [(steered[0].0.clone(), false)]);
}

#[test]
fn another_message_closes_an_open_card_unanswered() {
    let thread = "thread-1";
    let (mut runtime, receiver) = setup(thread);
    for message in [
        json!({ "method": "turn/started", "params": { "threadId": thread, "turn": { "id": "t1" } } }),
        json!({ "method": "item/started", "params": { "threadId": thread, "item": async_item("call_q", "Which?") } }),
        json!({ "method": "turn/completed", "params": { "threadId": thread, "turn": { "id": "t1", "status": "completed" } } }),
        json!({ "method": "turn/started", "params": { "threadId": thread, "turn": { "id": "t2" } } }),
        json!({ "method": "item/completed", "params": { "threadId": thread, "item": {
            "type": "userMessage", "id": "m2", "content": [{ "type": "text", "text": "skip that, go on" }] } } }),
    ] {
        smol::block_on(runtime.message(message)).expect("message");
    }
    let seen = payloads(&receiver);
    let (request_id, _, _) = asked_requests(&seen)[0].clone();
    assert!(seen.iter().any(|payload| matches!(payload,
        AgentStreamPayload::PermissionResolved { request_id: id, canceled: true } if *id == request_id)));
    assert!(runtime.permissions.is_empty());
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    let request = view.request(RequestId(request_id)).expect("request");
    assert_eq!(request.close, Some(answered(&[""])));
    assert!(view.asking().is_none());
}

fn answered(answers: &[&str]) -> RequestClose {
    RequestClose::Answered {
        answer: Answer::Questions {
            answers: answers
                .iter()
                .enumerate()
                .map(|(index, answer)| QuestionAnswer {
                    question: format!("q{index}"),
                    answers: (!answer.is_empty())
                        .then(|| (*answer).to_owned())
                        .into_iter()
                        .collect(),
                })
                .collect(),
        },
    }
}

#[test]
fn a_resumed_thread_shows_the_answered_question_and_reopens_the_open_one() {
    let thread = "thread-1";
    let (mut runtime, receiver) = setup(thread);
    let reply = translate::question_reply(
        "call_a",
        &translate::async_questions(&async_item("call_a", "First?")).expect("questions"),
        &[AgentQuestionAnswer {
            id: "q0".to_owned(),
            answers: vec!["A".to_owned()],
        }],
    );
    runtime.loading = Some(Loading {
        start: Start::Open,
        thread: json!({ "id": thread, "turns": [
            { "id": "t1", "items": [
                { "type": "userMessage", "id": "m1", "content": [{ "type": "text", "text": "ask me" }] },
                async_item("call_a", "First?"),
            ] },
            { "id": "t2", "items": [
                { "type": "userMessage", "id": "m2", "content": [{ "type": "text", "text": reply }] },
                async_item("call_b", "Second?"),
            ] },
        ] }),
        cwd: PathBuf::from("/work"),
        children: HashMap::new(),
    });
    smol::block_on(runtime.apply(&Fact::SessionClosed)).expect("closed");
    smol::block_on(runtime.load()).expect("load");
    let asked = asked_requests(&payloads(&receiver));
    assert_eq!(
        asked
            .iter()
            .map(|(_, tool_call, _)| tool_call["toolCallId"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["call_b"]
    );
    let (view, rejects) = chat_of(&runtime);
    assert!(rejects.is_empty(), "{rejects:#?}");
    let first = view
        .requests
        .iter()
        .find(|request| request.item == Some(ItemId::new("call_a")))
        .expect("first question");
    assert_eq!(first.close, Some(answered(&["A"])));
    let open = view.asking().expect("second question still open");
    assert_eq!(open.item, Some(ItemId::new("call_b")));
    assert_eq!(open.id.0, asked[0].0);
    assert!(
        !view.items.values().any(|item| item
            .body
            .text
            .contains("send_user_message_question_reply")
            || item.body.text.contains("- A")),
        "a reply or a fallback showed as a message"
    );
    answer_card(&mut runtime, asked[0].0, "B");
    assert_eq!(answer_out(&runtime).len(), 1);
}
