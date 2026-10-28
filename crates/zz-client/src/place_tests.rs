use zz_protocol::{PaneId, SessionId, WindowId, catalog_command_spec};

use super::*;
use crate::recents::RecentKey;

fn pane(id: u64, index: u32, title: &str, program: Option<&str>) -> FleetPane {
    FleetPane {
        id: PaneId(id),
        index,
        title: title.to_owned(),
        program: program.map(str::to_owned),
        cwd: "/home/fabrico".to_owned(),
        terminal: true,
        needs_you: false,
    }
}

fn window(id: u64, index: u32, name: &str, active: bool, panes: Vec<FleetPane>) -> FleetWindow {
    FleetWindow {
        id: WindowId(id),
        index,
        name: name.to_owned(),
        active,
        active_pane: panes[0].id,
        panes,
    }
}

fn session(id: u64, name: &str, label: &str, windows: Vec<FleetWindow>) -> FleetSession {
    FleetSession {
        id: SessionId(id),
        name: name.to_owned(),
        label: label.to_owned(),
        windows,
    }
}

fn fleet() -> Fleet {
    Fleet {
        here: 0,
        hosts: vec![
            FleetHost {
                id: 0,
                name: "macbook".to_owned(),
                online: true,
                current: Some(SessionId(3)),
                sessions: vec![
                    session(
                        3,
                        "Working on foo",
                        "Working on foo",
                        vec![
                            window(
                                10,
                                1,
                                "claude",
                                true,
                                vec![
                                    pane(20, 1, "Review sidebar", Some("claude")),
                                    pane(21, 2, "", Some("zsh")),
                                ],
                            ),
                            window(
                                11,
                                2,
                                "cargo watch",
                                false,
                                vec![pane(22, 1, "", Some("cargo"))],
                            ),
                            window(12, 3, "zsh", false, vec![pane(23, 1, "", Some("zsh"))]),
                        ],
                    ),
                    session(
                        5,
                        "web overlay kit",
                        "web overlay kit",
                        vec![window(
                            13,
                            1,
                            "nvim",
                            true,
                            vec![pane(24, 1, "flat.rs", Some("nvim"))],
                        )],
                    ),
                    session(
                        1,
                        "1",
                        "zz 1",
                        vec![window(
                            14,
                            1,
                            "zsh",
                            true,
                            vec![pane(25, 1, "", Some("zsh"))],
                        )],
                    ),
                ],
            },
            FleetHost {
                id: 1,
                name: "xps".to_owned(),
                online: true,
                current: Some(SessionId(7)),
                sessions: vec![
                    session(
                        7,
                        "ci",
                        "ci",
                        vec![
                            window(40, 1, "zsh", true, vec![pane(50, 1, "", Some("zsh"))]),
                            window(
                                41,
                                3,
                                "cargo test",
                                false,
                                vec![pane(52, 1, "", Some("cargo"))],
                            ),
                            window(45, 4, "editor", false, vec![pane(55, 1, "", Some("nvim"))]),
                        ],
                    ),
                    session(
                        8,
                        "notes",
                        "notes",
                        vec![window(
                            44,
                            1,
                            "editor",
                            true,
                            vec![pane(53, 1, "", Some("nvim"))],
                        )],
                    ),
                ],
            },
            FleetHost {
                id: 2,
                name: "devbox".to_owned(),
                online: false,
                current: None,
                sessions: Vec::new(),
            },
        ],
    }
}

fn no_boost(_: &Place) -> i64 {
    0
}

fn place(host: u32, session: u64, window: Option<u64>, pane: Option<u64>) -> Place {
    Place {
        host: Some(host),
        session: Some(SessionId(session)),
        window: window.map(WindowId),
        pane: pane.map(PaneId),
    }
}

fn text(level: Level, query: &str) -> Part {
    Part {
        level,
        query: Query::Text(query.to_owned()),
        range: 0..0,
    }
}

fn labels(found: &[Candidate]) -> Vec<&str> {
    found
        .iter()
        .map(|candidate| candidate.label.as_str())
        .collect()
}

