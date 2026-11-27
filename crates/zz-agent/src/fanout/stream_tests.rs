use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use serde_json::json;

use super::*;
use crate::{
    fixture::{Behavior, fixture_runner},
    stream::{AgentImage, AgentPromptOutcome, AgentSessionCapabilities, AgentSessionSummary},
};

#[test]
fn only_a_page_marked_user_facing_counts_as_an_agents_page() {
    let write = |user_facing: bool, path: &str| {
        json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_1",
            "status": "completed",
            "kind": "edit",
            "locations": [{ "path": path }],
            "rawInput": {
                "ArtifactMetadata": { "RequestFeedback": false, "UserFacing": user_facing },
                "Overwrite": true,
                "Path": path,
            },
        })
    };
    let page = "/home/me/.gemini/antigravity-cli/brain/s1/button-comparison.html";
    assert_eq!(user_facing_page(&write(true, page)).as_deref(), Some(page));
    assert_eq!(user_facing_page(&write(false, page)), None);
    assert_eq!(
        user_facing_page(&write(true, "/home/me/brain/s1/plan.md")),
        None
    );
    assert_eq!(
        user_facing_page(&json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_2",
            "kind": "edit",
            "locations": [{ "path": "/work/site/index.html" }],
            "rawInput": { "Path": "/work/site/index.html" },
        })),
        None
    );
}

const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Recorder {
    broadcast: Mutex<Vec<(PaneId, u64, Vec<Vec<u8>>, Option<ClientId>)>>,
    direct: Mutex<Vec<(ClientId, PaneId, u64, Vec<Vec<u8>>)>>,
    answered: Mutex<Vec<(ClientId, PaneId, usize)>>,
    states: Mutex<Vec<(PaneId, AgentPaneWire)>>,
    replies: Mutex<Vec<(ClientId, PaneId, u64, String)>>,
    update_gate: Mutex<Option<(Sender<()>, Receiver<()>)>>,
}

impl Recorder {
    fn delivered(&self) -> usize {
        let broadcast = self
            .broadcast
            .lock()
            .iter()
            .map(|(_, _, items, _)| items.len())
            .sum::<usize>();
        let direct = self
            .direct
            .lock()
            .iter()
            .map(|(_, _, _, items)| items.len())
            .sum::<usize>();
        broadcast + direct
    }

    fn payloads(&self) -> Vec<AgentStreamItem> {
        let mut items = self
            .broadcast
            .lock()
            .iter()
            .flat_map(|(_, _, items, _)| items.clone())
            .collect::<Vec<_>>();
        items.extend(
            self.direct
                .lock()
                .iter()
                .flat_map(|(_, _, _, items)| items.clone()),
        );
        items
            .iter()
            .map(|item| serde_json::from_slice(item).expect("decode stream item"))
            .collect()
    }

    fn wait_for_items(&self, count: usize) -> Vec<AgentStreamItem> {
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if self.delivered() >= count {
                return self.payloads();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!(
            "timed out waiting for {count} items, saw {}",
            self.delivered()
        );
    }
}

impl AgentPublisher for Recorder {
    fn publish_agent_updates(
        &self,
        _generation: u64,
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
        also: Option<ClientId>,
    ) {
        if let Some((entered, release)) = self.update_gate.lock().take() {
            let _ = entered.send(());
            let _ = release.recv();
        }
        self.broadcast.lock().push((pane, first_seq, items, also));
    }

    fn send_agent_replay(
        &self,
        _generation: u64,
        client: ClientId,
        pane: PaneId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
    ) {
        self.answered.lock().push((client, pane, frames.len()));
        self.direct.lock().extend(
            frames
                .into_iter()
                .map(|(first_seq, items)| (client, pane, first_seq, items)),
        );
    }

    fn publish_agent_state(&self, _generation: u64, pane: PaneId, state: AgentPaneWire) {
        self.states.lock().push((pane, state));
    }

    fn publish_agent_folder(&self, _generation: u64, _pane: PaneId, _cwd: PathBuf) {}

    fn publish_agent_quota(&self, _generation: u64, _pane: PaneId, _quota: QuotaWindows) {}

    fn publish_agent_tool_call(&self, _generation: u64, _pane: PaneId, _call: AgentToolCall) {}

    fn send_agent_reply(&self, _generation: u64, pane: PaneId, reply: AgentRequestReply) {
        let AgentRequestReply::Sessions { client, result } = reply;
        let request_id = 0;
        self.replies.lock().push((client, pane, request_id, result));
    }
}

struct Fixture {
    fanout: Arc<AgentFanout>,
    recorder: Arc<Recorder>,
    pane: PaneId,
}

impl Fixture {
    fn open(journal: Option<Arc<AgentJournal>>, session_id: Option<&str>) -> Self {
        let recorder = Arc::new(Recorder::default());
        let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
        let fanout = Arc::new(AgentFanout::new(
            &publisher,
            journal,
            Arc::new(Mutex::new(())),
        ));
        let pane = PaneId(4);
        fanout.open_lane(
            pane,
            1,
            AgentProvider::Codex,
            session_id.map(ToOwned::to_owned),
        );
        Self {
            fanout,
            recorder,
            pane,
        }
    }

    fn accept(&self, payload: AgentStreamPayload) {
        self.accept_generation(1, payload);
    }

    fn accept_generation(&self, generation: u64, payload: AgentStreamPayload) {
        self.fanout.accept(
            self.pane,
            generation,
            &state(AgentConnectionPhase::Running),
            Some(AgentStreamItem { seq: 0, payload }),
        );
    }

