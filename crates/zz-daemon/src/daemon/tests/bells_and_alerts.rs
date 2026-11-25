use super::*;

#[test]
fn a_bell_publishes_one_edge_and_a_flag_that_input_clears() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, mut context, first, second) = belled_session(&shared, &mailbox);
    let background_mailbox = OutboundMailbox::new();
    shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&background_mailbox),
    );
    take_reliable_messages(&background_mailbox);

    shared.raise_pane_bell(second);
    let messages = take_reliable_messages(&mailbox);
    let background_messages = take_reliable_messages(&background_mailbox);
    assert_eq!(bell_events(&messages), vec![second]);
    assert!(bell_events(&background_messages).is_empty());
    assert!(pane_bell(&messages, second));
    assert!(pane_bell(&background_messages, second));
    assert!(!pane_bell(&messages, first));

    shared.raise_pane_bell(second);
    assert_eq!(bell_events(&take_reliable_messages(&mailbox)), vec![second]);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: first,
                text: "true\n".to_owned(),
            },
        )
        .expect("type into the quiet pane");
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(second)
            .expect("second pane")
            .bell
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: second,
                text: "true\n".to_owned(),
            },
        )
        .expect("type into the belled pane");
    assert!(!pane_bell(&take_reliable_messages(&mailbox), second));

    shared.raise_pane_bell(second);
    assert_eq!(bell_events(&take_reliable_messages(&mailbox)), vec![second]);
}

#[test]
fn bell_action_and_visual_bell_fan_out_from_the_session_active_window() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let (first_client, mut context, _first, second) = belled_session(&shared, &first_mailbox);
    let session = context.session.expect("session");
    let (second_window, second_index) = {
        let inner = shared.inner.lock();
        let window = inner
            .engine
            .state
            .window_for_pane(second)
            .expect("second window");
        (window, inner.engine.state.windows[&window].index)
    };
    let second_mailbox = OutboundMailbox::new();
    let control_mailbox = OutboundMailbox::new();
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&second_mailbox),
    );
    let (control_client, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(second_client, session)
        .expect("attach second client");
    shared
        .attach(control_client, session)
        .expect("attach control client");
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(second_client)
            .focused_window
            .replace(second_window);
        inner
            .client_entry(control_client)
            .focused_window
            .replace(second_window);
    }
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&control_mailbox);

    for (name, value) in [("bell-action", "other"), ("visual-bell", "both")] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", name, value]),
            )
            .expect("configure bell option");
    }
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&control_mailbox);
    shared.raise_pane_bell(second);
    let first = take_reliable_messages(&first_mailbox);
    assert_eq!(bell_events(&first), vec![second]);
    assert_eq!(
        bell_messages(&first),
        vec![(Some(second), format!("Bell in window {second_index}"), 750,)]
    );
    let second_messages = take_reliable_messages(&second_mailbox);
    assert_eq!(bell_events(&second_messages), vec![second]);
    assert_eq!(
        bell_messages(&second_messages),
        vec![(Some(second), format!("Bell in window {second_index}"), 750,)]
    );
    let control_messages = take_reliable_messages(&control_mailbox);
    assert!(bell_events(&control_messages).is_empty());
    assert!(bell_messages(&control_messages).is_empty());

    for (name, value) in [("bell-action", "current"), ("visual-bell", "on")] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", name, value]),
            )
            .expect("reconfigure bell option");
    }
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&control_mailbox);
    shared.raise_pane_bell(second);
    let first_messages = take_reliable_messages(&first_mailbox);
    assert!(bell_events(&first_messages).is_empty());
    assert!(bell_messages(&first_messages).is_empty());
    let second_messages = take_reliable_messages(&second_mailbox);
    assert!(bell_events(&second_messages).is_empty());
    assert!(bell_messages(&second_messages).is_empty());
    let control_messages = take_reliable_messages(&control_mailbox);
    assert!(bell_events(&control_messages).is_empty());
    assert!(bell_messages(&control_messages).is_empty());

    let first_pane = context.pane.expect("active pane");
    shared.raise_pane_bell(first_pane);
    for messages in [
        take_reliable_messages(&first_mailbox),
        take_reliable_messages(&second_mailbox),
    ] {
        assert!(bell_events(&messages).is_empty());
        assert_eq!(
            bell_messages(&messages),
            vec![(Some(first_pane), "Bell in current window".to_owned(), 750)]
        );
    }
    let control_messages = take_reliable_messages(&control_mailbox);
    assert!(bell_events(&control_messages).is_empty());
    assert!(bell_messages(&control_messages).is_empty());
    assert_eq!(
        client_focused_window_for_attachment(&shared.inner.lock(), first_client),
        context.window
    );
}

