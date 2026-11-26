use super::*;

#[test]
fn pane_titles_change_snapshot_generation_without_overwriting_static_window_names() {
    let mut state = MuxState::default();
    let (_, window, terminal) = state.create_session("main").unwrap();
    let browser = state
        .split_pane(
            terminal,
            Axis::Horizontal,
            PaneKind::Browser(BrowserDescriptor::single(
                "https://example.com".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();
    state.rename_window(window, "manually named").unwrap();

    let generation = state.generation();
    assert!(state.update_pane_title(terminal, "~/src/zz").unwrap());
    assert_eq!(state.generation(), generation + 1);
    assert_eq!(state.windows[&window].name, "manually named");
    assert_eq!(state.windows[&window].panes[&terminal].title, "~/src/zz");
    assert!(!state.update_pane_title(terminal, "~/src/zz").unwrap());
    assert_eq!(state.generation(), generation + 1);

    assert!(state.update_pane_title(browser, "Example Domain").unwrap());
    state
        .update_browser_url(browser, "https://example.com".to_owned())
        .unwrap();
    assert_eq!(
        state.windows[&window].panes[&browser].title, "Example Domain",
        "a duplicate address event must not replace a newer document title"
    );
    state
        .update_browser_url(browser, "https://example.org".to_owned())
        .unwrap();
    assert_eq!(
        state.windows[&window].panes[&browser].title, "Example Domain",
        "URL persistence and displayed pane titles have separate ownership"
    );
    assert!(matches!(
        &state.windows[&window].panes[&browser].kind,
        PaneKind::Browser(browser) if browser.url() == "https://example.org"
    ));

    let generation = state.generation();
    state.update_browser_profile(browser, " Work ").unwrap();
    assert_eq!(state.generation(), generation + 1);
    assert!(matches!(
        &state.windows[&window].panes[&browser].kind,
        PaneKind::Browser(browser) if browser.profile == "Work"
    ));
    state.update_browser_profile(browser, "zz-default").unwrap();
    assert!(matches!(
        &state.windows[&window].panes[&browser].kind,
        PaneKind::Browser(browser) if browser.profile == "default"
    ));
}

#[test]
fn pane_bells_ride_the_snapshot_and_only_move_on_a_transition() {
    let mut state = MuxState::default();
    let (_, _, pane) = state.create_session("work").unwrap();
    let bell = |state: &MuxState| state.snapshot().sessions[0].windows[0].panes[&pane].bell;
    assert!(!bell(&state));

    let generation = state.generation();
    assert!(state.set_pane_bell(pane, true));
    assert!(bell(&state));
    assert!(state.generation() > generation);

    let generation = state.generation();
    assert!(!state.set_pane_bell(pane, true));
    assert_eq!(state.generation(), generation);

    assert!(state.set_pane_bell(pane, false));
    assert!(!bell(&state));
    assert!(!state.set_pane_bell(pane, false));

    assert!(!state.set_pane_bell(PaneId(9999), false));
}

#[test]
fn update_pane_summary_bumps_generation_only_on_change() {
    let mut state = MuxState::default();
    let (_, _, pane) = state.create_session("work").unwrap();
    let summary = |state: &MuxState| {
        state.snapshot().sessions[0].windows[0].panes[&pane]
            .summary
            .clone()
    };
    assert_eq!(summary(&state), PaneSummary::default());

    let generation = state.generation();
    assert!(state.update_pane_summary(pane, |summary| summary.changed_at = 42));
    assert!(state.generation() > generation);
    assert_eq!(summary(&state).changed_at, 42);

    let generation = state.generation();
    assert!(!state.update_pane_summary(pane, |summary| summary.changed_at = 42));
    assert!(!state.update_pane_summary(pane, |_| {}));
    assert_eq!(state.generation(), generation);

    assert!(!state.update_pane_summary(PaneId(9999), |summary| summary.unseen = true));
    assert_eq!(state.generation(), generation);
}

#[test]
fn clear_pane_alerts_reports_either_latch() {
    let mut state = MuxState::default();
    let (_, _, pane) = state.create_session("work").unwrap();
    assert!(!state.clear_pane_alerts(pane));
    assert!(state.update_pane_summary(pane, |summary| summary.unseen = true));
    assert!(state.clear_pane_alerts(pane));
    assert!(state.set_pane_bell(pane, true));
    assert!(state.clear_pane_alerts(pane));
    assert!(!state.clear_pane_alerts(pane));
}
