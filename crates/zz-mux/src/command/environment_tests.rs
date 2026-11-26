use super::test_support::command;
use super::*;

#[test]
fn environment_stores_and_prints_non_utf8_names_and_values_as_bytes() {
    let mut engine = MuxEngine::default();
    let (session, _, _) = engine.state.create_session("bytes").unwrap();
    let mut context = ExecutionContext::new(Some(session), None, None);

    let value = RawText::from_bytes(b"a\xffb".as_slice());
    let name = RawText::from_bytes(b"N\xff".as_slice());
    let session_value = RawText::from_bytes(b"x\xffy".as_slice());

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                [RawText::from("-g"), "ZZBYTES".into(), value.clone()],
            ),
        )
        .expect("global set-environment");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-g", "ZZBYTES"]),
            )
            .expect("global show-environment")
            .output
            .as_bytes(),
        b"ZZBYTES=a\xffb"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-gs", "ZZBYTES"]),
            )
            .expect("shell show-environment")
            .output
            .as_bytes(),
        b"ZZBYTES=\"a\xffb\"; export ZZBYTES;"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                [RawText::from("-g"), name.clone(), "plain".into()],
            ),
        )
        .expect("non-utf8 name set-environment");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("show-environment", [RawText::from("-g"), name.clone()],),
            )
            .expect("non-utf8 name show-environment")
            .output
            .as_bytes(),
        b"N\xff=plain"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                [RawText::from("ZZSESS"), session_value.clone()],
            ),
        )
        .expect("session set-environment");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-environment", &["ZZSESS"]))
            .expect("session show-environment")
            .output
            .as_bytes(),
        b"ZZSESS=x\xffy"
    );

    let spawn = engine.environment_for_session(session).expect("spawn env");
    assert_eq!(
        spawn
            .iter()
            .find(|(spawn_name, _)| spawn_name == "ZZBYTES")
            .and_then(|(_, spawn_value)| spawn_value.as_ref())
            .map(RawText::as_bytes),
        Some(b"a\xffb".as_slice())
    );
    assert_eq!(
        spawn
            .iter()
            .find(|(spawn_name, _)| *spawn_name == name)
            .and_then(|(_, spawn_value)| spawn_value.as_ref())
            .map(RawText::as_bytes),
        Some(b"plain".as_slice())
    );
}

#[test]
fn new_session_environment_options_apply_only_on_creation() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([("DISPLAY", "daemon")]);
    let mut context = ExecutionContext::default();
    context.set_client_environment(Some(Arc::new(ClientEnvironmentBlob::from_map(
        BTreeMap::from([("DISPLAY".into(), "client".into())]),
    ))));

    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &[
                    "-d",
                    "-s",
                    "seeded",
                    "-e",
                    "DISPLAY=first",
                    "-e",
                    "MALFORMED",
                    "-e",
                    "=ignored",
                    "-e",
                    "DISPLAY=last",
                ],
            ),
        )
        .expect("seeded session");
    let seeded = context.session.expect("seeded session id");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-t", "seeded", "DISPLAY"]),
            )
            .unwrap()
            .output,
        "DISPLAY=last"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-t", "seeded", ""]),
            )
            .unwrap()
            .output,
        "=ignored"
    );
    assert_eq!(
        engine
            .environment_for_session(seeded)
            .unwrap()
            .into_iter()
            .collect::<BTreeMap<_, _>>()["DISPLAY"]
            .as_deref(),
        Some("last")
    );

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-E", "-s", "unseeded"]),
        )
        .expect("unseeded session");
    assert!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-t", "unseeded"]),
            )
            .unwrap()
            .output
            .is_empty()
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-environment", &["-t", "unseeded", "DISPLAY"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "unknown variable: DISPLAY"
    ));

    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-E", "-s", "explicit", "-e", "DISPLAY=explicit"],
            ),
        )
        .expect("explicit unseeded session");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-t", "explicit", "DISPLAY"]),
            )
            .unwrap()
            .output,
        "DISPLAY=explicit"
    );

    engine
        .execute(
            &mut context,
            &command("set-environment", &["-t", "seeded", "STAYS", "original"]),
        )
        .expect("existing session value");
    let sessions = engine.state.sessions.len();
    let attached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &[
                    "-A",
                    "-d",
                    "-E",
                    "-s",
                    "seeded",
                    "-e",
                    "STAYS=replaced",
                    "-e",
                    "NEW=value",
                ],
            ),
        )
        .expect("attach existing session");
    assert!(matches!(
        attached.effects.as_slice(),
        [MuxEffect::Attach {
            update_environment: false,
            ..
        }]
    ));
    assert_eq!(engine.state.sessions.len(), sessions);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-t", "seeded", "STAYS"]),
            )
            .unwrap()
            .output,
        "STAYS=original"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-environment", &["-t", "seeded", "NEW"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "unknown variable: NEW"
    ));
}