fn shape(
    text: &str,
) -> (
    Vec<(Level, Query)>,
    Option<(ActionKind, String)>,
    Option<String>,
) {
    let line = parse(text);
    (
        line.parts
            .into_iter()
            .map(|part| (part.level, part.query))
            .collect(),
        line.action.map(|action| (action.kind, action.rest)),
        line.search,
    )
}

fn t(text: &str) -> Query {
    Query::Text(text.to_owned())
}

#[test]
fn places_run_to_the_next_sigil_and_actions_take_the_rest() {
    use ActionKind::*;
    use Level::*;
    assert_eq!(
        shape("^macbook$worki@2%1"),
        (
            vec![
                (Host, t("macbook")),
                (Session, t("worki")),
                (Window, t("2")),
                (Pane, t("1")),
            ],
            None,
            None,
        )
    );
    assert_eq!(
        shape("$foo?why is ci red"),
        (
            vec![(Session, t("foo"))],
            Some((Ask, "why is ci red".to_owned())),
            None
        )
    );
    assert_eq!(
        shape("@2:send-keys '$HOME' %1"),
        (
            vec![(Window, t("2"))],
            Some((Command, "send-keys '$HOME' %1".to_owned())),
            None,
        )
    );
    assert_eq!(
        shape("^macbook$Working on foo@1%1&~/dev/foo/"),
        (
            vec![
                (Host, t("macbook")),
                (Session, t("Working on foo")),
                (Window, t("1")),
                (Pane, t("1")),
            ],
            Some((Files, "~/dev/foo/".to_owned())),
            None,
        )
    );
    assert_eq!(
        shape(">theme dark"),
        (vec![], Some((Gui, "theme dark".to_owned())), None)
    );
    assert_eq!(
        shape(":kill-window"),
        (vec![], Some((Command, "kill-window".to_owned())), None)
    );
    assert_eq!(
        shape("$@"),
        (vec![(Session, t("")), (Window, t(""))], None, None)
    );
}

#[test]
fn only_a_deeper_place_sigil_starts_a_part() {
    use Level::*;
    assert_eq!(shape("%fabrico@xps").0, vec![(Pane, t("fabrico@xps"))]);
    assert_eq!(shape("@cost$5").0, vec![(Window, t("cost$5"))]);
    assert_eq!(shape("$a^b").0, vec![(Session, t("a^b"))]);
}

#[test]
fn plain_text_is_a_search_to_the_end() {
    assert_eq!(
        shape("why is ci red?"),
        (vec![], None, Some("why is ci red?".to_owned()))
    );
    assert_eq!(
        shape("cargo:kill"),
        (vec![], None, Some("cargo:kill".to_owned()))
    );
    assert_eq!(shape(""), (vec![], None, None));
    assert_eq!(shape("   "), (vec![], None, None));
}

#[test]
fn quotes_keep_sigils_and_hashes_pin_ids() {
    use Level::*;
    assert_eq!(shape("@\"logs: api\"").0, vec![(Window, t("logs: api"))]);
    assert_eq!(
        shape("$#3@#41%#52").0,
        vec![
            (Session, Query::Id(3)),
            (Window, Query::Id(41)),
            (Pane, Query::Id(52))
        ]
    );
    assert_eq!(shape("@#").0, vec![(Window, t("#"))]);
    assert_eq!(shape("@#4a").0, vec![(Window, t("#4a"))]);
    assert_eq!(shape("@\"#4\"").0, vec![(Window, t("#4"))]);
}

#[test]
fn ranges_point_into_the_text() {
    let text = "$foo@2:kill-window";
    let line = parse(text);
    assert_eq!(&text[line.parts[0].range.clone()], "$foo");
    assert_eq!(&text[line.parts[1].range.clone()], "@2");
    let action = line.action.expect("action");
    assert_eq!(&text[action.rest_range()], "kill-window");
    assert!(is_sigil('&') && is_sigil('^') && !is_sigil('#'));
}

