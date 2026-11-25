use zz_protocol::{AgentImage as WireAgentImage, AgentProvider};

use super::*;
use crate::agent::{
    fixture::{Behavior, fixture_runner_at_host_tier},
    journal::AgentJournal,
    stream::{AgentStreamItem, AgentStreamPayload},
};

const DEADLINE: Duration = Duration::from_secs(30);

fn take_agent_frames(mailbox: &OutboundMailbox) -> Vec<ProtocolMessage> {
    let frames = {
        let mut state = mailbox.state.lock();
        let panes = state.agent_order.drain(..).collect::<Vec<_>>();
        let mut frames = Vec::new();
        for pane in panes {
            let Some(queued) = state.agent.remove(&pane) else {
                continue;
            };
            state.queued_bytes = state.queued_bytes.saturating_sub(queued.bytes);
            frames.extend(queued.frames.into_iter().map(|(_, frame)| frame));
        }
        frames
    };
    frames
        .into_iter()
        .map(|frame| decode_protocol_frame(&frame).expect("decode agent frame"))
        .collect()
}

fn drain_items(mailbox: &OutboundMailbox) -> Vec<AgentStreamItem> {
    take_agent_frames(mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentUpdates { items, .. },
                ..
            }) => Some(items),
            _ => None,
        })
        .flatten()
        .map(|item| serde_json::from_slice(&item).expect("decode stream item"))
        .collect()
}

fn applied_from_start(items: &[AgentStreamItem]) -> Vec<AgentStreamItem> {
    let mut cursor = 0;
    items
        .iter()
        .filter(|item| {
            let next = item.seq == cursor + 1;
            if next {
                cursor = item.seq;
            }
            next
        })
        .cloned()
        .collect()
}

fn chunk_text(item: &AgentStreamItem) -> Option<&str> {
    let AgentStreamPayload::Update { update } = &item.payload else {
        return None;
    };
    update.get("content")?.get("text")?.as_str()
}

fn encoded_updates(pane: PaneId, first_seq: u64, items: usize, bytes: usize) -> Vec<u8> {
    let message = Shared::event(EventPayload::AgentUpdates {
        pane,
        first_seq,
        items: (0..items).map(|_| vec![b'x'; bytes]).collect(),
    });
    encode_protocol_message(&message).expect("encode agent updates")
}

#[track_caller]
fn wait_for_items_in<F>(mailbox: &OutboundMailbox, what: &str, accept: F) -> Vec<AgentStreamItem>
where
    F: Fn(&[AgentStreamItem]) -> bool,
{
    let deadline = Instant::now() + DEADLINE;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        seen.extend(drain_items(mailbox));
        if accept(&seen) {
            return seen;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for {what}: {seen:#?}");
}

fn wait_for_ready_broadcast(mailbox: &OutboundMailbox, agent: PaneId) {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        let ready = take_reliable_messages(mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentState { pane, state },
                    ..
                }) if *pane == agent
                    && matches!(state.phase, zz_protocol::AgentConnectionPhase::Ready)
            )
        });
        if ready {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for agent pane {agent:?} to broadcast Ready");
}

fn agent_states_for(mailbox: &OutboundMailbox, agent: PaneId) -> usize {
    take_reliable_messages(mailbox)
        .iter()
        .filter(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentState { pane, .. },
                    ..
                }) if *pane == agent
            )
        })
        .count()
}

struct Workspace {
    shared: Arc<Shared>,
    mailbox: Arc<OutboundMailbox>,
    client: ClientId,
    session: SessionId,
    agent: PaneId,
    runtime: Arc<crate::agent::fanout::AgentRuntime>,
    _journal: tempfile::TempDir,
}

fn workspace(behavior: Behavior) -> Workspace {
    workspace_with(&[behavior])
}

fn workspace_with(behaviors: &[Behavior]) -> Workspace {
    let behaviors = behaviors.to_vec();
    let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let runtime = shared
        .build_agent_runtime(Some(journal))
        .expect("agent runtime");
    runtime.set_runner_factory(Box::new(move |_| {
        let index = opened.fetch_add(1, Ordering::Relaxed);
        fixture_runner_at_host_tier(
            AgentProvider::Codex,
            behaviors[index.min(behaviors.len() - 1)],
            true,
        )
    }));

    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "agents"]),
        )
        .expect("workspace session");
    let session = context.session.expect("session");
    shared.attach(client, session).expect("attach");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-v"]),
        )
        .expect("workspace command");
    let agent = context.pane.expect("agent terminal");
    assert!(shared.open_test_agent(agent));
    wait_for_ready_broadcast(&mailbox, agent);
    shared.send_resync(client, &mailbox);
    take_reliable_messages(&mailbox);
    Workspace {
        shared,
        mailbox,
        client,
        session,
        agent,
        runtime,
        _journal: directory,
    }
}

impl Workspace {
    #[track_caller]
    fn wait_for_items<F>(&self, what: &str, accept: F) -> Vec<AgentStreamItem>
    where
        F: Fn(&[AgentStreamItem]) -> bool,
    {
        wait_for_items_in(&self.mailbox, what, accept)
    }

    fn prompt(&self, text: &str) -> Result<(), ServerError> {
        self.shared.agent_message(
            self.client,
            ProtocolMessage::AgentPrompt {
                pane: self.agent,
                text: text.to_owned(),
                images: Vec::new(),
            },
        )
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.shared.shutdown_agents();
    }
}

#[test]
fn an_agent_pane_is_visible_before_its_runtime_starts() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let runtime = shared
        .build_agent_runtime(Some(journal))
        .expect("agent runtime");

    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "agents"]),
        )
        .expect("workspace session");
    let session = context.session.expect("session");
    shared.attach(client, session).expect("attach");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-v"]),
        )
        .expect("workspace command");
    let agent = context.pane.expect("agent terminal");
    let observed = Arc::new(AtomicBool::new(false));
    let runner_observed = Arc::clone(&observed);
    let weak_shared = Arc::downgrade(&shared);
    runtime.set_runner_factory(Box::new(move |_| {
        let shared = weak_shared.upgrade().expect("shared daemon");
        runner_observed.store(
            shared.read_client(client, |c| {
                c.and_then(|c| c.visible_agents.as_ref())
                    .is_some_and(|panes| panes.contains(&agent))
            }),
            Ordering::Release,
        );
        Box::new(|_| Box::pin(std::future::pending::<Result<(), String>>()))
    }));

    assert!(shared.open_test_agent(agent));

    assert!(
        observed.load(Ordering::Acquire),
        "the runner started before the attached client could see the agent pane"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentState { pane, .. },
                ..
            }) if *pane == agent
        )
    }) {
        assert!(
            Instant::now() < deadline,
            "opening the runtime did not publish the agent state"
        );
        thread::sleep(Duration::from_millis(5));
    }
    shared.shutdown_agents();
}

#[test]
fn the_agent_lane_batches_per_pane_and_drains_behind_the_reliable_one() {
    let mailbox = OutboundMailbox::new();
    let first = PaneId(1);
    let second = PaneId(2);

    assert!(mailbox.enqueue_agent(first, 1, encoded_updates(first, 1, 2, 16)));
    assert!(mailbox.enqueue_agent(second, 1, encoded_updates(second, 1, 1, 16)));
    assert!(mailbox.enqueue_agent(first, 3, encoded_updates(first, 3, 1, 16)));
    assert!(mailbox.enqueue_reliable(&Shared::event(EventPayload::Bell { pane: first })));

    let panes = std::iter::from_fn(|| mailbox.recv())
        .take(4)
        .map(
            |frame| match decode_protocol_frame(&frame).expect("decode") {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::AgentUpdates {
                            pane, first_seq, ..
                        },
                    ..
                }) => (Some(pane), first_seq),
                _ => (None, 0),
            },
        )
        .collect::<Vec<_>>();

    assert_eq!(
        panes,
        [
            (None, 0),
            (Some(first), 1),
            (Some(second), 1),
            (Some(first), 3)
        ],
        "the reliable lane goes first, then one agent frame per pane per turn"
    );
}

