use super::*;

#[test]
fn caller_stdin_request_waits_for_the_alias_reader() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, Arc::clone(&mailbox));
    let _writer = ClientWriterRegistrationGuard::new(&shared, client, Arc::clone(&mailbox));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[80]",
                    "deferred=set -g @before yes ; source-file - ; set -g @after yes",
                ],
            ),
        )
        .expect("alias");
    take_reliable_messages(&mailbox);
    let mut command = CommandInvocation::new("deferred", [] as [&str; 0]);
    command.set_stdin_available(true);
    let worker = Arc::clone(&shared);
    let pending = thread::spawn(move || {
        worker.execute_command_request(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            1,
            &command,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let request = loop {
        if let Some(request) = take_reliable_messages(&mailbox)
            .into_iter()
            .find_map(|message| {
                if let ProtocolMessage::ClientFileRequest(request) = message {
                    Some(request)
                } else {
                    None
                }
            })
        {
            break request;
        }
        assert!(Instant::now() < deadline, "reader never requested stdin");
        thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        request.operation,
        ClientFileOperation::ReadStdin { binary: true }
    );
    assert_eq!(read_global_option(&shared, "@before"), "yes");
    assert_eq!(read_global_option(&shared, "@after"), "");
    shared.complete_client_file(
        client,
        ClientFileResponse {
            request_id: request.request_id,
            data: b"set -g @payload yes\n".to_vec(),
            error: None,
        },
    );
    assert!(matches!(
        pending.join().expect("command worker"),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(read_global_option(&shared, "@payload"), "yes");
    assert_eq!(read_global_option(&shared, "@after"), "yes");
}

#[test]
fn unreadable_caller_stdin_reports_eio_once_and_continues() {
    for (reader, spent_reader) in [
        ("source-file -", "source-file -"),
        ("load-buffer -b eio -", "load-buffer -b eio2 -"),
    ] {
        let shared = Arc::new(Shared::new(1));
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Command, None, None, Arc::clone(&mailbox));
        let _writer = ClientWriterRegistrationGuard::new(&shared, client, Arc::clone(&mailbox));
        let alias = format!("unreadable={reader} ; {spent_reader} ; set -g @after yes");
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", ["-s", "command-alias[80]", &alias]),
            )
            .expect("alias");
        take_reliable_messages(&mailbox);
        let mut command = CommandInvocation::new("unreadable", [] as [&str; 0]);
        command.set_stdin_available(true);
        let worker = Arc::clone(&shared);
        let pending = thread::spawn(move || {
            worker.execute_command_request(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                1,
                &command,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let request = loop {
            if let Some(request) =
                take_reliable_messages(&mailbox)
                    .into_iter()
                    .find_map(|message| match message {
                        ProtocolMessage::ClientFileRequest(request) => Some(request),
                        _ => None,
                    })
            {
                break request;
            }
            assert!(Instant::now() < deadline, "reader never requested stdin");
            thread::sleep(Duration::from_millis(5));
        };
        shared.complete_client_file(
            client,
            ClientFileResponse {
                request_id: request.request_id,
                data: Vec::new(),
                error: Some("Input/output error".to_owned()),
            },
        );
        let response = pending.join().expect("command worker");
        let CommandResponse::Success {
            exit_code, stderr, ..
        } = &response
        else {
            panic!("{reader}: {response:?}");
        };
        assert_eq!(*exit_code, 1, "{reader}");
        assert_eq!(
            stderr, "Input/output error: -\nBad file descriptor: -\n",
            "{reader}"
        );
        assert_eq!(read_global_option(&shared, "@after"), "yes", "{reader}");
    }
}

#[test]
fn control_source_stdin_read_failure_keeps_alias_continuation() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[80]",
                    "controlstream=source-file - ; display-message -p after ; set -g @after yes",
                ],
            ),
        )
        .expect("alias");
    take_reliable_messages(&mailbox);
    let response = shared.execute_command_request(
        client,
        ClientKind::Control,
        &mut ExecutionContext::default(),
        1,
        &control_stdin_command("controlstream", [] as [&str; 0]),
    );
    assert!(
        matches!(response, CommandResponse::Success { .. }),
        "{response:?}"
    );
    assert_eq!(read_global_option(&shared, "@after"), "yes");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(message,
        ProtocolMessage::Event(Event { payload: EventPayload::ControlSourceFile { event: ControlSourceFileEvent::ReadError(output) }, .. })
            if output == "Bad file descriptor: -")), "{messages:?}");
    let guards = control_command_guards(messages);
    assert!(
        guards.iter().any(|(text, _, _)| text == "after"),
        "{guards:?}"
    );
}

