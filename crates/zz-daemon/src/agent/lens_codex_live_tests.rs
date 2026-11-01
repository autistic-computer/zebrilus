use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::{
    AgentProvider, AgentQuestionAnswer, PaneId,
    agent_stream::AgentImage,
    chat::{Answer, ChatView, ItemId, ItemKind, ItemStatus, QuestionAnswer, RequestClose},
};

use super::{super::*, Connection, embedded};
use crate::agent::codex::control::Peer;

const THREAD: &str = "01a1943b-e5c3-7841-880b-f6c0a9341ec8";
const QUESTION: &str = "call_a04acfd0ee5f43b584adae85ea080afd";
const APPROVED: &str = "exec-8304a739-393a-4bf2-9048-97053c7db43f";
const ELSEWHERE: &str = "exec-75c49d1e-6c84-41d2-960e-130500bc5a4e";
const SLEEP_TURN: &str = "01a1943f-d7fe-7a52-8ac0-2d6333888c1d";
const MODELS: &str = r#"{"models":[
  {"slug":"gpt-6-luna","display_name":"GPT-6-Luna","visibility":"list","priority":1,
   "default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"}]},
  {"slug":"gpt-5.6-luna","display_name":"GPT-5.6-Luna","visibility":"list","priority":2,
   "default_reasoning_level":"medium","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"}]}
]}"#;

#[derive(Default)]
struct Pane {
    typed: Mutex<Vec<String>>,
}

impl LensPane for Pane {
    fn pid(&self) -> Option<u32> {
        None
    }

    fn submit(&self, text: &str) -> Result<(), String> {
        self.typed.lock().push(text.to_owned());
        Ok(())
    }

    fn keys(&self, keys: &[&str]) -> Result<(), String> {
        self.typed
            .lock()
            .extend(keys.iter().map(|key| (*key).to_owned()));
        Ok(())
    }

    fn attach(&self, _images: &[AgentImage]) -> Result<(), String> {
        Ok(())
    }

    fn screen(&self) -> Option<String> {
        None
    }
}

fn recorded() -> Vec<Value> {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/agent/codex/fixtures/app-server-live.jsonl"),
    )
    .expect("fixture")
    .lines()
    .map(|line| serde_json::from_str::<Value>(line).expect("frame"))
    .collect()
}

fn frames(step: &str) -> Vec<Value> {
    recorded()
        .into_iter()
        .filter(|frame| frame["step"] == step && frame["dir"] == "in")
        .map(|frame| frame["msg"].clone())
        .filter(|message| message.get("method").is_some())
        .collect()
}

fn resume() -> Value {
    recorded()
        .into_iter()
        .find(|frame| frame["step"] == "resume")
        .expect("resume frame")["msg"]["result"]
        .clone()
}

fn rollout_line(kind: &str, payload: &Value) -> Value {
    json!({ "timestamp": "2026-01-01T00:00:00.000Z", "type": kind, "payload": payload })
}

fn item_line(item: &Value) -> Value {
    rollout_line(
        "event_msg",
        &json!({ "type": "item_completed", "item": item }),
    )
}

fn server(loaded: &[&str]) -> (Connection, Peer) {
    let (connection, peer) = Connection::pair();
    peer.deliver(json!({ "id": 0, "result": { "userAgent": "codex" } }));
    peer.deliver(json!({ "id": 1, "result": { "data": loaded, "nextCursor": null } }));
    peer.deliver(json!({ "id": 2, "result": resume() }));
    (connection, peer)
}

struct Harness {
    lens: Lens,
    peer: Peer,
    pane: Arc<Pane>,
    opened: Vec<AgentStreamPayload>,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn open(history: &[Value], loaded: &[&str]) -> Self {
        Self::beside(history, loaded, &[])
    }

