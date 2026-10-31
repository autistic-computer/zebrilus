use std::rc::Rc;

use zz_client::{completion::PaneKindAvailability, recents::Recents, verbs::Verb};
use zz_protocol::{PaneId, SessionId, WindowId};

use super::*;
use crate::command::{
    PaletteChat, PaletteHostId, PaletteTreeHost, PaletteTreeSession, PaletteTreeWindow,
    palette_model::{PaletteAction, PaletteMode},
};

fn pane(id: u64, index: u32, label: &str, needs_you: bool) -> PaletteTreePane {
    PaletteTreePane {
        id: PaneId(id),
        index,
        label: label.to_owned(),
        detail: if needs_you {
            "Agent · waiting for input"
        } else {
            "Terminal"
        }
        .into(),
        program: None,
        icon: IconName::SquareTerminal,
        running: false,
        needs_you,
        terminal: true,
        cwd: String::new(),
    }
}

fn window(
    id: u64,
    index: u32,
    name: &str,
    active: bool,
    panes: Vec<PaletteTreePane>,
) -> PaletteTreeWindow {
    PaletteTreeWindow {
        id: WindowId(id),
        index,
        name: name.to_owned(),
        active,
        active_pane: panes[0].id,
        panes,
    }
}

fn tree() -> PaletteTree {
    PaletteTree {
        attached: PaletteHostId(0),
        chats: Vec::new(),
        chat_mode: false,
        hosts: vec![
            PaletteTreeHost {
                id: PaletteHostId(0),
                name: "macbook".to_owned(),
                detail: "This Mac".to_owned(),
                right: "2 sessions".to_owned(),
                status: PaletteStatus::Online,
                sessions: vec![
                    PaletteTreeSession {
                        id: SessionId(3),
                        name: "Working on foo".to_owned(),
                        tmux_name: "Working on foo".to_owned(),
                        active: true,
                        windows: vec![
                            window(
                                10,
                                1,
                                "claude",
                                true,
                                vec![pane(20, 0, "Review sidebar", true)],
                            ),
                            window(
                                11,
                                2,
                                "cargo watch",
                                false,
                                vec![pane(21, 0, "cargo", false)],
                            ),
                        ],
                    },
                    PaletteTreeSession {
                        id: SessionId(5),
                        name: "zz 1".to_owned(),
                        tmux_name: "1".to_owned(),
                        active: false,
                        windows: vec![window(
                            12,
                            1,
                            "nvim",
                            false,
                            vec![pane(22, 0, "flat.rs", false)],
                        )],
                    },
                ],
            },
            PaletteTreeHost {
                id: PaletteHostId(1),
                name: "xps".to_owned(),
                detail: "ssh://xps".to_owned(),
                right: "1 session".to_owned(),
                status: PaletteStatus::Online,
                sessions: vec![PaletteTreeSession {
                    id: SessionId(7),
                    name: "ci".to_owned(),
                    tmux_name: "ci".to_owned(),
                    active: false,
                    windows: vec![window(
                        41,
                        3,
                        "cargo test",
                        false,
                        vec![pane(52, 0, "cargo", false)],
                    )],
                }],
            },
        ],
    }
}

fn availability() -> PaneKindAvailability {
    PaneKindAvailability {
        browser: true,
        editor: true,
    }
}

fn palette(chips: Vec<Chip>) -> UnifiedPalette {
    let mut palette = UnifiedPalette::new(None);
    palette.chips = chips;
    palette.verbs = Rc::from(zz_client::verbs::verbs(availability(), &[], true, ""));
    palette
}

fn auto_host() -> Chip {
    let fleet = palette_fleet(&tree());
    Chip {
        auto: true,
        ..chip_for(&fleet, Place::host(0)).expect("host chip")
    }
}

fn session_chip() -> Chip {
    let fleet = palette_fleet(&tree());
    chip_for(
        &fleet,
        Place {
            host: Some(0),
            session: Some(SessionId(3)),
            window: None,
            pane: None,
        },
    )
    .expect("session chip")
}

fn verb(palette: &UnifiedPalette, name: &str) -> Verb {
    palette
        .verbs
        .iter()
        .find(|verb| verb.name == name)
        .cloned()
        .expect("verb")
}

