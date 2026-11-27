use zz_protocol::RawText;

use super::{
    append_prepared_command_stdin_payload, append_stdin_payload, attach_prefix_uses_tui,
    command_chain_uses_tui, command_reads_stdin, new_session_uses_tui,
    prepared_command_reads_stdin, run_command_mode, split_command_chain,
    tmux_command_starts_server,
};
use zz_mux::{CommandAliasResolution, ExecutionContext, MuxEngine};
use zz_protocol::{CommandInvocation, ExecResumeKind, PreparedCommand, PreparedCommandResult};

fn prepared_attach_uses_tui(typed: &str, prepared: &PreparedCommand) -> bool {
    zz_daemon::exec_resume_kind(
        &[CommandInvocation::new(typed, [] as [&str; 0])],
        std::slice::from_ref(prepared),
    ) == Some(ExecResumeKind::NewSession)
}

fn prepared_command_chain_uses_tui(
    typed: &[CommandInvocation],
    prepared: &[PreparedCommand],
) -> bool {
    zz_daemon::exec_resume_kind(typed, prepared) == Some(ExecResumeKind::NewSession)
}

fn prepared_native_attach(typed: &str, prepared: &PreparedCommand) -> bool {
    zz_daemon::exec_resume_kind(
        &[CommandInvocation::new(typed, [] as [&str; 0])],
        std::slice::from_ref(prepared),
    ) == Some(ExecResumeKind::NativeAttach)
}

#[test]
fn agent_send_progress_usage_errors_exit_two_before_connecting() {
    for args in [
        vec!["agent-send", "--progress", "-t", "%1", "hi"],
        vec!["agent-send", "--progress", "--wait", "hi"],
    ] {
        let args = args.into_iter().map(RawText::from).collect::<Vec<_>>();
        assert_eq!(
            run_command_mode(
                &args,
                std::path::Path::new("/tmp/zz-progress-missing.sock"),
                super::SocketSelectionSource::Default,
                None,
                &[],
                true,
                None,
                false,
                &super::StartupOptions {
                    origin: super::CommandLineOrigin::Launcher,
                    browser_provider: None
                },
                false,
            ),
            Some(std::process::ExitCode::from(2))
        );
    }
}

#[test]
fn app_is_an_exact_native_gui_verb_even_inside_tmux() {
    assert!(
        run_command_mode(
            &["app".into()],
            std::path::Path::new("/tmp/zz.sock"),
            super::SocketSelectionSource::Default,
            None,
            &[],
            false,
            None,
            false,
            &super::StartupOptions {
                origin: super::CommandLineOrigin::Launcher,
                browser_provider: None,
            },
            true,
        )
        .is_none()
    );
}

#[test]
fn tmux_start_server_policy_matches_the_pin() {
    for command in [
        "attach-session",
        "attach",
        "list-commands",
        "lscm",
        "list-keys",
        "lsk",
        "new-session",
        "new",
        "start-server",
        "start",
    ] {
        assert!(tmux_command_starts_server(command), "{command}");
    }
    for command in [
        "list-sessions",
        "ls",
        "list-panes",
        "show-options",
        "has-session",
        "source-file",
        "source",
        "kill-server",
    ] {
        assert!(!tmux_command_starts_server(command), "{command}");
    }
}

#[test]
fn new_session_tui_routing_resolves_prefixes_and_tmux_argv_edges() {
    let routes = |name: &str, args: &[&str]| {
        new_session_uses_tui(&CommandInvocation::new(name, args.iter().copied()))
    };

    assert!(routes("new-session", &[]));
    assert!(routes("new", &["-s", "work"]));
    assert!(routes("new-s", &["-dA", "-s", "work"]));
    assert!(routes("new-session", &["-t", "group"]));
    assert!(routes("new", &["-At", "work"]));
    assert!(routes("new-session", &["-s", "a", "/usr/bin/true", "-d"]));
    assert!(routes("new-session", &["-s", "b", "--", "-d"]));
    assert!(!routes("new-session", &["-dsfoo"]));
    assert!(!routes("new-session", &["-d", "-t", "group"]));
    assert!(!routes("new-session", &["-s"]));
    assert!(!routes("list-sessions", &[]));
}