#[test]
fn new_session_seeds_patterns_from_the_client_snapshot() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([("EXACT", "daemon"), ("APP_DAEMON", "daemon")]);
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "update-environment", "EXACT APP_*"]),
        )
        .expect("global update patterns");
    let snapshot = Arc::new(ClientEnvironmentBlob::from_map(BTreeMap::from([
        ("APP_ONE".into(), "one".into()),
        ("APP_TWO".into(), RawText::default()),
        ("UNSELECTED".into(), "client".into()),
    ])));
    context.set_client_environment(Some(Arc::clone(&snapshot)));
    let cloned = context.clone();
    assert!(Arc::ptr_eq(
        cloned.client_environment.as_ref().unwrap(),
        &snapshot
    ));
    let debug = format!("{context:?}");
    assert!(debug.contains("client_environment_entries: Some(3)"));
    assert!(!debug.contains("APP_ONE"));
    assert!(!debug.contains("\"one\""));

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "client"]),
        )
        .expect("client-backed session");
    let session = context.session.unwrap();
    let environment = engine.session_environments[&session].inner.lock();
    assert_eq!(environment["APP_ONE"].value.as_deref(), Some("one"));
    assert_eq!(environment["APP_TWO"].value.as_deref(), Some(""));
    assert_eq!(environment["EXACT"].value, None);
    assert!(!environment.contains_key("APP_DAEMON"));
    assert!(!environment.contains_key("UNSELECTED"));
    drop(environment);

    let mut no_client = ExecutionContext::default();
    no_client.set_no_client();
    assert!(no_client.client_environment().is_none());
    engine
        .execute(
            &mut no_client,
            &command("new-session", &["-d", "-s", "clientless"]),
        )
        .expect("clientless session");
    assert!(
        engine.session_environments[&no_client.session.unwrap()]
            .inner
            .lock()
            .is_empty(),
        "no client source differs from an empty client environment"
    );
}

