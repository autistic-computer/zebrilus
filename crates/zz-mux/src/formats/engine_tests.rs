use super::test_support::Stub;
use super::*;
use crate::ExecutionContext;
use zz_protocol::{Axis, CommandInvocation};

#[test]
fn snapshot_contexts_share_universe_and_match_independent_formats() {
    let mut engine = MuxEngine::default();
    let (work, first_window, first_pane) = engine.state.create_session("work").unwrap();
    let (other, _, _) = engine.state.create_session("other").unwrap();
    engine
        .state
        .create_window(work, Some("logs".to_owned()), PaneKind::Terminal)
        .unwrap();
    let mut execution = crate::ExecutionContext::default();
    for (name, args) in [
        ("split-window", vec!["-d", "-t", "work:0"]),
        ("set-option", vec!["-g", "@batch", "value"]),
        ("set-environment", vec!["-g", "BATCH", "value"]),
    ] {
        engine
            .execute(
                &mut execution,
                &zz_protocol::CommandInvocation::new(name, args),
            )
            .unwrap();
    }
    let mut targets = vec![
        (None, None, None),
        (Some(work), None, None),
        (None, Some(first_window), None),
        (None, None, Some(first_pane)),
        (
            Some(SessionId(u64::MAX)),
            Some(WindowId(u64::MAX)),
            Some(PaneId(u64::MAX)),
        ),
    ];
    targets.extend(engine.state.windows.values().flat_map(|window| {
        window
            .panes
            .keys()
            .map(move |pane| (Some(window.session), Some(window.id), Some(*pane)))
    }));
    for client in [
        FormatClient::NoClient,
        FormatClient::Unattached,
        FormatClient::Attached(work),
        FormatClient::Attached(other),
    ] {
        let contexts = engine.format_context_snapshot(client);
        assert!(contexts.universe.parts.sessions.get().is_none());
        assert!(contexts.universe.parts.windows.lock().is_empty());
        let first = contexts.status_context(Some(work), Some(first_window), Some(first_pane));
        assert_eq!(
            first.session_active,
            match client {
                FormatClient::NoClient => None,
                FormatClient::Unattached => Some(false),
                FormatClient::Attached(session) => Some(session == work),
            }
        );
        for (session, window, pane) in &targets {
            let expected = engine.format_status_context_with_format_client(
                *session, *window, *pane, *session, client,
            );
            let actual = contexts.status_context(*session, *window, *pane);
            assert_eq!(*actual, *expected);
            assert!(Arc::ptr_eq(
                &first.format_universe.parts,
                &actual.format_universe.parts
            ));
            for format in [
                "#S:#I:#W:#{pane_id}:#{session_active}:#{window_active}:#{pane_active}",
                "#{S:#{session_name}=#{session_active}:[#{W:#{window_name}:[#{P:#{pane_id}|}]}]}",
                "#{Og:#{option_name}=#{option_value}|}",
                "#{Vg:#{environ_name}=#{environ_value}|}",
            ] {
                assert_eq!(
                    expand_status(format, &actual, &mut Stub),
                    expand_status(format, &expected, &mut Stub),
                    "{client:?}: {format}",
                );
            }
        }
    }
}

#[test]
fn new_context_snapshot_observes_engine_mutations() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    engine.set_format_now(1_700_000_000);
    let formats = [
        "#{W:#{window_name}|}",
        "#{Og:#{option_name}=#{option_value}|}",
        "#{Vg:#{environ_name}=#{environ_value}|}",
    ];
    let first = engine
        .format_context_snapshot(FormatClient::Attached(session))
        .status_context(Some(session), Some(window), Some(pane))
        .detach(engine.format_needs(formats));
    let mut execution = crate::ExecutionContext::default();
    for (name, args) in [
        ("new-window", vec!["-d", "-t", "work:", "-n", "logs"]),
        ("select-window", vec!["-t", "work:1"]),
        ("set-option", vec!["-g", "@batch", "changed"]),
        ("set-environment", vec!["-g", "BATCH", "changed"]),
    ] {
        engine
            .execute(
                &mut execution,
                &zz_protocol::CommandInvocation::new(name, args),
            )
            .unwrap();
    }
    engine.set_format_now(1_700_000_001);
    let second = engine
        .format_context_snapshot(FormatClient::Attached(session))
        .status_context(Some(session), Some(window), Some(pane));
    assert!(!Arc::ptr_eq(
        &first.format_universe.parts,
        &second.format_universe.parts
    ));
    assert_eq!(first.window_active, Some(true));
    assert_eq!(second.window_active, Some(false));
    assert_eq!(first.format_now, Some(1_700_000_000));
    assert_eq!(second.format_now, Some(1_700_000_001));
    for format in formats {
        assert_ne!(
            expand_status(format, &first, &mut Stub),
            expand_status(format, &second, &mut Stub),
            "{format}",
        );
    }
}

