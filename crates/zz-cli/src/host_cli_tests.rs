use std::io;

use zz_protocol::RawText;

use zz_daemon_client::DaemonError;
use zz_protocol::{CommandInvocation, ServerError};

#[test]
fn a_host_named_after_this_machine_runs_here_unless_configured() {
    let configured = [zz_daemon_client::HostEntry {
        name: "ubuntu".to_owned(),
        endpoint: zz_daemon_client::Endpoint::parse("ssh://ubuntu").expect("endpoint"),
    }];
    assert!(super::unconfigured_local_name(
        &[],
        Some("ubuntu"),
        "ubuntu"
    ));
    assert!(super::unconfigured_local_name(&[], Some("xps.lan"), "xps"));
    assert!(super::unconfigured_local_name(&[], Some("XPS"), "xps"));
    assert!(!super::unconfigured_local_name(
        &configured,
        Some("ubuntu"),
        "ubuntu"
    ));
    assert!(!super::unconfigured_local_name(&[], Some("ubuntu"), "xps"));
    assert!(!super::unconfigured_local_name(&[], None, "ubuntu"));
}

#[test]
fn a_host_without_a_current_session_says_how_to_make_one() {
    let error = DaemonError::Server(ServerError::SessionNotFound("current session".to_owned()));
    assert_eq!(
        super::host_command_error_message("xps", &error),
        "can't find session: current session\nzz --host xps: name the session with -t NAME \
         (zz --host xps list-sessions, or make one with zz --host xps new-session -d -s NAME)"
    );
}

#[test]
fn hints_from_a_host_name_it_in_their_commands() {
    for (relayed, shown) in [
        (
            "agent-send: read %5's reply with: zz agent-send -t %5 --wait ''",
            "agent-send: read %5's reply with: zz --host macbook agent-send -t %5 --wait ''",
        ),
        (
            "%5 has no running turn; read its last reply with: zz capture-pane -p -t %5",
            "%5 has no running turn; read its last reply with: zz --host macbook capture-pane -p -t %5",
        ),
        (
            "zz --host macbook: name the pane with -t %N (zz --host macbook list-panes -a)",
            "zz --host macbook: name the pane with -t %N (zz --host macbook list-panes -a)",
        ),
        (
            "fizz new-pane, zz/config, zz nonsense",
            "fizz new-pane, zz/config, zz nonsense",
        ),
    ] {
        assert_eq!(super::with_host_commands("macbook", relayed), shown);
    }
    let missing = DaemonError::Server(ServerError::InvalidCommand(
        "%5 is an agent chat; send with: zz agent-send -t %5 TEXT".to_owned(),
    ));
    assert_eq!(
        super::host_command_error_message("macbook", &missing),
        "%5 is an agent chat; send with: zz --host macbook agent-send -t %5 TEXT"
    );
}

