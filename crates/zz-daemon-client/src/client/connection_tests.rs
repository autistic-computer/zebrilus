use std::{ffi::OsString, path::PathBuf};

#[test]
fn a_remote_host_gets_only_the_file_a_buffer_command_names() {
    use std::path::Path;

    use zz_protocol::{ClientFileOperation, ClientFileRequest, CommandInvocation};

    use super::{FILE_NOT_GRANTED, FileGrant, answer_command_file, file_grants, granted};

    let grants = file_grants(&[
        CommandInvocation::new("list-sessions", ["/etc/hosts"]),
        CommandInvocation::new("loadb", ["-b", "notes", "./notes.txt"]),
        CommandInvocation::new("save-buffer", ["/tmp/out"]),
    ]);
    assert_eq!(
        grants,
        [
            FileGrant::Read(PathBuf::from("./notes.txt")),
            FileGrant::Write(PathBuf::from("/tmp/out")),
        ]
    );
    let ask = |path: &str, operation| {
        let request = ClientFileRequest {
            request_id: 7,
            path: path.to_owned(),
            operation,
        };
        granted(&grants, &request).map(Path::to_path_buf)
    };
    let write = || ClientFileOperation::Write {
        append: false,
        data: Vec::new(),
    };
    assert_eq!(
        ask("/srv/home/notes.txt", ClientFileOperation::Read),
        Some(PathBuf::from("./notes.txt"))
    );
    assert_eq!(ask("/tmp/out", write()), Some(PathBuf::from("/tmp/out")));
    assert_eq!(
        ask("/home/me/.ssh/id_ed25519", ClientFileOperation::Read),
        None
    );
    assert_eq!(ask("/etc/hosts", ClientFileOperation::Read), None);
    assert_eq!(ask("/srv/home/notes.txt", write()), None);
    assert_eq!(ask("/tmp/out", ClientFileOperation::Read), None);
    assert_eq!(ask("/home/me/.zshrc/tmp/out", write()), None);
    let refused = answer_command_file(
        &ClientFileRequest {
            request_id: 7,
            path: "/home/me/.ssh/id_ed25519".to_owned(),
            operation: ClientFileOperation::Read,
        },
        Some(&grants),
        false,
        &mut false,
    );
    assert_eq!(refused.data, Vec::<u8>::new());
    assert_eq!(refused.error.as_deref(), Some(FILE_NOT_GRANTED));
}

use zz_protocol::{
    ClientHello, ClientKind, ClientPath, ExecFlags, MAX_CLIENT_ENVIRONMENT_BYTES,
    MAX_CLIENT_ENVIRONMENT_ENTRIES, MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES,
    MAX_CLIENT_WORKING_DIRECTORY_BYTES, RawText,
};

use super::{
    CallerTtyScope, CommandRoute, EndpointFactsScope, MAX_CLIENT_FEATURE_SPECS,
    attach_session_command, client_environment_with, client_features_capabilities,
    client_takes_utf8, client_working_directory, scoped_environment,
    startup_config_owner_capability, terminal_facts_capabilities, terminal_facts_capabilities_with,
};

static SIGTERM_DISPOSITION: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn traced_answers_keep_the_question_and_drop_what_was_typed() {
    let message = zz_protocol::ProtocolMessage::AgentAnswerQuestion {
        pane: zz_protocol::PaneId(3),
        request_id: 9,
        answers: vec![zz_protocol::AgentQuestionAnswer {
            id: "Which token?".to_owned(),
            answers: vec!["hunter2".to_owned()],
        }],
    };
    let traced = format!("{:#?}", super::TracedMessage(&message));
    assert!(traced.contains("Which token?"));
    assert!(!traced.contains("hunter2"));
    let prompt = zz_protocol::ProtocolMessage::AgentCancel {
        pane: zz_protocol::PaneId(3),
    };
    assert_eq!(
        format!("{:#?}", super::TracedMessage(&prompt)),
        format!("{prompt:#?}")
    );
}