    fn chunk(&self, text: &str) {
        self.accept(AgentStreamPayload::Update {
            update: json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": text } }),
        });
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.fanout.shutdown();
    }
}

fn state(phase: AgentConnectionPhase) -> AgentPaneState {
    let mut state = AgentPaneState::for_test();
    state.phase = phase;
    state
}

fn text_of(item: &AgentStreamItem) -> Option<String> {
    let AgentStreamPayload::Update { update } = &item.payload else {
        return None;
    };
    update
        .get("content")?
        .get("text")?
        .as_str()
        .map(ToOwned::to_owned)
}

#[test]
fn a_window_of_items_leaves_as_one_frame_numbered_from_the_first() {
    let fixture = Fixture::open(None, Some("s-1"));
    for text in ["a", "b", "c"] {
        fixture.chunk(text);
    }
    let items = fixture.recorder.wait_for_items(3);

    assert_eq!(
        items.iter().map(|item| item.seq).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        items.iter().filter_map(text_of).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    let broadcast = fixture.recorder.broadcast.lock();
    assert_eq!(broadcast.len(), 1, "one window, one frame: {broadcast:?}");
    assert_eq!(broadcast[0].1, 1);
}

#[test]
fn a_flush_settles_and_drains_before_replay_admission() {
    let fixture = Fixture::open(None, Some("s-1"));
    let (entered_send, entered_recv) = mpsc::channel();
    let (release_send, release_recv) = mpsc::channel();
    *fixture.recorder.update_gate.lock() = Some((entered_send, release_recv));

    fixture.chunk("a");
    entered_recv
        .recv_timeout(DEADLINE)
        .expect("flusher should enter publication");
    assert!(fixture.fanout.lanes.try_lock().is_none());

    release_send.send(()).expect("release flusher");
    let deadline = Instant::now() + DEADLINE;
    let settled = loop {
        if fixture.fanout.lanes.try_lock().is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::yield_now();
    };
    assert!(settled, "flusher should release the lane");
    fixture.chunk("b");
    assert!(
        fixture
            .fanout
            .lanes
            .lock()
            .get(&fixture.pane)
            .is_some_and(|lane| !lane.batch.is_empty())
    );
    fixture.fanout.replay(ClientId(8), fixture.pane, 0);
    assert!(
        fixture
            .fanout
            .lanes
            .lock()
            .get(&fixture.pane)
            .is_some_and(|lane| lane.batch.is_empty())
    );
    assert_eq!(fixture.recorder.direct.lock().len(), 1);
}

#[test]
fn an_unflushed_batch_stays_inside_its_byte_bound() {
    let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
    let item = vec![b'x'; MAX_AGENT_RESULT_BYTES];
    for seq in 1..=32 {
        lane.push(seq, item.clone());
    }
    assert!(lane.batch_bytes <= MAX_AGENT_UPDATES_BYTES);
    assert!(lane.ring_bytes <= MAX_REPLAY_RING_BYTES);
    assert!(lane.batch.len() <= MAX_AGENT_UPDATES_BYTES / MAX_AGENT_RESULT_BYTES);
}

#[test]
fn zero_byte_reclaimed_prompts_stay_inside_the_count_bound() {
    let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
    for _ in 0..MAX_RECLAIMED_PROMPTS + 8 {
        lane.enqueue_reclaimed(
            PaneId(1),
            vec![AgentPrompt {
                owner: ClientInstanceId::default(),
                text: String::new(),
                images: vec![AgentImage {
                    format: "image/png".to_owned(),
                    data: Vec::new(),
                }],
            }],
        );
    }

    assert_eq!(lane.reclaimed.len(), MAX_RECLAIMED_PROMPTS);
    assert_eq!(lane.reclaimed_bytes, 0);
}

#[test]
fn only_the_prompt_owner_can_retire_a_restored_prompt() {
    let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
    lane.enqueue_reclaimed(
        PaneId(1),
        vec![AgentPrompt {
            owner: ClientInstanceId(7),
            text: "keep me".to_owned(),
            images: Vec::new(),
        }],
    );
    let reclaim_id = lane.reclaimed[0].reclaim_id;
    let last_seq = lane.reclaimed[0].last_seq;

    lane.acknowledge_reclaimed(ClientInstanceId(8), reclaim_id);
    assert_eq!(lane.reclaimed.len(), 1);

    lane.acknowledge_reclaimed(ClientInstanceId(7), reclaim_id);
    assert!(lane.reclaimed.is_empty());
    assert_eq!(lane.reclaimed_bytes, 0);
    assert!(lane.evicted_seq >= last_seq);
    assert!(lane.ring.iter().all(|(seq, _)| *seq > last_seq));
}

#[test]
fn a_replay_inside_the_ring_serves_only_what_the_client_missed() {
    let fixture = Fixture::open(None, Some("s-1"));
    for text in ["a", "b", "c"] {
        fixture.chunk(text);
    }
    fixture.recorder.wait_for_items(3);

    fixture.fanout.replay(ClientId(9), fixture.pane, 2);
    let direct = fixture.recorder.direct.lock().clone();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].0, ClientId(9));
    assert_eq!(direct[0].2, 2, "the replay starts where the client stopped");
    let items = direct[0]
        .3
        .iter()
        .map(|item| serde_json::from_slice::<AgentStreamItem>(item).expect("decode"))
        .collect::<Vec<_>>();
    assert_eq!(
        items.iter().filter_map(text_of).collect::<Vec<_>>(),
        ["b", "c"]
    );

    fixture.fanout.replay(ClientId(9), fixture.pane, 9);
    assert_eq!(
        fixture.recorder.direct.lock().len(),
        1,
        "a client already ahead of the ring is sent no updates"
    );
    assert_eq!(
        fixture.recorder.answered.lock().last(),
        Some(&(ClientId(9), fixture.pane, 0)),
        "but its replay is still answered, so it knows it has caught up"
    );
}

#[test]
fn a_replay_older_than_the_ring_resets_and_replays_the_journal() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    for text in ["one", "two"] {
        journal
            .append(
                "s-1",
                &json!({
                    "sessionUpdate": "agent_message_chunk",
                    "messageId": text,
                    "content": { "text": text }
                }),
            )
            .expect("append");
    }
    let fixture = Fixture::open(Some(journal), Some("s-1"));
    fixture.accept(AgentStreamPayload::Ready {
        agent_name: "Codex".to_owned(),
        agent_key: "codex".to_owned(),
        auth_methods: Vec::new(),
        capabilities: AgentSessionCapabilities::default(),
    });
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: Some(json!({ "currentModeId": "plan", "availableModes": [] })),
        config_options: None,
    });
    fixture.recorder.wait_for_items(2);

    let filler = "x".repeat(MAX_AGENT_UPDATES_BYTES - 4096);
    let filler_count = MAX_REPLAY_RING_BYTES / filler.len() + 2;
    for _ in 0..filler_count {
        fixture.chunk(&filler);
    }
    let last_live = fixture.fanout.lanes.lock()[&fixture.pane]
        .next_seq
        .saturating_sub(1);

    fixture.fanout.replay(ClientId(3), fixture.pane, 0);
    let replayed = direct_items(&fixture.recorder)
        .into_iter()
        .map(|(client, _, item)| {
            assert_eq!(client, ClientId(3));
            item
        })
        .collect::<Vec<_>>();

    assert!(
        matches!(
            replayed.first().map(|item| &item.payload),
            Some(AgentStreamPayload::SessionReset { restoring: true })
        ),
        "the fallback starts with an explicit stream reset: {replayed:?}"
    );
    assert!(matches!(
        replayed.get(1).map(|item| &item.payload),
        Some(AgentStreamPayload::Ready { agent_name, .. }) if agent_name == "Codex"
    ));
    assert!(matches!(
        replayed.get(replayed.len().saturating_sub(2)).map(|item| &item.payload),
        Some(AgentStreamPayload::SessionReady {
            session_id,
            modes: Some(_),
            ..
        }) if session_id == "s-1"
    ));
    assert!(matches!(
        replayed.last().map(|item| &item.payload),
        Some(AgentStreamPayload::StateSynced { .. })
    ));
    assert_eq!(
        replayed.iter().filter_map(text_of).collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert_eq!(
        replayed.iter().map(|item| item.seq).collect::<Vec<_>>(),
        (last_live - 5..=last_live).collect::<Vec<_>>(),
        "the replay takes the numbers the asker skips and mints none"
    );
}

#[test]
fn a_journal_too_large_to_send_rebuilds_the_asker_from_memory() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let body = "x".repeat(850 * 1024);
    for index in 0..12 {
        journal
            .append(
                "s-large",
                &json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "text": format!("{index}:{body}") }
                }),
            )
            .expect("append");
    }
    let fixture = Fixture::open(Some(journal), Some("s-large"));
    fixture.accept(AgentStreamPayload::Ready {
        agent_name: "Codex".to_owned(),
        agent_key: "codex".to_owned(),
        auth_methods: Vec::new(),
        capabilities: AgentSessionCapabilities::default(),
    });
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-large".to_owned(),
        modes: None,
        config_options: None,
    });
    for text in ["a", "b", "c"] {
        fixture.chunk(text);
    }
    fixture.recorder.wait_for_items(5);
    fixture
        .fanout
        .lanes
        .lock()
        .get_mut(&fixture.pane)
        .expect("lane")
        .evicted_seq = 2;
    fixture.recorder.broadcast.lock().clear();

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);
    let kind = |payload: AgentStreamPayload| match payload {
        AgentStreamPayload::SessionReset { restoring: true } => "reset",
        AgentStreamPayload::Ready { .. } => "ready",
        AgentStreamPayload::SessionReady { .. } => "session",
        AgentStreamPayload::StateSynced { .. } => "state",
        AgentStreamPayload::Chat {
            event: ChatEvent::Restart,
        } => "restart",
        AgentStreamPayload::Chat {
            event: ChatEvent::QueueChanged { prompts },
        } if prompts.is_empty() => "empty queue",
        _ => "other",
    };
    assert_eq!(
        direct_items(&fixture.recorder)
            .into_iter()
            .map(|(client, seq, item)| (client, seq, kind(item.payload)))
            .collect::<Vec<_>>(),
        [
            (ClientId(3), 2, "reset"),
            (ClientId(3), 3, "ready"),
            (ClientId(3), 4, "session"),
            (ClientId(3), 5, "state"),
            (ClientId(3), 6, "restart"),
        ],
        "past the rebuild cap the journal stays out and the asker is rebuilt from memory"
    );
    fixture.recorder.wait_for_items(6);
    assert_eq!(
        fixture
            .recorder
            .broadcast
            .lock()
            .iter()
            .flat_map(|(_, _, items, _)| items)
            .map(|item| {
                let item: AgentStreamItem = serde_json::from_slice(item).expect("decode");
                (item.seq, kind(item.payload))
            })
            .collect::<Vec<_>>(),
        [(6, "empty queue")],
        "the other viewers get only the number the rebuild needed, as a no-op"
    );
}

