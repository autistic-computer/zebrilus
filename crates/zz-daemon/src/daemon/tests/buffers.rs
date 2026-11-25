use super::*;

#[test]
fn only_the_commands_that_print_through_the_pin_are_sanitized() {
    let shared = Arc::new(Shared::new(1));
    let plain = ClientId(1);
    let utf8 = ClientId(2);
    shared.inner.lock().client_entry(utf8).utf8 = true;

    for kind in [ClientKind::Command, ClientKind::Control] {
        assert!(shared.sanitizes_output_for(plain, kind, "display-message"));
        assert!(shared.sanitizes_output_for(plain, kind, "list-buffers"));
        assert!(!shared.sanitizes_output_for(plain, kind, "capture-pane"));
        assert!(!shared.sanitizes_output_for(plain, kind, "save-buffer"));
        assert!(!shared.sanitizes_output_for(utf8, kind, "display-message"));
    }

    assert!(!shared.sanitizes_output_for(plain, ClientKind::Command, "show-buffer"));
    assert!(!shared.sanitizes_output_for(plain, ClientKind::Control, "show-buffer"));

    assert!(!shared.sanitizes_output_for(plain, ClientKind::Interactive, "display-message"));
}

#[test]
fn show_buffer_hands_binary_bytes_to_each_client_shape_the_way_the_pin_does() {
    let shared = Arc::new(Shared::new(1));
    let plain = ClientId(1);
    let utf8 = ClientId(2);
    shared.inner.lock().client_entry(utf8).utf8 = true;
    let mut context = ExecutionContext::default();
    let stored = b"a\xfeb\nc\x01d".to_vec();
    shared
        .execute(
            plain,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-buffer",
                [
                    RawText::from("-b"),
                    RawText::from("binary"),
                    RawText::from_bytes(stored.clone()),
                ],
            ),
        )
        .expect("binary buffer");
    let show = CommandInvocation::new("show-buffer", ["-b", "binary"]);
    let mut shown =
        |client, kind| match shared.execute_command_request(client, kind, &mut context, 1, &show) {
            CommandResponse::Success {
                output,
                exit_code: 0,
                stdout_claim,
                ..
            } => (output.as_bytes().to_vec(), stdout_claim),
            other => panic!("show-buffer failed: {other:?}"),
        };

    assert_eq!(
        shown(plain, ClientKind::Command),
        (stored.clone(), StdoutClaim::Raw)
    );
    assert_eq!(
        shown(utf8, ClientKind::Command),
        (stored.clone(), StdoutClaim::Raw)
    );
    assert_eq!(shown(plain, ClientKind::Control).0, b"a_b_c_d");
    assert_eq!(shown(utf8, ClientKind::Control).0, stored);

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut context,
        2,
        &CommandInvocation::new("if-shell", ["-F", "1", "show-buffer -b binary"]),
    );
    let guards = control_command_guards(take_reliable_messages(&mailbox));
    assert!(
        guards
            .iter()
            .any(|(output, error, _)| output == "a_b_c_d" && !error),
        "{guards:?}"
    );
}

#[test]
fn show_buffer_prints_for_the_control_client_that_ran_it_in_every_queue_shape() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("show.conf");
    fs::write(&source, "show-buffer -b mixed\n").expect("show-buffer source");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "show-shapes"]),
        )
        .expect("session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-buffer",
                [
                    RawText::from("-b"),
                    RawText::from("mixed"),
                    RawText::from_bytes(b"a\xfeb\0c".to_vec()),
                ],
            ),
        )
        .expect("mixed buffer");
    let session = context.session.expect("session");
    let shapes = [
        control_stdin_command("show-buffer", ["-b", "mixed"]),
        control_stdin_command("if-shell", ["-F", "1", "show-buffer -b mixed"]),
        control_stdin_command("source-file", [source.display().to_string()]),
    ];
    for utf8 in [false, true] {
        let mailbox = OutboundMailbox::new();
        let (control, _) =
            shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
        shared.inner.lock().client_entry(control).utf8 = utf8;
        shared
            .attach(control, session)
            .expect("attach control client");
        take_reliable_messages(&mailbox);
        for (request_id, shape) in (1..).zip(&shapes) {
            let mut printed = Vec::new();
            if let CommandResponse::Success { output, .. } = shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                shape,
            ) {
                printed.push(output.as_bytes().to_vec());
            }
            printed.extend(
                control_command_guards(take_reliable_messages(&mailbox))
                    .into_iter()
                    .map(|(output, _, _)| output.into_bytes()),
            );
            printed.retain(|output| !output.is_empty());
            let expected: &[&[u8]] = if utf8 {
                &[b"a\xfeb", "a\u{fffd}b".as_bytes()]
            } else {
                &[b"a_b"]
            };
            assert!(
                printed.len() == 1 && expected.contains(&printed[0].as_slice()),
                "{} utf8={utf8}: {printed:?}",
                shape.name
            );
        }
    }
}

#[test]
fn show_buffer_stops_a_control_client_at_the_first_nul_the_way_the_pin_does() {
    let shared = Arc::new(Shared::new(1));
    let plain = ClientId(1);
    let utf8 = ClientId(2);
    shared.inner.lock().client_entry(utf8).utf8 = true;
    let mut context = ExecutionContext::default();
    let stored = b"a\0b\xfec".to_vec();
    shared
        .execute(
            plain,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-buffer",
                [
                    RawText::from("-b"),
                    RawText::from("nul"),
                    RawText::from_bytes(stored.clone()),
                ],
            ),
        )
        .expect("buffer with a NUL");
    let show = CommandInvocation::new("show-buffer", ["-b", "nul"]);
    let mut shown =
        |client, kind| match shared.execute_command_request(client, kind, &mut context, 1, &show) {
            CommandResponse::Success {
                output,
                exit_code: 0,
                ..
            } => output.as_bytes().to_vec(),
            other => panic!("show-buffer failed: {other:?}"),
        };

    assert_eq!(shown(plain, ClientKind::Command), stored);
    assert_eq!(shown(plain, ClientKind::Control), b"a");
    assert_eq!(shown(utf8, ClientKind::Control), b"a");
}