#[cfg(unix)]
#[test]
fn an_undecodable_frame_is_skipped_without_losing_the_next_one() {
    use std::io::Write as _;

    let (mut daemon, client) = std::os::unix::net::UnixStream::pair().unwrap();
    let oversized = zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
        sequence: 1,
        payload: zz_protocol::EventPayload::ChooseTree {
            state: Some(zz_protocol::ChooseTreeState {
                items: Vec::new(),
                search: None,
                selected: 0,
                kind: zz_protocol::ChooseTreeKind::Panes,
                filter_no_matches: false,
                prompt: "x".repeat(zz_protocol::MAX_CHOOSE_ITEM_TEXT_BYTES + 1),
                help: false,
            }),
        },
    });
    let next = zz_protocol::ProtocolMessage::Attach {
        session: "next".to_owned(),
    };
    for message in [&oversized, &next] {
        daemon
            .write_all(&zz_protocol::encode_protocol_message(message).unwrap())
            .unwrap();
    }
    let mut receiver = super::ProtocolReceiver::new(client);
    assert_eq!(receiver.recv_decodable().unwrap(), (next, true));
}

#[cfg(unix)]
#[test]
#[allow(
    unsafe_code,
    reason = "query signal dispositions without changing them"
)]
fn stdin_read_restores_signal_disposition_on_success_and_errors() {
    fn disposition() -> libc::sighandler_t {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            assert_eq!(
                libc::sigaction(libc::SIGTERM, std::ptr::null(), &raw mut action),
                0
            );
            action.sa_sigaction
        }
    }
    struct FailedRead;
    impl std::io::Read for FailedRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("read failed"))
        }
    }
    let _disposition = SIGTERM_DISPOSITION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = disposition();
    let command = super::StdinReadSignal::install().expect("command signal");
    let pending = disposition();
    assert_eq!(
        super::read_command_stdin_from(&b"ok"[..], false),
        Ok(b"ok".to_vec())
    );
    assert_eq!(disposition(), pending);
    assert!(super::read_command_stdin_from(FailedRead, false).is_err());
    assert_eq!(disposition(), pending);
    assert!(super::read_command_stdin_from(&b"\xff"[..], false).is_err());
    assert_eq!(disposition(), pending);
    assert!(super::read_command_stdin_from(std::io::repeat(b'x'), true).is_err());
    assert_eq!(disposition(), pending);
    drop(command);
    assert_eq!(disposition(), previous);
}

#[test]
fn stdin_chunk_returns_available_bytes_and_maps_read_errors_to_eio() {
    struct OneRead(Option<&'static [u8]>);
    impl std::io::Read for OneRead {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let bytes = self
                .0
                .take()
                .expect("a chunk read never waits for more input");
            buffer[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        }
    }
    struct FailedRead;
    impl std::io::Read for FailedRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("is a directory"))
        }
    }
    let _disposition = SIGTERM_DISPOSITION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        super::read_command_stdin_chunk_from(OneRead(Some(b"\x1b[31"))),
        Ok(b"\x1b[31".to_vec())
    );
    assert_eq!(
        super::read_command_stdin_chunk_from(OneRead(Some(b""))),
        Ok(Vec::new())
    );
    assert_eq!(
        super::read_command_stdin_chunk_from(FailedRead),
        Err("Input/output error".to_owned())
    );
    assert_eq!(
        super::read_command_stdin_from(FailedRead, true),
        Err("Input/output error".to_owned())
    );
    let chunk = super::read_command_stdin_chunk_from(std::io::repeat(b'x')).expect("chunk");
    assert_eq!(chunk.len(), super::STDIN_CHUNK_BYTES);
}

#[test]
fn attach_session_command_preserves_the_exact_requested_flag_mutation() {
    assert_eq!(
        attach_session_command(
            "work".to_owned(),
            true,
            true,
            Some("ignore-size,!no-detach-on-destroy"),
        ),
        zz_protocol::CommandInvocation::new(
            "attach-session",
            [
                "-d",
                "-r",
                "-f",
                "ignore-size,!no-detach-on-destroy",
                "-t",
                "work",
            ],
        )
    );
}

#[test]
fn local_client_fact_scopes_publish_the_supplied_working_directory() {
    let fixture = PathBuf::from("/tmp/zz-client-cwd-fixture");
    for scope in [
        EndpointFactsScope::LocalHostWorkingDirectory,
        EndpointFactsScope::LocalControlTerminalIdentity,
        EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
    ] {
        assert_eq!(
            client_working_directory(scope, || Some(fixture.clone())),
            ClientPath::from_path(&fixture)
        );
    }
}