#[test]
fn verbs_that_act_on_a_pane_window_or_session_need_a_target_over_a_host() {
    let chain = |words: &[&str]| {
        super::split_command_chain(&words.iter().copied().map(RawText::from).collect::<Vec<_>>())
    };
    let make_pane = "or make one with zz --host xps new-session -d -P -F '#{pane_id}')";
    let make_window = "or make one with zz --host xps new-session -d -P -F '#{window_id}')";
    let pane = format!(
        "zz --host xps: name the pane with -t %N (zz --host xps list-panes -a, {make_pane}"
    );
    let pane = pane.as_str();
    let run_pane =
        format!("{pane}; a command that needs no pane runs with zz --host xps run-shell 'COMMAND'");
    let run_pane = run_pane.as_str();
    let window = format!(
        "zz --host xps: name the window with -t @N (zz --host xps list-windows -a, {make_window}"
    );
    let window = window.as_str();
    let session = "zz --host xps: name the session with -t NAME (zz --host xps list-sessions, \
                   or make one with zz --host xps new-session -d -s NAME)";
    let pane_source = format!(
        "zz --host xps: name the pane with -s %N (zz --host xps list-panes -a, {make_pane}"
    );
    let pane_source = pane_source.as_str();
    let window_source = format!(
        "zz --host xps: name the window with -s @N (zz --host xps list-windows -a, {make_window}"
    );
    let window_source = window_source.as_str();
    for (words, refusal) in [
        (&["send-keys", "ls", "Enter"][..], pane),
        (&["send", "-l", "x"][..], pane),
        (&["send-text", "hi"][..], pane),
        (&["run-pane", "--", "make"][..], run_pane),
        (&["run-pane", "--timeout=5", "--", "make"][..], run_pane),
        (
            &["run-pane", "--timeout=60", "docker", "run", "-t", "x"][..],
            run_pane,
        ),
        (&["send-text", "--timeout=5", "hi"][..], pane),
        (&["agent-send", "--on-block=deny", "hi"][..], pane),
        (&["agent-respond", "--option=allow"][..], pane),
        (&["run-pane", "--bogus=1", "--", "make"][..], run_pane),
        (&["pasteb", "-b", "x"][..], pane),
        (&["agent-send", "--wait", "hi"][..], pane),
        (&["list-panes", ";", "send-keys", "x"][..], pane),
        (&["kill-pane"][..], pane),
        (&["killp", "-a"][..], pane),
        (&["respawn-pane", "-k"][..], pane),
        (&["clear-history"][..], pane),
        (&["split-window", "-h"][..], pane),
        (&["pipe-pane", "cat"][..], pane),
        (&["html", "-"][..], pane),
        (&["capture-pane"][..], pane),
        (&["set-option", "-p", "@name", "x"][..], pane),
        (&["kill-window"][..], window),
        (&["respawnw", "-k"][..], window),
        (&["setw", "mode-keys", "vi"][..], window),
        (&["set-option", "-w", "@name", "x"][..], window),
        (&["kill-session"][..], session),
        (&["rename-session", "work"][..], session),
        (&["join-pane", "-d", "-t", "%1"][..], pane_source),
        (&["movep", "-t", "%1"][..], pane_source),
        (&["break-pane", "-d", "-t", "four:"][..], pane_source),
        (&["move-window", "-d", "-t", "base:7"][..], window_source),
        (&["swapw", "-d", "-t", "@0"][..], window_source),
        (&["join-pane", "-s", "%3"][..], pane),
        (&["new-window", "-d", ";", "split-window"][..], pane),
        (&["kill-pane", ";", "new-session", "-d"][..], pane),
    ] {
        assert_eq!(
            super::untargeted_host_input("xps", &chain(words)).as_deref(),
            Some(refusal),
            "{words:?}"
        );
    }
    for words in [
        &["send-keys", "-t", "%3", "ls"][..],
        &["send-text", "-t%3", "hi"][..],
        &["run-pane", "-t", "%1", "--", "make"][..],
        &["run-pane", "--timeout=5", "-t", "%1", "--", "make"][..],
        &["send-text", "--timeout=5", "-t", "%1", "hi"][..],
        &["run-pane", "-t", "%1", "--bogus=1", "--", "make"][..],
        &["agent-respond", "-t", "%2", "--option=allow"][..],
        &["join-pane", "-s", "%3", "-t", "%1"][..],
        &["break-pane", "-s", "%3", "-t", "four:"][..],
        &["swap-window", "-s", "@1", "-t", "@0"][..],
        &["new-session", "-d", "-s", "x", ";", "split-window", "-h"][..],
        &["new-window", ";", "send-keys", "ls", "Enter"][..],
        &["agent-send", "-t", "%2", "--wait", "hi"][..],
        &["kill-pane", "-t", "%3"][..],
        &["kill-session", "-t", "agent"][..],
        &["list-panes", "-a"][..],
        &["capture-pane", "-p"][..],
        &["display-message", "-p", "#{pane_id}"][..],
        &["show-last-output"][..],
        &["inspect"][..],
        &["events"][..],
        &["new-window", "-d", "-P", "-F", "#{pane_id}"][..],
        &["new-session", "-d"][..],
        &["attach"][..],
        &["run-shell", "ls"][..],
        &["source-file", "x.conf"][..],
        &["set-hook", "-g", "pane-exited", "x"][..],
        &["setw", "-g", "mode-keys", "vi"][..],
        &["set-option", "-g", "@name", "x"][..],
        &["set-option", "status", "off"][..],
    ] {
        assert_eq!(
            super::untargeted_host_input("xps", &chain(words)),
            None,
            "{words:?}"
        );
    }
}

