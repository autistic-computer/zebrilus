use std::{
    io::{BufRead as _, BufReader, Write as _},
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};
use serde_json::{Value, json};
use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_protocol::{
    AgentProvider, MAX_AGENT_RESULT_BYTES,
    agent_mention::{SentMessage, is_relayed},
};

use crate::agent::{
    runtime::{StderrTail, validate_payload},
    stream::{AgentPromptOutcome, AgentStreamPayload},
    tape::{Direction, Tape},
    turn::Outcome,
};

const REAP_GRACE: Duration = Duration::from_secs(2);
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
#[cfg(unix)]
const SOCKET_HANDSHAKE: Duration = Duration::from_secs(5);
#[cfg(unix)]
const SOCKET_POLL: Duration = Duration::from_millis(20);

pub(crate) enum Input<C, K> {
    Command(Result<C, async_channel::RecvError>),
    Control(Result<K, async_channel::RecvError>),
    Child(Result<ChildEvent, async_channel::RecvError>),
    Deadline,
}

pub(crate) async fn next_input<C, K>(
    commands: &Receiver<C>,
    controls: &Receiver<K>,
    child: &Receiver<ChildEvent>,
    deadline: Option<Instant>,
) -> Input<C, K> {
    let timer = async {
        match deadline {
            Some(deadline) => {
                smol::Timer::at(deadline).await;
            }
            None => futures_lite::future::pending::<()>().await,
        }
        Input::Deadline
    };
    let controls = async {
        if controls.is_closed() && controls.is_empty() {
            futures_lite::future::pending::<()>().await;
        }
        Input::Control(controls.recv().await)
    };
    futures_lite::future::or(
        futures_lite::future::or(controls, async { Input::Child(child.recv().await) }),
        futures_lite::future::or(async { Input::Command(commands.recv().await) }, timer),
    )
    .await
}

pub(crate) enum ChildEvent {
    Line(u64, Vec<u8>),
    Closed(u64),
}

pub(crate) struct Process {
    child: Option<Child>,
    stdin: Sender<String>,
    tape: Option<Arc<Tape>>,
}

impl Process {
    pub(crate) fn send(&self, frame: &Value) {
        let _ = self.stdin.try_send(frame.to_string());
    }

    pub(crate) fn exit_detail(&mut self) -> Option<String> {
        let child = self.child.as_mut()?;
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status.to_string()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                _ => return None,
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stdin.close();
        let Some(mut child) = self.child.take() else {
            return;
        };
        let tape = self.tape.take();
        let spawned = thread::Builder::new()
            .name("zz-claude-reap".to_owned())
            .spawn(move || {
                reap(&mut child);
                if let Some(tape) = tape {
                    let status = child.try_wait().ok().flatten();
                    tape.exit(&status.map_or(Value::Null, |status| {
                        status
                            .code()
                            .map_or_else(|| Value::from(status.to_string()), Value::from)
                    }));
                }
            });
        match spawned {
            Ok(reaper) => {
                let mut reapers = REAPERS.lock();
                reapers.retain(|reaper| !reaper.is_finished());
                reapers.push(reaper);
            }
            Err(error) => log::warn!(target: "zz::agent", "could not reap Claude Code: {error}"),
        }
    }
}

static REAPERS: parking_lot::Mutex<Vec<thread::JoinHandle<()>>> =
    parking_lot::Mutex::new(Vec::new());

pub(crate) fn join_reapers() {
    let reapers = std::mem::take(&mut *REAPERS.lock());
    for reaper in reapers {
        let _ = reaper.join();
    }
}