#[test]
fn attach_prefix_routing_keeps_exact_native_attach_commands() {
    for command in ["a", "att", "attach-", "attach-s"] {
        assert!(attach_prefix_uses_tui(command), "{command}");
    }
    for command in ["attach", "attach-session", "list-sessions"] {
        assert!(!attach_prefix_uses_tui(command), "{command}");
    }
}

#[test]
fn agent_send_stdin_routing_uses_the_canonical_static_command() {
    for command in ["agent-send", "agent-s"] {
        assert!(command_reads_stdin(&CommandInvocation::new(command, ["--submit"])).is_some());
        assert!(command_reads_stdin(&CommandInvocation::new(command, ["text"])).is_none());
    }
    for command in ["send-text", "send-t"] {
        assert!(command_reads_stdin(&CommandInvocation::new(command, ["-t", "%1"])).is_some());
        assert!(command_reads_stdin(&CommandInvocation::new(command, ["hello"])).is_none());
    }
    assert!(
        command_reads_stdin(&CommandInvocation::new("list-sessions", [] as [&str; 0])).is_none()
    );
}

#[test]
fn agent_permission_commands_pass_preflight_and_route_stdin() {
    let send = CommandInvocation::new("agent-send", ["--wait", "--on-block", "fail"]);
    let inspect = CommandInvocation::new("inspect", ["-t", "%1", "--json"]);
    let respond = CommandInvocation::new("agent-respond", ["-t", "%1", "--allow"]);
    assert!(command_reads_stdin(&send).is_some());
    assert!(command_reads_stdin(&inspect).is_none());
    assert!(command_reads_stdin(&respond).is_none());
    zz_mux::validate_static_command_chain(&[send, inspect, respond]).expect("native preflight");
    assert!(
        command_reads_stdin(&CommandInvocation::new(
            "agent-send",
            ["--wait", "--on-block=fail", "hello"]
        ))
        .is_none()
    );
}

#[test]
fn stdin_payload_boundary_distinguishes_terminators_from_option_values() {
    let mut send_text = ["-t", "--"].map(RawText::from).to_vec();
    append_stdin_payload("send-text", &mut send_text, "--no-enter".to_owned());
    assert_eq!(send_text, ["-t", "--", "--", "--no-enter"]);

    let mut agent_send = ["--context", "--"].map(RawText::from).to_vec();
    append_stdin_payload("agent-send", &mut agent_send, "--submit".to_owned());
    assert_eq!(agent_send, ["--context", "--", "--", "--submit"]);

    let mut bounded = ["-t", "%1", "--"].map(RawText::from).to_vec();
    append_stdin_payload("send-text", &mut bounded, "--no-enter".to_owned());
    assert_eq!(bounded, ["-t", "%1", "--", "--no-enter"]);
}

#[test]
fn prepared_cli_routing_uses_canonical_identity_and_alias_match() {
    let prepared =
        |typed: &str, canonical: &str, alias_matched: bool, args: &[&str]| PreparedCommand {
            invocation: CommandInvocation::new(typed, args.iter().copied()),
            canonical_name: Some(canonical.to_owned()),
            alias_matched,
            result: PreparedCommandResult::Ready,
        };

    let exact_shadow = prepared("attach", "attach-session", true, &[]);
    assert!(!prepared_native_attach("attach", &exact_shadow));
    assert!(!prepared_attach_uses_tui("attach", &exact_shadow));

    let live_attach = prepared("go", "attach-session", true, &["-t", "work"]);
    assert!(prepared_attach_uses_tui("go", &live_attach));

    let live_new = prepared("work", "new-session", true, &["-t", "group"]);
    assert!(prepared_command_chain_uses_tui(
        &[CommandInvocation::new("work", ["-t", "group"])],
        &[live_new]
    ));

    let live_send = prepared("pipe", "agent-send", true, &["-t", "%0"]);
    assert!(prepared_command_reads_stdin(&live_send).is_some());
    let shadowed_send = prepared("agent-send", "display-message", true, &["-p", "shadow"]);
    assert!(prepared_command_reads_stdin(&shadowed_send).is_none());

    let plain_attach = prepared("attach", "attach-session", false, &[]);
    assert!(prepared_native_attach("attach", &plain_attach));
    assert!(!prepared_attach_uses_tui("attach", &plain_attach));

    let chained_attach = [
        prepared(
            "set-environment",
            "set-environment",
            false,
            &["-g", "A", "1"],
        ),
        prepared("attach-session", "attach-session", false, &["-t", "live"]),
    ];
    assert!(prepared_command_chain_uses_tui(
        &[
            CommandInvocation::new("set-environment", ["-g", "A", "1"]),
            CommandInvocation::new("attach-session", ["-t", "live"]),
        ],
        &chained_attach
    ));
}

