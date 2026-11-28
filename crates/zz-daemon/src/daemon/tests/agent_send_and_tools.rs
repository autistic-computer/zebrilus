use super::*;

#[test]
fn agent_send_notify_submits_and_refuses_wait() {
    let parsed =
        parse_agent_send_args(&["-t", "%2", "--notify", "go"].map(RawText::from)).expect("parse");
    assert_eq!(
        (
            parsed.notify,
            parsed.submit,
            parsed.wait,
            parsed.target.as_deref(),
            parsed.text.as_slice()
        ),
        (true, true, false, Some("%2"), ["go".to_owned()].as_slice())
    );
    assert_eq!(
        parse_agent_send_args(&["--notify", "--wait", "go"].map(RawText::from)).err(),
        Some(ServerError::CommandParse(
            "agent-send --notify and --wait cannot be combined".to_owned()
        ))
    );
    assert_eq!(
        parse_agent_send_args(&["--notify", "--timeout", "5", "go"].map(RawText::from)).err(),
        Some(ServerError::CommandParse(
            "agent-send --timeout needs --wait".to_owned()
        ))
    );
}

#[cfg(feature = "agent")]
#[test]
fn agent_notify_messages_carry_the_reply_or_why_there_is_none() {
    use crate::agent::host::{AgentTurnFailure, AgentTurnReply};

    let pane = PaneId(5);
    let reply = AgentTurnReply {
        text: "thinking... done".to_owned(),
        final_text: "done".to_owned(),
        ..AgentTurnReply::default()
    };
    assert_eq!(
        agent_notify_message(pane, Some(Ok(reply))),
        "[zz] Agent %5 finished:\n\ndone"
    );
    assert_eq!(
        agent_notify_message(pane, Some(Err(AgentTurnFailure::Cancelled))),
        "[zz] Agent %5 was stopped."
    );
    assert_eq!(
        agent_notify_message(pane, None),
        "[zz] Agent %5 did not take the message."
    );
}

#[test]
fn agent_send_parser_reads_target_context_and_trailing_text() {
    let args = [
        "-t%4",
        "--submit",
        "--context",
        "src/lib.rs:10-42",
        "--",
        "--- a/src/lib.rs",
    ]
    .map(RawText::from);
    let parsed = parse_agent_send_args(&args).expect("agent-send args");
    assert_eq!(parsed.target.as_deref(), Some("%4"));
    assert!(parsed.submit);
    assert_eq!(
        parsed.context,
        Some(ContextReference {
            path: "src/lib.rs".to_owned(),
            start: Some(10),
            end: Some(42),
        })
    );
    assert_eq!(parsed.text, ["--- a/src/lib.rs"]);

    let attached = ["--context=notes.md", "-t", "%9", "hello", "world"].map(RawText::from);
    let parsed = parse_agent_send_args(&attached).expect("attached options");
    assert_eq!(parsed.target.as_deref(), Some("%9"));
    assert!(!parsed.submit);
    assert_eq!(
        parsed
            .context
            .as_ref()
            .map(ContextReference::header)
            .as_deref(),
        Some("notes.md")
    );
    assert_eq!(
        parsed.payload().expect("payload"),
        "notes.md\n```\nhello world\n```"
    );
}

#[test]
fn inspect_all_json_orders_panes_and_separates_text() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("new-session", ["-d", "-s", "z-first"]),
        CommandInvocation::new("split-window", ["--kind", "picker"]),
    ] {
        shared
            .inner
            .lock()
            .engine
            .execute(&mut context, &command)
            .unwrap();
    }
    let rows = shared
        .inspect(client, &context, &["-a".into(), "--json".into()])
        .unwrap()
        .output;
    let rows = rows
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["pane_id"], "%0");
    assert_eq!(rows[1]["pane_id"], "%1");
    assert!(
        rows.iter()
            .all(|row| row.as_object().unwrap().len() == INSPECT_FIELDS.len())
    );
    assert!(rows.iter().all(|row| {
        !row["events"]
            .as_array()
            .unwrap()
            .contains(&"agent-tool-call".into())
    }));
    let text = shared
        .inspect(client, &context, &["-a".into()])
        .unwrap()
        .output;
    assert_eq!(text.split("\n\n").count(), 2);
    for command in [
        CommandInvocation::new("new-window", ["-t", "z-first:5"]),
        CommandInvocation::new("new-window", ["-t", "z-first:2"]),
        CommandInvocation::new("new-session", ["-d", "-s", "a-second"]),
    ] {
        shared
            .inner
            .lock()
            .engine
            .execute(&mut context, &command)
            .unwrap();
    }
    let output = shared
        .inspect(client, &context, &["-a".into(), "--json".into()])
        .unwrap()
        .output;
    let panes = output
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["pane_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(panes, ["%0", "%1", "%3", "%2", "%4"]);
}

#[test]
fn inspect_all_rejects_target_with_exit_two() {
    let shared = Arc::new(Shared::new(1));
    let error = shared
        .inspect(
            ClientId(7),
            &ExecutionContext::default(),
            &["-a".into(), "-t".into(), "%1".into()],
        )
        .unwrap_err();
    let DaemonError::Server(error) = error else {
        panic!("expected usage error");
    };
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn agent_send_progress_requires_wait_and_explicit_pane() {
    for args in [
        vec!["--progress", "-t", "%1"],
        vec!["--progress", "--wait"],
        vec!["--progress", "--wait", "-t", "work"],
    ] {
        let error = parse_agent_send_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>())
            .unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(
            error
                .tmux_message()
                .contains("--wait and an explicit -t %N")
        );
    }
    let parsed =
        parse_agent_send_args(&["--progress", "--wait", "-t%1", "hi"].map(RawText::from)).unwrap();
    assert!(parsed.wait && parsed.submit);
    assert_eq!(parsed.text, ["hi"]);
}

#[test]
fn inspect_reports_kind_facts_and_verbs() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let run = |args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("inspect fixture command")
            .output
    };
    run(&["new-session", "-d", "-s", "inspect"]);
    let inspect = || {
        serde_json::from_str::<serde_json::Value>(&run(&["inspect", "--json", "-t", "%0"]))
            .expect("inspect JSON object")
    };
    let row = inspect();
    assert_eq!(row["session_id"], "$0");
    assert_eq!(row["session_name"], "inspect");
    assert_eq!(row["window_id"], "@0");
    assert_eq!(row["pane_kind"], "terminal");
    assert_eq!(row["pane_id"], "%0");
    assert_eq!(row["window_size"], "latest");
    assert_eq!(row["agent_state"], "");
    assert!(row["permission"].is_null());
    let verbs = row["verbs"].as_array().expect("verbs array");
    assert!(verbs.contains(&"wait-pane".into()));
    assert!(!verbs.contains(&"agent-send".into()));
    assert!(!verbs.contains(&"select-pane-kind".into()));
    assert!(
        row["events"]
            .as_array()
            .unwrap()
            .contains(&"pane-exited".into())
    );
    run(&["set-option", "-p", "-t", "%0", "@agent_state", "idle"]);
    let row = inspect();
    assert_eq!(row["agent_state"], "idle");
    assert!(
        row["verbs"]
            .as_array()
            .unwrap()
            .contains(&"agent-send".into())
    );
}