#[test]
fn a_replay_older_than_the_ring_still_rebuilds_the_whole_chat() {
    use zz_protocol::chat::{ChatView, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId};
    let fixture = Fixture::open(None, Some("s-1"));
    let events = vec![
        ChatEvent::SessionReady {
            session: "s-1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::User { prompt: 1 },
            at: 1,
        },
        ChatEvent::ItemStarted {
            item: ItemId::new("a"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Agent,
            body: ItemBody::text("hel"),
            at: 2,
        },
        ChatEvent::ItemAppended {
            item: ItemId::new("a"),
            text: "lo".to_owned(),
        },
        ChatEvent::ItemEnded {
            item: ItemId::new("a"),
            status: ItemStatus::Completed,
            at: 3,
        },
    ];
    let mut expected = ChatView::default();
    for event in &events {
        expected.apply(event).expect("valid");
        fixture.chunk("update");
        fixture.accept(AgentStreamPayload::Chat {
            event: event.clone(),
        });
    }
    fixture.recorder.wait_for_items(2 * events.len());
    {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
        while lane.ring.front().is_some_and(|(seq, _)| *seq <= 6) {
            let (seq, dropped) = lane.ring.pop_front().expect("ring item");
            lane.ring_bytes -= dropped.len();
            lane.evicted_seq = seq;
        }
    }

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);

    let mut rebuilt = ChatView::default();
    for item in fixture
        .recorder
        .direct
        .lock()
        .iter()
        .filter(|(client, ..)| *client == ClientId(3))
        .flat_map(|(_, _, _, items)| items.clone())
    {
        let item = serde_json::from_slice::<AgentStreamItem>(&item).expect("decode");
        if let AgentStreamPayload::Chat { event } = item.payload {
            let _ = rebuilt.apply(&event);
        }
    }
    rebuilt.skipped = 0;
    assert_eq!(rebuilt, expected);
}

#[test]
fn a_long_chat_the_journal_cannot_vouch_for_is_rebuilt_for_the_asker_alone() {
    use zz_protocol::chat::{
        ChatPhase, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, SessionView, TurnBy, TurnId,
        TurnOutcome,
    };

    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let refused = (0..10_000).any(|index| {
        journal
            .append(
                "s-1",
                &json!({
                    "sessionUpdate": "agent_message_chunk",
                    "messageId": index.to_string(),
                    "content": { "text": "x" }
                }),
            )
            .is_err()
    });
    assert!(refused, "the journal should hit its record cap");
    let fixture = Fixture::open(Some(journal), Some("s-1"));
    fixture.accept(AgentStreamPayload::Ready {
        agent_name: "Codex".to_owned(),
        agent_key: "codex".to_owned(),
        auth_methods: Vec::new(),
        capabilities: AgentSessionCapabilities::default(),
    });
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: None,
        config_options: None,
    });
    let item = |id: &str, turn: u64, text: String| ChatEvent::ItemStarted {
        item: ItemId::new(id),
        turn: Some(TurnId(turn)),
        parent: None,
        kind: ItemKind::Agent,
        body: ItemBody::text(text),
        at: 0,
    };
    let ended = |id: &str| ChatEvent::ItemEnded {
        item: ItemId::new(id),
        status: ItemStatus::Completed,
        at: 0,
    };
    let started = |turn: u64| ChatEvent::TurnStarted {
        turn: TurnId(turn),
        by: TurnBy::Agent,
        at: 0,
    };
    let mut events = vec![ChatEvent::SessionReady {
        session: "s-1".to_owned(),
    }];
    for turn in 1..=24 {
        let id = format!("a{turn}");
        events.extend([
            started(turn),
            item(&id, turn, "x".repeat(1024 * 1024)),
            ended(&id),
            ChatEvent::TurnEnded {
                turn: TurnId(turn),
                outcome: TurnOutcome::Completed {
                    stop_reason: "end_turn".to_owned(),
                },
                at: 0,
            },
        ]);
    }
    events.extend([
        started(25),
        item("b1", 25, "y".repeat(3 * 1024 * 1024)),
        ended("b1"),
        item("b2", 25, "z".repeat(3 * 1024 * 1024)),
        ended("b2"),
        item("b3", 25, "now".to_owned()),
    ]);
    for event in events {
        fixture.accept(AgentStreamPayload::Chat { event });
    }
    let deadline = Instant::now() + DEADLINE;
    while fixture.fanout.lanes.lock()[&fixture.pane]
        .deadline
        .is_some()
    {
        assert!(Instant::now() < deadline, "the live batch flushes");
        std::thread::sleep(Duration::from_millis(2));
    }
    let (next_seq, ring) = {
        let lanes = fixture.fanout.lanes.lock();
        let lane = &lanes[&fixture.pane];
        assert!(lane.evicted_seq > 1, "the ring no longer holds the start");
        (lane.next_seq, lane.ring.len())
    };
    let broadcast_before = fixture.recorder.broadcast.lock().len();

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);

    {
        let lanes = fixture.fanout.lanes.lock();
        let lane = &lanes[&fixture.pane];
        assert!(lane.batch.is_empty() && lane.deadline.is_none());
        assert_eq!((lane.next_seq, lane.ring.len()), (next_seq, ring));
    }
    assert_eq!(
        fixture.recorder.broadcast.lock().len(),
        broadcast_before,
        "no viewer that did not ask gets anything"
    );
    let direct = fixture.recorder.direct.lock().clone();
    let bytes = direct
        .iter()
        .flat_map(|(_, _, _, items)| items.iter().map(Vec::len))
        .sum::<usize>();
    assert!(bytes <= MAX_REBUILD_BYTES, "{bytes} bytes");
    let replayed = direct_items(&fixture.recorder);
    assert!(replayed.iter().all(|(client, ..)| *client == ClientId(3)));
    assert_eq!(
        replayed.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
        (next_seq - 11..next_seq).collect::<Vec<_>>(),
        "the rebuild ends on the lane's last number and mints none"
    );
    assert!(matches!(
        replayed[0].2.payload,
        AgentStreamPayload::SessionReset { restoring: true }
    ));
    let mut rebuilt = ChatView::default();
    for (_, _, item) in replayed {
        if let AgentStreamPayload::Chat { event } = item.payload {
            let _ = rebuilt.apply(&event);
        }
    }
    assert_eq!(
        rebuilt.session,
        SessionView::Ready {
            session: "s-1".to_owned()
        }
    );
    assert_eq!(
        rebuilt.turns.iter().map(|turn| turn.id).collect::<Vec<_>>(),
        [TurnId(25)]
    );
    assert_eq!(
        rebuilt.turns[0].items,
        [ItemId::new("b2"), ItemId::new("b3")]
    );
    assert_eq!(rebuilt.items[&ItemId::new("b3")].body.text, "now");
    assert_eq!(rebuilt.phase(), ChatPhase::Running);
    assert_eq!(
        rebuilt.skipped, 1,
        "the end of b1, whose start was left out"
    );

    let asked = next_seq - 9;
    {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
        while lane.ring.front().is_some_and(|(seq, _)| *seq <= asked + 1) {
            let (seq, dropped) = lane.ring.pop_front().expect("ring item");
            lane.ring_bytes -= dropped.len();
            lane.evicted_seq = seq;
        }
    }
    fixture.fanout.replay(ClientId(4), fixture.pane, asked);
    let tight = direct_items(&fixture.recorder)
        .into_iter()
        .filter(|(client, ..)| *client == ClientId(4))
        .collect::<Vec<_>>();
    assert!(matches!(
        tight[0].2.payload,
        AgentStreamPayload::SessionReset { restoring: true }
    ));
    assert!(
        tight[0].1 > asked,
        "the reset at {} lands past {asked}, the last number the client holds",
        tight[0].1
    );
    assert_eq!(tight.last().map(|(_, seq, _)| *seq), Some(next_seq - 1));
}

