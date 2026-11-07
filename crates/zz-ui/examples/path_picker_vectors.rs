use std::{collections::BTreeMap, error::Error, rc::Rc};

use serde_json::{Value, json};
use zz_client::{
    path_insert::insert_text,
    place::{ActionKind, Fleet, Level, LevelSet, Place},
};
use zz_protocol::{
    Event, EventPayload, InsertStyle, LayoutNode, MuxSnapshot, PaneBorderIndicators,
    PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneSummary,
    PathKind, ProtocolMessage, SessionId, SessionSnapshot, ShellKind, WindowId, WindowSnapshot,
    encode_protocol_message,
};
use zz_ui::{
    command::{ActionContext, PaletteHostId},
    files,
};

fn uint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        bytes.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    uint(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn optional_text(bytes: &mut Vec<u8>, value: Option<&str>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        text(bytes, value);
    }
}

fn optional_id(bytes: &mut Vec<u8>, value: Option<u64>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        uint(bytes, value);
    }
}

fn record(cases: &mut Vec<Value>, name: &str, request: &[u8], expected: &[u8]) {
    cases.push(json!({"name":name,"request":request,"expected":expected}));
}

fn window(id: u64, phase: usize) -> WindowSnapshot {
    let panes: BTreeMap<_, _> = (0..4)
        .map(|index| {
            let key = id * 10 + index;
            let pane = PaneSnapshot {
                id: PaneId(if phase == 2 && index == 1 { 999 } else { key }),
                title: String::new(),
                kind: if index == 3 {
                    PaneKindSnapshot::Picker
                } else {
                    PaneKindSnapshot::Terminal
                },
                synchronized_input: false,
                bell: false,
                dead: index == 2,
                dead_status: None,
                border_colour: None,
                active_border_colour: None,
                border_status_text: String::new(),
                mode: None,
                status: None,
                summary: PaneSummary {
                    cwd: format!("/home/me/folder-{key}"),
                    process: vec![
                        if index == 0 {
                            "/usr/bin/ssh"
                        } else if phase == 1 {
                            "SSH"
                        } else {
                            "C:\\bin\\ssh.exe"
                        }
                        .to_owned(),
                    ],
                    ..PaneSummary::default()
                },
            };
            (PaneId(key), pane)
        })
        .collect();
    WindowSnapshot {
        id: WindowId(id),
        index: 0,
        name: String::new(),
        automatic_rename: false,
        active_pane: PaneId(id * 10),
        zoomed_pane: None,
        layout: LayoutNode::Empty,
        panes,
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::default(),
        pane_border_lines: PaneBorderLines::default(),
        pane_border_indicators: PaneBorderIndicators::default(),
        pane_order: Vec::new(),
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 0,
    }
}