#[test]
fn fanning_out_refuses_by_what_the_verb_does() {
    let refusal = |words: &[&str]| {
        super::fan_out_refusal(&super::split_command_chain(
            &words.iter().copied().map(RawText::from).collect::<Vec<_>>(),
        ))
        .map(|(message, _)| message)
    };
    assert_eq!(refusal(&["show-last-output", "-t", "%0"]), None);
    assert_eq!(refusal(&["list-panes", "-a"]), None);
    assert_eq!(
        refusal(&["events"]).as_deref(),
        Some("zz: --host all: `events` streams; run it per host with --host NAME")
    );
    assert_eq!(
        refusal(&["fleet", "list"]).as_deref(),
        Some(
            "zz: --host all: `fleet` works on one machine's own files; run it per host with --host NAME"
        )
    );
    assert_eq!(
        refusal(&["kill-pane", "-t", "%0"]).as_deref(),
        Some(
            "zz: --host all runs read-only commands only, and `kill-pane` can change state; name one host with --host NAME"
        )
    );
}

#[test]
fn fanned_out_lines_name_their_host() {
    assert_eq!(
        super::host_lines("xps", b"%0 bash\n\n%1 vim", false, false),
        b"xps: %0 bash\nxps:\nxps: %1 vim\n"
    );
    assert_eq!(
        super::host_lines("xps", b"{\"pane_id\":\"%0\"}\n{}\n", true, false),
        b"{\"host_name\":\"xps\",\"pane_id\":\"%0\"}\n{\"host_name\":\"xps\"}\n"
    );
    assert_eq!(
        super::host_lines("xps", b"{\"host_name\":\"ubuntu\"}\n", true, false),
        b"{\"from_host\":\"xps\",\"host_name\":\"ubuntu\"}\n"
    );
    assert_eq!(
        super::host_lines("xps", b"{literal}\n", false, false),
        b"xps: {literal}\n"
    );
    assert_eq!(
        super::host_lines("xps", b"zz: can't reach xps: timed out\n", false, true),
        b"xps: can't reach xps: timed out\n"
    );
    assert!(super::host_lines("xps", b"", true, false).is_empty());
}

#[test]
fn only_a_chain_that_changes_nothing_may_be_sent_twice() {
    let chain = |invocations: &[&[&str]]| {
        invocations
            .iter()
            .map(|words| CommandInvocation::new(words[0], words[1..].iter().copied()))
            .collect::<Vec<_>>()
    };
    for read_only in [
        &[&["list-panes", "-a"][..]][..],
        &[
            &["display-message", "-p", "#{pane_id}"],
            &["capture-pane", "-p"],
        ],
        &[&["lsp"]],
    ] {
        assert!(
            super::chain_is_read_only(&chain(read_only)),
            "{read_only:?}"
        );
    }
    for mutating in [
        &[&["kill-window", "-t", "work"][..]][..],
        &[&["list-panes"], &["send-keys", "-t", "%1", "x"]],
        &[&["capture-pane", "-t", "%1"]],
        &[&["bogus-verb"]],
    ] {
        assert!(!super::chain_is_read_only(&chain(mutating)), "{mutating:?}");
    }
    assert_eq!(
        super::host_failure_hint(
            "xps",
            &["xps"],
            "me@xps",
            &DaemonError::from(zz_daemon_client::EndpointError::StoppedAnswering {
                target: "ssh://me@xps".to_owned(),
                seconds: 15,
            })
        ),
        "xps stopped answering for 15 s; the command may have run there, check before retrying"
    );
}