#[test]
fn inspect_verb_table_names_real_commands() {
    for (kind, verbs) in INSPECT_VERBS {
        assert_eq!(verbs.contains(&"select-pane-kind"), *kind == "picker");
        assert!(!verbs.contains(&"set-browser-tabs"));
        assert!(!verbs.contains(&"set-editor-path"));
        for verb in *verbs {
            assert!(
                zz_mux::command_spec(verb).is_some()
                    || zz_protocol::catalog_command_spec(verb).is_some(),
                "inspect lists an unknown command: {verb}"
            );
        }
    }
}

#[test]
fn inspect_text_lists_fields_in_order() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "inspect-text"]),
        )
        .expect("inspect text session");
    let output = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("inspect", ["-t", "%0"]),
        )
        .expect("inspect text")
        .output;
    assert!(output.lines().any(|line| line == "pane_kind: terminal"));
    assert!(
        output
            .lines()
            .any(|line| line.starts_with("verbs: send-keys send-text "))
    );
    let keys = output
        .lines()
        .map(|line| {
            let (key, value) = line.split_once(": ").expect("field line");
            assert!(!value.is_empty(), "{line:?}");
            key
        })
        .collect::<Vec<_>>();
    for empty in [
        "agent_state",
        "agent_pending_permission",
        "permission",
        "browser_url",
    ] {
        assert!(
            !keys.contains(&empty),
            "{empty} is empty on a terminal pane"
        );
    }
    let json = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("inspect", ["--json", "-t", "%0"]),
        )
        .expect("inspect json")
        .output;
    let row = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&json)
        .expect("inspect JSON object");
    assert_eq!(row["agent_state"], "");
    assert!(row["permission"].is_null());
    let mut ordered = INSPECT_FIELDS.iter().filter(|field| keys.contains(field));
    assert!(keys.iter().all(|key| ordered.next() == Some(key)));
    assert_eq!(keys.first(), Some(&"session_id"));
    assert_eq!(keys.last(), Some(&"events"));
    assert_eq!(
        row.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        [
            "session_id",
            "session_name",
            "window_id",
            "window_index",
            "window_name",
            "window_width",
            "window_height",
            "window_size",
            "pane_id",
            "pane_index",
            "pane_active",
            "pane_kind",
            "pane_pid",
            "pane_current_command",
            "pane_current_path",
            "pane_title",
            "pane_width",
            "pane_height",
            "pane_dead",
            "pane_dead_status",
            "pane_dead_signal",
            "pane_last_command_status",
            "pane_pb_state",
            "pane_pb_progress",
            "pane_status",
            "pane_status_kind",
            "pane_status_progress",
            "pane_status_app",
            "pane_status_title",
            "pane_status_message",
            "pane_status_raw_title",
            "pane_status_raw_message",
            "pane_status_reported",
            "agent_state",
            "agent_pending_permission",
            "agent_provider",
            "agent_model",
            "agent_session_id",
            "agent_parent",
            "permission",
            "browser_url",
            "verbs",
            "events",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
}

#[test]
fn formats_expand_pane_kind_and_user_options() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, Arc::clone(&mailbox));
    let run = |args: &[&str]| {
        let mut context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .map(|execution| (execution.output, context))
    };
    let (_, context) = run(&["new-session", "-d", "-s", "kinds"]).expect("session");
    let pane = context.pane.expect("terminal pane").to_string();
    let display = |format: &str| {
        run(&["display-message", "-p", "-t", &pane, format])
            .expect("display-message")
            .0
    };

    assert_eq!(display("#{pane_kind}|#{@agent_state}|"), "terminal||");

    run(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]).expect("pane option");
    run(&["set-option", "-w", "-t", &pane, "@team", "blue"]).expect("window option");
    run(&["set-option", "-g", "@fleet", "north"]).expect("global option");
    assert_eq!(
        display("#{pane_kind}|#{@agent_state}|#{@team}|#{@fleet}"),
        "terminal|idle|blue|north"
    );

    run(&["set-option", "-p", "-t", &pane, "-u", "@agent_state"]).expect("unset");
    assert_eq!(display("#{@agent_state}|#{@team}"), "|blue");

    let (_, context) =
        run(&["split-window", "--kind", "picker", "-v", "-t", &pane]).expect("picker");
    let picker = context.pane.expect("picker pane").to_string();
    assert_eq!(
        run(&["display-message", "-p", "-t", &picker, "#{pane_kind}"])
            .expect("display-message")
            .0,
        "picker"
    );
}

#[test]
fn formats_expand_the_zz_version_and_revision() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "#{zz_version}|#{zz_revision}"]),
        )
        .expect("display-message")
        .output;

    assert_eq!(
        output,
        format!("{}|{}", env!("CARGO_PKG_VERSION"), zz_protocol::REVISION)
    );
}

#[test]
fn agent_send_reply_flags_and_policies_validate_usage() {
    for args in [
        vec!["--json"],
        vec!["--final"],
        vec!["--wait", "--json", "--final"],
        vec!["--wait", "--on-block", "x"],
    ] {
        let arguments = args.iter().copied().map(RawText::from).collect::<Vec<_>>();
        assert!(matches!(
            parse_agent_send_args(&arguments),
            Err(ServerError::CommandParse(_))
        ));
        let shared = Arc::new(Shared::new(1));
        let error = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("agent-send", args),
            )
            .expect_err("usage error");
        assert_eq!(daemon_server_error(error).exit_code(), 2);
    }
    for (name, policy) in [
        ("allow", AgentBlockPolicy::Allow),
        ("deny", AgentBlockPolicy::Deny),
    ] {
        let parsed = parse_agent_send_args(&["--wait", "--on-block", name].map(RawText::from))
            .expect("policy");
        assert_eq!(parsed.on_block, policy);
    }
    for flag in ["--json", "--final"] {
        let parsed =
            parse_agent_send_args(&["--wait", flag].map(RawText::from)).expect("output mode");
        assert_eq!(parsed.json, flag == "--json");
        assert_eq!(parsed.final_only, flag == "--final");
    }
}