#[test]
fn one_resolver_answers_every_command_stream_sink() {
    let args = |args: &[&str]| args.iter().copied().map(RawText::from).collect::<Vec<_>>();
    for (name, arguments, expected) in [
        (
            "load-buffer",
            &["-"][..],
            Some(CommandStdinSink::Argument { binary: true }),
        ),
        (
            "send-text",
            &["-t", "%1"][..],
            Some(CommandStdinSink::Argument { binary: false }),
        ),
        (
            "agent-send",
            &["--submit"][..],
            Some(CommandStdinSink::Argument { binary: false }),
        ),
        ("source-file", &["-"][..], Some(CommandStdinSink::Config)),
        (
            "source-file",
            &["-q", "one", "-"][..],
            Some(CommandStdinSink::Config),
        ),
        (
            "source-file",
            &["one"][..],
            Some(CommandStdinSink::ConfigReplay),
        ),
        (
            "display-message",
            &["-I"][..],
            Some(CommandStdinSink::PaneInput),
        ),
        (
            "display-message",
            &["-I", "-t", "%1"][..],
            Some(CommandStdinSink::PaneInput),
        ),
        ("display-message", &["-p", "#{pane_id}"][..], None),
        ("display-message", &["-pIj", "{}"][..], None),
        ("display-message", &["-I", "-j", "{}"][..], None),
        (
            "split-window",
            &["-I", "-t", "%1"][..],
            Some(CommandStdinSink::PaneInput),
        ),
        ("split-window", &["-h"][..], None),
        (
            "new-pane",
            &["-I", "-t", "%1"][..],
            Some(CommandStdinSink::PaneInput),
        ),
        ("new-pane", &["-d"][..], None),
        ("list-sessions", &[][..], None),
    ] {
        assert_eq!(
            command_stdin_sink(name, &args(arguments)),
            expected,
            "{name} {arguments:?}"
        );
    }
    assert!(CommandStdinSink::Argument { binary: true }.accepts_binary());
    assert!(!CommandStdinSink::Argument { binary: false }.accepts_binary());
    assert!(CommandStdinSink::Config.accepts_binary());
    assert!(CommandStdinSink::PaneInput.accepts_binary());
}

#[test]
fn source_file_stdin_applies_bytes_that_are_not_utf8() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(1);
    let mut context = ExecutionContext::default();
    let mut source = CommandInvocation::new("source-file", ["-"]);
    source.set_stdin(RawText::from_bytes(
        b"set -g @binary a\xfeb\nset-buffer -b sourced c\xfed\nBINARY_ENV=e\xfef\n".to_vec(),
    ));
    assert_eq!(
        shared.execute_command_request(client, ClientKind::Command, &mut context, 1, &source),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let mut output = |name: &str, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .expect("readback")
            .output
            .as_bytes()
            .to_vec()
    };
    assert_eq!(output("show-options", &["-gv", "@binary"]), b"a\xfeb");
    assert_eq!(
        output("show-options", &["-g", "@binary"]),
        b"@binary a\\376b"
    );
    assert_eq!(output("display-message", &["-p", "#{@binary}"]), b"a\xfeb");
    assert_eq!(output("show-buffer", &["-b", "sourced"]), b"c\xfed");
    assert_eq!(
        output("show-environment", &["-g", "BINARY_ENV"]),
        b"BINARY_ENV=e\xfef"
    );
}