#[test]
fn alert_messages_fan_out_with_owned_lifecycle_and_bounded_log() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let (first_client, mut context, first, second) = belled_session(&shared, &first_mailbox);
    let session = context.session.expect("session");
    let second_mailbox = OutboundMailbox::new();
    let control_mailbox = OutboundMailbox::new();
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("second-alert".to_owned()),
        None,
        Arc::clone(&second_mailbox),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-alert".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(second_client, session)
        .expect("attach second client");
    shared
        .attach(control, session)
        .expect("attach control client");

    for arguments in [
        ["-g", "display-time", "5000"],
        ["-g", "bell-action", "any"],
        ["-g", "visual-bell", "on"],
        ["-s", "message-limit", "2"],
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", arguments),
            )
            .expect("configure alert lifecycle");
    }
    for (client, text) in [(first_client, "first"), (second_client, "second")] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-message", ["-N", "-d", "5000", text]),
            )
            .expect("seed sticky message");
    }
    let first_old = armed_client_message(&shared, first_client);
    let second_old = armed_client_message(&shared, second_client);
    {
        let mut inner = shared.inner.lock();
        assert!(
            inner
                .client(first_client)
                .is_some_and(|c| c.message_ignore_keys)
        );
        assert!(
            inner
                .client(second_client)
                .is_some_and(|c| c.message_ignore_keys)
        );
        inner.history.message_log.clear();
        push_server_message(&mut inner, "evicted".to_owned());
    }
    let message_number = shared.inner.lock().history.next_message_number;
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&control_mailbox);
    drain_terminal_lane(&first_mailbox);
    drain_terminal_lane(&second_mailbox);

    shared.raise_pane_bell(second);

    let first_messages = take_reliable_messages(&first_mailbox);
    let second_messages = take_reliable_messages(&second_mailbox);
    let control_messages = take_reliable_messages(&control_mailbox);
    let first_timed = timed_message_events(&first_messages);
    let second_timed = timed_message_events(&second_messages);
    assert_eq!(first_timed.len(), 1);
    assert_eq!(second_timed.len(), 1);
    let first_record = armed_client_message(&shared, first_client);
    let second_record = armed_client_message(&shared, second_client);
    assert_eq!(first_timed[0].3, first_record.token);
    assert_eq!(second_timed[0].3, second_record.token);
    assert_ne!(first_record.token, second_record.token);
    assert!(first_record.freeze && first_record.deadline.is_some());
    assert!(second_record.freeze && second_record.deadline.is_some());
    assert_eq!(
        message_lifecycle_events(&first_messages),
        vec![(true, first_record.token), (false, first_old.token)]
    );
    assert_eq!(
        message_lifecycle_events(&second_messages),
        vec![(true, second_record.token), (false, second_old.token)]
    );
    assert!(timed_message_events(&control_messages).is_empty());
    assert!(cleared_message_ids(&control_messages).is_empty());
    {
        let inner = shared.inner.lock();
        assert!(inner.client(control).is_none_or(|c| c.message.is_none()));
        assert!(
            !inner
                .client(first_client)
                .is_some_and(|c| c.message_ignore_keys)
        );
        assert!(
            !inner
                .client(second_client)
                .is_some_and(|c| c.message_ignore_keys)
        );
        assert_eq!(inner.history.next_message_number, message_number + 2);
        assert_eq!(
            inner
                .history
                .message_log
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>(),
            [
                format!("device-{} message: Bell in window 1", first_client.0),
                "second-alert message: Bell in window 1".to_owned(),
            ]
        );
    }
    assert!(first_mailbox.state.lock().terminals.is_empty());
    assert!(second_mailbox.state.lock().terminals.is_empty());

    let stale = ClientMessageDeadline {
        client: first_client,
        token: first_old.token,
        deadline: first_old.deadline.expect("old deadline"),
    };
    shared.expire_client_message(stale, stale.deadline);
    assert_eq!(armed_client_message(&shared, first_client), first_record);
    assert!(cleared_message_ids(&take_reliable_messages(&first_mailbox)).is_empty());
    assert!(first_mailbox.state.lock().terminals.is_empty());

    wait_for_viewport(
        &Arc::clone(&shared.inner.lock().terminals[&first]),
        TerminalViewId(first_client.0),
        "the first client's view never rendered",
        |_| true,
    );
    let live = ClientMessageDeadline {
        client: first_client,
        token: first_record.token,
        deadline: first_record.deadline.expect("live deadline"),
    };
    shared.expire_client_message(live, live.deadline);
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&first_mailbox)),
        vec![first_record.token]
    );
    assert_eq!(
        first_mailbox
            .state
            .lock()
            .terminals
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [first]
    );
}

