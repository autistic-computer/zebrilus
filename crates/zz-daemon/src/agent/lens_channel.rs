use std::{
    collections::HashMap,
    io::{self, Write as _},
    os::{
        fd::AsRawFd as _,
        unix::{
            fs::PermissionsExt as _,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::{Condvar, Mutex};
use serde_json::{Value, json};
use zz_protocol::PaneId;

const LONG_POLL: Duration = Duration::from_secs(15);
const HANG_UP_CHECK: Duration = Duration::from_millis(250);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const STALE: Duration = Duration::from_secs(3);
const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 20_000;
const MAX_ASKS: usize = 256;
const MAX_HELLOS: usize = 8;
const MAX_POLLS: usize = 32;
const MAX_PANES: usize = 4096;

pub(crate) struct Hub {
    shared: Arc<Shared>,
    path: PathBuf,
}

pub(crate) struct Channel {
    shared: Arc<Shared>,
    pane: PaneId,
    lens: u64,
    greeting: Vec<Value>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    panes: HashMap<PaneId, Pane>,
    lenses: u64,
    closed: bool,
}

#[derive(Default)]
struct Pane {
    lens: u64,
    hellos: Vec<Value>,
    events: Vec<Value>,
    commands: HashMap<String, Vec<Value>>,
    lost: Vec<Value>,
    asks: HashMap<String, Option<Value>>,
    polls: HashMap<String, Poll>,
}

#[derive(Default)]
struct Poll {
    open: usize,
    last: Option<Instant>,
    gone: bool,
}

struct Request {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

impl Hub {
    pub(crate) fn bind(path: PathBuf) -> io::Result<Self> {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let shared = Arc::new(Shared::default());
        let accepting = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("zz-lens-channel".to_owned())
            .spawn(move || accept(&accepting, &listener))?;
        Ok(Self { shared, path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn attach(&self, pane: PaneId) -> Option<Channel> {
        let mut state = self.shared.state.lock();
        if state.closed {
            return None;
        }
        state.lenses += 1;
        let lens = state.lenses;
        let held = state.panes.entry(pane).or_default();
        held.lens = lens;
        held.events.clear();
        held.commands.clear();
        held.lost.clear();
        held.asks.clear();
        let greeting = held.hellos.clone();
        drop(state);
        self.shared.wake.notify_all();
        Some(Channel {
            shared: Arc::clone(&self.shared),
            pane,
            lens,
            greeting,
        })
    }

    #[cfg(test)]
    pub(crate) fn polling(&self, pane: PaneId, instance: &str) -> bool {
        self.shared
            .state
            .lock()
            .panes
            .get(&pane)
            .is_some_and(|held| held.polling(instance))
    }

    pub(crate) fn forget(&self, pane: PaneId) {
        if self.shared.state.lock().panes.remove(&pane).is_some() {
            self.shared.wake.notify_all();
        }
    }

    pub(crate) fn close(&self) {
        {
            let mut state = self.shared.state.lock();
            if state.closed {
                return;
            }
            state.closed = true;
            state.panes.clear();
        }
        self.shared.wake.notify_all();
        drop(UnixStream::connect(&self.path));
        drop(std::fs::remove_file(&self.path));
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.close();
    }
}

impl Channel {
    fn with<T>(&self, use_pane: impl FnOnce(&mut Pane) -> T) -> Option<T> {
        let mut state = self.shared.state.lock();
        state
            .panes
            .get_mut(&self.pane)
            .filter(|held| held.lens == self.lens)
            .map(use_pane)
    }

    pub(crate) fn greeting(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.greeting)
    }

    #[cfg(test)]
    pub(crate) fn hellos(&self) -> Vec<Value> {
        self.with(|held| held.hellos.clone()).unwrap_or_default()
    }

    pub(crate) fn drain(&self) -> Vec<Value> {
        self.with(|held| std::mem::take(&mut held.events))
            .unwrap_or_default()
    }

    pub(crate) fn lost(&self) -> Vec<Value> {
        self.with(|held| std::mem::take(&mut held.lost))
            .unwrap_or_default()
    }

    pub(crate) fn send(&self, instance: &str, command: Value) -> bool {
        let sent = self
            .with(|held| {
                if !held.polling(instance) {
                    return false;
                }
                held.commands
                    .entry(instance.to_owned())
                    .or_default()
                    .push(command);
                true
            })
            .unwrap_or(false);
        if sent {
            self.shared.wake.notify_all();
        }
        sent
    }

    pub(crate) fn abandon(&self, instance: &str) {
        self.with(|held| held.gone(instance));
    }

    pub(crate) fn answer(&self, key: &str, answer: Value) -> bool {
        let answered = self
            .with(|held| {
                let Some(slot @ None) = held.asks.get_mut(key) else {
                    return false;
                };
                *slot = Some(answer);
                true
            })
            .unwrap_or(false);
        if answered {
            self.shared.wake.notify_all();
        }
        answered
    }

    pub(crate) fn polling(&self, instance: &str) -> bool {
        self.with(|held| held.polling(instance)).unwrap_or(false)
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let detached = self
            .with(|held| {
                held.lens = 0;
                held.events.clear();
                held.commands.clear();
                held.lost.clear();
                held.asks.clear();
            })
            .is_some();
        if detached {
            self.shared.wake.notify_all();
        }
    }
}

impl Pane {
    fn polling(&self, instance: &str) -> bool {
        self.polls.get(instance).is_some_and(|poll| {
            !poll.gone && (poll.open > 0 || poll.last.is_some_and(|at| at.elapsed() < STALE))
        })
    }

    fn gone(&mut self, instance: &str) {
        if let Some(poll) = self.polls.get_mut(instance) {
            poll.gone = true;
        }
        self.hellos.retain(|hello| hello["i"] != instance);
        if let Some(queue) = self.commands.remove(instance).filter(|_| self.lens != 0) {
            self.lost.extend(queue);
        }
    }

    fn note(&mut self, event: Value) {
        self.remember(&event);
        if self.lens != 0 && self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }

    fn remember(&mut self, event: &Value) {
        let Some(from) = event["i"].as_str() else {
            return;
        };
        if event["t"] == "hello" {
            self.hellos.retain(|hello| hello["i"] != from);
            if self.hellos.len() >= MAX_HELLOS {
                self.hellos.remove(0);
            }
            self.hellos.push(event.clone());
            return;
        }
        let Some(hello) = self.hellos.iter_mut().find(|hello| hello["i"] == from) else {
            return;
        };
        let main = event["agent"].is_null();
        match event["t"].as_str().unwrap_or_default() {
            "turn" => hello["turn"] = event["turn"].clone(),
            "step" if main && event["turn"].is_string() => hello["turn"] = event["turn"].clone(),
            "done" if main => hello["turn"] = Value::Null,
            "end" => hello["turn"] = Value::Null,
            "model" => hello["model"] = event["model"].clone(),
            _ => {}
        }
    }
}

fn reachable(state: &mut State, pane: PaneId) -> Option<&mut Pane> {
    if state.closed || (state.panes.len() >= MAX_PANES && !state.panes.contains_key(&pane)) {
        return None;
    }
    Some(state.panes.entry(pane).or_default())
}

fn accept(shared: &Arc<Shared>, listener: &UnixListener) {
    for stream in listener.incoming() {
        if shared.state.lock().closed {
            return;
        }
        let Ok(stream) = stream else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        let serving = Arc::clone(shared);
        drop(
            std::thread::Builder::new()
                .name("zz-lens-request".to_owned())
                .spawn(move || drop(serve(&serving, stream))),
        );
    }
}

fn serve(shared: &Shared, mut stream: UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => return respond(&mut stream, 400, &json!({ "error": error.to_string() })),
    };
    let (status, body) = route(shared, &request, &stream);
    let written = respond(&mut stream, status, &body);
    if written.is_err() && request.path == "/commands" {
        undelivered(shared, &request, &body);
    }
    written
}

fn respond(stream: &mut UnixStream, status: u16, body: &Value) -> io::Result<()> {
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Bad Request",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn read_request(stream: &mut impl io::Read) -> io::Result<Request> {
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidData, message.to_owned());
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(invalid("the request ended early"));
        }
        buffer.extend_from_slice(&chunk[..read]);
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut parsed = httparse::Request::new(&mut headers);
        let head = match parsed.parse(&buffer) {
            Ok(httparse::Status::Complete(head)) => head,
            Ok(httparse::Status::Partial) if buffer.len() < MAX_HEAD => continue,
            Ok(httparse::Status::Partial) => return Err(invalid("the request head is too long")),
            Err(error) => return Err(invalid(&error.to_string())),
        };
        let length = parsed
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-length"))
            .and_then(|header| std::str::from_utf8(header.value).ok()?.trim().parse().ok())
            .unwrap_or(0_usize);
        if length > MAX_BODY {
            return Err(invalid("the request body is too large"));
        }
        let target = parsed.path.unwrap_or("/");
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let mut request = Request {
            method: parsed.method.unwrap_or_default().to_owned(),
            path: path.to_owned(),
            query: query.to_owned(),
            body: buffer[head..].to_vec(),
        };
        while request.body.len() < length {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                return Err(invalid("the request body ended early"));
            }
            request.body.extend_from_slice(&chunk[..read]);
        }
        request.body.truncate(length);
        return Ok(request);
    }
}

fn param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        pair.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

fn pane_of(request: &Request) -> Option<PaneId> {
    param(&request.query, "pane")?.parse().ok().map(PaneId)
}

fn route(shared: &Shared, request: &Request, stream: &UnixStream) -> (u16, Value) {
    let Some(pane) = pane_of(request) else {
        return (400, json!({ "error": "a request names its pane" }));
    };
    let body = || serde_json::from_slice::<Value>(&request.body).unwrap_or(Value::Null);
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/events") => events(shared, pane, body()),
        ("GET", "/commands") => {
            let instance = param(&request.query, "instance").unwrap_or_default();
            commands(shared, pane, instance, stream)
        }
        ("POST", "/ask") => ask(shared, pane, body()),
        _ => (404, json!({})),
    }
}