#[test]
fn remote_scopes_send_only_terminal_and_locale_names() {
    let environment = [
        ("TERM", "xterm-ghostty"),
        ("COLORTERM", "truecolor"),
        ("LANG", "en_US.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
        ("LC_CTYPE", "UTF-8"),
        ("SSH_AUTH_SOCK", "/var/run/com.apple.launchd.x/Listeners"),
        ("DISPLAY", ":0"),
        ("ZZ_PANE", "%3"),
        ("TERMINFO", "/Applications/zz.app/terminfo"),
    ]
    .map(|(name, value)| (OsString::from(name), OsString::from(value)));
    let names = |facts| {
        scoped_environment(facts, environment.clone())
            .map(|(name, _)| name.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    for facts in [
        EndpointFactsScope::None,
        EndpointFactsScope::PortableTerminalSize,
    ] {
        assert_eq!(
            names(facts),
            ["TERM", "COLORTERM", "LANG", "LC_ALL", "LC_CTYPE"]
        );
    }
    for facts in [
        EndpointFactsScope::LocalHostWorkingDirectory,
        EndpointFactsScope::LocalControlTerminalIdentity,
        EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
    ] {
        assert_eq!(names(facts).len(), environment.len());
    }
}

#[test]
fn remote_client_fact_scopes_never_read_or_publish_a_local_working_directory() {
    for scope in [
        EndpointFactsScope::None,
        EndpointFactsScope::PortableTerminalSize,
    ] {
        assert_eq!(client_working_directory(scope, || panic!()), None);
    }
}

#[test]
fn local_client_fact_scopes_omit_oversized_working_directories() {
    let fixture = PathBuf::from("x".repeat(MAX_CLIENT_WORKING_DIRECTORY_BYTES + 1));
    assert_eq!(
        client_working_directory(EndpointFactsScope::LocalHostWorkingDirectory, || Some(
            fixture
        )),
        None
    );
}

#[cfg(unix)]
#[test]
fn local_client_fact_scopes_carry_non_utf8_working_directory_bytes() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let fixture = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));
    let published = client_working_directory(EndpointFactsScope::LocalHostWorkingDirectory, || {
        Some(fixture.clone())
    })
    .expect("non-UTF-8 working directory is published");
    assert_eq!(published.to_path_buf(), Some(fixture));
}