#[cfg(feature = "agent")]
#[test]
fn agent_reply_json_and_final_output_preserve_facts() {
    let reply = crate::agent::host::AgentTurnReply {
        text: "before\nafter".to_owned(),
        final_text: "after".to_owned(),
        stop_reason: "refusal".to_owned(),
        duration: Duration::from_millis(123),
        tool_calls: 2,
        permissions_requested: 3,
        permissions_allowed: 1,
        permissions_denied: 2,
        truncated: true,
    };
    let mut parsed = ParsedAgentSend {
        json: true,
        ..Default::default()
    };
    let output = agent_turn_reply_output(PaneId(7), &reply, &parsed, None);
    assert_eq!(
        output,
        r#"{"pane":"%7","stop_reason":"refusal","duration_ms":123,"tool_calls":2,"permissions":{"requested":3,"allowed":1,"denied":2},"truncated":true,"final_text":"after","transcript":"before\nafter"}"#
    );
    parsed.json = false;
    parsed.final_only = true;
    assert_eq!(
        agent_turn_reply_output(PaneId(7), &reply, &parsed, None),
        "after"
    );
    parsed.json = true;
    let permission = agent_permission_json(&zz_protocol::AgentPermissionWire {
        request_id: 7,
        payload: r#"{"toolCall":{"title":"run"},"options":[]}"#.to_owned(),
    });
    assert_eq!(
        permission,
        serde_json::json!({"request_id": 7, "tool_call": {"title": "run"}, "options": []})
    );
    let output: serde_json::Value = serde_json::from_str(&agent_turn_reply_output(
        PaneId(7),
        &reply,
        &parsed,
        Some(&permission),
    ))
    .expect("JSON");
    assert_eq!(output["permission"], permission);
    let terminal: serde_json::Value = serde_json::from_str(&agent_terminal_reply_output(
        PaneId(8),
        "peer reply",
        Duration::from_millis(50),
        "end_turn",
        &parsed,
    ))
    .expect("JSON");
    assert_eq!(terminal["duration_ms"], 50);
    assert_eq!(terminal["final_text"], "peer reply");
    assert_eq!(terminal["transcript"], "peer reply");
    assert_eq!(terminal["tool_calls"], 0);
    assert_eq!(terminal["truncated"], false);
}

#[test]
fn agent_send_parser_reads_wait_and_timeout() {
    let parsed =
        parse_agent_send_args(&["--wait", "-t", "%1", "hi"].map(RawText::from)).expect("wait args");
    assert!(parsed.submit && parsed.wait);
    assert_eq!(parsed.wait_timeout(), Some(AGENT_SEND_WAIT_TIMEOUT));

    let parsed = parse_agent_send_args(&["--wait", "--timeout=30", "hi"].map(RawText::from))
        .expect("attached timeout");
    assert_eq!(parsed.wait_timeout(), Some(Duration::from_secs(30)));

    let parsed = parse_agent_send_args(&["--wait", "--timeout", "0", "hi"].map(RawText::from))
        .expect("zero waits forever");
    assert_eq!(parsed.wait_timeout(), None);

    assert!(matches!(
        parse_agent_send_args(&["--wait", "--timeout", "soon"].map(RawText::from)),
        Err(ServerError::CommandParse(_))
    ));
    assert!(matches!(
        parse_agent_send_args(&["--timeout", "5", "hi"].map(RawText::from)),
        Err(ServerError::CommandParse(_))
    ));
    assert!(agent_send_reads_stdin(
        &["--wait", "-t", "%1"].map(RawText::from)
    ));
}

#[test]
fn agent_permission_parsers_require_one_choice_and_a_wait_policy() {
    for args in [
        vec!["--wait", "--on-block", "fail"],
        vec!["--wait", "--on-block=fail"],
    ] {
        assert_eq!(
            parse_agent_send_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>())
                .expect("fail policy")
                .on_block,
            AgentBlockPolicy::Fail
        );
    }
    assert_eq!(
        parse_agent_send_args(&["--wait", "--on-block", "wait"].map(RawText::from))
            .expect("wait policy")
            .on_block,
        AgentBlockPolicy::Wait
    );
    assert_eq!(
        parse_agent_send_args(&["--wait"].map(RawText::from))
            .expect("default policy")
            .on_block,
        AgentBlockPolicy::Wait
    );
    for args in [
        vec!["--on-block", "fail"],
        vec!["--wait", "--on-block", "other"],
        vec!["--wait", "--on-block"],
    ] {
        assert!(
            parse_agent_send_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>())
                .is_err()
        );
    }
    let parsed = parse_agent_respond_args(&["-t%7", "--option=allow", "42"].map(RawText::from))
        .expect("option");
    assert_eq!(parsed.target.as_deref(), Some("%7"));
    assert_eq!(parsed.option.as_deref(), Some("allow"));
    assert_eq!(parsed.request_id, Some(42));
    for args in [
        vec![],
        vec!["--allow", "--deny"],
        vec!["--allow", "--option", "x"],
        vec!["--allow", "wrong"],
        vec!["--allow", "1", "2"],
    ] {
        assert!(
            parse_agent_respond_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>())
                .is_err()
        );
    }
}

#[test]
fn agent_send_reads_stdin_only_when_argv_carries_no_text() {
    assert!(agent_send_reads_stdin(&["-t".into(), "%1".into()]));
    assert!(agent_send_reads_stdin(&["-t%1".into(), "--submit".into()]));
    assert!(!agent_send_reads_stdin(&[
        "-t%1".into(),
        "look at this".into()
    ]));
    assert!(!agent_send_reads_stdin(&[
        "-t%1".into(),
        "--".into(),
        "-piped-".into()
    ]));
    assert!(!agent_send_reads_stdin(&["-Z".into()]));
}

#[test]
fn agent_send_payload_enforces_bounds_and_wraps_context() {
    let empty = ParsedAgentSend::default();
    assert!(matches!(
        empty.payload(),
        Err(ServerError::InvalidCommand(_))
    ));

    let control = ParsedAgentSend {
        text: vec!["before\u{7}after".to_owned()],
        ..ParsedAgentSend::default()
    };
    assert!(matches!(
        control.payload(),
        Err(ServerError::InvalidCommand(_))
    ));

    let oversized = ParsedAgentSend {
        text: vec!["x".repeat(MAX_AGENT_SEND_BYTES + 1)],
        ..ParsedAgentSend::default()
    };
    assert!(matches!(
        oversized.payload(),
        Err(ServerError::InvalidCommand(_))
    ));

    let fenced = ParsedAgentSend {
        context: Some(ContextReference {
            path: "src/lib.rs".to_owned(),
            start: Some(7),
            end: None,
        }),
        text: vec!["```rust\nfn main() {}\n```".to_owned()],
        ..ParsedAgentSend::default()
    };
    assert_eq!(
        fenced.payload().expect("fenced payload"),
        "src/lib.rs:7\n````\n```rust\nfn main() {}\n```\n````"
    );
}

