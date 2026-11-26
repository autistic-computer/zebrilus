use super::*;

#[test]
fn kill_and_move_window_touch_only_newly_selected_windows() {
    let mut state = MuxState::default();
    state.set_format_now(10);
    let (source_session, fallback, _) = state.create_session("source").unwrap();
    let (current, _) = state
        .create_window(source_session, None, PaneKind::Terminal)
        .unwrap();
    let fallback_activity = state.windows[&fallback].activity;

    state.set_format_now(20);
    state.kill_window(current).unwrap();

    assert_eq!(state.sessions[&source_session].active_window, fallback);
    assert!(state.windows[&fallback].activity > fallback_activity);
    assert_eq!(state.windows[&fallback].activity_time, Some(20));

    let (inactive, _) = state
        .create_window_at(source_session, None, None, PaneKind::Terminal, false)
        .unwrap();
    let fallback_activity = state.windows[&fallback].activity;
    state.set_format_now(21);
    state.kill_window(inactive).unwrap();
    assert_eq!(state.windows[&fallback].activity, fallback_activity);
    assert_eq!(state.windows[&fallback].activity_time, Some(20));

    let (moving, _) = state
        .create_window(source_session, None, PaneKind::Terminal)
        .unwrap();
    let fallback_activity = state.windows[&fallback].activity;
    state.set_format_now(30);
    state
        .move_window(moving, source_session, 2, false, false)
        .unwrap();
    assert_eq!(state.sessions[&source_session].active_window, fallback);
    assert!(state.windows[&fallback].activity > fallback_activity);
    assert_eq!(state.windows[&fallback].activity_time, Some(30));

    let moving_activity = state.windows[&moving].activity;
    state.set_format_now(31);
    state
        .move_window(moving, source_session, 3, false, true)
        .unwrap();
    assert_eq!(state.sessions[&source_session].active_window, moving);
    assert!(state.windows[&moving].activity > moving_activity);
    assert_eq!(state.windows[&moving].activity_time, Some(31));

    let moving_activity = state.windows[&moving].activity;
    state.set_format_now(32);
    state
        .move_window(moving, source_session, 4, false, true)
        .unwrap();
    assert!(state.windows[&moving].activity > moving_activity);
    assert_eq!(state.windows[&moving].activity_time, Some(32));

    let (destination_session, _, _) = state.create_session("destination").unwrap();
    let moving_activity = state.windows[&moving].activity;
    let fallback_activity = state.windows[&fallback].activity;
    state.set_format_now(40);
    state
        .move_window(moving, destination_session, 1, false, true)
        .unwrap();
    assert_eq!(state.sessions[&destination_session].active_window, moving);
    assert_eq!(state.sessions[&source_session].active_window, fallback);
    assert!(state.windows[&moving].activity > moving_activity);
    assert!(state.windows[&fallback].activity > fallback_activity);
    assert!(state.windows[&fallback].activity > state.windows[&moving].activity);
    assert_eq!(state.windows[&moving].activity_time, Some(40));
    assert_eq!(state.windows[&fallback].activity_time, Some(40));
    assert!(state.validate().is_ok());
}

#[test]
fn swap_window_updates_activity_only_when_destination_selection_changes() {
    let mut state = MuxState::default();
    state.set_format_now(10);
    let (session, source, _) = state.create_session("work").unwrap();
    let (target, _) = state
        .create_window_at(session, None, None, PaneKind::Terminal, false)
        .unwrap();
    let source_activity = state.windows[&source].activity;
    let target_activity = state.windows[&target].activity;
    let next_sort_point = state.next_sort_point;

    state.swap_windows(source, target, false).unwrap();

    assert_eq!(state.sessions[&session].active_window, target);
    assert_eq!(state.windows[&source].activity, source_activity);
    assert_eq!(state.windows[&target].activity, target_activity);
    assert_eq!(state.windows[&source].activity_time, Some(10));
    assert_eq!(state.windows[&target].activity_time, Some(10));
    assert_eq!(state.next_sort_point, next_sort_point);

    state.set_format_now(20);
    state.select_window(session, source).unwrap();
    let source_activity = state.windows[&source].activity;
    let target_activity = state.windows[&target].activity;
    let next_sort_point = state.next_sort_point;
    state.set_format_now(30);
    state.swap_windows(source, target, true).unwrap();

    assert_eq!(state.sessions[&session].active_window, source);
    assert!(state.windows[&source].activity > source_activity);
    assert_eq!(state.windows[&target].activity, target_activity);
    assert_eq!(state.windows[&source].activity_time, Some(30));
    assert_eq!(state.windows[&target].activity_time, Some(10));
    assert_eq!(state.next_sort_point, next_sort_point + 1);
    assert!(state.validate().is_ok());
}