#[test]
fn a_session_matches_its_label_fuzzily_or_its_number_exactly() {
    let fleet = fleet();
    let scope = Place::host(0);
    let found = candidates(&fleet, &scope, &text(Level::Session, "worki"), &no_boost);
    assert_eq!(labels(&found), vec!["Working on foo", "web overlay kit"]);
    let found = candidates(&fleet, &scope, &text(Level::Session, "1"), &no_boost);
    assert_eq!(labels(&found), vec!["zz 1"]);
    let found = candidates(&fleet, &scope, &text(Level::Session, ""), &no_boost);
    assert_eq!(found[0].label, "Working on foo");
    assert_eq!(found.len(), 3);
}

#[test]
fn digits_are_an_index_inside_the_parent() {
    let mut fleet = fleet();
    let found = candidates(
        &fleet,
        &Place::host(0),
        &text(Level::Window, "2"),
        &no_boost,
    );
    assert_eq!(
        found.iter().map(|c| c.place).collect::<Vec<_>>(),
        vec![place(0, 3, Some(11), None)]
    );
    let found = candidates(
        &fleet,
        &Place::default(),
        &text(Level::Pane, "1"),
        &no_boost,
    );
    assert_eq!(
        found.iter().map(|c| c.place).collect::<Vec<_>>(),
        vec![place(0, 3, Some(10), Some(20))]
    );
    fleet.hosts[0].current = Some(SessionId(5));
    let found = candidates(
        &fleet,
        &Place::host(0),
        &text(Level::Window, "2"),
        &no_boost,
    );
    assert!(found.is_empty());
}

#[test]
fn pane_digits_count_from_the_windows_pane_base_index() {
    let snapshot = zz_protocol::WindowSnapshot {
        id: WindowId(10),
        index: 1,
        name: "claude".to_owned(),
        automatic_rename: false,
        active_pane: PaneId(21),
        zoomed_pane: None,
        layout: zz_protocol::LayoutNode::Empty,
        panes: std::collections::BTreeMap::new(),
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: zz_protocol::PaneBorderStatus::Off,
        pane_border_lines: zz_protocol::PaneBorderLines::Single,
        pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
        pane_order: vec![PaneId(21), PaneId(20)],
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 1,
    };
    let mut fleet = fleet();
    for pane in &mut fleet.hosts[0].sessions[0].windows[0].panes {
        pane.index = crate::navigation::pane_index(&snapshot, pane.id);
    }
    let pane_at = |index: &str| {
        candidates(
            &fleet,
            &Place::default(),
            &text(Level::Pane, index),
            &no_boost,
        )
        .iter()
        .map(|c| c.place)
        .collect::<Vec<_>>()
    };
    assert_eq!(pane_at("1"), vec![place(0, 3, Some(10), Some(21))]);
    assert_eq!(pane_at("2"), vec![place(0, 3, Some(10), Some(20))]);
    assert!(pane_at("0").is_empty());
}

#[test]
fn a_name_searches_every_session_under_the_parent() {
    let fleet = fleet();
    let found = candidates(
        &fleet,
        &Place::host(1),
        &text(Level::Window, "editor"),
        &no_boost,
    );
    assert_eq!(
        found.iter().map(|c| c.place).collect::<Vec<_>>(),
        vec![place(1, 7, Some(45), None), place(1, 8, Some(44), None)]
    );
    let found = candidates(
        &fleet,
        &Place::default(),
        &text(Level::Window, "editor"),
        &no_boost,
    );
    assert_eq!(found.len(), 2);
    let found = candidates(
        &fleet,
        &Place::default(),
        &text(Level::Window, "zsh"),
        &no_boost,
    );
    assert_eq!(
        found.iter().take(3).map(|c| c.place).collect::<Vec<_>>(),
        vec![
            place(0, 3, Some(12), None),
            place(0, 1, Some(14), None),
            place(1, 7, Some(40), None),
        ]
    );
    assert_eq!(labels(&found[3..]), vec!["1 claude"]);
}