#[test]
fn context_references_keep_colons_that_are_not_line_ranges() {
    assert_eq!(
        ContextReference::parse("C:\\src\\lib.rs").expect("windows path"),
        ContextReference {
            path: "C:\\src\\lib.rs".to_owned(),
            start: None,
            end: None,
        }
    );
    assert_eq!(
        ContextReference::parse("notes.md:12").expect("single line"),
        ContextReference {
            path: "notes.md".to_owned(),
            start: Some(12),
            end: None,
        }
    );
    assert!(ContextReference::parse("notes.md:40-12").is_err());
    assert!(ContextReference::parse("").is_err());
    assert!(ContextReference::parse(&"x".repeat(MAX_CONTEXT_PATH_BYTES + 1)).is_err());
}

#[test]
fn a_file_written_in_two_steps_reads_back_the_same_bytes() {
    let folder = tempfile::tempdir().expect("temporary folder");
    let path = folder.path().join("frame.bin").display().to_string();
    let words = |words: &[&str]| words.iter().copied().map(RawText::from).collect::<Vec<_>>();
    let read = |words: &[RawText]| {
        Shared::read_file(words)
            .map(|execution| execution.output.to_string())
            .map_err(|error| error.to_string())
    };
    Shared::write_file(&words(&[&path, "00ff10"])).expect("write");
    Shared::write_file(&words(&["-a", &path, "0a"])).expect("append");
    assert_eq!(std::fs::read(&path).expect("file"), [0, 255, 16, 10]);
    assert_eq!(read(&words(&[&path])), Ok("00ff100a".to_owned()));
    assert_eq!(
        read(&words(&["-s", "2", "--", &path])),
        Ok("100a".to_owned())
    );
    Shared::write_file(&words(&["-a", "--", &path, ""])).expect("dashes before the path");
    assert_eq!(
        read(&words(&["-s", "1", "-n", "2", &path])),
        Ok("ff10".to_owned())
    );
    assert_eq!(read(&words(&["-s", "9", &path])), Ok(String::new()));
    Shared::write_file(&words(&[&path, ""])).expect("empty");
    assert_eq!(std::fs::read(&path).expect("file"), [0_u8; 0]);
    assert_eq!(
        read(&words(&["frame.bin"])),
        Err(
            "mux command failed: invalid command: read-file: frame.bin must be an absolute path or start with ~/"
                .to_owned()
        )
    );
    assert!(Shared::write_file(&words(&[&path, "0g"])).is_err());
}