#[test]
fn window_activation_clears_bell_and_unseen() {
    let mut state = MuxState::default();
    let (session, first, _) = state.create_session("work").unwrap();
    let (second, pane) = state
        .create_window(session, Some("second".to_owned()), PaneKind::Terminal)
        .unwrap();
    state.select_window(session, first).unwrap();
    assert!(state.set_pane_bell(pane, true));
    assert!(state.update_pane_summary(pane, |summary| {
        summary.unseen = true;
        summary.changed_at = 7;
    }));

    state.select_window(session, second).unwrap();
    let pane = state.pane(pane).unwrap();
    assert!(!pane.bell);
    assert!(!pane.summary.unseen);
    assert_eq!(pane.summary.changed_at, 7);
}

#[test]
fn window_activity_time_tracks_only_creation_selection_and_output() {
    let mut state = MuxState::default();
    let (session, first_window, _) = state.create_session("work").unwrap();
    assert_eq!(state.windows[&first_window].activity_time, None);

    state.set_format_now(100);
    let (second_window, first_pane) = state
        .create_window_at(session, None, None, PaneKind::Terminal, false)
        .unwrap();
    assert_eq!(state.windows[&second_window].activity_time, Some(100));
    assert_eq!(state.windows[&first_window].activity_time, None);

    state.set_format_now(200);
    state.select_window(session, second_window).unwrap();
    let selected_activity = state.windows[&second_window].activity;
    assert_eq!(state.windows[&second_window].activity_time, Some(200));

    state.set_format_now(300);
    state.select_window(session, second_window).unwrap();
    assert_eq!(state.windows[&second_window].activity, selected_activity);
    assert_eq!(state.windows[&second_window].activity_time, Some(200));

    state.set_format_now(400);
    let second_pane = state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    state.select_pane(first_pane).unwrap();
    state
        .select_layout(
            second_window,
            LayoutPreset::EvenVertical,
            &PresetOptions::default(),
        )
        .unwrap();
    assert_eq!(state.windows[&second_window].activity_time, Some(200));

    state.set_format_now(500);
    let broken_window = state
        .break_pane(second_pane, session, None, None, true)
        .unwrap();
    assert_eq!(state.windows[&broken_window].activity_time, Some(500));
    assert_eq!(state.windows[&second_window].activity_time, Some(200));

    state.set_format_now(600);
    state.touch_window_activity_for_pane(first_pane);
    assert!(state.windows[&second_window].activity > selected_activity);
    assert_eq!(state.windows[&second_window].activity_time, Some(600));
    assert_eq!(state.windows[&broken_window].activity_time, Some(500));
}

#[test]
fn window_alert_flags_move_on_transition_and_clear_on_activation() {
    let mut state = MuxState::default();
    let (session, first, _) = state.create_session("work").unwrap();
    let (second, _) = state
        .create_window_at(session, None, None, PaneKind::Terminal, false)
        .unwrap();

    let generation = state.generation();
    assert!(state.set_window_activity_flag(second, true));
    assert!(state.set_window_silence_flag(second, true));
    assert!(state.generation() > generation);
    assert!(state.windows[&second].activity_flag);
    assert!(state.windows[&second].silence_flag);
    let snapshot = state.snapshot();
    assert!(
        snapshot.sessions[0]
            .windows
            .iter()
            .find(|window| window.id == second)
            .expect("second window snapshot")
            .activity
    );

    let generation = state.generation();
    assert!(!state.set_window_activity_flag(second, true));
    assert!(!state.set_window_silence_flag(second, true));
    assert_eq!(state.generation(), generation);

    assert!(state.activate_window(session, second));
    assert!(!state.windows[&second].activity_flag);
    assert!(!state.windows[&second].silence_flag);
    let snapshot = state.snapshot();
    assert!(
        !snapshot.sessions[0]
            .windows
            .iter()
            .find(|window| window.id == second)
            .expect("second window snapshot")
            .activity
    );

    assert!(!state.set_window_activity_flag(WindowId(9999), true));
    assert!(!state.set_window_silence_flag(WindowId(9999), true));
    let _ = first;
}