fn reap(child: &mut Child) {
    let deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < deadline {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    signal_group(child, Signal::Terminate);
    let deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < deadline {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    signal_group(child, Signal::Kill);
    let _ = child.wait();
}

#[derive(Clone, Copy)]
enum Signal {
    Terminate,
    Kill,
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "signal the process group zz created for this Claude Code child"
)]
fn signal_group(child: &mut Child, signal: Signal) {
    let Ok(pid) = libc::pid_t::try_from(child.id()) else {
        return;
    };
    let signal = match signal {
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };
    unsafe {
        libc::killpg(pid, signal);
    }
}

#[cfg(not(unix))]
fn signal_group(child: &mut Child, _signal: Signal) {
    let _ = child.kill();
}

impl Process {
    pub(crate) fn launch(
        command: &mut Command,
        provider: AgentProvider,
        agent: &str,
        generation: u64,
        events: &Sender<ChildEvent>,
        tail: &StderrTail,
    ) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let tape = Tape::start(
            provider,
            command.get_program(),
            command.get_args(),
            command.get_current_dir(),
        );
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn_unmasked()
            .map_err(|error| format!("could not start {agent}: {error}"))?;
        let (stdin_tx, stdin_rx) = async_channel::unbounded::<String>();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let process = Self {
            child: Some(child),
            stdin: stdin_tx,
            tape: tape.clone(),
        };
        if let Some(mut stdin) = stdin {
            let tape = tape.clone();
            spawn_thread("zz-agent-in", move || {
                while let Ok(line) = stdin_rx.recv_blocking() {
                    if let Some(tape) = &tape {
                        tape.line(Direction::In, &line);
                    }
                    if stdin
                        .write_all(line.as_bytes())
                        .and_then(|()| stdin.write_all(b"\n"))
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        }
        if let Some(stdout) = stdout {
            let events = events.clone();
            let agent = agent.to_owned();
            spawn_thread("zz-agent-out", move || {
                let mut reader = BufReader::with_capacity(1 << 16, stdout);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line.len() > MAX_FRAME_BYTES => {
                            log::warn!(target: "zz::agent", "dropping a {} byte {agent} frame", line.len());
                        }
                        Ok(_) => {
                            if let Some(tape) = &tape {
                                tape.line(Direction::Out, &String::from_utf8_lossy(&line));
                            }
                            if events
                                .send_blocking(ChildEvent::Line(
                                    generation,
                                    std::mem::take(&mut line),
                                ))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                let _ = events.send_blocking(ChildEvent::Closed(generation));
            })?;
        }
        if let Some(stderr) = stderr {
            let tail = tail.clone();
            spawn_thread("zz-agent-err", move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::warn!(target: "zz::agent::stderr", "{line}");
                    tail.push(&line);
                }
            })?;
        }
        Ok(process)
    }
}

// Managed app-server daemons listen on a Unix socket, so callers never find
// one to connect to elsewhere.
#[cfg(not(unix))]
impl Process {
    pub(crate) fn connect(
        _path: &std::path::Path,
        _provider: AgentProvider,
        _agent: &str,
        _generation: u64,
        _events: &Sender<ChildEvent>,
    ) -> Result<Self, String> {
        Err("connecting to a managed agent daemon needs a Unix socket".to_owned())
    }
}

#[cfg(unix)]
impl Process {
    pub(crate) fn connect(
        path: &std::path::Path,
        provider: AgentProvider,
        agent: &str,
        generation: u64,
        events: &Sender<ChildEvent>,
    ) -> Result<Self, String> {
        use std::{io::ErrorKind, os::unix::net::UnixStream};

        use tungstenite::{Message, protocol::WebSocketConfig};

        let failed = |error: &dyn std::fmt::Display| format!("could not reach {agent}: {error}");
        let stream = UnixStream::connect(path).map_err(|error| failed(&error))?;
        stream
            .set_read_timeout(Some(SOCKET_HANDSHAKE))
            .and_then(|()| stream.set_write_timeout(Some(SOCKET_HANDSHAKE)))
            .map_err(|error| failed(&error))?;
        let limits = WebSocketConfig::default()
            .max_frame_size(Some(MAX_FRAME_BYTES))
            .max_message_size(Some(MAX_FRAME_BYTES));
        let (mut socket, _) =
            tungstenite::client::client_with_config("ws://localhost/", stream, Some(limits))
                .map_err(|error| failed(&error))?;
        socket
            .get_ref()
            .set_read_timeout(Some(SOCKET_POLL))
            .map_err(|error| failed(&error))?;
        let (outgoing, sent) = async_channel::unbounded::<String>();
        let events = events.clone();
        let tape = Tape::start(provider, path.as_os_str(), std::iter::empty(), None);
        spawn_thread("zz-agent-socket", move || {
            'pump: loop {
                loop {
                    match sent.try_recv() {
                        Ok(text) => {
                            if let Some(tape) = &tape {
                                tape.line(Direction::In, &text);
                            }
                            if socket.send(Message::text(text)).is_err() {
                                break 'pump;
                            }
                        }
                        Err(async_channel::TryRecvError::Empty) => break,
                        Err(async_channel::TryRecvError::Closed) => {
                            let _ = socket.close(None);
                            let _ = socket.flush();
                            return;
                        }
                    }
                }
                match socket.read() {
                    Ok(Message::Text(text)) => {
                        if let Some(tape) = &tape {
                            tape.line(Direction::Out, text.as_str());
                        }
                        let line = text.as_str().as_bytes().to_vec();
                        if events
                            .send_blocking(ChildEvent::Line(generation, line))
                            .is_err()
                        {
                            while let Ok(text) = sent.recv_blocking() {
                                if socket.send(Message::text(text)).is_err() {
                                    break;
                                }
                            }
                            let _ = socket.close(None);
                            let _ = socket.flush();
                            return;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => break,
                }
            }
            if let Some(tape) = &tape {
                tape.exit(&Value::Null);
            }
            let _ = events.send_blocking(ChildEvent::Closed(generation));
        })?;
        Ok(Self {
            child: None,
            stdin: outgoing,
            tape: None,
        })
    }
}

pub(crate) fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), String> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(|_| ())
        .map_err(|error| format!("could not start {name}: {error}"))
}

pub(crate) fn cancelled() -> AgentPromptOutcome {
    AgentPromptOutcome::Finished {
        stop_reason: Value::from("cancelled"),
    }
}

pub(crate) fn prompt_outcome(outcome: Outcome) -> AgentPromptOutcome {
    match outcome {
        Outcome::Completed { stop_reason } => AgentPromptOutcome::Finished {
            stop_reason: Value::from(stop_reason),
        },
        Outcome::Cancelled | Outcome::Dropped => cancelled(),
        Outcome::Failed { message } => AgentPromptOutcome::Failed { message },
    }
}

pub(crate) fn option(id: &str, name: &str, kind: &str) -> Value {
    json!({ "optionId": id, "name": name, "kind": kind })
}

pub(crate) fn fit_update(mut update: Value) -> Value {
    let fits = |update: &Value| {
        validate_payload(&AgentStreamPayload::Update {
            update: update.clone(),
        })
        .is_ok()
    };
    if fits(&update) {
        return update;
    }
    if let Some(fields) = update.as_object_mut() {
        fields.remove("rawInput");
        fields.remove("rawOutput");
    }
    if fits(&update) {
        return update;
    }
    if update.get("content").is_some_and(Value::is_array) {
        update["content"] = json!([{
            "type": "content",
            "content": { "type": "text", "text": format!("[output larger than {} KiB not shown]", MAX_AGENT_RESULT_BYTES / 1024) },
        }]);
    } else if update["content"]["type"] == "text"
        && let Some(text) = update["content"]["text"].as_str()
    {
        let mut end = MAX_AGENT_RESULT_BYTES / 2;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        update["content"]["text"] = Value::from(format!("{}…", &text[..end]));
    }
    update
}

pub(crate) fn random_u64() -> u64 {
    getrandom::u64().unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos() as u64)
    })
}