#[test]
fn scoped_values_match_list_format_expansion() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    for (scope, format_type) in [
        ("session", FormatType::Session),
        ("window", FormatType::Window),
        ("pane", FormatType::Pane),
    ] {
        let format_context = FormatContext {
            session: Some(session),
            window: (format_type != FormatType::Session).then_some(window),
            pane: (format_type == FormatType::Pane).then_some(pane),
            active_session: Some(session),
            format_client: FormatClient::NoClient,
            format_type,
        };
        let context = format_context.resolve(&engine).values;
        let values = context.scoped_format_values(scope, &mut Stub);
        for (name, value) in values.as_object().unwrap() {
            assert_eq!(
                value.as_str().unwrap(),
                expand_format_with_hooks(
                    &format!("#{{{name}}}"),
                    &engine,
                    format_context,
                    &mut Stub,
                )
                .to_string(),
                "{scope}: {name}",
            );
        }
    }
}

fn expand_session_path(engine: &MuxEngine, session: Option<SessionId>) -> String {
    expand_format(
        "#{session_path}",
        engine,
        FormatContext {
            session,
            window: None,
            pane: None,
            active_session: None,
            format_client: FormatClient::NoClient,
            format_type: FormatType::Session,
        },
    )
}

fn command(name: &str, args: &[&str]) -> CommandInvocation {
    CommandInvocation::new(name, args.iter().copied())
}

#[test]
fn session_active_uses_a_distinct_tri_state_format_client() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let (second, second_window, second_pane) = engine.state.create_session("second").unwrap();
    let expand_for = |session, window, pane, format_client| {
        expand_format(
            "#{session_active}",
            &engine,
            FormatContext {
                session,
                window,
                pane,
                active_session: Some(first),
                format_client,
                format_type: FormatType::Session,
            },
        )
    };

    for format_client in [
        FormatClient::NoClient,
        FormatClient::Unattached,
        FormatClient::Attached(first),
        FormatClient::Attached(second),
    ] {
        assert_eq!(expand_for(None, None, None, format_client), "");
    }
    assert_eq!(
        expand_for(
            Some(first),
            Some(first_window),
            Some(first_pane),
            FormatClient::NoClient,
        ),
        ""
    );
    assert_eq!(
        expand_for(
            Some(first),
            Some(first_window),
            Some(first_pane),
            FormatClient::Unattached,
        ),
        "0"
    );
    assert_eq!(
        expand_for(
            Some(first),
            Some(first_window),
            Some(first_pane),
            FormatClient::Attached(first),
        ),
        "1"
    );
    assert_eq!(
        expand_for(
            Some(first),
            Some(first_window),
            Some(first_pane),
            FormatClient::Attached(second),
        ),
        "0"
    );
    assert_eq!(
        expand_for(
            Some(second),
            Some(second_window),
            Some(second_pane),
            FormatClient::Attached(second),
        ),
        "1"
    );

    assert_eq!(
        engine
            .format_status_context(Some(first), Some(first_window), Some(first_pane))
            .session_active,
        None
    );
    assert_eq!(
        engine
            .format_status_context_for_client(
                Some(first),
                Some(first_window),
                Some(first_pane),
                first,
            )
            .session_active,
        Some(true)
    );
    assert_eq!(
        engine
            .format_status_context_for_client(
                Some(second),
                Some(second_window),
                Some(second_pane),
                first,
            )
            .session_active,
        Some(false)
    );
    for (format_client, expected) in [
        (FormatClient::NoClient, None),
        (FormatClient::Unattached, Some(false)),
        (FormatClient::Attached(first), Some(true)),
        (FormatClient::Attached(second), Some(false)),
    ] {
        assert_eq!(
            engine
                .format_status_context_with_format_client(
                    Some(first),
                    Some(first_window),
                    Some(first_pane),
                    Some(second),
                    format_client,
                )
                .session_active,
            expected
        );
    }
}