#[test]
fn daemon_argument_parser_handles_compact_options_and_explicit_boundaries() {
    let args = ["-bCE", "-c/tmp", "-s::", "--", "-literal"].map(RawText::from);
    let parsed =
        parse_buffer_command_args("run-shell", &args, &['c', 'd', 's', 't'], &['b', 'C', 'E'])
            .expect("daemon arguments");
    assert!(parsed.has('b'));
    assert!(parsed.has('C'));
    assert!(parsed.has('E'));
    assert_eq!(parsed.value('c'), Some("/tmp"));
    assert_eq!(parsed.value('s'), Some("::"));
    assert_eq!(parsed.positional, ["-literal"]);

    let parsed = parse_buffer_command_args(
        "load-buffer",
        &["path", "-b", "late"].map(RawText::from),
        &['b', 't'],
        &[],
    )
    .expect_err("positional maximum");
    assert_eq!(
        parsed,
        ServerError::CommandParse(
            "command load-buffer: too many arguments (need at most 1)".to_owned()
        )
    );

    let error = parse_buffer_command_args("paste-buffer", &["-x".into()], &['b', 't'], &['d', 'p'])
        .expect_err("unsupported option");
    assert!(matches!(
        error,
        ServerError::CommandParse(command)
            if command == "unsupported command: paste-buffer -x"
    ));
}

#[test]
fn standard_buffer_streams_preserve_bytes_empty_input_and_stdin_bound() {
    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    let payload = b"a\0b\xff";
    for input in [payload.as_slice(), b""] {
        shared
            .buffer_command(
                &context,
                "load-buffer",
                &[
                    "-b".into(),
                    "named".into(),
                    "-".into(),
                    RawText::from_bytes(input.to_vec()),
                ],
            )
            .expect("bounded stdin payload");
        let saved = shared
            .buffer_command(
                &context,
                "save-buffer",
                &["-a".into(), "-b".into(), "named".into(), "-".into()],
            )
            .expect("raw stdout");
        assert_eq!(saved.output.as_bytes(), payload);
    }
    assert!(
        shared
            .buffer_command(
                &context,
                "load-buffer",
                &[
                    "-".into(),
                    RawText::from_bytes(vec![b'x'; MAX_AGENT_SEND_BYTES + 1]),
                ]
            )
            .is_err()
    );
}

#[test]
fn load_and_save_buffers_preserve_binary_bytes_and_append() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let input = directory.path().join("input.bin");
    let output = directory.path().join("output.bin");
    let bytes = b"head\0middle\n\xfftail";
    fs::write(&input, bytes).expect("binary fixture");

    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    shared
        .buffer_command(
            &context,
            "load-buffer",
            &[
                "-bbinary".into(),
                input.to_string_lossy().into_owned().into(),
            ],
        )
        .expect("load binary buffer");
    {
        let inner = shared.inner.lock();
        let buffer =
            resolve_buffer(&inner, Some("binary"), BufferMissing::NoBuffer).expect("named buffer");
        assert_eq!(buffer.data.as_ref(), bytes);
        assert!(!buffer.automatic);
    }

    assert_eq!(
        shared
            .buffer_command(
                &context,
                "show-buffer",
                &["-b", "binary"].map(RawText::from),
            )
            .expect("show binary buffer")
            .output
            .as_bytes(),
        bytes
    );

    shared
        .buffer_command(
            &context,
            "save-buffer",
            &["-b", "binary", output.to_string_lossy().as_ref()].map(RawText::from),
        )
        .expect("save binary buffer");
    assert_eq!(fs::read(&output).expect("saved bytes"), bytes);

    fs::write(&output, b"prefix:").expect("append prefix");
    shared
        .buffer_command(
            &context,
            "save-buffer",
            &["-a", "-bbinary", output.to_string_lossy().as_ref()].map(RawText::from),
        )
        .expect("append binary buffer");
    let mut expected = b"prefix:".to_vec();
    expected.extend_from_slice(bytes);
    assert_eq!(fs::read(&output).expect("appended bytes"), expected);
}

#[test]
fn save_buffer_fails_instead_of_writing_locally_when_the_command_client_is_gone() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = directory.path().join("output.txt");
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let context = ExecutionContext::default();
    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-b", "remote", "payload"].map(RawText::from),
        )
        .expect("set buffer");
    let path = output.to_string_lossy().into_owned();
    let args = ["-b", "remote", path.as_str()].map(RawText::from);
    let mailbox = OutboundMailbox::new();
    let _writer = ClientWriterRegistrationGuard::new(&shared, client, Arc::clone(&mailbox));
    mailbox.close();

    let error = shared
        .buffer_command_for_client(
            Some(client),
            ClientKind::Command,
            &context,
            "save-buffer",
            &args,
        )
        .expect_err("a closed writer cannot take the write");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("the invoking client is gone")
    ));
    assert!(!output.exists());
    assert!(shared.inner.lock().client_file_waiters.is_empty());
}

