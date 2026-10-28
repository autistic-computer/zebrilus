use std::collections::BTreeMap;

use zz_protocol::{
    LayoutNode, PaneBorderIndicators, PaneBorderLines, PaneBorderStatus, PaneSnapshot, PaneSummary,
    SessionSnapshot, WindowSnapshot,
};

use super::*;

fn split(rest: &str) -> (String, String, String) {
    let path = split_rest(rest, "~/dev/zz/");
    (path.dir, path.typed, path.filter)
}

fn owned(dir: &str, typed: &str, filter: &str) -> (String, String, String) {
    (dir.to_owned(), typed.to_owned(), filter.to_owned())
}

#[test]
fn the_text_after_the_sigil_is_a_folder_and_a_filter() {
    assert_eq!(split(""), owned("~/dev/zz/", "", ""));
    assert_eq!(split("mai"), owned("~/dev/zz/", "", "mai"));
    assert_eq!(split("src/ma"), owned("~/dev/zz/src/", "src/", "ma"));
    assert_eq!(split("../"), owned("~/dev/", "../", ""));
    assert_eq!(split("./a/./b/"), owned("~/dev/zz/a/b/", "./a/./b/", ""));
    assert_eq!(split("/etc/ho"), owned("/etc/", "/etc/", "ho"));
    assert_eq!(split("~/notes"), owned("~/", "~/", "notes"));
    assert_eq!(split("~"), owned("~/", "~/", ""));
    assert_eq!(split("~fab/dev/"), owned("~fab/dev/", "~fab/dev/", ""));
    assert_eq!(
        split("My Files/a b"),
        owned("~/dev/zz/My Files/", "My Files/", "a b"),
        "spaces are plain characters"
    );
}

#[test]
fn folders_fold_dots_and_doubled_slashes() {
    assert_eq!(normalize_dir("/a//b/../c"), "/a/c/");
    assert_eq!(normalize_dir("/../.."), "/");
    assert_eq!(normalize_dir("~/x/../../"), "~/../");
    assert_eq!(normalize_dir("~"), "~/");
    assert_eq!(normalize_dir("/"), "/");
}

#[test]
fn going_up_stops_at_the_root_and_passes_the_home() {
    assert_eq!(parent_dir("/a/b/", None).as_deref(), Some("/a/"));
    assert_eq!(parent_dir("/a/", None).as_deref(), Some("/"));
    assert_eq!(parent_dir("/", None), None);
    assert_eq!(parent_dir("~/dev/", None).as_deref(), Some("~/"));
    assert_eq!(parent_dir("~/", None).as_deref(), Some("~/../"));
    assert_eq!(
        parent_dir("~/", Some("/Users/fab")).as_deref(),
        Some("/Users/"),
        "the host said where the home is"
    );
    assert_eq!(last_name("~/dev/zz/").as_deref(), Some("zz"));
    assert_eq!(last_name("/Users/fab").as_deref(), Some("fab"));
    assert_eq!(last_name("~/"), None);
}

#[test]
fn a_copied_path_is_plain_and_names_another_host() {
    assert_eq!(
        plain_path("/Users/fab/dev/zz", "Cargo.toml", None),
        "/Users/fab/dev/zz/Cargo.toml"
    );
    assert_eq!(
        plain_path("/home/fab/", "src/", Some("xps")),
        "xps:/home/fab/src"
    );
    assert_eq!(plain_path("/", "etc/hosts", None), "/etc/hosts");
    assert_eq!(
        plain_path("/srv/My Files", "a b.txt", None),
        "/srv/My Files/a b.txt"
    );
}

#[test]
fn a_pane_folder_is_written_under_the_home() {
    let home = Some("/home/me");
    assert_eq!(start_dir(Some("/home/me/dev/zz"), home), "~/dev/zz/");
    assert_eq!(start_dir(Some("/home/me"), home), "~/");
    assert_eq!(start_dir(Some("/home/meow"), home), "/home/meow/");
    assert_eq!(start_dir(Some("/srv/My Files/"), home), "/srv/My Files/");
    assert_eq!(start_dir(Some("/srv"), None), "/srv/");
    assert_eq!(start_dir(Some(""), home), "~/");
    assert_eq!(start_dir(None, home), "~/");
}

fn pane(id: u64, kind: PaneKindSnapshot, cwd: &str) -> (PaneId, PaneSnapshot) {
    let pane = PaneSnapshot {
        id: PaneId(id),
        title: String::new(),
        kind,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary {
            cwd: cwd.to_owned(),
            ..PaneSummary::default()
        },
    };
    (PaneId(id), pane)
}

fn window(id: u64, active: u64, panes: Vec<(PaneId, PaneSnapshot)>) -> WindowSnapshot {
    WindowSnapshot {
        id: WindowId(id),
        index: u32::try_from(id).unwrap(),
        name: format!("w{id}"),
        automatic_rename: true,
        active_pane: PaneId(active),
        zoomed_pane: None,
        layout: LayoutNode::Pane(PaneId(active)),
        panes: panes.into_iter().collect::<BTreeMap<_, _>>(),
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::Off,
        pane_border_lines: PaneBorderLines::Single,
        pane_border_indicators: PaneBorderIndicators::Colour,
        pane_order: Vec::new(),
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 0,
    }
}

pub(crate) fn snapshot() -> MuxSnapshot {
    MuxSnapshot {
        sessions: vec![
            SessionSnapshot {
                id: SessionId(1),
                name: "work".to_owned(),
                active_window: WindowId(2),
                windows: vec![
                    window(
                        1,
                        10,
                        vec![pane(10, PaneKindSnapshot::Terminal, "/home/me/one")],
                    ),
                    window(
                        2,
                        21,
                        vec![
                            pane(20, PaneKindSnapshot::Terminal, "/home/me/two"),
                            pane(21, PaneKindSnapshot::Terminal, "/home/me/three"),
                        ],
                    ),
                ],
                viewers: Vec::new(),
            },
            SessionSnapshot {
                id: SessionId(2),
                name: "web".to_owned(),
                active_window: WindowId(3),
                windows: vec![window(3, 30, vec![pane(30, PaneKindSnapshot::Picker, "")])],
                viewers: Vec::new(),
            },
        ],
        ..MuxSnapshot::default()
    }
}

#[test]
fn the_place_picks_the_folder_files_start_in() {
    let snapshot = snapshot();
    let home = Some("/home/me");
    let host = PaletteHostId(0);
    let start = |place: FilesPlace, here: bool| {
        start_folder(&place, here, Some(&snapshot), Some(PaneId(20)), home)
    };
    assert_eq!(
        start(FilesPlace::host(host), true),
        "~/two/",
        "this host alone: the pane a path goes into"
    );
    assert_eq!(
        start(FilesPlace::host(host), false),
        "~/",
        "another host alone: its home"
    );
    let place = |session, window, pane| FilesPlace {
        host,
        session,
        window,
        pane,
    };
    assert_eq!(start(place(None, None, Some(PaneId(10))), true), "~/one/");
    assert_eq!(
        start(place(None, Some(WindowId(2)), None), true),
        "~/three/",
        "a window: its active pane"
    );
    assert_eq!(
        start(place(Some(SessionId(1)), None, None), false),
        "~/three/",
        "a session: its active window's active pane"
    );
    assert_eq!(
        start(place(Some(SessionId(2)), None, None), true),
        "~/",
        "a pane that is not a terminal has no folder"
    );
}