    fn beside(history: &[Value], loaded: &[&str], others: &[u64]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(format!("rollout-2026-10-31T20-11-55-{THREAD}.jsonl"));
        let meta = rollout_line(
            "session_meta",
            &json!({ "id": THREAD, "cwd": "/work/proj" }),
        );
        let text = std::iter::once(&meta)
            .chain(history)
            .fold(String::new(), |all, line| all + &line.to_string() + "\n");
        std::fs::write(&path, text).unwrap();
        let (connection, peer) = server(loaded);
        let mut codex = Codex::default();
        codex.link.wire = Some(connection);
        codex.link.neighbors = Some(others.to_vec());
        codex.picks.catalog = Catalog::codex_json(MODELS);
        let pane = Arc::new(Pane::default());
        let mut lens = Lens::with_source(
            LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::Codex,
                io: pane.clone(),
            },
            Source::Codex(Box::new(codex)),
            Arc::new(AtomicU64::new(1)),
        );
        let mut opened = lens.reload(
            Found {
                session: THREAD.to_owned(),
                path,
                cwd: PathBuf::from("/work/proj"),
            },
            true,
        );
        opened.extend(lens.observe(false, Vec::new()));
        lens.next_locate = Instant::now() + Duration::from_mins(1);
        Self {
            lens,
            peer,
            pane,
            opened,
            _dir: dir,
        }
    }

    fn joined() -> Self {
        Self::open(&[], &[THREAD])
    }

    fn live(&self) -> bool {
        matches!(&self.lens.source, Source::Codex(codex) if codex.is_live())
    }

    fn sent(&self) -> Vec<Value> {
        self.peer
            .outgoing
            .try_iter()
            .map(|text| serde_json::from_str(&text).unwrap())
            .collect()
    }

    fn drain(&mut self) -> Vec<AgentStreamPayload> {
        let mut payloads = Vec::new();
        loop {
            let tick = self.lens.tick();
            if tick.is_empty() && self.peer.incoming.is_empty() {
                return payloads;
            }
            payloads.extend(tick);
        }
    }

    fn play(&mut self, messages: &[Value]) -> Vec<AgentStreamPayload> {
        for message in messages {
            self.peer.deliver(message.clone());
        }
        self.drain()
    }

    fn respond(&mut self, request: &Value, result: &Value) -> Vec<AgentStreamPayload> {
        self.play(&[json!({ "id": request["id"], "result": result })])
    }
}

fn view(rounds: &[&[AgentStreamPayload]]) -> (Shadow, ChatView) {
    let mut shadow = Shadow::recording().codex().attached();
    for payloads in rounds {
        shadow_lens(&mut shadow, payloads);
    }
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event)
            .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
    }
    (shadow, view)
}

fn texts(view: &ChatView, kind: ItemKind) -> Vec<String> {
    let mut items = view
        .items
        .values()
        .filter(|item| item.kind == kind)
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.started_at);
    items.iter().map(|item| item.body.text.clone()).collect()
}

fn requested(payloads: &[AgentStreamPayload]) -> Vec<(u64, &Value, &Value)> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                ..
            } => Some((*request_id, tool_call, options)),
            _ => None,
        })
        .collect()
}

fn resolved(payloads: &[AgentStreamPayload]) -> Vec<u64> {
    payloads
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::PermissionResolved { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .collect()
}

fn named<'a>(sent: &'a [Value], method: &str) -> Vec<&'a Value> {
    sent.iter()
        .filter(|message| message["method"] == method)
        .collect()
}

fn until<'a>(messages: &'a [Value], method: &str) -> (&'a [Value], &'a [Value]) {
    let at = messages
        .iter()
        .position(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("no {method}"));
    messages.split_at(at + 1)
}