#[test]
fn a_page_load_ends_when_the_window_reports_it_or_nothing_loads() {
    let pane = PaneId(4);
    let load = |reported, loading, finished| {
        Some(BrowserLoad {
            reported,
            loading,
            finished,
        })
    };
    let outcome = |state, waited_ms| {
        load_outcome(
            pane,
            state,
            3,
            Duration::from_millis(waited_ms),
            Duration::from_secs(30),
        )
        .map(|result| result.map_err(|error| error.to_string()))
    };
    assert_eq!(outcome(load(true, true, 3), 100), None);
    assert_eq!(outcome(load(true, false, 3), 100), None);
    assert_eq!(outcome(load(true, false, 4), 100), Some(Ok(())));
    assert_eq!(outcome(load(true, false, 3), 1500), Some(Ok(())));
    assert_eq!(outcome(load(true, true, 3), 1500), None);
    assert_eq!(
        outcome(load(false, false, 0), 1500),
        Some(Err(
            "mux command failed: invalid command: %4 went to the page, but no window reported its load: open its session in a zz app that is up to date"
                .to_owned()
        ))
    );
    assert_eq!(
        outcome(None, 0),
        Some(Err(
            "mux command failed: invalid command: %4 is no longer a browser pane".to_owned()
        ))
    );
    assert!(matches!(
        load_outcome(
            pane,
            load(true, true, 3),
            3,
            Duration::from_secs(30),
            Duration::from_secs(30)
        ),
        Some(Err(DaemonError::CommandExit { exit_code: 124, .. }))
    ));
    assert_eq!(
        load_outcome(
            pane,
            load(true, true, 3),
            3,
            Duration::from_secs(90),
            Duration::ZERO
        )
        .map(|result| result.is_ok()),
        None
    );
    let parsed = parse_load_browser_url_args(
        &["-t", "%4", "--timeout", "2.5", "--", "https://example.com/"].map(RawText::from),
    )
    .expect("arguments");
    assert_eq!(
        (
            parsed.target.as_deref(),
            parsed.timeout,
            parsed.url.as_str()
        ),
        (
            Some("%4"),
            Some(Duration::from_millis(2500)),
            "https://example.com/"
        )
    );
    for bad in [&[][..], &["-t", "%4"][..], &["a", "b"][..]] {
        assert!(
            parse_load_browser_url_args(
                &bad.iter().copied().map(RawText::from).collect::<Vec<_>>()
            )
            .is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn load_browser_url_returns_once_the_window_reports_the_load() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let mut run = |name: &str, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .output
            .to_string()
    };
    run("new-session", &["-s", "load"]);
    let browser = run(
        "split-window",
        &["--kind", "browser", "-P", "-F", "#{pane_id}", "about:blank"],
    );
    let browser = browser.trim().to_owned();
    run(
        "set-option",
        &["-p", "-t", &browser, "@browser_loaded", "7"],
    );
    run(
        "set-option",
        &["-p", "-t", &browser, "@browser_loading", "0"],
    );

    let caller = {
        let (shared, browser) = (Arc::clone(&shared), browser.clone());
        thread::spawn(move || {
            shared.execute(
                ClientId(98),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "load-browser-url",
                    ["-t", &browser, "https://example.com/"],
                ),
            )
        })
    };
    let url = |run: &mut dyn FnMut(&str, &[&str]) -> String| {
        run("display-message", &["-p", "-t", &browser, "#{browser_url}"])
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while url(&mut run).trim() != "https://example.com/" {
        assert!(Instant::now() < deadline, "the URL was never set");
        thread::sleep(Duration::from_millis(10));
    }
    run(
        "set-option",
        &["-p", "-t", &browser, "@browser_loading", "1"],
    );
    thread::sleep(Duration::from_millis(200));
    assert!(!caller.is_finished(), "returned while the page was loading");
    run(
        "set-option",
        &["-p", "-t", &browser, "@browser_loaded", "8"],
    );
    run(
        "set-option",
        &["-p", "-t", &browser, "@browser_loading", "0"],
    );
    caller
        .join()
        .expect("caller thread")
        .expect("the page loaded");
}

#[test]
fn capture_browser_and_send_last_output_parsers_reject_stray_positionals() {
    let args = ["-t", "%2", "-o", "/tmp/frame.png"].map(RawText::from);
    let parsed = parse_capture_browser_args(&args).expect("capture-browser args");
    assert_eq!(parsed.target.as_deref(), Some("%2"));
    assert_eq!(parsed.output.as_deref(), Some("/tmp/frame.png"));
    assert!(parse_capture_browser_args(&["frame.png".into()]).is_err());
    assert!(
        parse_capture_browser_args(&["-o".into()]).is_err(),
        "-o without a value is rejected"
    );

    assert_eq!(
        parse_target_only_args("send-last-output", &["-t%5".into()]).expect("target"),
        Some("%5".to_owned())
    );
    assert!(parse_target_only_args("send-last-output", &["%5".into()]).is_err());
}

#[test]
fn tools_catalog_matches_dispatchable_verbs() {
    const TOOL_VERBS: &[&str] = &[
        "inspect",
        "events",
        "capture-pane",
        "agent-send",
        "agent-respond",
        "flip-agent",
        "agent-account",
        "agent-accounts",
        "send-last-output",
        "show-last-output",
        "send-text",
        "wait-pane",
        "run-pane",
        "capture-browser",
        "load-browser-url",
        "copy-file",
        "read-file",
        "write-file",
        "capture-simulator",
        "tap-simulator",
        "html",
        "debug-marker",
        "tools",
        "wait-for",
        "run-shell",
        "pipe-pane",
        "refresh-client",
        "fleet",
        "list-hosts",
        "update",
        "build-info",
        "update-server",
        "restart-server",
    ];
    const MUX_VERBS: &[&str] = &[
        "set-hook",
        "set-option",
        "show-options",
        "list-sessions",
        "list-windows",
        "list-panes",
        "display-message",
        "split-window",
        "new-window",
        "send-keys",
        "set-browser-url",
    ];
    let catalog = workspace_tools_catalog(&[]).expect("plain catalog").output;
    let documented = catalog
        .split("zz ")
        .skip(1)
        .filter_map(|text| {
            let text = text.strip_prefix("--host ").map_or(text, |host| {
                host.split_once(' ').map_or("", |(_, verb)| verb)
            });
            let verb = text
                .split(|ch: char| !ch.is_ascii_lowercase() && ch != '-')
                .next()?;
            (!verb.is_empty()).then_some(verb)
        })
        .collect::<BTreeSet<_>>();
    for verb in TOOL_VERBS {
        assert!(documented.contains(verb), "catalog is missing `zz {verb}`");
        assert!(
            daemon_command_dispatch(verb).is_some(),
            "catalog documents `zz {verb}`, which has no daemon dispatch"
        );
    }
    for verb in MUX_VERBS {
        assert!(documented.contains(verb), "catalog is missing `zz {verb}`");
    }
    for verb in &documented {
        assert!(
            zz_mux::command_spec(verb).is_some()
                || zz_protocol::catalog_command_spec(verb).is_some(),
            "catalog documents `zz {verb}`, which no command implements"
        );
        if daemon_command_dispatch(verb).is_some() {
            assert!(TOOL_VERBS.contains(verb), "add `zz {verb}` to TOOL_VERBS");
        }
    }
    assert!(catalog.contains(zz_daemon_client::transport::SOCKET_ENVIRONMENT_VARIABLE));
}

#[test]
fn tools_skill_matches_workspace_skill() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agents/skills/zz-workspace/SKILL.md");
    let skill = std::fs::read_to_string(path)
        .expect("read workspace skill; run just tools skill to regenerate it");
    let generated = workspace_tools_catalog(&["--skill".into()])
        .expect("skill catalog")
        .output;
    assert_eq!(
        skill, generated,
        "run just tools skill to regenerate the skill"
    );
    assert_eq!(
        generated.strip_prefix(WORKSPACE_TOOLS_FRONTMATTER),
        Some(
            format!("{WORKSPACE_TOOLS_HEAD}{WORKSPACE_TOOLS_AGENT}{WORKSPACE_TOOLS_ETIQUETTE}")
                .as_str()
        ),
        "skill body must contain the shared head, agent guide, and etiquette"
    );
}

#[test]
fn tools_catalog_accepts_sections_and_skill_flag() {
    let spec = zz_protocol::catalog_command_spec("tools").expect("tools spec");
    assert_eq!(
        spec.options
            .iter()
            .map(|option| option.name)
            .collect::<Vec<_>>(),
        [
            "--skill",
            "--context",
            "--install-hooks",
            "--remove-hooks",
            "--fork-chat"
        ]
    );
    assert!(spec.options.iter().all(|option| option.value.is_none()));
    assert_eq!(spec.positional_maximum(), Some(1));
    assert_eq!(spec.positional_minimum(), 0);
    for args in [
        vec!["--unknown"],
        vec!["--skill=true"],
        vec!["argument"],
        vec!["--skill", "argument"],
        vec!["agent", "terminal"],
        vec!["agent", "--skill"],
        vec!["--skill", "agent"],
        vec!["--skill", "--skill"],
        vec!["--", "--skill"],
    ] {
        let args = args.into_iter().map(RawText::from).collect::<Vec<_>>();
        assert!(workspace_tools_catalog(&args).is_err(), "accepted {args:?}");
    }
}

#[test]
fn tools_local_flags_are_refused_by_the_daemon() {
    for flag in WORKSPACE_TOOLS_LOCAL_FLAGS {
        assert_eq!(
            workspace_tools_catalog(&[RawText::from(flag)]).err(),
            Some(ServerError::CommandParse(format!(
                "tools {flag} runs in the local zz CLI"
            )))
        );
    }
}