#[test]
fn session_path_is_scoped_to_the_selected_session() {
    let mut engine = MuxEngine::default();
    let (first, _, _) = engine.state.create_session("first").unwrap();
    let (second, _, _) = engine.state.create_session("second").unwrap();
    engine
        .state
        .set_session_working_directory(first, "/srv/first".into())
        .unwrap();
    engine
        .state
        .set_session_working_directory(second, "/srv/second".into())
        .unwrap();

    assert_eq!(expand_session_path(&engine, Some(first)), "/srv/first");
    assert_eq!(expand_session_path(&engine, Some(second)), "/srv/second");
}

#[test]
fn session_path_is_empty_without_retained_or_existing_state() {
    let mut engine = MuxEngine::default();
    let (session, _, _) = engine.state.create_session("unset").unwrap();

    assert_eq!(expand_session_path(&engine, Some(session)), "");
    assert_eq!(expand_session_path(&engine, Some(SessionId(u64::MAX))), "");
    assert_eq!(expand_session_path(&engine, None), "");
}

#[test]
fn session_path_preserves_lexical_utf8() {
    let mut engine = MuxEngine::default();
    let (session, _, _) = engine.state.create_session("work").unwrap();
    let path = "/workspace/../δ project/./[draft]*";
    engine
        .state
        .set_session_working_directory(session, path.into())
        .unwrap();

    assert_eq!(expand_session_path(&engine, Some(session)), path);
}

#[test]
fn attach_session_cwd_update_is_visible_on_the_next_expansion() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "source", "-c", "/source"]),
        )
        .unwrap();
    let source = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target", "-c", "/before"]),
        )
        .unwrap();
    let target = context.session.unwrap();
    engine
        .execute(&mut context, &command("attach-session", &["-t", "=source"]))
        .unwrap();

    assert_eq!(expand_session_path(&engine, Some(target)), "/before");

    let updated = "/after/../next path/[draft]*";
    engine
        .execute(
            &mut context,
            &command("attach-session", &["-t", "=target", "-c", updated]),
        )
        .unwrap();

    assert_eq!(expand_session_path(&engine, Some(target)), updated);
    assert_eq!(expand_session_path(&engine, Some(source)), "/source");
}

#[test]
fn built_scene_backfills_scope_and_reports_live_shapes() {
    let mut engine = MuxEngine::default();
    engine.set_format_server_context("tower.local", "tower", "/tmp/zz.sock", 946_728_000);
    let (session, first_window, first_pane) = engine.state.create_session("work").unwrap();
    engine.state.rename_window(first_window, "shell").unwrap();
    let second_pane = engine
        .state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (second_window, _) = engine
        .state
        .create_window(session, Some("editor".to_owned()), PaneKind::Terminal)
        .unwrap();
    let resolved = FormatContext {
        session: Some(session),
        window: Some(first_window),
        pane: Some(second_pane),
        active_session: Some(session),
        format_client: FormatClient::Attached(session),
        format_type: FormatType::Pane,
    }
    .resolve(&engine);
    assert_eq!(
        resolved.variable("session_id").unwrap(),
        session.to_string()
    );
    assert_eq!(resolved.variable("session_windows").unwrap(), "2");
    assert_eq!(resolved.variable("active_window_index").unwrap(), "1");
    assert_eq!(resolved.variable("last_window_index").unwrap(), "1");
    assert_eq!(
        resolved.variable("window_id").unwrap(),
        first_window.to_string()
    );
    assert_eq!(resolved.variable("window_active").unwrap(), "0");
    assert_eq!(resolved.variable("window_panes").unwrap(), "2");
    assert_eq!(
        resolved.variable("pane_id").unwrap(),
        second_pane.to_string()
    );
    assert_eq!(resolved.variable("pane_format").unwrap(), "1");
    assert_eq!(resolved.variable("pane_z").unwrap(), "1");
    assert_eq!(resolved.variable("window_format").unwrap(), "0");
    assert_eq!(resolved.variable("server_sessions").unwrap(), "1");
    assert_eq!(resolved.variable("socket_path").unwrap(), "/tmp/zz.sock");
    assert!(engine.state.windows.contains_key(&second_window));
    for variable in &FORMAT_VARIABLES {
        assert!(
            resolved.variable(variable.name).is_some(),
            "{}",
            variable.name
        );
    }

    engine.state.toggle_zoom(second_pane).unwrap();
    let zoomed = FormatContext {
        session: Some(session),
        window: Some(first_window),
        pane: Some(second_pane),
        active_session: Some(session),
        format_client: FormatClient::Attached(session),
        format_type: FormatType::Pane,
    }
    .resolve(&engine);
    assert_ne!(
        zoomed.variable("window_layout"),
        zoomed.variable("window_visible_layout")
    );
    assert!(
        zoomed
            .variable("window_visible_layout")
            .unwrap()
            .ends_with(&format!(",\"I\":\"%{}\"}}}}", second_pane.0))
    );
}