#[test]
fn a_capped_rebuild_keeps_an_old_approval_and_a_running_background_item() {
    use zz_protocol::chat::{
        Ask, ChatPhase, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, RequestId, TurnBy,
        TurnId, TurnOutcome,
    };

    let fixture = Fixture::open(None, Some("s-1"));
    let watcher = ItemId::new("watcher");
    let turn = |id: u64, item: &str, text: String| {
        let item = ItemId::new(item);
        [
            ChatEvent::TurnStarted {
                turn: TurnId(id),
                by: TurnBy::Agent,
                at: 0,
            },
            ChatEvent::ItemStarted {
                item: item.clone(),
                turn: Some(TurnId(id)),
                parent: None,
                kind: ItemKind::Agent,
                body: ItemBody::text(text),
                at: 0,
            },
            ChatEvent::ItemEnded {
                item,
                status: ItemStatus::Completed,
                at: 0,
            },
            ChatEvent::TurnEnded {
                turn: TurnId(id),
                outcome: TurnOutcome::Completed {
                    stop_reason: "end_turn".to_owned(),
                },
                at: 0,
            },
        ]
    };
    let mut events = vec![
        ChatEvent::SessionReady {
            session: "s-1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::Agent,
            at: 0,
        },
        ChatEvent::ItemStarted {
            item: watcher.clone(),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Execute,
            body: ItemBody::titled("tail -f build.log"),
            at: 0,
        },
        ChatEvent::ItemBackgrounded {
            item: watcher.clone(),
        },
        ChatEvent::RequestOpened {
            request: RequestId(7),
            turn: None,
            item: Some(watcher.clone()),
            ask: Ask::Approval {
                title: "rm -rf build".to_owned(),
                detail: None,
                options: Vec::new(),
            },
            blocking: true,
            at: 0,
        },
        ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome: TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: 0,
        },
    ];
    for id in 2..=9 {
        events.extend(turn(id, &format!("a{id}"), "x".repeat(1024 * 1024)));
    }
    let mut live = ChatView::default();
    for event in events {
        live.apply(&event).expect("live event");
        fixture.accept(AgentStreamPayload::Chat { event });
    }
    assert_eq!(live.phase(), ChatPhase::Waiting);
    fixture
        .fanout
        .lanes
        .lock()
        .get_mut(&fixture.pane)
        .expect("lane")
        .evicted_seq = 2;

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);

    let replayed = direct_items(&fixture.recorder);
    let bytes = fixture
        .recorder
        .direct
        .lock()
        .iter()
        .flat_map(|(_, _, _, items)| items.iter().map(Vec::len))
        .sum::<usize>();
    assert!(bytes <= MAX_REBUILD_BYTES, "{bytes} bytes");
    let mut rebuilt = ChatView::default();
    for (_, _, item) in replayed {
        if let AgentStreamPayload::Chat { event } = item.payload {
            let _ = rebuilt.apply(&event);
        }
    }
    assert_eq!(rebuilt.phase(), ChatPhase::Waiting);
    assert_eq!(rebuilt.item(&watcher), live.item(&watcher));
    assert_eq!(rebuilt.request(RequestId(7)), live.request(RequestId(7)));
    assert_eq!(
        rebuilt
            .turns
            .iter()
            .map(|turn| turn.id.0)
            .collect::<Vec<_>>(),
        [1, 6, 7, 8, 9]
    );
    assert_eq!(
        rebuilt.skipped, 2,
        "the end of a5 and of its turn, whose starts were left out"
    );
}

fn label(item: &AgentStreamItem) -> String {
    match &item.payload {
        AgentStreamPayload::SessionReset { restoring: true } => "reset".to_owned(),
        AgentStreamPayload::Chat {
            event: ChatEvent::Restart,
        } => "restart".to_owned(),
        _ => text_of(item).unwrap_or_default(),
    }
}

fn direct_items(recorder: &Recorder) -> Vec<(ClientId, u64, AgentStreamItem)> {
    recorder
        .direct
        .lock()
        .iter()
        .flat_map(|(client, _, _, items)| {
            items.iter().map(move |item| {
                let item: AgentStreamItem = serde_json::from_slice(item).expect("decode item");
                (*client, item.seq, item)
            })
        })
        .collect()
}

