use std::path::PathBuf;

use serde_json::json;

use super::{Known, born, choose};

const PROJECT: &str = "proj";

fn thread(id: &str, born: u64, name: Option<&str>) -> Known {
    Known {
        id: id.to_owned(),
        born,
        name: name.map(str::to_owned),
        path: None,
        cwd: PathBuf::from("/work/proj"),
    }
}

fn picked(threads: &[Known], mine: u64, others: &[u64], title: Option<&str>) -> Option<String> {
    choose(threads, mine, others, title, PROJECT).map(|thread| thread.id.clone())
}

#[test]
fn a_thread_id_carries_the_time_it_was_made() {
    assert_eq!(
        born("01a194d9-8031-7481-b3a6-9e43c148a57b"),
        Some(1_793_498_644_529)
    );
    assert_eq!(born("2c3bdb36-3c6e-4f6a-9d40-0c0ef9c1e5a1"), None);
    assert_eq!(born("next"), None);
}

#[test]
fn a_fresh_thread_is_read_by_its_id_not_its_created_at() {
    let (id, known) = Known::of(&json!({
        "id": "01a194d9-8031-7481-b3a6-9e43c148a57b", "cwd": "/work/proj", "ephemeral": false,
        "parentThreadId": null, "threadSource": "user", "createdAt": 1_793_498_657u64,
        "name": null, "path": "/work/codex/sessions/rollout-x.jsonl",
    }))
    .unwrap();
    assert_eq!(id, "01a194d9-8031-7481-b3a6-9e43c148a57b");
    let known = known.expect("a top-level thread");
    assert_eq!(known.born, 1_793_498_644_529);
    assert_eq!(known.name, None);
    for skipped in [
        json!({ "id": "a", "cwd": "/work/proj", "ephemeral": true, "createdAt": 1 }),
        json!({ "id": "b", "cwd": "/work/proj", "parentThreadId": "a", "createdAt": 1 }),
        json!({ "id": "c", "cwd": "/work/proj", "threadSource": "thread_title", "createdAt": 1 }),
    ] {
        assert!(Known::of(&skipped).unwrap().1.is_none(), "{skipped}");
    }
}

#[test]
fn a_tui_alone_in_its_folder_owns_the_one_thread_made_after_it_started() {
    let threads = [
        thread("old", 900_000, Some("Earlier work")),
        thread("fresh", 1_003_500, None),
    ];
    assert_eq!(
        picked(&threads, 1_000, &[], Some("proj")).as_deref(),
        Some("fresh")
    );
    let two = [
        thread("fresh", 1_003_500, None),
        thread("new", 1_050_000, None),
    ];
    assert_eq!(picked(&two, 1_000, &[], Some("proj")), None);
}

#[test]
fn tuis_started_apart_each_own_the_thread_made_in_their_turn() {
    let threads = [
        thread("first", 1_003_000, None),
        thread("second", 1_013_000, None),
    ];
    assert_eq!(
        picked(&threads, 1_000, &[1_010], None).as_deref(),
        Some("first")
    );
    assert_eq!(
        picked(&threads, 1_010, &[1_000], None).as_deref(),
        Some("second")
    );
}

#[test]
fn tuis_started_too_close_together_wait_for_the_title() {
    let threads = [
        thread("first", 1_003_000, None),
        thread("second", 1_004_000, None),
    ];
    assert_eq!(picked(&threads, 1_000, &[1_001], None), None);
    assert_eq!(picked(&threads, 1_001, &[1_000], Some("proj")), None);
    let named = [
        thread("first", 1_003_000, Some("Reply with pong")),
        thread("second", 1_004_000, Some("Count to five")),
    ];
    assert_eq!(
        picked(&named, 1_001, &[1_000], Some("Count to five | proj")).as_deref(),
        Some("second")
    );
    assert_eq!(
        picked(&named, 1_000, &[1_001], Some("⠋ Reply with pong | proj")).as_deref(),
        Some("first")
    );
}

#[test]
fn a_title_naming_several_threads_falls_back_to_the_start_order() {
    let threads = [
        thread("first", 1_003_000, Some("Reply with pong")),
        thread("second", 1_013_000, Some("Reply with pong")),
    ];
    assert_eq!(
        picked(&threads, 1_010, &[1_000], Some("Reply with pong | proj")).as_deref(),
        Some("second")
    );
    let close = [
        thread("first", 1_003_000, Some("Reply with pong")),
        thread("second", 1_004_000, Some("Reply with pong")),
    ];
    assert_eq!(
        picked(&close, 1_001, &[1_000], Some("Reply with pong | proj")),
        None
    );
}

#[test]
fn a_resumed_thread_is_found_by_its_name_alone() {
    let threads = [
        thread("resumed", 500_000, Some("Fix the parser")),
        thread("other", 1_013_000, None),
    ];
    assert_eq!(
        picked(&threads, 1_000, &[1_010], Some("● ⠙ Fix the parser | proj")).as_deref(),
        Some("resumed")
    );
}

#[test]
fn the_title_matches_whole_segments_as_the_cli_shortens_them() {
    let long = "Investigate why the release pipeline drops the notarized build artifacts";
    let threads = [
        thread("long", 1_003_000, Some(long)),
        thread("short", 1_013_000, Some("Investigate")),
    ];
    let shown = format!("{}... | proj", &long[..45]);
    assert_eq!(
        picked(&threads, 1_000, &[1_001], Some(&shown)).as_deref(),
        Some("long")
    );
    assert_eq!(
        picked(&threads, 1_000, &[1_001], Some("Investigate | proj")).as_deref(),
        Some("short")
    );
    assert_eq!(
        picked(&threads, 1_000, &[1_001], Some("Investigate why | proj")),
        None
    );
}

#[test]
fn a_thread_named_after_the_folder_does_not_match_the_folder_segment() {
    let threads = [
        thread("named", 1_003_000, Some("proj")),
        thread("mine", 1_004_000, None),
    ];
    assert_eq!(picked(&threads, 1_001, &[1_000], Some("proj")), None);
}

#[test]
fn a_created_at_past_any_real_time_is_kept_without_overflow() {
    let (_, known) = Known::of(&json!({
        "id": "2c3bdb36-3c6e-4f6a-9d40-0c0ef9c1e5a1", "cwd": "/work/proj",
        "createdAt": u64::MAX,
    }))
    .unwrap();
    assert_eq!(known.expect("a top-level thread").born, u64::MAX);
}
