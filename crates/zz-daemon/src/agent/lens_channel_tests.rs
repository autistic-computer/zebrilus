use std::{
    io::Write as _,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use zz_protocol::PaneId;

use super::{Hub, request};

const PANE: PaneId = PaneId(3);

fn hub() -> (tempfile::TempDir, Hub, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lens.sock");
    let hub = Hub::bind(path.clone()).unwrap();
    (root, hub, path)
}

fn post(path: &Path, events: &Value) -> (u16, Value) {
    request(path, "POST", "/events?pane=3", &events.to_string())
}

fn poll(path: &Path, instance: &str) -> std::thread::JoinHandle<Value> {
    let path = path.to_path_buf();
    let target = format!("/commands?pane=3&instance={instance}");
    std::thread::spawn(move || request(&path, "GET", &target, "").1)
}

fn until(mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn events_posted_by_the_mod_reach_the_lens_in_order() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    let (status, reply) = post(
        &path,
        &json!([{ "t": "a", "i": "m" }, { "t": "b", "i": "m" }]),
    );
    assert_eq!(status, 200);
    assert_eq!(reply, json!({ "watched": true }));
    post(&path, &json!([{ "t": "c", "i": "m" }]));
    let kinds = channel
        .drain()
        .iter()
        .map(|event| event["t"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["a", "b", "c"]);
    assert!(channel.drain().is_empty());
    assert_eq!(request(&path, "GET", "/nothing?pane=3", "").0, 404);
    assert_eq!(request(&path, "POST", "/events", "[]").0, 400);
}

#[test]
fn each_pane_hears_only_its_own_plugin() {
    let (_root, hub, path) = hub();
    let three = hub.attach(PANE).unwrap();
    let four = hub.attach(PaneId(4)).unwrap();
    request(&path, "POST", "/events?pane=4", r#"[{"t":"x","i":"m"}]"#);
    assert!(three.drain().is_empty());
    assert_eq!(four.drain().len(), 1);
}

#[test]
fn a_command_reaches_only_the_instance_it_was_sent_to() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    assert!(!channel.polling("one"));
    assert!(!channel.send("one", json!({ "id": 0, "op": "prompt" })));
    let one = poll(&path, "one");
    until(|| channel.polling("one"));
    let two = poll(&path, "two");
    until(|| channel.polling("two"));
    assert!(channel.send("two", json!({ "id": 1, "op": "prompt" })));
    assert!(channel.send("one", json!({ "id": 2, "op": "abort" })));
    assert_eq!(
        one.join().unwrap(),
        json!({ "commands": [{ "id": 2, "op": "abort" }], "watched": true })
    );
    assert_eq!(
        two.join().unwrap()["commands"],
        json!([{ "id": 1, "op": "prompt" }])
    );
    assert!(channel.polling("one"));
}

#[test]
fn an_ask_waits_for_the_lens_and_a_repeat_gets_the_same_answer() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    let body = r#"{"t":"ask","i":"m","key":"k1","tool":"Bash"}"#;
    let waiting = std::thread::spawn({
        let path = path.clone();
        move || request(&path, "POST", "/ask?pane=3", body)
    });
    let mut event = None;
    until(|| {
        event = channel.drain().pop();
        event.is_some()
    });
    assert_eq!(event.unwrap()["key"], "k1");
    assert!(channel.answer("k1", json!({ "decision": { "behavior": "allow" } })));
    assert!(!channel.answer("k1", json!({ "pass": true })));
    assert!(!channel.answer("unknown", json!({ "pass": true })));
    let (_, reply) = waiting.join().unwrap();
    assert_eq!(reply, json!({ "decision": { "behavior": "allow" } }));
    let (_, again) = request(&path, "POST", "/ask?pane=3", body);
    assert_eq!(again, reply);
    assert!(channel.drain().is_empty());
}

#[test]
fn with_no_lens_an_ask_passes_at_once_and_only_the_hello_is_kept() {
    let (_root, hub, path) = hub();
    let (_, reply) = post(
        &path,
        &json!([
            { "t": "hello", "i": "m", "session": "s1", "model": "haiku", "turn": null },
            { "t": "turn", "i": "m", "turn": "t1" },
            { "t": "text", "i": "m", "turn": "t1", "step": 0, "index": 0, "off": 0, "text": "hi" },
            { "t": "model", "i": "m", "model": "sonnet" },
        ]),
    );
    assert_eq!(reply, json!({ "watched": false }));
    let started = Instant::now();
    let (_, passed) = request(
        &path,
        "POST",
        "/ask?pane=3",
        r#"{"t":"ask","i":"m","key":"m:1","tool":"Bash"}"#,
    );
    assert_eq!(passed, json!({ "pass": true }));
    assert!(started.elapsed() < Duration::from_secs(1));

    let mut channel = hub.attach(PANE).unwrap();
    assert!(channel.drain().is_empty());
    assert_eq!(channel.greeting(), channel.hellos());
    assert!(channel.greeting().is_empty());
    let hellos = channel.hellos();
    assert_eq!(hellos.len(), 1);
    assert_eq!(hellos[0]["session"], "s1");
    assert_eq!(hellos[0]["turn"], "t1");
    assert_eq!(hellos[0]["model"], "sonnet");

    post(&path, &json!([{ "t": "done", "i": "m", "turn": "t1" }]));
    assert_eq!(channel.hellos()[0]["turn"], Value::Null);
    assert_eq!(channel.drain().len(), 1);
}

#[test]
fn a_lens_that_opens_wakes_the_plugin_to_start_streaming() {
    let (_root, hub, path) = hub();
    let waiting = poll(&path, "m");
    let started = Instant::now();
    std::thread::sleep(Duration::from_millis(50));
    let channel = hub.attach(PANE).unwrap();
    assert_eq!(
        waiting.join().unwrap(),
        json!({ "commands": [], "watched": true })
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    let waiting = poll(&path, "m");
    until(|| channel.polling("m"));
    drop(channel);
    assert_eq!(waiting.join().unwrap()["watched"], false);
}

#[test]
fn a_plugin_that_hangs_up_stops_counting_at_once_and_its_commands_come_back() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    post(
        &path,
        &json!([{ "t": "hello", "i": "m", "session": "s1", "turn": null }]),
    );
    let mut stream = UnixStream::connect(&path).unwrap();
    write!(
        stream,
        "GET /commands?pane=3&instance=m HTTP/1.1\r\nHost: zz\r\nContent-Length: 0\r\n\r\n"
    )
    .unwrap();
    until(|| channel.polling("m"));
    drop(stream);
    let started = Instant::now();
    until(|| !channel.polling("m"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(channel.hellos().is_empty());
    assert!(!channel.send("m", json!({ "id": 1, "op": "prompt" })));
    assert!(channel.lost().is_empty());
}

#[test]
fn a_command_queued_for_a_plugin_that_left_comes_back_as_lost() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    let waiting = poll(&path, "m");
    until(|| channel.polling("m"));
    assert!(channel.send("m", json!({ "id": 1, "op": "prompt", "text": "hi" })));
    waiting.join().unwrap();
    assert!(channel.polling("m"));
    assert!(channel.send("m", json!({ "id": 2, "op": "prompt", "text": "again" })));
    channel.abandon("m");
    assert!(!channel.polling("m"));
    assert_eq!(
        channel.lost(),
        [json!({ "id": 2, "op": "prompt", "text": "again" })]
    );
    let back = poll(&path, "m");
    until(|| channel.polling("m"));
    hub.forget(PANE);
    assert_eq!(back.join().unwrap()["commands"], json!([]));
    assert!(!channel.polling("m"));
}

#[test]
fn a_closed_hub_lets_a_waiting_ask_pass_and_removes_its_socket() {
    let (_root, hub, path) = hub();
    let channel = hub.attach(PANE).unwrap();
    let waiting = std::thread::spawn({
        let path = path.clone();
        move || request(&path, "POST", "/ask?pane=3", r#"{"key":"k2","i":"m"}"#)
    });
    until(|| !channel.drain().is_empty());
    drop(hub);
    assert_eq!(waiting.join().unwrap().1, json!({ "pass": true }));
    assert!(!path.exists());
    assert!(!channel.polling("m"));
}