#[test]
fn sourced_command_blocks_and_variables_keep_bytes_that_are_not_utf8() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(1);
    let mut context = ExecutionContext::default();
    let mut source = |stdin: &[u8]| {
        let mut command = CommandInvocation::new("source-file", ["-"]);
        command.set_stdin(RawText::from_bytes(stdin.to_vec()));
        assert!(matches!(
            shared.execute_command_request(client, ClientKind::Command, &mut context, 1, &command),
            CommandResponse::Success { exit_code: 0, .. }
        ));
    };
    source(b"if-shell -F 1 { set-buffer -b block a\xfeb }\n");
    source(b"BYTES_VARIABLE=a\xfdb\n");
    source(b"set-buffer -b variable \"$BYTES_VARIABLE\"\n");
    source(b"OCTAL_VARIABLE=a\\375b\n");
    source(b"set-buffer -b octal \"$OCTAL_VARIABLE\"\n");
    source(b"set-buffer -b escaped \"\\303\\251\\376\"\n");
    let mut buffer = |name: &str| {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-buffer", ["-b", name]),
            )
            .expect("show-buffer")
            .output
            .as_bytes()
            .to_vec()
    };
    assert_eq!(buffer("block"), b"a\xfeb");
    assert_eq!(buffer("variable"), b"a\xfdb");
    assert_eq!(buffer("octal"), b"a\xfdb");
    assert_eq!(buffer("escaped"), b"\xc3\xa9\xfe");
}

#[test]
fn config_assignments_hidden_readback_expansion_and_conditions_match_the_pin() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(
        &mux_config,
        "CFGVAR=fromconfig\n\
             %hidden CFGHID=hiddencfg\n\
             JOINED=$CFGVAR-${CFGVAR}\n\
             ORDER=first\n\
             set-environment -g ORDER command\n\
             ORDER=last\n\
             %if \"#{==:x,x}\"\n\
             set-environment -g TRUE_BRANCH selected\n\
             %else\n\
             set-environment -g TRUE_BRANCH wrong\n\
             %endif\n\
             %if \"#(printf spawned)\"\n\
             set-environment -g NOJOB wrong\n\
             %else\n\
             set-environment -g NOJOB empty\n\
             %endif\n",
    )
    .expect("mux config fixture");

    let shared = Arc::new(Shared::new(48));
    let mut context = ExecutionContext::default();
    shared
        .load_config_file(&mux_config, &mut context, 0)
        .expect("source mux config");

    for (args, expected) in [
        (&["-g", "CFGVAR"] as &[&str], "CFGVAR=fromconfig"),
        (&["-gh", "CFGHID"], "CFGHID=hiddencfg"),
        (&["-g", "CFGHID"], ""),
        (&["-g", "JOINED"], "JOINED=fromconfig-fromconfig"),
        (&["-g", "ORDER"], "ORDER=command"),
        (&["-g", "TRUE_BRANCH"], "TRUE_BRANCH=selected"),
        (&["-g", "NOJOB"], "NOJOB=empty"),
    ] {
        let output = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", args.iter().copied()),
            )
            .expect("show config environment")
            .output;
        assert_eq!(output, expected, "{args:?}");
    }
}

#[cfg(unix)]
#[test]
fn config_shell_commands_block_and_execute_in_order_without_an_output_sink() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let ready = directory.path().join("ready");
    let ready = shell_quote(&ready);
    let missing = shell_quote(&directory.path().join("missing-tpm"));
    let unexpected = directory.path().join("unexpected-tpm-run");
    let config = directory.path().join("mux.conf");
    fs::write(
        &config,
        format!(
            "run-shell \"sleep 0.05; printf ready > {ready}\"\n\
                 if-shell \"test -f {ready}\" \"set-option -g @phase ready\" \
                 \"set-option -g @phase missed\"\n\
                 if-shell \"test -d {missing}\" \
                 \"run-shell 'printf unexpected > {}'\"\n\
                 run-shell \"printf hidden\"\n",
            unexpected.display()
        ),
    )
    .expect("shell config fixture");

    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    let started = Instant::now();
    shared
        .load_config_file(&config, &mut context, 0)
        .expect("blocking shell config");
    assert!(started.elapsed() >= Duration::from_millis(40));
    let phase = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@phase"]),
        )
        .expect("config branch result");
    assert_eq!(phase.output, "ready");
    assert!(!unexpected.exists());
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_output.is_none())
    );
}

