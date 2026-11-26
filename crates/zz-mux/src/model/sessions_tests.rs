use super::*;

#[test]
fn session_working_directory_changes_generation_only_when_value_changes() {
    let mut state = MuxState::default();
    let (session, _, _) = state.create_session("main").unwrap();
    let created_generation = state.generation();

    assert_eq!(state.session_working_directory(session), None);
    assert!(
        state
            .set_session_working_directory(session, PathBuf::from("/first"))
            .unwrap()
    );
    assert_eq!(
        state.session_working_directory(session),
        Some(Path::new("/first"))
    );
    assert_eq!(state.generation(), created_generation + 1);

    assert!(
        !state
            .set_session_working_directory(session, PathBuf::from("/first"))
            .unwrap()
    );
    assert_eq!(state.generation(), created_generation + 1);

    assert!(
        state
            .set_session_working_directory(session, PathBuf::from("/second"))
            .unwrap()
    );
    assert_eq!(
        state.session_working_directory(session),
        Some(Path::new("/second"))
    );
    assert_eq!(state.generation(), created_generation + 2);
}

#[test]
fn session_activity_marks_keep_exact_seconds_and_logical_mru() {
    let mut state = MuxState::default();
    let (first, _, _) = state.create_session("first").unwrap();
    let (second, _, _) = state.create_session("second").unwrap();
    let second_created = state.sessions[&second].sort_activity;

    state.mark_session_active_at(first, 1_700_000_123);
    assert_eq!(state.sessions[&first].activity, Some(1_700_000_123));
    assert!(state.sessions[&first].sort_activity > second_created);
    assert_eq!(
        state.most_recent_context().map(|context| context.0),
        Some(first)
    );

    let first_activity = state.sessions[&first].sort_activity;
    state.mark_session_active_at(second, 1_700_000_123);
    assert_eq!(state.sessions[&second].activity, Some(1_700_000_123));
    assert!(state.sessions[&second].sort_activity > first_activity);
    assert_eq!(
        state.most_recent_context().map(|context| context.0),
        Some(second)
    );
}
