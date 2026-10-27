use super::*;

const DAY: u64 = 24 * 60 * 60;

fn command(name: &str) -> RecentKey {
    RecentKey::Command(name.to_owned())
}

#[test]
fn a_pick_decays_by_half_every_three_days() {
    let mut recents = Recents::default();
    recents.record(command("split-window"), "split-window", 0);
    assert!((recents.frecency(&command("split-window"), 3 * DAY) - 0.5).abs() < 1e-9);
    recents.record(command("split-window"), "split-window", 3 * DAY);
    assert!((recents.frecency(&command("split-window"), 3 * DAY) - 1.5).abs() < 1e-9);
    assert_eq!(recents.frecency(&command("kill-server"), 0), 0.0);
}

#[test]
fn boosts_grow_with_picks_and_stop_at_a_cap() {
    let mut recents = Recents::default();
    assert_eq!(recents.boost(&command("a"), 0), 0);
    recents.record(command("a"), "a", 0);
    assert_eq!(recents.boost(&command("a"), 0), 4);
    for _ in 0..100 {
        recents.record(command("a"), "a", 0);
    }
    assert_eq!(recents.boost(&command("a"), 0), 12);
}

#[test]
fn top_lists_one_kind_by_frecency() {
    let mut recents = Recents::default();
    recents.record(command("old"), "old", 0);
    recents.record(command("old"), "old", 0);
    recents.record(command("new"), "new", 10 * DAY);
    recents.record(RecentKey::Gui("theme".to_owned()), "Theme", 10 * DAY);
    let top = recents.top(RecentKind::Command, 3, 10 * DAY);
    assert_eq!(
        top.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
        vec!["new", "old"]
    );
    assert_eq!(recents.top(RecentKind::Command, 1, 10 * DAY).len(), 1);
}

#[test]
fn the_weakest_entry_goes_past_the_limit() {
    let mut recents = Recents::default();
    recents.record(command("weak"), "weak", 0);
    for index in 0..RECENT_LIMIT {
        recents.record(command(&index.to_string()), "x", DAY);
    }
    assert_eq!(recents.entries().len(), RECENT_LIMIT);
    assert_eq!(recents.frecency(&command("weak"), DAY), 0.0);
}

#[test]
fn recents_survive_a_json_round_trip() {
    let mut recents = Recents::default();
    recents.record(
        RecentKey::Place {
            host: "xps".to_owned(),
            session: SessionId(7),
            window: Some(WindowId(41)),
            pane: Some(PaneId(52)),
        },
        "cargo test",
        5,
    );
    recents.record(
        RecentKey::File {
            host: "macbook".to_owned(),
            path: "~/dev/zz/Cargo.toml".to_owned(),
            dir: false,
        },
        "Cargo.toml",
        6,
    );
    recents.record(RecentKey::Gui("pane-gaps=on".to_owned()), "Pane gaps", 7);
    let json = recents.to_json();
    assert_eq!(Recents::from_json(&json), recents);
    assert_eq!(Recents::from_json("not json"), Recents::default());
}