#[test]
fn zero_duration_alert_replacement_waits_for_input_past_the_old_deadline() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "zero-alert", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for arguments in [
        ["-g", "display-time", "0"],
        ["-g", "bell-action", "any"],
        ["-g", "visual-bell", "on"],
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", arguments),
            )
            .expect("configure zero-duration alert");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "old"]),
        )
        .expect("seed positive message");
    let old = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);

    shared.raise_pane_bell(pane);
    let messages = take_reliable_messages(&mailbox);
    let current = armed_client_message(&shared, client);
    assert_eq!(current.deadline, None);
    assert!(current.freeze);
    assert_eq!(
        message_lifecycle_events(&messages),
        vec![(true, current.token), (false, old.token)]
    );

    let stale = ClientMessageDeadline {
        client,
        token: old.token,
        deadline: old.deadline.expect("old deadline"),
    };
    shared.expire_client_message(stale, stale.deadline);
    assert_eq!(armed_client_message(&shared, client), current);
    assert!(cleared_message_ids(&take_reliable_messages(&mailbox)).is_empty());

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("q"),
                text_follows: false,
            },
        )
        .expect("dismiss zero-duration alert");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        vec![current.token]
    );
}

#[test]
fn alert_expiry_does_not_resume_while_a_prompt_still_freezes() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "alert-prompt", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for arguments in [
        ["-g", "display-time", "5000"],
        ["-g", "bell-action", "any"],
        ["-g", "visual-bell", "on"],
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", arguments),
            )
            .expect("configure alert");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "hold"]),
        )
        .expect("open freezing prompt");
    assert!(prompt_state(&shared, client).is_some());
    take_reliable_messages(&mailbox);
    drain_terminal_lane(&mailbox);

    shared.raise_pane_bell(pane);
    let armed = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);
    drain_terminal_lane(&mailbox);
    let deadline = ClientMessageDeadline {
        client,
        token: armed.token,
        deadline: armed.deadline.expect("alert deadline"),
    };
    shared.expire_client_message(deadline, deadline.deadline);
    assert!(prompt_state(&shared, client).is_some());
    assert!(client_terminal_publication_frozen(
        &shared.inner.lock(),
        client
    ));
    assert!(mailbox.state.lock().terminals.is_empty());

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .expect("close prompt");
    assert!(mailbox.state.lock().terminals.contains_key(&pane));
}