#[test]
fn workspace_context_names_the_pane_and_session() {
    let text = workspace_context("%4", "work", false);
    assert!(text.contains("ZZ_SESSION=work\nZZ_PANE=%4\n"));
    assert!(text.contains("'#{pane_id}'"));
    assert!(text.contains("zz list-hosts") && text.contains("zz --host NAME VERB -t %N"));
    assert!(!text.contains("{pane}") && !text.contains("{session}"));
    assert!(!text.contains("```html preview"));
    assert!(text.contains("--kind agent claude|codex|agy|grok|cursor [--model M] [--effort E]"));
    assert!(text.contains("goes to your own subagents, never a zz chat"));
    assert!(text.len() < 2600, "context grew to {} bytes", text.len());
    let agent = workspace_context("%4", "work", true);
    assert!(agent.starts_with(&text) && !agent.contains("```html preview"));
    assert!(!text.contains("```ui") && agent.contains("```ui"));
    assert!(
        agent.ends_with("options is a list of strings or of `{label: \"A\", value: \"a\"}`.\n")
    );
    assert!(agent.contains("starting with `<!doctype html>`, in a plain ```html block"));
    assert!(agent.contains("use your own question tool") && agent.contains("never the user"));
    assert!(
        agent.len() < 6250,
        "agent context grew to {} bytes",
        agent.len()
    );
    assert!(WORKSPACE_STUB_SKILL.contains("zz-stub:"));
    assert!(WORKSPACE_STUB_SKILL.contains("zz tools --skill"));
}

#[test]
fn tools_catalog_sections_include_only_the_selected_concern() {
    let shared = Arc::new(Shared::new(1));
    let sections = [
        ("agent", "## Agents"),
        ("terminal", "## Terminal panes"),
        ("browser", "## Browser panes"),
        ("advanced", "## Advanced state"),
    ];
    for (section, heading) in sections {
        let output = shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("tools", [section]),
            )
            .expect("section catalog")
            .output;
        assert_eq!(
            output
                .split_once("\n\n")
                .map(|(head, rest)| (head, rest.starts_with(heading))),
            Some((
                "Run `zz tools` for targets, remote hosts, exit codes and every section.",
                true
            ))
        );
        assert!(!output.contains("## CLI contract"));
        for (_, other) in sections {
            assert_eq!(output.contains(other), heading == other);
        }
        assert!(!output.contains("## Etiquette"));
        for line in output
            .lines()
            .filter_map(|line| line.strip_prefix("### `zz "))
        {
            let verb = line.split_whitespace().next().expect("verb heading");
            assert!(
                daemon_command_dispatch(verb).is_some() || zz_mux::command_spec(verb).is_some(),
                "section {section} documents an undispatched verb: {verb}"
            );
        }
        if section == "agent" {
            assert!(output.contains("### `zz agent-send"));
            for advanced in [
                "Peers",
                "sticky channel",
                "zz wait-for ",
                "refresh-client -B",
                "Foreign agents",
            ] {
                assert!(
                    !output.contains(advanced),
                    "agent guide contains {advanced}"
                );
            }
            assert!(WORKSPACE_TOOLS_AGENT.lines().count() < 90);
        }
    }
}

#[test]
fn tools_catalog_unknown_section_exits_two() {
    let shared = Arc::new(Shared::new(1));
    let error = shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("tools", ["bogus"]),
        )
        .expect_err("unknown section");
    let DaemonError::Server(error) = error else {
        panic!("expected a server usage error: {error:?}");
    };
    assert_eq!(error.exit_code(), 2);
    let usage = error.to_string();
    for section in ["agent", "terminal", "browser", "advanced"] {
        assert!(usage.contains(section));
    }
}

#[test]
fn host_verbs_point_a_prompt_user_at_the_shell() {
    let shared = Arc::new(Shared::new(1));
    for verb in ["fleet", "list-hosts"] {
        let error = shared
            .execute(
                ClientId(1),
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(verb, Vec::<RawText>::new()),
            )
            .expect_err(verb);
        let DaemonError::Server(ServerError::InvalidCommand(message)) = error else {
            panic!("{verb}: {error:?}");
        };
        assert_eq!(
            message,
            format!(
                "{verb} runs in the zz CLI of the machine it acts for; run it from a shell as `zz {verb}`"
            )
        );
    }
}

#[test]
fn gui_requests_prefer_the_desktop_app_and_name_the_missing_window() {
    let shared = Arc::new(Shared::new(1));
    let terminal_ui = OutboundMailbox::new();
    let desktop = OutboundMailbox::new();
    let (tui, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&terminal_ui),
    );
    let (app, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&desktop));
    shared.inner.lock().client_entry(app).path_picker = true;
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let run = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("fixture command");
    };
    run(&mut context, &["new-session", "-d", "-s", "gui"]);
    run(
        &mut context,
        &["split-window", "--kind", "browser", "about:blank"],
    );
    let browser = context.pane.expect("browser pane").to_string();
    let session = context.session.expect("session");
    let host = shared.inner.lock().engine.format_host_short().to_owned();
    let output = std::env::temp_dir()
        .join("zz-gui-shot.png")
        .to_string_lossy()
        .into_owned();
    let failure = |args: &[&str]| match shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
        .expect_err("no zz window")
    {
        DaemonError::Server(ServerError::InvalidCommand(message)) => message,
        error => panic!("{error:?}"),
    };
    let shot = ["capture-browser", "-t", &browser, "-o", &output];
    let not_rendering = format!(
        "capture-browser: no zz window shows session gui on {host}, so {browser} is not \
             rendering. Open that session in the zz app, then retry."
    );
    assert_eq!(failure(&shot), not_rendering);
    shared.attach(control, session).expect("attach control");
    assert_eq!(failure(&shot), not_rendering);
    assert!(matches!(
        shared.execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-keys", ["-t", &browser, "x"]),
        ),
        Err(DaemonError::Server(ServerError::PaneNotAttached(_)))
    ));
    shared
        .attach(tui, session)
        .expect("attach the terminal client");
    shared.attach(app, session).expect("attach the desktop app");
    take_reliable_messages(&terminal_ui);
    take_reliable_messages(&desktop);
    let sender = Arc::clone(&shared);
    let request = shot.map(str::to_owned);
    let caller = thread::spawn(move || {
        sender.execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(&request[0], request[1..].iter().map(String::as_str)),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let request_id = loop {
        let found =
            take_reliable_messages(&desktop)
                .into_iter()
                .find_map(|message| match message {
                    ProtocolMessage::Event(Event {
                        payload:
                            EventPayload::BrowserCommand {
                                command: BrowserCommand::Screenshot { request_id, .. },
                                ..
                            },
                        ..
                    }) => Some(request_id),
                    _ => None,
                });
        if let Some(request_id) = found {
            break request_id;
        }
        assert!(
            Instant::now() < deadline,
            "the desktop app got no screenshot request"
        );
        thread::sleep(Duration::from_millis(10));
    };
    shared.complete_gui_request(
        app,
        GuiResponse::Success {
            request_id,
            output: String::new(),
        },
    );
    caller
        .join()
        .expect("caller thread")
        .expect("capture-browser");
    assert!(
        !take_reliable_messages(&terminal_ui)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            ))
    );
}