#[test]
fn buffer_file_paths_expand_once_in_the_selected_client_context() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(1));
    let (source_session, source_window, source_pane, target_session, target_window, target_pane) = {
        let mut inner = shared.inner.lock();
        let (source_session, source_window, source_pane) = inner
            .engine
            .state
            .create_session("source-session")
            .expect("source session");
        let (target_session, target_window, target_pane) = inner
            .engine
            .state
            .create_session("target-session")
            .expect("target session");
        inner
            .engine
            .state
            .rename_window(target_window, "target-window")
            .expect("target window name");
        inner
            .engine
            .state
            .update_pane_title(target_pane, "target-pane")
            .expect("target pane title");
        (
            source_session,
            source_window,
            source_pane,
            target_session,
            target_window,
            target_pane,
        )
    };
    let (source_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("source-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (target_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("target-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (command_client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared
        .attach(source_client, source_session)
        .expect("attach source client");
    shared
        .attach(target_client, target_session)
        .expect("attach target client");
    shared
        .inner
        .lock()
        .client_entry(command_client)
        .origin
        .replace(source_pane);
    let mut command_context =
        ExecutionContext::new(Some(source_session), Some(source_window), Some(source_pane));
    let mut target_context =
        ExecutionContext::new(Some(target_session), Some(target_window), Some(target_pane));

    {
        let mut inner = shared.inner.lock();
        assert_eq!(
            expand_buffer_path(
                &mut inner,
                Some(command_client),
                &command_context,
                Some("target-client"),
                "load-buffer",
                "#{session_name}=[#{session_active}]",
            ),
            "target-session=[1]"
        );
        assert_eq!(
            expand_buffer_path(
                &mut inner,
                Some(command_client),
                &command_context,
                None,
                "load-buffer",
                "#{session_name}=[#{session_active}]",
            ),
            "source-session=[1]"
        );
        assert_eq!(
            expand_buffer_path(
                &mut inner,
                Some(command_client),
                &command_context,
                Some("missing-client"),
                "load-buffer",
                "[#{session_active}]",
            ),
            "[]"
        );
    }

    for (index, value) in ["zzload=load-buffer", "zzsave=save-buffer"]
        .into_iter()
        .enumerate()
    {
        shared
            .execute(
                command_client,
                ClientKind::Command,
                &mut command_context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{}]", index + 50), value],
                ),
            )
            .expect("install buffer command alias");
    }

    let targeted_template = directory
        .path()
        .join("#{session_name}-#{window_name}-#{pane_title}-#{command}-#{path_tag}")
        .to_string_lossy()
        .into_owned();
    for (spelling, tag) in [
        ("load-buffer", "canonical"),
        ("loadb", "alias"),
        ("loa", "prefix"),
        ("zzload", "user-alias"),
    ] {
        let input = directory.path().join(format!(
            "target-session-target-window-target-pane-load-buffer-{tag}"
        ));
        fs::write(&input, tag).expect("formatted load fixture");
        command_context
            .format_variables
            .insert("path_tag".to_owned(), tag.to_owned());
        shared
            .execute(
                command_client,
                ClientKind::Command,
                &mut command_context,
                &CommandInvocation::new(
                    spelling,
                    [
                        "-b".to_owned(),
                        tag.to_owned(),
                        "-t".to_owned(),
                        "target-client".to_owned(),
                        targeted_template.clone(),
                    ],
                ),
            )
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
    }
    {
        let inner = shared.inner.lock();
        for tag in ["canonical", "alias", "prefix", "user-alias"] {
            assert_eq!(
                resolve_buffer(&inner, Some(tag), BufferMissing::NoBuffer)
                    .expect("loaded formatted buffer")
                    .data
                    .as_ref(),
                tag.as_bytes()
            );
        }
    }

    let origin_input = directory.path().join("source-session-load-buffer-origin");
    fs::write(&origin_input, b"origin client").expect("origin client fixture");
    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "load-buffer",
                [
                    "-b".to_owned(),
                    "origin".to_owned(),
                    directory
                        .path()
                        .join("#{session_name}-#{command}-origin")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("command origin client target");

    let non_time_input = directory.path().join("target-session-%Y");
    fs::write(&non_time_input, b"non-time format").expect("non-time fixture");
    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "load-buffer",
                [
                    "-b".to_owned(),
                    "non-time".to_owned(),
                    "-t".to_owned(),
                    "target-client".to_owned(),
                    directory
                        .path()
                        .join("#{session_name}-%Y")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("buffer path does not apply strftime");

    command_context
        .format_variables
        .insert("command".to_owned(), "explicit-#{session_name}".to_owned());
    let one_pass = directory.path().join("explicit-#{session_name}");
    fs::write(&one_pass, b"one pass").expect("one-pass fixture");
    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "loadb",
                [
                    "-b".to_owned(),
                    "one-pass".to_owned(),
                    "-t".to_owned(),
                    "target-client".to_owned(),
                    directory
                        .path()
                        .join("#{command}")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("explicit command state load");
    command_context.format_variables.remove("command");

    let missing_target_input = directory.path().join("target-session-missing-target");
    fs::write(&missing_target_input, b"quiet target miss").expect("quiet miss fixture");
    let mut source_context =
        ExecutionContext::new(Some(source_session), Some(source_window), Some(source_pane));
    shared
        .execute(
            source_client,
            ClientKind::Interactive,
            &mut source_context,
            &CommandInvocation::new(
                "load-buffer",
                [
                    "-b".to_owned(),
                    "quiet-target".to_owned(),
                    "-t".to_owned(),
                    "missing-client".to_owned(),
                    directory
                        .path()
                        .join("#{session_name}-missing-target")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("missing target falls back quietly");

    let empty = directory.path().join("target-session-empty");
    fs::write(&empty, []).expect("formatted empty fixture");
    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "load-buffer",
                [
                    "-b".to_owned(),
                    String::new(),
                    "-t".to_owned(),
                    "target-client".to_owned(),
                    directory
                        .path()
                        .join("#{session_name}-empty")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("formatted empty load");
    assert!(resolve_buffer(&shared.inner.lock(), Some(""), BufferMissing::NoBuffer).is_err());

    let missing_path = directory.path().join("missing-target-session");
    let error = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "load-buffer",
                [
                    "-t".to_owned(),
                    "target-client".to_owned(),
                    directory
                        .path()
                        .join("missing-#{session_name}")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect_err("expanded missing path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains(missing_path.to_string_lossy().as_ref())
                && !message.contains("#{session_name}")
    ));

    command_context
        .format_variables
        .insert("path".to_owned(), String::new());
    let error = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("load-buffer", ["#{path}"]),
        )
        .expect_err("empty expanded load path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message
                .split_once(": ")
                .is_some_and(|(_, path)| path.is_empty() || Path::new(path).is_absolute())
    ));
    command_context
        .format_variables
        .insert("path".to_owned(), "-".to_owned());
    let error = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("load-buffer", ["#{path}"]),
        )
        .expect_err("formatted stdin path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "load-buffer from standard input requires a command client stdin payload"
    ));
    command_context.format_variables.remove("path");

    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("set-buffer", ["-b", "saved", "saved payload"]),
        )
        .expect("seed saved buffer");
    command_context
        .format_variables
        .insert("path".to_owned(), String::new());
    let error = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("save-buffer", ["-b", "saved", "#{path}"]),
        )
        .expect_err("empty expanded save path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message
                .split_once(": ")
                .is_some_and(|(_, path)| path.is_empty() || Path::new(path).is_absolute())
    ));
    command_context.format_variables.remove("path");
    let origin_output = directory.path().join("source-session-save-buffer-origin");
    shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "save-buffer",
                [
                    "-b".to_owned(),
                    "saved".to_owned(),
                    directory
                        .path()
                        .join("#{session_name}-#{command}-origin")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("save through command origin client");
    assert_eq!(
        fs::read(origin_output).expect("origin client save"),
        b"saved payload"
    );
    for (spelling, tag) in [
        ("save-buffer", "canonical"),
        ("saveb", "alias"),
        ("sa", "prefix"),
        ("zzsave", "user-alias"),
    ] {
        target_context
            .format_variables
            .insert("path_tag".to_owned(), tag.to_owned());
        let output = directory.path().join(format!(
            "target-session-target-window-target-pane-save-buffer-{tag}"
        ));
        shared
            .execute(
                target_client,
                ClientKind::Interactive,
                &mut target_context,
                &CommandInvocation::new(
                    spelling,
                    [
                        "-b".to_owned(),
                        "saved".to_owned(),
                        targeted_template.clone(),
                    ],
                ),
            )
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(fs::read(output).expect("formatted save"), b"saved payload");
    }

    target_context
        .format_variables
        .insert("command".to_owned(), "explicit-#{session_name}".to_owned());
    let explicit_save = directory.path().join("save-explicit-#{session_name}");
    shared
        .execute(
            target_client,
            ClientKind::Interactive,
            &mut target_context,
            &CommandInvocation::new(
                "saveb",
                [
                    "-b".to_owned(),
                    "saved".to_owned(),
                    directory
                        .path()
                        .join("save-#{command}")
                        .to_string_lossy()
                        .into_owned(),
                ],
            ),
        )
        .expect("explicit command state save");
    assert_eq!(
        fs::read(explicit_save).expect("explicit formatted save"),
        b"saved payload"
    );
    target_context.format_variables.remove("command");
    target_context
        .format_variables
        .insert("path".to_owned(), "-".to_owned());
    let saved = shared
        .execute(
            target_client,
            ClientKind::Interactive,
            &mut target_context,
            &CommandInvocation::new("save-buffer", ["-b", "saved", "#{path}"]),
        )
        .expect("formatted stdout path");
    assert_eq!(saved.output.as_bytes(), b"saved payload");

    let empty_shared = Arc::new(Shared::new(1));
    assert!(matches!(
        empty_shared.buffer_command(
            &ExecutionContext::default(),
            "save-buffer",
            &["-".into()],
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no buffers"
    ));
}

#[test]
fn set_buffer_append_updates_named_buffers_but_creates_automatic_buffers_without_a_name() {
    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    for arguments in [
        vec!["-b", "named", "alpha"],
        vec!["automatic"],
        vec!["-abnamed", "--", "-omega"],
        vec!["-a", "new automatic"],
    ] {
        let arguments = arguments.into_iter().map(RawText::from).collect::<Vec<_>>();
        shared
            .buffer_command(&context, "set-buffer", &arguments)
            .expect("set buffer");
    }

    let inner = shared.inner.lock();
    assert_eq!(inner.paste_buffers.len(), 3);
    assert_eq!(inner.paste_buffers[0].data.as_ref(), b"new automatic");
    assert!(inner.paste_buffers[0].automatic);
    let named =
        resolve_buffer(&inner, Some("named"), BufferMissing::NoBuffer).expect("named buffer");
    assert_eq!(named.data.as_ref(), b"alpha-omega");
    assert!(!named.automatic);
}

#[test]
fn background_insertions_drain_in_job_start_order_not_arrival_order() {
    let shared = Arc::new(Shared::new(1));
    let applied = Arc::new(Mutex::new(Vec::new()));

    let tickets = (0..6)
        .map(|_| shared.background_insertion_ticket())
        .collect::<Vec<_>>();
    assert_eq!(tickets, vec![0, 1, 2, 3, 4, 5]);

    let recorder = |applied: &Arc<Mutex<Vec<u64>>>, label: u64| {
        let applied = Arc::clone(applied);
        move || applied.lock().push(label)
    };

    let nested_shared = Arc::clone(&shared);
    let nested_applied = Arc::clone(&applied);
    let nested_tickets = tickets.clone();
    shared.apply_background_insertion(tickets[5], move || {
        nested_applied.lock().push(5);
        for ticket in [4_usize, 1, 3, 2] {
            nested_shared.apply_background_insertion(
                nested_tickets[ticket],
                recorder(&nested_applied, ticket as u64),
            );
        }
    });
    assert_eq!(*applied.lock(), vec![5, 1, 2, 3, 4]);

    applied.lock().clear();
    shared.apply_background_insertion(tickets[0], recorder(&applied, 0));
    assert_eq!(*applied.lock(), vec![0]);
}

#[test]
fn a_missing_buffer_is_worded_per_verb_the_way_the_pin_words_it() {
    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();

    let message = |command: &str, arguments: &[&str]| {
        let arguments = arguments
            .iter()
            .copied()
            .map(RawText::from)
            .collect::<Vec<_>>();
        let error = shared
            .buffer_command(&context, command, &arguments)
            .expect_err("missing buffer");
        match error {
            DaemonError::Server(error) => error.tmux_message(),
            other => panic!("unexpected error: {other:?}"),
        }
    };

    for (command, arguments) in [
        ("show-buffer", vec!["-b", "nosuch"]),
        ("save-buffer", vec!["-b", "nosuch", "/dev/null"]),
    ] {
        assert_eq!(message(command, &arguments), "no buffer nosuch");
    }
    for (command, arguments) in [("show-buffer", vec![]), ("save-buffer", vec!["/dev/null"])] {
        assert_eq!(message(command, &arguments), "no buffers");
    }
    assert_eq!(
        message("delete-buffer", &["-b", "nosuch"]),
        "unknown buffer: nosuch"
    );
    assert_eq!(
        message("set-buffer", &["-n", "newname", "-b", "nosuch"]),
        "unknown buffer: nosuch"
    );
    assert_eq!(message("delete-buffer", &[]), "no buffer");
    assert_eq!(message("set-buffer", &["-n", "newname"]), "no buffer");

    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-b", "present", "payload"].map(RawText::from),
        )
        .expect("set buffer");
    assert_eq!(
        message("show-buffer", &["-b", "nosuch"]),
        "no buffer nosuch"
    );
    assert_eq!(
        message("delete-buffer", &["-b", "nosuch"]),
        "unknown buffer: nosuch"
    );
    assert_eq!(message("show-buffer", &[]), "no buffers");
    assert_eq!(message("delete-buffer", &[]), "no buffer");
}

#[test]
fn set_buffer_rename_replaces_destinations_ignores_data_and_uses_the_top_automatic() {
    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    for arguments in [
        vec!["-b", "source", "alpha"],
        vec!["-b", "destination", "beta"],
        vec!["top automatic"],
    ] {
        shared
            .buffer_command(
                &context,
                "set-buffer",
                &arguments.into_iter().map(RawText::from).collect::<Vec<_>>(),
            )
            .expect("seed buffer");
    }

    shared
        .buffer_command(
            &context,
            "set-buffer",
            &[
                "-b",
                "source",
                "-n",
                "destination",
                "ignored replacement data",
            ]
            .map(RawText::from),
        )
        .expect("rename over destination");
    {
        let inner = shared.inner.lock();
        assert!(resolve_buffer(&inner, Some("source"), BufferMissing::NoBuffer).is_err());
        let renamed = resolve_buffer(&inner, Some("destination"), BufferMissing::NoBuffer)
            .expect("renamed buffer");
        assert_eq!(renamed.data.as_ref(), b"alpha");
        assert_eq!(inner.paste_buffers.len(), 2);
    }

    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-an", "named-top", "ignored"].map(RawText::from),
        )
        .expect("rename top automatic");
    {
        let inner = shared.inner.lock();
        let renamed = resolve_buffer(&inner, Some("named-top"), BufferMissing::NoBuffer)
            .expect("named top buffer");
        assert_eq!(renamed.data.as_ref(), b"top automatic");
        assert!(!renamed.automatic);
        assert!(!inner.paste_buffers.iter().any(|buffer| buffer.automatic));
    }

    assert!(matches!(
        shared.buffer_command(
            &context,
            "set-buffer",
            &["-n", "nowhere", "ignored"].map(RawText::from),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no buffer"
    ));
    assert!(matches!(
        shared.buffer_command(
            &context,
            "set-buffer",
            &["-b", "missing", "-n", "", "ignored"].map(RawText::from),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "unknown buffer: missing"
    ));
}

#[test]
fn buffer_formats_read_named_rows_and_the_top_automatic_buffer() {
    let shared = Arc::new(Shared::new(1));
    let mut context = {
        let mut inner = shared.inner.lock();
        let (_, _, pane) = inner
            .engine
            .state
            .create_session("w")
            .expect("format target session");
        ExecutionContext::for_pane(&inner.engine.state, pane).expect("format target pane")
    };
    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-b", "named", "alpha"].map(RawText::from),
        )
        .expect("set named buffer");
    let named = shared
        .buffer_command(
            &context,
            "list-buffers",
            &[
                "-F".into(),
                "#{command}|#{buffer_name}|#{buffer_size}|#{buffer_sample}|#{!!:#{buffer_created}}"
                    .into(),
            ],
        )
        .expect("format named buffer");
    assert_eq!(named.output, "list-buffers|named|5|alpha|1");

    shared
        .buffer_command(&context, "set-buffer", &["bravo".into()])
        .expect("set automatic buffer");
    let displayed = shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "#{buffer_name}|#{buffer_size}|#{buffer_sample}|#{!!:#{buffer_created}}",
                ],
            ),
        )
        .expect("format automatic buffer");
    assert_eq!(displayed.output, "buffer0|5|bravo|1");

    for (arguments, expected) in [
        (vec!["-F", "#{buffer_name}"], "buffer0\nnamed"),
        (vec!["-r", "-F", "#{buffer_name}"], "buffer0\nnamed"),
        (
            vec!["-O", "name", "-r", "-F", "#{buffer_name}"],
            "named\nbuffer0",
        ),
        (
            vec![
                "-f",
                "#{&&:#{==:#{command},list-buffers},#{==:#{buffer_name},named}}",
                "-F",
                "#{command}:#{buffer_name}",
            ],
            "list-buffers:named",
        ),
    ] {
        let listed = shared
            .buffer_command(
                &context,
                "list-buffers",
                &arguments.into_iter().map(RawText::from).collect::<Vec<_>>(),
            )
            .expect("sort and filter buffers");
        assert_eq!(listed.output, expected);
    }

    let pane = context.pane.expect("format target pane").to_string();
    let targeted = shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-t",
                    pane.as_str(),
                    "#{buffer_name}|#{buffer_size}|#{buffer_sample}|#{!!:#{buffer_created}}",
                ],
            ),
        )
        .expect("format automatic buffer with a pane target");
    assert_eq!(targeted.output, "buffer0|5|bravo|1");
}