#[test]
fn bell_activity_and_silence_messages_share_the_client_lifecycle() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, mut context, _first, second) = belled_session(&shared, &mailbox);
    let second_window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(second)
        .expect("second window");
    let second_index = shared.inner.lock().engine.state.windows[&second_window].index;
    for (command, arguments) in [
        ("set-option", ["-g", "display-time", "5000"]),
        ("set-option", ["-g", "bell-action", "any"]),
        ("set-option", ["-g", "visual-bell", "on"]),
        ("set-window-option", ["-g", "monitor-activity", "on"]),
        ("set-option", ["-g", "activity-action", "any"]),
        ("set-option", ["-g", "visual-activity", "on"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(command, arguments),
            )
            .expect("configure alert producer");
    }
    shared.inner.lock().history.message_log.clear();
    take_reliable_messages(&mailbox);

    shared.raise_pane_bell(second);
    let bell = timed_message_events(&take_reliable_messages(&mailbox));
    assert_eq!(bell.len(), 1);
    assert!(bell[0].1.starts_with("Bell in "));
    let bell_record = armed_client_message(&shared, client);
    assert_eq!(bell[0].3, bell_record.token);

    shared.raise_window_activity(second_window, second);
    let activity_messages = take_reliable_messages(&mailbox);
    let activity = timed_message_events(&activity_messages);
    assert_eq!(activity.len(), 1);
    assert!(activity[0].1.starts_with("Activity in "));
    let activity_record = armed_client_message(&shared, client);
    assert_eq!(activity[0].3, activity_record.token);
    assert_eq!(
        message_lifecycle_events(&activity_messages),
        vec![(true, activity_record.token), (false, bell_record.token)]
    );

    for (command, arguments) in [
        ("set-window-option", ["-g", "monitor-silence", "1"]),
        ("set-option", ["-g", "silence-action", "any"]),
        ("set-option", ["-g", "visual-silence", "on"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(command, arguments),
            )
            .expect("configure silence producer");
    }
    take_reliable_messages(&mailbox);
    let silence_deadline = shared.inner.lock().silence_deadlines[&second_window];
    shared.expire_window_silence(silence_deadline, silence_deadline.deadline);
    let silence_messages = take_reliable_messages(&mailbox);
    let silence = timed_message_events(&silence_messages);
    assert_eq!(silence.len(), 1);
    assert!(silence[0].1.starts_with("Silence in "));
    let silence_record = armed_client_message(&shared, client);
    assert_eq!(silence[0].3, silence_record.token);
    assert_eq!(
        message_lifecycle_events(&silence_messages),
        vec![(true, silence_record.token), (false, activity_record.token)]
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>(),
        [
            format!("device-{} message: Bell in window {second_index}", client.0),
            format!(
                "device-{} message: Activity in window {second_index}",
                client.0
            ),
            format!(
                "device-{} message: Silence in window {second_index}",
                client.0
            ),
        ]
    );
}

#[test]
fn monitor_bell_off_silences_the_whole_bell_path() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (_client, mut context, _first, second) = belled_session(&shared, &mailbox);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "monitor-bell", "off"]),
        )
        .expect("disable monitor-bell");
    take_reliable_messages(&mailbox);
    shared.raise_pane_bell(second);
    let messages = take_reliable_messages(&mailbox);
    assert!(bell_events(&messages).is_empty());
    assert!(bell_messages(&messages).is_empty());
    assert!(
        !shared
            .inner
            .lock()
            .engine
            .state
            .pane(second)
            .expect("second pane")
            .bell
    );
}