#[test]
fn agent_request_replies_reach_only_the_requesting_client() {
    let shared = Arc::new(Shared::new(1));
    let first = OutboundMailbox::new();
    let second = OutboundMailbox::new();
    let (first_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&first));
    let (_second_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&second));

    shared.send_agent_reply(
        PaneId(4),
        AgentRequestReply::Sessions {
            client: first_client,
            result: "history".to_owned(),
        },
    );

    assert!(
        take_reliable_messages(&first)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentSessions { result, .. },
                    ..
                }) if result == "history"
            ))
    );
    assert!(take_reliable_messages(&second).is_empty());
}

#[test]
fn an_overflowing_agent_lane_asks_for_a_replay_instead_of_closing() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(1);
    let chunk = MAX_PENDING_AGENT_BYTES / 4;

    for seq in 0..3 {
        assert!(mailbox.enqueue_agent(
            pane,
            seq * 10 + 1,
            encoded_updates(pane, seq * 10 + 1, 1, chunk)
        ));
    }
    assert!(mailbox.enqueue_agent(pane, 31, encoded_updates(pane, 31, 1, chunk)));

    {
        let state = mailbox.state.lock();
        assert!(!state.closed, "a slow client is not disconnected");
        assert!(state.agent.is_empty(), "the pane's lane was cleared");
    }
    let frame = mailbox.recv().expect("the lag marker");
    assert!(matches!(
        decode_protocol_frame(&frame).expect("decode"),
        ProtocolMessage::Event(Event {
            payload: EventPayload::AgentLagged { pane: lagged, next_seq: 1 },
            ..
        }) if lagged == pane
    ));
}

#[test]
fn a_multiframe_agent_replay_is_admitted_atomically() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(1);
    let chunk = MAX_PENDING_AGENT_BYTES / 3;
    let frames = (0..4)
        .map(|index| {
            let first_seq = index * 10 + 1;
            (first_seq, encoded_updates(pane, first_seq, 1, chunk))
        })
        .collect();

    assert!(mailbox.enqueue_agent_replay(pane, frames));
    {
        let state = mailbox.state.lock();
        let queued = state.agent.get(&pane).expect("replay lane");
        assert!(queued.bytes > MAX_PENDING_AGENT_BYTES);
        assert_eq!(queued.frames.len(), 4);
        assert!(state.reliable.is_empty());
    }

    for expected in [1, 11, 21, 31] {
        let frame = mailbox.recv().expect("replay frame");
        assert!(matches!(
            decode_protocol_frame(&frame).expect("decode"),
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentUpdates {
                    pane: target,
                    first_seq,
                    ..
                },
                ..
            }) if target == pane && first_seq == expected
        ));
    }
}

#[test]
fn agent_updates_reach_only_the_clients_the_pane_is_visible_to() {
    let workspace = workspace(Behavior::Chunk);
    let elsewhere = OutboundMailbox::new();
    let (watcher, _) = workspace.shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&elsewhere),
    );
    workspace
        .shared
        .attach(watcher, workspace.session)
        .expect("attach the second client");
    let mut context = ExecutionContext::default();
    workspace
        .shared
        .execute(
            watcher,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-t", "agents"]),
        )
        .expect("a window of its own");
    workspace.shared.publish_snapshot();
    take_reliable_messages(&elsewhere);
    let _ = drain_items(&elsewhere);

    workspace.prompt("go").expect("prompt");
    workspace.wait_for_items("the turn", |items| {
        items.iter().any(|item| chunk_text(item) == Some("turn 0"))
    });

    assert!(
        drain_items(&elsewhere).is_empty(),
        "a client looking at another window is not sent the transcript"
    );
    assert!(
        take_reliable_messages(&elsewhere)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentState { pane, .. },
                    ..
                }) if *pane == workspace.agent
            )),
        "but it still sees the pane state its badges need"
    );
}

#[test]
fn a_client_that_asks_for_a_replay_follows_the_pane_live_whether_attached_or_not() {
    if !crate::daemon::solo_tests::rerun_alone(
        "daemon::tests::agent::a_client_that_asks_for_a_replay_follows_the_pane_live_whether_attached_or_not",
    ) {
        return;
    }
    let workspace = workspace(Behavior::Chunk);
    let subscribe = || {
        let mailbox = OutboundMailbox::new();
        let (client, _) = workspace.shared.register_subscribed(
            ClientKind::Interactive,
            None,
            None,
            Arc::clone(&mailbox),
        );
        (client, mailbox)
    };
    let (outside, outside_mailbox) = subscribe();
    let (elsewhere, elsewhere_mailbox) = subscribe();
    let (_bystander, bystander_mailbox) = subscribe();
    workspace
        .shared
        .attach(elsewhere, workspace.session)
        .expect("attach the client that looks at another window");
    let mut context = ExecutionContext::default();
    workspace
        .shared
        .execute(
            elsewhere,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-t", "agents"]),
        )
        .expect("a window of its own");
    workspace.shared.publish_snapshot();
    assert!(
        workspace.shared.read_client(elsewhere, |c| {
            c.and_then(|c| c.visible_agents.as_ref())
                .is_some_and(|visible| !visible.contains(&workspace.agent))
        }),
        "the attached client's window does not show the agent pane"
    );
    for mailbox in [
        &workspace.mailbox,
        &outside_mailbox,
        &elsewhere_mailbox,
        &bystander_mailbox,
    ] {
        take_reliable_messages(mailbox);
        let _ = drain_items(mailbox);
    }

    for client in [outside, elsewhere, workspace.client] {
        workspace
            .shared
            .agent_message(
                client,
                ProtocolMessage::AgentReplay {
                    pane: workspace.agent,
                    from_seq: 0,
                },
            )
            .expect("replay");
    }
    assert_eq!(
        agent_states_for(&outside_mailbox, workspace.agent),
        1,
        "an unattached watcher is sent the pane state as it starts watching"
    );
    assert_eq!(
        agent_states_for(&elsewhere_mailbox, workspace.agent),
        1,
        "so is an attached one whose window does not show the pane"
    );

    workspace.prompt("go").expect("prompt");
    let turn =
        |items: &[AgentStreamItem]| items.iter().any(|item| chunk_text(item) == Some("turn 0"));
    for (mailbox, who) in [
        (&outside_mailbox, "the unattached watcher"),
        (&elsewhere_mailbox, "the attached watcher on another window"),
        (
            &workspace.mailbox,
            "the attached client that sees and watches the pane",
        ),
    ] {
        let mut items = wait_for_items_in(mailbox, who, turn);
        items.extend(drain_items(mailbox));
        assert_eq!(
            items
                .iter()
                .filter(|item| chunk_text(item) == Some("turn 0"))
                .count(),
            1,
            "{who} is sent each live item once"
        );
    }
    let deadline = Instant::now() + DEADLINE;
    while agent_states_for(&outside_mailbox, workspace.agent) == 0 {
        assert!(
            Instant::now() < deadline,
            "the unattached watcher was not sent the turn's state changes"
        );
        thread::sleep(Duration::from_millis(5));
    }

    assert!(
        drain_items(&bystander_mailbox).is_empty(),
        "a client that never asked is sent no transcript"
    );
    assert_eq!(
        agent_states_for(&bystander_mailbox, workspace.agent),
        0,
        "nor the pane state"
    );

    workspace
        .shared
        .execute(
            workspace.client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-pane", ["-t", &workspace.agent.to_string()]),
        )
        .expect("close the agent pane");
    assert!(
        !workspace.shared.read_client(outside, |c| c
            .is_some_and(|c| c.watched_agents.contains(&workspace.agent))),
        "a closed pane is no longer watched"
    );
}

#[test]
fn a_waiting_agent_send_returns_the_turns_reply() {
    let workspace = workspace(Behavior::Chunk);
    let mut context = ExecutionContext::default();

    let execution = workspace
        .shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "agent-send",
                ["-t", &workspace.agent.to_string(), "--wait", "review this"],
            ),
        )
        .expect("agent-send --wait returns once the turn ends");
    assert_eq!(execution.output, "turn 0");
}