#[test]
fn client_environment_refresh_uses_session_patterns_and_marker_rules() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "update-environment", "GLOBAL_ONLY"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-E", "-s", "work"]),
        )
        .unwrap();
    let session = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-t",
                    "work",
                    "update-environment",
                    "EXACT WILD_* MISSING LITERAL* HIDDEN EMPTY_*",
                ],
            ),
        )
        .unwrap();
    for args in [
        &["-t", "work", "UNSELECTED", "keep"][..],
        &["-t", "work", "WILD_OLD", "old"][..],
        &["-h", "-t", "work", "HIDDEN", "old"][..],
        &["-h", "-t", "work", "MISSING", "old"][..],
    ] {
        engine
            .execute(&mut context, &command("set-environment", args))
            .unwrap();
    }
    let client_environment = BTreeMap::from([
        ("EMPTY_ONE".into(), RawText::default()),
        ("EXACT".into(), "new".into()),
        ("HIDDEN".into(), "visible".into()),
        ("WILD_ONE".into(), "one".into()),
        ("WILD_TWO".into(), "two".into()),
    ]);

    engine
        .update_session_environment_from_client(session, &client_environment)
        .unwrap();
    let environment = engine.session_environments[&session].inner.lock();
    assert_eq!(environment["EXACT"].value.as_deref(), Some("new"));
    assert_eq!(environment["WILD_ONE"].value.as_deref(), Some("one"));
    assert_eq!(environment["WILD_TWO"].value.as_deref(), Some("two"));
    assert_eq!(environment["WILD_OLD"].value.as_deref(), Some("old"));
    assert_eq!(environment["EMPTY_ONE"].value.as_deref(), Some(""));
    assert_eq!(environment["UNSELECTED"].value.as_deref(), Some("keep"));
    assert_eq!(environment["MISSING"].value, None);
    assert!(environment["MISSING"].hidden);
    assert_eq!(environment["LITERAL*"].value, None);
    assert!(!environment["LITERAL*"].hidden);
    assert_eq!(environment["HIDDEN"].value.as_deref(), Some("visible"));
    assert!(!environment["HIDDEN"].hidden);
    assert!(!environment.contains_key("GLOBAL_ONLY"));
    drop(environment);

    engine
        .update_session_environment_from_client(session, &BTreeMap::new())
        .unwrap();
    let environment = engine.session_environments[&session].inner.lock();
    assert_eq!(environment["EXACT"].value, None);
    assert_eq!(environment["WILD_*"].value, None);
    assert_eq!(environment["WILD_ONE"].value.as_deref(), Some("one"));
    assert_eq!(environment["WILD_TWO"].value.as_deref(), Some("two"));
    assert_eq!(environment["EMPTY_ONE"].value.as_deref(), Some(""));
    assert_eq!(environment["HIDDEN"].value, None);
    assert!(!environment["HIDDEN"].hidden);
    assert!(environment["MISSING"].hidden);
    drop(environment);

    assert!(matches!(
        engine.update_session_environment_from_client(SessionId(u64::MAX), &BTreeMap::new()),
        Err(ServerError::MissingTarget(_))
    ));
}

#[test]
fn new_and_split_window_carry_pane_local_environment() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");

    for name in ["new-window", "split-window"] {
        let execution = engine
            .execute(
                &mut context,
                &command(
                    name,
                    &[
                        "-d",
                        "-e",
                        "PANE_ONLY=first",
                        "-e",
                        "MALFORMED",
                        "-e",
                        "=ignored",
                        "-e",
                        "PANE_ONLY=last",
                    ],
                ),
            )
            .expect("pane-local environment");
        assert!(matches!(
            execution.effects.first(),
            Some(MuxEffect::PaneCreated {
                environment,
                empty: false,
                ..
            }) if environment == &vec![
                ("PANE_ONLY".to_owned(), "first".to_owned()),
                ("PANE_ONLY".to_owned(), "last".to_owned()),
            ]
        ));
    }

    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-environment", &["-t", "work", "PANE_ONLY"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "unknown variable: PANE_ONLY"
    ));
}