#[test]
fn a_reopened_chat_continues_its_numbers_and_replays_only_to_the_asker() {
    let fixture = Fixture::open(None, Some("s-1"));
    for text in ["a", "b", "c", "d", "e"] {
        fixture.chunk(text);
    }
    fixture.recorder.wait_for_items(5);
    fixture.fanout.close_lane(fixture.pane);
    fixture.fanout.open_lane(
        fixture.pane,
        2,
        AgentProvider::Codex,
        Some("s-1".to_owned()),
    );
    for text in ["f", "g", "h"] {
        fixture.accept_generation(
            2,
            AgentStreamPayload::Update {
                update: json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": text } }),
            },
        );
    }
    let items = fixture.recorder.wait_for_items(10);
    assert_eq!(
        items.iter().map(|item| item.seq).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    let broadcast = fixture.recorder.broadcast.lock().len();

    fixture.fanout.replay(ClientId(9), fixture.pane, 0);
    let direct = direct_items(&fixture.recorder)
        .into_iter()
        .map(|(client, seq, item)| (client, seq, label(&item)))
        .collect::<Vec<_>>();
    assert_eq!(
        direct,
        [
            (ClientId(9), 5, "reset".to_owned()),
            (ClientId(9), 6, "reset".to_owned()),
            (ClientId(9), 7, "restart".to_owned()),
            (ClientId(9), 8, "f".to_owned()),
            (ClientId(9), 9, "g".to_owned()),
            (ClientId(9), 10, "h".to_owned()),
        ]
    );
    assert_eq!(fixture.recorder.broadcast.lock().len(), broadcast);
}

#[test]
fn a_client_kept_across_a_reopen_shows_the_resumed_chat_once() {
    use zz_protocol::chat::{
        ChatView, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId, TurnOutcome,
    };

    let turn = |id: u64, item: &str, text: &str| {
        let item = ItemId::new(item);
        [
            ChatEvent::TurnStarted {
                turn: TurnId(id),
                by: TurnBy::Agent,
                at: 0,
            },
            ChatEvent::ItemStarted {
                item: item.clone(),
                turn: Some(TurnId(id)),
                parent: None,
                kind: ItemKind::Agent,
                body: ItemBody::default(),
                at: 0,
            },
            ChatEvent::ItemAppended {
                item: item.clone(),
                text: text.to_owned(),
            },
            ChatEvent::ItemEnded {
                item,
                status: ItemStatus::Completed,
                at: 0,
            },
            ChatEvent::TurnEnded {
                turn: TurnId(id),
                outcome: TurnOutcome::Completed {
                    stop_reason: "end_turn".to_owned(),
                },
                at: 0,
            },
        ]
    };
    let fixture = Fixture::open(None, Some("s-1"));
    for event in turn(7, "agent:live", "before the flip") {
        fixture.accept(AgentStreamPayload::Chat { event });
    }
    let mut view = ChatView::default();
    for item in fixture.recorder.wait_for_items(5) {
        if let AgentStreamPayload::Chat { event } = item.payload {
            let _ = view.apply(&event);
        }
    }
    assert_eq!(view.turns.len(), 1);

    fixture.fanout.close_lane(fixture.pane);
    fixture.fanout.open_lane(
        fixture.pane,
        2,
        AgentProvider::Codex,
        Some("s-1".to_owned()),
    );
    for event in turn(1, "agent:resumed", "the resumed history") {
        fixture.accept_generation(2, AgentStreamPayload::Chat { event });
    }
    fixture.recorder.wait_for_items(12);
    fixture.fanout.replay(ClientId(9), fixture.pane, 6);
    let direct = direct_items(&fixture.recorder);
    assert_eq!(
        direct.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
        [6, 7, 8, 9, 10, 11, 12]
    );
    for (_, _, item) in direct {
        if let AgentStreamPayload::Chat { event } = item.payload {
            let _ = view.apply(&event);
        }
    }
    assert_eq!(
        view.turns.iter().map(|turn| turn.id).collect::<Vec<_>>(),
        [TurnId(1)]
    );
    assert_eq!(view.items.len(), 1);
    assert_eq!(
        view.items[&ItemId::new("agent:resumed")].body.text,
        "the resumed history"
    );
    assert_eq!(view.skipped, 0);
}

#[test]
fn opening_an_existing_pane_leaves_its_live_lane_intact() {
    let recorder = Arc::new(Recorder::default());
    let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
    let runtime = AgentRuntime::new(&publisher, AgentSpawnConfig::default(), None);
    runtime.set_runner_factory(Box::new(|spec| {
        fixture_runner(
            spec.provider,
            Behavior::Chunk,
            zz_protocol::AgentAutoApprove::Off,
            true,
        )
    }));
    let pane = PaneId(45);
    let spec = AgentPaneSpec {
        model: None,
        effort: None,
        provider: AgentProvider::Codex,
        cwd: PathBuf::from("/"),
        resume_session: None,
        auto_approve: None,
        workspace: crate::environment::AgentWorkspaceEnvironment::default(),
    };

    assert!(runtime.open(pane, spec.clone()));
    let generation = runtime.fanout.lanes.lock()[&pane].generation;
    assert!(runtime.open(pane, spec));
    assert_eq!(runtime.host.pane_count(), 1);
    assert_eq!(runtime.fanout.lanes.lock()[&pane].generation, generation);
    assert!(runtime.prompt(
        pane,
        AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "probe".to_owned(),
            images: Vec::new(),
        },
    ));
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        if recorder
            .payloads()
            .iter()
            .filter_map(text_of)
            .any(|text| text == "turn 0")
        {
            runtime.shutdown();
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("the original lane stopped delivering after a duplicate open");
}

#[test]
fn closing_a_busy_chat_hands_its_queue_back_to_the_composer() {
    let recorder = Arc::new(Recorder::default());
    let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
    let runtime = AgentRuntime::new(&publisher, AgentSpawnConfig::default(), None);
    runtime.set_runner_factory(Box::new(|spec| {
        fixture_runner(
            spec.provider,
            Behavior::Hang,
            zz_protocol::AgentAutoApprove::Off,
            true,
        )
    }));
    let pane = PaneId(46);
    let spec = AgentPaneSpec {
        model: None,
        effort: None,
        provider: AgentProvider::Codex,
        cwd: PathBuf::from("/"),
        resume_session: None,
        auto_approve: None,
        workspace: crate::environment::AgentWorkspaceEnvironment::default(),
    };
    let prompt = |text: &str| AgentPrompt {
        owner: ClientInstanceId(7),
        text: text.to_owned(),
        images: Vec::new(),
    };
    let deadline = Instant::now() + DEADLINE;
    let wait = |done: &dyn Fn(&AgentPaneState) -> bool, what: &str| {
        while !runtime
            .host
            .snapshot_state(pane)
            .is_some_and(|state| done(&state))
        {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    for (running, waiting) in [("running", "waiting"), ("running again", "waiting again")] {
        assert!(runtime.open(pane, spec.clone()));
        wait(&|state| state.session_id.is_some(), "the chat never opened");
        assert!(runtime.prompt(pane, prompt(running)));
        wait(
            &|state| state.phase == AgentConnectionPhase::Running,
            "the turn never ran",
        );
        assert!(runtime.prompt(pane, prompt(waiting)));
        wait(
            &|state| state.queued_prompts == 1,
            "the prompt never queued",
        );
        runtime.close_and_wait(pane);
    }

    let restored = recorder
        .payloads()
        .into_iter()
        .filter_map(|item| match item.payload {
            AgentStreamPayload::PromptsRestored {
                reclaim_id,
                prompts,
            } => Some((reclaim_id, prompts[0].owner, prompts[0].text.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        restored,
        [
            (1, ClientInstanceId(7), "running".to_owned()),
            (2, ClientInstanceId(7), "waiting".to_owned()),
            (3, ClientInstanceId(7), "running again".to_owned()),
            (4, ClientInstanceId(7), "waiting again".to_owned()),
        ]
    );
    runtime.shutdown();
}

#[test]
fn journal_replay_restores_the_latest_mode_and_live_state() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let fixture = Fixture::open(Some(journal), Some("s-1"));
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: Some(json!({
            "currentModeId": "plan",
            "availableModes": [
                { "id": "plan", "name": "Plan" },
                { "id": "ask", "name": "Ask" }
            ]
        })),
        config_options: None,
    });
    fixture.accept(AgentStreamPayload::Update {
        update: serde_json::to_value(SessionUpdate::CurrentModeUpdate(
            agent_client_protocol::v1::schema::CurrentModeUpdate::new("ask"),
        ))
        .expect("encode mode update"),
    });
    fixture.chunk("a");
    fixture.chunk("b");
    fixture.recorder.wait_for_items(4);
    {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
        lane.evicted_seq = lane.next_seq.saturating_sub(1);
    }

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);
    let replayed = direct_items(&fixture.recorder)
        .into_iter()
        .map(|(_, _, item)| item)
        .collect::<Vec<_>>();
    let modes = replayed.iter().find_map(|item| match &item.payload {
        AgentStreamPayload::SessionReady {
            modes: Some(modes), ..
        } => Some(modes),
        _ => None,
    });
    assert_eq!(
        modes
            .and_then(|modes| modes.get("currentModeId"))
            .and_then(Value::as_str),
        Some("ask")
    );
    let state = replayed.last().and_then(|item| match &item.payload {
        AgentStreamPayload::StateSynced { state } => Some(state),
        _ => None,
    });
    assert!(matches!(
        state.map(|state| &state.phase),
        Some(zz_protocol::AgentConnectionPhase::Running)
    ));
    assert_eq!(
        state
            .and_then(|state| serde_json::from_str::<Value>(&state.modes).ok())
            .and_then(|modes| modes.get("currentModeId").cloned())
            .and_then(|mode| mode.as_str().map(ToOwned::to_owned))
            .as_deref(),
        Some("ask")
    );
}

#[test]
fn journal_replay_restores_raw_config_option_updates() {
    use agent_client_protocol::v1::schema::{ConfigOptionUpdate, SessionConfigOption};

    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let fixture = Fixture::open(Some(journal), Some("s-1"));
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: None,
        config_options: Some(
            serde_json::to_value(vec![SessionConfigOption::boolean(
                "sandbox", "Sandbox", false,
            )])
            .expect("encode initial config"),
        ),
    });
    fixture.accept(AgentStreamPayload::Update {
        update: serde_json::to_value(SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
            vec![SessionConfigOption::boolean("sandbox", "Sandbox", true)],
        )))
        .expect("encode config update"),
    });
    fixture.chunk("a");
    fixture.chunk("b");
    fixture.recorder.wait_for_items(4);
    {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
        lane.evicted_seq = lane.next_seq.saturating_sub(1);
    }

    fixture.fanout.replay(ClientId(3), fixture.pane, 1);
    let replayed = direct_items(&fixture.recorder)
        .into_iter()
        .map(|(_, _, item)| item)
        .collect::<Vec<_>>();
    let config = replayed.iter().find_map(|item| match &item.payload {
        AgentStreamPayload::SessionReady {
            config_options: Some(config),
            ..
        } => Some(config),
        _ => None,
    });
    assert_eq!(
        config
            .and_then(Value::as_array)
            .and_then(|options| options.first())
            .and_then(|option| option.get("currentValue"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn a_journal_replay_shows_the_effort_option_as_the_live_chat_does() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    let options = |effort: &str, level: &str| {
        json!([
            {"id": "model", "name": "Model", "type": "select", "currentValue": "gemini-3.8-flash", "options": [{"value": "gemini-3.8-flash", "name": "Flash"}]},
            {"id": effort, "name": "Thinking", "type": "select", "currentValue": level, "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]},
        ])
    };
    let fixture = Fixture::open(Some(Arc::clone(&journal)), None);
    let pane = PaneId(5);
    fixture
        .fanout
        .open_lane(pane, 1, AgentProvider::Antigravity, Some("s-1".to_owned()));
    let accept = |payload| {
        fixture.fanout.accept(
            pane,
            1,
            &state(AgentConnectionPhase::Ready),
            Some(AgentStreamItem { seq: 0, payload }),
        );
    };
    accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: None,
        config_options: Some(options("effort", "low")),
    });
    journal
        .append_for(
            AgentProvider::Antigravity,
            "s-1",
            &json!({"sessionUpdate": "config_option_update", "configOptions": options("thought_level", "high")}),
        )
        .expect("journal the agent's update");
    accept(AgentStreamPayload::Update {
        update: json!({"sessionUpdate": "config_option_update", "configOptions": options("effort", "high")}),
    });
    for text in ["a", "b"] {
        accept(AgentStreamPayload::Update {
            update: json!({"sessionUpdate": "agent_message_chunk", "content": {"text": text}}),
        });
    }
    let live = fixture.recorder.wait_for_items(4);
    {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&pane).expect("pane lane");
        lane.evicted_seq = lane.next_seq.saturating_sub(1);
    }
    fixture.recorder.broadcast.lock().clear();

    fixture.fanout.replay(ClientId(3), pane, 1);
    let lists = |items: &[AgentStreamItem]| {
        items
            .iter()
            .filter_map(|item| match &item.payload {
                AgentStreamPayload::SessionReady {
                    config_options: Some(options),
                    ..
                } => Some(options.clone()),
                AgentStreamPayload::Update { update }
                    if update["sessionUpdate"] == "config_option_update" =>
                {
                    Some(update["configOptions"].clone())
                }
                _ => None,
            })
            .map(|options| {
                options
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|option| format!("{}={}", option["id"], option["currentValue"]))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lists(&live),
        [
            ["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"low\""],
            ["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"high\""],
        ]
    );
    assert_eq!(
        lists(&fixture.recorder.payloads()),
        [["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"high\""]]
    );
}

#[test]
fn a_journal_replay_goes_to_the_asker_alone_on_the_numbers_it_skips() {
    let directory = tempfile::tempdir().expect("journal directory");
    let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
    journal
        .append_for(
            AgentProvider::Codex,
            "s-1",
            &json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "journalled"}}),
        )
        .expect("journal");
    let fixture = Fixture::open(Some(journal), Some("s-1"));
    fixture.accept(AgentStreamPayload::SessionReady {
        session_id: "s-1".to_owned(),
        modes: None,
        config_options: None,
    });
    for text in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
        fixture.chunk(text);
    }
    fixture.recorder.wait_for_items(11);
    let evict = |upto: u64| {
        let mut lanes = fixture.fanout.lanes.lock();
        let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
        while lane.ring.front().is_some_and(|(seq, _)| *seq <= upto) {
            let (seq, dropped) = lane.ring.pop_front().expect("ring item");
            lane.ring_bytes -= dropped.len();
            lane.evicted_seq = seq;
        }
        (lane.evicted_seq, lane.next_seq)
    };
    fixture.recorder.broadcast.lock().clear();
    let sent = |from_seq| {
        fixture.recorder.direct.lock().clear();
        fixture.fanout.replay(ClientId(3), fixture.pane, from_seq);
        fixture
            .recorder
            .direct
            .lock()
            .iter()
            .flat_map(|(client, _, _, items)| {
                assert_eq!(*client, ClientId(3));
                items.clone()
            })
            .map(|item| {
                let item = serde_json::from_slice::<AgentStreamItem>(&item).expect("decode");
                let kind = match &item.payload {
                    AgentStreamPayload::Update { update } => update["content"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    AgentStreamPayload::SessionReset { .. } => "reset".to_owned(),
                    AgentStreamPayload::SessionReady { .. } => "ready".to_owned(),
                    AgentStreamPayload::StateSynced { .. } => "state".to_owned(),
                    other => format!("{other:?}"),
                };
                (item.seq, kind)
            })
            .collect::<Vec<_>>()
    };
    let journal_replay = [
        (8, "reset".to_owned()),
        (9, "journalled".to_owned()),
        (10, "ready".to_owned()),
        (11, "state".to_owned()),
    ];

    assert_eq!(evict(1), (1, 12));
    assert_eq!(
        sent(0),
        journal_replay,
        "a client that holds nothing is reset when the ring lacks its first item"
    );
    assert_eq!(evict(9), (9, 12));
    assert_eq!(
        sent(9),
        [(10, "i".to_owned()), (11, "j".to_owned())],
        "a client that has the last evicted number takes the ring"
    );
    assert_eq!(sent(1), journal_replay);
    assert!(fixture.recorder.broadcast.lock().is_empty());
    let lanes = fixture.fanout.lanes.lock();
    let lane = &lanes[&fixture.pane];
    assert_eq!((lane.evicted_seq, lane.next_seq), (9, 12));
}

