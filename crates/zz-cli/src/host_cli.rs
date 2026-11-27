use super::*;

pub(super) const SSH_LOGIN_REFUSED: &str = "ssh rejected the login";

pub(super) const SSH_HOST_KEY_UNTRUSTED: &str = "known_hosts has no matching key for the host";

const UNTARGETED_HOST_VERBS: [&str; 5] = [
    "new-window",
    "attach-session",
    "if-shell",
    "run-shell",
    "source-file",
];

const SOURCE_HOST_VERBS: [&str; 5] = [
    "join-pane",
    "move-pane",
    "break-pane",
    "move-window",
    "swap-window",
];

const SINGLE_HOST_VERBS: [(&str, &str); 5] = [
    ("events", "streams"),
    ("wait-pane", "waits on one pane"),
    ("fleet", "works on one machine's own files"),
    ("tailnet", "reports on one machine's own Tailscale"),
    ("tools", "works on one machine's own files"),
];

pub(super) fn with_host_hint(host: Option<&str>, message: String) -> String {
    match host {
        Some(host) if message == "can't find session: current session" => format!(
            "{message}\nzz --host {host}: name the session with -t NAME (zz --host {host} \
             list-sessions, or make one with zz --host {host} new-session -d -s NAME)"
        ),
        Some(host) => with_host_commands(host, &message),
        None => message,
    }
}

pub(super) fn with_host_commands(host: &str, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("zz ") {
        out.push_str(&rest[..index]);
        let starts_word = out.chars().next_back().is_none_or(|before| {
            !(before.is_alphanumeric() || matches!(before, '-' | '_' | '/' | '.'))
        });
        out.push_str("zz ");
        rest = &rest[index + 3..];
        let verb = rest.split(char::is_whitespace).next().unwrap_or_default();
        if starts_word && catalog_command_spec(canonical_command(verb)).is_some() {
            out.push_str("--host ");
            out.push_str(host);
            out.push(' ');
        }
    }
    out.push_str(rest);
    out
}

