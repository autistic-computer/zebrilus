use zz_client::{completion::PaneKindAvailability, recents::Recents};
use zz_protocol::{PaneId, SessionId, WindowId};

use super::*;
use crate::command::{
    PaletteHostId, PaletteTreeHost, PaletteTreeSession, PaletteTreeWindow,
    palette_model::PaletteMode,
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
    palette
}

fn auto_host() -> Chip {
    let fleet = palette_fleet(&tree());
    Chip {
        auto: true,
        ..chip_for(&fleet, Place::host(0), None).expect("host chip")
    }
}

fn rows(
    palette: &mut UnifiedPalette,
    text: &str,
    history: &[String],
    recents: &Recents,
) -> Vec<String> {
    palette.rebuild_places(text, history, recents, 0, tree(), availability(), &|_| None);
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
fn the_empty_palette_lists_what_needs_you_then_recents_then_hosts() {
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
        RecentKey::Place {
            host: "macbook".to_owned(),
            session: SessionId(3),
            window: Some(WindowId(10)),
            pane: None,
        },
        "claude",
        0,
    );
    let mut palette = palette(vec![auto_host()]);
    assert_eq!(
        rows(&mut palette, "", &["split-window -h".to_owned()], &recents),
        [
            "Needs you",
            "  macbook / Working on foo / @1 claude",
            "Recent places",
            "  xps / ci / @3 cargo test",
            "Recent commands",
            "  split-window",
            "Hosts",
            "  ^macbook",
            "  ^xps",
        ]
    );
}

#[test]
fn a_sigil_lists_its_level_under_the_chips() {
    let mut palette = palette(vec![auto_host()]);
    let recents = Recents::default();
    assert_eq!(
        rows(&mut palette, "$", &[], &recents),
        ["Sessions on macbook", "  $Working on foo", "  $zz 1"]
    );
    assert_eq!(
        rows(&mut palette, "$1", &[], &recents),
        ["Sessions on macbook", "  $zz 1"]
    );
    assert_eq!(
        rows(&mut palette, "$worki@", &[], &recents),
        [
            "Windows in Working on foo",
            "  @1 claude",
            "  @2 cargo watch"
        ]
    );
    assert_eq!(rows(&mut palette, "^", &[], &recents)[0], "Hosts");
    assert!(rows(&mut palette, "$nothing", &[], &recents).is_empty());
}

#[test]
fn plain_text_searches_places_and_commands_and_hosts_only_without_a_host_chip() {
    let recents = Recents::default();
    let mut scoped = palette(vec![auto_host()]);
    let found = rows(&mut scoped, "cargo", &[], &recents);
    assert_eq!(found[0], "Places");
    assert_eq!(found[1], "  Working on foo / @2 cargo watch");
    assert!(
        !found.iter().any(|row| row.contains("cargo test")),
        "the host chip filters"
    );
    assert!(!found.contains(&"Hosts".to_owned()));
    let mut fleet = palette(Vec::new());
    let found = rows(&mut fleet, "xps", &[], &recents);
    assert!(found.contains(&"Hosts".to_owned()));
    let found = rows(&mut fleet, "cargo", &[], &recents);
    assert!(found.iter().any(|row| row == "  xps / ci / @3 cargo test"));
}

#[test]
fn an_entered_level_lists_its_children_and_the_chips_draw_as_pills() {
    let fleet = palette_fleet(&tree());
    let session = chip_for(
        &fleet,
        Place {
            host: Some(0),
            session: Some(SessionId(3)),
            window: None,
            pane: None,
        },
        None,
    )
    .expect("session chip");
    let mut palette = palette(vec![auto_host(), session]);
    assert_eq!(
        rows(&mut palette, "", &[], &Recents::default()),
        [
            "Windows in Working on foo",
            "  @1 claude",
            "  @2 cargo watch"
        ]
    );
    assert_eq!(
        palette.placeholder(),
        "Search Working on foo, or type ^ $ @ % : > & ?"
    );
    let pills = palette.pills();
    assert_eq!(pills.len(), 2);
    assert!(
        matches!(&pills[1], PalettePill::Mode { prefix, label } if prefix == "$" && label == "Working on foo")
    );
}

#[test]
fn a_colon_lists_commands_and_other_actions_leave_rows_to_their_level() {
    let recents = Recents::default();
    let mut palette = palette(vec![auto_host()]);
    let found = rows(&mut palette, ":kill-w", &[], &recents);
    assert_eq!(found.first().map(String::as_str), Some("  kill-window"));
    assert!(rows(&mut palette, ">theme", &[], &recents).is_empty());
    assert!(rows(&mut palette, "&~/dev/", &[], &recents).is_empty());
    assert!(!UnifiedPalette::new(Some(PaletteMode::Command)).is_places());
}