#[test]
fn new_pane_kind_agent_opens_a_chat_that_runs_its_brief_first() {
    let workspace = workspace(Behavior::Chunk);
    let mut context =
        ExecutionContext::for_pane(&workspace.shared.inner.lock().engine.state, workspace.agent)
            .expect("chat context");
    let mut run = |verb: &str, args: &[&str]| {
        workspace
            .shared
            .execute(
                ClientId(99),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(verb, args.iter().copied()),
            )
            .map(|execution| execution.output.to_string())
    };
    let created = run(
        "new-pane",
        &[
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "--kind",
            "agent",
            "codex",
            "hi",
        ],
    )
    .expect("new-pane --kind agent");
    assert_eq!(created, "%2\n");
    let created = created.trim_end();
    let pane = created.parse::<PaneId>().expect("pane id");
    assert!(matches!(
        workspace.shared.inner.lock().engine.state.pane(pane).map(|pane| &pane.kind),
        Some(PaneKind::Agent(agent)) if agent.provider == AgentProvider::Codex
    ));
    assert_eq!(
        run("agent-send", &["-t", created, "--wait", "again"]).expect("agent-send"),
        "turn 1"
    );
}

fn received(workspace: &Workspace, pane: PaneId) -> Vec<String> {
    workspace
        .runtime
        .retained_items(pane)
        .iter()
        .filter(|item| {
            matches!(&item.payload, AgentStreamPayload::Update { update }
                if update["sessionUpdate"] == "user_message_chunk")
        })
        .filter_map(chunk_text)
        .map(str::to_owned)
        .collect()
}

#[track_caller]
fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + DEADLINE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn run_as(
    workspace: &Workspace,
    client: ClientId,
    origin: Option<PaneId>,
    verb: &str,
    args: &[&str],
) -> Result<String, String> {
    let mut context = {
        let mut inner = workspace.shared.inner.lock();
        let entry = inner.client_entry(client);
        entry.origin = origin;
        entry.command_streams.get_or_insert_default();
        origin
            .and_then(|pane| ExecutionContext::for_pane(&inner.engine.state, pane))
            .unwrap_or_default()
    };
    workspace
        .shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(verb, args.iter().copied()),
        )
        .map(|execution| execution.output.to_string())
        .map_err(|error| match error {
            DaemonError::Server(error) => error.tmux_message(),
            error => error.to_string(),
        })
}

fn take_stderr(workspace: &Workspace, client: ClientId) -> String {
    std::mem::take(
        &mut workspace
            .shared
            .inner
            .lock()
            .client_entry(client)
            .command_streams
            .get_or_insert_default()
            .stderr,
    )
}

#[test]
fn a_question_ask_prints_option_lines_a_shell_hands_to_agent_respond() {
    let permission = crate::agent::host::AgentPendingPermission {
        request_id: 7,
        tool_call: serde_json::json!({ "title": "Which color?" }),
        options: serde_json::json!([
            { "optionId": "answer-0", "name": "Dark blue", "kind": "allow_once" },
            { "optionId": "answer-1", "name": "Red", "kind": "allow_once" },
        ]),
        questions: vec![zz_protocol::agent_stream::AgentQuestion {
            id: "q1".to_owned(),
            header: None,
            question: "Which color?".to_owned(),
            options: Vec::new(),
            multi_select: false,
            allow_other: false,
            secret: false,
        }],
    };
    let ask = agent_ask(PaneId(2), AgentProvider::Codex, &permission).to_string();
    assert_eq!(
        ask,
        "[zz] %2 (codex) asks: Which color?\nzz agent-respond -t %2 --option answer-0 7  # Dark blue\nzz agent-respond -t %2 --option answer-1 7  # Red\nzz agent-respond -t %2 --deny 7"
    );
    let line = ask.lines().nth(1).expect("option line");
    let words = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("set -- {line}\nprintf '%s\\n' \"$@\""))
        .output()
        .expect("sh");
    let words = String::from_utf8(words.stdout).expect("words");
    let args = words.lines().skip(2).map(RawText::from).collect::<Vec<_>>();
    let parsed = parse_agent_respond_args(&args).expect("agent-respond takes the line");
    assert_eq!(parsed.target.as_deref(), Some("%2"));
    assert_eq!(parsed.option.as_deref(), Some("answer-0"));
    assert_eq!(parsed.request_id, Some(7));
}

#[test]
fn a_chat_sends_eight_messages_to_a_chat_until_the_user_writes_again() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(97),
        None,
        "new-pane",
        &["-d", "-t", "%1", "--kind", "agent", "codex"],
    )
    .expect("a chat with no parent");
    let send = || {
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "agent-send",
            &["-t", "%2", "--wait", "thanks"],
        )
    };
    for turn in 0..8 {
        assert_eq!(send(), Ok(format!("turn {turn}")));
    }
    assert_eq!(
        send(),
        Err(
            "%2 has had 8 messages from you since the user last wrote; answer the user now"
                .to_owned()
        )
    );
    workspace.prompt("one more, please").expect("user prompt");
    assert_eq!(send(), Ok("turn 8".to_owned()));
}

#[test]
fn a_closed_child_reopens_on_its_session_and_keeps_its_old_id() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    assert_eq!(
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "new-pane",
            &[
                "-d",
                "-P",
                "-F",
                "#{pane_id}",
                "--kind",
                "agent",
                "claude",
                "reply with ok"
            ],
        ),
        Ok("%2\n".to_owned())
    );
    wait_until("the child's session in its record", || {
        workspace
            .shared
            .child_record(host, PaneId(2))
            .is_some_and(|record| record.session.as_deref() == Some("fixture-session"))
    });
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%2"]).expect("kill");
    let reopen = || {
        run_as(
            &workspace,
            ClientId(97),
            None,
            "select-child-chat",
            &["-t", "%1", "%2"],
        )
    };
    assert_eq!(reopen(), Ok("%3".to_owned()));
    {
        let inner = workspace.shared.inner.lock();
        assert!(matches!(
            inner.engine.state.pane(PaneId(3)).map(|pane| &pane.kind),
            Some(PaneKind::Agent(agent))
                if agent.provider == AgentProvider::ClaudeCode
                    && agent.session_id.as_deref() == Some("fixture-session")
        ));
        assert_eq!(inner.engine.agent_parent(PaneId(3)), Some(host));
    }
    assert_eq!(reopen(), Ok("%3".to_owned()));
    assert!(
        workspace
            .shared
            .inner
            .lock()
            .engine
            .state
            .pane(PaneId(4))
            .is_none()
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "agent-send",
            &["-t", "%2", "--wait", "hi again"],
        ),
        Ok("turn 0".to_owned())
    );
    let select = |child: &str| {
        run_as(
            &workspace,
            ClientId(97),
            None,
            "select-child-chat",
            &["-t", "%1", child],
        )
    };
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%3"]).expect("kill");
    assert_eq!(select("%3"), Ok("%4".to_owned()));
    assert!(matches!(
        workspace.shared.inner.lock().engine.state.pane(PaneId(4)).map(|pane| &pane.kind),
        Some(PaneKind::Agent(agent))
            if agent.session_id.as_deref() == Some("fixture-session")
    ));
    assert_eq!(
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "agent-send",
            &["-t", "%3", "--wait", "and again"],
        ),
        Ok("turn 0".to_owned())
    );
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%4"]).expect("kill");
    assert_eq!(select("%2"), Ok("%5".to_owned()));
    assert_eq!(select("%3"), Ok("%5".to_owned()));
    workspace.runtime.update_child(
        AgentProvider::Codex,
        "fixture-session",
        PaneId(9),
        |record| record.provider = AgentProvider::Codex,
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "select-child-chat",
            &["-t", "%1", "%9"],
        ),
        Err("%9 was closed before its session started; it cannot be opened again".to_owned())
    );
}

#[test]
fn a_child_card_click_answers_the_pane_id_without_a_command_output_view() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "claude", "reply with ok"],
    )
    .expect("child chat");
    wait_until("the child's session in its record", || {
        workspace
            .shared
            .child_record(host, PaneId(2))
            .is_some_and(|record| record.session.is_some())
    });
    let click = || {
        let mut context =
            ExecutionContext::for_pane(&workspace.shared.inner.lock().engine.state, host)
                .expect("host context");
        let response = workspace.shared.execute_command_request(
            workspace.client,
            ClientKind::Interactive,
            &mut context,
            1,
            &CommandInvocation::new("select-child-chat", ["-t", "%1", "%2"]),
        );
        let output = match response {
            CommandResponse::Success { output, .. } => output.to_string(),
            other @ CommandResponse::Error { .. } => {
                panic!("select-child-chat failed: {other:?}")
            }
        };
        let viewing = workspace
            .shared
            .inner
            .lock()
            .client(workspace.client)
            .is_some_and(|client| client.command_output.is_some());
        (output, viewing)
    };
    assert_eq!(click(), ("%2".to_owned(), false));
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%2"]).expect("kill");
    assert_eq!(click(), ("%3".to_owned(), false));
}