#[test]
fn ids_match_only_the_id() {
    let fleet = fleet();
    let part = Part {
        level: Level::Window,
        query: Query::Id(41),
        range: 0..0,
    };
    let found = candidates(&fleet, &Place::host(1), &part, &no_boost);
    assert_eq!(labels(&found), vec!["3 cargo test"]);
}

#[test]
fn a_program_finds_its_window_behind_name_matches() {
    let fleet = fleet();
    let found = candidates(
        &fleet,
        &Place::host(0),
        &text(Level::Window, "cargo"),
        &no_boost,
    );
    assert_eq!(labels(&found), vec!["2 cargo watch"]);
    let found = candidates(
        &fleet,
        &Place::host(0),
        &text(Level::Window, "nvim"),
        &no_boost,
    );
    assert_eq!(labels(&found), vec!["1 nvim"]);
    let found = candidates(
        &fleet,
        &Place::host(1),
        &text(Level::Window, "nvim"),
        &no_boost,
    );
    assert!(found.iter().all(|c| c.score >= METADATA_PENALTY));
}

#[test]
fn recent_rows_rise_and_nearer_rows_rank_first() {
    let fleet = fleet();
    let found = candidates(
        &fleet,
        &Place::host(0),
        &text(Level::Window, "zsh"),
        &no_boost,
    );
    assert_eq!(labels(&found), vec!["3 zsh", "1 zsh", "1 claude"]);
    let favourite = place(0, 1, Some(14), None);
    let boost = move |candidate: &Place| if *candidate == favourite { 12 } else { 0 };
    let found = candidates(&fleet, &Place::host(0), &text(Level::Window, "zsh"), &boost);
    assert_eq!(labels(&found), vec!["1 zsh", "3 zsh", "1 claude"]);
}

#[test]
fn the_empty_level_lists_children_and_search_covers_every_level() {
    let fleet = fleet();
    assert_eq!(
        labels(&children(&fleet, &Place::default(), &no_boost)),
        vec!["macbook", "xps", "devbox"]
    );
    assert_eq!(
        labels(&children(&fleet, &Place::host(1), &no_boost)),
        vec!["ci", "notes"]
    );
    let found = search(&fleet, &place(0, 3, None, None), "review", &no_boost);
    assert_eq!(found[0].place, place(0, 3, Some(10), Some(20)));
    assert!(found.iter().all(|c| c.level > Level::Session));
}

#[test]
fn a_line_resolves_part_by_part() {
    let fleet = fleet();
    let line = parse("^xps$ci@3");
    let resolved = resolve(&fleet, &Place::default(), &line.parts, &no_boost);
    assert_eq!(resolved.place, place(1, 7, Some(41), None));
    assert_eq!(resolved.failed, None);
    let line = parse("$worki@9");
    let resolved = resolve(&fleet, &Place::host(0), &line.parts, &no_boost);
    assert_eq!(resolved.place, place(0, 3, None, None));
    assert_eq!(resolved.failed, Some(1));
}

fn target(command: &str, place: &Place) -> Option<String> {
    command_target(catalog_command_spec(command).expect("command"), place)
}

#[test]
fn a_place_compiles_to_the_commands_target_kind() {
    let pane = place(0, 3, Some(11), Some(22));
    let window = place(0, 3, Some(11), None);
    assert_eq!(target("kill-window", &window).as_deref(), Some("@11"));
    assert_eq!(target("kill-window", &pane).as_deref(), Some("@11"));
    assert_eq!(target("resize-pane", &pane).as_deref(), Some("%22"));
    assert_eq!(target("resize-pane", &window).as_deref(), Some("@11"));
    assert_eq!(target("kill-session", &pane).as_deref(), Some("$3"));
    assert_eq!(target("new-window", &window).as_deref(), Some("$3"));
    assert_eq!(target("new-session", &window), None);
    assert_eq!(target("kill-server", &window), None);
    assert_eq!(target("kill-window", &Place::host(0)), None);
}