#[test]
fn unnamed_buffer_commands_target_the_newest_automatic_buffer() {
    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    for arguments in [
        vec!["-b", "named-old", "named old"],
        vec!["automatic"],
        vec!["-b", "named-new", "named new"],
    ] {
        shared
            .buffer_command(
                &context,
                "set-buffer",
                &arguments.into_iter().map(RawText::from).collect::<Vec<_>>(),
            )
            .expect("set buffer");
    }

    let shown = shared
        .buffer_command(&context, "show-buffer", &[])
        .expect("show automatic buffer");
    assert_eq!(shown.output, "automatic");
    shared
        .buffer_command(&context, "delete-buffer", &[])
        .expect("delete automatic buffer");
    let inner = shared.inner.lock();
    assert_eq!(inner.paste_buffers.len(), 2);
    assert!(inner.paste_buffers.iter().all(|buffer| !buffer.automatic));
    assert_eq!(inner.paste_buffers[0].name, "named-new");
    drop(inner);
    assert!(matches!(
        shared.buffer_command(&context, "show-buffer", &[]),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no buffers"
    ));
}

#[cfg(unix)]
#[test]
fn paste_buffer_reaches_a_detached_terminal_actor() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "detached"]),
        )
        .expect("detached session");
    let pane = context.pane.expect("terminal pane");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-buffer", ["printf 'ZZ_DETACHED_BUFFER_OK\\n'\n"]),
        )
        .expect("shell buffer");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("paste-buffer", ["-t", &pane.to_string()]),
        )
        .expect("paste without an interactive attachment");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let captured = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
            )
            .expect("capture detached terminal")
            .output;
        if captured.contains("ZZ_DETACHED_BUFFER_OK") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "detached terminal did not receive paste-buffer"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let buffered = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "capture-pane",
                ["-b", "detached-capture", "-t", &pane.to_string()],
            ),
        )
        .expect("capture detached terminal into a named buffer");
    assert!(buffered.output.is_empty());
    let shown = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-buffer", ["-b", "detached-capture"]),
        )
        .expect("show captured named buffer")
        .output;
    assert!(shown.contains("ZZ_DETACHED_BUFFER_OK"));
}