#[test]
fn session_replies_leave_the_stream_without_spending_a_sequence() {
    let fixture = Fixture::open(None, Some("s-1"));
    fixture.accept(AgentStreamPayload::SessionsListed {
        client: ClientId(8),
        sessions: vec![AgentSessionSummary {
            session_id: "s-2".to_owned(),
            cwd: PathBuf::from("/work"),
            additional_directories: Vec::new(),
            title: None,
            updated_at: None,
        }],
        next_cursor: None,
        cwd_filter: None,
        replace: true,
    });
    fixture.accept(AgentStreamPayload::SessionDeleted {
        client: ClientId(10),
        session_id: "s-3".to_owned(),
    });
    fixture.chunk("after");
    let items = fixture.recorder.wait_for_items(1);

    assert_eq!(
        items.iter().map(|item| item.seq).collect::<Vec<_>>(),
        [1],
        "a reply is not part of the transcript the client replays"
    );
    let replies = fixture.recorder.replies.lock();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].0, ClientId(8));
    assert_eq!(replies[0].2, 0);
    assert_eq!(replies[1].0, ClientId(10));
    assert!(replies[1].3.contains("s-3"));
}

#[test]
fn an_oversized_session_page_returns_a_targeted_failure() {
    let fixture = Fixture::open(None, Some("s-1"));
    fixture.accept(AgentStreamPayload::SessionsListed {
        client: ClientId(8),
        sessions: (0..300)
            .map(|index| AgentSessionSummary {
                session_id: format!("s-{index}"),
                cwd: PathBuf::from("/work"),
                additional_directories: Vec::new(),
                title: Some("x".repeat(4 * 1024)),
                updated_at: None,
            })
            .collect(),
        next_cursor: None,
        cwd_filter: None,
        replace: true,
    });

    let replies = fixture.recorder.replies.lock();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].0, ClientId(8));
    assert!(matches!(
        serde_json::from_str::<AgentStreamPayload>(&replies[0].3),
        Ok(AgentStreamPayload::SessionListFailed { client, message })
            if client == ClientId(8) && message.contains("too much session history")
    ));
}