#[test]
fn a_reopened_chat_starts_on_the_model_and_effort_it_last_showed() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &[
            "-d", "--kind", "agent", "--model", "haiku", "--effort", "low", "claude",
        ],
    )
    .expect("child chat");
    let child = PaneId(2);
    wait_until("the child's session in its record", || {
        workspace
            .shared
            .child_record(host, child)
            .is_some_and(|record| record.session.as_deref() == Some("fixture-session"))
    });
    let _ = workspace.shared.apply_agent_state(
        child,
        AgentPaneWire {
            session_id: Some("fixture-session".to_owned()),
            config_options: serde_json::json!([
                { "id": "model", "currentValue": "opus" },
                { "id": "effort", "currentValue": "high" },
            ])
            .to_string(),
            ..AgentPaneWire::default()
        },
    );
    let picks = |pane: PaneId| {
        let spec = workspace.shared.agent_pane_spec(pane).expect("a chat");
        (spec.model, spec.effort)
    };
    let shown = (Some("opus".to_owned()), Some("high".to_owned()));
    assert_eq!(picks(child), shown);
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%2"]).expect("kill");
    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "select-child-chat",
            &["-t", "%1", "%2"],
        ),
        Ok("%3".to_owned())
    );
    assert_eq!(picks(PaneId(3)), shown);
}

#[cfg(unix)]
#[test]
fn a_child_record_follows_the_account_its_chats_moved_to() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "claude"],
    )
    .expect("child chat");
    let child = PaneId(2);
    wait_until("the child's session in its record", || {
        workspace
            .shared
            .child_record(host, child)
            .is_some_and(|record| record.session.as_deref() == Some("fixture-session"))
    });
    let account = |workspace: &Workspace| {
        workspace
            .shared
            .child_record(host, child)
            .and_then(|record| record.account)
    };
    assert_eq!(account(&workspace), None);
    workspace
        .shared
        .relaunch_agent_chats(AgentProvider::ClaudeCode, || Some("a2".to_owned()));
    assert_eq!(account(&workspace), Some("a2".to_owned()));
}

#[test]
fn chat_verbs_say_what_to_run_next() {
    let workspace = workspace_with(&[Behavior::Hang, Behavior::Chunk]);
    let terminal = ClientId(97);
    let run = |verb: &str, args: &[&str]| run_as(&workspace, terminal, None, verb, args);
    run(
        "new-pane",
        &["-d", "-t", "%1", "--kind", "agent", "codex", "hi"],
    )
    .expect("new-pane with a brief");
    assert_eq!(
        take_stderr(&workspace, terminal),
        "new-pane: opened %2 (codex); read its reply with: zz agent-send -t %2 --wait ''\n"
    );
    run("new-pane", &["-d", "-t", "%1", "--kind", "agent", "codex"]).expect("new-pane");
    assert_eq!(
        take_stderr(&workspace, terminal),
        "new-pane: opened %3 (codex)\n"
    );
    assert_eq!(
        run("agent-send", &["-t", "%1", "busy"]),
        Ok("%1".to_owned())
    );
    assert_eq!(
        take_stderr(&workspace, terminal),
        "agent-send: read %1's reply with: zz agent-send -t %1 --wait ''\n"
    );
    wait_until("the turn to run", || {
        run("capture-pane", &["-p", "-t", "%1"]).is_ok()
            && take_stderr(&workspace, terminal) == "capture-pane: %1's turn is still running\n"
    });
    for queued in 1..=4 {
        run("agent-send", &["-t", "%1", "more"]).expect("queued");
        wait_until("the prompt to queue", || {
            workspace.runtime.waiting_prompts(workspace.agent) == queued
        });
    }
    assert_eq!(
        run("agent-send", &["-t", "%1", "one too many"]),
        Err(
            "%1 has 4 prompts waiting; wait for its turn with: zz agent-send -t %1 --wait ''"
                .to_owned()
        )
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(98),
            Some(PaneId(2)),
            "agent-send",
            &["--notify", "-t", "%3", "hi"]
        ),
        Ok("%3".to_owned())
    );
    assert_eq!(
        take_stderr(&workspace, ClientId(98)),
        "agent-send: %3's reply comes back to you as a message; end your turn now\n"
    );
    assert_eq!(
        run("wait-pane", &["-t", "%1"]),
        Err(
            "%1 is an agent chat; wait for its turn with: zz agent-send -t %1 --wait ''".to_owned()
        )
    );
    assert_eq!(
        run("show-last-output", &["-t", "%1"]),
        Err("%1 is an agent chat; read it with: zz capture-pane -p -t %1".to_owned())
    );
    assert_eq!(
        run("agent-respond", &["-t", "%3", "--allow"]),
        Err("%3 has no pending request (already answered, or none was asked)".to_owned())
    );
}

#[test]
fn a_timed_out_wait_names_a_wait_that_joins_the_running_turn() {
    let workspace = workspace(Behavior::Hang);
    let timed_out = |args: &[&str]| {
        let mut context = ExecutionContext::default();
        match workspace.shared.execute(
            ClientId(97),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("agent-send", args.iter().copied()),
        ) {
            Err(DaemonError::CommandExit {
                output,
                exit_code: 124,
            }) => output.to_string(),
            other => panic!("{args:?}: {other:?}"),
        }
    };
    let message = "%1: no reply within 1 seconds; the turn is still running. `agent-send -t %1 --wait ''` waits again without sending";
    assert_eq!(
        timed_out(&["-t", "%1", "--wait", "--timeout", "1", "go"]),
        message
    );
    assert_eq!(
        timed_out(&["-t", "%1", "--wait", "--timeout", "1", ""]),
        message
    );
}

#[test]
fn capture_pane_prints_a_chat_as_text() {
    let workspace = workspace(Behavior::Chunk);
    let capture = |args: &[&str]| {
        run_as(&workspace, ClientId(97), None, "capture-pane", args).expect("capture-pane")
    };
    assert_eq!(capture(&["-p", "-t", "%1"]), "");
    for (turn, text) in ["one", "two"].into_iter().enumerate() {
        run_as(
            &workspace,
            ClientId(97),
            None,
            "agent-send",
            &["-t", "%1", "--wait", text],
        )
        .expect("turn");
        wait_until("the reply in the chat state", || {
            capture(&["-p", "-t", "%1"]).ends_with(&format!("turn {turn}\n"))
        });
    }
    assert_eq!(
        capture(&["-p", "-t", "%1"]),
        "> [zz] a command sent this:\n  two\n\nturn 1\n"
    );
    assert_eq!(
        capture(&["-p", "-J", "-t", "%1", "-S", "-"]),
        "> [zz] a command sent this:\n  one\n\nturn 0\n\n> [zz] a command sent this:\n  two\n\nturn 1\n"
    );
}

#[test]
fn a_send_from_the_apps_own_ui_keeps_the_users_words() {
    let workspace = workspace(Behavior::Chunk);
    workspace
        .shared
        .execute(
            workspace.client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("agent-send", ["-t", "%1", "hi"]),
        )
        .expect("a send bound to a key");
    wait_until("the prompt in the chat", || {
        !received(&workspace, workspace.agent).is_empty()
    });
    assert_eq!(received(&workspace, workspace.agent), ["hi"]);
}