#[test]
fn the_lens_joins_the_tuis_thread_and_streams_a_turn_typed_in_the_terminal() {
    let history = [
        rollout_line(
            "event_msg",
            &json!({ "type": "task_started", "turn_id": "t0" }),
        ),
        item_line(&json!({ "type": "UserMessage", "id": "u0",
            "content": [{ "type": "text", "text": "Reply with exactly the word: pong" }] })),
        item_line(&json!({ "type": "AgentMessage", "id": "a0",
            "content": [{ "type": "Text", "text": "pong" }] })),
        rollout_line(
            "event_msg",
            &json!({ "type": "task_complete", "turn_id": "t0" }),
        ),
    ];
    let mut live = Harness::open(&history, &[THREAD]);
    assert!(live.live());
    let sent = live.sent();
    let methods = sent
        .iter()
        .map(|message| message["method"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "thread/loaded/list",
            "thread/resume"
        ]
    );
    assert_eq!(sent[0]["params"]["capabilities"]["experimentalApi"], true);
    assert_eq!(
        sent[3]["params"],
        json!({ "threadId": THREAD, "excludeTurns": true })
    );

    let turn = live.play(&frames("tui-turn"));
    let chunks = turn
        .iter()
        .filter(|payload| matches!(payload, AgentStreamPayload::Update { update } if update["sessionUpdate"] == "agent_message_chunk"))
        .count();
    assert_eq!(chunks, 9, "every delta streams on its own");
    assert!(
        live.pane.typed.lock().is_empty(),
        "nothing goes through the terminal"
    );
    let (shadow, view) = view(&[&live.opened, &turn]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        texts(&view, ItemKind::User),
        [
            "Reply with exactly the word: pong",
            "Count from 1 to 5, one number per line, nothing else."
        ]
    );
    assert_eq!(texts(&view, ItemKind::Agent), ["pong", "1\n2\n3\n4\n5"]);
    assert!(view.running().is_none());
    assert_eq!(view.usage.map(|usage| usage.size), Some(258_400));
}

#[test]
fn items_already_in_the_rollout_are_not_shown_twice() {
    let tui = frames("tui-turn");
    let user = tui
        .iter()
        .find(|message| message["method"] == "item/completed")
        .unwrap()["params"]["item"]
        .clone();
    let history = [item_line(&json!({
        "type": "UserMessage",
        "id": user["id"],
        "client_id": user["clientId"],
        "content": user["content"],
    }))];
    let mut live = Harness::open(&history, &[THREAD]);
    let turn = live.play(&tui);
    let (shadow, view) = view(&[&live.opened, &turn]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        texts(&view, ItemKind::User),
        ["Count from 1 to 5, one number per line, nothing else."]
    );
}

#[test]
fn a_lens_prompt_starts_a_turn_over_the_app_server() {
    let mut live = Harness::joined();
    live.sent();
    let payloads = live.lens.command(RuntimeCommand::Prompt {
        turn_id: 9,
        prompt: AgentPrompt {
            text: "Reply with exactly: hello from the side channel".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(payloads.is_empty());
    let sent = live.sent();
    let [start] = named(&sent, "turn/start")[..] else {
        panic!("one turn/start in {sent:?}");
    };
    assert_eq!(start["params"]["threadId"], THREAD);
    assert_eq!(
        start["params"]["input"],
        json!([{ "type": "text", "text": "Reply with exactly: hello from the side channel", "text_elements": [] }])
    );
    assert!(live.pane.typed.lock().is_empty());
    let mut done = live.respond(
        start,
        &json!({ "turn": { "id": "01a1943d-b799-77a0-a087-385bb2870a47" } }),
    );
    done.extend(live.play(&frames("lens-turn")));
    assert!(
        done.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished {
                turn_id: 9,
                outcome: AgentPromptOutcome::Finished { .. }
            }
        )),
        "{done:?}"
    );
    let (_, view) = view(&[&live.opened, &done]);
    assert_eq!(
        texts(&view, ItemKind::Agent),
        ["hello from the side channel"]
    );
}

#[test]
fn a_refused_turn_start_fails_the_lens_prompt() {
    let mut live = Harness::joined();
    live.sent();
    live.lens.command(RuntimeCommand::Prompt {
        turn_id: 3,
        prompt: AgentPrompt {
            text: "hi".to_owned(),
            ..AgentPrompt::default()
        },
    });
    let sent = live.sent();
    let start = named(&sent, "turn/start")[0];
    let done = live.play(&[
        json!({ "id": start["id"], "error": { "code": -32600, "message": "model not allowed" } }),
    ]);
    assert!(done.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 3, outcome: AgentPromptOutcome::Failed { message } }
            if message == "model not allowed"
    )), "{done:?}");
}

