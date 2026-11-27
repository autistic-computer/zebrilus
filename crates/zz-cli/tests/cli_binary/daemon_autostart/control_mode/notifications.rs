use super::*;

#[test]
fn control_notifications_layout_and_output_follow_the_live_socket_stream() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "watched", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "watched"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }

    let added = fixture.run(&[
        "new-window",
        "-d",
        "-t",
        "watched",
        "-n",
        "added",
        "exec /bin/cat",
    ]);
    assert_eq!(added.status.code(), Some(0));
    let foreign = fixture.run(&["new-session", "-d", "-s", "foreign", "exec /bin/cat"]);
    assert_eq!(foreign.status.code(), Some(0));
    let split = fixture.run(&[
        "split-window",
        "-h",
        "-d",
        "-t",
        "watched:0",
        "exec /bin/cat",
    ]);
    assert_eq!(split.status.code(), Some(0));
    let typed = fixture.run(&["send-keys", "-l", "-t", "watched:0.0", "CONTROL_OUTPUT"]);
    assert_eq!(typed.status.code(), Some(0));
    let entered = fixture.run(&["send-keys", "-t", "watched:0.0", "Enter"]);
    assert_eq!(entered.status.code(), Some(0));

    stdin
        .write_all(b"run-shell \"sleep 1\"\n")
        .expect("start slow control command");
    stdin.flush().expect("flush slow control command");
    thread::sleep(Duration::from_millis(100));
    let renamed = fixture.run(&["rename-window", "-t", "watched:0", "during"]);
    assert_eq!(renamed.status.code(), Some(0));
    thread::sleep(Duration::from_millis(1100));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert!(
        stream
            .blocks
            .iter()
            .all(|block| { block.payload.iter().all(|line| !line.starts_with('%')) })
    );

    let notifications = stream
        .outside
        .iter()
        .filter(|line| {
            !line.starts_with("%output ")
                && !line.starts_with("%window-renamed @")
                && line.as_str() != "%exit"
        })
        .collect::<Vec<_>>();
    let position = |predicate: &dyn Fn(&str) -> bool| {
        notifications
            .iter()
            .position(|line| predicate(line))
            .unwrap_or_else(|| panic!("missing startup notification: {notifications:?}"))
    };
    let added = position(&|line| line.starts_with("%window-add @"));
    let listed = position(&|line| line == "%sessions-changed");
    let switched =
        position(&|line| line.starts_with("%session-changed $") && line.ends_with(" watched"));
    assert!(added < listed && listed < switched, "{notifications:?}");
    assert!(
        notifications
            .iter()
            .any(|line| line.starts_with("%window-add @"))
    );
    assert!(
        notifications
            .iter()
            .any(|line| line.starts_with("%unlinked-window-add @"))
    );
    assert!(notifications.iter().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.first() == Some(&"%layout-change")
            && fields.get(2).is_some_and(|layout| checked_layout(layout))
            && fields.get(3).is_some_and(|layout| checked_layout(layout))
    }));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line.starts_with("%window-renamed @") && line.ends_with(" during"))
    );
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line.starts_with("%output %") && line.contains("CONTROL_OUTPUT"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn refresh_client_pane_states_gate_the_live_control_stream() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "flow-state", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "flow-state"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let pane_output = fixture.run(&["list-panes", "-t", "flow-state", "-F", "#{pane_id}"]);
    assert!(pane_output.status.success());
    let pane = String::from_utf8(pane_output.stdout)
        .expect("pane id")
        .trim()
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(10);
    while fixture
        .run(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command}",
        ])
        .stdout
        != b"cat\n"
    {
        assert!(Instant::now() < deadline, "cat did not start");
        thread::sleep(Duration::from_millis(10));
    }
    fn control(stdin: &mut ChildStdin, pane: &str, state: &str) {
        writeln!(stdin, "refresh-client -A '{pane}:{state}'").expect("write pane state");
        stdin.flush().expect("flush pane state");
        thread::sleep(Duration::from_millis(100));
    }
    fn type_line(stdin: &mut ChildStdin, pane: &str, text: &str) {
        writeln!(stdin, "send-keys -l -t {pane} {text}").expect("write typed text");
        writeln!(stdin, "send-keys -t {pane} Enter").expect("write typed enter");
        stdin.flush().expect("flush typed text");
        thread::sleep(Duration::from_millis(150));
    }

    control(&mut stdin, &pane, "off");
    type_line(&mut stdin, &pane, "HIDDEN_WHILE_OFF");
    control(&mut stdin, &pane, "on");
    type_line(&mut stdin, &pane, "VISIBLE_AFTER_ON");
    thread::sleep(Duration::from_millis(400));
    control(&mut stdin, &pane, "pause");
    control(&mut stdin, &pane, "pause");
    type_line(&mut stdin, &pane, "HIDDEN_WHILE_PAUSED");
    control(&mut stdin, &pane, "continue");
    control(&mut stdin, &pane, "continue");
    type_line(&mut stdin, &pane, "VISIBLE_AFTER_CONTINUE");
    thread::sleep(Duration::from_millis(400));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    let output_lines = stream
        .outside
        .iter()
        .filter(|line| line.starts_with("%output "))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!output_lines.contains("HIDDEN_WHILE_OFF"));
    assert!(!output_lines.contains("HIDDEN_WHILE_PAUSED"));
    assert!(output_lines.contains("VISIBLE_AFTER_ON"));
    assert!(output_lines.contains("VISIBLE_AFTER_CONTINUE"));
    let guarded = stream
        .blocks
        .iter()
        .flat_map(|block| block.payload.iter())
        .cloned()
        .collect::<Vec<_>>();
    for state in ["pause", "continue"] {
        let line = format!("%{state} {pane}");
        assert_eq!(
            stream
                .outside
                .iter()
                .filter(|candidate| candidate.as_str() == line)
                .count(),
            1
        );
        assert_eq!(
            guarded
                .iter()
                .filter(|candidate| candidate.as_str() == line)
                .count(),
            0
        );
    }
}