#[test]
fn the_pane_state_is_published_only_when_something_a_client_renders_moves() {
    let fixture = Fixture::open(None, Some("s-1"));
    fixture.chunk("a");
    fixture.chunk("b");
    assert_eq!(
        fixture.recorder.states.lock().len(),
        1,
        "a steady turn republishes nothing"
    );

    let mut running = state(AgentConnectionPhase::Running);
    running.own_prompts = 1;
    fixture.fanout.accept(fixture.pane, 1, &running, None);
    running.git = Some(zz_protocol::AgentGitSummary {
        branch: Some("main".to_owned()),
        changed_files: 2,
        additions: 8,
        deletions: 3,
    });
    fixture.fanout.accept(fixture.pane, 1, &running, None);
    let states = fixture.recorder.states.lock();
    assert_eq!(states.len(), 3);
    assert_eq!(states[1].1.queued_prompts, 1);
    assert_eq!(states[2].1.git, running.git);
}

fn accept_in(fixture: &Fixture, phase: AgentConnectionPhase, payload: AgentStreamPayload) {
    fixture.fanout.accept(
        fixture.pane,
        1,
        &state(phase),
        Some(AgentStreamItem { seq: 0, payload }),
    );
}

fn finished(reason: &str) -> AgentStreamPayload {
    AgentStreamPayload::PromptFinished {
        turn_id: 1,
        outcome: AgentPromptOutcome::Finished {
            stop_reason: json!(reason),
        },
    }
}

fn replays_a_running_turn(fixture: &Fixture) -> bool {
    fixture
        .fanout
        .lanes
        .lock()
        .get(&fixture.pane)
        .expect("lane")
        .recovery_metadata()
        .iter()
        .any(|payload| matches!(payload, AgentStreamPayload::TurnStarted { .. }))
}

#[test]
fn a_finished_turn_is_not_replayed_as_running() {
    let fixture = Fixture::open(None, Some("s-1"));
    accept_in(
        &fixture,
        AgentConnectionPhase::Running,
        AgentStreamPayload::TurnStarted { turn_id: 1 },
    );
    assert!(replays_a_running_turn(&fixture));
    accept_in(&fixture, AgentConnectionPhase::Ready, finished("end_turn"));
    assert!(!replays_a_running_turn(&fixture));
}

struct Watched {
    fixture: Fixture,
    seen: u64,
    chat: Vec<(u64, ChatEvent)>,
    straight: zz_protocol::chat::ChatView,
}

impl Watched {
    fn open(
        tape: &crate::cut_points::Tape,
        journal: Option<Arc<AgentJournal>>,
        parked: Option<u64>,
    ) -> Self {
        let recorder = Arc::new(Recorder::default());
        let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
        let fanout = Arc::new(AgentFanout::new(
            &publisher,
            journal,
            Arc::new(Mutex::new(())),
        ));
        let pane = PaneId(4);
        if let Some(next_seq) = parked {
            fanout.parked.lock().insert(pane, (next_seq, 1));
        }
        fanout.open_lane(pane, 1, tape.provider, Some(tape.session.clone()));
        let mut watched = Self {
            fixture: Fixture {
                fanout,
                recorder,
                pane,
            },
            seen: 0,
            chat: Vec::new(),
            straight: zz_protocol::chat::ChatView::default(),
        };
        watched.catch_up();
        watched
    }

    fn accept(&mut self, payload: &AgentStreamPayload) {
        self.fixture.accept(payload.clone());
        self.fixture.recorder.broadcast.lock().clear();
        self.catch_up();
    }

    fn catch_up(&mut self) {
        let lanes = self.fixture.fanout.lanes.lock();
        let seen = self.seen;
        for (seq, encoded) in lanes[&self.fixture.pane]
            .ring
            .iter()
            .filter(|(seq, _)| *seq > seen)
        {
            let item: AgentStreamItem = serde_json::from_slice(encoded).expect("decode");
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = self.straight.apply(&event);
                self.chat.push((*seq, event));
            }
            self.seen = *seq;
        }
    }

    fn next_seq(&self) -> u64 {
        self.fixture.fanout.lanes.lock()[&self.fixture.pane].next_seq
    }

    fn log(&self) -> Vec<ChatEvent> {
        std::iter::once(ChatEvent::Restart)
            .chain(
                self.fixture.fanout.lanes.lock()[&self.fixture.pane]
                    .chat
                    .events()
                    .cloned(),
            )
            .collect()
    }

    fn known(&self, from_seq: u64) -> zz_protocol::chat::ChatView {
        crate::cut_points::fold(
            self.chat
                .iter()
                .take_while(|(seq, _)| *seq <= from_seq)
                .map(|(_, event)| event),
        )
    }

    fn replay(&mut self, from_seq: u64, evicted: bool) -> Replayed {
        let pane = self.fixture.pane;
        let decode =
            |item: &Vec<u8>| -> AgentStreamItem { serde_json::from_slice(item).expect("decode") };
        let (before, next_seq) = {
            let mut lanes = self.fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&pane).expect("lane");
            let before = lane.evicted_seq;
            if evicted {
                lane.evicted_seq = lane.next_seq;
            }
            (before, lane.next_seq)
        };
        self.fixture.recorder.broadcast.lock().clear();
        let straight = self.straight.clone();
        self.fixture.fanout.replay(ClientId(7), pane, from_seq);
        let mut others = {
            let mut lanes = self.fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&pane).expect("lane");
            lane.evicted_seq = before;
            lane.take_batch()
                .into_iter()
                .flat_map(|(_, items)| items)
                .collect::<Vec<_>>()
        };
        others.extend(
            std::mem::take(&mut *self.fixture.recorder.broadcast.lock())
                .into_iter()
                .flat_map(|(_, _, items, _)| items),
        );
        let mut others = others
            .iter()
            .map(decode)
            .filter(|item| item.seq >= next_seq)
            .collect::<Vec<_>>();
        others.sort_by_key(|item| item.seq);
        let asker = std::mem::take(&mut *self.fixture.recorder.direct.lock())
            .into_iter()
            .flat_map(|(_, _, _, items)| items)
            .map(|item| decode(&item))
            .collect();
        self.catch_up();
        Replayed {
            asker,
            others,
            before: next_seq,
            after: self.next_seq(),
            straight,
        }
    }
}

struct Replayed {
    asker: Vec<AgentStreamItem>,
    others: Vec<AgentStreamItem>,
    before: u64,
    after: u64,
    straight: zz_protocol::chat::ChatView,
}

fn client_view(
    mut cursor: u64,
    mut view: zz_protocol::chat::ChatView,
    items: &[AgentStreamItem],
) -> Result<(u64, zz_protocol::chat::ChatView), u64> {
    for item in items {
        if item.seq > cursor.saturating_add(1)
            && matches!(
                item.payload,
                AgentStreamPayload::SessionReset { restoring: true }
            )
        {
            cursor = item.seq.saturating_sub(1);
        }
        if item.seq <= cursor {
            continue;
        }
        if item.seq > cursor.saturating_add(1) {
            return Err(item.seq);
        }
        cursor = item.seq;
        if let AgentStreamPayload::Chat { event } = &item.payload {
            let _ = view.apply(event);
        }
    }
    Ok((cursor, view))
}