#[test]
fn the_client_utf8_flag_reads_the_inputs_tmux_c_reads_in_the_order_it_reads_them() {
    let takes = |pairs: &[(&str, &str)]| {
        let pairs = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(*value)))
            .collect::<Vec<_>>();
        client_takes_utf8(|name| {
            pairs
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.clone())
        })
    };

    assert!(!takes(&[]));
    assert!(!takes(&[("LC_ALL", "C"), ("LC_CTYPE", "C"), ("LANG", "C")]));
    assert!(takes(&[("TMUX", "/tmp/zz,1,0")]));
    assert!(takes(&[("TMUX", ""), ("LC_ALL", "C")]));
    assert!(takes(&[("LC_ALL", "en_US.UTF-8")]));
    assert!(takes(&[("LC_ALL", "en_US.utf8")]));
    assert!(takes(&[("LANG", "C.UTF-8")]));
    assert!(!takes(&[("LC_CTYPE", "C"), ("LANG", "en_US.UTF-8")]));
    assert!(takes(&[("LC_CTYPE", ""), ("LANG", "en_US.UTF-8")]));
    assert!(!takes(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")]));
}

#[test]
fn global_feature_flags_travel_as_one_bounded_token_per_flag() {
    let mut capabilities = Vec::new();
    let mut features = vec!["256".to_owned(), "sixel:RGB".to_owned(), "x".repeat(201)];
    features.extend((0..20).map(|index| format!("title{index}")));
    client_features_capabilities(&features, &mut capabilities);
    assert_eq!(capabilities.len(), MAX_CLIENT_FEATURE_SPECS);
    assert_eq!(capabilities[0], "client-features-v1:256");
    assert_eq!(capabilities[1], "client-features-v1:sixel:RGB");
    assert_eq!(capabilities[2], "client-features-v1:title0");
}

#[test]
fn client_environment_is_sorted_deduplicated_and_bounded() {
    let mut environment = (0..MAX_CLIENT_ENVIRONMENT_ENTRIES + 4)
        .rev()
        .map(|index| (format!("ZZ_{index:04}"), index.to_string()))
        .collect::<Vec<_>>();
    environment.extend([
        (String::new(), "empty-name".to_owned()),
        ("BAD=NAME".to_owned(), "value".to_owned()),
        ("NUL_NAME\0".to_owned(), "value".to_owned()),
        ("NUL_VALUE".to_owned(), "value\0".to_owned()),
        ("ZZ_0000".to_owned(), "replacement".to_owned()),
    ]);
    let snapshot = client_environment_with(environment);
    assert_eq!(snapshot.len(), MAX_CLIENT_ENVIRONMENT_ENTRIES);
    assert_eq!(
        snapshot.first().map(crate::RawText::as_str),
        Some("ZZ_0000=replacement")
    );
    assert_eq!(
        snapshot.last().map(crate::RawText::as_str),
        Some("ZZ_4095=4095")
    );
    assert!(snapshot.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn client_environment_skips_oversized_entries_and_caps_aggregate_deterministically() {
    let environment = (0..300)
        .map(|index| {
            (
                format!("ZZ_{index:03}"),
                "x".repeat(MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES - 7),
            )
        })
        .chain([(
            "ZZ_OVERSIZED".to_owned(),
            "x".repeat(MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES),
        )])
        .collect::<Vec<_>>();
    let forward = client_environment_with(environment.clone());
    let reverse = client_environment_with(environment.into_iter().rev());
    assert_eq!(forward, reverse);
    assert_eq!(
        forward.len(),
        MAX_CLIENT_ENVIRONMENT_BYTES / MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES
    );
    assert!(
        forward
            .iter()
            .map(zz_protocol::RawText::byte_len)
            .sum::<usize>()
            <= MAX_CLIENT_ENVIRONMENT_BYTES
    );
    assert!(
        forward
            .iter()
            .all(|entry| entry.len() <= MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES)
    );
    assert!(
        !forward
            .iter()
            .any(|entry| entry.starts_with("ZZ_OVERSIZED="))
    );
}

#[cfg(unix)]
#[test]
fn client_environment_carries_non_utf8_os_strings_as_bytes() {
    use std::os::unix::ffi::OsStringExt;

    let snapshot = client_environment_with([
        (
            OsString::from_vec(vec![b'Z', b'Z', b'_', 0xff]),
            OsString::from("value"),
        ),
        (
            OsString::from("ZZ_VALUE"),
            OsString::from_vec(vec![b'a', 0xff, b'b']),
        ),
        (OsString::from("ZZ_VALID"), OsString::from("kept")),
    ]);
    assert_eq!(
        snapshot
            .iter()
            .map(crate::RawText::as_bytes)
            .collect::<Vec<_>>(),
        [
            b"ZZ_VALID=kept".as_slice(),
            b"ZZ_VALUE=a\xffb".as_slice(),
            b"ZZ_\xff=value".as_slice(),
        ]
    );
}

#[cfg(unix)]
#[test]
fn client_environment_still_rejects_nul_and_malformed_names() {
    use std::os::unix::ffi::OsStringExt;

    let snapshot = client_environment_with([
        (
            OsString::from("ZZ_NUL"),
            OsString::from_vec(vec![b'v', 0, b'x']),
        ),
        (
            OsString::from_vec(vec![b'Z', 0, b'Z']),
            OsString::from("value"),
        ),
        (OsString::from(""), OsString::from("value")),
        (OsString::from("ZZ=EQ"), OsString::from("value")),
        (OsString::from("ZZ_VALID"), OsString::from("kept")),
    ]);
    assert_eq!(
        snapshot
            .iter()
            .map(crate::RawText::as_bytes)
            .collect::<Vec<_>>(),
        [b"ZZ_VALID=kept".as_slice()]
    );
}

#[test]
fn portable_terminal_facts_never_include_a_local_tty() {
    assert!(EndpointFactsScope::PortableTerminalSize.includes_terminal_size());
    assert!(!EndpointFactsScope::PortableTerminalSize.includes_tty());
    assert!(!EndpointFactsScope::LocalControlTerminalIdentity.includes_terminal_size());
    assert!(EndpointFactsScope::LocalControlTerminalIdentity.includes_tty());
    assert!(EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal.includes_terminal_size());
    assert!(EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal.includes_tty());
}

#[test]
fn only_ssh_endpoint_scopes_count_as_remote_for_the_flow_window() {
    assert!(EndpointFactsScope::None.is_remote());
    assert!(EndpointFactsScope::PortableTerminalSize.is_remote());
    assert!(!EndpointFactsScope::LocalHostWorkingDirectory.is_remote());
    assert!(!EndpointFactsScope::LocalControlTerminalIdentity.is_remote());
    assert!(!EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal.is_remote());
}

#[cfg(unix)]
#[test]
fn handshake_advertises_native_ui_only_for_graphical_terminal_clients() {
    use super::{ProtocolReceiver, ProtocolSender, connect_stream_with_startup_owner};
    use crate::transport::{LocalTransport, Transport, TransportListener};
    use zz_protocol::{CommandResponse, ProtocolMessage, ServerError};

    for (kind, client_has_terminal, scope, expected, desktop) in [
        (
            ClientKind::Interactive,
            true,
            EndpointFactsScope::LocalHostWorkingDirectory,
            true,
        ),
        (
            ClientKind::Interactive,
            true,
            EndpointFactsScope::None,
            true,
        ),
        (
            ClientKind::Interactive,
            true,
            EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
            false,
        ),
        (
            ClientKind::Interactive,
            true,
            EndpointFactsScope::PortableTerminalSize,
            false,
        ),
        (
            ClientKind::Interactive,
            false,
            EndpointFactsScope::LocalHostWorkingDirectory,
            false,
        ),
        (
            ClientKind::Command,
            true,
            EndpointFactsScope::LocalHostWorkingDirectory,
            false,
        ),
        (ClientKind::Control, true, EndpointFactsScope::None, false),
    ]
    .into_iter()
    .map(|row| (row, false))
    .chain([(
        (
            ClientKind::Interactive,
            true,
            EndpointFactsScope::LocalHostWorkingDirectory,
            true,
        ),
        true,
    )])
    .map(|((kind, client_has_terminal, scope, expected), desktop)| {
        (kind, client_has_terminal, scope, expected, desktop)
    }) {
        let directory = tempfile::Builder::new()
            .prefix("zz-search-")
            .tempdir_in("/tmp")
            .expect("create socket directory");
        let socket = directory.path().join("daemon.sock");
        let listener = LocalTransport::bind(&socket).expect("bind handshake listener");
        let server = std::thread::spawn(move || {
            let stream = listener.accept().expect("accept client");
            stream
                .set_timeout(Some(std::time::Duration::from_secs(5)))
                .expect("set timeout");
            let mut reader = ProtocolReceiver::new(
                super::TransportStream::try_clone(&stream).expect("clone stream"),
            );
            let mut writer = ProtocolSender::new(stream);
            let hello = match reader.recv().expect("receive handshake") {
                ProtocolMessage::ClientHello(hello) => hello,
                ProtocolMessage::Hello(hello) => hello.into_client(),
                _ => panic!("expected client handshake"),
            };
            writer
                .send(&ProtocolMessage::CommandResponse(CommandResponse::Error {
                    request_id: 0,
                    error: ServerError::Internal("handshake captured".to_owned()),
                    output: RawText::default(),
                }))
                .expect("finish handshake");
            hello
        });
        let extra: &[&str] = if desktop {
            &[ClientHello::CLIENT_PATH_PICKER_CAPABILITY]
        } else {
            &[]
        };
        let result = connect_stream_with_startup_owner(
            LocalTransport::connect(&socket).expect("connect handshake client"),
            socket.display(),
            kind,
            None,
            None,
            client_has_terminal,
            false,
            false,
            extra,
            scope,
        );
        assert!(
            matches!(result, Err(crate::DaemonError::Server(ServerError::Internal(message))) if message == "handshake captured")
        );
        let hello = server.join().expect("join handshake server");
        assert_eq!(
            hello
                .capabilities
                .iter()
                .any(|capability| capability
                    == ClientHello::CLIENT_NATIVE_TERMINAL_SEARCH_CAPABILITY),
            expected,
        );
        assert_eq!(
            hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_NATIVE_CHOOSER_CAPABILITY),
            expected,
        );
        assert_eq!(
            hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_TERMINAL_CAPABILITY),
            kind == ClientKind::Interactive && client_has_terminal,
        );
        assert_eq!(
            hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_PATH_PICKER_CAPABILITY),
            desktop,
        );
        assert_eq!(
            hello
                .capabilities
                .iter()
                .any(|capability| capability == "client-remote-v1"),
            scope.is_remote(),
        );
    }
}

