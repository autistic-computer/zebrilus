use super::*;

#[test]
fn stable_targets_and_names_resolve() {
    let mut state = MuxState::default();
    let (session, window, pane) = state.create_session("work").unwrap();
    assert_eq!(state.resolve_session(Some("work"), None).unwrap(), session);
    assert_eq!(
        state
            .resolve_window(Some("0"), Some(session), None)
            .unwrap(),
        window
    );
    assert_eq!(state.resolve_pane(Some("%0"), None, None).unwrap(), pane);
}

#[test]
fn fnmatch_uses_tmux_flags_zero_metacharacters() {
    assert!(fnmatch("w[oa]rk", "work"));
    assert!(fnmatch("w[!x]rk", "work"));
    assert!(fnmatch("file?", "file1"));
    assert!(fnmatch("**/name", "path/to/name"));
    assert!(fnmatch(r"literal\*", "literal*"));
    assert!(!fnmatch("w[a-c]rk", "work"));
    assert!(!fnmatch("file?", "file10"));
}

#[test]
fn session_targets_follow_id_exact_prefix_and_fnmatch_passes() {
    let mut state = MuxState::default();
    let (work, ..) = state.create_session("work").unwrap();
    let (workshop, ..) = state.create_session("workshop").unwrap();
    let (other, ..) = state.create_session("other").unwrap();
    let (slash, ..) = state.create_session("path/name").unwrap();

    assert_eq!(state.resolve_session(Some(""), Some(work)).unwrap(), work);
    assert_eq!(
        state
            .resolve_session(Some(&work.to_string()), Some(other))
            .unwrap(),
        work
    );
    assert_eq!(
        state
            .resolve_session(Some(&format!("={work}")), Some(other))
            .unwrap(),
        work
    );
    assert_eq!(
        state.resolve_session(Some("work"), None).unwrap(),
        work,
        "an exact name wins over the longer session it prefixes"
    );
    assert_eq!(
        state.resolve_session(Some("workshop"), None).unwrap(),
        workshop
    );
    assert_eq!(
        state.resolve_session(Some("works"), None).unwrap(),
        workshop
    );
    assert_eq!(state.resolve_session(Some("o"), None).unwrap(), other);
    assert_eq!(state.resolve_session(Some("oth?r"), None).unwrap(), other);
    assert_eq!(state.resolve_session(Some("path/*"), None).unwrap(), slash);

    let ambiguous = state.resolve_session(Some("wor"), None).unwrap_err();
    assert!(
        matches!(&ambiguous, ServerError::SessionNotFound(target) if target == "wor"),
        "{ambiguous:?}"
    );
    let ambiguous = state.resolve_session(Some("work*"), None).unwrap_err();
    assert!(matches!(ambiguous, ServerError::SessionNotFound(target) if target == "work*"));
    let exact_only = state.resolve_session(Some("=works"), None).unwrap_err();
    assert!(matches!(exact_only, ServerError::SessionNotFound(target) if target == "works"));
    let missing = state.resolve_session(Some("nope"), None).unwrap_err();
    assert!(matches!(missing, ServerError::SessionNotFound(target) if target == "nope"));
    let malformed = state.resolve_session(Some("$nope"), None).unwrap_err();
    assert!(matches!(malformed, ServerError::SessionNotFound(target) if target == "$nope"));
}

#[test]
fn session_targets_accept_pane_and_window_ids() {
    let mut state = MuxState::default();
    let (work, work_window, work_pane) = state.create_session("work").unwrap();
    let (other, ..) = state.create_session("other").unwrap();
    assert_eq!(
        state
            .resolve_session(Some(&work_pane.to_string()), Some(other))
            .unwrap(),
        work
    );
    assert_eq!(
        state
            .resolve_session(Some(&work_window.to_string()), Some(other))
            .unwrap(),
        work
    );
    let missing_pane = state.resolve_session(Some("%99"), None).unwrap_err();
    assert!(matches!(missing_pane, ServerError::PaneNotFound(target) if target == "%99"));
    let missing_window = state.resolve_session(Some("@99"), None).unwrap_err();
    assert!(matches!(missing_window, ServerError::WindowNotFound(target) if target == "@99"));
    let malformed = state.resolve_session(Some("%x"), None).unwrap_err();
    assert!(matches!(malformed, ServerError::PaneNotFound(target) if target == "%x"));
}