#[test]
fn a_send_relayed_to_another_daemon_brings_its_reply_back_to_the_calling_chat() {
    use super::super::agent_relay::{Gate, Relay};

    let here = workspace(Behavior::Chunk);
    let there = workspace(Behavior::Chunk);
    let remote = |command: CommandInvocation| {
        let stderr = || take_stderr(&there, ClientId(97));
        stderr();
        let mut context = ExecutionContext::default();
        let ran = there
            .shared
            .execute(ClientId(97), ClientKind::Command, &mut context, &command);
        let (stdout, exit_code) = match ran {
            Ok(execution) => (execution.output, 0),
            Err(DaemonError::CommandExit { output, exit_code }) => (output, exit_code),
            Err(error) => return Err(error),
        };
        Ok(zz_daemon_client::CommandOutcome {
            stdout,
            stderr: stderr(),
            exit_code,
            stdout_claim: zz_protocol::StdoutClaim::default(),
            client_exit: false,
        })
    };
    let relay = |pane| Relay {
        shared: Arc::downgrade(&here.shared),
        caller: here.agent,
        host: "macbook".to_owned(),
        pane,
        payload: "[zz] xps:%1 (codex) sent this, not the user:\nstatus?".to_owned(),
        gate: Arc::new(Mutex::new(Gate::Opening)),
    };
    let (opened, taken) = crossbeam_channel::bounded(2);
    relay(there.agent).run_on(remote, &opened.clone().into());
    assert_eq!(taken.try_recv(), Ok(Ok(())));
    wait_until("the reply in the calling chat", || {
        !received(&here, here.agent).is_empty()
    });
    assert_eq!(
        received(&here, here.agent),
        [
            "[zz] macbook:%1 (codex) finished:\nturn 0\n\nIf this answers the user, answer the user now and send nothing to macbook:%1. Only if something is missing: zz --host macbook agent-send -t %1 'TEXT'"
        ]
    );
    assert_eq!(
        received(&there, there.agent),
        ["[zz] xps:%1 (codex) sent this, not the user:\nstatus?"]
    );
    assert_eq!(here.runtime.sends_since_user(here.agent, there.agent), 1);
    relay(PaneId(9)).run_on(remote, &opened.into());
    assert_eq!(
        taken.try_recv(),
        Ok(Err("found no chat in that pane".to_owned()))
    );
}

#[test]
fn a_chat_opened_by_a_chat_reports_back_and_closes_with_it() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    let created = run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &[
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "--kind",
            "agent",
            "codex",
            "reply with ok",
        ],
    )
    .expect("new-pane from a chat");
    assert_eq!(created, "%2\n");
    let child = PaneId(2);
    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "display-message",
            &["-p", "-t", "%2", "#{agent_parent}"],
        ),
        Ok("%1".to_owned())
    );
    wait_until("the reply in the host", || {
        !received(&workspace, host).is_empty()
    });
    assert_eq!(
        received(&workspace, host),
        [
            "[zz] %2 (codex) finished:\nturn 0\n\nIf this answers the user, answer the user now and send nothing to %2. Only if something is missing: zz agent-send -t %2 'TEXT'"
        ]
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "agent-send",
            &["-t", "%2", "--wait", ""]
        ),
        Err(
            "%2 has no running turn; read its last reply with: zz capture-pane -p -t %2".to_owned()
        )
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(98),
            Some(child),
            "agent-send",
            &["-t", "%1", "x"]
        ),
        Err("%1 opened this chat; your reply reaches it when your turn ends".to_owned())
    );
    let panes = workspace
        .shared
        .inner
        .lock()
        .engine
        .state
        .windows
        .values()
        .map(|window| window.panes.len())
        .sum::<usize>();
    assert_eq!(
        run_as(
            &workspace,
            ClientId(98),
            Some(child),
            "new-pane",
            &["--kind", "agent", "codex", "x"],
        ),
        Err(
            "%2 was opened by %1; a chat opened by another chat cannot open one. Do the work yourself, or say in your reply what you need".to_owned()
        )
    );
    assert_eq!(
        workspace
            .shared
            .inner
            .lock()
            .engine
            .state
            .windows
            .values()
            .map(|window| window.panes.len())
            .sum::<usize>(),
        panes
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(98),
            Some(child),
            "split-window",
            &["-d", "-P", "-F", "#{pane_id}"],
        ),
        Ok("%3\n".to_owned())
    );
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%1"]).expect("kill-pane");
    wait_until("the child and its terminal to close with its host", || {
        let inner = workspace.shared.inner.lock();
        [host, child, PaneId(3)]
            .iter()
            .all(|pane| inner.engine.state.pane(*pane).is_none())
    });
}

#[test]
fn an_ask_answered_while_its_host_was_busy_never_reaches_the_host() {
    let workspace = workspace_with(&[Behavior::AskPermission]);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(97),
        None,
        "set-option",
        &["-g", "agent-auto-approve", "off"],
    )
    .expect("auto-approve off");
    let pending = |pane: PaneId| {
        workspace
            .runtime
            .wire_state(pane)
            .and_then(|state| state.pending_permission)
            .map(|permission| permission.request_id)
    };
    let queued = || workspace.runtime.queued_prompts(host);
    workspace.prompt("work").expect("host prompt");
    wait_until("the host's own ask", || pending(host).is_some());
    for child in ["%2", "%3"] {
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "new-pane",
            &["-d", "--kind", "agent", "codex", "run it"],
        )
        .expect("new-pane from a chat");
        let child = child.parse().expect("pane id");
        wait_until("the child's ask", || pending(child).is_some());
    }
    wait_until("both asks queued behind the host's turn", || queued() == 2);
    run_as(
        &workspace,
        ClientId(97),
        None,
        "agent-respond",
        &["-t", "%2", "--allow"],
    )
    .expect("answer %2 in its own pane");
    wait_until("%2's report queued too", || queued() == 3);
    run_as(
        &workspace,
        ClientId(97),
        None,
        "agent-respond",
        &["-t", "%1", "--allow"],
    )
    .expect("end the host's turn");
    wait_until("the host's next turn", || {
        received(&workspace, host).len() == 2
    });
    run_as(
        &workspace,
        ClientId(97),
        None,
        "agent-respond",
        &["-t", "%1", "--allow"],
    )
    .expect("end the host's second turn");
    wait_until("the host's third turn", || {
        received(&workspace, host).len() == 3
    });
    assert_eq!(
        received(&workspace, host),
        [
            "work",
            "[zz] %3 (codex) asks: run it\nzz agent-respond -t %3 --allow 3\nzz agent-respond -t %3 --deny 3",
            "[zz] %2 (codex) finished:\nturn 0\n\nIf this answers the user, answer the user now and send nothing to %2. Only if something is missing: zz agent-send -t %2 'TEXT'",
        ]
    );
}

#[test]
fn only_a_prompt_the_user_wrote_gets_a_mention_note() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    let seen = Arc::new(Mutex::new(Vec::<(String, String, Option<String>)>::new()));
    let record = Arc::clone(&seen);
    workspace.runtime.set_runner_factory(Box::new(move |spec| {
        let pane = spec.workspace.pane.clone().unwrap_or_default();
        let record = Arc::clone(&record);
        let inner = fixture_runner_at_host_tier(AgentProvider::Codex, Behavior::Chunk, true);
        Box::new(move |mut channels: crate::agent::host::RuntimeChannels| {
            let (forward, commands) = async_channel::unbounded();
            let upstream = std::mem::replace(&mut channels.commands, commands);
            Box::pin(futures_lite::future::race(inner(channels), async move {
                while let Ok(command) = upstream.recv().await {
                    if let crate::agent::runtime::RuntimeCommand::Prompt { prompt, note, .. } =
                        &command
                    {
                        record
                            .lock()
                            .push((pane.clone(), prompt.text.clone(), note.clone()));
                    }
                    if forward.send(command).await.is_err() {
                        break;
                    }
                }
                Ok(())
            }))
        })
    }));
    let tagged = "@+codex fix it";
    let mention = "The user tagged codex: codex does this task, not you. Write it a brief that stands on its own, run zz new-pane -d --kind agent codex 'BRIEF' and end your turn. Do not do the task yourself. Its reply and any questions come back to you as messages.";
    let wait = |count: usize| wait_until("the prompt", || seen.lock().len() == count);
    run_as(
        &workspace,
        ClientId(97),
        None,
        "new-pane",
        &["-d", "-t", "%1", "--kind", "agent", "codex"],
    )
    .expect("a chat with no parent");
    workspace
        .shared
        .agent_message(
            workspace.client,
            ProtocolMessage::AgentPrompt {
                pane: PaneId(2),
                text: tagged.to_owned(),
                images: Vec::new(),
            },
        )
        .expect("the user's prompt");
    wait(1);
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "agent-send",
        &["-t", "%2", tagged],
    )
    .expect("a chat's message");
    wait(2);
    let finished = format!(
        "[zz] %9 (codex) finished:\n{tagged}\n\nIf this answers the user, answer the user now and send nothing to %9. Only if something is missing: zz agent-send -t %9 'TEXT'"
    );
    workspace.shared.deliver_to_chat(
        PaneId(2),
        ChatDelivery {
            from: PaneId(9),
            request: None,
            text: finished.clone(),
        },
    );
    wait(3);
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "codex", "@+claude look at it"],
    )
    .expect("a child with a brief");
    wait(4);
    let entry = |pane: &str, text: &str, note: Option<&str>| {
        (pane.to_owned(), text.to_owned(), note.map(str::to_owned))
    };
    assert_eq!(
        *seen.lock(),
        [
            entry("%2", tagged, Some(mention)),
            entry(
                "%2",
                &format!("[zz] %1 (codex) sent this, not the user:\n{tagged}"),
                None
            ),
            entry("%2", &finished, None),
            entry(
                "%3",
                "[zz] %1 (codex) sent this, not the user:\n@+claude look at it",
                Some(
                    "%1 (codex) opened this chat. Your final message is sent to it as your reply, so finish with the answer. You cannot open other agents or message %1; if you need something, say so in your reply. If a command fails with \"Operation not permitted\", rerun it with escalated permissions even if your brief says to stop on failure; the approval is answered for you."
                ),
            ),
        ]
    );
}