#[test]
fn only_remote_routes_mark_their_execs_remote() {
    for facts in [
        EndpointFactsScope::None,
        EndpointFactsScope::PortableTerminalSize,
        EndpointFactsScope::LocalHostWorkingDirectory,
        EndpointFactsScope::LocalControlTerminalIdentity,
        EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
    ] {
        let route = CommandRoute {
            socket: PathBuf::new(),
            display: String::new(),
            facts,
            send_origin: false,
        };
        let request = route.exec_request(Vec::new(), ExecFlags::default(), None, None);
        assert_eq!(request.flags.contains(ExecFlags::REMOTE), facts.is_remote());
    }
}

#[test]
fn an_interactive_client_refuses_a_daemon_without_pane_frames() {
    use super::{ProtocolReceiver, ProtocolSender, connect_stream_with_startup_owner};
    use crate::transport::{LocalTransport, Transport, TransportListener};
    use zz_protocol::{
        ClientId, ClientInstanceId, MuxOptions, PROTOCOL_VERSION, ProtocolError, ProtocolMessage,
        ServerHello, StatusLine,
    };

    for kind in [ClientKind::Interactive, ClientKind::Command] {
        #[cfg(unix)]
        let directory = tempfile::Builder::new()
            .prefix("zz-paneframe-")
            .tempdir_in("/tmp")
            .expect("create socket directory");
        #[cfg(unix)]
        let socket = directory.path().join("daemon.sock");
        #[cfg(windows)]
        let socket = std::path::PathBuf::from(format!(
            r"\\.\pipe\zz-paneframe-{}-{kind:?}",
            std::process::id()
        ));
        let listener = LocalTransport::bind(&socket).expect("bind handshake listener");
        let server = std::thread::spawn(move || {
            let stream = listener.accept().expect("accept client");
            let mut reader = ProtocolReceiver::new(
                super::TransportStream::try_clone(&stream).expect("clone stream"),
            );
            let mut writer = ProtocolSender::new(stream);
            assert!(matches!(
                reader.recv().expect("receive handshake"),
                ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
            ));

            writer
                .send(&ProtocolMessage::ServerHello(Box::new(ServerHello {
                    protocol_version: PROTOCOL_VERSION,
                    server_id: 1,
                    client_id: ClientId(1),
                    client_instance_id: ClientInstanceId(1),
                    capabilities: Vec::new(),
                    appearance: zz_terminal::TerminalAppearance::default(),
                    appearance_provenance: zz_terminal::AppearanceProvenance::default(),
                    mux_options: MuxOptions::default(),
                    status: StatusLine::default(),
                    key_tables: Vec::new(),
                    clock: 0,
                    platform: zz_protocol::HostPlatform::default(),
                    revision: zz_protocol::REVISION,
                })))
                .expect("send hello");
        });
        let result = connect_stream_with_startup_owner(
            LocalTransport::connect(&socket).expect("connect handshake client"),
            socket.display(),
            kind,
            None,
            None,
            kind == ClientKind::Interactive,
            false,
            false,
            &[],
            EndpointFactsScope::None,
        );
        server.join().expect("join handshake server");
        match kind {
            ClientKind::Interactive => {
                let Err(error) = result else {
                    panic!("an interactive client accepted a daemon without pane frames");
                };
                assert!(matches!(
                    error,
                    crate::DaemonError::Protocol(ProtocolError::VersionMismatch {
                        expected: PROTOCOL_VERSION,
                        received: PROTOCOL_VERSION,
                    })
                ));
                assert!(matches!(
                    crate::classify_local_connect_error(&socket, error),
                    crate::DaemonError::IncompatibleDaemon {
                        daemon: Some(PROTOCOL_VERSION),
                        client: PROTOCOL_VERSION,
                    }
                ));
            }
            _ => assert!(result.is_ok(), "a command client needs no terminal frames"),
        }
    }
}