#[cfg(unix)]
#[test]
fn source_file_globs_follow_the_posix_dialect_losslessly() {
    use std::os::unix::ffi::OsStrExt as _;

    assert_eq!(
        source_working_directory_glob_prefix(Path::new("/tmp/client [literal]*?\\ cwd"))
            .as_os_str()
            .as_bytes(),
        b"/tmp/client\\ \\[literal\\]\\*\\?\\\\\\ cwd"
    );
    assert_eq!(
        resolve_source_path(
            &RawText::from("~/literal.conf"),
            Some(Path::new("/tmp/client cwd"))
        ),
        PathBuf::from("/tmp/client\\ cwd/~/literal.conf")
    );
    assert_eq!(
        expand_relative(
            Path::new("/tmp/nested source/entry.conf"),
            &RawText::from("~/literal.conf")
        ),
        PathBuf::from("/tmp/nested\\ source/~/literal.conf")
    );

    let directory = tempfile::tempdir().expect("temporary directory");
    let escaped = directory.path().join("escaped [literal]*?\\ cwd");
    fs::create_dir(&escaped).expect("escaped-character directory");
    let literal_star = escaped.join("literal*.conf");
    fs::write(&literal_star, "").expect("literal-star file");
    fs::write(escaped.join("literalX.conf"), "").expect("wildcard peer");
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("literal\\*.conf"),
        Some(&escaped),
    ));
    assert_eq!(matches.paths, [literal_star]);
    assert!(matches.errors.is_empty());

    let nested_literal_parent = directory.path().join("nested[1]");
    let nested_pattern_peer = directory.path().join("nested1");
    fs::create_dir(&nested_literal_parent).expect("literal nested-source parent");
    fs::create_dir(&nested_pattern_peer).expect("pattern nested-source peer");
    let nested_entry = nested_literal_parent.join("entry.conf");
    fs::write(&nested_entry, "").expect("nested-source entry");
    let nested_literal_leaf = nested_literal_parent.join("leaf.conf");
    fs::write(&nested_literal_leaf, "").expect("literal nested-source leaf");
    fs::write(nested_pattern_peer.join("leaf.conf"), "").expect("misleading nested-source peer");
    let matches = source_glob_matches(&expand_relative(&nested_entry, &RawText::from("leaf.conf")));
    assert_eq!(matches.paths, [nested_literal_leaf]);
    assert!(matches.errors.is_empty());

    let hidden = directory.path().join("hidden");
    fs::create_dir(&hidden).expect("hidden-file directory");
    let visible = hidden.join("visible.conf");
    fs::write(&visible, "").expect("visible file");
    fs::write(hidden.join(".hidden.conf"), "").expect("hidden file");
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("*.conf"),
        Some(&hidden),
    ));
    assert_eq!(matches.paths, [visible]);
    assert!(matches.errors.is_empty());

    let double_star = directory.path().join("double-star");
    let one = double_star.join("one");
    let deep = one.join("deep");
    fs::create_dir_all(&deep).expect("double-star directories");
    fs::write(double_star.join("root.conf"), "").expect("root file");
    let one_file = one.join("one.conf");
    fs::write(&one_file, "").expect("one-component file");
    fs::write(deep.join("deep.conf"), "").expect("deep file");
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("**/*.conf"),
        Some(&double_star),
    ));
    assert_eq!(matches.paths, [one_file]);
    assert!(matches.errors.is_empty());

    let triple_star = directory.path().join("triple-star");
    fs::create_dir(&triple_star).expect("triple-star directory");
    let alpha = triple_star.join("alpha.conf");
    fs::write(&alpha, "").expect("triple-star match");
    fs::write(triple_star.join("alpha.txt"), "").expect("triple-star miss");
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("***.conf"),
        Some(&triple_star),
    ));
    assert_eq!(matches.paths, [alpha]);
    assert!(matches.errors.is_empty());

    let prefix_siblings = directory.path().join("prefix-siblings");
    let mut expected = Vec::new();
    for name in ["zz-client-ffi", "zz-client", "zz"] {
        let child = prefix_siblings.join(name);
        fs::create_dir_all(&child).expect("prefix-sibling directory");
        let path = child.join("match.conf");
        fs::write(&path, "").expect("prefix-sibling match");
        expected.push(path);
    }
    let SourceGlobMatches { mut paths, errors } =
        source_glob_matches(&prefix_siblings.join("zz*/match.conf"));
    paths.sort();
    expected.sort();
    assert_eq!(paths, expected);
    assert!(errors.is_empty());

    let bracket = directory.path().join("bracket");
    fs::create_dir(&bracket).expect("bracket directory");
    let literal_bracket = bracket.join("literal[.conf");
    fs::write(&literal_bracket, "").expect("literal-bracket file");
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("literal[.conf"),
        Some(&bracket),
    ));
    assert_eq!(matches.paths, [literal_bracket]);
    assert!(matches.errors.is_empty());
    let matches = source_glob_matches(&resolve_source_path(
        &RawText::from("missing[.conf"),
        Some(&bracket),
    ));
    assert!(matches.paths.is_empty());
    assert!(matches.errors.is_empty());

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStringExt as _;

        let non_utf8_name = std::ffi::OsString::from_vec(b"non-utf8-\xff.conf".to_vec());
        let non_utf8 = directory.path().join(&non_utf8_name);
        fs::write(&non_utf8, "").expect("non-UTF-8 file");
        let matches = source_glob_matches(&directory.path().join("non-utf8-?.conf"));
        assert_eq!(matches.paths, [non_utf8]);
        assert!(matches.errors.is_empty());
    }

    let nul = std::ffi::OsStr::from_bytes(b"/tmp/source\0glob");
    let matches = source_glob_matches(Path::new(nul));
    assert!(matches.paths.is_empty());
    assert_eq!(matches.errors.len(), 1);
    assert_eq!(
        source_glob_error_warning(Path::new("declared.conf"), &matches.errors[0]),
        format!("{}: declared.conf", matches.errors[0])
    );
}

