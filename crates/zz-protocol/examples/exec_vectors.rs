use std::{error::Error, path::Path};

use serde_json::{Value, json};
use zz_protocol::{
    ClientEnvironmentBlob, ClientInstanceId, ClientMessageKind, ClientPath, CommandInvocation,
    Event, EventPayload, ExecExit, ExecFlags, ExecOutcome, ExecRequest, ExecResume, ExecResumeKind,
    PaneId, PreparedCommand, PreparedCommandResult, ProtocolMessage, RawText, ServerError,
    SourceSpan, decode_protocol_frame, encode_protocol_message, split_command_words,
};

fn request() -> ExecRequest {
    ExecRequest {
        protocol_version: 118,
        flags: ExecFlags::default(),
        client_instance_id: ClientInstanceId(u64::MAX),
        origin: Some(PaneId(u64::MAX)),
        working_directory: ClientPath::from_path(Path::new("/tmp/世界")),
        tty: Some("/dev/pts/17".to_owned()),
        size: Some((u16::MAX, 0)),
        features: u32::MAX,
        startup_reentry: Some(0),
        spawned_server_id: Some(u64::MAX),
        expect_server_id: Some(128),
        process_id: u32::MAX,
        environment: ClientEnvironmentBlob::from_bytes(b"A=first\0A=last\0B=\0RAW=\xff\0".to_vec()),
        commands: vec![invocation()],
        raw_control_line: Some("display-message '世界'\n".to_owned()),
    }
}

fn invocation() -> CommandInvocation {
    let mut command = CommandInvocation::new(
        "display-message",
        [
            RawText::from("世界"),
            RawText::from_bytes(vec![0, 255, 128, 10]),
        ],
    )
    .with_source(SourceSpan {
        source: "source 世界\n".to_owned(),
        line: u32::MAX,
        column: 0,
    })
    .with_command_blocks([1, 0])
    .into_expanded_alias_group();
    command.set_stdin(RawText::from_bytes(vec![0, 255, 10]));
    command.set_stdin_available(true);
    command
}

fn raw_fixture(bytes: &[u8], kind: &str) -> Result<Value, Box<dyn Error>> {
    let canonical = decode_protocol_frame(bytes)
        .ok()
        .map(|message| encode_protocol_message(&message))
        .transpose()?;
    Ok(json!({"kind":kind,"bytes":bytes,"canonical":canonical,"accepted":canonical.is_some()}))
}