#[test]
fn a_target_goes_after_the_command_unless_one_is_there() {
    assert_eq!(with_target("kill-window", "@41"), "kill-window -t @41");
    assert_eq!(
        with_target("resize-pane -Z", "%52"),
        "resize-pane -t %52 -Z"
    );
    assert_eq!(
        with_target("new-window -n x", "$7"),
        "new-window -t '$7' -n x"
    );
    assert_eq!(with_target("kill-window -t @1", "@41"), "kill-window -t @1");
    assert_eq!(
        with_target("split-window -dt %1", "%52"),
        "split-window -dt %1"
    );
    assert_eq!(with_target("select-pane -t%1", "%52"), "select-pane -t%1");
    assert_eq!(
        with_target("send-keys -l x ; kill-pane -t %3", "%52"),
        "send-keys -t %52 -l x ; kill-pane -t %3"
    );
}

#[test]
fn a_place_is_written_with_its_host_and_deepest_id() {
    let fleet = fleet();
    assert_eq!(
        written(&fleet, &place(1, 7, Some(41), Some(52))).as_deref(),
        Some("@xps:%52")
    );
    assert_eq!(
        written(&fleet, &place(1, 7, Some(41), None)).as_deref(),
        Some("@xps:@41")
    );
    assert_eq!(
        written(&fleet, &place(1, 7, None, None)).as_deref(),
        Some("@xps:$7")
    );
    assert_eq!(written(&fleet, &Place::host(1)).as_deref(), Some("@xps"));
    assert_eq!(written(&fleet, &Place::default()), None);
}

#[test]
fn both_written_forms_read_back_to_the_place() {
    let fleet = fleet();
    let read = |text| match read_written(text, &fleet) {
        Some(Written::Place(place)) => Some(place),
        _ => None,
    };
    assert_eq!(read("@xps:%52"), Some(place(1, 7, Some(41), Some(52))));
    assert_eq!(read("@xps:@41"), Some(place(1, 7, Some(41), None)));
    assert_eq!(read("@xps:$7"), Some(place(1, 7, None, None)));
    assert_eq!(read("@xps"), Some(Place::host(1)));
    assert_eq!(read("@xps:ci:3.1"), Some(place(1, 7, Some(41), Some(52))));
    assert_eq!(read("@xps:ci:3.%52"), Some(place(1, 7, Some(41), Some(52))));
    assert_eq!(
        read("@macbook:Working\\ on\\ foo:2"),
        Some(place(0, 3, Some(11), None))
    );
    assert_eq!(read("@xps:ghost"), None);
    assert_eq!(read("@xps:%99"), None);
    assert_eq!(read("@nope:ci"), None);
    assert_eq!(read("@ci"), None);
    assert_eq!(read("$ci"), None);
    assert_eq!(
        read_written("@xps:~/src/main\\ file.rs:42:7", &fleet),
        Some(Written::Path {
            host: 1,
            path: "~/src/main file.rs".to_owned(),
            line: Some(42),
        })
    );
}

#[test]
fn a_place_lands_on_a_pane() {
    let fleet = fleet();
    assert_eq!(fleet.current(), place(0, 3, Some(10), Some(20)));
    assert_eq!(
        fleet.landing_pane(&place(1, 7, None, None)).map(|p| p.id),
        Some(PaneId(50))
    );
    assert_eq!(
        fleet.landing_pane(&Place::host(1)).map(|p| p.id),
        Some(PaneId(50))
    );
    assert_eq!(fleet.landing_pane(&Place::host(2)), None);
}

#[test]
fn a_recent_place_comes_back_while_it_is_there() {
    let fleet = fleet();
    let window = place(1, 7, Some(41), None);
    let key = recent_key(&fleet, &window).expect("key");
    assert_eq!(
        key,
        RecentKey::Place {
            host: "xps".to_owned(),
            session: SessionId(7),
            window: Some(WindowId(41)),
            pane: None,
        }
    );
    assert_eq!(recent_place(&fleet, &key), Some(window));
    let gone = RecentKey::Place {
        host: "xps".to_owned(),
        session: SessionId(7),
        window: Some(WindowId(99)),
        pane: None,
    };
    assert_eq!(recent_place(&fleet, &gone), None);
    assert_eq!(recent_key(&fleet, &Place::host(1)), None);
}