#[test]
fn slash_commands_still_go_to_the_terminal() {
    let mut live = Harness::joined();
    live.sent();
    live.lens.command(RuntimeCommand::Prompt {
        turn_id: 4,
        prompt: AgentPrompt {
            text: "/review".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert_eq!(*live.pane.typed.lock(), ["/review"]);
    assert!(named(&live.sent(), "turn/start").is_empty());
}

#[test]
fn an_approval_answered_in_the_lens_replies_with_the_decision_it_offered() {
    let mut live = Harness::joined();
    live.sent();
    let frames = frames("approval");
    let (asked, rest) = frames.split_at(1);
    let card = live.play(asked);
    let [(request_id, tool_call, options)] = requested(&card)[..] else {
        panic!("one card in {card:?}");
    };
    assert_eq!(tool_call["toolCallId"], APPROVED);
    assert_eq!(
        tool_call["title"],
        "/opt/homebrew/bin/bash -lc 'touch approved.txt'"
    );
    let names = options
        .as_array()
        .unwrap()
        .iter()
        .map(|option| option["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["Allow", "Always allow `touch approved.txt`", "Reject"]
    );
    let answered = live.lens.control(RuntimeControl::RespondPermission {
        request_id,
        option_id: Some("decision-0".to_owned()),
    });
    assert_eq!(resolved(&answered), [request_id]);
    let sent = live.sent();
    assert_eq!(
        sent,
        [json!({ "id": 0, "result": { "decision": "accept" } })]
    );
    let after = live.play(rest);
    assert!(resolved(&after).is_empty(), "{after:?}");
    let (shadow, view) = view(&[&live.opened, &card, &answered, &after]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert!(view.open_requests().next().is_none());
    assert_eq!(
        view.item(&ItemId::new(APPROVED)).map(|item| &item.status),
        Some(&ItemStatus::Completed)
    );
}

#[test]
fn an_approval_answered_in_the_terminal_clears_the_lens_card() {
    let mut live = Harness::joined();
    live.sent();
    let frames = frames("approval-elsewhere");
    let (asked, rest) = until(&frames, "item/commandExecution/requestApproval");
    let card = live.play(asked);
    let [(request_id, tool_call, _)] = requested(&card)[..] else {
        panic!("one card in {card:?}");
    };
    assert_eq!(tool_call["toolCallId"], ELSEWHERE);
    let after = live.play(rest);
    assert_eq!(resolved(&after), [request_id]);
    assert!(
        live.sent()
            .iter()
            .all(|message| message.get("result").is_none()),
        "zz never answers for the user"
    );
    let (shadow, view) = view(&[&live.opened, &card, &after]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert!(view.open_requests().next().is_none());
}

#[test]
fn an_async_question_is_answered_from_the_lens_with_a_reply_message() {
    let mut live = Harness::joined();
    live.sent();
    let asked = live.play(&frames("question"));
    let [(request_id, tool_call, options)] = requested(&asked)[..] else {
        panic!("one card in {asked:?}");
    };
    assert_eq!(tool_call["toolCallId"], QUESTION);
    assert_eq!(tool_call["title"], "Which color?");
    assert_eq!(options.as_array().map(Vec::len), Some(2));
    assert_eq!(
        live.lens.source.status().and_then(|status| status.waiting),
        None
    );
    let answered = live.lens.control(RuntimeControl::AnswerQuestion {
        request_id,
        answers: vec![AgentQuestionAnswer {
            id: "q0".to_owned(),
            answers: vec!["Blue".to_owned()],
        }],
    });
    assert_eq!(resolved(&answered), [request_id]);
    let sent = live.sent();
    let [start] = named(&sent, "turn/start")[..] else {
        panic!("one turn/start in {sent:?}");
    };
    let text = start["params"]["input"][0]["text"].as_str().unwrap();
    let body = text
        .strip_prefix("<send_user_message_question_reply>\n")
        .and_then(|body| body.strip_suffix("\n</send_user_message_question_reply>\n"))
        .expect("the reply wrapper");
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        json!([{
            "questionItemId": format!("[\"request_user_input_async\",\"{QUESTION}\",0]"),
            "question": "Which color?",
            "answer": "Blue",
        }])
    );
    let mut shadow = Shadow::recording().codex().attached();
    shadow_lens(&mut shadow, &live.opened);
    shadow_lens(&mut shadow, &asked);
    shadow.answered(
        request_id,
        chat_answers(&[AgentQuestionAnswer {
            id: "q0".to_owned(),
            answers: vec!["Blue".to_owned()],
        }]),
    );
    shadow_lens(&mut shadow, &answered);
    let replied = live.play(&frames("question-reply"));
    assert!(resolved(&replied).is_empty(), "{replied:?}");
    shadow_lens(&mut shadow, &replied);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event).unwrap();
    }
    assert_eq!(
        texts(&view, ItemKind::Agent),
        ["Which color would you like?", "You chose Blue."]
    );
    assert!(
        !texts(&view, ItemKind::User)
            .iter()
            .any(|text| text.contains("send_user_message_question_reply"))
    );
    let request = view
        .requests
        .iter()
        .find(|request| request.id.0 == request_id)
        .unwrap();
    assert_eq!(
        request.close,
        Some(RequestClose::Answered {
            answer: Answer::Questions {
                answers: vec![QuestionAnswer {
                    question: "q0".to_owned(),
                    answers: vec!["Blue".to_owned()],
                }],
            },
        })
    );
}

#[test]
fn a_question_answered_in_the_terminal_withdraws_the_card() {
    let mut live = Harness::joined();
    live.sent();
    let asked = live.play(&frames("question"));
    let [(request_id, _, _)] = requested(&asked)[..] else {
        panic!("one card in {asked:?}");
    };
    let replied = live.play(&frames("question-reply"));
    assert_eq!(resolved(&replied), [request_id]);
}

#[test]
fn model_effort_and_plan_mode_go_through_thread_settings() {
    let mut live = Harness::joined();
    live.sent();
    let changed = live.lens.command(RuntimeCommand::SetConfigOption {
        option_id: "model".to_owned(),
        value: "gpt-5.6-luna".to_owned(),
    });
    assert!(
        matches!(&changed[..], [AgentStreamPayload::ConfigOptionsChanged { option_id, value, .. }]
            if option_id == "model" && value == "gpt-5.6-luna"),
        "{changed:?}"
    );
    let typed = live.lens.command(RuntimeCommand::Prompt {
        turn_id: 5,
        prompt: AgentPrompt {
            text: "/effort medium".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(typed.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished {
            turn_id: 5,
            outcome: AgentPromptOutcome::Finished { .. }
        }
    )));
    let plan = live.lens.command(RuntimeCommand::Prompt {
        turn_id: 6,
        prompt: AgentPrompt {
            text: "/plan".to_owned(),
            ..AgentPrompt::default()
        },
    });
    assert!(plan.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::PromptFinished { turn_id: 6, .. }
    )));
    assert!(live.pane.typed.lock().is_empty(), "nothing is typed");
    let sent = live.sent();
    let updates = named(&sent, "thread/settings/update")
        .into_iter()
        .map(|message| message["params"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        updates,
        [
            json!({ "threadId": THREAD, "model": "gpt-5.6-luna", "effort": "low" }),
            json!({ "threadId": THREAD, "model": "gpt-5.6-luna", "effort": "medium" }),
            json!({ "threadId": THREAD, "collaborationMode": { "mode": "plan", "settings": {
                "model": "gpt-5.6-luna", "reasoning_effort": "medium", "developer_instructions": null,
            } } }),
        ]
    );
    let settled = live.play(&frames("settings"));
    assert!(
        !settled
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::SettingFailed { .. })),
        "{settled:?}"
    );
    let facts = live.lens.source.facts();
    assert_eq!(facts.model.as_deref(), Some("gpt-5.6-luna"));
    assert_eq!(facts.effort.as_deref(), Some("medium"));
}

#[test]
fn a_refused_setting_reports_and_falls_back_to_what_codex_says() {
    let mut live = Harness::joined();
    live.sent();
    live.lens.command(RuntimeCommand::SetConfigOption {
        option_id: "effort".to_owned(),
        value: "medium".to_owned(),
    });
    let sent = live.sent();
    let update = named(&sent, "thread/settings/update")[0];
    let failed = live.play(&[json!({ "id": update["id"], "error": { "message": "no" } })]);
    assert!(failed.iter().any(|payload| matches!(
        payload,
        AgentStreamPayload::SettingFailed { option_id, message } if option_id == "effort" && message == "no"
    )), "{failed:?}");
    assert_eq!(live.lens.source.facts().effort.as_deref(), Some("low"));
}

#[test]
fn stop_interrupts_the_running_turn_instead_of_pressing_escape() {
    let mut live = Harness::joined();
    live.sent();
    let frames = frames("interrupt");
    let at = frames
        .iter()
        .position(|message| message["params"]["item"]["type"] == "commandExecution")
        .unwrap();
    let (running, rest) = frames.split_at(at + 1);
    let started = live.play(running);
    assert!(!started.is_empty());
    let stopped = live.lens.control(RuntimeControl::Cancel {
        turn_id: 0,
        session_id: None,
    });
    assert!(stopped.is_empty());
    let sent = live.sent();
    let [interrupt] = named(&sent, "turn/interrupt")[..] else {
        panic!("one turn/interrupt in {sent:?}");
    };
    assert_eq!(
        interrupt["params"],
        json!({ "threadId": THREAD, "turnId": SLEEP_TURN })
    );
    assert!(live.pane.typed.lock().is_empty(), "no Escape typed");
    let ended = live.play(rest);
    let (shadow, view) = view(&[&live.opened, &started, &ended]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert!(view.running().is_none());
}

#[test]
fn a_tui_in_embedded_mode_keeps_the_rollout_path() {
    let words = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
    for line in [
        "codex -c model=gpt-6-luna",
        "codex -cmodel=gpt-6-luna",
        "codex --config=model=x",
        "codex --search",
        "codex --profile work",
        "codex -p work",
        "codex --oss",
        "codex --no-daemon",
        "codex --strict-config",
        "codex --remote ws://127.0.0.1:1",
        "codex --dangerously-bypass-hook-trust",
        "codex -c features.web_search=true -c model=x",
        "codex -c features.web_search=maybe",
        "codex -c tui.theme=dark",
    ] {
        assert!(embedded(&words(line)), "{line}");
    }
    for line in [
        "codex",
        "codex -m gpt-6-luna",
        "codex resume abc",
        "codex --cd /tmp",
        "codex --enable web_search",
        "codex --disable=tools",
        "codex -c features.web_search=true",
        "codex -cfeatures.web_search=false",
        "codex --config tui.fullscreen_transcript=true",
        "codex --config=suppress_unstable_features_warning=true",
    ] {
        assert!(!embedded(&words(line)), "{line}");
    }
}

#[test]
fn a_thread_the_app_server_has_not_loaded_falls_back_to_the_rollout() {
    let live = Harness::open(&[], &["someone-else"]);
    assert!(!live.live());
    let sent = live.sent();
    assert!(named(&sent, "thread/resume").is_empty());
}

#[test]
fn leaving_unsubscribes_from_the_thread() {
    let mut live = Harness::joined();
    live.sent();
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.leave();
    }
    assert!(!live.live());
    let sent = live.sent();
    assert_eq!(
        named(&sent, "thread/unsubscribe")
            .first()
            .map(|message| message["params"].clone()),
        Some(json!({ "threadId": THREAD }))
    );
}

#[test]
fn a_new_thread_in_the_same_folder_becomes_the_session_once_its_rollout_exists() {
    let mut live = Harness::joined();
    let next = live._dir.path().join("rollout-next.jsonl");
    live.play(
        &[json!({ "method": "thread/started", "params": { "thread": {
        "id": "next", "cwd": "/work/proj", "ephemeral": false, "parentThreadId": null,
        "threadSource": "user", "path": next,
    } } })],
    );
    let Source::Codex(codex) = &live.lens.source else {
        unreachable!()
    };
    assert!(codex.located().is_none(), "no rollout yet");
    std::fs::write(&next, "").unwrap();
    assert_eq!(
        codex.located().map(|found| found.session),
        Some("next".to_owned())
    );
    live.play(
        &[json!({ "method": "thread/started", "params": { "thread": {
        "id": "title", "cwd": "/work/proj", "ephemeral": true, "path": null,
    } } })],
    );
    let Source::Codex(codex) = &live.lens.source else {
        unreachable!()
    };
    assert_eq!(
        codex.located().map(|found| found.session),
        Some("next".to_owned())
    );
}

#[test]
fn cards_open_when_the_tui_exits_are_withdrawn() {
    let mut live = Harness::joined();
    live.sent();
    let mut cards = live.play(&frames("question"));
    let approval = frames("approval");
    cards.extend(live.play(&approval[..1]));
    let open = requested(&cards)
        .iter()
        .map(|(request_id, _, _)| *request_id)
        .collect::<Vec<_>>();
    assert_eq!(open.len(), 2, "{cards:?}");
    assert!(resolved(&cards).is_empty(), "{cards:?}");
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.vanished();
    }
    let after = live.drain();
    let mut withdrawn = resolved(&after);
    withdrawn.sort_unstable();
    assert_eq!(withdrawn, open, "{after:?}");
    let (shadow, view) = view(&[&live.opened, &cards, &after]);
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        view.open_requests()
            .map(|request| request.id.0)
            .collect::<Vec<_>>()
            .len(),
        1,
        "only the terminal's own question card is left"
    );
    assert!(
        view.open_requests()
            .all(|request| !open.contains(&request.id.0))
    );
}