pub(crate) fn new_uuid() -> String {
    let high = random_u64();
    let low = random_u64();
    let high = (high & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    let low = (low & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        high >> 32,
        (high >> 16) & 0xffff,
        high & 0xffff,
        low >> 48,
        low & 0xffff_ffff_ffff
    )
}

pub(crate) enum Rewind {
    Back(usize),
    To(String),
}

impl Rewind {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self::Back(1));
        }
        match text.parse::<usize>() {
            Ok(0) => None,
            Ok(count) => Some(Self::Back(count)),
            Err(_) if !text.contains(char::is_whitespace) => Some(Self::To(text.to_owned())),
            Err(_) => None,
        }
    }

    pub(crate) fn pick(
        &self,
        prompts: usize,
        find: impl Fn(&str) -> Option<usize>,
    ) -> Result<usize, String> {
        match self {
            Self::Back(count) => prompts
                .checked_sub(*count)
                .ok_or_else(|| rewind_shortfall(prompts)),
            Self::To(id) => {
                find(id).ok_or_else(|| "That prompt is no longer in this conversation.".to_owned())
            }
        }
    }
}

const MAX_TITLE_CHARS: usize = 80;
const MAX_TITLE_PROMPT_CHARS: usize = 1000;

#[derive(Default)]
pub(crate) struct Title {
    current: Option<String>,
    asked: bool,
}

impl Title {
    pub(crate) fn start(&mut self, title: Option<String>) -> bool {
        self.asked = false;
        self.rename(title)
    }

    pub(crate) fn rename(&mut self, title: Option<String>) -> bool {
        if self.current == title {
            return false;
        }
        self.current = title;
        true
    }

    pub(crate) fn generated(&mut self, title: Option<String>) -> bool {
        self.current.is_none() && self.rename(title)
    }

    pub(crate) fn ask(&mut self, prompt: &str) -> Option<String> {
        if self.current.is_some() || self.asked {
            return None;
        }
        let subject = title_subject(prompt)?;
        self.asked = true;
        Some(subject.chars().take(MAX_TITLE_PROMPT_CHARS).collect())
    }

    pub(crate) fn current(&self) -> Option<String> {
        self.current.clone()
    }
}