#[test]
fn buffer_file_commands_bound_input_and_reject_unsupported_stream_modes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let empty = directory.path().join("empty");
    let oversized = directory.path().join("oversized");
    let targeted = directory.path().join("targeted");
    fs::write(&empty, []).expect("empty fixture");
    fs::write(&targeted, b"loaded through inert target").expect("targeted fixture");
    fs::File::create(&oversized)
        .expect("oversized fixture")
        .set_len(u64::try_from(MAX_PASTE_BUFFER_BYTES).unwrap() + 1)
        .expect("sparse oversized fixture");

    let shared = Arc::new(Shared::new(1));
    let context = ExecutionContext::default();
    shared
        .buffer_command(
            &context,
            "load-buffer",
            &[empty.to_string_lossy().into_owned().into()],
        )
        .expect("empty load is a no-op");
    assert!(shared.inner.lock().paste_buffers.is_empty());

    let error = shared
        .buffer_command(
            &context,
            "load-buffer",
            &[oversized.to_string_lossy().into_owned().into()],
        )
        .expect_err("oversized buffer");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("exceeds")
    ));
    assert!(shared.inner.lock().paste_buffers.is_empty());

    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-t", "missing", "set through inert target"].map(RawText::from),
        )
        .expect("set-buffer target is inert without clipboard output");
    shared
        .buffer_command(
            &context,
            "load-buffer",
            &[
                "-b".into(),
                "targeted".into(),
                "-t".into(),
                "missing".into(),
                targeted.to_string_lossy().into_owned().into(),
            ],
        )
        .expect("load-buffer target is inert without clipboard output");
    let inner = shared.inner.lock();
    assert_eq!(inner.paste_buffers.len(), 2);
    assert_eq!(
        inner.paste_buffers[0].data.as_ref(),
        b"loaded through inert target"
    );
    assert_eq!(
        inner.paste_buffers[1].data.as_ref(),
        b"set through inert target"
    );
    drop(inner);

    let error = shared
        .buffer_command(&context, "paste-buffer", &["-x".into()])
        .expect_err("unsupported buffer mode");
    let DaemonError::Server(error) = error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(error.tmux_message(), "unsupported command: paste-buffer -x");
    assert!(error.is_command_parse());
}