#[test]
fn a_client_refuses_a_welcome_below_the_daemon_revision_floor() {
    use super::{ProtocolReceiver, ProtocolSender, connect_stream_with_startup_owner};
    use crate::transport::{LocalTransport, Transport, TransportListener};
    use zz_protocol::{
        Batch, ClientId, ClientInstanceId, MIN_DAEMON_REVISION, PROTOCOL_VERSION, ProtocolMessage,
        Welcome,
    };

    #[cfg(unix)]
    let directory = tempfile::Builder::new()
        .prefix("zz-revision-")
        .tempdir_in("/tmp")
        .expect("create socket directory");
    #[cfg(unix)]
    let socket = directory.path().join("daemon.sock");
    #[cfg(windows)]
    let socket = std::path::PathBuf::from(format!(r"\\.\pipe\zz-revision-{}", std::process::id()));
    let stale = MIN_DAEMON_REVISION.saturating_sub(1);
    let listener = LocalTransport::bind(&socket).expect("bind handshake listener");
    let server = std::thread::spawn(move || {
        let stream = listener.accept().expect("accept client");
        let mut reader = ProtocolReceiver::new(
            super::TransportStream::try_clone(&stream).expect("clone stream"),
        );
        let mut writer = ProtocolSender::new(stream);
        assert!(matches!(
            reader.recv().expect("receive handshake"),
            ProtocolMessage::Hello(_)
        ));
        let capabilities = [
            zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
            zz_protocol::CONTROL_CAPABILITY.to_owned(),
        ];
        writer
            .send(&ProtocolMessage::Welcome(Welcome {
                protocol_version: PROTOCOL_VERSION,
                server_id: 1,
                client_id: ClientId(1),
                client_instance_id: ClientInstanceId(1),
                caps: Welcome::caps_from_strings(&capabilities),
                clock: 0,
                platform: zz_protocol::HostPlatform::default(),
                revision: stale,
            }))
            .expect("send welcome");
        writer
            .send(&ProtocolMessage::Batch(
                Batch::from_messages(1, []).expect("empty batch"),
            ))
            .expect("send batch");
    });
    let result = connect_stream_with_startup_owner(
        LocalTransport::connect(&socket).expect("connect handshake client"),
        socket.display(),
        ClientKind::Interactive,
        None,
        None,
        true,
        false,
        false,
        &[],
        EndpointFactsScope::None,
    );
    server.join().expect("join handshake server");
    let Err(error) = result else {
        panic!("a client attached to a daemon below its revision floor");
    };
    assert!(
        matches!(
            error,
            crate::DaemonError::DaemonTooOld { daemon, minimum }
                if daemon == stale && minimum == MIN_DAEMON_REVISION
        ),
        "{error:?}"
    );
}