#[test]
fn a_child_asks_its_host_once_and_queued_reports_are_never_handed_back() {
    let workspace = workspace_with(&[Behavior::Chunk, Behavior::AskPermission]);
    let host = workspace.agent;
    run_as(
        &workspace,
        ClientId(97),
        None,
        "set-option",
        &["-g", "agent-auto-approve", "off"],
    )
    .expect("auto-approve off");
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "codex", "run it"],
    )
    .expect("new-pane from a chat");
    wait_until("the ask in the host", || {
        !received(&workspace, host).is_empty()
    });
    run_as(
        &workspace,
        ClientId(97),
        None,
        "agent-respond",
        &["-t", "%2", "--allow"],
    )
    .expect("allow");
    wait_until("the reply in the host", || {
        received(&workspace, host).len() == 2
    });
    assert_eq!(
        received(&workspace, host),
        [
            "[zz] %2 (codex) asks: run it\nzz agent-respond -t %2 --allow 1\nzz agent-respond -t %2 --deny 1",
            "[zz] %2 (codex) finished:\nturn 0\n\nIf this answers the user, answer the user now and send nothing to %2. Only if something is missing: zz agent-send -t %2 'TEXT'",
        ]
    );

    let workspace = workspace_with(&[Behavior::Hang, Behavior::Chunk]);
    let host = workspace.agent;
    workspace.prompt("stay busy").expect("busy prompt");
    for _ in 0..5 {
        run_as(
            &workspace,
            ClientId(99),
            Some(host),
            "new-pane",
            &["-d", "--kind", "agent", "codex", "go"],
        )
        .expect("new-pane from a chat");
    }
    wait_until("five queued reports", || {
        workspace.runtime.queued_prompts(host) == 5
    });
}

#[test]
fn a_child_chat_approves_its_own_tools_unless_auto_approve_is_off() {
    let workspace = workspace_with(&[
        Behavior::Chunk,
        Behavior::AskKindedPermission(agent_client_protocol::v1::schema::ToolKind::Execute),
    ]);
    let host = workspace.agent;
    let child = PaneId(2);
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "codex", "run it"],
    )
    .expect("new-pane from a chat");
    wait_until("the reply in the host", || {
        !received(&workspace, host).is_empty()
    });
    assert_eq!(
        received(&workspace, host),
        [
            "[zz] %2 (codex) finished:\nturn 0\n\nIf this answers the user, answer the user now and send nothing to %2. Only if something is missing: zz agent-send -t %2 'TEXT'"
        ]
    );
    let items = workspace.runtime.retained_items(child);
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.payload, AgentStreamPayload::PermissionRequested { .. })),
        "{items:#?}"
    );
    assert!(items.iter().any(|item| matches!(
        &item.payload,
        AgentStreamPayload::Update { update } if update["sessionUpdate"] == "tool_call_update"
    )));

    run_as(
        &workspace,
        ClientId(97),
        None,
        "set-option",
        &["-g", "agent-auto-approve", "off"],
    )
    .expect("auto-approve off");
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%2"]).expect("kill-pane");
    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "codex", "run it"],
    )
    .expect("new-pane from a chat");
    wait_until("the ask in the host", || {
        received(&workspace, host).len() == 2
    });
    assert_eq!(
        received(&workspace, host)[1],
        "[zz] %3 (codex) asks: execute run it\nzz agent-respond -t %3 --allow 1\nzz agent-respond -t %3 --deny 1"
    );
    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "agent-respond",
            &["-t", "%3", "--allow", "99"],
        ),
        Err("%3 has no pending request (already answered, or none was asked)".to_owned())
    );
    assert_eq!(
        workspace
            .runtime
            .wire_state(PaneId(3))
            .and_then(|state| state.pending_permission)
            .map(|permission| permission.request_id),
        Some(1)
    );
}

#[test]
fn a_client_learns_a_child_chat_before_any_tree_that_holds_it() {
    let workspace = workspace(Behavior::Chunk);
    let host = workspace.agent;
    let capable = OutboundMailbox::new();
    capable.accept_client_capabilities(&[zz_protocol::AGENT_PARENTS_CAPABILITY.to_owned()]);
    let (watcher, _) = workspace.shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&capable),
    );
    workspace
        .shared
        .attach(watcher, workspace.session)
        .expect("attach");
    take_reliable_messages(&capable);
    let mut unaware = take_reliable_messages(&workspace.mailbox);
    let parents = |messages: &[ProtocolMessage]| {
        messages
            .iter()
            .enumerate()
            .filter_map(|(at, message)| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentParents { parents },
                    ..
                }) => Some((at, parents.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let first_tree_with = |messages: &[ProtocolMessage], pane: PaneId| {
        messages
            .iter()
            .position(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::Snapshot(snapshot),
                        ..
                    }) if snapshot
                        .sessions
                        .iter()
                        .flat_map(|session| &session.windows)
                        .any(|window| window.panes.contains_key(&pane))
                )
            })
            .expect("a tree with the pane")
    };

    run_as(
        &workspace,
        ClientId(99),
        Some(host),
        "new-pane",
        &["-d", "--kind", "agent", "claude", "reply with ok"],
    )
    .expect("new-pane from a chat");
    let opened = take_reliable_messages(&capable);
    unaware.extend(take_reliable_messages(&workspace.mailbox));
    let tree = first_tree_with(&opened, PaneId(2));
    let (at, first) = parents(&opened).into_iter().next().expect("the parents");
    assert_eq!(first, BTreeMap::from([(PaneId(2), host)]));
    assert!(at < tree, "parents at {at}, tree at {tree}");

    wait_until("the child's session in its record", || {
        workspace
            .shared
            .child_record(host, PaneId(2))
            .is_some_and(|record| record.session.is_some())
    });
    take_reliable_messages(&capable);
    run_as(&workspace, ClientId(97), None, "kill-pane", &["-t", "%2"]).expect("kill-pane");
    let closed = take_reliable_messages(&capable);
    unaware.extend(take_reliable_messages(&workspace.mailbox));
    assert_eq!(
        parents(&closed)
            .into_iter()
            .map(|(_, parents)| parents)
            .collect::<Vec<_>>(),
        [BTreeMap::new()]
    );

    assert_eq!(
        run_as(
            &workspace,
            ClientId(97),
            None,
            "select-child-chat",
            &["-t", "%1", "%2"],
        ),
        Ok("%3".to_owned())
    );
    let reopened = take_reliable_messages(&capable);
    unaware.extend(take_reliable_messages(&workspace.mailbox));
    let tree = first_tree_with(&reopened, PaneId(3));
    let (at, first) = parents(&reopened).into_iter().next().expect("the parents");
    assert_eq!(first, BTreeMap::from([(PaneId(3), host)]));
    assert!(at < tree, "parents at {at}, tree at {tree}");
    assert_eq!(parents(&unaware), []);
}