#[test]
fn named_buffers_replace_in_place_and_automatic_eviction_preserves_them() {
    let mut inner = ServerState::default();
    insert_paste_buffer(&mut inner, Some("named"), "buffer", b"alpha".to_vec(), true)
        .expect("named buffer");
    insert_paste_buffer(&mut inner, None, "buffer", b"automatic".to_vec(), true)
        .expect("automatic buffer");
    insert_paste_buffer(
        &mut inner,
        Some("named"),
        "buffer",
        b"replacement".to_vec(),
        true,
    )
    .expect("replace named buffer");
    assert_eq!(inner.paste_buffers[0].name, "named");
    assert_eq!(inner.paste_buffers[0].data.as_ref(), b"replacement");
    assert!(!inner.paste_buffers[0].automatic);
    assert_eq!(
        inner
            .paste_buffers
            .iter()
            .filter(|buffer| buffer.name == "named")
            .count(),
        1
    );

    for index in 0..=DEFAULT_BUFFER_LIMIT {
        insert_paste_buffer(
            &mut inner,
            None,
            "buffer",
            vec![u8::try_from(index).unwrap_or(u8::MAX)],
            true,
        )
        .expect("bounded automatic buffer");
    }
    assert_eq!(
        inner
            .paste_buffers
            .iter()
            .filter(|buffer| buffer.automatic)
            .count(),
        DEFAULT_BUFFER_LIMIT
    );
    assert!(inner.paste_buffers.iter().any(|buffer| {
        buffer.name == "named" && buffer.data.as_ref() == b"replacement" && !buffer.automatic
    }));
}