#[test]
fn local_control_facts_use_only_stdin_identity() {
    let mut capabilities = Vec::new();
    terminal_facts_capabilities_with(
        EndpointFactsScope::LocalControlTerminalIdentity,
        true,
        || panic!("control facts must not inspect terminal size"),
        |scope| {
            assert_eq!(scope, CallerTtyScope::StandardInput);
            Some("/dev/ttys007".to_owned())
        },
        &mut capabilities,
    );
    assert_eq!(
        capabilities,
        ["client-tty-v1:/dev/ttys007", "client-nested-v1"]
    );
}

#[test]
fn startup_config_owner_capability_is_control_only_and_opt_in() {
    assert_eq!(
        ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY,
        "startup-config-owner-v1"
    );
    for (kind, startup_config_owner, expected) in [
        (ClientKind::Interactive, false, false),
        (ClientKind::Interactive, true, false),
        (ClientKind::Command, false, false),
        (ClientKind::Command, true, false),
        (ClientKind::Control, false, false),
        (ClientKind::Control, true, true),
    ] {
        let mut capabilities = Vec::new();
        startup_config_owner_capability(kind, startup_config_owner, &mut capabilities);
        assert_eq!(
            capabilities,
            if expected {
                vec![ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY.to_owned()]
            } else {
                Vec::new()
            }
        );
    }
}

#[test]
fn nested_capability_requires_local_terminal_facts_and_a_nonempty_environment() {
    for (scope, nested, expected) in [
        (EndpointFactsScope::None, true, false),
        (EndpointFactsScope::PortableTerminalSize, true, false),
        (
            EndpointFactsScope::LocalControlTerminalIdentity,
            false,
            false,
        ),
        (EndpointFactsScope::LocalControlTerminalIdentity, true, true),
        (
            EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
            false,
            false,
        ),
        (
            EndpointFactsScope::LocalHostWorkingDirectoryAndTerminal,
            true,
            true,
        ),
    ] {
        let mut capabilities = Vec::new();
        terminal_facts_capabilities(scope, nested, &mut capabilities);
        assert_eq!(
            capabilities.iter().any(|capability| {
                capability == zz_protocol::ClientHello::CLIENT_NESTED_CAPABILITY
            }),
            expected
        );
    }
}