#[test]
fn a_card_open_when_the_app_server_goes_away_is_withdrawn() {
    let mut live = Harness::joined();
    live.sent();
    let approval = frames("approval");
    let card = live.play(&approval[..1]);
    let [(request_id, _, _)] = requested(&card)[..] else {
        panic!("one card in {card:?}");
    };
    live.peer = Connection::pair().1;
    let mut after = live.lens.tick();
    after.extend(live.lens.tick());
    assert!(!live.live());
    assert_eq!(resolved(&after), [request_id], "{after:?}");
}

fn started(id: &str, created: u64, path: &std::path::Path) -> Value {
    json!({ "method": "thread/started", "params": { "thread": {
        "id": id, "cwd": "/work/proj", "ephemeral": false, "parentThreadId": null,
        "threadSource": "user", "path": path, "createdAt": created,
    } } })
}

fn session(live: &Harness) -> Option<String> {
    let Source::Codex(codex) = &live.lens.source else {
        unreachable!()
    };
    codex.located().map(|found| found.session)
}

#[test]
fn a_tui_started_beside_this_one_does_not_take_the_lens() {
    let mut live = Harness::beside(&[], &[THREAD], &[]);
    let path = live._dir.path().join("rollout-theirs.jsonl");
    std::fs::write(&path, "").unwrap();
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.link.neighbors = Some(vec![1_793_488_400]);
    }
    live.play(&[started("theirs", 1_793_488_403, &path)]);
    assert!(live.live());
    assert_eq!(session(&live).as_deref(), Some(THREAD));
}

