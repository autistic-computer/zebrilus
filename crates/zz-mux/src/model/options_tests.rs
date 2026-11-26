use super::*;

#[test]
fn synchronized_input_resolves_global_window_and_pane_inheritance() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();

    assert_eq!(state.synchronized_input_targets(first).unwrap(), [first]);
    state.set_global_synchronize_panes(true);
    assert_eq!(
        state.synchronized_input_targets(first).unwrap(),
        [first, second]
    );

    state
        .set_pane_synchronize_panes(second, Some(false))
        .unwrap();
    assert_eq!(state.synchronized_input_targets(first).unwrap(), [first]);
    assert_eq!(state.synchronized_input_targets(second).unwrap(), [second]);

    state.clear_pane_synchronize_overrides(window).unwrap();
    state
        .set_window_synchronize_panes(window, Some(false))
        .unwrap();
    state
        .set_pane_synchronize_panes(second, Some(true))
        .unwrap();
    assert_eq!(state.synchronized_input_targets(first).unwrap(), [first]);
    assert_eq!(state.synchronized_input_targets(second).unwrap(), [second]);

    state.set_window_synchronize_panes(window, None).unwrap();
    let panes = &state.snapshot().sessions[0].windows[0].panes;
    assert!(panes[&first].synchronized_input);
    assert!(panes[&second].synchronized_input);
    assert_eq!(
        state.synchronized_input_targets(second).unwrap(),
        [first, second]
    );
}