fn contexts(cases: &mut Vec<Value>) -> Result<(), Box<dyn Error>> {
    for phase in 0..3 {
        let snapshot = MuxSnapshot {
            generation: 1,
            focused_window: Some(WindowId(10)),
            sessions: if phase == 0 {
                Vec::new()
            } else {
                vec![
                    SessionSnapshot {
                        id: SessionId(1),
                        name: String::new(),
                        active_window: WindowId(20),
                        windows: vec![window(10, phase), window(20, phase)],
                        viewers: Vec::new(),
                    },
                    SessionSnapshot {
                        id: SessionId(2),
                        name: String::new(),
                        active_window: WindowId(30),
                        windows: vec![window(30, phase)],
                        viewers: Vec::new(),
                    },
                ]
            },
        };
        let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::Snapshot(snapshot.clone()),
        }))?;
        let places = [
            (None, None, None),
            (Some(1), None, None),
            (Some(2), None, None),
            (None, Some(10), None),
            (Some(1), Some(10), None),
            (None, None, Some(100)),
            (None, None, Some(101)),
            (None, None, Some(102)),
            (Some(2), Some(999), Some(100)),
            (None, None, Some(999)),
        ];
        for (session, window, pane) in places {
            let place = files::FilesPlace {
                host: PaletteHostId(3),
                session: session.map(SessionId),
                window: window.map(WindowId),
                pane: pane.map(PaneId),
            };
            for here in [false, true] {
                for insert in [
                    None,
                    Some(PaneId(100)),
                    Some(PaneId(101)),
                    Some(PaneId(102)),
                    Some(PaneId(999)),
                ] {
                    for home in [None, Some("/home/me")] {
                        for available in [false, true] {
                            let mut request = vec![7];
                            uint(&mut request, frame.len() as u64);
                            request.extend_from_slice(&frame);
                            request.push(u8::from(available));
                            uint(&mut request, 3);
                            optional_id(&mut request, session);
                            optional_id(&mut request, window);
                            optional_id(&mut request, pane);
                            request.push(u8::from(here));
                            optional_id(&mut request, insert.map(|id| id.0));
                            optional_text(&mut request, home);
                            let mut expected = Vec::new();
                            text(
                                &mut expected,
                                &files::start_folder(
                                    &place,
                                    here,
                                    available.then_some(&snapshot),
                                    insert,
                                    home,
                                ),
                            );
                            optional_id(
                                &mut expected,
                                files::listing_pane(&snapshot, insert).map(|id| id.0),
                            );
                            record(cases, "snapshot-context", &request, &expected);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    let paths = [
        "",
        "/",
        "//",
        "/a/./b/../c/",
        "/../../x/",
        "~/",
        "~",
        "~other/../a/",
        "relative",
        "relative/../x/",
        "a/../../b/",
        "../a/../../",
        "//a///b/",
        "λ/世界/../é/",
        "a\\b/",
        "C:/work/",
        "/home/me/src/",
        "/home/meow/",
    ];
    for path in paths {
        let mut request = vec![0];
        text(&mut request, path);
        let mut expected = Vec::new();
        text(&mut expected, &files::normalize_dir(path));
        record(&mut cases, "normalize", &request, &expected);
        for root in [None, Some("/"), Some("/home/me/src"), Some("~/elsewhere/")] {
            let mut request = vec![2];
            text(&mut request, path);
            optional_text(&mut request, root);
            let mut expected = Vec::new();
            optional_text(&mut expected, files::parent_dir(path, root).as_deref());
            record(&mut cases, "parent", &request, &expected);
        }
        for start in ["~/", "/home/me/src/", "relative/", "/"] {
            let mut request = vec![1];
            text(&mut request, path);
            text(&mut request, start);
            let value = files::split_rest(path, start);
            let mut expected = Vec::new();
            text(&mut expected, &value.dir);
            text(&mut expected, &value.typed);
            text(&mut expected, &value.filter);
            record(&mut cases, "split", &request, &expected);
        }
        for home in [
            None,
            Some(""),
            Some("/"),
            Some("/home/me"),
            Some("/home/me/"),
        ] {
            let mut request = vec![4];
            optional_text(&mut request, Some(path));
            optional_text(&mut request, home);
            let mut expected = Vec::new();
            text(&mut expected, &files::start_dir(Some(path), home));
            record(&mut cases, "start", &request, &expected);
        }
        for rel in ["", "a/", "λ/世界///"] {
            for host in [None, Some(""), Some("remote")] {
                let mut request = vec![3];
                text(&mut request, path);
                text(&mut request, rel);
                optional_text(&mut request, host);
                let mut expected = Vec::new();
                text(&mut expected, &files::plain_path(path, rel, host));
                record(&mut cases, "plain", &request, &expected);
            }
        }
    }
    let roots = [
        "/home/me/src",
        "/",
        "relative/root",
        "C:\\work\\src",
        "c:/work/src",
    ];
    let rels = [
        "",
        ".",
        "..",
        "main.rs",
        "sub/file.txt",
        "sp ace",
        "quote's",
        "%job",
        "@item",
        "1file",
        ".1",
        "+2",
        "a,b",
        "with\\slash",
        "双语/🦀",
        "hash'#'##",
        "smart‘’‚‛",
        "control\0",
        "line\n",
        "nonbreak\u{a0}space",
    ];
    let styles = [
        InsertStyle::Shell(ShellKind::Posix),
        InsertStyle::Shell(ShellKind::Fish),
        InsertStyle::Shell(ShellKind::Pwsh),
        InsertStyle::Shell(ShellKind::Nu),
        InsertStyle::Claude,
        InsertStyle::Codex,
    ];
    for root in roots {
        for rel in rels {
            for cwd in [
                None,
                Some("/home/me/src"),
                Some("/home/me"),
                Some("C:/work/src"),
            ] {
                for (index, style) in styles.iter().copied().enumerate() {
                    for absolute in [false, true] {
                        for kind in [PathKind::File, PathKind::Dir] {
                            let mut request = vec![5];
                            text(&mut request, root);
                            text(&mut request, rel);
                            request.push(u8::from(kind == PathKind::Dir));
                            optional_text(&mut request, cwd);
                            request.push(index as u8);
                            request.push(u8::from(absolute));
                            let mut expected = Vec::new();
                            optional_text(
                                &mut expected,
                                insert_text(root, rel, kind, cwd, style, absolute).as_deref(),
                            );
                            record(&mut cases, "insert", &request, &expected);
                        }
                    }
                }
            }
        }
    }
    for mask in 0_u8..16 {
        for supplied in 0_u8..16 {
            for here in [0, 3, u32::MAX] {
                let place = Place {
                    host: (supplied & 1 != 0).then_some(4),
                    session: (supplied & 2 != 0).then_some(SessionId(11)),
                    window: (supplied & 4 != 0).then_some(WindowId(22)),
                    pane: (supplied & 8 != 0).then_some(PaneId(33)),
                };
                let mut written = LevelSet::default();
                for (index, level) in [Level::Host, Level::Session, Level::Window, Level::Pane]
                    .into_iter()
                    .enumerate()
                {
                    if mask & (1 << index) != 0 {
                        written = written.with(level);
                    }
                }
                let context = ActionContext {
                    kind: ActionKind::Files,
                    fleet: Rc::new(Fleet {
                        hosts: Vec::new(),
                        here,
                    }),
                    place,
                    written,
                    host_auto: true,
                    rest: String::new(),
                    rest_range: 0..0,
                    insert_pane: Some(PaneId(99)),
                    focus: None,
                };
                let result = files::files_place(&context);
                let mut request = vec![6];
                optional_id(&mut request, place.host.map(u64::from));
                optional_id(&mut request, place.session.map(|id| id.0));
                optional_id(&mut request, place.window.map(|id| id.0));
                optional_id(&mut request, place.pane.map(|id| id.0));
                request.push(mask);
                uint(&mut request, u64::from(here));
                let mut expected = Vec::new();
                uint(&mut expected, u64::from(result.host.0));
                optional_id(&mut expected, result.session.map(|id| id.0));
                optional_id(&mut expected, result.window.map(|id| id.0));
                optional_id(&mut expected, result.pane.map(|id| id.0));
                record(&mut cases, "written-scope", &request, &expected);
            }
        }
    }
    contexts(&mut cases)?;
    println!("{}", json!({"cases":cases}));
    Ok(())
}
