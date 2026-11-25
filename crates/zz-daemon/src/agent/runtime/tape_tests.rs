use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, OnceLock, atomic::AtomicU64},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use agent_client_protocol::ByteStreams;
use async_channel::{Receiver, Sender};
use futures_lite::io::{AsyncRead, AsyncWrite};
use parking_lot::Mutex;
use serde_json::Value;
use zz_protocol::{AgentAutoApprove, AgentProvider, ClientInstanceId, chat::ChatEvent};

use super::{
    AgentPrompt, AgentStreamPayload, RuntimeCommand, RuntimeControl, journal_replay,
    run_agent_connection, runtime_failure_message,
};
use crate::agent::{
    acp_chat::Transcript,
    cut_points::{self, Findings, Tape, Taping, chat_event, short},
    dialect::{self, Dialect},
    host::RuntimeChannels,
    journal::AgentJournal,
};

const WAIT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Pipe {
    written: Vec<u8>,
    incoming: VecDeque<u8>,
    closed: bool,
    reader: Option<Waker>,
}

struct Writer(Arc<Mutex<Pipe>>);

struct Reader(Arc<Mutex<Pipe>>);

impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.lock().written.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for Reader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut pipe = self.0.lock();
        if pipe.incoming.is_empty() {
            if pipe.closed {
                return Poll::Ready(Ok(0));
            }
            pipe.reader = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let count = buf.len().min(pipe.incoming.len());
        for (slot, byte) in buf.iter_mut().zip(pipe.incoming.drain(..count)) {
            *slot = byte;
        }
        Poll::Ready(Ok(count))
    }
}

struct Played {
    name: &'static str,
    lines: usize,
    consumed: usize,
    waits: usize,
    differences: Vec<(String, String)>,
}

struct Connection {
    executor: smol::LocalExecutor<'static>,
    pipe: Arc<Mutex<Pipe>>,
    commands: Sender<RuntimeCommand>,
    controls: Sender<RuntimeControl>,
    events: Receiver<AgentStreamPayload>,
    _task: smol::Task<Result<(), String>>,
}

impl Connection {
    fn release(&self, message: &Value) {
        let mut pipe = self.pipe.lock();
        pipe.incoming.extend(message.to_string().bytes());
        pipe.incoming.push_back(b'\n');
        if let Some(reader) = pipe.reader.take() {
            reader.wake();
        }
    }

    fn close(&self) {
        let mut pipe = self.pipe.lock();
        pipe.closed = true;
        if let Some(reader) = pipe.reader.take() {
            reader.wake();
        }
    }
}

fn matches(recorded: &Value, written: &Value) -> bool {
    match (recorded.get("method"), recorded.get("id")) {
        (Some(method), id) => {
            written.get("method") == Some(method) && written.get("id").is_some() == id.is_some()
        }
        (None, id) => written.get("method").is_none() && written.get("id") == id,
    }
}

fn normalized(message: &Value) -> Value {
    let mut message = message.clone();
    if let Some(object) = message.as_object_mut() {
        object.remove("id");
        object.remove("jsonrpc");
    }
    for pointer in ["/params/_meta/rules", "/params/clientInfo/version"] {
        if let Some(value) = message.pointer_mut(pointer) {
            *value = Value::from("varies");
        }
    }
    message
}

fn first_difference(recorded: Option<&Value>, written: Option<&Value>, at: &str) -> Option<String> {
    match (recorded, written) {
        (Some(Value::Object(recorded)), Some(Value::Object(written))) => recorded
            .keys()
            .chain(written.keys().filter(|key| !recorded.contains_key(*key)))
            .find_map(|key| {
                first_difference(recorded.get(key), written.get(key), &format!("{at}/{key}"))
            }),
        (Some(Value::Array(recorded)), Some(Value::Array(written)))
            if recorded.len() == written.len() =>
        {
            recorded
                .iter()
                .zip(written)
                .enumerate()
                .find_map(|(index, (one, other))| {
                    first_difference(Some(one), Some(other), &format!("{at}/{index}"))
                })
        }
        (recorded, written) => (recorded != written).then(|| at.to_owned()),
    }
}

fn label(message: &Value) -> &str {
    message["method"].as_str().unwrap_or("response")
}