#[test]
fn configured_buffer_limit_controls_daemon_automatic_eviction() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "buffer-limit", "2"]),
        )
        .expect("configure buffer limit");
    assert_eq!(shared.inner.lock().automatic_paste_buffer_limit.0, 2);

    for data in ["zero", "one", "two"] {
        shared
            .buffer_command(&context, "set-buffer", &[data.to_owned().into()])
            .expect("automatic buffer");
    }
    let inner = shared.inner.lock();
    assert_eq!(
        inner
            .paste_buffers
            .iter()
            .filter(|buffer| buffer.automatic)
            .map(|buffer| buffer.name.as_str())
            .collect::<Vec<_>>(),
        ["buffer2", "buffer1"]
    );
}

#[test]
fn native_copy_buffers_create_named_entries_and_append_to_the_top() {
    let shared = Arc::new(Shared::new(1));
    shared.store_copy_buffer(
        "alpha".to_owned(),
        PasteBufferAction::Create {
            prefix: Some("selection".to_owned()),
        },
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.paste_buffers.len(), 1);
        assert_eq!(inner.paste_buffers[0].name, "selection0");
        assert_eq!(inner.paste_buffers[0].data.as_ref(), b"alpha");
        assert!(inner.paste_buffers[0].automatic);
    }

    shared.store_copy_buffer("-beta".to_owned(), PasteBufferAction::Append);
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.paste_buffers.len(), 1);
        assert_eq!(inner.paste_buffers[0].data.as_ref(), b"alpha-beta");
        assert!(!inner.paste_buffers[0].automatic);
    }

    shared.store_copy_buffer(
        "gamma".to_owned(),
        PasteBufferAction::Create { prefix: None },
    );
    let inner = shared.inner.lock();
    assert_eq!(inner.paste_buffers.len(), 2);
    assert_eq!(inner.paste_buffers[0].name, "buffer1");
    assert_eq!(inner.paste_buffers[0].data.as_ref(), b"gamma");
    assert!(inner.paste_buffers[0].automatic);
}

#[test]
fn app_clipboard_writes_reach_every_viewer_only_under_set_clipboard_on() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "clip"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(client, session).expect("attach session");

    let observer_mailbox = OutboundMailbox::new();
    let (observer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&observer_mailbox),
    );
    shared.attach(observer, session).expect("attach observer");
    take_reliable_messages(&mailbox);
    take_reliable_messages(&observer_mailbox);

    shared.deliver_clipboard_write(pane, ClipboardTarget::Clipboard, "external".to_owned());
    assert!(
        take_clipboard_writes(&mailbox, pane).is_empty(),
        "input_osc_52_parse refuses unless set-clipboard is on"
    );
    assert!(take_clipboard_writes(&observer_mailbox, pane).is_empty());
    assert!(shared.inner.lock().paste_buffers.is_empty());

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "set-clipboard", "on"]),
        )
        .expect("set-clipboard on");
    take_reliable_messages(&mailbox);
    take_reliable_messages(&observer_mailbox);
    shared.deliver_clipboard_write(pane, ClipboardTarget::Primary, "buffered".to_owned());
    assert_eq!(
        take_clipboard_writes(&mailbox, pane),
        vec![(ClipboardTarget::Primary, "buffered".to_owned())]
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.paste_buffers.len(), 1);
        assert_eq!(inner.paste_buffers[0].data.as_ref(), b"buffered");
        assert!(inner.paste_buffers[0].automatic);
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "set-clipboard", "off"]),
        )
        .expect("set-clipboard off");
    take_reliable_messages(&mailbox);
    take_reliable_messages(&observer_mailbox);
    shared.deliver_clipboard_write(pane, ClipboardTarget::Clipboard, "dropped".to_owned());
    assert!(take_clipboard_writes(&mailbox, pane).is_empty());
    assert!(take_clipboard_writes(&observer_mailbox, pane).is_empty());
    assert_eq!(shared.inner.lock().paste_buffers.len(), 1);
}