#[test]
fn activity_flag_rides_the_monitor_gate_suppression_and_selection_clear() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, mut context, first, second) = belled_session(&shared, &mailbox);
    let (first_window, second_window) = {
        let inner = shared.inner.lock();
        (
            inner.engine.state.window_for_pane(first).expect("first"),
            inner.engine.state.window_for_pane(second).expect("second"),
        )
    };
    shared.raise_window_activity(second_window, second);
    assert!(!window_activity_flag(&shared, second_window));

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "monitor-activity", "on"]),
        )
        .expect("enable monitor-activity");
    take_reliable_messages(&mailbox);
    shared.raise_window_activity(second_window, second);
    assert!(window_activity_flag(&shared, second_window));

    shared.raise_window_activity(first_window, first);
    assert!(!window_activity_flag(&shared, first_window));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", ":1"]),
        )
        .expect("select the flagged window");
    assert!(!window_activity_flag(&shared, second_window));
}

#[test]
fn activity_action_and_visual_activity_fan_out_with_pin_texts() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (_client, mut context, _first, second) = belled_session(&shared, &mailbox);
    let (second_window, second_index) = {
        let inner = shared.inner.lock();
        let window = inner.engine.state.window_for_pane(second).expect("second");
        (window, inner.engine.state.windows[&window].index)
    };
    for (command, arguments) in [
        ("set-window-option", ["-g", "monitor-activity", "on"]),
        ("set-option", ["-g", "activity-action", "any"]),
        ("set-option", ["-g", "visual-activity", "both"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(command, arguments),
            )
            .expect("configure activity alerts");
    }
    take_reliable_messages(&mailbox);
    shared.raise_window_activity(second_window, second);
    let messages = take_reliable_messages(&mailbox);
    assert_eq!(bell_events(&messages), vec![second]);
    assert_eq!(
        bell_messages(&messages),
        vec![(
            Some(second),
            format!("Activity in window {second_index}"),
            750,
        )]
    );

    shared.raise_window_activity(second_window, second);
    let latched = take_reliable_messages(&mailbox);
    assert!(bell_events(&latched).is_empty());
    assert!(bell_messages(&latched).is_empty());
}

#[test]
fn monitor_silence_arms_resets_expires_and_cancels_deterministically() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (_client, mut context, _first, second) = belled_session(&shared, &mailbox);
    let second_window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(second)
        .expect("second window");
    assert!(shared.inner.lock().silence_deadlines.is_empty());

    for (command, arguments) in [
        ("set-window-option", ["-g", "monitor-silence", "1"]),
        ("set-option", ["-g", "silence-action", "any"]),
        ("set-option", ["-g", "visual-silence", "on"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(command, arguments),
            )
            .expect("configure silence alerts");
    }
    let original_deadlines = {
        let inner = shared.inner.lock();
        assert_eq!(inner.silence_deadlines.len(), 2);
        inner.silence_deadlines.clone()
    };
    take_reliable_messages(&mailbox);

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "monitor-silence", "1"]),
        )
        .expect("repeat monitor-silence");
    let replacement_deadlines = shared.inner.lock().silence_deadlines.clone();
    assert_eq!(
        replacement_deadlines.keys().collect::<BTreeSet<_>>(),
        original_deadlines.keys().collect::<BTreeSet<_>>()
    );
    for (window, original) in &original_deadlines {
        assert_ne!(replacement_deadlines[window].token, original.token);
    }
    let original_deadline = original_deadlines[&second_window];
    let deadline = replacement_deadlines[&second_window];
    shared.expire_window_silence(original_deadline, original_deadline.deadline);
    assert!(!window_silence_flag(&shared, second_window));

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-u", "-t", ":1", "monitor-silence"]),
        )
        .expect("unset missing local monitor-silence");
    assert!(shared.inner.lock().silence_deadlines == replacement_deadlines);
    assert!(
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-window-option", ["-g", "-o", "monitor-silence", "1"],),
            )
            .is_err()
    );
    assert!(shared.inner.lock().silence_deadlines == replacement_deadlines);

    let stale = SilenceDeadline {
        token: deadline.token.wrapping_add(1),
        ..deadline
    };
    shared.expire_window_silence(stale, deadline.deadline);
    assert!(!window_silence_flag(&shared, second_window));

    shared.expire_window_silence(deadline, deadline.deadline);
    assert!(window_silence_flag(&shared, second_window));
    let second_index = shared.inner.lock().engine.state.windows[&second_window].index;
    let messages = take_reliable_messages(&mailbox);
    assert_eq!(
        bell_messages(&messages),
        vec![(
            Some(second),
            format!("Silence in window {second_index}"),
            750,
        )]
    );
    let rearmed = shared.inner.lock().silence_deadlines[&second_window];
    assert_ne!(rearmed.token, deadline.token);

    shared.expire_window_silence(rearmed, rearmed.deadline);
    let latched = take_reliable_messages(&mailbox);
    assert!(bell_messages(&latched).is_empty());

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "monitor-silence", "0"]),
        )
        .expect("disable monitor-silence");
    assert!(shared.inner.lock().silence_deadlines.is_empty());
}