#[test]
fn name_checks_and_nested_scope_loops_use_the_engine_universe() {
    let mut engine = MuxEngine::default();
    let (zeta, first_window, first_pane) = engine.state.create_session("zeta").unwrap();
    engine.state.rename_window(first_window, "charlie").unwrap();
    let second_pane = engine
        .state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (second_window, _) = engine
        .state
        .create_window(zeta, Some("alpha".to_owned()), PaneKind::Terminal)
        .unwrap();
    let (alpha, _, _) = engine.state.create_session("alpha").unwrap();
    let context = FormatContext {
        session: Some(zeta),
        window: Some(first_window),
        pane: Some(second_pane),
        active_session: Some(zeta),
        format_client: FormatClient::Attached(zeta),
        format_type: FormatType::Pane,
    };

    assert_eq!(
        expand_format("#{N/s:alpha}|#{N/w:charlie}|#{N:nope}", &engine, context),
        "1|1|0"
    );
    assert_eq!(
        expand_format("#{S:#{session_name} }", &engine, context),
        "zeta alpha "
    );
    assert_eq!(
        expand_format("#{S/n:#{session_name} }", &engine, context),
        "alpha zeta "
    );
    assert_eq!(
        expand_format("#{S:#{session_name},[#{session_name}]} ", &engine, context),
        "[zeta]alpha "
    );
    assert_eq!(
        expand_format(
            "#{S:#{session_name}=#{session_active},[#{session_name}=#{session_active}]} ",
            &engine,
            FormatContext {
                format_client: FormatClient::Attached(alpha),
                ..context
            },
        ),
        "zeta=0[alpha=1] "
    );
    assert_eq!(
        expand_format(
            "#{S:#{session_name}=#{session_active},[#{session_name}=#{session_active}]} ",
            &engine,
            FormatContext {
                format_client: FormatClient::NoClient,
                ..context
            },
        ),
        "zeta=alpha= "
    );
    assert_eq!(
        expand_format(
            "#{S:#{session_name}=#{session_active},[#{session_name}=#{session_active}]} ",
            &engine,
            FormatContext {
                format_client: FormatClient::Unattached,
                ..context
            },
        ),
        "zeta=0alpha=0 "
    );
    assert_eq!(
        expand_format("#{W:#{window_name},[#{window_name}]}", &engine, context),
        "charlie[alpha]"
    );
    assert_eq!(
        expand_format("#{P:#{pane_index},[#{pane_index}]}", &engine, context),
        "0[1]"
    );
    assert_eq!(
        expand_format(
            "#{S:#{session_name}:#{loop_index}:#{loop_last_flag};}",
            &engine,
            context,
        ),
        "zeta:0:0;alpha:1:1;"
    );
    assert!(engine.state.sessions.contains_key(&alpha));
    assert!(engine.state.windows.contains_key(&second_window));
}

#[test]
fn the_client_loop_body_traces_through_the_shared_sink() {
    struct Roster;

    impl StatusHooks for Roster {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn client_loop_rows(&mut self, _context: &StatusContext) -> Vec<FormatClientRow> {
            vec![FormatClientRow {
                name: "/dev/pts/2".to_owned(),
                activity: 0,
                variables: BTreeMap::from([("client_name".to_owned(), "/dev/pts/2".to_owned())]),
                environment: Vec::new(),
            }]
        }
    }

    let engine = MuxEngine::default();
    let (value, trace) = expand_format_time_traced(
        "#{L:<#{client_name}>}",
        &engine,
        FormatContext::default(),
        &mut Roster,
    );
    assert_eq!(value, "</dev/pts/2>");
    assert_eq!(
        trace,
        vec![
            "# expanding format: #{L:<#{client_name}>}",
            "# found #{}: L:<#{client_name}>",
            "# modifier 0 is L",
            "#  expanding format: <#{client_name}>",
            "#  found #{}: client_name",
            "#  format 'client_name' found: /dev/pts/2",
            "#  replaced 'client_name' with '/dev/pts/2'",
            "#  result is: </dev/pts/2>",
            "# replaced 'L:<#{client_name}>' with '</dev/pts/2>'",
            "# result is: </dev/pts/2>",
        ]
    );
}