fn fixture(message: &ProtocolMessage, kind: &str) -> Result<Value, Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(message)?);
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    raw_fixture(&bytes, kind)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for sequence in [0, 127, 128, u64::MAX] {
        for output in [
            Vec::new(),
            vec![0, 255, 128, 10],
            "世界\n".as_bytes().to_vec(),
        ] {
            cases.push(fixture(
                &ProtocolMessage::Event(Event {
                    sequence,
                    payload: EventPayload::CommandStdout {
                        output: RawText::from_bytes(output),
                    },
                }),
                "notice",
            )?);
        }
        cases.push(fixture(
            &ProtocolMessage::Event(Event {
                sequence,
                payload: EventPayload::CommandClientExit,
            }),
            "notice",
        )?);
        for pane in [None, Some(PaneId(0)), Some(PaneId(u64::MAX))] {
            for kind in [
                ClientMessageKind::Info,
                ClientMessageKind::Success,
                ClientMessageKind::Warning,
                ClientMessageKind::Error,
            ] {
                for text in ["", "世界", "line\n"] {
                    cases.push(fixture(
                        &ProtocolMessage::Event(Event {
                            sequence,
                            payload: EventPayload::ClientMessage {
                                pane,
                                kind,
                                text: text.to_owned(),
                            },
                        }),
                        "notice",
                    )?);
                }
            }
        }
    }
    for bits in 0_u16..256 {
        let mut request = request();
        for (index, flag) in [
            ExecFlags::UTF8,
            ExecFlags::STDIN_AVAILABLE,
            ExecFlags::NESTED,
            ExecFlags::RESUME,
            ExecFlags::PREPARED,
            ExecFlags::LAST,
            ExecFlags::CONTROL,
            ExecFlags::REMOTE,
        ]
        .into_iter()
        .enumerate()
        {
            request.flags.set(flag, bits & (1 << index) != 0);
        }
        cases.push(fixture(&ProtocolMessage::Exec(request), "request")?);
    }
    for mask in 0_u16..256 {
        let mut request = request();
        if mask & 1 == 0 {
            request.origin = None;
        }
        if mask & 2 == 0 {
            request.working_directory = None;
        }
        if mask & 4 == 0 {
            request.tty = None;
        }
        if mask & 8 == 0 {
            request.size = None;
        }
        if mask & 16 == 0 {
            request.startup_reentry = None;
        }
        if mask & 32 == 0 {
            request.spawned_server_id = None;
        }
        if mask & 64 == 0 {
            request.expect_server_id = None;
        }
        if mask & 128 == 0 {
            request.raw_control_line = None;
        }
        request.commands = vec![CommandInvocation::new("", Vec::<RawText>::new())];
        cases.push(fixture(&ProtocolMessage::Exec(request), "request")?);
    }
    for environment in [
        Vec::new(),
        b"\0\0".to_vec(),
        b"NAME=value".to_vec(),
        b"NAME=\0NAME=next\0".to_vec(),
        b"=bad\0".to_vec(),
        b"NAME\0".to_vec(),
        b"A=\0\0B=x\0".to_vec(),
        [b"A=".as_slice(), &vec![b'x'; 16_365]].concat(),
        [b"A=".as_slice(), &vec![b'x'; 16_366]].concat(),
        b"A=x\0".repeat(4096),
        b"A=x\0".repeat(4097),
    ] {
        let mut request = request();
        request.environment = ClientEnvironmentBlob::from_bytes(environment);
        cases.push(fixture(&ProtocolMessage::Exec(request), "request")?);
    }
    for length in [1024, 1025, 16_384, 16_385, 65_536, 65_537] {
        let mut request = request();
        if length <= 1025 {
            request.tty = Some("t".repeat(length));
        } else if length <= 16_385 {
            request.working_directory = ClientPath::from_path(Path::new(&"p".repeat(length)));
        } else {
            request.raw_control_line = Some("c".repeat(length));
        }
        cases.push(fixture(&ProtocolMessage::Exec(request), "request")?);
    }
    for kind in [ExecResumeKind::NewSession, ExecResumeKind::NativeAttach] {
        for canonical in [None, Some(String::new()), Some("new-session".to_owned())] {
            for alias_matched in [false, true] {
                for failed in [false, true] {
                    let outcome = ExecOutcome::Resume(ExecResume {
                        kind,
                        commands: vec![PreparedCommand {
                            invocation: invocation(),
                            canonical_name: canonical.clone(),
                            alias_matched,
                            result: if failed {
                                PreparedCommandResult::Error(ServerError::PostAdmissionCallback(
                                    Box::new(ServerError::NativeCommandParse("世界".to_owned())),
                                ))
                            } else {
                                PreparedCommandResult::Ready
                            },
                        }],
                    });
                    cases.push(fixture(
                        &ProtocolMessage::ExecExit(ExecExit {
                            server_id: u64::MAX,
                            outcome,
                        }),
                        "exit",
                    )?);
                }
            }
        }
    }
    for server_id in [0, 127, 128, u64::MAX] {
        for outcome in [
            ExecOutcome::Ran,
            ExecOutcome::ServerMismatch,
            ExecOutcome::Rejected(ServerError::InvalidCommand("bad 世界".to_owned())),
            ExecOutcome::Rejected(ServerError::PaneExited(PaneId(u64::MAX))),
            ExecOutcome::Resume(ExecResume {
                kind: ExecResumeKind::NewSession,
                commands: Vec::new(),
            }),
        ] {
            cases.push(fixture(
                &ProtocolMessage::ExecExit(ExecExit { server_id, outcome }),
                "exit",
            )?);
        }
    }
    for words in [
        Vec::<RawText>::new(),
        vec![";".into(), ";".into()],
        vec!["one;".into(), "two".into()],
        vec!["one\\;".into(), "arg".into(), ";".into(), "two".into()],
        vec!["one\\\\;".into(), ";".into(), "two".into()],
        vec!["one;two".into(), "arg;".into(), "two".into(), "".into()],
        vec!["display-message".into(), RawText::from_bytes(vec![255, 59])],
        vec!["世界".into(), "a\n;".into(), ";".into()],
    ] {
        let commands = split_command_words(words.clone())
            .into_iter()
            .filter_map(|mut words| {
                if words.is_empty() {
                    return None;
                }
                let name = words.remove(0).to_string();
                Some(CommandInvocation::new(name, words))
            })
            .collect();
        let mut request = request();
        request.commands = commands;
        let mut case = fixture(&ProtocolMessage::Exec(request), "words")?;
        let mut input = vec![0; 8];
        input.extend(postcard::to_stdvec(&words)?);
        case["words"] = json!(input);
        cases.push(case);
    }
    let base: Vec<u8> = serde_json::from_value(
        cases
            .iter()
            .find(|case| case["kind"] == "request")
            .ok_or("missing request fixture")?["bytes"]
            .clone(),
    )?;
    for cut in 8..base.len() {
        let mut bytes = base[..cut].to_vec();
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        cases.push(raw_fixture(&bytes, "request")?);
    }
    let mut extended = base;
    extended.extend_from_slice(&[0, 255, 128]);
    let length = u32::try_from(extended.len() - 4)?;
    extended[..4].copy_from_slice(&length.to_le_bytes());
    cases.push(raw_fixture(&extended, "request")?);
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