#[test]
fn allow_set_title_gates_terminal_title_synchronization() {
    let shared = Arc::new(Shared::new(1));
    let (_, pane, terminal) = output_view_session_fixture(&shared, "title-gate", "first");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("title context");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-p", "-t", &pane.to_string(), "allow-set-title", "off"],
            ),
        )
        .expect("disable terminal titles");
    shared.synchronize_pane_title(pane, &terminal, "blocked title", true);
    assert_ne!(
        shared.inner.lock().engine.state.pane(pane).unwrap().title,
        "blocked title"
    );
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-p", "-t", &pane.to_string(), "allow-set-title", "on"],
            ),
        )
        .expect("enable terminal titles");
    shared.synchronize_pane_title(pane, &terminal, "allowed title", true);
    assert_eq!(
        shared.inner.lock().engine.state.pane(pane).unwrap().title,
        "allowed title"
    );
}

#[test]
fn select_pane_title_survives_shell_integration_titles_until_a_program_retitles() {
    let shared = Arc::new(Shared::new(1));
    let (_, pane, terminal) = output_view_session_fixture(&shared, "title-pin", "first");
    let title = |shared: &Arc<Shared>| {
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .unwrap()
            .title
            .clone()
    };
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("title context");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &pane.to_string(), "-T", "mytitle"]),
        )
        .expect("set the pane title");
    shared.synchronize_pane_title(pane, &terminal, "bash", false);
    shared.synchronize_pane_title(pane, &terminal, "ls -la", false);
    assert_eq!(title(&shared), "mytitle");
    shared.synchronize_pane_title(pane, &terminal, "vim", true);
    assert_eq!(title(&shared), "vim");
    shared.synchronize_pane_title(pane, &terminal, "bash", false);
    assert_eq!(title(&shared), "bash");
}

#[test]
fn selecting_a_belled_window_clears_it() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, mut context, _first, second) = belled_session(&shared, &mailbox);

    shared.raise_pane_bell(second);
    assert!(pane_bell(&take_reliable_messages(&mailbox), second));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", ":0"]),
        )
        .expect("reselect the active window");
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(second)
            .expect("second pane")
            .bell
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", ":1"]),
        )
        .expect("select the belled window");
    assert!(!pane_bell(&take_reliable_messages(&mailbox), second));
}