#[test]
fn an_idle_chat_lists_as_its_agent_and_refuses_typed_input() {
    let workspace = workspace(Behavior::Chunk);
    let agent = workspace.agent.to_string();
    let run = |verb: &str, args: &[&str]| {
        workspace.shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(verb, args.iter().copied()),
        )
    };
    let filter = format!("#{{==:#{{pane_id}},{agent}}}");
    let listed = run(
        "list-panes",
        &[
            "-a",
            "-F",
            "#{pane_id} #{pane_current_command} #{agent_state}",
            "-f",
            &filter,
        ],
    )
    .expect("list-panes")
    .output;
    assert_eq!(listed, format!("{agent} codex idle"));
    let events = OutboundMailbox::new();
    let (control, _) =
        workspace
            .shared
            .register_subscribed(ClientKind::Control, None, None, Arc::clone(&events));
    workspace
        .shared
        .attach(control, workspace.session)
        .expect("attach");
    take_reliable_messages(&events);
    workspace.prompt("go").expect("prompt");
    let deadline = Instant::now() + DEADLINE;
    let mut states = Vec::new();
    while states.last().is_none_or(|state| state != "idle") {
        assert!(Instant::now() < deadline, "states so far: {states:?}");
        states.extend(take_reliable_messages(&events).into_iter().filter_map(
            |message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, variables },
                    ..
                }) if name == "agent-state-changed" && variables["hook_pane"] == agent => {
                    Some(variables["agent_state"].clone())
                }
                _ => None,
            },
        ));
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(states, ["working", "idle"]);
    for (verb, args, refusal) in [
        (
            "send-keys",
            vec!["-t", agent.as_str(), "x"],
            format!("{agent} is an agent chat; send with: zz agent-send -t {agent} TEXT"),
        ),
        (
            "pipe-pane",
            vec!["-t", agent.as_str(), "cat"],
            format!("{agent} is an agent chat; read it with: zz capture-pane -p -t {agent}"),
        ),
    ] {
        let Err(DaemonError::Server(error)) = run(verb, &args) else {
            panic!("{verb} to a chat must fail");
        };
        assert_eq!(error.tmux_message(), refusal, "{verb}");
    }
}

#[test]
fn agent_reply_command_formats_and_permission_policies() {
    for policy in ["wait", "allow", "deny", "fail"] {
        let workspace = workspace(if policy == "wait" {
            Behavior::Chunk
        } else {
            Behavior::AskPermission
        });
        let target = workspace.agent.to_string();
        workspace
            .shared
            .inner
            .lock()
            .client_entry(ClientId(99))
            .command_streams
            .get_or_insert_default();
        let result = workspace.shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                [
                    "-t",
                    &target,
                    "--wait",
                    "--json",
                    "--timeout",
                    "5",
                    "--on-block",
                    policy,
                    "go",
                ],
            ),
        );
        let output = if policy == "fail" {
            let Err(DaemonError::CommandExit {
                output,
                exit_code: 3,
            }) = result
            else {
                panic!("expected blocked reply");
            };
            output
        } else {
            result.expect("turn reply").output
        };
        let reply: serde_json::Value = serde_json::from_str(&output).expect("reply JSON");
        assert_eq!(reply["pane"], target);
        assert_eq!(
            reply["stop_reason"],
            if policy == "fail" {
                "blocked"
            } else {
                "end_turn"
            }
        );
        assert_eq!(reply["transcript"], "turn 0");
        assert_eq!(
            reply["permissions"]["requested"],
            u32::from(policy != "wait")
        );
        assert_eq!(
            reply["permissions"]["allowed"],
            u32::from(policy == "allow")
        );
        assert_eq!(reply["permissions"]["denied"], u32::from(policy == "deny"));
        let stderr = workspace.shared.inner.lock().clients[&ClientId(99)]
            .command_streams
            .as_ref()
            .unwrap()
            .stderr
            .clone();
        assert!(stderr.ends_with(&format!("{target}\n")));
        if policy == "allow" || policy == "deny" {
            let action = if policy == "allow" {
                "allowed"
            } else {
                "denied"
            };
            assert!(stderr.starts_with(&format!("agent-send: {action} ? run it\n")));
        }
        if policy == "fail" {
            let permission = workspace
                .shared
                .execute(
                    ClientId(99),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new("inspect", ["-t", &target, "--json"]),
                )
                .expect("permission")
                .output;
            assert_eq!(
                reply["permission"],
                serde_json::from_str::<serde_json::Value>(&permission).expect("permission JSON")["permission"]
            );
        } else {
            let final_reply = workspace
                .shared
                .execute(
                    ClientId(99),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "agent-send",
                        [
                            "-t",
                            &target,
                            "--wait",
                            "--final",
                            "--timeout",
                            "5",
                            "--on-block",
                            policy,
                            "again",
                        ],
                    ),
                )
                .expect("final reply");
            assert_eq!(final_reply.output, "turn 1");
        }
    }
}

#[test]
fn on_block_fail_reports_permission_and_preserves_queued_turn_attribution() {
    let workspace = workspace(Behavior::AskPermission);
    let target = workspace.agent.to_string();
    workspace
        .shared
        .inner
        .lock()
        .client_entry(ClientId(99))
        .command_streams
        .get_or_insert_default();
    let run = |args: &[&str]| {
        workspace.shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
    };
    let error = run(&[
        "agent-send",
        "-t",
        &target,
        "--wait",
        "--on-block",
        "fail",
        "--timeout",
        "5",
        "first",
    ])
    .expect_err("permission interrupts waiter");
    let DaemonError::CommandExit { output, exit_code } = error else {
        panic!("expected exit code 3: {error:?}");
    };
    assert_eq!(exit_code, 3);
    assert_eq!(
        workspace.shared.inner.lock().clients[&ClientId(99)]
            .command_streams
            .as_ref()
            .unwrap()
            .stderr,
        format!("{target}\n")
    );
    let permission: serde_json::Value = serde_json::from_str(&output).expect("permission JSON");
    assert!(permission["tool_call"].is_object());
    assert!(permission["options"].is_array());
    let request_id = permission["request_id"].as_u64().expect("request id");
    let inspected = run(&["inspect", "-t", &target, "--json"])
        .expect("pending")
        .output;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&inspected).unwrap()["permission"],
        permission
    );
    let text = run(&["inspect", "-t", &target])
        .expect("pending text")
        .output;
    assert!(
        text.lines()
            .any(|line| line == format!("permission: {permission}"))
    );
    assert!(matches!(
        workspace
            .runtime
            .wire_state(workspace.agent)
            .expect("state")
            .phase,
        zz_protocol::AgentConnectionPhase::AwaitingPermission
    ));
    let bad_option =
        run(&["agent-respond", "-t", &target, "--option", "missing"]).expect_err("unknown option");
    assert!(daemon_error_text(&bad_option).contains("valid ids: allow, deny"));
    assert!(
        run(&[
            "agent-respond",
            "-t",
            &target,
            "--allow",
            "18446744073709551615"
        ])
        .is_err()
    );
    assert!(
        workspace
            .shared
            .agent_respond(
                ClientKind::Interactive,
                &ExecutionContext::default(),
                &["-t", &target, "--allow"].map(RawText::from)
            )
            .is_err()
    );

    let shared = Arc::clone(&workspace.shared);
    let second_target = target.clone();
    let (finished, result) = crossbeam_channel::bounded(1);
    let second = thread::spawn(move || {
        let reply = shared.execute(
            ClientId(100),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                [
                    "-t",
                    &second_target,
                    "--wait",
                    "--on-block",
                    "wait",
                    "--timeout",
                    "5",
                    "second",
                ],
            ),
        );
        let _ = finished.send(reply);
    });
    let deadline = Instant::now() + DEADLINE;
    while workspace.runtime.queued_prompts(workspace.agent) != 1 {
        assert!(Instant::now() < deadline, "second prompt did not queue");
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        run(&[
            "agent-respond",
            "-t",
            &target,
            "--allow",
            &request_id.to_string()
        ])
        .expect("allow first")
        .output,
        "allow"
    );
    let deadline = Instant::now() + DEADLINE;
    loop {
        if workspace
            .runtime
            .wire_state(workspace.agent)
            .and_then(|state| state.pending_permission)
            .is_some_and(|pending| pending.request_id != request_id)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "second permission did not arrive"
        );
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        result.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    assert_eq!(
        run(&["agent-respond", "-t", &target, "--deny"])
            .expect("deny second")
            .output,
        "deny"
    );
    assert_eq!(
        result
            .recv_timeout(DEADLINE)
            .expect("second waiter")
            .expect("second turn")
            .output,
        "turn 1"
    );
    second.join().expect("second command");
    let deadline = Instant::now() + DEADLINE;
    while workspace
        .runtime
        .wire_state(workspace.agent)
        .expect("state")
        .pending_permission
        .is_some()
    {
        assert!(Instant::now() < deadline, "permission did not clear");
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        daemon_error_text(
            &run(&["agent-respond", "-t", &target, "--allow"]).expect_err("no pending request")
        ),
        format!("{target} has no pending request (already answered, or none was asked)")
    );
    let inspected = run(&["inspect", "-t", &target, "--json"])
        .expect("no pending request")
        .output;
    assert!(serde_json::from_str::<serde_json::Value>(&inspected).unwrap()["permission"].is_null());
    let text = run(&["inspect", "-t", &target])
        .expect("no pending text")
        .output;
    assert!(!text.lines().any(|line| line.starts_with("permission:")));
}