#[test]
fn environments_read_back_and_merge_with_pin_shell_syntax() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .unwrap();
    let first = context.session.unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "second"]))
        .unwrap();
    let second = context.session.unwrap();

    for args in [
        &["-g", "GLOBAL", "global"] as &[&str],
        &["-g", "SHARED", "global"],
        &["-gF", "GLOBAL_FORMAT", "#{session_name}"],
        &["-t", "first", "SHARED", "session"],
        &["-t", "first", "QUOTED", "a$b`c\"d\\e"],
        &["-ht", "first", "SECRET", "hidden"],
        &["-rt", "first", "REMOVED"],
        &["-Ft", "first", "FORMATTED", "#{session_name}"],
    ] {
        engine
            .execute(&mut context, &command("set-environment", args))
            .unwrap();
    }

    for (args, expected) in [
        (&["-g", "GLOBAL"] as &[&str], "GLOBAL=global"),
        (&["-g", "GLOBAL_FORMAT"], "GLOBAL_FORMAT=second"),
        (&["-t", "first", "SHARED"], "SHARED=session"),
        (
            &["-st", "first", "QUOTED"],
            r#"QUOTED="a\$b\`c\"d\\e"; export QUOTED;"#,
        ),
        (&["-t", "first", "SECRET"], ""),
        (&["-ht", "first", "SECRET"], "SECRET=hidden"),
        (&["-t", "first", "REMOVED"], "-REMOVED"),
        (&["-st", "first", "REMOVED"], "unset REMOVED;"),
        (&["-t", "first", "FORMATTED"], "FORMATTED=first"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-environment", args))
                .unwrap()
                .output,
            expected
        );
    }

    let first_environment = engine
        .environment_for_session(first)
        .unwrap()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    assert_eq!(first_environment["GLOBAL"].as_deref(), Some("global"));
    assert_eq!(first_environment["SHARED"].as_deref(), Some("session"));
    assert_eq!(first_environment["SECRET"], None);
    assert_eq!(first_environment["REMOVED"], None);
    let second_environment = engine
        .environment_for_session(second)
        .unwrap()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    assert_eq!(second_environment["SHARED"].as_deref(), Some("global"));

    engine
        .execute(
            &mut context,
            &command("set-environment", &["-ut", "first", "SHARED"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .environment_for_session(first)
            .unwrap()
            .into_iter()
            .collect::<BTreeMap<_, _>>()["SHARED"]
            .as_deref(),
        Some("global")
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-environment", &["-t", "first", "MISSING"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "unknown variable: MISSING"
    ));
}

#[test]
fn retained_job_environment_tracks_the_original_session_through_destruction() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "keep"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target"]),
        )
        .unwrap();
    let original = context.session.unwrap();

    for args in [
        &["-g", "GLOBAL", "scheduled-global"] as &[&str],
        &["-t", "=target", "SESSION", "scheduled-session"],
    ] {
        engine
            .execute(&mut context, &command("set-environment", args))
            .unwrap();
    }
    let retained = engine
        .retain_session_job_environment(original)
        .expect("live session environment");

    for args in [
        &["-g", "GLOBAL", "launch-global"] as &[&str],
        &["-t", "=target", "SESSION", "before-kill"],
        &["-ht", "=target", "HIDDEN", "secret"],
        &["-rt", "=target", "UNSET"],
    ] {
        engine
            .execute(&mut context, &command("set-environment", args))
            .unwrap();
    }
    engine
        .execute(&mut context, &command("kill-session", &["-t", "=target"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target"]),
        )
        .unwrap();
    let replacement = context.session.unwrap();
    assert_ne!(replacement, original);
    engine
        .execute(
            &mut context,
            &command(
                "set-environment",
                &["-t", "=target", "SESSION", "replacement"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-environment", &["-g", "GLOBAL", "latest-global"]),
        )
        .unwrap();

    let environment = engine
        .job_environment_with_retained_session(Some(&retained))
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    assert_eq!(environment["GLOBAL"].as_deref(), Some("latest-global"));
    assert_eq!(environment["SESSION"].as_deref(), Some("before-kill"));
    assert_eq!(environment["HIDDEN"], None);
    assert_eq!(environment["UNSET"], None);
    assert!(engine.retain_session_job_environment(original).is_none());
    assert_eq!(
        engine
            .job_environment(Some(replacement))
            .into_iter()
            .collect::<BTreeMap<_, _>>()["SESSION"]
            .as_deref(),
        Some("replacement")
    );
}

#[test]
fn retained_empty_job_environment_observes_later_session_writes() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("empty").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    assert!(!engine.session_environments.contains_key(&session));

    let retained = engine
        .retain_session_job_environment(session)
        .expect("live session environment");
    assert!(engine.session_environments.contains_key(&session));
    engine
        .execute(
            &mut context,
            &command("set-environment", &["LATER", "visible"]),
        )
        .unwrap();

    assert_eq!(
        engine
            .job_environment_with_retained_session(Some(&retained))
            .into_iter()
            .collect::<BTreeMap<_, _>>()["LATER"]
            .as_deref(),
        Some("visible")
    );
}

#[test]
fn global_environment_seed_populates_session_update_markers() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([
        ("DISPLAY", ":7"),
        ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
        ("PHASE4D_EXTRA", "global"),
    ]);
    let mut context = ExecutionContext::default();
    context.set_client_environment(Some(Arc::new(ClientEnvironmentBlob::from_map(
        BTreeMap::from([
            ("DISPLAY".into(), ":7".into()),
            ("SSH_AUTH_SOCK".into(), "/tmp/agent.sock".into()),
        ]),
    ))));
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session");

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-environment", &["-g", "PHASE4D_EXTRA"]),
            )
            .unwrap()
            .output,
        "PHASE4D_EXTRA=global"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-environment", &["KRB5CCNAME"]),)
            .unwrap()
            .output,
        "-KRB5CCNAME"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-environment", &[]))
            .unwrap()
            .output
            .lines()
            .count(),
        13
    );
    let environment = engine
        .environment_for_session(session)
        .unwrap()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    assert_eq!(environment["DISPLAY"].as_deref(), Some(":7"));
    assert_eq!(
        environment["SSH_AUTH_SOCK"].as_deref(),
        Some("/tmp/agent.sock")
    );
    assert_eq!(environment["KRB5CCNAME"], None);
    assert_eq!(environment["PHASE4D_EXTRA"].as_deref(), Some("global"));
}