#[test]
fn session_targets_validate_window_and_pane_components() {
    let mut state = MuxState::default();
    let (session, window, pane) = state.create_session("cli").unwrap();
    state.rename_window(window, "win").unwrap();
    let (_, other_pane) = state
        .create_window(session, Some("other".to_owned()), PaneKind::Terminal)
        .unwrap();
    state.select_window(session, window).unwrap();
    for target in [
        "cli",
        "=cli:win",
        "cli:win.0",
        "cli:.0",
        ":win.0",
        "win.0",
        "%0",
        "@0",
        "=cli:=win",
        "cli:win.",
        "cli:=",
        "=:",
        "=cli:=",
        "cli:=.0",
        "cli:=.%0",
        "=cli:=.0",
        ":=.0",
        "cli:=.",
    ] {
        assert_eq!(
            state.resolve_session(Some(target), Some(session)),
            Ok(session),
            "{target}"
        );
    }
    assert_eq!(
        state.resolve_session(Some(&format!("cli:.{other_pane}")), None),
        Ok(session)
    );
    assert_eq!(
        state.resolve_session(Some(&format!("cli:=.{other_pane}")), None),
        Ok(session)
    );
    for (target, error) in [
        (
            "=cli:nosuchwin",
            ServerError::WindowNotFound("nosuchwin".to_owned()),
        ),
        (
            "=nosuch:win",
            ServerError::SessionNotFound("nosuch".to_owned()),
        ),
        ("cli:win.9", ServerError::PaneNotFound("9".to_owned())),
        ("cli:=.9", ServerError::PaneNotFound("9".to_owned())),
        (
            "cli:nosuchwin.9",
            ServerError::WindowNotFound("nosuchwin".to_owned()),
        ),
        ("%99", ServerError::PaneNotFound("%99".to_owned())),
        ("@99", ServerError::WindowNotFound("@99".to_owned())),
        ("=%0", ServerError::SessionNotFound("%0".to_owned())),
        ("=cl:win", ServerError::SessionNotFound("cl".to_owned())),
    ] {
        assert_eq!(
            state.resolve_session(Some(target), Some(session)),
            Err(error),
            "{target}"
        );
    }
    let (other_session, _, foreign_pane) = state.create_session("foreign").unwrap();
    for target in [
        format!(":.{foreign_pane}"),
        format!("=:.{foreign_pane}"),
        format!(":=.{foreign_pane}"),
        format!("=:=.{foreign_pane}"),
    ] {
        assert_eq!(
            state.resolve_session(Some(&target), Some(session)),
            Ok(other_session),
            "{target}"
        );
    }
    assert_eq!(
        state.resolve_session(Some(&format!("cli:.{foreign_pane}")), Some(session)),
        Err(ServerError::PaneNotFound(foreign_pane.to_string()))
    );
    assert_eq!(
        state.resolve_session(Some(&format!("cli:=.{foreign_pane}")), Some(session)),
        Err(ServerError::PaneNotFound(foreign_pane.to_string()))
    );
    assert_eq!(pane, PaneId(0));
    assert_eq!(
        state.resolve_session(Some(&format!("cli:win.{other_pane}")), None),
        Err(ServerError::PaneNotFound(other_pane.to_string()))
    );
}