#[test]
fn source_file_globs_nested_sources_and_refuses_standard_input() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let top = directory.path().join("top");
    let nested = directory.path().join("nested");
    fs::create_dir_all(&top).expect("top-level source directory");
    fs::create_dir_all(&nested).expect("nested source directory");
    fs::write(
        top.join("10-first.conf"),
        "set -g prefix C-a\nbind-key F2 new-window\n",
    )
    .expect("first top-level source");
    fs::write(
        top.join("20-second.conf"),
        "set -g prefix C-z\nbind-key F3 kill-pane\n",
    )
    .expect("second top-level source");
    fs::write(nested.join("10-first.conf"), "bind-key F4 new-window\n")
        .expect("first nested source");
    fs::write(nested.join("20-second.conf"), "bind-key F5 kill-pane\n")
        .expect("second nested source");
    let entry = directory.path().join("entry.conf");
    fs::write(
        &entry,
        format!("source-file '{}'\n", nested.join("*.conf").display()),
    )
    .expect("nested source entry");

    let shared = Arc::new(Shared::new(50));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "source-glob"]),
        )
        .expect("source-file session");
    take_reliable_messages(&mailbox);

    let top_pattern = top.join("*.conf").display().to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", [&top_pattern]),
        )
        .expect("top-level source glob");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", [entry.display().to_string()]),
        )
        .expect("nested source glob");
    let formatted = directory.path().join("formatted");
    fs::create_dir_all(&formatted).expect("formatted source directory");
    fs::write(
        formatted.join("source-glob-0-0-10.conf"),
        "bind-key F9 new-window\n",
    )
    .expect("first formatted source");
    fs::write(
        formatted.join("source-glob-0-0-20.conf"),
        "bind-key F10 new-window\n",
    )
    .expect("second formatted source");
    let formatted_pattern = formatted
        .join("#{session_name}-#{window_index}-#{pane_index}-*.conf")
        .display()
        .to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", ["-F", &formatted_pattern]),
        )
        .expect("format-expanded source glob");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.keys.prefix(), "C-z");
        for key in ["F2", "F3", "F4", "F5", "F9", "F10"] {
            assert!(inner.engine.keys.get("prefix", key).is_some(), "{key}");
        }
    }

    let verbose_directory = directory.path().join("verbose");
    fs::create_dir_all(&verbose_directory).expect("verbose source directory");
    fs::write(
        verbose_directory.join("loaded.conf"),
        "bind-key F8 new-window\n",
    )
    .expect("verbose nested source");
    let verbose_entry = directory.path().join("verbose-entry.conf");
    fs::write(
        &verbose_entry,
        "source-file -v 'verbose/*.conf'\nbind-key F7 new-window\n",
    )
    .expect("verbose source entry");
    let mut report = ConfigLoadReport::default();
    shared
        .load_config_file_with_report(&verbose_entry, &mut context, 0, &mut report)
        .expect("verbose nested source loads");
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.keys.get("prefix", "F7").is_some());
        assert!(inner.engine.keys.get("prefix", "F8").is_some());
    }
    assert_eq!(
        report.verbose_lines(),
        [format!(
            "{}:1: bind-key F8 new-window",
            verbose_directory.join("loaded.conf").display()
        )]
    );
    assert!(report.summary().is_none());

    take_reliable_messages(&mailbox);
    let missing = directory
        .path()
        .join("missing-*.conf")
        .display()
        .to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", ["-q", &missing]),
        )
        .expect("quiet no-match source glob");
    assert!(!take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage { .. },
                ..
            })
        )
    }));

    let malformed = directory.path().join("[").display().to_string();
    let after_error = directory.path().join("after-error.conf");
    fs::write(&after_error, "bind-key F6 new-window\n").expect("post-error source");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "source-file",
                ["-q", &malformed, &after_error.display().to_string()],
            ),
        )
        .expect("glob no-match continues to later sources");
    assert!(
        shared
            .inner
            .lock()
            .engine
            .keys
            .get("prefix", "F6")
            .is_some()
    );
    let missing_warning = format!("No such file or directory: {missing}");
    assert!(!take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Warning,
                    ..
                },
                ..
            })
        )
    }));

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", [&missing]),
        ),
        Err(DaemonError::CommandExit {
            output,
            exit_code: 1,
        }) if output.is_empty()
    ));
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Warning,
                    text,
                    ..
                },
                ..
            }) if text == &missing_warning
        )
    }));

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("source-file", ["-q", "-"]),
        ),
        Err(DaemonError::CommandExit {
            output,
            exit_code: 1,
        }) if output.is_empty()
    ));
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Warning,
                    text,
                    ..
                },
                ..
            }) if text.contains("source-file from standard input is not supported")
        )
    }));
}