fn events(shared: &Shared, pane: PaneId, body: Value) -> (u16, Value) {
    let Value::Array(events) = body else {
        return (400, json!({ "error": "events come as a list" }));
    };
    let mut state = shared.state.lock();
    let Some(held) = reachable(&mut state, pane) else {
        return (503, json!({}));
    };
    for event in events {
        held.note(event);
    }
    (200, json!({ "watched": held.lens != 0 }))
}

#[allow(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "peek one byte without blocking to see whether the plugin hung up"
)]
fn hung_up(stream: &UnixStream) -> bool {
    let mut byte = 0_u8;
    let peeked = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            (&raw mut byte).cast(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    peeked == 0
}

fn commands(shared: &Shared, pane: PaneId, instance: &str, stream: &UnixStream) -> (u16, Value) {
    let deadline = Instant::now() + LONG_POLL;
    let mut state = shared.state.lock();
    let Some(held) = reachable(&mut state, pane) else {
        return (503, json!({}));
    };
    if held.polls.len() >= MAX_POLLS && !held.polls.contains_key(instance) {
        held.polls
            .retain(|_, poll| poll.open > 0 || poll.last.is_some_and(|at| at.elapsed() < STALE));
    }
    let poll = held.polls.entry(instance.to_owned()).or_default();
    poll.open += 1;
    poll.gone = false;
    let watched = held.lens != 0;
    let mut gone = false;
    let taken = loop {
        if state.closed {
            break Vec::new();
        }
        let Some(held) = state.panes.get_mut(&pane) else {
            break Vec::new();
        };
        if let Some(queue) = held
            .commands
            .get_mut(instance)
            .filter(|queue| !queue.is_empty())
        {
            break std::mem::take(queue);
        }
        let now = Instant::now();
        if (held.lens != 0) != watched || now >= deadline {
            break Vec::new();
        }
        if hung_up(stream) {
            gone = true;
            break Vec::new();
        }
        shared
            .wake
            .wait_until(&mut state, deadline.min(now + HANG_UP_CHECK));
    };
    let Some(held) = state.panes.get_mut(&pane) else {
        return (200, json!({ "commands": taken, "watched": false }));
    };
    if let Some(poll) = held.polls.get_mut(instance) {
        poll.open = poll.open.saturating_sub(1);
        poll.last = Some(Instant::now());
        gone &= poll.open == 0;
    }
    if gone {
        log::debug!(target: "zz::agent", "the Claude Code plugin in {pane} hung up");
        held.gone(instance);
    }
    (200, json!({ "commands": taken, "watched": held.lens != 0 }))
}

fn undelivered(shared: &Shared, request: &Request, body: &Value) {
    let (Some(pane), Some(instance)) = (pane_of(request), param(&request.query, "instance")) else {
        return;
    };
    let mut state = shared.state.lock();
    let Some(held) = state.panes.get_mut(&pane) else {
        return;
    };
    if let Some(commands) = body["commands"].as_array().filter(|_| held.lens != 0) {
        held.lost.extend(commands.iter().cloned());
    }
    held.gone(instance);
}

fn ask(shared: &Shared, pane: PaneId, body: Value) -> (u16, Value) {
    let pass = || (200, json!({ "pass": true }));
    let Some(key) = body["key"].as_str().map(str::to_owned) else {
        return (400, json!({ "error": "an ask needs a key" }));
    };
    let deadline = Instant::now() + LONG_POLL;
    let mut state = shared.state.lock();
    let Some(held) = reachable(&mut state, pane) else {
        return pass();
    };
    let lens = held.lens;
    if lens == 0 {
        return pass();
    }
    if !held.asks.contains_key(&key) {
        if held.asks.len() >= MAX_ASKS {
            held.asks.retain(|_, answer| answer.is_none());
        }
        held.asks.insert(key.clone(), None);
        held.note(body);
    }
    loop {
        let Some(held) = state
            .panes
            .get(&pane)
            .filter(|held| held.lens == lens && !state.closed)
        else {
            return pass();
        };
        match held.asks.get(&key) {
            Some(Some(answer)) => return (200, answer.clone()),
            Some(None) => {}
            None => return pass(),
        }
        if Instant::now() >= deadline {
            return (200, json!({ "pending": true }));
        }
        shared.wake.wait_until(&mut state, deadline);
    }
}

#[cfg(test)]
pub(crate) fn request(
    path: &std::path::Path,
    method: &str,
    target: &str,
    body: &str,
) -> (u16, Value) {
    use std::io::Read as _;
    let mut stream = UnixStream::connect(path).expect("connect to the lens channel");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: zz\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("write the request");
    let mut reply = String::new();
    stream.read_to_string(&mut reply).expect("read the reply");
    let (head, body) = reply.split_once("\r\n\r\n").expect("a reply head");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

#[cfg(test)]
#[path = "lens_channel_tests.rs"]
mod tests;