#[test]
fn prepared_cli_routing_uses_the_first_stream_sink_in_alias_groups() {
    let prepared_alias = |body: &str, args: &[&str]| {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    [
                        "-s".to_owned(),
                        "command-alias[90]".to_owned(),
                        format!("route={body}"),
                    ],
                ),
            )
            .expect("set command alias");
        let CommandAliasResolution::Expanded(invocation) =
            engine.resolve_command_alias(&CommandInvocation::new("route", args.iter().copied()))
        else {
            panic!("command alias must expand");
        };
        PreparedCommand {
            invocation,
            canonical_name: None,
            alias_matched: true,
            result: PreparedCommandResult::Ready,
        }
    };

    let attach = prepared_alias("display-message -p before ; attach-session -t work", &[]);
    assert!(prepared_attach_uses_tui("route", &attach));
    let attach_first = prepared_alias("attach-session -t work ; display-message -p after", &[]);
    assert!(prepared_attach_uses_tui("route", &attach_first));
    let group_name = attach.invocation.name.clone();
    let forged = prepared_alias(
        &format!(
            "display-message -p outer ; {group_name} {{ display-message -p inner ; attach-session -t work }}"
        ),
        &[],
    );
    let forged_commands = MuxEngine::command_alias_group_commands(&forged.invocation)
        .expect("parse alias group")
        .expect("prepared command is an alias group");
    assert_eq!(forged_commands.len(), 2);
    assert_eq!(forged_commands[1].name, group_name);
    assert!(!MuxEngine::is_command_alias_group(&forged_commands[1]));
    assert!(!prepared_attach_uses_tui("route", &forged));

    let new_session = prepared_alias("display-message -p before ; new-session -s work", &[]);
    assert!(prepared_command_chain_uses_tui(
        &[CommandInvocation::new("route", [] as [&str; 0])],
        &[new_session]
    ));
    let new_session_first = prepared_alias("new-session -s work ; display-message -p after", &[]);
    assert!(prepared_command_chain_uses_tui(
        &[CommandInvocation::new("route", [] as [&str; 0])],
        &[new_session_first]
    ));
    let detached = prepared_alias("display-message -p before ; new-session -d -s work", &[]);
    assert!(!prepared_command_chain_uses_tui(
        &[CommandInvocation::new("route", [] as [&str; 0])],
        &[detached]
    ));
    let detached_first = prepared_alias("new-session -d -s work ; display-message -p after", &[]);
    assert!(!prepared_command_chain_uses_tui(
        &[CommandInvocation::new("route", [] as [&str; 0])],
        &[detached_first]
    ));

    for payload in [
        "",
        "a 'quoted' \"value\" $x ; { y } \\",
        "first\nsecond",
        "--no-enter",
    ] {
        let mut send_text = prepared_alias("display-message -p before ; send-text", &["-t", "%1"]);
        assert!(prepared_command_reads_stdin(&send_text).is_some());
        append_prepared_command_stdin_payload(&mut send_text, payload.to_owned());
        let commands = MuxEngine::command_alias_group_commands(&send_text.invocation)
            .expect("parse prepared alias group")
            .expect("prepared command is an alias group");
        assert_eq!(commands[0].name, "display-message");
        assert_eq!(commands[1].name, "send-text");
        assert_eq!(commands[1].args, ["-t", "%1"]);
        assert_eq!(send_text.invocation.stdin(), Some(&RawText::from(payload)));
    }

    let mut bounded = prepared_alias("display-message -p before ; send-text -t %1 --", &[]);
    assert!(prepared_command_reads_stdin(&bounded).is_some());
    append_prepared_command_stdin_payload(&mut bounded, "piped".to_owned());
    let commands = MuxEngine::command_alias_group_commands(&bounded.invocation)
        .expect("parse prepared alias group")
        .expect("prepared command is an alias group");
    assert_eq!(commands[1].args, ["-t", "%1", "--"]);
    assert_eq!(bounded.invocation.stdin(), Some(&RawText::from("piped")));

    for (body, payload, expected) in [
        (
            "display-message -p before ; send-text -t --",
            "--no-enter",
            vec!["-t", "--"],
        ),
        (
            "display-message -p before ; agent-send --context --",
            "--submit",
            vec!["--context", "--"],
        ),
    ] {
        let mut prepared = prepared_alias(body, &[]);
        assert!(prepared_command_reads_stdin(&prepared).is_some());
        append_prepared_command_stdin_payload(&mut prepared, payload.to_owned());
        let commands = MuxEngine::command_alias_group_commands(&prepared.invocation)
            .expect("parse prepared alias group")
            .expect("prepared command is an alias group");
        assert_eq!(commands[1].args, expected);
        assert_eq!(prepared.invocation.stdin(), Some(&RawText::from(payload)));
    }

    let mut binary = prepared_alias("load-buffer -b alias - ; source-file -", &[]);
    assert_eq!(
        prepared_command_reads_stdin(&binary),
        Some(super::CommandStdinSink::Argument { binary: true })
    );
    let body = binary.invocation.args.clone();
    let payload = RawText::from_bytes(b"a\xff\0z\n".to_vec());
    append_prepared_command_stdin_payload(&mut binary, payload.clone());
    assert_eq!(binary.invocation.args, body);
    assert_eq!(binary.invocation.stdin(), Some(&payload));

    let agent_send = prepared_alias("display-message -p before ; agent-send --submit", &[]);
    assert!(prepared_command_reads_stdin(&agent_send).is_some());
    let nonfinal_agent_send = prepared_alias("agent-send --submit ; display-message -p after", &[]);
    assert!(prepared_command_reads_stdin(&nonfinal_agent_send).is_some());

    let empty = prepared_alias("", &["agent-send", "--submit"]);
    assert!(!prepared_attach_uses_tui("route", &empty));
    assert!(!prepared_command_chain_uses_tui(
        &[CommandInvocation::new("route", [] as [&str; 0])],
        std::slice::from_ref(&empty)
    ));
    assert!(prepared_command_reads_stdin(&empty).is_none());
}