#[cfg(unix)]
#[test]
fn source_file_of_a_directory_answers_the_command_client_read_error() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().display().to_string();
    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    for (request_id, args) in [
        (1, vec![path.clone()]),
        (2, vec!["-q".to_owned(), path.clone()]),
    ] {
        let response = shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            request_id,
            &CommandInvocation::new("source-file", args),
        );
        let text = format!("{response:?}");
        assert!(
            text.contains(&format!("Input/output error: {path}")),
            "{text}"
        );
        assert!(!text.contains("os error"), "{text}");
    }
    let nested = directory.path().join("nested.conf");
    fs::write(&nested, format!("source-file '{path}'\n")).expect("nested source");
    let response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        3,
        &CommandInvocation::new("source-file", [nested.display().to_string()]),
    );
    let text = format!("{response:?}");
    assert!(
        text.contains(&format!("Input/output error: {path}")),
        "{text}"
    );
    assert!(!text.contains("os error"), "{text}");
    let directory_error = || std::io::Error::from_raw_os_error(libc::EISDIR);
    assert_eq!(
        client_source_read_error_warning(false, directory.path(), &directory_error()),
        format!("Is a directory: {path}")
    );
}

#[test]
fn source_file_diagnostics_split_streams_for_command_clients_only() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let bad = directory.path().join("bad.conf");
    fs::write(&bad, "wibble\nwibble\n").expect("invalid source fixture");
    let bad = bad.display().to_string();
    let missing = directory.path().join("missing.conf").display().to_string();
    let good = directory.path().join("good.conf");
    fs::write(&good, "").expect("valid source fixture");
    let good = good.display().to_string();
    let verbose_leaf = directory.path().join("verbose-leaf.conf");
    fs::write(&verbose_leaf, "set-option -g @verbose-source loaded\n")
        .expect("verbose source leaf");
    let verbose = directory.path().join("verbose.conf");
    fs::write(&verbose, "source-file -v verbose-leaf.conf\n").expect("verbose source");
    let verbose = verbose.display().to_string();

    let shared = Arc::new(Shared::new(63));
    let mailbox = OutboundMailbox::new();
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();

    let invalid = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [&bad]),
    );
    assert_eq!(
        invalid,
        CommandResponse::Success {
            request_id: 1,
            output: format!("{bad}:1: unknown command: wibble\n").into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let quiet = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("source-file", ["-q", &missing]),
    );
    assert_eq!(
        quiet,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );

    let loud = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        3,
        &CommandInvocation::new("source-file", [&bad, &missing]),
    );
    assert_eq!(
        loud,
        CommandResponse::Success {
            request_id: 3,
            output: format!("{bad}:1: unknown command: wibble\n").into(),
            exit_code: 1,
            stderr: format!("No such file or directory: {missing}\n"),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let verbose_response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        4,
        &CommandInvocation::new("source-file", [&verbose]),
    );
    assert_eq!(
        verbose_response,
        CommandResponse::Success {
            request_id: 4,
            output: format!(
                "{}:1: set-option -g @verbose-source loaded\n",
                verbose_leaf.display()
            )
            .into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let control = ClientId(u64::from(u16::MAX) - 1);
    let control_loud = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut context,
        5,
        &CommandInvocation::new("source-file", [&missing]),
    );
    assert_eq!(
        control_loud,
        CommandResponse::Error {
            request_id: 5,
            error: ServerError::InvalidCommand(format!("No such file or directory: {missing}")),
            output: RawText::default(),
        }
    );
    let control_partial = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut context,
        6,
        &CommandInvocation::new("source-file", [&good, &missing]),
    );
    assert_eq!(
        control_partial,
        CommandResponse::Success {
            request_id: 6,
            output: format!("No such file or directory: {missing}\n").into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );

    take_reliable_messages(&mailbox);
    let mut interactive_context = ExecutionContext::default();
    let attended = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut interactive_context,
        7,
        &CommandInvocation::new("source-file", [&bad, &missing]),
    );
    assert_eq!(
        attended,
        CommandResponse::Success {
            request_id: 7,
            output: format!("{bad}:1: unknown command: wibble\n").into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    let warnings = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ClientMessage {
                        kind: ClientMessageKind::Warning,
                        text,
                        ..
                    },
                ..
            }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        warnings,
        [
            format!("No such file or directory: {missing}"),
            format!("{bad}:1: unknown command: wibble"),
        ]
    );
}