#[test]
fn every_replay_path_at_every_cut_of_a_recording_shows_the_live_chat() {
    use crate::cut_points::{
        Findings, difference, fold, payload_kind, short, sub_view_difference, tapes,
    };
    use zz_protocol::chat::{ChatView, TurnBy, TurnId};

    const REOPENED_AT: u64 = 1000;
    const LONG_LANE: usize = 16;
    const NEAR_ROOMS: u64 = 24;
    let stale = fold(&[
        ChatEvent::SessionReady {
            session: "stale".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(99),
            by: TurnBy::Agent,
            at: 0,
        },
    ]);
    let mut findings = Findings::new("B fanout replay");
    for tape in tapes() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let mut reopened = Watched::open(tape, None, Some(REOPENED_AT));
        let mut journalled = Watched::open(tape, Some(Arc::clone(&journal)), None);
        let mut memory = Watched::open(tape, None, None);
        let total = tape
            .lines
            .iter()
            .map(|line| line.payloads.len())
            .sum::<usize>();
        let kinds = tape
            .lines
            .iter()
            .map(|line| line.payloads.iter().map(payload_kind).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let mut position = 0;
        for (index, line) in tape.lines.iter().enumerate() {
            let inside_a_run = position > LONG_LANE
                && index > 0
                && kinds.get(index - 1) == Some(&kinds[index])
                && kinds.get(index + 1) == Some(&kinds[index]);
            if inside_a_run {
                for payload in &line.payloads {
                    position += 1;
                    for watched in [&mut reopened, &mut journalled, &mut memory] {
                        watched.accept(payload);
                    }
                }
                continue;
            }
            journal
                .remove_for(tape.provider, &tape.session)
                .expect("clear the journal");
            for update in tape.journal_at(index) {
                journal
                    .append_for(tape.provider, &tape.session, &update)
                    .expect("journal");
            }
            for (offset, payload) in line.payloads.iter().enumerate() {
                position += 1;
                for watched in [&mut reopened, &mut journalled, &mut memory] {
                    watched.accept(payload);
                }
                let sampled = position > LONG_LANE;
                if sampled && offset + 1 < line.payloads.len() {
                    continue;
                }
                let at = payload_kind(payload);
                let mut check = |path: &str,
                                 watched: &mut Watched,
                                 from_seq: u64,
                                 evicted: bool| {
                    let replayed = watched.replay(from_seq, evicted);
                    let (next_seq, items) = (replayed.after, &replayed.asker);
                    let room = replayed.before - from_seq - 1;
                    let rank = (
                        total,
                        position * 10_000 + usize::try_from(room).unwrap_or(0),
                    );
                    let example = |detail: &str| {
                        format!(
                            "{} pos {position}/{total} after {} from_seq {from_seq} next_seq {} room {room}: {detail}",
                            tape.name,
                            short(payload),
                            replayed.before,
                        )
                    };
                    findings.checked();
                    if let Some(diff) = difference(&replayed.straight, &watched.straight) {
                        findings.add(
                            format!("B | {path} | a widening no-op changed the chat"),
                            at.clone(),
                            rank,
                            || example(&diff.describe()),
                        );
                    }
                    match client_view(
                        replayed.before - 1,
                        replayed.straight.clone(),
                        &replayed.others,
                    ) {
                        Ok((cursor, view)) if cursor == next_seq - 1 => {
                            if let Some(diff) = difference(&watched.straight, &view) {
                                findings.add(
                                    format!("B | {path} | a current viewer's chat changed"),
                                    at.clone(),
                                    rank,
                                    || example(&diff.describe()),
                                );
                            }
                        }
                        Ok((cursor, _)) => findings.add(
                            format!("B | {path} | a current viewer ends short of the lane"),
                            at.clone(),
                            rank,
                            || example(&format!("cursor {cursor} of {next_seq}")),
                        ),
                        Err(seq) => findings.add(
                            format!("B | {path} | a current viewer sees a gap"),
                            at.clone(),
                            rank,
                            || example(&format!("item {seq} after {}", replayed.before - 1)),
                        ),
                    }
                    let chat = items
                        .iter()
                        .filter_map(|item| match &item.payload {
                            AgentStreamPayload::Chat { event } => Some(event.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    let restart = chat.contains(&ChatEvent::Restart);
                    let prior = match (restart, path) {
                        (true, _) => ChatView::default(),
                        (false, "b") => stale.clone(),
                        (false, _) => watched.known(from_seq),
                    };
                    let (cursor, view) = match client_view(from_seq, prior, items) {
                        Ok(caught) => caught,
                        Err(seq) => {
                            findings.add(
                                format!("B | {path} | gap in the replay"),
                                at.clone(),
                                rank,
                                || example(&format!("item {seq} after {from_seq}")),
                            );
                            return items.len();
                        }
                    };
                    if items.iter().any(|item| {
                        matches!(&item.payload, AgentStreamPayload::Update { update } if is_zz_note(update))
                    }) {
                        findings.add(
                            format!("B | {path} | sends a journal note"),
                            at.clone(),
                            rank,
                            || example("a turn mark or decline note from the journal reached the asker"),
                        );
                    }
                    if cursor != next_seq - 1 {
                        findings.add(
                            format!("B | {path} | replay ends short of the lane"),
                            at.clone(),
                            rank,
                            || example(&format!("cursor {cursor}")),
                        );
                    }
                    let journal_path = items
                        .iter()
                        .any(|item| matches!(item.payload, AgentStreamPayload::Update { .. }));
                    let rebuilt = !matches!(path, "a" | "b") && !journal_path;
                    let numbers = items.iter().map(|item| item.seq).collect::<Vec<_>>();
                    let first = numbers.first().copied().unwrap_or(next_seq);
                    let widened = next_seq > replayed.before;
                    if rebuilt
                        && (numbers != (first..next_seq).collect::<Vec<_>>()
                            || first <= from_seq
                            || (widened && first != from_seq + 1)
                            || !matches!(
                                items.first().map(|item| &item.payload),
                                Some(AgentStreamPayload::SessionReset { restoring: true })
                            ))
                    {
                        findings.add(
                            format!("B | {path} | the rebuild is not numbered from_seq + 1 ..= next_seq - 1"),
                            at.clone(),
                            rank,
                            || example(&format!("numbered {numbers:?}")),
                        );
                    }
                    let (path, diff) = match path {
                        "a" | "b" => (path, difference(&watched.straight, &view)),
                        _ if journal_path => ("c", difference(&watched.straight, &view)),
                        _ if chat == watched.log() => {
                            ("d whole", difference(&watched.straight, &view))
                        }
                        _ if restart => ("d", sub_view_difference(&watched.straight, &view)),
                        _ => (
                            "d without Restart",
                            sub_view_difference(&watched.straight, &view).map(|mut diff| {
                                diff.at = format!("{}, {}", diff.field, diff.at);
                                "the asker keeps its old chat".clone_into(&mut diff.field);
                                diff
                            }),
                        ),
                    };
                    if let Some(diff) = diff {
                        findings.add(
                            format!("B | {path} | {}", diff.field),
                            at.clone(),
                            rank,
                            || example(&diff.describe()),
                        );
                    }
                    items.len()
                };
                let next_seq = reopened.next_seq();
                for from_seq in [0, REOPENED_AT - 2] {
                    check("b", &mut reopened, from_seq, false);
                }
                let span = next_seq - (REOPENED_AT - 1);
                for step in (0..=8u64).filter(|step| !sampled || step.is_multiple_of(4)) {
                    let from_seq = REOPENED_AT - 1 + span * step / 8;
                    if from_seq < next_seq {
                        check("a", &mut reopened, from_seq, false);
                    }
                }
                let count = u64::try_from(check("c", &mut journalled, 0, true)).unwrap_or(0);
                for short_by in [0, 1] {
                    if let Some(from_seq) = journalled
                        .next_seq()
                        .checked_sub(1 + count)
                        .map(|fits| fits + short_by)
                        .filter(|from_seq| (1..journalled.next_seq()).contains(from_seq))
                    {
                        check("c", &mut journalled, from_seq, true);
                    }
                }
                check("d", &mut memory, 0, true);
                let log = u64::try_from(memory.log().len()).unwrap_or(0);
                let stride = if sampled {
                    (log + 6).saturating_sub(NEAR_ROOMS) / 4
                } else {
                    1
                }
                .max(1);
                for room in (0..=log + 6).filter(|room| {
                    *room <= NEAR_ROOMS
                        || (room - NEAR_ROOMS).is_multiple_of(stride)
                        || *room == log + 6
                }) {
                    if let Some(from_seq) = memory.next_seq().checked_sub(room + 1) {
                        check("d", &mut memory, from_seq, true);
                    }
                }
            }
        }
    }
    findings.finish(&[
        "B | a | sends a journal note",
        "B | b | sends a journal note",
    ]);
}