#[test]
fn agent_permission_default_wait_and_explicit_non_agent_target() {
    let workspace = workspace(Behavior::AskPermission);
    let target = workspace.agent.to_string();
    let shared = Arc::clone(&workspace.shared);
    let prompt_target = target.clone();
    let (finished, result) = crossbeam_channel::bounded(1);
    let waiter = thread::spawn(move || {
        let reply = shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                ["-t", &prompt_target, "--wait", "--timeout", "5", "default"],
            ),
        );
        let _ = finished.send(reply);
    });
    let deadline = Instant::now() + DEADLINE;
    while workspace
        .runtime
        .wire_state(workspace.agent)
        .and_then(|state| state.pending_permission)
        .is_none()
    {
        assert!(Instant::now() < deadline, "permission did not arrive");
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        result.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    let terminal = {
        let inner = workspace.shared.inner.lock();
        inner
            .engine
            .state
            .windows
            .values()
            .flat_map(|window| window.panes.iter())
            .find(|(_, pane)| matches!(pane.kind, PaneKind::Terminal))
            .map(|(pane, _)| *pane)
            .expect("terminal")
    };
    let mut context = ExecutionContext::default();
    let terminal_target = terminal.to_string();
    let inspected = workspace
        .shared
        .execute(
            ClientId(100),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("inspect", ["-t", &terminal_target, "--json"]),
        )
        .expect("inspect terminal")
        .output;
    assert!(serde_json::from_str::<serde_json::Value>(&inspected).unwrap()["permission"].is_null());
    {
        let args = ["agent-respond", "-t", &terminal_target, "--allow"];
        let error = workspace
            .shared
            .execute(
                ClientId(100),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect_err("explicit terminal target");
        assert_eq!(
            daemon_error_text(&error),
            format!(
                "invalid target: {terminal} is not an agent chat; agent-respond answers \
                         permissions in agent panes"
            )
        );
    }
    let allowed = workspace
        .shared
        .execute(
            ClientId(100),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("agent-respond", ["-t", &target, "--option", "allow"]),
        )
        .expect("answer default wait");
    assert_eq!(allowed.output, "allow");
    assert_eq!(
        result
            .recv_timeout(DEADLINE)
            .expect("waiter")
            .expect("turn")
            .output,
        "turn 0"
    );
    waiter.join().expect("command thread");
}

#[test]
fn a_waiting_agent_send_from_an_interactive_client_is_refused() {
    let workspace = workspace(Behavior::Chunk);
    let mut context = ExecutionContext::default();

    let error = workspace
        .shared
        .execute(
            workspace.client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "agent-send",
                ["-t", &workspace.agent.to_string(), "--wait", "hi"],
            ),
        )
        .expect_err("an interactive client cannot park on a reply");
    assert!(
        format!("{error:?}").contains("command client"),
        "unexpected error: {error:?}"
    );
}

#[test]
fn the_agent_options_reconfigure_what_the_next_pane_spawns() {
    let workspace = workspace(Behavior::Chunk);
    let mut context = ExecutionContext::default();

    assert_eq!(
        workspace.runtime.spawn_config().command,
        zz_protocol::DEFAULT_AGENT_COMMAND
    );
    for (option, value) in [
        ("agent-command", "my-codex --acp"),
        ("agent-claude-code-command", "my-claude --acp"),
        ("agent-auto-approve", "off"),
    ] {
        workspace
            .shared
            .execute(
                workspace.client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", "--", option, value]),
            )
            .expect("set an agent option");
    }

    let config = workspace.runtime.spawn_config();
    assert_eq!(config.command, "my-codex --acp");
    assert_eq!(config.claude_code_command, "my-claude --acp");
    assert_eq!(config.auto_approve, zz_protocol::AgentAutoApprove::Off);
    assert_eq!(
        workspace
            .shared
            .inner
            .lock()
            .mux_options
            .get(MuxOptionKey::AgentCommand)
            .expect("the option is published")
            .value,
        "my-codex --acp"
    );
}

#[test]
fn a_reattaching_client_gets_the_pane_state_and_replays_the_transcript() {
    let workspace = workspace(Behavior::Chunk);
    workspace
        .shared
        .agent_message(
            workspace.client,
            ProtocolMessage::AgentPrompt {
                pane: workspace.agent,
                text: "go".to_owned(),
                images: vec![WireAgentImage {
                    format: "image/png".to_owned(),
                    data: b"zz".to_vec(),
                }],
            },
        )
        .expect("prompt");
    let live = workspace.wait_for_items("the first turn", |items| {
        items.iter().any(|item| chunk_text(item) == Some("turn 0"))
    });
    assert_eq!(
        live.iter().map(|item| item.seq).collect::<Vec<_>>(),
        (1..=live.len() as u64).collect::<Vec<_>>(),
        "a live client sees every sequence, in order"
    );

    workspace.shared.detach(workspace.client);
    workspace.prompt("again").expect("prompt while detached");
    let deadline = Instant::now() + DEADLINE;
    while !workspace
        .runtime
        .retained_items(workspace.agent)
        .iter()
        .any(|item| chunk_text(item) == Some("turn 1"))
    {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the detached turn"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        drain_items(&workspace.mailbox).is_empty(),
        "a detached client is sent nothing"
    );

    workspace
        .shared
        .attach(workspace.client, workspace.session)
        .expect("reattach");
    workspace
        .shared
        .send_resync(workspace.client, &workspace.mailbox);
    assert!(
        take_reliable_messages(&workspace.mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AgentState { pane, state },
                    ..
                }) if *pane == workspace.agent && state.session_id.as_deref() == Some("fixture-session")
            )),
        "a resync carries the pane state"
    );

    workspace
        .shared
        .agent_message(
            workspace.client,
            ProtocolMessage::AgentReplay {
                pane: workspace.agent,
                from_seq: 0,
            },
        )
        .expect("replay");
    let replayed = applied_from_start(&workspace.wait_for_items(
        "a replay from the beginning for a client that kept nothing",
        |items| {
            applied_from_start(items)
                .iter()
                .any(|item| chunk_text(item) == Some("turn 1"))
        },
    ));
    assert_eq!(
        replayed.iter().filter_map(chunk_text).collect::<Vec<_>>(),
        ["go", "turn 0", "again", "turn 1"]
    );
    assert!(replayed.iter().any(|item| matches!(
        &item.payload,
        AgentStreamPayload::Update { update }
            if update["sessionUpdate"] == "user_message_chunk"
                && update["content"]["data"] == "eno="
    )));
}