fn prompt_text(message: &Value) -> String {
    message["params"]["prompt"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn segments(tape: &str) -> Vec<Vec<Value>> {
    let mut segments: Vec<Vec<Value>> = Vec::new();
    for line in tape.lines().filter(|line| !line.trim().is_empty()) {
        let entry: Value = serde_json::from_str(line).expect("tape line");
        if entry["dir"] == "meta" {
            segments.push(Vec::new());
        } else if let Some(segment) = segments.last_mut() {
            segment.push(entry);
        }
    }
    segments
}

fn opened(segment: &[Value]) -> Option<(String, PathBuf)> {
    let asked = segment
        .iter()
        .filter(|entry| entry["dir"] == "in")
        .map(|entry| &entry["msg"])
        .filter(|message| {
            matches!(
                message["method"].as_str(),
                Some("session/new" | "session/load")
            )
        })
        .collect::<Vec<_>>();
    segment
        .iter()
        .rev()
        .filter(|entry| entry["dir"] == "out" && entry["msg"]["error"].is_null())
        .find_map(|entry| {
            let request = asked
                .iter()
                .find(|message| message["id"] == entry["msg"]["id"])?;
            let session = entry["msg"]["result"]["sessionId"]
                .as_str()
                .or_else(|| request["params"]["sessionId"].as_str())?;
            let cwd = request["params"]["cwd"].as_str()?;
            Some((session.to_owned(), PathBuf::from(cwd)))
        })
}

fn metered(payload: &AgentStreamPayload) -> bool {
    matches!(
        payload,
        AgentStreamPayload::SessionReady { .. }
            | AgentStreamPayload::SessionSwitched { .. }
            | AgentStreamPayload::PromptFinished { .. }
    )
}

struct Pane {
    provider: AgentProvider,
    dialect: Dialect,
    journal: Arc<AgentJournal>,
    tier: Arc<Mutex<AgentAutoApprove>>,
    permission_ids: Arc<AtomicU64>,
    turn: u64,
    session: Option<String>,
}

impl Pane {
    fn connect(&self, cwd: PathBuf, resume: Option<String>) -> Connection {
        let pipe = Arc::new(Mutex::new(Pipe::default()));
        let (commands, command_rx) = async_channel::unbounded();
        let (controls, control_rx) = async_channel::unbounded();
        let (event_tx, events) = async_channel::unbounded();
        let channels = RuntimeChannels {
            auto_approve: Arc::clone(&self.tier),
            permission_ids: Arc::clone(&self.permission_ids),
            journal: Some(Arc::clone(&self.journal)),
            ended: Arc::default(),
            commands: command_rx,
            controls: control_rx,
            events: event_tx,
        };
        let provider = self.provider;
        let session_meta = self.dialect.rules.map(|key| {
            serde_json::Map::from_iter([(
                key.to_owned(),
                Value::String(zz_daemon_client::agent_context::workspace_context(
                    "%1", "$1", true,
                )),
            )])
        });
        let transport = ByteStreams::new(Writer(Arc::clone(&pipe)), Reader(Arc::clone(&pipe)));
        let executor = smol::LocalExecutor::new();
        let task = executor.spawn(dialect::run_through(
            self.dialect,
            provider,
            channels,
            move |inner| async move {
                run_agent_connection(
                    provider,
                    inner.auto_approve,
                    transport,
                    inner.permission_ids,
                    inner.journal,
                    inner.commands,
                    inner.controls,
                    inner.events,
                    session_meta,
                )
                .await
                .map_err(|error| runtime_failure_message(provider.label(), &error, None))
            },
        ));
        commands
            .try_send(RuntimeCommand::Open {
                cwd,
                resume_session: resume,
            })
            .expect("open");
        Connection {
            executor,
            pipe,
            commands,
            controls,
            events,
            _task: task,
        }
    }
}

struct Player<'p, 't> {
    name: &'static str,
    pane: &'p mut Pane,
    connection: Connection,
    segment: &'t [Value],
    next: usize,
    injected: HashSet<usize>,
    pending: Vec<AgentStreamPayload>,
    asks: VecDeque<u64>,
    written: Vec<Value>,
    seen: usize,
    ours: HashMap<String, (Value, String)>,
    theirs: HashMap<String, String>,
    consumed: usize,
    waits: usize,
    differences: Vec<(String, String)>,
}

impl Player<'_, '_> {
    fn settle(&mut self) {
        loop {
            self.drain();
            if !self.connection.executor.try_tick() {
                break;
            }
        }
        let pipe = self.connection.pipe.lock();
        let fresh = &pipe.written[self.seen..];
        let complete = fresh
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |end| end + 1);
        let text = String::from_utf8_lossy(&fresh[..complete]).into_owned();
        drop(pipe);
        self.seen += complete;
        self.written.extend(
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    serde_json::from_str(line).unwrap_or_else(|_| Value::String(line.to_owned()))
                }),
        );
    }

    fn drain(&mut self) {
        let before = self.pending.len();
        self.pending.extend(std::iter::from_fn(|| {
            self.connection.events.try_recv().ok()
        }));
        let mut react = false;
        for payload in &self.pending[before..] {
            match payload {
                AgentStreamPayload::SessionReady { session_id, .. }
                | AgentStreamPayload::SessionSwitched { session_id, .. } => {
                    self.pane.session = Some(session_id.clone());
                }
                AgentStreamPayload::PermissionRequested { request_id, .. } => {
                    self.asks.push_back(*request_id);
                }
                _ => {}
            }
            react |= metered(payload);
        }
        if react {
            self.react();
        }
    }

    fn react(&mut self) {
        let Some(quota) = self.pane.dialect.quota else {
            return;
        };
        let segment = self.segment;
        let ahead = &segment[self.next.min(segment.len())..];
        let Some(until) = ahead
            .iter()
            .position(|entry| entry["dir"] == "in" && entry["msg"]["method"] == quota.method)
        else {
            return;
        };
        for (offset, entry) in ahead[..until].iter().enumerate() {
            if entry["dir"] == "in" && self.injected.insert(self.next + offset) {
                self.inject(&entry["msg"]);
            }
        }
    }

    fn wait(&mut self, done: impl Fn(&Self) -> bool) -> bool {
        self.settle();
        if done(self) {
            return true;
        }
        self.waits += 1;
        let deadline = Instant::now() + WAIT;
        loop {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            smol::block_on(futures_lite::future::or(
                self.connection.executor.tick(),
                async {
                    smol::Timer::after(left).await;
                },
            ));
            self.settle();
            if done(self) {
                return true;
            }
        }
    }

    fn inject(&mut self, message: &Value) {
        let connection = &self.connection;
        let params = &message["params"];
        let text = |field: &str| params[field].as_str().unwrap_or_default().to_owned();
        let sent = match message["method"].as_str() {
            Some("session/prompt") => {
                self.pane.turn += 1;
                connection
                    .commands
                    .try_send(RuntimeCommand::Prompt {
                        turn_id: self.pane.turn,
                        prompt: AgentPrompt {
                            owner: ClientInstanceId::default(),
                            text: prompt_text(message),
                            images: Vec::new(),
                        },
                        note: None,
                    })
                    .is_ok()
            }
            Some("session/cancel") => connection
                .controls
                .try_send(RuntimeControl::Cancel {
                    turn_id: self.pane.turn,
                    session_id: self.pane.session.clone(),
                })
                .is_ok(),
            Some("session/set_config_option") => connection
                .commands
                .try_send(RuntimeCommand::SetConfigOption {
                    option_id: text("configId"),
                    value: text("value"),
                })
                .is_ok(),
            Some("session/set_mode") => connection
                .commands
                .try_send(RuntimeCommand::SetMode {
                    mode_id: text("modeId"),
                })
                .is_ok(),
            None if self
                .theirs
                .get(&message["id"].to_string())
                .is_some_and(|method| method == "session/request_permission") =>
            {
                let request_id = self.asks.pop_front().expect("an open permission request");
                connection
                    .controls
                    .try_send(RuntimeControl::RespondPermission {
                        request_id,
                        option_id: message["result"]["outcome"]["optionId"]
                            .as_str()
                            .map(str::to_owned),
                    })
                    .is_ok()
            }
            _ => true,
        };
        assert!(sent, "the replay could not inject {}", short(message));
    }

    fn written(&mut self, index: usize, message: &Value) {
        if self.injected.insert(index) {
            self.inject(message);
        }
        if !self.wait(|player| {
            player
                .written
                .iter()
                .any(|written| matches(message, written))
        }) {
            self.differences.push((
                format!("P | never wrote {}", label(message)),
                format!("{}: {}", self.name, short(&normalized(message))),
            ));
            return;
        }
        let at = self
            .written
            .iter()
            .position(|written| matches(message, written))
            .expect("a written line");
        let written = self.written.remove(at);
        if let (Some(method), Some(id)) = (message["method"].as_str(), written.get("id")) {
            self.ours
                .insert(message["id"].to_string(), (id.clone(), method.to_owned()));
        }
        let (recorded, written) = (normalized(message), normalized(&written));
        if let Some(at) = first_difference(Some(&recorded), Some(&written), "") {
            self.differences.push((
                format!("P | {} differs at {at}", label(message)),
                format!(
                    "{}: wrote {} where the tape has {}",
                    self.name,
                    short(&written.pointer(&at)),
                    short(&recorded.pointer(&at))
                ),
            ));
        }
        self.consumed += 1;
    }

    fn released(&mut self, message: &Value) {
        let mut message = message.clone();
        let recorded = message["id"].to_string();
        let answers = message
            .get("method")
            .is_none()
            .then(|| self.ours.get(&recorded).cloned())
            .flatten();
        if let Some((id, _)) = &answers {
            message["id"] = id.clone();
        }
        if let (Some(method), Some(_)) = (message["method"].as_str(), message.get("id")) {
            self.theirs.insert(recorded, method.to_owned());
        }
        self.connection.release(&message);
        self.settle();
        if answers
            .is_some_and(|(_, method)| matches!(method.as_str(), "session/new" | "session/load"))
            && message["error"].is_null()
        {
            self.wait(|player| {
                player.pending.iter().any(|payload| {
                    metered(payload) || matches!(payload, AgentStreamPayload::PaneFailed { .. })
                })
            });
        }
        self.consumed += 1;
    }

    fn play(mut self, taping: &mut Taping) -> (Played, Vec<ChatEvent>) {
        let mut emitted = Vec::new();
        self.settle();
        let segment = self.segment;
        for (index, entry) in std::iter::once(None)
            .chain(segment.iter().map(Some))
            .enumerate()
        {
            self.next = index;
            if let Some(entry) = entry {
                let message = &entry["msg"];
                match entry["dir"].as_str() {
                    Some("in") => self.written(index - 1, message),
                    Some("out") => self.released(message),
                    Some("exit") => {
                        self.connection.close();
                        self.settle();
                        self.consumed += 1;
                    }
                    _ => {}
                }
            }
            let payloads = std::mem::take(&mut self.pending);
            emitted.extend(payloads.iter().filter_map(chat_event).cloned());
            taping.line(entry, payloads);
        }
        let name = self.name;
        self.differences.extend(self.written.iter().map(|written| {
            (
                format!("P | wrote {} the tape does not have", label(written)),
                format!("{name}: {}", short(&normalized(written))),
            )
        }));
        let played = Played {
            name,
            lines: segment.len(),
            consumed: self.consumed,
            waits: self.waits,
            differences: self.differences,
        };
        (played, emitted)
    }
}