#[cfg(feature = "agent")]
#[test]
fn agent_send_notify_takes_the_caller_from_the_client_not_the_active_pane() {
    let shared = Arc::new(Shared::new(1));
    let runtime = shared.build_agent_runtime(None).expect("agent runtime");
    runtime.set_runner_factory(Box::new(|_| {
        Box::new(|_| Box::pin(std::future::pending::<Result<(), String>>()))
    }));
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let run = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Command,
                context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("fixture command");
    };
    run(&mut context, &["new-session", "-d", "-s", "notify"]);
    let mut agents = Vec::new();
    for _ in 0..2 {
        run(&mut context, &["split-window"]);
        let pane = context.pane.expect("agent terminal");
        assert!(shared.open_test_agent(pane));
        agents.push(pane);
    }
    let (active, target) = (agents[0], agents[1]);
    run(&mut context, &["select-pane", "-t", &active.to_string()]);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, active)
        .expect("last active pane");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("agent-send", ["--notify", "-t", &target.to_string(), "hi"]),
        )
        .expect_err("a client outside any pane has no caller");
    assert!(
        matches!(&error, DaemonError::Server(ServerError::InvalidCommand(message)) if message == "agent-send --notify runs from inside another terminal pane"),
        "{error:?}"
    );
    shared.shutdown_agents();
}

#[test]
fn gui_requests_fail_when_the_window_disconnects() {
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
            &CommandInvocation::new("new-session", ["-s", "disconnect"]),
        )
        .expect("session");
    let session = context.session.expect("session");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("browser");
    let browser = context.pane.expect("browser");
    shared.attach(client, session).expect("attach");

    let sender = Arc::clone(&shared);
    let target = browser.to_string();
    let output = std::env::temp_dir()
        .join("zz-gui-disconnect.png")
        .to_string_lossy()
        .into_owned();
    let caller = thread::spawn(move || {
        let mut context = ExecutionContext::default();
        sender.execute(
            ClientId(98),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-browser", ["-t", &target, "-o", &output]),
        )
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    while shared
        .inner
        .lock()
        .transfers
        .pending_gui_requests
        .is_empty()
    {
        assert!(Instant::now() < deadline, "request was never registered");
        thread::sleep(Duration::from_millis(10));
    }
    shared.fail_gui_requests_for(client);
    let error = caller
        .join()
        .expect("caller thread")
        .expect_err("disconnected window");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message)) if message.contains("disconnected")
    ));
}

#[test]
fn send_last_output_requires_a_terminal_and_an_agent_pane() {
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
            &CommandInvocation::new("new-session", ["-s", "explain", "exec /bin/cat"]),
        )
        .expect("session");
    let session = context.session.expect("session");
    let terminal = context.pane.expect("terminal");
    shared.attach(client, session).expect("attach");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "picker", "-v"]),
        )
        .expect("picker");
    let picker = context.pane.expect("picker");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("send-last-output", ["-t", &picker.to_string()]),
        )
        .expect_err("picker is not a terminal");
    assert!(
        matches!(error, DaemonError::Server(ServerError::InvalidTarget(_))),
        "{error:?}"
    );

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("send-last-output", ["-t", &terminal.to_string()]),
        )
        .expect_err("no marks");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("OSC 133")
    ));
}

#[test]
fn show_last_output_requires_a_terminal_with_marks() {
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
            &CommandInvocation::new("new-session", ["-s", "readback", "exec /bin/cat"]),
        )
        .expect("session");
    let terminal = context.pane.expect("terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "picker", "-v"]),
        )
        .expect("picker");
    let picker = context.pane.expect("picker");

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("show-last-output", ["-t", &picker.to_string()]),
        )
        .expect_err("picker is not a terminal");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidTarget(_))
    ));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("show-last-output", ["-t", &terminal.to_string()]),
        )
        .expect_err("no marks");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("show-last-output needs") && message.contains("OSC 133")
    ));
}

#[test]
fn show_last_output_includes_only_known_command_status() {
    let shared = Arc::new(Shared::new(1));
    let terminal = Arc::new(TerminalSession::spawn_empty_with_appearance(
        64,
        Arc::new(TerminalAppearance::default()),
    ));
    let (session, window, pane) = {
        let mut inner = shared.inner.lock();
        let ids = inner
            .engine
            .state
            .create_session("exit-status")
            .expect("session");
        inner.terminals_mut().insert(ids.2, Arc::clone(&terminal));
        ids
    };
    let context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    assert!(terminal.feed(Arc::from(
        b"\x1b]133;A\x07$ \x1b]133;B\x07false\x1b]133;C\x07\r\nfailed\r\n".as_slice()
    )));
    assert_eq!(
        shared
            .show_last_output(ClientId(u64::MAX), &context, &[])
            .expect("running command")
            .output,
        format!("{pane} $ false\nexit: running\n```\nfailed\n```")
    );
    assert!(terminal.feed(Arc::from(b"\x1b]133;D;1\x07\x1b]133;A\x07$ ".as_slice())));
    assert_eq!(
        shared
            .show_last_output(ClientId(u64::MAX), &context, &[])
            .expect("failed command")
            .output,
        format!("{pane} $ false\nexit: 1\n```\nfailed\n```")
    );
    assert!(terminal.feed(Arc::from(b"\x1b]133;D\x07".as_slice())));
    assert_eq!(
        shared
            .show_last_output(ClientId(u64::MAX), &context, &[])
            .expect("cleared exit status")
            .output,
        format!("{pane} $ false\n```\nfailed\n```")
    );
    assert!(terminal.feed(Arc::from(
        b"\x1b]133;D;1\x07\x1b]133;A\x07$ \x1b]133;B\x07sleep 9\x1b]133;C\x07\r\n".as_slice()
    )));
    assert_eq!(
        shared
            .show_last_output(ClientId(u64::MAX), &context, &[])
            .expect("running command after a failure")
            .output,
        format!("{pane} $ sleep 9\nexit: running\n```\n\n```")
    );
}

