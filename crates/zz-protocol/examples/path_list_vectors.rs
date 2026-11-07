use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    ClientEnvironmentBlob, ClientHello, ClientInstanceId, ClientKind, Event, EventPayload, GitMark,
    Hello, InsertStyle, KeySubscription, PROTOCOL_VERSION, PaneId, PathEntry, PathKind,
    PathListRoot, ProtocolMessage, ShellKind, Subscriptions, TreeSubscription,
    decode_protocol_frame, encode_protocol_message,
};

fn raw_case(name: &str, mut bytes: Vec<u8>) -> Result<Value, Box<dyn Error>> {
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    let canonical = decode_protocol_frame(&bytes)
        .ok()
        .map(|message| encode_protocol_message(&message))
        .transpose()?;
    Ok(
        json!({"name": name, "bytes": bytes, "accepted": canonical.is_some(), "canonical": canonical}),
    )
}

fn add(cases: &mut Vec<Value>, name: &str, message: ProtocolMessage) -> Result<(), Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(&message)?);
    let mut case = raw_case(name, bytes)?;
    case["message"] = serde_json::to_value(message)?;
    cases.push(case);
    Ok(())
}

fn root(style: InsertStyle, cwd: Option<&str>) -> PathListRoot {
    PathListRoot {
        root: "/home/me/src".to_owned(),
        display_root: "~/src/".to_owned(),
        cwd: cwd.map(str::to_owned),
        insert: style,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for request_id in [0, 1, 127, 128, u64::MAX] {
        for pane in [None, Some(PaneId(0)), Some(PaneId(u64::MAX))] {
            for dir in [None, Some(""), Some("~/src/"), Some("é/世界\0../")] {
                add(
                    &mut cases,
                    "request",
                    ProtocolMessage::PathListRequest {
                        request_id,
                        pane,
                        dir: dir.map(str::to_owned),
                    },
                )?;
            }
        }
        add(
            &mut cases,
            "cancel",
            ProtocolMessage::PathListCancel { request_id },
        )?;
        for insert in [
            InsertStyle::Shell(ShellKind::Posix),
            InsertStyle::Shell(ShellKind::Fish),
            InsertStyle::Shell(ShellKind::Pwsh),
            InsertStyle::Shell(ShellKind::Nu),
            InsertStyle::Claude,
            InsertStyle::Codex,
        ] {
            for cwd in [None, Some(""), Some("/home/me/src")] {
                add(
                    &mut cases,
                    "begin",
                    ProtocolMessage::PathListBegin {
                        request_id,
                        result: Ok(root(insert, cwd)),
                    },
                )?;
            }
        }
        add(
            &mut cases,
            "error",
            ProtocolMessage::PathListBegin {
                request_id,
                result: Err("no such directory 世界\0".to_owned()),
            },
        )?;
        for done in [false, true] {
            for truncated in [false, true] {
                for entries in [
                    vec![],
                    vec![PathEntry {
                        rel: "src/".to_owned(),
                        kind: PathKind::Dir,
                        symlink: true,
                    }],
                    vec![
                        PathEntry {
                            rel: "src/main.rs".to_owned(),
                            kind: PathKind::File,
                            symlink: false,
                        },
                        PathEntry {
                            rel: "é世界🦀".to_owned(),
                            kind: PathKind::File,
                            symlink: true,
                        },
                    ],
                ] {
                    add(
                        &mut cases,
                        "chunk",
                        ProtocolMessage::PathListChunk {
                            request_id,
                            entries,
                            done,
                            truncated,
                        },
                    )?;
                }
            }
        }
        add(
            &mut cases,
            "git",
            ProtocolMessage::PathListGit {
                request_id,
                marks: vec![],
            },
        )?;
        add(
            &mut cases,
            "git",
            ProtocolMessage::PathListGit {
                request_id,
                marks: vec![
                    ("src/".to_owned(), GitMark::Modified),
                    ("src/main.rs".to_owned(), GitMark::Added),
                    ("é世界".to_owned(), GitMark::Untracked),
                    (String::new(), GitMark::Conflicted),
                ],
            },
        )?;
    }
    for start_dir in [None, Some(""), Some("~/src/"), Some("é\0世界")] {
        add(
            &mut cases,
            "open",
            ProtocolMessage::Event(Event {
                sequence: u64::MAX,
                payload: EventPayload::OpenPathPicker {
                    pane: PaneId(7),
                    start_dir: start_dir.map(str::to_owned),
                },
            }),
        )?;
    }
    for length in [4096, 4097] {
        let text = "x".repeat(length);
        for message in [
            ProtocolMessage::PathListRequest {
                request_id: 1,
                pane: None,
                dir: Some(text.clone()),
            },
            ProtocolMessage::PathListBegin {
                request_id: 1,
                result: Err(text.clone()),
            },
            ProtocolMessage::PathListBegin {
                request_id: 1,
                result: Ok(PathListRoot {
                    root: text.clone(),
                    ..root(InsertStyle::Claude, None)
                }),
            },
            ProtocolMessage::PathListBegin {
                request_id: 1,
                result: Ok(PathListRoot {
                    display_root: text.clone(),
                    ..root(InsertStyle::Claude, None)
                }),
            },
            ProtocolMessage::PathListBegin {
                request_id: 1,
                result: Ok(PathListRoot {
                    cwd: Some(text.clone()),
                    ..root(InsertStyle::Claude, None)
                }),
            },
            ProtocolMessage::PathListChunk {
                request_id: 1,
                entries: vec![PathEntry {
                    rel: text.clone(),
                    kind: PathKind::File,
                    symlink: false,
                }],
                done: true,
                truncated: false,
            },
            ProtocolMessage::PathListGit {
                request_id: 1,
                marks: vec![(text.clone(), GitMark::Added)],
            },
            ProtocolMessage::Event(Event {
                sequence: 0,
                payload: EventPayload::OpenPathPicker {
                    pane: PaneId(0),
                    start_dir: Some(text),
                },
            }),
        ] {
            add(&mut cases, "text-bound", message)?;
        }
    }
    for count in [50_000, 50_001] {
        add(
            &mut cases,
            "entry-bound",
            ProtocolMessage::PathListChunk {
                request_id: 1,
                entries: vec![
                    PathEntry {
                        rel: String::new(),
                        kind: PathKind::File,
                        symlink: false
                    };
                    count
                ],
                done: false,
                truncated: false,
            },
        )?;
        add(
            &mut cases,
            "mark-bound",
            ProtocolMessage::PathListGit {
                request_id: 1,
                marks: vec![(String::new(), GitMark::Modified); count],
            },
        )?;
    }
    add(
        &mut cases,
        "producer-target-is-not-decoder-bound",
        ProtocolMessage::PathListChunk {
            request_id: 1,
            entries: vec![
                PathEntry {
                    rel: "q".repeat(1024),
                    kind: PathKind::File,
                    symlink: false
                };
                128
            ],
            done: true,
            truncated: false,
        },
    )?;
    let base = encode_protocol_message(&ProtocolMessage::PathListChunk {
        request_id: 1,
        entries: vec![PathEntry {
            rel: "x".to_owned(),
            kind: PathKind::File,
            symlink: false,
        }],
        done: true,
        truncated: false,
    })?;
    for length in 9..base.len() {
        cases.push(raw_case("truncated", base[..length].to_vec())?);
    }
    for (offset, value) in [(13, 255), (14, 2), (15, 2), (16, 2), (17, 2)] {
        let mut bytes = base.clone();
        if offset < bytes.len() {
            bytes[offset] = value;
            cases.push(raw_case("invalid-field", bytes)?);
        }
    }
    let mut trailing = base.clone();
    trailing.push(91);
    cases.push(raw_case("trailing", trailing)?);
    let mut overlong = base;
    overlong.splice(9..10, [0x81, 0x00]);
    cases.push(raw_case("nonminimal-id", overlong)?);
    let mut hellos = Vec::new();
    for search in [false, true] {
        for paths in [false, true] {
            for origin in [None, Some(PaneId(99))] {
                let mut capabilities = vec![
                    "client-terminal-v1".to_owned(),
                    "client-utf8-v1".to_owned(),
                    "control-plane-v2".to_owned(),
                ];
                if search {
                    capabilities.push("client-native-terminal-search-v1".to_owned());
                }
                if paths {
                    capabilities.push(ClientHello::CLIENT_PATH_PICKER_CAPABILITY.to_owned());
                }
                let hello = ProtocolMessage::Hello(Hello {
                    client: ClientHello {
                        protocol_version: PROTOCOL_VERSION,
                        client_instance_id: ClientInstanceId(42),
                        kind: ClientKind::Interactive,
                        device_name: Some("zz Mojo".to_owned()),
                        capabilities,
                        color_scheme: None,
                        origin,
                        working_directory: None,
                        environment: Vec::new(),
                        process_id: 7,
                    },
                    viewport: None,
                    subscriptions: Subscriptions {
                        tree: TreeSubscription::All,
                        status: true,
                        options: 31,
                        keys: KeySubscription::None,
                        pane_stream: true,
                    },
                    attach: None,
                    environment: ClientEnvironmentBlob::from_bytes(Vec::new()),
                });
                hellos.push(json!({"search":search,"paths":paths,"origin":origin.map(|id|id.0),"bytes":encode_protocol_message(&hello)?}));
            }
        }
    }
    println!("{}", json!({"cases": cases,"hellos":hellos}));
    Ok(())
}