fn reload(provider: AgentProvider, session: &str, journal: &[Value]) -> Vec<ChatEvent> {
    let directory = tempfile::tempdir().expect("journal directory");
    let held = AgentJournal::open(directory.path()).expect("open journal");
    for update in journal {
        held.append_for(provider, session, update).expect("journal");
    }
    let restored = journal_replay(Some(&held), provider, Some(session));
    let mut transcript = Transcript::default();
    let mut events = transcript.payload(&AgentStreamPayload::SessionReset {
        restoring: !restored.is_empty(),
    });
    for item in &restored {
        events.extend(transcript.payload(&item.payload().expect("replayed payload")));
    }
    events.extend(transcript.payload(&AgentStreamPayload::SessionReady {
        session_id: session.to_owned(),
        modes: None,
        config_options: None,
    }));
    events
}

const RECORDED: [(&str, AgentProvider, &str); 12] = [
    (
        "agy-hello",
        AgentProvider::Antigravity,
        include_str!("../fixtures/agy-hello.ndjson"),
    ),
    (
        "agy-tools",
        AgentProvider::Antigravity,
        include_str!("../fixtures/agy-tools.ndjson"),
    ),
    (
        "agy-stop",
        AgentProvider::Antigravity,
        include_str!("../fixtures/agy-stop.ndjson"),
    ),
    (
        "agy-reload",
        AgentProvider::Antigravity,
        include_str!("../fixtures/agy-reload.ndjson"),
    ),
    (
        "agy-queue",
        AgentProvider::Antigravity,
        include_str!("../fixtures/agy-queue.ndjson"),
    ),
    (
        "grok-hello",
        AgentProvider::Grok,
        include_str!("../fixtures/grok-hello.ndjson"),
    ),
    (
        "grok-tools",
        AgentProvider::Grok,
        include_str!("../fixtures/grok-tools.ndjson"),
    ),
    (
        "grok-stop",
        AgentProvider::Grok,
        include_str!("../fixtures/grok-stop.ndjson"),
    ),
    (
        "grok-queue",
        AgentProvider::Grok,
        include_str!("../fixtures/grok-queue.ndjson"),
    ),
    (
        "grok-reload",
        AgentProvider::Grok,
        include_str!("../fixtures/grok-reload.ndjson"),
    ),
    (
        "cursor-hello",
        AgentProvider::Cursor,
        include_str!("../fixtures/cursor-hello.ndjson"),
    ),
    (
        "cursor-reload",
        AgentProvider::Cursor,
        include_str!("../fixtures/cursor-reload.ndjson"),
    ),
];