#[test]
fn show_last_output_shows_the_command_run_pane_typed() {
    let shared = Arc::new(Shared::new(1));
    let terminal = Arc::new(TerminalSession::spawn_empty_with_appearance(
        64,
        Arc::new(TerminalAppearance::default()),
    ));
    let (session, window, pane) = {
        let mut inner = shared.inner.lock();
        let ids = inner
            .engine
            .state
            .create_session("run-pane-block")
            .expect("session");
        inner.terminals_mut().insert(ids.2, Arc::clone(&terminal));
        ids
    };
    let context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let marker = "ZZRUN-0123456789abcdef";
    let typed = run_pane_line(marker, "command eval", "echo 'hi'; false");
    assert!(terminal.feed(Arc::from(
        format!(
            "\x1b]133;A\x07$ \x1b]133;B\x07{typed}\x1b]133;C\x07\r\n\r\n{marker}-BEGIN\r\nhi\r\npartial"
        )
        .into_bytes()
    )));
    let show = || {
        shared
            .show_last_output(ClientId(u64::MAX), &context, &[])
            .expect("run-pane block")
            .output
    };
    assert_eq!(
        show(),
        format!("{pane} $ echo 'hi'; false\nexit: running\n```\nhi\npartial\n```")
    );
    assert!(terminal.feed(Arc::from(
        format!("\r\n{marker}-RC=1=END\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ ").into_bytes()
    )));
    assert_eq!(
        show(),
        format!("{pane} $ echo 'hi'; false\nexit: 1\n```\nhi\npartial\n```")
    );
}

#[test]
fn run_pane_blocks_drop_their_markers() {
    let marker = "ZZRUN-0123456789abcdef";
    let capture = |output: &str| LastCommandCapture {
        command: format!("host$ {}", run_pane_line(marker, "eval", "true")),
        output: output.to_owned(),
        truncated_rows: 0,
        running: false,
    };
    for (output, expected, code) in [
        (
            format!("{marker}-BEGIN\nhi\n\n{marker}-RC=0=END"),
            "hi",
            Some(0),
        ),
        (
            format!("{marker}-BEGIN\nok\n{marker}-RC=3=END"),
            "ok",
            Some(3),
        ),
        (
            format!("{marker}-BEGIN\na\n\n\n{marker}-RC=0=END"),
            "a\n",
            Some(0),
        ),
        (format!("{marker}-BEGIN\n\n{marker}-RC=0=END"), "", Some(0)),
        (
            format!("[1]+  Done  sleep 2\n\n{marker}-BEGIN\nafter-job\n\n{marker}-RC=0=END"),
            "[1]+  Done  sleep 2\nafter-job",
            Some(0),
        ),
        (format!("tail\n\n{marker}-RC=0=END"), "tail", Some(0)),
        (format!("{marker}-BEGIN\nstill"), "still", None),
    ] {
        let (unwrapped, status) = unwrap_run_pane(&capture(&output)).expect("wrapper");
        assert_eq!(unwrapped.command, "true");
        assert_eq!(
            (unwrapped.output.as_str(), status),
            (expected, code),
            "{output:?}"
        );
    }
    for command in [
        "printf '\\n%s\\n' 'OTHER-BEGIN'; eval 'x'; printf '\\nOTHER-RC=%d=END\\n' $?",
        "printf hi",
    ] {
        assert!(
            unwrap_run_pane(&LastCommandCapture {
                command: command.to_owned(),
                ..LastCommandCapture::default()
            })
            .is_none()
        );
    }
}

#[test]
fn run_pane_lines_type_only_plain_words_of_the_command() {
    let marker = "ZZRUN-0123456789abcdef";
    assert_eq!(
        run_pane_line(marker, "eval", "cargo test -p zz-daemon run_pane"),
        format!(
            " printf '\\n%s\\n' '{marker}-BEGIN'; eval \"$(printf '%b' 'cargo test -p \
                 zz-daemon run_pane')\"; printf '\\n{marker}-RC=%d=END\\n' $?"
        )
    );
    for command in [
        "grep -c 'x' /dev/null",
        "echo \"it's\" 'a b' hi! \\back $((1)) `x` ${HOME} é # c\nnext\tline",
    ] {
        let line = run_pane_line(marker, "command eval", command);
        let escaped = line
            .split_once("'%b' '")
            .and_then(|(_, rest)| rest.split_once('\''))
            .expect("escaped command")
            .0;
        assert!(
            escaped
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || !character.is_ascii()
                    || " _./,:=+@%~-\\".contains(character)),
            "{escaped}"
        );
        let (unwrapped, _) = unwrap_run_pane(&LastCommandCapture {
            command: line,
            ..LastCommandCapture::default()
        })
        .expect("wrapper");
        assert_eq!(unwrapped.command, command);
    }
}

#[test]
fn capture_browser_rejects_non_browser_panes_and_relative_paths() {
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
            &CommandInvocation::new("new-session", ["-s", "shots"]),
        )
        .expect("session");
    let session = context.session.expect("session");
    let terminal = context.pane.expect("terminal");
    shared.attach(client, session).expect("attach");
    let output = std::env::temp_dir()
        .join("zz-shot.png")
        .to_string_lossy()
        .into_owned();

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("capture-browser", [] as [&str; 0]),
        )
        .expect_err("missing output path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::NativeCommandParse(message))
            if message == "capture-browser needs an output path (-o)"
    ));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new(
                "capture-browser",
                ["-t", &terminal.to_string(), "-o", &output],
            ),
        )
        .expect_err("terminal target");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidTarget(_))
    ));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new(
                "capture-browser",
                ["-t", &terminal.to_string(), "-o", "relative.png"],
            ),
        )
        .expect_err("relative path");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("absolute")
    ));
}

#[test]
fn capture_pane_parser_rejects_unimplemented_flags() {
    for flag in ["-P", "-R"] {
        let error = parse_capture_pane_args(&[flag.into()]).expect_err("unsupported flag");
        assert!(matches!(error, ServerError::CommandParse(_)), "{flag}");
    }
    let parsed = parse_capture_pane_args(&["-C".into(), "-L".into()]).expect("transforms");
    assert!(parsed.options.escape_nonprintable);
    assert!(parsed.options.number_lines);
}

#[test]
fn capture_boundaries_use_the_selected_format_client() {
    let mut engine = MuxEngine::default();
    let (session, _, pane) = engine.state.create_session("capture-format").unwrap();
    let (other, _, _) = engine.state.create_session("capture-other").unwrap();
    let target = ExecutionContext::for_pane(&engine.state, pane).unwrap();

    for (format_client, expected) in [
        (
            FormatClient::Attached(session),
            CaptureBoundary::Relative(1),
        ),
        (FormatClient::Attached(other), CaptureBoundary::Relative(0)),
        (FormatClient::NoClient, CaptureBoundary::VisibleEnd),
    ] {
        assert_eq!(
            {
                let mut hooks = InertFormatHooks::default();
                resolve_capture_boundary(
                    &engine,
                    Some("#{session_active}"),
                    false,
                    &target,
                    Some(session),
                    format_client,
                    &mut hooks,
                )
            },
            expected
        );
    }
}