#[test]
fn source_file_qualifies_post_admission_parse_errors() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let client = ClientId(u64::from(u16::MAX));

    for (request_id, (name, contents)) in (1_u64..).zip([
        (
            "if-shell.conf",
            "if-shell -F 1 \"{ display-message -p quoted }\"\n",
        ),
        (
            "run-shell.conf",
            "run-shell -C \"{ display-message -p quoted }\"\n",
        ),
        (
            "nested-if-shell.conf",
            "if-shell -F 1 \"if-shell -F 1 \\\"{ display-message -p quoted }\\\"\"\n",
        ),
        (
            "nested-run-shell.conf",
            "run-shell -C \"run-shell -C \\\"{ display-message -p quoted }\\\"\"\n",
        ),
    ]) {
        let path = directory.path().join(name);
        fs::write(&path, contents).expect("source fixture");
        let path = path.display().to_string();
        let mut context = ExecutionContext::default();

        assert_eq!(
            shared.execute_command_request(
                client,
                ClientKind::Command,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", [&path]),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: format!("{path}:1: syntax error\n"),
                stdout_claim: StdoutClaim::None,
            }
        );
    }

    let inner = directory.path().join("inner.conf");
    fs::write(&inner, "if-shell -F 1 \"{ display-message -p quoted }\"\n")
        .expect("inner source fixture");
    let outer = directory.path().join("outer.conf");
    fs::write(
        &outer,
        format!("if-shell -F 1 \"source-file '{}'\"\n", tmux_path(&inner)),
    )
    .expect("outer source fixture");
    let inner = inner.display().to_string();
    let outer = outer.display().to_string();
    let mut context = ExecutionContext::default();

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new("source-file", [&outer]),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{inner}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
}

#[cfg(unix)]
#[test]
fn a_source_path_no_utf8_string_can_spell_reaches_the_file_system_whole() {
    use std::os::unix::ffi::OsStrExt as _;

    let byte_path =
        RawText::from_bytes([b"/tmp/zz-".as_slice(), &[0xff], b"-real.conf".as_slice()].concat());
    assert_eq!(byte_path.to_utf8(), None);
    assert_eq!(
        resolve_source_path(&byte_path, Some(Path::new("/tmp/cwd")))
            .as_os_str()
            .as_bytes(),
        byte_path.as_bytes()
    );

    let byte_leaf = RawText::from_bytes([b"lea".as_slice(), &[0xff], b".conf".as_slice()].concat());
    let joined = [b"/tmp/cwd/lea".as_slice(), &[0xff], b".conf".as_slice()].concat();
    assert_eq!(
        resolve_source_path(&byte_leaf, Some(Path::new("/tmp/cwd")))
            .as_os_str()
            .as_bytes(),
        joined
    );
    assert_eq!(
        expand_relative(Path::new("/tmp/cwd/entry.conf"), &byte_leaf)
            .as_os_str()
            .as_bytes(),
        joined
    );
}