#[test]
fn a_new_thread_that_could_be_another_tuis_drops_the_lens_to_the_rollout() {
    let mut live = Harness::beside(&[], &[THREAD], &[]);
    let path = live._dir.path().join("rollout-unknown.jsonl");
    std::fs::write(&path, "").unwrap();
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.link.neighbors = Some(vec![1_793_400_000]);
    }
    live.play(&[started("unknown", 1_793_488_403, &path)]);
    assert!(!live.live());
    assert!(session(&live).is_none());
}

#[test]
fn a_fork_of_the_joined_thread_is_followed_even_beside_another_tui() {
    let crowded = Harness::beside(&[], &[THREAD], &[1_793_400_000]);
    assert!(
        !crowded.live(),
        "another TUI in the folder keeps the rollout path"
    );
    assert!(named(&crowded.sent(), "thread/resume").is_empty());
    let mut live = Harness::beside(&[], &[THREAD], &[]);
    let path = live._dir.path().join("rollout-fork.jsonl");
    std::fs::write(&path, "").unwrap();
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.link.neighbors = Some(vec![1_793_400_000]);
    }
    let mut fork = started("fork", 1_793_488_403, &path);
    fork["params"]["thread"]["forkedFromId"] = Value::from(THREAD);
    live.play(&[fork]);
    assert_eq!(session(&live).as_deref(), Some("fork"));
}

