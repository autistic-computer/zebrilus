use super::*;

#[test]
#[cfg(unix)]
fn event_loop_delivers_suspend_and_resume() {
    let (events, incoming) = mpsc::channel();
    let signals = event_loop::SignalInbox::new().unwrap();
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::TSTP)
        .unwrap();
    signals.wait_for_signal(&events).unwrap();
    assert!(matches!(
        incoming.recv_timeout(Duration::from_secs(2)).unwrap(),
        MainEvent::Suspend
    ));
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::CONT)
        .unwrap();
    signals.wait_for_signal(&events).unwrap();
    assert!(matches!(
        incoming.recv_timeout(Duration::from_secs(2)).unwrap(),
        MainEvent::Resume
    ));
}

#[test]
fn escape_timeout_honors_the_pinned_default_and_live_values() {
    let mut options = zz_protocol::MuxOptions::default();
    assert_eq!(escape_timeout_ms(&options), 10);
    options.set(
        zz_protocol::MuxOptionKey::EscapeTime,
        "0",
        zz_protocol::MuxOptionSource::RuntimeCommand,
    );
    assert_eq!(escape_timeout_ms(&options), 1);
    options.set(
        zz_protocol::MuxOptionKey::EscapeTime,
        "50",
        zz_protocol::MuxOptionSource::RuntimeCommand,
    );
    assert_eq!(escape_timeout_ms(&options), 50);
    options.set(
        zz_protocol::MuxOptionKey::EscapeTime,
        "bogus",
        zz_protocol::MuxOptionSource::RuntimeCommand,
    );
    assert_eq!(escape_timeout_ms(&options), 10);
}

#[test]
fn tui_exit_notices_and_codes_match_tmux() {
    for (exit, notice, code) in [
        (
            TuiExit::Detached("work".to_owned()),
            "[detached (from session work)]",
            0,
        ),
        (TuiExit::Exited, "[exited]", 0),
        (TuiExit::ServerExited, "[server exited]", 1),
        (
            TuiExit::ServerExitedUnexpectedly,
            "[server exited unexpectedly]",
            1,
        ),
    ] {
        assert_eq!(exit.notice(), notice);
        assert_eq!(exit.exit_code(), code);
    }
}

#[test]
fn failed_connect_first_host_switch_keeps_the_current_endpoint() {
    let current = Endpoint::parse("unix:///tmp/zz.sock").unwrap();
    let original = current.clone();
    let target = HostSwitch {
        label: "box".to_owned(),
        endpoint: Endpoint::parse("ssh://box").unwrap(),
    };

    let result: Result<HostSwitchDecision<()>, String> =
        prepare_host_switch(&current, target, |_| Err("offline".to_owned()));

    assert!(matches!(result, Err(error) if error == "offline"));
    assert_eq!(current, original);
}

#[test]
fn provider_navigation_publishes_the_full_tab_descriptor_once_changed() {
    let current = BrowserDescriptor {
        tabs: vec!["https://one".to_owned()],
        active_tab: 0,
        profile: "default".to_owned(),
    };
    assert!(
        set_browser_tabs_command(PaneId(3), &current, vec!["https://one".to_owned()], 0,).is_none()
    );

    let command = set_browser_tabs_command(
        PaneId(3),
        &current,
        vec!["https://one".to_owned(), "https://two".to_owned()],
        1,
    )
    .unwrap();
    assert_eq!(command.name, "set-browser-tabs");
    assert_eq!(
        command.args,
        ["-t", "%3", "-a", "1", "--", "https://one", "https://two",]
    );
    assert!(set_browser_tabs_command(PaneId(3), &current, Vec::new(), 0).is_none());
    assert!(
        set_browser_tabs_command(PaneId(3), &current, vec!["https://two".to_owned()], 1).is_none()
    );
}