#[test]
fn window_targets_follow_tokens_indexes_names_prefixes_and_fnmatch() {
    let mut state = MuxState::default();
    let (session, first, _) = state.create_session("work").unwrap();
    state.rename_window(first, "root").unwrap();
    let (alpha, _) = state
        .create_window(session, Some("alpha".to_owned()), PaneKind::Terminal)
        .unwrap();
    let (alpine, _) = state
        .create_window(session, Some("alpine".to_owned()), PaneKind::Terminal)
        .unwrap();
    let (caret, _) = state
        .create_window(session, Some("^".to_owned()), PaneKind::Terminal)
        .unwrap();
    state.set_window_index(caret, 9).unwrap();
    state.set_window_index(alpine, 7).unwrap();
    state.set_window_index(alpha, 3).unwrap();
    state.select_window(session, first).unwrap();

    for target in ["^", "{start}"] {
        assert_eq!(
            state
                .resolve_window(Some(target), Some(session), Some(first))
                .unwrap(),
            first
        );
    }
    for target in ["work:$", "{end}", "!", "{last}"] {
        assert_eq!(
            state
                .resolve_window(Some(target), Some(session), Some(first))
                .unwrap(),
            caret
        );
    }
    for (target, expected) in [
        ("+", alpha),
        ("{next}", alpha),
        ("+2", alpine),
        ("-", caret),
        ("{previous}", caret),
        ("-2", alpine),
        ("7", alpine),
        ("alph", alpha),
        ("a*ha", alpha),
        ("=^", caret),
        ("={start}", caret),
    ] {
        assert_eq!(
            state
                .resolve_window(Some(target), Some(session), Some(first))
                .unwrap(),
            expected,
            "{target}"
        );
    }
    assert_eq!(
        state
            .resolve_window(Some(&caret.to_string()), Some(session), Some(first))
            .unwrap(),
        caret
    );
    assert_eq!(
        state
            .resolve_window_index_target(Some("+5"), Some(session), Some(first))
            .unwrap(),
        (session, Some(5))
    );
    assert_eq!(
        state
            .resolve_window_index_target(Some("alph"), Some(session), Some(first))
            .unwrap(),
        (session, Some(3))
    );

    for target in ["alp", "a*"] {
        assert!(matches!(
            state.resolve_window(Some(target), Some(session), Some(first)),
            Err(ServerError::WindowNotFound(component)) if component == target
        ));
    }
    assert!(matches!(
        state.resolve_window(Some("work:=missing"), Some(session), Some(first)),
        Err(ServerError::WindowNotFound(component)) if component == "missing"
    ));

    let (fallback, fallback_window, _) = state.create_session("fallback").unwrap();
    assert_eq!(
        state
            .resolve_window(Some("=fallback"), Some(session), Some(first))
            .unwrap(),
        fallback_window
    );
    assert_eq!(
        state
            .resolve_window(Some(&fallback.to_string()), Some(session), Some(first))
            .unwrap(),
        fallback_window
    );

    let (lonely, lonely_window, _) = state.create_session("lonely").unwrap();
    assert!(matches!(
        state.resolve_window(Some("lonely:{last}"), Some(lonely), Some(lonely_window)),
        Err(ServerError::WindowNotFound(component)) if component == "!"
    ));
}

#[test]
fn compound_window_targets_resolve_within_the_named_session() {
    let mut state = MuxState::default();
    let (named_session, named_window, _) = state.create_session("a").unwrap();
    state.rename_window(named_window, "b").unwrap();
    let (current_session, current_window, _) = state.create_session("current").unwrap();
    state.rename_window(current_window, "a:b").unwrap();

    assert_eq!(
        state
            .resolve_window(Some("a:b"), Some(current_session), Some(current_window),)
            .unwrap(),
        named_window,
        "the first colon selects session a instead of the current window named a:b"
    );
    assert_eq!(
        state
            .resolve_window(
                Some(&format!("{named_session}:0")),
                Some(current_session),
                Some(current_window),
            )
            .unwrap(),
        named_window
    );
    assert_eq!(
        state
            .resolve_window(
                Some(&format!("a:{named_window}")),
                Some(current_session),
                Some(current_window),
            )
            .unwrap(),
        named_window
    );
    assert_eq!(
        state
            .resolve_window(Some(":a:b"), None, Some(current_window))
            .unwrap(),
        current_window,
        "an empty session component keeps the current session"
    );
}