#[test]
fn update_environment_writes_drive_seeding_and_readback() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([("DISPLAY", ":7"), ("PHASE_C7", "seeded")]);
    let mut context = ExecutionContext::default();
    context.set_client_environment(Some(Arc::new(ClientEnvironmentBlob::from_map(
        BTreeMap::from([("PHASE_C7".into(), "seeded".into())]),
    ))));
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-g", "update-environment", "PHASE_C7 ABSENT_C7"],
            ),
        )
        .expect("array write");
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-environment", &[]))
            .unwrap()
            .output,
        "-ABSENT_C7\nPHASE_C7=seeded"
    );
    assert_eq!(
        engine
            .global_tmux_option_value("update-environment")
            .as_deref(),
        Some("PHASE_C7 ABSENT_C7")
    );
}

#[test]
fn update_environment_names_take_only_their_own_entry_and_globs_every_match() {
    let client = BTreeMap::from([
        (RawText::from("DISPLAY"), RawText::from(":0")),
        (RawText::from("DISPLAY_EXTRA"), RawText::from("extra")),
        (RawText::from("SSH_AGENT_PID"), RawText::from("7")),
        (RawText::from("SSH_AUTH_SOCK"), RawText::from("/sock")),
        (RawText::from("STAR*"), RawText::from("star")),
        (
            RawText::from_bytes(b"BYTE\xff".to_vec()),
            RawText::from("raw"),
        ),
    ]);
    let mut environment = Environment::default();
    apply_client_environment_update(
        &mut environment,
        &[
            "DISPLAY".to_owned(),
            "SSH_*".to_owned(),
            "STAR\\*".to_owned(),
            "BYTE\u{fffd}".to_owned(),
            "ABSENT".to_owned(),
        ],
        &client,
    );
    let values = environment
        .iter()
        .map(|(name, entry)| {
            (
                name.as_bytes().to_vec(),
                entry.value.as_ref().map(ToString::to_string),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            (b"ABSENT".to_vec(), None),
            (b"BYTE\xff".to_vec(), Some("raw".to_owned())),
            (b"DISPLAY".to_vec(), Some(":0".to_owned())),
            (b"SSH_AGENT_PID".to_vec(), Some("7".to_owned())),
            (b"SSH_AUTH_SOCK".to_vec(), Some("/sock".to_owned())),
            (b"STAR*".to_vec(), Some("star".to_owned())),
        ]
    );
}