#[test]
fn host_connect_failures_name_the_host_and_the_fix() {
    use zz_daemon_client::EndpointError;

    let target = || "ssh://me@xps".to_owned();
    let failure = |configured: bool, error: &DaemonError| {
        let known: &[&str] = if configured { &["xps"] } else { &[] };
        super::host_failure_hint("xps", known, "me@xps", error)
    };
    let hint = |configured, error: EndpointError| failure(configured, &DaemonError::from(error));
    assert_eq!(
        hint(
            true,
            EndpointError::RemoteBinaryMissing { target: target() }
        ),
        "xps has no zz; run: zz fleet install xps"
    );
    assert_eq!(
        hint(
            true,
            EndpointError::RemoteProtocolMismatch {
                target: target(),
                daemon: 107,
                client: 110,
            }
        ),
        "xps runs protocol v107, this zz speaks v110; run: zz fleet install --restart-daemon xps (ends its sessions)"
    );
    assert_eq!(
        hint(
            true,
            EndpointError::RemoteProtocolMismatch {
                target: target(),
                daemon: 111,
                client: 110,
            }
        ),
        "xps runs protocol v111, newer than this zz (v110); update zz on this machine"
    );
    let refused = "xps refused every login zz tried; run ssh me@xps once, or zz fleet sync from a machine that reaches it";
    assert_eq!(
        hint(
            true,
            EndpointError::AuthenticationFailed {
                target: target(),
                reason: "Permission denied".to_owned(),
            }
        ),
        refused
    );
    assert_eq!(
        hint(
            true,
            EndpointError::SshFailed {
                target: target(),
                reason: format!("{}: add your key", super::SSH_LOGIN_REFUSED),
            }
        ),
        refused
    );
    assert_eq!(
        hint(
            true,
            EndpointError::SshFailed {
                target: target(),
                reason: format!("{}: verify the host", super::SSH_HOST_KEY_UNTRUSTED),
            }
        ),
        "xps's ssh host key is not trusted; run ssh me@xps once to check and accept it"
    );
    assert_eq!(
        hint(
            true,
            EndpointError::SshFailed {
                target: target(),
                reason: "Could not resolve hostname xps".to_owned(),
            }
        ),
        "can't reach xps: Could not resolve hostname xps"
    );
    assert_eq!(
        hint(
            false,
            EndpointError::RemoteBinaryMissing { target: target() }
        ),
        "xps has no zz; run: zz fleet add NAME xps, then zz fleet install NAME"
    );
    assert_eq!(
        failure(
            true,
            &DaemonError::IncompatibleDaemon {
                daemon: Some(107),
                client: 110,
            }
        ),
        "xps runs protocol v107, this zz speaks v110; run: zz fleet install --restart-daemon xps (ends its sessions)"
    );
    assert_eq!(
        failure(
            true,
            &DaemonError::Server(zz_protocol::ServerError::ClientTooOld {
                client: 1,
                minimum: 2,
            })
        ),
        "xps runs a newer zz; update zz on this machine"
    );
    assert!(
        failure(
            true,
            &DaemonError::Io(io::Error::from(io::ErrorKind::ConnectionRefused))
        )
        .starts_with("can't reach xps: ")
    );
    let unresolved = DaemonError::from(EndpointError::SshFailed {
        target: "ssh://xpz".to_owned(),
        reason: "Could not resolve hostname xpz".to_owned(),
    });
    assert_eq!(
        super::host_failure_hint("xpz", &["xps", "ubuntu"], "xpz", &unresolved),
        "can't reach xpz: Could not resolve hostname xpz; known hosts: xps, ubuntu"
    );
    assert_eq!(
        super::host_failure_hint("xps", &["xps", "ubuntu"], "me@xps", &unresolved),
        "can't reach xps: Could not resolve hostname xpz"
    );
    for (destination, expected) in [
        ("ssh://me@xps", "me@xps"),
        ("ssh://gpu-box", "gpu-box"),
        ("ssh://me@gpu-box:2222/tmp/zz.sock", "ssh://me@gpu-box:2222"),
    ] {
        assert_eq!(
            super::ssh_destination(&super::Endpoint::parse(destination).unwrap()).as_deref(),
            Some(expected),
            "{destination}"
        );
    }
}