fn rows(
    palette: &mut UnifiedPalette,
    text: &str,
    history: &[String],
    recents: &Recents,
) -> Vec<String> {
    rows_in(palette, text, history, recents, tree())
}

fn rows_in(
    palette: &mut UnifiedPalette,
    text: &str,
    history: &[String],
    recents: &Recents,
    tree: PaletteTree,
) -> Vec<String> {
    palette.rebuild_places(text, history, recents, 0, tree, availability(), &|_| None);
    palette
        .rows
        .iter()
        .map(|entry| {
            if entry.action.is_some() {
                format!("  {}", entry.row.label)
            } else {
                entry.row.label.to_string()
            }
        })
        .collect()
}

#[test]
fn the_empty_palette_lists_what_needs_you_recents_then_this_hosts_sessions() {
    let mut recents = Recents::default();
    recents.record(
        RecentKey::Place {
            host: "xps".to_owned(),
            session: SessionId(7),
            window: Some(WindowId(41)),
            pane: None,
        },
        "cargo test",
        0,
    );
    recents.record(
        RecentKey::Command("capture-pane -p -t %52".to_owned()),
        "capture-pane -p -t ci:3.0",
        0,
    );
    let mut palette = palette(vec![auto_host()]);
    assert_eq!(
        rows(&mut palette, "", &["split-window -h".to_owned()], &recents),
        [
            "Needs you",
            "  macbook › Working on foo › 1 claude",
            "Recent places",
            "  xps › ci › 3 cargo test",
            "Recent actions",
            "  capture-pane -p -t ci:3.0",
            "  split-window",
            "Sessions on macbook",
            "  Working on foo",
            "  zz 1",
            "Other hosts",
            "  xps",
        ]
    );
}

fn chat_tree(chat_mode: bool) -> PaletteTree {
    let chat = |host, session, window, pane, title: &str| PaletteChat {
        host: PaletteHostId(host),
        session: SessionId(session),
        window: WindowId(window),
        pane: PaneId(pane),
        title: title.to_owned(),
        detail: "Claude Code".to_owned(),
        icon: IconName::SquareTerminal,
        running: false,
    };
    PaletteTree {
        chats: vec![
            chat(0, 3, 10, 20, "Review sidebar"),
            chat(0, 5, 12, 22, "Fix flat rows"),
            chat(1, 7, 41, 52, "Atlas tests"),
        ],
        chat_mode,
        ..tree()
    }
}

#[test]
fn chat_mode_lists_every_chat_with_the_current_one_last() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host()]);
    let listed = rows_in(&mut palette, "", &[], &recents, chat_tree(true));
    let chats = listed.iter().position(|row| row == "Chats").expect("chats");
    assert_eq!(
        listed[chats + 1..chats + 4],
        ["  Fix flat rows", "  Atlas tests", "  Review sidebar"]
    );
    assert!(matches!(
        palette.rows[chats + 1].action,
        Some(PaletteAction::Place(Place {
            window: Some(WindowId(12)),
            pane: None,
            ..
        }))
    ));
    assert_eq!(palette.rows[chats + 2].row.right.as_ref(), "xps");
    assert_eq!(palette.rows[chats + 3].row.right.as_ref(), "current");
    let raw = rows_in(&mut palette, "", &[], &recents, chat_tree(false));
    assert!(!raw.iter().any(|row| row == "Chats"));
}

#[test]
fn typing_in_chat_mode_finds_chats_by_title() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host()]);
    let found = rows_in(&mut palette, "atlas", &[], &recents, chat_tree(true));
    assert_eq!(found[..2], ["Chats", "  Atlas tests"]);
    let found = rows_in(&mut palette, "atlas", &[], &recents, chat_tree(false));
    assert!(!found.iter().any(|row| row == "Chats"));
}