static PLAYED: OnceLock<Vec<Played>> = OnceLock::new();

pub(crate) fn tapes() -> Vec<Tape> {
    let mut tapes = Vec::new();
    let mut outcomes = Vec::new();
    for (file, provider, recording) in RECORDED {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let mut pane = Pane {
            provider,
            dialect: match provider {
                AgentProvider::Antigravity => crate::agent::agy::DIALECT,
                AgentProvider::Grok => crate::agent::grok::DIALECT,
                _ => crate::agent::cursor::DIALECT,
            },
            journal: Arc::clone(&journal),
            tier: Arc::new(Mutex::new(AgentAutoApprove::Off)),
            permission_ids: Arc::new(AtomicU64::new(1)),
            turn: 0,
            session: None,
        };
        let segments = segments(recording);
        let count = segments.len();
        for (index, segment) in segments.iter().enumerate() {
            let name: &'static str = if count == 1 {
                file
            } else {
                Box::leak(format!("{file} {}/{count}", index + 1).into_boxed_str())
            };
            let (session, cwd) =
                opened(segment).unwrap_or_else(|| (String::new(), PathBuf::from("/work")));
            let resume = pane.session.take();
            let mut taping = Taping::sharing(name, provider, &session, Arc::clone(&journal));
            let connection = pane.connect(cwd, resume);
            let player = Player {
                name,
                pane: &mut pane,
                connection,
                segment,
                next: 0,
                injected: HashSet::new(),
                pending: Vec::new(),
                asks: VecDeque::new(),
                written: Vec::new(),
                seen: 0,
                ours: HashMap::new(),
                theirs: HashMap::new(),
                consumed: 0,
                waits: 0,
                differences: Vec::new(),
            };
            let (outcome, emitted) = player.play(&mut taping);
            outcomes.push(outcome);
            tapes.push(taping.finish(&emitted, reload));
        }
    }
    let _ = PLAYED.set(outcomes);
    tapes
}

#[test]
fn every_acp_tape_plays_through_the_real_driver() {
    let tapes = cut_points::tapes();
    let mut findings = Findings::new("P tape player");
    for played in PLAYED.get().expect("the ACP tapes played") {
        let tape = tapes
            .iter()
            .find(|tape| tape.name == played.name)
            .expect("a played tape");
        eprintln!(
            "{}: {} lines, {} consumed, {} payloads, {} chat events, {} waits",
            played.name,
            played.lines,
            played.consumed,
            tape.lines
                .iter()
                .map(|line| line.payloads.len())
                .sum::<usize>(),
            tape.events().len(),
            played.waits,
        );
        findings.checked();
        if played.consumed != played.lines {
            findings.add(
                "P | tape not consumed".to_owned(),
                played.name.to_owned(),
                (played.lines, played.consumed),
                || format!("{}: {} of {}", played.name, played.consumed, played.lines),
            );
        }
        for (signature, example) in &played.differences {
            findings.add(
                signature.clone(),
                played.name.to_owned(),
                (played.lines, 0),
                || example.clone(),
            );
        }
    }
    findings.finish(&[]);
}