pub(super) fn run_host_cli(host: &str, arguments: &[RawText]) -> ExitCode {
    let endpoint = match configured_host_endpoint(host) {
        Ok(Endpoint::Ssh(endpoint)) => endpoint,
        Ok(Endpoint::Local(path)) => {
            eprintln!(
                "zz: {host} is the local socket {}; run the command without --host",
                path.display()
            );
            return exit_code_for(CliFailure::Usage);
        }
        Ok(Endpoint::Tailnet(endpoint)) => {
            eprintln!(
                "zz: this command runs on the host over ssh, and {host} is the tailnet host {endpoint}"
            );
            return exit_code_for(CliFailure::Usage);
        }
        Ok(Endpoint::Wsl(endpoint)) => {
            eprintln!(
                "zz: this command runs on the host over ssh, and {host} is the WSL host {endpoint}"
            );
            return exit_code_for(CliFailure::Usage);
        }
        Err(error) => {
            eprintln!("zz: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let arguments = arguments
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    match zz_daemon_client::run_remote_cli(&endpoint, &arguments) {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or_else(|| exit_code_for(CliFailure::Runtime), ExitCode::from),
        Err(error) => {
            eprintln!(
                "zz: {}",
                host_connect_failure(host, &DaemonError::from(error))
            );
            exit_code_for(CliFailure::Runtime)
        }
    }
}

pub(super) fn host_command_error_message(host: &str, error: &DaemonError) -> String {
    if zz_daemon_client::EndpointError::find(error).is_some() {
        format!("zz: {}", host_connect_failure(host, error))
    } else {
        with_host_hint(Some(host), command_error_message(error))
    }
}

pub(crate) fn chain_is_read_only(command_chain: &[CommandInvocation]) -> bool {
    command_chain.iter().all(|invocation| {
        catalog_command_spec(canonical_command(&invocation.name))
            .is_some_and(|spec| !spec.mutates(&invocation.args))
    })
}

pub(super) fn names_this_machine(name: &str) -> bool {
    let hosts = zz_daemon_client::configured_fleet_hosts()
        .map(|(hosts, _)| hosts)
        .unwrap_or_default();
    unconfigured_local_name(
        &hosts,
        zz_daemon_client::process_info::host_name().as_deref(),
        name,
    )
}

pub(super) fn unconfigured_local_name(
    hosts: &[zz_daemon_client::HostEntry],
    local: Option<&str>,
    name: &str,
) -> bool {
    !hosts.iter().any(|host| host.name == name)
        && local.is_some_and(|local| {
            name.eq_ignore_ascii_case(local)
                || local
                    .split('.')
                    .next()
                    .is_some_and(|short| name.eq_ignore_ascii_case(short))
        })
}

pub(super) fn connect_host_command_client(
    name: &str,
    read_only: bool,
) -> Result<CommandClient, String> {
    let endpoint = configured_host_endpoint(name)?;
    CommandClient::connect_endpoint(&endpoint, read_only)
        .map_err(|error| host_connect_failure(name, &error))
}

pub(crate) fn configured_host_endpoint(name: &str) -> Result<Endpoint, String> {
    zz_daemon_client::host_endpoint(name)
}

fn host_connect_failure(name: &str, error: &DaemonError) -> String {
    let mut hosts = zz_daemon_client::configured_fleet_hosts()
        .map(|(hosts, _)| hosts)
        .unwrap_or_default();
    if !hosts.iter().any(|host| host.name == name) && !name.contains(['@', ':', '/']) {
        hosts.extend(zz_daemon_client::wsl_hosts_beside(&hosts));
    }
    let known = hosts
        .iter()
        .map(|host| host.name.as_str())
        .collect::<Vec<_>>();
    let destination = zz_daemon_client::resolve_fleet_host(&hosts, name)
        .ok()
        .and_then(|endpoint| ssh_destination(&endpoint))
        .unwrap_or_else(|| name.to_owned());
    host_failure_hint(name, &known, &destination, error)
}

pub(super) fn ssh_destination(endpoint: &Endpoint) -> Option<String> {
    let Endpoint::Ssh(endpoint) = endpoint else {
        return None;
    };
    Some(match (&endpoint.user, endpoint.port) {
        (_, Some(_)) => zz_daemon_client::SshEndpoint {
            remote_socket: None,
            ..endpoint.clone()
        }
        .to_string(),
        (Some(user), None) => format!("{user}@{}", endpoint.host),
        (None, None) => endpoint.host.clone(),
    })
}

pub(super) fn tui_failure_message(host: Option<&str>, error: &zz_tui::Error) -> String {
    match (host, error.connect_error()) {
        (Some(host), Some(connect)) => format!("zz: {}", host_connect_failure(host, connect)),
        _ => error.to_string(),
    }
}

pub(super) fn untargeted_host_input(
    host: &str,
    command_chain: &[CommandInvocation],
) -> Option<String> {
    use zz_protocol::CommandValueKind;

    let (verb, (flag, kind)) = command_chain
        .iter()
        .take_while(|invocation| !starts_its_own_target(invocation))
        .find_map(|invocation| {
            Some((
                canonical_command(&invocation.name),
                untargeted_kind(invocation)?,
            ))
        })?;
    let (noun, id, list, make) = match kind {
        CommandValueKind::Window => (
            "window",
            "@N",
            "list-windows -a",
            "new-session -d -P -F '#{window_id}'",
        ),
        CommandValueKind::Session => ("session", "NAME", "list-sessions", "new-session -d -s NAME"),
        _ => (
            "pane",
            "%N",
            "list-panes -a",
            "new-session -d -P -F '#{pane_id}'",
        ),
    };
    let no_pane = if verb == "run-pane" {
        format!("; a command that needs no pane runs with zz --host {host} run-shell 'COMMAND'")
    } else {
        String::new()
    };
    Some(format!(
        "zz --host {host}: name the {noun} with {flag} {id} (zz --host {host} {list}, or make one \
         with zz --host {host} {make}){no_pane}"
    ))
}

fn starts_its_own_target(invocation: &CommandInvocation) -> bool {
    match canonical_command(&invocation.name) {
        "new-session" => true,
        "new-window" => catalog_command_spec("new-window")
            .and_then(|spec| zz_protocol::parse_tmux_options(spec, &invocation.args).ok())
            .is_some_and(|parsed| {
                !parsed
                    .options
                    .contains(&zz_protocol::TmuxOption::Flag("-d"))
            }),
        _ => false,
    }
}

fn untargeted_kind(
    invocation: &CommandInvocation,
) -> Option<(&'static str, zz_protocol::CommandValueKind)> {
    use zz_protocol::{CommandValueKind, TmuxOption};

    let spec = catalog_command_spec(canonical_command(&invocation.name)).filter(|spec| {
        !UNTARGETED_HOST_VERBS.contains(&spec.name) && spec.mutates(&invocation.args)
    })?;
    let args = split_attached_long_values(spec, &invocation.args);
    let parsed = zz_protocol::parse_tmux_options(spec, &args);
    let flagged =
        |flag| matches!(&parsed, Ok(parsed) if parsed.options.contains(&TmuxOption::Flag(flag)));
    let given = |flag: &str| match &parsed {
        Ok(parsed) => parsed
            .options
            .iter()
            .any(|option| matches!(option, TmuxOption::Value(name, _) if *name == flag)),
        Err(_) => args
            .iter()
            .take_while(|argument| argument.as_str() != "--")
            .any(|argument| argument.starts_with(flag)),
    };
    if flagged("-g") {
        return None;
    }
    let flag = ["-t", "-s"]
        .into_iter()
        .find(|flag| (*flag == "-t" || SOURCE_HOST_VERBS.contains(&spec.name)) && !given(flag))?;
    let kind = match (flag, spec.name) {
        ("-t", "set-option") if flagged("-p") => CommandValueKind::Pane,
        ("-t", "set-option") if flagged("-w") => CommandValueKind::Window,
        _ => spec.option(flag)?.value?,
    };
    matches!(
        kind,
        CommandValueKind::Pane | CommandValueKind::Window | CommandValueKind::Session
    )
    .then_some((flag, kind))
}

fn split_attached_long_values(spec: &zz_protocol::CommandSpec, args: &[RawText]) -> Vec<RawText> {
    let mut split = Vec::with_capacity(args.len());
    let mut arguments = args.iter();
    for argument in arguments.by_ref() {
        if argument.as_str() == "--" {
            split.push(argument.clone());
            break;
        }
        match argument
            .split_once('=')
            .filter(|(name, _)| name.starts_with("--") && spec.option(name).is_some())
        {
            Some((name, value)) => split.extend([RawText::from(name), RawText::from(value)]),
            None => split.push(argument.clone()),
        }
    }
    split.extend(arguments.cloned());
    split
}

pub(super) fn fan_out_refusal(command_chain: &[CommandInvocation]) -> Option<(String, ExitCode)> {
    let refuse = |error: ServerError| {
        Some((
            server_error_message(&error),
            exit_code_for(CliFailure::Server(&error)),
        ))
    };
    for invocation in command_chain.iter().filter(|invocation| {
        !matches!(
            invocation.name.as_str(),
            "protocol-version" | "build-info" | "--version"
        )
    }) {
        if let Err(error) = zz_mux::validate_static_command_chain(std::slice::from_ref(invocation))
        {
            return refuse(error);
        }
        let spec = match zz_protocol::resolve_command(&invocation.name) {
            zz_protocol::CommandResolution::Unimplemented(name) => {
                return refuse(ServerError::UnsupportedCommand(name.to_owned()));
            }
            zz_protocol::CommandResolution::Canonical(name) => {
                catalog_command_spec(name).filter(|spec| !spec.mutates(&invocation.args))
            }
            _ => None,
        };
        let Some(spec) = spec else {
            let name = canonical_command(&invocation.name);
            let message = match SINGLE_HOST_VERBS.iter().find(|(verb, _)| *verb == name) {
                Some((_, reason)) => format!(
                    "zz: --host all: `{}` {reason}; run it per host with --host NAME",
                    invocation.name
                ),
                None => format!(
                    "zz: --host all runs read-only commands only, and `{}` can change state; name one host with --host NAME",
                    invocation.name
                ),
            };
            return Some((message, exit_code_for(CliFailure::Usage)));
        };
        if !spec.uses_tmux_option_grammar()
            && let Err(error) = zz_protocol::parse_tmux_options(spec, &invocation.args)
                .and_then(|parsed| spec.validate_positional_maximum(parsed.positionals.len()))
        {
            return refuse(error);
        }
    }
    None
}

pub(super) fn run_on_every_host(
    arguments: &[RawText],
    command_chain: &[CommandInvocation],
) -> ExitCode {
    if let Some((message, exit)) = fan_out_refusal(command_chain) {
        eprintln!("{message}");
        return exit;
    }
    let hosts = match zz_daemon_client::fleet_hosts_with_tailnet() {
        Ok((hosts, _)) if !hosts.is_empty() => hosts,
        Ok(_) => {
            eprintln!(
                "zz: --host all: no hosts in zz/config or on the tailnet; add one with zz fleet add NAME user@box"
            );
            return exit_code_for(CliFailure::Runtime);
        }
        Err(error) => {
            eprintln!("zz: could not read zz/config: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("zz: could not find this zz to run per host: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let json = command_chain
        .iter()
        .any(|invocation| invocation.args.iter().any(|argument| argument == "--json"));
    let mut failed = false;
    thread::scope(|scope| {
        let runs = hosts
            .iter()
            .map(|host| {
                let executable = &executable;
                let run = scope.spawn(move || {
                    let mut command = Command::new(executable);
                    command
                        .arg("--host")
                        .arg(&host.name)
                        .args(arguments.iter().map(RawText::to_os_string))
                        .env(zz_daemon_client::SSH_BATCH_ENVIRONMENT_VARIABLE, "1");
                    #[cfg(unix)]
                    without_controlling_terminal(&mut command);
                    command.output_unmasked()
                });
                (host.name.as_str(), run)
            })
            .collect::<Vec<_>>();
        for (name, run) in runs {
            match run.join() {
                Ok(Ok(output)) => {
                    let _ = io::stdout().write_all(&host_lines(name, &output.stdout, json, false));
                    let _ = io::stderr().write_all(&host_lines(name, &output.stderr, false, true));
                    failed |= !output.status.success();
                }
                Ok(Err(error)) => {
                    eprintln!("{name}: could not run zz: {error}");
                    failed = true;
                }
                Err(_) => {
                    eprintln!("{name}: the run for this host panicked");
                    failed = true;
                }
            }
        }
    });
    if failed {
        exit_code_for(CliFailure::Runtime)
    } else {
        ExitCode::SUCCESS
    }
}

pub(super) fn host_lines(name: &str, bytes: &[u8], json: bool, errors: bool) -> Vec<u8> {
    let quoted = serde_json::Value::from(name).to_string();
    let mut output = Vec::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        if let Some(object) = line.strip_prefix(b"{").filter(|_| json) {
            let named = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(line)
                .is_ok_and(|row| row.contains_key("host_name"));
            output.extend_from_slice(if named {
                b"{\"from_host\":".as_slice()
            } else {
                b"{\"host_name\":".as_slice()
            });
            output.extend_from_slice(quoted.as_bytes());
            if !object.starts_with(b"}") {
                output.push(b',');
            }
            output.extend_from_slice(object);
        } else {
            let line = if errors {
                line.strip_prefix(b"zz: ").unwrap_or(line)
            } else {
                line
            };
            output.extend_from_slice(name.as_bytes());
            output.push(b':');
            if !line.is_empty() {
                output.push(b' ');
                output.extend_from_slice(line);
            }
        }
        output.push(b'\n');
    }
    output
}

pub(super) fn host_failure_hint(
    name: &str,
    known: &[&str],
    destination: &str,
    error: &DaemonError,
) -> String {
    use zz_daemon_client::EndpointError;

    let configured = known.contains(&name);
    let unreachable = |reason: &dyn std::fmt::Display| {
        if configured || known.is_empty() {
            format!("can't reach {name}: {reason}")
        } else {
            format!(
                "can't reach {name}: {reason}; known hosts: {}",
                known.join(", ")
            )
        }
    };
    let install = |restart: bool| {
        let flag = if restart { "--restart-daemon " } else { "" };
        let ends = if restart { " (ends its sessions)" } else { "" };
        if configured {
            format!("run: zz fleet install {flag}{name}{ends}")
        } else {
            format!("run: zz fleet add NAME {name}, then zz fleet install {flag}NAME{ends}")
        }
    };
    let protocol = |daemon: u16, client: u16| {
        if daemon < client {
            format!(
                "{name} runs protocol v{daemon}, this zz speaks v{client}; {}",
                install(true)
            )
        } else {
            format!(
                "{name} runs protocol v{daemon}, newer than this zz (v{client}); update zz on this machine"
            )
        }
    };
    let refused = || {
        format!(
            "{name} refused every login zz tried; run ssh {destination} once, or zz fleet sync from a machine that reaches it"
        )
    };
    let untrusted = || {
        format!(
            "{name}'s ssh host key is not trusted; run ssh {destination} once to check and accept it"
        )
    };
    match EndpointError::find(error) {
        Some(EndpointError::RemoteBinaryMissing { .. }) => {
            format!("{name} has no zz; {}", install(false))
        }
        Some(EndpointError::RemoteProtocolMismatch { daemon, client, .. }) => {
            protocol(*daemon, *client)
        }
        Some(EndpointError::RemoteProtocolUnknown { .. }) => format!(
            "{name} runs a zz too old to report its protocol; {}",
            install(true)
        ),
        Some(EndpointError::AuthenticationFailed { .. } | EndpointError::SignInExpired { .. }) => {
            refused()
        }
        Some(EndpointError::HostKeyRejected { .. }) => untrusted(),
        Some(EndpointError::StoppedAnswering { seconds, .. }) => format!(
            "{name} stopped answering for {seconds} s; the command may have run there, check before retrying"
        ),
        Some(
            EndpointError::ProbeFailure { reason, .. } | EndpointError::SshFailed { reason, .. },
        ) if reason.starts_with(SSH_LOGIN_REFUSED) => refused(),
        Some(
            EndpointError::ProbeFailure { reason, .. } | EndpointError::SshFailed { reason, .. },
        ) if reason.starts_with(SSH_HOST_KEY_UNTRUSTED) => untrusted(),
        Some(
            EndpointError::ProbeFailure { reason, .. } | EndpointError::SshFailed { reason, .. },
        ) => unreachable(reason),
        Some(error) => unreachable(error),
        None => match error {
            DaemonError::IncompatibleDaemon {
                daemon: Some(daemon),
                client,
            } if daemon != client => protocol(*daemon, *client),
            DaemonError::IncompatibleDaemon { .. } | DaemonError::DaemonTooOld { .. } => {
                format!("{name}: {error}; {}", install(true))
            }
            DaemonError::Server(ServerError::ClientTooOld { .. }) => {
                format!("{name} runs a newer zz; update zz on this machine")
            }
            error => unreachable(error),
        },
    }
}
