use std::{
    collections::HashMap,
    io::{self, Write as _},
    os::unix::{
        fs::PermissionsExt as _,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::{Condvar, Mutex};
use serde_json::{Value, json};

const LONG_POLL: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const STALE: Duration = Duration::from_secs(3);
const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 20_000;
const MAX_ASKS: usize = 256;

pub(crate) struct Channel {
    shared: Arc<Shared>,
    path: PathBuf,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    events: Vec<Value>,
    commands: HashMap<String, Vec<Value>>,
    asks: HashMap<String, Option<Value>>,
    polls: HashMap<String, Poll>,
    closed: bool,
}

#[derive(Default)]
struct Poll {
    open: usize,
    last: Option<Instant>,
}

struct Request {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

impl Channel {
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

    pub(crate) fn drain(&self) -> Vec<Value> {
        std::mem::take(&mut self.shared.state.lock().events)
    }

    pub(crate) fn send(&self, instance: &str, command: Value) {
        self.shared
            .state
            .lock()
            .commands
            .entry(instance.to_owned())
            .or_default()
            .push(command);
        self.shared.wake.notify_all();
    }

    pub(crate) fn answer(&self, key: &str, answer: Value) -> bool {
        let mut state = self.shared.state.lock();
        let Some(slot @ None) = state.asks.get_mut(key) else {
            return false;
        };
        *slot = Some(answer);
        drop(state);
        self.shared.wake.notify_all();
        true
    }

    pub(crate) fn polling(&self, instance: &str) -> bool {
        self.shared
            .state
            .lock()
            .polls
            .get(instance)
            .is_some_and(|poll| poll.open > 0 || poll.last.is_some_and(|at| at.elapsed() < STALE))
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        self.shared.state.lock().closed = true;
        self.shared.wake.notify_all();
        drop(UnixStream::connect(&self.path));
        drop(std::fs::remove_file(&self.path));
    }
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
    let (status, body) = match read_request(&mut stream) {
        Ok(request) => route(shared, &request),
        Err(error) => (400, json!({ "error": error.to_string() })),
    };
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
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

fn route(shared: &Shared, request: &Request) -> (u16, Value) {
    let body = || serde_json::from_slice::<Value>(&request.body).unwrap_or(Value::Null);
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/events") => events(shared, body()),
        ("GET", "/commands") => {
            let instance = request
                .query
                .split('&')
                .find_map(|pair| pair.strip_prefix("instance="))
                .unwrap_or_default();
            (200, commands(shared, instance))
        }
        ("POST", "/ask") => ask(shared, body()),
        _ => (404, json!({})),
    }
}

fn events(shared: &Shared, body: Value) -> (u16, Value) {
    let Value::Array(events) = body else {
        return (400, json!({ "error": "events come as a list" }));
    };
    let mut state = shared.state.lock();
    let room = MAX_EVENTS.saturating_sub(state.events.len());
    state.events.extend(events.into_iter().take(room));
    (200, json!({}))
}

fn commands(shared: &Shared, instance: &str) -> Value {
    let deadline = Instant::now() + LONG_POLL;
    let mut state = shared.state.lock();
    state.polls.entry(instance.to_owned()).or_default().open += 1;
    let taken = loop {
        let waiting = state
            .commands
            .get_mut(instance)
            .filter(|queue| !queue.is_empty());
        if let Some(queue) = waiting {
            break std::mem::take(queue);
        }
        if state.closed || Instant::now() >= deadline {
            break Vec::new();
        }
        shared.wake.wait_until(&mut state, deadline);
    };
    let poll = state.polls.entry(instance.to_owned()).or_default();
    poll.open = poll.open.saturating_sub(1);
    poll.last = Some(Instant::now());
    Value::Array(taken)
}

fn ask(shared: &Shared, body: Value) -> (u16, Value) {
    let Some(key) = body["key"].as_str().map(str::to_owned) else {
        return (400, json!({ "error": "an ask needs a key" }));
    };
    let deadline = Instant::now() + LONG_POLL;
    let mut state = shared.state.lock();
    if !state.asks.contains_key(&key) {
        if state.asks.len() >= MAX_ASKS {
            state.asks.retain(|_, answer| answer.is_none());
        }
        state.asks.insert(key.clone(), None);
        state.events.push(body);
    }
    loop {
        if state.closed {
            return (200, json!({ "pass": true }));
        }
        if let Some(Some(answer)) = state.asks.get(&key) {
            return (200, answer.clone());
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
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::{Channel, request};

    fn channel() -> (tempfile::TempDir, Channel) {
        let root = tempfile::tempdir().unwrap();
        let channel = Channel::bind(root.path().join("lens.sock")).unwrap();
        (root, channel)
    }

    #[test]
    fn events_posted_by_the_mod_reach_the_lens_in_order() {
        let (_root, channel) = channel();
        let path = channel.path.clone();
        let (status, _) = request(&path, "POST", "/events", r#"[{"t":"a"},{"t":"b"}]"#);
        assert_eq!(status, 200);
        request(&path, "POST", "/events", r#"[{"t":"c"}]"#);
        let kinds = channel
            .drain()
            .iter()
            .map(|event| event["t"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["a", "b", "c"]);
        assert!(channel.drain().is_empty());
        assert_eq!(request(&path, "GET", "/nothing", "").0, 404);
    }

    #[test]
    fn a_command_reaches_only_the_instance_it_was_sent_to() {
        let (_root, channel) = channel();
        let path = channel.path.clone();
        assert!(!channel.polling("one"));
        let waiting = std::thread::spawn({
            let path = path.clone();
            move || request(&path, "GET", "/commands?instance=one", "")
        });
        let started = Instant::now();
        while !channel.polling("one") {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        channel.send("two", json!({ "id": 1, "op": "prompt" }));
        channel.send("one", json!({ "id": 2, "op": "abort" }));
        let (status, commands) = waiting.join().unwrap();
        assert_eq!(status, 200);
        assert_eq!(commands, json!([{ "id": 2, "op": "abort" }]));
        assert!(channel.polling("one"));
        let (_, commands) = request(&path, "GET", "/commands?instance=two", "");
        assert_eq!(commands, json!([{ "id": 1, "op": "prompt" }]));
    }

    #[test]
    fn an_ask_waits_for_the_lens_and_a_repeat_gets_the_same_answer() {
        let (_root, channel) = channel();
        let path = channel.path.clone();
        let body = r#"{"t":"ask","key":"k1","tool":"Bash"}"#;
        let waiting = std::thread::spawn({
            let path = path.clone();
            move || request(&path, "POST", "/ask", body)
        });
        let started = Instant::now();
        let event = loop {
            if let Some(event) = channel.drain().pop() {
                break event;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        };
        assert_eq!(event["key"], "k1");
        assert!(channel.answer("k1", json!({ "decision": { "behavior": "allow" } })));
        assert!(!channel.answer("k1", json!({ "pass": true })));
        assert!(!channel.answer("unknown", json!({ "pass": true })));
        let (_, reply) = waiting.join().unwrap();
        assert_eq!(reply, json!({ "decision": { "behavior": "allow" } }));
        let (_, again) = request(&path, "POST", "/ask", body);
        assert_eq!(again, reply);
        assert!(channel.drain().is_empty());
    }

    #[test]
    fn a_closed_channel_lets_a_waiting_ask_pass_and_removes_its_socket() {
        let (_root, channel) = channel();
        let path = channel.path.clone();
        let waiting = std::thread::spawn({
            let path = path.clone();
            move || request(&path, "POST", "/ask", r#"{"key":"k2"}"#)
        });
        let started = Instant::now();
        while channel.drain().is_empty() {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        drop(channel);
        assert_eq!(waiting.join().unwrap().1, json!({ "pass": true }));
        assert!(!path.exists());
    }
}