#[test]
fn buffer_writes_reach_only_the_target_client_and_ignore_set_clipboard() {
    let shared = Arc::new(Shared::new(1));
    let alpha_mailbox = OutboundMailbox::new();
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        None,
        Arc::clone(&alpha_mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            alpha,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "clipwrite"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(alpha, session).expect("attach alpha");

    let bravo_mailbox = OutboundMailbox::new();
    let (bravo, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("bravo".to_owned()),
        None,
        Arc::clone(&bravo_mailbox),
    );
    shared.attach(bravo, session).expect("attach bravo");

    let charlie_mailbox = OutboundMailbox::new();
    let (charlie, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("charlie".to_owned()),
        None,
        Arc::clone(&charlie_mailbox),
    );
    shared.attach(charlie, session).expect("attach charlie");

    let run = |arguments: Vec<&str>| {
        shared
            .execute(
                alpha,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(arguments[0], arguments[1..].to_vec()),
            )
            .unwrap_or_else(|error| panic!("{arguments:?}: {error:?}"))
    };

    run(vec!["set-option", "-g", "set-clipboard", "off"]);
    run(vec![
        "set-hook",
        "-g",
        "pane-set-clipboard",
        "set-environment -g CLIPBOARD_HOOK fired",
    ]);
    take_reliable_messages(&alpha_mailbox);
    take_reliable_messages(&bravo_mailbox);
    take_reliable_messages(&charlie_mailbox);

    run(vec!["set-buffer", "-w", "-t", "bravo", "payload-one"]);
    assert!(take_clipboard_writes(&alpha_mailbox, pane).is_empty());
    assert_eq!(
        take_clipboard_writes(&bravo_mailbox, pane),
        vec![(ClipboardTarget::Clipboard, "payload-one".to_owned())]
    );
    assert!(take_clipboard_writes(&charlie_mailbox, pane).is_empty());

    run(vec!["set-buffer", "-t", "bravo", "payload-two"]);
    assert!(take_clipboard_writes(&alpha_mailbox, pane).is_empty());
    assert!(take_clipboard_writes(&bravo_mailbox, pane).is_empty());

    run(vec![
        "set-buffer",
        "-w",
        "-a",
        "-b",
        "joined",
        "-t",
        "alpha",
        "head-",
    ]);
    take_reliable_messages(&alpha_mailbox);
    run(vec![
        "set-buffer",
        "-w",
        "-a",
        "-b",
        "joined",
        "-t",
        "alpha",
        "tail",
    ]);
    assert_eq!(
        take_clipboard_writes(&alpha_mailbox, pane),
        vec![(ClipboardTarget::Clipboard, "head-tail".to_owned())]
    );

    let directory = tempfile::tempdir().expect("temporary directory");
    let file = directory.path().join("payload");
    std::fs::write(&file, b"payload-three").expect("write payload");
    run(vec![
        "load-buffer",
        "-w",
        "-t",
        "bravo",
        &file.to_string_lossy(),
    ]);
    assert!(take_clipboard_writes(&alpha_mailbox, pane).is_empty());
    assert_eq!(
        take_clipboard_writes(&bravo_mailbox, pane),
        vec![(ClipboardTarget::Clipboard, "payload-three".to_owned())]
    );

    run(vec!["set-buffer", "-w", "-t", "charlie", "payload-four"]);
    assert!(take_clipboard_writes(&charlie_mailbox, pane).is_empty());

    run(vec!["set-buffer", "-w", "-t", "missing", "payload-five"]);
    assert!(take_clipboard_writes(&alpha_mailbox, pane).is_empty());
    assert!(take_clipboard_writes(&bravo_mailbox, pane).is_empty());

    run(vec!["set-option", "-g", "set-clipboard", "on"]);
    shared.deliver_clipboard_write(pane, ClipboardTarget::Clipboard, "app".to_owned());
    assert!(
        run(vec!["show-environment", "-g", "CLIPBOARD_HOOK"])
            .output
            .contains("fired")
    );
    run(vec!["set-environment", "-gu", "CLIPBOARD_HOOK"]);
    take_reliable_messages(&alpha_mailbox);
    take_reliable_messages(&bravo_mailbox);

    run(vec!["set-buffer", "-w", "-t", "alpha", "payload-six"]);
    assert_eq!(
        take_clipboard_writes(&alpha_mailbox, pane),
        vec![(ClipboardTarget::Clipboard, "payload-six".to_owned())]
    );
    shared
        .execute(
            alpha,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-environment", ["-g", "CLIPBOARD_HOOK"]),
        )
        .expect_err("pane-set-clipboard never fires for a buffer write");

    let inner = shared.inner.lock();
    let stored = inner
        .paste_buffers
        .iter()
        .map(|buffer| String::from_utf8_lossy(&buffer.data).into_owned())
        .collect::<BTreeSet<_>>();
    assert!(stored.contains("payload-one"));
    assert!(stored.contains("payload-two"));
    assert!(stored.contains("payload-three"));
    assert!(stored.contains("payload-four"));
    assert!(stored.contains("payload-five"));
    assert!(stored.contains("payload-six"));
    assert!(stored.contains("head-tail"));
}