#[test]
fn pane_indexes_and_compound_targets_resolve_without_weakening_absolute_ids() {
    let mut state = MuxState::default();
    let (session, first_window, first_pane) = state.create_session("a").unwrap();
    state.rename_window(first_window, "shell").unwrap();
    let (second_window, second_pane) = state
        .create_window(session, Some("b".to_owned()), PaneKind::Terminal)
        .unwrap();
    let third_pane = state
        .split_pane(second_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (_, _, absolute_pane) = state.create_session("other").unwrap();
    assert_eq!(absolute_pane, PaneId(3));

    for target in [".0", "0"] {
        assert_eq!(
            state
                .resolve_pane(Some(target), Some(second_window), Some(second_pane))
                .unwrap(),
            second_pane
        );
    }
    assert_eq!(
        state
            .resolve_pane(Some(".1"), Some(second_window), Some(second_pane))
            .unwrap(),
        third_pane
    );
    for target in ["a:b.1", ":b.1"] {
        assert_eq!(
            state
                .resolve_pane(Some(target), Some(second_window), Some(second_pane))
                .unwrap(),
            third_pane
        );
    }
    for target in ["a:0.0", "a:shell.0"] {
        assert_eq!(
            state
                .resolve_pane(Some(target), Some(second_window), Some(second_pane))
                .unwrap(),
            first_pane
        );
    }
    assert_eq!(
        state
            .resolve_pane(Some("%3"), Some(second_window), Some(second_pane))
            .unwrap(),
        absolute_pane,
        "absolute pane ids retain priority over index parsing"
    );
    for target in ["a", "b", "a:b"] {
        assert_eq!(
            state
                .resolve_pane(Some(target), Some(second_window), Some(second_pane))
                .unwrap(),
            third_pane
        );
    }
    for target in ["a:0", "a:shell"] {
        assert_eq!(
            state
                .resolve_pane(Some(target), Some(second_window), Some(second_pane))
                .unwrap(),
            first_pane
        );
    }
    assert!(matches!(
        state.resolve_pane(Some("a:b.9"), Some(second_window), Some(second_pane)),
        Err(ServerError::PaneNotFound(target)) if target == "9"
    ));
}

#[test]
fn window_targets_accept_pane_forms_like_tmux() {
    let mut state = MuxState::default();
    let (session, first_window, _first_pane) = state.create_session("a").unwrap();
    state.rename_window(first_window, "shell").unwrap();
    let (second_window, second_pane) = state
        .create_window(session, Some("b".to_owned()), PaneKind::Terminal)
        .unwrap();
    for target in ["a:shell.0", "a:0.0", "0.0"] {
        assert_eq!(
            state
                .resolve_window(Some(target), Some(session), Some(first_window))
                .unwrap(),
            first_window
        );
    }
    assert_eq!(
        state
            .resolve_window(Some(&second_pane.to_string()), Some(session), None)
            .unwrap(),
        second_window
    );
    assert!(matches!(
        state.resolve_window(Some("a:shell.9"), Some(session), Some(first_window)),
        Err(ServerError::PaneNotFound(target)) if target == "9"
    ));
}

#[test]
fn deeply_segmented_targets_do_not_recurse_between_resolvers() {
    let mut state = MuxState::default();
    let (session, window, pane) = state.create_session("work").unwrap();
    let target = format!("{}1", "1.".repeat(700));

    assert!(matches!(
        state.resolve_window(Some(&target), Some(session), Some(window)),
        Err(ServerError::WindowNotFound(_))
    ));
    assert!(matches!(
        state.resolve_pane(Some(&target), Some(window), Some(pane)),
        Err(ServerError::WindowNotFound(_))
    ));
}