fn title_subject(prompt: &str) -> Option<String> {
    let prompt = prompt.trim();
    if is_relayed(prompt) {
        return None;
    }
    let sent = SentMessage::parse(prompt).map(|message| message.text);
    let subject = sent.as_deref().unwrap_or(prompt).trim();
    let bare_command = subject.starts_with('/') && !subject.contains(char::is_whitespace);
    (!subject.is_empty() && !bare_command).then(|| subject.to_owned())
}

pub(crate) fn verb_echo<'a>(name: &str, rest: &'a str, text: &'a str) -> &'a str {
    if name == "steer" && is_relayed(rest) {
        rest
    } else {
        text.trim()
    }
}

pub(crate) fn clean_title(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let title = line
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_TITLE_CHARS)
        .collect::<String>();
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

pub(crate) fn rewind_shortfall(prompts: usize) -> String {
    match prompts {
        0 => "Nothing to rewind yet: send a prompt first.".to_owned(),
        1 => "There is only 1 prompt to go back over.".to_owned(),
        count => format!("There are only {count} prompts to go back over."),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_title_comes_from_the_task_and_never_from_a_reply_or_a_bare_command() {
        let pane = zz_protocol::PaneId(13);
        let provider = AgentProvider::Codex;
        let reply = zz_protocol::agent_mention::AgentReport::Finished {
            pane,
            provider,
            text: "Updated both generators".to_owned(),
        }
        .to_string();
        let ask = zz_protocol::agent_mention::AgentAsk {
            pane,
            provider,
            what: "execute cargo test".to_owned(),
            answers: zz_protocol::agent_mention::AskAnswers::AllowDeny,
            request: Some(3),
        }
        .to_string();
        let brief = SentMessage {
            sender: zz_protocol::agent_mention::Sender::Chat { pane, provider },
            host: None,
            text: "Fact-check the playbooks".to_owned(),
        }
        .to_string();
        let mut title = Title::default();
        assert_eq!(title.ask("/compact"), None);
        assert_eq!(title.ask(&reply), None);
        assert_eq!(title.ask(&ask), None);
        assert_eq!(
            title.ask("/goal sweep the knowledge folder"),
            Some("/goal sweep the knowledge folder".to_owned())
        );
        assert_eq!(title.ask("a later prompt"), None);
        assert_eq!(
            Title::default().ask(&brief),
            Some("Fact-check the playbooks".to_owned())
        );
    }

    #[test]
    fn a_managed_server_frame_as_large_as_a_stdio_line_arrives_whole() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("s");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let size = 20 << 20;
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut socket = tungstenite::accept(stream).expect("handshake");
            socket
                .send(tungstenite::Message::text("x".repeat(size)))
                .expect("send");
            while socket.read().is_ok() {}
        });
        let (events, received) = async_channel::unbounded();
        let process = Process::connect(&path, AgentProvider::Codex, "test agent", 1, &events)
            .expect("connect");
        let first = smol::block_on(futures_lite::future::or(
            async { received.recv().await.ok() },
            async {
                smol::Timer::after(Duration::from_secs(20)).await;
                None
            },
        ));
        let arrived = match first {
            Some(ChildEvent::Line(1, line)) => Some(line.len()),
            _ => None,
        };
        assert_eq!(arrived, Some(size));
        drop(process);
        server.join().expect("server");
    }

    #[test]
    fn join_reapers_waits_until_a_child_that_outlives_stdin_cleans_up() {
        let directory = tempfile::tempdir().expect("tempdir");
        let job_path = directory.path().join("job");
        let script = format!(
            "set -m; sleep 30 & job=$!; trap 'kill $job; exit 0' TERM; echo $job > {}; while :; do sleep 0.05; done",
            job_path.display()
        );
        let mut command = Command::new("/bin/sh");
        command.args(["-c", &script]);
        let (events, _events) = async_channel::unbounded();
        let process = Process::launch(
            &mut command,
            AgentProvider::Codex,
            "test agent",
            1,
            &events,
            &StderrTail::default(),
        )
        .expect("launch");
        let deadline = Instant::now() + Duration::from_secs(5);
        let job = loop {
            if let Some(job) = std::fs::read_to_string(&job_path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                break job;
            }
            assert!(Instant::now() < deadline, "job never started");
            thread::sleep(Duration::from_millis(10));
        };
        let alive = |pid: i32| {
            rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).expect("pid"))
                .is_ok()
        };
        assert!(alive(job));
        drop(process);
        join_reapers();
        let deadline = Instant::now() + Duration::from_secs(1);
        while alive(job) {
            assert!(
                Instant::now() < deadline,
                "the background job outlived its agent"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