#[test]
fn a_tui_relaunched_in_the_pane_is_joined_only_once_its_rollout_is_this_one() {
    let mut live = Harness::joined();
    let rollout = live
        ._dir
        .path()
        .join(format!("rollout-2026-10-31T20-11-55-{THREAD}.jsonl"));
    let (wire, peer) = server(&[THREAD]);
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.vanished();
        codex.pid = Some(std::process::id());
        codex.relaunched(std::process::id());
        codex.link.wire = Some(wire);
    }
    live.drain();
    assert!(!live.live(), "a new TUI may be on a new thread");
    assert!(peer.outgoing.is_empty());
    if let Source::Codex(codex) = &mut live.lens.source {
        codex.relocated(&rollout);
    }
    live.lens.tick();
    assert!(live.live());
    let sent = peer
        .outgoing
        .try_iter()
        .map(|text| serde_json::from_str::<Value>(&text).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(named(&sent, "thread/resume").len(), 1);
}

#[test]
fn a_model_picked_in_the_terminal_moves_the_lens_controls() {
    let mut live = Harness::joined();
    live.sent();
    assert_eq!(
        live.lens.source.facts().model.as_deref(),
        Some("gpt-6-luna")
    );
    let mut updated = frames("settings")
        .into_iter()
        .find(|message| message["method"] == "thread/settings/updated")
        .expect("a settings frame");
    updated["params"]["threadSettings"]["model"] = Value::from("gpt-5.6-luna");
    updated["params"]["threadSettings"]["effort"] = Value::from("medium");
    let mut elsewhere = updated.clone();
    elsewhere["params"]["threadId"] = Value::from("another-thread");
    elsewhere["params"]["threadSettings"]["model"] = Value::from("gpt-6-sol");
    let moved = live.play(&[elsewhere, updated]);
    let changed = moved
        .iter()
        .filter_map(|payload| match payload {
            AgentStreamPayload::ConfigOptionsChanged {
                option_id, value, ..
            } => Some((option_id.as_str(), value.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(changed, [("model", "gpt-5.6-luna"), ("effort", "medium")]);
    assert!(live.sent().is_empty(), "nothing goes back to the server");
}