#[test]
fn typing_matches_places_and_actions_with_no_sigils() {
    let recents = Recents::default();
    let mut scoped = palette(vec![auto_host()]);
    let found = rows(&mut scoped, "foo 2", &[], &recents);
    assert_eq!(found[0], "Places");
    assert_eq!(found[1], "  Working on foo › 2 cargo watch");
    let found = rows(&mut scoped, "rea", &[], &recents);
    assert_eq!(found[0], "Actions");
    assert_eq!(found[1], "  read pane");
    let found = rows(&mut scoped, "cargo", &[], &recents);
    assert!(
        !found.iter().any(|row| row.contains("cargo test")),
        "the host chip filters"
    );
    assert!(!found.contains(&"Hosts".to_owned()));
    let mut fleet = palette(Vec::new());
    assert!(rows(&mut fleet, "xps", &[], &recents).contains(&"Hosts".to_owned()));
    assert!(
        rows(&mut fleet, "cargo", &[], &recents)
            .iter()
            .any(|row| row == "  xps › ci › 3 cargo test")
    );
}

#[test]
fn on_a_place_typing_lists_the_actions_that_take_it_first() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host(), session_chip()]);
    assert_eq!(
        rows(&mut palette, "", &[], &recents),
        ["Windows in Working on foo", "  1 claude", "  2 cargo watch"]
    );
    let found = rows(&mut palette, "kil", &[], &recents);
    assert_eq!(found[0], "Actions on Working on foo");
    assert!(found[1].starts_with("  kill "));
    assert!(found.iter().all(|row| row != "  detach"));
    let pills = palette.pills();
    assert_eq!(pills.len(), 2);
    assert!(matches!(&pills[1], PalettePill::Place(label) if label == "Working on foo"));
}

#[test]
fn a_verb_waiting_on_a_pane_lists_panes_with_the_current_one_first() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host()]);
    palette.draft = Some(Draft::new(verb(&palette, "read pane")));
    let found = rows(&mut palette, "", &[], &recents);
    assert_eq!(found[0], "Panes on macbook");
    assert_eq!(found[1], "  Working on foo › 1 claude › 0 Review sidebar");
    assert!(matches!(
        palette.rows[1].action,
        Some(PaletteAction::Fill(_))
    ));
    assert_eq!(palette.placeholder(), "Pick a pane");
    let pills = palette.pills();
    assert!(matches!(&pills[1], PalettePill::Verb(label) if label == "read pane"));
    let found = rows(&mut palette, "flat", &[], &recents);
    assert_eq!(found[1], "  zz 1 › 1 nvim › 0 flat.rs");
}

#[test]
fn a_text_slot_offers_what_you_typed_and_other_slots_leave_rows_to_their_level() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host(), session_chip()]);
    let mut draft = Draft::new(verb(&palette, "rename session"));
    draft.filled.push((
        zz_client::verbs::Filled::Place(session_chip().place),
        String::new(),
    ));
    palette.draft = Some(draft);
    assert!(rows(&mut palette, "", &[], &recents).is_empty());
    assert_eq!(palette.placeholder(), "New name");
    assert_eq!(rows(&mut palette, "api", &[], &recents), ["  api"]);
    palette.draft = Some(Draft::new(verb(&palette, "insert path")));
    assert!(rows(&mut palette, "~/dev/", &[], &recents).is_empty());
    assert_eq!(palette.placeholder(), "A folder or file");
}

#[test]
fn a_colon_lists_tmux_commands() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host()]);
    let found = rows(&mut palette, ":kill-w", &[], &recents);
    assert_eq!(found.first().map(String::as_str), Some("  kill-window"));
    assert!(!UnifiedPalette::new(Some(PaletteMode::Command)).is_places());
}

#[test]
fn command_aliases_are_offered_only_on_the_host_they_came_from() {
    let recents = Recents::default();
    let fleet = palette_fleet(&tree());
    let mut palette = palette(vec![auto_host()]);
    palette.verbs = Rc::from(zz_client::verbs::verbs(
        availability(),
        &[],
        true,
        "clean=kill-window",
    ));
    assert!(rows(&mut palette, "clean", &[], &recents).contains(&"  clean".to_owned()));
    palette.chips = vec![chip_for(&fleet, Place::host(1)).expect("xps chip")];
    assert!(!rows(&mut palette, "clean", &[], &recents).contains(&"  clean".to_owned()));
}