#[test]
fn refresh_client_c_sizes_a_control_target_and_display_menu_returns_at_once() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (control, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "sizing", "exec /bin/cat"]);
    let target = wait_for_control_clients(&fixture, 1, "refresh-client sizing")
        .into_iter()
        .next()
        .expect("control client list row");
    let menu_args = [
        "display-menu",
        "-c",
        target.as_str(),
        "Item",
        "i",
        "display-message chosen",
    ];
    let no_size = fixture.run(&menu_args);
    assert_eq!(no_size.status.code(), Some(0));

    stdin
        .write_all(b"refresh-client -C 100x50\n")
        .expect("set control size");
    stdin.flush().expect("flush control size");
    let expected_width = format!("{target}:100");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let width = fixture.run(&["list-clients", "-F", "#{client_name}:#{client_width}"]);
        if String::from_utf8_lossy(&width.stdout)
            .lines()
            .any(|line| line == expected_width)
        {
            break;
        }
        assert!(Instant::now() < deadline, "control size did not settle");
        thread::sleep(Duration::from_millis(10));
    }

    let sized = fixture.run(&menu_args);
    assert_eq!(sized.status.code(), Some(0));
    stdin.write_all(b"\n").expect("detach sized control");
    drop(stdin);
    let control_output = control.wait_with_output().expect("wait for control client");
    assert_eq!(control_output.status.code(), Some(0));
}

#[test]
fn refresh_client_b_reports_initial_change_and_exact_removal() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "subscribed", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fixture
            .run(&["has-session", "-t", "subscribed"])
            .status
            .success()
        {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }

    stdin
        .write_all(b"refresh-client -B 'watch::#{session_name}'\n")
        .expect("add subscription");
    stdin.flush().expect("flush subscription");
    thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        fixture
            .run(&["rename-session", "-t", "subscribed", "renamed"])
            .status
            .code(),
        Some(0)
    );
    thread::sleep(Duration::from_millis(1200));
    stdin
        .write_all(b"refresh-client -B watch\nrename-session -t renamed removed\n")
        .expect("remove subscription and rename session");
    stdin.flush().expect("flush removal");
    thread::sleep(Duration::from_millis(1200));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    let subscriptions = stream
        .outside
        .iter()
        .filter(|line| line.starts_with("%subscription-changed watch "))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(subscriptions.len(), 2);
    assert!(subscriptions[0].ends_with(" - - - : subscribed"));
    assert!(subscriptions[1].ends_with(" - - - : renamed"));
    assert!(
        subscriptions
            .iter()
            .all(|line| !line.ends_with(" : removed"))
    );
}

#[test]
fn pause_after_emits_extended_output_then_pauses_a_slow_control_client() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "pause-after", "exec /bin/sh"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "pause-after"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let pane_output = fixture.run(&["list-panes", "-t", "pause-after", "-F", "#{pane_id}"]);
    assert!(pane_output.status.success());
    let pane = String::from_utf8(pane_output.stdout)
        .expect("pane id")
        .trim()
        .to_owned();
    stdin
        .write_all(b"refresh-client -f pause-after=1\n")
        .expect("enable pause-after");
    stdin.flush().expect("flush pause-after");
    thread::sleep(Duration::from_millis(100));
    let marker = fixture.socket.with_extension("flooded");
    let flood = format!("yes x | head -c 1048576; touch {}", marker.display());
    assert!(
        fixture
            .run(&["send-keys", "-l", "-t", &pane, &flood])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&["send-keys", "-t", &pane, "Enter"])
            .status
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "flood never completed");
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(1500));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("wait for paused control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert!(stream.outside.iter().any(|line| {
        let fields = line.splitn(5, ' ').collect::<Vec<_>>();
        fields.first() == Some(&"%extended-output")
            && fields.get(1) == Some(&pane.as_str())
            && fields.get(2).is_some_and(|age| age.parse::<u64>().is_ok())
            && fields.get(3) == Some(&":")
    }));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%pause {pane}"))
    );
}

fn checked_layout(layout: &str) -> bool {
    layout.split_once(',').is_some_and(|(checksum, body)| {
        checksum.len() == 4
            && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !body.is_empty()
    })
}