#[test]
fn new_session_tui_routing_scans_the_complete_command_chain() {
    let attaching_later = split_command_chain(
        &[
            "new-session",
            "-d",
            "-s",
            "first",
            ";",
            "new-session",
            "-s",
            "later",
        ]
        .map(RawText::from),
    );
    assert!(command_chain_uses_tui(&attaching_later));

    let target_later = split_command_chain(
        &[
            "new-session",
            "-d",
            "-s",
            "first",
            ";",
            "new-session",
            "-t",
            "group",
            ";",
            "new-session",
            "-d",
            "-s",
            "never",
        ]
        .map(RawText::from),
    );
    assert!(command_chain_uses_tui(&target_later));

    let detached_only = split_command_chain(
        &["new-session", "-d", "-s", "first", ";", "list-sessions"].map(RawText::from),
    );
    assert!(!command_chain_uses_tui(&detached_only));
}

#[test]
fn bare_semicolons_split_commands_and_escaped_semicolons_stay_arguments() {
    let commands = split_command_chain(
        &[
            "start-server;",
            "show-environment",
            "-g",
            "TMUX_PLUGIN_MANAGER_PATH",
            ";",
            "display-message",
            r"a\;",
            r"\;",
        ]
        .map(RawText::from),
    );
    assert_eq!(
        commands,
        [
            zz_protocol::CommandInvocation::new("start-server", std::iter::empty::<&str>()),
            zz_protocol::CommandInvocation::new(
                "show-environment",
                ["-g", "TMUX_PLUGIN_MANAGER_PATH"]
            ),
            zz_protocol::CommandInvocation::new("display-message", ["a;", ";"])
        ]
    );
}