#[cfg(unix)]
#[test]
fn attaching_clears_current_window_alerts_without_suppressing_later_bells() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, context, first, second) = belled_session(&shared, &mailbox);
    let session = context.session.expect("session");
    let (first_window, window, terminal) = {
        let inner = shared.inner.lock();
        let first_window = inner
            .engine
            .state
            .window_for_pane(first)
            .expect("first window");
        let window = inner.engine.state.window_for_pane(second).expect("window");
        (first_window, window, Arc::clone(&inner.terminals[&second]))
    };

    ring_terminal_and_wait_for_bell(&shared, &mailbox, second, &terminal, true);
    ring_terminal_and_wait_for_bell(&shared, &mailbox, second, &terminal, true);
    {
        let mut inner = shared.inner.lock();
        inner.engine.state.set_pane_bell(first, true);
        inner
            .engine
            .state
            .set_window_activity_flag(first_window, true);
        inner
            .engine
            .state
            .set_window_silence_flag(first_window, true);
        inner
            .engine
            .state
            .session_mut(session)
            .unwrap()
            .active_window = window;
        inner.engine.state.set_window_activity_flag(window, true);
        inner.engine.state.set_window_silence_flag(window, true);
    }

    let snapshot = shared.attach(client, session).expect("reattach session");
    let snapshot_window = snapshot.sessions[0]
        .windows
        .iter()
        .find(|candidate| candidate.id == window)
        .expect("snapshot window");
    assert!(!snapshot_window.panes[&second].bell);
    let snapshot_first_window = snapshot.sessions[0]
        .windows
        .iter()
        .find(|candidate| candidate.id == first_window)
        .expect("inactive snapshot window");
    assert!(snapshot_first_window.panes[&first].bell);
    {
        let inner = shared.inner.lock();
        assert!(!inner.engine.state.pane(second).expect("pane").bell);
        assert!(!inner.engine.state.windows[&window].activity_flag);
        assert!(!inner.engine.state.windows[&window].silence_flag);
        assert!(inner.engine.state.pane(first).expect("inactive pane").bell);
        assert!(inner.engine.state.windows[&first_window].activity_flag);
        assert!(inner.engine.state.windows[&first_window].silence_flag);
    }

    take_reliable_messages(&mailbox);
    ring_terminal_and_wait_for_bell(&shared, &mailbox, second, &terminal, false);
}

#[cfg(unix)]
#[test]
fn activating_a_belled_window_allows_later_bells() {
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
            &CommandInvocation::new("new-session", ["-s", "bell-window"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let first_window = context.window.expect("first window");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-d", "-n", "alert"]),
        )
        .expect("background window");
    let (alert_window, alert_pane, terminal) = {
        let inner = shared.inner.lock();
        let alert_window = inner.engine.state.sessions[&session]
            .windows
            .iter()
            .copied()
            .find(|window| *window != first_window)
            .expect("alert window");
        let alert_pane = inner.engine.state.windows[&alert_window].active_pane;
        (
            alert_window,
            alert_pane,
            Arc::clone(&inner.terminals[&alert_pane]),
        )
    };
    assert_eq!(
        shared.inner.lock().engine.state.sessions[&session].active_window,
        first_window
    );
    take_reliable_messages(&mailbox);

    ring_terminal_and_wait_for_bell(&shared, &mailbox, alert_pane, &terminal, true);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("next-window", ["-a"]),
        )
        .expect("activate alerted window");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.sessions[&session].active_window,
            alert_window
        );
        assert!(
            !inner
                .engine
                .state
                .pane(alert_pane)
                .expect("alert pane")
                .bell
        );
    }
    take_reliable_messages(&mailbox);

    ring_terminal_and_wait_for_bell(&shared, &mailbox, alert_pane, &terminal, false);
}

#[cfg(unix)]
#[test]
fn kill_session_dash_c_allows_later_bells() {
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
            &CommandInvocation::new("new-session", ["-s", "bell-clear"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    take_reliable_messages(&mailbox);
    ring_terminal_and_wait_for_bell(&shared, &mailbox, pane, &terminal, false);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("kill-session", ["-C", "-t", &session.to_string()]),
        )
        .expect("clear session alerts");
    assert!(
        !shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .expect("pane")
            .bell
    );
    take_reliable_messages(&mailbox);
    ring_terminal_and_wait_for_bell(&shared, &mailbox, pane, &terminal, false);
}
